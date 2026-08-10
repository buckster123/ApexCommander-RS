//! High-level discovery and perception operations over AT-SPI.

use atspi::proxy::accessible::{AccessibleProxy, ObjectRefExt};
use atspi::proxy::proxy_ext::ProxyExt;
use atspi::State;

use crate::a11y::find::find_in_tree;
use crate::a11y::id::encode_id;
use crate::a11y::session::AtspiSession;
use crate::a11y::walk::{
    bounds_of, has_state, id_of, is_active_or_focused, is_toplevel_role, nonempty, role_of,
    states_of, walk_compact, SnapshotStats,
};
use crate::error::{HarnessError, Result};
use crate::types::{
    A11yNode, ActivateResult, AppInfo, ElementHit, FindQuery, SnapshotOpts, TargetRef, WindowInfo,
};

/// List application roots on the accessibility bus.
pub async fn list_apps(session: &AtspiSession) -> Result<Vec<AppInfo>> {
    let root = session.registry_root().await?;
    let children = root
        .get_children()
        .await
        .map_err(|e| HarnessError::Unavailable(format!("list apps: {e}")))?;

    let mut apps = Vec::with_capacity(children.len());
    for child in children {
        if child.is_null() {
            continue;
        }
        let Ok(proxy) = child.as_accessible_proxy(session.connection()).await else {
            continue;
        };
        let name = nonempty(proxy.name().await.ok()).unwrap_or_else(|| "unknown".into());
        let id = id_of(&proxy);
        let bus = child.name_as_str().unwrap_or("");
        let pid = if bus.is_empty() {
            None
        } else {
            session.pid_for_bus(bus).await
        };
        let toolkit = toolkit_of(&proxy).await;
        let window_count = count_toplevels(session, &proxy).await;
        apps.push(AppInfo {
            id,
            name,
            pid,
            toolkit,
            window_count,
        });
    }
    apps.sort_by_key(|a| a.name.to_lowercase());
    Ok(apps)
}

/// List top-level windows (frame/window/dialog/…) across applications.
pub async fn list_windows(session: &AtspiSession) -> Result<Vec<WindowInfo>> {
    let root = session.registry_root().await?;
    let apps = root
        .get_children()
        .await
        .map_err(|e| HarnessError::Unavailable(format!("list windows: {e}")))?;

    let mut windows = Vec::new();
    for app_ref in apps {
        if app_ref.is_null() {
            continue;
        }
        let Ok(app) = app_ref.as_accessible_proxy(session.connection()).await else {
            continue;
        };
        let app_name = nonempty(app.name().await.ok());
        let bus = app_ref.name_as_str().unwrap_or("");
        let pid = if bus.is_empty() {
            None
        } else {
            session.pid_for_bus(bus).await
        };

        let Ok(frames) = app.get_children().await else {
            continue;
        };
        for frame_ref in frames {
            if frame_ref.is_null() {
                continue;
            }
            let Ok(frame) = frame_ref.as_accessible_proxy(session.connection()).await else {
                continue;
            };
            let role = role_of(&frame).await;
            if !is_toplevel_role(role) {
                continue;
            }

            let title = nonempty(frame.name().await.ok()).unwrap_or_default();
            let states = states_of(&frame).await;
            let focused = is_active_or_focused(&states);
            let bounds = bounds_of(&frame).await;
            windows.push(WindowInfo {
                id: id_of(&frame),
                title,
                app_name: app_name.clone(),
                pid,
                focused,
                bounds,
                a11y_root: true,
                role: Some(role.to_string()),
            });
        }
    }
    Ok(windows)
}

/// Best-effort frontmost window.
///
/// Order: (1) non-shell window with `active`/`focused`, (2) any focused window,
/// (3) non-shell window containing a focused descendant, (4) `NotFound`.
///
/// GNOME Shell's "Main stage" often reports focused without being a useful
/// agent target — it is deprioritized as shell chrome.
pub async fn frontmost(session: &AtspiSession) -> Result<WindowInfo> {
    let windows = list_windows(session).await?;

    if let Some(w) = windows.iter().find(|w| w.focused && !is_shell_chrome(w)) {
        return Ok(w.clone());
    }
    if let Some(w) = windows.iter().find(|w| w.focused) {
        // Shell chrome may be the only focused surface; still prefer a real app
        // via descendant scan before returning chrome.
        for w in windows.iter().filter(|w| !is_shell_chrome(w)) {
            if window_has_focused_descendant(session, &w.id).await {
                let mut hit = w.clone();
                hit.focused = true;
                return Ok(hit);
            }
        }
        return Ok(w.clone());
    }

    for w in windows.iter().filter(|w| !is_shell_chrome(w)) {
        if window_has_focused_descendant(session, &w.id).await {
            let mut hit = w.clone();
            hit.focused = true;
            return Ok(hit);
        }
    }

    Err(HarnessError::NotFound(
        "no active/focused window reported by AT-SPI (Wayland/GNOME often under-reports; try list_windows)"
            .into(),
    ))
}

fn is_shell_chrome(w: &WindowInfo) -> bool {
    matches!(w.app_name.as_deref(), Some("gnome-shell") | Some("gjs"))
        || w.title == "Main stage"
        || w.title.starts_with("Desktop Icons")
}

/// Focus / raise a window via AT-SPI `Component.GrabFocus`.
pub async fn activate(session: &AtspiSession, target: &TargetRef) -> Result<ActivateResult> {
    let window = resolve_window(session, target).await?;
    let proxy = session.proxy_for_id(&window.id).await?;
    let proxies = proxy
        .proxies()
        .await
        .map_err(|e| HarnessError::Unavailable(format!("interfaces: {e}")))?;
    let component = proxies.component().await.map_err(|e| {
        HarnessError::Unavailable(format!("no Component interface on {}: {e}", window.id))
    })?;

    let ok = component
        .grab_focus()
        .await
        .map_err(|e| HarnessError::Other(format!("GrabFocus failed on {}: {e}", window.id)))?;

    Ok(ActivateResult {
        ok,
        id: window.id,
        title: if window.title.is_empty() {
            None
        } else {
            Some(window.title)
        },
        detail: if ok {
            "GrabFocus returned true".into()
        } else {
            "GrabFocus returned false — window may not accept focus".into()
        },
    })
}

/// Compact a11y snapshot of a target window/app.
pub async fn snapshot(
    session: &AtspiSession,
    target: &TargetRef,
    opts: SnapshotOpts,
) -> Result<(A11yNode, SnapshotStats)> {
    let id = resolve_proxy_id(session, target).await?;
    let proxy = session.proxy_for_id(&id).await?;
    Ok(walk_compact(session, &proxy, &opts).await)
}

/// Find elements under a target (live walk → pure match).
pub async fn find_elements(
    session: &AtspiSession,
    target: &TargetRef,
    query: FindQuery,
    opts: SnapshotOpts,
) -> Result<Vec<ElementHit>> {
    // Walk with a generous budget for search; still capped.
    let mut opts = opts;
    if opts.max_nodes < 400 {
        opts.max_nodes = 400;
    }
    if opts.max_depth < 10 {
        opts.max_depth = 10;
    }
    let (tree, _stats) = snapshot(session, target, opts).await?;
    Ok(find_in_tree(&tree, &query))
}

/// Details of the currently focused accessible, if any, under a target (or desktop-wide).
pub async fn focused_element(
    session: &AtspiSession,
    target: Option<&TargetRef>,
) -> Result<ElementHit> {
    let target = target.cloned().unwrap_or(TargetRef::Frontmost);
    let hits = find_elements(
        session,
        &target,
        FindQuery {
            state: Some("focused".into()),
            max_results: 5,
            ..Default::default()
        },
        SnapshotOpts {
            max_depth: 12,
            max_nodes: 500,
            include_bounds: true,
            include_actions: true,
        },
    )
    .await?;

    hits.into_iter()
        .next()
        .ok_or_else(|| HarnessError::NotFound("no focused element in target tree".into()))
}

// ── resolvers ──────────────────────────────────────────────────────────

async fn resolve_window(session: &AtspiSession, target: &TargetRef) -> Result<WindowInfo> {
    match target {
        TargetRef::Frontmost => frontmost(session).await,
        TargetRef::Id(id) => {
            let windows = list_windows(session).await?;
            windows
                .into_iter()
                .find(|w| w.id == *id)
                .ok_or_else(|| HarnessError::NotFound(format!("window id not found: {id}")))
        }
        TargetRef::Name(name) => {
            let windows = list_windows(session).await?;
            let needle = name.to_lowercase();
            let matches: Vec<_> = windows
                .into_iter()
                .filter(|w| {
                    w.title.to_lowercase().contains(&needle)
                        || w.app_name
                            .as_deref()
                            .map(|a| a.to_lowercase().contains(&needle))
                            .unwrap_or(false)
                })
                .collect();
            match matches.len() {
                0 => Err(HarnessError::NotFound(format!(
                    "no window matching name '{name}'"
                ))),
                1 => Ok(matches.into_iter().next().unwrap()),
                n => {
                    let titles: Vec<_> = matches
                        .iter()
                        .take(5)
                        .map(|w| format!("'{}' ({})", w.title, w.id))
                        .collect();
                    Err(HarnessError::Ambiguous(format!(
                        "{n} windows match '{name}': {}",
                        titles.join(", ")
                    )))
                }
            }
        }
        TargetRef::Pid(pid) => {
            let windows = list_windows(session).await?;
            let matches: Vec<_> = windows
                .into_iter()
                .filter(|w| w.pid == Some(*pid))
                .collect();
            match matches.len() {
                0 => Err(HarnessError::NotFound(format!("no window for pid {pid}"))),
                1 => Ok(matches.into_iter().next().unwrap()),
                n => {
                    // Prefer a focused one if present.
                    if let Some(w) = matches.iter().find(|w| w.focused) {
                        return Ok(w.clone());
                    }
                    let titles: Vec<_> = matches
                        .iter()
                        .take(5)
                        .map(|w| format!("'{}'", w.title))
                        .collect();
                    Err(HarnessError::Ambiguous(format!(
                        "{n} windows for pid {pid}: {}",
                        titles.join(", ")
                    )))
                }
            }
        }
    }
}

async fn resolve_proxy_id(session: &AtspiSession, target: &TargetRef) -> Result<String> {
    match target {
        TargetRef::Id(id) => Ok(id.clone()),
        other => match resolve_window(session, other).await {
            Ok(w) => Ok(w.id),
            Err(win_err) => {
                if let TargetRef::Name(name) = other {
                    if let Ok(app) = resolve_app_by_name(session, name).await {
                        return Ok(app.id);
                    }
                }
                Err(win_err)
            }
        },
    }
}

async fn resolve_app_by_name(session: &AtspiSession, name: &str) -> Result<AppInfo> {
    let apps = list_apps(session).await?;
    let needle = name.to_lowercase();
    let matches: Vec<_> = apps
        .into_iter()
        .filter(|a| a.name.to_lowercase().contains(&needle))
        .collect();
    match matches.len() {
        0 => Err(HarnessError::NotFound(format!("no app matching '{name}'"))),
        1 => Ok(matches.into_iter().next().unwrap()),
        n => Err(HarnessError::Ambiguous(format!("{n} apps match '{name}'"))),
    }
}

async fn count_toplevels(session: &AtspiSession, app: &AccessibleProxy<'_>) -> u32 {
    let Ok(frames) = app.get_children().await else {
        return 0;
    };
    let mut n = 0u32;
    for f in frames {
        if f.is_null() {
            continue;
        }
        if let Ok(fp) = f.as_accessible_proxy(session.connection()).await {
            if is_toplevel_role(role_of(&fp).await) {
                n += 1;
            }
        }
    }
    n
}

async fn toolkit_of(proxy: &AccessibleProxy<'_>) -> Option<String> {
    let proxies = proxy.proxies().await.ok()?;
    let app = proxies.application().await.ok()?;
    let name = app.toolkit_name().await.ok()?;
    nonempty(Some(name))
}

async fn window_has_focused_descendant(session: &AtspiSession, window_id: &str) -> bool {
    // Iterative BFS — avoid recursive async futures.
    let mut queue: Vec<(String, u32)> = vec![(window_id.to_string(), 0)];
    let mut budget: u32 = 80;
    const MAX_DEPTH: u32 = 8;

    while let Some((id, depth)) = queue.pop() {
        if budget == 0 {
            break;
        }
        budget -= 1;
        let Ok(proxy) = session.proxy_for_id(&id).await else {
            continue;
        };
        if has_state(&proxy, State::Focused).await {
            return true;
        }
        if depth >= MAX_DEPTH {
            continue;
        }
        let Ok(children) = proxy.get_children().await else {
            continue;
        };
        for c in children {
            if budget == 0 {
                break;
            }
            if c.is_null() {
                continue;
            }
            let Some(bus) = c.name_as_str() else {
                continue;
            };
            queue.push((encode_id(bus, c.path_as_str()), depth + 1));
        }
    }
    false
}
