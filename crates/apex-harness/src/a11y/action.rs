//! AT-SPI hands — element actions, value set, type into.

use atspi::proxy::proxy_ext::ProxyExt;
use serde_json::json;
use tracing::debug;

use crate::a11y::session::AtspiSession;
use crate::a11y::walk::bounds_of;
use crate::audit;
use crate::error::{HarnessError, Result};
use crate::policy::{self, SensitiveConfig};
use crate::types::{ActionResult, MutationClass};

/// Perform a named (or indexed) AT-SPI action on an element.
///
/// Prefer this over coordinate clicks. Action names are case-insensitive
/// (e.g. `click`, `Click`, `press`).
pub async fn do_action(
    session: &AtspiSession,
    element_id: &str,
    action: Option<&str>,
    index: Option<i32>,
) -> Result<ActionResult> {
    let cfg = SensitiveConfig::load();
    policy::guard_name(element_id, &cfg)?;

    let proxy = session.proxy_for_id(element_id).await?;
    let proxies = proxy
        .proxies()
        .await
        .map_err(|e| HarnessError::Unavailable(format!("interfaces: {e}")))?;
    let action_proxy = proxies.action().await.map_err(|e| {
        HarnessError::Unavailable(format!(
            "no Action interface on {element_id}: {e} — element may not be actionable via a11y"
        ))
    })?;

    let actions = action_proxy
        .get_actions()
        .await
        .map_err(|e| HarnessError::Other(format!("GetActions: {e}")))?;

    if actions.is_empty() {
        let r = ActionResult {
            ok: false,
            id: element_id.into(),
            action: None,
            detail: "element exposes Action interface but has zero actions".into(),
        };
        audit::record(
            "do_action",
            MutationClass::Mutating,
            Some(json!({"id": element_id})),
            false,
            &r.detail,
        );
        return Ok(r);
    }

    let idx = if let Some(i) = index {
        if i < 0 || i as usize >= actions.len() {
            return Err(HarnessError::NotFound(format!(
                "action index {i} out of range 0..{}",
                actions.len()
            )));
        }
        i
    } else if let Some(name) = action {
        actions
            .iter()
            .position(|a| a.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| {
                let names: Vec<_> = actions.iter().map(|a| a.name.as_str()).collect();
                HarnessError::NotFound(format!(
                    "action {name:?} not found; available: {}",
                    names.join(", ")
                ))
            })? as i32
    } else {
        // Default: prefer Click/Press/Activate, else first action.
        actions
            .iter()
            .position(|a| {
                let n = a.name.to_lowercase();
                n == "click" || n == "press" || n == "activate" || n == "default"
            })
            .unwrap_or(0) as i32
    };

    let chosen = actions[idx as usize].name.clone();
    let ok = action_proxy
        .do_action(idx)
        .await
        .map_err(|e| HarnessError::Other(format!("DoAction({idx}): {e}")))?;

    let detail = if ok {
        format!("DoAction({idx}={chosen}) returned true")
    } else {
        format!("DoAction({idx}={chosen}) returned false")
    };
    debug!(%element_id, %chosen, ok, "do_action");

    audit::record(
        "do_action",
        MutationClass::Mutating,
        Some(json!({"id": element_id, "action": chosen, "index": idx})),
        ok,
        &detail,
    );

    Ok(ActionResult {
        ok,
        id: element_id.into(),
        action: Some(chosen),
        detail,
    })
}

/// Set a numeric Value interface (sliders, spinbuttons).
pub async fn set_value(
    session: &AtspiSession,
    element_id: &str,
    value: f64,
) -> Result<ActionResult> {
    let cfg = SensitiveConfig::load();
    policy::guard_name(element_id, &cfg)?;

    let proxy = session.proxy_for_id(element_id).await?;
    let proxies = proxy
        .proxies()
        .await
        .map_err(|e| HarnessError::Unavailable(format!("interfaces: {e}")))?;
    let value_proxy = proxies.value().await.map_err(|e| {
        HarnessError::Unavailable(format!("no Value interface on {element_id}: {e}"))
    })?;

    value_proxy
        .set_current_value(value)
        .await
        .map_err(|e| HarnessError::Other(format!("SetCurrentValue: {e}")))?;

    let detail = format!("set current value to {value}");
    audit::record(
        "set_value",
        MutationClass::Mutating,
        Some(json!({"id": element_id, "value": value})),
        true,
        &detail,
    );

    Ok(ActionResult {
        ok: true,
        id: element_id.into(),
        action: Some("set_value".into()),
        detail,
    })
}

/// Type / set text into an element.
///
/// Prefer `EditableText.SetTextContents`. Falls back to delete+insert when set
/// fails. Coordinate typing via input backends is a separate tool.
pub async fn type_into(
    session: &AtspiSession,
    element_id: &str,
    text: &str,
    append: bool,
) -> Result<ActionResult> {
    let cfg = SensitiveConfig::load();
    policy::guard_name(element_id, &cfg)?;

    let proxy = session.proxy_for_id(element_id).await?;
    // Best-effort focus first.
    if let Ok(proxies) = proxy.proxies().await {
        if let Ok(component) = proxies.component().await {
            let _ = component.grab_focus().await;
        }
    }

    let proxies = proxy
        .proxies()
        .await
        .map_err(|e| HarnessError::Unavailable(format!("interfaces: {e}")))?;

    let editable = proxies.editable_text().await.map_err(|e| {
        HarnessError::Unavailable(format!(
            "no EditableText on {element_id}: {e} — try type_text input fallback or pick a text field"
        ))
    })?;

    let ok = if append {
        // Insert at end: get length from Text if present, else position -1 / large.
        let pos = if let Ok(text_if) = proxies.text().await {
            text_if.character_count().await.unwrap_or(0)
        } else {
            0
        };
        editable
            .insert_text(pos, text, text.chars().count() as i32)
            .await
            .map_err(|e| HarnessError::Other(format!("InsertText: {e}")))?
    } else {
        match editable.set_text_contents(text).await {
            Ok(v) => v,
            Err(e) => {
                debug!(error = %e, "SetTextContents failed — try delete+insert");
                // Clear then insert.
                if let Ok(text_if) = proxies.text().await {
                    if let Ok(n) = text_if.character_count().await {
                        let _ = editable.delete_text(0, n).await;
                    }
                }
                editable
                    .insert_text(0, text, text.chars().count() as i32)
                    .await
                    .map_err(|e2| {
                        HarnessError::Other(format!(
                            "SetTextContents failed ({e}); InsertText also failed: {e2}"
                        ))
                    })?
            }
        }
    };

    let detail = if ok {
        format!(
            "{} {} char(s) via EditableText",
            if append { "appended" } else { "set" },
            text.chars().count()
        )
    } else {
        "EditableText returned false".into()
    };

    audit::record(
        "type_into",
        MutationClass::Mutating,
        Some(json!({
            "id": element_id,
            "chars": text.chars().count(),
            "append": append,
        })),
        ok,
        &detail,
    );

    Ok(ActionResult {
        ok,
        id: element_id.into(),
        action: Some(if append {
            "append_text".into()
        } else {
            "set_text".into()
        }),
        detail,
    })
}

/// Click the first matching action, resolving by element id (convenience).
pub async fn click_element(session: &AtspiSession, element_id: &str) -> Result<ActionResult> {
    do_action(session, element_id, Some("click"), None).await
}

/// Return element id + bounds for coordinate fallbacks (no mutation).
pub async fn element_bounds(
    session: &AtspiSession,
    element_id: &str,
) -> Result<crate::types::Bounds> {
    let proxy = session.proxy_for_id(element_id).await?;
    bounds_of(&proxy)
        .await
        .ok_or_else(|| HarnessError::Unavailable(format!("no bounds for {element_id}")))
}
