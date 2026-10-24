//! Bounded, opt-in observation of one owned tab. Events are never attributed
//! from an untagged transport, and cursors cannot cross a reconnect or turn.
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::{Arc, Mutex};

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
                // A main-document commit invalidates its previous descendants.
                self.frames.clear();
                self.frames_truncated = false;
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

/// Wait for a main-frame navigation observed after the supplied arm cursor.
/// Exact URL matching is intentionally not a Playwright glob/regex contract.
/// Load milestones are loader-bound; networkidle is deliberately unsupported.
pub(crate) async fn wait_for_navigation(
    store: &Mutex<ObservationStore>,
    tab_id: i64,
    cursor: &ObservationCursor,
    expected_url: Option<&str>,
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
                };
                if ready
                    && expected_url.is_none_or(|expected| snapshot.url.as_deref() == Some(expected))
                {
                    return Ok(snapshot);
                }
            }
        }
        timeout(deadline.saturating_duration_since(Instant::now()), notified)
            .await
            .map_err(|_| BridgeError::Timeout("observed navigation".into()))?;
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
}
