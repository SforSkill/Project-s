//! Procedural sounds (no third-party audio asset): short PCM clips synthesised
//! at start-up. World sounds are spatial (panning from the camera's
//! `SpatialListener`) with a linear-power distance falloff over their range.

use bevy::audio::{AudioPlayer, AudioSource, PlaybackSettings, Volume};
use bevy::prelude::*;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Sfx {
    ShotPistol,
    ShotRifle,
    ShotSniper,
    Knife,
    Footstep,
    Land,
    Hit,
    HitHead,
    Kill,
    Explosion,
    Flash,
    SmokePop,
    Reload,
    DryFire,
    Beep,
    Plant,
    Defuse,
    Click,
}

const RATE: u32 = 44_100;

/// Small deterministic noise generator (xorshift).
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

fn wav(samples: &[f32]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut v = Vec::with_capacity(44 + data_len as usize);
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&(36 + data_len).to_le_bytes());
    v.extend_from_slice(b"WAVEfmt ");
    v.extend_from_slice(&16u32.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes()); // PCM
    v.extend_from_slice(&1u16.to_le_bytes()); // mono
    v.extend_from_slice(&RATE.to_le_bytes());
    v.extend_from_slice(&(RATE * 2).to_le_bytes());
    v.extend_from_slice(&2u16.to_le_bytes());
    v.extend_from_slice(&16u16.to_le_bytes());
    v.extend_from_slice(b"data");
    v.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        v.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32000.0) as i16).to_le_bytes());
    }
    v
}

/// Noise burst through a one-pole low-pass, with exponential decay.
fn burst(seconds: f32, decay: f32, lowpass: f32, tone: Option<(f32, f32)>, seed: u32) -> Vec<f32> {
    let n = (seconds * RATE as f32) as usize;
    let mut rng = Noise(seed | 1);
    let mut lp = 0.0f32;
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            lp += lowpass * (rng.next() - lp);
            let mut s = lp;
            if let Some((f, amp)) = tone {
                s += (t * f * std::f32::consts::TAU).sin() * amp;
            }
            let attack = (t / 0.002).min(1.0);
            s * attack * (-t * decay).exp()
        })
        .collect()
}

fn beep(freq: f32, seconds: f32) -> Vec<f32> {
    let n = (seconds * RATE as f32) as usize;
    (0..n)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let env = (t / 0.005).min(1.0) * ((seconds - t) / 0.02).clamp(0.0, 1.0);
            (t * freq * std::f32::consts::TAU).sin() * 0.5 * env
        })
        .collect()
}

fn synth(s: Sfx) -> Vec<f32> {
    match s {
        Sfx::ShotPistol => burst(0.25, 22.0, 0.55, Some((140.0, 0.4)), 7),
        Sfx::ShotRifle => burst(0.35, 16.0, 0.6, Some((95.0, 0.5)), 11),
        Sfx::ShotSniper => burst(0.8, 6.0, 0.5, Some((60.0, 0.6)), 13),
        Sfx::Knife => burst(0.12, 40.0, 0.9, None, 17),
        Sfx::Footstep => burst(0.09, 45.0, 0.12, Some((70.0, 0.3)), 19),
        Sfx::Land => burst(0.15, 30.0, 0.1, Some((55.0, 0.5)), 23),
        Sfx::Hit => burst(0.08, 50.0, 0.8, Some((900.0, 0.3)), 29),
        Sfx::HitHead => {
            let mut v = beep(1800.0, 0.09);
            for (a, b) in v.iter_mut().zip(burst(0.09, 40.0, 0.9, None, 31)) {
                *a += b * 0.5;
            }
            v
        }
        Sfx::Kill => {
            let mut v = beep(660.0, 0.08);
            v.extend(beep(990.0, 0.12));
            v
        }
        Sfx::Explosion => burst(1.4, 3.5, 0.08, Some((40.0, 0.7)), 37),
        Sfx::Flash => burst(0.5, 8.0, 0.95, Some((3000.0, 0.2)), 41),
        Sfx::SmokePop => burst(0.9, 4.0, 0.3, None, 43),
        Sfx::Reload => {
            let mut v = burst(0.05, 80.0, 0.9, None, 47);
            v.extend(vec![0.0; 6000]);
            v.extend(burst(0.06, 70.0, 0.8, None, 53));
            v
        }
        Sfx::DryFire => burst(0.04, 90.0, 0.95, None, 59),
        Sfx::Beep => beep(1400.0, 0.08),
        Sfx::Plant => beep(800.0, 0.15),
        Sfx::Defuse => burst(0.2, 20.0, 0.7, Some((500.0, 0.2)), 61),
        Sfx::Click => beep(1000.0, 0.03),
    }
}

#[derive(Resource)]
pub struct Sounds {
    handles: Vec<(Sfx, Handle<AudioSource>)>,
    pub volume: f32,
}

pub fn setup(mut commands: Commands, mut assets: ResMut<Assets<AudioSource>>) {
    use Sfx::*;
    let all = [
        ShotPistol, ShotRifle, ShotSniper, Knife, Footstep, Land, Hit, HitHead, Kill, Explosion,
        Flash, SmokePop, Reload, DryFire, Beep, Plant, Defuse, Click,
    ];
    let handles = all
        .iter()
        .map(|s| {
            (
                *s,
                assets.add(AudioSource {
                    bytes: Arc::from(wav(&synth(*s))),
                }),
            )
        })
        .collect();
    commands.insert_resource(Sounds {
        handles,
        volume: 0.6,
    });
}

impl Sounds {
    fn handle(&self, s: Sfx) -> Option<Handle<AudioSource>> {
        self.handles
            .iter()
            .find(|(k, _)| *k == s)
            .map(|(_, h)| h.clone())
    }

    /// Plays `s` at full volume (UI, local player).
    pub fn play(&self, commands: &mut Commands, s: Sfx, gain: f32) {
        if let Some(h) = self.handle(s) {
            let v = (self.volume * gain).clamp(0.0, 2.0);
            if v > 0.001 {
                commands.spawn((
                    AudioPlayer(h),
                    PlaybackSettings::DESPAWN.with_volume(Volume::Linear(v)),
                ));
            }
        }
    }

    /// Plays `s` emitted at `pos` (game units) heard from `ear` (distance attenuation).
    pub fn play_at(
        &self,
        commands: &mut Commands,
        s: Sfx,
        pos: Vec3,
        ear: Vec3,
        range: f32,
        gain: f32,
    ) {
        let d = pos.distance(ear);
        if d > range {
            return;
        }
        let att = (1.0 - d / range).powf(1.6);
        let v = (self.volume * gain * att).clamp(0.0, 2.0);
        if let (Some(h), true) = (self.handle(s), v > 0.001) {
            commands.spawn((
                AudioPlayer(h),
                PlaybackSettings::DESPAWN
                    .with_volume(Volume::Linear(v))
                    .with_spatial(true),
                Transform::from_translation(pos * crate::world::SCALE),
            ));
        }
    }
}
