//! Player settings, saved as TOML in the user's configuration directory.

use bevy::prelude::Resource;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Crosshair {
    pub size: f32,
    pub gap: f32,
    pub thickness: f32,
    pub dot: bool,
    pub outline: bool,
    pub color: [u8; 3],
}

impl Default for Crosshair {
    fn default() -> Self {
        Self {
            size: 5.0,
            gap: 3.0,
            thickness: 1.5,
            dot: false,
            outline: true,
            color: [80, 255, 120],
        }
    }
}

/// Key bindings, one name per action (see `bindings.rs` for the accepted names;
/// an empty string leaves the action unbound). Physical keys: "W" is Z on AZERTY.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Keys {
    pub forward: String,
    pub back: String,
    pub left: String,
    pub right: String,
    pub jump: String,
    pub duck: String,
    pub walk: String,
    pub attack: String,
    pub attack2: String,
    pub reload: String,
    pub inspect: String,
    pub use_key: String,
    pub drop: String,
    pub slot1: String,
    pub slot2: String,
    pub slot3: String,
    pub slot4: String,
    pub slot5: String,
    pub last_weapon: String,
    pub buy: String,
    pub scoreboard: String,
    pub chat: String,
    pub chat_team: String,
    pub inspector: String,
}

impl Default for Keys {
    fn default() -> Self {
        Self {
            forward: "W".into(),
            back: "S".into(),
            left: "A".into(),
            right: "D".into(),
            jump: "Space".into(),
            duck: "ControlLeft".into(),
            walk: "ShiftLeft".into(),
            attack: "Mouse1".into(),
            attack2: "Mouse2".into(),
            reload: "R".into(),
            inspect: "F".into(),
            use_key: "E".into(),
            drop: "G".into(),
            slot1: "1".into(),
            slot2: "2".into(),
            slot3: "3".into(),
            slot4: "4".into(),
            slot5: "5".into(),
            last_weapon: "Q".into(),
            buy: "B".into(),
            scoreboard: "Tab".into(),
            chat: "Y".into(),
            chat_team: "U".into(),
            inspector: "F3".into(),
        }
    }
}

impl Keys {
    /// Every action with its label in the settings screen, in display order.
    pub fn entries_mut(&mut self) -> [(&'static str, &mut String); 24] {
        [
            ("Avancer", &mut self.forward),
            ("Reculer", &mut self.back),
            ("Gauche", &mut self.left),
            ("Droite", &mut self.right),
            ("Sauter", &mut self.jump),
            ("S'accroupir", &mut self.duck),
            ("Marcher", &mut self.walk),
            ("Tir", &mut self.attack),
            ("Tir secondaire / zoom", &mut self.attack2),
            ("Recharger", &mut self.reload),
            ("Inspecter l'arme", &mut self.inspect),
            ("Utiliser", &mut self.use_key),
            ("Lâcher l'arme", &mut self.drop),
            ("Arme principale", &mut self.slot1),
            ("Arme secondaire", &mut self.slot2),
            ("Couteau", &mut self.slot3),
            ("Grenades", &mut self.slot4),
            ("Bombe", &mut self.slot5),
            ("Dernière arme", &mut self.last_weapon),
            ("Menu d'achat", &mut self.buy),
            ("Tableau des scores", &mut self.scoreboard),
            ("Chat général", &mut self.chat),
            ("Chat d'équipe", &mut self.chat_team),
            ("Inspecteur de tirs", &mut self.inspector),
        ]
    }

    /// Weapon slot selections (`timeline.select` index) and their binding.
    pub fn slots(&self) -> [(&str, u8); 6] {
        [
            (&self.slot1, 1),
            (&self.slot2, 2),
            (&self.slot3, 3),
            (&self.slot4, 4),
            (&self.slot5, 5),
            (&self.last_weapon, 6),
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Resource)]
#[serde(default)]
pub struct Settings {
    pub name: String,
    /// Source-style sensitivity: degrees per count = sensitivity × m_yaw.
    pub sensitivity: f32,
    pub m_yaw: f32,
    pub m_pitch: f32,
    pub invert_y: bool,
    /// Scoped sensitivity multiplier (zoom_sensitivity_ratio).
    pub zoom_sensitivity: f32,
    pub viewmodel_fov: f32,
    pub viewmodel_offset: [f32; 3],
    pub show_viewmodel: bool,
    pub volume: f32,
    pub fps_limit: u32,
    pub vsync: bool,
    pub fullscreen: bool,
    pub crosshair: Crosshair,
    pub keys: Keys,
    pub net_graph: bool,
    /// Network-lab variant (A, B, C, D) used when connecting.
    pub net_variant: String,
    /// Opponent extrapolation horizon in milliseconds (0, 15, 30).
    pub extrapolation_ms: f32,
    pub last_server: String,
    pub ui_scale: f32,
    /// Liquid characters: quality and ally/enemy colours.
    pub characters: crate::character::settings::CharacterSettings,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            name: "Joueur".into(),
            sensitivity: 2.0,
            m_yaw: 0.022,
            m_pitch: 0.022,
            invert_y: false,
            zoom_sensitivity: 1.0,
            viewmodel_fov: 68.0,
            viewmodel_offset: [2.5, -2.0, -2.0],
            show_viewmodel: true,
            volume: 0.6,
            fps_limit: 0,
            vsync: false,
            fullscreen: false,
            crosshair: Crosshair::default(),
            keys: Keys::default(),
            net_graph: false,
            net_variant: "C".into(),
            extrapolation_ms: 15.0,
            last_server: "127.0.0.1:27015".into(),
            ui_scale: 1.0,
            characters: Default::default(),
        }
    }
}

pub fn config_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("ProjetS")
}

impl Settings {
    pub fn path() -> PathBuf {
        config_dir().join("settings.toml")
    }

    pub fn load() -> Settings {
        match std::fs::read_to_string(Self::path()) {
            Ok(t) => toml::from_str(&t).unwrap_or_else(|e| {
                tracing::warn!("settings.toml illisible ({e}) : valeurs par défaut");
                Settings::default()
            }),
            Err(_) => Settings::default(),
        }
    }

    pub fn save(&self) {
        let p = Self::path();
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        match toml::to_string_pretty(self) {
            Ok(t) => {
                if let Err(e) = std::fs::write(&p, t) {
                    tracing::warn!("sauvegarde des réglages impossible : {e}");
                }
            }
            Err(e) => tracing::warn!("réglages : {e}"),
        }
    }

    /// Degrees of yaw per mouse count.
    pub fn yaw_per_count(&self, zoomed: bool) -> f32 {
        self.sensitivity * self.m_yaw * if zoomed { self.zoom_sensitivity } else { 1.0 }
    }

    pub fn pitch_per_count(&self, zoomed: bool) -> f32 {
        self.sensitivity
            * self.m_pitch
            * if zoomed { self.zoom_sensitivity } else { 1.0 }
            * if self.invert_y { -1.0 } else { 1.0 }
    }

    /// cm per 360° for a given DPI (shown in the settings).
    pub fn cm_per_360(&self, dpi: f32) -> f32 {
        let counts = 360.0 / (self.sensitivity * self.m_yaw);
        counts / dpi * 2.54
    }
}

/// World field of view: horizontal degrees at 4:3 (CS convention). Fixed for every
/// player (not a setting) so that the perceived speed is the same for everyone;
/// an old `fov` key in `settings.toml` is ignored.
pub const FOV: f32 = 84.0;

/// Vertical FOV (radians) from the CS-style 4:3 horizontal FOV, for an aspect ratio.
pub fn vertical_fov(hfov_43_deg: f32) -> f32 {
    let h = hfov_43_deg.to_radians();
    2.0 * ((h * 0.5).tan() * 3.0 / 4.0).atan()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_sensitivity_math() {
        let s = Settings {
            sensitivity: 2.0,
            m_yaw: 0.022,
            ..Settings::default()
        };
        assert!((s.yaw_per_count(false) - 0.044).abs() < 1e-6);
        // 800 DPI, sens 2: 360 / 0.044 = 8181.8 counts = 10.23 in = 25.98 cm.
        assert!((s.cm_per_360(800.0) - 25.98).abs() < 0.05);
        // 90° at 4:3 is 73.74° vertical.
        assert!((vertical_fov(90.0).to_degrees() - 73.74).abs() < 0.05);
        let t = toml::to_string(&s).unwrap();
        assert_eq!(toml::from_str::<Settings>(&t).unwrap(), s);
    }
}
