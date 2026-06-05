use std::time::Duration;
use futures_util::{SinkExt, StreamExt};
use rand::Rng;
use serde::Deserialize;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{info, warn};

use crate::http_client::{self, HyperClient};
use crate::route_table::{RouteTable, UpstreamTarget};
use crate::startup::{build_route_table, is_visible};
use crate::state::{RegistryGate, RegistryRemote, SharedState};

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WsMessage {
    RemotesChanged { remotes: Vec<RegistryRemote> },
    HostChanged { host: HostChangedPayload },
    GateChanged { gate: GateChangedPayload },
    ConfigChanged { section: String, value: serde_json::Value },
    ReconnectPolicyChanged { policy: ReconnectPolicyPayload },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HostChangedPayload {
    id: String,
    url: Option<String>,
    framework: Option<crate::state::HostFramework>,
    remote_entry: Option<String>,
    exposed_module: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReconnectPolicyPayload {
    initial_delay_ms: u64,
    max_delay_ms: u64,
}

#[derive(Debug, Deserialize)]
struct GateChangedPayload {
    id: String,
}

pub async fn run(state: SharedState, routes: RouteTable, http: HyperClient) {
    let mut base_delay = Duration::from_secs(1);
    let mut max_delay = Duration::from_secs(30);

    loop {
        let ws_url = {
            let s = state.read().await;
            let base = s.registry_url.trim_end_matches('/');
            let ws_base = base
                .replacen("https://", "wss://", 1)
                .replacen("http://", "ws://", 1);
            format!("{}/api/ws?token={}", ws_base, s.nexus_token)
        };

        match connect_async(&ws_url).await {
            Ok((ws_stream, _)) => {
                {
                    let mut s = state.write().await;
                    s.registry_connected = true;
                }
                info!(url = ws_url, "connected to registry WebSocket");

                // Subscribe to gate events
                let (mut sink, mut stream) = ws_stream.split();
                let gate_name = state.read().await.gate_name.clone();
                let subscribe_msg = serde_json::json!({
                    "type": "subscribe_gate",
                    "gate_name": gate_name,
                });
                if let Err(e) = sink.send(Message::Text(subscribe_msg.to_string())).await {
                    warn!(error = %e, "failed to send subscribe_gate");
                }

                while let Some(result) = stream.next().await {
                    match result {
                        Ok(Message::Text(text)) => {
                            if let Err(e) = handle_message(
                                &text,
                                &state,
                                &routes,
                                &http,
                                &mut base_delay,
                                &mut max_delay,
                            )
                            .await
                            {
                                warn!(error = %e, "error handling WS message");
                            }
                        }
                        Ok(Message::Close(_)) => {
                            info!("registry closed WebSocket connection");
                            break;
                        }
                        Err(e) => {
                            warn!(error = %e, "WebSocket error");
                            break;
                        }
                        _ => {}
                    }
                }

                {
                    let mut s = state.write().await;
                    s.registry_connected = false;
                }
            }
            Err(e) => {
                warn!(error = %e, url = ws_url, "failed to connect to registry WebSocket");
            }
        }

        let jitter_ms = rand::thread_rng().gen_range(0u64..=500);
        let sleep = std::cmp::min(base_delay, max_delay) + Duration::from_millis(jitter_ms);
        info!(sleep_ms = sleep.as_millis(), "reconnecting after backoff");
        tokio::time::sleep(sleep).await;
        base_delay = std::cmp::min(base_delay.saturating_mul(2), max_delay);
    }
}

async fn handle_message(
    text: &str,
    state: &SharedState,
    routes: &RouteTable,
    http: &HyperClient,
    base_delay: &mut Duration,
    max_delay: &mut Duration,
) -> anyhow::Result<()> {
    let msg: WsMessage = match serde_json::from_str(text) {
        Ok(m) => m,
        Err(e) => {
            warn!(error = %e, "unrecognised WS message: {}", &text[..text.len().min(200)]);
            return Ok(());
        }
    };

    match msg {
        WsMessage::RemotesChanged { remotes } => {
            let host_id = state.read().await.host_id.clone();
            routes.clear_remotes();
            for remote in &remotes {
                if !is_visible(remote, &host_id) {
                    continue;
                }
                let prefix = format!("/remotes/{}/", remote.route_path.trim_matches('/'));
                routes.upsert(prefix.clone(), UpstreamTarget {
                    upstream_url: remote.url.clone(),
                    enabled: remote.enabled,
                });
                info!(prefix, url = %remote.url, "remote route upserted");
            }
        }

        WsMessage::HostChanged { host } => {
            let host_id = state.read().await.host_id.clone();
            if host.id != host_id {
                return Ok(());
            }
            let mut s = state.write().await;
            if let Some(url) = host.url {
                info!(old = %s.host_url, new = %url, "host_url changed");
                s.host_url = url.clone();
                routes.upsert("/host/", UpstreamTarget { upstream_url: url, enabled: true });
            }
            if let Some(fw) = host.framework {
                info!(framework = %fw, "host_framework changed");
                s.host_framework = fw;
            }
            if let Some(re) = host.remote_entry {
                s.host_remote_entry = re;
            }
            if let Some(em) = host.exposed_module {
                s.host_exposed_module = em;
            }
        }

        WsMessage::GateChanged { gate } => {
            let gate_id = state.read().await.gate_id.clone();
            if gate.id != gate_id {
                return Ok(());
            }
            // Re-fetch the full gate to handle host reassignment
            let (registry_url, token) = {
                let s = state.read().await;
                (s.registry_url.clone(), s.nexus_token.clone())
            };
            let gate_url = format!(
                "{}/api/gates/{}",
                registry_url.trim_end_matches('/'),
                gate.id
            );
            let gate_json = http_client::get_json(http, &gate_url, &token).await?;
            let full_gate: RegistryGate = serde_json::from_value(gate_json)?;

            let remotes: Vec<RegistryRemote> = if let Some(ref host) = full_gate.host {
                let remotes_url = format!(
                    "{}/api/hosts/{}/remotes",
                    registry_url.trim_end_matches('/'),
                    host.id
                );
                let mut remotes_json = http_client::get_json(http, &remotes_url, &token).await?;
                serde_json::from_value(remotes_json["remotes"].take())?
            } else {
                vec![]
            };

            let mut s = state.write().await;
            s.gate_id = full_gate.id;
            if let Some(host) = full_gate.host {
                s.host_id = host.id;
                s.host_name = host.name;
                s.host_url = host.url;
                s.host_framework = host.framework;
                s.host_remote_entry = host.remote_entry;
                s.host_exposed_module = host.exposed_module;
            } else {
                s.host_id = String::new();
                s.host_name = String::new();
                s.host_url = String::new();
                s.host_framework = Default::default();
                s.host_remote_entry = String::new();
                s.host_exposed_module = String::new();
            }

            // Rebuild routes from scratch
            routes.clear();
            let new_routes = build_route_table(&s, &remotes);
            for entry in new_routes.iter_all() {
                routes.upsert(entry.0, entry.1);
            }
            info!(host_id = %s.host_id, "gate reassigned, route table rebuilt");
        }

        WsMessage::ConfigChanged { section, value } => {
            if section == "gateway_protection" {
                match serde_json::from_value::<crate::state::ProtectionConfig>(value) {
                    Ok(prot) => {
                        state.write().await.gateway_config.protection = prot;
                        info!("gateway protection config updated via WS");
                    }
                    Err(e) => warn!(error = %e, "failed to deserialize gateway_protection config"),
                }
            }
        }

        WsMessage::ReconnectPolicyChanged { policy } => {
            *base_delay = Duration::from_millis(policy.initial_delay_ms);
            *max_delay = Duration::from_millis(policy.max_delay_ms);
            info!(?base_delay, ?max_delay, "reconnect policy updated");
        }

        WsMessage::Unknown => {}
    }

    Ok(())
}
