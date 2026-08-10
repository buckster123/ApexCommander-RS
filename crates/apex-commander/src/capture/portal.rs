//! XDG Desktop Portal screenshot (`org.freedesktop.portal.Screenshot`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_lite::StreamExt;
use tracing::debug;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Str, Value};

use crate::error::{HarnessError, Result};

/// Request a non-interactive portal screenshot and copy it to `dest`.
pub async fn portal_screenshot(dest: &Path) -> Result<PathBuf> {
    let conn = zbus::Connection::session()
        .await
        .map_err(|e| HarnessError::Unavailable(format!("session bus: {e}")))?;

    let token = format!("adh{}", uuid::Uuid::new_v4().simple());
    let unique = conn
        .unique_name()
        .ok_or_else(|| HarnessError::Unavailable("no unique bus name".into()))?
        .as_str()
        .trim_start_matches(':')
        .replace('.', "_");
    let request_path = format!("/org/freedesktop/portal/desktop/request/{unique}/{token}");

    let mut opts: HashMap<String, Value<'_>> = HashMap::new();
    opts.insert("handle_token".into(), Value::Str(Str::from(token.as_str())));
    opts.insert("interactive".into(), Value::Bool(false));

    let mut stream = zbus::MessageStream::from(&conn);

    let proxy = zbus::Proxy::new(
        &conn,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.Screenshot",
    )
    .await
    .map_err(|e| HarnessError::Unavailable(format!("portal proxy: {e}")))?;

    let handle: OwnedObjectPath = proxy
        .call("Screenshot", &("", opts))
        .await
        .map_err(|e| HarnessError::Unavailable(format!("portal Screenshot: {e}")))?;

    debug!(%handle, expect = %request_path, "portal screenshot request");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(45);
    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        if left.is_zero() {
            return Err(HarnessError::Unavailable(
                "portal screenshot timed out waiting for Response".into(),
            ));
        }

        let msg = tokio::time::timeout(left, stream.next())
            .await
            .map_err(|_| HarnessError::Unavailable("portal screenshot timed out".into()))?
            .ok_or_else(|| HarnessError::Unavailable("message stream closed".into()))?
            .map_err(|e| HarnessError::Other(format!("dbus stream: {e}")))?;

        if msg.message_type() != zbus::message::Type::Signal {
            continue;
        }
        let hdr = msg.header();
        if hdr.interface().map(|i| i.as_str()) != Some("org.freedesktop.portal.Request") {
            continue;
        }
        if hdr.member().map(|m| m.as_str()) != Some("Response") {
            continue;
        }
        let path = hdr
            .path()
            .map(|p| p.as_str().to_string())
            .unwrap_or_default();
        if path != request_path && path != handle.as_str() {
            continue;
        }

        let (code, results): (u32, HashMap<String, OwnedValue>) = msg
            .body()
            .deserialize()
            .map_err(|e| HarnessError::Other(format!("decode Response: {e}")))?;

        if code != 0 {
            return Err(HarnessError::Unavailable(format!(
                "portal screenshot cancelled or failed (response code {code})"
            )));
        }

        let uri = results
            .get("uri")
            .map(owned_value_to_string)
            .transpose()?
            .ok_or_else(|| HarnessError::Unavailable("portal Response missing uri".into()))?;

        let src = uri_to_path(&uri)?;
        if !src.is_file() {
            return Err(HarnessError::Unavailable(format!(
                "portal uri path missing: {}",
                src.display()
            )));
        }

        if src != dest {
            std::fs::copy(&src, dest).map_err(HarnessError::Io)?;
        }
        return Ok(dest.to_path_buf());
    }
}

fn owned_value_to_string(v: &OwnedValue) -> Result<String> {
    // OwnedValue → Value<'static> via clone into owned form.
    let value: Value<'_> = v
        .try_to_owned()
        .map_err(|e| HarnessError::Other(format!("uri value: {e}")))?
        .into();
    match value {
        Value::Str(s) => Ok(s.as_str().to_string()),
        other => Err(HarnessError::Other(format!(
            "uri value not a string: {other:?}"
        ))),
    }
}

fn uri_to_path(uri: &str) -> Result<PathBuf> {
    let path = uri
        .strip_prefix("file://")
        .ok_or_else(|| HarnessError::Other(format!("unexpected portal uri: {uri}")))?;
    Ok(PathBuf::from(percent_decode(path)))
}

fn percent_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let (Some(h), Some(l)) = (from_hex(b[i + 1]), from_hex(b[i + 2])) {
                out.push(char::from(h * 16 + l));
                i += 3;
                continue;
            }
        }
        out.push(b[i] as char);
        i += 1;
    }
    out
}

fn from_hex(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}
