//! State / config directory resolution.

use std::path::PathBuf;

/// Default state root: `$APEX_HARNESS_STATE_DIR` or `~/.local/share/apex-harness`.
pub fn state_dir() -> PathBuf {
    if let Ok(p) = std::env::var("APEX_HARNESS_STATE_DIR") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    directories::ProjectDirs::from("com", "buckster123", "apex-harness")
        .map(|d| d.data_local_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".").join("apex-harness-state"))
}

/// Config root: `$APEX_HARNESS_CONFIG_DIR` or `~/.config/apex-harness`.
pub fn config_dir() -> PathBuf {
    if let Ok(p) = std::env::var("APEX_HARNESS_CONFIG_DIR") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    directories::ProjectDirs::from("com", "buckster123", "apex-harness")
        .map(|d| d.config_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from(".").join("apex-harness-config"))
}

/// Ensure state dir exists; return it.
pub fn ensure_state_dir() -> std::io::Result<PathBuf> {
    let d = state_dir();
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

/// Screenshots drop directory under state.
pub fn screenshots_dir() -> std::io::Result<PathBuf> {
    let d = ensure_state_dir()?.join("screenshots");
    std::fs::create_dir_all(&d)?;
    Ok(d)
}

/// Audit JSONL path.
pub fn audit_log_path() -> PathBuf {
    state_dir().join("audit.jsonl")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_dir_is_absolute_or_relative_nonempty() {
        let p = state_dir();
        assert!(!p.as_os_str().is_empty());
    }
}
