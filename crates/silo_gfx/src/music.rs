//! Procedural soundtrack. Two loops are synthesised at startup (no audio assets): a brooding
//! "shift" track for the building phase and an "uprising" track for waves. They share tempo,
//! key and length, play together in sync, and are crossfaded with the game phase.

use crate::synth::{Lcg, SR, freq, wav};
use macroquad::audio::{
    PlaySoundParams, Sound, load_sound_from_bytes, play_sound, set_sound_volume,
};
use std::f32::consts::TAU;

const BPM: f32 = 84.0;
const BEAT: f32 = 60.0 / BPM;
const BARS: usize = 16;
/// Loop length in seconds (16 bars of 4/4).
const LEN: f32 = BARS as f32 * 4.0 * BEAT;

/// Two-bar chords, D minor: i - VI - III - VII - i - VI - iv - V.
/// (pad voicing as MIDI notes, bass root)
const CHORDS: [([u8; 3], u8); 8] = [
    ([50, 53, 57], 38), // Dm
    ([50, 53, 58], 34), // Bb
    ([53, 57, 60], 41), // F
    ([52, 55, 60], 36), // C
    ([50, 53, 57], 38), // Dm
    ([50, 53, 58], 34), // Bb
    ([50, 55, 58], 43), // Gm
    ([49, 52, 57], 45), // A
];

/// D minor pentatonic for the bell melody (octave 5).
const SCALE: [u8; 5] = [74, 77, 79, 81, 84];

/// A mono buffer that wraps around, so note tails spill into the start and the loop is seamless.
struct Track {
    buf: Vec<f32>,
}

impl Track {
    fn new() -> Self {
        Self {
            buf: vec![0.0; (LEN * SR as f32) as usize],
        }
    }

    /// Render `dur` seconds of `f(t)` starting at `start` (seconds), wrapping around.
    fn add(&mut self, start: f32, dur: f32, mut f: impl FnMut(f32) -> f32) {
        let n = self.buf.len();
        let s0 = (start * SR as f32) as usize;
        let count = (dur * SR as f32) as usize;
        for i in 0..count {
            let t = i as f32 / SR as f32;
            self.buf[(s0 + i) % n] += f(t);
        }
    }

    /// Feedback echo (wraps, so it stays seamless).
    fn echo(&mut self, delay: f32, feedback: f32, mix: f32) {
        let n = self.buf.len();
        let d = (delay * SR as f32) as usize;
        let mut wet = vec![0.0f32; n];
        // Two passes around the loop so the feedback settles across the seam.
        for _ in 0..2 {
            for i in 0..n {
                let j = (i + n - d) % n;
                wet[i] = self.buf[j] + wet[j] * feedback;
            }
        }
        for (x, w) in self.buf.iter_mut().zip(wet) {
            *x += w * mix;
        }
    }

    fn mix(&mut self, other: &Track, gain: f32) {
        for (a, b) in self.buf.iter_mut().zip(&other.buf) {
            *a += b * gain;
        }
    }

    /// 16-bit mono WAV, normalised to `peak`.
    fn wav(&self, peak: f32) -> Vec<u8> {
        wav(&self.buf, peak)
    }
}

/// Attack/release envelope over a note of length `len`.
fn env(t: f32, len: f32, attack: f32, release: f32) -> f32 {
    if t < attack {
        t / attack
    } else if t < len {
        1.0
    } else {
        (1.0 - (t - len) / release).max(0.0)
    }
}

/// Soft organ-ish pad tone with a slow chorus.
fn pad_tone(f: f32, t: f32) -> f32 {
    let v = 1.0 + 0.003 * (TAU * 0.3 * t).sin();
    let p = TAU * f * v * t;
    let q = TAU * f * 1.004 * t;
    (p.sin() + 0.45 * (2.0 * p).sin() + 0.2 * (3.0 * p).sin()) * 0.6
        + (q.sin() + 0.4 * (2.0 * q).sin()) * 0.4
}

/// The shared bed: drone, pad and wind. Used by both tracks.
fn bed() -> Track {
    let mut tr = Track::new();
    let chord_len = 8.0 * BEAT;
    // Drone on D and A, slowly breathing.
    tr.add(0.0, LEN, |t| {
        let breathe = 0.75 + 0.25 * (TAU * t / (LEN / 4.0)).sin();
        ((TAU * freq(38) * t).sin() * 0.5 + (TAU * freq(45) * t).sin() * 0.25) * 0.09 * breathe
    });
    // Pads.
    for (i, (notes, _)) in CHORDS.iter().enumerate() {
        let start = i as f32 * chord_len;
        for &n in notes {
            let f = freq(n);
            tr.add(start, chord_len + 2.0, |t| {
                pad_tone(f, t) * env(t, chord_len, 1.6, 2.0) * 0.055
            });
        }
    }
    // Wind through the stairwell: heavily filtered noise, swelling.
    let mut rng = Lcg(7);
    let mut lp = 0.0;
    let mut lp2 = 0.0;
    tr.add(0.0, LEN, |t| {
        lp += (rng.next() - lp) * 0.02;
        lp2 += (lp - lp2) * 0.05;
        let swell = 0.5 + 0.5 * (TAU * t / (LEN / 3.0) + 1.0).sin();
        lp2 * 0.5 * swell
    });
    tr
}

/// Shift track: the bed plus a sparse, echoing bell melody.
fn shift_track(bed: &Track) -> Track {
    let mut tr = Track::new();
    tr.mix(bed, 1.0);
    let mut bells = Track::new();
    let mut rng = Lcg(42);
    let mut idx = 2usize;
    for beat in 0..(BARS * 4) {
        // Mostly on downbeats; rests in the last bar of each phrase.
        let bar = beat / 4;
        let p = if beat % 4 == 0 { 0.6 } else { 0.22 };
        if bar % 4 == 3 && beat % 4 > 1 || !rng.chance(p) {
            continue;
        }
        // A wandering line: step up or down the pentatonic scale.
        idx = (idx as i32 + if rng.chance(0.5) { 1 } else { -1 }).clamp(0, SCALE.len() as i32 - 1)
            as usize;
        let f = freq(SCALE[idx]);
        let start = beat as f32 * BEAT + if rng.chance(0.3) { BEAT / 2.0 } else { 0.0 };
        bells.add(start, 3.0, |t| {
            let a = (TAU * f * t).sin() * (-t * 2.2).exp()
                + 0.35 * (TAU * f * 2.76 * t).sin() * (-t * 4.5).exp()
                + 0.12 * (TAU * f * 5.4 * t).sin() * (-t * 9.0).exp();
            a * (t * 400.0).min(1.0) * 0.12
        });
    }
    bells.echo(BEAT * 0.75, 0.38, 0.6);
    tr.mix(&bells, 1.0);
    tr
}

/// Uprising track: the bed under a driving bass, drums and a sixteenth-note arpeggio.
fn uprising_track(bed: &Track) -> Track {
    let mut tr = Track::new();
    tr.mix(bed, 0.8);
    let chord_beats = 8;
    let mut rng = Lcg(99);
    for beat in 0..(BARS * 4) {
        let (notes, root) = CHORDS[beat / chord_beats];
        let t0 = beat as f32 * BEAT;
        // Bass: eighth notes, root with an octave kick on the off-beat.
        for half in 0..2 {
            let f = freq(root) * if half == 1 && beat % 2 == 1 { 2.0 } else { 1.0 };
            tr.add(t0 + half as f32 * BEAT / 2.0, BEAT * 0.6, |t| {
                let mut v = 0.0;
                for h in 1..6 {
                    v += (TAU * f * h as f32 * t).sin() / h as f32;
                }
                v * (-t * 6.0).exp() * (t * 300.0).min(1.0) * 0.11
            });
        }
        // Kick on 1 and 3 (and a pickup on the last off-beat of every second bar).
        if beat % 2 == 0 || (beat % 8 == 7 && rng.chance(0.7)) {
            let start = if beat % 2 == 0 { t0 } else { t0 + BEAT / 2.0 };
            // Pitch sweeps 135 Hz -> 45 Hz; the phase is the integral of that sweep.
            tr.add(start, 0.4, |t| {
                let phase = TAU * (45.0 * t + 90.0 / 25.0 * (1.0 - (-t * 25.0).exp()));
                phase.sin() * (-t * 9.0).exp() * 0.5
            });
        }
        // Snare on 2 and 4.
        if beat % 2 == 1 {
            let mut n = Lcg(beat as u32 * 31 + 5);
            tr.add(t0, 0.3, |t| {
                (n.next() * 0.7 + (TAU * 185.0 * t).sin() * 0.5) * (-t * 18.0).exp() * 0.22
            });
        }
        // Closed hats on eighths.
        for half in 0..2 {
            let mut n = Lcg(beat as u32 * 7 + half);
            let mut prev = 0.0;
            tr.add(t0 + half as f32 * BEAT / 2.0, 0.08, |t| {
                let x = n.next();
                let hp = x - prev;
                prev = x;
                hp * (-t * 70.0).exp() * if half == 1 { 0.07 } else { 0.045 }
            });
        }
        // Arpeggio from the fifth bar: chord tones an octave up, sixteenths.
        if beat >= 16 {
            for s in 0..4 {
                let note = notes[(beat * 4 + s) % 3] + 12;
                let f = freq(note);
                tr.add(t0 + s as f32 * BEAT / 4.0, 0.3, |t| {
                    let p = TAU * f * t;
                    (p.sin() + 0.3 * (2.0 * p).sin())
                        * (-t * 14.0).exp()
                        * (t * 500.0).min(1.0)
                        * 0.05
                });
            }
        }
    }
    tr
}

pub struct Music {
    shift: Sound,
    uprising: Sound,
    /// 0 = shift track only, 1 = uprising track only.
    mix: f32,
    pub muted: bool,
    level: f32,
    /// Ducking gain (1 = full) that dips the music under a voice-over such as the chant.
    duck: f32,
}

impl Music {
    /// Synthesise both loops and start them, in sync, at zero volume.
    pub async fn load() -> Option<Self> {
        let bed = bed();
        let shift = load_sound_from_bytes(&shift_track(&bed).wav(0.85))
            .await
            .ok()?;
        let uprising = load_sound_from_bytes(&uprising_track(&bed).wav(0.9))
            .await
            .ok()?;
        for s in [&shift, &uprising] {
            play_sound(
                s,
                PlaySoundParams {
                    looped: true,
                    volume: 0.0,
                },
            );
        }
        Some(Self {
            shift,
            uprising,
            mix: 0.0,
            muted: false,
            level: 0.0,
            duck: 1.0,
        })
    }

    /// Steer the crossfade (`intensity` 0..1), overall level (`volume` 0..1) and ducking
    /// (`duck`: target gain while something should sit on top of the music, 1 = none).
    /// Ducking follows like a compressor's sidechain: quick to dip, slow to recover.
    pub fn update(&mut self, dt: f32, intensity: f32, volume: f32, duck: f32) {
        self.mix += (intensity - self.mix) * (1.0 - (-dt * 0.8).exp());
        let target = if self.muted { 0.0 } else { volume };
        self.level += (target - self.level) * (1.0 - (-dt * 2.0).exp());
        let rate = if duck < self.duck { 14.0 } else { 1.6 };
        self.duck += (duck - self.duck) * (1.0 - (-dt * rate).exp());
        let m = self.mix.clamp(0.0, 1.0);
        let g = self.level * self.duck;
        set_sound_volume(&self.shift, g * (1.0 - m) * 0.8);
        set_sound_volume(&self.uprising, g * m * 0.95);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_are_seamless_and_sane() {
        let bed = bed();
        for (name, tr) in [
            ("shift", shift_track(&bed)),
            ("uprising", uprising_track(&bed)),
        ] {
            assert_eq!(tr.buf.len(), (LEN * SR as f32) as usize);
            assert!(tr.buf.iter().all(|x| x.is_finite()));
            let rms = (tr.buf.iter().map(|x| x * x).sum::<f32>() / tr.buf.len() as f32).sqrt();
            assert!(rms > 0.01, "track is silent (rms {rms})");
            // The seam: last and first samples are close (no click when looping).
            let (a, b) = (tr.buf[tr.buf.len() - 1], tr.buf[0]);
            assert!((a - b).abs() < 0.15, "click at loop seam: {a} vs {b}");
            let wav = tr.wav(0.9);
            assert_eq!(&wav[0..4], b"RIFF");
            // SILO_MUSIC_OUT=dir: also write the loops out for listening.
            if let Some(dir) = std::env::var_os("SILO_MUSIC_OUT") {
                std::fs::write(std::path::Path::new(&dir).join(format!("{name}.wav")), &wav)
                    .unwrap();
            }
            assert_eq!(wav.len(), 44 + tr.buf.len() * 2);
        }
    }
}
