//! Pure find / match over compact [`A11yNode`] trees.
//!
//! No I/O. Unit tests are the load-bearing surface for selector correctness.

use crate::types::{A11yNode, ElementHit, FindQuery};

/// Does this node match the query? (children ignored)
pub fn matches_query(node: &A11yNode, q: &FindQuery) -> bool {
    if let Some(role) = q.role.as_deref() {
        if !eq_ci(&node.role, role) {
            return false;
        }
    }
    if let Some(name) = q.name.as_deref() {
        let n = node.name.as_deref().unwrap_or("");
        if q.name_exact {
            if !eq_ci(n, name) {
                return false;
            }
        } else if !contains_ci(n, name) {
            return false;
        }
    }
    if let Some(text) = q.text.as_deref() {
        let hay = [
            node.name.as_deref().unwrap_or(""),
            node.value.as_deref().unwrap_or(""),
            node.description.as_deref().unwrap_or(""),
        ]
        .join("\n");
        if !contains_ci(&hay, text) {
            return false;
        }
    }
    if let Some(state) = q.state.as_deref() {
        if !node.states.iter().any(|s| eq_ci(s, state)) {
            return false;
        }
    }
    if let Some(desc) = q.description.as_deref() {
        let d = node.description.as_deref().unwrap_or("");
        if !contains_ci(d, desc) {
            return false;
        }
    }
    // Empty query matches everything (useful for "list first N nodes").
    true
}

/// Depth-first find over a compact tree. Pure; no AT-SPI.
pub fn find_in_tree(root: &A11yNode, q: &FindQuery) -> Vec<ElementHit> {
    let max = if q.max_results == 0 {
        25
    } else {
        q.max_results as usize
    };
    let mut out = Vec::new();
    let mut path = Vec::new();
    walk(root, q, &mut path, &mut out, max);
    out
}

fn walk(
    node: &A11yNode,
    q: &FindQuery,
    path: &mut Vec<String>,
    out: &mut Vec<ElementHit>,
    max: usize,
) {
    if out.len() >= max {
        return;
    }
    path.push(crumb(node));
    if matches_query(node, q) {
        out.push(ElementHit {
            id: node.id.clone(),
            role: node.role.clone(),
            name: node.name.clone(),
            description: node.description.clone(),
            value: node.value.clone(),
            states: node.states.clone(),
            actions: node.actions.clone(),
            bounds: node.bounds,
            path: path.clone(),
        });
    }
    for child in &node.children {
        if out.len() >= max {
            break;
        }
        walk(child, q, path, out, max);
    }
    path.pop();
}

fn crumb(node: &A11yNode) -> String {
    match node.name.as_deref().filter(|s| !s.is_empty()) {
        Some(n) => format!("{}:{n:?}", node.role),
        None => node.role.clone(),
    }
}

fn eq_ci(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

fn contains_ci(hay: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    hay.to_lowercase().contains(&needle.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::A11yNode;

    fn sample_tree() -> A11yNode {
        A11yNode {
            id: Some(":1.1|/root".into()),
            role: "frame".into(),
            name: Some("Demo".into()),
            description: None,
            value: None,
            states: vec!["showing".into()],
            actions: vec![],
            bounds: None,
            children: vec![
                A11yNode {
                    id: Some(":1.1|/btn".into()),
                    role: "button".into(),
                    name: Some("OK".into()),
                    description: Some("Confirm dialog".into()),
                    value: None,
                    states: vec!["enabled".into(), "focusable".into()],
                    actions: vec!["click".into()],
                    bounds: None,
                    children: vec![],
                },
                A11yNode {
                    id: Some(":1.1|/entry".into()),
                    role: "text".into(),
                    name: Some("Name".into()),
                    description: None,
                    value: Some("Ada".into()),
                    states: vec!["editable".into(), "focused".into()],
                    actions: vec![],
                    bounds: None,
                    children: vec![],
                },
            ],
        }
    }

    #[test]
    fn find_by_role_and_name() {
        let tree = sample_tree();
        let hits = find_in_tree(
            &tree,
            &FindQuery {
                role: Some("button".into()),
                name: Some("ok".into()),
                ..Default::default()
            },
        );
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name.as_deref(), Some("OK"));
        assert_eq!(hits[0].id.as_deref(), Some(":1.1|/btn"));
        assert!(hits[0].path.iter().any(|p| p.contains("button")));
    }

    #[test]
    fn find_by_state_focused() {
        let tree = sample_tree();
        let hits = find_in_tree(
            &tree,
            &FindQuery {
                state: Some("focused".into()),
                ..Default::default()
            },
        );
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].role, "text");
    }

    #[test]
    fn find_by_text_matches_value() {
        let tree = sample_tree();
        let hits = find_in_tree(
            &tree,
            &FindQuery {
                text: Some("Ada".into()),
                ..Default::default()
            },
        );
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].value.as_deref(), Some("Ada"));
    }

    #[test]
    fn name_exact_rejects_substring() {
        let tree = sample_tree();
        let hits = find_in_tree(
            &tree,
            &FindQuery {
                name: Some("O".into()),
                name_exact: true,
                ..Default::default()
            },
        );
        assert!(hits.is_empty());
    }

    #[test]
    fn max_results_caps() {
        let tree = sample_tree();
        let hits = find_in_tree(
            &tree,
            &FindQuery {
                max_results: 1,
                ..Default::default()
            },
        );
        assert_eq!(hits.len(), 1);
    }
}
