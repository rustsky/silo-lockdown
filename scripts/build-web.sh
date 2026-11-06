#!/usr/bin/env bash
# Build the web version into dist/web. Serve it with any static server, e.g.
#   python3 -m http.server -d dist/web 8080
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release -p silo_gfx --target wasm32-unknown-unknown
mkdir -p dist/web
cp target/wasm32-unknown-unknown/release/silo_gfx.wasm dist/web/
cp crates/silo_gfx/web/index.html crates/silo_gfx/web/gl.js dist/web/
echo "Built dist/web — run: python3 -m http.server -d dist/web 8080"
