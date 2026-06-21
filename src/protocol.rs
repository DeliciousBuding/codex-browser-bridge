use serde::{Deserialize, Serialize};
use std::fmt;

use serde_json::{json, value::RawValue, Value};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::error::{BridgeError, Result};

pub const MAX_FRAME_BYTES: u32 = 10 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
pub struct Request {
    pub jsonrpc: &'static str,
    pub id: u64,
    pub method: String,
    #[serde(skip_serializing_if = "Value::is_null")]
    pub params: Value,
}

impl Request {
    pub fn new(id: u64, method: impl Into<String>, params: Value) -> Self {
        Self {
            jsonrpc: "2.0",
            id,
            method: method.into(),
            params,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Response {
    pub id: Option<u64>,
    #[serde(default)]
    pub result: Option<Box<RawValue>>,
    #[serde(default)]
    pub error: Option<RpcError>,
    /// CDP event method. Present only on server-pushed event frames (no id).
    #[serde(default)]
    pub method: Option<String>,
    /// CDP event params. Present only on event frames.
    #[serde(default)]
    pub params: Option<Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    #[serde(default)]
    #[allow(dead_code)]
    pub data: Option<Value>,
}

impl fmt::Display for RpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = self.message.replace('\n', "\\n").replace('\r', "\\r");
        write!(f, "json-rpc error {}: {}", self.code, message)
    }
}

impl std::error::Error for RpcError {}

pub async fn encode_frame<W, T>(writer: &mut W, msg: &T) -> Result<()>
where
    W: AsyncWrite + Unpin,
    T: Serialize + ?Sized,
{
    let payload = serde_json::to_vec(msg).map_err(|err| BridgeError::Protocol(err.to_string()))?;
    if payload.len() > MAX_FRAME_BYTES as usize {
        return Err(BridgeError::Protocol(format!(
            "frame too large: {} bytes",
            payload.len()
        )));
    }
    let len_bytes = (payload.len() as u32).to_le_bytes();
    // Combine length header + payload into single write to reduce syscalls
    let mut frame = Vec::with_capacity(4 + payload.len());
    frame.extend_from_slice(&len_bytes);
    frame.extend_from_slice(&payload);
    writer.write_all(&frame).await?;
    writer.flush().await?;
    Ok(())
}

pub async fn decode_frame<R>(reader: &mut R) -> Result<Vec<u8>>
where
    R: AsyncRead + Unpin,
{
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).await?;
    let len = u32::from_le_bytes(len_buf);
    if len == 0 {
        return Err(BridgeError::Protocol("empty frame".into()));
    }
    if len > MAX_FRAME_BYTES {
        return Err(BridgeError::Protocol(format!(
            "frame too large: {len} bytes"
        )));
    }
    // Allocate buffer. For small frames the memset cost is negligible;
    // for large frames the pipe I/O dominates. Safe and simple.
    let mut payload = vec![0u8; len as usize];
    reader.read_exact(&mut payload).await?;
    Ok(payload)
}

pub fn with_session_params(session_id: &str, turn_id: &str, params: Option<Value>) -> Value {
    let mut merged = match params {
        Some(Value::Object(map)) => map,
        _ => serde_json::Map::new(),
    };
    merged.insert("session_id".into(), json!(session_id));
    merged.insert("turn_id".into(), json!(turn_id));
    Value::Object(merged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::duplex;

    #[tokio::test]
    async fn frame_roundtrip_uses_little_endian_length() {
        let (mut tx, mut rx) = duplex(1024);
        let req = Request::new(7, "getInfo", json!({"x": 1}));

        encode_frame(&mut tx, &req).await.unwrap();
        let frame = decode_frame(&mut rx).await.unwrap();
        let value: Value = serde_json::from_slice(&frame).unwrap();

        assert_eq!(value["id"], 7);
        assert_eq!(value["method"], "getInfo");
        assert_eq!(value["params"]["x"], 1);
    }

    #[test]
    fn session_params_are_merged() {
        let got = with_session_params("s", "t", Some(json!({"tabId": 1})));
        assert_eq!(got["session_id"], "s");
        assert_eq!(got["turn_id"], "t");
        assert_eq!(got["tabId"], 1);
    }
}
