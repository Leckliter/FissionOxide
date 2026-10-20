# FissionOxide brand kit

Everything that carries the FissionOxide brand comes from this folder: the desktop app, the
website, and any app that comes later. Open [`index.html`](index.html) for the visual guide
(logo, colours, type, rules and every file).

## How it fits together

```
tokens.json  ─┐                       ┌─ logo/        SVG + PNG: fox, one-colour fox, icons, wordmarks
              ├──  python3 build.py  ─┼─ web/         tokens.css, tokens.js, Tailwind preset, favicons, social card
logo/fox.json ┘                       ├─ index.html   the brand guide
                                      ├─ windows/     the app icon (.ico) and the installer's wizard images
                                      └─ crates/brand fox-brand: the same tokens and logo for Rust apps
```

Edit only the two sources, run `python3 brand/build.py`, and commit what it writes. Nothing in
`logo/`, `web/`, `windows/`, `index.html` or `crates/brand/src/{tokens,logo}.rs` is edited by hand.

The build needs Python 3.9+, `pip install fonttools resvg-py`, ImageMagick (for `favicon.ico` and `windows/`),
`rustfmt`, and the Inter font family (set `FOX_FONT_DIR` if it isn't in
`/usr/share/fonts/opentype/inter`).

## The colours mean something

The fox is made of the app's colour families, so the logo and the toolbar read as one system:

| Family | In the app                            | On the fox                  |
| ------ | ------------------------------------- | --------------------------- |
| Blue   | Solid modelling; the primary accent   | Forehead, eye mask, chin    |
| Orange | Surface modelling                     | Right cheek and muzzle      |
| Amber  | Construction geometry                 | Left cheek and muzzle       |
| Green  | Inspecting, measuring, selecting      | Left jaw                    |
| Red    | Delete and stop                       | Right ear and jaw           |
| Purple | Assemblies, joints, highlights        | Left ear                    |
| Teal   | Sketching                             | —                           |
| Gold   | Mesh bodies                           | —                           |
| Coral  | Secondary warm accent                 | —                           |

Each family has four shades: `base` (icons and accents), `light`, `dark` and `deep` (the fox's
lit and shaded facets, hover and pressed states). Change a family in `tokens.json` and the logo,
the website and the app's icons all follow.

## Using it

**A website.** Serve `brand/web/` (for example at `/brand/`), paste
[`web/head.html`](web/head.html) into `<head>` for the favicon, manifest, social card and fonts,
and style with the CSS variables:

```css
body { background: var(--fox-background); color: var(--fox-text); font-family: var(--fox-font-body); }
.button { background: var(--fox-accent); color: var(--fox-accent-text); border-radius: var(--fox-radius-md); }
.sketch-badge { background: var(--fox-teal); }
```

The semantic colours (`--fox-background`, `--fox-text`, `--fox-accent` ...) follow the viewer's
light or dark setting; `data-theme="light"` or `"dark"` on `<html>` forces one. Family colours
are `--fox-blue`, `--fox-blue-light`, `--fox-blue-dark`, `--fox-blue-deep` and so on.

With Tailwind, add the preset (`presets: [require('./brand/web/tailwind.preset.js')]`) for
`bg-fox-blue`, `text-fox-text`, `font-display`, `rounded-fox-md`. From JavaScript,
`import tokens from './brand/web/tokens.js'`.

**Show the logo.** `logo/wordmark.svg` switches its text colour with the viewer's theme; where
that doesn't work (some email clients, image tags in GitHub), use `wordmark-light.svg` on light
backgrounds and `wordmark-dark.svg` on dark ones, or a `<picture>` with both. The fox itself
works on either.

**Animate it.** `logo/motion/` holds animated SVGs built from the same facets and wordmark:

| File                 | What it does                                                                    |
| -------------------- | ------------------------------------------------------------------------------- |
| `splash.svg`         | Start-up, plays once (~3 s): shards burst from a nucleus into the fox, "FOX" pops up from the name's own F, O and x, then opens out to FissionOxide. Ends on `wordmark.svg` exactly. |
| `loader-explode.svg` | Loading loop (2.4 s): the facets drift apart like an exploded assembly and snap back. |
| `loader-ripple.svg`  | Loading loop (1.6 s): a glow pulses from the nose and a wave runs out to the ear tips. |

They animate with CSS inside the SVG, so they play in a browser or webview (`<img>`, `<object>`
or inline) with no script. The splash's name follows the light or dark theme like `wordmark.svg`.
With reduced motion turned on, the splash shows the finished logo and the loaders slow down.
Native renderers such as resvg draw only a still frame; to animate the fox in the desktop app,
tween `logo::FACETS` the same way (each facet is one translate, rotate and scale from the
nucleus at (300, 400)).

**A Rust app.** Depend on `fox-brand` (no dependencies of its own):

```rust
use fox_brand::{BLUE, app, logo};
let accent = BLUE.base;                  // Rgb(30, 136, 229)
let hex = accent.hex();                  // "#1E88E5"
for facet in logo::FACETS { /* paint facet.points in facet.color */ }
```

The desktop app's theme (`crates/ui/src/theme.rs`) and icon colours read from it, and
`crates/ui/src/logo.rs` paints the fox from `logo::FACETS`.

**Anything else** (a mobile app, a design tool, slides): read `web/tokens.js` or the resolved
values in `index.html`; take logos from `logo/` and `logo/png/`.

## Rules

- The name is **FissionOxide**: one word, capital F and O. In code it is `fox`.
- Keep clear space of half the fox's width around it; use it at 16 px and up, the wordmark at
  100 px wide and up.
- Don't recolour, reorder or add facets, add outlines, shadows or gradients, or stretch, rotate
  or crop the fox. Don't retype the wordmark.
- Type: Inter Display for the wordmark and headlines, Inter for text, JetBrains Mono for code.
  All are free (SIL Open Font License); Roboto is the fallback.
