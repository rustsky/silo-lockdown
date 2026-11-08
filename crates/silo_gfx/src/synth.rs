//! Tiny synthesis toolkit shared by the music and the sound effects: deterministic noise,
//! MIDI pitch, and 16-bit mono WAV encoding (so the audio backend can load it like a file).

/// Sample rate of everything we synthesise. quad-snd resamples to the device rate.
pub const SR: u32 = 22_050;

pub fn freq(midi: u8) -> f32 {
    440.0 * 2f32.powf((midi as f32 - 69.0) / 12.0)
}

/// Deterministic white noise in -1..1.
pub struct Lcg(pub u32);

impl Lcg {
    pub fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
    }

    pub fn chance(&mut self, p: f32) -> bool {
        (self.next() + 1.0) / 2.0 < p
    }
}

/// Encode samples as a 16-bit mono WAV, normalised so the loudest sample hits `peak`.
pub fn wav(buf: &[f32], peak: f32) -> Vec<u8> {
    wav_at(buf, peak, SR)
}

/// Same, at sample rate `rate`.
pub fn wav_at(buf: &[f32], peak: f32, rate: u32) -> Vec<u8> {
    let max = buf.iter().fold(0.0f32, |m, x| m.max(x.abs())).max(1e-6);
    let k = peak / max;
    let data_len = (buf.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for x in buf {
        let v = (x * k).clamp(-1.0, 1.0);
        out.extend_from_slice(&((v * 32767.0) as i16).to_le_bytes());
    }
    out
}
