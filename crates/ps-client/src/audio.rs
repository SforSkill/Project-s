//! Procedural sounds (no third-party audio asset): short PCM clips synthesised
//! at start-up. World sounds are spatial (panning from the camera's
//! `SpatialListener`) with a linear-power distance falloff over their range.
//!
//! Art direction: "liquid paint", like the characters (ADR 0005). Every sound is
//! built from a few water-like layers, kept soft on the ear:
//! - `bubble`: a sine whose pitch rises as it decays (the resonance of an air
//!   bubble in water); short = a drop, low = a "glug", high = a "plip";
//! - `bubbles`: a random cloud of bubbles (gurgle, fizz, boiling);
//! - `thump`: a sine whose pitch falls (the body of shots, landings, blasts);
//! - `noise_band`: noise through a swept resonant filter (splash, swish, hiss);
//! - `wet`: a short diffuse tail (small Schroeder reverb) that rounds attacks.
//!
//! Each clip ends in `finish` (gentle `tanh` saturation, peak normalisation,
//! fades), so no clip clips or clicks. Frequent sounds (steps, hits, shots)
//! have several variants played in turn, so repeats do not sound mechanical.

use bevy::audio::{AudioPlayer, AudioSource, PlaybackSettings, Volume};
use bevy::prelude::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

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

/// Mixing category. Only these are adjustable by the player: one common volume
/// for everything that happens in the world (so nobody can mute footsteps or
/// grenades on their own), one for the interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    /// Weapons, footsteps, landings, grenades, reload, bomb.
    Game,
    /// Clicks and the local hit / kill feedback.
    Ui,
}

impl Sfx {
    pub fn category(self) -> Category {
        match self {
            Sfx::Click | Sfx::Beep | Sfx::Hit | Sfx::HitHead | Sfx::Kill => Category::Ui,
            _ => Category::Game,
        }
    }
}

/// Volume sliders as stored (0..=1, linear in the slider).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mix {
    pub master: f32,
    pub game: f32,
    pub ui: f32,
    pub muted: bool,
}

impl Mix {
    /// Perceptual curve: the gain is the square of the slider (about -12 dB at
    /// 50 %), applied to the gain only, never to what is stored.
    pub fn curve(v: f32) -> f32 {
        let v = v.clamp(0.0, 1.0);
        v * v
    }

    /// Final linear gain for a category (0 when muted).
    pub fn gain(&self, c: Category) -> f32 {
        if self.muted {
            return 0.0;
        }
        let cat = match c {
            Category::Game => self.game,
            Category::Ui => self.ui,
        };
        Self::curve(self.master) * Self::curve(cat)
    }
}

const ALL: [Sfx; 18] = [
    Sfx::ShotPistol,
    Sfx::ShotRifle,
    Sfx::ShotSniper,
    Sfx::Knife,
    Sfx::Footstep,
    Sfx::Land,
    Sfx::Hit,
    Sfx::HitHead,
    Sfx::Kill,
    Sfx::Explosion,
    Sfx::Flash,
    Sfx::SmokePop,
    Sfx::Reload,
    Sfx::DryFire,
    Sfx::Beep,
    Sfx::Plant,
    Sfx::Defuse,
    Sfx::Click,
];

const RATE: u32 = 44_100;
const SR: f32 = RATE as f32;
const TAU: f32 = std::f32::consts::TAU;

/// Small deterministic noise generator (xorshift).
struct Noise(u32);

impl Noise {
    fn new(seed: u32) -> Self {
        Noise(seed.wrapping_mul(0x9E37_79B9) | 1)
    }

    /// Uniform in [-1, 1].
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        (self.0 as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    /// Uniform in [0, 1].
    fn unit(&mut self) -> f32 {
        self.next() * 0.5 + 0.5
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

fn len(seconds: f32) -> usize {
    (seconds * SR) as usize
}

fn peak(v: &[f32]) -> f32 {
    v.iter().fold(0.0f32, |m, s| m.max(s.abs()))
}

/// Adds `src`, normalised to a peak of `gain`, into `dst` from `at` seconds.
fn layer(dst: &mut Vec<f32>, src: &[f32], at: f32, gain: f32) {
    let p = peak(src);
    if p <= f32::EPSILON {
        return;
    }
    let start = len(at);
    if dst.len() < start + src.len() {
        dst.resize(start + src.len(), 0.0);
    }
    for (d, s) in dst[start..].iter_mut().zip(src) {
        *d += s / p * gain;
    }
}

/// Air bubble in water: sine whose pitch rises (`f0 · (1 + rise · t)`) while
/// it decays. The 1 ms attack avoids a click.
fn bubble(f0: f32, rise: f32, seconds: f32, decay: f32) -> Vec<f32> {
    let mut phase = 0.0f32;
    (0..len(seconds))
        .map(|i| {
            let t = i as f32 / SR;
            phase = (phase + TAU * f0 * (1.0 + rise * t) / SR) % TAU;
            phase.sin() * (t / 0.001).min(1.0) * (-t * decay).exp()
        })
        .collect()
}

/// Cloud of `count` random bubbles between `fmin` and `fmax` Hz over
/// `seconds`, fading towards the end (gurgle, fizz).
fn bubbles(seconds: f32, count: u32, fmin: f32, fmax: f32, seed: u32) -> Vec<f32> {
    let mut rng = Noise::new(seed);
    let mut out = vec![0.0; len(seconds)];
    for _ in 0..count {
        let at = rng.unit() * seconds * 0.85;
        let f = fmin * (fmax / fmin).powf(rng.unit());
        let amp = (1.0 - at / seconds).powf(1.5) * (0.4 + 0.6 * rng.unit());
        let life = (12.0 / f).clamp(0.012, 0.08);
        let b = bubble(f, 6.0 + 14.0 * rng.unit(), life * 3.0, 3.0 / life);
        let start = len(at);
        for (d, s) in out[start..].iter_mut().zip(&b) {
            *d += s * amp;
        }
    }
    out
}

/// Sine whose pitch falls from `f0` to `f1` Hz: the body of an impact.
fn thump(f0: f32, f1: f32, seconds: f32, decay: f32) -> Vec<f32> {
    let mut phase = 0.0f32;
    (0..len(seconds))
        .map(|i| {
            let t = i as f32 / SR;
            let f = f1 + (f0 - f1) * (-t * 30.0).exp();
            phase = (phase + TAU * f / SR) % TAU;
            phase.sin() * (t / 0.002).min(1.0) * (-t * decay).exp()
        })
        .collect()
}

#[derive(Clone, Copy)]
enum Pass {
    Low,
    Band,
}

/// Noise through a resonant state-variable filter whose cutoff glides
/// exponentially from `fc0` to `fc1` Hz. `damp` is 1/Q (low = more resonant,
/// more "watery"). The envelope rises over `attack` then decays.
#[allow(clippy::too_many_arguments)]
fn noise_band(
    seconds: f32,
    fc0: f32,
    fc1: f32,
    damp: f32,
    pass: Pass,
    attack: f32,
    decay: f32,
    seed: u32,
) -> Vec<f32> {
    let n = len(seconds);
    let mut rng = Noise::new(seed);
    let (mut low, mut band) = (0.0f32, 0.0f32);
    (0..n)
        .map(|i| {
            let t = i as f32 / SR;
            let fc = (fc0 * (fc1 / fc0).powf(t / seconds)).min(7000.0);
            let f = 2.0 * (std::f32::consts::PI * fc / SR).sin();
            low += f * band;
            let high = rng.next() - low - damp * band;
            band += f * high;
            let s = match pass {
                Pass::Low => low,
                Pass::Band => band,
            };
            let env = (t / attack.max(0.0005)).min(1.0) * (-(t - attack).max(0.0) * decay).exp();
            s * env
        })
        .collect()
}

/// Short diffuse tail (4 damped combs + 2 allpasses, Freeverb tunings), mixed
/// at `mix` and lasting `tail` seconds after the dry sound.
fn wet(dry: Vec<f32>, mix: f32, tail: f32) -> Vec<f32> {
    let n = dry.len() + len(tail);
    let mut verb = vec![0.0f32; n];
    for d in [1116usize, 1188, 1277, 1356] {
        let (mut buf, mut idx, mut store) = (vec![0.0f32; d], 0, 0.0f32);
        for (i, out) in verb.iter_mut().enumerate() {
            let y = buf[idx];
            store = y * 0.65 + store * 0.35;
            buf[idx] = dry.get(i).copied().unwrap_or(0.0) + store * 0.74;
            idx = (idx + 1) % d;
            *out += y * 0.25;
        }
    }
    for d in [556usize, 441] {
        let (mut buf, mut idx) = (vec![0.0f32; d], 0);
        for s in verb.iter_mut() {
            let b = buf[idx];
            buf[idx] = *s + b * 0.5;
            *s = b - *s;
            idx = (idx + 1) % d;
        }
    }
    let wet_peak = peak(&verb).max(f32::EPSILON);
    let dry_peak = peak(&dry);
    (0..n)
        .map(|i| dry.get(i).copied().unwrap_or(0.0) + verb[i] / wet_peak * dry_peak * mix)
        .collect()
}

/// Gentle saturation, normalisation to `level` and edge fades.
fn finish(mut v: Vec<f32>, level: f32) -> Vec<f32> {
    let p = peak(&v);
    if p <= f32::EPSILON {
        return v;
    }
    let k = 1.4f32;
    let norm = k.tanh();
    for s in v.iter_mut() {
        *s = (*s / p * k).tanh() / norm * level;
    }
    let n = v.len();
    let fade = len(0.012).min(n / 2);
    for i in 0..fade {
        v[n - 1 - i] *= i as f32 / fade as f32;
    }
    if let Some(first) = v.first_mut() {
        *first = 0.0;
    }
    v
}

/// Number of variants played in turn for `s`.
fn variants(s: Sfx) -> u32 {
    match s {
        Sfx::Footstep => 4,
        Sfx::ShotPistol | Sfx::ShotRifle | Sfx::Hit => 3,
        Sfx::Land => 2,
        _ => 1,
    }
}

fn synth(s: Sfx, var: u32) -> Vec<f32> {
    let v = var as f32;
    let seed = (s as u32) * 97 + var * 13 + 1;
    let mut o = Vec::new();
    match s {
        // Shots: falling body, splash going down, a drop in the tail.
        Sfx::ShotPistol => {
            layer(&mut o, &thump(230.0 + 12.0 * v, 62.0, 0.2, 20.0), 0.0, 0.9);
            let splash = noise_band(0.13, 2600.0, 700.0, 0.55, Pass::Band, 0.001, 32.0, seed);
            layer(&mut o, &splash, 0.0, 0.7);
            layer(
                &mut o,
                &bubble(360.0 + 35.0 * v, 7.0, 0.1, 32.0),
                0.025,
                0.3,
            );
            finish(wet(o, 0.18, 0.3), 0.9)
        }
        Sfx::ShotRifle => {
            layer(
                &mut o,
                &thump(190.0 + 10.0 * v, 55.0, 0.22, 18.0),
                0.0,
                0.95,
            );
            let splash = noise_band(0.16, 3300.0, 850.0, 0.5, Pass::Band, 0.001, 27.0, seed);
            layer(&mut o, &splash, 0.0, 0.8);
            layer(
                &mut o,
                &bubble(290.0 + 30.0 * v, 6.0, 0.12, 28.0),
                0.03,
                0.3,
            );
            finish(wet(o, 0.2, 0.32), 0.95)
        }
        Sfx::ShotSniper => {
            layer(&mut o, &thump(150.0, 34.0, 0.7, 6.5), 0.0, 1.0);
            let splash = noise_band(0.5, 3000.0, 380.0, 0.42, Pass::Band, 0.001, 8.5, seed);
            layer(&mut o, &splash, 0.0, 0.8);
            layer(&mut o, &bubbles(0.6, 9, 180.0, 620.0, seed), 0.1, 0.3);
            finish(wet(o, 0.3, 0.5), 1.0)
        }
        // Swish through water, then a small drop.
        Sfx::Knife => {
            let swish = noise_band(0.17, 650.0, 3600.0, 0.32, Pass::Band, 0.035, 15.0, seed);
            layer(&mut o, &swish, 0.0, 0.8);
            layer(&mut o, &bubble(900.0, 9.0, 0.05, 60.0), 0.09, 0.22);
            finish(wet(o, 0.15, 0.2), 0.6)
        }
        // Soft squelch: low glug + damp noise + tiny droplet.
        Sfx::Footstep => {
            layer(
                &mut o,
                &bubble(135.0 + 22.0 * v, 10.0, 0.08, 42.0),
                0.0,
                0.6,
            );
            let squelch = noise_band(
                0.08,
                520.0 + 40.0 * v,
                240.0,
                1.0,
                Pass::Band,
                0.002,
                45.0,
                seed,
            );
            layer(&mut o, &squelch, 0.0, 0.55);
            layer(
                &mut o,
                &bubble(680.0 + 95.0 * v, 12.0, 0.035, 90.0),
                0.016,
                0.12,
            );
            finish(o, 0.45)
        }
        Sfx::Land => {
            layer(
                &mut o,
                &thump(125.0 + 10.0 * v, 44.0, 0.22, 17.0),
                0.0,
                0.85,
            );
            let splat = noise_band(0.2, 950.0, 280.0, 0.8, Pass::Band, 0.002, 21.0, seed);
            layer(&mut o, &splat, 0.0, 0.6);
            layer(&mut o, &bubbles(0.28, 5, 300.0, 850.0, seed), 0.035, 0.25);
            finish(wet(o, 0.12, 0.25), 0.6)
        }
        // Feedback: rising "plip", clear but soft.
        Sfx::Hit => {
            layer(
                &mut o,
                &bubble(640.0 + 60.0 * v, 24.0, 0.07, 45.0),
                0.0,
                0.8,
            );
            let tick = noise_band(0.025, 4200.0, 2200.0, 0.5, Pass::Band, 0.0005, 140.0, seed);
            layer(&mut o, &tick, 0.0, 0.3);
            finish(wet(o, 0.1, 0.15), 0.6)
        }
        // Headshot: bright drop plus a glassy ping, unmistakable.
        Sfx::HitHead => {
            layer(&mut o, &bubble(1300.0, 18.0, 0.09, 30.0), 0.0, 0.75);
            layer(&mut o, &bubble(2600.0, 0.4, 0.3, 22.0), 0.004, 0.35);
            let tick = noise_band(0.02, 5000.0, 3000.0, 0.5, Pass::Band, 0.0005, 160.0, seed);
            layer(&mut o, &tick, 0.0, 0.25);
            finish(wet(o, 0.22, 0.3), 0.7)
        }
        // Two rising drops (D5 then A5) and a shimmer of fine bubbles.
        Sfx::Kill => {
            layer(&mut o, &bubble(587.0, 1.2, 0.13, 17.0), 0.0, 0.7);
            layer(&mut o, &bubble(880.0, 1.0, 0.22, 11.0), 0.085, 0.8);
            layer(&mut o, &bubbles(0.3, 6, 1500.0, 2700.0, seed), 0.1, 0.15);
            finish(wet(o, 0.3, 0.4), 0.7)
        }
        // Deep boom, rolling wash, then a long gurgle.
        Sfx::Explosion => {
            layer(&mut o, &thump(95.0, 28.0, 1.3, 3.2), 0.0, 1.0);
            let wash = noise_band(1.3, 1300.0, 140.0, 0.7, Pass::Low, 0.004, 3.8, seed);
            layer(&mut o, &wash, 0.0, 0.8);
            layer(&mut o, &bubbles(1.3, 30, 110.0, 720.0, seed), 0.08, 0.4);
            finish(wet(o, 0.3, 0.5), 1.0)
        }
        // Fizz: bright hiss and a cloud of tiny bubbles (carbonation).
        Sfx::Flash => {
            let hiss = noise_band(0.65, 6500.0, 3800.0, 0.5, Pass::Band, 0.002, 6.0, seed);
            layer(&mut o, &hiss, 0.0, 0.45);
            layer(&mut o, &bubbles(0.7, 45, 1900.0, 5200.0, seed), 0.0, 0.4);
            layer(&mut o, &bubble(3000.0, 0.2, 0.5, 8.0), 0.0, 0.15);
            finish(wet(o, 0.25, 0.35), 0.8)
        }
        // Round pop, then a soft hiss that slowly bubbles out.
        Sfx::SmokePop => {
            layer(&mut o, &bubble(210.0, 4.0, 0.16, 20.0), 0.0, 0.8);
            let hiss = noise_band(1.0, 1900.0, 1100.0, 0.8, Pass::Band, 0.05, 3.5, seed);
            layer(&mut o, &hiss, 0.0, 0.35);
            layer(&mut o, &bubbles(1.0, 14, 380.0, 1200.0, seed), 0.05, 0.2);
            finish(wet(o, 0.25, 0.4), 0.7)
        }
        // Magazine out (low glug), magazine in (higher, with a click).
        Sfx::Reload => {
            for (at, f, g) in [(0.0, 260.0, 0.55), (0.17, 380.0, 0.65)] {
                layer(&mut o, &bubble(f, 12.0, 0.06, 50.0), at, g);
                let click = noise_band(0.03, 2600.0, 2000.0, 0.4, Pass::Band, 0.0005, 120.0, seed);
                layer(&mut o, &click, at, 0.45);
            }
            finish(wet(o, 0.1, 0.15), 0.6)
        }
        // Empty: small hollow pop, dry.
        Sfx::DryFire => {
            layer(&mut o, &bubble(420.0, 6.0, 0.045, 85.0), 0.0, 0.6);
            let tick = noise_band(0.015, 3000.0, 2500.0, 0.6, Pass::Band, 0.0005, 200.0, seed);
            layer(&mut o, &tick, 0.0, 0.3);
            finish(o, 0.5)
        }
        // Bomb tick: tonal and clear, rounded by a slight glide.
        Sfx::Beep => {
            layer(&mut o, &bubble(1320.0, 0.8, 0.09, 12.0), 0.0, 1.0);
            finish(wet(o, 0.12, 0.12), 0.55)
        }
        Sfx::Plant => {
            layer(&mut o, &bubble(700.0, 1.5, 0.09, 15.0), 0.0, 0.7);
            layer(&mut o, &bubble(1050.0, 1.2, 0.14, 12.0), 0.075, 0.8);
            finish(wet(o, 0.2, 0.25), 0.6)
        }
        // Draining bubbles ending in a rising drop.
        Sfx::Defuse => {
            layer(&mut o, &bubbles(0.3, 10, 480.0, 1400.0, seed), 0.0, 0.5);
            layer(&mut o, &bubble(900.0, 3.0, 0.16, 14.0), 0.22, 0.8);
            finish(wet(o, 0.2, 0.25), 0.6)
        }
        // UI: tiny "plip".
        Sfx::Click => {
            layer(&mut o, &bubble(1100.0, 14.0, 0.035, 70.0), 0.0, 1.0);
            finish(o, 0.4)
        }
    }
}

#[derive(Resource)]
pub struct Sounds {
    handles: Vec<(Sfx, Vec<Handle<AudioSource>>)>,
    /// Round-robin counter over the variants of a sound.
    next: AtomicUsize,
    pub mix: Mix,
}

pub fn setup(mut commands: Commands, mut assets: ResMut<Assets<AudioSource>>) {
    let handles = ALL
        .iter()
        .map(|&s| {
            let clips = (0..variants(s))
                .map(|v| {
                    assets.add(AudioSource {
                        bytes: Arc::from(wav(&synth(s, v))),
                    })
                })
                .collect();
            (s, clips)
        })
        .collect();
    commands.insert_resource(Sounds {
        handles,
        next: AtomicUsize::new(0),
        mix: Mix {
            master: 0.8,
            game: 1.0,
            ui: 1.0,
            muted: false,
        },
    });
}

impl Sounds {
    fn handle(&self, s: Sfx) -> Option<Handle<AudioSource>> {
        let (_, clips) = self.handles.iter().find(|(k, _)| *k == s)?;
        if clips.is_empty() {
            return None;
        }
        let i = self.next.fetch_add(1, Ordering::Relaxed) % clips.len();
        Some(clips[i].clone())
    }

    /// Plays `s` at full volume (UI, local player).
    pub fn play(&self, commands: &mut Commands, s: Sfx, gain: f32) {
        let v = (self.mix.gain(s.category()) * gain).clamp(0.0, 2.0);
        if v > 0.001
            && let Some(h) = self.handle(s)
        {
            commands.spawn((
                AudioPlayer(h),
                PlaybackSettings::DESPAWN.with_volume(Volume::Linear(v)),
            ));
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
        let v = (self.mix.gain(s.category()) * gain * att).clamp(0.0, 2.0);
        if v > 0.001
            && let Some(h) = self.handle(s)
        {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mix_curve_and_mute() {
        let m = Mix {
            master: 0.5,
            game: 1.0,
            ui: 0.5,
            muted: false,
        };
        assert!((m.gain(Category::Game) - 0.25).abs() < 1e-6);
        assert!((m.gain(Category::Ui) - 0.0625).abs() < 1e-6);
        assert_eq!(Mix { muted: true, ..m }.gain(Category::Game), 0.0);
        assert_eq!(Mix::curve(2.0), 1.0);
        assert_eq!(Mix::curve(-1.0), 0.0);
    }

    #[test]
    fn categories() {
        assert_eq!(Sfx::Footstep.category(), Category::Game);
        assert_eq!(Sfx::ShotRifle.category(), Category::Game);
        assert_eq!(Sfx::Click.category(), Category::Ui);
        assert_eq!(Sfx::Kill.category(), Category::Ui);
    }

    #[test]
    fn every_clip_is_clean() {
        for s in ALL {
            for var in 0..variants(s) {
                let v = synth(s, var);
                assert!(!v.is_empty(), "{s:?}/{var} empty");
                assert!(v.iter().all(|x| x.is_finite()), "{s:?}/{var} not finite");
                let p = peak(&v);
                assert!(p > 0.2 && p <= 1.0, "{s:?}/{var} peak {p}");
                assert!(v.len() < len(2.5), "{s:?}/{var} too long");
                assert!(
                    v[0].abs() < 1e-3 && v[v.len() - 1].abs() < 1e-3,
                    "{s:?}/{var} clicks"
                );
            }
        }
    }

    #[test]
    fn synthesis_is_deterministic_and_variants_differ() {
        assert_eq!(synth(Sfx::Explosion, 0), synth(Sfx::Explosion, 0));
        assert_ne!(synth(Sfx::Footstep, 0), synth(Sfx::Footstep, 1));
    }

    /// Writes every clip to `target/sfx-preview/` to listen to them:
    /// `cargo test -p ps-client --release audio::tests::export_wavs -- --ignored`.
    #[test]
    #[ignore = "writes files; run by hand to listen"]
    fn export_wavs() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/sfx-preview");
        std::fs::create_dir_all(&dir).unwrap();
        for s in ALL {
            for var in 0..variants(s) {
                let path = dir.join(format!("{s:?}-{var}.wav"));
                std::fs::write(path, wav(&synth(s, var))).unwrap();
            }
        }
    }

    #[test]
    fn wav_header_matches_data() {
        let w = wav(&synth(Sfx::Click, 0));
        let data = u32::from_le_bytes(w[40..44].try_into().unwrap()) as usize;
        assert_eq!(w.len(), 44 + data);
    }
}
