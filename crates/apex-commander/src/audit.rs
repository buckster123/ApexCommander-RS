//! Mutation audit log — JSONL append-only trail.
//!
//! Path: `$APEX_COMMANDER_STATE_DIR/audit.jsonl` (default under XDG data).
//! Mutations must call [`ensure_writable`] *before* the desktop side effect.

use std::fs::OpenOptions;
use std::io::Write;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::error::{HarnessError, Result};
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

/// Create the log file (mode `0600`) so a later append is likely to succeed.
pub fn ensure_writable() -> std::io::Result<()> {
    ensure_state_dir()?;
    let path = audit_log_path();
    let _f = OpenOptions::new().create(true).append(true).open(&path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

/// Map I/O failure into a harness error (pre-mutation fail-closed).
pub fn require_writable() -> Result<()> {
    ensure_writable().map_err(|e| HarnessError::Other(format!("audit log not writable: {e}")))
}

/// Append a mutation event.
pub fn append(event: &AuditEvent) -> std::io::Result<()> {
    ensure_writable()?;
    let path = audit_log_path();
    let mut f = OpenOptions::new().create(true).append(true).open(&path)?;
    serde_json::to_writer(&mut f, event).map_err(std::io::Error::other)?;
    f.write_all(b"\n")?;
    f.flush()?;
    Ok(())
}

/// Build + append. Returns the I/O error — callers must not swallow it silently.
pub fn try_record(
    tool: &str,
    mutation: MutationClass,
    target: Option<serde_json::Value>,
    ok: bool,
    detail: impl Into<String>,
) -> std::io::Result<()> {
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
    append(&event)
}

/// Record after a mutation. On write failure, append a stated degrade to `detail`.
pub fn record_after(
    tool: &str,
    mutation: MutationClass,
    target: Option<serde_json::Value>,
    ok: bool,
    detail: &str,
) -> String {
    match try_record(tool, mutation, target, ok, detail) {
        Ok(()) => detail.to_string(),
        Err(e) => format!("{detail}; WARNING audit log write failed: {e}"),
    }
}

/// Detail for typed input: character count only — never the text.
pub fn typed_chars_detail(backend: &str, n: usize) -> String {
    format!("{backend} typed {n} char(s)")
}

/// Pure helper for tests: serialize one line.
pub fn event_to_line(event: &AuditEvent) -> std::result::Result<String, serde_json::Error> {
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
        std::env::set_var("APEX_COMMANDER_STATE_DIR", dir.path());
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
        std::env::remove_var("APEX_COMMANDER_STATE_DIR");
    }

    #[test]
    fn typed_detail_has_no_secrets() {
        let d = typed_chars_detail("Ydotool", 12);
        assert_eq!(d, "Ydotool typed 12 char(s)");
        assert!(!d.contains("secret"));
    }
}
