use anyhow::{bail, Context, Result};
use rand::Rng;
use std::time::Duration;
use tracing::{info, warn};

use crate::http_client::{self, HyperClient};
use crate::route_table::{RouteTable, UpstreamTarget};
use crate::state::{
    GatewayConfig, GatewayState, RegistryGate, RegistryGatewayConfig, RegistryRemote,
};

pub struct Env {
    pub nexus_token: String,
    pub registry_url: String,
    pub gate_name: String,
    // Auto-registration fields — only required if gate doesn't exist yet
    pub host_name: Option<String>,
    pub host_url: Option<String>,
    pub host_framework: Option<String>,
    pub host_remote_entry: Option<String>,
    pub host_exposed_module: Option<String>,
    pub gate_label: Option<String>,
}

pub fn read_env() -> Result<Env> {
    let nexus_token =
        std::env::var("NEXUS_TOKEN").context("NEXUS_TOKEN is required but not set")?;
    let registry_url =
        std::env::var("REGISTRY_URL").context("REGISTRY_URL is required but not set")?;
    let gate_name =
        std::env::var("NEXUS_GATE_NAME").context("NEXUS_GATE_NAME is required but not set")?;

    // Empty strings (e.g. from `ENV NEXUS_HOST_NAME=` defaults in the Dockerfile)
    // must collapse to None — otherwise `ensure_gate` walks into the host
    // auto-create branch with an empty name and the registry rejects it.
    fn opt(key: &str) -> Option<String> {
        std::env::var(key).ok().filter(|v| !v.is_empty())
    }

    Ok(Env {
        nexus_token,
        registry_url,
        gate_name,
        host_name: opt("NEXUS_HOST_NAME"),
        host_url: opt("NEXUS_HOST_URL"),
        host_framework: opt("NEXUS_HOST_FRAMEWORK"),
        host_remote_entry: opt("NEXUS_HOST_REMOTE_ENTRY"),
        host_exposed_module: opt("NEXUS_HOST_EXPOSED_MODULE"),
        gate_label: opt("NEXUS_GATE_LABEL"),
    })
}

async fn fetch_with_retry(
    client: &HyperClient,
    url: &str,
    token: &str,
) -> Result<serde_json::Value> {
    let start = std::time::Instant::now();
    let mut delay = Duration::from_secs(1);
    let max_delay = Duration::from_secs(30);
    let budget = Duration::from_secs(60);

    loop {
        match http_client::get_json(client, url, token).await {
            Ok(v) => return Ok(v),
            Err(e) => {
                warn!(url, error = %e, "registry request failed");
            }
        }

        if start.elapsed() >= budget {
            bail!("registry did not respond within 60s (url={})", url);
        }

        let jitter_ms = rand::thread_rng().gen_range(0u64..=500);
        let sleep = std::cmp::min(delay, max_delay) + Duration::from_millis(jitter_ms);
        info!(url, sleep_ms = sleep.as_millis(), "retrying after backoff");
        tokio::time::sleep(sleep).await;
        delay = delay.saturating_mul(2);
    }
}

// Find-or-create gate. When NEXUS_HOST_NAME is set, also find-or-create the host.
async fn ensure_gate(client: &HyperClient, env: &Env) -> Result<()> {
    let base = env.registry_url.trim_end_matches('/');

    let host_id: Option<String> = if let Some(host_name) = env.host_name.as_deref() {
        let host_url = env.host_url.as_deref().ok_or_else(|| {
            anyhow::anyhow!("NEXUS_HOST_URL is required when NEXUS_HOST_NAME is set")
        })?;

        let id = match http_client::get_json(
            client,
            &format!("{}/api/hosts/{}", base, host_name),
            &env.nexus_token,
        )
        .await
        {
            Ok(h) => {
                let id = h["id"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("host response missing id"))?
                    .to_owned();
                info!(host_name, host_id = %id, "found existing host");
                id
            }
            Err(_) => {
                info!(host_name, "host not found — creating");
                let framework = env.host_framework.as_deref().unwrap_or("angular");
                let remote_entry = env
                    .host_remote_entry
                    .as_deref()
                    .unwrap_or("/host/remoteEntry.json");
                let exposed_module = env.host_exposed_module.as_deref().unwrap_or("./AppShell");

                let h = http_client::post_json(
                    client,
                    &format!("{}/api/hosts", base),
                    &env.nexus_token,
                    serde_json::json!({
                        "name": host_name,
                        "url": host_url,
                        "framework": framework,
                        "remoteEntry": remote_entry,
                        "exposedModule": exposed_module,
                    }),
                )
                .await
                .context("failed to create host in registry")?;

                let id = h["id"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("create host response missing id"))?
                    .to_owned();
                info!(host_name, host_id = %id, "host created");
                id
            }
        };
        Some(id)
    } else {
        info!(domain = %env.gate_name, "no NEXUS_HOST_NAME set — registering gate without host");
        None
    };

    let gate_label = env
        .gate_label
        .clone()
        .unwrap_or_else(|| derive_gate_label(env.host_name.as_deref(), &env.gate_name));

    info!(domain = %env.gate_name, gate_label, "creating gate in registry");

    let mut body = serde_json::json!({
        "name": gate_label,
        "domain": env.gate_name,
    });
    if let Some(ref hid) = host_id {
        body["hostId"] = serde_json::Value::String(hid.clone());
    }

    match http_client::post_json(
        client,
        &format!("{}/api/gates", base),
        &env.nexus_token,
        body,
    )
    .await
    {
        Ok(_) => info!(domain = %env.gate_name, "gate created"),
        Err(e) if e.to_string().contains("conflict") => {
            warn!(domain = %env.gate_name, "gate domain already exists — another instance may have registered it");
        }
        Err(e) => return Err(e.context("failed to create gate in registry")),
    }

    Ok(())
}

/// Produce a gate label that satisfies the registry's `is_valid_entity_name`
/// validator (`[a-zA-Z][a-zA-Z0-9]*`). The previous implementation replaced
/// dots and colons with underscores, which the validator also rejects, and
/// the gateway crash-looped on `bootstrap error: failed to create gate in
/// registry`.
pub fn derive_gate_label(host_name: Option<&str>, gate_name: &str) -> String {
    if let Some(n) = host_name {
        return format!("{n}Gate");
    }
    let stripped: String = gate_name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    if stripped.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
        format!("{stripped}Gate")
    } else {
        format!("g{stripped}Gate")
    }
}

pub async fn bootstrap(env: &Env) -> Result<(GatewayState, RouteTable)> {
    let client = http_client::build();
    let base = env.registry_url.trim_end_matches('/');

    // Step 1: gate by domain — auto-register if not found
    let gate_url = format!("{}/api/gates/by-domain/{}", base, env.gate_name);
    info!(gate_url, "fetching gate");

    let gate_json = match http_client::get_json(&client, &gate_url, &env.nexus_token).await {
        Ok(v) => v,
        Err(e) if e.to_string().contains("404") => {
            info!(domain = %env.gate_name, "gate not found — attempting auto-registration");
            ensure_gate(&client, env).await?;
            fetch_with_retry(&client, &gate_url, &env.nexus_token)
                .await
                .with_context(|| {
                    format!("gate '{}' not available after registration", env.gate_name)
                })?
        }
        Err(_) => fetch_with_retry(&client, &gate_url, &env.nexus_token)
            .await
            .with_context(|| format!("gate '{}' not found in registry", env.gate_name))?,
    };

    let gate: RegistryGate =
        serde_json::from_value(gate_json).context("failed to deserialise gate response")?;
    info!(gate_id = %gate.id, host_attached = gate.host.is_some(), "gate resolved");

    // Step 2: remotes for this host (skip if gate has no host yet)
    let remotes: Vec<RegistryRemote> = if let Some(ref host) = gate.host {
        let remotes_url = format!("{}/api/hosts/{}/remotes", base, host.id);
        let mut remotes_json = fetch_with_retry(&client, &remotes_url, &env.nexus_token)
            .await
            .context("failed to fetch host remotes")?;
        serde_json::from_value(remotes_json["remotes"].take())
            .context("failed to deserialise remotes")?
    } else {
        vec![]
    };

    // Step 3: gateway config (non-fatal)
    let config_url = format!("{}/api/config/gateway", base);
    let reg_config: RegistryGatewayConfig =
        fetch_with_retry(&client, &config_url, &env.nexus_token)
            .await
            .and_then(|v| serde_json::from_value(v).map_err(Into::into))
            .unwrap_or_else(|e| {
                warn!(error = %e, "gateway config unavailable, using defaults");
                RegistryGatewayConfig::default()
            });

    let gateway_config = GatewayConfig {
        cors_origins: reg_config.cors_origins,
        custom_headers: reg_config.custom_headers,
        health_check_path: reg_config.health_check_path,
        public_url: reg_config.public_url,
        protection: reg_config.protection.unwrap_or_default(),
    };

    let (host_id, host_name, host_url, host_framework, host_remote_entry, host_exposed_module) =
        if let Some(host) = gate.host {
            (
                host.id,
                host.name,
                host.url,
                host.framework,
                host.remote_entry,
                host.exposed_module,
            )
        } else {
            (
                String::new(),
                String::new(),
                String::new(),
                Default::default(),
                String::new(),
                String::new(),
            )
        };

    let state = GatewayState {
        gate_id: gate.id.clone(),
        gate_name: env.gate_name.clone(),
        host_id,
        host_name,
        host_url,
        host_framework,
        host_remote_entry,
        host_exposed_module,
        gateway_config,
        registry_url: env.registry_url.clone(),
        nexus_token: env.nexus_token.clone(),
        registry_connected: false,
    };

    let route_table = build_route_table(&state, &remotes);
    Ok((state, route_table))
}

pub fn build_route_table(state: &GatewayState, remotes: &[RegistryRemote]) -> RouteTable {
    let table = RouteTable::new();
    // `/api/` proxies HTTP API calls to the registry — longest-prefix
    // matching guarantees this wins over the `/` host catch-all below
    // for every `/api/...` path. `/api/ws` is handled separately as an
    // axum route because it needs a WebSocket upgrade.
    //
    // Upstream URL includes `/api` because proxy::handler strips the
    // matched prefix from the request path; without it, `/api/remotes`
    // would become `/remotes` against the registry, and the registry
    // serves its API under `/api/*`.
    let registry_base = state.registry_url.trim_end_matches('/');
    table.upsert(
        "/api/",
        UpstreamTarget {
            upstream_url: format!("{}/api", registry_base),
            enabled: true,
        },
    );
    // Catch-all for the host shell. Longest-prefix matching lets /api/*
    // and /remotes/<name>/* still win where they apply. Everything else
    // — `/`, `/remoteEntry.json`, `/main-*.js`, `/assets/*` — proxies
    // to the host so the host's native-federation bootstrap chain runs
    // in the browser. The static SPA shim was unreachable at `/` and
    // could not import bare specifiers without an import map (see B-14).
    table.upsert(
        "/",
        UpstreamTarget {
            upstream_url: state.host_url.clone(),
            enabled: true,
        },
    );
    for remote in remotes {
        if !is_visible(remote, &state.host_id) {
            continue;
        }
        // Upstream MUST be the internal upstreamUrl (http://remote-x:80),
        // not the browser-facing url (which is a path like
        // /remotes/x/remoteEntry.json). Older registry payloads may omit
        // upstreamUrl — in that case skip the remote so we don't proxy
        // to a relative path. Observed as B-18.
        if remote.upstream_url.is_empty() {
            continue;
        }
        let prefix = format!("/remotes/{}/", remote.route_path.trim_matches('/'));
        table.upsert(
            prefix,
            UpstreamTarget {
                upstream_url: remote.upstream_url.clone(),
                enabled: remote.enabled,
            },
        );
    }
    table
}

pub fn is_visible(remote: &RegistryRemote, host_id: &str) -> bool {
    remote.visibility == "global" || remote.visibility == format!("host:{}", host_id)
}
