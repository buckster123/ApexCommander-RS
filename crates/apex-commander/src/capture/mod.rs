//! Screenshot / capture backend.
//!
//! Preferred order:
//! 1. XDG Desktop Portal `Screenshot` (works on GNOME Wayland when permitted)
//! 2. GNOME Shell.Screenshot (often AccessDenied without shell privilege)
//! 3. `gnome-screenshot -f` / `grim` CLI fallbacks
//!
//! Window-scoped: capture full frame then crop with AT-SPI bounds when available.

mod portal;

use std::path::{Path, PathBuf};

use serde_json::json;
use tracing::{debug, info, warn};

use crate::a11y::{bounds_of, frontmost, resolve_window, AtspiSession};
use crate::audit;
use crate::error::{HarnessError, Result};
use crate::paths::screenshots_dir;
use crate::policy::{self, SensitiveConfig};
use crate::types::{Bounds, Capability, MutationClass, ScreenshotResult, TargetRef};

/// Probe capture availability for `doctor` (does not write a file).
pub async fn probe_capture() -> Capability {
    // Prefer probing portal name ownership — a full screenshot would be invasive.
    match zbus::Connection::session().await {
        Ok(conn) => {
            let dbus = match zbus::fdo::DBusProxy::new(&conn).await {
                Ok(d) => d,
                Err(e) => {
                    return Capability {
                        name: "capture".into(),
                        available: false,
                        detail: Some(format!("session bus: {e}")),
                    };
                }
            };
            let portal = dbus
                .name_has_owner("org.freedesktop.portal.Desktop".try_into().unwrap())
                .await
                .unwrap_or(false);
            let shell = dbus
                .name_has_owner("org.gnome.Shell.Screenshot".try_into().unwrap())
                .await
                .unwrap_or(false);
            let grim = which("grim");
            let gss = which("gnome-screenshot");

            let mut parts = Vec::new();
            if portal {
                parts.push("xdg-desktop-portal");
            }
            if shell {
                parts.push("gnome-shell-screenshot");
            }
            if gss {
                parts.push("gnome-screenshot-cli");
            }
            if grim {
                parts.push("grim");
            }

            if parts.is_empty() {
                Capability {
                    name: "capture".into(),
                    available: false,
                    detail: Some(
                        "no portal / Shell.Screenshot / gnome-screenshot / grim detected".into(),
                    ),
                }
            } else {
                Capability {
                    name: "capture".into(),
                    available: true,
                    detail: Some(format!("backends: {}", parts.join(", "))),
                }
            }
        }
        Err(e) => Capability {
            name: "capture".into(),
            available: false,
            detail: Some(format!("no session bus: {e}")),
        },
    }
}

fn which(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| {
            std::env::split_paths(&p).any(|dir| {
                let c = dir.join(bin);
                c.is_file()
            })
        })
        .unwrap_or(false)
}

/// Take a screenshot.
///
/// - `target = None` or full display: full-screen capture
/// - `target = Some(window…)`: try window bounds crop after full capture
pub async fn screenshot(
    session: Option<&AtspiSession>,
    target: Option<&TargetRef>,
) -> Result<ScreenshotResult> {
    let cfg = SensitiveConfig::load();
    if let (Some(session), Some(target)) = (session, target) {
        let window = resolve_window(session, target).await?;
        policy::enforce_window(&window, "screenshot", &cfg)?;
    } else if let Some(session) = session {
        match frontmost(session).await {
            Ok(w) => policy::enforce_window(&w, "screenshot", &cfg)?,
            Err(_) => return Err(policy::unclassified("screenshot")),
        }
    } else {
        return Err(policy::unclassified("screenshot"));
    }

    let out_dir = screenshots_dir().map_err(HarnessError::Io)?;
    let filename = format!(
        "shot-{}.png",
        chrono::Utc::now().format("%Y%m%dT%H%M%S%.3f")
    );
    let dest = out_dir.join(&filename);

    let mut crop: Option<Bounds> = None;
    let mut scope = "display".to_string();

    if let (Some(session), Some(target)) = (session, target) {
        match resolve_capture_bounds(session, target).await {
            Ok(b) => {
                crop = Some(b);
                scope = "window".into();
            }
            Err(e) => debug!(error = %e, "no bounds for target — full display screenshot"),
        }
    }

    let (path, backend) = capture_to(&dest).await?;

    let final_path = if let Some(b) = crop {
        match crop_file(&path, &b, &dest) {
            Ok(p) => {
                // If crop wrote to dest and path was elsewhere, ok; if same file replaced, ok.
                if p != path && path != dest {
                    let _ = std::fs::remove_file(&path);
                }
                p
            }
            Err(e) => {
                warn!(error = %e, "crop failed — keeping full capture");
                scope = format!("{scope}+full_fallback");
                path
            }
        }
    } else {
        path
    };

    let bytes = std::fs::metadata(&final_path).map(|m| m.len()).unwrap_or(0);

    let result = ScreenshotResult {
        path: final_path.display().to_string(),
        scope: scope.clone(),
        backend: backend.clone(),
        bytes,
        bounds: crop,
    };

    let _ = audit::record_after(
        "screenshot",
        MutationClass::ReadOnly, // desktop-readonly; still logged for agent trails
        Some(json!({
            "scope": scope,
            "backend": backend,
            "path": result.path,
        })),
        true,
        &format!("{bytes} bytes"),
    );

    info!(path = %result.path, %backend, bytes, "screenshot saved");
    Ok(result)
}

async fn resolve_capture_bounds(session: &AtspiSession, target: &TargetRef) -> Result<Bounds> {
    match target {
        TargetRef::Id(id) => {
            let proxy = session.proxy_for_id(id).await?;
            bounds_of(&proxy)
                .await
                .ok_or_else(|| HarnessError::Unavailable(format!("no bounds for {id}")))
        }
        other => {
            let w = resolve_window(session, other).await?;
            w.bounds
                .ok_or_else(|| HarnessError::Unavailable(format!("window {} has no bounds", w.id)))
        }
    }
}

/// Capture full screen into `dest` (or a temp path). Returns (path, backend name).
async fn capture_to(dest: &Path) -> Result<(PathBuf, String)> {
    // 1. Portal
    match portal::portal_screenshot(dest).await {
        Ok(p) => return Ok((p, "xdg-desktop-portal".into())),
        Err(e) => debug!(error = %e, "portal screenshot failed"),
    }

    // 2. GNOME Shell
    match gnome_shell_screenshot(dest).await {
        Ok(p) => return Ok((p, "gnome-shell-screenshot".into())),
        Err(e) => debug!(error = %e, "shell screenshot failed"),
    }

    // 3. CLI tools
    if which("gnome-screenshot") {
        match cli_gnome_screenshot(dest).await {
            Ok(p) => return Ok((p, "gnome-screenshot-cli".into())),
            Err(e) => debug!(error = %e, "gnome-screenshot cli failed"),
        }
    }
    if which("grim") {
        match cli_grim(dest).await {
            Ok(p) => return Ok((p, "grim".into())),
            Err(e) => debug!(error = %e, "grim failed"),
        }
    }

    Err(HarnessError::Unavailable(
        "no screenshot backend succeeded (tried portal, gnome-shell, gnome-screenshot, grim)"
            .into(),
    ))
}

async fn gnome_shell_screenshot(dest: &Path) -> Result<PathBuf> {
    let conn = zbus::Connection::session()
        .await
        .map_err(|e| HarnessError::Unavailable(e.to_string()))?;
    let path_str = dest
        .to_str()
        .ok_or_else(|| HarnessError::Other("screenshot path not utf-8".into()))?;

    // Screenshot(include_cursor, flash, filename) -> (success, filename_used)
    let reply = conn
        .call_method(
            Some("org.gnome.Shell.Screenshot"),
            "/org/gnome/Shell/Screenshot",
            Some("org.gnome.Shell.Screenshot"),
            "Screenshot",
            &(false, false, path_str),
        )
        .await
        .map_err(|e| HarnessError::Unavailable(format!("Shell.Screenshot: {e}")))?;

    let (success, used): (bool, String) = reply
        .body()
        .deserialize()
        .map_err(|e| HarnessError::Other(format!("decode Shell.Screenshot reply: {e}")))?;

    if !success {
        return Err(HarnessError::Unavailable(
            "Shell.Screenshot returned success=false".into(),
        ));
    }
    let p = PathBuf::from(if used.is_empty() { path_str } else { &used });
    if !p.is_file() {
        return Err(HarnessError::Unavailable(format!(
            "Shell.Screenshot claimed success but missing file {}",
            p.display()
        )));
    }
    if p != dest {
        std::fs::rename(&p, dest).or_else(|_| std::fs::copy(&p, dest).map(|_| ()))?;
        return Ok(dest.to_path_buf());
    }
    Ok(p)
}

async fn cli_gnome_screenshot(dest: &Path) -> Result<PathBuf> {
    let out = tokio::process::Command::new("gnome-screenshot")
        .args(["-f", dest.to_str().unwrap_or("/tmp/adh.png")])
        .output()
        .await
        .map_err(|e| HarnessError::Unavailable(format!("spawn gnome-screenshot: {e}")))?;
    if !out.status.success() {
        return Err(HarnessError::Unavailable(format!(
            "gnome-screenshot exit {:?}: {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    if dest.is_file() {
        Ok(dest.to_path_buf())
    } else {
        Err(HarnessError::Unavailable(
            "gnome-screenshot exited 0 but file missing".into(),
        ))
    }
}

async fn cli_grim(dest: &Path) -> Result<PathBuf> {
    let out = tokio::process::Command::new("grim")
        .arg(dest)
        .output()
        .await
        .map_err(|e| HarnessError::Unavailable(format!("spawn grim: {e}")))?;
    if !out.status.success() || !dest.is_file() {
        return Err(HarnessError::Unavailable(format!(
            "grim failed: {}",
            String::from_utf8_lossy(&out.stderr)
        )));
    }
    Ok(dest.to_path_buf())
}

fn crop_file(src: &Path, bounds: &Bounds, dest: &Path) -> Result<PathBuf> {
    let img = image::open(src).map_err(|e| HarnessError::Other(format!("open image: {e}")))?;
    let (iw, ih) = (img.width(), img.height());

    let x = bounds.x.max(0) as u32;
    let y = bounds.y.max(0) as u32;
    let mut w = bounds.width;
    let mut h = bounds.height;
    if x >= iw || y >= ih {
        return Err(HarnessError::Other(format!(
            "crop origin ({x},{y}) outside image {iw}x{ih}"
        )));
    }
    w = w.min(iw - x);
    h = h.min(ih - y);
    if w == 0 || h == 0 {
        return Err(HarnessError::Other("zero-sized crop".into()));
    }

    let cropped = img.crop_imm(x, y, w, h);
    // Write to a sibling temp then rename if dest==src
    let tmp = dest.with_extension("crop.png");
    cropped
        .save(&tmp)
        .map_err(|e| HarnessError::Other(format!("save crop: {e}")))?;
    if tmp != dest {
        std::fs::rename(&tmp, dest).or_else(|_| {
            std::fs::copy(&tmp, dest)?;
            std::fs::remove_file(&tmp)?;
            Ok::<(), std::io::Error>(())
        })?;
    }
    Ok(dest.to_path_buf())
}
