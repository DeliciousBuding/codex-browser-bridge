//! Thin access to the installed native Computer Use MCP service.
//! No app objects, input engine, permissions, or screenshots are reimplemented.
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Mutex,
    time::Duration,
};

use anyhow::{bail, Context, Result};
use rmcp::{
    model::{
        ClientConfig, ClientRequest, ClientResult, CustomRequest, ErrorData, Implementation,
        ProtocolVersion, RequestMetaObject, ServerNotification, ServerRequest, ServerResult,
    },
    service::{NotificationContext, PeerRequestOptions, RequestContext, RunningService, Service},
    RoleClient, ServiceExt,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use uuid::Uuid;

const START_TIMEOUT: Duration = Duration::from_secs(120);
const END_TIMEOUT: Duration = Duration::from_secs(8);
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);
const MAX_CLI_LINE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Launch {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
}

impl Launch {
    /// Read the native host's launch configuration; never rewrite generated caches.
    pub fn discover(explicit: Option<&Path>) -> Result<Self> {
        if let Some(path) = explicit {
            return Self::read(path);
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
            let document: toml::Value = toml::from_str(&std::fs::read_to_string(&native_config)?)?;
            if let Some(row) = document.get("mcp_servers").and_then(|v| v.get("cua_repl")) {
                let launch: Self = serde_json::from_value(serde_json::to_value(row)?)?;
                return launch.validated();
            }
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
            return Self::read(path);
        }
        bail!("installed native cua_repl launch configuration unavailable; start the native app or pass --native-config <MCP JSON>");
    }

    pub fn read(path: &Path) -> Result<Self> {
        let document: Value = serde_json::from_str(
            &std::fs::read_to_string(path).context("read native MCP configuration")?,
        )?;
        let row = document
            .pointer("/mcpServers/cua_repl")
            .context("native configuration must contain mcpServers.cua_repl")?;
        serde_json::from_value::<Self>(row.clone())?.validated()
    }

    fn validated(mut self) -> Result<Self> {
        if !Path::new(&self.command).is_absolute() || !Path::new(&self.command).is_file() {
            bail!("native command must identify an installed absolute executable");
        }
        let repl = self
            .env
            .get("CUA_REPL_NODE_REPL_PATH")
            .context("native configuration lacks CUA_REPL_NODE_REPL_PATH")?;
        if !Path::new(repl).is_absolute() || !Path::new(repl).is_file() {
            bail!("native NodeREPL executable unavailable");
        }
        self.env
            .insert("CUA_REPL_ENABLED_SURFACES".into(), "computer".into());
        let mut services: BTreeMap<String, String> = self
            .env
            .get("NODE_REPL_TRUSTED_SERVICES")
            .map(|value| serde_json::from_str(value))
            .transpose()
            .context("native trusted services must be a JSON object of module paths")?
            .unwrap_or_default();
        services.remove("browser");
        services
            .entry("sky".into())
            .or_insert_with(|| "@oai/sky/service".into());
        self.env.insert(
            "NODE_REPL_TRUSTED_SERVICES".into(),
            serde_json::to_string(&services)?,
        );
        Ok(self)
    }

    fn command(&self) -> process_wrap::tokio::CommandWrap {
        let mut command = tokio::process::Command::new(&self.command);
        command
            .args(&self.args)
            .envs(&self.env)
            .stderr(Stdio::inherit());
        #[cfg(windows)]
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW
        if let Some(cwd) = &self.cwd {
            command.current_dir(cwd);
        }
        let mut command = process_wrap::tokio::CommandWrap::from(command);
        #[cfg(windows)]
        {
            command.wrap(process_wrap::tokio::CreationFlags(
                windows::Win32::System::Threading::CREATE_NO_WINDOW,
            ));
            command.wrap(process_wrap::tokio::JobObject);
        }
        #[cfg(unix)]
        command.wrap(process_wrap::tokio::ProcessGroup::leader());
        command.wrap(process_wrap::tokio::KillOnDrop);
        command
    }

    pub fn summary(&self) -> Value {
        json!({
            "surface":"computer","status":"launch_configuration_found",
            "command":self.command,"args":self.args,"cwd":self.cwd,
            "env_keys":self.env.keys().collect::<Vec<_>>(),
            "live_support":"not established by discovery"
        })
    }
}

#[derive(Clone)]
pub struct NativeClient {
    config: ClientConfig,
}

impl Service<RoleClient> for NativeClient {
    async fn handle_request(
        &self,
        request: ServerRequest,
        _: RequestContext<RoleClient>,
    ) -> std::result::Result<ClientResult, ErrorData> {
        if matches!(request, ServerRequest::PingRequest(_)) {
            return Ok(ClientResult::CustomResult(rmcp::model::CustomResult::new(
                json!({}),
            )));
        }
        Err(ErrorData::internal_error(
            "native request requires an interactive MCP host; CLI did not approve this application",
            None,
        ))
    }
    async fn handle_notification(
        &self,
        _: ServerNotification,
        _: NotificationContext<RoleClient>,
    ) -> std::result::Result<(), ErrorData> {
        Ok(())
    }
    fn get_info(&self) -> ClientConfig {
        self.config.clone()
    }
}

#[cfg(test)]
fn mcp_error(error: impl std::fmt::Display) -> ErrorData {
    ErrorData::internal_error(error.to_string(), None)
}

#[cfg(test)]
fn relay_error(error: rmcp::service::ServiceError) -> ErrorData {
    match error {
        rmcp::service::ServiceError::McpError(error) => error,
        error => mcp_error(error),
    }
}

pub struct Worker {
    service: RunningService<RoleClient, NativeClient>,
    process: OwnedProcess,
    session: String,
    turn: String,
    last_context: Mutex<Option<RequestMetaObject>>,
    active_request: Mutex<Option<rmcp::model::RequestId>>,
}

/// Keep process ownership outside the SDK service so cancelling a transport
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
        let mut command = launch.command();
        command
            .command_mut()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped());
        let mut child = command.spawn().context("start native cua_repl")?;
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

impl Worker {
    async fn start(launch: &Launch, mut config: ClientConfig) -> Result<Self> {
        // Negotiate the version actually supported by the installed native
        // service, independently of any outward adapter handshake.
        config.protocol_version = ProtocolVersion::V_2025_03_26;
        let (mut process, output, input) = OwnedProcess::spawn(launch)?;
        let transport = rmcp::transport::async_rw::AsyncRwTransport::new(output, input);
        let started = tokio::select! {
            result = tokio::time::timeout(START_TIMEOUT, NativeClient { config }.serve(transport)) =>
                result.context("native startup timed out").and_then(|r|r.map_err(Into::into)),
            _ = shutdown_signal() => Err(anyhow::anyhow!("native startup interrupted")),
        };
        let service = match started {
            Ok(service) => service,
            Err(error) => {
                if let Err(cleanup) = process.close().await {
                    bail!("{error:#}; native startup cleanup failed: {cleanup:#}");
                }
                return Err(error);
            }
        };
        let session = Uuid::new_v4().to_string();
        let turn = Uuid::new_v4().to_string();
        Ok(Self {
            service,
            process,
            last_context: Mutex::new(None),
            active_request: Mutex::new(None),
            session,
            turn,
        })
    }

    fn metadata(&self, mut meta: RequestMetaObject) -> Result<RequestMetaObject> {
        meta.entry("x-codex-turn-metadata").or_insert_with(|| {
            json!({
                "session_id":self.session,"turn_id":self.turn
            })
        });
        let context = parse_context(&meta["x-codex-turn-metadata"])?;
        meta.entry("sessionId")
            .or_insert_with(|| context["session_id"].clone());
        meta.entry("threadId")
            .or_insert_with(|| context["session_id"].clone());
        meta.entry("callId")
            .or_insert_with(|| Uuid::new_v4().to_string().into());
        Ok(meta)
    }

    async fn request(
        &self,
        request: ClientRequest,
        meta: RequestMetaObject,
    ) -> Result<ServerResult> {
        let meta = self.metadata(meta)?;
        let value = serde_json::to_value(&request)?;
        if value["method"] == "tools/call" && value["params"]["name"] == "js" {
            *self.last_context.lock().unwrap() = Some(meta.clone());
        }
        let handle = self
            .service
            .peer()
            .send_request_with_option(request, PeerRequestOptions::no_options().with_meta(meta))
            .await?;
        *self.active_request.lock().unwrap() = Some(handle.id.clone());
        let result = handle.await_response().await;
        self.active_request.lock().unwrap().take();
        result.map_err(Into::into)
    }

    async fn tool(&self, name: &str, arguments: Value) -> Result<Value> {
        let timeout = arguments
            .get("timeout_ms")
            .and_then(Value::as_u64)
            .map(|ms| Duration::from_millis(ms).saturating_add(START_TIMEOUT))
            .unwrap_or(START_TIMEOUT);
        let result = tokio::time::timeout(
            timeout,
            self.request(
                ClientRequest::CustomRequest(CustomRequest::new(
                    "tools/call",
                    Some(json!({"name":name,"arguments":arguments})),
                )),
                RequestMetaObject::default(),
            ),
        )
        .await
        .context("native tool timed out; outcome unknown and not replayed")??;
        Ok(serde_json::to_value(result)?)
    }

    async fn close(&mut self, event: &str) -> Result<()> {
        let active = self.active_request.lock().unwrap().take();
        if let Some(id) = active {
            let notification: rmcp::model::ClientNotification = serde_json::from_value(json!({
                "method":"notifications/cancelled","params":{"requestId":id,"reason":"client ended"}
            }))?;
            let _ = tokio::time::timeout(
                END_TIMEOUT,
                self.service.peer().send_notification(notification),
            )
            .await;
        }
        let meta = self.last_context.lock().unwrap().clone();
        let cleanup = if let Some(meta) = meta {
            let meta = cleanup_metadata(meta);
            let context = parse_context(&meta["x-codex-turn-metadata"])?;
            let (session, turn) = (&context["session_id"], &context["turn_id"]);
            tokio::time::timeout(
            END_TIMEOUT,
            self.request(ClientRequest::CustomRequest(CustomRequest::new("tools/call",Some(json!({
                "name":"turn_ended","arguments":{"hook_event_name":event,"session_id":session,"turn_id":turn}
            })))),meta),
        )
            .await
            .map(|r|r.and_then(|value| {
                if serde_json::to_value(value)?.get("isError") == Some(&Value::Bool(true)) {
                    bail!("native cleanup rejected");
                }
                Ok(())
            }))
            .unwrap_or_else(|_|Err(anyhow::anyhow!("native cleanup timed out")))
        } else {
            Ok(())
        };
        let closed = self.service.close_with_timeout(END_TIMEOUT).await;
        let terminated = self.process.close().await;
        if cleanup.is_err() || !matches!(closed, Ok(Some(_))) || terminated.is_err() {
            bail!("native cleanup was not confirmed");
        }
        Ok(())
    }
}

/// Standard MCP stdio framing with untyped JSON preserves native extensions
/// even when they extend messages already modeled by the SDK. Keep the read
/// buffer on the transport so select cancellation never discards a partial line.
struct JsonTransport<R, W> {
    reader: BufReader<R>,
    writer: W,
    buffer: Vec<u8>,
}

impl<R: AsyncRead + Unpin, W: AsyncWrite + Unpin> JsonTransport<R, W> {
    fn new(reader: R, writer: W) -> Self {
        Self {
            reader: BufReader::new(reader),
            writer,
            buffer: Vec::new(),
        }
    }

    async fn receive(&mut self) -> Result<Option<Value>> {
        let bytes = self.reader.read_until(b'\n', &mut self.buffer).await?;
        if bytes == 0 && self.buffer.is_empty() {
            return Ok(None);
        }
        let row = serde_json::from_slice(&self.buffer).context("invalid MCP JSON frame");
        self.buffer.clear();
        row.map(Some)
    }

    async fn send(&mut self, row: Value) -> Result<()> {
        let mut bytes = serde_json::to_vec(&row)?;
        bytes.push(b'\n');
        self.writer.write_all(&bytes).await?;
        self.writer.flush().await?;
        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        self.writer.shutdown().await?;
        Ok(())
    }
}

fn cleanup_metadata(mut meta: RequestMetaObject) -> RequestMetaObject {
    meta.remove("progressToken");
    meta.insert("callId".into(), Uuid::new_v4().to_string().into());
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

/// Relay native MCP frames without normalizing schemas or native extensions.
pub async fn run_mcp(launch: Launch) -> Result<()> {
    let (mut process, output, input) = OwnedProcess::spawn(&launch)?;
    let mut native = JsonTransport::new(output, input);
    let mut host = JsonTransport::new(tokio::io::stdin(), tokio::io::stdout());
    let result = relay(&mut host, &mut native, shutdown_signal()).await;
    // ChildStdin::shutdown is a no-op on supported platforms. Dropping the
    // handle delivers EOF before the process receives its graceful deadline.
    drop(native);
    let terminated = process.close().await;
    finish(result, terminated)
}

async fn relay<HR, HW, NR, NW, F>(
    host: &mut JsonTransport<HR, HW>,
    native: &mut JsonTransport<NR, NW>,
    stop: F,
) -> Result<()>
where
    HR: AsyncRead + Unpin,
    HW: AsyncWrite + Unpin,
    NR: AsyncRead + Unpin,
    NW: AsyncWrite + Unpin,
    F: std::future::Future<Output = ()>,
{
    let mut executing: Option<Value> = None;
    let mut active = BTreeMap::new();
    let mut write_failed = false;
    tokio::pin!(stop);
    let outcome=async {
        loop {
            tokio::select! {
                row=host.receive()=>{
                    let Some(mut row)=row? else{return Ok::<_,anyhow::Error>("Stop")};
                    if row.get("id").is_some() && row["method"]=="tools/call" {
                        let id=row["id"].clone();
                        if row["params"]["name"]=="js" {
                            let metadata=(|| -> Result<Value> {
                                let params=row["params"].as_object_mut().context("native call parameters missing")?;
                                let meta=params.entry("_meta").or_insert_with(||json!({})).as_object_mut().context("native metadata must be an object")?;
                                meta.entry("callId").or_insert_with(||json!(Uuid::new_v4().to_string()));
                                if !meta.contains_key("x-codex-turn-metadata") {
                                    let context=executing.as_ref().and_then(|m|m.get("x-codex-turn-metadata")).cloned()
                                        .unwrap_or_else(||json!({"session_id":Uuid::new_v4().to_string(),"turn_id":Uuid::new_v4().to_string()}));
                                    meta.insert("x-codex-turn-metadata".into(),context);
                                }
                                let context=parse_context(meta.get("x-codex-turn-metadata").unwrap())?;
                                meta.entry("sessionId").or_insert_with(||context["session_id"].clone());
                                meta.entry("threadId").or_insert_with(||context["session_id"].clone());
                                Ok(Value::Object(meta.clone()))
                            })();
                            match metadata {
                                Ok(meta)=>executing=Some(meta),
                                Err(error)=>{
                                    let reply=json!({"jsonrpc":"2.0","id":id,
                                        "error":{"code":-32602,"message":error.to_string()}});
                                    tokio::select! {
                                        result=tokio::time::timeout(WRITE_TIMEOUT,host.send(reply))=>
                                            result.context("client error response write timed out")??,
                                        _=&mut stop=>bail!("native action interrupted during client write; client stream may be truncated"),
                                    }
                                    continue;
                                }
                            }
                        }
                        active.insert(id.to_string(),id);
                    }
                    let cancelled_id = (row["method"]=="notifications/cancelled")
                        .then(||row["params"]["requestId"].to_string());
                    let sent=tokio::select! {
                        result = tokio::time::timeout(WRITE_TIMEOUT,native.send(row)) => result,
                        _ = &mut stop => {
                            write_failed=true;
                            bail!("native action interrupted during write; outcome unknown and not replayed");
                        }
                    };
                    if !matches!(sent,Ok(Ok(()))) {
                        write_failed=true;
                        bail!("native transport write failed; outcome unknown and not replayed");
                    }
                    if let Some(id)=cancelled_id {active.remove(&id);}
                }
                row=native.receive()=>{
                    let Some(row)=row? else{return Ok("Stop")};
                    if row.get("method").is_none() {
                        if let Some(id)=row.get("id"){active.remove(&id.to_string());}
                    }
                    tokio::select! {
                        result = tokio::time::timeout(WRITE_TIMEOUT,host.send(row)) =>
                            result.map_err(|_|anyhow::anyhow!("client transport write timed out"))??,
                        _ = &mut stop => {bail!("native action interrupted during client write; client stream may be truncated")}
                    }
                }
                _=&mut stop=>{return Ok("Interrupt")}
            }
        }
    }.await;
    let event = outcome.as_ref().copied().unwrap_or("Interrupt");
    let cleanup = async {
        if write_failed {
            bail!("native byte stream may be incomplete; cleanup requires process termination");
        }
        for id in active.into_values() {
            native
                .send(json!({"jsonrpc":"2.0","method":"notifications/cancelled",
                    "params":{"requestId":id,"reason":"client ended"}}))
                .await?;
        }
        if let Some(meta) = executing {
            let context = parse_context(
                meta.get("x-codex-turn-metadata")
                    .context("execution context missing")?,
            )?;
            let id = format!("bridge-cleanup-{}", Uuid::new_v4());
            let meta = cleanup_metadata(serde_json::from_value(meta)?);
            let request = json!({
                "jsonrpc":"2.0","id":id,"method":"tools/call","params":{
                "name":"turn_ended","arguments":{"hook_event_name":event,"session_id":context["session_id"],"turn_id":context["turn_id"]},"_meta":meta
            }});
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
                    bail!("native cleanup failed: {}", row["error"])
                } else if row.get("method").is_some() && row.get("id").is_some() {
                    native
                            .send(json!({"jsonrpc":"2.0","id":row["id"],"error":{
                                "code":-32603,"message":"client ended before native request could be presented"
                            }}))
                            .await?;
                }
            }
        }
        Ok::<_, anyhow::Error>(())
    };
    let cleanup = tokio::time::timeout(END_TIMEOUT, cleanup).await;
    let closed = tokio::time::timeout(END_TIMEOUT, native.close()).await;
    let cleanup = cleanup.context("native cleanup timed out").and_then(|r| r);
    let closed = closed
        .context("native transport close timed out")
        .and_then(|r| r);
    let cleanup = finish(cleanup, closed).context("native cleanup was not confirmed");
    let result = outcome.and_then(|event| {
        if event == "Interrupt" {
            bail!("native action interrupted; outcome unknown and not replayed");
        }
        Ok(())
    });
    finish(result, cleanup)
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
            tokio::select! { _=tokio::signal::ctrl_c()=>{},_=term.recv()=>{} }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}

pub async fn run_doctor(launch: Launch) -> Result<()> {
    let config = ClientConfig::new(
        Default::default(),
        Implementation::new("codex-browser-bridge/doctor", env!("CARGO_PKG_VERSION")),
    );
    let mut worker = Worker::start(&launch, config).await?;
    let catalog = tokio::select! {
        result = tokio::time::timeout(
        WRITE_TIMEOUT, worker.request(
            ClientRequest::CustomRequest(CustomRequest::new("tools/list", Some(json!({})))),
            Default::default(),
        ),
    ) => result.context("native doctor catalog timed out")
    .and_then(|result| result),
        _ = shutdown_signal() => Err(anyhow::anyhow!("native doctor interrupted")),
    };
    let protocol = worker
        .service
        .peer()
        .peer_info()
        .map(|info| info.protocol_version.clone());
    let cleanup = worker.close("Stop").await;
    let catalog = serde_json::to_value(finish(catalog, cleanup)?)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "launch":launch.summary(),"protocol":protocol,
            "tools":catalog["tools"].as_array().map(|tools|tools.iter().map(|t|t["name"].clone()).collect::<Vec<_>>()),
            "status":"native_mcp_connected","ui_actions":0,
            "cleanup_confirmed":true
        }))?
    );
    Ok(())
}

pub async fn run_cli(
    launch: Launch,
    command: crate::cli::Command,
    image_dir: Option<&Path>,
) -> Result<()> {
    let config = ClientConfig::new(
        Default::default(),
        Implementation::new("codex-browser-bridge/cli", env!("CARGO_PKG_VERSION")),
    );
    let mut worker = Worker::start(&launch, config).await?;
    let run = async {
        match command {
            crate::cli::Command::Tools { name } => {
                let result = tokio::time::timeout(
                    WRITE_TIMEOUT,
                    worker.request(
                        ClientRequest::CustomRequest(CustomRequest::new(
                            "tools/list",
                            Some(json!({})),
                        )),
                        Default::default(),
                    ),
                )
                .await
                .context("native catalog timed out")??;
                let result = serde_json::to_value(result)?;
                let tools = result
                    .get("tools")
                    .and_then(Value::as_array)
                    .context("native tools catalog missing")?;
                let output = if let Some(name) = name {
                    tools
                        .iter()
                        .find(|t| t["name"] == name)
                        .cloned()
                        .context("native tool unavailable")?
                } else {
                    Value::Array(tools.clone())
                };
                println!("{}", serde_json::to_string(&output)?);
            }
            crate::cli::Command::Call { name, arguments } => {
                print_tool_result(
                    worker
                        .tool(&name, serde_json::from_str(&arguments)?)
                        .await?,
                    image_dir,
                )?;
            }
            crate::cli::Command::Batch => {
                let mut reader = BufReader::new(tokio::io::stdin());
                loop {
                    let mut line = Vec::new();
                    let bytes =
                        tokio::io::AsyncReadExt::take(&mut reader, (MAX_CLI_LINE_BYTES + 1) as u64)
                            .read_until(b'\n', &mut line)
                            .await?;
                    if bytes == 0 {
                        break;
                    }
                    if line.len() > MAX_CLI_LINE_BYTES {
                        bail!("native batch line exceeds 1 MiB");
                    }
                    if line.iter().all(u8::is_ascii_whitespace) {
                        continue;
                    }
                    let call: Value = serde_json::from_slice(&line)?;
                    let name = call["name"].as_str().context("batch name missing")?;
                    print_tool_result(
                        worker
                            .tool(
                                name,
                                call.get("arguments").cloned().unwrap_or_else(|| json!({})),
                            )
                            .await?,
                        image_dir,
                    )?;
                }
            }
        }
        Ok::<_, anyhow::Error>(())
    };
    let (result, event) = tokio::select! {
        result = run => (result,"Stop"),
        _ = shutdown_signal() => (
            Err(anyhow::anyhow!("native action interrupted; outcome unknown and not replayed")),"Interrupt"
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
        artifacts.push(json!({"content_index":index,"path":path,"mimeType":mime,"bytes":bytes.len(),"_meta":content.get("_meta")}));
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
    use rmcp::model::{ClientNotification, CustomResult, ServerCapabilities, ServerConfig};
    use rmcp::RoleServer;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn native_launch_preserves_host_services_environment_and_working_directory() {
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
            }
        }}});
        let path = directory.path().join("native.json");
        std::fs::write(&path, configuration.to_string()).unwrap();
        let launch = Launch::read(&path).unwrap();
        assert_eq!(launch.command, executable.to_string_lossy());
        assert_eq!(launch.args, ["native-entry.mjs"]);
        assert_eq!(launch.cwd.as_deref(), Some(directory.path()));
        assert_eq!(launch.env["NATIVE_SANDBOX_CONTEXT"], "host-owned");
        assert_eq!(launch.env["CUA_REPL_ENABLED_SURFACES"], "computer");
        let selected: Value =
            serde_json::from_str(&launch.env["NODE_REPL_TRUSTED_SERVICES"]).unwrap();
        assert_eq!(selected["sky"], services["sky"]);
        assert_eq!(selected["host"], services["host"]);
        assert!(selected.get("browser").is_none());
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
        use tokio::io::{AsyncWriteExt, BufReader};
        let (host_io, mut client_io) = tokio::io::duplex(32768);
        let (bridge_io, fixture_io) = tokio::io::duplex(32768);
        let (hr, hw) = tokio::io::split(host_io);
        let (nr, nw) = tokio::io::split(bridge_io);
        let task = tokio::spawn(async move {
            relay(
                &mut JsonTransport::new(hr, hw),
                &mut JsonTransport::new(nr, nw),
                std::future::pending(),
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
        let request = json!({"jsonrpc":"2.0","id":"external","method":"tools/call",
            "nativeEnvelope":{"required":true},
            "params":{"name":"js","arguments":{"code":"synthetic"},
                "nativeTimeoutMs":5000}});
        client_io
            .write_all(format!("{request}\n").as_bytes())
            .await
            .unwrap();
        line.clear();
        timeout_read_line(&mut fixture, &mut line).await;
        let forwarded: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(forwarded["params"]["nativeTimeoutMs"], 5000);
        assert_eq!(forwarded["nativeEnvelope"], request["nativeEnvelope"]);
        let result = json!({"jsonrpc":"2.0","id":"external","result":{"tools":[{
            "name":"js","inputSchema":{"type":"object"},
            "nativeTool":{"keep":true},"annotations":{"nativeHint":"required"}
        }]}});
        fw.write_all(format!("{result}\n").as_bytes())
            .await
            .unwrap();
        let mut client = BufReader::new(client_io);
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

    struct NativeFixture {
        count: Arc<AtomicUsize>,
    }

    impl Service<RoleServer> for NativeFixture {
        async fn handle_request(
            &self,
            request: ClientRequest,
            context: RequestContext<RoleServer>,
        ) -> std::result::Result<ServerResult, ErrorData> {
            if matches!(request, ClientRequest::InitializeRequest(_)) {
                return Ok(ServerResult::InitializeResult(self.get_info()));
            }
            self.count.fetch_add(1, Ordering::SeqCst);
            let approval = ServerRequest::CustomRequest(CustomRequest::new(
                "elicitation/create",
                Some(json!({"mode":"form","message":"Synthetic app approval",
                    "requestedSchema":{"type":"object","properties":{}},
                    "_meta":{"persist":["session"],"connector_id":"computer-use"}})),
            ));
            let decision = context
                .peer
                .send_request(approval)
                .await
                .map_err(relay_error)?;
            let value = serde_json::to_value(decision).map_err(mcp_error)?;
            Ok(ServerResult::CustomResult(CustomResult::new(json!({
                "content":[{"type":"text","text":"synthetic"},{"type":"image","mimeType":"image/png","data":"aGVsbG8="}],
                "_meta":{"decision":value,"received":context.meta}
            }))))
        }
        async fn handle_notification(
            &self,
            _: ClientNotification,
            _: NotificationContext<RoleServer>,
        ) -> std::result::Result<(), ErrorData> {
            Ok(())
        }
        fn get_info(&self) -> ServerConfig {
            ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
                .with_protocol_version(ProtocolVersion::V_2025_03_26)
        }
    }

    #[tokio::test]
    async fn noninteractive_client_never_accepts_native_application_approval() {
        let (native_io, client_io) = tokio::io::duplex(32768);
        let count = Arc::new(AtomicUsize::new(0));
        let fixture = tokio::spawn(
            NativeFixture {
                count: count.clone(),
            }
            .serve(native_io),
        );
        let config = ClientConfig::default().with_protocol_version(ProtocolVersion::V_2025_03_26);
        let mut client = NativeClient { config }.serve(client_io).await.unwrap();
        let mut fixture = fixture.await.unwrap().unwrap();
        let error = client
            .peer()
            .send_request_with_option(
                ClientRequest::CustomRequest(CustomRequest::new(
                    "tools/call",
                    Some(json!({"name":"js","arguments":{}})),
                )),
                PeerRequestOptions::with_timeout(Duration::from_secs(2)),
            )
            .await
            .unwrap()
            .await_response()
            .await
            .unwrap_err();
        assert!(error.to_string().contains("CLI did not approve"));
        assert_eq!(count.load(Ordering::SeqCst), 1);
        client.close().await.unwrap();
        fixture.close().await.unwrap();
    }

    #[test]
    fn native_image_files_preserve_content_order_bytes_and_metadata() {
        let directory = tempfile::tempdir().unwrap();
        let value = json!({"content":[
            {"type":"text","text":"native output"},
            {"type":"image","mimeType":"image/png","data":"aGVsbG8=","_meta":{"screenshotId":"native-region"}},
            {"type":"image","mimeType":"image/jpeg","data":"d29ybGQ="}
        ]});
        let artifacts = save_images(&value, directory.path()).unwrap();
        assert_eq!(artifacts.len(), 2);
        assert_eq!(artifacts[0]["content_index"], 1);
        assert_eq!(artifacts[0]["_meta"]["screenshotId"], "native-region");
        assert_eq!(
            std::fs::read(artifacts[0]["path"].as_str().unwrap()).unwrap(),
            b"hello"
        );
        assert_eq!(
            std::fs::read(artifacts[1]["path"].as_str().unwrap()).unwrap(),
            b"world"
        );
        assert_eq!(value["content"][1]["data"], "aGVsbG8=");
    }
}
