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

Controls (both): `1-4` pick tower, click/tap to build or select, `U` upgrade, `X` sell,
`N`/Space send wave, `F` speed, `P` pause, `T` autopilot, `Esc` deselect.

## Platforms (silo_gfx)

**Desktop (macOS / Windows / Linux):** `cargo run --release -p silo_gfx`

**Web (WASM):**
```sh
rustup target add wasm32-unknown-unknown
./scripts/build-web.sh
python3 -m http.server -d dist/web 8080   # open http://localhost:8080
```

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

## Engine tools

```sh
cargo test                                            # unit, integration, fuzz, balance, render tests
cargo run --release -p silo_core --example balance    # win rates of different build strategies
cargo run --release -p silo_core --example trace      # wave-by-wave bot game
cargo run -p silo_core --example map_dump             # tile layout
SILO_SHOT=shot.png SILO_SHOT_FRAME=900 cargo run --release -p silo_gfx   # autoplay screenshot
```
