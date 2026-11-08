#!/usr/bin/env python3
"""Make the crowd chants the uprising shouts at the start of each wave, one per slogan.

Built like a real chant: every voice says each word on its own ("We", "want", "the", "truth!")
and every word is placed on a shared beat -- WE . WANT . THE . TRUUUTH -- with only small human
timing slips, so the crowd stays in rhythm. Several macOS text-to-speech voices (the `say`
command), each used twice at a slightly different pitch, make up the crowd. No echo.
Each slogan gets its own mix of voices. Writes crates/silo_gfx/assets/chant_N.mp3 (mono, 40 kbps;
the game decodes them at startup), N matching the order of SLOGANS in
crates/silo_gfx/src/uprising.rs.

Uses every installed English "(Enhanced)" or "(Premium)" voice (free downloads: System Settings >
Accessibility > Read & Speak (Spoken Content on older macOS) > System Voice > Manage Voices);
without any, falls back to the basic built-in voices, which sound robotic.

Needs macOS (`say`), numpy and the `lame` MP3 encoder (brew install lame).
Run from the repo root:  python3 scripts/make-chant.py
"""
import os
import subprocess
import sys
import tempfile
import wave

import numpy as np

LINE = "We want the truth!"
SR = 16000
OUT_DIR = os.path.join("crates", "silo_gfx", "assets")
# Fallback (basic voices): (voice, words per minute, gain)
BASIC_VOICES = [
    ("Daniel", 185, 1.0),
    ("Fred", 190, 0.8),
    ("Ralph", 180, 1.0),
    ("Rocko (English (US))", 185, 0.9),
    ("Rocko (English (UK))", 180, 0.9),
    ("Reed (English (US))", 190, 0.9),
    ("Eddy (English (UK))", 185, 0.8),
    ("Grandpa (English (US))", 175, 0.7),
    ("Aman", 185, 0.8),
    ("Rishi", 190, 0.7),
    ("Samantha", 190, 0.6),
    ("Karen", 185, 0.6),
    ("Moira", 185, 0.6),
    ("Tessa", 190, 0.55),
]


def quality_voices():
    """Installed English Enhanced/Premium voices, one per person (Premium preferred)."""
    out = subprocess.run(["say", "-v", "?"], capture_output=True, text=True).stdout
    best = {}
    for line in out.splitlines():
        name = line.partition("#")[0].strip()
        parts = name.rsplit(None, 1)
        if len(parts) != 2 or not parts[1].startswith("en_"):
            continue
        voice = parts[0].strip()
        for tier, rank in (("(Premium)", 2), ("(Enhanced)", 1)):
            if tier in voice:
                person = voice.replace(tier, "").strip()
                if rank > best.get(person, ("", 0))[1]:
                    best[person] = (voice, rank)
    return [(v, 210, 1.0) for v, _ in sorted(best.values())]


def say(voice, rate, text, path):
    # macOS's speech engine occasionally stalls on rapid back-to-back calls: give each word a
    # timeout and one retry before giving up on the voice.
    cmd = ["say", "-v", voice, "-r", str(rate), "--file-format=WAVE",
           f"--data-format=LEI16@{SR}", "-o", path, text]
    for attempt in range(2):
        try:
            subprocess.run(cmd, check=True, timeout=20)
            break
        except subprocess.TimeoutExpired:
            if attempt == 1:
                raise subprocess.CalledProcessError(1, cmd)
    with wave.open(path) as w:
        x = np.frombuffer(w.readframes(w.getnframes()), dtype=np.int16).astype(np.float32) / 32768
    # Trim silence at both ends.
    loud = np.nonzero(np.abs(x) > 0.02)[0]
    if len(loud) == 0:
        return x[:0]
    return x[loud[0] : loud[-1] + 1]


def resample(x, factor):
    """Speed up/down (and pitch with it) by `factor` with linear interpolation."""
    n = int(len(x) / factor)
    return np.interp(np.arange(n) * factor, np.arange(len(x)), x).astype(np.float32)


# Slogans in the game's order (uprising.rs SLOGANS): (word as spoken, syllables). Syllables 0
# marks a quick little word ("the", "to") that gets a short slot.
SLOGANS = [
    [("We!", 1), ("want", 1), ("the", 0), ("truth!", 1)],
    [("What", 1), ("is", 0), ("I.T.", 2), ("hiding?", 2)],
    [("The", 0), ("deep", 1), ("rises!", 2)],
    [("No", 1), ("more", 1), ("lies!", 1)],
    [("Who", 1), ("built", 1), ("the", 0), ("silo?", 2)],
    [("Power", 2), ("to", 0), ("the", 0), ("deep!", 1)],
    [("Open", 2), ("the", 0), ("records!", 2)],
    [("We", 1), ("keep", 1), ("you", 1), ("alive!", 2)],
    [("Remember", 3), ("the", 0), ("rebellion!", 3)],
    [("Up", 1), ("the", 0), ("stairs!", 1)],
]


def beats(words):
    """(word, beat time, longest it may ring on) for a chant: one beat per word, longer words
    get more time, the last word is held."""
    out, t = [], 0.0
    for i, (w, syl) in enumerate(words):
        slot = 0.2 if syl == 0 else 0.27 + 0.13 * (syl - 1)
        last = i == len(words) - 1
        ring = 0.35 + 0.15 * max(syl, 1) if last else slot + 0.02
        out.append((w, t, ring))
        t += slot
    return out


def chant(words, voices, rng, tmp, tag):
    plan = beats(words)
    length = plan[-1][1] + plan[-1][2] + 0.3
    mix = np.zeros(int(length * SR), np.float32)
    people = 0
    for i, (voice, rate, gain) in enumerate(voices):
        try:
            clips = [
                say(voice, rate, w, os.path.join(tmp, f"{tag}_{i}_{j}.wav"))
                for j, (w, _, _) in enumerate(plan)
            ]
        except subprocess.CalledProcessError:
            print(f"  skipping voice {voice} (failed or stalled)", file=sys.stderr, flush=True)
            continue
        # Each voice joins twice: two people, a little higher and lower.
        for pitch in (rng.uniform(0.95, 1.0), rng.uniform(1.01, 1.07)):
            people += 1
            late = rng.uniform(0.0, 0.025)  # some people are always a touch behind
            g = gain * rng.uniform(0.6, 1.0)
            for (_, beat, ring), w in zip(plan, clips):
                if len(w) == 0:
                    continue
                w = resample(w, pitch)
                # Same loudness for every word, cut off when the next word comes.
                w = w / (np.sqrt(np.mean(w * w)) + 1e-6) * 0.12
                n = min(len(w), int(ring * SR))
                w = w[:n].copy()
                fade = min(n, int(0.04 * SR))
                w[-fade:] *= np.linspace(1.0, 0.0, fade)
                t = beat + late + rng.normal(0.0, 0.012)
                at = max(0, int(t * SR))
                end = min(len(mix), at + len(w))
                mix[at:end] += w[: end - at] * g
    if people == 0:
        sys.exit("no voices available")
    mix /= np.max(np.abs(mix))
    # A little push, like shouting into a big space -- but no echo.
    mix = np.tanh(mix * 1.6) / np.tanh(1.6)
    fade = int(0.15 * SR)
    mix[-fade:] *= np.linspace(1.0, 0.0, fade)
    return mix / np.max(np.abs(mix)) * 0.9, people


def main():
    voices = quality_voices()
    if voices:
        print(f"{len(voices)} voices: {', '.join(v for v, _, _ in voices)}")
    else:
        print("no enhanced/premium English voices installed; using the basic (robotic) ones")
        voices = BASIC_VOICES
    os.makedirs(OUT_DIR, exist_ok=True)
    with tempfile.TemporaryDirectory() as tmp:
        for n, words in enumerate(SLOGANS):
            rng = np.random.default_rng(100 + n)
            # A different crowd for each slogan: its own pick of voices.
            k = min(len(voices), max(6, len(voices) * 2 // 3))
            crowd = [voices[i] for i in sorted(rng.choice(len(voices), k, replace=False))]
            print(f"slogan {n}: {' '.join(w for w, _ in words)} ...", flush=True)
            mix, people = chant(words, crowd, rng, tmp, f"s{n}")
            tmp_wav = os.path.join(tmp, f"chant_{n}.wav")
            with wave.open(tmp_wav, "wb") as w:
                w.setnchannels(1)
                w.setsampwidth(2)
                w.setframerate(SR)
                w.writeframes((mix * 32767).astype(np.int16).tobytes())
            path = os.path.join(OUT_DIR, f"chant_{n}.mp3")
            subprocess.run(["lame", "--quiet", "-m", "m", "-b", "40", "-q", "2", tmp_wav, path], check=True)
            text = " ".join(w for w, _ in words)
            print(f"  wrote {path}: {text}  ({len(mix) / SR:.2f}s, {people} people)", flush=True)


if __name__ == "__main__":
    main()
