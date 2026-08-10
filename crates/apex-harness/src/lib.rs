//! Apex Desktop Harness — core library.
//!
//! Agent hands for Linux: AT-SPI-first perception and action, with real input
//! injection and scoped screenshots as fallback. All logic lives here; the
//! `-mcp` / `-cli` crates are thin faces over it.
//!
//! See `docs/design.md` for the contract and `docs/CHARTER.md` for binding
//! decisions. Product intent: `docs/PRD.md`.

#![deny(unsafe_code)]

pub mod doctor;
pub mod error;
pub mod types;

/// Crate / product name used in logs, MCP `serverInfo`, and CLI `--version`.
pub const NAME: &str = "apex-harness";

/// Semver string from Cargo package metadata.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
