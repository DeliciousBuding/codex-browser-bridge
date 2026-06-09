mod error {
    use thiserror::Error;

    #[derive(Debug, Error)]
    pub enum BridgeError {
        #[error("pipe I/O: {0}")]
        PipeIo(#[from] std::io::Error),

        #[error("protocol: {0}")]
        Protocol(String),
    }

    pub type Result<T> = std::result::Result<T, BridgeError>;
}

#[path = "../src/protocol.rs"]
mod protocol;

use protocol::{decode_frame, encode_frame, Request, Response, RpcError, MAX_FRAME_BYTES};
use serde_json::{json, Value};
use tokio::io::{duplex, AsyncReadExt, AsyncWriteExt};

#[tokio::test]
async fn encode_frame_prefixes_json_with_four_byte_little_endian_length() {
    let (mut tx, mut rx) = duplex(1024);
    let req = Request::new(7, "getInfo", json!({"tabId": 3}));

    encode_frame(&mut tx, &req).await.unwrap();
    tx.shutdown().await.unwrap();
    let mut bytes = Vec::new();
    rx.read_to_end(&mut bytes).await.unwrap();

    let payload_len = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
    assert_eq!(payload_len as usize, bytes.len() - 4);

    let payload: Value = serde_json::from_slice(&bytes[4..]).unwrap();
    assert_eq!(
        payload,
        json!({
            "jsonrpc": "2.0",
            "id": 7,
            "method": "getInfo",
            "params": {"tabId": 3}
        })
    );
}

#[tokio::test]
async fn decode_frame_rejects_zero_length_frame() {
    let mut input = &b"\x00\x00\x00\x00"[..];

    let err = decode_frame(&mut input).await.unwrap_err();

    assert_eq!(err.to_string(), "protocol: empty frame");
}

#[tokio::test]
async fn decode_frame_rejects_frames_over_ten_mb() {
    let oversized = (MAX_FRAME_BYTES + 1).to_le_bytes();
    let mut input = &oversized[..];

    let err = decode_frame(&mut input).await.unwrap_err();

    assert_eq!(
        err.to_string(),
        format!("protocol: frame too large: {} bytes", MAX_FRAME_BYTES + 1)
    );
}

#[test]
fn request_json_shape_omits_null_params_like_go_omitempty() {
    let req = Request::new(12, "ping", Value::Null);

    let value = serde_json::to_value(&req).unwrap();

    assert_eq!(
        value,
        json!({
            "jsonrpc": "2.0",
            "id": 12,
            "method": "ping"
        })
    );
}

#[test]
fn response_id_may_be_missing() {
    let response: Response = serde_json::from_str(r#"{"result":{"ok":true}}"#).unwrap();

    assert_eq!(response.id, None);
    assert_eq!(response.result.unwrap().get(), r#"{"ok":true}"#);
    assert!(response.error.is_none());
}

#[test]
fn rpc_error_display_escapes_newlines_and_carriage_returns() {
    let err = RpcError {
        code: -32000,
        message: "line 1\nline 2\rline 3".to_string(),
        data: None,
    };

    assert_eq!(
        err.to_string(),
        r"json-rpc error -32000: line 1\nline 2\rline 3"
    );
}
