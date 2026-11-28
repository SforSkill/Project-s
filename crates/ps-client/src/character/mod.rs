//! Liquid-paint characters in the game: the generic presentation comes from
//! `ps-engine-character` (Project-s-engine); this module maps the game state
//! (teams, held weapon, events) onto it and drives one character per player.
//!
//! Nothing here feeds back into gameplay: hitboxes stay those of `ps-hitreg`
//! (driven by the networked pose only) and every effect is computed on the
//! client from the state it already receives. See `docs/client.md`.

#[cfg(test)]
mod hitbox_fit;
pub mod runtime;
pub mod settings;
mod weapon_models;

pub use ps_engine_character::{anim, fx, liquid, style};
pub use runtime::{CharEvent, CharState, CharacterFeed, FirstPerson};

use anim::Hold;
use ps_weapons::WeaponKind;

/// How a character holds the weapon kind it carries.
pub fn hold_for(kind: Option<WeaponKind>) -> Hold {
    match kind {
        Some(WeaponKind::Rifle | WeaponKind::Smg) => Hold::Rifle,
        Some(WeaponKind::SniperLight | WeaponKind::SniperHeavy) => Hold::Sniper,
        Some(WeaponKind::Pistol) => Hold::Pistol,
        Some(WeaponKind::Knife) => Hold::Knife,
        Some(WeaponKind::Grenade) => Hold::Grenade,
        Some(WeaponKind::Bomb) => Hold::Bomb,
        None => Hold::Empty,
    }
}

/// Registers the engine material and effects, then the game's resources and
/// shared meshes. The per-frame systems [`runtime::drive`] and
/// [`fx::update_fx`] are scheduled by `main.rs`, right after the camera (the
/// first-person arms hang from the viewmodel).
pub fn plugin(app: &mut bevy::prelude::App) {
    use bevy::prelude::*;
    ps_engine_character::plugin(app);
    app.init_resource::<CharacterFeed>()
        .init_resource::<runtime::Characters>()
        .add_systems(Startup, runtime::setup_assets);
}
