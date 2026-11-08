#!/usr/bin/env python3
"""Draw the game icon (crates/silo_gfx/web/icon-*.png, apple-touch-icon.png).

Looking straight down the silo: the round shaft's levels as rings shrinking into the dark, lit
windows around each ring, and the spiral stair winding down through the middle. Written as SVG
and rasterised with rsvg-convert (brew install librsvg). Run from the repo root:
python3 scripts/make-icon.py
"""
import math
import pathlib
import subprocess

S = 512
C = S / 2
RINGS = 9


def ring_r(i: float) -> float:
    """Radius of level i (0 = top rim); each level further down looks smaller."""
    return 236 * 0.78 ** i


def svg() -> str:
    out = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{S}" height="{S}" viewBox="0 0 {S} {S}">']
    out.append("""<defs>
  <radialGradient id="pit" cx="0.5" cy="0.5" r="0.5">
    <stop offset="0" stop-color="#000"/><stop offset="0.55" stop-color="#0b0d12"/>
    <stop offset="1" stop-color="#1d222b"/>
  </radialGradient>
  <radialGradient id="glow" cx="0.5" cy="0.5" r="0.5">
    <stop offset="0" stop-color="#ffcf7a" stop-opacity="0.8"/><stop offset="1" stop-color="#ffb347" stop-opacity="0"/>
  </radialGradient>
</defs>""")
    out.append(f'<rect width="{S}" height="{S}" fill="#0a0b0e"/>')
    out.append(f'<circle cx="{C}" cy="{C}" r="{ring_r(0)}" fill="url(#pit)"/>')
    # Levels, from the deepest up so nearer rings draw over farther ones.
    for i in range(RINGS, 0, -1):
        r = ring_r(i)
        fade = max(0.0, 1 - i / (RINGS + 0.5))
        w = max(1.2, 13 * 0.78 ** i)
        out.append(f'<circle cx="{C}" cy="{C}" r="{r}" fill="none" stroke="#6b717c" stroke-width="{w}" opacity="{0.25 + 0.75 * fade:.2f}"/>')
        # Lit windows around the ring (a few dark, like empty rooms).
        n = 12
        for k in range(n):
            if (k * 7 + i * 3) % 5 == 0:
                continue
            a = 2 * math.pi * (k + 0.5 * (i % 2)) / n
            rr = r + w * 1.1
            x, y = C + rr * math.cos(a), C + rr * math.sin(a)
            g = max(1.5, 9 * 0.78 ** i)
            out.append(f'<circle cx="{x:.1f}" cy="{y:.1f}" r="{g * 2.2:.1f}" fill="url(#glow)" opacity="{fade:.2f}"/>')
            out.append(f'<circle cx="{x:.1f}" cy="{y:.1f}" r="{g * 0.6:.1f}" fill="#ffd690" opacity="{min(1, 0.3 + fade):.2f}"/>')
    # The spiral stair: a band winding inwards, one turn per two levels, with rail and treads.
    steps = 900
    turns = RINGS / 2
    pts = []
    for k in range(steps + 1):
        t = k / steps * RINGS
        a = -math.pi / 2 + 2 * math.pi * turns * k / steps
        r = ring_r(t) * 0.62
        pts.append((C + r * math.cos(a), C + r * math.sin(a), t))
    for (x0, y0, t), (x1, y1, _) in zip(pts, pts[1:]):
        fade = max(0.0, 1 - t / (RINGS + 0.5))
        w = max(1.0, 22 * 0.78 ** t)
        out.append(f'<line x1="{x0:.1f}" y1="{y0:.1f}" x2="{x1:.1f}" y2="{y1:.1f}" stroke="#9ba3af" stroke-width="{w:.1f}" stroke-linecap="round" opacity="{0.2 + 0.8 * fade:.2f}"/>')
    for (x0, y0, t), (x1, y1, _) in zip(pts, pts[1:]):
        fade = max(0.0, 1 - t / (RINGS + 0.5))
        w = max(0.6, 4 * 0.78 ** t)
        out.append(f'<line x1="{x0:.1f}" y1="{y0:.1f}" x2="{x1:.1f}" y2="{y1:.1f}" stroke="#c96a2b" stroke-width="{w:.1f}" opacity="{0.2 + 0.8 * fade:.2f}"/>')
    for k in range(0, steps, 9):
        x, y, t = pts[k]
        dx, dy = C - x, C - y
        d = math.hypot(dx, dy) or 1
        L = 9 * 0.78 ** t
        fade = max(0.0, 1 - t / (RINGS + 0.5))
        out.append(f'<line x1="{x - dx / d * L:.1f}" y1="{y - dy / d * L:.1f}" x2="{x + dx / d * L:.1f}" y2="{y + dy / d * L:.1f}" stroke="#4f5560" stroke-width="{max(0.5, 2.2 * 0.78 ** t):.1f}" opacity="{fade:.2f}"/>')
    # Central shaft column: darkness at the bottom.
    out.append(f'<circle cx="{C}" cy="{C}" r="{ring_r(RINGS) * 0.9}" fill="#000"/>')
    # The rim: the top level's concrete edge with hazard stripes.
    R0 = ring_r(0)
    out.append(f'<circle cx="{C}" cy="{C}" r="{R0}" fill="none" stroke="#5d626b" stroke-width="22"/>')
    out.append(f'<circle cx="{C}" cy="{C}" r="{R0}" fill="none" stroke="#e0b22c" stroke-width="8" stroke-dasharray="18 18"/>')
    out.append(f'<circle cx="{C}" cy="{C}" r="{R0 - 11}" fill="none" stroke="#2a2e35" stroke-width="3"/>')
    out.append(f'<circle cx="{C}" cy="{C}" r="{R0 + 11}" fill="none" stroke="#2a2e35" stroke-width="3"/>')
    out.append("</svg>")
    return "\n".join(out)


def main() -> None:
    web = pathlib.Path("crates/silo_gfx/web")
    src = pathlib.Path("target/icon.svg")
    src.parent.mkdir(exist_ok=True)
    src.write_text(svg())
    for name, size in (("icon-512.png", 512), ("icon-192.png", 192), ("apple-touch-icon.png", 180)):
        subprocess.run(
            ["rsvg-convert", "-w", str(size), "-h", str(size), "-b", "#0d0b0a", "-o", str(web / name), str(src)],
            check=True,
        )
        print("wrote", web / name)


if __name__ == "__main__":
    main()
