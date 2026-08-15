//! Environment readiness probe.
//!
//! Agents and humans run `doctor` before any GUI action. Results are structured
//! and machine-readable so an MCP host can decide whether to proceed.

use serde::{Deserialize, Serialize};

use crate::a11y::probe_atspi;
use crate::audit;
use crate::capture::probe_capture;
use crate::input::probe_input;
use crate::paths::audit_log_path;
use crate::policy::{self, SensitiveConfig};
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

/// Run a best-effort environment probe (async — probes live AT-SPI / capture).
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
    let input_cap = probe_input();
    let capture_cap = probe_capture().await;

    let cfg = SensitiveConfig::load();
    let audit_ok = audit::ensure_writable().is_ok();
    let policy_cap = Capability {
        name: "policy".into(),
        available: true,
        detail: Some(format!(
            "denylist {} pattern(s); allow_override={}; config={}",
            cfg.patterns().len(),
            cfg.allow_override,
            policy::sensitive_config_path().display()
        )),
    };
    let audit_cap = Capability {
        name: "audit".into(),
        available: audit_ok,
        detail: Some(format!(
            "{}{}",
            audit_log_path().display(),
            if audit_ok {
                " (writable)"
            } else {
                " (not writable — mutations will refuse)"
            }
        )),
    };

    let capabilities = vec![
        session_cap,
        atspi_cap,
        input_cap.clone(),
        capture_cap.clone(),
        policy_cap,
        audit_cap,
        Capability {
            name: "window_backend".into(),
            available: atspi_ok,
            detail: Some(if atspi_ok {
                "via AT-SPI application/frame roots".into()
            } else {
                "depends on AT-SPI".into()
            }),
        },
        Capability {
            name: "element_actions".into(),
            available: atspi_ok,
            detail: Some(if atspi_ok {
                "do_action / type_into / set_value via AT-SPI".into()
            } else {
                "requires AT-SPI".into()
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
        recommendations.push(
            "Prefer do_action / type_into over coordinate clicks; snapshot before find.".into(),
        );
    }
    if !input_cap.available {
        recommendations.push(
            "No ydotool/xdotool/wtype — coordinate input unavailable; AT-SPI hands still work."
                .into(),
        );
    }
    if !capture_cap.available {
        recommendations
            .push("No screenshot backend — install xdg-desktop-portal and/or grim.".into());
    }

    // Ready for GUI work when we have a session + AT-SPI (hands/eyes). Capture is bonus.
    let ok = session_ok && atspi_ok;
    let summary = if ok {
        format!(
            "session={session:?} desktop={} — AT-SPI hands+eyes ready; capture={} input={}",
            desktop.as_deref().unwrap_or("unknown"),
            if capture_cap.available { "ok" } else { "no" },
            if input_cap.available { "ok" } else { "no" },
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
