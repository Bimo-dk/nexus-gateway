use axum::{
    body::Body,
    http::{HeaderName, HeaderValue, StatusCode},
    response::Response,
};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::Uri;
use hyper_util::client::legacy::{connect::HttpConnector, Client};
use hyper_util::rt::TokioExecutor;
use std::net::SocketAddr;
use std::str::FromStr;
use tracing::warn;
use ulid::Ulid;

use crate::headers::{apply_custom_headers, apply_security_headers, cache_control_value};
use crate::AppState;

pub type ProxyClient = Client<HttpConnector, Full<Bytes>>;

pub fn build_client() -> ProxyClient {
    Client::builder(TokioExecutor::new()).build(HttpConnector::new())
}

pub async fn handler(
    addr: SocketAddr,
    app: AppState,
    req: axum::extract::Request,
) -> Response<Body> {
    let path = req.uri().path().to_owned();
    let query = req
        .uri()
        .query()
        .map(|q| format!("?{}", q))
        .unwrap_or_default();

    let Some((prefix, target)) = app.routes.resolve(&path) else {
        return not_found();
    };

    let stripped = path.strip_prefix(&prefix).unwrap_or(&path);
    let upstream_path = format!("/{}{}", stripped.trim_start_matches('/'), query);
    let upstream_base = target.upstream_url.trim_end_matches('/');
    let upstream_url = format!("{}{}", upstream_base, upstream_path);

    let uri = match Uri::from_str(&upstream_url) {
        Ok(u) => u,
        Err(e) => {
            warn!(error = %e, url = upstream_url, "invalid upstream URL");
            return error_response(StatusCode::BAD_GATEWAY, "bad_gateway");
        }
    };

    let request_id = Ulid::new().to_string();
    let (mut parts, body) = req.into_parts();
    parts.uri = uri;

    let body_bytes = match body.collect().await {
        Ok(b) => b.to_bytes(),
        Err(e) => {
            warn!(error = %e, "failed to read request body");
            return error_response(StatusCode::BAD_REQUEST, "body_read_error");
        }
    };

    let mut upstream_req = hyper::Request::from_parts(parts, Full::new(body_bytes));
    let headers = upstream_req.headers_mut();

    if let Ok(v) = HeaderValue::from_str(&addr.ip().to_string()) {
        headers.insert(HeaderName::from_static("x-forwarded-for"), v);
    }
    headers.insert(
        HeaderName::from_static("x-nexus-gateway"),
        HeaderValue::from_static("true"),
    );
    if !headers.contains_key("x-request-id") {
        if let Ok(v) = HeaderValue::from_str(&request_id) {
            headers.insert(HeaderName::from_static("x-request-id"), v);
        }
    }

    let upstream_resp = match app.proxy_client.request(upstream_req).await {
        Ok(r) => r,
        Err(e) => {
            warn!(error = %e, upstream = upstream_url, "upstream request failed");
            return error_response(StatusCode::BAD_GATEWAY, "upstream_error");
        }
    };

    let status = upstream_resp.status();
    let mut builder = Response::builder().status(status);
    for (k, v) in upstream_resp.headers() {
        builder = builder.header(k, v);
    }

    let cc = cache_control_value(&path);
    builder = builder.header("cache-control", cc);
    if cc.contains("no-store") {
        builder = builder.header("pragma", "no-cache");
    }

    let resp_bytes = upstream_resp
        .into_body()
        .collect()
        .await
        .map(|b| b.to_bytes())
        .unwrap_or_default();

    let mut response = builder.body(Body::from(resp_bytes)).unwrap_or_else(|_| {
        error_response(StatusCode::INTERNAL_SERVER_ERROR, "response_build_error")
    });

    {
        let s = app.gateway.read().await;
        apply_custom_headers(&mut response, &s.gateway_config.custom_headers);
    }

    apply_security_headers(response)
}

fn not_found() -> Response<Body> {
    let body = serde_json::json!({
        "error": "not_found",
        "correlationId": Ulid::new().to_string(),
    });
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn error_response(status: StatusCode, code: &'static str) -> Response<Body> {
    let body = serde_json::json!({
        "error": code,
        "correlationId": Ulid::new().to_string(),
    });
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}
