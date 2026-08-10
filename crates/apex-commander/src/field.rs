//! Compositor / DE field matrix report.
//!
//! Produces a machine-readable matrix of session identity, backend probes, and
//! live AT-SPI exercises. Re-run under GNOME, KDE/Plasma, and Hyprland sessions;
//! results feed `docs/field-matrix.md`.

use std::collections::BTreeMap;
use std::process::Command;

use serde::{Deserialize, Serialize};
use tracing::info;

use crate::a11y::{activate, find_elements, list_apps, list_windows, snapshot, AtspiSession};
use crate::capture::{probe_capture, screenshot};
use crate::doctor::run_doctor;
use crate::error::Result;
use crate::input::probe_input;
use crate::types::{FindQuery, SnapshotOpts, TargetRef};

/// One named check in the field report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldCheck {
    pub name: String,
    pub ok: bool,
    #[serde(default)]
    pub skipped: bool,
    pub detail: String,
}

/// Host / session identity for the matrix row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionIdentity {
    pub session_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desktop: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desktop_session: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wayland_display: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
    /// Best-effort family: gnome | plasma | hyprland | sway | i3 | unknown
    pub family: String,
    /// Detected helper binaries (hyprctl, kwin, grim, …).
    pub helpers: BTreeMap<String, bool>,
    /// Installed wayland/x11 session desktop files (names only).
    pub session_files: Vec<String>,
}

/// Full field matrix report for one live session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldReport {
    pub ok: bool,
    pub identity: SessionIdentity,
    pub checks: Vec<FieldCheck>,
    /// Toolkit histogram from AT-SPI app roots (gtk, qt, clutter, …).
    pub toolkits: BTreeMap<String, u32>,
    pub app_count: u32,
    pub window_count: u32,
    pub summary: String,
    /// ISO-ish timestamp (local display).
    pub captured_at: String,
}

/// Run the full field matrix against the current session (non-destructive by default).
pub async fn run_field_report(confirm_mutate: bool) -> Result<FieldReport> {
    let captured_at = chrono::Local::now().to_rfc3339();
    let identity = detect_identity();
    let mut checks = Vec::new();

    // Doctor
    let doctor = run_doctor().await;
    checks.push(FieldCheck {
        name: "doctor".into(),
        ok: doctor.ok,
        skipped: false,
        detail: doctor.summary.clone(),
    });

    // Capture / input capability (from doctor-style probes, explicit)
    let cap = probe_capture().await;
    checks.push(FieldCheck {
        name: "capture_probe".into(),
        ok: cap.available,
        skipped: false,
        detail: cap.detail.unwrap_or_default(),
    });
    let inp = probe_input();
    checks.push(FieldCheck {
        name: "input_probe".into(),
        ok: inp.available,
        skipped: !inp.available,
        detail: inp.detail.unwrap_or_default(),
    });

    let mut toolkits = BTreeMap::new();
    let mut app_count = 0u32;
    let mut window_count = 0u32;

    match AtspiSession::connect().await {
        Ok(session) => {
            checks.push(FieldCheck {
                name: "atspi_connect".into(),
                ok: true,
                skipped: false,
                detail: "connected".into(),
            });

            match list_apps(&session).await {
                Ok(apps) => {
                    app_count = apps.len() as u32;
                    for a in &apps {
                        let k = a.toolkit.as_deref().unwrap_or("unknown").to_lowercase();
                        *toolkits.entry(k).or_insert(0) += 1;
                    }
                    checks.push(FieldCheck {
                        name: "list_apps".into(),
                        ok: app_count > 0,
                        skipped: false,
                        detail: format!("{app_count} apps; toolkits={toolkits:?}"),
                    });
                }
                Err(e) => checks.push(FieldCheck {
                    name: "list_apps".into(),
                    ok: false,
                    skipped: false,
                    detail: e.to_string(),
                }),
            }

            let windows = list_windows(&session).await;
            match &windows {
                Ok(wins) => {
                    window_count = wins.len() as u32;
                    let focused = wins.iter().filter(|w| w.focused).count();
                    checks.push(FieldCheck {
                        name: "list_windows".into(),
                        ok: true,
                        skipped: false,
                        detail: format!("{window_count} windows; focused_flag={focused}"),
                    });
                }
                Err(e) => checks.push(FieldCheck {
                    name: "list_windows".into(),
                    ok: false,
                    skipped: false,
                    detail: e.to_string(),
                }),
            }

            // Pick a non-shell window for deeper checks
            let target = pick_target(windows.as_ref().ok().map(|v| v.as_slice()));

            // activate GrabFocus honesty — false / NotSupported is expected on many
            // Wayland toolkits; field PASS means we reported it honestly, not
            // that focus moved.
            if let Some(ref t) = target {
                match activate(&session, t).await {
                    Ok(r) => checks.push(FieldCheck {
                        name: "activate_grab_focus".into(),
                        ok: true,
                        skipped: false,
                        detail: format!(
                            "GrabFocus returned {} on {:?} — {}",
                            r.ok, r.title, r.detail
                        ),
                    }),
                    Err(e) => {
                        let msg = e.to_string();
                        let expected = msg.contains("NotSupported")
                            || msg.contains("not supported")
                            || msg.contains("GrabFocus");
                        checks.push(FieldCheck {
                            name: "activate_grab_focus".into(),
                            ok: expected, // honest protocol error still "ok" for matrix
                            skipped: false,
                            detail: if expected {
                                format!("honest fail (expected on some Wayland apps): {msg}")
                            } else {
                                msg
                            },
                        });
                    }
                }
            } else {
                checks.push(FieldCheck {
                    name: "activate_grab_focus".into(),
                    ok: true,
                    skipped: true,
                    detail: "no non-shell window".into(),
                });
            }

            // snapshot + find
            if let Some(ref t) = target {
                let opts = SnapshotOpts {
                    max_depth: 4,
                    max_nodes: 60,
                    include_bounds: true,
                    include_actions: true,
                };
                match snapshot(&session, t, opts).await {
                    Ok((tree, stats)) => {
                        checks.push(FieldCheck {
                            name: "snapshot".into(),
                            ok: stats.nodes_emitted > 0,
                            skipped: false,
                            detail: format!(
                                "role={} nodes={} truncated={} max_depth_hit={}",
                                tree.role,
                                stats.nodes_emitted,
                                stats.truncated,
                                stats.max_depth_hit
                            ),
                        });
                        let q = FindQuery {
                            role: Some("button".into()),
                            max_results: 3,
                            ..Default::default()
                        };
                        match find_elements(&session, t, q, SnapshotOpts::default()).await {
                            Ok(hits) => checks.push(FieldCheck {
                                name: "find_button".into(),
                                ok: true,
                                skipped: hits.is_empty(),
                                detail: if hits.is_empty() {
                                    "no buttons in tree (ok for some apps)".into()
                                } else {
                                    format!("{} button(s); first={:?}", hits.len(), hits[0].name)
                                },
                            }),
                            Err(e) => checks.push(FieldCheck {
                                name: "find_button".into(),
                                ok: false,
                                skipped: false,
                                detail: e.to_string(),
                            }),
                        }
                    }
                    Err(e) => checks.push(FieldCheck {
                        name: "snapshot".into(),
                        ok: false,
                        skipped: false,
                        detail: e.to_string(),
                    }),
                }
            } else {
                checks.push(FieldCheck {
                    name: "snapshot".into(),
                    ok: true,
                    skipped: true,
                    detail: "no non-shell window".into(),
                });
            }

            // screenshot (writes a file — considered non-destructive capture)
            match screenshot(Some(&session), target.as_ref()).await {
                Ok(r) => checks.push(FieldCheck {
                    name: "screenshot".into(),
                    ok: r.bytes > 0,
                    skipped: false,
                    detail: format!(
                        "backend={} scope={} bytes={} path={}",
                        r.backend, r.scope, r.bytes, r.path
                    ),
                }),
                Err(e) => checks.push(FieldCheck {
                    name: "screenshot".into(),
                    ok: false,
                    skipped: false,
                    detail: e.to_string(),
                }),
            }
        }
        Err(e) => {
            checks.push(FieldCheck {
                name: "atspi_connect".into(),
                ok: false,
                skipped: false,
                detail: e.to_string(),
            });
        }
    }

    // Compositor helper presence (informational)
    checks.push(FieldCheck {
        name: "compositor_helpers".into(),
        ok: true,
        skipped: false,
        detail: format!("{:?}", identity.helpers),
    });

    if confirm_mutate {
        checks.push(FieldCheck {
            name: "mutate_note".into(),
            ok: true,
            skipped: true,
            detail: "confirm_mutate set — use selftest --confirm for mouse wiggle".into(),
        });
    }

    let failed: Vec<_> = checks
        .iter()
        .filter(|c| !c.ok && !c.skipped)
        .map(|c| c.name.as_str())
        .collect();
    let ok = failed.is_empty();
    let summary = if ok {
        format!(
            "field ok on {} ({}) — apps={} windows={} toolkits={:?}",
            identity.family,
            identity.desktop.as_deref().unwrap_or("?"),
            app_count,
            window_count,
            toolkits
        )
    } else {
        format!("field FAILED on {}: {}", identity.family, failed.join(", "))
    };

    info!(%summary, family = %identity.family, ok, "field report");

    Ok(FieldReport {
        ok,
        identity,
        checks,
        toolkits,
        app_count,
        window_count,
        summary,
        captured_at,
    })
}

fn pick_target(windows: Option<&[crate::types::WindowInfo]>) -> Option<TargetRef> {
    let wins = windows?;
    let is_shell = |w: &crate::types::WindowInfo| {
        matches!(w.app_name.as_deref(), Some("gnome-shell") | Some("gjs"))
            || w.title == "Main stage"
            || w.title.starts_with("Desktop Icons")
            || w.title.is_empty()
    };
    // Prefer rich a11y UIs over terminals for snapshot/find field checks.
    let prefer = |w: &crate::types::WindowInfo| -> i32 {
        let blob = format!("{} {}", w.app_name.as_deref().unwrap_or(""), w.title).to_lowercase();
        if blob.contains("calculator") || blob.contains("geany") || blob.contains("gedit") {
            3
        } else if blob.contains("nautilus") || blob.contains("settings") || blob.contains("control")
        {
            2
        } else if blob.contains("ptyxis")
            || blob.contains("terminal")
            || blob.contains("konsole")
            || blob.contains("alacritty")
        {
            0
        } else {
            1
        }
    };
    let mut candidates: Vec<_> = wins.iter().filter(|w| !is_shell(w)).collect();
    candidates.sort_by_key(|w| std::cmp::Reverse(prefer(w)));
    candidates
        .into_iter()
        .next()
        .map(|w| TargetRef::Id(w.id.clone()))
}

/// Detect session family and helper binaries (pure-ish — filesystem + env).
pub fn detect_identity() -> SessionIdentity {
    let session_type = std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| {
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            "wayland".into()
        } else if std::env::var_os("DISPLAY").is_some() {
            "x11".into()
        } else {
            "unknown".into()
        }
    });

    let desktop = std::env::var("XDG_CURRENT_DESKTOP")
        .ok()
        .filter(|s| !s.is_empty());
    let desktop_session = std::env::var("DESKTOP_SESSION")
        .ok()
        .filter(|s| !s.is_empty());
    let wayland_display = std::env::var("WAYLAND_DISPLAY").ok();
    let display = std::env::var("DISPLAY").ok();

    let family = classify_family(desktop.as_deref(), desktop_session.as_deref());

    let helper_names = [
        "hyprctl",
        "swaymsg",
        "i3-msg",
        "kdotool",
        "qdbus",
        "qdbus6",
        "grim",
        "slurp",
        "ydotool",
        "xdotool",
        "wtype",
        "gnome-screenshot",
        "gtk-launch",
        "gio",
    ];
    let mut helpers = BTreeMap::new();
    for h in helper_names {
        helpers.insert(h.to_string(), which(h));
    }

    let mut session_files = Vec::new();
    for dir in ["/usr/share/wayland-sessions", "/usr/share/xsessions"] {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for e in rd.flatten() {
                if let Some(n) = e.file_name().to_str() {
                    if n.ends_with(".desktop") {
                        session_files.push(n.to_string());
                    }
                }
            }
        }
    }
    session_files.sort();

    SessionIdentity {
        session_type,
        desktop,
        desktop_session,
        wayland_display,
        display,
        family,
        helpers,
        session_files,
    }
}

/// Map env desktop strings to a coarse family label.
pub fn classify_family(desktop: Option<&str>, session: Option<&str>) -> String {
    let blob = format!(
        "{} {}",
        desktop.unwrap_or("").to_lowercase(),
        session.unwrap_or("").to_lowercase()
    );
    if blob.contains("hypr") {
        "hyprland".into()
    } else if blob.contains("plasma") || blob.contains("kde") {
        "plasma".into()
    } else if blob.contains("sway") {
        "sway".into()
    } else if blob.contains("i3") {
        "i3".into()
    } else if blob.contains("gnome") || blob.contains("ubuntu") {
        "gnome".into()
    } else if blob.contains("xfce") {
        "xfce".into()
    } else if blob.trim().is_empty() {
        "unknown".into()
    } else {
        "other".into()
    }
}

fn which(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|dir| dir.join(bin).is_file()))
        .unwrap_or(false)
}

/// Markdown row helper for docs (pure).
pub fn markdown_summary(report: &FieldReport) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "### {} — {} ({})",
        report.identity.family,
        report.identity.desktop.as_deref().unwrap_or("?"),
        report.identity.session_type
    ));
    lines.push(format!("- **Captured:** {}", report.captured_at));
    lines.push(format!(
        "- **Result:** {}",
        if report.ok { "PASS" } else { "FAIL" }
    ));
    lines.push(format!(
        "- **Apps / windows:** {} / {}",
        report.app_count, report.window_count
    ));
    lines.push(format!("- **Toolkits:** `{:?}`", report.toolkits));
    lines.push("- **Checks:**".into());
    for c in &report.checks {
        let mark = if c.skipped {
            "skip"
        } else if c.ok {
            "ok"
        } else {
            "FAIL"
        };
        lines.push(format!("  - `{}` **{}** — {}", c.name, mark, c.detail));
    }
    lines.push(format!("- **Summary:** {}", report.summary));
    lines.join("\n")
}

/// Shell out only for optional version strings (best-effort, never fails the report).
#[allow(dead_code)]
fn try_version(bin: &str) -> Option<String> {
    let out = Command::new(bin).arg("--version").output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout);
    Some(s.lines().next()?.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_gnome_ubuntu() {
        assert_eq!(
            classify_family(Some("ubuntu:GNOME"), Some("ubuntu")),
            "gnome"
        );
    }

    #[test]
    fn classify_plasma() {
        assert_eq!(classify_family(Some("KDE"), Some("plasma")), "plasma");
        assert_eq!(
            classify_family(Some("plasma"), Some("plasmawayland")),
            "plasma"
        );
    }

    #[test]
    fn classify_hyprland() {
        assert_eq!(
            classify_family(Some("Hyprland"), Some("hyprland")),
            "hyprland"
        );
    }

    #[test]
    fn markdown_contains_family() {
        let r = FieldReport {
            ok: true,
            identity: SessionIdentity {
                session_type: "wayland".into(),
                desktop: Some("ubuntu:GNOME".into()),
                desktop_session: Some("ubuntu".into()),
                wayland_display: Some("wayland-0".into()),
                display: Some(":0".into()),
                family: "gnome".into(),
                helpers: BTreeMap::new(),
                session_files: vec!["ubuntu.desktop".into()],
            },
            checks: vec![FieldCheck {
                name: "doctor".into(),
                ok: true,
                skipped: false,
                detail: "ok".into(),
            }],
            toolkits: BTreeMap::from([("gtk".into(), 3)]),
            app_count: 3,
            window_count: 5,
            summary: "field ok".into(),
            captured_at: "now".into(),
        };
        let md = markdown_summary(&r);
        assert!(md.contains("gnome"));
        assert!(md.contains("PASS"));
    }
}
