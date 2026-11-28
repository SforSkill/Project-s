//! Key bindings: names stored in `settings.toml`, matching Bevy inputs, and the
//! rebinding state shared by the settings screen and the game input system.
//!
//! A binding is a physical key (`KeyCode`, independent of the keyboard layout:
//! "W" is the key right of Tab, Z on AZERTY) or a mouse button ("Mouse1" to
//! "Mouse5"). Escape is never bindable: it opens the menu and cancels a capture.
//! The name stays layout-independent in the file; the screen shows what the key
//! prints on the player's layout (`keyboard_layout.rs`).

use crate::keyboard_layout::KeyLayout;
use crate::settings::Keys;
use bevy::ecs::system::SystemParam;
use bevy::input::ButtonInput;
use bevy::prelude::{KeyCode, MouseButton, Res, ResMut, Resource};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bind {
    Key(KeyCode),
    Mouse(MouseButton),
}

/// Canonical names written to `settings.toml`, with their key.
#[rustfmt::skip]
const KEYS: &[(&str, KeyCode)] = {
    use KeyCode::*;
    &[
        ("A", KeyA), ("B", KeyB), ("C", KeyC), ("D", KeyD), ("E", KeyE), ("F", KeyF),
        ("G", KeyG), ("H", KeyH), ("I", KeyI), ("J", KeyJ), ("K", KeyK), ("L", KeyL),
        ("M", KeyM), ("N", KeyN), ("O", KeyO), ("P", KeyP), ("Q", KeyQ), ("R", KeyR),
        ("S", KeyS), ("T", KeyT), ("U", KeyU), ("V", KeyV), ("W", KeyW), ("X", KeyX),
        ("Y", KeyY), ("Z", KeyZ),
        ("0", Digit0), ("1", Digit1), ("2", Digit2), ("3", Digit3), ("4", Digit4),
        ("5", Digit5), ("6", Digit6), ("7", Digit7), ("8", Digit8), ("9", Digit9),
        ("F1", F1), ("F2", F2), ("F3", F3), ("F4", F4), ("F5", F5), ("F6", F6),
        ("F7", F7), ("F8", F8), ("F9", F9), ("F10", F10), ("F11", F11), ("F12", F12),
        ("Space", Space), ("Tab", Tab), ("CapsLock", CapsLock), ("Enter", Enter),
        ("Backspace", Backspace),
        ("ShiftLeft", ShiftLeft), ("ShiftRight", ShiftRight),
        ("ControlLeft", ControlLeft), ("ControlRight", ControlRight),
        ("AltLeft", AltLeft), ("AltRight", AltRight),
        ("ArrowUp", ArrowUp), ("ArrowDown", ArrowDown),
        ("ArrowLeft", ArrowLeft), ("ArrowRight", ArrowRight),
        ("Insert", Insert), ("Delete", Delete), ("Home", Home), ("End", End),
        ("PageUp", PageUp), ("PageDown", PageDown),
        ("Backquote", Backquote), ("Minus", Minus), ("Equal", Equal),
        ("BracketLeft", BracketLeft), ("BracketRight", BracketRight),
        ("Backslash", Backslash), ("IntlBackslash", IntlBackslash),
        ("Semicolon", Semicolon), ("Quote", Quote), ("Comma", Comma),
        ("Period", Period), ("Slash", Slash),
        ("Numpad0", Numpad0), ("Numpad1", Numpad1), ("Numpad2", Numpad2),
        ("Numpad3", Numpad3), ("Numpad4", Numpad4), ("Numpad5", Numpad5),
        ("Numpad6", Numpad6), ("Numpad7", Numpad7), ("Numpad8", Numpad8),
        ("Numpad9", Numpad9), ("NumpadAdd", NumpadAdd),
        ("NumpadSubtract", NumpadSubtract), ("NumpadMultiply", NumpadMultiply),
        ("NumpadDivide", NumpadDivide), ("NumpadDecimal", NumpadDecimal),
        ("NumpadEnter", NumpadEnter),
    ]
};

const MOUSE: &[(&str, MouseButton, &str)] = &[
    ("Mouse1", MouseButton::Left, "Clic gauche"),
    ("Mouse2", MouseButton::Right, "Clic droit"),
    ("Mouse3", MouseButton::Middle, "Clic molette"),
    ("Mouse4", MouseButton::Back, "Souris 4"),
    ("Mouse5", MouseButton::Forward, "Souris 5"),
];

/// Older or shorter spellings still accepted when reading `settings.toml`.
const ALIASES: &[(&str, &str)] = &[
    ("Ctrl", "ControlLeft"),
    ("Shift", "ShiftLeft"),
    ("Alt", "AltLeft"),
    ("MouseLeft", "Mouse1"),
    ("MouseRight", "Mouse2"),
    ("MouseMiddle", "Mouse3"),
];

impl Bind {
    /// Parses a name from `settings.toml` (case-insensitive). `None` leaves the
    /// action unbound.
    pub fn parse(name: &str) -> Option<Bind> {
        let name = name.trim();
        let name = ALIASES
            .iter()
            .find(|(a, _)| a.eq_ignore_ascii_case(name))
            .map_or(name, |(_, n)| n);
        if let Some((_, k)) = KEYS.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)) {
            return Some(Bind::Key(*k));
        }
        MOUSE
            .iter()
            .find(|(n, _, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, b, _)| Bind::Mouse(*b))
    }

    /// Canonical name, or `None` for a key or button that cannot be bound.
    pub fn name(self) -> Option<&'static str> {
        match self {
            Bind::Key(k) => KEYS.iter().find(|(_, c)| *c == k).map(|(n, _)| *n),
            Bind::Mouse(b) => MOUSE.iter().find(|(_, c, _)| *c == b).map(|(n, _, _)| *n),
        }
    }

    pub fn pressed(self, keys: &ButtonInput<KeyCode>, mouse: &ButtonInput<MouseButton>) -> bool {
        match self {
            Bind::Key(k) => keys.pressed(k),
            Bind::Mouse(b) => mouse.pressed(b),
        }
    }

    pub fn just_pressed(
        self,
        keys: &ButtonInput<KeyCode>,
        mouse: &ButtonInput<MouseButton>,
    ) -> bool {
        match self {
            Bind::Key(k) => keys.just_pressed(k),
            Bind::Mouse(b) => mouse.just_pressed(b),
        }
    }
}

/// Text shown to the player for a stored name: what the key prints on the
/// player's layout ("Z" for "W" on AZERTY), or "Espace", "Clic gauche", "—".
pub fn label(name: &str, layout: &KeyLayout) -> String {
    match Bind::parse(name) {
        None => "—".into(),
        Some(Bind::Mouse(b)) => MOUSE
            .iter()
            .find(|(_, c, _)| *c == b)
            .map_or_else(|| name.into(), |(_, _, l)| (*l).into()),
        Some(Bind::Key(k)) => {
            if let Some(s) = layout.name(k) {
                return s.into();
            }
            let canonical = Bind::Key(k).name().unwrap_or(name);
            if let Some(rest) = canonical.strip_prefix("Numpad") {
                let rest = match rest {
                    "Add" => "+",
                    "Subtract" => "-",
                    "Multiply" => "*",
                    "Divide" => "/",
                    "Decimal" => ".",
                    "Enter" => "Entrée",
                    r => r,
                };
                return format!("Pavé num. {rest}");
            }
            use KeyCode::*;
            let l = match k {
                Space => "Espace",
                Tab => "Tab",
                CapsLock => "Verr. Maj",
                Enter => "Entrée",
                Backspace => "Retour arrière",
                ShiftLeft => "Maj gauche",
                ShiftRight => "Maj droite",
                ControlLeft => "Ctrl gauche",
                ControlRight => "Ctrl droit",
                AltLeft => "Alt",
                AltRight => "Alt Gr",
                ArrowUp => "Flèche haut",
                ArrowDown => "Flèche bas",
                ArrowLeft => "Flèche gauche",
                ArrowRight => "Flèche droite",
                Delete => "Suppr",
                Insert => "Inser",
                Home => "Début",
                End => "Fin",
                PageUp => "Page préc.",
                PageDown => "Page suiv.",
                _ => canonical,
            };
            l.into()
        }
    }
}

pub fn pressed(name: &str, keys: &ButtonInput<KeyCode>, mouse: &ButtonInput<MouseButton>) -> bool {
    Bind::parse(name).is_some_and(|b| b.pressed(keys, mouse))
}

pub fn just_pressed(
    name: &str,
    keys: &ButtonInput<KeyCode>,
    mouse: &ButtonInput<MouseButton>,
) -> bool {
    Bind::parse(name).is_some_and(|b| b.just_pressed(keys, mouse))
}

/// Action waiting for a new key in the settings screen (index into
/// `Keys::entries_mut`). While it is set, Escape cancels the capture instead of
/// closing the menu, and F11 does not toggle fullscreen.
#[derive(Resource, Default)]
pub struct Rebind(pub Option<usize>);

/// Inputs and state needed by the key settings screen.
#[derive(SystemParam)]
pub struct Capture<'w> {
    pub keys: ResMut<'w, ButtonInput<KeyCode>>,
    pub mouse: Res<'w, ButtonInput<MouseButton>>,
    pub rebind: ResMut<'w, Rebind>,
    pub layout: Res<'w, KeyLayout>,
}

impl Capture<'_> {
    /// Applies this frame's capture to the action being rebound, if any. The
    /// pressed key is consumed so that it does not also trigger its old action.
    pub fn apply(&mut self, keys: &mut Keys) {
        let Some(i) = self.rebind.0 else { return };
        let Some(result) = captured(&self.keys, &self.mouse) else {
            return;
        };
        if let Ok(name) = result {
            if let Some((_, slot)) = keys.entries_mut().into_iter().nth(i) {
                *slot = name.to_string();
            }
            if let Some(Bind::Key(k)) = Bind::parse(name) {
                self.keys.clear_just_pressed(k);
            }
        } else {
            self.keys.clear_just_pressed(KeyCode::Escape);
        }
        self.rebind.0 = None;
    }
}

/// Labels of the other actions bound to the same input as each action.
pub fn conflicts(keys: &Keys) -> Vec<Vec<&'static str>> {
    let mut k = keys.clone();
    let binds: Vec<(&'static str, Option<Bind>)> = k
        .entries_mut()
        .into_iter()
        .map(|(l, n)| (l, Bind::parse(n)))
        .collect();
    binds
        .iter()
        .enumerate()
        .map(|(i, (_, b))| {
            binds
                .iter()
                .enumerate()
                .filter(|(j, (_, o))| *j != i && b.is_some() && o == b)
                .map(|(_, (l, _))| *l)
                .collect()
        })
        .collect()
}

/// First key or mouse button pressed this frame, for the capture. Escape
/// returns `Err(())` (cancel); unbindable keys are ignored.
pub fn captured(
    keys: &ButtonInput<KeyCode>,
    mouse: &ButtonInput<MouseButton>,
) -> Option<Result<&'static str, ()>> {
    if keys.just_pressed(KeyCode::Escape) {
        return Some(Err(()));
    }
    keys.get_just_pressed()
        .map(|k| Bind::Key(*k))
        .chain(mouse.get_just_pressed().map(|b| Bind::Mouse(*b)))
        .find_map(Bind::name)
        .map(Ok)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for (n, k) in KEYS {
            assert_eq!(Bind::parse(n), Some(Bind::Key(*k)), "{n}");
            assert_eq!(Bind::Key(*k).name(), Some(*n));
        }
        for (n, b, _) in MOUSE {
            assert_eq!(Bind::parse(n), Some(Bind::Mouse(*b)));
            assert_eq!(Bind::Mouse(*b).name(), Some(*n));
        }
    }

    #[test]
    fn aliases_and_case() {
        assert_eq!(Bind::parse("ctrl"), Some(Bind::Key(KeyCode::ControlLeft)));
        assert_eq!(Bind::parse("w"), Some(Bind::Key(KeyCode::KeyW)));
        assert_eq!(
            Bind::parse(" mouse2 "),
            Some(Bind::Mouse(MouseButton::Right))
        );
        assert_eq!(Bind::parse(""), None);
        assert_eq!(Bind::parse("Escape"), None);
        let l = KeyLayout::default();
        assert_eq!(label("", &l), "—");
        assert_eq!(label("Space", &l), "Espace");
        assert_eq!(label("Mouse1", &l), "Clic gauche");
        assert_eq!(label("Numpad7", &l), "Pavé num. 7");
        assert_eq!(label("NumpadAdd", &l), "Pavé num. +");
        assert_eq!(label("W", &l), "W");
    }

    #[test]
    fn defaults_have_no_conflict() {
        let mut k = Keys::default();
        assert!(conflicts(&k).iter().all(Vec::is_empty));
        for (l, n) in k.entries_mut() {
            assert!(Bind::parse(n).is_some(), "{l} : {n}");
        }
        k.reload = "ctrl".into();
        let c = conflicts(&k);
        assert!(c.iter().filter(|v| !v.is_empty()).count() == 2);
        assert!(c.iter().any(|v| v == &["S'accroupir"]));
    }

    #[test]
    fn capture() {
        let mouse = ButtonInput::<MouseButton>::default();
        let mut keys = ButtonInput::<KeyCode>::default();
        assert_eq!(captured(&keys, &mouse), None);
        keys.press(KeyCode::KeyK);
        assert_eq!(captured(&keys, &mouse), Some(Ok("K")));
        keys.press(KeyCode::Escape);
        assert_eq!(captured(&keys, &mouse), Some(Err(())));
    }
}
