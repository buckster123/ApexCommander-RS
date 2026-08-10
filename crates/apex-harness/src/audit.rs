//! Mutation audit log — JSONL append-only trail.
//!
//! Path: `$APEX_HARNESS_STATE_DIR/audit.jsonl` (default under XDG data).

use std::fs::OpenOptions;
use std::io::Write;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::paths::{audit_log_path, ensure_state_dir};
use crate::types::MutationClass;

/// One mutation event written to the audit trail.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditEvent {
    /// RFC3339 timestamp (UTC).
    pub ts: String,
    /// Tool / operation name (`do_action`, `type_into`, `screenshot`, …).
    pub tool: String,
    pub mutation: MutationClass,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<serde_json::Value>,
    /// `ok` or `error`.
    pub result: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Append a mutation event. Failures are logged and returned — callers may
/// surface them but must not invent a silent success for the mutation itself.
pub fn append(event: &AuditEvent) -> std::io::Result<()> {
    ensure_state_dir()?;
    let path = audit_log_path();
    let mut f = OpenOptions::new().create(true).append(true).open(&path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    serde_json::to_writer(&mut f, event).map_err(std::io::Error::other)?;
    f.write_all(b"\n")?;
    f.flush()?;
    Ok(())
}

/// Build + append a standard event. Logs a warning if the write fails.
pub fn record(
    tool: &str,
    mutation: MutationClass,
    target: Option<serde_json::Value>,
    ok: bool,
    detail: impl Into<String>,
) {
    let event = AuditEvent {
        ts: Utc::now().to_rfc3339(),
        tool: tool.into(),
        mutation,
        target,
        result: if ok { "ok".into() } else { "error".into() },
        detail: {
            let d = detail.into();
            if d.is_empty() {
                None
            } else {
                Some(d)
            }
        },
    };
    if let Err(e) = append(&event) {
        warn!(error = %e, tool, "failed to write audit log");
    }
}

/// Pure helper for tests: serialize one line.
pub fn event_to_line(event: &AuditEvent) -> Result<String, serde_json::Error> {
    serde_json::to_string(event)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn event_serializes_one_line() {
        let e = AuditEvent {
            ts: "2026-08-10T00:00:00Z".into(),
            tool: "do_action".into(),
            mutation: MutationClass::Mutating,
            target: Some(serde_json::json!({"id": ":1.1|/x", "action": "Click"})),
            result: "ok".into(),
            detail: Some("index 0".into()),
        };
        let line = event_to_line(&e).unwrap();
        assert!(line.contains("do_action"));
        assert!(!line.contains('\n'));
        let back: AuditEvent = serde_json::from_str(&line).unwrap();
        assert_eq!(back.tool, "do_action");
    }

    #[test]
    fn append_writes_jsonl() {
        let dir = tempdir().unwrap();
        std::env::set_var("APEX_HARNESS_STATE_DIR", dir.path());
        let e = AuditEvent {
            ts: Utc::now().to_rfc3339(),
            tool: "type_into".into(),
            mutation: MutationClass::Mutating,
            target: None,
            result: "ok".into(),
            detail: None,
        };
        append(&e).unwrap();
        append(&e).unwrap();
        let body = std::fs::read_to_string(dir.path().join("audit.jsonl")).unwrap();
        assert_eq!(body.lines().count(), 2);
        std::env::remove_var("APEX_HARNESS_STATE_DIR");
    }
}
