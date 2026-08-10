//! Shared agent-facing types (serialized on the MCP / CLI JSON surface).

use serde::{Deserialize, Serialize};

/// Whether a tool mutates the desktop. Faces and PolicyEngine use this annotation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationClass {
    /// Read-only perception / diagnostics.
    ReadOnly,
    /// Changes focus, pointer, or application state but is reversible / low risk.
    Mutating,
    /// High-risk: send/post/pay/delete/credentials/security settings.
    Destructive,
}

/// Session display server kind, as reported by `doctor`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionKind {
    X11,
    Wayland,
    Unknown,
}

/// Capability probe result for one backend surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capability {
    pub name: String,
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// Running application root (AT-SPI application role).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppInfo {
    /// Stable-ish id: `{bus_name}|{object_path}` for the application root.
    pub id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub toolkit: Option<String>,
    pub window_count: u32,
}

/// Compact window descriptor used by discovery tools.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowInfo {
    /// `{bus_name}|{object_path}` of the top-level frame/window.
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    /// True if the window reports `active` and/or `focused` state.
    pub focused: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounds: Option<Bounds>,
    /// Always true for windows discovered via AT-SPI.
    pub a11y_root: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

/// Screen-space rectangle in physical pixels (origin top-left of the display).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Bounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// One node in a compact accessibility snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct A11yNode {
    /// `{bus}|{path}` when known (live snapshots); pure fixtures may omit.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub states: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounds: Option<Bounds>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<A11yNode>,
}

/// Options for a compact a11y tree snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotOpts {
    /// Max tree depth from the root (0 = root only). Default 6.
    #[serde(default = "default_max_depth")]
    pub max_depth: u32,
    /// Hard cap on nodes visited/emitted. Default 200.
    #[serde(default = "default_max_nodes")]
    pub max_nodes: u32,
    /// Include Component extents when available. Default true.
    #[serde(default = "default_true")]
    pub include_bounds: bool,
    /// Include Action interface names when available. Default true.
    #[serde(default = "default_true")]
    pub include_actions: bool,
}

impl Default for SnapshotOpts {
    fn default() -> Self {
        Self {
            max_depth: default_max_depth(),
            max_nodes: default_max_nodes(),
            include_bounds: true,
            include_actions: true,
        }
    }
}

fn default_max_depth() -> u32 {
    6
}
fn default_max_nodes() -> u32 {
    200
}
fn default_true() -> bool {
    true
}

/// Semantic selector for `find_elements` (pure or live).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindQuery {
    /// Case-insensitive role name (e.g. `button`, `frame`).
    #[serde(default)]
    pub role: Option<String>,
    /// Substring match on accessible name (case-insensitive) unless `name_exact`.
    #[serde(default)]
    pub name: Option<String>,
    /// When true with `name`, require exact case-insensitive equality.
    #[serde(default)]
    pub name_exact: bool,
    /// Substring match against name, value, or description.
    #[serde(default)]
    pub text: Option<String>,
    /// Required state (e.g. `focused`, `enabled`) — case-insensitive.
    #[serde(default)]
    pub state: Option<String>,
    /// Substring match on description.
    #[serde(default)]
    pub description: Option<String>,
    /// Cap results. Default 25.
    #[serde(default = "default_max_results")]
    pub max_results: u32,
}

fn default_max_results() -> u32 {
    25
}

/// One hit from `find_elements`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElementHit {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub states: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounds: Option<Bounds>,
    /// Human-readable breadcrumb (`role:"name"` segments from root).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path: Vec<String>,
}

/// Result of `activate` (focus/raise).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivateResult {
    pub ok: bool,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub detail: String,
}

/// How to resolve a window/app target for snapshot/activate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetRef {
    /// Exact `{bus}|{path}` id.
    Id(String),
    /// Case-insensitive substring of window title or app name.
    Name(String),
    /// Process id.
    Pid(u32),
    /// Currently frontmost / focused window (best effort).
    Frontmost,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mutation_class_serializes_snake_case() {
        let v = serde_json::to_value(MutationClass::ReadOnly).unwrap();
        assert_eq!(v, "read_only");
        let v = serde_json::to_value(MutationClass::Destructive).unwrap();
        assert_eq!(v, "destructive");
    }

    #[test]
    fn a11y_node_omits_empty_optional_fields() {
        let n = A11yNode {
            id: None,
            role: "button".into(),
            name: Some("OK".into()),
            description: None,
            value: None,
            states: vec![],
            actions: vec!["click".into()],
            bounds: None,
            children: vec![],
        };
        let s = serde_json::to_string(&n).unwrap();
        assert!(s.contains("\"role\":\"button\""));
        assert!(s.contains("\"name\":\"OK\""));
        assert!(!s.contains("description"));
        assert!(!s.contains("children"));
        assert!(!s.contains("\"id\""));
        assert!(s.contains("actions"));
    }

    #[test]
    fn snapshot_opts_defaults() {
        let o = SnapshotOpts::default();
        assert_eq!(o.max_depth, 6);
        assert_eq!(o.max_nodes, 200);
        assert!(o.include_bounds);
    }
}
