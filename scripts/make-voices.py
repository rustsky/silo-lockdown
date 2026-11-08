#!/usr/bin/env python3
"""Make the spoken lines of the defeat sequence (Silo 1's Safeguard protocol).

- Three announcements from Silo 1 over the PA: one deep voice, band-limited and driven like an
  old loudspeaker, each after a two-tone chime.
- The crowd cheering "We're free!" as they come out of the airlock, built like the chants.
- Victory: Silo 1 calls the Head of IT on a secure phone line to congratulate them (a different,
  calm voice; telephone band, a line-open beep, faint static).

Writes crates/silo_gfx/assets/voice_N.mp3 (mono, 40 kbps), N in the order of VOICE_* in
crates/silo_gfx/src/sfx.rs. Needs what scripts/make-chant.py needs (macOS `say` with an
Enhanced/Premium English voice installed, numpy, lame). Run from the repo root:
    python3 scripts/make-voices.py           # everything
    python3 scripts/make-voices.py win       # only the victory call
"""
import sys
import importlib.util
import os
import subprocess
import tempfile
import wave

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("chant", os.path.join(HERE, "make-chant.py"))
chant = importlib.util.module_from_spec(spec)
spec.loader.exec_module(chant)
SR = chant.SR

ANNOUNCEMENTS = [
    "Attention, Silo Twelve. The airlock has been breached. Silo One has initiated the Safeguard protocol.",
    "Safeguard in progress. Remain where you are.",
    "Safeguard complete. You failed to prevent the Safeguard. Game over.",
]
# Silo 1's call when the silo holds (voice_4, voice_5).
WIN_LINES = [
    "This is Silo One, calling Silo Twelve. Head of I.T., the uprising in your silo has been put down.",
    "Your silo held, and the Pact is preserved. Well done. The Safeguard will not be needed. Silo One out.",
]
CALLERS = ["Jamie (Premium)", "Oliver (Enhanced)", "Daniel (Enhanced)", "Daniel"]

# Deep, calm voices for Silo 1, best first.
ANNOUNCERS = ["Daniel (Enhanced)", "Oliver (Enhanced)", "Lee (Premium)", "Jamie (Premium)", "Daniel"]


def biquad(x, kind, f0, q=0.707):
    """RBJ cookbook high/low-pass."""
    w = 2 * np.pi * f0 / SR
    alpha = np.sin(w) / (2 * q)
    cw = np.cos(w)
    if kind == "lp":
        b = [(1 - cw) / 2, 1 - cw, (1 - cw) / 2]
    else:
        b = [(1 + cw) / 2, -(1 + cw), (1 + cw) / 2]
    a = [1 + alpha, -2 * cw, 1 - alpha]
    b = [v / a[0] for v in b]
    a = [1.0, a[1] / a[0], a[2] / a[0]]
    y = np.zeros_like(x)
    x1 = x2 = y1 = y2 = 0.0
    for i, v in enumerate(x):
        out = b[0] * v + b[1] * x1 + b[2] * x2 - a[1] * y1 - a[2] * y2
        x2, x1 = x1, v
        y2, y1 = y1, out
        y[i] = out
    return y


def chime():
    """Two-tone PA chime: ding... dong."""
    t = np.arange(int(0.9 * SR)) / SR
    out = np.zeros_like(t, dtype=np.float32)
    for start, f in [(0.0, 659.3), (0.32, 523.3)]:
        u = np.clip(t - start, 0, None)
        on = t >= start
        tone = (np.sin(2 * np.pi * f * u) + 0.3 * np.sin(2 * np.pi * f * 2.0 * u)) * np.exp(-u * 3.2)
        out += (tone * on).astype(np.float32) * 0.5
    return out


def pa(speech):
    """Old loudspeaker: telephone band, a little drive, a short reflection off the silo walls."""
    x = biquad(speech, "hp", 380.0)
    x = biquad(x, "lp", 3300.0)
    x = x / (np.max(np.abs(x)) + 1e-6)
    x = np.tanh(x * 2.4) / np.tanh(2.4)
    d = int(0.085 * SR)
    y = x.copy()
    y[d:] += x[:-d] * 0.15
    return y


def phone(speech, first):
    """A secure phone line: telephone band, light drive, faint hiss; a beep when the line opens."""
    x = biquad(speech, "hp", 320.0)
    x = biquad(x, "lp", 3000.0)
    x = x / (np.max(np.abs(x)) + 1e-6)
    x = np.tanh(x * 1.6) / np.tanh(1.6)
    rng = np.random.default_rng(5)
    hiss = biquad(rng.normal(0, 1, len(x)).astype(np.float32), "hp", 2000.0) * 0.012
    x = x + hiss
    if first:
        t = np.arange(int(0.12 * SR)) / SR
        beep = (np.sin(2 * np.pi * 1000 * t) * 0.35).astype(np.float32)
        x = np.concatenate([beep, np.zeros(int(0.25 * SR), np.float32), x])
    return x


def pick(installed, names):
    return next(v for v in names if v + " " in installed or v + "\t" in installed)


def write_mp3(samples, path, tmp):
    samples = samples / (np.max(np.abs(samples)) + 1e-6) * 0.9
    fade = int(0.08 * SR)
    samples[-fade:] *= np.linspace(1.0, 0.0, fade)
    tmp_wav = os.path.join(tmp, os.path.basename(path) + ".wav")
    with wave.open(tmp_wav, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes((samples * 32767).astype(np.int16).tobytes())
    subprocess.run(["lame", "--quiet", "-m", "m", "-b", "40", "-q", "2", tmp_wav, path], check=True)
    print(f"  wrote {path} ({len(samples) / SR:.2f}s)", flush=True)


def win_call(installed, tmp):
    caller = pick(installed, CALLERS)
    print(f"Silo 1 caller: {caller}", flush=True)
    for k, line in enumerate(WIN_LINES):
        n = len(ANNOUNCEMENTS) + 1 + k
        print(f"call {k}: {line}", flush=True)
        path = os.path.join(tmp, f"w{k}.wav")
        subprocess.run(
            ["say", "-v", caller, "-r", "158", "--file-format=WAVE",
             f"--data-format=LEI16@{SR}", "-o", path, line],
            check=True, timeout=60,
        )
        with wave.open(path) as w:
            speech = np.frombuffer(w.readframes(w.getnframes()), dtype=np.int16).astype(np.float32) / 32768
        # Pitched well down and slowed: cold, not quite human.
        speech = chant.resample(speech, 0.84)
        clip = np.concatenate([phone(speech, k == 0), np.zeros(int(0.3 * SR), np.float32)])
        write_mp3(clip, os.path.join(chant.OUT_DIR, f"voice_{n}.mp3"), tmp)


def main():
    installed = subprocess.run(["say", "-v", "?"], capture_output=True, text=True).stdout
    if sys.argv[1:] == ["win"]:
        with tempfile.TemporaryDirectory() as tmp:
            win_call(installed, tmp)
        return
    announcer = pick(installed, ANNOUNCERS)
    print(f"announcer: {announcer}", flush=True)
    with tempfile.TemporaryDirectory() as tmp:
        for n, line in enumerate(ANNOUNCEMENTS):
            print(f"announcement {n}: {line}", flush=True)
            path = os.path.join(tmp, f"a{n}.wav")
            subprocess.run(
                ["say", "-v", announcer, "-r", "150", "--file-format=WAVE",
                 f"--data-format=LEI16@{SR}", "-o", path, line],
                check=True, timeout=60,
            )
            with wave.open(path) as w:
                speech = np.frombuffer(w.readframes(w.getnframes()), dtype=np.int16).astype(np.float32) / 32768
            # A touch deeper and slower: an official voice.
            speech = chant.resample(speech, 0.93)
            gap = np.zeros(int(0.15 * SR), np.float32)
            tail = np.zeros(int(0.3 * SR), np.float32)
            clip = np.concatenate([chime(), gap, pa(speech) * 0.9, tail])
            write_mp3(clip, os.path.join(chant.OUT_DIR, f"voice_{n}.mp3"), tmp)
        # The crowd bursting out: "We're free!" twice.
        print("crowd: We're free! We're free!", flush=True)
        rng = np.random.default_rng(42)
        voices = chant.quality_voices() or chant.BASIC_VOICES
        words = [("We're", 1), ("free!", 1), ("We're", 1), ("free!", 1)]
        mix, people = chant.chant(words, voices, rng, tmp, "free")
        write_mp3(mix, os.path.join(chant.OUT_DIR, f"voice_{len(ANNOUNCEMENTS)}.mp3"), tmp)
        win_call(installed, tmp)


if __name__ == "__main__":
    main()
