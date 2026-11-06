//! Procedural sound effects: every sound is synthesised at startup (no audio files), with a
//! few variations of the frequent ones so repeated gunfire doesn't sound mechanical.
//! Playback is rate-limited per sound and per frame, so 4x speed stays listenable.

use crate::synth::{Lcg, SR, wav};
use macroquad::audio::{PlaySoundParams, Sound, load_sound_from_bytes, play_sound};
use std::f32::consts::TAU;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Sfx {
    Deputy,
    Marksman,
    Gas,
    Jammer,
    RebelShot,
    Explosion,
    Kill,
    Coin,
    Breach,
    Place,
    Upgrade,
    Sell,
    BarrierHit,
    BarrierBroken,
    TowerDestroyed,
    ItDown,
    ItUp,
    PowerCut,
    Horn,
    Cleared,
    Equip,
    Click,
}

impl Sfx {
    pub const ALL: [Sfx; 22] = [
        Sfx::Deputy,
        Sfx::Marksman,
        Sfx::Gas,
        Sfx::Jammer,
        Sfx::RebelShot,
        Sfx::Explosion,
        Sfx::Kill,
        Sfx::Coin,
        Sfx::Breach,
        Sfx::Place,
        Sfx::Upgrade,
        Sfx::Sell,
        Sfx::BarrierHit,
        Sfx::BarrierBroken,
        Sfx::TowerDestroyed,
        Sfx::ItDown,
        Sfx::ItUp,
        Sfx::PowerCut,
        Sfx::Horn,
        Sfx::Cleared,
        Sfx::Equip,
        Sfx::Click,
    ];

    /// Minimum seconds between two plays of this sound, and its base volume.
    fn spacing_and_gain(self) -> (f64, f32) {
        match self {
            Sfx::Deputy => (0.05, 0.35),
            Sfx::RebelShot => (0.07, 0.3),
            Sfx::Marksman => (0.12, 0.6),
            Sfx::Gas => (0.1, 0.5),
            Sfx::Jammer => (0.12, 0.3),
            Sfx::Explosion => (0.1, 0.45),
            Sfx::Kill => (0.06, 0.35),
            Sfx::Coin => (0.08, 0.18),
            Sfx::BarrierHit => (0.12, 0.3),
            Sfx::Equip => (0.15, 0.35),
            Sfx::Click => (0.03, 0.3),
            _ => (0.3, 0.7),
        }
    }

    fn variants(self) -> u32 {
        match self {
            Sfx::Deputy | Sfx::RebelShot | Sfx::Kill | Sfx::BarrierHit | Sfx::Explosion => 3,
            _ => 1,
        }
    }
}

/// A one-shot buffer.
struct Buf(Vec<f32>);

impl Buf {
    fn new(secs: f32) -> Self {
        Self(vec![0.0; (secs * SR as f32) as usize])
    }

    fn add(&mut self, start: f32, mut f: impl FnMut(f32) -> f32) {
        let s0 = (start * SR as f32) as usize;
        for i in s0..self.0.len() {
            let t = (i - s0) as f32 / SR as f32;
            self.0[i] += f(t);
        }
    }

    /// Simple feedback echo for a sense of the concrete stairwell.
    fn echo(&mut self, delay: f32, fb: f32) {
        let d = (delay * SR as f32) as usize;
        for i in d..self.0.len() {
            self.0[i] += self.0[i - d] * fb;
        }
    }

    /// Fade the last 10 ms so nothing clicks at the end.
    fn finish(mut self) -> Vec<f32> {
        let n = self.0.len();
        let fade = (0.01 * SR as f32) as usize;
        for i in 0..fade.min(n) {
            self.0[n - 1 - i] *= i as f32 / fade as f32;
        }
        self.0
    }
}

/// One-pole low-pass filter state.
struct Lp(f32, f32);
impl Lp {
    fn new(cut: f32) -> Self {
        Lp(0.0, (TAU * cut / SR as f32).min(1.0))
    }
    fn run(&mut self, x: f32) -> f32 {
        self.0 += (x - self.0) * self.1;
        self.0
    }
}

/// Phase of a sine sweeping exponentially from `f0` to `f1` with rate `k`.
fn sweep(t: f32, f0: f32, f1: f32, k: f32) -> f32 {
    TAU * (f1 * t + (f0 - f1) / k * (1.0 - (-t * k).exp()))
}

fn synth(kind: Sfx, seed: u32) -> Vec<f32> {
    let mut rng = Lcg(seed * 7919 + kind as u32 * 104_729 + 1);
    let v = (seed as f32 - 1.0) * 0.06; // per-variant pitch nudge
    match kind {
        Sfx::Deputy => {
            let mut b = Buf::new(0.35);
            let mut lp = Lp::new(1800.0);
            b.add(0.0, |t| {
                let n = rng.next();
                let crack = n * (-t * 160.0).exp();
                let body = lp.run(n) * (-t * 22.0).exp() * 1.6;
                let thump = sweep(t, 180.0 * (1.0 + v), 60.0, 30.0).sin() * (-t * 25.0).exp() * 0.8;
                crack + body + thump
            });
            b.echo(0.09, 0.25);
            b.finish()
        }
        Sfx::RebelShot => {
            let mut b = Buf::new(0.25);
            let mut prev = 0.0;
            b.add(0.0, |t| {
                let n = rng.next();
                let hp = n - prev;
                prev = n;
                hp * (-t * 90.0).exp() * 0.8
                    + sweep(t, 420.0 * (1.0 + v), 160.0, 40.0).sin() * (-t * 40.0).exp() * 0.5
            });
            b.echo(0.07, 0.2);
            b.finish()
        }
        Sfx::Marksman => {
            let mut b = Buf::new(1.2);
            let mut lp = Lp::new(700.0);
            b.add(0.0, |t| {
                let n = rng.next();
                n * (-t * 120.0).exp() * 1.2
                    + lp.run(n) * (-t * 7.0).exp() * 2.5
                    + sweep(t, 120.0, 45.0, 12.0).sin() * (-t * 6.0).exp() * 0.9
            });
            b.echo(0.16, 0.35);
            b.finish()
        }
        Sfx::Gas => {
            let mut b = Buf::new(0.9);
            let mut lp = Lp::new(500.0);
            let mut prev = 0.0;
            b.add(0.0, |t| {
                let n = rng.next();
                let thoomp = sweep(t, 110.0, 40.0, 14.0).sin() * (-t * 10.0).exp();
                let hp = n - prev;
                prev = n;
                let hiss = hp * 0.35 * (t * 8.0).min(1.0) * (-t * 4.0).exp();
                thoomp + lp.run(n) * (-t * 14.0).exp() + hiss
            });
            b.finish()
        }
        Sfx::Jammer => {
            let mut b = Buf::new(0.45);
            b.add(0.0, |t| {
                let f = 700.0 - 900.0 * t + 120.0 * (TAU * 34.0 * t).sin();
                let p = TAU * f.max(120.0) * t;
                let sq = if p.sin() > 0.0 { 1.0 } else { -1.0 };
                // Bit-crushed square, a radio-static texture.
                (sq * 0.5 + p.sin() * 0.5) * (-t * 6.0).exp() * (t * 200.0).min(1.0) * 0.6
            });
            b.finish()
        }
        Sfx::Explosion => {
            let mut b = Buf::new(1.0);
            let mut lp = Lp::new(400.0 + 100.0 * seed as f32);
            b.add(0.0, |t| {
                let n = rng.next();
                lp.run(n) * (-t * 4.5).exp() * 3.0
                    + sweep(t, 90.0, 30.0, 8.0).sin() * (-t * 5.0).exp()
                    + n * (-t * 60.0).exp() * 0.6
            });
            b.finish()
        }
        Sfx::Kill => {
            // A body hitting the steel stair: dull thud and a rattle.
            let mut b = Buf::new(0.35);
            let mut lp = Lp::new(300.0);
            b.add(0.0, |t| {
                sweep(t, 140.0 * (1.0 + v), 55.0, 20.0).sin() * (-t * 18.0).exp()
                    + lp.run(rng.next()) * (-t * 12.0).exp() * 1.5
            });
            b.add(0.06, |t| {
                ((TAU * 610.0 * t).sin() + (TAU * 947.0 * t).sin()) * (-t * 30.0).exp() * 0.15
            });
            b.finish()
        }
        Sfx::Coin => {
            let mut b = Buf::new(0.3);
            for (i, f) in [1320.0, 1760.0].iter().enumerate() {
                b.add(i as f32 * 0.06, |t| {
                    (TAU * f * t).sin() * (-t * 22.0).exp() * 0.5
                });
            }
            b.finish()
        }
        Sfx::Breach => {
            // Klaxon: alternating two-tone square, three cycles.
            let mut b = Buf::new(1.3);
            let mut lp = Lp::new(2500.0);
            b.add(0.0, |t| {
                let f = if (t * 5.0) as i32 % 2 == 0 {
                    440.0
                } else {
                    554.0
                };
                let sq = if (TAU * f * t).sin() > 0.0 { 1.0 } else { -1.0 };
                lp.run(sq) * 0.5 * (1.0 - (t / 1.3)).max(0.0)
            });
            b.echo(0.21, 0.3);
            b.finish()
        }
        Sfx::Place => {
            // Steel plate set down and bolted: noise hit plus inharmonic ring.
            let mut b = Buf::new(0.5);
            b.add(0.0, |t| {
                rng.next() * (-t * 50.0).exp() * 0.6
                    + [220.0, 347.0, 521.0, 790.0]
                        .iter()
                        .map(|f| (TAU * f * t).sin())
                        .sum::<f32>()
                        * (-t * 12.0).exp()
                        * 0.25
            });
            for i in 0..2 {
                b.add(0.18 + i as f32 * 0.09, |t| {
                    (TAU * 2400.0 * t).sin() * (-t * 120.0).exp() * 0.3
                });
            }
            b.finish()
        }
        Sfx::Upgrade => {
            let mut b = Buf::new(0.7);
            for i in 0..5 {
                b.add(i as f32 * 0.06, |t| {
                    (TAU * (1800.0 + i as f32 * 200.0) * t).sin() * (-t * 90.0).exp() * 0.4
                });
            }
            b.add(0.32, |t| {
                ((TAU * 880.0 * t).sin() + 0.5 * (TAU * 1320.0 * t).sin()) * (-t * 7.0).exp() * 0.5
            });
            b.finish()
        }
        Sfx::Sell => {
            let mut b = Buf::new(0.5);
            for (i, f) in [1760.0, 1320.0, 990.0].iter().enumerate() {
                b.add(i as f32 * 0.07, |t| {
                    (TAU * f * t).sin() * (-t * 18.0).exp() * 0.45
                });
            }
            b.finish()
        }
        Sfx::BarrierHit => {
            // Sledgehammer on crates and steel: thwack and clang.
            let mut b = Buf::new(0.4);
            let mut lp = Lp::new(900.0);
            b.add(0.0, |t| {
                lp.run(rng.next()) * (-t * 30.0).exp() * 2.0
                    + [800.0, 1230.0, 1710.0]
                        .iter()
                        .map(|f| (TAU * f * (1.0 + v) * t).sin())
                        .sum::<f32>()
                        * (-t * 16.0).exp()
                        * 0.2
            });
            b.finish()
        }
        Sfx::BarrierBroken => {
            let mut b = Buf::new(0.9);
            let mut lp = Lp::new(1200.0);
            b.add(0.0, |t| lp.run(rng.next()) * (-t * 6.0).exp() * 2.0);
            // Splintering cracks.
            for i in 0..6 {
                let at = 0.02 + i as f32 * 0.07 + rng.next().abs() * 0.03;
                let mut r = Lcg(i * 13 + 5);
                b.add(at, |t| r.next() * (-t * 120.0).exp() * 0.9);
            }
            b.finish()
        }
        Sfx::TowerDestroyed => {
            let mut b = Buf::new(1.3);
            let mut lp = Lp::new(500.0);
            b.add(0.0, |t| {
                lp.run(rng.next()) * (-t * 3.5).exp() * 3.0
                    + sweep(t, 100.0, 35.0, 6.0).sin() * (-t * 4.0).exp()
            });
            b.add(0.05, |t| {
                [430.0, 655.0, 1010.0]
                    .iter()
                    .map(|f| (TAU * f * t).sin())
                    .sum::<f32>()
                    * (-t * 5.0).exp()
                    * 0.25
            });
            b.finish()
        }
        Sfx::ItDown => {
            // Servers powering down: a sagging hum and dying crackle.
            let mut b = Buf::new(1.8);
            b.add(0.0, |t| {
                let f = 40.0 + 360.0 * (-t * 1.8).exp();
                let p = TAU * (40.0 * t + 360.0 / 1.8 * (1.0 - (-t * 1.8).exp()));
                let saw = (p / TAU).fract() * 2.0 - 1.0;
                let crackle = if rng.chance(0.004) {
                    rng.next() * 2.0
                } else {
                    0.0
                };
                (saw * 0.5 + crackle) * (1.0 - t / 1.8).max(0.0) * (f / 400.0 + 0.3)
            });
            b.finish()
        }
        Sfx::ItUp => {
            let mut b = Buf::new(0.9);
            b.add(0.0, |t| {
                let p = sweep(t, 100.0, 800.0, 3.0);
                ((p / TAU).fract() * 2.0 - 1.0) * 0.35 * (t * 10.0).min(1.0) * (-t * 2.0).exp()
            });
            for (i, f) in [880.0, 1320.0].iter().enumerate() {
                b.add(0.4 + i as f32 * 0.12, |t| {
                    (TAU * f * t).sin() * (-t * 10.0).exp() * 0.4
                });
            }
            b.finish()
        }
        Sfx::PowerCut => {
            // Big relay slam, then the mains hum winding down.
            let mut b = Buf::new(1.6);
            let mut lp = Lp::new(300.0);
            b.add(0.0, |t| lp.run(rng.next()) * (-t * 25.0).exp() * 3.0);
            b.add(0.02, |t| {
                let f = 50.0 + 70.0 * (-t * 1.5).exp();
                ((TAU * f * t).sin() + 0.5 * (TAU * 2.0 * f * t).sin()) * (-t * 1.8).exp() * 0.5
            });
            b.finish()
        }
        Sfx::Horn => {
            // Low brassy horn: detuned saws through a swelling filter.
            let mut b = Buf::new(1.8);
            let mut lp = Lp::new(600.0);
            b.add(0.0, |t| {
                let mut x = 0.0;
                for (f, g) in [(73.4, 1.0), (110.0, 0.7), (73.9, 0.6)] {
                    x += ((f * t).fract() * 2.0 - 1.0) * g;
                }
                lp.1 = (TAU * (300.0 + 900.0 * (t * 2.0).min(1.0)) / SR as f32).min(1.0);
                lp.run(x) * (t * 4.0).min(1.0) * (1.0 - (t - 1.2).max(0.0) / 0.6).max(0.0) * 0.5
            });
            b.echo(0.25, 0.3);
            b.finish()
        }
        Sfx::Cleared => {
            let mut b = Buf::new(1.4);
            for (i, f) in [587.3, 698.5, 880.0, 1174.7].iter().enumerate() {
                b.add(i as f32 * 0.11, |t| {
                    ((TAU * f * t).sin() + 0.3 * (TAU * f * 2.76 * t).sin() * (-t * 4.0).exp())
                        * (-t * 3.5).exp()
                        * 0.4
                });
            }
            b.finish()
        }
        Sfx::Equip => {
            // Rifle being racked.
            let mut b = Buf::new(0.3);
            for (i, f) in [2600.0, 1900.0].iter().enumerate() {
                let mut r = Lcg(i as u32 + 3);
                b.add(i as f32 * 0.11, |t| {
                    (r.next() * 0.6 + (TAU * f * t).sin() * 0.5) * (-t * 80.0).exp()
                });
            }
            b.finish()
        }
        Sfx::Click => {
            let mut b = Buf::new(0.06);
            b.add(0.0, |t| (TAU * 1500.0 * t).sin() * (-t * 120.0).exp() * 0.6);
            b.finish()
        }
    }
}

pub struct SoundBank {
    sounds: Vec<(Sfx, Vec<Sound>)>,
    last: Vec<f64>,
    played_this_frame: u32,
    frame_time: f64,
    pub muted: bool,
}

impl SoundBank {
    pub async fn load() -> Option<Self> {
        let mut sounds = Vec::new();
        for kind in Sfx::ALL {
            let mut vs = Vec::new();
            for seed in 1..=kind.variants() {
                vs.push(
                    load_sound_from_bytes(&wav(&synth(kind, seed), 0.9))
                        .await
                        .ok()?,
                );
            }
            sounds.push((kind, vs));
        }
        Some(Self {
            last: vec![-1.0; sounds.len()],
            sounds,
            played_this_frame: 0,
            frame_time: 0.0,
            muted: false,
        })
    }

    /// Play `kind` at `volume` (0..1, scaled by the sound's own gain), unless it was just played
    /// or this frame is already busy.
    pub fn play(&mut self, kind: Sfx, volume: f32) {
        let now = macroquad::time::get_time();
        if now != self.frame_time {
            self.frame_time = now;
            self.played_this_frame = 0;
        }
        if self.muted || volume <= 0.01 || self.played_this_frame >= 6 {
            return;
        }
        let Some(i) = self.sounds.iter().position(|(k, _)| *k == kind) else {
            return;
        };
        let (spacing, gain) = kind.spacing_and_gain();
        if now - self.last[i] < spacing {
            return;
        }
        self.last[i] = now;
        self.played_this_frame += 1;
        let vs = &self.sounds[i].1;
        let s = &vs[macroquad::rand::gen_range(0, vs.len())];
        play_sound(
            s,
            PlaySoundParams {
                looped: false,
                volume: (volume * gain).min(1.0),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_effect_is_audible_finite_and_short() {
        for kind in Sfx::ALL {
            for seed in 1..=kind.variants() {
                let b = synth(kind, seed);
                assert!(
                    !b.is_empty() && b.len() <= 2 * SR as usize,
                    "{kind:?} length"
                );
                assert!(b.iter().all(|x| x.is_finite()), "{kind:?} not finite");
                let peak = b.iter().fold(0.0f32, |m, x| m.max(x.abs()));
                assert!(peak > 0.05, "{kind:?} is silent");
                assert!(b.last().unwrap().abs() < 1e-3, "{kind:?} clicks at the end");
            }
            if let Some(dir) = std::env::var_os("SILO_MUSIC_OUT") {
                let path = std::path::Path::new(&dir).join(format!("sfx_{kind:?}.wav"));
                std::fs::write(path, wav(&synth(kind, 1), 0.9)).unwrap();
            }
        }
    }
}
