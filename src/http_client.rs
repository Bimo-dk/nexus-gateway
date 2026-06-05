use anyhow::{Context, Result};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{Method, Request, Uri};
use hyper_util::client::legacy::{connect::HttpConnector, Client};
use hyper_util::rt::TokioExecutor;

pub type HyperClient = Client<HttpConnector, Full<Bytes>>;

pub fn build() -> HyperClient {
    Client::builder(TokioExecutor::new()).build(HttpConnector::new())
}

pub async fn get_json(client: &HyperClient, url: &str, token: &str) -> Result<serde_json::Value> {
    let uri: Uri = url.parse().with_context(|| format!("invalid URL: {url}"))?;

    let req = Request::builder()
        .method(Method::GET)
        .uri(uri)
        .header("X-Nexus-Token", token)
        .header("Accept", "application/json")
        .body(Full::new(Bytes::new()))
        .context("failed to build request")?;

    send(client, url, req).await
}

pub async fn post_json(
    client: &HyperClient,
    url: &str,
    token: &str,
    body: serde_json::Value,
) -> Result<serde_json::Value> {
    let uri: Uri = url.parse().with_context(|| format!("invalid URL: {url}"))?;
    let body_bytes = Bytes::from(serde_json::to_vec(&body).context("failed to serialise body")?);

    let req = Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header("X-Nexus-Token", token)
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .body(Full::new(body_bytes))
        .context("failed to build request")?;

    send(client, url, req).await
}

async fn send(
    client: &HyperClient,
    url: &str,
    req: Request<Full<Bytes>>,
) -> Result<serde_json::Value> {
    let resp = client
        .request(req)
        .await
        .with_context(|| format!("request to {url} failed"))?;

    let status = resp.status();
    let body_bytes = resp
        .into_body()
        .collect()
        .await
        .context("failed to read response body")?
        .to_bytes();

    if !status.is_success() {
        anyhow::bail!(
            "registry returned {} for {}: {}",
            status,
            url,
            String::from_utf8_lossy(&body_bytes)
        );
    }

    let value = serde_json::from_slice(&body_bytes)
        .with_context(|| format!("failed to parse JSON from {url}"))?;

    Ok(value)
}
