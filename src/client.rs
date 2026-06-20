use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde_json::{json, value::RawValue, Value};
use tokio::io::{split, ReadHalf, WriteHalf};
use tokio::sync::{oneshot, Mutex};
use tokio::time::{timeout, Duration, Instant};
use uuid::Uuid;

use crate::discovery;
use crate::error::{BridgeError, Result};
use crate::pipe::{dial_named_pipe, PipeStream};
use crate::protocol::{self, Request, Response};

const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

type PendingMap = HashMap<u64, oneshot::Sender<Response>>;

#[derive(Clone)]
pub struct Client {
    inner: Arc<ClientInner>,
}

struct ClientInner {
    writer: Mutex<WriteHalf<PipeStream>>,
    pending: Mutex<PendingMap>,
    next_id: AtomicU64,
    session_id: String,
    turn_id: String,
    tab_locks: Mutex<HashMap<i64, Arc<Mutex<()>>>>,
}

impl Client {
    pub async fn connect(pipe_name: Option<&str>) -> Result<Self> {
        match pipe_name {
            Some(name) => Self::from_stream(dial_named_pipe(&discovery::pipe_path(name)).await?),
            None => connect_discovered_client().await,
        }
    }

    pub fn from_stream(stream: PipeStream) -> Result<Self> {
        let (reader, writer) = split(stream);
        let client = Self {
            inner: Arc::new(ClientInner {
                writer: Mutex::new(writer),
                pending: Mutex::new(HashMap::new()),
                next_id: AtomicU64::new(1),
                session_id: Uuid::new_v4().to_string(),
                turn_id: Uuid::new_v4().to_string(),
                tab_locks: Mutex::new(HashMap::new()),
            }),
        };
        client.spawn_read_loop(reader);
        Ok(client)
    }

    pub async fn send_request(&self, method: &str, params: Option<Value>) -> Result<Box<RawValue>> {
        self.send_request_with_timeout(method, params, DEFAULT_REQUEST_TIMEOUT)
            .await
    }

    pub async fn send_request_with_timeout(
        &self,
        method: &str,
        params: Option<Value>,
        request_timeout: Duration,
    ) -> Result<Box<RawValue>> {
        let id = self.inner.next_id.fetch_add(1, Ordering::SeqCst);
        let params =
            protocol::with_session_params(&self.inner.session_id, &self.inner.turn_id, params);
        let req = Request::new(id, method, params);
        let (tx, rx) = oneshot::channel();

        self.inner.pending.lock().await.insert(id, tx);
        let write_result = {
            let mut writer = self.inner.writer.lock().await;
            protocol::encode_frame(&mut *writer, &req).await
        };
        if let Err(err) = write_result {
            self.inner.pending.lock().await.remove(&id);
            return Err(err);
        }

        let resp = match timeout(request_timeout, rx).await {
            Ok(Ok(resp)) => resp,
            Ok(Err(_)) => {
                self.inner.pending.lock().await.remove(&id);
                return Err(BridgeError::Protocol(format!(
                    "response channel closed for {method}"
                )));
            }
            Err(_) => {
                self.inner.pending.lock().await.remove(&id);
                return Err(BridgeError::Timeout(method.to_string()));
            }
        };

        if let Some(err) = resp.error {
            return Err(BridgeError::Rpc {
                method: method.to_string(),
                message: format!(
                    "json-rpc error {}: {}",
                    err.code,
                    err.message.replace('\n', "\\n").replace('\r', "\\r")
                ),
            });
        }

        match resp.result {
            Some(result) => Ok(result),
            None => serde_json::value::RawValue::from_string("null".to_string()).map_err(|err| {
                BridgeError::Protocol(format!("missing result for {method}: {err}"))
            }),
        }
    }

    fn spawn_read_loop(&self, mut reader: ReadHalf<PipeStream>) {
        let inner = Arc::clone(&self.inner);
        tokio::spawn(async move {
            loop {
                let frame = match protocol::decode_frame(&mut reader).await {
                    Ok(frame) => frame,
                    Err(err) => {
                        tracing::debug!("read loop ended: {err}");
                        break;
                    }
                };
                let response: Response = match serde_json::from_slice(&frame) {
                    Ok(response) => response,
                    Err(_) => continue,
                };
                let Some(id) = response.id else {
                    continue;
                };
                if let Some(tx) = inner.pending.lock().await.remove(&id) {
                    let _ = tx.send(response);
                }
            }
            inner.pending.lock().await.clear();
        });
    }

    pub async fn execute_cdp(
        &self,
        tab_id: i64,
        method: &str,
        params: Option<Value>,
    ) -> Result<Box<RawValue>> {
        let tab_lock = self.tab_lock(tab_id).await;
        let _guard = tab_lock.lock().await;

        let deadline = Instant::now() + DEFAULT_REQUEST_TIMEOUT;
        self.detach_tab_until(tab_id, deadline).await.ok();
        self.attach_tab_until(tab_id, deadline)
            .await
            .map_err(|err| {
                BridgeError::Protocol(format!("attach failed for tab {tab_id}: {err}"))
            })?;

        match self
            .execute_cdp_raw_until(tab_id, method, params.clone(), deadline)
            .await
        {
            Ok(raw) => Ok(raw),
            Err(err) if is_debugger_error(&err) => {
                self.detach_tab_until(tab_id, deadline).await.ok();
                self.attach_tab_until(tab_id, deadline)
                    .await
                    .map_err(|err| {
                        BridgeError::Protocol(format!(
                            "retry attach failed for tab {tab_id}: {err}"
                        ))
                    })?;
                self.execute_cdp_raw_until(tab_id, method, params, deadline)
                    .await
            }
            Err(err) => Err(err),
        }
    }

    async fn tab_lock(&self, tab_id: i64) -> Arc<Mutex<()>> {
        let mut locks = self.inner.tab_locks.lock().await;
        locks
            .entry(tab_id)
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    async fn attach_tab_until(&self, tab_id: i64, deadline: Instant) -> Result<()> {
        self.send_request_with_timeout(
            "attach",
            Some(json!({ "tabId": tab_id })),
            remaining(deadline),
        )
        .await
        .map(|_| ())
    }

    async fn detach_tab_until(&self, tab_id: i64, deadline: Instant) -> Result<()> {
        self.send_request_with_timeout(
            "detach",
            Some(json!({ "tabId": tab_id })),
            remaining(deadline),
        )
        .await
        .map(|_| ())
    }

    async fn execute_cdp_raw_until(
        &self,
        tab_id: i64,
        method: &str,
        params: Option<Value>,
        deadline: Instant,
    ) -> Result<Box<RawValue>> {
        let raw = self
            .send_request_with_timeout(
                "executeCdp",
                Some(json!({
                    "target": { "tabId": tab_id },
                    "method": method,
                    "commandParams": params.unwrap_or_else(|| json!({}))
                })),
                remaining(deadline),
            )
            .await?;
        check_cdp_error(method, &raw)?;
        Ok(raw)
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) async fn pending_len_for_test(&self) -> usize {
        self.inner.pending.lock().await.len()
    }

    async fn close_for_health_check_failure(&self) {
        use tokio::io::AsyncWriteExt;

        let mut writer = self.inner.writer.lock().await;
        let _ = writer.shutdown().await;
        self.inner.pending.lock().await.clear();
    }
}

async fn connect_discovered_client() -> Result<Client> {
    let pipes = discovery::discover_codex_pipes().await?;
    if pipes.is_empty() {
        return Err(BridgeError::User(
            "no codex-browser-use pipes found. Start Codex Desktop, Chrome, and the Codex Chrome extension".into(),
        ));
    }

    let mut last_err = None;
    for pipe in pipes {
        let path = discovery::pipe_path(&pipe.name);
        match dial_named_pipe(&path).await {
            Ok(stream) => {
                let client = Client::from_stream(stream)?;
                match client
                    .send_request_with_timeout("getInfo", None, Duration::from_secs(5))
                    .await
                {
                    Ok(info) => {
                        tracing::debug!(
                            pipe = %pipe.name,
                            info = %truncate(info.get(), 120),
                            "auto-discovered verified browser pipe"
                        );
                        return Ok(client);
                    }
                    Err(err) => {
                        client.close_for_health_check_failure().await;
                        tracing::debug!(pipe = %pipe.name, "pipe health check failed: {err}");
                        last_err = Some(err);
                    }
                }
            }
            Err(err) => last_err = Some(err),
        }
    }

    Err(last_err.unwrap_or_else(|| BridgeError::Discovery("all pipes failed".into())))
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        let end = s
            .char_indices()
            .map(|(idx, _)| idx)
            .take_while(|idx| *idx <= n)
            .last()
            .unwrap_or(0);
        format!("{}...", &s[..end])
    }
}

fn remaining(deadline: Instant) -> Duration {
    deadline
        .saturating_duration_since(Instant::now())
        .max(Duration::from_nanos(1))
}

fn is_debugger_error(err: &BridgeError) -> bool {
    err.to_string().contains("not attached")
}

/// Check if a CDP response contains a protocol-level error.
/// CDP errors come in the envelope `{"error": {"code": ..., "message": ...}}`
/// and must be surfaced as Rust errors so the MCP layer can set `isError: true`.
fn check_cdp_error(method: &str, raw: &RawValue) -> Result<()> {
    #[derive(serde::Deserialize)]
    struct CdpError {
        code: i64,
        message: String,
    }

    #[derive(serde::Deserialize)]
    struct CdpErrorEnvelope {
        error: Option<CdpError>,
    }

    if let Ok(envelope) = serde_json::from_str::<CdpErrorEnvelope>(raw.get()) {
        if let Some(err) = envelope.error {
            return Err(BridgeError::Cdp {
                method: method.to_string(),
                code: err.code,
                message: err.message,
            });
        }
    }
    Ok(())
}
