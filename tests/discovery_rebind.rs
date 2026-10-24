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
fn incomplete_inventory_cannot_prove_unique_family_or_default() {
    for incomplete in [inventory(1, 0), inventory(0, 1), inventory(2, 3)] {
        for selector in [
            BrowserSelector::default(),
            BrowserSelector::new(Some("edge".into()), None),
        ] {
            assert!(select_browser_from_inventory(&incomplete, &selector)
                .unwrap_err()
                .to_string()
                .contains("incomplete"));
        }
        assert_eq!(
            select_browser_from_inventory(
                &incomplete,
                &BrowserSelector::new(None, Some("exact-id".into()))
            )
            .unwrap()
            .id,
            "exact-id"
        );
    }
    assert!(select_browser_from_inventory(&inventory(0, 0), &BrowserSelector::default()).is_ok());
    let mut stale = inventory(0, 0);
    stale.pipe_count += 2;
    stale.probed_pipe_count += 2;
    stale.stale_pipe_count = 2;
    assert!(select_browser_from_inventory(&stale, &BrowserSelector::default()).is_ok());
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
