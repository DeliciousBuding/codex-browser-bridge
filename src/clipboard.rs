//! Session-scoped virtual clipboard matching the browser-use `TabClipboardAPI`.
//!
//! This is intentionally not the operating-system clipboard and never calls
//! `navigator.clipboard`.  The bridge owns one value per browser session; the
//! server should clear it after a successful rebind and retain it across turns.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::time::Duration;

use crate::{
    client::Client,
    error::{BridgeError, Result},
};

/// Keep clipboard payloads below the protocol's 8 MiB output ceiling.  This
/// limit applies to decoded binary bytes and UTF-8 text bytes together.
pub const MAX_CLIPBOARD_BYTES: usize = 4 * 1024 * 1024;
const MAX_ENCODED_CLIPBOARD_BYTES: usize = 8 * 1024 * 1024;
const MAX_ITEMS: usize = 256;
const MAX_ENTRIES: usize = 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClipboardEntry {
    #[serde(rename = "mime_type", alias = "mimeType")]
    pub mime_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base64: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClipboardItem {
    pub entries: Vec<ClipboardEntry>,
    #[serde(
        rename = "presentation_style",
        alias = "presentationStyle",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub presentation_style: Option<PresentationStyle>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PresentationStyle {
    Unspecified,
    Inline,
    Attachment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipboardShortcut {
    Paste,
    PastePlainText,
}

/// Recognize only the browser-use paste chords.  Other shortcuts (including
/// Ctrl/Cmd+C/X) remain ordinary keypresses; the bridge never pretends to read
/// the operating-system clipboard.
pub fn shortcut_action(keys: &[String]) -> Option<ClipboardShortcut> {
    if !(2..=3).contains(&keys.len()) {
        return None;
    }
    let normalized: Vec<String> = keys
        .iter()
        .map(|key| key.trim().to_ascii_lowercase())
        .collect();
    let mut modifier = false;
    let mut shift = false;
    let mut v = false;
    for key in &normalized {
        match key.as_str() {
            "ctrl" | "control" | "meta" | "cmd" | "command" if !modifier => modifier = true,
            "shift" if !shift => shift = true,
            "v" if !v => v = true,
            _ => return None,
        }
    }
    if !modifier || !v || (keys.len() == 3 && !shift) {
        return None;
    }
    Some(if shift {
        ClipboardShortcut::PastePlainText
    } else {
        ClipboardShortcut::Paste
    })
}

#[derive(Clone, Default, PartialEq, Eq)]
pub struct VirtualClipboard {
    items: Vec<ClipboardItem>,
    bytes: usize,
}

impl std::fmt::Debug for VirtualClipboard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VirtualClipboard")
            .field("item_count", &self.items.len())
            .field("bytes", &self.bytes)
            .finish_non_exhaustive()
    }
}

impl VirtualClipboard {
    pub fn new() -> Self {
        Self::default()
    }

    /// Return a defensive copy; clipboard contents are never logged.
    pub fn read(&self) -> Vec<ClipboardItem> {
        self.items.clone()
    }

    /// Return the first `text/plain` entry, or an empty string when absent.
    pub fn read_text(&self) -> String {
        self.items
            .iter()
            .flat_map(|item| item.entries.iter())
            .find_map(|entry| {
                (entry.mime_type.eq_ignore_ascii_case("text/plain"))
                    .then(|| entry.text.clone())
                    .flatten()
            })
            .unwrap_or_default()
    }

    pub fn byte_len(&self) -> usize {
        self.bytes
    }

    /// Replace the session clipboard after validating every item atomically.
    pub fn write(&mut self, items: Vec<ClipboardItem>) -> Result<()> {
        let (items, bytes) = validate_items(items, "clipboard.write")?;
        self.items = items;
        self.bytes = bytes;
        Ok(())
    }

    pub fn write_text(&mut self, text: impl Into<String>) -> Result<()> {
        self.write(vec![ClipboardItem {
            entries: vec![ClipboardEntry {
                mime_type: "text/plain".into(),
                text: Some(text.into()),
                base64: None,
            }],
            presentation_style: Some(PresentationStyle::Unspecified),
        }])
    }

    /// Clear contents. Called by the server after a successful browser rebind
    /// and on session disposal; turn end intentionally does not call this.
    pub fn clear(&mut self) {
        self.items.clear();
        self.bytes = 0;
    }

    /// Dispatch a browser `ClipboardEvent` using this virtual clipboard.
    ///
    /// The page's default paste insertion is performed only when the event is
    /// not canceled (`dispatchEvent` returns true), matching browser-use's
    /// implementation and preventing double insertion by event handlers.
    pub async fn paste(&self, client: &Client, tab_id: i64) -> Result<()> {
        self.paste_impl(client, tab_id, false).await
    }

    /// The browser-use Ctrl/Cmd+Shift+V action exposes only `text/plain`.
    pub async fn paste_plain_text(&self, client: &Client, tab_id: i64) -> Result<()> {
        self.paste_impl(client, tab_id, true).await
    }

    async fn paste_impl(&self, client: &Client, tab_id: i64, plain_only: bool) -> Result<()> {
        if self.items.is_empty() {
            return Err(BridgeError::User(
                "clipboard.paste requires clipboard data".into(),
            ));
        }
        let mut items = self.read();
        if plain_only {
            for item in &mut items {
                item.entries
                    .retain(|entry| entry.mime_type.eq_ignore_ascii_case("text/plain"));
            }
            items.retain(|item| !item.entries.is_empty());
            if items.is_empty() {
                return Err(BridgeError::User(
                    "clipboard.paste requires plain text data".into(),
                ));
            }
        }
        let items = serde_json::to_value(&items)
            .map_err(|err| BridgeError::Protocol(format!("encode clipboard items: {err}")))?;
        let expression = paste_script(&items);
        // Attach via a harmless read; dispatch the mutating event exactly once,
        // never through execute_cdp's session-invalid retry path.
        client
            .execute_cdp_with_timeout(
                tab_id,
                "Runtime.evaluate",
                Some(json!({
                    "expression": "true", "returnByValue": true
                })),
                Duration::from_secs(10),
            )
            .await?;
        let raw = client
            .send_request_with_timeout(
                "executeCdp",
                Some(json!({
                    "target": {"tabId": tab_id},
                    "method": "Runtime.evaluate",
                    "commandParams": {
                        "expression": expression,
                        "returnByValue": true,
                        "awaitPromise": false,
                        "timeout": 10000
                    }
                })),
                Duration::from_secs(10),
            )
            .await?;
        let result: Value = serde_json::from_str(raw.get()).map_err(|_| {
            BridgeError::Protocol("clipboard.paste returned invalid CDP data".into())
        })?;
        if result.get("error").is_some() {
            return Err(BridgeError::User(
                "clipboard.paste CDP execution failed; the paste was not retried".into(),
            ));
        }
        if let Some(message) = crate::browser::evaluate_exception_message(&raw) {
            return Err(BridgeError::User(format!("clipboard.paste: {message}")));
        }
        if result.pointer("/result/value") != Some(&Value::Bool(true)) {
            return Err(BridgeError::Protocol(
                "clipboard.paste did not confirm completion".into(),
            ));
        }
        Ok(())
    }
}

fn validate_items(items: Vec<ClipboardItem>, op: &str) -> Result<(Vec<ClipboardItem>, usize)> {
    if items.is_empty() {
        return Err(BridgeError::User(format!("{op} requires items")));
    }
    if items.len() > MAX_ITEMS {
        return Err(BridgeError::User(format!(
            "{op} supports at most {MAX_ITEMS} items"
        )));
    }
    let mut total = 0usize;
    let mut entry_count = 0usize;
    let mut out = Vec::with_capacity(items.len());
    for mut item in items {
        if item.entries.is_empty() {
            return Err(BridgeError::User(format!(
                "{op} requires clipboard item entries"
            )));
        }
        entry_count = checked_add(entry_count, item.entries.len(), op)?;
        if entry_count > MAX_ENTRIES {
            return Err(BridgeError::User(format!(
                "{op} supports at most {MAX_ENTRIES} entries"
            )));
        }
        for entry in &mut item.entries {
            entry.mime_type = entry.mime_type.trim().to_string();
            validate_mime(&entry.mime_type, op)?;
            match (&entry.text, &entry.base64) {
                (Some(text), None) => total = checked_add(total, text.len(), op)?,
                (None, Some(encoded)) => {
                    if encoded.len() > MAX_CLIPBOARD_BYTES.div_ceil(3) * 4 {
                        return Err(BridgeError::User(format!("{op} exceeds base64 size limit")));
                    }
                    let decoded = STANDARD.decode(encoded).map_err(|_| {
                        BridgeError::User(format!("{op} requires valid base64 entry data"))
                    })?;
                    total = checked_add(total, decoded.len(), op)?;
                }
                _ => {
                    return Err(BridgeError::User(format!(
                        "{op} requires exactly one of entry text or base64"
                    )))
                }
            }
            if total > MAX_CLIPBOARD_BYTES {
                return Err(BridgeError::User(format!(
                    "{op} exceeds {} byte limit",
                    MAX_CLIPBOARD_BYTES
                )));
            }
        }
        out.push(item);
    }
    // Includes MIME metadata and JSON escaping; this also bounds the paste
    // expression and subsequent JSON-encoded transport frame.
    if serde_json::to_vec(&out)
        .map_err(|_| BridgeError::Protocol("encode clipboard items".into()))?
        .len()
        > MAX_ENCODED_CLIPBOARD_BYTES
    {
        return Err(BridgeError::User(format!(
            "{op} exceeds {MAX_ENCODED_CLIPBOARD_BYTES} byte encoded size limit"
        )));
    }
    Ok((out, total))
}

fn checked_add(total: usize, add: usize, op: &str) -> Result<usize> {
    total
        .checked_add(add)
        .ok_or_else(|| BridgeError::User(format!("{op} exceeds size limit")))
}

fn validate_mime(mime: &str, op: &str) -> Result<()> {
    let token = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&b))
    };
    let mut parts = mime.split(';');
    let essence = parts.next().unwrap_or_default();
    let valid = essence
        .split_once('/')
        .is_some_and(|(ty, sub)| token(ty) && token(sub))
        && parts.all(|part| {
            part.trim().split_once('=').is_some_and(|(key, value)| {
                token(key.trim())
                    && (token(value.trim())
                        || (value.starts_with('"')
                            && value.ends_with('"')
                            && value.len() >= 2
                            && value[1..value.len() - 1]
                                .bytes()
                                .all(|b| (32..127).contains(&b) && b != b'"')))
            })
        });
    if mime.len() > 256 || !valid || mime.bytes().any(|b| b.is_ascii_control()) {
        return Err(BridgeError::User(format!("{op} requires valid MIME type")));
    }
    Ok(())
}

/// Build a self-contained expression.  DataTransfer/File are page-native;
/// no OS clipboard or `navigator.clipboard` permission is involved.
pub fn paste_script(items: &Value) -> String {
    let encoded = serde_json::to_string(items).expect("clipboard items are JSON");
    format!(
        r#"(() => {{
const items={encoded};
const focused=(root)=>{{
  const el=root.activeElement;
  if(!el) return null;
  if(el.shadowRoot) return focused(el.shadowRoot)||el;
  if(el.localName==="iframe"||el.localName==="frame") {{
    let doc; try{{doc=el.contentDocument||el.contentWindow?.document;}}catch{{throw new Error("virtual clipboard cannot reach focused cross-origin frame");}}
    if(!doc) throw new Error("virtual clipboard cannot reach focused cross-origin frame");
    return focused(doc)||doc.body;
  }}
  return el;
}};
const active=focused(document)||document.body;
if(!active) throw new Error("no focused element");
const view=active.ownerDocument.defaultView;
if(!view||typeof view.DataTransfer!=="function"||typeof view.ClipboardEvent!=="function") throw new Error("ClipboardEvent is unavailable");
const dt=new view.DataTransfer();
const decode=(s)=>{{const b=view.atob(s),u=new Uint8Array(b.length);for(let i=0;i<b.length;i++)u[i]=b.charCodeAt(i);return u;}};
for(const item of items) for(const entry of item.entries) {{
  if(typeof entry.text==="string") dt.setData(entry.mime_type,entry.text);
  else {{ const subtype=(entry.mime_type.split("/",2)[1]||"bin").split(";",1)[0]; dt.items.add(new view.File([decode(entry.base64)],`clipboard.${{subtype}}`,{{type:entry.mime_type}})); }}
}}
const ev=new view.ClipboardEvent("paste",{{bubbles:true,cancelable:true,composed:true,clipboardData:dt}});
if(!active.dispatchEvent(ev)) return true;
const plain=items.flatMap(i=>i.entries).find(e=>e.mime_type.toLowerCase()==="text/plain"&&typeof e.text==="string")?.text||"";
if(!active.isConnected) throw new Error("virtual clipboard target left the document during paste");
if(active instanceof view.HTMLInputElement||active instanceof view.HTMLTextAreaElement) {{
  if(active.disabled||active.readOnly||plain.length===0) return true;
  const a=active.selectionStart,b=active.selectionEnd;
  if(a==null||b==null) {{
    const setter=Object.getOwnPropertyDescriptor(Object.getPrototypeOf(active),"value")?.set;
    if(setter) setter.call(active,active.value+plain); else active.value+=plain;
  }} else active.setRangeText(plain,a,b,"end");
  active.dispatchEvent(new view.InputEvent("input",{{bubbles:true}}));
}}
else if(plain.length>0&&(active.isContentEditable||active.closest?.("[contenteditable=true]"))) {{active.focus();active.ownerDocument.execCommand("insertText",false,plain);}}
return true;
}})()"#
    )
}
