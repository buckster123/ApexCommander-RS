//! `apex-commander-mcp` — MCP-over-stdio agent face.
//!
//! Protocol `2024-11-05`, hand-rolled newline-delimited JSON-RPC.
//! **stdout is sacred** (JSON-RPC only). All tracing → stderr.

use std::io::{BufRead, Write};

use anyhow::{Context, Result};
use serde_json::{json, Value};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use apex_commander::a11y::{
    activate, click_element, do_action, find_elements, focused_element, frontmost, list_apps,
    list_windows, set_value, snapshot, type_into, AtspiSession,
};
use apex_commander::capture::screenshot;
use apex_commander::doctor::run_doctor;
use apex_commander::field::run_field_report;
use apex_commander::input::{key, mouse_click, mouse_move, type_text};
use apex_commander::launch::launch_app;
use apex_commander::selftest::{run_selftest, SelftestOpts};
use apex_commander::types::{FindQuery, SnapshotOpts, TargetRef};
use apex_commander::wait::{wait_for_element, wait_for_stable, wait_ms};
use apex_commander::{NAME, VERSION};

const PROTOCOL_VERSION: &str = "2024-11-05";
const MAX_FRAME_BYTES: usize = 32 * 1024 * 1024;

/// Process-local AT-SPI handle. Not a daemon — this MCP process is already warm.
struct McpState {
    atspi: Mutex<Option<AtspiSession>>,
}

impl McpState {
    fn new() -> Self {
        Self {
            atspi: Mutex::new(None),
        }
    }

    async fn lock_session(
        &self,
    ) -> std::result::Result<tokio::sync::MutexGuard<'_, Option<AtspiSession>>, String> {
        let mut g = self.atspi.lock().await;
        if g.is_none() {
            *g = Some(AtspiSession::connect().await.map_err(err_str)?);
        }
        Ok(g)
    }

    async fn invalidate(&self) {
        *self.atspi.lock().await = None;
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    info!(
        name = NAME,
        version = VERSION,
        "apex-commander-mcp starting"
    );

    let state = McpState::new();
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let mut reader = stdin.lock();
    let mut line = String::new();

    loop {
        line.clear();
        let n = reader.read_line(&mut line).context("read stdin frame")?;
        if n == 0 {
            info!("stdin closed");
            break;
        }
        if line.len() > MAX_FRAME_BYTES {
            error!(len = line.len(), "frame exceeds cap — dropping");
            continue;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let req: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(e) => {
                warn!(error = %e, "malformed JSON-RPC frame");
                write_frame(
                    &mut stdout,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": null,
                        "error": { "code": -32700, "message": format!("parse error: {e}") }
                    }),
                )?;
                continue;
            }
        };

        if let Some(resp) = dispatch(&state, &req).await {
            write_frame(&mut stdout, &resp)?;
        }
    }

    Ok(())
}

fn write_frame(out: &mut impl Write, value: &Value) -> Result<()> {
    serde_json::to_writer(&mut *out, value)?;
    out.write_all(b"\n")?;
    out.flush()?;
    Ok(())
}

async fn dispatch(state: &McpState, req: &Value) -> Option<Value> {
    let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let id = req.get("id").cloned();

    if id.is_none() || method.starts_with("notifications/") {
        return None;
    }

    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": { "tools": {} },
            "serverInfo": { "name": NAME, "version": VERSION }
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_schemas() })),
        "tools/call" => tools_call(state, req).await,
        other => Err(format!("method not found: {other}")),
    };

    Some(match result {
        Ok(r) => json!({ "jsonrpc": "2.0", "id": id, "result": r }),
        Err(msg) if msg.starts_with("method not found") => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32601, "message": msg }
        }),
        Err(msg) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "content": [{ "type": "text", "text": msg }],
                "isError": true
            }
        }),
    })
}

fn tool_schemas() -> Vec<Value> {
    vec![
        tool(
            "doctor",
            "Structured readiness report: session, AT-SPI, input/capture status, recommendations. Read-only. Run before GUI work.",
            json!({"type":"object","properties":{},"additionalProperties":false}),
            true,
            false,
        ),
        tool(
            "list_apps",
            "List AT-SPI application roots (name, pid, toolkit, window_count, id). Read-only.",
            json!({"type":"object","properties":{},"additionalProperties":false}),
            true,
            false,
        ),
        tool(
            "list_windows",
            "List top-level windows/frames (title, app, pid, focused, bounds, id). Read-only.",
            json!({"type":"object","properties":{},"additionalProperties":false}),
            true,
            false,
        ),
        tool(
            "frontmost",
            "Best-effort frontmost/focused window. Read-only. May NotFound on compositors that under-report active state.",
            json!({"type":"object","properties":{},"additionalProperties":false}),
            true,
            false,
        ),
        tool(
            "activate",
            "Focus/raise a window via AT-SPI GrabFocus. Mutating. Target by id, name substring, pid, or frontmost.",
            target_schema(),
            false,
            false,
        ),
        tool(
            "snapshot",
            "Compact accessibility tree of a window/app. Read-only. Prefer this over screenshots. Optional max_depth (default 6), max_nodes (default 200).",
            json!({
                "type":"object",
                "properties":{
                    "id":{"type":"string","description":"{bus}|{path} element/window id"},
                    "name":{"type":"string","description":"window title or app name substring"},
                    "pid":{"type":"integer"},
                    "frontmost":{"type":"boolean"},
                    "max_depth":{"type":"integer","minimum":0},
                    "max_nodes":{"type":"integer","minimum":1}
                },
                "additionalProperties":false
            }),
            true,
            false,
        ),
        tool(
            "find_elements",
            "Find elements under a target by role/name/text/state. Read-only. Prefer over pixel search.",
            json!({
                "type":"object",
                "properties":{
                    "id":{"type":"string"},
                    "name":{"type":"string","description":"window/app target substring"},
                    "pid":{"type":"integer"},
                    "frontmost":{"type":"boolean"},
                    "role":{"type":"string"},
                    "element_name":{"type":"string","description":"accessible name filter"},
                    "name_exact":{"type":"boolean"},
                    "text":{"type":"string"},
                    "state":{"type":"string"},
                    "max_results":{"type":"integer","minimum":1}
                },
                "additionalProperties":false
            }),
            true,
            false,
        ),
        tool(
            "focused_element",
            "Details of the focused accessible under a target (default frontmost). Read-only.",
            target_schema(),
            true,
            false,
        ),
        tool(
            "do_action",
            "AT-SPI DoAction on an element by id. Prefer over coordinate clicks. Optional action name (Click/Press/…) or index. Sensitive apps blocked.",
            json!({
                "type":"object",
                "properties":{
                    "id":{"type":"string","description":"{bus}|{path}"},
                    "action":{"type":"string"},
                    "index":{"type":"integer"}
                },
                "required":["id"],
                "additionalProperties":false
            }),
            false,
            false,
        ),
        tool(
            "click_element",
            "AT-SPI click on an element id (Click, else Press/Activate/first). Prefer over mouse_click. Sensitive apps blocked.",
            json!({
                "type":"object",
                "properties":{
                    "id":{"type":"string","description":"{bus}|{path}"}
                },
                "required":["id"],
                "additionalProperties":false
            }),
            false,
            false,
        ),
        tool(
            "type_into",
            "Set or append text via EditableText on an element id. Prefer over type_text key injection. May type secrets — host should require approval.",
            json!({
                "type":"object",
                "properties":{
                    "id":{"type":"string"},
                    "text":{"type":"string"},
                    "append":{"type":"boolean"}
                },
                "required":["id","text"],
                "additionalProperties":false
            }),
            false,
            true,
        ),
        tool(
            "set_value",
            "Set a numeric Value interface (slider/spin) on an element id.",
            json!({
                "type":"object",
                "properties":{
                    "id":{"type":"string"},
                    "value":{"type":"number"}
                },
                "required":["id","value"],
                "additionalProperties":false
            }),
            false,
            false,
        ),
        tool(
            "screenshot",
            "Capture display (or window crop via bounds). Uses xdg-desktop-portal when available. Returns path under state dir.",
            json!({
                "type":"object",
                "properties":{
                    "id":{"type":"string"},
                    "name":{"type":"string"},
                    "pid":{"type":"integer"},
                    "frontmost":{"type":"boolean"},
                    "full":{"type":"boolean","description":"force full display"}
                },
                "additionalProperties":false
            }),
            true,
            false,
        ),
        tool(
            "mouse_move",
            "Move pointer to absolute coords via ydotool/xdotool. Prefer do_action when possible.",
            json!({
                "type":"object",
                "properties":{"x":{"type":"integer"},"y":{"type":"integer"}},
                "required":["x","y"],
                "additionalProperties":false
            }),
            false,
            false,
        ),
        tool(
            "mouse_click",
            "Click at absolute coords (fallback). Prefer do_action.",
            json!({
                "type":"object",
                "properties":{
                    "x":{"type":"integer"},
                    "y":{"type":"integer"},
                    "button":{"type":"integer","description":"1=left 2=middle 3=right"}
                },
                "required":["x","y"],
                "additionalProperties":false
            }),
            false,
            false,
        ),
        tool(
            "type_text",
            "Type via real key events (fallback). Prefer type_into on EditableText. May type secrets — host should require approval.",
            json!({
                "type":"object",
                "properties":{"text":{"type":"string"}},
                "required":["text"],
                "additionalProperties":false
            }),
            false,
            true,
        ),
        tool(
            "key",
            "Send a key/combo via input backend (syntax backend-specific). May submit forms — host should require approval.",
            json!({
                "type":"object",
                "properties":{"key":{"type":"string"}},
                "required":["key"],
                "additionalProperties":false
            }),
            false,
            true,
        ),
        tool(
            "launch",
            "Launch an application by desktop id (org.gnome.Calculator) or executable. Mutating. Sensitive names blocked.",
            json!({
                "type":"object",
                "properties":{"target":{"type":"string","description":"desktop id or executable"}},
                "required":["target"],
                "additionalProperties":false
            }),
            false,
            true,
        ),
        tool(
            "wait",
            "Sleep for timeout_ms (clamped 1..120000, default 10000). Read-only. Use between steps when needed.",
            json!({
                "type":"object",
                "properties":{"timeout_ms":{"type":"integer","minimum":1}},
                "additionalProperties":false
            }),
            true,
            false,
        ),
        tool(
            "wait_for_element",
            "Poll find_elements until a match or timeout. Read-only. Prefer over fixed sleeps when waiting for UI.",
            json!({
                "type":"object",
                "properties":{
                    "id":{"type":"string"},
                    "name":{"type":"string"},
                    "pid":{"type":"integer"},
                    "frontmost":{"type":"boolean"},
                    "role":{"type":"string"},
                    "element_name":{"type":"string"},
                    "text":{"type":"string"},
                    "state":{"type":"string"},
                    "timeout_ms":{"type":"integer"},
                    "poll_ms":{"type":"integer"}
                },
                "additionalProperties":false
            }),
            true,
            false,
        ),
        tool(
            "wait_for_stable",
            "Poll until the a11y tree fingerprint stays unchanged for stable_for_ms. Read-only.",
            json!({
                "type":"object",
                "properties":{
                    "id":{"type":"string"},
                    "name":{"type":"string"},
                    "pid":{"type":"integer"},
                    "frontmost":{"type":"boolean"},
                    "timeout_ms":{"type":"integer"},
                    "poll_ms":{"type":"integer"},
                    "stable_for_ms":{"type":"integer"}
                },
                "additionalProperties":false
            }),
            true,
            false,
        ),
        tool(
            "selftest",
            "Structured smoke suite: doctor, list apps/windows, snapshot, find. Mutating mouse wiggle only if confirm_mutate=true.",
            json!({
                "type":"object",
                "properties":{
                    "confirm_mutate":{"type":"boolean"},
                    "target_name":{"type":"string"}
                },
                "additionalProperties":false
            }),
            false,
            false,
        ),
        tool(
            "field_report",
            "Compositor field matrix (identity, AT-SPI, capture, GrabFocus honesty, screenshot file write). Not read-only — may focus a window and write a PNG. Re-run under GNOME/Plasma/Hyprland.",
            json!({
                "type":"object",
                "properties":{
                    "confirm_mutate":{"type":"boolean"}
                },
                "additionalProperties":false
            }),
            false,
            false,
        ),
    ]
}

fn tool(
    name: &str,
    description: &str,
    input_schema: Value,
    read_only: bool,
    destructive: bool,
) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema,
        "annotations": {
            "readOnlyHint": read_only,
            "destructiveHint": destructive,
            "openWorldHint": false
        }
    })
}

fn target_schema() -> Value {
    json!({
        "type":"object",
        "properties":{
            "id":{"type":"string"},
            "name":{"type":"string"},
            "pid":{"type":"integer"},
            "frontmost":{"type":"boolean"}
        },
        "additionalProperties":false
    })
}

async fn tools_call(state: &McpState, req: &Value) -> Result<Value, String> {
    let params = req.get("params").cloned().unwrap_or(json!({}));
    let name = params
        .get("name")
        .and_then(|n| n.as_str())
        .ok_or_else(|| "tools/call missing params.name".to_string())?;
    let args = params.get("arguments").cloned().unwrap_or(json!({}));

    let result = match name {
        "doctor" => ok_json(&run_doctor().await),
        "list_apps" => {
            let g = state.lock_session().await?;
            let s = g.as_ref().unwrap();
            ok_json(&list_apps(s).await.map_err(err_str)?)
        }
        "list_windows" => {
            let g = state.lock_session().await?;
            let s = g.as_ref().unwrap();
            ok_json(&list_windows(s).await.map_err(err_str)?)
        }
        "frontmost" => {
            let g = state.lock_session().await?;
            let s = g.as_ref().unwrap();
            ok_json(&frontmost(s).await.map_err(err_str)?)
        }
        "activate" => {
            let g = state.lock_session().await?;
            let s = g.as_ref().unwrap();
            let target = target_from_args(&args)?;
            ok_json(&activate(s, &target).await.map_err(err_str)?)
        }
        "snapshot" => {
            let g = state.lock_session().await?;
            let s = g.as_ref().unwrap();
            let target = target_from_args(&args)?;
            let opts = SnapshotOpts {
                max_depth: args.get("max_depth").and_then(|v| v.as_u64()).unwrap_or(6) as u32,
                max_nodes: args
                    .get("max_nodes")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(200) as u32,
                include_bounds: true,
                include_actions: true,
            };
            let (tree, stats) = snapshot(s, &target, opts).await.map_err(err_str)?;
            ok_json(&json!({ "tree": tree, "stats": stats }))
        }
        "find_elements" => {
            let g = state.lock_session().await?;
            let s = g.as_ref().unwrap();
            let target = target_from_args(&args)?;
            let query = FindQuery {
                role: str_arg(&args, "role"),
                name: str_arg(&args, "element_name"),
                name_exact: args
                    .get("name_exact")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                text: str_arg(&args, "text"),
                state: str_arg(&args, "state"),
                description: None,
                max_results: args
                    .get("max_results")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(25) as u32,
            };
            let opts = SnapshotOpts {
                max_depth: 10,
                max_nodes: 400,
                include_bounds: true,
                include_actions: true,
            };
            ok_json(
                &find_elements(s, &target, query, opts)
                    .await
                    .map_err(err_str)?,
            )
        }
        "focused_element" => {
            let g = state.lock_session().await?;
            let s = g.as_ref().unwrap();
            let target = if args.as_object().map(|o| o.is_empty()).unwrap_or(true) {
                None
            } else {
                Some(target_from_args(&args)?)
            };
            ok_json(&focused_element(s, target.as_ref()).await.map_err(err_str)?)
        }
        "do_action" => {
            let g = state.lock_session().await?;
            let s = g.as_ref().unwrap();
            let id = str_arg(&args, "id").ok_or_else(|| "do_action requires id".to_string())?;
            let action = str_arg(&args, "action");
            let index = args.get("index").and_then(|v| v.as_i64()).map(|i| i as i32);
            ok_json(
                &do_action(s, &id, action.as_deref(), index)
                    .await
                    .map_err(err_str)?,
            )
        }
        "click_element" => {
            let g = state.lock_session().await?;
            let s = g.as_ref().unwrap();
            let id = str_arg(&args, "id").ok_or_else(|| "click_element requires id".to_string())?;
            ok_json(&click_element(s, &id).await.map_err(err_str)?)
        }
        "type_into" => {
            let g = state.lock_session().await?;
            let s = g.as_ref().unwrap();
            let id = str_arg(&args, "id").ok_or_else(|| "type_into requires id".to_string())?;
            let text =
                str_arg(&args, "text").ok_or_else(|| "type_into requires text".to_string())?;
            let append = args
                .get("append")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            ok_json(&type_into(s, &id, &text, append).await.map_err(err_str)?)
        }
        "set_value" => {
            let g = state.lock_session().await?;
            let s = g.as_ref().unwrap();
            let id = str_arg(&args, "id").ok_or_else(|| "set_value requires id".to_string())?;
            let value = args
                .get("value")
                .and_then(|v| v.as_f64())
                .ok_or_else(|| "set_value requires value".to_string())?;
            ok_json(&set_value(s, &id, value).await.map_err(err_str)?)
        }
        "screenshot" => {
            let full = args.get("full").and_then(|v| v.as_bool()).unwrap_or(false);
            let g = state.lock_session().await.ok();
            let session = g.as_ref().and_then(|g| g.as_ref());
            let target = if full {
                None
            } else if args.as_object().map(|o| !o.is_empty()).unwrap_or(false) {
                let has = str_arg(&args, "id").is_some()
                    || str_arg(&args, "name").is_some()
                    || args.get("pid").is_some()
                    || args
                        .get("frontmost")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);
                if has {
                    Some(target_from_args(&args)?)
                } else {
                    None
                }
            } else {
                None
            };
            ok_json(
                &screenshot(session, target.as_ref())
                    .await
                    .map_err(err_str)?,
            )
        }
        "mouse_move" => {
            let x = args
                .get("x")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| "mouse_move requires x".to_string())? as i32;
            let y = args
                .get("y")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| "mouse_move requires y".to_string())? as i32;
            let g = state.lock_session().await?;
            ok_json(&json!({
                "ok": true,
                "detail": mouse_move(g.as_ref(), x, y).await.map_err(err_str)?
            }))
        }
        "mouse_click" => {
            let x = args
                .get("x")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| "mouse_click requires x".to_string())? as i32;
            let y = args
                .get("y")
                .and_then(|v| v.as_i64())
                .ok_or_else(|| "mouse_click requires y".to_string())? as i32;
            let button = args.get("button").and_then(|v| v.as_u64()).unwrap_or(1) as u8;
            let g = state.lock_session().await?;
            ok_json(&json!({
                "ok": true,
                "detail": mouse_click(g.as_ref(), x, y, button).await.map_err(err_str)?
            }))
        }
        "type_text" => {
            let text =
                str_arg(&args, "text").ok_or_else(|| "type_text requires text".to_string())?;
            let g = state.lock_session().await?;
            ok_json(&json!({
                "ok": true,
                "detail": type_text(g.as_ref(), &text).await.map_err(err_str)?
            }))
        }
        "key" => {
            let keyspec = str_arg(&args, "key").ok_or_else(|| "key requires key".to_string())?;
            let g = state.lock_session().await?;
            ok_json(&json!({
                "ok": true,
                "detail": key(g.as_ref(), &keyspec).await.map_err(err_str)?
            }))
        }
        "launch" => {
            let target =
                str_arg(&args, "target").ok_or_else(|| "launch requires target".to_string())?;
            ok_json(&launch_app(&target).await.map_err(err_str)?)
        }
        "wait" => {
            let ms = args.get("timeout_ms").and_then(|v| v.as_u64());
            ok_json(&wait_ms(ms).await)
        }
        "wait_for_element" => {
            let g = state.lock_session().await?;
            let s = g.as_ref().unwrap();
            let target = target_from_args(&args)?;
            let query = FindQuery {
                role: str_arg(&args, "role"),
                name: str_arg(&args, "element_name"),
                name_exact: false,
                text: str_arg(&args, "text"),
                state: str_arg(&args, "state"),
                description: None,
                max_results: 5,
            };
            let timeout = args.get("timeout_ms").and_then(|v| v.as_u64());
            let poll = args.get("poll_ms").and_then(|v| v.as_u64());
            let opts_ok = wait_for_element(s, &target, query, timeout, poll)
                .await
                .map_err(err_str)?;
            ok_json(&opts_ok)
        }
        "wait_for_stable" => {
            let g = state.lock_session().await?;
            let s = g.as_ref().unwrap();
            ok_json(
                &wait_for_stable(
                    s,
                    &target_from_args(&args)?,
                    args.get("timeout_ms").and_then(|v| v.as_u64()),
                    args.get("poll_ms").and_then(|v| v.as_u64()),
                    args.get("stable_for_ms").and_then(|v| v.as_u64()),
                )
                .await
                .map_err(err_str)?,
            )
        }
        "selftest" => {
            let confirm = args
                .get("confirm_mutate")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let target_name = str_arg(&args, "target_name");
            ok_json(
                &run_selftest(SelftestOpts {
                    confirm_mutate: confirm,
                    target_name,
                })
                .await
                .map_err(err_str)?,
            )
        }
        "field_report" => {
            let confirm = args
                .get("confirm_mutate")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            ok_json(&run_field_report(confirm).await.map_err(err_str)?)
        }
        other => Err(format!(
            "unknown tool '{other}' — see tools/list (S5 catalog includes click_element)"
        )),
    };

    if let Err(msg) = &result {
        let l = msg.to_lowercase();
        if l.contains("at-spi") || l.contains("unavailable") || l.contains("disconnected") {
            state.invalidate().await;
        }
    }
    result
}

fn target_from_args(args: &Value) -> Result<TargetRef, String> {
    let id = str_arg(args, "id");
    let name = str_arg(args, "name");
    let pid = args.get("pid").and_then(|v| v.as_u64()).map(|p| p as u32);
    let frontmost = args
        .get("frontmost")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let set = id.is_some() as u8 + name.is_some() as u8 + pid.is_some() as u8 + u8::from(frontmost);
    if set > 1 {
        return Err("specify only one of id/name/pid/frontmost".into());
    }
    if let Some(id) = id {
        return Ok(TargetRef::Id(id));
    }
    if let Some(name) = name {
        return Ok(TargetRef::Name(name));
    }
    if let Some(pid) = pid {
        return Ok(TargetRef::Pid(pid));
    }
    if frontmost || set == 0 {
        // Default to frontmost when no selector given for tools that need a target.
        return Ok(TargetRef::Frontmost);
    }
    Err("need a target: id, name, pid, or frontmost".into())
}

fn str_arg(args: &Value, key: &str) -> Option<String> {
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .filter(|s| !s.is_empty())
}

fn ok_json<T: serde::Serialize>(value: &T) -> Result<Value, String> {
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    Ok(json!({
        "content": [{ "type": "text", "text": text }],
        "isError": false
    }))
}

fn err_str(e: impl std::fmt::Display) -> String {
    e.to_string()
}
