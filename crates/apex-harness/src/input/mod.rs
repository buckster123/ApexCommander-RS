//! Coordinate / real-input backend (fallback when AT-SPI actions are insufficient).
//!
//! Prefer AT-SPI `do_action` / `type_into`. This module injects real mouse/keyboard
//! events via external tools when present:
//! - `ydotool` (Wayland-friendly uinput)
//! - `xdotool` (X11)
//! - `wtype` (Wayland text only)
//!
//! Portal/libei is not wired in S2 — probed as future work.

use serde_json::json;
use tracing::debug;

use crate::audit;
use crate::error::{HarnessError, Result};
use crate::types::{Capability, InputBackendKind, MutationClass};

/// Probe which input backends are installed (not whether they can open uinput).
pub fn probe_input() -> Capability {
    let mut available = Vec::new();
    if which("ydotool") {
        available.push("ydotool");
    }
    if which("xdotool") {
        available.push("xdotool");
    }
    if which("wtype") {
        available.push("wtype");
    }
    // AT-SPI element actions always available when atspi is; reported separately.
    if available.is_empty() {
        Capability {
            name: "input".into(),
            available: false,
            detail: Some(
                "no ydotool/xdotool/wtype in PATH — element AT-SPI actions still work; \
                 install ydotool for coordinate fallback"
                    .into(),
            ),
        }
    } else {
        Capability {
            name: "input".into(),
            available: true,
            detail: Some(format!(
                "coordinate/type fallback: {} (prefer AT-SPI do_action/type_into)",
                available.join(", ")
            )),
        }
    }
}

fn which(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|dir| dir.join(bin).is_file()))
        .unwrap_or(false)
}

fn pick_mouse_backend() -> Result<InputBackendKind> {
    if which("ydotool") {
        Ok(InputBackendKind::Ydotool)
    } else if which("xdotool") {
        Ok(InputBackendKind::Xdotool)
    } else {
        Err(HarnessError::Unavailable(
            "no mouse input backend (install ydotool or xdotool); prefer do_action on a11y elements"
                .into(),
        ))
    }
}

fn pick_type_backend() -> Result<InputBackendKind> {
    if which("ydotool") {
        Ok(InputBackendKind::Ydotool)
    } else if which("wtype") {
        Ok(InputBackendKind::Wtype)
    } else if which("xdotool") {
        Ok(InputBackendKind::Xdotool)
    } else {
        Err(HarnessError::Unavailable(
            "no type backend (install ydotool, wtype, or xdotool); prefer type_into on EditableText"
                .into(),
        ))
    }
}

/// Move pointer to absolute screen coordinates.
pub async fn mouse_move(x: i32, y: i32) -> Result<String> {
    let backend = pick_mouse_backend()?;
    let detail = match backend {
        InputBackendKind::Ydotool => {
            run_cmd(
                "ydotool",
                &["mousemove", "--absolute", &x.to_string(), &y.to_string()],
            )
            .await?
        }
        InputBackendKind::Xdotool => {
            run_cmd(
                "xdotool",
                &["mousemove", "--sync", &x.to_string(), &y.to_string()],
            )
            .await?
        }
        other => {
            return Err(HarnessError::Unavailable(format!(
                "{other:?} cannot move mouse"
            )));
        }
    };
    audit::record(
        "mouse_move",
        MutationClass::Mutating,
        Some(json!({"x": x, "y": y, "backend": format!("{backend:?}")})),
        true,
        &detail,
    );
    Ok(detail)
}

/// Click at absolute coordinates (moves first).
pub async fn mouse_click(x: i32, y: i32, button: u8) -> Result<String> {
    let backend = pick_mouse_backend()?;
    let detail = match backend {
        InputBackendKind::Ydotool => {
            run_cmd(
                "ydotool",
                &["mousemove", "--absolute", &x.to_string(), &y.to_string()],
            )
            .await?;
            // ydotool click: 0xC0 = left down+up typically; use `click` subcommand
            let btn = match button {
                1 => "0xC0",
                2 => "0xC1",
                3 => "0xC2",
                _ => "0xC0",
            };
            run_cmd("ydotool", &["click", btn]).await?
        }
        InputBackendKind::Xdotool => {
            run_cmd(
                "xdotool",
                &[
                    "mousemove",
                    "--sync",
                    &x.to_string(),
                    &y.to_string(),
                    "click",
                    &button.to_string(),
                ],
            )
            .await?
        }
        other => {
            return Err(HarnessError::Unavailable(format!("{other:?} cannot click")));
        }
    };
    audit::record(
        "mouse_click",
        MutationClass::Mutating,
        Some(json!({"x": x, "y": y, "button": button, "backend": format!("{backend:?}")})),
        true,
        &detail,
    );
    Ok(detail)
}

/// Type text via real key events (fallback when EditableText is missing).
pub async fn type_text(text: &str) -> Result<String> {
    let backend = pick_type_backend()?;
    let detail = match backend {
        InputBackendKind::Ydotool => run_cmd("ydotool", &["type", "--", text]).await?,
        InputBackendKind::Wtype => run_cmd("wtype", &["--", text]).await?,
        InputBackendKind::Xdotool => {
            run_cmd("xdotool", &["type", "--clearmodifiers", "--", text]).await?
        }
        InputBackendKind::None => unreachable!(),
    };
    audit::record(
        "type_text",
        MutationClass::Mutating,
        Some(json!({
            "chars": text.chars().count(),
            "backend": format!("{backend:?}"),
        })),
        true,
        &detail,
    );
    Ok(detail)
}

/// Send a key combo like `ctrl+c` or `Return` (backend-specific syntax).
pub async fn key(keyspec: &str) -> Result<String> {
    let backend = pick_mouse_backend().or_else(|_| {
        if which("wtype") {
            Ok(InputBackendKind::Wtype)
        } else {
            Err(HarnessError::Unavailable(
                "no key backend (ydotool/xdotool/wtype)".into(),
            ))
        }
    })?;
    let detail = match backend {
        InputBackendKind::Ydotool => {
            // ydotool key uses keycodes; for S2 pass through as type of key name via `key`
            run_cmd("ydotool", &["key", keyspec]).await?
        }
        InputBackendKind::Xdotool => run_cmd("xdotool", &["key", keyspec]).await?,
        InputBackendKind::Wtype => {
            // wtype -k for keys
            run_cmd("wtype", &["-k", keyspec]).await?
        }
        InputBackendKind::None => unreachable!(),
    };
    audit::record(
        "key",
        MutationClass::Mutating,
        Some(json!({"key": keyspec, "backend": format!("{backend:?}")})),
        true,
        &detail,
    );
    Ok(detail)
}

async fn run_cmd(bin: &str, args: &[&str]) -> Result<String> {
    debug!(bin, ?args, "input backend spawn");
    let out = tokio::process::Command::new(bin)
        .args(args)
        .output()
        .await
        .map_err(|e| HarnessError::Unavailable(format!("spawn {bin}: {e}")))?;
    if !out.status.success() {
        return Err(HarnessError::Other(format!(
            "{bin} {:?}: exit {:?} stderr={}",
            args,
            out.status.code(),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(format!("{bin} {:?} ok", args))
}
