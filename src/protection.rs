use std::net::IpAddr;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::Request;
use axum::{
    body::Body,
    http::{HeaderValue, Method, StatusCode},
    middleware::Next,
    response::Response,
};
use dashmap::DashMap;
use parking_lot::Mutex;
use tracing::warn;
use ulid::Ulid;

use crate::metrics;
use crate::state::{ProtectionConfig, SharedState};

// ---- Internal state types ----

#[derive(Default)]
pub struct ConnectionCount {
    pub active_http: AtomicU32,
    pub active_ws: AtomicU32,
}

pub struct BanEntry {
    pub banned_until: Instant,
    pub reason: String,
    pub violation_count: u32,
}

pub struct ViolationRecord {
    pub count: AtomicU32,
    pub first_seen: Instant,
}

struct TokenBucket {
    tokens: f64,
    last_refill: Instant,
}

// ---- ProtectionState ----

pub struct ProtectionState {
    pub connections: DashMap<IpAddr, ConnectionCount>,
    pub bans: DashMap<IpAddr, BanEntry>,
    pub violations: DashMap<IpAddr, ViolationRecord>,
    rate_limits: DashMap<IpAddr, Mutex<TokenBucket>>,
    pub requests_blocked: AtomicU64,
}

impl Default for ProtectionState {
    fn default() -> Self {
        Self {
            connections: DashMap::new(),
            bans: DashMap::new(),
            violations: DashMap::new(),
            rate_limits: DashMap::new(),
            requests_blocked: AtomicU64::new(0),
        }
    }
}

pub type SharedProtection = Arc<ProtectionState>;

impl ProtectionState {
    pub fn new() -> SharedProtection {
        Arc::new(Self::default())
    }

    // Returns Err(retry_after_secs) if banned and ban hasn't expired.
    pub fn check_ban(&self, ip: IpAddr) -> Result<(), (u64, String)> {
        if let Some(entry) = self.bans.get(&ip) {
            if entry.banned_until > Instant::now() {
                let secs = entry
                    .banned_until
                    .saturating_duration_since(Instant::now())
                    .as_secs();
                return Err((secs, entry.reason.clone()));
            }
        }
        self.bans.remove(&ip);
        Ok(())
    }

    // Returns Err(retry_after_ms) if limit exceeded.
    pub fn try_rate_limit(&self, ip: IpAddr, cfg: &ProtectionConfig) -> Result<(), u64> {
        if !cfg.rate_limit_enabled {
            return Ok(());
        }
        let rate = cfg.rate_limit_requests_per_second as f64;
        let burst = cfg.rate_limit_burst as f64;

        let bucket = self.rate_limits.entry(ip).or_insert_with(|| {
            Mutex::new(TokenBucket {
                tokens: burst,
                last_refill: Instant::now(),
            })
        });

        let mut b = bucket.lock();
        let elapsed = b.last_refill.elapsed().as_secs_f64();
        b.tokens = (b.tokens + elapsed * rate).min(burst);
        b.last_refill = Instant::now();

        if b.tokens >= 1.0 {
            b.tokens -= 1.0;
            Ok(())
        } else {
            let wait_secs = (1.0 - b.tokens) / rate;
            Err((wait_secs * 1000.0).ceil() as u64)
        }
    }

    pub fn record_violation(&self, ip: IpAddr, reason: &str, cfg: &ProtectionConfig) {
        metrics::VIOLATIONS.with_label_values(&[reason]).inc();

        let entry = self
            .violations
            .entry(ip)
            .or_insert_with(|| ViolationRecord {
                count: AtomicU32::new(0),
                first_seen: Instant::now(),
            });
        let count = entry.count.fetch_add(1, Ordering::Relaxed) + 1;

        if count >= cfg.ban_threshold_violations {
            let banned_until = Instant::now() + Duration::from_secs(cfg.ban_duration_seconds);
            warn!(
                ip = %ip, reason, duration_secs = cfg.ban_duration_seconds,
                "IP auto-banned after threshold violations"
            );
            self.bans.insert(
                ip,
                BanEntry {
                    banned_until,
                    reason: reason.to_string(),
                    violation_count: count,
                },
            );
            metrics::BANNED_IPS.inc();
        }
    }

    pub fn try_acquire_ws(&self, ip: IpAddr, limit: u32) -> bool {
        let entry = self.connections.entry(ip).or_default();
        let prev = entry.active_ws.fetch_add(1, Ordering::Relaxed);
        if prev + 1 > limit {
            entry.active_ws.fetch_sub(1, Ordering::Relaxed);
            return false;
        }
        true
    }

    pub fn release_ws(&self, ip: IpAddr) {
        if let Some(entry) = self.connections.get(&ip) {
            entry.active_ws.fetch_sub(1, Ordering::Relaxed);
        }
    }

    pub fn unban(&self, ip: IpAddr) -> bool {
        let removed = self.bans.remove(&ip).is_some();
        if removed {
            metrics::BANNED_IPS.dec();
        }
        removed
    }

    pub fn clear_bans(&self) {
        let count = self.bans.len();
        self.bans.clear();
        for _ in 0..count {
            metrics::BANNED_IPS.dec();
        }
    }

    // Remaining ban time for an IP, or None if not banned / expired.
    pub fn ban_remaining_secs(&self, ip: &IpAddr) -> Option<u64> {
        self.bans.get(ip).and_then(|e| {
            let now = Instant::now();
            if e.banned_until > now {
                Some(e.banned_until.saturating_duration_since(now).as_secs())
            } else {
                None
            }
        })
    }
}

// RAII guard that decrements WS count on drop.
pub struct WsGuard {
    ip: IpAddr,
    protection: SharedProtection,
}

impl WsGuard {
    pub fn new(ip: IpAddr, protection: SharedProtection) -> Self {
        Self { ip, protection }
    }
}

impl Drop for WsGuard {
    fn drop(&mut self) {
        self.protection.release_ws(self.ip);
    }
}

// ---- Protection middleware (runs all 7 layers in order) ----

pub async fn middleware(
    protection: SharedProtection,
    gateway: SharedState,
    addr: std::net::SocketAddr,
    req: Request,
    next: Next,
) -> Response {
    let cfg = gateway.read().await.gateway_config.protection.clone();
    let ip = client_ip(&req, addr.ip());
    let path = req.uri().path().to_owned();
    let method = req.method().clone();
    let start = Instant::now();

    // Layer 1: ban check
    if let Err((retry_secs, reason)) = protection.check_ban(ip) {
        warn!(%ip, reason, "blocked banned IP");
        protection.requests_blocked.fetch_add(1, Ordering::Relaxed);
        metrics::REQUESTS_BLOCKED
            .with_label_values(&["ip_banned", &metrics::ip_class(&ip)])
            .inc();
        return ban_response(retry_secs);
    }

    // Layer 5: header size (cheap, no body involved)
    let header_bytes: usize = req
        .headers()
        .iter()
        .map(|(k, v)| k.as_str().len() + v.len() + 4)
        .sum();
    if header_bytes as u64 > cfg.max_header_bytes {
        protection.record_violation(ip, "headers_too_large", &cfg);
        protection.requests_blocked.fetch_add(1, Ordering::Relaxed);
        metrics::REQUESTS_BLOCKED
            .with_label_values(&["headers_too_large", &metrics::ip_class(&ip)])
            .inc();
        return err_response(
            StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE,
            "headers_too_large",
            "request headers exceed maximum allowed size",
        );
    }

    // Layer 4: Content-Length pre-check (before acquiring connection slot)
    if let Some(cl_val) = content_length(&req) {
        if cl_val > cfg.max_body_bytes {
            protection.record_violation(ip, "payload_too_large", &cfg);
            protection.requests_blocked.fetch_add(1, Ordering::Relaxed);
            metrics::REQUESTS_BLOCKED
                .with_label_values(&["payload_too_large", &metrics::ip_class(&ip)])
                .inc();
            return payload_too_large_response(cfg.max_body_bytes);
        }
    }

    // Layer 2: connection limit
    let conn_entry = protection.connections.entry(ip).or_default();
    let prev_http = conn_entry.active_http.fetch_add(1, Ordering::Relaxed);
    drop(conn_entry);

    if prev_http + 1 > cfg.max_connections_per_ip {
        if let Some(entry) = protection.connections.get(&ip) {
            entry.active_http.fetch_sub(1, Ordering::Relaxed);
        }
        protection.record_violation(ip, "too_many_connections", &cfg);
        protection.requests_blocked.fetch_add(1, Ordering::Relaxed);
        metrics::REQUESTS_BLOCKED
            .with_label_values(&["too_many_connections", &metrics::ip_class(&ip)])
            .inc();
        return too_many_connections_response(cfg.max_connections_per_ip);
    }

    // Layer 3: token bucket rate limit
    if let Err(retry_ms) = protection.try_rate_limit(ip, &cfg) {
        if let Some(entry) = protection.connections.get(&ip) {
            entry.active_http.fetch_sub(1, Ordering::Relaxed);
        }
        protection.requests_blocked.fetch_add(1, Ordering::Relaxed);
        metrics::REQUESTS_BLOCKED
            .with_label_values(&["rate_limited", &metrics::ip_class(&ip)])
            .inc();
        return rate_limited_response(
            retry_ms,
            cfg.rate_limit_requests_per_second,
            cfg.rate_limit_burst,
        );
    }

    // Layers 6 & 7: body read timeout + total request timeout
    let body_timeout = Duration::from_millis(cfg.body_read_timeout_ms);
    let total_timeout = Duration::from_millis(cfg.request_timeout_ms);
    let max_body = cfg.max_body_bytes;

    let response = tokio::time::timeout(total_timeout, async {
        let (parts, body) = req.into_parts();

        // Body read with timeout (catches slowloris-style slow body sending)
        let collected = match tokio::time::timeout(body_timeout, async {
            http_body_util::BodyExt::collect(body).await
        })
        .await
        {
            Ok(Ok(b)) => b.to_bytes(),
            _ => return request_timeout_response(),
        };

        // Layer 4 (streaming): enforce body size when no Content-Length
        if matches!(parts.method, Method::POST | Method::PUT | Method::PATCH)
            && collected.len() as u64 > max_body
        {
            return payload_too_large_response(max_body);
        }

        let req = Request::from_parts(parts, Body::from(collected));
        next.run(req).await
    })
    .await;

    // Always release http connection slot
    if let Some(entry) = protection.connections.get(&ip) {
        entry.active_http.fetch_sub(1, Ordering::Relaxed);
    }

    let elapsed_ms = start.elapsed().as_millis() as f64;

    let resp = match response {
        Ok(r) => r,
        Err(_) => {
            protection.record_violation(ip, "request_timeout", &cfg);
            protection.requests_blocked.fetch_add(1, Ordering::Relaxed);
            metrics::REQUESTS_BLOCKED
                .with_label_values(&["request_timeout", &metrics::ip_class(&ip)])
                .inc();
            gateway_timeout_response()
        }
    };

    metrics::REQUEST_DURATION
        .with_label_values(&[
            method.as_str(),
            metrics::path_pattern(&path),
            status_class(resp.status().as_u16()),
        ])
        .observe(elapsed_ms);

    resp
}

// ---- Helpers ----

fn client_ip(req: &Request, socket_ip: IpAddr) -> IpAddr {
    req.headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next_back())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(socket_ip)
}

fn content_length(req: &Request) -> Option<u64> {
    req.headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse().ok())
}

fn status_class(code: u16) -> &'static str {
    match code / 100 {
        2 => "2xx",
        3 => "3xx",
        4 => "4xx",
        _ => "5xx",
    }
}

fn json_body(body: serde_json::Value) -> Body {
    Body::from(body.to_string())
}

fn ban_response(retry_secs: u64) -> Response {
    let mut resp = Response::builder()
        .status(StatusCode::FORBIDDEN)
        .header("content-type", "application/json")
        .body(json_body(serde_json::json!({
            "error": "ip_banned",
            "message": "temporarily blocked due to repeated violations",
            "retry_after": retry_secs,
            "correlationId": Ulid::new().to_string(),
        })))
        .unwrap();
    if let Ok(v) = HeaderValue::from_str(&retry_secs.to_string()) {
        resp.headers_mut().insert("retry-after", v);
    }
    resp
}

fn too_many_connections_response(limit: u32) -> Response {
    Response::builder()
        .status(StatusCode::TOO_MANY_REQUESTS)
        .header("content-type", "application/json")
        .body(json_body(serde_json::json!({
            "error": "too_many_connections",
            "message": "too many concurrent connections from this IP",
            "limit": limit,
            "correlationId": Ulid::new().to_string(),
        })))
        .unwrap()
}

fn rate_limited_response(retry_ms: u64, rps: u32, burst: u32) -> Response {
    let retry_secs = (retry_ms as f64 / 1000.0).ceil() as u64;
    let mut resp = Response::builder()
        .status(StatusCode::TOO_MANY_REQUESTS)
        .header("content-type", "application/json")
        .body(json_body(serde_json::json!({
            "error": "rate_limited",
            "message": "request rate limit exceeded",
            "retry_after_ms": retry_ms,
            "limit": rps,
            "burst": burst,
            "correlationId": Ulid::new().to_string(),
        })))
        .unwrap();
    if let Ok(v) = HeaderValue::from_str(&retry_secs.to_string()) {
        resp.headers_mut().insert("retry-after", v);
    }
    resp
}

fn payload_too_large_response(max_bytes: u64) -> Response {
    Response::builder()
        .status(StatusCode::PAYLOAD_TOO_LARGE)
        .header("content-type", "application/json")
        .body(json_body(serde_json::json!({
            "error": "payload_too_large",
            "message": "request body exceeds maximum allowed size",
            "max_bytes": max_bytes,
            "correlationId": Ulid::new().to_string(),
        })))
        .unwrap()
}

fn err_response(status: StatusCode, code: &str, message: &str) -> Response {
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(json_body(serde_json::json!({
            "error": code,
            "message": message,
            "correlationId": Ulid::new().to_string(),
        })))
        .unwrap()
}

fn request_timeout_response() -> Response {
    Response::builder()
        .status(StatusCode::REQUEST_TIMEOUT)
        .header("content-type", "application/json")
        .body(json_body(serde_json::json!({
            "error": "request_timeout",
            "correlationId": Ulid::new().to_string(),
        })))
        .unwrap()
}

fn gateway_timeout_response() -> Response {
    Response::builder()
        .status(StatusCode::GATEWAY_TIMEOUT)
        .header("content-type", "application/json")
        .body(json_body(serde_json::json!({
            "error": "gateway_timeout",
            "message": "upstream did not respond in time",
            "correlationId": Ulid::new().to_string(),
        })))
        .unwrap()
}

pub fn ws_rejected_response(limit: u32) -> Response {
    Response::builder()
        .status(StatusCode::TOO_MANY_REQUESTS)
        .header("content-type", "application/json")
        .body(json_body(serde_json::json!({
            "error": "too_many_websocket_connections",
            "message": "websocket connection limit reached for this IP",
            "limit": limit,
            "correlationId": Ulid::new().to_string(),
        })))
        .unwrap()
}
