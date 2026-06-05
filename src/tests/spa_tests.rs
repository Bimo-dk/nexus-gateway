use crate::state::{GatewayConfig, GatewayState, HostFramework, new_shared};

fn make_state(framework: HostFramework) -> crate::state::SharedState {
    let s = GatewayState {
        gate_id: "g1".into(),
        gate_name: "gate-dk".into(),
        host_id: "h1".into(),
        host_name: "shop".into(),
        host_url: "http://shop:80".into(),
        host_framework: framework,
        host_remote_entry: "/host/remoteEntry.json".into(),
        host_exposed_module: "./AppShell".into(),
        gateway_config: GatewayConfig::default(),
        registry_url: "http://registry:3000".into(),
        nexus_token: "tok".into(),
        registry_connected: false,
    };
    new_shared(s)
}

#[tokio::test]
async fn spa_injects_config_into_html() {
    let state = make_state(HostFramework::Angular);
    let html = crate::spa::handler(state).await;
    let body = html.0;

    assert!(body.contains("window.__NEXUS_GATEWAY_CONFIG__"), "config injection missing");
    assert!(body.contains("\"gateName\":\"gate-dk\""), "gateName missing");
    assert!(body.contains("\"gateId\":\"g1\""), "gateId missing");
    assert!(body.contains("\"hostName\":\"shop\""), "hostName missing");
    assert!(body.contains("\"hostId\":\"h1\""), "hostId missing");
    assert!(body.contains("\"hostFramework\":\"angular\""), "hostFramework missing");
    assert!(body.contains("\"registryUrl\":\"/api\""), "registryUrl must be /api");
    assert!(body.contains("\"wsUrl\":\"/ws\""), "wsUrl must be /ws");
}

#[tokio::test]
async fn spa_html_contains_all_mount_points() {
    let state = make_state(HostFramework::Vue);
    let html = crate::spa::handler(state).await;
    let body = html.0;

    assert!(body.contains("id=\"nexus-root\""), "#nexus-root for Angular");
    assert!(body.contains("id=\"app\""), "#app for Vue");
    assert!(body.contains("id=\"root\""), "#root for React");
}
