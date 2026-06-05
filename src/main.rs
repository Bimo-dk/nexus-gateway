mod headers;
mod health;
mod http_client;
mod metrics;
mod protection;
mod proxy;
mod registry_listener;
mod route_table;
mod spa;
mod startup;
mod state;
mod ws_proxy;

#[cfg(test)]
mod tests {
    mod headers_tests;
    mod health_tests;
    mod protection_tests;
    mod route_table_tests;
    mod spa_tests;
    mod startup_tests;
}

use axum::{
    extract::{ConnectInfo, Path, Request, State, WebSocketUpgrade},
    http::StatusCode,
    middleware,
    response::{IntoResponse, Response},
    routing::{delete, get, post},
    Router,
};
use serde::Deserialize;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tower_http::{compression::CompressionLayer, trace::TraceLayer};
use tracing::info;
use tracing_subscriber::EnvFilter;

use crate::protection::{BanEntry, ProtectionState, SharedProtection, WsGuard};
use crate::route_table::RouteTable;
use crate::state::{new_shared, SharedState};

#[derive(Clone)]
pub struct AppState {
    pub gateway: SharedState,
    pub routes: RouteTable,
    pub proxy_client: proxy::ProxyClient,
    pub http_client: http_client::HyperClient,
    pub protection: SharedProtection,
}

#[tokio::main]
async fn main() {
    let use_json = std::env::var("LOG_JSON")
        .map(|v| v == "1" || v == "true")
        .unwrap_or(false);
    if use_json {
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(EnvFilter::from_default_env())
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(
                EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()),
            )
            .init();
    }

    dotenvy::dotenv().ok();

    metrics::init();

    let env = startup::read_env().unwrap_or_else(|e| {
        eprintln!("startup error: {e}");
        std::process::exit(1);
    });

    let (gateway_state, route_table) = startup::bootstrap(&env).await.unwrap_or_else(|e| {
        eprintln!("bootstrap error: {e}");
        std::process::exit(1);
    });

    let app_state = AppState {
        gateway: new_shared(gateway_state),
        routes: route_table,
        proxy_client: proxy::build_client(),
        http_client: http_client::build(),
        protection: ProtectionState::new(),
    };

    // Registry WebSocket listener
    {
        let s = app_state.clone();
        tokio::spawn(async move {
            registry_listener::run(s.gateway, s.routes, s.http_client).await;
        });
    }

    // Build protection middleware closure
    let protection_mw = {
        let prot = app_state.protection.clone();
        let gway = app_state.gateway.clone();
        middleware::from_fn(move |req: Request, next: middleware::Next| {
            let prot = prot.clone();
            let gway = gway.clone();
            async move {
                let addr = req
                    .extensions()
                    .get::<ConnectInfo<SocketAddr>>()
                    .map(|ci| ci.0)
                    .unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], 0)));
                protection::middleware(prot, gway, addr, req, next).await
            }
        })
    };

    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/ws", get(ws_handler))
        .route("/metrics", get(metrics_handler))
        .route("/api/protection/status", get(protection_status))
        .route("/api/protection/ban", post(ban_ip))
        .route("/api/protection/ban/:ip", delete(unban_ip))
        .route("/api/protection/bans", delete(clear_bans))
        .fallback(fallback_handler)
        .with_state(app_state)
        .layer(protection_mw)
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http());

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(8668);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    info!(%addr, "nexus-gateway listening");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .unwrap_or_else(|e| {
            eprintln!("bind error: {e}");
            std::process::exit(1);
        });

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap_or_else(|e| {
        eprintln!("server error: {e}");
        std::process::exit(1);
    });
}

// ---- Handlers ----

async fn health_handler(State(app): State<AppState>) -> impl IntoResponse {
    health::handler(app).await
}

async fn ws_handler(
    State(app): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    ws: WebSocketUpgrade,
    req: Request,
) -> impl IntoResponse {
    let query = req
        .uri()
        .query()
        .map(|q| format!("?{}", q))
        .unwrap_or_default();
    let ip = addr.ip();
    let (limit, ws_url) = {
        let s = app.gateway.read().await;
        let limit = s.gateway_config.protection.max_websocket_connections_per_ip;
        let base = s.registry_url.trim_end_matches('/');
        let url = base
            .replacen("https://", "wss://", 1)
            .replacen("http://", "ws://", 1);
        (limit, format!("{}/ws{}", url, query))
    };

    if !app.protection.try_acquire_ws(ip, limit) {
        metrics::REQUESTS_BLOCKED
            .with_label_values(&["too_many_websocket_connections", &metrics::ip_class(&ip)])
            .inc();
        return protection::ws_rejected_response(limit).into_response();
    }

    let guard = WsGuard::new(ip, app.protection.clone());
    ws.on_upgrade(move |socket| async move {
        let _guard = guard;
        ws_proxy::pipe_socket(socket, ws_url).await;
    })
    .into_response()
}

async fn metrics_handler() -> impl IntoResponse {
    (
        StatusCode::OK,
        [("content-type", "text/plain; version=0.0.4")],
        metrics::gather_text(),
    )
}

// Request must be the last extractor.
async fn fallback_handler(
    State(app): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req: Request,
) -> impl IntoResponse {
    let path = req.uri().path().to_owned();
    if app.routes.resolve(&path).is_some() {
        proxy::handler(addr, app, req).await.into_response()
    } else {
        spa::handler(app.gateway).await.into_response()
    }
}

// ---- Ban management ----

async fn protection_status(State(app): State<AppState>) -> impl IntoResponse {
    let now = Instant::now();
    let prot = &app.protection;

    let active_bans: Vec<_> = prot
        .bans
        .iter()
        .filter(|e| e.banned_until > now)
        .map(|e| {
            let remaining_secs = e.banned_until.saturating_duration_since(now).as_secs();
            serde_json::json!({
                "ip": e.key().to_string(),
                "remaining_seconds": remaining_secs,
                "reason": e.value().reason,
                "violation_count": e.value().violation_count,
            })
        })
        .collect();

    let mut top_ips: Vec<_> = prot
        .violations
        .iter()
        .map(|e| {
            let ip = *e.key();
            let vc = e.value().count.load(Ordering::Relaxed);
            let (http, ws) = prot
                .connections
                .get(&ip)
                .map(|c| {
                    (
                        c.active_http.load(Ordering::Relaxed),
                        c.active_ws.load(Ordering::Relaxed),
                    )
                })
                .unwrap_or((0, 0));
            let is_banned = prot
                .bans
                .get(&ip)
                .map(|b| b.banned_until > now)
                .unwrap_or(false);
            (ip, vc, http, ws, is_banned)
        })
        .collect();
    top_ips.sort_by_key(|b| std::cmp::Reverse(b.1));
    top_ips.truncate(10);

    let top_ips_json: Vec<_> = top_ips
        .into_iter()
        .map(|(ip, vc, http, ws, banned)| {
            serde_json::json!({
                "ip": ip.to_string(),
                "violation_count": vc,
                "active_connections_http": http,
                "active_connections_ws": ws,
                "is_banned": banned,
            })
        })
        .collect();

    let cfg = {
        let s = app.gateway.read().await;
        serde_json::to_value(&s.gateway_config.protection).unwrap_or_default()
    };

    axum::Json(serde_json::json!({
        "active_bans": active_bans,
        "top_ips": top_ips_json,
        "rate_limit_config": cfg,
        "total_requests_blocked_since_start": prot.requests_blocked.load(Ordering::Relaxed),
    }))
}

#[derive(Deserialize)]
struct BanRequest {
    ip: String,
    duration_seconds: Option<u64>,
}

async fn ban_ip(State(app): State<AppState>, req: Request) -> Response {
    let token = req
        .headers()
        .get("x-nexus-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    if !is_authed(&token, &app).await {
        return (StatusCode::UNAUTHORIZED, "Missing or invalid X-Nexus-Token").into_response();
    }
    let body = axum::body::to_bytes(req.into_body(), 16_384)
        .await
        .unwrap_or_default();
    let ban_req: BanRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => return (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    };
    let ip: IpAddr = match ban_req.ip.parse() {
        Ok(ip) => ip,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid IP address").into_response(),
    };
    let duration = {
        let s = app.gateway.read().await;
        ban_req
            .duration_seconds
            .unwrap_or(s.gateway_config.protection.ban_duration_seconds)
    };
    app.protection.bans.insert(
        ip,
        BanEntry {
            banned_until: Instant::now() + Duration::from_secs(duration),
            reason: "manual ban".into(),
            violation_count: 0,
        },
    );
    metrics::BANNED_IPS.inc();
    (
        StatusCode::OK,
        axum::Json(serde_json::json!({
            "banned": ip.to_string(),
            "duration_seconds": duration,
        })),
    )
        .into_response()
}

async fn unban_ip(
    State(app): State<AppState>,
    Path(ip_str): Path<String>,
    req: Request,
) -> Response {
    let token = req
        .headers()
        .get("x-nexus-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    if !is_authed(&token, &app).await {
        return (StatusCode::UNAUTHORIZED, "Missing or invalid X-Nexus-Token").into_response();
    }
    let ip: IpAddr = match ip_str.parse() {
        Ok(ip) => ip,
        Err(_) => return (StatusCode::BAD_REQUEST, "invalid IP address").into_response(),
    };
    let removed = app.protection.unban(ip);
    (
        StatusCode::OK,
        axum::Json(serde_json::json!({
            "unbanned": ip.to_string(),
            "was_banned": removed,
        })),
    )
        .into_response()
}

async fn clear_bans(State(app): State<AppState>, req: Request) -> Response {
    let token = req
        .headers()
        .get("x-nexus-token")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_owned();
    if !is_authed(&token, &app).await {
        return (StatusCode::UNAUTHORIZED, "Missing or invalid X-Nexus-Token").into_response();
    }
    let count = app.protection.bans.len();
    app.protection.clear_bans();
    (
        StatusCode::OK,
        axum::Json(serde_json::json!({ "cleared": count })),
    )
        .into_response()
}

async fn is_authed(token: &str, app: &AppState) -> bool {
    let expected = app.gateway.read().await.nexus_token.clone();
    !expected.is_empty() && constant_time_eq(token.as_bytes(), expected.as_bytes())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}
