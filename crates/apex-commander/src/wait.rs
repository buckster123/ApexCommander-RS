//! Wait / poll helpers for multi-step agent workflows.
//!
//! All waits respect a hard timeout (default 30s min floor for portal-like
//! operations is *not* applied here — agent-supplied timeouts are used, but
//! `timeout_ms` is clamped to a maximum of 120s to prevent runaway polls).

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::a11y::{find_elements, snapshot, AtspiSession};
use crate::error::{HarnessError, Result};
use crate::types::{ElementHit, FindQuery, SnapshotOpts, TargetRef};

const MAX_TIMEOUT_MS: u64 = 120_000;
const DEFAULT_TIMEOUT_MS: u64 = 10_000;
const DEFAULT_POLL_MS: u64 = 200;

/// Result of a simple timed wait.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaitResult {
    pub ok: bool,
    pub waited_ms: u64,
    pub detail: String,
}

/// Result of waiting for an element.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WaitForElementResult {
    pub ok: bool,
    pub waited_ms: u64,
    pub polls: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hit: Option<ElementHit>,
    pub detail: String,
}

/// Result of waiting for tree stability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaitForStableResult {
    pub ok: bool,
    pub waited_ms: u64,
    pub polls: u32,
    /// Fingerprint of the last observed tree (role+name path count).
    pub fingerprint: String,
    pub detail: String,
}

fn clamp_timeout(ms: Option<u64>) -> u64 {
    ms.unwrap_or(DEFAULT_TIMEOUT_MS).clamp(1, MAX_TIMEOUT_MS)
}

fn clamp_poll(ms: Option<u64>) -> u64 {
    ms.unwrap_or(DEFAULT_POLL_MS).clamp(50, 5_000)
}

/// Sleep for `timeout_ms` (clamped). Read-only; no AT-SPI.
pub async fn wait_ms(timeout_ms: Option<u64>) -> WaitResult {
    let ms = clamp_timeout(timeout_ms);
    tokio::time::sleep(Duration::from_millis(ms)).await;
    WaitResult {
        ok: true,
        waited_ms: ms,
        detail: format!("slept {ms}ms"),
    }
}

/// Poll until `find_elements` returns at least one hit or timeout.
pub async fn wait_for_element(
    session: &AtspiSession,
    target: &TargetRef,
    query: FindQuery,
    timeout_ms: Option<u64>,
    poll_ms: Option<u64>,
) -> Result<WaitForElementResult> {
    let timeout = Duration::from_millis(clamp_timeout(timeout_ms));
    let poll = Duration::from_millis(clamp_poll(poll_ms));
    let start = Instant::now();
    let mut polls = 0u32;

    let mut query = query;
    if query.max_results == 0 {
        query.max_results = 5;
    }

    loop {
        polls += 1;
        let opts = SnapshotOpts {
            max_depth: 12,
            max_nodes: 400,
            include_bounds: true,
            include_actions: true,
        };
        match find_elements(session, target, query.clone(), opts).await {
            Ok(hits) if !hits.is_empty() => {
                let waited_ms = start.elapsed().as_millis() as u64;
                return Ok(WaitForElementResult {
                    ok: true,
                    waited_ms,
                    polls,
                    hit: hits.into_iter().next(),
                    detail: format!("element found after {polls} poll(s)"),
                });
            }
            Ok(_) => {
                debug!(polls, "wait_for_element: no match yet");
            }
            Err(e) => {
                if matches!(e, crate::error::HarnessError::PolicyBlocked(_)) {
                    return Err(e);
                }
                // Target window may not exist yet — keep polling until timeout.
                debug!(error = %e, polls, "wait_for_element: transient error");
            }
        }

        if start.elapsed() >= timeout {
            let waited_ms = start.elapsed().as_millis() as u64;
            return Ok(WaitForElementResult {
                ok: false,
                waited_ms,
                polls,
                hit: None,
                detail: format!(
                    "timeout after {waited_ms}ms ({polls} polls) — element not found; re-snapshot"
                ),
            });
        }
        tokio::time::sleep(poll).await;
    }
}

/// Poll until consecutive fingerprints of the target tree match for `stable_for_ms`.
pub async fn wait_for_stable(
    session: &AtspiSession,
    target: &TargetRef,
    timeout_ms: Option<u64>,
    poll_ms: Option<u64>,
    stable_for_ms: Option<u64>,
) -> Result<WaitForStableResult> {
    let timeout = Duration::from_millis(clamp_timeout(timeout_ms));
    let poll = Duration::from_millis(clamp_poll(poll_ms));
    let need_stable = Duration::from_millis(stable_for_ms.unwrap_or(400).clamp(100, 10_000));
    let start = Instant::now();
    let mut polls = 0u32;
    let mut last_fp: Option<String> = None;
    let mut stable_since: Option<Instant> = None;

    loop {
        polls += 1;
        let opts = SnapshotOpts {
            max_depth: 8,
            max_nodes: 250,
            include_bounds: false,
            include_actions: false,
        };
        let fp = match snapshot(session, target, opts).await {
            Ok((tree, stats)) => fingerprint_tree(&tree, stats.nodes_emitted),
            Err(e) => {
                if matches!(e, crate::error::HarnessError::PolicyBlocked(_)) {
                    return Err(e);
                }
                debug!(error = %e, "wait_for_stable: snapshot error");
                last_fp = None;
                stable_since = None;
                if start.elapsed() >= timeout {
                    return Ok(WaitForStableResult {
                        ok: false,
                        waited_ms: start.elapsed().as_millis() as u64,
                        polls,
                        fingerprint: String::new(),
                        detail: format!("timeout; last error: {e}"),
                    });
                }
                tokio::time::sleep(poll).await;
                continue;
            }
        };

        match &last_fp {
            Some(prev) if prev == &fp => {
                let since = stable_since.get_or_insert_with(Instant::now);
                if since.elapsed() >= need_stable {
                    return Ok(WaitForStableResult {
                        ok: true,
                        waited_ms: start.elapsed().as_millis() as u64,
                        polls,
                        fingerprint: fp,
                        detail: format!(
                            "stable for ≥{}ms after {polls} poll(s)",
                            need_stable.as_millis()
                        ),
                    });
                }
            }
            _ => {
                last_fp = Some(fp);
                stable_since = Some(Instant::now());
            }
        }

        if start.elapsed() >= timeout {
            return Ok(WaitForStableResult {
                ok: false,
                waited_ms: start.elapsed().as_millis() as u64,
                polls,
                fingerprint: last_fp.unwrap_or_default(),
                detail: format!(
                    "timeout after {}ms — tree did not stay stable for {}ms",
                    start.elapsed().as_millis(),
                    need_stable.as_millis()
                ),
            });
        }
        tokio::time::sleep(poll).await;
    }
}

/// Compact fingerprint for stability comparison (pure — unit-tested).
pub fn fingerprint_tree(tree: &crate::types::A11yNode, nodes: u32) -> String {
    let mut parts = Vec::new();
    walk_fp(tree, &mut parts, 0, 64);
    format!("n={nodes}|{}", parts.join(";"))
}

fn walk_fp(node: &crate::types::A11yNode, out: &mut Vec<String>, depth: u32, max: usize) {
    if out.len() >= max {
        return;
    }
    let name = node.name.as_deref().unwrap_or("");
    out.push(format!("{}:{}:{name}", depth, node.role));
    for c in &node.children {
        walk_fp(c, out, depth + 1, max);
    }
}

/// Convert a failed wait into a hard error when the agent needs fail-fast.
pub fn require_element(result: WaitForElementResult) -> Result<ElementHit> {
    if result.ok {
        result
            .hit
            .ok_or_else(|| HarnessError::Other("ok but no hit".into()))
    } else {
        Err(HarnessError::NotFound(result.detail))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::A11yNode;

    fn sample() -> A11yNode {
        A11yNode {
            id: None,
            role: "frame".into(),
            name: Some("A".into()),
            description: None,
            value: None,
            states: vec![],
            actions: vec![],
            bounds: None,
            children: vec![A11yNode {
                id: None,
                role: "button".into(),
                name: Some("OK".into()),
                description: None,
                value: None,
                states: vec![],
                actions: vec![],
                bounds: None,
                children: vec![],
            }],
        }
    }

    #[test]
    fn fingerprint_stable_for_same_tree() {
        let t = sample();
        let a = fingerprint_tree(&t, 2);
        let b = fingerprint_tree(&t, 2);
        assert_eq!(a, b);
        assert!(a.contains("button"));
    }

    #[test]
    fn fingerprint_changes_with_name() {
        let mut t = sample();
        let a = fingerprint_tree(&t, 2);
        t.children[0].name = Some("Cancel".into());
        let b = fingerprint_tree(&t, 2);
        assert_ne!(a, b);
    }

    #[test]
    fn clamp_timeout_bounds() {
        assert_eq!(clamp_timeout(Some(0)), 1); // clamp min via clamp(1,…) — wait 0 becomes 1
        assert_eq!(clamp_timeout(Some(999_999)), MAX_TIMEOUT_MS);
        assert_eq!(clamp_timeout(None), DEFAULT_TIMEOUT_MS);
    }

    #[tokio::test]
    async fn wait_ms_sleeps() {
        let r = wait_ms(Some(50)).await;
        assert!(r.ok);
        assert!(r.waited_ms >= 50);
    }
}
