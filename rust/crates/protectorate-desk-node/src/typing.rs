//! Plan text input as keyboard events using the foreground layout when possible.

const KEY_UP: u32 = 0x0002;
const KEY_UNICODE: u32 = 0x0004;
const VK_SHIFT: u16 = 0x10;
const VK_RETURN: u16 = 0x0d;
const VK_TAB: u16 = 0x09;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TypedKey {
    pub vk: u16,
    pub scan: u16,
    pub flags: u32,
}

fn virtual_key(vk: u16, scan: u16, up: bool) -> TypedKey {
    TypedKey {
        vk,
        scan,
        flags: if up { KEY_UP } else { 0 },
    }
}

/// One group is sent in one SendInput call. Layout-produced characters use
/// virtual keys because some Windows editors mangle runs of Unicode packets.
pub(crate) fn plan_text(
    text: &str,
    mut key: impl FnMut(char) -> Option<(u16, bool)>,
    mut scan: impl FnMut(u16) -> u16,
) -> Vec<Vec<TypedKey>> {
    let mut groups = Vec::new();
    for ch in text.chars() {
        let mapped = match ch {
            '\r' => continue,
            '\n' => Some((VK_RETURN, false)),
            '\t' => Some((VK_TAB, false)),
            _ => key(ch),
        };
        if let Some((vk, shift)) = mapped {
            let mut group = Vec::with_capacity(if shift { 4 } else { 2 });
            if shift {
                group.push(virtual_key(VK_SHIFT, scan(VK_SHIFT), false));
            }
            group.push(virtual_key(vk, scan(vk), false));
            group.push(virtual_key(vk, scan(vk), true));
            if shift {
                group.push(virtual_key(VK_SHIFT, scan(VK_SHIFT), true));
            }
            groups.push(group);
            continue;
        }
        let mut units = [0u16; 2];
        for &unit in ch.encode_utf16(&mut units).iter() {
            groups.push(vec![
                TypedKey {
                    vk: 0,
                    scan: unit,
                    flags: KEY_UNICODE,
                },
                TypedKey {
                    vk: 0,
                    scan: unit,
                    flags: KEY_UNICODE | KEY_UP,
                },
            ]);
        }
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_uses_layout_keys_for_printable_chars_and_special_keys_for_controls() {
        let groups = plan_text(
            "aA\r\n\t",
            |ch| match ch {
                'a' => Some((b'A' as u16, false)),
                'A' => Some((b'A' as u16, true)),
                _ => None,
            },
            |vk| vk + 100,
        );
        let press = |vk| virtual_key(vk, vk + 100, false);
        let release = |vk| virtual_key(vk, vk + 100, true);
        assert_eq!(
            groups,
            vec![
                vec![press(65), release(65)],
                vec![press(VK_SHIFT), press(65), release(65), release(VK_SHIFT)],
                vec![press(VK_RETURN), release(VK_RETURN)],
                vec![press(VK_TAB), release(VK_TAB)],
            ]
        );
    }

    #[test]
    fn unmappable_bmp_and_supplementary_characters_use_unicode_units() {
        let groups = plan_text("é😀", |_| None, |_| panic!("no virtual keys expected"));
        assert_eq!(groups.len(), 3);
        let units: Vec<u16> = "é😀".encode_utf16().collect();
        for (group, unit) in groups.iter().zip(units) {
            assert_eq!(group.len(), 2);
            assert_eq!(group[0].vk, 0);
            assert_eq!(group[0].scan, unit);
            assert_eq!(group[0].flags, KEY_UNICODE);
            assert_eq!(group[1].scan, unit);
            assert_eq!(group[1].flags, KEY_UNICODE | KEY_UP);
        }
    }
}
