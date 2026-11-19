//! Thin access to the installed native Codex MCP services (`node_repl` and `cua_repl`).
//! No app objects, input engine, permissions, or screenshots are reimplemented.
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use uuid::Uuid;

const DEFAULT_START_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_TOOL_TIMEOUT: Duration = Duration::from_secs(300);
const END_TIMEOUT: Duration = Duration::from_secs(8);
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_RELAY_OUTBOUND_FRAMES: usize = 256;
const MAX_RELAY_OUTBOUND_BYTES: usize = 8 * 1024 * 1024;
const MAX_NATIVE_FRAME_BYTES: usize = 64 * 1024 * 1024;
const MAX_CLI_LINE_BYTES: usize = 1024 * 1024;
const MAX_NATIVE_CATALOG_PAGES: usize = 100;
const MAX_NATIVE_CATALOG_ITEMS: usize = 2048;
const MAX_NATIVE_CATALOG_CURSOR_BYTES: usize = 64 * 1024;
const MAX_TRACKED_CATALOG_REQUESTS: usize = 1024;
const CLIENT_PROTOCOL_VERSION: &str = "2025-06-18";

#[cfg(unix)]
const CORE_ENV_VARS: &[&str] = &[
    "HOME",
    "LOGNAME",
    "PATH",
    "SHELL",
    "USER",
    "__CF_USER_TEXT_ENCODING",
    "LANG",
    "LC_ALL",
    "TERM",
    "TMPDIR",
    "TZ",
];

#[cfg(windows)]
const CORE_ENV_VARS: &[&str] = &[
    "PATH",
    "PATHEXT",
    "SHELL",
    "COMSPEC",
    "SYSTEMROOT",
    "WINDIR",
    "SYSTEMDRIVE",
    "USERNAME",
    "USERDOMAIN",
    "USERPROFILE",
    "HOMEDRIVE",
    "HOMEPATH",
    "PROGRAMFILES",
    "PROGRAMFILES(X86)",
    "PROGRAMW6432",
    "PROGRAMDATA",
    "LOCALAPPDATA",
    "APPDATA",
    "TEMP",
    "TMP",
    "TMPDIR",
    "POWERSHELL",
    "PWSH",
];

const CUSTOM_CA_ENV_VARS: &[&str] = &[
    "CODEX_CA_CERTIFICATE",
    "SSL_CERT_FILE",
    "REQUESTS_CA_BUNDLE",
    "CURL_CA_BUNDLE",
    "NODE_EXTRA_CA_CERTS",
    "GIT_SSL_CAINFO",
    "CARGO_HTTP_CAINFO",
    "PIP_CERT",
    "BUNDLE_SSL_CA_CERT",
    "npm_config_cafile",
    "NPM_CONFIG_CAFILE",
];

const NON_INHERITABLE_ENV_VARS: &[&str] = &[
    "CODEX_EXEC_SERVER_NOISE_AUTH_TOKEN",
    "NODE_REPL_AUTH_TOKEN",
    "CODEX_GUARDIAN_DECISIONS_API_KEY",
    "OPENAI_FEDERATION_RULE_ID",
    "OPENAI_IDENTITY_TOKEN_FILE",
    "OPENAI_WORKLOAD_IDENTITY_CONTEXT",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum NativeEnvVar {
    Name(String),
    Config {
        name: String,
        #[serde(default)]
        source: Option<String>,
    },
}

impl NativeEnvVar {
    fn name(&self) -> &str {
        match self {
            Self::Name(name) => name,
            Self::Config { name, .. } => name,
        }
    }

    fn source(&self) -> Option<&str> {
        match self {
            Self::Name(_) => None,
            Self::Config { source, .. } => source.as_deref(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeServer {
    Computer,
    Browser,
}

impl NativeServer {
    fn name(self) -> &'static str {
        match self {
            Self::Computer => "cua_repl",
            Self::Browser => "node_repl",
        }
    }

    fn surface(self) -> &'static str {
        match self {
            Self::Computer => "computer",
            Self::Browser => "native-browser",
        }
    }
}

#[derive(Debug, Deserialize)]
struct LaunchRow {
    command: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    env_vars: Vec<NativeEnvVar>,
    #[serde(default)]
    cwd: Option<PathBuf>,
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default)]
    startup_timeout_sec: Option<f64>,
    #[serde(default)]
    startup_timeout_ms: Option<u64>,
    #[serde(default)]
    tool_timeout_sec: Option<f64>,
    #[serde(default)]
    enabled_tools: Option<Vec<String>>,
    #[serde(default)]
    disabled_tools: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Launch {
    #[serde(skip)]
    server: NativeServer,
    pub command: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub env_vars: Vec<NativeEnvVar>,
    pub cwd: Option<PathBuf>,
    pub enabled: Option<bool>,
    pub startup_timeout_sec: Option<f64>,
    pub startup_timeout_ms: Option<u64>,
    pub tool_timeout_sec: Option<f64>,
    pub enabled_tools: Option<Vec<String>>,
    pub disabled_tools: Option<Vec<String>>,
}

impl Launch {
    /// Read the native host's launch configuration; never rewrite generated caches.
    pub fn discover(server: NativeServer, explicit: Option<&Path>) -> Result<Self> {
        if let Some(path) = explicit {
            return Self::read(server, path);
        }
        let home = std::env::var_os("CODEX_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(
                    std::env::var_os("USERPROFILE")
                        .or_else(|| std::env::var_os("HOME"))
                        .unwrap_or_default(),
                )
                .join(".codex")
            });
        // Explicit native MCP tables own runtime paths/display context. Cache
        // templates can be stale or lack the host's current X11 environment.
        let native_config = home.join("config.toml");
        if native_config.is_file() {
            let text = std::fs::read_to_string(&native_config)
                .context("read Codex native configuration")?;
            let document: toml::Value = toml::from_str(&text).map_err(|err: toml::de::Error| {
                let line = err.span().map(|span| {
                    text.as_bytes()[..span.start.min(text.len())]
                        .iter()
                        .filter(|b| **b == b'\n')
                        .count()
                        + 1
                });
                anyhow::anyhow!(
                    "invalid Codex configuration TOML{}",
                    line.map(|line| format!(" at line {line}"))
                        .unwrap_or_default()
                )
            })?;
            if let Some(row) = document
                .get("mcp_servers")
                .and_then(|v| v.get(server.name()))
            {
                return Self::from_row(server, serde_json::to_value(row)?);
            }
        }
        if matches!(server, NativeServer::Browser) {
            bail!("native node_repl launch configuration unavailable; configure [mcp_servers.node_repl] in Codex or pass --native-config <MCP JSON>");
        }
        let cache = home.join("plugins/cache/openai-bundled/unified-computer-use");
        let mut candidates: Vec<_> = std::fs::read_dir(cache)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path().join(".mcp.json"))
            .filter(|path| path.is_file())
            .collect();
        candidates.sort_by_key(|path| std::fs::metadata(path).and_then(|m| m.modified()).ok());
        candidates.reverse();
        if let Some(path) = candidates.first() {
            return Self::read(server, path);
        }
        bail!("installed native cua_repl launch configuration unavailable; start the native app or pass --native-config <MCP JSON>");
    }

    fn read(server: NativeServer, path: &Path) -> Result<Self> {
        let document: Value = serde_json::from_str(
            &std::fs::read_to_string(path).context("read native MCP configuration")?,
        )?;
        let row = document
            .get("mcpServers")
            .and_then(|table| table.get(server.name()))
            .with_context(|| {
                format!(
                    "native configuration must contain mcpServers.{}",
                    server.name()
                )
            })?;
        Self::from_row(server, row.clone())
    }

    fn from_row(server: NativeServer, row: Value) -> Result<Self> {
        let row: LaunchRow = serde_json::from_value(row)?;
        let launch = Self {
            server,
            command: row.command,
            args: row.args,
            env: row.env,
            env_vars: row.env_vars,
            cwd: row.cwd,
            enabled: row.enabled,
            startup_timeout_sec: row.startup_timeout_sec,
            startup_timeout_ms: row.startup_timeout_ms,
            tool_timeout_sec: row.tool_timeout_sec,
            enabled_tools: row.enabled_tools,
            disabled_tools: row.disabled_tools,
        };
        launch.validated()
    }

    fn validated(self) -> Result<Self> {
        if self.enabled == Some(false) {
            bail!(
                "native {} launch configuration is disabled",
                self.server.name()
            );
        }
        let command = Path::new(&self.command);
        if command.is_absolute() && !command.is_file() {
            bail!("native command must identify an installed executable");
        }
        let environment = self.environment()?;
        if matches!(self.server, NativeServer::Computer) {
            let repl = environment
                .get(OsStr::new("CUA_REPL_NODE_REPL_PATH"))
                .context("native configuration lacks CUA_REPL_NODE_REPL_PATH")?;
            let repl = Path::new(repl);
            if !repl.is_absolute() || !repl.is_file() {
                bail!("native NodeREPL executable unavailable");
            }
        }
        if let Some(value) = self.startup_timeout_sec {
            if !value.is_finite() || value < 0.0 {
                bail!("native startup_timeout_sec must be finite and nonnegative");
            }
        }
        if let Some(value) = self.tool_timeout_sec {
            if !value.is_finite() || value < 0.0 {
                bail!("native tool_timeout_sec must be finite and nonnegative");
            }
        }
        let _ = self.startup_timeout()?;
        let _ = self.tool_timeout()?;
        Ok(self)
    }

    fn environment(&self) -> Result<BTreeMap<OsString, OsString>> {
        let mut environment = CORE_ENV_VARS
            .iter()
            .filter_map(|name| std::env::var_os(name).map(|value| (OsString::from(name), value)))
            .collect::<BTreeMap<_, _>>();

        for env_var in &self.env_vars {
            match env_var.source() {
                None | Some("local") => {}
                Some("remote") => bail!(
                    "env_vars entry '{}' uses source 'remote', which requires remote MCP stdio",
                    env_var.name()
                ),
                Some(source) => {
                    bail!("unsupported env_vars source '{source}'; expected 'local' or 'remote'")
                }
            }
            if is_non_inheritable_env_var(env_var.name()) {
                continue;
            }
            if let Some(value) = std::env::var_os(env_var.name()) {
                environment.insert(OsString::from(env_var.name()), value);
            }
        }

        for name in CUSTOM_CA_ENV_VARS {
            let Some(value) = std::env::var_os(name) else {
                continue;
            };
            if value.is_empty() {
                continue;
            }
            let value = std::path::absolute(PathBuf::from(value))?.into_os_string();
            #[cfg(windows)]
            environment.retain(|key, _| !key.to_string_lossy().eq_ignore_ascii_case(name));
            environment.insert(OsString::from(name), value);
        }

        for (name, value) in &self.env {
            let name_os = OsString::from(name);
            if cfg!(windows)
                || CUSTOM_CA_ENV_VARS
                    .iter()
                    .any(|ca_name| ca_name.eq_ignore_ascii_case(name))
            {
                environment.retain(|key, _| {
                    !key.to_string_lossy()
                        .eq_ignore_ascii_case(&name_os.to_string_lossy())
                });
            }
            environment.insert(name_os, OsString::from(value));
        }
        environment.retain(|name, _| {
            name.to_str()
                .is_none_or(|name| !is_non_inheritable_env_var(name))
        });
        Ok(environment)
    }

    fn startup_timeout(&self) -> Result<Duration> {
        match (self.startup_timeout_sec, self.startup_timeout_ms) {
            (Some(seconds), _) => Duration::try_from_secs_f64(seconds)
                .context("native startup_timeout_sec is invalid"),
            (None, Some(millis)) => Ok(Duration::from_millis(millis)),
            (None, None) => Ok(DEFAULT_START_TIMEOUT),
        }
    }

    fn tool_timeout(&self) -> Result<Duration> {
        self.tool_timeout_sec
            .map(|seconds| {
                Duration::try_from_secs_f64(seconds).context("native tool_timeout_sec is invalid")
            })
            .transpose()
            .map(|timeout| timeout.unwrap_or(DEFAULT_TOOL_TIMEOUT))
    }

    fn tool_policy(&self) -> ToolPolicy {
        ToolPolicy {
            enabled: self
                .enabled_tools
                .as_ref()
                .map(|tools| tools.iter().cloned().collect()),
            disabled: self
                .disabled_tools
                .as_ref()
                .map(|tools| tools.iter().cloned().collect())
                .unwrap_or_default(),
        }
    }

    fn command(&self) -> Result<process_wrap::tokio::CommandWrap> {
        let environment = self.environment()?;
        let cwd = self
            .cwd
            .clone()
            .unwrap_or(std::env::current_dir().context("resolve native working directory")?);
        let program = resolve_program(&self.command, &environment, &cwd);
        let mut command = tokio::process::Command::new(program);
        command
            .args(&self.args)
            .env_clear()
            .envs(&environment)
            .current_dir(&cwd)
            .stderr(Stdio::inherit());
        #[cfg(windows)]
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
        let mut command = process_wrap::tokio::CommandWrap::from(command);
        #[cfg(windows)]
        command.wrap(process_wrap::tokio::JobObject);
        #[cfg(unix)]
        command.wrap(process_wrap::tokio::ProcessGroup::leader());
        command.wrap(process_wrap::tokio::KillOnDrop);
        Ok(command)
    }

    pub fn summary(&self) -> Value {
        let mut out = json!({
            "surface":self.server.surface(),"status":"launch_configuration_found",
            "command":self.command,"args":self.args,"cwd":self.cwd,
            "env_keys":self.env.keys().collect::<Vec<_>>(),
            "env_vars":self.env_vars.iter().map(NativeEnvVar::name).collect::<Vec<_>>(),
            "enabled":self.enabled.unwrap_or(true),
            "startup_timeout":self.startup_timeout().ok(),
            "tool_timeout":self.tool_timeout().ok(),
            "enabled_tools":self.enabled_tools,
            "disabled_tools":self.disabled_tools,
            "live_support":"not established by discovery"
        });
        if matches!(self.server, NativeServer::Browser) {
            if let Ok(env) = self.environment() {
                let has = |target: &str| {
                    env.keys()
                        .any(|key| key.to_string_lossy().eq_ignore_ascii_case(target))
                };
                out["browser_access_env"] = json!({
                    "executor_id": has("BROWSER_USE_ACCESS_EXECUTOR_ID"),
                    "pipe_path": has("BROWSER_USE_ACCESS_PIPE_PATH"),
                    "verifying_identity": has("BROWSER_USE_ACCESS_VERIFYING_IDENTITY")
                });
            }
            let (client_path, plugins) = discover_browser_client();
            out["browser_client_url"] = json!(client_path.as_deref().map(path_to_file_url));
            out["browser_client"] = json!(client_path);
            if !plugins.is_empty() {
                out["browser_plugins"] = json!(plugins);
            }
        }
        out
    }
}

fn path_to_file_url(path: &Path) -> String {
    let raw = path.to_string_lossy().replace('\\', "/");
    let stripped = raw.strip_prefix("//?/").unwrap_or(&raw);
    let mut encoded = String::with_capacity(stripped.len() + 16);
    for ch in stripped.chars() {
        match ch {
            ' ' => encoded.push_str("%20"),
            '#' => encoded.push_str("%23"),
            '?' => encoded.push_str("%3F"),
            '%' => encoded.push_str("%25"),
            _ => encoded.push(ch),
        }
    }
    if encoded.starts_with('/') {
        format!("file://{encoded}")
    } else {
        format!("file:///{encoded}")
    }
}

/// Discover the installed Codex `browser` / `chrome` plugin `browser-client.mjs`
/// entry point under `<CODEX_HOME>/plugins/cache/openai-bundled/` so non-Codex
/// MCP hosts can bootstrap `setupBrowserRuntime()` without guessing paths.
fn discover_browser_client() -> (Option<PathBuf>, BTreeMap<String, String>) {
    let mut plugins = BTreeMap::new();
    let mut selected: Option<PathBuf> = None;
    let home = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(
                std::env::var_os("USERPROFILE")
                    .or_else(|| std::env::var_os("HOME"))
                    .unwrap_or_default(),
            )
            .join(".codex")
        });
    let base = home.join("plugins").join("cache").join("openai-bundled");
    for plugin in ["browser", "chrome"] {
        let dir = base.join(plugin);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut versions: Vec<(bool, std::time::SystemTime, String, PathBuf)> = entries
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let version = e.file_name().to_str()?.to_string();
                let candidate = e.path().join("scripts").join("browser-client.mjs");
                if !candidate.is_file() {
                    return None;
                }
                let mtime = std::fs::metadata(&candidate)
                    .and_then(|m| m.modified())
                    .unwrap_or(std::time::UNIX_EPOCH);
                Some((version != "latest", mtime, version, candidate))
            })
            .collect();
        versions.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| b.1.cmp(&a.1))
                .then_with(|| b.2.cmp(&a.2))
        });
        if let Some((_, _, version, candidate)) = versions.into_iter().next() {
            plugins.insert(plugin.to_string(), version);
            if selected.is_none() {
                selected = Some(candidate);
            }
        }
    }
    (selected, plugins)
}

#[derive(Clone, Debug, Default)]
struct ToolPolicy {
    enabled: Option<BTreeSet<String>>,
    disabled: BTreeSet<String>,
}

impl ToolPolicy {
    fn allows(&self, name: &str) -> bool {
        self.enabled
            .as_ref()
            .is_none_or(|enabled| enabled.contains(name))
            && !self.disabled.contains(name)
    }

    fn filter_catalog(&self, result: &mut Value) {
        if let Some(tools) = result.get_mut("tools").and_then(Value::as_array_mut) {
            tools.retain(|tool| {
                tool.get("name")
                    .and_then(Value::as_str)
                    .is_some_and(|name| self.allows(name))
            });
        }
    }
}

fn is_non_inheritable_env_var(name: &str) -> bool {
    NON_INHERITABLE_ENV_VARS
        .iter()
        .any(|restricted| restricted.eq_ignore_ascii_case(name))
}

#[cfg(unix)]
fn resolve_program(
    program: &str,
    _environment: &BTreeMap<OsString, OsString>,
    _cwd: &Path,
) -> OsString {
    OsString::from(program)
}

#[cfg(windows)]
fn resolve_program(
    program: &str,
    environment: &BTreeMap<OsString, OsString>,
    cwd: &Path,
) -> OsString {
    let search_path = environment.iter().find_map(|(name, value)| {
        name.to_string_lossy()
            .eq_ignore_ascii_case("PATH")
            .then_some(value)
    });
    which::which_in(program, search_path, cwd)
        .map(PathBuf::into_os_string)
        .unwrap_or_else(|_| OsString::from(program))
}

/// Keep process ownership outside the stdio transport so cancelling a transport
/// shutdown cannot drop the only handle before the process tree has exited.
struct OwnedProcess {
    child: Box<dyn process_wrap::tokio::ChildWrapper>,
    closed: bool,
}

impl OwnedProcess {
    fn spawn(
        launch: &Launch,
    ) -> Result<(
        Self,
        tokio::process::ChildStdout,
        tokio::process::ChildStdin,
    )> {
        let mut command = launch.command()?;
        command
            .command_mut()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped());
        let mut child = command
            .spawn()
            .with_context(|| format!("start native {}", launch.server.name()))?;
        let input = child.stdin().take().context("native stdin missing")?;
        let output = child.stdout().take().context("native stdout missing")?;
        Ok((
            Self {
                child,
                closed: false,
            },
            output,
            input,
        ))
    }

    async fn close(&mut self) -> Result<()> {
        let graceful = tokio::time::timeout(Duration::from_secs(3), self.child.wait()).await;
        tracing::debug!(
            graceful = matches!(graceful, Ok(Ok(_))),
            "native launcher exit before owned tree termination"
        );
        // Also terminate descendants if the launcher already exited on EOF.
        if let Err(error) = self.child.start_kill() {
            #[cfg(unix)]
            if error.raw_os_error() != Some(libc::ESRCH) {
                return Err(error.into());
            }
            #[cfg(not(unix))]
            return Err(error.into());
        }
        tokio::time::timeout(END_TIMEOUT, self.child.wait())
            .await
            .context("native process termination timed out")??;
        self.closed = true;
        Ok(())
    }
}

impl Drop for OwnedProcess {
    fn drop(&mut self) {
        if !self.closed {
            let _ = self.child.start_kill();
        }
    }
}

struct OutboundFrame {
    bytes: Vec<u8>,
    written: usize,
    deadline: Option<tokio::time::Instant>,
    cancelled_id: Option<String>,
}

struct JsonReader<R> {
    reader: BufReader<R>,
    buffer: Vec<u8>,
    side: &'static str,
}

impl<R: AsyncRead + Unpin> JsonReader<R> {
    async fn receive(&mut self) -> Result<Option<Value>> {
        let remaining = MAX_NATIVE_FRAME_BYTES + 1 - self.buffer.len();
        let bytes = tokio::io::AsyncReadExt::take(&mut self.reader, remaining as u64)
            .read_until(b'\n', &mut self.buffer)
            .await?;
        if self.buffer.len() > MAX_NATIVE_FRAME_BYTES {
            self.buffer.clear();
            self.buffer.shrink_to(0);
            bail!("{} MCP frame exceeds 64 MiB", self.side);
        }
        if bytes == 0 && self.buffer.is_empty() {
            return Ok(None);
        }
        let row = serde_json::from_slice(&self.buffer).context("invalid MCP JSON frame");
        self.buffer.clear();
        if self.buffer.capacity() > MAX_CLI_LINE_BYTES {
            self.buffer.shrink_to(64 * 1024);
        }
        row.map(Some)
    }
}

struct JsonWriter<W> {
    writer: W,
    outbound: VecDeque<OutboundFrame>,
    outbound_bytes: usize,
}

impl<W: AsyncWrite + Unpin> JsonWriter<W> {
    fn enqueue(&mut self, row: Value, cancelled_id: Option<String>) -> Result<()> {
        let mut bytes = serde_json::to_vec(&row)?;
        bytes.push(b'\n');
        self.outbound_bytes = self.outbound_bytes.saturating_add(bytes.len());
        self.outbound.push_back(OutboundFrame {
            bytes,
            written: 0,
            deadline: None,
            cancelled_id,
        });
        Ok(())
    }

    fn has_outbound(&self) -> bool {
        !self.outbound.is_empty()
    }

    fn can_accept_outbound(&self) -> bool {
        self.outbound.len() < MAX_RELAY_OUTBOUND_FRAMES
            && self.outbound_bytes < MAX_RELAY_OUTBOUND_BYTES
    }

    fn has_active_write(&self) -> bool {
        self.outbound
            .front()
            .is_some_and(|frame| frame.written > 0 || frame.deadline.is_some())
    }

    async fn flush_next(&mut self, timeout_context: &'static str) -> Result<Option<String>> {
        let Some(front) = self.outbound.front_mut() else {
            return Ok(None);
        };
        let deadline = *front
            .deadline
            .get_or_insert_with(|| tokio::time::Instant::now() + WRITE_TIMEOUT);
        while front.written < front.bytes.len() {
            let count =
                tokio::time::timeout_at(deadline, self.writer.write(&front.bytes[front.written..]))
                    .await
                    .map_err(|_| anyhow::anyhow!("{timeout_context}"))??;
            if count == 0 {
                bail!("transport closed during write");
            }
            front.written += count;
        }
        tokio::time::timeout_at(deadline, self.writer.flush())
            .await
            .map_err(|_| anyhow::anyhow!("{timeout_context}"))??;
        let completed = self.outbound.pop_front().expect("front frame existed");
        self.outbound_bytes = self.outbound_bytes.saturating_sub(completed.bytes.len());
        Ok(completed.cancelled_id)
    }
}

/// Standard MCP stdio framing with untyped JSON preserves native extensions
/// across both MCP relay and CLI/doctor modes. Keep the read buffer and queued
/// outbound frames on disjoint halves so select cancellation never discards a
/// partial line or partial write.
struct JsonTransport<R, W> {
    reader: JsonReader<R>,
    writer: JsonWriter<W>,
}

impl<R: AsyncRead + Unpin, W: AsyncWrite + Unpin> JsonTransport<R, W> {
    fn new(reader: R, writer: W) -> Self {
        Self::for_side(reader, writer, "native")
    }

    fn for_side(reader: R, writer: W, side: &'static str) -> Self {
        Self {
            reader: JsonReader {
                reader: BufReader::new(reader),
                buffer: Vec::new(),
                side,
            },
            writer: JsonWriter {
                writer,
                outbound: VecDeque::new(),
                outbound_bytes: 0,
            },
        }
    }

    async fn receive(&mut self) -> Result<Option<Value>> {
        self.reader.receive().await
    }

    async fn send(&mut self, row: Value) -> Result<()> {
        let mut bytes = serde_json::to_vec(&row)?;
        bytes.push(b'\n');
        self.writer.writer.write_all(&bytes).await?;
        self.writer.writer.flush().await?;
        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        self.writer.writer.shutdown().await?;
        Ok(())
    }
}

fn prepare_call_meta(
    params: &mut Value,
    executing: &mut Option<Value>,
    default_session: &str,
    default_turn: &str,
    tool_name: &str,
) -> Result<()> {
    let params = params
        .as_object_mut()
        .context("native call parameters missing")?;
    let meta = params
        .entry("_meta")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .context("native metadata must be an object")?;
    meta.entry("callId")
        .or_insert_with(|| json!(Uuid::new_v4().to_string()));
    if !meta.contains_key("x-codex-turn-metadata") {
        let context = executing
            .as_ref()
            .and_then(|m| m.get("x-codex-turn-metadata"))
            .cloned()
            .unwrap_or_else(|| json!({"session_id": default_session, "turn_id": default_turn}));
        meta.insert("x-codex-turn-metadata".into(), context);
    }
    let context = parse_context(meta.get("x-codex-turn-metadata").unwrap())?;
    meta.entry("sessionId")
        .or_insert_with(|| context["session_id"].clone());
    meta.entry("threadId").or_insert_with(|| {
        context
            .get("thread_id")
            .cloned()
            .unwrap_or_else(|| context["session_id"].clone())
    });
    if tool_name == "js" {
        *executing = Some(Value::Object(meta.clone()));
    } else if tool_name == "turn_ended" {
        let args = params
            .entry("arguments")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .context("turn_ended arguments must be an object")?;
        args.entry("hook_event_name")
            .or_insert_with(|| json!("Stop"));
        args.entry("session_id")
            .or_insert_with(|| context["session_id"].clone());
        args.entry("turn_id")
            .or_insert_with(|| context["turn_id"].clone());
    }
    Ok(())
}

fn cleanup_metadata(mut meta: Value) -> Value {
    if let Some(object) = meta.as_object_mut() {
        object.remove("progressToken");
        object.insert("callId".into(), json!(Uuid::new_v4().to_string()));
    }
    meta
}

fn finish<T>(result: Result<T>, cleanup: Result<()>) -> Result<T> {
    match (result, cleanup) {
        (Err(error), Err(cleanup)) => Err(anyhow::anyhow!("{error:#}; {cleanup:#}")),
        (Err(error), Ok(())) => Err(error),
        (Ok(value), cleanup) => {
            cleanup?;
            Ok(value)
        }
    }
}

async fn cleanup_session<R, W>(
    native: &mut JsonTransport<R, W>,
    write_failed: bool,
    active: impl IntoIterator<Item = Value>,
    executing: Option<Value>,
    tools: &ToolPolicy,
    event: &str,
) -> Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let cleanup = async {
        if write_failed {
            bail!("native byte stream may be incomplete; cleanup requires process termination");
        }
        for id in active {
            native
                .send(json!({
                    "jsonrpc": "2.0",
                    "method": "notifications/cancelled",
                    "params": {"requestId": id, "reason": "client ended"}
                }))
                .await?;
        }
        if tools.allows("turn_ended") {
            if let Some(meta) = executing {
                let context = parse_context(
                    meta.get("x-codex-turn-metadata")
                        .context("execution context missing")?,
                )?;
                let id = format!("bridge-cleanup-{}", Uuid::new_v4());
                let meta = cleanup_metadata(meta);
                let request = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "method": "tools/call",
                    "params": {
                        "name": "turn_ended",
                        "arguments": {
                            "hook_event_name": event,
                            "session_id": context["session_id"],
                            "turn_id": context["turn_id"]
                        },
                        "_meta": meta
                    }
                });
                native.send(request).await?;
                loop {
                    let row = native
                        .receive()
                        .await?
                        .context("native disconnected before cleanup")?;
                    if row["id"] == id && row.get("result").is_some() {
                        if row["result"]["isError"] == true {
                            bail!("native cleanup rejected");
                        }
                        break;
                    } else if row["id"] == id && row.get("error").is_some() {
                        if is_missing_cleanup_tool(&row["error"]) {
                            tracing::debug!(
                                error = %row["error"],
                                "native runtime does not expose turn_ended; skipping synthetic hook"
                            );
                            break;
                        }
                        bail!("native cleanup failed: {}", row["error"]);
                    } else if row["method"] == "ping" && row.get("id").is_some() {
                        native
                            .send(json!({"jsonrpc":"2.0","id":row["id"],"result":{}}))
                            .await?;
                    } else if row["method"] == "elicitation/create" && row.get("id").is_some() {
                        native
                            .send(json!({
                                "jsonrpc": "2.0",
                                "id": row["id"],
                                "result": {"action": "cancel"}
                            }))
                            .await?;
                    } else if row.get("method").is_some() && row.get("id").is_some() {
                        native
                            .send(json!({
                                "jsonrpc": "2.0",
                                "id": row["id"],
                                "error": {
                                    "code": -32603,
                                    "message": "client ended before native request could be presented"
                                }
                            }))
                            .await?;
                    }
                }
            }
        }
        Ok::<_, anyhow::Error>(())
    };
    let cleanup = tokio::time::timeout(END_TIMEOUT, cleanup)
        .await
        .context("native cleanup timed out")
        .and_then(|r| r);
    let closed = tokio::time::timeout(END_TIMEOUT, native.close())
        .await
        .context("native transport close timed out")
        .and_then(|r| r);
    finish(cleanup, closed).context("native cleanup was not confirmed")
}

/// Relay native MCP frames without normalizing schemas or native extensions.
/// The external MCP host owns handshake negotiation, pagination traversal,
/// interactive elicitation prompts, and client request deadlines.
pub async fn run_mcp(launch: Launch) -> Result<()> {
    let tools = launch.tool_policy();
    let (mut process, output, input) = OwnedProcess::spawn(&launch)?;
    let mut native = JsonTransport::new(output, input);
    let mut host = JsonTransport::for_side(tokio::io::stdin(), tokio::io::stdout(), "client");
    let result = relay(&mut host, &mut native, shutdown_signal(), &tools).await;
    // ChildStdin::shutdown is a no-op on supported platforms. Dropping the
    // handle delivers EOF before the process receives its graceful deadline.
    drop(native);
    let terminated = process.close().await;
    finish(result, terminated)
}

fn is_missing_cleanup_tool(error: &Value) -> bool {
    if error.get("code").and_then(Value::as_i64) == Some(-32601) {
        return true;
    }
    let Some(message) = error.get("message").and_then(Value::as_str) else {
        return false;
    };
    let lower = message.to_ascii_lowercase();
    lower.contains("unknown tool") || lower.contains("method not found")
}

fn clear_executing_if_ended(executing: &mut Option<Value>, arguments: Option<&Value>) {
    let Some(active_meta) = executing.as_ref() else {
        return;
    };
    let Ok(active_ctx) = active_meta
        .get("x-codex-turn-metadata")
        .context("missing")
        .and_then(parse_context)
    else {
        *executing = None;
        return;
    };
    let Some(args) = arguments.and_then(Value::as_object) else {
        return;
    };
    let same_session = args
        .get("session_id")
        .and_then(Value::as_str)
        .is_some_and(|id| id == active_ctx["session_id"].as_str().unwrap_or_default());
    let same_turn = args
        .get("turn_id")
        .and_then(Value::as_str)
        .is_some_and(|id| id == active_ctx["turn_id"].as_str().unwrap_or_default());
    if same_session && same_turn {
        *executing = None;
    }
}

async fn relay<HR, HW, NR, NW, F>(
    host: &mut JsonTransport<HR, HW>,
    native: &mut JsonTransport<NR, NW>,
    stop: F,
    tools: &ToolPolicy,
) -> Result<()>
where
    HR: AsyncRead + Unpin,
    HW: AsyncWrite + Unpin,
    NR: AsyncRead + Unpin,
    NW: AsyncWrite + Unpin,
    F: std::future::Future<Output = ()>,
{
    let default_session = Uuid::new_v4().to_string();
    let mut default_turn = Uuid::new_v4().to_string();
    let mut executing: Option<Value> = None;
    let mut active = BTreeMap::<String, (Value, bool)>::new();
    let mut catalog_requests = BTreeSet::<String>::new();
    let mut write_failed = false;
    let mut host_eof = false;
    tokio::pin!(stop);

    let outcome = async {
        let host_reader = &mut host.reader;
        let host_writer = &mut host.writer;
        let native_reader = &mut native.reader;
        let native_writer = &mut native.writer;
        loop {
            if host_eof && !native_writer.has_outbound() && !host_writer.has_outbound() {
                return Ok::<_, anyhow::Error>(if active.values().any(|(_, is_call)| *is_call) {
                    "Interrupt"
                } else {
                    "Stop"
                });
            }
            tokio::select! {
                row = host_reader.receive(), if !host_eof && native_writer.can_accept_outbound() && host_writer.can_accept_outbound() => {
                    let Some(mut row) = row? else {
                        host_eof = true;
                        continue;
                    };
                    let request_id = row.get("id").cloned();
                    if row["method"] == "tools/call" {
                        let Some(id) = request_id else {
                            continue;
                        };
                        if !(id.is_string() || id.is_i64() || id.is_u64()) {
                            host_writer.enqueue(json!({
                                "jsonrpc":"2.0","id":Value::Null,
                                "error":{"code":-32600,"message":"native tools/call id must be a string or integer"}
                            }), None)?;
                            continue;
                        }
                        let tool_name = row["params"]["name"].as_str().filter(|name| !name.is_empty());
                        let Some(tool_name) = tool_name else {
                            host_writer.enqueue(json!({
                                "jsonrpc":"2.0","id":id,
                                "error":{"code":-32602,"message":"native tools/call requires a nonempty name"}
                            }), None)?;
                            continue;
                        };
                        if !tools.allows(tool_name) {
                            let reply = json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "error": {
                                        "code": -32602,
                                        "message": format!("native tool '{tool_name}' is disabled by launch configuration")
                                    }
                                });
                            host_writer.enqueue(reply, None)?;
                            continue;
                        }
                        let tool_name = tool_name.to_string();
                        let ends_turn = tool_name == "turn_ended";
                        if let Err(error) = prepare_call_meta(
                            &mut row["params"],
                            &mut executing,
                            &default_session,
                            &default_turn,
                            &tool_name,
                        ) {
                                let reply = json!({
                                    "jsonrpc": "2.0",
                                    "id": id,
                                    "error": {"code": -32602, "message": error.to_string()}
                                });
                                host_writer.enqueue(reply, None)?;
                                continue;
                            }
                        if ends_turn {
                            let was_active = executing.is_some();
                            clear_executing_if_ended(&mut executing, row["params"].get("arguments"));
                            if was_active && executing.is_none() {
                                default_turn = Uuid::new_v4().to_string();
                            }
                        }
                        active.insert(id.to_string(), (id, true));
                    } else if row["method"] == "tools/list" {
                        if let Some(id) = request_id {
                            let key = id.to_string();
                            if catalog_requests.len() >= MAX_TRACKED_CATALOG_REQUESTS {
                                catalog_requests.pop_first();
                            }
                            catalog_requests.insert(key.clone());
                            active.insert(key, (id, false));
                        }
                    } else if row.get("method").is_some() {
                        if let Some(id) = request_id {
                            active.insert(id.to_string(), (id, false));
                        }
                    }
                    let cancelled_id = (row["method"] == "notifications/cancelled")
                        .then(|| row["params"]["requestId"].to_string());
                    native_writer.enqueue(row, cancelled_id)?;
                }
                row = native_reader.receive(), if host_writer.can_accept_outbound() => {
                    let row = match row {
                        Ok(Some(row)) => row,
                        Ok(None) => {
                            write_failed = true;
                            bail!("native service closed connection");
                        }
                        Err(err) => {
                            write_failed = true;
                            return Err(err);
                        }
                    };
                    let mut row = row;
                    if row.get("method").is_none() {
                        if let Some(id) = row.get("id").map(ToString::to_string) {
                            active.remove(&id);
                            if catalog_requests.remove(&id) {
                                if let Some(result) = row.get_mut("result") {
                                    tools.filter_catalog(result);
                                }
                            }
                        }
                    }
                    host_writer.enqueue(row, None)?;
                }
                flushed = native_writer.flush_next("native transport write timed out"), if native_writer.has_outbound() => {
                    match flushed {
                        Ok(Some(id)) => {
                            active.remove(&id);
                        }
                        Ok(None) => {}
                        Err(error) => {
                            write_failed = true;
                            bail!("native transport write failed; outcome unknown and not replayed: {error:#}");
                        }
                    }
                }
                flushed = host_writer.flush_next("client transport write timed out"), if host_writer.has_outbound() => {
                    flushed?;
                }
                _ = &mut stop => {
                    if native_writer.has_active_write() {
                        write_failed = true;
                        bail!("native action interrupted during write; outcome unknown and not replayed");
                    }
                    if host_writer.has_active_write() {
                        bail!("native action interrupted during client write; client stream may be truncated");
                    }
                    return Ok("Interrupt");
                }
            }
        }
    }
    .await;

    let event = outcome.as_ref().copied().unwrap_or("Interrupt");
    let cleanup = cleanup_session(
        native,
        write_failed,
        active.into_values().map(|(id, _)| id),
        executing,
        tools,
        event,
    )
    .await;
    let result = outcome.and_then(|event| {
        if event == "Interrupt" {
            bail!("native action interrupted; outcome unknown and not replayed");
        }
        Ok(())
    });
    finish(result, cleanup)
}

struct Worker<R, W> {
    native: JsonTransport<R, W>,
    process: Option<OwnedProcess>,
    protocol: Option<String>,
    session: String,
    turn: String,
    next_id: u64,
    last_context: Option<Value>,
    active_request: Option<Value>,
    write_failed: bool,
    tool_policy: ToolPolicy,
    startup_timeout: Duration,
    tool_timeout: Duration,
}

impl Worker<tokio::process::ChildStdout, tokio::process::ChildStdin> {
    async fn start(launch: &Launch, client_name: &str) -> Result<Self> {
        let startup_timeout = launch.startup_timeout()?;
        let tool_timeout = launch.tool_timeout()?;
        let tool_policy = launch.tool_policy();
        let (process, output, input) = OwnedProcess::spawn(launch)?;
        let mut worker = Self::from_transport(
            JsonTransport::new(output, input),
            Some(process),
            tool_policy,
            startup_timeout,
            tool_timeout,
        );
        let initialized = tokio::select! {
            result = tokio::time::timeout(startup_timeout, worker.initialize(client_name)) =>
                result.context("native startup timed out").and_then(|r| r),
            _ = shutdown_signal() => Err(anyhow::anyhow!("native startup interrupted")),
        };
        if let Err(error) = initialized {
            let Worker {
                native,
                mut process,
                ..
            } = worker;
            drop(native);
            if let Some(mut process) = process.take() {
                if let Err(cleanup) = process.close().await {
                    bail!("{error:#}; native startup cleanup failed: {cleanup:#}");
                }
            }
            return Err(error);
        }
        Ok(worker)
    }
}

impl<R: AsyncRead + Unpin, W: AsyncWrite + Unpin> Worker<R, W> {
    fn from_transport(
        native: JsonTransport<R, W>,
        process: Option<OwnedProcess>,
        tool_policy: ToolPolicy,
        startup_timeout: Duration,
        tool_timeout: Duration,
    ) -> Self {
        Self {
            native,
            process,
            protocol: None,
            session: Uuid::new_v4().to_string(),
            turn: Uuid::new_v4().to_string(),
            next_id: 0,
            last_context: None,
            active_request: None,
            write_failed: false,
            tool_policy,
            startup_timeout,
            tool_timeout,
        }
    }

    async fn initialize(&mut self, client_name: &str) -> Result<()> {
        let result = self
            .request(
                "initialize",
                json!({
                    "protocolVersion": CLIENT_PROTOCOL_VERSION,
                    "capabilities": {},
                    "clientInfo": {
                        "name": client_name,
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }),
            )
            .await?;
        self.protocol = result
            .get("protocolVersion")
            .and_then(Value::as_str)
            .map(ToString::to_string);
        self.write_failed = true;
        self.native
            .send(json!({
                "jsonrpc": "2.0",
                "method": "notifications/initialized"
            }))
            .await?;
        self.write_failed = false;
        Ok(())
    }

    async fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        self.next_id += 1;
        let id = json!(self.next_id);
        self.active_request = Some(id.clone());
        let frame = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });
        self.write_failed = true;
        self.native.send(frame).await?;
        self.write_failed = false;

        loop {
            let row = self
                .native
                .receive()
                .await?
                .context("native service closed connection")?;
            if let Some(req_method) = row.get("method").and_then(Value::as_str) {
                if let Some(req_id) = row.get("id").cloned() {
                    let reply = if req_method == "ping" {
                        json!({"jsonrpc": "2.0", "id": req_id, "result": {}})
                    } else if req_method == "elicitation/create" {
                        json!({"jsonrpc": "2.0", "id": req_id, "result": {"action": "cancel"}})
                    } else {
                        json!({
                            "jsonrpc": "2.0",
                            "id": req_id,
                            "error": {
                                "code": -32603,
                                "message": "native request requires an interactive MCP host; CLI did not approve this application"
                            }
                        })
                    };
                    self.write_failed = true;
                    self.native.send(reply).await?;
                    self.write_failed = false;
                }
                continue;
            }
            if row.get("id") == Some(&id) {
                self.active_request = None;
                if let Some(error) = row.get("error") {
                    if let Some(message) = error.get("message").and_then(Value::as_str) {
                        bail!("{message}");
                    }
                    bail!("native MCP error: {error}");
                }
                return Ok(row.get("result").cloned().unwrap_or(Value::Null));
            }
        }
    }

    async fn tool(&mut self, name: &str, arguments: Value) -> Result<Value> {
        if !self.tool_policy.allows(name) {
            bail!("native tool '{name}' is disabled by launch configuration");
        }
        let mut params = json!({"name": name, "arguments": arguments});
        prepare_call_meta(
            &mut params,
            &mut self.last_context,
            &self.session,
            &self.turn,
            name,
        )?;
        tokio::time::timeout(self.tool_timeout, self.request("tools/call", params))
            .await
            .context("native tool timed out; outcome unknown and not replayed")?
    }

    async fn catalog(&mut self) -> Result<Vec<Value>> {
        let timeout = self.startup_timeout;
        let collect = async {
            let mut tools = Vec::new();
            let mut listed_count = 0usize;
            let mut cursor = None::<String>;
            let mut seen_cursors = BTreeSet::new();
            for _ in 0..MAX_NATIVE_CATALOG_PAGES {
                let params = cursor
                    .as_ref()
                    .map(|cursor| json!({"cursor": cursor}))
                    .unwrap_or_else(|| json!({}));
                let mut result = self.request("tools/list", params).await?;
                let page_len = result
                    .get("tools")
                    .and_then(Value::as_array)
                    .context("native tools catalog missing")?
                    .len();
                if page_len > MAX_NATIVE_CATALOG_ITEMS.saturating_sub(listed_count) {
                    bail!("native tools/list exceeded the catalog limit of {MAX_NATIVE_CATALOG_ITEMS} items");
                }
                listed_count += page_len;
                self.tool_policy.filter_catalog(&mut result);
                let page = result
                    .get("tools")
                    .and_then(Value::as_array)
                    .context("native tools catalog missing")?;
                tools.extend(page.iter().cloned());

                let next_cursor = match result.get("nextCursor") {
                    None | Some(Value::Null) => None,
                    Some(Value::String(cursor)) => Some(cursor.clone()),
                    Some(_) => bail!("native tools/list returned a non-string cursor"),
                };
                let Some(next_cursor) = next_cursor else {
                    return Ok(tools);
                };
                if next_cursor.len() > MAX_NATIVE_CATALOG_CURSOR_BYTES {
                    bail!("native tools/list returned a pagination cursor exceeding {MAX_NATIVE_CATALOG_CURSOR_BYTES} bytes");
                }
                if !seen_cursors.insert(next_cursor.clone()) {
                    bail!("native tools/list returned a repeated pagination cursor");
                }
                cursor = Some(next_cursor);
            }
            bail!("native tools/list exceeded the pagination limit of {MAX_NATIVE_CATALOG_PAGES} pages")
        };
        tokio::time::timeout(timeout, collect)
            .await
            .context("native catalog pagination timed out")?
    }

    async fn close(mut self, event: &str) -> Result<()> {
        let cleanup = cleanup_session(
            &mut self.native,
            self.write_failed,
            self.active_request.take(),
            self.last_context.take(),
            &self.tool_policy,
            event,
        )
        .await;
        drop(self.native);
        let terminated = match self.process.as_mut() {
            Some(process) => process.close().await,
            None => Ok(()),
        };
        finish(cleanup, terminated).context("native cleanup was not confirmed")
    }
}

fn parse_context(value: &Value) -> Result<Value> {
    let context = if let Value::String(text) = value {
        serde_json::from_str::<Value>(text)?
    } else {
        value.clone()
    };
    for key in ["session_id", "turn_id"] {
        if context
            .get(key)
            .and_then(Value::as_str)
            .is_none_or(|v| v.trim().is_empty())
        {
            bail!("native turn metadata requires nonempty {key}");
        }
    }
    Ok(context)
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        if let Ok(mut term) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = term.recv() => {} }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}

pub async fn run_doctor(launch: Launch) -> Result<()> {
    let mut worker = Worker::start(&launch, "codex-browser-bridge/doctor").await?;
    let tools = tokio::select! {
        result = worker.catalog() => result,
        _ = shutdown_signal() => Err(anyhow::anyhow!("native doctor interrupted")),
    };
    let protocol = worker.protocol.clone();
    let cleanup = worker.close("Stop").await;
    let tools = finish(tools, cleanup)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "launch": launch.summary(),
            "protocol": protocol,
            "tools": tools.iter().map(|tool| tool["name"].clone()).collect::<Vec<_>>(),
            "status": "native_mcp_connected",
            "cleanup_confirmed": true
        }))?
    );
    Ok(())
}

pub async fn run_cli(
    launch: Launch,
    command: crate::cli::Command,
    image_dir: Option<&Path>,
) -> Result<()> {
    let mut worker = Worker::start(&launch, "codex-browser-bridge/cli").await?;
    let run = async {
        match command {
            crate::cli::Command::Tools { name } => {
                let tools = worker.catalog().await?;
                let output = if let Some(name) = name {
                    tools
                        .iter()
                        .find(|t| t["name"] == name)
                        .cloned()
                        .context("native tool unavailable")?
                } else {
                    Value::Array(tools)
                };
                println!("{}", serde_json::to_string(&output)?);
            }
            crate::cli::Command::Call { name, arguments } => {
                if name.trim().is_empty() {
                    bail!("native call requires a nonempty tool name");
                }
                let parsed: Value =
                    serde_json::from_str(&arguments).context("invalid arguments JSON")?;
                if !parsed.is_object() {
                    bail!("arguments must be a JSON object");
                }
                print_tool_result(worker.tool(&name, parsed).await?, image_dir)?;
            }
            crate::cli::Command::Batch => {
                let mut reader = BufReader::new(tokio::io::stdin());
                let mut line_number = 0usize;
                loop {
                    let mut line = Vec::new();
                    let bytes =
                        tokio::io::AsyncReadExt::take(&mut reader, (MAX_CLI_LINE_BYTES + 1) as u64)
                            .read_until(b'\n', &mut line)
                            .await?;
                    if bytes == 0 {
                        break;
                    }
                    line_number += 1;
                    if line.len() > MAX_CLI_LINE_BYTES {
                        bail!("native batch line {line_number} exceeds 1 MiB");
                    }
                    if line.iter().all(u8::is_ascii_whitespace) {
                        continue;
                    }
                    let call: Value = serde_json::from_slice(&line)
                        .with_context(|| format!("invalid batch JSON at line {line_number}"))?;
                    let name = call
                        .get("name")
                        .and_then(Value::as_str)
                        .filter(|name| !name.trim().is_empty())
                        .with_context(|| format!("batch name missing at line {line_number}"))?;
                    let arguments = call.get("arguments").cloned().unwrap_or_else(|| json!({}));
                    if !arguments.is_object() {
                        bail!("batch arguments must be a JSON object at line {line_number}");
                    }
                    print_tool_result(
                        worker
                            .tool(name, arguments)
                            .await
                            .with_context(|| format!("batch stopped at line {line_number}"))?,
                        image_dir,
                    )?;
                }
            }
        }
        Ok::<_, anyhow::Error>(())
    };
    let (result, event) = tokio::select! {
        result = run => {
            let event = if result.is_ok() { "Stop" } else { "Interrupt" };
            (result, event)
        },
        _ = shutdown_signal() => (
            Err(anyhow::anyhow!("native action interrupted; outcome unknown and not replayed")),
            "Interrupt"
        ),
    };
    let cleanup = worker.close(event).await;
    finish(result, cleanup)
}

fn save_images(value: &Value, directory: &Path) -> Result<Vec<Value>> {
    use base64::Engine;
    std::fs::create_dir_all(directory)?;
    let mut artifacts = Vec::new();
    for (index, content) in value
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        if content["type"] != "image" {
            continue;
        }
        let mime = content["mimeType"]
            .as_str()
            .context("native image MIME missing")?;
        let extension = match mime {
            "image/png" => "png",
            "image/jpeg" => "jpg",
            "image/webp" => "webp",
            _ => bail!("native image format cannot be saved: {mime}"),
        };
        let bytes = base64::engine::general_purpose::STANDARD.decode(
            content["data"]
                .as_str()
                .context("native image data missing")?,
        )?;
        let path = directory.join(format!("native-{}-{index}.{extension}", Uuid::new_v4()));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)?;
        std::io::Write::write_all(&mut file, &bytes)?;
        artifacts.push(json!({
            "content_index": index,
            "path": path,
            "mimeType": mime,
            "bytes": bytes.len(),
            "_meta": content.get("_meta")
        }));
    }
    Ok(artifacts)
}

fn print_tool_result(mut value: Value, image_dir: Option<&Path>) -> Result<()> {
    // Preserve native image data and metadata instead of the compact browser
    // CLI's summaries. The receiving shell may write image blocks to files.
    if let Some(directory) = image_dir {
        let artifacts = save_images(&value, directory)?;
        value
            .as_object_mut()
            .context("native result is not an object")?
            .insert("bridge_artifacts".into(), Value::Array(artifacts));
    }
    println!("{}", serde_json::to_string(&value)?);
    if value.get("isError") == Some(&Value::Bool(true)) {
        bail!("native tool failed");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn native_launch_preserves_host_policy_environment_and_working_directory() {
        let directory = tempfile::tempdir().unwrap();
        let executable = std::env::current_exe().unwrap();
        let services = json!({
            "browser":"@oai/browser-desktop/service",
            "sky":"@installed/sky/service",
            "host":"@installed/host/service"
        });
        let configuration = json!({"mcpServers":{"cua_repl":{
            "command":executable,"args":["native-entry.mjs"],"cwd":directory.path(),
            "env":{
                "CUA_REPL_NODE_REPL_PATH":executable,
                "CUA_REPL_ENABLED_SURFACES":"browser,computer",
                "NODE_REPL_TRUSTED_SERVICES":services.to_string(),
                "NATIVE_SANDBOX_CONTEXT":"host-owned"
            },
            "env_vars":[
                "HOST_FORWARDED_ENV",
                {"name":"HOST_LOCAL_FORWARDED_ENV","source":"local"}
            ],
            "startup_timeout_ms":4500,
            "tool_timeout_sec":12.5,
            "enabled_tools":["js","turn_ended"],
            "disabled_tools":["js_reset"]
        }}});
        let path = directory.path().join("native.json");
        std::fs::write(&path, configuration.to_string()).unwrap();
        let launch = Launch::read(NativeServer::Computer, &path).unwrap();
        assert_eq!(launch.command, executable.to_string_lossy());
        assert_eq!(launch.args, ["native-entry.mjs"]);
        assert_eq!(launch.cwd.as_deref(), Some(directory.path()));
        assert_eq!(launch.env["NATIVE_SANDBOX_CONTEXT"], "host-owned");
        assert_eq!(launch.env["CUA_REPL_ENABLED_SURFACES"], "browser,computer");
        let selected: Value =
            serde_json::from_str(&launch.env["NODE_REPL_TRUSTED_SERVICES"]).unwrap();
        assert_eq!(selected, services);
        assert_eq!(launch.env_vars.len(), 2);
        assert_eq!(
            launch.startup_timeout().unwrap(),
            Duration::from_millis(4500)
        );
        assert_eq!(launch.tool_timeout().unwrap(), Duration::from_millis(12500));
        let policy = launch.tool_policy();
        assert!(policy.allows("js"));
        assert!(policy.allows("turn_ended"));
        assert!(!policy.allows("js_reset"));
        assert!(!policy.allows("js_add_node_module_dir"));
    }

    #[test]
    fn tool_timeout_defaults_to_codex_cli_values() {
        let directory = tempfile::tempdir().unwrap();
        let configuration = json!({"mcpServers":{"cua_repl":{
            "command":std::env::current_exe().unwrap(),
            "env":{"CUA_REPL_NODE_REPL_PATH":std::env::current_exe().unwrap()}
        }}});
        let path = directory.path().join("native.json");
        std::fs::write(&path, configuration.to_string()).unwrap();
        let launch = Launch::read(NativeServer::Computer, &path).unwrap();
        assert_eq!(launch.startup_timeout().unwrap(), Duration::from_secs(30));
        assert_eq!(launch.tool_timeout().unwrap(), Duration::from_secs(300));
    }

    #[test]
    fn native_launch_rejects_invalid_absolute_command_and_timeouts() {
        let directory = tempfile::tempdir().unwrap();
        let executable = std::env::current_exe().unwrap();
        for (name, row, expected) in [
            (
                "missing-command",
                json!({
                    "command":directory.path().join("missing.exe"),
                    "env":{"CUA_REPL_NODE_REPL_PATH":executable}
                }),
                "installed executable",
            ),
            (
                "negative-timeout",
                json!({
                    "command":executable,
                    "env":{"CUA_REPL_NODE_REPL_PATH":executable},
                    "tool_timeout_sec":-1
                }),
                "finite and nonnegative",
            ),
            (
                "disabled",
                json!({
                    "command":executable,
                    "env":{"CUA_REPL_NODE_REPL_PATH":executable},
                    "enabled":false
                }),
                "disabled",
            ),
        ] {
            let path = directory.path().join(format!("{name}.json"));
            std::fs::write(&path, json!({"mcpServers":{"cua_repl":row}}).to_string()).unwrap();
            let error = Launch::read(NativeServer::Computer, &path).unwrap_err();
            assert!(error.to_string().contains(expected), "{name}: {error:#}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn windows_program_resolution_uses_constructed_path() {
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("native-probe.exe");
        std::fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
        let environment = BTreeMap::from([(
            OsString::from("PATH"),
            directory.path().as_os_str().to_os_string(),
        )]);
        let resolved = resolve_program(
            "native-probe",
            &environment,
            &std::env::current_dir().unwrap(),
        );
        assert_eq!(
            std::fs::canonicalize(resolved).unwrap(),
            std::fs::canonicalize(executable).unwrap()
        );

        let empty = tempfile::tempdir().unwrap();
        let environment = BTreeMap::from([(
            OsString::from("PATH"),
            empty.path().as_os_str().to_os_string(),
        )]);
        assert_eq!(
            resolve_program(
                "native-probe",
                &environment,
                &std::env::current_dir().unwrap()
            ),
            OsString::from("native-probe")
        );
    }

    #[test]
    fn parse_context_accepts_object_and_json_string_forms() {
        for value in [
            json!({"session_id":"session","turn_id":"turn","thread_id":"thread"}),
            json!(r#"{"session_id":"session","turn_id":"turn"}"#),
        ] {
            assert_eq!(parse_context(&value).unwrap()["session_id"], "session");
        }
        let error = parse_context(&json!({"session_id":"session","turn_id":""})).unwrap_err();
        assert!(error.to_string().contains("turn_id"), "{error:#}");
    }

    #[test]
    fn native_launch_rejects_remote_env_vars_for_local_stdio() {
        let directory = tempfile::tempdir().unwrap();
        let executable = std::env::current_exe().unwrap();
        let configuration = json!({"mcpServers":{"cua_repl":{
            "command":executable,
            "env":{"CUA_REPL_NODE_REPL_PATH":executable},
            "env_vars":[{"name":"REMOTE_ONLY","source":"remote"}]
        }}});
        let path = directory.path().join("native.json");
        std::fs::write(&path, configuration.to_string()).unwrap();
        let error = Launch::read(NativeServer::Computer, &path).unwrap_err();
        assert!(error.to_string().contains("requires remote MCP stdio"));
    }

    #[test]
    fn tool_policy_filters_catalog_after_allowlist_then_denylist() {
        let policy = ToolPolicy {
            enabled: Some(BTreeSet::from(["js".to_string(), "js_reset".to_string()])),
            disabled: BTreeSet::from(["js_reset".to_string()]),
        };
        let mut catalog = json!({"tools":[
            {"name":"js"},
            {"name":"js_reset"},
            {"name":"turn_ended"}
        ],"nextCursor":"next"});
        policy.filter_catalog(&mut catalog);
        assert_eq!(
            catalog,
            json!({"tools":[{"name":"js"}],"nextCursor":"next"})
        );
    }

    async fn send<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
        transport: &mut JsonTransport<R, W>,
        row: Value,
    ) {
        tokio::time::timeout(Duration::from_secs(2), transport.send(row))
            .await
            .unwrap()
            .unwrap();
    }

    async fn receive<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(
        transport: &mut JsonTransport<R, W>,
    ) -> Value {
        tokio::time::timeout(Duration::from_secs(2), transport.receive())
            .await
            .unwrap()
            .unwrap()
            .unwrap()
    }

    fn assert_cleanup_metadata(actual: &Value, execution: &Value) {
        let mut expected = execution.clone();
        expected.as_object_mut().unwrap().remove("progressToken");
        expected["callId"] = actual["callId"].clone();
        Uuid::parse_str(actual["callId"].as_str().unwrap()).unwrap();
        assert_ne!(actual["callId"], execution["callId"]);
        assert_eq!(*actual, expected);
    }

    #[tokio::test]
    async fn relay_preserves_unknown_fields_in_known_mcp_messages() {
        let (host_io, mut client_io) = tokio::io::duplex(32768);
        let (bridge_io, fixture_io) = tokio::io::duplex(32768);
        let (hr, hw) = tokio::io::split(host_io);
        let (nr, nw) = tokio::io::split(bridge_io);
        let task = tokio::spawn(async move {
            relay(
                &mut JsonTransport::new(hr, hw),
                &mut JsonTransport::new(nr, nw),
                std::future::pending(),
                &ToolPolicy::default(),
            )
            .await
        });
        let (fr, mut fw) = tokio::io::split(fixture_io);
        let mut fixture = BufReader::new(fr);
        let initialize = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion":"2025-03-26","capabilities":{"nativeCapability":{"required":true}},
            "clientInfo":{"name":"synthetic","version":"1"},"nativeHandshake":{"keep":true}
        }});
        client_io
            .write_all(format!("{initialize}\n").as_bytes())
            .await
            .unwrap();
        let mut line = String::new();
        timeout_read_line(&mut fixture, &mut line).await;
        assert_eq!(serde_json::from_str::<Value>(&line).unwrap(), initialize);
        let initialized = json!({"jsonrpc":"2.0","id":1,"result":{
            "protocolVersion":"2025-03-26","capabilities":{"tools":{},"nativeCapability":{"required":true}},
            "serverInfo":{"name":"synthetic-native","version":"1"},
            "instructions":"Native startup instructions"
        }});
        fw.write_all(format!("{initialized}\n").as_bytes())
            .await
            .unwrap();
        let mut client = BufReader::new(client_io);
        line.clear();
        timeout_read_line(&mut client, &mut line).await;
        assert_eq!(serde_json::from_str::<Value>(&line).unwrap(), initialized);

        let list = json!({"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}});
        client
            .get_mut()
            .write_all(format!("{list}\n").as_bytes())
            .await
            .unwrap();
        line.clear();
        timeout_read_line(&mut fixture, &mut line).await;
        assert_eq!(serde_json::from_str::<Value>(&line).unwrap(), list);
        let catalog = json!({"jsonrpc":"2.0","id":2,"result":{"tools":[{
            "name":"js","description":"Native JS instructions","inputSchema":{"type":"object"},
            "nativeTool":{"keep":true},"annotations":{"nativeHint":"required"}
        }]}});
        fw.write_all(format!("{catalog}\n").as_bytes())
            .await
            .unwrap();
        line.clear();
        timeout_read_line(&mut client, &mut line).await;
        assert_eq!(serde_json::from_str::<Value>(&line).unwrap(), catalog);

        let request = json!({"jsonrpc":"2.0","id":"external","method":"tools/call",
            "nativeEnvelope":{"required":true},
            "params":{"name":"js","arguments":{"code":"synthetic"},
                "nativeTimeoutMs":5000}});
        client
            .get_mut()
            .write_all(format!("{request}\n").as_bytes())
            .await
            .unwrap();
        line.clear();
        timeout_read_line(&mut fixture, &mut line).await;
        let forwarded: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(forwarded["params"]["nativeTimeoutMs"], 5000);
        assert_eq!(forwarded["nativeEnvelope"], request["nativeEnvelope"]);
        let result = json!({"jsonrpc":"2.0","id":"external","result":{
            "content":[{"type":"text","text":"Native per-session instructions and result"}],
            "_meta":{"nativeExtension":true}
        }});
        fw.write_all(format!("{result}\n").as_bytes())
            .await
            .unwrap();
        line.clear();
        timeout_read_line(&mut client, &mut line).await;
        assert_eq!(serde_json::from_str::<Value>(&line).unwrap(), result);
        task.abort();
        let _ = task.await;
    }

    #[tokio::test]
    async fn relay_rejects_bad_call_metadata_without_closing_the_session() {
        let (host_io, client_io) = tokio::io::duplex(32768);
        let (native_io, fixture_io) = tokio::io::duplex(32768);
        let (hr, hw) = tokio::io::split(host_io);
        let (nr, nw) = tokio::io::split(native_io);
        let task = tokio::spawn(async move {
            relay(
                &mut JsonTransport::new(hr, hw),
                &mut JsonTransport::new(nr, nw),
                std::future::pending(),
                &ToolPolicy::default(),
            )
            .await
        });
        let (cr, cw) = tokio::io::split(client_io);
        let (fr, fw) = tokio::io::split(fixture_io);
        let mut client = JsonTransport::new(cr, cw);
        let mut fixture = JsonTransport::new(fr, fw);
        for meta in [
            json!([]),
            json!({"x-codex-turn-metadata":{"session_id":"s"}}),
        ] {
            send(
                &mut client,
                json!({"jsonrpc":"2.0","id":7,"method":"tools/call",
                "params":{"name":"js","arguments":{},"_meta":meta}}),
            )
            .await;
            let error = receive(&mut client).await;
            assert_eq!(error["id"], 7);
            assert_eq!(error["error"]["code"], -32602);
        }
        let ping = json!({"jsonrpc":"2.0","id":8,"method":"ping","params":{}});
        send(&mut client, ping.clone()).await;
        assert_eq!(receive(&mut fixture).await, ping);
        drop(client);
        tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn relay_filters_catalog_and_rejects_disabled_tool_calls() {
        let (host_io, client_io) = tokio::io::duplex(32768);
        let (native_io, fixture_io) = tokio::io::duplex(32768);
        let (hr, hw) = tokio::io::split(host_io);
        let (nr, nw) = tokio::io::split(native_io);
        let policy = ToolPolicy {
            enabled: Some(BTreeSet::from([
                "js".to_string(),
                "js_add_node_module_dir".to_string(),
                "js_reset".to_string(),
                "turn_ended".to_string(),
            ])),
            disabled: BTreeSet::from(["js_reset".to_string(), "turn_ended".to_string()]),
        };
        let task = tokio::spawn(async move {
            relay(
                &mut JsonTransport::new(hr, hw),
                &mut JsonTransport::new(nr, nw),
                std::future::pending(),
                &policy,
            )
            .await
        });
        let (cr, cw) = tokio::io::split(client_io);
        let (fr, fw) = tokio::io::split(fixture_io);
        let mut client = JsonTransport::new(cr, cw);
        let mut fixture = JsonTransport::new(fr, fw);

        let list = json!({"jsonrpc":"2.0","id":4,"method":"tools/list","params":{}});
        send(&mut client, list.clone()).await;
        assert_eq!(receive(&mut fixture).await, list);
        send(
            &mut fixture,
            json!({"jsonrpc":"2.0","id":4,"result":{"tools":[
                {"name":"js","inputSchema":{"type":"object"}},
                {"name":"js_reset","inputSchema":{"type":"object"}},
                {"name":"turn_ended","inputSchema":{"type":"object"}}
            ]}}),
        )
        .await;
        assert_eq!(
            receive(&mut client).await,
            json!({"jsonrpc":"2.0","id":4,"result":{"tools":[
                {"name":"js","inputSchema":{"type":"object"}}
            ]}})
        );

        send(
            &mut client,
            json!({"jsonrpc":"2.0","id":5,"method":"tools/call",
                "params":{"name":"js_reset","arguments":{}}}),
        )
        .await;
        let error = receive(&mut client).await;
        assert_eq!(error["id"], 5);
        assert_eq!(error["error"]["code"], -32602);
        assert!(error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("disabled"));

        send(
            &mut client,
            json!({"jsonrpc":"2.0","method":"tools/call","params":{"name":"js_reset"}}),
        )
        .await;
        send(
            &mut client,
            json!({"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":false}}),
        )
        .await;
        let malformed = receive(&mut client).await;
        assert_eq!(malformed["id"], 6);
        assert_eq!(malformed["error"]["code"], -32602);

        send(
            &mut client,
            json!({"jsonrpc":"2.0","id":"call-2","method":"tools/call",
            "params":{"name":"js_add_node_module_dir","arguments":{"path":"/tmp"},
                "_meta":{"x-codex-turn-metadata":{
                    "session_id":"session","thread_id":"thread","turn_id":"turn"
                }}}}),
        )
        .await;
        let forwarded = receive(&mut fixture).await;
        assert_eq!(forwarded["params"]["name"], "js_add_node_module_dir");
        Uuid::parse_str(forwarded["params"]["_meta"]["callId"].as_str().unwrap()).unwrap();
        assert_eq!(forwarded["params"]["_meta"]["sessionId"], "session");
        assert_eq!(forwarded["params"]["_meta"]["threadId"], "thread");
        send(
            &mut fixture,
            json!({"jsonrpc":"2.0","id":"call-2","result":{"content":[]}}),
        )
        .await;
        assert_eq!(receive(&mut client).await["id"], "call-2");

        drop(client);
        tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn relay_skips_disabled_turn_ended_cleanup() {
        let (host_io, client_io) = tokio::io::duplex(32768);
        let (native_io, fixture_io) = tokio::io::duplex(32768);
        let (hr, hw) = tokio::io::split(host_io);
        let (nr, nw) = tokio::io::split(native_io);
        let policy = ToolPolicy {
            enabled: Some(BTreeSet::from(["js".to_string(), "turn_ended".to_string()])),
            disabled: BTreeSet::from(["turn_ended".to_string()]),
        };
        let task = tokio::spawn(async move {
            relay(
                &mut JsonTransport::new(hr, hw),
                &mut JsonTransport::new(nr, nw),
                std::future::pending(),
                &policy,
            )
            .await
        });
        let (cr, cw) = tokio::io::split(client_io);
        let (fr, fw) = tokio::io::split(fixture_io);
        let mut client = JsonTransport::new(cr, cw);
        let mut fixture = JsonTransport::new(fr, fw);
        send(
            &mut client,
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{"name":"js","arguments":{"code":"synthetic"},
                "_meta":{"x-codex-turn-metadata":{
                    "session_id":"session","turn_id":"turn"
                }}}}),
        )
        .await;
        assert_eq!(receive(&mut fixture).await["params"]["name"], "js");
        send(
            &mut fixture,
            json!({"jsonrpc":"2.0","id":1,"result":{"content":[]}}),
        )
        .await;
        assert_eq!(receive(&mut client).await["id"], 1);

        drop(client);
        let row = tokio::time::timeout(Duration::from_secs(2), fixture.receive())
            .await
            .unwrap()
            .unwrap();
        assert!(
            row.is_none(),
            "disabled turn_ended cleanup was forwarded to native"
        );
        tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn relay_keeps_a_partial_input_line_while_forwarding_native_events() {
        let (host_io, mut client_io) = tokio::io::duplex(32768);
        let (native_io, fixture_io) = tokio::io::duplex(32768);
        let (hr, hw) = tokio::io::split(host_io);
        let (nr, nw) = tokio::io::split(native_io);
        let task = tokio::spawn(async move {
            relay(
                &mut JsonTransport::new(hr, hw),
                &mut JsonTransport::new(nr, nw),
                std::future::pending(),
                &ToolPolicy::default(),
            )
            .await
        });
        let (fr, fw) = tokio::io::split(fixture_io);
        let mut fixture = JsonTransport::new(fr, fw);
        client_io.write_all(b"{\"jsonrpc\":\"2.0\",").await.unwrap();
        let event =
            json!({"jsonrpc":"2.0","method":"notifications/native-event","params":{"keep":true}});
        send(&mut fixture, event.clone()).await;
        let mut client = BufReader::new(client_io);
        let mut line = String::new();
        timeout_read_line(&mut client, &mut line).await;
        assert_eq!(serde_json::from_str::<Value>(&line).unwrap(), event);
        client
            .get_mut()
            .write_all(b"\"id\":3,\"method\":\"ping\"}\n")
            .await
            .unwrap();
        assert_eq!(
            receive(&mut fixture).await,
            json!({"jsonrpc":"2.0","id":3,"method":"ping"})
        );
        drop(client);
        tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn relay_reports_interruption_of_a_partial_client_write_as_an_error() {
        let (host_io, _client_io) = tokio::io::duplex(8);
        let (native_io, fixture_io) = tokio::io::duplex(32768);
        let (hr, hw) = tokio::io::split(host_io);
        let (nr, nw) = tokio::io::split(native_io);
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            relay(
                &mut JsonTransport::new(hr, hw),
                &mut JsonTransport::new(nr, nw),
                async {
                    let _ = stop_rx.await;
                },
                &ToolPolicy::default(),
            )
            .await
        });
        let (fr, fw) = tokio::io::split(fixture_io);
        let mut fixture = JsonTransport::new(fr, fw);
        send(
            &mut fixture,
            json!({"jsonrpc":"2.0","id":1,"result":{"native": "x".repeat(4096)}}),
        )
        .await;
        tokio::time::sleep(Duration::from_millis(20)).await;
        stop_tx.send(()).unwrap();
        let error = tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err();
        assert!(error.to_string().contains("client stream may be truncated"));
    }

    async fn timeout_read_line<R: tokio::io::AsyncRead + Unpin>(
        reader: &mut BufReader<R>,
        line: &mut String,
    ) {
        tokio::time::timeout(Duration::from_secs(2), reader.read_line(line))
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn relay_keeps_approval_progress_extensions_and_execution_context() {
        for action in ["accept", "decline", "cancel"] {
            let (host_io, client_io) = tokio::io::duplex(32768);
            let (bridge_io, fixture_io) = tokio::io::duplex(32768);
            let (hr, hw) = tokio::io::split(host_io);
            let (nr, nw) = tokio::io::split(bridge_io);
            let task = tokio::spawn(async move {
                relay(
                    &mut JsonTransport::new(hr, hw),
                    &mut JsonTransport::new(nr, nw),
                    std::future::pending(),
                    &ToolPolicy::default(),
                )
                .await
            });
            let (cr, cw) = tokio::io::split(client_io);
            let (fr, fw) = tokio::io::split(fixture_io);
            let mut client = JsonTransport::new(cr, cw);
            let mut fixture = JsonTransport::new(fr, fw);
            let meta = json!({"sessionId":"external-session","threadId":"external-thread",
                "windowId":"external-window","callId":"external-call","progressToken":"host-progress",
                "x-codex-turn-metadata":{"session_id":"actual-session","turn_id":"actual-turn"},
                "native_extension":{"opaque":true}});
            let call = json!({"jsonrpc":"2.0","id":"host-request","method":"tools/call",
                "params":{"name":"js","arguments":{"code":"synthetic"},"_meta":meta}});
            send(&mut client, call.clone()).await;
            assert_eq!(receive(&mut fixture).await, call);
            let progress = json!({"jsonrpc":"2.0","method":"notifications/progress",
                "params":{"progressToken":"host-progress","progress":1.0,"total":2.0,
                "_meta":{"native_event":"preserved"}}});
            send(&mut fixture, progress.clone()).await;
            assert_eq!(receive(&mut client).await, progress);
            let approval = json!({"jsonrpc":"2.0","id":"native-request","method":"elicitation/create",
                "params":{"mode":"form","message":"Synthetic approval",
                "requestedSchema":{"type":"object","properties":{}},
                "_meta":{"persist":["session","always"],"connector_id":"computer-use"}}});
            send(&mut fixture, approval.clone()).await;
            assert_eq!(receive(&mut client).await, approval);
            let decision = json!({"jsonrpc":"2.0","id":"native-request",
                "result":{"action":action,"content":{},"_meta":{"persist":"session"}}});
            send(&mut client, decision.clone()).await;
            assert_eq!(receive(&mut fixture).await, decision);
            let event = json!({"jsonrpc":"2.0","method":"notifications/native-event",
                "params":{"payload":[1,2],"_meta":{"sequence":12}}});
            send(&mut fixture, event.clone()).await;
            assert_eq!(receive(&mut client).await, event);
            send(&mut client, event.clone()).await;
            assert_eq!(receive(&mut fixture).await, event);
            let result = json!({"jsonrpc":"2.0","id":"host-request","result":{
                "content":[{"type":"image","mimeType":"image/png","data":"aGVsbG8=",
                "_meta":{"screenshotId":"native-region"}}],"_meta":{"result":"native"}}});
            send(&mut fixture, result.clone()).await;
            assert_eq!(receive(&mut client).await, result);
            let catalog = json!({"jsonrpc":"2.0","id":4,"method":"tools/list","params":{}});
            send(&mut client, catalog.clone()).await;
            assert_eq!(receive(&mut fixture).await, catalog);
            drop(client);
            let pending_catalog = receive(&mut fixture).await;
            assert_eq!(pending_catalog["method"], "notifications/cancelled");
            assert_eq!(pending_catalog["params"]["requestId"], 4);
            let cleanup = receive(&mut fixture).await;
            assert_eq!(cleanup["params"]["name"], "turn_ended");
            assert_eq!(
                cleanup["params"]["arguments"],
                json!({"hook_event_name":"Stop","session_id":"actual-session","turn_id":"actual-turn"})
            );
            assert_cleanup_metadata(&cleanup["params"]["_meta"], &meta);
            send(
                &mut fixture,
                json!({"jsonrpc":"2.0","id":cleanup["id"],"result":{"content":[]}}),
            )
            .await;
            tokio::time::timeout(Duration::from_secs(2), task)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
        }
    }

    #[tokio::test]
    async fn relay_external_cancellation_is_not_repeated_at_eof() {
        let (host_io, client_io) = tokio::io::duplex(32768);
        let (native_io, fixture_io) = tokio::io::duplex(32768);
        let (hr, hw) = tokio::io::split(host_io);
        let (nr, nw) = tokio::io::split(native_io);
        let task = tokio::spawn(async move {
            relay(
                &mut JsonTransport::new(hr, hw),
                &mut JsonTransport::new(nr, nw),
                std::future::pending(),
                &ToolPolicy::default(),
            )
            .await
        });
        let (cr, cw) = tokio::io::split(client_io);
        let (fr, fw) = tokio::io::split(fixture_io);
        let mut client = JsonTransport::new(cr, cw);
        let mut fixture = JsonTransport::new(fr, fw);
        send(
            &mut client,
            json!({"jsonrpc":"2.0","id":42,"method":"tools/call",
            "params":{"name":"js","arguments":{}}}),
        )
        .await;
        let call = receive(&mut fixture).await;
        let cancel = json!({"jsonrpc":"2.0","method":"notifications/cancelled",
            "params":{"requestId":42,"reason":"host stopped","_meta":{"native":"context"}}});
        send(&mut client, cancel.clone()).await;
        assert_eq!(receive(&mut fixture).await, cancel);
        drop(client);
        let cleanup = receive(&mut fixture).await;
        assert_eq!(cleanup["params"]["name"], "turn_ended");
        assert_cleanup_metadata(&cleanup["params"]["_meta"], &call["params"]["_meta"]);
        send(
            &mut fixture,
            json!({"jsonrpc":"2.0","id":cleanup["id"],"result":{"content":[]}}),
        )
        .await;
        tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn relay_treats_eof_with_pending_tool_as_interrupt() {
        let (host_io, client_io) = tokio::io::duplex(32768);
        let (native_io, fixture_io) = tokio::io::duplex(32768);
        let (hr, hw) = tokio::io::split(host_io);
        let (nr, nw) = tokio::io::split(native_io);
        let task = tokio::spawn(async move {
            relay(
                &mut JsonTransport::new(hr, hw),
                &mut JsonTransport::new(nr, nw),
                std::future::pending(),
                &ToolPolicy::default(),
            )
            .await
        });
        let (cr, cw) = tokio::io::split(client_io);
        let (fr, fw) = tokio::io::split(fixture_io);
        let mut client = JsonTransport::new(cr, cw);
        let mut fixture = JsonTransport::new(fr, fw);
        send(
            &mut client,
            json!({"jsonrpc":"2.0","id":42,"method":"tools/call","params":{"name":"js"}}),
        )
        .await;
        assert_eq!(receive(&mut fixture).await["id"], 42);
        drop(client);
        let cancel = receive(&mut fixture).await;
        assert_eq!(cancel["params"]["requestId"], 42);
        let cleanup = receive(&mut fixture).await;
        assert_eq!(
            cleanup["params"]["arguments"]["hook_event_name"],
            "Interrupt"
        );
        send(
            &mut fixture,
            json!({"jsonrpc":"2.0","id":"ping","method":"ping"}),
        )
        .await;
        assert_eq!(receive(&mut fixture).await["result"], json!({}));
        send(
            &mut fixture,
            json!({"jsonrpc":"2.0","id":cleanup["id"],"result":{"content":[]}}),
        )
        .await;
        let error = tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err();
        assert!(error
            .to_string()
            .contains("outcome unknown and not replayed"));
    }

    #[tokio::test]
    async fn relay_interrupts_a_backpressured_write_and_retires_the_stream() {
        let (host_io, client_io) = tokio::io::duplex(32768);
        let (native_io, _fixture_io) = tokio::io::duplex(8);
        let (hr, hw) = tokio::io::split(host_io);
        let (nr, nw) = tokio::io::split(native_io);
        let (stop_tx, stop_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            relay(
                &mut JsonTransport::new(hr, hw),
                &mut JsonTransport::new(nr, nw),
                async {
                    let _ = stop_rx.await;
                },
                &ToolPolicy::default(),
            )
            .await
        });
        let (cr, cw) = tokio::io::split(client_io);
        let mut client = JsonTransport::new(cr, cw);
        send(
            &mut client,
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
            "params":{"name":"js","arguments":{"code":"x".repeat(16000)}}}),
        )
        .await;
        tokio::time::sleep(Duration::from_millis(20)).await;
        stop_tx.send(()).unwrap();
        let error = tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap_err();
        assert!(error.to_string().contains("unknown and not replayed"));
    }

    #[tokio::test]
    async fn relay_forwards_opposite_direction_while_one_write_is_backpressured() {
        let (host_io, client_io) = tokio::io::duplex(64);
        let (native_io, fixture_io) = tokio::io::duplex(32768);
        let (hr, hw) = tokio::io::split(host_io);
        let (nr, nw) = tokio::io::split(native_io);
        let task = tokio::spawn(async move {
            relay(
                &mut JsonTransport::new(hr, hw),
                &mut JsonTransport::new(nr, nw),
                std::future::pending(),
                &ToolPolicy::default(),
            )
            .await
        });
        let (cr, cw) = tokio::io::split(client_io);
        let (fr, fw) = tokio::io::split(fixture_io);
        let mut client = JsonTransport::new(cr, cw);
        let mut fixture = JsonTransport::new(fr, fw);

        // Native sends a large frame that saturates the 64-byte host duplex buffer.
        let large_event = json!({
            "jsonrpc": "2.0",
            "method": "notifications/native-event",
            "params": {"payload": "x".repeat(4096)}
        });
        send(&mut fixture, large_event.clone()).await;
        tokio::time::sleep(Duration::from_millis(20)).await;

        // While host write is backpressured, client -> native frames must still flow immediately.
        let cancel = json!({
            "jsonrpc": "2.0",
            "method": "notifications/cancelled",
            "params": {"requestId": 99, "reason": "user cancelled"}
        });
        send(&mut client, cancel.clone()).await;
        assert_eq!(receive(&mut fixture).await, cancel);

        // Now drain the large event from client and close cleanly.
        assert_eq!(receive(&mut client).await, large_event);
        drop(client);
        tokio::time::timeout(Duration::from_secs(2), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn noninteractive_client_never_accepts_native_application_approval() {
        let (native_io, client_io) = tokio::io::duplex(32768);
        let count = Arc::new(AtomicUsize::new(0));
        let fixture_count = count.clone();
        let fixture = tokio::spawn(async move {
            let (fr, fw) = tokio::io::split(native_io);
            let mut transport = JsonTransport::new(fr, fw);
            let init = receive(&mut transport).await;
            assert_eq!(init["method"], "initialize");
            send(
                &mut transport,
                json!({
                    "jsonrpc": "2.0",
                    "id": init["id"],
                    "result": {
                        "protocolVersion": "2025-03-26",
                        "capabilities": {"tools": {}},
                        "serverInfo": {"name": "synthetic", "version": "1"}
                    }
                }),
            )
            .await;
            let initialized = receive(&mut transport).await;
            assert_eq!(initialized["method"], "notifications/initialized");

            let call = receive(&mut transport).await;
            assert_eq!(call["method"], "tools/call");
            fixture_count.fetch_add(1, Ordering::SeqCst);

            send(
                &mut transport,
                json!({
                    "jsonrpc": "2.0",
                    "id": "approval-1",
                    "method": "elicitation/create",
                    "params": {
                        "mode": "form",
                        "message": "Synthetic app approval",
                        "requestedSchema": {"type": "object", "properties": {}},
                        "_meta": {"persist": ["session"], "connector_id": "computer-use"}
                    }
                }),
            )
            .await;
            let decision = receive(&mut transport).await;
            assert_eq!(decision["id"], "approval-1");
            assert_eq!(decision["result"]["action"], "cancel");

            send(
                &mut transport,
                json!({
                    "jsonrpc": "2.0",
                    "id": call["id"],
                    "error": {
                        "code": -32603,
                        "message": "elicitation cancelled by non-interactive client"
                    }
                }),
            )
            .await;
        });

        let (cr, cw) = tokio::io::split(client_io);
        let mut worker = Worker::from_transport(
            JsonTransport::new(cr, cw),
            None,
            ToolPolicy::default(),
            DEFAULT_START_TIMEOUT,
            DEFAULT_TOOL_TIMEOUT,
        );
        worker.initialize("synthetic-cli").await.unwrap();
        let err = worker
            .tool("js", json!({"code": "await cua.listWindows();"}))
            .await
            .unwrap_err();
        assert!(err.to_string().contains("elicitation cancelled"));
        assert_eq!(count.load(Ordering::SeqCst), 1);
        fixture.await.unwrap();
    }

    #[test]
    fn save_images_preserves_order_and_metadata() {
        let directory = tempfile::tempdir().unwrap();
        let value = json!({"content":[
            {"type":"text","text":"before"},
            {"type":"image","mimeType":"image/png","data":"aGVsbG8=","_meta":{"screenshotId":"first"}},
            {"type":"image","mimeType":"image/webp","data":"d29ybGQ=","_meta":{"screenshotId":"second"}}
        ]});
        let artifacts = save_images(&value, directory.path()).unwrap();
        assert_eq!(artifacts.len(), 2);
        assert_eq!(artifacts[0]["content_index"], 1);
        assert_eq!(artifacts[0]["_meta"]["screenshotId"], "first");
        assert_eq!(artifacts[1]["content_index"], 2);
        assert_eq!(artifacts[1]["_meta"]["screenshotId"], "second");
        let first = PathBuf::from(artifacts[0]["path"].as_str().unwrap());
        let second = PathBuf::from(artifacts[1]["path"].as_str().unwrap());
        assert_eq!(std::fs::read(first).unwrap(), b"hello");
        assert_eq!(std::fs::read(second).unwrap(), b"world");
    }

    #[tokio::test]
    async fn relay_filters_late_tools_list_response_after_cancellation() {
        let (host_client, host_bridge) = tokio::io::duplex(32768);
        let (native_bridge, native_server) = tokio::io::duplex(32768);
        let (hr, hw) = tokio::io::split(host_bridge);
        let (nr, nw) = tokio::io::split(native_bridge);
        let mut host_transport = JsonTransport::new(hr, hw);
        let mut native_transport = JsonTransport::new(nr, nw);
        let policy = ToolPolicy {
            enabled: Some(BTreeSet::from(["js".to_string()])),
            disabled: BTreeSet::new(),
        };
        let relay_task = tokio::spawn(async move {
            relay(
                &mut host_transport,
                &mut native_transport,
                std::future::pending::<()>(),
                &policy,
            )
            .await
        });

        let (hcr, hcw) = tokio::io::split(host_client);
        let mut client = JsonTransport::new(hcr, hcw);
        let (nsr, nsw) = tokio::io::split(native_server);
        let mut server = JsonTransport::new(nsr, nsw);

        send(
            &mut client,
            json!({"jsonrpc":"2.0","id":11,"method":"tools/list","params":{}}),
        )
        .await;
        assert_eq!(receive(&mut server).await["method"], "tools/list");
        send(
            &mut client,
            json!({"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":11}}),
        )
        .await;
        assert_eq!(
            receive(&mut server).await["method"],
            "notifications/cancelled"
        );
        send(
            &mut server,
            json!({
                "jsonrpc":"2.0","id":11,
                "result":{"tools":[{"name":"js"},{"name":"turn_ended"}]}
            }),
        )
        .await;
        let reply = receive(&mut client).await;
        let names: Vec<_> = reply["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert_eq!(names, vec!["js"]);
        drop(client);
        relay_task.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn relay_skips_duplicate_turn_ended_after_explicit_host_turn_ended() {
        let (host_client, host_bridge) = tokio::io::duplex(32768);
        let (native_bridge, native_server) = tokio::io::duplex(32768);
        let (hr, hw) = tokio::io::split(host_bridge);
        let (nr, nw) = tokio::io::split(native_bridge);
        let mut host_transport = JsonTransport::new(hr, hw);
        let mut native_transport = JsonTransport::new(nr, nw);
        let policy = ToolPolicy::default();
        let relay_task = tokio::spawn(async move {
            relay(
                &mut host_transport,
                &mut native_transport,
                std::future::pending::<()>(),
                &policy,
            )
            .await
        });

        let (hcr, hcw) = tokio::io::split(host_client);
        let mut client = JsonTransport::new(hcr, hcw);
        let (nsr, nsw) = tokio::io::split(native_server);
        let mut server = JsonTransport::new(nsr, nsw);

        send(
            &mut client,
            json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"js","arguments":{"code":"1+1"}}}),
        )
        .await;
        let js_req = receive(&mut server).await;
        send(
            &mut server,
            json!({"jsonrpc":"2.0","id":1,"result":{"content":[]}}),
        )
        .await;
        let _ = receive(&mut client).await;

        let ctx = js_req["params"]["_meta"]["x-codex-turn-metadata"].clone();
        send(
            &mut client,
            json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"turn_ended","arguments":{"hook_event_name":"Stop","session_id":ctx["session_id"],"turn_id":ctx["turn_id"]}}}),
        )
        .await;
        assert_eq!(receive(&mut server).await["params"]["name"], "turn_ended");
        send(
            &mut server,
            json!({"jsonrpc":"2.0","id":2,"result":{"content":[]}}),
        )
        .await;
        let _ = receive(&mut client).await;

        drop(client);
        assert!(
            tokio::time::timeout(Duration::from_millis(200), server.receive())
                .await
                .unwrap()
                .unwrap()
                .is_none(),
            "EOF must not synthesize a duplicate turn_ended"
        );
        relay_task.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn cleanup_tolerates_missing_turn_ended_tool() {
        let (native_bridge, native_server) = tokio::io::duplex(32768);
        let (nr, nw) = tokio::io::split(native_bridge);
        let mut native_transport = JsonTransport::new(nr, nw);
        let (nsr, nsw) = tokio::io::split(native_server);
        let mut server = JsonTransport::new(nsr, nsw);

        let responder = tokio::spawn(async move {
            let req = receive(&mut server).await;
            assert_eq!(req["params"]["name"], "turn_ended");
            send(
                &mut server,
                json!({"jsonrpc":"2.0","id":req["id"],"error":{"code":-32601,"message":"unknown tool turn_ended"}}),
            )
            .await;
        });

        let meta = json!({"x-codex-turn-metadata":{"session_id":"s1","turn_id":"t1"}});
        cleanup_session(
            &mut native_transport,
            false,
            Vec::new(),
            Some(meta),
            &ToolPolicy::default(),
            "Stop",
        )
        .await
        .unwrap();
        responder.await.unwrap();
    }
}
