use crate::{Error, Result};

/// Windows virtual-key code and whether the extended-key flag is needed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Key {
    pub vk: u16,
    pub extended: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chord {
    pub modifiers: Vec<Key>,
    pub key: Key,
}
const fn key(vk: u16) -> Key {
    Key {
        vk,
        extended: false,
    }
}
const fn ext(vk: u16) -> Key {
    Key { vk, extended: true }
}

fn modifier(s: &str) -> Option<Key> {
    match s {
        "ctrl" | "control" => Some(key(0x11)),
        "alt" => Some(key(0x12)),
        "shift" => Some(key(0x10)),
        "win" | "super" | "meta" => Some(ext(0x5b)),
        _ => None,
    }
}
fn named(s: &str) -> Option<Key> {
    Some(match s {
        "enter" | "return" => key(0x0d),
        "tab" => key(0x09),
        "esc" | "escape" => key(0x1b),
        "space" => key(0x20),
        "backspace" => key(0x08),
        "delete" | "del" => ext(0x2e),
        "insert" | "ins" => ext(0x2d),
        "home" => ext(0x24),
        "end" => ext(0x23),
        "pageup" | "pgup" => ext(0x21),
        "pagedown" | "pgdn" => ext(0x22),
        "left" => ext(0x25),
        "up" => ext(0x26),
        "right" => ext(0x27),
        "down" => ext(0x28),
        "printscreen" => ext(0x2c),
        "capslock" => key(0x14),
        "-" => key(0xbd),
        "=" | "plus" => key(0xbb),
        "," => key(0xbc),
        "." => key(0xbe),
        "/" => key(0xbf),
        ";" => key(0xba),
        "'" => key(0xde),
        "[" => key(0xdb),
        "]" => key(0xdd),
        "\\" => key(0xdc),
        "`" => key(0xc0),
        _ => return None,
    })
}
fn parse_key(s: &str) -> Option<Key> {
    if let Some(key) = named(s) {
        return Some(key);
    }
    if let Some(n) = s.strip_prefix('f').and_then(|n| n.parse::<u16>().ok()) {
        if (1..=24).contains(&n) {
            return Some(key(0x70 + n - 1));
        }
    }
    match s.as_bytes() {
        [b'a'..=b'z'] => Some(key(s.as_bytes()[0].to_ascii_uppercase() as u16)),
        [b'0'..=b'9'] => Some(key(s.as_bytes()[0] as u16)),
        _ => None,
    }
}

/// Parses modifiers followed by one key, such as `ctrl+shift+esc`.
pub fn parse_chord(s: &str) -> Result<Chord> {
    let lowered = s.trim().to_ascii_lowercase();
    let parts = lowered.split('+').map(str::trim).collect::<Vec<_>>();
    if parts.iter().any(|part| part.is_empty()) {
        return Err(Error::InvalidChord(s.into()));
    }
    let Some((last, modifiers)) = parts.split_last() else {
        return Err(Error::InvalidChord(s.into()));
    };
    let mut out = Vec::with_capacity(modifiers.len());
    for part in modifiers {
        out.push(modifier(part).ok_or_else(|| Error::InvalidChord(s.into()))?);
    }
    let key = parse_key(last).ok_or_else(|| Error::InvalidChord(s.into()))?;
    Ok(Chord {
        modifiers: out,
        key,
    })
}
