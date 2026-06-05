use axum::response::Json;
use serde_json::{json, Value};

use crate::AppState;

pub async fn handler(app: AppState) -> Json<Value> {
    let s = app.gateway.read().await;
    Json(json!({
        "status": "ok",
        "service": "gateway",
        "version": env!("CARGO_PKG_VERSION"),
        "registry_connected": s.registry_connected,
        "route_count": app.routes.len(),
        "gate_name": s.gate_name,
        "gate_id": s.gate_id,
        "host_name": s.host_name,
        "host_id": s.host_id,
        "host_framework": s.host_framework.to_string(),
        "host_remote_entry": s.host_remote_entry,
    }))
}
