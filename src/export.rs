//! Safe, local content exports for non-Codex browser clients.
//!
//! This module deliberately does not use the ChatGPT artifact library.  It
//! reads the already-authorized page through CDP, performs same-page fetches
//! (therefore retaining the browser's login state), and writes a generated
//! file into an explicitly supplied, existing directory.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use base64::Engine;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::browser::{evaluate, list_tabs};
use crate::client::Client;
use crate::error::{BridgeError, Result};

const MAX_EXPORT_BYTES: usize = 10 * 1024 * 1024;

#[derive(Debug, Clone, Copy)]
struct ExportType {
    extension: &'static str,
    mime: &'static str,
}

fn export_type(format: &str) -> Result<ExportType> {
    match format.to_ascii_lowercase().as_str() {
        "pdf" => Ok(ExportType { extension: "pdf", mime: "application/pdf" }),
        "md" => Ok(ExportType { extension: "md", mime: "text/markdown; charset=utf-8" }),
        "xlsx" => Ok(ExportType { extension: "xlsx", mime: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" }),
        "csv" => Ok(ExportType { extension: "csv", mime: "text/csv; charset=utf-8" }),
        "docx" => Ok(ExportType { extension: "docx", mime: "application/vnd.openxmlformats-officedocument.wordprocessingml.document" }),
        "pptx" => Ok(ExportType { extension: "pptx", mime: "application/vnd.openxmlformats-officedocument.presentationml.presentation" }),
        other => Err(BridgeError::User(format!(
            "unsupported export format {other:?}; supported formats are pdf, md, xlsx, csv, docx, and pptx"
        ))),
    }
}

fn tab_number(tab_id: &str) -> Result<i64> {
    tab_id
        .parse::<i64>()
        .map_err(|_| BridgeError::User(format!("export requires numeric tab_id, got {:?}", tab_id)))
}

fn runtime_string(raw: &serde_json::value::RawValue) -> Result<String> {
    let value: Value = serde_json::from_str(raw.get())
        .map_err(|e| BridgeError::Protocol(format!("decode Runtime.evaluate result: {e}")))?;
    if let Some(details) = value.get("exceptionDetails") {
        return Err(BridgeError::User(format!(
            "page export evaluation failed: {}",
            details
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or("unknown page error")
        )));
    }
    value
        .get("result")
        .and_then(|v| v.get("value"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| BridgeError::Protocol("Runtime.evaluate returned no string value".into()))
}

async fn page_read(client: &Client, tab_id: &str, expression: &str) -> Result<Value> {
    let raw = evaluate(client, tab_id, expression).await?;
    let text = runtime_string(&raw)?;
    serde_json::from_str(&text)
        .map_err(|e| BridgeError::Protocol(format!("decode page export response: {e}")))
}

fn host_and_path(url: &str) -> Option<(&str, &str, &str)> {
    let scheme_end = url.find("://")?;
    let rest = &url[scheme_end + 3..];
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let host = &rest[..authority_end];
    let path_start = authority_end;
    let path_end = rest.find(['?', '#']).unwrap_or(rest.len());
    let path = &rest[path_start..path_end];
    let query = if path_end < rest.len() && rest.as_bytes()[path_end] == b'?' {
        let end = rest[path_end + 1..]
            .find('#')
            .map(|i| path_end + 1 + i)
            .unwrap_or(rest.len());
        &rest[path_end + 1..end]
    } else {
        ""
    };
    Some((host, path, query))
}

fn google_export_url(page_url: &str, format: &str) -> Result<(String, ExportType)> {
    let ty = export_type(format)?;
    let (host, path, query) =
        host_and_path(page_url).ok_or_else(|| BridgeError::User("malformed tab URL".into()))?;
    let host = host.to_ascii_lowercase();
    if !page_url.starts_with("https://") || host != "docs.google.com" {
        return Err(BridgeError::User(
            "Google Workspace export requires a docs.google.com tab".into(),
        ));
    }
    let parts: Vec<_> = path.split('/').filter(|part| !part.is_empty()).collect();
    let product = parts.first().copied().unwrap_or_default();
    let id = parts
        .iter()
        .position(|part| *part == "d")
        .and_then(|position| parts.get(position + 1))
        .copied()
        .unwrap_or_default();
    if id.is_empty()
        || parts.last() == Some(&"pub")
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(BridgeError::User(
            "tab is not a supported Google Workspace document URL".into(),
        ));
    }
    let allowed = match product {
        "document" => matches!(ty.extension, "pdf" | "md" | "docx"),
        "spreadsheets" => matches!(ty.extension, "pdf" | "xlsx" | "csv"),
        "presentation" => matches!(ty.extension, "pdf" | "pptx"),
        _ => false,
    };
    if !allowed {
        return Err(BridgeError::User(format!(
            "format {} is not supported for Google {product}",
            ty.extension
        )));
    }
    // Preserve the explicit GSuite export type. In particular, `md` is not
    // plain text: substituting `txt` would lose links and formatting.
    let wire_format = ty.extension;
    let mut url = format!("https://{host}/{product}/d/{id}/export?format={wire_format}");
    if let Some(tab) = query
        .split('&')
        .find_map(|part| part.strip_prefix("tab="))
        .filter(|value| !value.is_empty())
    {
        url.push_str("&tab=");
        url.push_str(tab);
    }
    Ok((url, ty))
}

fn page_fetch_script(url: &str, binary: bool) -> String {
    let url = serde_json::to_string(url).expect("URL serialization cannot fail");
    if binary {
        format!(
            r#"(async()=>{{try{{const r=await fetch({url},{{credentials:'include'}});if(!r.ok)return JSON.stringify({{error:'HTTP '+r.status}});const b=new Uint8Array(await r.arrayBuffer());if(b.byteLength>{MAX_EXPORT_BYTES})return JSON.stringify({{error:'response exceeds 10 MiB'}});let s='';for(let i=0;i<b.length;i+=0x8000)s+=String.fromCharCode(...b.subarray(i,i+0x8000));return JSON.stringify({{base64:btoa(s)}})}}catch(e){{return JSON.stringify({{error:'page fetch failed'}})}}}})()"#
        )
    } else {
        format!(
            r#"(async()=>{{try{{const r=await fetch({url},{{credentials:'include'}});if(!r.ok)return JSON.stringify({{error:'HTTP '+r.status}});const t=await r.text();if(new TextEncoder().encode(t).length>{MAX_EXPORT_BYTES})return JSON.stringify({{error:'response exceeds 10 MiB'}});return JSON.stringify({{text:t}})}}catch(e){{return JSON.stringify({{error:'page fetch failed'}})}}}})()"#
        )
    }
}

fn normalize_markdown(text: &str) -> String {
    text.lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            !(trimmed.starts_with('[') && trimmed.contains("]: <data:"))
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn youtube_script() -> &'static str {
    // This follows the current official bridge strategy: ask the page's
    // player to load captions, observe the resulting timedtext request, then
    // fetch its json3 representation with the page's existing credentials.
    r#"(async()=>{try{const id=new URL(location.href).searchParams.get('v');if(!id||!/^[A-Za-z0-9_-]{11}$/.test(id))return JSON.stringify({error:'unsupported YouTube watch URL'});const p=document.querySelector('ytd-player')||document.querySelector('#movie_player');const call=(o,k)=>{const f=o&&Reflect.get(o,k);return typeof f==='function'?Reflect.apply(f,o,[]):undefined};const data=call(p,'getVideoData');if(!p||!data||data.video_id!==id||!['getVideoData','isSubtitlesOn','toggleSubtitles','toggleSubtitlesOn'].every(k=>typeof Reflect.get(p,k)==='function'))return JSON.stringify({error:'YouTube transcript is unavailable'});let found=null;const obs=new PerformanceObserver(es=>{for(const e of es.getEntries()){try{const u=new URL(e.name);if(u.protocol==='https:'&&u.hostname==='www.youtube.com'&&u.pathname==='/api/timedtext'&&u.searchParams.get('v')===id)found=u}catch{}}});obs.observe({type:'resource',buffered:true});const was=!!call(p,'isSubtitlesOn');try{if(was)call(p,'toggleSubtitles');call(p,'toggleSubtitlesOn');const end=Date.now()+4000;while(!found&&Date.now()<end)await new Promise(r=>setTimeout(r,150))}finally{if(was&&!call(p,'isSubtitlesOn'))call(p,'toggleSubtitlesOn');if(!was&&call(p,'isSubtitlesOn'))call(p,'toggleSubtitles');obs.disconnect()}if(!found)return JSON.stringify({error:'YouTube transcript is unavailable'});found.searchParams.set('fmt','json3');const r=await fetch(found.toString(),{credentials:'include'});if(!r.ok)return JSON.stringify({error:'YouTube transcript request failed with HTTP '+r.status});const j=await r.json();const out=['YouTube transcript','Video ID: '+id,'Language: '+(found.searchParams.get('lang')||'unknown'),'Captions: '+(found.searchParams.get('kind')==='asr'?'auto-generated':'authored or unspecified'),''];for(const ev of (j.events||[])){if(!Array.isArray(ev.segs))continue;const text=ev.segs.map(s=>typeof s.utf8==='string'?s.utf8:'').join('').replace(/\s+/g,' ').trim();if(!text)continue;const sec=Math.floor((Number(ev.tStartMs)||0)/1000),h=Math.floor(sec/3600),m=Math.floor(sec%3600/60),s=sec%60;out.push('['+(h?h+':'+String(m).padStart(2,'0')+':':m)+String(s).padStart(2,'0')+'] '+text);if(out.join('\n').length>10485760){out.push('','[Transcript truncated]');break}}return out.length>5?JSON.stringify({text:out.join('\n')}):JSON.stringify({error:'YouTube transcript is unavailable'})}catch{return JSON.stringify({error:'YouTube transcript is unavailable'})}})()"#
}

fn validate_output_dir(output_dir: &Path) -> Result<PathBuf> {
    if !output_dir.is_absolute() {
        return Err(BridgeError::User(
            "output_dir must be an existing absolute directory".into(),
        ));
    }
    let meta = fs::symlink_metadata(output_dir)
        .map_err(|_| BridgeError::User("output_dir must already exist".into()))?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(BridgeError::User(
            "output_dir must be a real directory, not a symlink or file".into(),
        ));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return Err(BridgeError::User(
                "output_dir reparse points are not allowed".into(),
            ));
        }
    }
    // Inspect every lexical component as well as the leaf.  Checking only
    // the final directory would allow `safe\link\exports` to traverse a
    // junction/symlink in an ancestor before canonicalization.
    let mut component = output_dir.to_path_buf();
    loop {
        let component_meta = fs::symlink_metadata(&component)
            .map_err(|_| BridgeError::User("cannot inspect output_dir components".into()))?;
        if component_meta.file_type().is_symlink() {
            return Err(BridgeError::User(
                "output_dir path components may not be symlinks".into(),
            ));
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if component_meta.file_attributes() & 0x400 != 0 {
                return Err(BridgeError::User(
                    "output_dir path components may not be reparse points".into(),
                ));
            }
        }
        let Some(parent) = component.parent() else {
            break;
        };
        if parent == component {
            break;
        }
        component = parent.to_path_buf();
    }
    let canonical = fs::canonicalize(output_dir)
        .map_err(|_| BridgeError::User("cannot resolve output_dir".into()))?;
    let canonical_meta = fs::symlink_metadata(&canonical)
        .map_err(|_| BridgeError::User("cannot inspect output_dir".into()))?;
    if canonical_meta.file_type().is_symlink() {
        return Err(BridgeError::User(
            "output_dir symlink is not allowed".into(),
        ));
    }
    Ok(canonical)
}

/// Validate an export destination without creating anything.  The directory
/// must be an existing absolute real directory; symlinks and Windows reparse
/// points are rejected to prevent writes escaping the user-selected folder.
pub fn validate_export_directory(output_dir: &Path) -> Result<PathBuf> {
    validate_output_dir(output_dir)
}

fn write_new(dir: &Path, ty: ExportType, bytes: &[u8]) -> Result<PathBuf> {
    if bytes.len() > MAX_EXPORT_BYTES {
        return Err(BridgeError::User("export exceeds 10 MiB".into()));
    }
    let path = dir.join(format!("codex-export-{}.{}", Uuid::new_v4(), ty.extension));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| BridgeError::User(format!("create generated export failed: {e}")))?;
    let result = (|| -> std::io::Result<()> {
        file.write_all(bytes)?;
        file.sync_all()
    })();
    if let Err(err) = result {
        let _ = fs::remove_file(&path);
        return Err(BridgeError::PipeIo(err));
    }
    Ok(path)
}

/// Export the currently authorized tab to a newly-created file in `output_dir`.
/// `kind` is `gsuite` or `youtube_transcript`; `format` is required for GSuite.
pub async fn export(
    client: &Client,
    tab_id: &str,
    kind: &str,
    format: Option<&str>,
    output_dir: &Path,
) -> Result<Value> {
    let dir = validate_output_dir(output_dir)?;
    let id = tab_number(tab_id)?;
    let tabs = list_tabs(client).await?;
    let listed = tabs
        .into_iter()
        .find(|tab| tab.id == tab_id || tab.raw_id.as_i64() == Some(id))
        .ok_or_else(|| BridgeError::User("tab_id is not an open tab".into()))?;
    let current = page_read(
        client,
        tab_id,
        "JSON.stringify({url:location.href,title:document.title})",
    )
    .await?;
    let page_url = current
        .get("url")
        .and_then(Value::as_str)
        .ok_or_else(|| BridgeError::Protocol("current tab URL unavailable".into()))?;
    if page_url != listed.url {
        return Err(BridgeError::User(
            "tab changed while preparing export; retry".into(),
        ));
    }
    let (ty, bytes) = match kind.to_ascii_lowercase().as_str() {
        "gsuite" | "google_workspace" => {
            let format = format
                .ok_or_else(|| BridgeError::User("format is required for gsuite export".into()))?;
            let (url, ty) = google_export_url(page_url, format)?;
            let binary = ty.extension != "md";
            let value = page_read(client, tab_id, &page_fetch_script(&url, binary)).await?;
            if let Some(error) = value.get("error").and_then(Value::as_str) {
                return Err(BridgeError::User(format!("Google export failed: {error}")));
            }
            let bytes = if binary {
                let encoded = value.get("base64").and_then(Value::as_str).ok_or_else(|| {
                    BridgeError::Protocol("Google export returned no bytes".into())
                })?;
                if encoded.len() > MAX_EXPORT_BYTES.div_ceil(3) * 4 {
                    return Err(BridgeError::User("export exceeds 10 MiB".into()));
                }
                base64::engine::general_purpose::STANDARD
                    .decode(encoded)
                    .map_err(|_| {
                        BridgeError::Protocol("Google export returned invalid base64".into())
                    })?
            } else {
                normalize_markdown(value.get("text").and_then(Value::as_str).ok_or_else(|| {
                    BridgeError::Protocol("Google export returned no text".into())
                })?)
                .into_bytes()
            };
            (ty, bytes)
        }
        "youtube" | "youtube_transcript" => {
            let (host, path, _) = host_and_path(page_url)
                .ok_or_else(|| BridgeError::User("malformed tab URL".into()))?;
            if !page_url.to_ascii_lowercase().starts_with("https://")
                || !matches!(
                    host.to_ascii_lowercase().as_str(),
                    "youtube.com" | "www.youtube.com"
                )
                || !path.eq_ignore_ascii_case("/watch")
            {
                return Err(BridgeError::User(
                    "YouTube transcript export requires an HTTPS youtube.com /watch tab".into(),
                ));
            }
            let value = page_read(client, tab_id, youtube_script()).await?;
            if let Some(error) = value.get("error").and_then(Value::as_str) {
                return Err(BridgeError::User(error.into()));
            }
            let text = value
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(|| BridgeError::User("YouTube transcript unavailable".into()))?;
            (
                ExportType {
                    extension: "txt",
                    mime: "text/plain; charset=utf-8",
                },
                text.as_bytes().to_vec(),
            )
        }
        other => {
            return Err(BridgeError::User(format!(
                "unsupported export kind {other:?}; use gsuite or youtube_transcript"
            )))
        }
    };
    if bytes.is_empty() {
        return Err(BridgeError::User("export returned no content".into()));
    }
    let bytes_len = bytes.len();
    let path = write_new(&dir, ty, &bytes)?;
    Ok(
        json!({"path": path, "bytes": bytes_len, "format": ty.extension, "mime": ty.mime, "kind": kind}),
    )
}
