//! Environment readiness probe.
//!
//! Agents and humans run `doctor` before any GUI action. Results are structured
//! and machine-readable so an MCP host can decide whether to proceed.

use serde::{Deserialize, Serialize};

use crate::a11y::probe_atspi;
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

/// Run a best-effort environment probe (async — probes live AT-SPI).
pub async fn run_doctor() -> DoctorReport {
    let session = detect_session();
    let desktop = detect_desktop();
    let session_ok = !matches!(session, SessionKind::Unknown);

    let mut session_cap = Capability {
        name: "session_detect".into(),
        available: session_ok,
        detail: Some(format!("{session:?}").to_lowercase()),
    };
    if !session_ok {
        session_cap.detail = Some(
            "neither WAYLAND_DISPLAY nor DISPLAY set — headless or missing session env".into(),
        );
    }

    let atspi_cap = probe_atspi().await;
    let atspi_ok = atspi_cap.available;

    let capabilities = vec![
        session_cap,
        atspi_cap,
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
            available: atspi_ok,
            detail: Some(if atspi_ok {
                "via AT-SPI application/frame roots (S1)".into()
            } else {
                "depends on AT-SPI".into()
            }),
        },
    ];

    let mut recommendations = Vec::new();
    if !session_ok {
        recommendations
            .push("Run inside a graphical session (or export DISPLAY / WAYLAND_DISPLAY).".into());
    }
    if !atspi_ok {
        recommendations.push(
            "AT-SPI bus unreachable — ensure at-spi2-core is installed and an AT is enabled \
             (this harness sets session IsEnabled on connect)."
                .into(),
        );
    } else {
        recommendations
            .push("AT-SPI eyes ready: try `list-windows`, `snapshot --name <app>`, `find`.".into());
    }
    recommendations.push("Input injection + screenshots land in S2 — see BACKLOG.md.".into());

    let ok = session_ok && atspi_ok;
    let summary = if ok {
        format!(
            "session={session:?} desktop={} — AT-SPI ready (input/capture still S2)",
            desktop.as_deref().unwrap_or("unknown")
        )
    } else if !session_ok {
        "no graphical session detected — cannot drive a desktop from here".into()
    } else {
        "graphical session present but AT-SPI unavailable — see capabilities".into()
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

    #[tokio::test]
    async fn doctor_report_is_json_serializable() {
        let r = run_doctor().await;
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
