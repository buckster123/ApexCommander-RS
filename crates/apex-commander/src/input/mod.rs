//! Coordinate / real-input backend (fallback when AT-SPI actions are insufficient).
//!
//! Prefer AT-SPI `do_action` / `type_into`. This module injects real mouse/keyboard
//! events via external tools when present:
//! - `ydotool` (Wayland-friendly uinput)
//! - `xdotool` (X11)
//! - `wtype` (Wayland text only)
//!
//! Coordinate tools fail closed when the frontmost window cannot be classified
//! against the sensitive-app denylist. Portal/libei is not wired — probed later.

use serde_json::json;
use tracing::debug;

use crate::a11y::{frontmost, AtspiSession};
use crate::audit;
use crate::error::{HarnessError, Result};
use crate::policy::{self, SensitiveConfig};
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
                "coordinate/type fallback: {} (prefer AT-SPI do_action/type_into; PATH probe only)",
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

async fn guard_frontmost(session: Option<&AtspiSession>, tool: &str) -> Result<()> {
    audit::require_writable()?;
    let session = session.ok_or_else(|| policy::unclassified(tool))?;
    let window = frontmost(session)
        .await
        .map_err(|_| policy::unclassified(tool))?;
    let cfg = SensitiveConfig::load();
    policy::enforce_window(&window, tool, &cfg)
}

/// Move pointer to absolute screen coordinates.
pub async fn mouse_move(session: Option<&AtspiSession>, x: i32, y: i32) -> Result<String> {
    guard_frontmost(session, "mouse_move").await?;
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
    Ok(audit::record_after(
        "mouse_move",
        MutationClass::Mutating,
        Some(json!({"x": x, "y": y, "backend": format!("{backend:?}")})),
        true,
        &detail,
    ))
}

/// Click at absolute coordinates (moves first).
pub async fn mouse_click(
    session: Option<&AtspiSession>,
    x: i32,
    y: i32,
    button: u8,
) -> Result<String> {
    guard_frontmost(session, "mouse_click").await?;
    let backend = pick_mouse_backend()?;
    let btn = match button {
        1 => 1u8,
        2 => 2,
        3 => 3,
        other => {
            return Err(HarnessError::Other(format!(
                "unsupported mouse button {other} (use 1=left 2=middle 3=right)"
            )));
        }
    };
    let detail = match backend {
        InputBackendKind::Ydotool => {
            run_cmd(
                "ydotool",
                &["mousemove", "--absolute", &x.to_string(), &y.to_string()],
            )
            .await?;
            let mask = match btn {
                1 => "0xC0",
                2 => "0xC1",
                3 => "0xC2",
                _ => unreachable!(),
            };
            run_cmd("ydotool", &["click", mask]).await?
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
                    &btn.to_string(),
                ],
            )
            .await?
        }
        other => {
            return Err(HarnessError::Unavailable(format!("{other:?} cannot click")));
        }
    };
    Ok(audit::record_after(
        "mouse_click",
        MutationClass::Mutating,
        Some(json!({"x": x, "y": y, "button": btn, "backend": format!("{backend:?}")})),
        true,
        &detail,
    ))
}

/// Type text via real key events (fallback when EditableText is missing).
///
/// Audit/result detail is a character count only — never the typed string.
pub async fn type_text(session: Option<&AtspiSession>, text: &str) -> Result<String> {
    guard_frontmost(session, "type_text").await?;
    let backend = pick_type_backend()?;
    match backend {
        InputBackendKind::Ydotool => run_cmd_quiet("ydotool", &["type", "--", text]).await?,
        InputBackendKind::Wtype => run_cmd_quiet("wtype", &["--", text]).await?,
        InputBackendKind::Xdotool => {
            run_cmd_quiet("xdotool", &["type", "--clearmodifiers", "--", text]).await?
        }
        InputBackendKind::None => unreachable!(),
    };
    let detail = audit::typed_chars_detail(&format!("{backend:?}"), text.chars().count());
    Ok(audit::record_after(
        "type_text",
        MutationClass::Mutating,
        Some(json!({
            "chars": text.chars().count(),
            "backend": format!("{backend:?}"),
        })),
        true,
        &detail,
    ))
}

/// Send a key combo like `ctrl+c` or `Return` (backend-specific syntax).
pub async fn key(session: Option<&AtspiSession>, keyspec: &str) -> Result<String> {
    guard_frontmost(session, "key").await?;
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
        InputBackendKind::Ydotool => run_cmd("ydotool", &["key", keyspec]).await?,
        InputBackendKind::Xdotool => run_cmd("xdotool", &["key", keyspec]).await?,
        InputBackendKind::Wtype => run_cmd("wtype", &["-k", keyspec]).await?,
        InputBackendKind::None => unreachable!(),
    };
    Ok(audit::record_after(
        "key",
        MutationClass::Mutating,
        Some(json!({"key": keyspec, "backend": format!("{backend:?}")})),
        true,
        &detail,
    ))
}

async fn run_cmd(bin: &str, args: &[&str]) -> Result<String> {
    debug!(bin, ?args, "input backend spawn");
    run_cmd_quiet(bin, args).await?;
    Ok(format!("{bin} ok"))
}

async fn run_cmd_quiet(bin: &str, args: &[&str]) -> Result<()> {
    debug!(bin, argc = args.len(), "input backend spawn");
    let out = tokio::process::Command::new(bin)
        .args(args)
        .output()
        .await
        .map_err(|e| HarnessError::Unavailable(format!("spawn {bin}: {e}")))?;
    if !out.status.success() {
        return Err(HarnessError::Other(format!(
            "{bin} exit {:?} stderr={}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(())
}
