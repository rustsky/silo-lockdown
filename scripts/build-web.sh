#!/usr/bin/env bash
# Build the web version into dist/web: an installable, offline-capable web app (PWA).
# Serve it from any HTTPS host (GitHub Pages, Netlify, ...), open it on the phone and use
# "Add to Home Screen". Locally: python3 -m http.server -d dist/web 8080
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release -p silo_gfx --target wasm32-unknown-unknown
rm -rf dist/web && mkdir -p dist/web
cp target/wasm32-unknown-unknown/release/silo_gfx.wasm dist/web/
cp crates/silo_gfx/web/{index.html,gl.js,audio.js,audio-unlock.js,stats.js,manifest.webmanifest,icon-192.png,icon-512.png,apple-touch-icon.png} dist/web/
# New build -> new service-worker cache, so installed copies pick up the update. Hashed over every
# served file, so a change to any of them (index.html too) reaches players who already have it cached.
version=$(cat dist/web/* | shasum | cut -c1-12)
sed "s/CACHE_VERSION/$version/" crates/silo_gfx/web/sw.js > dist/web/sw.js
echo "Built dist/web (cache $version) — run: python3 -m http.server -d dist/web 8080"
