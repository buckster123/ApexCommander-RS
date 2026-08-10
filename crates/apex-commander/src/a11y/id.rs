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
}
