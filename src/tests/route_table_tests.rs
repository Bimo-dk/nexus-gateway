use crate::route_table::{RouteTable, UpstreamTarget};

fn target(url: &str) -> UpstreamTarget {
    UpstreamTarget { upstream_url: url.into(), enabled: true }
}

#[test]
fn longest_prefix_wins() {
    let table = RouteTable::new();
    table.upsert("/remotes/", target("http://short"));
    table.upsert("/remotes/checkout/", target("http://checkout"));

    let (prefix, t) = table.resolve("/remotes/checkout/bundle.js").unwrap();
    assert_eq!(prefix, "/remotes/checkout/");
    assert_eq!(t.upstream_url, "http://checkout");
}

#[test]
fn disabled_target_not_returned() {
    let table = RouteTable::new();
    table.upsert("/host/", UpstreamTarget { upstream_url: "http://host".into(), enabled: false });
    assert!(table.resolve("/host/remoteEntry.json").is_none());
}

#[test]
fn no_match_returns_none() {
    let table = RouteTable::new();
    table.upsert("/host/", target("http://host"));
    assert!(table.resolve("/api/gates").is_none());
}

#[test]
fn clear_remotes_leaves_host() {
    let table = RouteTable::new();
    table.upsert("/host/", target("http://host"));
    table.upsert("/remotes/checkout/", target("http://checkout"));
    table.upsert("/remotes/cart/", target("http://cart"));

    table.clear_remotes();

    assert!(table.resolve("/host/anything").is_some());
    assert!(table.resolve("/remotes/checkout/").is_none());
    assert!(table.resolve("/remotes/cart/").is_none());
}

#[test]
fn host_changed_updates_host_route() {
    let table = RouteTable::new();
    table.upsert("/host/", target("http://old-host:80"));

    // Simulate host_changed handler updating the route
    table.upsert("/host/", UpstreamTarget {
        upstream_url: "http://new-host:80".into(),
        enabled: true,
    });

    let (_, t) = table.resolve("/host/remoteEntry.json").unwrap();
    assert_eq!(t.upstream_url, "http://new-host:80");
}
