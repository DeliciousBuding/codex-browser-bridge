use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex as StdMutex;
use std::sync::{Arc, Weak};

use serde_json::{json, value::RawValue, Value};
use tokio::io::{split, ReadHalf, WriteHalf};
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::time::{timeout, Duration, Instant};
use uuid::Uuid;

use crate::discovery;
use crate::error::{BridgeError, Result};
use crate::pipe::{dial_transport, PipeStream, TransportIo};
use crate::protocol::{self, Request, Response};
use crate::registry::{self, BrowserSelector};

const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
const TURN_END_TIMEOUT: Duration = Duration::from_secs(10);
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
/// this long so a dead ChatGPT desktop app does not cause a busy-loop of failed dials.
const RECONNECT_COOLDOWN: Duration = Duration::from_secs(5);
const RECONNECT_HEALTH_CHECK_TIMEOUT: Duration = Duration::from_secs(5);

type PendingMap = HashMap<u64, oneshot::Sender<Response>>;

#[derive(Clone)]
struct SessionTurn {
    session_id: String,
    turn_id: String,
}

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
    allowed_methods: Option<Vec<String>>,
    /// When set, only events the extension attributes to this tab are delivered.
    /// Events without a `source.tabId` stay deliverable so bridge-native
    /// transports that don't tag events keep working.
    tab_id: Option<i64>,
    max_buffered_bytes: Option<usize>,
    stats: Option<EventSubscriptionStats>,
    sender: mpsc::Sender<Value>,
}

struct EventDispatchTarget {
    sender: mpsc::Sender<Value>,
    allowed_methods: Option<Vec<String>>,
    tab_id: Option<i64>,
    max_buffered_bytes: Option<usize>,
    stats: Option<EventSubscriptionStats>,
}

/// Tab the extension attributed an event frame to, when it says so.
fn event_tab_id(event: &Value) -> Option<i64> {
    event.pointer("/source/tabId").and_then(Value::as_i64)
}

/// A subscription with no tab scope accepts every event; a scoped one accepts
/// its own tab plus unscoped events from transports that don't annotate them.
fn event_in_scope(scope: Option<i64>, event: &Value) -> bool {
    match (scope, event_tab_id(event)) {
        (Some(scope), Some(actual)) => scope == actual,
        _ => true,
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EventSubscriptionSnapshot {
    pub observed_count: usize,
    pub accepted_count: usize,
    pub accepted_bytes: usize,
    pub dropped_count: usize,
    pub truncated: bool,
    pub limit_reason: Option<&'static str>,
}

#[derive(Clone)]
pub struct EventSubscriptionStats {
    inner: Arc<StdMutex<EventSubscriptionSnapshot>>,
}

impl EventSubscriptionStats {
    fn new() -> Self {
        Self {
            inner: Arc::new(StdMutex::new(EventSubscriptionSnapshot::default())),
        }
    }

    pub fn snapshot(&self) -> EventSubscriptionSnapshot {
        self.inner.lock().unwrap().clone()
    }

    fn record_observed(&self) {
        let mut stats = self.inner.lock().unwrap();
        stats.observed_count += 1;
    }

    fn can_accept(&self, bytes: usize, max_bytes: usize) -> bool {
        let mut stats = self.inner.lock().unwrap();
        if stats.truncated {
            stats.dropped_count += 1;
            return false;
        }
        if stats.accepted_bytes.saturating_add(bytes) > max_bytes {
            record_event_drop(&mut stats, "event_byte_limit");
            return false;
        }
        true
    }

    fn record_accepted(&self, bytes: usize) {
        let mut stats = self.inner.lock().unwrap();
        stats.accepted_count += 1;
        stats.accepted_bytes += bytes;
    }

    fn record_dropped(&self, reason: &'static str) {
        let mut stats = self.inner.lock().unwrap();
        record_event_drop(&mut stats, reason);
    }
}

fn record_event_drop(stats: &mut EventSubscriptionSnapshot, reason: &'static str) {
    stats.dropped_count += 1;
    stats.truncated = true;
    if stats.limit_reason.is_none() {
        stats.limit_reason = Some(reason);
    }
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
    session: tokio::sync::RwLock<SessionTurn>,
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
    /// A deliberately closed client must never reconnect in the background.
    closed: AtomicBool,
    /// Current read task. Kept so rebinding can close the previous pipe instead
    /// of leaking its half-open session.
    read_task: StdMutex<Option<tokio::task::JoinHandle<()>>>,
    /// Serializes reconnect attempts so concurrent dead-connection callers only
    /// trigger one dial cycle.
    reconnect_lock: Mutex<()>,
    /// When a reconnect cycle fully fails, requests fast-fail until this instant.
    reconnect_cooldown_until: Mutex<Option<Instant>>,
    /// How to obtain a fresh connection on reconnect.
    connection_factory: ConnectionFactory,
}

impl Drop for ClientInner {
    fn drop(&mut self) {
        // The read task holds only a Weak reference while waiting for a frame.
        // Abort it when the last Client drops so forgetting shutdown() cannot
        // leave a blocked read half and its task alive indefinitely.
        self.closed.store(true, Ordering::Release);
        self.alive.store(false, Ordering::Release);
        self.connection_epoch.fetch_add(1, Ordering::AcqRel);
        if let Ok(read_task) = self.read_task.get_mut() {
            if let Some(handle) = read_task.take() {
                handle.abort();
            }
        }
    }
}

impl Client {
    pub async fn connect(pipe_name: Option<&str>) -> Result<Self> {
        Self::connect_with_selector(pipe_name, BrowserSelector::default()).await
    }

    pub async fn connect_with_selector(
        pipe_name: Option<&str>,
        selector: BrowserSelector,
    ) -> Result<Self> {
        selector.validate()?;
        let factory = selected_connection_factory(pipe_name.map(str::to_string), selector);
        let mut stream = factory().await?;
        let session_id = Uuid::new_v4().to_string();
        let turn_id = Uuid::new_v4().to_string();
        verify_pipe_stream(&mut stream, &session_id, &turn_id).await?;
        Self::from_stream_inner_with_ids(stream, factory, session_id, turn_id)
    }

    pub fn lazy(pipe_name: Option<String>) -> Self {
        Self::lazy_with_selector(pipe_name, BrowserSelector::default())
    }

    pub fn lazy_with_selector(pipe_name: Option<String>, selector: BrowserSelector) -> Self {
        let factory = selected_connection_factory(pipe_name, selector);
        Self::from_lazy_inner(factory)
    }

    fn from_lazy_inner(factory: ConnectionFactory) -> Self {
        Self::from_lazy_inner_with_ids(factory, None)
    }

    /// Build a lazy client that reuses externally supplied session identifiers.
    ///
    /// Shell invocations are separate processes; callers that want a dependent
    /// sequence (`createTab` then `navigate`) must reuse one browser session.
    /// Passing `None` keeps the default of a fresh session per process.
    pub fn lazy_with_selector_and_ids(
        pipe_name: Option<String>,
        selector: BrowserSelector,
        ids: Option<(String, String)>,
    ) -> Self {
        let factory = selected_connection_factory(pipe_name, selector);
        Self::from_lazy_inner_with_ids(factory, ids)
    }

    fn from_lazy_inner_with_ids(factory: ConnectionFactory, ids: Option<(String, String)>) -> Self {
        let (session_id, turn_id) =
            ids.unwrap_or_else(|| (Uuid::new_v4().to_string(), Uuid::new_v4().to_string()));
        Self {
            inner: Arc::new(ClientInner {
                writer: Mutex::new(None),
                pending: Mutex::new(HashMap::new()),
                next_id: AtomicU64::new(1),
                session: tokio::sync::RwLock::new(SessionTurn {
                    session_id,
                    turn_id,
                }),
                tab_locks: Mutex::new(HashMap::new()),
                attached_tabs: Mutex::new(HashMap::new()),
                event_subs: Mutex::new(Vec::new()),
                next_sub_id: AtomicU64::new(1),
                connection_epoch: AtomicU64::new(0),
                #[cfg(test)]
                read_loop_exits: AtomicU64::new(0),
                alive: AtomicBool::new(false),
                closed: AtomicBool::new(false),
                read_task: StdMutex::new(None),
                reconnect_lock: Mutex::new(()),
                reconnect_cooldown_until: Mutex::new(None),
                connection_factory: factory,
            }),
        }
    }

    fn from_stream_inner(stream: PipeStream, factory: ConnectionFactory) -> Result<Self> {
        let session_id = Uuid::new_v4().to_string();
        let turn_id = Uuid::new_v4().to_string();
        Self::from_stream_inner_with_ids(stream, factory, session_id, turn_id)
    }

    fn from_stream_inner_with_ids(
        stream: PipeStream,
        factory: ConnectionFactory,
        session_id: String,
        turn_id: String,
    ) -> Result<Self> {
        let (reader, writer) = split(stream);
        let client = Self {
            inner: Arc::new(ClientInner {
                writer: Mutex::new(Some(writer)),
                pending: Mutex::new(HashMap::new()),
                next_id: AtomicU64::new(1),
                session: tokio::sync::RwLock::new(SessionTurn {
                    session_id,
                    turn_id,
                }),
                tab_locks: Mutex::new(HashMap::new()),
                attached_tabs: Mutex::new(HashMap::new()),
                event_subs: Mutex::new(Vec::new()),
                next_sub_id: AtomicU64::new(1),
                connection_epoch: AtomicU64::new(1),
                #[cfg(test)]
                read_loop_exits: AtomicU64::new(0),
                alive: AtomicBool::new(true),
                closed: AtomicBool::new(false),
                read_task: StdMutex::new(None),
                reconnect_lock: Mutex::new(()),
                reconnect_cooldown_until: Mutex::new(None),
                connection_factory: factory,
            }),
        };
        client.spawn_read_loop(reader, 1);
        Ok(client)
    }

    /// Stop reconnecting and close the active pipe read loop. Pending requests
    /// fail as `Connection` errors instead of hanging until their timeout.
    pub async fn shutdown(&self) {
        if self.inner.closed.swap(true, Ordering::AcqRel) {
            return;
        }
        self.inner.alive.store(false, Ordering::Release);
        self.inner.connection_epoch.fetch_add(1, Ordering::AcqRel);
        if let Some(handle) = self
            .inner
            .read_task
            .lock()
            .expect("client read-task lock poisoned")
            .take()
        {
            handle.abort();
        }
        self.inner.writer.lock().await.take();
        self.inner.pending.lock().await.clear();
        self.inner.event_subs.lock().await.clear();
        self.inner.tab_locks.lock().await.clear();
        self.inner.attached_tabs.lock().await.clear();
        *self.inner.reconnect_cooldown_until.lock().await = None;
    }

    /// End the current browser turn without closing the pipe. The extension
    /// closes unmarked agent-created tabs, releases unmarked claimed user tabs,
    /// and preserves deliverable/handoff marks. A fresh turn id is used for
    /// subsequent requests in this MCP process.
    /// Returns `true` only when the extension acknowledged `turnEnded`.
    pub async fn end_turn(&self) -> Result<bool> {
        if self.inner.closed.load(Ordering::Acquire) || !self.is_alive() {
            self.clear_tab_state().await;
            return Ok(false);
        }

        let deadline = Instant::now() + TURN_END_TIMEOUT;
        let attached_tabs: Vec<i64> = self
            .inner
            .attached_tabs
            .lock()
            .await
            .keys()
            .copied()
            .collect();
        for tab_id in attached_tabs {
            if let Err(err) = self.detach_tab_until(tab_id, deadline).await {
                // A tab may already be closed. turnEnded below remains the
                // authoritative session cleanup operation.
                tracing::debug!(tab_id, error = %err, "detach before turnEnded failed");
            }
        }

        self.send_request_with_timeout("turnEnded", None, TURN_END_TIMEOUT)
            .await?;
        {
            let mut session = self.inner.session.write().await;
            session.turn_id = Uuid::new_v4().to_string();
        }
        self.clear_tab_state().await;
        Ok(true)
    }

    /// Use an externally supplied stream. Its browser identity cannot be
    /// rediscovered safely; use `connect`/`lazy` for identity-pinned reconnects.
    pub fn from_stream<T: TransportIo + 'static>(stream: T) -> Result<Self> {
        Self::from_stream_inner(Box::new(stream), supplied_stream_connection_factory())
    }

    /// Test-only constructor: inject a custom connection factory so reconnect
    /// logic can be exercised without a real ChatGPT desktop browser-extension pipe.
    #[cfg(test)]
    pub(crate) fn from_stream_with_factory<T: TransportIo + 'static>(
        stream: T,
        factory: ConnectionFactory,
    ) -> Result<Self> {
        Self::from_stream_inner(Box::new(stream), factory)
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
            Err(err) if Self::is_connection_error(&err) && replay_safe(method) => {
                // The pipe broke mid-request (or just after ensure_alive). Force a
                // reconnect and retry exactly once on the fresh connection.
                tracing::debug!(method, error = %err, "request failed, reconnecting + retrying once");
                self.force_reconnect().await?;
                self.send_request_once(method, params, request_timeout)
                    .await
            }
            Err(err) if Self::is_connection_error(&err) => {
                Err(BridgeError::Connection(format!(
                    "{method} may have reached the browser before the pipe failed; outcome unknown, so it was not replayed: {err}"
                )))
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
        let session = self.inner.session.read().await.clone();
        let params = protocol::with_session_params(&session.session_id, &session.turn_id, params);
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

        response_result(method, resp)
    }

    fn is_alive(&self) -> bool {
        self.inner.alive.load(Ordering::Acquire)
    }

    pub async fn has_live_connection(&self) -> bool {
        self.is_alive() && self.inner.writer.lock().await.is_some()
    }

    /// Current session and turn identifiers, used by the shell to persist one
    /// browser session across separate processes.
    pub async fn session_ids(&self) -> (String, String) {
        let session = self.inner.session.read().await.clone();
        (session.session_id, session.turn_id)
    }

    /// Dial if this client has not connected yet.
    ///
    /// A separate shell process can resume a session through `--session-file`
    /// without having issued a request. Turn cleanup must still reach the
    /// extension, so callers that end a turn may force the connection first.
    pub async fn connect_if_needed(&self) -> Result<()> {
        self.ensure_alive().await
    }

    /// Errors that mean the connection is broken and a reconnect may help.
    /// Protocol/Rpc/Cdp errors are server-side problems — reconnecting won't fix them.
    fn is_connection_error(err: &BridgeError) -> bool {
        matches!(err, BridgeError::Connection(_) | BridgeError::PipeIo(_))
    }

    /// Reconnect if the connection is dead. Double-checked locking: the second
    /// caller to arrive while the first is reconnecting sees `alive=true` and skips.
    async fn ensure_alive(&self) -> Result<()> {
        if self.inner.closed.load(Ordering::Acquire) {
            return Err(BridgeError::Connection("client is closed".into()));
        }
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
        if self.inner.closed.load(Ordering::Acquire) {
            return Err(BridgeError::Connection("client is closed".into()));
        }
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
                Ok(mut stream) => {
                    let session = self.inner.session.read().await.clone();
                    match verify_pipe_stream(&mut stream, &session.session_id, &session.turn_id)
                        .await
                    {
                        Ok(info) => {
                            tracing::debug!(
                                attempt = attempt + 1,
                                info = %truncate(info.get(), 120),
                                "reconnect health check succeeded"
                            );
                        }
                        Err(err) => {
                            tracing::debug!(
                                attempt = attempt + 1,
                                error = %err,
                                "reconnect health check failed"
                            );
                            last_err = Some(err);
                            continue;
                        }
                    }
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
        let inner = Arc::downgrade(&self.inner);
        let handle = tokio::spawn(async move {
            loop {
                let frame = match protocol::decode_frame(&mut reader).await {
                    Ok(frame) => frame,
                    Err(err) => {
                        tracing::debug!("read loop ended: {err}");
                        break;
                    }
                };
                // Keep no strong reference across the next pipe read. A dropped
                // client would otherwise be kept alive forever by its own read
                // task, which is especially costly for discovery probes.
                let Some(inner) = Weak::upgrade(&inner) else {
                    return;
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
                        // The extension wraps debugger notifications as
                        // `onCDPEvent` with `{method, params, source}` nested
                        // under `params`. Unwrap that envelope before routing so
                        // subscribers see the same shape as direct CDP frames.
                        let event = if method == "onCDPEvent" {
                            let Some(event) = value.get("params").cloned() else {
                                continue;
                            };
                            event
                        } else {
                            value.clone()
                        };
                        let Some(event_method) = event.get("method").and_then(|m| m.as_str())
                        else {
                            continue;
                        };
                        let event_bytes = serde_json::to_vec(&event)
                            .map(|encoded| encoded.len())
                            .unwrap_or(frame.len());
                        let matched: Vec<EventDispatchTarget> = {
                            let mut subs = inner.event_subs.lock().await;
                            subs.retain(|s| !s.sender.is_closed());
                            subs.iter()
                                .filter(|s| event_method.starts_with(s.method_prefix.as_str()))
                                .map(|s| EventDispatchTarget {
                                    sender: s.sender.clone(),
                                    allowed_methods: s.allowed_methods.clone(),
                                    tab_id: s.tab_id,
                                    max_buffered_bytes: s.max_buffered_bytes,
                                    stats: s.stats.clone(),
                                })
                                .collect()
                        };
                        let mut closed_sender = false;
                        for sub in matched {
                            // Tab scope is decided before any bookkeeping so a
                            // noisy neighbour tab can't consume this
                            // subscription's byte budget.
                            if !event_in_scope(sub.tab_id, &event) {
                                continue;
                            }
                            if let Some(stats) = &sub.stats {
                                stats.record_observed();
                            }
                            if let Some(allowed) = &sub.allowed_methods {
                                if !allowed.iter().any(|allowed| allowed == event_method) {
                                    continue;
                                }
                            }
                            if let (Some(stats), Some(max_bytes)) =
                                (&sub.stats, sub.max_buffered_bytes)
                            {
                                if !stats.can_accept(event_bytes, max_bytes) {
                                    continue;
                                }
                            }
                            // try_send: never block the read loop on a slow consumer
                            match sub.sender.try_send(event.clone()) {
                                Ok(()) => {
                                    if let Some(stats) = &sub.stats {
                                        stats.record_accepted(event_bytes);
                                    }
                                }
                                Err(mpsc::error::TrySendError::Closed(_)) => {
                                    closed_sender = true;
                                }
                                Err(mpsc::error::TrySendError::Full(_)) => {
                                    if let Some(stats) = &sub.stats {
                                        stats.record_dropped("event_buffer_full");
                                    }
                                }
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
            let Some(inner) = Weak::upgrade(&inner) else {
                return;
            };
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
        if let Some(previous) = self
            .inner
            .read_task
            .lock()
            .expect("client read-task lock poisoned")
            .replace(handle)
        {
            previous.abort();
        }
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
        let (id, rx, _) = self
            .subscribe_events_inner(method_prefix, buffer, None, None, None, None)
            .await;
        (id, rx)
    }

    pub async fn subscribe_events_with_budget(
        &self,
        method_prefix: &str,
        allowed_methods: &[&str],
        buffer: usize,
        max_buffered_bytes: usize,
    ) -> (u64, mpsc::Receiver<Value>, EventSubscriptionStats) {
        self.subscribe_events_with_budget_scoped(
            method_prefix,
            allowed_methods,
            buffer,
            max_buffered_bytes,
            None,
        )
        .await
    }

    /// Same as [`Self::subscribe_events_with_budget`], but when the transport
    /// tags events with `source.tabId` only that tab's events are observed or
    /// buffered. Untagged events are still delivered, since not every transport
    /// annotates them.
    pub async fn subscribe_events_with_budget_for_tab(
        &self,
        method_prefix: &str,
        allowed_methods: &[&str],
        buffer: usize,
        max_buffered_bytes: usize,
        tab_id: i64,
    ) -> (u64, mpsc::Receiver<Value>, EventSubscriptionStats) {
        self.subscribe_events_with_budget_scoped(
            method_prefix,
            allowed_methods,
            buffer,
            max_buffered_bytes,
            Some(tab_id),
        )
        .await
    }

    async fn subscribe_events_with_budget_scoped(
        &self,
        method_prefix: &str,
        allowed_methods: &[&str],
        buffer: usize,
        max_buffered_bytes: usize,
        tab_id: Option<i64>,
    ) -> (u64, mpsc::Receiver<Value>, EventSubscriptionStats) {
        let stats = EventSubscriptionStats::new();
        let allowed_methods = if allowed_methods.is_empty() {
            None
        } else {
            Some(
                allowed_methods
                    .iter()
                    .map(|method| method.to_string())
                    .collect(),
            )
        };
        let (id, rx, _) = self
            .subscribe_events_inner(
                method_prefix,
                buffer,
                allowed_methods,
                tab_id,
                Some(max_buffered_bytes),
                Some(stats.clone()),
            )
            .await;
        (id, rx, stats)
    }

    async fn subscribe_events_inner(
        &self,
        method_prefix: &str,
        buffer: usize,
        allowed_methods: Option<Vec<String>>,
        tab_id: Option<i64>,
        max_buffered_bytes: Option<usize>,
        stats: Option<EventSubscriptionStats>,
    ) -> (u64, mpsc::Receiver<Value>, Option<EventSubscriptionStats>) {
        let (tx, rx) = mpsc::channel(buffer);
        let id = self.inner.next_sub_id.fetch_add(1, Ordering::SeqCst);
        self.inner.event_subs.lock().await.push(EventSubscription {
            id,
            method_prefix: method_prefix.to_string(),
            allowed_methods,
            tab_id,
            max_buffered_bytes,
            stats: stats.clone(),
            sender: tx,
        });
        (id, rx, stats)
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
                Err(err) => {
                    // A silent sticky attempt usually means the extension dropped
                    // the debugger (background throttling, renderer restart) and
                    // will keep rejecting every later command for this tab. Drop
                    // the cached attachment so the next call performs a full
                    // re-attach. The command is deliberately not replayed.
                    //
                    // Only a full sticky-window silence counts as evidence. A
                    // caller-supplied timeout shorter than that window (input
                    // cleanup, for example) is a deliberate short deadline, not a
                    // dead debugger, so the attachment stays valid.
                    if sticky_failure_invalidates(&err, request_timeout) {
                        self.inner.attached_tabs.lock().await.remove(&tab_id);
                    }
                    return Err(err);
                }
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

    /// Execute one state-changing CDP command at most once.
    ///
    /// `execute_cdp_with_timeout` may recover an invalid debugger session and
    /// retry the command. That is useful for observations, but unsafe for
    /// pointer input: a retried click or drop can execute twice. This variant
    /// still establishes the attachment when needed, then deliberately does
    /// not retry the mutation. An invalidated session is reported as an
    /// ambiguous outcome so callers cannot silently replay it.
    pub async fn execute_cdp_once_with_timeout(
        &self,
        tab_id: i64,
        method: &str,
        params: Option<Value>,
        request_timeout: Duration,
    ) -> Result<Box<RawValue>> {
        let tab_lock = self.tab_lock(tab_id).await;
        let _guard = tab_lock.lock().await;

        let deadline = Instant::now() + request_timeout;
        let already_attached = self
            .inner
            .attached_tabs
            .lock()
            .await
            .get(&tab_id)
            .copied()
            .unwrap_or(false);
        if !already_attached {
            self.detach_tab_until(tab_id, deadline).await.ok();
            if let Err(err) = self.attach_tab_until(tab_id, deadline).await {
                self.retire_current_tab_state(tab_id).await;
                return Err(BridgeError::Protocol(format!(
                    "attach failed for tab {tab_id}: {err}"
                )));
            }
            self.inner.attached_tabs.lock().await.insert(tab_id, true);
        }

        match self
            .execute_cdp_raw_until(tab_id, method, params, deadline)
            .await
        {
            Ok(raw) => Ok(raw),
            Err(err) if is_session_invalid_error(&err) => {
                self.inner.attached_tabs.lock().await.remove(&tab_id);
                Err(BridgeError::Connection(format!(
                    "{method} may have reached the browser before its CDP session became invalid; outcome unknown, so it was not replayed: {err}"
                )))
            }
            // A timeout here is expected for input commands the extension applies
            // without acknowledging (for example `mouseMoved`); it does not mean
            // the debugger is gone, so the attachment stays valid. A real
            // detached session is matched above.
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
            remaining_before_write("attach", deadline)?,
        )
        .await
        .map(|_| ())
    }

    async fn detach_tab_until(&self, tab_id: i64, deadline: Instant) -> Result<()> {
        self.send_request_with_timeout(
            "detach",
            Some(json!({ "tabId": tab_id })),
            remaining_before_write("detach", deadline)?,
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
                remaining_before_write(method, deadline)?,
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
}

fn response_result(method: &str, resp: Response) -> Result<Box<RawValue>> {
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
        // The extension serializes JavaScript `undefined` returns by omitting
        // `result`. Accept this only for its known void native methods, not
        // for discovery/data-bearing responses where omission is corruption.
        None if matches!(
            method,
            "attach"
                | "attachTarget"
                | "detach"
                | "detachTarget"
                | "markTab"
                | "nameSession"
                | "turnEnded"
                | "moveMouse"
                | "allowDownload"
                | "setBlobDownloadOrigin"
                | "followSessionTab"
        ) =>
        {
            RawValue::from_string("null".into())
                .map_err(|err| BridgeError::Protocol(err.to_string()))
        }
        None => Err(BridgeError::Protocol(format!(
            "response for {method} contained neither result nor error"
        ))),
    }
}

/// Only replay requests whose repeated execution cannot compound browser-side
/// effects. In particular, never replay tab creation/claiming, interaction,
/// arbitrary CDP, management mutations, file input, or turn cleanup after an
/// ambiguous connection failure.
fn replay_safe(method: &str) -> bool {
    matches!(
        method,
        "getInfo"
            | "getTabs"
            | "getUserTabs"
            | "getUserHistory"
            | "ping"
            | "attach"
            | "detach"
            | "nameSession"
            | "markTab"
    )
}

pub(crate) async fn verify_pipe_stream(
    stream: &mut PipeStream,
    session_id: &str,
    turn_id: &str,
) -> Result<Box<RawValue>> {
    let params = protocol::with_session_params(session_id, turn_id, None);
    let req = Request::new(0, "getInfo", params);
    protocol::encode_frame(stream, &req).await?;
    let deadline = Instant::now() + RECONNECT_HEALTH_CHECK_TIMEOUT;
    loop {
        let frame = timeout(remaining(deadline), protocol::decode_frame(stream))
            .await
            .map_err(|_| BridgeError::Timeout("getInfo".into()))??;
        let resp: Response = serde_json::from_slice(&frame)
            .map_err(|err| BridgeError::Protocol(format!("invalid getInfo response: {err}")))?;
        if resp.id == Some(0) {
            let raw = response_result("getInfo", resp)?;
            let info: Value = serde_json::from_str(raw.get()).map_err(|err| {
                BridgeError::Protocol(format!("invalid getInfo identity response: {err}"))
            })?;
            if info.get("type").and_then(Value::as_str) != Some("extension")
                || info
                    .get("family")
                    .and_then(Value::as_str)
                    .is_none_or(|family| family.trim().is_empty())
            {
                return Err(BridgeError::Protocol(
                    "getInfo did not identify a browser extension with a browser family".into(),
                ));
            }
            return Ok(raw);
        }
        if Instant::now() >= deadline {
            return Err(BridgeError::Timeout("getInfo".into()));
        }
    }
}

fn supplied_stream_connection_factory() -> ConnectionFactory {
    Arc::new(|| {
        Box::pin(async {
            Err(BridgeError::Connection("externally supplied transport closed; reconnect requires an explicit browser selector".into()))
        })
    })
}

fn endpoint_connection_factory(pipe_name: String) -> ConnectionFactory {
    Arc::new(move || {
        let path = discovery::pipe_path(&pipe_name);
        Box::pin(async move { dial_transport(&path).await })
    })
}

fn selected_connection_factory(
    pipe_name: Option<String>,
    selector: BrowserSelector,
) -> ConnectionFactory {
    if let Some(pipe_name) = pipe_name {
        return endpoint_connection_factory(pipe_name);
    }

    let pinned_instance = Arc::new(Mutex::new(None::<String>));
    Arc::new(move || {
        let selector = selector.clone();
        let pinned_instance = Arc::clone(&pinned_instance);
        Box::pin(async move {
            let mut pinned = pinned_instance.lock().await;
            if let Some(instance_id) = pinned.as_deref() {
                return dial_browser_instance(instance_id).await;
            }

            let inventory = registry::discover_browsers().await?;
            let browser = registry::select_browser(&inventory.browsers, &selector)?;
            let mut stream = dial_transport(&browser.transport.path).await?;
            verify_selected_stream(&mut stream, Some(&browser.family), Some(&browser.id)).await?;
            *pinned = Some(browser.id.clone());
            tracing::info!(
                browser_id = %browser.id,
                family = %browser.family,
                name = %browser.name,
                pipe = %browser.transport.name,
                "selected browser-extension instance"
            );
            Ok(stream)
        })
    })
}

async fn dial_browser_instance(instance_id: &str) -> Result<PipeStream> {
    let inventory = registry::discover_browsers().await?;
    let selector = BrowserSelector::new(None, Some(instance_id.to_string()));
    let browser = registry::select_browser(&inventory.browsers, &selector)?;
    let mut stream = dial_transport(&browser.transport.path).await?;
    verify_selected_stream(&mut stream, Some(&browser.family), Some(instance_id)).await?;
    Ok(stream)
}

async fn verify_selected_stream(
    stream: &mut PipeStream,
    expected_family: Option<&str>,
    expected_instance_id: Option<&str>,
) -> Result<()> {
    let session_id = Uuid::new_v4().to_string();
    let turn_id = Uuid::new_v4().to_string();
    let raw = verify_pipe_stream(stream, &session_id, &turn_id).await?;
    let info: Value = serde_json::from_str(raw.get()).map_err(|err| {
        BridgeError::Protocol(format!("invalid getInfo identity response: {err}"))
    })?;
    if info.get("type").and_then(Value::as_str) != Some("extension") {
        return Err(BridgeError::Protocol(
            "selected pipe did not identify a browser extension".into(),
        ));
    }
    if let Some(expected) = expected_family {
        let actual = info
            .get("family")
            .and_then(Value::as_str)
            .ok_or_else(|| BridgeError::Protocol("getInfo omitted browser family".into()))?;
        if !actual.eq_ignore_ascii_case(expected) {
            return Err(BridgeError::Protocol(format!(
                "browser identity changed while connecting: expected family {expected:?}, got {actual:?}"
            )));
        }
    }
    if let Some(expected) = expected_instance_id {
        let actual = info
            .get("metadata")
            .and_then(|metadata| metadata.get("extensionInstanceId"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                BridgeError::Protocol("getInfo omitted metadata.extensionInstanceId".into())
            })?;
        if actual != expected {
            return Err(BridgeError::Protocol(format!(
                "browser identity changed while connecting: expected instance {expected:?}, got {actual:?}"
            )));
        }
    }
    Ok(())
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

fn remaining_before_write(method: &str, deadline: Instant) -> Result<Duration> {
    if Instant::now() >= deadline {
        return Err(BridgeError::Timeout(method.to_string()));
    }
    Ok(remaining(deadline))
}

/// Errors that indicate the CDP session is no longer valid and needs re-attach.
fn is_session_invalid_error(err: &BridgeError) -> bool {
    let msg = err.to_string().to_ascii_lowercase();
    [
        "target closed",
        "not attached",
        // The extension reports a dropped debugger as "Debugger unattached",
        // which does not contain "not attached".
        "unattached",
        "debugger detached",
        "debugger is not attached",
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

/// Whether a sticky-attach failure is evidence that the cached attachment is
/// stale. Only a silence longer than the sticky window, or a broken connection,
/// qualifies; a deliberately short caller timeout does not.
fn sticky_failure_invalidates(err: &BridgeError, request_timeout: Duration) -> bool {
    request_timeout >= STICKY_FAST_TIMEOUT
        && (matches!(err, BridgeError::Timeout(_)) || Client::is_connection_error(err))
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

#[cfg(test)]
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

    async fn no_request(server: &mut DuplexStream, reason: &str) {
        if let Ok(Ok(frame)) = timeout(Duration::from_millis(50), decode_frame(server)).await {
            panic!(
                "{reason}: unexpected pipe request {}",
                String::from_utf8_lossy(&frame)
            );
        }
    }

    /// Read the next request frame off the server side and reply with `result`.
    async fn reply_ok(server: &mut DuplexStream) {
        let req = next_request(server).await;
        let id = req["id"].as_u64().unwrap();
        let result = if req["method"] == "getInfo" {
            json!({"type": "extension", "family": "chrome"})
        } else {
            json!({})
        };
        encode_frame(server, &json!({"id": id, "result": result}))
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
    async fn verified_stream_session_ids_are_reused_for_requests() {
        let (client_end, mut server) = duplex(4096);
        let client = Client::from_stream_inner_with_ids(
            Box::new(client_end),
            supplied_stream_connection_factory(),
            "verified-session".into(),
            "verified-turn".into(),
        )
        .unwrap();
        let handle = tokio::spawn({
            let client = client.clone();
            async move { client.send_request("getTabs", None).await }
        });

        let request = next_request(&mut server).await;
        assert_eq!(request["params"]["session_id"], "verified-session");
        assert_eq!(request["params"]["turn_id"], "verified-turn");
        let id = request["id"].as_u64().unwrap();
        encode_frame(&mut server, &json!({"id": id, "result": []}))
            .await
            .unwrap();
        assert!(handle.await.unwrap().is_ok());
        client.shutdown().await;
    }

    #[tokio::test]
    async fn shutdown_fails_pending_requests_and_does_not_reconnect() {
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

        let _in_flight = next_request(&mut server).await;
        client.shutdown().await;

        let err = handle.await.unwrap().unwrap_err();
        assert!(matches!(err, BridgeError::Connection(_)), "got: {err}");
        expect_pending_len(&client, 0).await;
        let err = client.send_request("getInfo", None).await.unwrap_err();
        assert!(matches!(err, BridgeError::Connection(_)), "got: {err}");
        no_request(&mut server, "closed client must not reconnect or send").await;
    }

    #[tokio::test]
    async fn dropping_last_client_aborts_its_read_loop() {
        let (client, _server) = test_client(4096).await;
        let weak = Arc::downgrade(&client.inner);

        drop(client);
        wait_until("last client inner dropped", || async {
            weak.upgrade().is_none()
        })
        .await;
    }

    #[tokio::test]
    async fn end_turn_detaches_then_rotates_turn_id() {
        let (client, mut server) = test_client(4096).await;
        client.mark_attached(7).await;

        let task = tokio::spawn({
            let client = client.clone();
            async move { client.end_turn().await }
        });
        let detach = next_request(&mut server).await;
        assert_eq!(detach["method"], "detach");
        assert_eq!(detach["params"]["tabId"], 7);
        encode_frame(&mut server, &json!({"id": detach["id"], "result": {}}))
            .await
            .unwrap();

        let ended = next_request(&mut server).await;
        assert_eq!(ended["method"], "turnEnded");
        let session_id = ended["params"]["session_id"].as_str().unwrap().to_string();
        let old_turn_id = ended["params"]["turn_id"].as_str().unwrap().to_string();
        encode_frame(&mut server, &json!({"id": ended["id"], "result": {}}))
            .await
            .unwrap();
        task.await.unwrap().unwrap();

        let next = tokio::spawn({
            let client = client.clone();
            async move { client.send_request("getInfo", None).await }
        });
        let request = next_request(&mut server).await;
        assert_eq!(request["params"]["session_id"], session_id);
        assert_ne!(request["params"]["turn_id"], old_turn_id);
        encode_frame(
            &mut server,
            &json!({"id": request["id"], "result": {"type":"extension","family":"chrome"}}),
        )
        .await
        .unwrap();
        assert!(next.await.unwrap().is_ok());
        client.shutdown().await;
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
    async fn expired_cdp_deadline_does_not_write_frame() {
        let (client, mut server) = test_client(4096).await;

        let err = client
            .execute_cdp_raw_until(
                7,
                "Runtime.evaluate",
                Some(json!({"expression":"window.sideEffect = true"})),
                Instant::now(),
            )
            .await
            .unwrap_err();

        assert!(matches!(err, BridgeError::Timeout(_)), "got: {err}");
        no_request(&mut server, "expired deadline should fail before pipe use").await;
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
    async fn extension_event_envelope_is_unwrapped_for_subscribers() {
        let (client, mut server) = test_client(4096).await;
        let (_sub_id, mut rx) = client.subscribe_events("Runtime.", 8).await;

        encode_frame(
            &mut server,
            &json!({
                "method": "onCDPEvent",
                "params": {
                    "method": "Runtime.consoleAPICalled",
                    "params": {"type": "log"},
                    "source": {"tabId": 7}
                }
            }),
        )
        .await
        .unwrap();

        let event = timeout(TEST_STATE_TIMEOUT, rx.recv())
            .await
            .expect("event timeout")
            .expect("event channel closed");
        assert_eq!(event["method"], "Runtime.consoleAPICalled");
        assert_eq!(event["params"]["type"], "log");
        assert_eq!(event["source"]["tabId"], 7);
    }

    #[tokio::test]
    async fn tab_scoped_subscription_ignores_other_tabs_before_observing() {
        let (client, mut server) = test_client(8192).await;
        let (_sub_id, mut rx, stats) = client
            .subscribe_events_with_budget_for_tab(
                "Network.",
                &["Network.requestWillBeSent"],
                8,
                200,
                7,
            )
            .await;

        // A neighbour tab must not reach the receiver...
        encode_frame(
            &mut server,
            &json!({
                "method": "onCDPEvent",
                "params": {
                    "method": "Network.requestWillBeSent",
                    "params": {"requestId": "other"},
                    "source": {"tabId": 8}
                }
            }),
        )
        .await
        .unwrap();
        // ...and it must not eat the byte budget either: each event below is
        // ~90 bytes, so a 200-byte budget fits exactly these two and nothing
        // more. A neighbour that reached accounting would truncate us.
        encode_frame(
            &mut server,
            &json!({
                "method": "onCDPEvent",
                "params": {
                    "method": "Network.requestWillBeSent",
                    "params": {"requestId": "mine"},
                    "source": {"tabId": 7}
                }
            }),
        )
        .await
        .unwrap();
        // Untagged events stay compatible with transports that don't say which
        // tab they came from.
        encode_frame(
            &mut server,
            &json!({"method":"Network.requestWillBeSent","params":{"requestId":"untagged"}}),
        )
        .await
        .unwrap();

        let first = timeout(TEST_STATE_TIMEOUT, rx.recv())
            .await
            .expect("event timeout")
            .expect("event channel closed");
        let second = timeout(TEST_STATE_TIMEOUT, rx.recv())
            .await
            .expect("event timeout")
            .expect("event channel closed");
        assert_eq!(first["params"]["requestId"], "mine");
        assert_eq!(second["params"]["requestId"], "untagged");

        let snapshot = stats.snapshot();
        assert_eq!(snapshot.observed_count, 2, "neighbour tab was observed");
        assert_eq!(snapshot.accepted_count, 2);
        assert!(!snapshot.truncated);
    }

    #[tokio::test]
    async fn budgeted_event_subscription_filters_before_enqueue() {
        let (client, mut server) = test_client(8192).await;
        let (_sub_id, mut rx, stats) = client
            .subscribe_events_with_budget(
                "Network.",
                &["Network.requestWillBeSent", "Network.responseReceived"],
                8,
                4096,
            )
            .await;

        encode_frame(
            &mut server,
            &json!({"method":"Network.dataReceived","params":{"requestId":"1","payload":"x".repeat(1000)}}),
        )
        .await
        .unwrap();
        encode_frame(
            &mut server,
            &json!({"method":"Network.requestWillBeSent","params":{"requestId":"1"}}),
        )
        .await
        .unwrap();
        encode_frame(
            &mut server,
            &json!({"method":"Network.responseReceived","params":{"requestId":"1"}}),
        )
        .await
        .unwrap();

        wait_until("event subscription stats observed=3", || {
            let stats = stats.clone();
            async move { stats.snapshot().observed_count == 3 }
        })
        .await;

        assert_eq!(
            rx.try_recv().unwrap()["method"],
            json!("Network.requestWillBeSent")
        );
        assert_eq!(
            rx.try_recv().unwrap()["method"],
            json!("Network.responseReceived")
        );
        assert!(rx.try_recv().is_err());
        let snapshot = stats.snapshot();
        assert_eq!(snapshot.accepted_count, 2);
        assert_eq!(snapshot.dropped_count, 0);
        assert!(!snapshot.truncated);
    }

    #[tokio::test]
    async fn budgeted_event_subscription_stops_after_byte_limit() {
        let (client, mut server) = test_client(8192).await;
        let (_sub_id, mut rx, stats) = client
            .subscribe_events_with_budget("Runtime.consoleAPICalled", &[], 8, 180)
            .await;

        encode_frame(
            &mut server,
            &json!({"method":"Runtime.consoleAPICalled","params":{"args":[{"value":"ok"}]}}),
        )
        .await
        .unwrap();
        encode_frame(
            &mut server,
            &json!({"method":"Runtime.consoleAPICalled","params":{"args":[{"value":"x".repeat(500)}]}}),
        )
        .await
        .unwrap();
        encode_frame(
            &mut server,
            &json!({"method":"Runtime.consoleAPICalled","params":{"args":[{"value":"after-limit"}]}}),
        )
        .await
        .unwrap();

        wait_until("event subscription stats observed=3", || {
            let stats = stats.clone();
            async move { stats.snapshot().observed_count == 3 }
        })
        .await;

        assert_eq!(
            rx.try_recv().unwrap()["method"],
            json!("Runtime.consoleAPICalled")
        );
        assert!(rx.try_recv().is_err());
        let snapshot = stats.snapshot();
        assert_eq!(snapshot.accepted_count, 1);
        assert_eq!(snapshot.dropped_count, 2);
        assert!(snapshot.truncated);
        assert_eq!(snapshot.limit_reason, Some("event_byte_limit"));
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
                    Ok(Box::new(c) as PipeStream)
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

        encode_frame(
            &mut new_srv,
            &json!({"method": "Network.requestWillBeSent", "params": {}}),
        )
        .await
        .unwrap();
        reply_ok(&mut new_srv).await;
        expect_alive(&client, true).await;
        reply_ok(&mut new_srv).await;
        let result = task.await.unwrap();
        assert!(result.is_ok(), "got: {:?}", result.err());
    }

    #[tokio::test]
    async fn lazy_client_connects_on_first_request() {
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
                    Ok(Box::new(c) as PipeStream)
                })
            })
        };
        let client = Client::from_lazy_inner(factory);

        assert!(!client.is_alive());
        assert_eq!(call_count.load(Ordering::SeqCst), 0);

        let task = tokio::spawn({
            let client = client.clone();
            async move { client.send_request("getInfo", None).await }
        });

        timeout(TEST_STATE_TIMEOUT, ready.notified())
            .await
            .expect("timed out waiting for lazy connect factory");
        let mut new_srv = new_server.lock().await.take().unwrap();
        assert_eq!(call_count.load(Ordering::SeqCst), 1);

        reply_ok(&mut new_srv).await;
        expect_alive(&client, true).await;
        reply_ok(&mut new_srv).await;
        let result = task.await.unwrap();
        assert!(result.is_ok(), "got: {:?}", result.err());
    }

    #[tokio::test]
    async fn reconnect_skips_pipe_that_fails_health_check() {
        let (client_end1, server1) = duplex(4096);
        let new_servers = Arc::new(Mutex::new(Vec::<DuplexStream>::new()));
        let ready = Arc::new(Notify::new());
        let call_count = Arc::new(AtomicUsize::new(0));
        let factory: ConnectionFactory = {
            let new_servers = Arc::clone(&new_servers);
            let ready = Arc::clone(&ready);
            let call_count = Arc::clone(&call_count);
            Arc::new(move || {
                let new_servers = Arc::clone(&new_servers);
                let ready = Arc::clone(&ready);
                let call_count = Arc::clone(&call_count);
                Box::pin(async move {
                    let (c, s) = duplex(4096);
                    new_servers.lock().await.push(s);
                    call_count.fetch_add(1, Ordering::SeqCst);
                    ready.notify_waiters();
                    Ok(Box::new(c) as PipeStream)
                })
            })
        };

        let client = Client::from_stream_with_factory(client_end1, factory).unwrap();
        drop(server1);
        expect_alive(&client, false).await;

        let task = tokio::spawn({
            let client = client.clone();
            async move { client.send_request("getInfo", None).await }
        });

        timeout(TEST_STATE_TIMEOUT, ready.notified())
            .await
            .expect("timed out waiting for first reconnect factory");
        let mut first = new_servers.lock().await.pop().unwrap();
        reply_error(&mut first, -32000, "stale pipe").await;

        wait_until("second reconnect factory", || async {
            call_count.load(Ordering::SeqCst) >= 2
        })
        .await;
        let mut second = new_servers.lock().await.pop().unwrap();
        reply_ok(&mut second).await;
        reply_ok(&mut second).await;

        let result = task.await.unwrap();
        assert!(result.is_ok(), "got: {:?}", result.err());
        assert_eq!(call_count.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn replacing_read_loop_aborts_old_connection_without_clearing_new_one() {
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
                    Ok(Box::new(c) as PipeStream)
                })
            })
        };

        let client = Client::from_stream_with_factory(client_end1, factory).unwrap();
        let reconnect_task = tokio::spawn({
            let client = client.clone();
            async move { client.force_reconnect().await }
        });
        timeout(TEST_STATE_TIMEOUT, ready.notified())
            .await
            .expect("timed out waiting for reconnect factory");
        let mut new_srv = new_server.lock().await.take().unwrap();
        reply_ok(&mut new_srv).await;
        reconnect_task.await.unwrap().unwrap();

        drop(server1);
        tokio::task::yield_now().await;
        assert_eq!(
            client.read_loop_exit_count_for_test(),
            0,
            "old read loop was aborted, not allowed to clean up the new epoch"
        );
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

#[cfg(test)]
mod attach_recovery_tests {
    use super::*;
    use crate::protocol::decode_frame;

    #[test]
    fn session_invalid_error_matches_extension_detached_text() {
        for message in [
            "Debugger unattached",
            "debugger is not attached",
            "Target closed",
        ] {
            let err = BridgeError::Rpc {
                method: "executeCdp".into(),
                message: message.into(),
            };
            assert!(is_session_invalid_error(&err), "{message}");
        }
        let benign = BridgeError::Rpc {
            method: "executeCdp".into(),
            message: "No dialog is showing".into(),
        };
        assert!(!is_session_invalid_error(&benign));
    }

    #[test]
    fn sticky_window_timeout_invalidates_the_attachment() {
        let err = BridgeError::Timeout("Runtime.evaluate".into());
        assert!(sticky_failure_invalidates(&err, Duration::from_secs(60)));
    }

    #[test]
    fn short_caller_timeout_keeps_the_attachment() {
        // Input cleanup uses sub-second deadlines; a timeout there is expected
        // and says nothing about the debugger's health.
        let err = BridgeError::Timeout("Input.dispatchKeyEvent".into());
        assert!(!sticky_failure_invalidates(
            &err,
            Duration::from_millis(250)
        ));
    }

    #[test]
    fn protocol_errors_never_invalidate_the_attachment() {
        let err = BridgeError::Rpc {
            method: "executeCdp".into(),
            message: "No dialog is showing".into(),
        };
        assert!(!sticky_failure_invalidates(&err, Duration::from_secs(60)));
    }

    #[tokio::test]
    async fn short_timeout_keeps_the_cached_attachment() {
        let (client_end, mut server) = tokio::io::duplex(4096);
        let client = Client::from_stream(client_end).unwrap();
        client.mark_attached(9).await;

        let request = tokio::spawn({
            let client = client.clone();
            async move {
                client
                    .execute_cdp_with_timeout(
                        9,
                        "Input.dispatchKeyEvent",
                        None,
                        Duration::from_millis(60),
                    )
                    .await
            }
        });

        let frame = timeout(Duration::from_secs(2), decode_frame(&mut server))
            .await
            .expect("request timeout")
            .unwrap();
        let request_frame: Value = serde_json::from_slice(&frame).unwrap();
        assert_eq!(request_frame["method"], "executeCdp");

        let err = request.await.unwrap().unwrap_err();
        assert!(matches!(err, BridgeError::Timeout(_)), "{err:?}");
        assert!(client.inner.attached_tabs.lock().await.contains_key(&9));
    }
}
