use axum::{extract::ws::WebSocket, response::Response};
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::connect_async;
use tracing::warn;

use crate::state::SharedState;

pub async fn handler(
    ws: axum::extract::WebSocketUpgrade,
    gateway: SharedState,
    query: String,
) -> Response {
    let registry_ws_url = {
        let s = gateway.read().await;
        let base = s.registry_url.trim_end_matches('/');
        let ws_base = base
            .replacen("https://", "wss://", 1)
            .replacen("http://", "ws://", 1);
        format!("{}/ws{}", ws_base, query)
    };

    ws.on_upgrade(move |client_socket| pipe(client_socket, registry_ws_url))
}

pub async fn pipe_socket(client: WebSocket, upstream_url: String) {
    pipe(client, upstream_url).await
}

async fn pipe(client: WebSocket, upstream_url: String) {
    let (upstream_ws, _) = match connect_async(&upstream_url).await {
        Ok(pair) => pair,
        Err(e) => {
            warn!(error = %e, url = upstream_url, "failed to connect to registry WS");
            return;
        }
    };

    let (mut client_tx, mut client_rx) = client.split();
    let (mut up_tx, mut up_rx) = upstream_ws.split();

    let c2u = tokio::spawn(async move {
        while let Some(Ok(msg)) = client_rx.next().await {
            if up_tx.send(axum_to_tung(msg)).await.is_err() {
                break;
            }
        }
    });

    let u2c = tokio::spawn(async move {
        while let Some(Ok(msg)) = up_rx.next().await {
            if client_tx.send(tung_to_axum(msg)).await.is_err() {
                break;
            }
        }
    });

    tokio::select! {
        _ = c2u => {}
        _ = u2c => {}
    }
}

fn axum_to_tung(msg: axum::extract::ws::Message) -> tokio_tungstenite::tungstenite::Message {
    use axum::extract::ws::Message as A;
    use tokio_tungstenite::tungstenite::Message as T;
    match msg {
        A::Text(t) => T::Text(t.to_string()),
        A::Binary(b) => T::Binary(b.to_vec()),
        A::Ping(p) => T::Ping(p.to_vec()),
        A::Pong(p) => T::Pong(p.to_vec()),
        A::Close(_) => T::Close(None),
    }
}

fn tung_to_axum(msg: tokio_tungstenite::tungstenite::Message) -> axum::extract::ws::Message {
    use axum::extract::ws::Message as A;
    use tokio_tungstenite::tungstenite::Message as T;
    match msg {
        T::Text(t) => A::Text(t.into()),
        T::Binary(b) => A::Binary(b.into()),
        T::Ping(p) => A::Ping(p.into()),
        T::Pong(p) => A::Pong(p.into()),
        T::Close(_) | T::Frame(_) => A::Close(None),
    }
}
