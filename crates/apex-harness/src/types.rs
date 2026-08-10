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

/// Compact window descriptor used by discovery tools.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindowInfo {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub app_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    pub focused: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounds: Option<Bounds>,
    pub a11y_root: bool,
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
        assert!(s.contains("actions"));
    }
}
