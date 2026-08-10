//! AT-SPI accessibility backend — eyes and element-level hands.
//!
//! Live discovery and tree walking sit here. Pure selectors / matchers live in
//! [`find`] so they stay unit-testable without a bus. Element actions:
//! [`action`].

mod action;
mod find;
mod id;
mod session;
mod tree;
mod walk;

pub use action::{click_element, do_action, element_bounds, set_value, type_into};
pub use find::{find_in_tree, matches_query};
pub use id::{decode_id, encode_id};
pub use session::{probe_atspi, AtspiSession};
pub use tree::{
    activate, find_elements, focused_element, frontmost, list_apps, list_windows, snapshot,
};
pub use walk::{bounds_of, SnapshotStats};
