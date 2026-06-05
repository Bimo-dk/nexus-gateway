use crate::AppState;
use crate::state::{GatewayConfig, GatewayState, HostFramework, new_shared};
use crate::route_table::RouteTable;
use crate::proxy::build_client;
use crate::http_client;

fn make_app_state() -> AppState {
    let state = GatewayState {
        gate_id: "gate-1".into(),
        gate_name: "gate-dk".into(),
        host_id: "host-1".into(),
        host_name: "shop-host".into(),
        host_url: "http://shop:80".into(),
        host_framework: HostFramework::Angular,
        host_remote_entry: "/host/remoteEntry.json".into(),
        host_exposed_module: "./AppShell".into(),
        gateway_config: GatewayConfig::default(),
        registry_url: "http://registry:3000".into(),
        nexus_token: "tok".into(),
        registry_connected: true,
    };
    AppState {
        gateway: new_shared(state),
        routes: RouteTable::new(),
        proxy_client: build_client(),
        http_client: http_client::build(),
        protection: crate::protection::ProtectionState::new(),
    }
}

#[tokio::test]
async fn health_response_contains_required_fields() {
    let app = make_app_state();
    let resp = crate::health::handler(app).await;
    let body = resp.0;

    assert_eq!(body["status"], "ok");
    assert_eq!(body["gate_name"], "gate-dk");
    assert_eq!(body["gate_id"], "gate-1");
    assert_eq!(body["host_name"], "shop-host");
    assert_eq!(body["host_id"], "host-1");
    assert_eq!(body["host_framework"], "angular");
    assert_eq!(body["host_remote_entry"], "/host/remoteEntry.json");
    assert!(body["version"].is_string());
    assert!(body["registry_connected"].is_boolean());
    assert!(body["route_count"].is_number());
}
