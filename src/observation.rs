//! Bounded, opt-in observation of one owned tab. Events are never attributed
//! from an untagged transport, and cursors cannot cross a reconnect or turn.
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};

use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::Notify;
use tokio::time::{timeout, Duration, Instant};
use uuid::Uuid;

use crate::error::{BridgeError, Result};

const MAX_TABS: usize = 16;
const MAX_EVENTS: usize = 512;
const MAX_BYTES: usize = 1024 * 1024;
const MAX_EVENT_BYTES: usize = 64 * 1024;
const MAX_FRAMES: usize = 256;
const MAX_STATE_TEXT: usize = 4 * 1024;
/// In-flight request IDs tracked per tab. A runaway page that never finishes
/// requests must not grow memory without limit; a full set simply never reads
/// as idle until the next main-frame commit clears it.
const MAX_INFLIGHT_REQUESTS: usize = 1024;
/// Quiet window after the last network state change before `networkidle`.
const NETWORK_IDLE_QUIET: Duration = Duration::from_millis(500);
const MAX_URL_GLOB_BYTES: usize = 512;
/// Same ceiling as the locator regex subset.
const MAX_URL_REGEX_BYTES: usize = 256;
const REGEX_SIZE_LIMIT: usize = 64 * 1024;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ObservationCursor {
    pub generation: String,
    pub sequence: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct ObservedEvent {
    pub sequence: u64,
    pub document_epoch: u64,
    pub event: Value,
}

#[derive(Clone, Debug, Serialize)]
pub struct EventBatch {
    pub events: Vec<ObservedEvent>,
    pub cursor: ObservationCursor,
    pub gap: bool,
    pub truncated: bool,
    pub dropped_event_count: u64,
    pub retained_event_count: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct FrameSnapshot {
    pub frame_id: String,
    pub parent_id: Option<String>,
    pub loader_id: Option<String>,
    pub url: String,
    pub document_epoch: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct DialogSnapshot {
    pub id: String,
    pub dialog_type: String,
    pub message: String,
    pub default_prompt: Option<String>,
    pub url: Option<String>,
    pub document_epoch: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct ChooserSnapshot {
    pub id: String,
    pub backend_node_id: i64,
    pub mode: String,
    pub frame_id: String,
    pub document_epoch: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct TabSnapshot {
    pub tab_id: i64,
    pub cursor: ObservationCursor,
    pub enabled: bool,
    pub document_epoch: u64,
    pub navigation_sequence: u64,
    pub main_frame_id: Option<String>,
    pub url: Option<String>,
    pub domcontentloaded: bool,
    pub load: bool,
    /// Same-target requests started but not yet finished.
    pub inflight_network_requests: usize,
    pub frames: Vec<FrameSnapshot>,
    pub frames_truncated: bool,
    pub dialog: Option<DialogSnapshot>,
    pub file_chooser: Option<ChooserSnapshot>,
    pub dropped_event_count: u64,
    pub retained_event_count: usize,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LoadState {
    Commit,
    DomContentLoaded,
    Load,
    /// `load` plus no in-flight top-document request and a quiet window since
    /// the last network state change.
    NetworkIdle,
}

/// All allocations have per-tab and global bounds. No receiver/task is created
/// per observation: the client's existing read loop records tagged events.
#[derive(Default)]
pub(crate) struct ObservationStore {
    tabs: HashMap<i64, TabState>,
}

struct TabState {
    generation: String,
    sequence: u64,
    enabled: bool,
    document_epoch: u64,
    navigation_sequence: u64,
    main_frame_id: Option<String>,
    frames: BTreeMap<String, FrameSnapshot>,
    frames_truncated: bool,
    domcontentloaded: bool,
    load: bool,
    dialog: Option<DialogSnapshot>,
    file_chooser: Option<ChooserSnapshot>,
    events: VecDeque<(ObservedEvent, usize)>,
    event_bytes: usize,
    dropped_count: u64,
    /// Highest lost sequence, including oversize events that were not retained.
    lost_through: u64,
    /// Request IDs in flight for the current target session, bounded by
    /// `MAX_INFLIGHT_REQUESTS` and cleared by a main-frame navigation commit.
    inflight_requests: HashSet<String>,
    inflight_overflowed: bool,
    /// Last moment the target's network state changed; the `networkidle`
    /// quiet window is measured from here.
    last_network_change: Instant,
    notify: Arc<Notify>,
}

impl TabState {
    fn new() -> Self {
        Self {
            generation: Uuid::new_v4().to_string(),
            sequence: 0,
            enabled: false,
            document_epoch: 1,
            navigation_sequence: 0,
            main_frame_id: None,
            frames: BTreeMap::new(),
            frames_truncated: false,
            domcontentloaded: false,
            load: false,
            dialog: None,
            file_chooser: None,
            events: VecDeque::new(),
            event_bytes: 0,
            dropped_count: 0,
            lost_through: 0,
            inflight_requests: HashSet::new(),
            inflight_overflowed: false,
            last_network_change: Instant::now(),
            notify: Arc::new(Notify::new()),
        }
    }

    fn cursor(&self) -> ObservationCursor {
        ObservationCursor {
            generation: self.generation.clone(),
            sequence: self.sequence,
        }
    }

    fn snapshot(&self, tab_id: i64) -> TabSnapshot {
        TabSnapshot {
            tab_id,
            cursor: self.cursor(),
            enabled: self.enabled,
            document_epoch: self.document_epoch,
            navigation_sequence: self.navigation_sequence,
            main_frame_id: self.main_frame_id.clone(),
            url: self
                .main_frame_id
                .as_ref()
                .and_then(|id| self.frames.get(id))
                .map(|f| f.url.clone()),
            domcontentloaded: self.domcontentloaded,
            load: self.load,
            inflight_network_requests: self.inflight_requests.len(),
            frames: self.frames.values().cloned().collect(),
            frames_truncated: self.frames_truncated,
            dialog: self.dialog.clone(),
            file_chooser: self.file_chooser.clone(),
            dropped_event_count: self.dropped_count,
            retained_event_count: self.events.len(),
        }
    }

    fn check_cursor(&self, cursor: &ObservationCursor) -> Result<()> {
        if cursor.generation != self.generation || cursor.sequence > self.sequence {
            return Err(BridgeError::User("stale or invalid observation cursor; start observation again after reconnect, tab retirement or turn cleanup".into()));
        }
        Ok(())
    }

    fn add_frame(&mut self, frame: &Value, navigation: bool) {
        let Some(id) = frame.get("id").and_then(Value::as_str) else {
            return;
        };
        if id.len() > MAX_STATE_TEXT {
            return;
        }
        let parent = bounded_string(frame.get("parentId"));
        let loader = bounded_string(frame.get("loaderId"));
        let is_main = parent.is_none();
        // A new top frame replaces its entire previous document registry. It
        // must still commit when the old document used every frame slot.
        if !is_main && !self.frames.contains_key(id) && self.frames.len() >= MAX_FRAMES {
            self.frames_truncated = true;
            return;
        }
        let old = self.frames.get(id);
        let changed = old.is_some_and(|f| f.loader_id != loader);
        let frame_epoch = old.map_or(1, |f| f.document_epoch + u64::from(navigation && changed));
        if navigation && self.file_chooser.as_ref().is_some_and(|c| c.frame_id == id) {
            self.file_chooser = None;
        }
        if is_main {
            if navigation {
                self.document_epoch += 1;
                self.navigation_sequence = self.sequence;
                self.domcontentloaded = false;
                self.load = false;
                self.dialog = None;
                self.file_chooser = None;
                // A main-document commit invalidates its previous descendants
                // and every request still counted for the previous document.
                self.frames.clear();
                self.frames_truncated = false;
                self.inflight_requests.clear();
                self.inflight_overflowed = false;
                self.last_network_change = Instant::now();
            }
            self.main_frame_id = Some(id.into());
        }
        self.frames.insert(
            id.into(),
            FrameSnapshot {
                frame_id: id.into(),
                parent_id: parent,
                loader_id: loader,
                url: bounded_string(frame.get("url")).unwrap_or_default(),
                document_epoch: if is_main {
                    self.document_epoch
                } else {
                    frame_epoch
                },
            },
        );
    }

    fn update(&mut self, method: &str, params: &Value) {
        match method {
            "Page.frameNavigated" => {
                if let Some(frame) = params.get("frame") {
                    self.add_frame(frame, true);
                }
            }
            "Page.navigatedWithinDocument" => {
                if let Some(id) = params.get("frameId").and_then(Value::as_str) {
                    if let Some(frame) = self.frames.get_mut(id) {
                        if let Some(url) = bounded_string(params.get("url")) {
                            frame.url = url;
                        }
                        if self.main_frame_id.as_deref() == Some(id) {
                            self.navigation_sequence = self.sequence;
                        }
                    }
                }
            }
            "Page.frameAttached" => {
                if let Some(id) = bounded_string(params.get("frameId")) {
                    let frame = serde_json::json!({"id": id, "parentId": params.get("parentFrameId"), "url":""});
                    if !self.frames.contains_key(&id) {
                        self.add_frame(&frame, false);
                    }
                }
            }
            "Page.frameDetached" => {
                if let Some(id) = params.get("frameId").and_then(Value::as_str) {
                    self.remove_frame(id);
                }
            }
            "Page.lifecycleEvent" => {
                let Some(id) = params.get("frameId").and_then(Value::as_str) else {
                    return;
                };
                if self.main_frame_id.as_deref() != Some(id) {
                    return;
                }
                let Some(frame) = self.frames.get(id) else {
                    return;
                };
                // Delayed events from the previous loader must not ready the new document.
                if params.get("loaderId").and_then(Value::as_str) != frame.loader_id.as_deref() {
                    return;
                }
                match params.get("name").and_then(Value::as_str) {
                    Some("DOMContentLoaded") => self.domcontentloaded = true,
                    Some("load") => {
                        self.domcontentloaded = true;
                        self.load = true;
                    }
                    _ => {}
                }
            }
            "Network.requestWillBeSent" => {
                if let Some(id) = bounded_string(params.get("requestId")) {
                    self.track_request(id);
                }
            }
            "Network.loadingFinished" | "Network.loadingFailed" => {
                if let Some(id) = params.get("requestId").and_then(Value::as_str) {
                    if self.inflight_requests.remove(id) {
                        self.last_network_change = Instant::now();
                    }
                }
            }
            "Page.javascriptDialogOpening" => {
                self.dialog = Some(DialogSnapshot {
                    id: Uuid::new_v4().to_string(),
                    dialog_type: bounded_string(params.get("type")).unwrap_or_default(),
                    message: bounded_string(params.get("message")).unwrap_or_default(),
                    default_prompt: bounded_string(params.get("defaultPrompt")),
                    url: bounded_string(params.get("url")),
                    document_epoch: self.document_epoch,
                });
            }
            "Page.javascriptDialogClosed" => self.dialog = None,
            "Page.fileChooserOpened" => {
                if let (Some(node), Some(frame)) = (
                    params.get("backendNodeId").and_then(Value::as_i64),
                    bounded_string(params.get("frameId")),
                ) {
                    if node > 0 {
                        self.file_chooser = Some(ChooserSnapshot {
                            id: Uuid::new_v4().to_string(),
                            backend_node_id: node,
                            mode: bounded_string(params.get("mode")).unwrap_or_default(),
                            frame_id: frame,
                            document_epoch: self.document_epoch,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    fn track_request(&mut self, id: String) {
        if self.inflight_requests.contains(&id) {
            self.last_network_change = Instant::now();
            return;
        }
        if self.inflight_requests.len() >= MAX_INFLIGHT_REQUESTS {
            self.inflight_overflowed = true;
            return;
        }
        self.inflight_requests.insert(id);
        self.last_network_change = Instant::now();
    }

    fn remove_frame(&mut self, id: &str) {
        let mut ids = vec![id.to_string()];
        let mut index = 0;
        while index < ids.len() {
            let children: Vec<String> = self
                .frames
                .values()
                .filter(|f| f.parent_id.as_deref() == Some(&ids[index]))
                .map(|f| f.frame_id.clone())
                .collect();
            for child in children {
                if !ids.contains(&child) {
                    ids.push(child);
                }
            }
            index += 1;
        }
        for id in ids {
            self.frames.remove(&id);
            if self.file_chooser.as_ref().is_some_and(|c| c.frame_id == id) {
                self.file_chooser = None;
            }
        }
    }
}

fn bounded_string(value: Option<&Value>) -> Option<String> {
    value.and_then(Value::as_str).map(|text| {
        let mut end = text.len().min(MAX_STATE_TEXT);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text[..end].into()
    })
}

impl ObservationStore {
    pub(crate) fn register(&mut self, tab_id: i64) -> Result<TabSnapshot> {
        if !self.tabs.contains_key(&tab_id) && self.tabs.len() >= MAX_TABS {
            return Err(BridgeError::User(format!(
                "observation tab limit ({MAX_TABS}) reached; stop an observation first"
            )));
        }
        Ok(self
            .tabs
            .entry(tab_id)
            .or_insert_with(TabState::new)
            .snapshot(tab_id))
    }

    pub(crate) fn snapshot(&self, tab_id: i64) -> Result<TabSnapshot> {
        Ok(self.tab(tab_id)?.snapshot(tab_id))
    }

    fn tab(&self, tab_id: i64) -> Result<&TabState> {
        self.tabs.get(&tab_id).ok_or_else(|| {
            BridgeError::User("observation is not active for this tab; start it first".into())
        })
    }

    pub(crate) fn record(&mut self, event: &Value) {
        let Some(tab_id) = event.pointer("/source/tabId").and_then(Value::as_i64) else {
            return;
        };
        let Some(tab) = self.tabs.get_mut(&tab_id) else {
            return;
        };
        let Some(method) = event.get("method").and_then(Value::as_str) else {
            return;
        };
        tab.sequence += 1;
        // Child debugger sessions (including OOPIFs) have their own root frame
        // and lifecycle. Retain raw events, but never mistake their root for
        // this tab's top document. Cross-target routing is a separate contract.
        if event
            .pointer("/source/sessionId")
            .is_none_or(Value::is_null)
        {
            tab.update(method, event.get("params").unwrap_or(&Value::Null));
        }
        let observed = ObservedEvent {
            sequence: tab.sequence,
            document_epoch: tab.document_epoch,
            event: event.clone(),
        };
        let bytes = serde_json::to_vec(&observed).map_or(usize::MAX, |encoded| encoded.len());
        if bytes > MAX_EVENT_BYTES {
            tab.dropped_count += 1;
            tab.lost_through = tab.sequence;
        } else {
            while tab.events.len() >= MAX_EVENTS
                || tab.event_bytes.saturating_add(bytes) > MAX_BYTES
            {
                let Some((old, size)) = tab.events.pop_front() else {
                    break;
                };
                tab.event_bytes -= size;
                tab.dropped_count += 1;
                tab.lost_through = tab.lost_through.max(old.sequence);
            }
            tab.events.push_back((observed, bytes));
            tab.event_bytes += bytes;
        }
        tab.notify.notify_waiters();
    }

    pub(crate) fn seed_frame_tree(&mut self, tab_id: i64, tree: &Value) {
        let Some(tab) = self.tabs.get_mut(&tab_id) else {
            return;
        };
        // Events received during getFrameTree are newer than its response.
        // Never overwrite that observed navigation with a bootstrap snapshot.
        if tab.main_frame_id.is_some() {
            return;
        }
        let mut pending = vec![tree];
        while let Some(node) = pending.pop() {
            if let Some(frame) = node.get("frame") {
                tab.add_frame(frame, false);
            }
            if let Some(children) = node.get("childFrames").and_then(Value::as_array) {
                pending.extend(children.iter().take(MAX_FRAMES));
            }
            if tab.frames.len() >= MAX_FRAMES {
                tab.frames_truncated |= !pending.is_empty();
                break;
            }
        }
    }

    pub(crate) fn finish_start(
        &mut self,
        tab_id: i64,
        cursor: &ObservationCursor,
        ready_state: Option<&str>,
    ) -> Result<TabSnapshot> {
        let tab = self
            .tabs
            .get_mut(&tab_id)
            .ok_or_else(|| BridgeError::User("observation invalidated while starting".into()))?;
        if cursor.generation != tab.generation {
            return Err(BridgeError::User(
                "observation invalidated while starting".into(),
            ));
        }
        if tab.navigation_sequence <= cursor.sequence {
            tab.domcontentloaded |= matches!(ready_state, Some("interactive" | "complete"));
            tab.load |= ready_state == Some("complete");
        }
        tab.enabled = true;
        Ok(tab.snapshot(tab_id))
    }

    pub(crate) fn remove(&mut self, tab_id: i64) {
        if let Some(tab) = self.tabs.remove(&tab_id) {
            tab.notify.notify_waiters();
        }
    }

    pub(crate) fn clear(&mut self) {
        for tab in self.tabs.values() {
            tab.notify.notify_waiters();
        }
        self.tabs.clear();
    }

    pub(crate) fn read(
        &self,
        tab_id: i64,
        cursor: Option<&ObservationCursor>,
        methods: &[&str],
        limit: usize,
        max_bytes: usize,
    ) -> Result<EventBatch> {
        if limit == 0 || limit > MAX_EVENTS || max_bytes == 0 || max_bytes > MAX_BYTES {
            return Err(BridgeError::User(format!(
                "event limit must be 1..{MAX_EVENTS} and max_bytes 1..{MAX_BYTES}"
            )));
        }
        let tab = self.tab(tab_id)?;
        if let Some(cursor) = cursor {
            tab.check_cursor(cursor)?;
        }
        let after = cursor.map_or(0, |c| c.sequence);
        let mut through = after;
        let mut events = Vec::new();
        let mut bytes: usize = 0;
        let mut truncated = false;
        for (event, size) in &tab.events {
            if event.sequence <= after {
                continue;
            }
            let method = event
                .event
                .get("method")
                .and_then(Value::as_str)
                .unwrap_or_default();
            if !methods.is_empty() && !methods.contains(&method) {
                through = event.sequence;
                continue;
            }
            if events.len() >= limit || bytes.saturating_add(*size) > max_bytes {
                truncated = true;
                break;
            }
            through = event.sequence;
            events.push(event.clone());
            bytes += size;
        }
        // Only advance past a lost tail if all retained entries were scanned.
        if !truncated {
            through = tab.sequence;
        }
        Ok(EventBatch {
            events,
            cursor: ObservationCursor {
                generation: tab.generation.clone(),
                sequence: through,
            },
            gap: after < tab.lost_through,
            truncated,
            dropped_event_count: tab.dropped_count,
            retained_event_count: tab.events.len(),
        })
    }

    pub(crate) fn dialog(&self, tab_id: i64, id: &str) -> Result<DialogSnapshot> {
        self.tab(tab_id)?
            .dialog
            .as_ref()
            .filter(|d| d.id == id)
            .cloned()
            .ok_or_else(|| BridgeError::User("stale dialog id or no active observed dialog".into()))
    }

    pub(crate) fn close_dialog(&mut self, tab_id: i64, id: &str) {
        if let Some(tab) = self.tabs.get_mut(&tab_id) {
            if tab.dialog.as_ref().is_some_and(|d| d.id == id) {
                tab.dialog = None;
            }
        }
    }

    pub(crate) fn chooser(&self, tab_id: i64, id: &str) -> Result<ChooserSnapshot> {
        self.tab(tab_id)?
            .file_chooser
            .as_ref()
            .filter(|c| c.id == id)
            .cloned()
            .ok_or_else(|| {
                BridgeError::User("stale file chooser id or no active observed chooser".into())
            })
    }

    pub(crate) fn close_chooser(&mut self, tab_id: i64, id: &str) {
        if let Some(tab) = self.tabs.get_mut(&tab_id) {
            if tab.file_chooser.as_ref().is_some_and(|c| c.id == id) {
                tab.file_chooser = None;
            }
        }
    }

    pub(crate) fn clear_chooser(&mut self, tab_id: i64) {
        if let Some(tab) = self.tabs.get_mut(&tab_id) {
            tab.file_chooser = None;
        }
    }

    fn notifier(&self, tab_id: i64) -> Result<Arc<Notify>> {
        Ok(self.tab(tab_id)?.notify.clone())
    }
}

/// URL predicate for `wait_navigation` and `wait_url`. `Exact` keeps the
/// original contract; `Glob` and `Regex` are opt-in and validated up front so
/// an untrusted pattern never reaches an unrestricted matcher.
#[derive(Clone, Debug)]
pub enum UrlMatcher {
    Exact(String),
    Substring(String),
    /// Compiled glob (`^...$` regex), validated at parse time.
    Glob(Regex),
    Regex(Regex),
}

impl UrlMatcher {
    /// Build a matcher from at most one of `url`, `url_glob` and `url_regex`.
    /// Returns `None` when none is supplied.
    pub fn parse(
        url: Option<&str>,
        url_glob: Option<&str>,
        url_regex: Option<&Value>,
    ) -> Result<Option<Self>> {
        let provided = [url.is_some(), url_glob.is_some(), url_regex.is_some()]
            .into_iter()
            .filter(|present| *present)
            .count();
        if provided > 1 {
            return Err(BridgeError::User(
                "url matching accepts at most one of url, url_glob and url_regex".into(),
            ));
        }
        if let Some(url) = url {
            return Ok(Some(Self::Exact(url.to_string())));
        }
        if let Some(glob) = url_glob {
            if glob.len() > MAX_URL_GLOB_BYTES {
                return Err(BridgeError::User(format!(
                    "url_glob supports at most {MAX_URL_GLOB_BYTES} bytes"
                )));
            }
            let pattern = glob_to_regex(glob);
            let compiled = RegexBuilder::new(&pattern)
                .size_limit(REGEX_SIZE_LIMIT)
                .build()
                .map_err(|_| BridgeError::User("url_glob is not a usable pattern".into()))?;
            return Ok(Some(Self::Glob(compiled)));
        }
        let Some(spec) = url_regex else {
            return Ok(None);
        };
        Ok(Some(Self::Regex(compile_url_regex(spec)?)))
    }

    pub fn matches(&self, candidate: &str) -> bool {
        match self {
            Self::Exact(url) => candidate == url,
            Self::Substring(needle) => candidate.contains(needle.as_str()),
            Self::Glob(regex) | Self::Regex(regex) => regex.is_match(candidate),
        }
    }
}

/// Translate a URL glob into an anchored regex source: `**` spans segments,
/// `*` stays inside one segment, `?` is one non-separator character.
fn glob_to_regex(glob: &str) -> String {
    let mut pattern = String::with_capacity(glob.len() + 8);
    pattern.push('^');
    let mut chars = glob.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' if chars.peek() == Some(&'*') => {
                chars.next();
                pattern.push_str(".*");
            }
            '*' => pattern.push_str("[^/]*"),
            '?' => pattern.push_str("[^/]"),
            c => {
                if "\\.+()[]{}^$|".contains(c) {
                    pattern.push('\\');
                }
                pattern.push(c);
            }
        }
    }
    pattern.push('$');
    pattern
}

fn url_regex_error(message: impl Into<String>) -> BridgeError {
    BridgeError::User(message.into())
}

/// Accepts a bare pattern or `{"regex": "...", "flags": "..."}` and compiles it
/// with the same restricted subset as the locator.
fn compile_url_regex(spec: &Value) -> Result<Regex> {
    let (pattern, flags) = match spec {
        Value::String(pattern) => (pattern.as_str(), ""),
        Value::Object(object) => {
            if object.keys().any(|key| key != "regex" && key != "flags") {
                return Err(url_regex_error("url_regex accepts only regex and flags"));
            }
            let pattern = object
                .get("regex")
                .and_then(Value::as_str)
                .ok_or_else(|| url_regex_error("url_regex must contain a regex string"))?;
            let flags = match object.get("flags") {
                Some(value) => value
                    .as_str()
                    .ok_or_else(|| url_regex_error("url_regex flags must be a string"))?,
                None => "",
            };
            (pattern, flags)
        }
        _ => {
            return Err(url_regex_error(
                "url_regex must be a pattern string or {regex, flags} object",
            ))
        }
    };
    validate_safe_regex(pattern, flags)?;
    let mut builder = RegexBuilder::new(pattern);
    builder.size_limit(REGEX_SIZE_LIMIT);
    for flag in flags.chars() {
        match flag {
            'i' => {
                builder.case_insensitive(true);
            }
            'm' => {
                builder.multi_line(true);
            }
            's' => {
                builder.dot_matches_new_line(true);
            }
            'u' => {
                builder.unicode(true);
            }
            _ => {}
        }
    }
    builder
        .build()
        .map_err(|err| url_regex_error(format!("url_regex does not compile: {err}")))
}

/// Small explicitly documented subset: no grouping, alternation, backreferences
/// or counted repetition, one quantifier at most, unique `imsu` flags.
fn validate_safe_regex(pattern: &str, flags: &str) -> Result<()> {
    let mut escaped = false;
    let mut quantifiers = 0;
    for c in pattern.chars() {
        if escaped {
            if c.is_ascii_digit() && c != '0' {
                return Err(url_regex_error("regex backreferences are unsupported"));
            }
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if "()|{}".contains(c) {
            return Err(url_regex_error(
                "regex grouping, alternation and counted repetition are unsupported",
            ));
        } else if "*+?".contains(c) {
            quantifiers += 1;
        }
    }
    if pattern.len() > MAX_URL_REGEX_BYTES
        || quantifiers > 1
        || flags.chars().any(|c| !"imsu".contains(c))
        || flags
            .chars()
            .enumerate()
            .any(|(i, c)| flags[..i].contains(c))
    {
        return Err(url_regex_error(format!(
            "regex supports at most {MAX_URL_REGEX_BYTES} bytes, one quantifier and unique imsu flags"
        )));
    }
    Ok(())
}

/// Collapse CDP/console level spellings onto the four levels log filters use.
/// Unknown or missing levels count as `info`.
pub fn normalize_log_level(raw: Option<&str>) -> &'static str {
    let Some(raw) = raw else {
        return "info";
    };
    if raw.eq_ignore_ascii_case("warning") || raw.eq_ignore_ascii_case("warn") {
        "warning"
    } else if raw.eq_ignore_ascii_case("error") || raw.eq_ignore_ascii_case("assert") {
        "error"
    } else if raw.eq_ignore_ascii_case("debug")
        || raw.eq_ignore_ascii_case("verbose")
        || raw.eq_ignore_ascii_case("trace")
    {
        "debug"
    } else {
        "info"
    }
}

/// An empty filter admits every entry. Otherwise a level is admitted when the
/// raw or normalized spelling matches on either side, so `warn` and `warning`
/// are interchangeable.
pub fn log_level_matches(raw_level: Option<&str>, allowed_levels: &[String]) -> bool {
    if allowed_levels.is_empty() {
        return true;
    }
    let normalized = normalize_log_level(raw_level);
    allowed_levels.iter().any(|allowed| {
        raw_level.is_some_and(|raw| allowed.eq_ignore_ascii_case(raw))
            || allowed.eq_ignore_ascii_case(normalized)
            || normalize_log_level(Some(allowed)) == normalized
    })
}

/// Wait for a main-frame navigation observed after the supplied arm cursor.
/// `expected_url` is matched exactly; use [`wait_for_navigation_with_matcher`]
/// for glob, regex or substring predicates.
pub(crate) async fn wait_for_navigation(
    store: &Mutex<ObservationStore>,
    tab_id: i64,
    cursor: &ObservationCursor,
    expected_url: Option<&str>,
    wait_until: LoadState,
    wait_timeout: Duration,
) -> Result<TabSnapshot> {
    let matcher = expected_url.map(|url| UrlMatcher::Exact(url.to_string()));
    wait_for_navigation_with_matcher(
        store,
        tab_id,
        cursor,
        matcher.as_ref(),
        wait_until,
        wait_timeout,
    )
    .await
}

/// Wait for a main-frame navigation after `cursor` whose URL matches `matcher`
/// and whose readiness reaches `wait_until`.
///
/// `NetworkIdle` also requires the `load` milestone, an empty top-document
/// request set and a quiet window since the last network change. No CDP event
/// marks the end of that window, so the wait races the notifier against the
/// remaining quiet time instead of blocking on it.
pub(crate) async fn wait_for_navigation_with_matcher(
    store: &Mutex<ObservationStore>,
    tab_id: i64,
    cursor: &ObservationCursor,
    matcher: Option<&UrlMatcher>,
    wait_until: LoadState,
    wait_timeout: Duration,
) -> Result<TabSnapshot> {
    let deadline = Instant::now() + wait_timeout;
    loop {
        let notify = store.lock().unwrap().notifier(tab_id)?;
        let notified = notify.notified();
        tokio::pin!(notified);
        // Register before checking state, avoiding event-before-wait races.
        notified.as_mut().enable();
        let mut quiet_wait = None;
        {
            let guard = store.lock().unwrap();
            let tab = guard.tab(tab_id)?;
            tab.check_cursor(cursor)?;
            let snapshot = tab.snapshot(tab_id);
            if snapshot.navigation_sequence > cursor.sequence {
                if snapshot
                    .url
                    .as_deref()
                    .is_some_and(|url| url.starts_with("chrome-error://"))
                {
                    return Err(BridgeError::User(
                        "navigation reached a browser error document".into(),
                    ));
                }
                let ready = match wait_until {
                    LoadState::Commit => true,
                    LoadState::DomContentLoaded => snapshot.domcontentloaded,
                    LoadState::Load => snapshot.load,
                    LoadState::NetworkIdle => {
                        snapshot.load
                            && !tab.inflight_overflowed
                            && tab.inflight_requests.is_empty()
                    }
                };
                let matched = matcher.is_none_or(|matcher| {
                    snapshot
                        .url
                        .as_deref()
                        .is_some_and(|url| matcher.matches(url))
                });
                if ready && matched {
                    if wait_until == LoadState::NetworkIdle {
                        let quiet =
                            Instant::now().saturating_duration_since(tab.last_network_change);
                        if quiet >= NETWORK_IDLE_QUIET {
                            return Ok(snapshot);
                        }
                        quiet_wait = Some(NETWORK_IDLE_QUIET - quiet);
                    } else {
                        return Ok(snapshot);
                    }
                }
            }
        }
        match quiet_wait {
            Some(remaining) => {
                let remaining = remaining.min(deadline.saturating_duration_since(Instant::now()));
                if remaining.is_zero() {
                    return Err(BridgeError::Timeout("observed navigation".into()));
                }
                // Any later network event fires the notifier and restarts the
                // quiet window; otherwise wake when it elapses.
                tokio::select! {
                    _ = notified => {}
                    _ = tokio::time::sleep(remaining) => {}
                }
            }
            None => {
                timeout(deadline.saturating_duration_since(Instant::now()), notified)
                    .await
                    .map_err(|_| BridgeError::Timeout("observed navigation".into()))?;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn event(tab: i64, method: &str, params: Value) -> Value {
        json!({"source":{"tabId":tab}, "method":method, "params":params})
    }

    fn navigate(store: &mut ObservationStore, loader: &str) {
        store.record(&event(
            1,
            "Page.frameNavigated",
            json!({"frame":{"id":"main", "loaderId":loader, "url":"https://example.com/"}}),
        ));
    }

    #[test]
    fn journal_is_tab_scoped_before_budgets_and_ignores_untagged() {
        let mut store = ObservationStore::default();
        store.register(1).unwrap();
        store.record(&event(
            2,
            "Runtime.consoleAPICalled",
            json!({"args":["x".repeat(MAX_EVENT_BYTES)]}),
        ));
        store.record(&json!({"method":"Runtime.consoleAPICalled", "params":{}}));
        store.record(&event(
            1,
            "Runtime.consoleAPICalled",
            json!({"args":[{"value":"kept"}]}),
        ));
        let result = store.read(1, None, &[], MAX_EVENTS, MAX_BYTES).unwrap();
        assert_eq!(result.events.len(), 1);
        assert_eq!(result.cursor.sequence, 1);
        assert_eq!(result.dropped_event_count, 0);
        assert!(!result.gap);
    }

    #[test]
    fn journal_retains_bounded_tail_and_reports_oversize_gap() {
        let mut store = ObservationStore::default();
        let arm = store.register(1).unwrap().cursor;
        for index in 0..MAX_EVENTS + 3 {
            store.record(&event(
                1,
                "Log.entryAdded",
                json!({"entry":{"text":index.to_string()}}),
            ));
        }
        let result = store.read(1, Some(&arm), &[], 2, MAX_BYTES).unwrap();
        assert_eq!(result.events.len(), 2);
        assert_eq!(result.events[0].sequence, 4);
        assert!(result.gap && result.truncated);
        assert_eq!(result.dropped_event_count, 3);
        assert_eq!(result.retained_event_count, MAX_EVENTS);
        store.record(&event(
            1,
            "Log.entryAdded",
            json!({"entry":{"text":"x".repeat(MAX_EVENT_BYTES)}}),
        ));
        let latest = store.snapshot(1).unwrap().cursor;
        let before_lost = ObservationCursor {
            sequence: latest.sequence - 1,
            ..latest.clone()
        };
        let tail = store
            .read(1, Some(&before_lost), &[], MAX_EVENTS, MAX_BYTES)
            .unwrap();
        assert!(tail.gap);
        assert_eq!(tail.cursor, latest);
        assert!(tail.events.is_empty());
    }

    #[test]
    fn filtered_pagination_does_not_skip_an_unreturned_match() {
        let mut store = ObservationStore::default();
        store.register(1).unwrap();
        for method in [
            "Log.entryAdded",
            "Page.frameStoppedLoading",
            "Log.entryAdded",
        ] {
            store.record(&event(1, method, json!({})));
        }
        let first = store
            .read(1, None, &["Log.entryAdded"], 1, MAX_BYTES)
            .unwrap();
        assert!(first.truncated);
        assert_eq!(first.cursor.sequence, 2);
        let second = store
            .read(1, Some(&first.cursor), &["Log.entryAdded"], 1, MAX_BYTES)
            .unwrap();
        assert_eq!(second.events[0].sequence, 3);
        assert!(!second.truncated);
        assert!(store.read(1, None, &[], 0, MAX_BYTES).is_err());
        assert!(store.read(1, None, &[], 1, 1).unwrap().truncated);
    }

    #[test]
    fn retirement_and_global_capacity_are_explicit() {
        let mut store = ObservationStore::default();
        let old = store.register(1).unwrap().cursor;
        for tab in 2..=MAX_TABS as i64 {
            store.register(tab).unwrap();
        }
        assert!(store.register(17).is_err());
        store.remove(1);
        store.register(1).unwrap();
        assert!(store.read(1, Some(&old), &[], 1, MAX_BYTES).is_err());
        store.clear();
        assert!(store.snapshot(1).is_err());
    }

    #[test]
    fn epochs_loader_readiness_and_same_document_navigation_are_separate() {
        let mut store = ObservationStore::default();
        store.register(1).unwrap();
        navigate(&mut store, "a");
        let a = store.snapshot(1).unwrap();
        store.record(&event(
            1,
            "Page.lifecycleEvent",
            json!({"frameId":"main", "loaderId":"a", "name":"load"}),
        ));
        assert!(store.snapshot(1).unwrap().load);
        navigate(&mut store, "b");
        store.record(&event(
            1,
            "Page.lifecycleEvent",
            json!({"frameId":"main", "loaderId":"a", "name":"load"}),
        ));
        let b = store.snapshot(1).unwrap();
        assert_eq!(b.document_epoch, a.document_epoch + 1);
        assert!(!b.load);
        store.record(&event(
            1,
            "Page.navigatedWithinDocument",
            json!({"frameId":"main", "url":"https://example.com/#new"}),
        ));
        let same = store.snapshot(1).unwrap();
        assert_eq!(same.document_epoch, b.document_epoch);
        assert!(same.navigation_sequence > b.navigation_sequence);
        assert_eq!(same.url.as_deref(), Some("https://example.com/#new"));
    }

    #[test]
    fn dialog_and_chooser_ids_cannot_clear_new_handles() {
        let mut store = ObservationStore::default();
        store.register(1).unwrap();
        navigate(&mut store, "a");
        let dialog = event(
            1,
            "Page.javascriptDialogOpening",
            json!({"type":"prompt", "message":"message", "defaultPrompt":"default"}),
        );
        store.record(&dialog);
        let old = store.snapshot(1).unwrap().dialog.unwrap();
        store.record(&dialog);
        let current = store.snapshot(1).unwrap().dialog.unwrap();
        store.close_dialog(1, &old.id);
        assert!(store.dialog(1, &old.id).is_err());
        assert!(store.dialog(1, &current.id).is_ok());
        store.record(&event(
            1,
            "Page.fileChooserOpened",
            json!({"backendNodeId":42, "mode":"selectMultiple", "frameId":"main"}),
        ));
        let chooser = store.snapshot(1).unwrap().file_chooser.unwrap();
        assert_eq!(store.chooser(1, &chooser.id).unwrap().backend_node_id, 42);
        navigate(&mut store, "b");
        assert!(store.dialog(1, &current.id).is_err());
        assert!(store.chooser(1, &chooser.id).is_err());
    }

    #[test]
    fn frame_registry_is_bounded_and_removes_descendants() {
        let mut store = ObservationStore::default();
        store.register(1).unwrap();
        navigate(&mut store, "a");
        for index in 1..=MAX_FRAMES {
            store.record(&event(
                1,
                "Page.frameAttached",
                json!({"frameId":format!("child{index}"), "parentFrameId":"main"}),
            ));
        }
        let snapshot = store.snapshot(1).unwrap();
        assert_eq!(snapshot.frames.len(), MAX_FRAMES);
        assert!(snapshot.frames_truncated);
        store.record(&event(1, "Page.frameDetached", json!({"frameId":"main"})));
        assert!(store.snapshot(1).unwrap().frames.is_empty());
    }

    #[test]
    fn child_session_events_do_not_replace_main_document() {
        let mut store = ObservationStore::default();
        store.register(1).unwrap();
        navigate(&mut store, "main-loader");
        let before = store.snapshot(1).unwrap();
        let mut child = event(
            1,
            "Page.frameNavigated",
            json!({"frame":{"id":"oopif", "loaderId":"child-loader", "url":"https://example.com/child"}}),
        );
        child["source"]["sessionId"] = json!("child-session");
        store.record(&child);
        let after = store.snapshot(1).unwrap();
        assert_eq!(after.main_frame_id, before.main_frame_id);
        assert_eq!(after.document_epoch, before.document_epoch);
        assert_eq!(after.retained_event_count, before.retained_event_count + 1);
    }

    #[test]
    fn new_main_frame_commits_even_when_previous_frame_registry_is_full() {
        let mut store = ObservationStore::default();
        store.register(1).unwrap();
        navigate(&mut store, "old-loader");
        for index in 1..=MAX_FRAMES {
            store.record(&event(
                1,
                "Page.frameAttached",
                json!({"frameId":format!("old-child{index}"),"parentFrameId":"main"}),
            ));
        }
        store.record(&event(
            1,
            "Page.lifecycleEvent",
            json!({"frameId":"main","loaderId":"old-loader","name":"load"}),
        ));
        store.record(&event(
            1,
            "Page.javascriptDialogOpening",
            json!({"type":"confirm","message":"old"}),
        ));
        let before = store.snapshot(1).unwrap();
        assert_eq!(before.frames.len(), MAX_FRAMES);
        assert!(before.frames_truncated && before.load && before.dialog.is_some());
        store.record(&event(1, "Page.frameNavigated", json!({"frame":{"id":"replacement-main","loaderId":"new-loader","url":"https://example.com/new"}})));
        let after = store.snapshot(1).unwrap();
        assert_eq!(after.document_epoch, before.document_epoch + 1);
        assert_eq!(after.main_frame_id.as_deref(), Some("replacement-main"));
        assert_eq!(after.url.as_deref(), Some("https://example.com/new"));
        assert_eq!(after.frames.len(), 1);
        assert!(!after.frames_truncated && !after.load && after.dialog.is_none());
    }

    #[test]
    fn same_document_navigation_keeps_loader_readiness() {
        let mut store = ObservationStore::default();
        store.register(1).unwrap();
        navigate(&mut store, "same-loader");
        store.record(&event(
            1,
            "Page.lifecycleEvent",
            json!({"frameId":"main","loaderId":"same-loader","name":"load"}),
        ));
        let before = store.snapshot(1).unwrap();
        store.record(&event(
            1,
            "Page.navigatedWithinDocument",
            json!({"frameId":"main","url":"https://example.com/#hash"}),
        ));
        let after = store.snapshot(1).unwrap();
        assert_eq!(after.document_epoch, before.document_epoch);
        assert!(after.load && after.domcontentloaded);
        assert!(after.navigation_sequence > before.navigation_sequence);
    }

    #[tokio::test]
    async fn wait_captures_event_before_call_and_checks_load_or_timeout() {
        let store = Mutex::new(ObservationStore::default());
        let arm = store.lock().unwrap().register(1).unwrap().cursor;
        navigate(&mut store.lock().unwrap(), "a");
        let committed = wait_for_navigation(
            &store,
            1,
            &arm,
            Some("https://example.com/"),
            LoadState::Commit,
            Duration::from_millis(20),
        )
        .await
        .unwrap();
        assert!(committed.navigation_sequence > arm.sequence);
        let err = wait_for_navigation(
            &store,
            1,
            &arm,
            None,
            LoadState::Load,
            Duration::from_millis(20),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, BridgeError::Timeout(_)));
        store.lock().unwrap().record(&event(
            1,
            "Page.lifecycleEvent",
            json!({"frameId":"main", "loaderId":"a", "name":"load"}),
        ));
        assert!(
            wait_for_navigation(
                &store,
                1,
                &arm,
                None,
                LoadState::Load,
                Duration::from_millis(20)
            )
            .await
            .unwrap()
            .load
        );
    }

    #[tokio::test]
    async fn waiting_wakes_on_retirement_and_rejects_old_cursor() {
        let store = Arc::new(Mutex::new(ObservationStore::default()));
        let arm = store.lock().unwrap().register(1).unwrap().cursor;
        let task = tokio::spawn({
            let store = store.clone();
            async move {
                wait_for_navigation(
                    &store,
                    1,
                    &arm,
                    None,
                    LoadState::Commit,
                    Duration::from_secs(1),
                )
                .await
            }
        });
        tokio::task::yield_now().await;
        store.lock().unwrap().remove(1);
        assert!(timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap()
            .is_err());
    }

    #[test]
    fn url_matchers_accept_exact_glob_and_safe_regex() {
        let exact = UrlMatcher::parse(Some("https://example.com/a"), None, None)
            .unwrap()
            .unwrap();
        assert!(exact.matches("https://example.com/a"));
        assert!(!exact.matches("https://example.com/a#fragment"));
        assert!(UrlMatcher::Substring("example.com/a".into()).matches("https://example.com/a"));

        let segment = UrlMatcher::parse(None, Some("https://example.com/*"), None)
            .unwrap()
            .unwrap();
        assert!(segment.matches("https://example.com/page"));
        assert!(!segment.matches("https://example.com/a/page"));

        let any_depth = UrlMatcher::parse(None, Some("https://example.com/**"), None)
            .unwrap()
            .unwrap();
        assert!(any_depth.matches("https://example.com/a/b/c"));
        assert!(any_depth.matches("https://example.com/"));

        let single = UrlMatcher::parse(None, Some("https://example.com/article-?*"), None)
            .unwrap()
            .unwrap();
        assert!(single.matches("https://example.com/article-7x"));
        assert!(!single.matches("https://example.com/article-"));
        assert!(!single.matches("https://example.com/article-7/x"));

        let regex = UrlMatcher::parse(
            None,
            None,
            Some(&json!({"regex":"^https://example\\.com/[a-z]+$","flags":"i"})),
        )
        .unwrap()
        .unwrap();
        assert!(regex.matches("https://EXAMPLE.com/Path"));
        assert!(!regex.matches("https://example.com/Path/2"));

        let plain = UrlMatcher::parse(None, None, Some(&json!("^https://example\\.com/")))
            .unwrap()
            .unwrap();
        assert!(plain.matches("https://example.com/x"));
        assert!(UrlMatcher::parse(None, None, None).unwrap().is_none());
    }

    #[test]
    fn url_matchers_reject_ambiguous_or_unsafe_patterns() {
        assert!(UrlMatcher::parse(Some("a"), Some("b"), None).is_err());
        assert!(UrlMatcher::parse(Some("a"), None, Some(&json!("b"))).is_err());
        assert!(UrlMatcher::parse(None, Some("a"), Some(&json!("b"))).is_err());
        for pattern in [
            json!({"regex":"(a+)+$"}),
            json!({"regex":"a|b"}),
            json!({"regex":"a.*.*b"}),
            json!({"regex":"\\1"}),
            json!({"regex":"ok","flags":"ii"}),
            json!({"regex":"ok","flags":"x"}),
            json!({"regex":"ok","extra":1}),
            json!({"flags":"i"}),
            json!({"regex":"a".repeat(MAX_URL_REGEX_BYTES + 1)}),
        ] {
            assert!(
                UrlMatcher::parse(None, None, Some(&pattern)).is_err(),
                "{pattern}"
            );
        }
        assert!(UrlMatcher::parse(None, Some(&"*".repeat(MAX_URL_GLOB_BYTES + 1)), None).is_err());
    }

    #[test]
    fn log_levels_normalize_and_filter_with_aliases() {
        for (raw, expected) in [
            (Some("warning"), "warning"),
            (Some("warn"), "warning"),
            (Some("ERROR"), "error"),
            (Some("assert"), "error"),
            (Some("verbose"), "debug"),
            (Some("trace"), "debug"),
            (Some("debug"), "debug"),
            (Some("log"), "info"),
            (Some("dir"), "info"),
            (Some("table"), "info"),
            (Some("info"), "info"),
            (Some("notice"), "info"),
            (None, "info"),
        ] {
            assert_eq!(normalize_log_level(raw), expected, "{raw:?}");
        }

        fn levels(raw: &[&str]) -> Vec<String> {
            raw.iter().map(|level| level.to_string()).collect()
        }
        assert!(log_level_matches(Some("error"), &[]));
        assert!(log_level_matches(Some("warning"), &levels(&["warn"])));
        assert!(log_level_matches(Some("warn"), &levels(&["warning"])));
        assert!(log_level_matches(Some("log"), &levels(&["info"])));
        assert!(log_level_matches(Some("DEBUG"), &levels(&["debug"])));
        assert!(log_level_matches(None, &levels(&["info"])));
        assert!(!log_level_matches(
            Some("info"),
            &levels(&["warning", "error"])
        ));
        assert!(!log_level_matches(Some("debug"), &levels(&["info"])));
    }

    #[test]
    fn network_requests_are_bounded_and_cleared_by_navigation() {
        let mut store = ObservationStore::default();
        store.register(1).unwrap();
        navigate(&mut store, "a");
        store.record(&event(
            1,
            "Network.requestWillBeSent",
            json!({"requestId":"r1"}),
        ));
        store.record(&event(
            1,
            "Network.requestWillBeSent",
            json!({"requestId":"r2"}),
        ));
        assert_eq!(store.snapshot(1).unwrap().inflight_network_requests, 2);
        store.record(&event(
            1,
            "Network.loadingFinished",
            json!({"requestId":"r1"}),
        ));
        assert_eq!(store.snapshot(1).unwrap().inflight_network_requests, 1);
        store.record(&event(
            1,
            "Network.loadingFailed",
            json!({"requestId":"r2"}),
        ));
        assert_eq!(store.snapshot(1).unwrap().inflight_network_requests, 0);
        for index in 0..MAX_INFLIGHT_REQUESTS + 16 {
            store.record(&event(
                1,
                "Network.requestWillBeSent",
                json!({"requestId":format!("req{index}")}),
            ));
        }
        assert_eq!(
            store.snapshot(1).unwrap().inflight_network_requests,
            MAX_INFLIGHT_REQUESTS
        );
        navigate(&mut store, "b");
        assert_eq!(store.snapshot(1).unwrap().inflight_network_requests, 0);
    }

    #[tokio::test]
    async fn network_idle_requires_load_and_waits_out_the_quiet_window() {
        let store = Mutex::new(ObservationStore::default());
        let arm = store.lock().unwrap().register(1).unwrap().cursor;
        navigate(&mut store.lock().unwrap(), "a");
        store.lock().unwrap().record(&event(
            1,
            "Page.lifecycleEvent",
            json!({"frameId":"main", "loaderId":"a", "name":"load"}),
        ));
        let started = Instant::now();
        let snapshot = wait_for_navigation_with_matcher(
            &store,
            1,
            &arm,
            None,
            LoadState::NetworkIdle,
            Duration::from_secs(5),
        )
        .await
        .unwrap();
        assert!(snapshot.load);
        assert_eq!(snapshot.inflight_network_requests, 0);
        // Readiness is reached immediately, but the quiet window still has to
        // pass before the wait may return.
        assert!(started.elapsed() >= NETWORK_IDLE_QUIET);
    }

    #[tokio::test]
    async fn network_idle_waits_for_inflight_requests() {
        let store = Arc::new(Mutex::new(ObservationStore::default()));
        let arm = store.lock().unwrap().register(1).unwrap().cursor;
        navigate(&mut store.lock().unwrap(), "a");
        store.lock().unwrap().record(&event(
            1,
            "Page.lifecycleEvent",
            json!({"frameId":"main", "loaderId":"a", "name":"load"}),
        ));
        store.lock().unwrap().record(&event(
            1,
            "Network.requestWillBeSent",
            json!({"requestId":"r1"}),
        ));
        let mut task = tokio::spawn({
            let store = store.clone();
            async move {
                wait_for_navigation_with_matcher(
                    &store,
                    1,
                    &arm,
                    None,
                    LoadState::NetworkIdle,
                    Duration::from_secs(5),
                )
                .await
            }
        });
        tokio::time::sleep(NETWORK_IDLE_QUIET + Duration::from_millis(50)).await;
        assert!(
            timeout(Duration::from_millis(50), &mut task).await.is_err(),
            "networkidle must stay blocked while a request is in flight"
        );
        store.lock().unwrap().record(&event(
            1,
            "Network.loadingFinished",
            json!({"requestId":"r1"}),
        ));
        let snapshot = task.await.unwrap().unwrap();
        assert_eq!(snapshot.inflight_network_requests, 0);
    }

    #[tokio::test]
    async fn network_idle_restarts_the_quiet_window_on_new_requests() {
        let store = Arc::new(Mutex::new(ObservationStore::default()));
        let arm = store.lock().unwrap().register(1).unwrap().cursor;
        navigate(&mut store.lock().unwrap(), "a");
        store.lock().unwrap().record(&event(
            1,
            "Page.lifecycleEvent",
            json!({"frameId":"main", "loaderId":"a", "name":"load"}),
        ));
        let started = Instant::now();
        let task = tokio::spawn({
            let store = store.clone();
            async move {
                wait_for_navigation_with_matcher(
                    &store,
                    1,
                    &arm,
                    None,
                    LoadState::NetworkIdle,
                    Duration::from_secs(5),
                )
                .await
            }
        });
        tokio::time::sleep(Duration::from_millis(250)).await;
        store.lock().unwrap().record(&event(
            1,
            "Network.requestWillBeSent",
            json!({"requestId":"late"}),
        ));
        tokio::time::sleep(Duration::from_millis(50)).await;
        store.lock().unwrap().record(&event(
            1,
            "Network.loadingFinished",
            json!({"requestId":"late"}),
        ));
        let snapshot = task.await.unwrap().unwrap();
        assert_eq!(snapshot.inflight_network_requests, 0);
        // Without the timer reset the first quiet window ends near 500ms; the
        // late request plus its completion must push the return past that.
        assert!(
            started.elapsed() >= NETWORK_IDLE_QUIET + Duration::from_millis(200),
            "quiet window was not restarted: {:?}",
            started.elapsed()
        );
    }

    #[tokio::test]
    async fn wait_uses_a_url_matcher_and_reports_mismatches_by_timeout() {
        let store = Mutex::new(ObservationStore::default());
        let arm = store.lock().unwrap().register(1).unwrap().cursor;
        navigate(&mut store.lock().unwrap(), "a");
        let glob = UrlMatcher::parse(None, Some("https://example.com/**"), None)
            .unwrap()
            .unwrap();
        let matched = wait_for_navigation_with_matcher(
            &store,
            1,
            &arm,
            Some(&glob),
            LoadState::Commit,
            Duration::from_millis(20),
        )
        .await
        .unwrap();
        assert_eq!(matched.url.as_deref(), Some("https://example.com/"));
        let other = UrlMatcher::parse(None, Some("https://other.example/**"), None)
            .unwrap()
            .unwrap();
        let err = wait_for_navigation_with_matcher(
            &store,
            1,
            &arm,
            Some(&other),
            LoadState::Commit,
            Duration::from_millis(20),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, BridgeError::Timeout(_)));
    }
}
