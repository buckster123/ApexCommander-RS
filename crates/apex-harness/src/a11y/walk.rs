//! Live tree walking — build compact [`A11yNode`]s under depth/node budgets.
//!
//! Iteration (not recursive async) so the future stays sized.

use atspi::proxy::accessible::AccessibleProxy;
use atspi::proxy::proxy_ext::ProxyExt;
use atspi::{CoordType, Role, State};
use tracing::trace;

use crate::a11y::id::encode_id;
use crate::a11y::session::AtspiSession;
use crate::types::{A11yNode, Bounds, SnapshotOpts};

/// Stats returned alongside a snapshot for agent debugging.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SnapshotStats {
    pub nodes_emitted: u32,
    pub nodes_visited: u32,
    pub truncated: bool,
    pub max_depth_hit: bool,
}

/// Walk from `root` into a compact tree under `opts`.
pub async fn walk_compact(
    session: &AtspiSession,
    root: &AccessibleProxy<'_>,
    opts: &SnapshotOpts,
) -> (A11yNode, SnapshotStats) {
    let mut stats = SnapshotStats::default();
    let budget = opts.max_nodes.max(1);

    // Stack of work: (proxy id, depth). Build nodes in a flat arena, then link children.
    #[derive(Clone)]
    struct Frame {
        id: String,
        depth: u32,
    }

    let mut frames: Vec<Frame> = vec![Frame {
        id: id_of(root),
        depth: 0,
    }];
    let mut nodes: Vec<Option<A11yNode>> = Vec::new();
    let mut child_lists: Vec<Vec<usize>> = Vec::new();
    let mut i = 0usize;

    while i < frames.len() {
        if stats.nodes_emitted >= budget {
            stats.truncated = true;
            break;
        }
        let frame = frames[i].clone();
        stats.nodes_visited += 1;

        let proxy = match session.proxy_for_id(&frame.id).await {
            Ok(p) => p,
            Err(e) => {
                trace!(error = %e, id = %frame.id, "skip unreachable node");
                nodes.push(None);
                child_lists.push(Vec::new());
                i += 1;
                continue;
            }
        };

        let role = proxy
            .get_role()
            .await
            .map(|r| r.to_string())
            .unwrap_or_else(|_| "unknown".into());
        let name = nonempty(proxy.name().await.ok());
        let description = nonempty(proxy.description().await.ok());
        let states = states_of(&proxy).await;
        let value = value_of(&proxy).await;
        let actions = if opts.include_actions {
            actions_of(&proxy).await
        } else {
            vec![]
        };
        let bounds = if opts.include_bounds {
            bounds_of(&proxy).await
        } else {
            None
        };

        let mut child_idxs = Vec::new();
        if frame.depth < opts.max_depth && stats.nodes_emitted + 1 < budget {
            match proxy.get_children().await {
                Ok(refs) => {
                    for child_ref in refs {
                        if stats.nodes_emitted as usize + frames.len() - i > budget as usize {
                            stats.truncated = true;
                            break;
                        }
                        if child_ref.is_null() {
                            continue;
                        }
                        let bus = child_ref.name_as_str().unwrap_or("");
                        let path = child_ref.path_as_str();
                        if bus.is_empty() {
                            continue;
                        }
                        let child_id = encode_id(bus, path);
                        let child_frame_idx = frames.len();
                        frames.push(Frame {
                            id: child_id,
                            depth: frame.depth + 1,
                        });
                        child_idxs.push(child_frame_idx);
                    }
                }
                Err(e) => trace!(error = %e, "get_children failed"),
            }
        } else if frame.depth >= opts.max_depth {
            if let Ok(n) = proxy.child_count().await {
                if n > 0 {
                    stats.max_depth_hit = true;
                }
            }
        }

        stats.nodes_emitted += 1;
        nodes.push(Some(A11yNode {
            id: Some(frame.id),
            role,
            name,
            description,
            value,
            states,
            actions,
            bounds,
            children: vec![], // filled below
        }));
        child_lists.push(child_idxs);
        i += 1;
    }

    // Link children bottom-up (children indices are always greater than parent).
    for idx in (0..nodes.len()).rev() {
        let child_ids = &child_lists[idx];
        let mut kids = Vec::with_capacity(child_ids.len());
        for &c in child_ids {
            if c < nodes.len() {
                if let Some(Some(node)) = nodes.get_mut(c).map(|n| n.take()) {
                    kids.push(node);
                }
            }
        }
        if let Some(Some(node)) = nodes.get_mut(idx) {
            node.children = kids;
        }
    }

    let root_node = nodes.into_iter().next().flatten().unwrap_or(A11yNode {
        id: Some(id_of(root)),
        role: "unknown".into(),
        name: None,
        description: None,
        value: None,
        states: vec![],
        actions: vec![],
        bounds: None,
        children: vec![],
    });

    (root_node, stats)
}

pub(crate) fn id_of(proxy: &AccessibleProxy<'_>) -> String {
    let bus = proxy.inner().destination().as_str();
    let path = proxy.inner().path().as_str();
    encode_id(bus, path)
}

pub(crate) async fn states_of(proxy: &AccessibleProxy<'_>) -> Vec<String> {
    match proxy.get_state().await {
        Ok(set) => {
            let mut v: Vec<String> = set.iter().map(|s| s.to_string()).collect();
            v.sort();
            v
        }
        Err(_) => vec![],
    }
}

pub(crate) async fn bounds_of(proxy: &AccessibleProxy<'_>) -> Option<Bounds> {
    let proxies = proxy.proxies().await.ok()?;
    let component = proxies.component().await.ok()?;
    let (x, y, w, h) = component.get_extents(CoordType::Screen).await.ok()?;
    if w < 0 || h < 0 {
        return None;
    }
    Some(Bounds {
        x,
        y,
        width: w as u32,
        height: h as u32,
    })
}

async fn actions_of(proxy: &AccessibleProxy<'_>) -> Vec<String> {
    let Ok(proxies) = proxy.proxies().await else {
        return vec![];
    };
    let Ok(action) = proxies.action().await else {
        return vec![];
    };
    match action.get_actions().await {
        Ok(list) => list.into_iter().map(|a| a.name).collect(),
        Err(_) => vec![],
    }
}

async fn value_of(proxy: &AccessibleProxy<'_>) -> Option<String> {
    let proxies = proxy.proxies().await.ok()?;
    if let Ok(text) = proxies.text().await {
        if let Ok(n) = text.character_count().await {
            if n > 0 {
                let end = n.min(512);
                if let Ok(s) = text.get_text(0, end).await {
                    let s = s.trim().to_string();
                    if !s.is_empty() {
                        return Some(s);
                    }
                }
            }
        }
    }
    if let Ok(value) = proxies.value().await {
        if let Ok(v) = value.current_value().await {
            return Some(format!("{v}"));
        }
    }
    None
}

pub(crate) fn is_toplevel_role(role: Role) -> bool {
    matches!(
        role,
        Role::Frame
            | Role::Window
            | Role::Dialog
            | Role::Alert
            | Role::FileChooser
            | Role::ColorChooser
            | Role::FontChooser
            | Role::InputMethodWindow
    )
}

pub(crate) fn is_active_or_focused(states: &[String]) -> bool {
    states
        .iter()
        .any(|s| s.eq_ignore_ascii_case("active") || s.eq_ignore_ascii_case("focused"))
}

pub(crate) async fn role_of(proxy: &AccessibleProxy<'_>) -> Role {
    proxy.get_role().await.unwrap_or(Role::Invalid)
}

pub(crate) fn nonempty(s: Option<String>) -> Option<String> {
    s.and_then(|s| {
        let t = s.trim().to_string();
        if t.is_empty() {
            None
        } else {
            Some(t)
        }
    })
}

pub(crate) async fn has_state(proxy: &AccessibleProxy<'_>, state: State) -> bool {
    proxy
        .get_state()
        .await
        .map(|s| s.contains(state))
        .unwrap_or(false)
}
