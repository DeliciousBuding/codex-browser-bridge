//! Failure-safe pointer input shared by CUA and locator actions.
//!
//! CDP pointer commands are state-changing and are sent at most once. The
//! public operations accept the official CUA coordinate fields (button 1..5,
//! held modifiers, path coordinates) and always attempt a release after a
//! partially delivered press.

use std::time::Duration;

use serde_json::{json, Value};
use tokio::time::{timeout_at, Instant};

use crate::{
    client::Client,
    error::{BridgeError, Result},
};

const OP_TIMEOUT: Duration = Duration::from_secs(3);
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(2);
const CLEANUP_EVENT_TIMEOUT: Duration = Duration::from_millis(250);
const MAX_PATH_POINTS: usize = 1_000;
const MAX_COORDINATE: f64 = 10_000_000.0;
/// Time allowed for the renderer to commit one frame after activation.
const FOREGROUND_SETTLE: Duration = Duration::from_millis(50);

#[derive(Clone, Copy)]
struct ButtonSpec {
    name: &'static str,
    bits: i64,
}

#[derive(Clone, Copy)]
struct ClickSpec {
    x: f64,
    y: f64,
    button: ButtonSpec,
    click_count: u8,
    modifiers: i64,
}

fn button(number: u8) -> Result<ButtonSpec> {
    match number {
        1 => Ok(ButtonSpec {
            name: "left",
            bits: 1,
        }),
        2 => Ok(ButtonSpec {
            name: "middle",
            bits: 4,
        }),
        3 => Ok(ButtonSpec {
            name: "right",
            bits: 2,
        }),
        4 => Ok(ButtonSpec {
            name: "back",
            bits: 8,
        }),
        5 => Ok(ButtonSpec {
            name: "forward",
            bits: 16,
        }),
        _ => Err(BridgeError::User(
            "pointer button must be 1 (left), 2 (middle), 3 (right), 4 (back), or 5 (forward)"
                .into(),
        )),
    }
}

fn coordinate(name: &str, value: f64) -> Result<f64> {
    if !value.is_finite() || !(0.0..=MAX_COORDINATE).contains(&value) {
        return Err(BridgeError::User(format!(
            "{name} must be a finite coordinate between 0 and {MAX_COORDINATE}"
        )));
    }
    Ok(value)
}

fn auto_foreground_disabled() -> bool {
    matches!(
        std::env::var("CODEX_BRIDGE_NO_AUTO_FOREGROUND").as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    )
}

/// Bring the tab to the foreground before synthesized input.
///
/// Chrome acknowledges `Input.dispatchMouseEvent` for a background or occluded
/// tab but may never deliver the event to the renderer, and the extension mutes
/// inactive tabs. Without this check a click can report success while the page
/// state is unchanged. Set `CODEX_BRIDGE_NO_AUTO_FOREGROUND=1` to keep the
/// previous behaviour and accept that risk.
pub(crate) async fn ensure_foreground(client: &Client, tab: i64, deadline: Instant) -> Result<()> {
    if auto_foreground_disabled() {
        return Ok(());
    }
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(BridgeError::Timeout("pointer operation deadline".into()));
    }
    let raw = client
        .execute_cdp_with_timeout(
            tab,
            "Runtime.evaluate",
            Some(json!({
                "expression": "document.visibilityState",
                "returnByValue": true
            })),
            remaining,
        )
        .await?;
    let state = crate::browser::runtime_value_string(&raw)?.unwrap_or_default();
    if state == "visible" {
        return Ok(());
    }
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(BridgeError::Timeout("pointer operation deadline".into()));
    }
    client
        .execute_cdp_once_with_timeout(tab, "Page.bringToFront", None, remaining)
        .await?;
    tokio::time::sleep(FOREGROUND_SETTLE).await;
    tracing::debug!(tab, state = %state, "activated background tab before input");
    Ok(())
}

async fn dispatch_until(client: &Client, tab: i64, params: Value, deadline: Instant) -> Result<()> {
    dispatch_method_until(client, tab, "Input.dispatchMouseEvent", params, deadline).await
}

async fn dispatch_method_until(
    client: &Client,
    tab: i64,
    method: &str,
    params: Value,
    deadline: Instant,
) -> Result<()> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(BridgeError::Timeout("pointer operation deadline".into()));
    }
    timeout_at(
        deadline,
        client.execute_cdp_once_with_timeout(tab, method, Some(params), remaining),
    )
    .await
    .map_err(|_| BridgeError::Timeout("pointer operation deadline".into()))?
    .map(|_| ())
}

async fn dispatch_move_until(
    client: &Client,
    tab: i64,
    params: Value,
    deadline: Instant,
) -> Result<()> {
    // Current extension builds apply mouseMoved but may never return its
    // response. Wait briefly so ordering and renderer work stay bounded, then
    // treat only the missing ack as success. A real CDP or connection error is
    // still surfaced.
    let remaining = deadline
        .saturating_duration_since(Instant::now())
        .min(Duration::from_millis(3_000));
    if remaining.is_zero() {
        return Err(BridgeError::Timeout("pointer operation deadline".into()));
    }
    let move_deadline = Instant::now() + remaining;
    match timeout_at(
        move_deadline,
        client.execute_cdp_once_with_timeout(
            tab,
            "Input.dispatchMouseEvent",
            Some(params),
            remaining,
        ),
    )
    .await
    {
        Ok(Ok(_)) | Err(_) => Ok(()),
        Ok(Err(BridgeError::Timeout(_))) => Ok(()),
        Ok(Err(error)) => Err(error),
    }
}

async fn released_once(
    client: &Client,
    tab: i64,
    click: ClickSpec,
    deadline: Instant,
) -> Result<()> {
    dispatch_until(
        client,
        tab,
        json!({
            "type": "mouseReleased",
            "x": click.x,
            "y": click.y,
            "button": click.button.name,
            "buttons": 0,
            "clickCount": click.click_count,
            "modifiers": click.modifiers
        }),
        deadline,
    )
    .await
}

async fn click_once_until(
    client: &Client,
    tab: i64,
    click: ClickSpec,
    deadline: Instant,
) -> Result<()> {
    dispatch_until(
        client,
        tab,
        json!({
            "type": "mousePressed",
            "x": click.x,
            "y": click.y,
            "button": click.button.name,
            "buttons": click.button.bits,
            "clickCount": click.click_count,
            "modifiers": click.modifiers
        }),
        deadline,
    )
    .await?;
    released_once(client, tab, click, deadline).await
}

async fn release_after_failure(
    client: &Client,
    tab: i64,
    click: ClickSpec,
    original: BridgeError,
) -> Result<()> {
    let deadline = std::cmp::min(
        Instant::now() + CLEANUP_TIMEOUT,
        Instant::now() + CLEANUP_EVENT_TIMEOUT,
    );
    match released_once(client, tab, click, deadline).await {
        Ok(()) => Err(original),
        Err(cleanup) => Err(BridgeError::Protocol(format!(
            "{original}; pointer release cleanup failed: {cleanup}"
        ))),
    }
}

/// Official `CUAAPI.click` / `CUAAPI.double_click` semantics. `click_count=2`
/// sends a complete click with count 1 followed by a complete click with count
/// 2, matching a browser double-click.
pub async fn click(
    client: &Client,
    tab: i64,
    x: f64,
    y: f64,
    button_number: u8,
    click_count: u8,
    modifiers: &[String],
) -> Result<()> {
    let x = coordinate("x", x)?;
    let y = coordinate("y", y)?;
    if !(1..=3).contains(&click_count) {
        return Err(BridgeError::User(
            "click_count must be between 1 and 3".into(),
        ));
    }
    let button = button(button_number)?;
    let modifiers = crate::input::modifier_mask(modifiers)?;
    ensure_foreground(client, tab, Instant::now() + OP_TIMEOUT).await?;
    let deadline = Instant::now() + OP_TIMEOUT;
    for count in 1..=click_count {
        let click = ClickSpec {
            x,
            y,
            button,
            click_count: count,
            modifiers,
        };
        if let Err(original) = click_once_until(client, tab, click, deadline).await {
            return release_after_failure(client, tab, click, original).await;
        }
    }
    Ok(())
}

/// Official `CUAAPI.move` semantics, including held modifier keys.
pub async fn move_to(
    client: &Client,
    tab: i64,
    x: f64,
    y: f64,
    modifiers: &[String],
) -> Result<()> {
    let x = coordinate("x", x)?;
    let y = coordinate("y", y)?;
    let modifiers = crate::input::modifier_mask(modifiers)?;
    ensure_foreground(client, tab, Instant::now() + OP_TIMEOUT).await?;
    let deadline = Instant::now() + OP_TIMEOUT;
    dispatch_move_until(
        client,
        tab,
        json!({
            "type": "mouseMoved",
            "x": x,
            "y": y,
            "button": "none",
            "buttons": 0,
            "modifiers": modifiers
        }),
        deadline,
    )
    .await
}

/// Official `CUAAPI.scroll` semantics. The method accepts both the official
/// camelCase values at the tool boundary and the existing snake_case names.
pub async fn scroll(
    client: &Client,
    tab: i64,
    x: f64,
    y: f64,
    scroll_x: f64,
    scroll_y: f64,
    modifiers: &[String],
) -> Result<()> {
    let x = coordinate("x", x)?;
    let y = coordinate("y", y)?;
    let scroll_x = coordinate("scrollX magnitude", scroll_x.abs())? * scroll_x.signum();
    let scroll_y = coordinate("scrollY magnitude", scroll_y.abs())? * scroll_y.signum();
    let modifiers = crate::input::modifier_mask(modifiers)?;
    ensure_foreground(client, tab, Instant::now() + OP_TIMEOUT).await?;
    let deadline = Instant::now() + OP_TIMEOUT;
    if modifiers != 0 {
        dispatch_move_until(
            client,
            tab,
            json!({
                "type": "mouseMoved",
                "x": x,
                "y": y,
                "button": "none",
                "buttons": 0,
                "modifiers": modifiers
            }),
            deadline,
        )
        .await?;
    }
    dispatch_method_until(
        client,
        tab,
        "Input.synthesizeScrollGesture",
        json!({
            "x": x,
            "y": y,
            "xDistance": -scroll_x,
            "yDistance": -scroll_y,
            "gestureSourceType": "mouse",
            "preventFling": true,
            "speed": 8000
        }),
        deadline,
    )
    .await
}

/// Official `CUAAPI.drag` semantics: press at the first path point, move
/// through every supplied point in order, and release at the last point.
pub async fn drag(
    client: &Client,
    tab: i64,
    path: &[(f64, f64)],
    modifiers: &[String],
) -> Result<()> {
    if !(1..=MAX_PATH_POINTS).contains(&path.len()) {
        return Err(BridgeError::User(format!(
            "drag path must contain between 1 and {MAX_PATH_POINTS} points"
        )));
    }
    let path = path
        .iter()
        .enumerate()
        .map(|(index, (x, y))| {
            Ok((
                coordinate(&format!("path[{index}].x"), *x)?,
                coordinate(&format!("path[{index}].y"), *y)?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let modifiers = crate::input::modifier_mask(modifiers)?;
    let button = button(1)?;
    let (start_x, start_y) = path[0];
    let (end_x, end_y) = *path.last().expect("validated nonempty drag path");
    let (mut release_x, mut release_y) = (start_x, start_y);
    ensure_foreground(client, tab, Instant::now() + OP_TIMEOUT).await?;

    // Mark the press as potentially delivered before awaiting it. If the pipe
    // breaks after the browser receives mousePressed, cleanup must still release
    // the button; an extra release without a press is harmless.
    let pressed = dispatch_until(
        client,
        tab,
        json!({
            "type": "mousePressed",
            "x": start_x,
            "y": start_y,
            "button": button.name,
            "buttons": button.bits,
            "clickCount": 1,
            "modifiers": modifiers
        }),
        Instant::now() + OP_TIMEOUT,
    )
    .await;
    let result: Result<()> = async {
        pressed?;
        for (x, y) in path.iter().skip(1) {
            release_x = *x;
            release_y = *y;
            dispatch_move_until(
                client,
                tab,
                json!({
                    "type": "mouseMoved",
                    "x": x,
                    "y": y,
                    "button": button.name,
                    "buttons": button.bits,
                    "modifiers": modifiers
                }),
                Instant::now() + OP_TIMEOUT,
            )
            .await?;
        }
        released_once(
            client,
            tab,
            ClickSpec {
                x: end_x,
                y: end_y,
                button,
                click_count: 1,
                modifiers,
            },
            Instant::now() + OP_TIMEOUT,
        )
        .await
    }
    .await;

    match result {
        Ok(()) => Ok(()),
        Err(original) => {
            release_after_failure(
                client,
                tab,
                ClickSpec {
                    x: release_x,
                    y: release_y,
                    button,
                    click_count: 1,
                    modifiers,
                },
                original,
            )
            .await
        }
    }
}

#[cfg(test)]
mod foreground_tests {
    use super::*;
    use crate::client::Client;
    use crate::protocol::{decode_frame, encode_frame};
    use tokio::time::timeout;

    async fn connected() -> (Client, tokio::io::DuplexStream) {
        let (client_end, server) = tokio::io::duplex(8192);
        let client = Client::from_stream(client_end).unwrap();
        client.mark_attached(7).await;
        (client, server)
    }

    #[tokio::test]
    async fn hidden_tab_is_activated_before_input() {
        let (client, mut server) = connected().await;
        let responder = tokio::spawn(async move {
            let frame = timeout(Duration::from_secs(2), decode_frame(&mut server))
                .await
                .expect("probe timeout")
                .unwrap();
            let probe: Value = serde_json::from_slice(&frame).unwrap();
            assert_eq!(probe["method"], "executeCdp");
            assert_eq!(probe["params"]["method"], "Runtime.evaluate");
            encode_frame(
                &mut server,
                &json!({"id": probe["id"], "result": {"result": {"type": "string", "value": "hidden"}}}),
            )
            .await
            .unwrap();

            let frame = timeout(Duration::from_secs(2), decode_frame(&mut server))
                .await
                .expect("activation timeout")
                .unwrap();
            let activate: Value = serde_json::from_slice(&frame).unwrap();
            assert_eq!(activate["params"]["method"], "Page.bringToFront");
            encode_frame(&mut server, &json!({"id": activate["id"], "result": {}}))
                .await
                .unwrap();
        });

        ensure_foreground(&client, 7, Instant::now() + Duration::from_secs(2))
            .await
            .unwrap();
        responder.await.unwrap();
    }

    #[tokio::test]
    async fn visible_tab_is_not_activated() {
        let (client, mut server) = connected().await;
        let responder = tokio::spawn(async move {
            let frame = timeout(Duration::from_secs(2), decode_frame(&mut server))
                .await
                .expect("probe timeout")
                .unwrap();
            let probe: Value = serde_json::from_slice(&frame).unwrap();
            assert_eq!(probe["params"]["method"], "Runtime.evaluate");
            encode_frame(
                &mut server,
                &json!({"id": probe["id"], "result": {"result": {"type": "string", "value": "visible"}}}),
            )
            .await
            .unwrap();
            // No further request may arrive: a visible tab needs no activation.
            assert!(
                timeout(Duration::from_millis(100), decode_frame(&mut server))
                    .await
                    .is_err()
            );
        });

        ensure_foreground(&client, 7, Instant::now() + Duration::from_secs(2))
            .await
            .unwrap();
        responder.await.unwrap();
    }
}
