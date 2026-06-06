use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

use crate::startup::{bootstrap, derive_gate_label, is_visible, Env};
use crate::state::RegistryRemote;

fn make_gate_response(gate_id: &str, host_id: &str) -> serde_json::Value {
    json!({
        "id": gate_id,
        "name": "gatedK",
        "host": {
            "id": host_id,
            "name": "shopHost",
            "url": "http://shop:80",
            "framework": "angular",
            "remoteEntry": "/host/remoteEntry.json",
            "exposedModule": "./AppShell"
        }
    })
}

fn make_remotes(host_id: &str) -> serde_json::Value {
    json!({
        "hostId": host_id,
        "remotes": [
            {
                "name": "checkout",
                "url": "http://checkout:80",
                "routePath": "checkout",
                "visibility": "global",
                "enabled": true
            },
            {
                "name": "privateRemote",
                "url": "http://private:80",
                "routePath": "private",
                "visibility": format!("host:{}", host_id),
                "enabled": true
            },
            {
                "name": "otherHostRemote",
                "url": "http://other:80",
                "routePath": "other",
                "visibility": "host:other-host-id",
                "enabled": true
            }
        ],
        "total": 3
    })
}

#[tokio::test]
async fn startup_fetches_gate_and_builds_route_table() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/api/gates/by-domain/gate-dk"))
        .and(header("X-Nexus-Token", "test-token"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(make_gate_response("gate-1", "host-1")),
        )
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/hosts/host-1/remotes"))
        .respond_with(ResponseTemplate::new(200).set_body_json(make_remotes("host-1")))
        .mount(&server)
        .await;

    Mock::given(method("GET"))
        .and(path("/api/config/gateway"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"protection": {}})))
        .mount(&server)
        .await;

    let env = Env {
        nexus_token: "test-token".into(),
        registry_url: server.uri(),
        gate_name: "gate-dk".into(),
        host_name: None,
        host_url: None,
        host_framework: None,
        host_remote_entry: None,
        host_exposed_module: None,
        gate_label: None,
    };

    let (state, routes) = bootstrap(&env).await.expect("bootstrap should succeed");

    assert_eq!(state.gate_id, "gate-1");
    assert_eq!(state.host_id, "host-1");
    assert_eq!(state.host_url, "http://shop:80");

    // global and host-matching routes should be present
    assert!(
        routes.resolve("/remotes/checkout/foo").is_some(),
        "checkout route missing"
    );
    assert!(
        routes.resolve("/remotes/private/foo").is_some(),
        "private route missing"
    );

    // other-host-id remote must not appear
    assert!(
        routes.resolve("/remotes/other/foo").is_none(),
        "other-host remote should be excluded"
    );

    // host route always present
    assert!(
        routes.resolve("/host/remoteEntry.json").is_some(),
        "host route missing"
    );
}

#[test]
fn visibility_filtering() {
    let global = RegistryRemote {
        name: "a".into(),
        url: "http://a".into(),
        route_path: "a".into(),
        visibility: "global".into(),
        enabled: true,
    };
    let host_match = RegistryRemote {
        visibility: "host:my-host".into(),
        ..global.clone()
    };
    let host_other = RegistryRemote {
        visibility: "host:other-host".into(),
        ..global.clone()
    };

    assert!(is_visible(&global, "my-host"));
    assert!(is_visible(&host_match, "my-host"));
    assert!(!is_visible(&host_other, "my-host"));
}

#[test]
fn derive_gate_label_uses_host_name_when_available() {
    assert_eq!(derive_gate_label(Some("shopHost"), "shop.example.com"), "shopHostGate");
}

#[test]
fn derive_gate_label_strips_dots_and_dashes_when_no_host() {
    assert_eq!(derive_gate_label(None, "angular.local"), "angularlocalGate");
    assert_eq!(derive_gate_label(None, "vue.local"), "vuelocalGate");
    assert_eq!(derive_gate_label(None, "react.local"), "reactlocalGate");
    assert_eq!(derive_gate_label(None, "shop-eu.example.com"), "shopeuexamplecomGate");
    assert_eq!(derive_gate_label(None, "host:8080"), "host8080Gate");
}

#[test]
fn derive_gate_label_handles_label_starting_with_digit() {
    // is_valid_entity_name forbids a leading digit; prefix with 'g'.
    assert_eq!(derive_gate_label(None, "8080.local"), "g8080localGate");
}

#[test]
fn derive_gate_label_never_contains_invalid_chars() {
    let cases = [
        "x_y", "x-y", "x.y", "x:y", "x y", "x/y", "x+y", "1x", "1.2.3.4",
    ];
    for c in cases {
        let label = derive_gate_label(None, c);
        assert!(
            label.chars().all(|ch| ch.is_ascii_alphanumeric()),
            "label \"{label}\" derived from \"{c}\" contains non-alphanumeric chars"
        );
        let first = label.chars().next().expect("non-empty");
        assert!(first.is_ascii_alphabetic(), "label \"{label}\" must start with a letter");
    }
}
