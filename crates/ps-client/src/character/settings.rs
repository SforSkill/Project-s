//! The player's character colour settings: maps the game's teams onto the
//! engine palettes (`ps_engine_character::style::PaletteId`).

use ps_engine_character::style::{PaletteId, Quality};
use ps_sim_core::Team;
use serde::{Deserialize, Serialize};

/// How players are coloured.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColorMode {
    /// Each side keeps its colour (defense = team A, attack = team B).
    Sides,
    /// Allies and enemies use the player's chosen colours, whatever the side.
    Relative,
}

/// Saved in `settings.toml` under `[characters]`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CharacterSettings {
    pub quality: Quality,
    pub mode: ColorMode,
    /// `Sides` mode.
    pub defense: PaletteId,
    pub attack: PaletteId,
    /// `Relative` mode.
    pub allies: PaletteId,
    pub enemies: PaletteId,
}

impl Default for CharacterSettings {
    fn default() -> Self {
        Self {
            quality: Quality::High,
            mode: ColorMode::Sides,
            defense: PaletteId::Blue,
            attack: PaletteId::Red,
            allies: PaletteId::Blue,
            enemies: PaletteId::Orange,
        }
    }
}

impl CharacterSettings {
    /// Palette of a player of `team` seen by a viewer of `viewer` team.
    /// Two different teams never get the same palette: if the player picked
    /// the same one twice, the second side falls back to a distinct colour.
    pub fn palette_for(&self, team: Team, viewer: Team) -> PaletteId {
        let (a, b) = self.pair();
        match self.mode {
            ColorMode::Sides => match team {
                Team::Defense => a,
                Team::Attack => b,
                Team::Spectator => PaletteId::White,
            },
            ColorMode::Relative => {
                if team == Team::Spectator {
                    PaletteId::White
                } else if viewer == Team::Spectator {
                    // A spectator has no allies: fall back to sides.
                    if team == Team::Defense { a } else { b }
                } else if team == viewer {
                    a
                } else {
                    b
                }
            }
        }
    }

    /// Same as [`Self::palette_for`], aware of free-for-all modes
    /// (deathmatch): there, in `Relative` mode, every other player is an
    /// enemy whatever their side.
    pub fn palette_in(&self, team: Team, viewer: Team, free_for_all: bool) -> PaletteId {
        if free_for_all && self.mode == ColorMode::Relative && team != Team::Spectator {
            return self.pair().1;
        }
        self.palette_for(team, viewer)
    }

    /// (first, second) palettes of the current mode, guaranteed distinct.
    pub fn pair(&self) -> (PaletteId, PaletteId) {
        let (a, b) = match self.mode {
            ColorMode::Sides => (self.defense, self.attack),
            ColorMode::Relative => (self.allies, self.enemies),
        };
        if a != b {
            return (a, b);
        }
        let alt = if a == PaletteId::Red {
            PaletteId::Blue
        } else {
            PaletteId::Red
        };
        (a, alt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn teams_never_share_a_palette() {
        let mut s = CharacterSettings::default();
        for mode in [ColorMode::Sides, ColorMode::Relative] {
            s.mode = mode;
            for a in PaletteId::ALL {
                for b in PaletteId::ALL {
                    s.defense = a;
                    s.attack = b;
                    s.allies = a;
                    s.enemies = b;
                    for viewer in [Team::Attack, Team::Defense, Team::Spectator] {
                        assert_ne!(
                            s.palette_for(Team::Attack, viewer),
                            s.palette_for(Team::Defense, viewer)
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn relative_mode_follows_viewer() {
        let s = CharacterSettings {
            mode: ColorMode::Relative,
            ..Default::default()
        };
        assert_eq!(s.palette_for(Team::Attack, Team::Attack), s.allies);
        assert_eq!(s.palette_for(Team::Defense, Team::Attack), s.enemies);
        assert_eq!(s.palette_for(Team::Defense, Team::Defense), s.allies);
        // Deathmatch: nobody is an ally.
        assert_eq!(s.palette_in(Team::Attack, Team::Attack, true), s.enemies);
        assert_eq!(s.palette_in(Team::Defense, Team::Attack, true), s.enemies);
    }
}
