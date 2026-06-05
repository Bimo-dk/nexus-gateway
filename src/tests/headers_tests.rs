use axum::body::Body;
use axum::http::{Response, StatusCode};
use crate::headers::{apply_security_headers, apply_custom_headers, cache_control_value};
use crate::state::CustomHeader;

fn empty_response() -> Response<Body> {
    Response::builder().status(StatusCode::OK).body(Body::empty()).unwrap()
}

#[test]
fn security_headers_present() {
    let resp = apply_security_headers(empty_response());
    let h = resp.headers();
    assert_eq!(h["x-frame-options"], "SAMEORIGIN");
    assert_eq!(h["x-content-type-options"], "nosniff");
    assert_eq!(h["x-xss-protection"], "1; mode=block");
    assert_eq!(h["referrer-policy"], "strict-origin-when-cross-origin");
    assert_eq!(h["permissions-policy"], "camera=(), microphone=(), geolocation=()");
}

#[test]
fn custom_headers_cannot_override_security_headers() {
    let mut resp = empty_response();
    let custom = vec![
        CustomHeader { name: "x-frame-options".into(), value: "ALLOW-FROM evil.com".into() },
        CustomHeader { name: "x-extra".into(), value: "ok".into() },
    ];
    apply_custom_headers(&mut resp, &custom);
    // security headers applied after — they always win
    let resp = apply_security_headers(resp);
    assert_eq!(resp.headers()["x-frame-options"], "SAMEORIGIN");
    assert_eq!(resp.headers()["x-extra"], "ok");
}

#[test]
fn remote_entry_gets_no_store() {
    assert!(cache_control_value("/host/remoteEntry.json").contains("no-store"));
    assert!(cache_control_value("/remotes/checkout/remoteEntry.js").contains("no-store"));
}

#[test]
fn assets_get_immutable() {
    assert_eq!(
        cache_control_value("/assets/main.abc123.js"),
        "public, max-age=31536000, immutable"
    );
    assert_eq!(
        cache_control_value("/assets/style.min.css"),
        "public, max-age=31536000, immutable"
    );
    assert_eq!(
        cache_control_value("/assets/font.woff2"),
        "public, max-age=31536000, immutable"
    );
}

#[test]
fn other_paths_get_no_cache() {
    assert_eq!(cache_control_value("/"), "no-cache");
    assert_eq!(cache_control_value("/api/gates"), "no-cache");
}
