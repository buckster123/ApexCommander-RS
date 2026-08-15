//! Sensitive-app denylist and mutation policy hooks.
//!
//! Matches **window title + app name**, never AT-SPI element ids. PolicyEngine
//! (ApexOS) remains the high-level approval layer; this is the local hard block
//! for password managers / keyrings etc.
//!
//! `allow_override` is honoured and **audited**. Callers must fail closed when
//! the target cannot be classified (`unclassified`).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::audit;
use crate::error::{HarnessError, Result};
use crate::paths::config_dir;
use crate::types::{MutationClass, WindowInfo};

/// Default sensitive name/title substrings (case-insensitive).
pub const DEFAULT_DENY_SUBSTRINGS: &[&str] = &[
    "1password",
    "bitwarden",
    "keepass",
    "keepassxc",
    "lastpass",
    "gnome-keyring",
    "seahorse",
    "kwallet",
    "password",
    "credentials",
    "secret service",
    "authy",
    "2fa",
];

/// Loaded denylist configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SensitiveConfig {
    /// Extra substrings to block (merged with defaults unless `replace_defaults`).
    #[serde(default)]
    pub deny_substrings: Vec<String>,
    /// When true, only `deny_substrings` apply (no built-in list).
    #[serde(default)]
    pub replace_defaults: bool,
    /// Allow tools to proceed despite a match (must be audited by [`enforce`]).
    #[serde(default)]
    pub allow_override: bool,
}

impl SensitiveConfig {
    /// Load from `$APEX_COMMANDER_CONFIG_DIR/sensitive.toml` if present, else defaults.
    pub fn load() -> Self {
        let path = config_dir().join("sensitive.toml");
        Self::load_from(&path).unwrap_or_default()
    }

    pub fn load_from(path: &std::path::Path) -> Option<Self> {
        let body = std::fs::read_to_string(path).ok()?;
        // Minimal TOML-ish without a toml dep: one pattern per line under [deny],
        // or JSON if the file starts with `{`.
        if body.trim_start().starts_with('{') {
            return serde_json::from_str(&body).ok();
        }
        let mut cfg = SensitiveConfig::default();
        for line in body.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with('[') {
                continue;
            }
            if let Some(rest) = line.strip_prefix("deny=") {
                cfg.deny_substrings
                    .push(rest.trim().trim_matches('"').to_string());
            } else if line == "replace_defaults=true" {
                cfg.replace_defaults = true;
            } else if line == "allow_override=true" {
                cfg.allow_override = true;
            } else if !line.contains('=') {
                cfg.deny_substrings.push(line.to_string());
            }
        }
        Some(cfg)
    }

    /// Patterns effective for matching.
    pub fn patterns(&self) -> Vec<String> {
        let mut out = Vec::new();
        if !self.replace_defaults {
            out.extend(DEFAULT_DENY_SUBSTRINGS.iter().map(|s| (*s).to_string()));
        }
        out.extend(self.deny_substrings.iter().cloned());
        out
    }
}

/// Match result for a window/app name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SensitiveHit {
    pub pattern: String,
    pub haystack: String,
}

/// Outcome of a denylist check (pure).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardDecision {
    Allow,
    AllowOverride { hit: SensitiveHit },
    Block { hit: SensitiveHit },
}

/// Check app name + window title against the denylist.
pub fn match_sensitive(
    app_name: Option<&str>,
    title: &str,
    cfg: &SensitiveConfig,
) -> Option<SensitiveHit> {
    let hay = format!("{} {}", app_name.unwrap_or(""), title).to_lowercase();
    for p in cfg.patterns() {
        if p.is_empty() {
            continue;
        }
        if hay.contains(&p.to_lowercase()) {
            return Some(SensitiveHit {
                pattern: p,
                haystack: hay,
            });
        }
    }
    None
}

/// Pure decision: match + override flag. No I/O.
pub fn decide(app_name: Option<&str>, title: &str, cfg: &SensitiveConfig) -> GuardDecision {
    match match_sensitive(app_name, title, cfg) {
        None => GuardDecision::Allow,
        Some(hit) if cfg.allow_override => GuardDecision::AllowOverride { hit },
        Some(hit) => GuardDecision::Block { hit },
    }
}

/// Apply a decision. Override is written to the audit log before returning Ok.
pub fn enforce(
    decision: GuardDecision,
    tool: &str,
    target: Option<serde_json::Value>,
) -> Result<()> {
    match decision {
        GuardDecision::Allow => Ok(()),
        GuardDecision::AllowOverride { hit } => {
            audit::ensure_writable().map_err(|e| {
                HarnessError::Other(format!("audit log not writable (override refused): {e}"))
            })?;
            audit::try_record(
                "policy_override",
                MutationClass::Mutating,
                Some(json!({
                    "tool": tool,
                    "pattern": hit.pattern,
                    "target": target,
                })),
                true,
                format!("allow_override matched pattern {:?}", hit.pattern),
            )
            .map_err(|e| {
                HarnessError::Other(format!("audit log write failed (override refused): {e}"))
            })?;
            Ok(())
        }
        GuardDecision::Block { hit } => Err(HarnessError::PolicyBlocked(format!(
            "sensitive app/window matched pattern {:?} (title/app: {:?}); \
             set allow_override in sensitive config or pick another target",
            hit.pattern, hit.haystack
        ))),
    }
}

/// Block when the target window looks sensitive.
pub fn enforce_window(window: &WindowInfo, tool: &str, cfg: &SensitiveConfig) -> Result<()> {
    let target = json!({
        "id": window.id,
        "title": window.title,
        "app": window.app_name,
    });
    enforce(
        decide(window.app_name.as_deref(), &window.title, cfg),
        tool,
        Some(target),
    )
}

/// Fail closed if `windows` is empty; otherwise enforce each (any hit blocks).
pub fn enforce_classified(windows: &[WindowInfo], tool: &str, cfg: &SensitiveConfig) -> Result<()> {
    if windows.is_empty() {
        return Err(unclassified(tool));
    }
    for w in windows {
        enforce_window(w, tool, cfg)?;
    }
    Ok(())
}

/// Guard a free-form name (launch spec, etc.).
pub fn enforce_name(name: &str, tool: &str, cfg: &SensitiveConfig) -> Result<()> {
    enforce(decide(None, name, cfg), tool, Some(json!({"name": name})))
}

/// Error when the harness cannot tell which app would be affected.
pub fn unclassified(tool: &str) -> HarnessError {
    HarnessError::Unavailable(format!(
        "cannot classify target for sensitive-app policy ({tool}) — refuse"
    ))
}

/// Path helper for docs / doctor.
pub fn sensitive_config_path() -> PathBuf {
    config_dir().join("sensitive.toml")
}

/// Compatibility wrappers (S2 names). Prefer [`enforce_window`] / [`enforce_name`].
pub fn guard_window(window: &WindowInfo, cfg: &SensitiveConfig) -> Result<()> {
    enforce_window(window, "guard_window", cfg)
}

pub fn guard_name(name: &str, cfg: &SensitiveConfig) -> Result<()> {
    enforce_name(name, "guard_name", cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bitwarden() -> WindowInfo {
        WindowInfo {
            id: ":1.9|/org/a11y/atspi/accessible/1".into(),
            title: "Bitwarden".into(),
            app_name: Some("bitwarden".into()),
            pid: None,
            focused: false,
            bounds: None,
            a11y_root: true,
            role: None,
        }
    }

    #[test]
    fn blocks_password_manager_title() {
        let cfg = SensitiveConfig::default();
        let hit = match_sensitive(Some("1Password"), "Vault", &cfg);
        assert!(hit.is_some());
        assert_eq!(hit.unwrap().pattern, "1password");
    }

    #[test]
    fn allows_normal_app() {
        let cfg = SensitiveConfig::default();
        assert!(match_sensitive(Some("geany"), "PRD.md", &cfg).is_none());
        assert!(matches!(
            decide(Some("geany"), "PRD.md", &cfg),
            GuardDecision::Allow
        ));
    }

    #[test]
    fn element_id_is_not_a_sensitive_name() {
        let cfg = SensitiveConfig::default();
        assert!(match_sensitive(None, ":1.11|/org/a11y/atspi/accessible/943", &cfg).is_none());
    }

    #[test]
    fn replace_defaults_only_custom() {
        let cfg = SensitiveConfig {
            deny_substrings: vec!["only-this".into()],
            replace_defaults: true,
            allow_override: false,
        };
        assert!(match_sensitive(Some("1Password"), "Vault", &cfg).is_none());
        assert!(match_sensitive(None, "only-this-window", &cfg).is_some());
    }

    #[test]
    fn decide_override_vs_block() {
        let mut cfg = SensitiveConfig::default();
        assert!(matches!(
            decide(Some("bitwarden"), "Vault", &cfg),
            GuardDecision::Block { .. }
        ));
        cfg.allow_override = true;
        assert!(matches!(
            decide(Some("bitwarden"), "Vault", &cfg),
            GuardDecision::AllowOverride { .. }
        ));
    }

    #[test]
    fn guard_window_errors() {
        let cfg = SensitiveConfig::default();
        assert!(enforce_window(&bitwarden(), "do_action", &cfg).is_err());
    }

    #[test]
    fn empty_classification_fails_closed() {
        let cfg = SensitiveConfig::default();
        let err = enforce_classified(&[], "do_action", &cfg).unwrap_err();
        assert!(matches!(err, HarnessError::Unavailable(_)));
    }

    #[test]
    fn override_is_audited() {
        let dir = tempfile::tempdir().unwrap();
        std::env::set_var("APEX_COMMANDER_STATE_DIR", dir.path());
        let cfg = SensitiveConfig {
            allow_override: true,
            ..Default::default()
        };
        enforce_window(&bitwarden(), "do_action", &cfg).unwrap();
        let body = std::fs::read_to_string(dir.path().join("audit.jsonl")).unwrap();
        assert!(body.contains("policy_override"));
        assert!(
            body.contains("bitwarden") || body.contains("1password") || body.contains("pattern")
        );
        std::env::remove_var("APEX_COMMANDER_STATE_DIR");
    }
}
