//! `apex-harness-mcp` — MCP-over-stdio agent face.
//!
//! Protocol `2024-11-05`, hand-rolled newline-delimited JSON-RPC.
//! **stdout is sacred** (JSON-RPC only). All tracing → stderr.

use std::io::{BufRead, Write};

use anyhow::{Context, Result};
use serde_json::{json, Value};
use tracing::{error, info, warn};

use apex_harness::a11y::{
    activate, find_elements, focused_element, frontmost, list_apps, list_windows, snapshot,
    AtspiSession,
};
use apex_harness::doctor::run_doctor;
use apex_harness::types::{FindQuery, SnapshotOpts, TargetRef};
use apex_harness::{NAME, VERSION};

const PROTOCOL_VERSION: &str = "2024-11-05";
const MAX_FRAME_BYTES: usize = 32 * 1024 * 1024;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    info!(name = NAME, version = VERSION, "apex-harness-mcp starting");

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

        if let Some(resp) = dispatch(&req).await {
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

async fn dispatch(req: &Value) -> Option<Value> {
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
        "tools/call" => tools_call(req).await,
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

async fn tools_call(req: &Value) -> Result<Value, String> {
    let params = req.get("params").cloned().unwrap_or(json!({}));
    let name = params
        .get("name")
        .and_then(|n| n.as_str())
        .ok_or_else(|| "tools/call missing params.name".to_string())?;
    let args = params.get("arguments").cloned().unwrap_or(json!({}));

    match name {
        "doctor" => ok_json(&run_doctor().await),
        "list_apps" => {
            let s = AtspiSession::connect().await.map_err(err_str)?;
            ok_json(&list_apps(&s).await.map_err(err_str)?)
        }
        "list_windows" => {
            let s = AtspiSession::connect().await.map_err(err_str)?;
            ok_json(&list_windows(&s).await.map_err(err_str)?)
        }
        "frontmost" => {
            let s = AtspiSession::connect().await.map_err(err_str)?;
            ok_json(&frontmost(&s).await.map_err(err_str)?)
        }
        "activate" => {
            let s = AtspiSession::connect().await.map_err(err_str)?;
            let target = target_from_args(&args)?;
            ok_json(&activate(&s, &target).await.map_err(err_str)?)
        }
        "snapshot" => {
            let s = AtspiSession::connect().await.map_err(err_str)?;
            let target = target_from_args(&args)?;
            let opts = SnapshotOpts {
                max_depth: args
                    .get("max_depth")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(6) as u32,
                max_nodes: args
                    .get("max_nodes")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(200) as u32,
                include_bounds: true,
                include_actions: true,
            };
            let (tree, stats) = snapshot(&s, &target, opts).await.map_err(err_str)?;
            ok_json(&json!({ "tree": tree, "stats": stats }))
        }
        "find_elements" => {
            let s = AtspiSession::connect().await.map_err(err_str)?;
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
            ok_json(
                &find_elements(&s, &target, query, SnapshotOpts::default())
                    .await
                    .map_err(err_str)?,
            )
        }
        "focused_element" => {
            let s = AtspiSession::connect().await.map_err(err_str)?;
            let target = if args.as_object().map(|o| o.is_empty()).unwrap_or(true) {
                None
            } else {
                Some(target_from_args(&args)?)
            };
            ok_json(&focused_element(&s, target.as_ref()).await.map_err(err_str)?)
        }
        other => Err(format!(
            "unknown tool '{other}' — see tools/list (S1 surface: doctor, list_*, frontmost, activate, snapshot, find_elements, focused_element)"
        )),
    }
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
