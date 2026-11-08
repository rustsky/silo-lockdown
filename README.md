# Silo: Lockdown

A tower defense game set in an underground silo. You are IT. The uprising climbs the spiral
stair from Mechanical toward the airlock. Build security on the living levels and catwalks;
if the airlock seals fail, the silo is opened.

## Layout

| crate | what |
|---|---|
| `crates/silo_core` | Game engine. No dependencies, deterministic fixed-tick simulation, all rules. Heavily tested. |
| `crates/silo_tui` | Terminal frontend (ratatui). |
| `crates/silo_gfx` | Graphical frontend (macroquad): desktop, web, Android, iOS. |

Frontends only render and send commands; every rule lives in `silo_core`.

## Play

```sh
cargo run --release -p silo_gfx   # graphical (desktop)
cargo run --release -p silo_tui   # terminal, needs at least 90x29
```

Controls (both): `1-4` pick tower, `5` barrier, click/tap to build or select, `U` upgrade, `X` sell,
`N`/Space send wave, `F` speed, `P` pause, `T` autopilot, `M` sound (GUI), `Esc` deselect.

The silo is three sections tall (Up Top, Mids, Down Deep); the field shows one at a time.
Roll the view with `W`/`S`/arrows/PageUp/PageDown, the mouse wheel, or by dragging the field
(GUI); `PgUp`/`PgDn`, `[`/`]` or the wheel (TUI). The GUI minimap on the right wall shows the
whole silo and every rebel; the arrows pulse red with a count when rebels are out of sight.

Threats beyond the stair:
- **IT's server room** (level ~33). Rebels passing its door attack it. If it falls, IT loses
  the cameras below Up Top (Mids and Down Deep go black) for 25s; it reboots and is repaired
  between waves.
- **Supply joins the uprising** (wave 6+). Rebels passing the Supply depot (level ~101) are
  handed guns (they shoot the nearest tower) and breaching tools (they smash barriers). The
  share grows every wave; kill them below Supply to keep them unarmed. Towers have HP, are lost
  at 0, and are repaired between waves.
- **Barriers** (`5`/`B`, 40 chits): barricades on the stair, only above Supply. Rebels stop and
  must break them; bare hands take ages, tools from Supply take seconds. `X` removes one.
- **Main power cuts** (wave 15+). Mechanical blacks out one whole section at a time; you can't
  see the rebels in it.

## Platforms (silo_gfx)

**Desktop (macOS / Windows / Linux):** `cargo run --release -p silo_gfx`

**Web (WASM), installable on phones:**
```sh
rustup target add wasm32-unknown-unknown
./scripts/build-web.sh
python3 -m http.server -d dist/web 8080   # open http://localhost:8080
```
`dist/web` is a complete installable web app (PWA): `index.html`, the wasm, miniquad's
`gl.js` and quad-snd's `audio.js` (vendored from the exact versions we build against),
`audio-unlock.js` (fixes silent audio on iPhone), a manifest, icons and a service worker.
Upload the folder to any HTTPS host (GitHub Pages, Netlify, itch.io...). On iPhone: open the
URL in Safari, Share -> Add to Home Screen. It then launches full-screen from its icon and
runs offline from the phone (the service worker caches the whole game on the device).

**Share as a single file:**
```sh
./scripts/build-single-html.sh   # -> dist/silo-lockdown.html (~1.3 MB)
```
One self-contained HTML file (wasm embedded as base64, loaders inlined). Open it by
double-clicking, send it to someone, or drop it on any web host; no server or install needed.

**Android** (via [cargo-quad-apk](https://github.com/not-fl3/cargo-quad-apk), easiest with Docker):
```sh
docker run --rm -v $(pwd):/root/src -w /root/src notfl3/cargo-apk \
  cargo quad-apk build --release -p silo_gfx
# APK: target/android-artifacts/release/apk/silo_gfx.apk
```

**iOS:**
```sh
rustup target add aarch64-apple-ios          # or aarch64-apple-ios-sim for the simulator
cargo build --release -p silo_gfx --target aarch64-apple-ios
```
Then put the binary in a `.app` bundle with an `Info.plist` and sign it
(see the [macroquad iOS guide](https://macroquad.rs/articles/ios/)).

## Play online (GitHub Pages)

`.github/workflows/pages.yml` tests the game, builds the web version and publishes it on every
push to `main`/`master`. One-time setup on GitHub: **Settings → Pages → Source: GitHub Actions**.
The game is then at `https://<user>.github.io/<repo>/`, playable in any browser and installable
from Safari/Chrome ("Add to Home Screen") to run offline.

**Player counts.** The title screen of the published web version shows how many people are
playing right now and how many have played so far (`web/stats.js`). It uses two public counters
on [abacus](https://abacus.jasoncameron.dev) (free, no account): one goes up once per device, the
other once every 30 s per open game, and the game reads "playing now" from how fast that one
rises. Nothing personal is stored. Local and single-file builds don't count and hide the panel.

## Music and sound

All audio is synthesised at startup (no audio files):
- `music.rs`: a D-minor "shift" loop between waves and an "uprising" loop with bass, drums and
  arpeggio during waves, crossfaded with the game phase.
- `sfx.rs`: 22 effects (each tower's shot, rebel gunfire, explosions, kills and bounty, breach
  klaxon, wave horn, IT power-down, power cuts, barricade hits, build/upgrade/sell...). Frequent
  ones have variations; playback is rate-limited and quieter for off-screen action.

`M` or the speaker button cycles music + effects, effects only (FX), silent. To listen to the
raw sounds: `SILO_MUSIC_OUT=/tmp cargo test -p silo_gfx` writes every loop and effect as WAV.

## Engine tools

```sh
cargo test                                            # unit, integration, fuzz, balance, render tests
cargo run --release -p silo_core --example balance    # win rates of different build strategies
cargo run --release -p silo_core --example trace      # wave-by-wave bot game
cargo run -p silo_core --example map_dump             # tile layout
SILO_SHOT=shot.png SILO_SHOT_FRAME=900 cargo run --release -p silo_gfx   # autoplay screenshot
#   SILO_SHOT_VIEW=0|1|2 picks the section, SILO_SHOT_SKIP=N lets the AI play N waves first,
#   SILO_SHOT_BARRIERS=1 builds a few barriers in Mids, SILO_SHOT_NOAI=1 leaves the silo undefended
```
