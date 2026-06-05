use axum::body::Body;
use axum::http::{HeaderName, HeaderValue, Response};

// Headers that custom_headers from GatewayConfig must not override
const LOCKED: &[&str] = &[
    "x-frame-options",
    "x-content-type-options",
    "x-xss-protection",
    "referrer-policy",
    "permissions-policy",
];

pub fn apply_security_headers(mut response: Response<Body>) -> Response<Body> {
    let headers = response.headers_mut();

    set(headers, "x-frame-options", "SAMEORIGIN");
    set(headers, "x-content-type-options", "nosniff");
    set(headers, "x-xss-protection", "1; mode=block");
    set(
        headers,
        "referrer-policy",
        "strict-origin-when-cross-origin",
    );
    set(
        headers,
        "permissions-policy",
        "camera=(), microphone=(), geolocation=()",
    );

    response
}

pub fn apply_custom_headers(
    response: &mut Response<Body>,
    custom_headers: &[crate::state::CustomHeader],
) {
    let hmap = response.headers_mut();
    for ch in custom_headers {
        if LOCKED.contains(&ch.name.to_lowercase().as_str()) {
            continue;
        }
        if let (Ok(name), Ok(value)) = (
            ch.name.parse::<HeaderName>(),
            HeaderValue::from_str(&ch.value),
        ) {
            hmap.insert(name, value);
        }
    }
}

pub fn cache_control_value(path: &str) -> &'static str {
    let lower = path.to_lowercase();
    if lower.ends_with("remoteentry.json") || lower.ends_with("remoteentry.js") {
        return "no-store, no-cache, must-revalidate";
    }
    if is_immutable_asset(path) {
        return "public, max-age=31536000, immutable";
    }
    "no-cache"
}

fn is_immutable_asset(path: &str) -> bool {
    if !path.starts_with("/assets/") {
        return false;
    }
    let lower = path.to_lowercase();
    lower.ends_with(".js") || lower.ends_with(".css") || lower.ends_with(".woff2")
}

fn set(headers: &mut axum::http::HeaderMap, name: &'static str, value: &'static str) {
    headers.insert(
        HeaderName::from_static(name),
        HeaderValue::from_static(value),
    );
}
