use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use serde_json::{json, value::RawValue, Value};
use tokio::io::{split, ReadHalf, WriteHalf};
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::time::{timeout, Duration, Instant};
use uuid::Uuid;

use crate::discovery;
use crate::error::{BridgeError, Result};
use crate::pipe::{dial_named_pipe, PipeStream};
use crate::protocol::{self, Request, Response};

const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
/// Sticky-attach fast-path timeout. A healthy attached tab answers CDP within
/// milliseconds; if it stays silent this long, assume Chrome has throttled or
/// discarded the background tab and fall through to a full re-attach instead of
/// waiting the full 60s. Without this, a suspended background tab burns the
/// entire shared deadline on the sticky attempt, leaving no budget for re-attach.
const STICKY_FAST_TIMEOUT: Duration = Duration::from_secs(20);
/// Backoff schedule for a single reconnect cycle (sum ≈ 350ms).
const RECONNECT_BACKOFFS: [Duration; 3] = [
    Duration::from_millis(0),
    Duration::from_millis(100),
    Duration::from_millis(250),
];
/// After a reconnect cycle fully fails, refuse further reconnect attempts for
/// this long so a dead Codex Desktop does not cause a busy-loop of failed dials.
const RECONNECT_COOLDOWN: Duration = Duration::from_secs(5);

type PendingMap = HashMap<u64, oneshot::Sender<Response>>;

/// A factory that dials a fresh pipe connection. Production uses real
/// discovery and dial; tests inject a mock returning a `tokio::io::duplex()`
/// pair. Kept as a boxed-future closure (no async_trait) to avoid adding a
/// trait layer for a single use.
type ConnectionFactory =
    Arc<dyn Fn() -> Pin<Box<dyn Future<Output = Result<PipeStream>> + Send>> + Send + Sync>;

/// A live subscription to a CDP event stream (e.g. "Network.", "Runtime.consoleAPICalled").
/// Subscribers receive the event `params` object as a JSON Value.
struct EventSubscription {
    id: u64,
    method_prefix: String,
    sender: mpsc::Sender<Value>,
}

#[derive(Clone)]
pub struct Client {
    inner: Arc<ClientInner>,
}

struct ClientInner {
    /// `Option` so a dead connection's writer can be reclaimed (`take`), forcing
    /// in-flight writers to see `None` and return a `Connection` error instead of
    /// writing into a broken pipe.
    writer: Mutex<Option<WriteHalf<PipeStream>>>,
    pending: Mutex<PendingMap>,
    next_id: AtomicU64,
    session_id: String,
    turn_id: String,
    tab_locks: Mutex<HashMap<i64, Arc<Mutex<()>>>>,
    /// Per-tab sticky attach cache: tabs known to have an active CDP debugger session.
    /// Avoids detach+attach round-trips for repeated CDP calls on the same tab.
    attached_tabs: Mutex<HashMap<i64, bool>>,
    /// Active CDP event subscriptions. Routed by read_loop for frames with a `method` and no `id`.
    event_subs: Mutex<Vec<EventSubscription>>,
    next_sub_id: AtomicU64,
    /// Monotonic generation for the active pipe. Read loops from stale
    /// generations must not tear down a newer reconnect.
    connection_epoch: AtomicU64,
    #[cfg(test)]
    read_loop_exits: AtomicU64,
    /// Connection health. Set false by the read loop on exit; set true after a
    /// successful reconnect. Read with Acquire, written with Release.
    alive: AtomicBool,
    /// Serializes reconnect attempts so concurrent dead-connection callers only
    /// trigger one dial cycle.
    reconnect_lock: Mutex<()>,
    /// When a reconnect cycle fully fails, requests fast-fail until this instant.
    reconnect_cooldown_until: Mutex<Option<Instant>>,
    /// How to obtain a fresh connection on reconnect.
    connection_factory: ConnectionFactory,
}

impl Client {
    pub async fn connect(pipe_name: Option<&str>) -> Result<Self> {
        match pipe_name {
            Some(name) => Self::from_stream(dial_named_pipe(&discovery::pipe_path(name)).await?),
            None => connect_discovered_client().await,
        }
    }

    fn from_stream_inner(stream: PipeStream, factory: ConnectionFactory) -> Result<Self> {
        let (reader, writer) = split(stream);
        let client = Self {
            inner: Arc::new(ClientInner {
                writer: Mutex::new(Some(writer)),
                pending: Mutex::new(HashMap::new()),
                next_id: AtomicU64::new(1),
                session_id: Uuid::new_v4().to_string(),
                turn_id: Uuid::new_v4().to_string(),
                tab_locks: Mutex::new(HashMap::new()),
                attached_tabs: Mutex::new(HashMap::new()),
                event_subs: Mutex::new(Vec::new()),
                next_sub_id: AtomicU64::new(1),
                connection_epoch: AtomicU64::new(1),
                #[cfg(test)]
                read_loop_exits: AtomicU64::new(0),
                alive: AtomicBool::new(true),
                reconnect_lock: Mutex::new(()),
                reconnect_cooldown_until: Mutex::new(None),
                connection_factory: factory,
            }),
        };
        client.spawn_read_loop(reader, 1);
        Ok(client)
    }

    pub fn from_stream(stream: PipeStream) -> Result<Self> {
        Self::from_stream_inner(stream, real_connection_factory())
    }

    /// Test-only constructor: inject a custom connection factory so reconnect
    /// logic can be exercised without a real Codex Desktop pipe.
    #[cfg(all(test, not(windows)))]
    pub(crate) fn from_stream_with_factory(
        stream: PipeStream,
        factory: ConnectionFactory,
    ) -> Result<Self> {
        Self::from_stream_inner(stream, factory)
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
        // Ensure the connection is alive before the first attempt. If the read
        // loop has died since the last call, this transparently reconnects.
        self.ensure_alive().await?;

        match self
            .send_request_once(method, params.clone(), request_timeout)
            .await
        {
            Ok(raw) => Ok(raw),
            Err(err) if Self::is_connection_error(&err) => {
                // The pipe broke mid-request (or just after ensure_alive). Force a
                // reconnect and retry exactly once on the fresh connection.
                tracing::debug!(method, error = %err, "request failed, reconnecting + retrying once");
                self.force_reconnect().await?;
                self.send_request_once(method, params, request_timeout)
                    .await
            }
            Err(err) => Err(err),
        }
    }

    /// Single attempt: register pending → encode → await response. Extracted so
    /// `send_request_with_timeout` can retry on a fresh connection.
    async fn send_request_once(
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
            let mut writer_guard = self.inner.writer.lock().await;
            match writer_guard.as_mut() {
                Some(w) => protocol::encode_frame(w, &req).await,
                None => Err(BridgeError::Connection(
                    "writer is none (connection dropped)".into(),
                )),
            }
        };
        if let Err(err) = write_result {
            self.inner.pending.lock().await.remove(&id);
            return Err(err);
        }

        let resp = match timeout(request_timeout, rx).await {
            Ok(Ok(resp)) => resp,
            Ok(Err(_)) => {
                self.inner.pending.lock().await.remove(&id);
                // Channel closed = the read loop died (connection down). Surface as a
                // Connection error so the caller's retry path can trigger a reconnect.
                return Err(BridgeError::Connection(format!(
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

    fn is_alive(&self) -> bool {
        self.inner.alive.load(Ordering::Acquire)
    }

    /// Errors that mean the connection is broken and a reconnect may help.
    /// Protocol/Rpc/Cdp errors are server-side problems — reconnecting won't fix them.
    fn is_connection_error(err: &BridgeError) -> bool {
        matches!(err, BridgeError::Connection(_) | BridgeError::PipeIo(_))
    }

    /// Reconnect if the connection is dead. Double-checked locking: the second
    /// caller to arrive while the first is reconnecting sees `alive=true` and skips.
    async fn ensure_alive(&self) -> Result<()> {
        if self.is_alive() {
            return Ok(());
        }
        let _guard = self.inner.reconnect_lock.lock().await;
        if self.is_alive() {
            return Ok(());
        }
        self.reconnect_locked().await
    }

    /// Force a reconnect even if `alive` still reads true (the pipe just broke).
    async fn force_reconnect(&self) -> Result<()> {
        self.inner.alive.store(false, Ordering::Release);
        let _guard = self.inner.reconnect_lock.lock().await;
        if self.is_alive() {
            return Ok(()); // another caller already reconnected
        }
        self.reconnect_locked().await
    }

    /// Run one reconnect cycle. Caller must hold `reconnect_lock`.
    async fn reconnect_locked(&self) -> Result<()> {
        {
            let cooldown = self.inner.reconnect_cooldown_until.lock().await;
            if let Some(until) = *cooldown {
                if Instant::now() < until {
                    let wait = until.saturating_duration_since(Instant::now());
                    return Err(BridgeError::Connection(format!(
                        "reconnect cooling down, retry in {:.1}s",
                        wait.as_secs_f64()
                    )));
                }
            }
        }

        let mut last_err = None;
        for (attempt, delay) in RECONNECT_BACKOFFS.iter().enumerate() {
            if !delay.is_zero() {
                tokio::time::sleep(*delay).await;
            }
            tracing::debug!(attempt, "attempting reconnect");
            match (self.inner.connection_factory)().await {
                Ok(stream) => {
                    let (reader, writer) = split(stream);
                    let epoch = self.inner.connection_epoch.fetch_add(1, Ordering::AcqRel) + 1;
                    *self.inner.writer.lock().await = Some(writer);
                    // New connection has no CDP debugger sessions — drop the cache so
                    // execute_cdp falls back to a full attach instead of trusting a stale entry.
                    self.inner.attached_tabs.lock().await.clear();
                    *self.inner.reconnect_cooldown_until.lock().await = None;
                    self.inner.alive.store(true, Ordering::Release);
                    self.spawn_read_loop(reader, epoch);
                    tracing::info!(attempt = attempt + 1, "bridge reconnected");
                    return Ok(());
                }
                Err(err) => {
                    tracing::debug!(attempt, error = %err, "reconnect attempt failed");
                    last_err = Some(err);
                }
            }
        }

        *self.inner.reconnect_cooldown_until.lock().await =
            Some(Instant::now() + RECONNECT_COOLDOWN);
        Err(BridgeError::Connection(format!(
            "reconnect failed after {} attempts: {}",
            RECONNECT_BACKOFFS.len(),
            last_err.map(|e| e.to_string()).unwrap_or_default()
        )))
    }

    fn spawn_read_loop(&self, mut reader: ReadHalf<PipeStream>, epoch: u64) {
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
                let value: Value = match serde_json::from_slice(&frame) {
                    Ok(value) => value,
                    Err(_) => continue,
                };
                // Event frames carry a method and no id -> route the WHOLE frame
                // to subscribers so they can dispatch on method themselves
                // (e.g. Network.requestWillBeSent vs Network.responseReceived).
                if value.get("id").is_none() {
                    if let Some(method) = value.get("method").and_then(|m| m.as_str()) {
                        let matched: Vec<mpsc::Sender<Value>> = {
                            let mut subs = inner.event_subs.lock().await;
                            subs.retain(|s| !s.sender.is_closed());
                            subs.iter()
                                .filter(|s| method.starts_with(s.method_prefix.as_str()))
                                .map(|s| s.sender.clone())
                                .collect()
                        };
                        let mut closed_sender = false;
                        for tx in matched {
                            // try_send: never block the read loop on a slow consumer
                            if matches!(
                                tx.try_send(value.clone()),
                                Err(mpsc::error::TrySendError::Closed(_))
                            ) {
                                closed_sender = true;
                            }
                        }
                        if closed_sender {
                            inner
                                .event_subs
                                .lock()
                                .await
                                .retain(|s| !s.sender.is_closed());
                        }
                    }
                    continue;
                }
                // Response path.
                let response: Response = match serde_json::from_value(value) {
                    Ok(response) => response,
                    Err(_) => continue,
                };
                if let Some(id) = response.id {
                    if let Some(tx) = inner.pending.lock().await.remove(&id) {
                        let _ = tx.send(response);
                    }
                }
            }
            // ── read loop exited: the connection is dead ──
            #[cfg(test)]
            inner.read_loop_exits.fetch_add(1, Ordering::SeqCst);
            if inner.connection_epoch.load(Ordering::Acquire) != epoch {
                tracing::debug!(
                    epoch,
                    "stale bridge read loop exited; current connection unchanged"
                );
                return;
            }
            tracing::warn!("bridge read loop exited; marking connection dead");
            inner.alive.store(false, Ordering::Release);
            // Drop pending waiters' senders — they return Connection errors via the
            // RecvError → Connection translation in send_request_once.
            inner.pending.lock().await.clear();
            inner.event_subs.lock().await.clear();
            // Reclaim the writer so any in-flight encode sees None instead of
            // writing into a broken pipe.
            *inner.writer.lock().await = None;
        });
    }

    /// Subscribe to CDP events whose `method` starts with `method_prefix`
    /// (e.g. `"Network."`, `"Runtime.consoleAPICalled"`). Returns a subscription
    /// id and a receiver that yields each event's `params` object. The read loop
    /// never blocks on a slow consumer — events are dropped if the buffer is full.
    /// Call `unsubscribe_events(id)` when done.
    pub async fn subscribe_events(
        &self,
        method_prefix: &str,
        buffer: usize,
    ) -> (u64, mpsc::Receiver<Value>) {
        let (tx, rx) = mpsc::channel(buffer);
        let id = self.inner.next_sub_id.fetch_add(1, Ordering::SeqCst);
        self.inner.event_subs.lock().await.push(EventSubscription {
            id,
            method_prefix: method_prefix.to_string(),
            sender: tx,
        });
        (id, rx)
    }

    /// Remove an event subscription by id.
    pub async fn unsubscribe_events(&self, id: u64) {
        self.inner.event_subs.lock().await.retain(|s| s.id != id);
    }

    pub async fn execute_cdp(
        &self,
        tab_id: i64,
        method: &str,
        params: Option<Value>,
    ) -> Result<Box<RawValue>> {
        self.execute_cdp_with_timeout(tab_id, method, params, DEFAULT_REQUEST_TIMEOUT)
            .await
    }

    pub async fn execute_cdp_with_timeout(
        &self,
        tab_id: i64,
        method: &str,
        params: Option<Value>,
        request_timeout: Duration,
    ) -> Result<Box<RawValue>> {
        let tab_lock = self.tab_lock(tab_id).await;
        let _guard = tab_lock.lock().await;

        let already_attached = self
            .inner
            .attached_tabs
            .lock()
            .await
            .get(&tab_id)
            .copied()
            .unwrap_or(false);

        if already_attached {
            // Sticky attach: skip detach+attach, go direct to CDP.
            // Use a short independent timeout — a healthy attached tab responds
            // fast; silence past this point means the tab is likely background-
            // throttled by Chrome and we should fall through rather than burn
            // the full 60s budget on a doomed wait.
            let sticky_deadline = Instant::now() + STICKY_FAST_TIMEOUT.min(request_timeout);
            match self
                .execute_cdp_raw_until(tab_id, method, params.clone(), sticky_deadline)
                .await
            {
                Ok(raw) => return Ok(raw),
                Err(err) if is_session_invalid_error(&err) => {
                    self.inner.attached_tabs.lock().await.remove(&tab_id);
                }
                Err(err) => return Err(err),
            }
        }

        // Full attach flow. Reset the deadline so re-attach and retry get a fresh
        // budget even if the sticky fast-path just consumed STICKY_FAST_TIMEOUT.
        let deadline = Instant::now() + request_timeout;
        self.detach_tab_until(tab_id, deadline).await.ok();
        if let Err(err) = self.attach_tab_until(tab_id, deadline).await {
            self.retire_current_tab_state(tab_id).await;
            return Err(BridgeError::Protocol(format!(
                "attach failed for tab {tab_id}: {err}"
            )));
        }
        self.inner.attached_tabs.lock().await.insert(tab_id, true);

        match self
            .execute_cdp_raw_until(tab_id, method, params.clone(), deadline)
            .await
        {
            Ok(raw) => Ok(raw),
            Err(err) if is_session_invalid_error(&err) => {
                self.inner.attached_tabs.lock().await.remove(&tab_id);
                self.detach_tab_until(tab_id, deadline).await.ok();
                if let Err(err) = self.attach_tab_until(tab_id, deadline).await {
                    self.retire_current_tab_state(tab_id).await;
                    return Err(BridgeError::Protocol(format!(
                        "retry attach failed for tab {tab_id}: {err}"
                    )));
                }
                self.inner.attached_tabs.lock().await.insert(tab_id, true);
                self.execute_cdp_raw_until(tab_id, method, params, deadline)
                    .await
            }
            Err(err) => Err(err),
        }
    }

    /// Invalidate the sticky attach cache for a tab.
    pub async fn invalidate_attachment(&self, tab_id: i64) {
        self.inner.attached_tabs.lock().await.remove(&tab_id);
    }

    /// Retire per-tab state after a tab closes. The lock is removed only when no
    /// other task already cloned it, preserving serialization for in-flight work.
    pub async fn retire_tab_state(&self, tab_id: i64) {
        self.inner.attached_tabs.lock().await.remove(&tab_id);
        let mut locks = self.inner.tab_locks.lock().await;
        if let Some(lock) = locks.get(&tab_id) {
            if Arc::strong_count(lock) == 1 {
                locks.remove(&tab_id);
            }
        }
    }

    async fn retire_current_tab_state(&self, tab_id: i64) {
        self.inner.attached_tabs.lock().await.remove(&tab_id);
        let mut locks = self.inner.tab_locks.lock().await;
        if let Some(lock) = locks.get(&tab_id) {
            // The current execute_cdp call still owns one Arc while holding the
            // guard. Remove only when there are no other waiters.
            if Arc::strong_count(lock) <= 2 {
                locks.remove(&tab_id);
            }
        }
    }

    /// Mark a tab as attached (call after manual attach, e.g. from claim_user_tab).
    pub async fn mark_attached(&self, tab_id: i64) {
        self.inner.attached_tabs.lock().await.insert(tab_id, true);
    }

    /// Clear all per-tab state (call on session finalize).
    pub async fn clear_tab_state(&self) {
        self.inner.attached_tabs.lock().await.clear();
        self.inner
            .tab_locks
            .lock()
            .await
            .retain(|_, lock| Arc::strong_count(lock) > 1);
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

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) async fn tab_lock_len_for_test(&self) -> usize {
        self.inner.tab_locks.lock().await.len()
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) async fn event_sub_len_for_test(&self) -> usize {
        self.inner.event_subs.lock().await.len()
    }

    #[cfg(test)]
    #[allow(dead_code)]
    pub(crate) fn read_loop_exit_count_for_test(&self) -> u64 {
        self.inner.read_loop_exits.load(Ordering::SeqCst)
    }

    async fn close_for_health_check_failure(&self) {
        // Reclaim the writer; the read loop reads EOF and exits, which clears
        // pending + marks the connection dead.
        let mut writer = self.inner.writer.lock().await;
        if let Some(mut w) = writer.take() {
            use tokio::io::AsyncWriteExt;
            let _ = w.shutdown().await;
        }
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

/// Dial the first reachable pipe without a `getInfo` health check. Used by the
/// reconnect factory — a reachable dial is enough to resume; a bad pipe surfaces
/// on the next request and triggers another reconnect.
async fn discover_and_dial_first() -> Result<PipeStream> {
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
                tracing::debug!(pipe = %pipe.name, "reconnect dial succeeded");
                return Ok(stream);
            }
            Err(err) => last_err = Some(err),
        }
    }
    Err(last_err.unwrap_or_else(|| BridgeError::Discovery("all pipes failed".into())))
}

/// Production connection factory: discover + dial the first reachable pipe.
fn real_connection_factory() -> ConnectionFactory {
    Arc::new(|| Box::pin(async { discover_and_dial_first().await }))
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

/// Errors that indicate the CDP session is no longer valid and needs re-attach.
fn is_session_invalid_error(err: &BridgeError) -> bool {
    let msg = err.to_string().to_ascii_lowercase();
    [
        "target closed",
        "not attached",
        "no session",
        "session not found",
        "no target",
        "target does not exist",
        "cannot find target",
        "execution context destroyed",
    ]
    .iter()
    .any(|needle| msg.contains(needle))
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
                message: err.message.replace('\n', "\\n").replace('\r', "\\r"),
            });
        }
    }
    Ok(())
}

#[cfg(all(test, not(windows)))]
mod reconnect_tests {
    use super::*;
    use crate::error::BridgeError;
    use crate::protocol::{decode_frame, encode_frame};
    use std::sync::atomic::AtomicUsize;
    use tokio::io::{duplex, AsyncWriteExt, DuplexStream};
    use tokio::sync::Notify;

    const TEST_STATE_TIMEOUT: Duration = Duration::from_secs(2);

    async fn test_client(buf: usize) -> (Client, DuplexStream) {
        let (client_end, server_end) = duplex(buf);
        let client = Client::from_stream(client_end).unwrap();
        (client, server_end)
    }

    async fn next_request(server: &mut DuplexStream) -> Value {
        let frame = timeout(TEST_STATE_TIMEOUT, decode_frame(server))
            .await
            .expect("timed out waiting for client request")
            .unwrap();
        serde_json::from_slice(&frame).unwrap()
    }

    /// Read the next request frame off the server side and reply with `result`.
    async fn reply_ok(server: &mut DuplexStream) {
        let req = next_request(server).await;
        let id = req["id"].as_u64().unwrap();
        encode_frame(server, &json!({"id": id, "result": {}}))
            .await
            .unwrap();
    }

    async fn reply_ok_and_return_request(server: &mut DuplexStream) -> Value {
        let req = next_request(server).await;
        let id = req["id"].as_u64().unwrap();
        encode_frame(server, &json!({"id": id, "result": {}}))
            .await
            .unwrap();
        req
    }

    async fn reply_error(server: &mut DuplexStream, code: i64, message: &str) {
        let req = next_request(server).await;
        let id = req["id"].as_u64().unwrap();
        encode_frame(
            server,
            &json!({"id": id, "error": {"code": code, "message": message}}),
        )
        .await
        .unwrap();
    }

    async fn wait_until<F, Fut>(label: &str, mut condition: F)
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = bool>,
    {
        timeout(TEST_STATE_TIMEOUT, async {
            loop {
                if condition().await {
                    return;
                }
                tokio::task::yield_now().await;
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for {label}"));
    }

    async fn expect_pending_len(client: &Client, expected: usize) {
        wait_until(&format!("pending len {expected}"), || async {
            client.pending_len_for_test().await == expected
        })
        .await;
    }

    async fn expect_event_sub_len(client: &Client, expected: usize) {
        wait_until(&format!("event subscription len {expected}"), || async {
            client.event_sub_len_for_test().await == expected
        })
        .await;
    }

    async fn expect_alive(client: &Client, expected: bool) {
        wait_until(&format!("alive={expected}"), || async {
            client.is_alive() == expected
        })
        .await;
    }

    async fn expect_read_loop_exits(client: &Client, expected: u64) {
        wait_until(&format!("read loop exits >= {expected}"), || async {
            client.read_loop_exit_count_for_test() >= expected
        })
        .await;
    }

    #[tokio::test]
    async fn normal_roundtrip_tracks_pending() {
        let (client, mut server) = test_client(4096).await;
        let handle = tokio::spawn({
            let client = client.clone();
            async move { client.send_request("getInfo", None).await }
        });
        expect_pending_len(&client, 1).await;

        reply_ok(&mut server).await;
        let result = handle.await.unwrap();
        assert!(result.is_ok());
        expect_pending_len(&client, 0).await;
    }

    #[tokio::test]
    async fn execute_cdp_timeout_cleans_pending() {
        let (client, mut server) = test_client(4096).await;
        let handle = tokio::spawn({
            let client = client.clone();
            async move {
                client
                    .execute_cdp_with_timeout(
                        7,
                        "Page.getResourceContent",
                        None,
                        Duration::from_millis(40),
                    )
                    .await
            }
        });

        let detach = reply_ok_and_return_request(&mut server).await;
        assert_eq!(detach["method"], "detach");
        let attach = reply_ok_and_return_request(&mut server).await;
        assert_eq!(attach["method"], "attach");

        let req = next_request(&mut server).await;
        assert_eq!(req["method"], "executeCdp");
        expect_pending_len(&client, 1).await;

        let err = handle.await.unwrap().unwrap_err();
        assert!(matches!(err, BridgeError::Timeout(_)), "got: {err}");
        expect_pending_len(&client, 0).await;
    }

    #[tokio::test]
    async fn server_drop_marks_dead_and_drains_pending() {
        let (client, mut server) = test_client(4096).await;
        let handle = tokio::spawn({
            let client = client.clone();
            async move {
                client
                    .send_request_with_timeout("getInfo", None, Duration::from_secs(5))
                    .await
            }
        });
        expect_pending_len(&client, 1).await;

        // Kill the pipe: shut down + drop the server half.
        server.shutdown().await.ok();
        drop(server);
        expect_alive(&client, false).await;

        assert!(!client.is_alive(), "connection should be dead");
        expect_pending_len(&client, 0).await;

        let err = handle.await.unwrap().unwrap_err();
        assert!(matches!(err, BridgeError::Connection(_)), "got: {err}");
    }

    #[tokio::test]
    async fn server_drop_clears_event_subscriptions() {
        let (client, server) = test_client(4096).await;
        let (_sub_id, _rx) = client.subscribe_events("Network.", 8).await;
        expect_event_sub_len(&client, 1).await;

        drop(server);

        expect_alive(&client, false).await;
        expect_event_sub_len(&client, 0).await;
    }

    #[tokio::test]
    async fn closed_event_receivers_are_pruned_on_next_event() {
        let (client, mut server) = test_client(4096).await;
        let (_sub_id, rx) = client.subscribe_events("Network.", 8).await;
        expect_event_sub_len(&client, 1).await;
        drop(rx);

        encode_frame(
            &mut server,
            &json!({"method": "Network.requestWillBeSent", "params": {}}),
        )
        .await
        .unwrap();

        expect_event_sub_len(&client, 0).await;
    }

    #[tokio::test]
    async fn send_request_reconnects_on_dead_connection() {
        let (client_end1, mut server1) = duplex(4096);

        // Factory hands back a fresh duplex on each call and stashes its server
        // half where the test can answer it.
        let new_server = Arc::new(Mutex::new(None::<DuplexStream>));
        let ready = Arc::new(Notify::new());
        let call_count = Arc::new(AtomicUsize::new(0));
        let factory: ConnectionFactory = {
            let new_server = Arc::clone(&new_server);
            let ready = Arc::clone(&ready);
            let call_count = Arc::clone(&call_count);
            Arc::new(move || {
                let new_server = Arc::clone(&new_server);
                let ready = Arc::clone(&ready);
                let call_count = Arc::clone(&call_count);
                Box::pin(async move {
                    let (c, s) = duplex(4096);
                    *new_server.lock().await = Some(s);
                    call_count.fetch_add(1, Ordering::SeqCst);
                    ready.notify_one();
                    Ok(c)
                })
            })
        };

        let client = Client::from_stream_with_factory(client_end1, factory).unwrap();
        assert!(client.is_alive());

        // Kill the first connection.
        server1.shutdown().await.ok();
        drop(server1);
        expect_alive(&client, false).await;
        assert!(!client.is_alive());
        assert_eq!(call_count.load(Ordering::SeqCst), 0); // no reconnect yet

        // A request on the dead connection triggers a reconnect.
        let task = tokio::spawn({
            let client = client.clone();
            async move { client.send_request("getInfo", None).await }
        });

        timeout(TEST_STATE_TIMEOUT, ready.notified())
            .await
            .expect("timed out waiting for reconnect factory");
        let mut new_srv = new_server.lock().await.take().unwrap();
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
        assert!(client.is_alive());

        reply_ok(&mut new_srv).await;
        let result = task.await.unwrap();
        assert!(result.is_ok(), "got: {:?}", result.err());
    }

    #[tokio::test]
    async fn stale_read_loop_exit_does_not_clear_new_connection() {
        let (client_end1, server1) = duplex(4096);

        let new_server = Arc::new(Mutex::new(None::<DuplexStream>));
        let ready = Arc::new(Notify::new());
        let call_count = Arc::new(AtomicUsize::new(0));
        let factory: ConnectionFactory = {
            let new_server = Arc::clone(&new_server);
            let ready = Arc::clone(&ready);
            let call_count = Arc::clone(&call_count);
            Arc::new(move || {
                let new_server = Arc::clone(&new_server);
                let ready = Arc::clone(&ready);
                let call_count = Arc::clone(&call_count);
                Box::pin(async move {
                    let (c, s) = duplex(4096);
                    *new_server.lock().await = Some(s);
                    call_count.fetch_add(1, Ordering::SeqCst);
                    ready.notify_one();
                    Ok(c)
                })
            })
        };

        let client = Client::from_stream_with_factory(client_end1, factory).unwrap();
        client.force_reconnect().await.unwrap();
        timeout(TEST_STATE_TIMEOUT, ready.notified())
            .await
            .expect("timed out waiting for reconnect factory");
        let mut new_srv = new_server.lock().await.take().unwrap();

        drop(server1);
        expect_read_loop_exits(&client, 1).await;
        assert!(
            client.is_alive(),
            "stale read loop must not mark new connection dead"
        );
        assert_eq!(call_count.load(Ordering::SeqCst), 1);

        let task = tokio::spawn({
            let client = client.clone();
            async move { client.send_request("getInfo", None).await }
        });
        reply_ok(&mut new_srv).await;
        assert!(task.await.unwrap().is_ok());
        assert_eq!(
            call_count.load(Ordering::SeqCst),
            1,
            "new request should not need another reconnect"
        );
    }

    #[tokio::test]
    async fn reconnect_failure_returns_connection_error() {
        let (client_end, server) = duplex(4096);
        // Factory that always fails.
        let factory: ConnectionFactory =
            Arc::new(|| Box::pin(async { Err(BridgeError::User("no pipe".into())) }));
        let client = Client::from_stream_with_factory(client_end, factory).unwrap();

        drop(server); // break the read loop
        expect_alive(&client, false).await;
        assert!(!client.is_alive());

        let err = client.send_request("getInfo", None).await.unwrap_err();
        assert!(matches!(err, BridgeError::Connection(_)), "got: {err}");
    }

    #[tokio::test]
    async fn retire_tab_state_removes_idle_tab_lock() {
        let (client, _server) = test_client(4096).await;

        let lock = client.tab_lock(7).await;
        drop(lock);
        assert_eq!(client.tab_lock_len_for_test().await, 1);

        client.mark_attached(7).await;
        client.retire_tab_state(7).await;

        assert_eq!(client.tab_lock_len_for_test().await, 0);
    }

    #[tokio::test]
    async fn retire_tab_state_keeps_lock_with_waiters() {
        let (client, _server) = test_client(4096).await;

        let lock = client.tab_lock(7).await;
        assert_eq!(client.tab_lock_len_for_test().await, 1);

        client.retire_tab_state(7).await;

        assert_eq!(client.tab_lock_len_for_test().await, 1);
        drop(lock);
        client.retire_tab_state(7).await;
        assert_eq!(client.tab_lock_len_for_test().await, 0);
    }

    #[tokio::test]
    async fn clear_tab_state_removes_all_tab_locks() {
        let (client, _server) = test_client(4096).await;

        drop(client.tab_lock(7).await);
        drop(client.tab_lock(8).await);
        assert_eq!(client.tab_lock_len_for_test().await, 2);

        client.clear_tab_state().await;

        assert_eq!(client.tab_lock_len_for_test().await, 0);
    }

    #[tokio::test]
    async fn clear_tab_state_keeps_locks_with_waiters() {
        let (client, _server) = test_client(4096).await;

        let lock = client.tab_lock(7).await;
        drop(client.tab_lock(8).await);
        assert_eq!(client.tab_lock_len_for_test().await, 2);

        client.clear_tab_state().await;

        assert_eq!(client.tab_lock_len_for_test().await, 1);
        drop(lock);
        client.clear_tab_state().await;
        assert_eq!(client.tab_lock_len_for_test().await, 0);
    }

    #[tokio::test]
    async fn attach_failure_retires_idle_tab_lock() {
        let (client, mut server) = test_client(4096).await;
        let task = tokio::spawn({
            let client = client.clone();
            async move { client.execute_cdp(7, "Runtime.evaluate", None).await }
        });

        let detach = reply_ok_and_return_request(&mut server).await;
        assert_eq!(detach["method"], "detach");
        reply_error(&mut server, -32000, "No target with given id").await;

        let err = task.await.unwrap().unwrap_err();
        assert!(
            err.to_string().contains("attach failed for tab 7"),
            "got: {err}"
        );
        assert_eq!(client.tab_lock_len_for_test().await, 0);
    }
}

#[cfg(test)]
mod cdp_error_tests {
    use super::*;

    #[test]
    fn check_cdp_error_detects_error_envelope() {
        let raw =
            RawValue::from_string(r#"{"error":{"code":-32000,"message":"Target closed"}}"#.into())
                .unwrap();
        let err = check_cdp_error("Page.navigate", &raw).unwrap_err();
        match err {
            BridgeError::Cdp {
                method,
                code,
                message,
            } => {
                assert_eq!(method, "Page.navigate");
                assert_eq!(code, -32000);
                assert_eq!(message, "Target closed");
            }
            other => panic!("expected Cdp, got {other:?}"),
        }
    }

    #[test]
    fn check_cdp_error_passes_through_success() {
        let raw = RawValue::from_string(r#"{"result":{}}"#.into()).unwrap();
        assert!(check_cdp_error("Runtime.evaluate", &raw).is_ok());
    }

    #[test]
    fn check_cdp_error_sanitizes_newlines_in_message() {
        // Newlines in CDP error messages are escaped (matching RPC error handling),
        // so they can't smuggle log injection through the surfaced message.
        let raw =
            RawValue::from_string(r#"{"error":{"code":1,"message":"line1\nline2\rline3"}}"#.into())
                .unwrap();
        let err = check_cdp_error("x", &raw).unwrap_err();
        let msg = match err {
            BridgeError::Cdp { message, .. } => message,
            other => panic!("expected Cdp, got {other:?}"),
        };
        assert!(!msg.contains('\n'), "raw newline leaked: {msg:?}");
        assert!(!msg.contains('\r'));
        assert!(msg.contains("\\n"));
    }
}
