//! Environment readiness probe — the first vertical slice after bootstrap.
//!
//! Agents and humans run `doctor` before any GUI action. Results are structured
//! and machine-readable so an MCP host can decide whether to proceed.

use serde::{Deserialize, Serialize};

use crate::types::{Capability, SessionKind};

/// Full readiness report produced by [`run_doctor`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DoctorReport {
    pub ok: bool,
    pub session: SessionKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desktop: Option<String>,
    pub capabilities: Vec<Capability>,
    pub recommendations: Vec<String>,
    /// Human-readable summary line (also useful when JSON is not wanted).
    pub summary: String,
}

/// Run a best-effort environment probe.
///
/// v0 (bootstrap): detects session type from env only; AT-SPI / input / capture
/// backends are reported as not-yet-wired so agents get an honest degrade.
/// Later slices fill real probes without changing the report shape.
pub fn run_doctor() -> DoctorReport {
    let session = detect_session();
    let desktop = detect_desktop();

    let mut capabilities = vec![
        Capability {
            name: "session_detect".into(),
            available: !matches!(session, SessionKind::Unknown),
            detail: Some(format!("{session:?}").to_lowercase()),
        },
        Capability {
            name: "atspi".into(),
            available: false,
            detail: Some("not wired yet (S1)".into()),
        },
        Capability {
            name: "input".into(),
            available: false,
            detail: Some("not wired yet (S2)".into()),
        },
        Capability {
            name: "capture".into(),
            available: false,
            detail: Some("not wired yet (S2)".into()),
        },
        Capability {
            name: "window_backend".into(),
            available: false,
            detail: Some("not wired yet (S1)".into()),
        },
    ];

    // Session detection itself is a real probe even in S0.
    let session_ok = capabilities[0].available;
    if !session_ok {
        capabilities[0].detail = Some(
            "neither WAYLAND_DISPLAY nor DISPLAY set — headless or missing session env".into(),
        );
    }

    let mut recommendations = Vec::new();
    if !session_ok {
        recommendations
            .push("Run inside a graphical session (or export DISPLAY / WAYLAND_DISPLAY).".into());
    }
    recommendations.push(
        "S0 bootstrap only: AT-SPI, input, and capture land in later slices — see BACKLOG.md."
            .into(),
    );

    let ok = session_ok; // S0: "ready enough to keep scaffolding" != "ready for GUI control"
    let summary = if ok {
        format!(
            "session={session:?} desktop={} — core backends not yet wired (honest S0)",
            desktop.as_deref().unwrap_or("unknown")
        )
    } else {
        "no graphical session detected — cannot drive a desktop from here".into()
    };

    DoctorReport {
        ok,
        session,
        desktop,
        capabilities,
        recommendations,
        summary,
    }
}

fn detect_session() -> SessionKind {
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        SessionKind::Wayland
    } else if std::env::var_os("DISPLAY").is_some() {
        SessionKind::X11
    } else {
        SessionKind::Unknown
    }
}

fn detect_desktop() -> Option<String> {
    std::env::var("XDG_CURRENT_DESKTOP")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| {
            std::env::var("DESKTOP_SESSION")
                .ok()
                .filter(|s| !s.is_empty())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doctor_report_is_json_serializable() {
        let r = run_doctor();
        let v = serde_json::to_value(&r).expect("serialize");
        assert!(v.get("ok").is_some());
        assert!(v.get("capabilities").unwrap().as_array().unwrap().len() >= 4);
        assert!(v.get("summary").is_some());
    }

    #[test]
    fn session_kind_roundtrips() {
        for kind in [SessionKind::X11, SessionKind::Wayland, SessionKind::Unknown] {
            let s = serde_json::to_string(&kind).unwrap();
            let back: SessionKind = serde_json::from_str(&s).unwrap();
            assert_eq!(kind, back);
        }
    }
}
