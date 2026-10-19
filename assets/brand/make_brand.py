#!/usr/bin/env python3
"""Generates the FissionOxide brand assets from one description of the fox.

The fox is a symmetrical low-poly head: each half is a list of facets (polygons) with a colour
role, and the right half is the mirror of the left with its own colours. The colours are the
app's icon families (see crates/ui/src/icons.rs and theme.rs), so the logo carries the same
colours as the tools:

  purple ear  - the app's purple tone       red ear / jaw - delete red (ICON_RED)
  yellow face - construction amber          orange face   - surfaces (ICON_ORANGE)
  blue centre - solid modelling (ACCENT)    green jaw     - inspect / select (ICON_GREEN)

Writes into this directory:
  fox.svg                  the mark alone, transparent (works on light and dark)
  icon.svg, icon-light.svg the app icon: the mark on a dark or a light rounded tile
  wordmark.svg             mark + "FissionOxide", text switches colour with prefers-color-scheme
  wordmark-light.svg       for light backgrounds (dark text)
  wordmark-dark.svg        for dark backgrounds (light text)
  icon-*.png               raster app icons (needs resvg-py)
and crates/ui/src/logo.rs, the same facets for painting the mark inside the app.

Requires fontTools (pip install fonttools resvg-py) and the Inter Display Bold font
(OFL licensed, https://rsms.me/inter/); the wordmark text is converted to outlines, so the SVGs
need no font installed. Run: python3 assets/brand/make_brand.py
"""

import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, "..", ".."))
FONT = os.environ.get("FOX_BRAND_FONT", "/usr/share/fonts/opentype/inter/InterDisplay-Bold.otf")

# The mark's design grid: 600 x 680, centre line x = 300.
W, H = 600, 680

# Shades per colour role: light (lit), base, dark, deep (shadow).
PALETTE = {
  "purple": ["#B07FEA", "#9660D6", "#7846BC", "#55289A"],
  "red": ["#F0625A", "#DE3832", "#BC2A27", "#93201F"],
  "yellow": ["#FFCD4A", "#F7B92E", "#E8A21E", "#CC8812"],
  "orange": ["#FFA54D", "#F5961E", "#E37A12", "#C2600C"],
  "blue": ["#4BA6F2", "#1E88E5", "#1669C4", "#0F4F9E"],
  "green": ["#8ED04F", "#58BB4A", "#38AC46", "#25863A"],
}

# Left half facets: (colour role on the left, colour role on the right, shade, points).
# Shades are given for the left (lit) side; the right side uses one step darker.
LEFT = [
  # ear
  ("purple", "red", 0, [(50, 20), (240, 170), (150, 178)]),
  ("purple", "red", 1, [(50, 20), (150, 178), (62, 368)]),
  ("purple", "red", 3, [(150, 178), (240, 170), (62, 368)]),
  # face
  ("yellow", "orange", 0, [(240, 170), (15, 412), (175, 322)]),
  ("yellow", "orange", 1, [(240, 170), (175, 322), (300, 460)]),
  ("yellow", "orange", 2, [(15, 412), (175, 322), (118, 384)]),
  ("yellow", "orange", 3, [(118, 384), (175, 322), (300, 460)]),
  # eye mask
  ("blue", "blue", 1, [(118, 384), (300, 460), (205, 485)]),
  ("blue", "blue", 2, [(205, 485), (300, 460), (256, 605)]),
  # muzzle
  ("yellow", "orange", 1, [(262, 425), (300, 460), (300, 605), (256, 605)]),
  # jaw
  ("green", "red", 0, [(15, 412), (118, 384), (205, 485)]),
  ("green", "red", 1, [(15, 412), (205, 485), (226, 552)]),
  ("green", "red", 2, [(15, 412), (226, 552), (252, 662)]),
  ("green", "red", 3, [(205, 485), (256, 605), (252, 662), (226, 552)]),
  # chin
  ("blue", "blue", 3, [(256, 605), (300, 638), (252, 662)]),
  ("blue", "blue", 2, [(252, 662), (300, 638), (300, 662)]),
  # forehead
  ("blue", "blue", 1, [(240, 170), (300, 236), (300, 460)]),
]
# Shared centre facets that straddle the centre line.
CENTRE = [
  ("blue", 0, [(240, 170), (360, 170), (300, 236)]),
  ("blue", 0, [(256, 605), (344, 605), (300, 638)]),
]


def facets():
  """Every facet in drawing order as (hex colour, [(x, y)])."""
  out = []
  for left, right, shade, pts in LEFT:
    out.append((PALETTE[left][shade], pts))
    out.append((PALETTE[right][min(shade + 1, 3)], [(W - x, y) for x, y in pts]))
  for role, shade, pts in CENTRE:
    out.append((PALETTE[role][shade], pts))
  return out


def poly_path(pts):
  return "M" + " L".join(f"{x:g} {y:g}" for x, y in pts) + " Z"


def mark_group(transform=""):
  t = f' transform="{transform}"' if transform else ""
  # a hairline stroke in the facet's own colour closes the anti-aliasing seams between facets
  body = "\n".join(
    f'    <path d="{poly_path(p)}" fill="{c}" stroke="{c}" stroke-width="0.8" stroke-linejoin="round"/>'
    for c, p in facets()
  )
  return f"  <g{t}>\n{body}\n  </g>"


def svg(w, h, inner, title, style=""):
  s = f"\n  <style>{style}</style>" if style else ""
  return (
    f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w:g} {h:g}" width="{w:g}" height="{h:g}"'
    f' role="img" aria-label="{title}">\n  <title>{title}</title>{s}\n{inner}\n</svg>\n'
  )


def text_path(text, height_px):
  """Outlines of `text` in the brand font, scaled so the cap height is `height_px`.

  Returns (path data with the baseline at y=0 and the first glyph at x=0, advance width)."""
  from fontTools.pens.svgPathPen import SVGPathPen
  from fontTools.pens.transformPen import TransformPen
  from fontTools.ttLib import TTFont

  font = TTFont(FONT)
  cmap = font.getBestCmap()
  gs = font.getGlyphSet()
  scale = height_px / font["OS/2"].sCapHeight
  pen = SVGPathPen(gs, ntos=lambda v: f"{v:.2f}".rstrip("0").rstrip("."))
  x = 0
  kern = _kerning(font)
  names = [cmap[ord(ch)] for ch in text]
  for i, name in enumerate(names):
    gs[name].draw(TransformPen(pen, (scale, 0, 0, -scale, x * scale, 0)))
    x += gs[name].width
    if i + 1 < len(names):
      x += kern.get((name, names[i + 1]), 0)
  return pen.getCommands(), x * scale


def _kerning(font):
  """Pair kerning from the GPOS table (format 1 pairs only; enough for the wordmark)."""
  pairs = {}
  if "GPOS" not in font:
    return pairs
  for lookup in font["GPOS"].table.LookupList.Lookup:
    for st in lookup.SubTable:
      st = getattr(st, "ExtSubTable", st)
      if getattr(st, "Format", None) == 1 and st.__class__.__name__ == "PairPos":
        cov = st.Coverage.glyphs
        for first, ps in zip(cov, st.PairSet):
          for rec in ps.PairValueRecord:
            v = getattr(rec.Value1, "XAdvance", 0) if rec.Value1 else 0
            if v:
              pairs.setdefault((first, rec.SecondGlyph), v)
  return pairs


INK_LIGHT = "#161B24"  # wordmark text on light backgrounds
INK_DARK = "#F3F5F8"  # on dark backgrounds
TILE_DARK = ("#2A3342", "#161B24")
TILE_LIGHT = ("#FFFFFF", "#E9EDF2")


def tile_icon(size, colours, border):
  pad = size * 0.13
  s = (size - 2 * pad) / H
  dx = (size - W * s) / 2
  r = size * 0.225
  top, bottom = colours
  inner = f"""  <defs>
    <linearGradient id="tile" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="{top}"/>
      <stop offset="1" stop-color="{bottom}"/>
    </linearGradient>
  </defs>
  <rect x="0.5" y="0.5" width="{size - 1:g}" height="{size - 1:g}" rx="{r:g}" fill="url(#tile)" stroke="{border}"/>
{mark_group(f"translate({dx:.3f} {pad:.3f}) scale({s:.5f})")}"""
  return inner


def write(name, data):
  path = os.path.join(HERE, name)
  mode = "wb" if isinstance(data, bytes) else "w"
  with open(path, mode) as f:
    f.write(data)
  print("wrote", os.path.relpath(path, ROOT))


def wordmark(ink, auto=False):
  mark_h = 680
  cap = 168
  d, adv = text_path("FissionOxide", cap)
  gap = 70
  text_x = W + gap
  baseline = mark_h * 0.55 + cap / 2
  width = text_x + adv + 8
  fill = "" if auto else f' fill="{ink}"'
  inner = mark_group() + f'\n  <path class="ink"{fill} transform="translate({text_x:g} {baseline:g})" d="{d}"/>'
  style = ""
  if auto:
    style = f".ink{{fill:{INK_LIGHT}}}@media (prefers-color-scheme:dark){{.ink{{fill:{INK_DARK}}}}}"
  return svg(width, mark_h, inner, "FissionOxide", style)


def rust_module():
  lines = [
    "//! The FissionOxide fox, as flat-coloured facets for painting the logo inside the app.",
    "//!",
    "//! Generated by `assets/brand/make_brand.py` from the same description as the SVG logo; edit",
    "//! that script, not this file.",
    "",
    "use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, pos2};",
    "",
    "/// Width and height of the design grid the facets are given in.",
    f"pub const SIZE: (f32, f32) = ({W}.0, {H}.0);",
    "",
    "/// Every facet in drawing order: colour and corners on the design grid.",
    "pub const FACETS: &[(Color32, &[(f32, f32)])] = &[",
  ]
  for c, pts in facets():
    r, g, b = int(c[1:3], 16), int(c[3:5], 16), int(c[5:7], 16)
    p = ", ".join(f"({x}.0, {y}.0)" for x, y in pts)
    lines.append(f"  (Color32::from_rgb({r}, {g}, {b}), &[{p}]),")
  lines += [
    "];",
    "",
    "/// Paints the fox centred in `rect`, as large as fits.",
    "pub fn paint(painter: &Painter, rect: Rect) {",
    "  let s = (rect.width() / SIZE.0).min(rect.height() / SIZE.1);",
    "  let origin = rect.center() - egui::vec2(SIZE.0, SIZE.1) * s / 2.0;",
    "  let at = |&(x, y): &(f32, f32)| -> Pos2 { pos2(origin.x + x * s, origin.y + y * s) };",
    "  for (c, pts) in FACETS {",
    "    let pts: Vec<Pos2> = pts.iter().map(at).collect();",
    "    // a hairline in the facet's colour hides the anti-aliasing seams between facets",
    "    painter.add(Shape::convex_polygon(pts, *c, Stroke::new(0.6, *c)));",
    "  }",
    "}",
    "",
  ]
  return "\n".join(lines)


def main():
  write("fox.svg", svg(W, H, mark_group(), "FissionOxide"))
  write("icon.svg", svg(1024, 1024, tile_icon(1024, TILE_DARK, "#0B0E13"), "FissionOxide"))
  write("icon-light.svg", svg(1024, 1024, tile_icon(1024, TILE_LIGHT, "#D3D9E1"), "FissionOxide"))
  write("wordmark.svg", wordmark(None, auto=True))
  write("wordmark-light.svg", wordmark(INK_LIGHT))
  write("wordmark-dark.svg", wordmark(INK_DARK))
  with open(os.path.join(ROOT, "crates", "ui", "src", "logo.rs"), "w") as f:
    f.write(rust_module())
  print("wrote crates/ui/src/logo.rs")
  try:
    import resvg_py
  except ImportError:
    print("resvg-py not installed; skipping PNGs", file=sys.stderr)
    return
  icon = os.path.join(HERE, "icon.svg")
  for size in (32, 64, 128, 256, 512, 1024):
    png = resvg_py.svg_to_bytes(svg_path=icon, width=size, height=size)
    write(f"icon-{size}.png", bytes(png))


if __name__ == "__main__":
  main()
