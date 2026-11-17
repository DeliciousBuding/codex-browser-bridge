use codex_browser_bridge::{
    client::Client,
    registry::{
        select_browser_from_inventory, BrowserDescriptor, BrowserInventory, BrowserSelector,
        BrowserTransport,
    },
};
use serde_json::Value;

fn inventory(unprobed: usize, failed: usize) -> BrowserInventory {
    BrowserInventory {
        pipe_count: 1 + unprobed + failed,
        probed_pipe_count: 1 + failed,
        unprobed_pipe_count: unprobed,
        failed_pipe_count: failed,
        stale_pipe_count: 0,
        excluded_pipe_count: 0,
        browsers: vec![BrowserDescriptor {
            id: "exact-id".into(),
            family: "edge".into(),
            name: "Edge".into(),
            version: None,
            extension_id: None,
            profile_name: None,
            transport: BrowserTransport {
                kind: "mock",
                name: "mock".into(),
                uuid: "mock".into(),
                path: "mock".into(),
            },
            capabilities: Value::Null,
        }],
    }
}

#[test]
fn partial_inventory_requires_exact_browser_identity() {
    for partial in [inventory(1, 0), inventory(0, 1), inventory(2, 3)] {
        for selector in [
            BrowserSelector::default(),
            BrowserSelector::new(Some("edge".into()), None),
            BrowserSelector::new(Some("chrome".into()), None),
        ] {
            assert!(select_browser_from_inventory(&partial, &selector)
                .unwrap_err()
                .to_string()
                .contains("incomplete"));
        }
        let exact = BrowserSelector::new(None, Some("exact-id".into()));
        assert_eq!(
            select_browser_from_inventory(&partial, &exact).unwrap().id,
            "exact-id"
        );
    }
}

#[test]
fn complete_inventory_ignores_known_stale_and_app_private_routes_only() {
    let mut complete = inventory(0, 0);
    complete.pipe_count += 2;
    complete.probed_pipe_count += 2;
    complete.stale_pipe_count = 1;
    complete.excluded_pipe_count = 1;
    let default = BrowserSelector::default();
    assert_eq!(
        select_browser_from_inventory(&complete, &default)
            .unwrap()
            .id,
        "exact-id"
    );
    let family = BrowserSelector::new(Some("edge".into()), None);
    assert_eq!(
        select_browser_from_inventory(&complete, &family)
            .unwrap()
            .id,
        "exact-id"
    );

    let mut ambiguous = complete;
    let mut second = ambiguous.browsers[0].clone();
    second.id = "second-id".into();
    ambiguous.browsers.push(second);
    ambiguous.pipe_count += 1;
    ambiguous.probed_pipe_count += 1;
    assert!(select_browser_from_inventory(&ambiguous, &default)
        .unwrap_err()
        .to_string()
        .contains("2 connected browser instances"));
}

#[test]
fn exact_identity_never_falls_back_and_duplicate_identity_fails_closed() {
    let mut inventory = inventory(0, 1);
    assert!(select_browser_from_inventory(
        &inventory,
        &BrowserSelector::new(None, Some("missing".into()))
    )
    .is_err());
    inventory.browsers.push(inventory.browsers[0].clone());
    assert!(select_browser_from_inventory(
        &inventory,
        &BrowserSelector::new(None, Some("exact-id".into()))
    )
    .unwrap_err()
    .to_string()
    .contains("multiple live pipes"));
}

#[test]
fn fresh_lazy_client_has_no_prior_turn_but_resumed_client_has_unknown_ownership() {
    assert!(!Client::lazy(Some("unused-mock".into())).has_unresolved_turn());
    assert!(Client::lazy_with_selector_and_ids(
        Some("unused-mock".into()),
        BrowserSelector::default(),
        Some(("saved-session".into(), "saved-turn".into()))
    )
    .has_unresolved_turn());
}
