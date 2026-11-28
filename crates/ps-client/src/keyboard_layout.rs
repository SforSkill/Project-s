//! What each physical key prints on the player's keyboard layout, so that the
//! settings show "Z" (AZERTY) rather than the layout-independent name "W".
//!
//! - Windows: the active layout is asked directly (`MapVirtualKeyExW`), and asked
//!   again when the player switches layout (Win+Space).
//! - Elsewhere, and for keys Windows cannot name: the character a key produced
//!   the last time it was pressed without modifiers. Before the first press the
//!   settings show the physical name.
//!
//! Only character keys (letters, digits, punctuation) depend on the layout; the
//! others (Space, Ctrl, F1…) keep the labels of `bindings.rs`.

use bevy::ecs::system::NonSendMarker;
use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use std::collections::HashMap;

/// Character keys and their PC set 1 scan code.
#[rustfmt::skip]
const CHAR_KEYS: &[(KeyCode, u32)] = {
    use KeyCode::*;
    &[
        (KeyA, 0x1E), (KeyB, 0x30), (KeyC, 0x2E), (KeyD, 0x20), (KeyE, 0x12),
        (KeyF, 0x21), (KeyG, 0x22), (KeyH, 0x23), (KeyI, 0x17), (KeyJ, 0x24),
        (KeyK, 0x25), (KeyL, 0x26), (KeyM, 0x32), (KeyN, 0x31), (KeyO, 0x18),
        (KeyP, 0x19), (KeyQ, 0x10), (KeyR, 0x13), (KeyS, 0x1F), (KeyT, 0x14),
        (KeyU, 0x16), (KeyV, 0x2F), (KeyW, 0x11), (KeyX, 0x2D), (KeyY, 0x15),
        (KeyZ, 0x2C),
        (Digit1, 0x02), (Digit2, 0x03), (Digit3, 0x04), (Digit4, 0x05),
        (Digit5, 0x06), (Digit6, 0x07), (Digit7, 0x08), (Digit8, 0x09),
        (Digit9, 0x0A), (Digit0, 0x0B),
        (Backquote, 0x29), (Minus, 0x0C), (Equal, 0x0D), (BracketLeft, 0x1A),
        (BracketRight, 0x1B), (Backslash, 0x2B), (IntlBackslash, 0x56),
        (Semicolon, 0x27), (Quote, 0x28), (Comma, 0x33), (Period, 0x34),
        (Slash, 0x35),
    ]
};

#[derive(Resource, Default)]
pub struct KeyLayout {
    names: HashMap<KeyCode, String>,
    /// Layout the Windows names were read from (0: not read yet).
    #[cfg(windows)]
    hkl: usize,
}

impl KeyLayout {
    /// Text printed by a character key on the current layout, if known.
    pub fn name(&self, key: KeyCode) -> Option<&str> {
        self.names.get(&key).map(String::as_str)
    }

    /// Records the character a key produced; ignored for non-character keys,
    /// with a modifier held, or for text that is not a single visible character.
    fn learn(&mut self, key: KeyCode, text: &str) {
        if !CHAR_KEYS.iter().any(|(k, _)| *k == key) || self.names.contains_key(&key) {
            return;
        }
        let mut chars = text.chars();
        if let (Some(c), None) = (chars.next(), chars.next())
            && !c.is_control()
            && !c.is_whitespace()
        {
            self.names.insert(key, c.to_uppercase().collect());
        }
    }
}

/// Keeps the layout names current. Runs on the main thread (`NonSendMarker`):
/// Windows keyboard layouts are per thread, and the window belongs to it.
pub fn update(
    _main_thread: NonSendMarker,
    mut layout: ResMut<KeyLayout>,
    mut events: MessageReader<KeyboardInput>,
    keys: Res<ButtonInput<KeyCode>>,
) {
    #[cfg(windows)]
    {
        let hkl = os::current_layout();
        if hkl != layout.hkl {
            layout.hkl = hkl;
            layout.names.clear();
            for &(key, scan) in CHAR_KEYS {
                if let Some(s) = os::key_text(scan, hkl) {
                    layout.names.insert(key, s);
                }
            }
        }
    }
    let modified = keys.any_pressed([
        KeyCode::ShiftLeft,
        KeyCode::ShiftRight,
        KeyCode::ControlLeft,
        KeyCode::ControlRight,
        KeyCode::AltLeft,
        KeyCode::AltRight,
        KeyCode::SuperLeft,
        KeyCode::SuperRight,
    ]);
    for ev in events.read() {
        if ev.state == ButtonState::Pressed
            && !modified
            && let Key::Character(text) = &ev.logical_key
        {
            layout.learn(ev.key_code, text);
        }
    }
}

/// The only unsafe code of the client: two Win32 calls that take integers and
/// a layout handle, return integers, and touch no memory of ours.
#[cfg(windows)]
#[allow(unsafe_code)]
mod os {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyboardLayout, MAPVK_VK_TO_CHAR, MAPVK_VSC_TO_VK_EX, MapVirtualKeyExW,
    };

    /// Keyboard layout of the calling thread, as an opaque number.
    pub fn current_layout() -> usize {
        // SAFETY: no argument besides a thread id (0 = this thread), no memory access.
        unsafe { GetKeyboardLayout(0) as usize }
    }

    /// Text of the key with this scan code on the layout: the key's own letter
    /// or digit (Windows names the AZERTY top row 1…0, not &…à), otherwise the
    /// unshifted character (dead keys such as ^ included).
    pub fn key_text(scan: u32, hkl: usize) -> Option<String> {
        let hkl = hkl as windows_sys::Win32::UI::Input::KeyboardAndMouse::HKL;
        // SAFETY: integer arguments and a layout handle returned by
        // GetKeyboardLayout; MapVirtualKeyExW only reads layout tables.
        let vk = unsafe { MapVirtualKeyExW(scan, MAPVK_VSC_TO_VK_EX, hkl) };
        if let Some(c) = char::from_u32(vk).filter(|c| c.is_ascii_alphanumeric()) {
            return Some(c.to_string());
        }
        if vk == 0 {
            return None;
        }
        // SAFETY: as above.
        let ch = unsafe { MapVirtualKeyExW(vk, MAPVK_VK_TO_CHAR, hkl) } & 0x7FFF_FFFF;
        char::from_u32(ch)
            .filter(|c| *c != '\0' && !c.is_control() && !c.is_whitespace())
            .map(|c| c.to_uppercase().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn learns_single_characters_only() {
        let mut l = KeyLayout::default();
        l.learn(KeyCode::KeyW, "z");
        l.learn(KeyCode::Semicolon, "m");
        l.learn(KeyCode::KeyQ, "ab");
        l.learn(KeyCode::Space, "x");
        l.learn(KeyCode::Digit1, "\u{7}");
        assert_eq!(l.name(KeyCode::KeyW), Some("Z"));
        assert_eq!(l.name(KeyCode::Semicolon), Some("M"));
        assert_eq!(l.name(KeyCode::KeyQ), None);
        assert_eq!(l.name(KeyCode::Space), None);
        assert_eq!(l.name(KeyCode::Digit1), None);
        // The first character seen wins; the layout reload clears the map.
        l.learn(KeyCode::KeyW, "w");
        assert_eq!(l.name(KeyCode::KeyW), Some("Z"));
    }

    /// On Windows every letter and digit has a name on any installed layout.
    #[cfg(windows)]
    #[test]
    fn windows_names_letters_and_digits() {
        let hkl = os::current_layout();
        for &(key, scan) in &CHAR_KEYS[..36] {
            let s = os::key_text(scan, hkl).unwrap_or_default();
            assert!(!s.is_empty(), "{key:?}");
        }
    }
}
