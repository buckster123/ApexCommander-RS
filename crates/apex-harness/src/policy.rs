//! Sensitive-app denylist and mutation policy hooks.
//!
//! Stub for S2: static default patterns + optional config file overlay.
//! PolicyEngine (ApexOS) remains the high-level approval layer; this is the
//! local hard block for password managers / keyrings etc.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{HarnessError, Result};
use crate::paths::config_dir;
use crate::types::WindowInfo;

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
    /// Allow mutating tools to proceed despite a match (audited by caller).
    #[serde(default)]
    pub allow_override: bool,
}

impl SensitiveConfig {
    /// Load from `$APEX_HARNESS_CONFIG_DIR/sensitive.toml` if present, else defaults.
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

/// Block mutating tools when the target window looks sensitive.
pub fn guard_window(window: &WindowInfo, cfg: &SensitiveConfig) -> Result<()> {
    if let Some(hit) = match_sensitive(window.app_name.as_deref(), &window.title, cfg) {
        if cfg.allow_override {
            return Ok(());
        }
        return Err(HarnessError::PolicyBlocked(format!(
            "sensitive app/window matched pattern {:?} (title/app: {:?}); \
             set allow_override in sensitive config or pick another target",
            hit.pattern, hit.haystack
        )));
    }
    Ok(())
}

/// Guard by free-form name string (when only a name/id is known).
pub fn guard_name(name: &str, cfg: &SensitiveConfig) -> Result<()> {
    if let Some(hit) = match_sensitive(None, name, cfg) {
        if cfg.allow_override {
            return Ok(());
        }
        return Err(HarnessError::PolicyBlocked(format!(
            "sensitive name matched pattern {:?}",
            hit.pattern
        )));
    }
    Ok(())
}

/// Path helper for docs.
pub fn sensitive_config_path() -> PathBuf {
    config_dir().join("sensitive.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn guard_window_errors() {
        let cfg = SensitiveConfig::default();
        let w = WindowInfo {
            id: "x".into(),
            title: "Bitwarden".into(),
            app_name: Some("bitwarden".into()),
            pid: None,
            focused: false,
            bounds: None,
            a11y_root: true,
            role: None,
        };
        assert!(guard_window(&w, &cfg).is_err());
    }
}
