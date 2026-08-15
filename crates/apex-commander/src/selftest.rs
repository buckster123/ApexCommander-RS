//! Structured readiness + smoke suite for agents and humans.
//!
//! Default mode is **non-destructive**: doctor, discovery, snapshot.
//! Mutating checks (mouse wiggle, optional safe click) require `confirm_mutate`.

use serde::{Deserialize, Serialize};
use tracing::info;

use crate::a11y::{find_elements, list_apps, list_windows, snapshot, AtspiSession};
use crate::doctor::{run_doctor, DoctorReport};
use crate::error::Result;
use crate::input::{mouse_move, probe_input};
use crate::types::{FindQuery, SnapshotOpts, TargetRef};

/// One step in the selftest report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelftestStep {
    pub name: String,
    pub ok: bool,
    pub detail: String,
    /// When true, step was skipped (headless / no backend / no confirm).
    #[serde(default)]
    pub skipped: bool,
}

/// Full selftest report — machine-readable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelftestReport {
    pub ok: bool,
    pub steps: Vec<SelftestStep>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doctor: Option<DoctorReport>,
    pub summary: String,
}

/// Options for [`run_selftest`].
#[derive(Debug, Clone, Default)]
pub struct SelftestOpts {
    /// Allow mutating steps (mouse wiggle; never types secrets).
    pub confirm_mutate: bool,
    /// Preferred app/window name for snapshot (default: first non-shell window or skip).
    pub target_name: Option<String>,
}

/// Run the selftest suite.
pub async fn run_selftest(opts: SelftestOpts) -> Result<SelftestReport> {
    let mut steps = Vec::new();

    // 1. Doctor
    let doctor = run_doctor().await;
    steps.push(SelftestStep {
        name: "doctor".into(),
        ok: doctor.ok,
        detail: doctor.summary.clone(),
        skipped: false,
    });

    // 2. AT-SPI connect + list
    let session = match AtspiSession::connect().await {
        Ok(s) => {
            steps.push(SelftestStep {
                name: "atspi_connect".into(),
                ok: true,
                detail: "connected".into(),
                skipped: false,
            });
            Some(s)
        }
        Err(e) => {
            steps.push(SelftestStep {
                name: "atspi_connect".into(),
                ok: false,
                detail: e.to_string(),
                skipped: false,
            });
            None
        }
    };

    if let Some(ref session) = session {
        // list_apps
        match list_apps(session).await {
            Ok(apps) => steps.push(SelftestStep {
                name: "list_apps".into(),
                ok: !apps.is_empty(),
                detail: format!("{} application root(s)", apps.len()),
                skipped: false,
            }),
            Err(e) => steps.push(SelftestStep {
                name: "list_apps".into(),
                ok: false,
                detail: e.to_string(),
                skipped: false,
            }),
        }

        // list_windows
        let windows = list_windows(session).await;
        match &windows {
            Ok(wins) => steps.push(SelftestStep {
                name: "list_windows".into(),
                ok: true,
                detail: format!("{} window(s)", wins.len()),
                skipped: false,
            }),
            Err(e) => steps.push(SelftestStep {
                name: "list_windows".into(),
                ok: false,
                detail: e.to_string(),
                skipped: false,
            }),
        }

        // snapshot of a safe target
        let target = resolve_selftest_target(opts.target_name.as_deref(), windows.as_ref().ok());
        match target {
            Some(t) => {
                let opts_snap = SnapshotOpts {
                    max_depth: 3,
                    max_nodes: 40,
                    include_bounds: true,
                    include_actions: true,
                };
                match snapshot(session, &t, opts_snap).await {
                    Ok((tree, stats)) => {
                        steps.push(SelftestStep {
                            name: "snapshot".into(),
                            ok: stats.nodes_emitted > 0,
                            detail: format!(
                                "role={} nodes={} truncated={}",
                                tree.role, stats.nodes_emitted, stats.truncated
                            ),
                            skipped: false,
                        });
                        // find anything with a role (smoke the pure path live)
                        let q = FindQuery {
                            role: Some(tree.role.clone()),
                            max_results: 1,
                            ..Default::default()
                        };
                        match find_elements(session, &t, q, SnapshotOpts::default()).await {
                            Ok(hits) => steps.push(SelftestStep {
                                name: "find_elements".into(),
                                ok: !hits.is_empty(),
                                detail: format!("{} hit(s)", hits.len()),
                                skipped: false,
                            }),
                            Err(e) => steps.push(SelftestStep {
                                name: "find_elements".into(),
                                ok: false,
                                detail: e.to_string(),
                                skipped: false,
                            }),
                        }
                    }
                    Err(e) => steps.push(SelftestStep {
                        name: "snapshot".into(),
                        ok: false,
                        detail: e.to_string(),
                        skipped: false,
                    }),
                }
            }
            None => steps.push(SelftestStep {
                name: "snapshot".into(),
                ok: true,
                detail: "no non-shell window to snapshot — skipped".into(),
                skipped: true,
            }),
        }
    }

    // Mutating: mouse wiggle if confirmed and input available
    let input = probe_input();
    if opts.confirm_mutate {
        if input.available {
            // Small relative-safe absolute nudge: read nothing; move to a fixed on-screen point then back is hard
            // without get-position. Do a short diagonal hop that humans can see.
            match mouse_move(session.as_ref(), 200, 200).await {
                Ok(d) => {
                    let _ = mouse_move(session.as_ref(), 220, 220).await;
                    steps.push(SelftestStep {
                        name: "mouse_wiggle".into(),
                        ok: true,
                        detail: d,
                        skipped: false,
                    });
                }
                Err(e) => steps.push(SelftestStep {
                    name: "mouse_wiggle".into(),
                    ok: false,
                    detail: e.to_string(),
                    skipped: false,
                }),
            }
        } else {
            steps.push(SelftestStep {
                name: "mouse_wiggle".into(),
                ok: true,
                detail: "no input backend — skipped (AT-SPI hands still OK)".into(),
                skipped: true,
            });
        }
    } else {
        steps.push(SelftestStep {
            name: "mouse_wiggle".into(),
            ok: true,
            detail: "skipped — pass confirm_mutate / --confirm to enable".into(),
            skipped: true,
        });
    }

    let failed: Vec<_> = steps
        .iter()
        .filter(|s| !s.ok && !s.skipped)
        .map(|s| s.name.as_str())
        .collect();
    let ok = failed.is_empty() && doctor.ok;
    let summary = if ok {
        format!(
            "selftest ok — {} step(s) ({} skipped)",
            steps.len(),
            steps.iter().filter(|s| s.skipped).count()
        )
    } else {
        format!("selftest FAILED: {}", failed.join(", "))
    };

    info!(%summary, ok, "selftest complete");

    Ok(SelftestReport {
        ok,
        steps,
        doctor: Some(doctor),
        summary,
    })
}

fn resolve_selftest_target(
    preferred: Option<&str>,
    windows: Option<&Vec<crate::types::WindowInfo>>,
) -> Option<TargetRef> {
    if let Some(name) = preferred {
        return Some(TargetRef::Name(name.to_string()));
    }
    let wins = windows?;
    let w = wins.iter().find(|w| {
        !matches!(w.app_name.as_deref(), Some("gnome-shell") | Some("gjs"))
            && w.title != "Main stage"
            && !w.title.starts_with("Desktop Icons")
    })?;
    Some(TargetRef::Id(w.id.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_skips_shell() {
        let wins = vec![
            crate::types::WindowInfo {
                id: "a".into(),
                title: "Main stage".into(),
                app_name: Some("gnome-shell".into()),
                pid: None,
                focused: true,
                bounds: None,
                a11y_root: true,
                role: None,
            },
            crate::types::WindowInfo {
                id: "b".into(),
                title: "Editor".into(),
                app_name: Some("geany".into()),
                pid: None,
                focused: false,
                bounds: None,
                a11y_root: true,
                role: None,
            },
        ];
        let t = resolve_selftest_target(None, Some(&wins)).unwrap();
        assert_eq!(t, TargetRef::Id("b".into()));
    }
}
