//! Bounded implementation of the browser pageAssets capability.
use crate::{
    browser::parse_tab_id,
    client::Client,
    error::{BridgeError, Result},
    file_results::{validate_output_directory, FileBudget, FileLimits, PathScope, StagedFiles},
    security::validate_url,
};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
};
use tokio::{
    sync::Mutex,
    time::{timeout, Duration},
};
use uuid::Uuid;

const MAX_ASSETS: usize = 512;
const MAX_RESOURCE: usize = 4 * 1024 * 1024;
const MAX_TOTAL: usize = 16 * 1024 * 1024;
const MAX_META: usize = 256 * 1024;
const BUDGET: Duration = Duration::from_secs(30);
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AssetKind {
    Document,
    Script,
    Other,
    Font,
    Image,
    Stylesheet,
    Video,
}
impl AssetKind {
    fn bundleable(self) -> bool {
        matches!(
            self,
            Self::Font | Self::Image | Self::Stylesheet | Self::Video
        )
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetSource {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub property: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageAsset {
    pub id: String,
    pub kind: AssetKind,
    pub name: String,
    pub url: String,
    pub sources: Vec<AssetSource>,
    pub mime_type: Option<String>,
    pub size: Option<u64>,
    #[serde(skip)]
    pub(crate) frame_id: Option<String>,
    #[serde(skip)]
    pub(crate) inline: Option<Vec<u8>>,
}
#[derive(Debug, Clone, Serialize)]
pub struct InventoryResult {
    pub id: String,
    pub page_url: String,
    pub assets: Vec<PageAsset>,
    pub partial: bool,
    pub limitations: Vec<String>,
}
#[derive(Debug, Clone, Default)]
pub struct BundleOptions {
    pub inventory_id: String,
    pub output_dir: PathBuf,
    pub asset_ids: Option<Vec<String>>,
    pub kinds: Option<Vec<AssetKind>>,
    pub max_resource_bytes: usize,
    pub max_total_bytes: usize,
}
#[derive(Debug, Clone, Serialize)]
pub struct ExportedAsset {
    pub id: String,
    pub kind: AssetKind,
    pub name: String,
    pub url: String,
    pub content_type: Option<String>,
    pub path: PathBuf,
    pub size: usize,
}
#[derive(Debug, Serialize)]
pub struct BundleResult {
    pub directory_path: PathBuf,
    pub manifest_path: PathBuf,
    pub assets: Vec<ExportedAsset>,
    pub partial_inventory: bool,
    pub path_scope: PathScope,
    pub browser_download: bool,
}
#[derive(Debug, Clone, Default)]
pub struct DownloadMediaOptions {
    pub output_dir: PathBuf,
    pub url: Option<String>,
    pub selector: Option<String>,
    pub max_bytes: usize,
}
#[derive(Clone, Default)]
pub struct PageAssetsRegistry {
    inventories: std::sync::Arc<Mutex<VecDeque<Inventory>>>,
}
#[derive(Clone)]
struct Inventory {
    tab: i64,
    epoch: Value,
    result: InventoryResult,
}

impl PageAssetsRegistry {
    pub async fn list(&self, c: &Client, tab_id: &str) -> Result<InventoryResult> {
        timeout(BUDGET, self.list_inner(c, tab_id))
            .await
            .map_err(|_| BridgeError::Timeout("page assets list".into()))?
    }
    async fn list_inner(&self, c: &Client, tab_id: &str) -> Result<InventoryResult> {
        let tab = parse_tab_id("page_assets", tab_id)?;
        let raw = cdp(c, tab, "Page.getResourceTree", None).await?;
        let root = raw.get("frameTree").unwrap_or(&raw);
        let ep = epoch(root)?;
        let mut assets = Vec::new();
        let mut partial = false;
        collect(root, &mut assets, &mut partial, 0);
        let mut limits=vec!["Current rendered page only; closed shadow roots and cross-origin iframe DOM are not inventoried.".into()];
        match cdp(
            c,
            tab,
            "Runtime.evaluate",
            Some(json!({"expression":SCRIPT,"returnByValue":true})),
        )
        .await
        {
            Ok(v) if v.get("exceptionDetails").is_none() => {
                let x = v
                    .pointer("/result/value")
                    .ok_or_else(|| proto("missing inventory value"))?;
                if serde_json::to_vec(x)
                    .map_err(|e| proto(&e.to_string()))?
                    .len()
                    > MAX_META
                {
                    return Err(user("page asset metadata limit exceeded"));
                }
                partial |= x["truncated"].as_bool().unwrap_or(false);
                if let Some(xs) = x["assets"].as_array() {
                    for a in xs.iter().take(MAX_ASSETS) {
                        let u = a["url"].as_str().unwrap_or("");
                        if u.is_empty() {
                            continue;
                        }
                        if let Some(old) = assets.iter_mut().find(|z| z.url == u) {
                            old.sources.push(AssetSource {
                                kind: "dom".into(),
                                node_id: None,
                                property: None,
                            });
                        } else if assets.len() < MAX_ASSETS {
                            assets.push(PageAsset {
                                id: stable(u),
                                kind: kind(a["kind"].as_str().unwrap_or("other")),
                                name: bound(a["name"].as_str().unwrap_or("asset"), 128),
                                url: u.into(),
                                sources: vec![AssetSource {
                                    kind: "dom".into(),
                                    node_id: None,
                                    property: None,
                                }],
                                mime_type: None,
                                size: None,
                                frame_id: None,
                                inline: None,
                            });
                        } else {
                            partial = true
                        }
                    }
                }
                if let Some(xs) = x["inlineSvgs"].as_array() {
                    for a in xs.iter().take(MAX_ASSETS) {
                        let m = a["markup"]
                            .as_str()
                            .ok_or_else(|| proto("invalid inline svg"))?;
                        if assets.len() >= MAX_ASSETS {
                            partial = true;
                            break;
                        }
                        let id = stable(m);
                        assets.push(PageAsset {
                            id: id.clone(),
                            kind: AssetKind::Image,
                            name: bound(a["name"].as_str().unwrap_or("svg"), 128),
                            url: format!("inline-svg:{id}"),
                            sources: vec![AssetSource {
                                kind: "inlineSvg".into(),
                                node_id: None,
                                property: None,
                            }],
                            mime_type: Some("image/svg+xml".into()),
                            size: Some(m.len() as u64),
                            frame_id: None,
                            inline: Some(m.as_bytes().to_vec()),
                        });
                    }
                }
            }
            _ => {
                partial = true;
                limits.push(
                    "DOM observation unavailable; only resource-tree metadata is included.".into(),
                )
            }
        }
        if !same(
            &ep,
            &epoch(
                cdp(c, tab, "Page.getFrameTree", None)
                    .await?
                    .get("frameTree")
                    .ok_or_else(|| proto("missing frame tree"))?,
            )?,
        ) {
            return Err(user("page changed while listing assets; list again"));
        }
        let out = InventoryResult {
            id: Uuid::new_v4().to_string(),
            page_url: ep["url"].as_str().unwrap_or("").into(),
            assets,
            partial,
            limitations: limits,
        };
        let mut store = self.inventories.lock().await;
        store.retain(|x| x.tab != tab);
        while store.len() >= 8 {
            store.pop_front();
        }
        store.push_back(Inventory {
            tab,
            epoch: ep,
            result: out.clone(),
        });
        Ok(out)
    }
    pub async fn bundle(&self, c: &Client, tab_id: &str, o: BundleOptions) -> Result<BundleResult> {
        validate_output_dir(&o.output_dir)?;
        let budget = bundle_budget(&o)?;
        self.bundle_with_budget(c, tab_id, o, budget).await
    }
    async fn bundle_with_budget(
        &self,
        c: &Client,
        tab_id: &str,
        o: BundleOptions,
        budget: FileBudget,
    ) -> Result<BundleResult> {
        let dir = validate_output_directory(&o.output_dir)?;
        let rc = cap(o.max_resource_bytes, MAX_RESOURCE)?;
        let tc = cap(o.max_total_bytes, MAX_TOTAL)?;
        timeout(BUDGET, async {
            let tab = parse_tab_id("page_assets", tab_id)?;
            let inv = self
                .inventories
                .lock()
                .await
                .iter()
                .find(|x| x.tab == tab && x.result.id == o.inventory_id)
                .cloned()
                .ok_or_else(|| user("unknown inventory; list this tab again"))?;
            if !same(
                &inv.epoch,
                &epoch(
                    cdp(c, tab, "Page.getFrameTree", None)
                        .await?
                        .get("frameTree")
                        .ok_or_else(|| proto("missing frame tree"))?,
                )?,
            ) {
                return Err(user("asset inventory is stale; list again"));
            }
            let ids = o.asset_ids.as_ref();
            if ids.is_some_and(|ids| ids.is_empty() || ids.len() > MAX_ASSETS) {
                return Err(user("asset_ids must contain 1..512 IDs"));
            }
            if let Some(ids) = ids {
                for id in ids {
                    let asset = inv
                        .result
                        .assets
                        .iter()
                        .find(|a| &a.id == id)
                        .ok_or_else(|| user("unknown asset id"))?;
                    if !asset.kind.bundleable() {
                        return Err(user("unsupported asset kind"));
                    }
                }
            }
            if o.kinds
                .as_ref()
                .is_some_and(|kinds| kinds.is_empty() || kinds.iter().any(|k| !k.bundleable()))
            {
                return Err(user("kinds must be image, font, stylesheet or video"));
            }
            let selected: Vec<_> = inv
                .result
                .assets
                .iter()
                .filter(|a| {
                    a.kind.bundleable()
                        && ids.is_none_or(|x| x.contains(&a.id))
                        && o.kinds.as_ref().is_none_or(|x| x.contains(&a.kind))
                })
                .collect();
            if selected.is_empty() {
                return Err(user("bundle matched no assets"));
            }
            let mut data = Vec::new();
            let mut total = 0usize;
            for a in selected.iter() {
                budget.check_elapsed()?;
                if a.size.is_some_and(|size| size > rc as u64) {
                    return Err(user("known resource size exceeds max_resource_bytes"));
                }
                let b = read(c, tab, a, rc).await?;
                total = total
                    .checked_add(b.len())
                    .ok_or_else(|| user("asset byte overflow"))?;
                if total > tc {
                    return Err(user("bundle exceeds max_total_bytes"));
                }
                data.push(((*a).clone(), b));
            }
            if !same(
                &inv.epoch,
                &epoch(
                    cdp(c, tab, "Page.getFrameTree", None)
                        .await?
                        .get("frameTree")
                        .ok_or_else(|| proto("missing frame tree"))?,
                )?,
            ) {
                return Err(user("page changed during bundle; no output written"));
            }
            publish(&dir, data, inv.result.partial, budget)
        })
        .await
        .map_err(|_| BridgeError::Timeout("page assets bundle; no output published".into()))?
    }
    pub async fn download_media(
        &self,
        c: &Client,
        tab_id: &str,
        o: DownloadMediaOptions,
    ) -> Result<BundleResult> {
        validate_output_dir(&o.output_dir)?;
        timeout(BUDGET, self.download_media_inner(c, tab_id, o))
            .await
            .map_err(|_| {
                BridgeError::Timeout("cached page media export; no output published".into())
            })?
    }
    async fn download_media_inner(
        &self,
        c: &Client,
        tab_id: &str,
        o: DownloadMediaOptions,
    ) -> Result<BundleResult> {
        let budget = bundle_budget(&BundleOptions {
            max_resource_bytes: o.max_bytes,
            max_total_bytes: o.max_bytes,
            ..Default::default()
        })?;
        let tab = parse_tab_id("download_media", tab_id)?;
        let url = match (o.url, o.selector) {
            (Some(x), None) => validate_url(&x)?,
            (None, Some(s)) => {
                if s.len() > 4096 || s.is_empty() {
                    return Err(user("invalid media selector"));
                }
                let e=format!("(()=>{{const es=document.querySelectorAll({});if(es.length!==1)throw Error('selector must match one element');const e=es[0];return e.currentSrc||e.src||e.href||null}})()",serde_json::to_string(&s).unwrap());
                let v = cdp(
                    c,
                    tab,
                    "Runtime.evaluate",
                    Some(json!({"expression":e,"returnByValue":true})),
                )
                .await?;
                validate_url(
                    v.pointer("/result/value")
                        .and_then(Value::as_str)
                        .ok_or_else(|| user("selector did not resolve media URL"))?,
                )?
            }
            _ => return Err(user("provide exactly one of url or selector")),
        };
        let inv = self.list(c, tab_id).await?;
        let a = inv
            .assets
            .iter()
            .find(|a| a.url == url && a.kind.bundleable())
            .ok_or_else(|| user("media URL is not an observed resource"))?;
        self.bundle_with_budget(
            c,
            tab_id,
            BundleOptions {
                inventory_id: inv.id,
                output_dir: o.output_dir,
                asset_ids: Some(vec![a.id.clone()]),
                kinds: None,
                max_resource_bytes: o.max_bytes,
                max_total_bytes: o.max_bytes,
            },
            budget,
        )
        .await
    }
    pub async fn clear(&self) {
        self.inventories.lock().await.clear();
    }
}

async fn cdp(c: &Client, t: i64, m: &str, p: Option<Value>) -> Result<Value> {
    let r = c
        .execute_cdp_with_timeout(t, m, p, Duration::from_secs(5))
        .await?;
    serde_json::from_str(r.get()).map_err(|e| proto(&e.to_string()))
}
fn epoch(v: &Value) -> Result<Value> {
    let f = &v["frame"];
    if ["id", "loaderId", "url"]
        .iter()
        .any(|k| f[k].as_str().is_none_or(str::is_empty))
    {
        return Err(proto("missing asset epoch"));
    }
    Ok(json!({"id":f["id"],"url":f["url"],"loaderId":f["loaderId"]}))
}
fn same(a: &Value, b: &Value) -> bool {
    a["id"] == b["id"] && a["url"] == b["url"] && a["loaderId"] == b["loaderId"]
}
fn collect(v: &Value, o: &mut Vec<PageAsset>, p: &mut bool, d: usize) {
    if d > 16 {
        *p = true;
        return;
    }
    let f = v["frame"]["id"].as_str().map(str::to_owned);
    if let Some(u) = v["frame"]["url"].as_str() {
        push(o, u, AssetKind::Document, None, None, f.clone(), p)
    }
    if let Some(xs) = v["resources"].as_array() {
        for x in xs {
            if let Some(u) = x["url"].as_str() {
                push(
                    o,
                    u,
                    kind(x["type"].as_str().unwrap_or("other")),
                    x["mimeType"].as_str(),
                    x["contentSize"]
                        .as_f64()
                        .filter(|n| n.is_finite() && *n >= 0.0)
                        .map(|n| n.ceil() as u64),
                    f.clone(),
                    p,
                )
            }
        }
    }
    if let Some(xs) = v["childFrames"].as_array() {
        for x in xs {
            collect(x, o, p, d + 1)
        }
    }
}
fn push(
    o: &mut Vec<PageAsset>,
    u: &str,
    k: AssetKind,
    m: Option<&str>,
    s: Option<u64>,
    f: Option<String>,
    p: &mut bool,
) {
    if u.len() > 8192 || o.len() >= MAX_ASSETS {
        *p = true;
        return;
    }
    if o.iter().any(|x| x.url == u) {
        return;
    }
    o.push(PageAsset {
        id: stable(u),
        kind: k,
        name: bound(
            u.split(['?', '#'])
                .next()
                .unwrap_or(u)
                .rsplit('/')
                .next()
                .unwrap_or("asset"),
            128,
        ),
        url: u.into(),
        sources: vec![AssetSource {
            kind: "resource".into(),
            node_id: None,
            property: None,
        }],
        mime_type: m.map(|x| bound(x, 128)),
        size: s,
        frame_id: f,
        inline: None,
    })
}
fn kind(s: &str) -> AssetKind {
    match s.to_ascii_lowercase().as_str() {
        "document" => AssetKind::Document,
        "script" => AssetKind::Script,
        "font" => AssetKind::Font,
        "image" => AssetKind::Image,
        "stylesheet" => AssetKind::Stylesheet,
        "media" | "video" => AssetKind::Video,
        _ => AssetKind::Other,
    }
}
fn cap(n: usize, m: usize) -> Result<usize> {
    if n > m {
        Err(user("byte cap exceeds hard limit"))
    } else {
        Ok(if n == 0 { m } else { n })
    }
}
fn bundle_budget(options: &BundleOptions) -> Result<FileBudget> {
    let resource = cap(options.max_resource_bytes, MAX_RESOURCE)?;
    let total = cap(options.max_total_bytes, MAX_TOTAL)?;
    FileBudget::new(FileLimits {
        max_files: MAX_ASSETS + 1,
        max_file_bytes: resource.max(MAX_META),
        max_total_bytes: total + MAX_META,
        max_duration: BUDGET,
    })
}
fn bound(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}
fn stable(s: &str) -> String {
    let mut h = 0xcbf29ce484222325u64;
    for b in s.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3)
    }
    format!("{h:016x}")
}
fn user(s: &str) -> BridgeError {
    BridgeError::User(s.into())
}
fn proto(s: &str) -> BridgeError {
    BridgeError::Protocol(s.into())
}
#[derive(Deserialize)]
struct Content {
    content: String,
    #[serde(default, rename = "base64Encoded")]
    base64_encoded: bool,
}
async fn read(c: &Client, t: i64, a: &PageAsset, cap: usize) -> Result<Vec<u8>> {
    if let Some(x) = &a.inline {
        return if x.len() > cap {
            Err(user("inline SVG exceeds cap"))
        } else {
            Ok(x.clone())
        };
    }
    validate_url(&a.url)?;
    let f = a
        .frame_id
        .as_ref()
        .ok_or_else(|| user("resource frame unavailable; no network fallback attempted"))?;
    let v = cdp(
        c,
        t,
        "Page.getResourceContent",
        Some(json!({"frameId":f,"url":a.url})),
    )
    .await?;
    let x: Content = serde_json::from_value(v).map_err(|e| proto(&e.to_string()))?;
    let b = if x.base64_encoded {
        if x.content.len() > cap.div_ceil(3) * 4 {
            return Err(user("encoded resource exceeds max_resource_bytes"));
        }
        base64::engine::general_purpose::STANDARD
            .decode(x.content)
            .map_err(|_| proto("invalid resource base64"))?
    } else {
        x.content.into_bytes()
    };
    if b.len() > cap {
        Err(user("resource exceeds max_resource_bytes"))
    } else {
        Ok(b)
    }
}
pub fn validate_output_dir(p: &Path) -> Result<()> {
    validate_output_directory(p).map(|_| ())
}
fn publish(
    dir: &Path,
    data: Vec<(PageAsset, Vec<u8>)>,
    partial: bool,
    budget: FileBudget,
) -> Result<BundleResult> {
    // Keep the acquisition deadline rather than resetting it before writing.
    let name = format!("codex-assets-{}", Uuid::new_v4());
    let mut files = StagedFiles::new(dir, budget)?;
    let p = files.destination(&name)?;
    let mut assets = Vec::new();
    for (i, (a, b)) in data.into_iter().enumerate() {
        let ext = match a.mime_type.as_deref() {
            Some("image/svg+xml") => "svg",
            Some("image/png") => "png",
            Some("image/jpeg") => "jpg",
            Some("image/webp") => "webp",
            Some("image/gif") => "gif",
            Some("text/css") => "css",
            Some("font/woff2") => "woff2",
            Some("font/woff") => "woff",
            Some("font/ttf") => "ttf",
            Some("video/mp4") => "mp4",
            Some("video/webm") => "webm",
            _ => "bin",
        };
        let basename = format!("asset-{i:04}.{ext}");
        let q = p.join(&basename);
        files.write(&basename, &b)?;
        assets.push(ExportedAsset {
            id: a.id,
            kind: a.kind,
            name: a.name,
            url: a.url,
            content_type: a.mime_type,
            path: q,
            size: b.len(),
        })
    }
    let m = p.join("manifest.json");
    let out = BundleResult {
        directory_path: p.to_owned(),
        manifest_path: m.clone(),
        assets,
        partial_inventory: partial,
        path_scope: PathScope::BridgeHost,
        browser_download: false,
    };
    let manifest = serde_json::to_vec_pretty(&out).map_err(|e| proto(&e.to_string()))?;
    if manifest.len() > MAX_META {
        return Err(user("asset manifest exceeds metadata limit"));
    }
    files.write("manifest.json", &manifest)?;
    files.publish_bundle(&name)?;
    Ok(out)
}
const SCRIPT: &str = r#"(()=>{
const assets=[],inlineSvgs=[],seen=new Set;let truncated=false,used=0;
const add=(url,kind,name)=>{
 if(!url||typeof url!=='string'||seen.has(url)||!/^https?:/.test(url))return;
 if(assets.length>=512||url.length>8192||used+url.length*3+256>240000){truncated=true;return}
 used+=url.length*3+256;seen.add(url);assets.push({url,kind,name:(name||url.split('/').pop()||'asset').slice(0,128)});
};
for(const e of performance.getEntriesByType('resource'))add(e.name,({img:'image',css:'stylesheet',script:'script',font:'font',video:'video'})[e.initiatorType]||'other');
for(const e of document.querySelectorAll('img,video,source,script,link,use')){
 const t=e.localName,u=e.currentSrc||e.src||e.href?.baseVal||e.href,k=t==='script'?'script':t==='video'||t==='source'?'video':t==='link'?(e.rel==='stylesheet'?'stylesheet':'other'):'image';
 add(u,k);if(t==='video')add(e.poster,'image');
}
const elements=document.querySelectorAll('*');
for(let i=0;i<Math.min(elements.length,3000);i++){
 const style=getComputedStyle(elements[i]);
 for(const prop of ['backgroundImage','maskImage','borderImageSource','listStyleImage']){
  for(const m of style[prop].matchAll(/url\(["']?([^"')]+)["']?\)/g)){try{add(new URL(m[1],document.baseURI).href,'image')}catch{}}
 }
}
if(elements.length>3000)truncated=true;
for(const [i,e]of [...document.querySelectorAll('svg')].entries()){
 const markup=e.outerHTML;
 if(inlineSvgs.length>=512||used+markup.length*3+256>240000){truncated=true;break}
 used+=markup.length*3+256;
 inlineSvgs.push({markup,name:(e.getAttribute('aria-label')||e.querySelector('title')?.textContent||e.id||('svg-'+i)).slice(0,128)});
}
return{assets,inlineSvgs,truncated}
})()"#;
