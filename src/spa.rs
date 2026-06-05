use axum::response::Html;
use crate::state::SharedState;

static INDEX_HTML: &str = include_str!("../static/index.html");
static NOT_READY_HTML: &str = include_str!("../static/not-ready.html");

pub async fn handler(gateway: SharedState) -> Html<String> {
    let s = gateway.read().await;
    let config = serde_json::json!({
        "gateName": s.gate_name,
        "gateId": s.gate_id,
        "hostName": s.host_name,
        "hostId": s.host_id,
        "hostFramework": s.host_framework.to_string(),
        "hostRemoteEntry": s.host_remote_entry,
        "hostExposedModule": s.host_exposed_module,
        "registryUrl": "/api",
        "wsUrl": "/ws",
    });
    let host_ready = !s.host_remote_entry.is_empty();
    drop(s);

    let template = if host_ready { INDEX_HTML } else { NOT_READY_HTML };
    let config_str = config.to_string().replace("</", "<\\/");
    let script = format!(
        "<script>window.__NEXUS_GATEWAY_CONFIG__ = {};</script>",
        config_str
    );
    let html = template.replacen("</head>", &format!("{}\n</head>", script), 1);
    Html(html)
}
