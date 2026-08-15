//! Launch applications by desktop-entry id or executable path.
//!
//! Safety: sensitive denylist + refuse obvious shell metacharacters in bare
//! executable names. Prefer desktop-ids (`org.gnome.Calculator`) over raw paths.

use std::path::Path;
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::info;

use crate::audit;
use crate::error::{HarnessError, Result};
use crate::policy::{self, SensitiveConfig};
use crate::types::MutationClass;

/// Result of a launch attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchResult {
    pub ok: bool,
    /// What was launched (desktop id or path).
    pub target: String,
    /// Backend used (`gtk-launch`, `gio`, `exec`).
    pub backend: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    pub detail: String,
}

/// Launch by desktop file id (e.g. `org.gnome.Calculator`) or executable.
pub async fn launch_app(spec: &str) -> Result<LaunchResult> {
    let spec = spec.trim();
    if spec.is_empty() {
        return Err(HarnessError::Other("launch target is empty".into()));
    }

    crate::audit::require_writable()?;
    let cfg = SensitiveConfig::load();
    policy::enforce_name(spec, "launch", &cfg)?;

    if looks_dangerous(spec) {
        return Err(HarnessError::PolicyBlocked(format!(
            "refusing launch target with shell metacharacters: {spec:?}"
        )));
    }

    // Desktop id path: no slashes, or ends with .desktop
    if is_desktop_id(spec) {
        let id = spec.strip_suffix(".desktop").unwrap_or(spec);
        if which("gtk-launch") {
            return run_detached("gtk-launch", &[id], spec, "gtk-launch").await;
        }
        if which("gio") {
            // gio launch wants a path to the .desktop file when possible
            let desktop_path = find_desktop_file(id);
            if let Some(p) = desktop_path {
                return run_detached("gio", &["launch", p.to_str().unwrap_or(id)], spec, "gio")
                    .await;
            }
            return Err(HarnessError::NotFound(format!(
                "desktop entry {id}.desktop not found in XDG applications dirs"
            )));
        }
        return Err(HarnessError::Unavailable(
            "neither gtk-launch nor gio available to launch desktop entry".into(),
        ));
    }

    // Executable path or PATH binary
    let bin = spec;
    if bin.contains('/') && !Path::new(bin).exists() {
        return Err(HarnessError::NotFound(format!(
            "executable not found: {bin}"
        )));
    }
    run_detached(bin, &[], spec, "exec").await
}

fn is_desktop_id(spec: &str) -> bool {
    if spec.ends_with(".desktop") {
        return true;
    }
    // Reverse-DNS desktop ids (org.gnome.Calculator). Bare PATH names go via exec.
    !spec.contains('/') && !spec.contains('\\') && spec.contains('.')
}

fn looks_dangerous(spec: &str) -> bool {
    spec.chars().any(|c| {
        matches!(
            c,
            ';' | '|' | '&' | '`' | '$' | '\n' | '\r' | '>' | '<' | '(' | ')'
        )
    })
}

fn find_desktop_file(id: &str) -> Option<std::path::PathBuf> {
    let name = if id.ends_with(".desktop") {
        id.to_string()
    } else {
        format!("{id}.desktop")
    };
    let mut dirs = Vec::new();
    if let Ok(d) = std::env::var("XDG_DATA_HOME") {
        dirs.push(Path::new(&d).join("applications"));
    }
    if let Ok(home) = std::env::var("HOME") {
        dirs.push(Path::new(&home).join(".local/share/applications"));
    }
    dirs.push(Path::new("/usr/share/applications").to_path_buf());
    dirs.push(Path::new("/usr/local/share/applications").to_path_buf());
    for d in dirs {
        let p = d.join(&name);
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

fn which(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|dir| dir.join(bin).is_file()))
        .unwrap_or(false)
}

async fn run_detached(
    bin: &str,
    args: &[&str],
    target: &str,
    backend: &str,
) -> Result<LaunchResult> {
    info!(bin, ?args, backend, "launch");
    let child = tokio::process::Command::new(bin)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(false)
        .spawn()
        .map_err(|e| HarnessError::Unavailable(format!("spawn {bin}: {e}")))?;

    let pid = child.id();
    // Detach: forget the child handle so we don't wait / kill on drop.
    std::mem::forget(child);

    let detail = audit::record_after(
        "launch",
        MutationClass::Mutating,
        Some(json!({"target": target, "backend": backend, "pid": pid})),
        true,
        &format!("spawned via {backend} (not waited)"),
    );

    Ok(LaunchResult {
        ok: true,
        target: target.into(),
        backend: backend.into(),
        pid,
        detail,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_id_detection() {
        assert!(is_desktop_id("org.gnome.Calculator"));
        assert!(is_desktop_id("org.gnome.Calculator.desktop"));
        assert!(!is_desktop_id("/usr/bin/foo"));
        assert!(!is_desktop_id("gnome-calculator")); // PATH binary, not desktop id
        assert!(!is_desktop_id("foo bar"));
    }

    #[test]
    fn rejects_metacharacters() {
        assert!(looks_dangerous("foo;rm -rf /"));
        assert!(looks_dangerous("$(evil)"));
        assert!(!looks_dangerous("org.gnome.Calculator"));
        assert!(!looks_dangerous("/usr/bin/gnome-calculator"));
    }
}
