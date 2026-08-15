//! Element / window identity: `{bus_name}|{object_path}`.

/// Encode a bus unique name + object path into a stable-ish agent-facing id.
pub fn encode_id(bus: &str, path: &str) -> String {
    format!("{bus}|{path}")
}

/// Split an id back into `(bus, path)`. Returns `None` if the separator is missing.
pub fn decode_id(id: &str) -> Option<(&str, &str)> {
    id.split_once('|')
        .filter(|(b, p)| !b.is_empty() && p.starts_with('/'))
}

/// True when `id` is on accessibility bus `bus` (`{bus}|{path}`).
pub fn id_on_bus(id: &str, bus: &str) -> bool {
    decode_id(id).is_some_and(|(b, _)| b == bus)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let id = encode_id(":1.9", "/org/a11y/atspi/accessible/root");
        assert_eq!(id, ":1.9|/org/a11y/atspi/accessible/root");
        let (b, p) = decode_id(&id).unwrap();
        assert_eq!(b, ":1.9");
        assert_eq!(p, "/org/a11y/atspi/accessible/root");
    }

    #[test]
    fn rejects_bad() {
        assert!(decode_id("nope").is_none());
        assert!(decode_id("|/path").is_none());
        assert!(decode_id("bus|notapath").is_none());
    }

    #[test]
    fn bus_match() {
        let id = encode_id(":1.9", "/org/a11y/atspi/accessible/1");
        assert!(id_on_bus(&id, ":1.9"));
        assert!(!id_on_bus(&id, ":1.10"));
        assert!(!id_on_bus("not-an-id", ":1.9"));
    }
}
