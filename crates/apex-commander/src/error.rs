//! Error types for the harness core.

use thiserror::Error;

/// Public error surface for faces (CLI / MCP) to map into exit codes or MCP `isError`.
#[derive(Debug, Error)]
pub enum HarnessError {
    /// Backend or environment is missing a required capability.
    #[error("capability unavailable: {0}")]
    Unavailable(String),

    /// Caller asked for something that needs human/policy approval first.
    #[error("policy blocked: {0}")]
    PolicyBlocked(String),

    /// Target not found (window, element, app).
    #[error("not found: {0}")]
    NotFound(String),

    /// Ambiguous match — agent should re-snapshot and narrow the selector.
    #[error("ambiguous: {0}")]
    Ambiguous(String),

    /// I/O or OS-level failure with the real reason preserved.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// Catch-all with a real reason string (never empty success).
    #[error("{0}")]
    Other(String),
}

impl HarnessError {
    /// Suggested process exit code for the CLI face.
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::NotFound(_) => 3,
            Self::Ambiguous(_) => 4,
            Self::Unavailable(_) => 2,
            Self::PolicyBlocked(_) => 5,
            _ => 1,
        }
    }
}

/// Convenience alias.
pub type Result<T> = std::result::Result<T, HarnessError>;
