#!/usr/bin/env python3
"""Builds the FissionOxide brand kit from its two sources.

Sources (edit these):
  brand/tokens.json     colours, type, sizes, radii, spacing
  brand/logo/fox.json   the fox mark's facets

Outputs (never edit by hand; re-run this script):
  brand/logo/           fox, single-colour fox, app icons, wordmarks (SVG) and PNG renders
  brand/web/            tokens.css, tokens.js, tailwind.preset.js, favicons, web manifest,
                        social preview image, head.html snippet
  brand/index.html      the brand guide
  crates/brand/src/tokens.rs, logo.rs   the same tokens and logo for Rust apps (fox-brand)

Needs Python 3.9+, fontTools and resvg-py (pip install fonttools resvg-py), ImageMagick's
`convert` for favicon.ico, and the Inter font family (OFL, https://rsms.me/inter/). The wordmark
and other text in images are converted to outlines, so no output depends on installed fonts.
Run from anywhere: python3 brand/build.py
"""

import html
import json
import os
import re
import shutil
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
FONT_DIR = os.environ.get("FOX_FONT_DIR", "/usr/share/fonts/opentype/inter")

TOKENS = json.load(open(os.path.join(HERE, "tokens.json")))
FOX = json.load(open(os.path.join(HERE, "logo", "fox.json")))
W, H = FOX["width"], FOX["height"]


# ---------------------------------------------------------------- tokens


def lookup(path):
  v = TOKENS
  for k in path.split("."):
    v = v[k]
  return resolve(v)


def resolve(v):
  """Replaces "{a.b.c}" references with the token they name."""
  if isinstance(v, str):
    m = re.fullmatch(r"\{([\w.-]+)\}", v)
    return lookup(m.group(1)) if m else v
  if isinstance(v, list):
    return [resolve(x) for x in v]
  if isinstance(v, dict):
    return {k: resolve(x) for k, x in v.items() if not k.startswith("$")}
  return v


T = resolve(TOKENS)
COLOR = T["color"]
SHADES = FOX["shades"]


def rgb(hex_):
  return int(hex_[1:3], 16), int(hex_[3:5], 16), int(hex_[5:7], 16)


# ---------------------------------------------------------------- the mark


def facets():
  """Every facet in drawing order as (hex colour, [(x, y)])."""
  out = []
  for f in FOX["left"]:
    out.append((COLOR[f["left"]][SHADES[f["shade"]]], f["points"]))
    right = [(W - x, y) for x, y in f["points"]]
    out.append((COLOR[f["right"]][SHADES[min(f["shade"] + 1, 3)]], right))
  for f in FOX["centre"]:
    out.append((COLOR[f["family"]][SHADES[f["shade"]]], f["points"]))
  return out


def poly(pts):
  return "M" + " L".join(f"{x:g} {y:g}" for x, y in pts) + " Z"


def mark(transform="", mono=None, uid="m"):
  """The fox as an SVG group. `mono` draws it in one colour, facets cut apart by thin gaps."""
  t = f' transform="{transform}"' if transform else ""
  if mono is None:
    # a hairline in each facet's own colour closes the anti-aliasing seams between facets
    body = "\n".join(
      f'    <path d="{poly(p)}" fill="{c}" stroke="{c}" stroke-width="0.8" stroke-linejoin="round"/>'
      for c, p in facets()
    )
    return f"  <g{t}>\n{body}\n  </g>"
  shapes = " ".join(poly(p) for _, p in facets())
  return f"""  <g{t}>
    <mask id="{uid}" maskUnits="userSpaceOnUse" x="0" y="0" width="{W}" height="{H}">
      <path d="{shapes}" fill="#fff" stroke="#fff" stroke-width="0.8"/>
      <path d="{shapes}" fill="none" stroke="#000" stroke-width="6" stroke-linejoin="round"/>
    </mask>
    <rect width="{W}" height="{H}" fill="{mono}" mask="url(#{uid})"/>
  </g>"""


def svg(w, h, inner, title=None, style=""):
  title = title or T["name"]
  s = f"\n  <style>{style}</style>" if style else ""
  return (
    f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w:g} {h:g}" width="{w:g}" height="{h:g}"'
    f' role="img" aria-label="{html.escape(title)}">\n  <title>{html.escape(title)}</title>{s}\n{inner}\n</svg>\n'
  )


# ---------------------------------------------------------------- text as outlines


_fonts = {}


def text_path(text, cap_px, font="InterDisplay-Bold.otf"):
  """Outlines of `text` with its cap height `cap_px`, baseline at y=0. Returns (path, width)."""
  from fontTools.pens.svgPathPen import SVGPathPen
  from fontTools.pens.transformPen import TransformPen
  from fontTools.ttLib import TTFont

  if font not in _fonts:
    f = TTFont(os.path.join(FONT_DIR, font))
    _fonts[font] = (f, f.getBestCmap(), f.getGlyphSet(), kerning(f))
  f, cmap, gs, kern = _fonts[font]
  scale = cap_px / f["OS/2"].sCapHeight
  pen = SVGPathPen(gs, ntos=lambda v: f"{v:.2f}".rstrip("0").rstrip("."))
  names = [cmap[ord(ch)] for ch in text]
  x = 0
  for i, name in enumerate(names):
    gs[name].draw(TransformPen(pen, (scale, 0, 0, -scale, x * scale, 0)))
    x += gs[name].width
    if i + 1 < len(names):
      x += kern.get((name, names[i + 1]), 0)
  return pen.getCommands(), x * scale


def kerning(font):
  """Pair kerning from GPOS (format 1 pair lists; enough for short brand text)."""
  pairs = {}
  if "GPOS" not in font:
    return pairs
  for lookup_ in font["GPOS"].table.LookupList.Lookup:
    for st in lookup_.SubTable:
      st = getattr(st, "ExtSubTable", st)
      if st.__class__.__name__ == "PairPos" and getattr(st, "Format", None) == 1:
        for first, ps in zip(st.Coverage.glyphs, st.PairSet):
          for rec in ps.PairValueRecord:
            v = getattr(rec.Value1, "XAdvance", 0) if rec.Value1 else 0
            if v:
              pairs.setdefault((first, rec.SecondGlyph), v)
  return pairs


# ---------------------------------------------------------------- logo files

LIGHT, DARK = T["theme"]["light"], T["theme"]["dark"]


def tile_icon(size, tile, border):
  pad = size * 0.13
  s = (size - 2 * pad) / H
  dx = (size - W * s) / 2
  r = size * 0.225
  top, bottom = tile
  return f"""  <defs>
    <linearGradient id="tile" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="{top}"/>
      <stop offset="1" stop-color="{bottom}"/>
    </linearGradient>
  </defs>
  <rect x="0.5" y="0.5" width="{size - 1:g}" height="{size - 1:g}" rx="{r:g}" fill="url(#tile)" stroke="{border}"/>
{mark(f"translate({dx:.3f} {pad:.3f}) scale({s:.5f})")}"""


def wordmark(ink=None, mono=None):
  """Mark + name. With no `ink` the text follows prefers-color-scheme."""
  cap = 168
  d, adv = text_path(T["name"], cap)
  text_x = W + 70
  baseline = H * 0.55 + cap / 2
  width = text_x + adv + 8
  fill = f' fill="{ink}"' if ink else ""
  inner = mark(mono=mono, uid="wm") + f'\n  <path class="ink"{fill} transform="translate({text_x:g} {baseline:g})" d="{d}"/>'
  style = ""
  if not ink:
    style = f".ink{{fill:{LIGHT['wordmark']}}}@media (prefers-color-scheme:dark){{.ink{{fill:{DARK['wordmark']}}}}}"
  return svg(width, H, inner, style=style)


def social_card():
  """1200 x 630 link preview (Open Graph / Twitter)."""
  w, h = 1200, 630
  s = 360 / H
  d, adv = text_path(T["name"], 74)
  t, tadv = text_path(T["tagline"], 30, "Inter-Medium.otf")
  x0 = (w - (W * s + 56 + adv)) / 2
  tx = x0 + W * s + 56
  return svg(
    w,
    h,
    f"""  <rect width="{w}" height="{h}" fill="{DARK['background']}"/>
{mark(f"translate({x0:.2f} {(h - H * s) / 2:.2f}) scale({s:.5f})")}
  <path fill="{DARK['wordmark']}" transform="translate({tx:.2f} {h / 2 + 18:.2f})" d="{d}"/>
  <path fill="{DARK['text-muted']}" transform="translate({tx + 3:.2f} {h / 2 + 76:.2f})" d="{t}"/>
  <rect y="{h - 10}" width="{w}" height="10" fill="url(#stripe)"/>
  <defs>
    <linearGradient id="stripe" x1="0" x2="1">
{stripe_stops()}
    </linearGradient>
  </defs>""",
    title=f"{T['name']} - {T['tagline']}",
  )


def stripe_stops():
  """Hard-edged stops: one band per logo family, in the order they sit on the fox."""
  fams = T["logo_families"]
  out = []
  for i, f in enumerate(fams):
    a, b = i / len(fams), (i + 1) / len(fams)
    c = COLOR[f]["base"]
    out.append(f'      <stop offset="{a:.4f}" stop-color="{c}"/><stop offset="{b:.4f}" stop-color="{c}"/>')
  return "\n".join(out)


# ---------------------------------------------------------------- web files


def css_name(s):
  return s.replace("_", "-")


def tokens_css():
  out = [
    "/* FissionOxide design tokens. Generated by brand/build.py from brand/tokens.json; do not edit. */",
    "",
    ":root {",
  ]
  for fam, c in COLOR.items():
    for shade in SHADES:
      suffix = "" if shade == "base" else f"-{shade}"
      out.append(f"  --fox-{fam}{suffix}: {c[shade]};")
  for k, v in T["neutral"].items():
    out.append(f"  --fox-{k}: {v};")
  for k, v in T["status"].items():
    out.append(f"  --fox-{k}: {v};")
  for k, f in T["font"].items():
    if "stack" in f:
      out.append(f"  --fox-font-{k}: {f['stack']};")
  for k, v in T["size"].items():
    out.append(f"  --fox-text-{k}: {v / 16:g}rem;")
  for k, v in T["radius"].items():
    out.append(f"  --fox-radius-{css_name(k)}: {v if isinstance(v, str) else f'{v}px'};")
  for k, v in T["space"].items():
    out.append(f"  --fox-space-{k}: {v}px;")
  out.append("  color-scheme: light dark;")
  out += theme_vars(LIGHT) + ["}", ""]
  dark = theme_vars(DARK)
  out += ['@media (prefers-color-scheme: dark) {', '  :root:not([data-theme="light"]) {']
  out += ["  " + l for l in dark] + ["  }", "}", "", ':root[data-theme="dark"] {'] + dark + ["}", ""]
  out += [':root[data-theme="light"] {'] + theme_vars(LIGHT) + ["}", ""]
  return "\n".join(out)


def theme_vars(theme):
  return [f"  --fox-{k}: {v};" for k, v in theme.items()]


def plain_tokens():
  """tokens.json fully resolved, without descriptions: what web code and other tools import."""
  return T


def tokens_js():
  body = json.dumps(plain_tokens(), indent=2)
  return (
    "// FissionOxide design tokens. Generated by brand/build.py from brand/tokens.json; do not edit.\n"
    f"export const tokens = {body};\n\nexport default tokens;\n"
  )


def tailwind_preset():
  colors = {"fox": {}}
  for fam, c in COLOR.items():
    colors["fox"][fam] = {"light": c["light"], "DEFAULT": c["base"], "dark": c["dark"], "deep": c["deep"]}
  colors["fox"].update(T["neutral"])
  colors["fox"].update(T["status"])
  # semantic colours switch with the theme through the CSS variables in tokens.css
  for k in T["theme"]["light"]:
    colors["fox"][k] = f"var(--fox-{k})"
  stack = lambda k: [s.strip().strip('"') for s in T["font"][k]["stack"].split(",")]
  preset = {
    "theme": {
      "extend": {
        "colors": colors,
        "fontFamily": {"display": stack("display"), "sans": stack("body"), "mono": stack("mono")},
        "borderRadius": {f"fox-{k}": (v if isinstance(v, str) else f"{v}px") for k, v in T["radius"].items()},
      }
    }
  }
  return (
    "// FissionOxide Tailwind preset. Generated by brand/build.py; do not edit.\n"
    "// Use: presets: [require('./brand/web/tailwind.preset.js')], and import tokens.css for the theme colours.\n"
    f"module.exports = {json.dumps(preset, indent=2)};\n"
  )


def manifest():
  return json.dumps(
    {
      "name": T["name"],
      "short_name": T["name"],
      "description": T["tagline"],
      "icons": [
        {"src": "icon-192.png", "sizes": "192x192", "type": "image/png"},
        {"src": "icon-512.png", "sizes": "512x512", "type": "image/png"},
        {"src": "favicon.svg", "sizes": "any", "type": "image/svg+xml"},
      ],
      "theme_color": DARK["background"],
      "background_color": DARK["background"],
      "display": "standalone",
    },
    indent=2,
  ) + "\n"


HEAD_HTML = """<!-- FissionOxide: paste into <head>; paths assume brand/web is served at /brand/ -->
<link rel="icon" href="/brand/favicon.ico" sizes="48x48">
<link rel="icon" href="/brand/favicon.svg" type="image/svg+xml">
<link rel="apple-touch-icon" href="/brand/apple-touch-icon.png">
<link rel="manifest" href="/brand/site.webmanifest">
<meta name="theme-color" content="{dark_bg}">
<meta property="og:title" content="{name}">
<meta property="og:description" content="{tagline}">
<meta property="og:image" content="/brand/og-image.png">
<meta name="twitter:card" content="summary_large_image">
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&family=JetBrains+Mono:wght@400;600&display=swap">
<link rel="stylesheet" href="/brand/tokens.css">
"""


# ---------------------------------------------------------------- Rust (fox-brand)


def rs_rgb(h):
  r, g, b = rgb(h)
  return f"Rgb({r}, {g}, {b})"


def rs_const(name):
  return re.sub(r"[^A-Z0-9_]", "_", name.upper())


GEN = "// Generated by brand/build.py from brand/tokens.json; do not edit.\n"


def tokens_rs():
  o = [GEN, "use crate::{Family, Rgb};", ""]
  o += [
    f'/// The product name.\npub const NAME: &str = "{T["name"]}";',
    f'/// The short name used in code (crates, file extensions, environment variables).\npub const SHORT_NAME: &str = "{T["short_name"]}";',
    f'/// The one-line description.\npub const TAGLINE: &str = "{T["tagline"]}";',
    "",
  ]
  for fam, c in COLOR.items():
    role = TOKENS["color"][fam]["role"]
    o.append(f"/// {role}.")
    o.append(
      f"pub const {rs_const(fam)}: Family = Family {{ light: {rs_rgb(c['light'])}, base: {rs_rgb(c['base'])}, "
      f"dark: {rs_rgb(c['dark'])}, deep: {rs_rgb(c['deep'])} }};"
    )
  o.append("")
  o.append("/// Every colour family by name, the six the fox wears first.")
  o.append("pub const FAMILIES: &[(&str, Family)] = &[")
  order = T["logo_families"] + [f for f in COLOR if f not in T["logo_families"]]
  o += [f'  ("{f}", {rs_const(f)}),' for f in order]
  o += ["];", ""]

  def module(name, doc, items):
    o.append(f"/// {doc}")
    o.append(f"pub mod {name} {{")
    o.append("  use crate::Rgb;")
    for k, v in items.items():
      if isinstance(v, list):
        o.append(f"  pub const {rs_const(k)}: [Rgb; {len(v)}] = [{', '.join(rs_rgb(x) for x in v)}];")
      else:
        o.append(f"  pub const {rs_const(k)}: Rgb = {rs_rgb(v)};")
    o.append("}")
    o.append("")

  module("neutral", "Slate greys, darkest first.", T["neutral"])
  module("status", "Error, warning, success and info.", T["status"])
  module("light", "Semantic colours for light mode.", T["theme"]["light"])
  module("dark", "Semantic colours for dark mode.", T["theme"]["dark"])
  module("app", "The desktop app's light theme, by the part of the window it paints.", T["app"])
  o.append("/// Font families (Inter, SIL Open Font License).")
  o.append("pub mod font {")
  for k, f in T["font"].items():
    if "family" in f:
      o.append(f'  pub const {rs_const(k)}: &str = "{f["family"]}";')
  o += ["}", ""]
  return "\n".join(o)


def logo_rs():
  o = [
    GEN.replace("tokens.json", "logo/fox.json"),
    "use crate::Rgb;",
    "",
    "/// Width and height of the grid the facets are drawn on.",
    f"pub const SIZE: (f32, f32) = ({W}.0, {H}.0);",
    "",
    "/// One flat-coloured polygon of the fox.",
    "#[derive(Debug, Clone, Copy)]",
    "pub struct Facet {",
    "  pub color: Rgb,",
    "  pub points: &'static [(f32, f32)],",
    "}",
    "",
    "/// Every facet in drawing order.",
    "pub const FACETS: &[Facet] = &[",
  ]
  for c, pts in facets():
    p = ", ".join(f"({x}.0, {y}.0)" for x, y in pts)
    o.append(f"  Facet {{ color: {rs_rgb(c)}, points: &[{p}] }},")
  o += ["];", ""]
  return "\n".join(o)


# ---------------------------------------------------------------- the guide


def guide():
  sw = []
  for fam in T["logo_families"] + [f for f in COLOR if f not in T["logo_families"]]:
    c = COLOR[fam]
    meta = TOKENS["color"][fam]
    chips = "".join(
      f'<div class="chip" style="background:{c[s]}"><span>{s}</span><code>{c[s]}</code></div>' for s in SHADES
    )
    on_fox = f'<p class="muted">On the fox: {html.escape(meta["logo"])}</p>' if "logo" in meta else ""
    sw.append(
      f'<article class="family"><header><h3>{fam.title()}</h3><code>--fox-{fam}</code></header>'
      f'<p>{html.escape(meta["role"])}</p>{on_fox}<div class="chips">{chips}</div></article>'
    )
  neutrals = "".join(
    f'<div class="neutral"><div class="dot" style="background:{v}"></div><b>{k}</b><code>{v}</code></div>'
    for k, v in T["neutral"].items()
  )
  themes = ""
  for mode, th in T["theme"].items():
    rows = "".join(
      f'<tr><td><span class="dot" style="background:{v}"></span></td><td><code>--fox-{k}</code></td><td><code>{v}</code></td></tr>'
      for k, v in th.items()
    )
    themes += f'<div class="theme-table"><h3>{mode.title()} mode</h3><table>{rows}</table></div>'
  fonts = "".join(
    f'<div class="font-card"><p class="sample" style="font-family:var(--fox-font-{k});font-weight:{f["weights"][-1]}">'
    f'Aa Bb 0123</p><b>{f["family"]}</b><span class="muted">{html.escape(f["use"])} · weights {", ".join(map(str, f["weights"]))}</span></div>'
    for k, f in T["font"].items()
    if "family" in f
  )
  files = [
    ("logo/fox.svg", "The fox, full colour, transparent. The everyday mark."),
    ("logo/fox-mono-ink.svg", "One colour, for light backgrounds, print, engraving and stamps."),
    ("logo/fox-mono-white.svg", "One colour, for dark or photographic backgrounds."),
    ("logo/wordmark.svg", "Fox + name; the name switches colour with the viewer's theme."),
    ("logo/wordmark-light.svg", "Fox + name for light backgrounds."),
    ("logo/wordmark-dark.svg", "Fox + name for dark backgrounds."),
    ("logo/icon.svg", "App icon on the dark tile (the default)."),
    ("logo/icon-light.svg", "App icon on the light tile."),
    ("logo/png/", "PNG renders of all of the above."),
    ("web/tokens.css", "CSS custom properties, with light and dark themes."),
    ("web/tokens.js", "The tokens as an ES module."),
    ("web/tailwind.preset.js", "Tailwind preset: fox colours, fonts and radii."),
    ("web/head.html", "Favicon, manifest, social-card and font tags to paste into a page."),
    ("web/og-image.png", "1200 x 630 link preview."),
    ("tokens.json", "The source of every colour and size here."),
  ]
  file_rows = "".join(f'<tr><td><a href="{p}"><code>{p}</code></a></td><td>{html.escape(d)}</td></tr>' for p, d in files)
  stripe = "".join(f'<span style="background:{COLOR[f]["base"]}"></span>' for f in T["logo_families"])
  return f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{T['name']} Brand</title>
<link rel="icon" href="web/favicon.svg" type="image/svg+xml">
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Inter:wght@400;500;600;700&family=JetBrains+Mono:wght@400;600&display=swap">
<link rel="stylesheet" href="web/tokens.css">
<!-- Generated by brand/build.py; do not edit. -->
<style>
  * {{ box-sizing: border-box; }}
  body {{ margin: 0; background: var(--fox-background); color: var(--fox-text); font: 400 var(--fox-text-md)/1.55 var(--fox-font-body); }}
  main {{ max-width: 1080px; margin: 0 auto; padding: var(--fox-space-7) var(--fox-space-4) var(--fox-space-9); }}
  h1, h2, h3 {{ font-family: var(--fox-font-display); letter-spacing: -0.01em; margin: 0; }}
  h2 {{ font-size: var(--fox-text-2xl); margin: var(--fox-space-8) 0 var(--fox-space-2); }}
  h3 {{ font-size: var(--fox-text-md); }}
  p {{ margin: var(--fox-space-1) 0; }}
  code {{ font: 400 var(--fox-text-xs) var(--fox-font-mono); }}
  a {{ color: var(--fox-accent); }}
  .muted {{ color: var(--fox-text-muted); font-size: var(--fox-text-sm); }}
  .lede {{ font-size: var(--fox-text-lg); color: var(--fox-text-muted); max-width: 60ch; }}
  .stripe {{ display: flex; height: 6px; border-radius: var(--fox-radius-pill); overflow: hidden; margin: var(--fox-space-5) 0; }}
  .stripe span {{ flex: 1; }}
  .toggle {{ float: right; font: 500 var(--fox-text-sm) var(--fox-font-body); background: var(--fox-surface); color: var(--fox-text);
    border: 1px solid var(--fox-border); border-radius: var(--fox-radius-pill); padding: 6px 14px; cursor: pointer; }}
  .hero img {{ width: min(560px, 100%); height: auto; display: block; }}
  .grid {{ display: grid; gap: var(--fox-space-4); grid-template-columns: repeat(auto-fit, minmax(240px, 1fr)); }}
  .panel {{ border: 1px solid var(--fox-border); border-radius: var(--fox-radius-lg); padding: var(--fox-space-5);
    display: flex; flex-direction: column; gap: var(--fox-space-3); }}
  .panel .art {{ height: 150px; display: grid; place-items: center; border-radius: var(--fox-radius-md); }}
  .panel .art img {{ max-height: 120px; max-width: 90%; }}
  .on-light {{ background: #fff; }} .on-dark {{ background: #121418; }}
  .on-check {{ background: repeating-conic-gradient(#d9dde3 0 25%, #f3f5f8 0 50%) 0 0 / 16px 16px; }}
  .family {{ border: 1px solid var(--fox-border); border-radius: var(--fox-radius-lg); padding: var(--fox-space-4); }}
  .family header {{ display: flex; justify-content: space-between; align-items: baseline; }}
  .chips {{ display: grid; grid-template-columns: repeat(4, 1fr); gap: 6px; margin-top: var(--fox-space-3); }}
  .chip {{ height: 74px; border-radius: var(--fox-radius-md); padding: 6px; display: flex; flex-direction: column; justify-content: flex-end;
    color: #fff; font-size: 11px; text-shadow: 0 1px 2px rgba(0,0,0,.35); }}
  .chip code {{ font-size: 10px; }}
  .neutrals {{ display: grid; grid-template-columns: repeat(auto-fill, minmax(150px, 1fr)); gap: var(--fox-space-2); }}
  .neutral {{ display: grid; grid-template-columns: 28px 1fr; column-gap: 8px; align-items: center; font-size: var(--fox-text-sm); }}
  .neutral .dot {{ grid-row: span 2; width: 28px; height: 28px; }}
  .dot {{ display: inline-block; width: 16px; height: 16px; border-radius: 50%; border: 1px solid var(--fox-border); vertical-align: middle; }}
  .themes {{ display: grid; gap: var(--fox-space-4); grid-template-columns: repeat(auto-fit, minmax(300px, 1fr)); }}
  table {{ border-collapse: collapse; width: 100%; font-size: var(--fox-text-sm); }}
  td {{ padding: 5px 8px 5px 0; border-bottom: 1px solid var(--fox-border); vertical-align: top; }}
  .font-card {{ border: 1px solid var(--fox-border); border-radius: var(--fox-radius-lg); padding: var(--fox-space-4); display: flex; flex-direction: column; gap: 2px; }}
  .sample {{ font-size: var(--fox-text-3xl); margin: 0 0 var(--fox-space-2); line-height: 1.1; }}
  .rules {{ display: grid; gap: var(--fox-space-4); grid-template-columns: repeat(auto-fit, minmax(300px, 1fr)); }}
  .rules ul {{ margin: var(--fox-space-2) 0 0; padding-left: 1.2em; }}
  .do h3 {{ color: var(--fox-success); }} .dont h3 {{ color: var(--fox-error); }}
  .files {{ overflow-x: auto; }}
  @media (max-width: 600px) {{ .chips {{ grid-template-columns: repeat(2, 1fr); }} .toggle {{ float: none; margin-bottom: var(--fox-space-4); }} }}
</style>
</head>
<body>
<main>
  <button class="toggle" id="toggle" type="button">Toggle light / dark</button>
  <section class="hero">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="logo/wordmark-dark.svg">
      <img id="hero" src="logo/wordmark-light.svg" alt="{T['name']}">
    </picture>
    <div class="stripe">{stripe}</div>
    <p class="lede">{T['name']} is parametric 3D CAD. Its fox is made of the colours of the app itself: every facet
      is the colour of one part of the tool set, so the logo and the toolbar read as one family.</p>
  </section>

  <h2>Name</h2>
  <p><b>{T['name']}</b>: one word, capital F and capital O. In code and files the short name is
    <code>{T['short_name']}</code> (crates <code>fox-*</code>, <code>.fox</code> designs, <code>FOX_*</code> settings).</p>

  <h2>Logo</h2>
  <div class="grid">
    <div class="panel"><div class="art on-light"><img src="logo/fox.svg" alt=""></div><h3>The fox</h3><p class="muted">Full colour on any background, light or dark.</p></div>
    <div class="panel"><div class="art on-dark"><img src="logo/fox.svg" alt=""></div><h3>On dark</h3><p class="muted">The same file; no dark variant needed.</p></div>
    <div class="panel"><div class="art on-check"><img src="logo/fox-mono-ink.svg" alt=""></div><h3>One colour</h3><p class="muted">For print, engraving, embossing and stamps.</p></div>
    <div class="panel"><div class="art on-dark"><img src="logo/fox-mono-white.svg" alt=""></div><h3>One colour, reversed</h3><p class="muted">On photos and strong colour.</p></div>
    <div class="panel"><div class="art on-light"><img src="logo/icon.svg" alt=""></div><h3>App icon</h3><p class="muted">Dark tile, the default for docks, taskbars and stores.</p></div>
    <div class="panel"><div class="art on-dark"><img src="logo/icon-light.svg" alt=""></div><h3>App icon, light</h3><p class="muted">For a light tile where the platform calls for it.</p></div>
  </div>
  <div class="rules" style="margin-top:var(--fox-space-5)">
    <div class="panel do"><h3>Do</h3><ul>
      <li>Keep clear space of at least half the fox's width around it.</li>
      <li>Use the fox at 16 px and up, and the wordmark at 100 px wide and up.</li>
      <li>Use the files here; they are exact.</li>
    </ul></div>
    <div class="panel dont"><h3>Don't</h3><ul>
      <li>Recolour, reorder or add facets, or add outlines, shadows or gradients.</li>
      <li>Stretch, rotate or crop the fox.</li>
      <li>Retype the wordmark in another font.</li>
    </ul></div>
  </div>

  <h2>Colour</h2>
  <p class="muted">Each family's <b>base</b> is the colour of its icons in the app; <b>light</b>, <b>dark</b> and <b>deep</b> are the fox's lit and shaded facets, and hover and pressed states.</p>
  <div class="grid" style="margin-top:var(--fox-space-4)">{''.join(sw)}</div>

  <h3 style="margin-top:var(--fox-space-6)">Neutrals</h3>
  <div class="neutrals" style="margin-top:var(--fox-space-3)">{neutrals}</div>

  <h3 style="margin-top:var(--fox-space-6)">Light and dark themes</h3>
  <p class="muted">Use these semantic names in new pages and apps; <code>tokens.css</code> switches them with the viewer's theme.</p>
  <div class="themes" style="margin-top:var(--fox-space-3)">{themes}</div>

  <h2>Type</h2>
  <div class="grid">{fonts}</div>
  <p class="muted" style="margin-top:var(--fox-space-3)">Inter and JetBrains Mono are free (SIL Open Font License). Roboto is the fallback.</p>

  <h2>Files</h2>
  <div class="files"><table>{file_rows}</table></div>
</main>
<script>
  const root = document.documentElement, hero = document.getElementById('hero');
  const dark = () => root.dataset.theme ? root.dataset.theme === 'dark' : matchMedia('(prefers-color-scheme: dark)').matches;
  const sync = () => {{
    const src = dark() ? 'logo/wordmark-dark.svg' : 'logo/wordmark-light.svg';
    hero.parentElement.querySelector('source').srcset = src; hero.src = src;
  }};
  document.getElementById('toggle').onclick = () => {{ root.dataset.theme = dark() ? 'light' : 'dark'; sync(); }};
</script>
</body>
</html>
"""


# ---------------------------------------------------------------- write everything


def write(rel, data):
  path = os.path.join(HERE, rel) if not rel.startswith("/") else rel
  os.makedirs(os.path.dirname(path), exist_ok=True)
  with open(path, "wb" if isinstance(data, bytes) else "w") as f:
    f.write(data)
  print("wrote", os.path.relpath(path, ROOT))


def png(svg_rel, out_rel, width):
  import resvg_py

  data = resvg_py.svg_to_bytes(svg_path=os.path.join(HERE, svg_rel), width=width)
  write(out_rel, bytes(data))


def main():
  write("logo/fox.svg", svg(W, H, mark()))
  write("logo/fox-mono-ink.svg", svg(W, H, mark(mono=LIGHT["wordmark"])))
  write("logo/fox-mono-white.svg", svg(W, H, mark(mono="#FFFFFF")))
  write("logo/icon.svg", svg(1024, 1024, tile_icon(1024, T["app"]["icon_tile_dark"], "#0B0E13")))
  write("logo/icon-light.svg", svg(1024, 1024, tile_icon(1024, T["app"]["icon_tile_light"], "#D3D9E1")))
  write("logo/wordmark.svg", wordmark())
  write("logo/wordmark-light.svg", wordmark(LIGHT["wordmark"]))
  write("logo/wordmark-dark.svg", wordmark(DARK["wordmark"]))

  write("web/tokens.css", tokens_css())
  write("web/tokens.js", tokens_js())
  write("web/tailwind.preset.js", tailwind_preset())
  write("web/site.webmanifest", manifest())
  write("web/head.html", HEAD_HTML.format(dark_bg=DARK["background"], name=T["name"], tagline=T["tagline"]))
  write("web/og-image.svg", social_card())
  shutil.copyfile(os.path.join(HERE, "logo/fox.svg"), os.path.join(HERE, "web/favicon.svg"))
  print("wrote brand/web/favicon.svg")
  write("index.html", guide())

  crate = os.path.join(ROOT, "crates", "brand", "src")
  write(os.path.join(crate, "tokens.rs"), tokens_rs())
  write(os.path.join(crate, "logo.rs"), logo_rs())
  try:
    rs = [os.path.join(crate, f) for f in ("tokens.rs", "logo.rs")]
    subprocess.run(["rustfmt", "--edition", "2024", "--config-path", os.path.join(ROOT, "rustfmt.toml"), *rs], check=True)
  except (OSError, subprocess.CalledProcessError) as e:
    print(f"rustfmt skipped ({e}); run cargo fmt", file=sys.stderr)

  for size in (32, 64, 128, 256, 512, 1024):
    png("logo/icon.svg", f"logo/png/icon-{size}.png", size)
    png("logo/icon-light.svg", f"logo/png/icon-light-{size}.png", size)
  for size in (64, 256, 1024):
    png("logo/fox.svg", f"logo/png/fox-{size}.png", size)
    png("logo/fox-mono-ink.svg", f"logo/png/fox-mono-ink-{size}.png", size)
    png("logo/fox-mono-white.svg", f"logo/png/fox-mono-white-{size}.png", size)
  for v in ("light", "dark"):
    png(f"logo/wordmark-{v}.svg", f"logo/png/wordmark-{v}-1200.png", 1200)
  png("logo/icon.svg", "web/apple-touch-icon.png", 180)
  png("logo/icon.svg", "web/icon-192.png", 192)
  png("logo/icon.svg", "web/icon-512.png", 512)
  png("web/og-image.svg", "web/og-image.png", 1200)
  ico_src = []
  for size in (16, 32, 48):
    out = f"web/.favicon-{size}.png"
    png("logo/fox.svg", out, size * W // H)  # the fox is taller than wide: fit its height
    ico_src.append(os.path.join(HERE, out))
  try:
    args = []
    for size, p in zip((16, 32, 48), ico_src):
      args += ["(", p, "-background", "none", "-gravity", "center", "-extent", f"{size}x{size}", ")"]
    subprocess.run(["convert", *args, os.path.join(HERE, "web/favicon.ico")], check=True)
    print("wrote brand/web/favicon.ico")
  except (OSError, subprocess.CalledProcessError) as e:
    print(f"favicon.ico skipped ({e})", file=sys.stderr)
  for p in ico_src:
    os.remove(p)


if __name__ == "__main__":
  main()
