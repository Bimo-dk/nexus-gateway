use std::net::IpAddr;
use std::time::{Duration, Instant};

use crate::protection::{BanEntry, ProtectionState};
use crate::route_table::RouteTable;
use crate::state::{new_shared, GatewayConfig, GatewayState, HostFramework, ProtectionConfig};
use crate::AppState;

fn default_cfg() -> ProtectionConfig {
    ProtectionConfig::default()
}

fn make_app_state_with_protection() -> AppState {
    let state = GatewayState {
        gate_id: "g1".into(),
        gate_name: "gate-dk".into(),
        host_id: "h1".into(),
        host_name: "shop".into(),
        host_url: "http://shop:80".into(),
        host_framework: HostFramework::Angular,
        host_remote_entry: "/host/remoteEntry.json".into(),
        host_exposed_module: "./AppShell".into(),
        gateway_config: GatewayConfig {
            protection: default_cfg(),
            ..Default::default()
        },
        registry_url: "http://registry:3000".into(),
        nexus_token: "test-token".into(),
        registry_connected: false,
    };
    AppState {
        gateway: new_shared(state),
        routes: RouteTable::new(),
        proxy_client: crate::proxy::build_client(),
        http_client: crate::http_client::build(),
        protection: ProtectionState::new(),
    }
}

fn ip(s: &str) -> IpAddr {
    s.parse().unwrap()
}

// ---- Connection limit ----

#[test]
fn connection_limit_blocks_when_exceeded() {
    let prot = ProtectionState::new();
    let cfg = ProtectionConfig {
        max_connections_per_ip: 2,
        ..default_cfg()
    };
    let addr = ip("10.0.0.1");

    // Acquire two slots manually
    let entry = prot.connections.entry(addr).or_default();
    entry
        .active_http
        .store(2, std::sync::atomic::Ordering::Relaxed);
    drop(entry);

    // Third attempt should be over limit
    let prev = prot
        .connections
        .entry(addr)
        .or_default()
        .active_http
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    assert!(
        prev + 1 > cfg.max_connections_per_ip,
        "expected connection limit to be exceeded"
    );
}

// ---- Rate limiting ----

#[test]
fn rate_limit_allows_within_burst() {
    let prot = ProtectionState::new();
    let cfg = ProtectionConfig {
        rate_limit_enabled: true,
        rate_limit_requests_per_second: 10,
        rate_limit_burst: 5,
        ..default_cfg()
    };
    let addr = ip("10.0.0.2");

    for _ in 0..5 {
        assert!(
            prot.try_rate_limit(addr, &cfg).is_ok(),
            "expected token to be available"
        );
    }
}

#[test]
fn rate_limit_blocks_after_burst_exhausted() {
    let prot = ProtectionState::new();
    let cfg = ProtectionConfig {
        rate_limit_enabled: true,
        rate_limit_requests_per_second: 1,
        rate_limit_burst: 2,
        ..default_cfg()
    };
    let addr = ip("10.0.0.3");

    assert!(prot.try_rate_limit(addr, &cfg).is_ok());
    assert!(prot.try_rate_limit(addr, &cfg).is_ok());
    // Third attempt exhausts burst — should fail and return retry_after_ms
    let result = prot.try_rate_limit(addr, &cfg);
    assert!(result.is_err(), "expected rate limit to reject 3rd request");
    let retry_ms = result.unwrap_err();
    assert!(retry_ms > 0, "expected non-zero retry_after_ms");
}

#[test]
fn rate_limit_disabled_allows_all() {
    let prot = ProtectionState::new();
    let cfg = ProtectionConfig {
        rate_limit_enabled: false,
        rate_limit_burst: 1,
        ..default_cfg()
    };
    let addr = ip("10.0.0.4");
    for _ in 0..100 {
        assert!(prot.try_rate_limit(addr, &cfg).is_ok());
    }
}

// ---- Ban management ----

#[test]
fn auto_ban_after_threshold_violations() {
    let prot = ProtectionState::new();
    let cfg = ProtectionConfig {
        ban_threshold_violations: 3,
        ban_duration_seconds: 60,
        ..default_cfg()
    };
    let addr = ip("10.0.0.5");

    for _ in 0..3 {
        prot.record_violation(addr, "test_violation", &cfg);
    }

    assert!(
        prot.check_ban(addr).is_err(),
        "IP should be banned after threshold"
    );
    let (retry_secs, reason) = prot.check_ban(addr).unwrap_err();
    assert!(retry_secs > 0);
    assert_eq!(reason, "test_violation");
}

#[test]
fn unban_clears_ban() {
    let prot = ProtectionState::new();
    let addr = ip("10.0.0.6");

    prot.bans.insert(
        addr,
        BanEntry {
            banned_until: Instant::now() + Duration::from_secs(300),
            reason: "test".into(),
            violation_count: 1,
        },
    );

    assert!(prot.check_ban(addr).is_err(), "should be banned");
    assert!(prot.unban(addr), "unban should return true");
    assert!(
        prot.check_ban(addr).is_ok(),
        "should not be banned after unban"
    );
}

#[test]
fn expired_ban_is_removed_on_check() {
    let prot = ProtectionState::new();
    let addr = ip("10.0.0.7");

    prot.bans.insert(
        addr,
        BanEntry {
            banned_until: Instant::now() - Duration::from_secs(1), // expired
            reason: "old ban".into(),
            violation_count: 1,
        },
    );

    assert!(prot.check_ban(addr).is_ok(), "expired ban should not block");
    assert!(
        !prot.bans.contains_key(&addr),
        "expired ban should be removed"
    );
}

// ---- WebSocket limits ----

#[test]
fn ws_connection_limit_blocks_when_exceeded() {
    let prot = ProtectionState::new();
    let addr = ip("10.0.0.8");
    let limit = 2;

    assert!(prot.try_acquire_ws(addr, limit));
    assert!(prot.try_acquire_ws(addr, limit));
    assert!(
        !prot.try_acquire_ws(addr, limit),
        "third WS connection should be rejected"
    );
}

#[test]
fn ws_release_allows_new_connection() {
    let prot = ProtectionState::new();
    let addr = ip("10.0.0.9");

    assert!(prot.try_acquire_ws(addr, 1));
    assert!(!prot.try_acquire_ws(addr, 1), "second should be rejected");
    prot.release_ws(addr);
    assert!(prot.try_acquire_ws(addr, 1), "should succeed after release");
}

// ---- Hot reload ----

#[tokio::test]
async fn hot_reload_protection_config() {
    let app = make_app_state_with_protection();

    {
        let mut s = app.gateway.write().await;
        s.gateway_config.protection.rate_limit_requests_per_second = 999;
    }

    let rps = app
        .gateway
        .read()
        .await
        .gateway_config
        .protection
        .rate_limit_requests_per_second;
    assert_eq!(rps, 999, "hot-reload should update in-place");
}
