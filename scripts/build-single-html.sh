#!/usr/bin/env bash
# Build the whole game into ONE self-contained HTML file (dist/silo-lockdown.html):
# the wasm is embedded as base64 and miniquad's/quad-snd's loaders are inlined.
# Open it by double-clicking, email it, or upload it anywhere; no server needed.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release -p silo_gfx --target wasm32-unknown-unknown
mkdir -p dist
python3 - <<'PY'
import base64, pathlib
web = pathlib.Path("crates/silo_gfx/web")
wasm = pathlib.Path("target/wasm32-unknown-unknown/release/silo_gfx.wasm").read_bytes()
b64 = base64.b64encode(wasm).decode()
gl = (web / "gl.js").read_text()
audio = (web / "audio.js").read_text()
unlock = (web / "audio-unlock.js").read_text()
stats = (web / "stats.js").read_text()
html = f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1, maximum-scale=1, user-scalable=no, viewport-fit=cover">
<title>Silo: Lockdown</title>
<style>
  html, body, canvas {{ margin: 0; padding: 0; width: 100%; height: 100%; overflow: hidden;
    background: #08080a; touch-action: none; }}
  canvas:focus {{ outline: none; }}
</style>
</head>
<body>
<canvas id="glcanvas" tabindex="1"></canvas>
<script>{gl}</script>
<script>{audio}</script>
<script>{unlock}</script>
<script>{stats}</script>
<script id="wasm" type="application/octet-stream">{b64}</script>
<script>
  // The game is embedded below as base64; hand it to miniquad's loader as a Blob URL so the
  // page works from file:// and from any static host.
  const b64 = document.getElementById("wasm").textContent;
  const bytes = Uint8Array.from(atob(b64), c => c.charCodeAt(0));
  load(URL.createObjectURL(new Blob([bytes], {{ type: "application/wasm" }})));
</script>
</body>
</html>
"""
out = pathlib.Path("dist/silo-lockdown.html")
out.write_text(html)
print(f"Built {out} ({out.stat().st_size / 1e6:.1f} MB)")
PY
