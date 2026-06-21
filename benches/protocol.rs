//! Frame encode/decode + session-param benchmarks.
//!
//! Run: `cargo bench`. Criterion writes results under `target/criterion/`.
//! These cover the hot path every request/response traverses, so a regression
//! in the wire layer shows up here.

use codex_browser_bridge::protocol::{decode_frame, encode_frame, with_session_params, Request};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use serde_json::json;
use tokio::io::duplex;

/// A representative request payload — `executeCdp` is the largest common frame
/// in normal agent traffic, so it is the meaningful thing to keep fast.
fn sample_request() -> Request {
    Request::new(
        42,
        "executeCdp",
        json!({
            "session_id": "00000000-0000-4000-8000-000000000000",
            "turn_id": "00000000-0000-4000-8000-000000000001",
            "target": { "tabId": 1 },
            "method": "Page.navigate",
            "commandParams": { "url": "https://example.com/long/path?query=value&other=2" }
        }),
    )
}

/// Encode a request frame and immediately decode it back — the full round-trip
/// the client and read loop perform per message.
fn bench_frame_roundtrip(c: &mut Criterion) {
    let req = sample_request();
    c.bench_function("frame_encode_decode_roundtrip", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter(|| async {
                let (mut tx, mut rx) = duplex(8192);
                encode_frame(&mut tx, black_box(&req)).await.unwrap();
                let _ = decode_frame(&mut rx).await.unwrap();
            });
    });
}

/// Merging session_id / turn_id into outbound params — pure allocation work,
/// called on every request.
fn bench_session_params(c: &mut Criterion) {
    let params = Some(json!({
        "target": { "tabId": 1 },
        "method": "Page.navigate",
        "commandParams": { "url": "https://example.com" }
    }));
    c.bench_function("with_session_params", |b| {
        b.iter(|| {
            with_session_params(
                black_box("00000000-0000-4000-8000-000000000000"),
                black_box("00000000-0000-4000-8000-000000000001"),
                black_box(params.clone()),
            )
        });
    });
}

criterion_group!(benches, bench_frame_roundtrip, bench_session_params);
criterion_main!(benches);
