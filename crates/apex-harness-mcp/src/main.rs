//! `apex-harness-mcp` — MCP-over-stdio agent face.
//!
//! Protocol `2024-11-05`, hand-rolled newline-delimited JSON-RPC.
//! **stdout is sacred** (JSON-RPC only). All tracing → stderr.
//!
//! Transport skeleton mirrors CerebroCortex-RS / OmniOcular-RS house style.
//! Full tool surface lands in later slices; S0 advertises `doctor` only.

use std::io::{BufRead, Write};

use anyhow::{Context, Result};
use serde_json::{json, Value};
use tracing::{error, info, warn};

use apex_harness::doctor::run_doctor;
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

        if let Some(resp) = dispatch(&req) {
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

/// Handle one request. Returns `None` for notifications (no response).
fn dispatch(req: &Value) -> Option<Value> {
    let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let id = req.get("id").cloned();

    // Notifications: no id, no response.
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
        "tools/call" => tools_call(req),
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
    vec![json!({
        "name": "doctor",
        "description": "Structured readiness report: session type (X11/Wayland), AT-SPI/input/capture/window backend availability, and recommended next steps. Read-only. Run this before any GUI action.",
        "inputSchema": {
            "type": "object",
            "properties": {},
            "additionalProperties": false
        },
        "annotations": {
            "readOnlyHint": true,
            "destructiveHint": false,
            "openWorldHint": false
        }
    })]
}

fn tools_call(req: &Value) -> Result<Value, String> {
    let params = req.get("params").cloned().unwrap_or(json!({}));
    let name = params
        .get("name")
        .and_then(|n| n.as_str())
        .ok_or_else(|| "tools/call missing params.name".to_string())?;

    match name {
        "doctor" => {
            let report = run_doctor();
            let text = serde_json::to_string_pretty(&report)
                .map_err(|e| format!("serialize doctor report: {e}"))?;
            Ok(json!({
                "content": [{ "type": "text", "text": text }],
                "isError": false
            }))
        }
        other => Err(format!(
            "unknown tool '{other}' — this S0 surface only exposes doctor; see BACKLOG.md"
        )),
    }
}
