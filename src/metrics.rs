use once_cell::sync::Lazy;
use prometheus::{
    register_counter_vec, register_gauge, register_histogram_vec, register_int_counter_vec,
    CounterVec, Encoder, Gauge, HistogramVec, IntCounterVec, TextEncoder,
};

pub static REQUESTS_BLOCKED: Lazy<CounterVec> = Lazy::new(|| {
    register_counter_vec!(
        "nexus_gateway_requests_blocked_total",
        "Total requests blocked by protection layers",
        &["reason", "ip_class"]
    )
    .unwrap()
});

pub static ACTIVE_CONNECTIONS: Lazy<IntCounterVec> = Lazy::new(|| {
    register_int_counter_vec!(
        "nexus_gateway_active_connections",
        "Active connections by type (use gauge pattern via inc/dec)",
        &["type"]
    )
    .unwrap()
});

pub static BANNED_IPS: Lazy<Gauge> = Lazy::new(|| {
    register_gauge!(
        "nexus_gateway_banned_ips_total",
        "Number of currently active IP bans"
    )
    .unwrap()
});

pub static VIOLATIONS: Lazy<CounterVec> = Lazy::new(|| {
    register_counter_vec!(
        "nexus_gateway_violations_total",
        "Total violations recorded by protection layer",
        &["reason"]
    )
    .unwrap()
});

pub static REQUEST_DURATION: Lazy<HistogramVec> = Lazy::new(|| {
    register_histogram_vec!(
        "nexus_gateway_request_duration_ms",
        "Request duration in milliseconds",
        &["method", "path_pattern", "status_class"],
        vec![5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 5000.0, 30000.0]
    )
    .unwrap()
});

pub fn init() {
    // Force lazy statics to initialise so they appear in the registry from start
    Lazy::force(&REQUESTS_BLOCKED);
    Lazy::force(&ACTIVE_CONNECTIONS);
    Lazy::force(&BANNED_IPS);
    Lazy::force(&VIOLATIONS);
    Lazy::force(&REQUEST_DURATION);
}

pub fn gather_text() -> String {
    let encoder = TextEncoder::new();
    let mf = prometheus::gather();
    let mut buf = Vec::new();
    encoder.encode(&mf, &mut buf).unwrap_or_default();
    String::from_utf8(buf).unwrap_or_default()
}

/// Coarsen a path to a fixed set of patterns to avoid cardinality explosion.
pub fn path_pattern(path: &str) -> &'static str {
    if path.starts_with("/api/") {
        "/api/*"
    } else if path.starts_with("/remotes/") {
        "/remotes/:name/*"
    } else if path.starts_with("/host/") {
        "/host/*"
    } else if path == "/ws" {
        "/ws"
    } else if path.starts_with("/assets/") {
        "/assets/*"
    } else {
        "other"
    }
}

/// Privacy-preserving IP class: first two octets of IPv4, first group of IPv6.
pub fn ip_class(ip: &std::net::IpAddr) -> String {
    match ip {
        std::net::IpAddr::V4(v4) => {
            let octets = v4.octets();
            format!("{}.{}", octets[0], octets[1])
        }
        std::net::IpAddr::V6(v6) => {
            let segments = v6.segments();
            format!("{:x}", segments[0])
        }
    }
}
