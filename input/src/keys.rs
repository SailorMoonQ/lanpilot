//! USB HID keyboard page (0x07) usages, the key codes on the wire.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HidUsage(pub u32);

/// Every key LanPilot can send, by name. Backends must map all of these.
/// Names are lowercase; lookups are case-insensitive. Aliases share a usage.
#[rustfmt::skip]
pub const SUPPORTED_KEYS: &[(&str, u32)] = &[
    ("a", 0x04), ("b", 0x05), ("c", 0x06), ("d", 0x07), ("e", 0x08), ("f", 0x09),
    ("g", 0x0A), ("h", 0x0B), ("i", 0x0C), ("j", 0x0D), ("k", 0x0E), ("l", 0x0F),
    ("m", 0x10), ("n", 0x11), ("o", 0x12), ("p", 0x13), ("q", 0x14), ("r", 0x15),
    ("s", 0x16), ("t", 0x17), ("u", 0x18), ("v", 0x19), ("w", 0x1A), ("x", 0x1B),
    ("y", 0x1C), ("z", 0x1D),
    ("1", 0x1E), ("2", 0x1F), ("3", 0x20), ("4", 0x21), ("5", 0x22),
    ("6", 0x23), ("7", 0x24), ("8", 0x25), ("9", 0x26), ("0", 0x27),
    ("enter", 0x28), ("return", 0x28), ("esc", 0x29), ("escape", 0x29),
    ("backspace", 0x2A), ("tab", 0x2B), ("space", 0x2C),
    ("minus", 0x2D), ("equal", 0x2E), ("leftbracket", 0x2F), ("rightbracket", 0x30),
    ("backslash", 0x31), ("semicolon", 0x33), ("quote", 0x34), ("grave", 0x35),
    ("comma", 0x36), ("period", 0x37), ("slash", 0x38), ("capslock", 0x39),
    ("f1", 0x3A), ("f2", 0x3B), ("f3", 0x3C), ("f4", 0x3D), ("f5", 0x3E), ("f6", 0x3F),
    ("f7", 0x40), ("f8", 0x41), ("f9", 0x42), ("f10", 0x43), ("f11", 0x44), ("f12", 0x45),
    ("printscreen", 0x46), ("scrolllock", 0x47), ("pause", 0x48),
    ("insert", 0x49), ("home", 0x4A), ("pageup", 0x4B), ("delete", 0x4C),
    ("end", 0x4D), ("pagedown", 0x4E),
    ("right", 0x4F), ("left", 0x50), ("down", 0x51), ("up", 0x52),
    ("menu", 0x65),
    ("ctrl", 0xE0), ("shift", 0xE1), ("alt", 0xE2),
    ("win", 0xE3), ("super", 0xE3), ("cmd", 0xE3),
    ("rctrl", 0xE4), ("rshift", 0xE5), ("ralt", 0xE6), ("rwin", 0xE7),
];

pub fn usage_from_name(name: &str) -> Option<HidUsage> {
    let lower = name.to_ascii_lowercase();
    SUPPORTED_KEYS
        .iter()
        .find(|(n, _)| *n == lower)
        .map(|(_, u)| HidUsage(*u))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_resolve_case_insensitively() {
        assert_eq!(usage_from_name("a"), Some(HidUsage(0x04)));
        assert_eq!(usage_from_name("Z"), Some(HidUsage(0x1D)));
        assert_eq!(usage_from_name("Enter"), Some(HidUsage(0x28)));
        assert_eq!(usage_from_name("ctrl"), Some(HidUsage(0xE0)));
        assert_eq!(usage_from_name("f12"), Some(HidUsage(0x45)));
        assert_eq!(usage_from_name("0"), Some(HidUsage(0x27)));
        assert_eq!(usage_from_name("nope"), None);
    }

    #[test]
    fn aliases_map_to_the_same_usage() {
        assert_eq!(usage_from_name("win"), usage_from_name("super"));
        assert_eq!(usage_from_name("escape"), usage_from_name("esc"));
        assert_eq!(usage_from_name("return"), usage_from_name("enter"));
    }

    #[test]
    fn table_has_unique_names() {
        let mut names: Vec<&str> = SUPPORTED_KEYS.iter().map(|(n, _)| *n).collect();
        let before = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(before, names.len(), "duplicate key name in SUPPORTED_KEYS");
    }
}
