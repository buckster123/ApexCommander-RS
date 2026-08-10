//! AT-SPI accessibility backend — the eyes of the harness.
//!
//! Live discovery and tree walking sit here. Pure selectors / matchers live in
//! [`find`] so they stay unit-testable without a bus.

mod find;
mod id;
mod session;
mod tree;
mod walk;

pub use find::{find_in_tree, matches_query};
pub use id::{decode_id, encode_id};
pub use session::{probe_atspi, AtspiSession};
pub use tree::{
    activate, find_elements, focused_element, frontmost, list_apps, list_windows, snapshot,
};
pub use walk::SnapshotStats;
