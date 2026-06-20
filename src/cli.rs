//! Interactive CLI mode for debugging the bridge without an MCP client.
//!
//! Connects to the Codex browser pipe and provides a simple REPL for
//! tab management, navigation, inspection, and raw RPC calls.

use crate::browser;
use crate::client::Client;
use serde_json::Value;
use std::io::{self, Write};

/// Run the interactive CLI REPL on the given client connection.
pub async fn run_cli(client: Client) -> anyhow::Result<()> {
    println!("Connected to Codex browser pipe");
    println!("Commands: tabs, create, close <id>, user-tabs, claim <id>, nav <id> <url>,");
    println!("          snapshot <id>, screenshot <id>, info, ping, try <method>, quit");

    let stdin = io::stdin();
    loop {
        print!("> ");
        io::stdout().flush()?;

        let mut line = String::new();
        if stdin.read_line(&mut line)? == 0 {
            println!("\nGoodbye.");
            return Ok(());
        }

        let args = split_args(line.trim_end_matches(['\r', '\n']));
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
                    for tab in tabs {
                        println!(
                            "  [{}] {} - {} (group: {})",
                            tab.id, tab.title, tab.url, tab.tab_group
                        );
                    }
                }
                Err(err) => println!("Error: {err}"),
            },
            "claim" => {
                if args.len() < 2 {
                    println!("Usage: claim <tab_id>");
                    continue;
                }
                match browser::claim_user_tab(&client, &args[1]).await {
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
                match browser::screenshot(&client, &args[1], false).await {
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
            "try" => {
                if args.len() < 2 {
                    println!("Usage: try <method> [json_params]");
                    continue;
                }
                let params = if args.len() > 2 {
                    let json = args[2..].join(" ");
                    match serde_json::from_str::<Value>(&json) {
                        Ok(Value::Object(_)) => Some(serde_json::from_str(&json)?),
                        Ok(_) => {
                            println!("Invalid JSON params: expected object");
                            continue;
                        }
                        Err(err) => {
                            println!("Invalid JSON params: {err}");
                            continue;
                        }
                    }
                } else {
                    None
                };
                match client.send_request(&args[1], params).await {
                    Ok(raw) => println!("{}", raw.get()),
                    Err(err) => println!("Error: {err}"),
                }
            }
            "quit" | "exit" => return Ok(()),
            other => println!("Unknown command: {other}"),
        }
    }
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
    use super::split_args;

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
}
