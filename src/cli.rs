//! Compact shell commands and the legacy interactive debugging REPL.
//!
//! Connects to the ChatGPT browser-extension pipe and provides a simple REPL for
//! tab management, navigation, inspection, and raw RPC calls.

use crate::browser;
use crate::client::Client;
use crate::mcp::Server;
use anyhow::{bail, Context};
use clap::Subcommand;
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::{self, Write};
use std::path::Path;
use tokio::io::{AsyncBufRead, AsyncBufReadExt, BufReader};

const MAX_COMMAND_BYTES: usize = 1024 * 1024;
pub use crate::session::{clear_session, load_session, save_session};

/// Tools that create or claim a tab. A tab is bound to the process session that
/// created it, so a one-shot `call` without cleanup leaves it unowned.
const TAB_OWNING_TOOLS: &[&str] = &["codex_create_tab", "codex_claim_tab"];

#[derive(Debug, Subcommand)]
pub enum Command {
    /// List tool names, or fetch exactly one tool's MCP schema. No browser connection.
    Tools { name: Option<String> },
    /// Call an MCP tool with a JSON object, without the MCP protocol envelope.
    Call {
        name: String,
        #[arg(default_value = "{}")]
        arguments: String,
    },
    /// Read {"name":"codex_...","arguments":{}} JSONL from stdin in one session; stop on error.
    Batch,
}

pub fn parse_output_budget(value: &str) -> Result<usize, String> {
    let bytes = value
        .parse::<usize>()
        .map_err(|_| "output budget must be an integer".to_string())?;
    if !(256..=8 * 1024 * 1024).contains(&bytes) {
        return Err("output budget must be between 256 and 8388608 bytes".into());
    }
    Ok(bytes)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ToolCall {
    name: String,
    #[serde(default = "empty_arguments")]
    arguments: Value,
}

fn empty_arguments() -> Value {
    json!({})
}

/// Run commands through the MCP server's existing profile, handlers and Content limits.
/// The caller decides whether this command uses the default one-shot cleanup.
pub async fn run_command(
    server: &Server,
    command: Command,
    budget: usize,
    session_file: Option<&Path>,
    finalize: bool,
) -> anyhow::Result<()> {
    let mut reader = BufReader::new(tokio::io::stdin());
    let mut writer = io::stdout();
    run_command_with_io(
        server,
        command,
        budget,
        session_file,
        finalize,
        &mut reader,
        &mut writer,
    )
    .await
}

async fn run_command_with_io<R: AsyncBufRead + Unpin, W: Write>(
    server: &Server,
    command: Command,
    budget: usize,
    session_file: Option<&Path>,
    finalize: bool,
    reader: &mut R,
    writer: &mut W,
) -> anyhow::Result<()> {
    // Ending the turn through the tool itself replaces the implicit lifecycle
    // cleanup, and the persisted session must not outlive that turn.
    let explicit_turn_end =
        matches!(&command, Command::Call { name, .. } if name == "codex_finalize");
    // `tools` never opens a turn, so it must not dial or send `turnEnded`.
    let touches_browser = !matches!(&command, Command::Tools { .. });
    let finalize = finalize && !explicit_turn_end;
    if let Command::Call { name, .. } = &command {
        if TAB_OWNING_TOOLS.contains(&name.as_str()) {
            if finalize {
                bail!(
                    "{name} creates or claims a browser tab, and this one-shot call would close it again immediately. Use `batch` (one session, several steps), `--session-file <path>` (continue in a later command), or `--keep-turn` to deliberately leave the tab open."
                );
            }
            if session_file.is_none() {
                tracing::warn!(
                    tool = %name,
                    "the created tab stays owned by a closed session; prefer batch or --session-file"
                );
            }
        }
    }
    let result = match command {
        Command::Tools { name } => {
            let tools = server.tool_list();
            let value = if let Some(name) = name {
                tools
                    .into_iter()
                    .find(|tool| tool["name"] == name)
                    .with_context(|| format!("tool not found in active profile: {name}"))?
            } else {
                Value::Array(tools.into_iter().map(|tool| tool["name"].clone()).collect())
            };
            write_result(writer, value, budget)
        }
        Command::Call {
            name, arguments, ..
        } => match serde_json::from_str::<Value>(&arguments) {
            Ok(arguments) => {
                execute_and_write(server, ToolCall { name, arguments }, writer, budget).await
            }
            Err(err) => Err(anyhow::anyhow!("invalid arguments JSON: {err}")),
        },
        Command::Batch => run_batch(server, reader, writer, budget).await,
    };
    // Cleanup only applies to a turn this process actually opened. A rejected
    // request or an empty batch never dialed, and dialing now would turn a
    // validation error into a connection error.
    let opened_turn =
        finalize && touches_browser && server.client_snapshot().has_live_connection().await;
    let mut turn_ended = false;
    if opened_turn {
        // This goes through the same current server, including any bind_browser changes.
        // Do not short-circuit on an earlier failure: cleanup must still be attempted.
        let cleanup = call_tool(
            server,
            ToolCall {
                name: "codex_finalize".into(),
                arguments: empty_arguments(),
            },
        )
        .await;
        let cleanup = cleanup.and_then(|value| {
            if failed(&value) {
                bail!("{}", encode_result(value, budget)?);
            }
            Ok(())
        });
        if let Err(err) = cleanup {
            return match result {
                Ok(()) => Err(err.context("finalize failed")),
                Err(original) => Err(anyhow::anyhow!(
                    "{original:#}; finalize also failed: {err:#}"
                )),
            };
        }
        turn_ended = true;
    }
    if explicit_turn_end && result.is_ok() {
        turn_ended = true;
    }
    let persisted = if touches_browser {
        persist_session(server, session_file, turn_ended).await
    } else {
        Ok(())
    };
    match (result, persisted) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(original), Ok(())) => Err(original),
        (Ok(()), Err(session)) => Err(session),
        (Err(original), Err(session)) => {
            Err(anyhow::anyhow!("{original:#}; additionally, {session:#}"))
        }
    }
}

/// Save the active session for the next process, or drop it once the turn ended.
async fn persist_session(
    server: &Server,
    session_file: Option<&Path>,
    turn_ended: bool,
) -> anyhow::Result<()> {
    let Some(path) = session_file else {
        return Ok(());
    };
    if turn_ended {
        return clear_session(path);
    }
    // A command rejected before dialing must not create a session file for a
    // turn that never existed. An existing file is left untouched in that case.
    let client = server.client_snapshot();
    if !client.has_live_connection().await && !client.has_unresolved_turn() {
        return Ok(());
    }
    let browser = client
        .browser_identity()
        .context("cannot persist a turn without verified browser identity")?;
    let (session_id, turn_id) = client.session_ids().await;
    save_session(path, browser, &session_id, &turn_id)
        .with_context(|| format!("could not persist browser session file {}", path.display()))?;
    Ok(())
}

async fn call_tool(server: &Server, call: ToolCall) -> anyhow::Result<Value> {
    if call.name.trim().is_empty() || !call.arguments.is_object() {
        bail!("tool name must be nonempty and arguments must be a JSON object");
    }
    let request = json!({
        "jsonrpc": "2.0", "id": 1, "method": "tools/call",
        "params": {"name": call.name, "arguments": call.arguments}
    });
    let response = server
        .handle_jsonrpc_line(&serde_json::to_string(&request)?)
        .await
        .context("MCP tool handler returned no response")?;
    let mut response: Value = serde_json::from_str(&response)?;
    if response.get("error").is_some() {
        Ok(json!({"error": response["error"].take()}))
    } else {
        response
            .get_mut("result")
            .map(Value::take)
            .context("MCP tool handler returned no result")
    }
}

fn failed(value: &Value) -> bool {
    value.get("error").is_some() || value["isError"] == true
}

async fn execute_and_write<W: Write>(
    server: &Server,
    call: ToolCall,
    writer: &mut W,
    budget: usize,
) -> anyhow::Result<()> {
    let result = call_tool(server, call).await?;
    let is_error = failed(&result);
    write_result(writer, result, budget)?;
    if is_error {
        bail!("tool call failed (see JSON result on stdout)");
    }
    Ok(())
}

async fn run_batch<R: AsyncBufRead + Unpin, W: Write>(
    server: &Server,
    reader: &mut R,
    writer: &mut W,
    budget: usize,
) -> anyhow::Result<()> {
    let mut line = Vec::new();
    let mut line_number = 0;
    loop {
        line.clear();
        // Bound allocation before reading a potentially untrusted/accidentally huge stdin line.
        loop {
            let available = reader.fill_buf().await?;
            if available.is_empty() {
                break;
            }
            let count = available
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(available.len(), |index| index + 1);
            if line.len() + count > MAX_COMMAND_BYTES {
                bail!("batch line {} exceeds 1048576 bytes", line_number + 1);
            }
            line.extend_from_slice(&available[..count]);
            reader.consume(count);
            if line.last() == Some(&b'\n') {
                break;
            }
        }
        if line.is_empty() {
            return Ok(());
        }
        line_number += 1;
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        let call = serde_json::from_slice::<ToolCall>(&line)
            .with_context(|| format!("invalid batch JSON at line {line_number}"))?;
        execute_and_write(server, call, writer, budget)
            .await
            .with_context(|| format!("batch stopped at line {line_number}"))?;
    }
}

fn write_result<W: Write>(writer: &mut W, value: Value, budget: usize) -> anyhow::Result<()> {
    writeln!(writer, "{}", encode_result(value, budget)?)?;
    writer.flush()?;
    Ok(())
}

fn encode_result(mut value: Value, budget: usize) -> anyhow::Result<String> {
    if let Some(content) = value.get_mut("content").and_then(Value::as_array_mut) {
        for item in content {
            if item["type"] == "image" {
                let bytes = item["data"].as_str().map_or(0, str::len);
                let mime = item["mimeType"].clone();
                *item =
                    json!({"type":"image", "mimeType":mime, "base64Bytes":bytes, "omitted":true});
            } else if let Some(text) = item.get_mut("text") {
                // Handlers often pretty-print JSON for MCP readers. Avoid that whitespace in shell results.
                if let Some(compact) = text
                    .as_str()
                    .and_then(|s| serde_json::from_str::<Value>(s).ok())
                {
                    *text = Value::String(serde_json::to_string(&compact)?);
                }
            }
        }
    }
    let encoded = serde_json::to_string(&value)?;
    if encoded.len() < budget {
        return Ok(encoded);
    }
    // Keep the record valid JSON, even when truncation lands inside UTF-8 or escaped text.
    let boundaries: Vec<usize> = encoded.char_indices().map(|(index, _)| index).collect();
    let mut low = 0;
    let mut high = boundaries.len();
    while low < high {
        let mid = (low + high).div_ceil(2);
        let end = boundaries.get(mid).copied().unwrap_or(encoded.len());
        let candidate = json!({"truncated":true,"isError":failed(&value),"originalBytes":encoded.len(),"preview":&encoded[..end]});
        if serde_json::to_string(&candidate)?.len() < budget {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    let end = boundaries.get(low).copied().unwrap_or(encoded.len());
    Ok(serde_json::to_string(
        &json!({"truncated":true,"isError":failed(&value),"originalBytes":encoded.len(),"preview":&encoded[..end]}),
    )?)
}

/// Run the interactive CLI REPL on the given client connection.
pub async fn run_cli(client: Client) -> anyhow::Result<()> {
    println!("Connected to ChatGPT browser-extension pipe");
    println!("Commands: tabs, create, close <id>, user-tabs, claim <id>, nav <id> <url>,");
    println!("          snapshot <id>, screenshot <id>, info, ping, try <method>, quit");

    let stdin = io::stdin();
    let mut user_tab_snapshots = Vec::<browser::UserTab>::new();
    loop {
        print!("> ");
        io::stdout().flush()?;

        let mut line = String::new();
        if stdin.read_line(&mut line)? == 0 {
            println!("\nGoodbye.");
            return Ok(());
        }

        let command_line = line.trim_end_matches(['\r', '\n']);
        if let Some(rest) = command_line.strip_prefix("try ") {
            let (method, params) = match parse_try_command(rest) {
                Ok(command) => command,
                Err(err) => {
                    println!("{err}");
                    continue;
                }
            };
            if method.is_empty() {
                println!("Usage: try <method> [json_params]");
                continue;
            }
            match client.send_request(method, params).await {
                Ok(raw) => println!("{}", raw.get()),
                Err(err) => println!("Error: {err}"),
            }
            continue;
        }

        let args = split_args(command_line);
        if args.is_empty() {
            continue;
        }

        match args[0].as_str() {
            "tabs" => match browser::list_tabs(&client).await {
                Ok(tabs) => {
                    for tab in tabs {
                        println!("  [{}] {} - {}", tab.id, tab.title, tab.url);
                    }
                }
                Err(err) => println!("Error: {err}"),
            },
            "create" => match browser::create_tab(&client).await {
                Ok(id) => println!("Created tab: {id}"),
                Err(err) => println!("Error: {err}"),
            },
            "close" => {
                if args.len() < 2 {
                    println!("Usage: close <tab_id>");
                    continue;
                }
                match browser::close_tab(&client, &args[1]).await {
                    Ok(()) => println!("Closed tab {}", args[1]),
                    Err(err) => println!("Error: {err}"),
                }
            }
            "user-tabs" => match browser::list_user_tabs(&client).await {
                Ok(tabs) => {
                    for tab in &tabs {
                        println!(
                            "  [{}] {} - {} (group: {})",
                            tab.id, tab.title, tab.url, tab.tab_group
                        );
                    }
                    user_tab_snapshots = tabs;
                }
                Err(err) => {
                    user_tab_snapshots.clear();
                    println!("Error: {err}");
                }
            },
            "claim" => {
                if args.len() < 2 {
                    println!("Usage: claim <tab_id>");
                    continue;
                }
                let Some(snapshot) = user_tab_snapshots.iter().find(|tab| tab.id == args[1]) else {
                    println!("Run user-tabs first and choose a tab from that snapshot");
                    continue;
                };
                let result =
                    browser::claim_user_tab(&client, &snapshot.id, &snapshot.title, &snapshot.url)
                        .await;
                user_tab_snapshots.clear();
                match result {
                    Ok(tab) => println!("Claimed: [{}] {} - {}", tab.id, tab.title, tab.url),
                    Err(err) => println!("Error: {err}"),
                }
            }
            "nav" => {
                if args.len() < 3 {
                    println!("Usage: nav <tab_id> <url>");
                    continue;
                }
                match browser::navigate(&client, &args[1], &args[2]).await {
                    Ok(()) => println!("Navigated tab {} to {}", args[1], args[2]),
                    Err(err) => println!("Error: {err}"),
                }
            }
            "snapshot" => {
                if args.len() < 2 {
                    println!("Usage: snapshot <tab_id>");
                    continue;
                }
                match browser::dom_snapshot(&client, &args[1]).await {
                    Ok(snapshot) => println!("{snapshot}"),
                    Err(err) => println!("Error: {err}"),
                }
            }
            "screenshot" => {
                if args.len() < 2 {
                    println!("Usage: screenshot <tab_id>");
                    continue;
                }
                match browser::screenshot(&client, &args[1], false, "png", None).await {
                    Ok(data) => println!("Screenshot ({} bytes base64)", data.len()),
                    Err(err) => println!("Error: {err}"),
                }
            }
            "info" => match client.send_request("getInfo", None).await {
                Ok(info) => println!("{}", info.get()),
                Err(err) => println!("Error: {err}"),
            },
            "ping" => match client.send_request("ping", None).await {
                Ok(raw) => println!("{}", raw.get()),
                Err(err) => println!("Error: {err}"),
            },
            "quit" | "exit" => return Ok(()),
            other => println!("Unknown command: {other}"),
        }
    }
}

fn parse_try_command(rest: &str) -> Result<(&str, Option<Value>), String> {
    let (method, json) = rest
        .split_once(' ')
        .map(|(method, json)| (method, Some(json.trim())))
        .unwrap_or((rest.trim(), None));
    let params = match json.filter(|json| !json.is_empty()) {
        None => None,
        Some(json) => match serde_json::from_str::<Value>(json) {
            Ok(value @ Value::Object(_)) => Some(value),
            Ok(_) => return Err("Invalid JSON params: expected object".into()),
            Err(err) => return Err(format!("Invalid JSON params: {err}")),
        },
    };
    Ok((method, params))
}

/// Split a command line into tokens, respecting double-quoted segments.
pub fn split_args(s: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_quote = false;

    for ch in s.chars() {
        match ch {
            '"' => in_quote = !in_quote,
            ' ' if !in_quote => {
                if !current.is_empty() {
                    args.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }

    if !current.is_empty() {
        args.push(current);
    }

    args
}

#[cfg(test)]
mod tests {
    use super::parse_try_command;
    use super::split_args;
    use super::{encode_result, run_command_with_io, save_session, Command};

    #[test]
    fn split_args_keeps_quoted_segments() {
        assert_eq!(
            split_args(r#"try method {"value": "hello world"}"#),
            vec!["try", "method", "{value:", "hello world}"]
        );
        assert_eq!(
            split_args(r#"nav 1 "https://example.com/a b""#),
            vec!["nav", "1", "https://example.com/a b"]
        );
    }

    #[test]
    fn try_command_preserves_json_quotes() {
        let (method, params) =
            parse_try_command(r#"executeUnhandledCommand {"type":"browser_visibility_get"}"#)
                .unwrap();

        assert_eq!(method, "executeUnhandledCommand");
        assert_eq!(params.unwrap()["type"], "browser_visibility_get");
    }

    #[test]
    fn compact_output_omits_image_data_and_compacts_json_text() {
        let encoded = encode_result(
            serde_json::json!({"content":[
                {"type":"image","data":"SECRET_BASE64","mimeType":"image/png"},
                {"type":"text","text":"{\n  \"count\": 2\n}"}
            ]}),
            16384,
        )
        .unwrap();
        assert!(!encoded.contains("SECRET_BASE64"));
        let value: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(value["content"][0]["base64Bytes"], 13);
        assert_eq!(value["content"][0]["omitted"], true);
        assert_eq!(value["content"][1]["text"], "{\"count\":2}");
    }

    #[test]
    fn output_budget_keeps_valid_json_and_error_status() {
        let text = "中文\"\\\n".repeat(10000);
        let encoded = encode_result(
            serde_json::json!({"isError":true,"content":[
                {"type":"text","text":text}
            ]}),
            256,
        )
        .unwrap();
        assert!(encoded.len() < 256);
        let value: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(value["isError"], true);
        assert_eq!(value["truncated"], true);
        assert!(value["originalBytes"].as_u64().unwrap() > 256);
    }

    async fn finalize_after_failure(input: &[u8], reply_error: bool) -> String {
        use crate::{client::Client, mcp::Server, protocol};
        use serde_json::{json, Value};
        use tokio::time::{timeout, Duration};

        let (client_end, mut peer) = tokio::io::duplex(8192);
        let server = Server::new(Client::from_stream(Box::new(client_end)).unwrap());
        let cleanup = tokio::spawn(async move {
            let bytes = timeout(Duration::from_secs(2), protocol::decode_frame(&mut peer))
                .await
                .expect("finalize request timeout")
                .unwrap();
            let request: Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(request["method"], "turnEnded");
            let response = if reply_error {
                json!({"id":request["id"],"error":{"code":-1,"message":"cleanup rejected"}})
            } else {
                json!({"id":request["id"],"result":{}})
            };
            protocol::encode_frame(&mut peer, &response).await.unwrap();
        });
        let mut reader = tokio::io::BufReader::new(input);
        let mut output = Vec::new();
        let error = timeout(
            Duration::from_secs(2),
            run_command_with_io(
                &server,
                Command::Batch,
                16384,
                None,
                true,
                &mut reader,
                &mut output,
            ),
        )
        .await
        .expect("command timeout")
        .unwrap_err();
        timeout(Duration::from_secs(2), cleanup)
            .await
            .expect("cleanup task timeout")
            .unwrap();
        format!("{error:#}")
    }

    #[tokio::test]
    async fn batch_finalizes_after_tool_failure_and_stops_before_next_call() {
        let error = finalize_after_failure(
            b"{\"name\":\"not_a_tool\"}\n{\"name\":\"codex_name_session\",\"arguments\":{\"name\":\"must not run\"}}\n", false,
        ).await;
        assert!(error.contains("batch stopped at line 1"));
    }

    #[tokio::test]
    async fn batch_finalizes_after_invalid_json() {
        let error = finalize_after_failure(b"not json\n", false).await;
        assert!(error.contains("invalid batch JSON at line 1"));
    }

    #[tokio::test]
    async fn finalize_failure_is_not_hidden_by_original_failure() {
        let error = finalize_after_failure(b"not json\n", true).await;
        assert!(error.contains("invalid batch JSON"));
        assert!(error.contains("finalize also failed"));
        assert!(error.contains("cleanup rejected"));
    }

    #[tokio::test]
    async fn batch_keeps_one_session_and_explicitly_finalizes_after_success() {
        use crate::{client::Client, mcp::Server, protocol};
        use serde_json::{json, Value};
        use tokio::time::{timeout, Duration};

        let (client_end, mut peer) = tokio::io::duplex(8192);
        let server = Server::new(Client::from_stream(Box::new(client_end)).unwrap());
        let responder = tokio::spawn(async move {
            let mut session = None;
            for method in ["nameSession", "nameSession", "turnEnded"] {
                let bytes = timeout(Duration::from_secs(2), protocol::decode_frame(&mut peer))
                    .await
                    .expect("request timeout")
                    .unwrap();
                let request: Value = serde_json::from_slice(&bytes).unwrap();
                assert_eq!(request["method"], method);
                let ids = (
                    request["params"]["session_id"].clone(),
                    request["params"]["turn_id"].clone(),
                );
                assert!(ids.0.is_string());
                assert!(ids.1.is_string());
                if let Some(expected) = &session {
                    assert_eq!(expected, &ids);
                } else {
                    session = Some(ids);
                }
                protocol::encode_frame(&mut peer, &json!({"id":request["id"],"result":{}}))
                    .await
                    .unwrap();
            }
        });
        let input = b"{\"name\":\"codex_name_session\",\"arguments\":{\"name\":\"first\"}}\n{\"name\":\"codex_name_session\",\"arguments\":{\"name\":\"second\"}}";
        let mut reader = tokio::io::BufReader::new(&input[..]);
        let mut output = Vec::new();
        let dir = tempfile::tempdir().unwrap();
        let session_file = dir.path().join("session.json");
        save_session(
            &session_file,
            crate::session::BrowserIdentity {
                family: "chrome".into(),
                instance_id: "test-instance".into(),
            },
            "session-before",
            "turn-before",
        )
        .unwrap();
        timeout(
            Duration::from_secs(2),
            run_command_with_io(
                &server,
                Command::Batch,
                16384,
                Some(&session_file),
                true,
                &mut reader,
                &mut output,
            ),
        )
        .await
        .expect("command timeout")
        .unwrap();
        timeout(Duration::from_secs(2), responder)
            .await
            .expect("responder timeout")
            .unwrap();
        let results: Vec<Value> = String::from_utf8(output)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0]["content"][0]["text"], "Session named: first");
        assert_eq!(results[1]["content"][0]["text"], "Session named: second");
        assert!(!session_file.exists());
    }
}
