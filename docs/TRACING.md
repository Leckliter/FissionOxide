# Image tracing

Status: pieces 1–5 below are built (`crates/trace`, `crates/exchange/src/svg.rs`,
`crates/engine/src/trace.rs`, `crates/ui/src/trace.rs`; centre lines in
`crates/trace/src/centerline.rs`, constraint inference in `crates/sketch/src/infer.rs`, canvases
in `crates/engine/src/canvas.rs` and `crates/ui/src/canvas_ui.rs`); section 6 is what's left.

Turn a photo, logo or scan into clean, closed sketch geometry inside FissionOxide — the job the
separate TracingKit tool was meant to do for Fusion 360, without the SVG round trip. In Fusion a
traced SVG has to be imported, then checked for gaps, slivers and overlaps by hand; here the
tracer writes lines, arcs and splines straight into the sketch, and the sketch's own profile
finder confirms that every shape is a closed region before you extrude, flange or cut it.

## What the user gets

1. While editing a sketch: **Insert → Trace Image…** or **Create → Trace Image…**. Opening or
   dropping a picture does the same, starting a sketch on the XY plane if none is being edited.
2. Pick a PNG/JPEG/BMP/WebP. The image appears on the sketch plane, half transparent, with the
   traced outlines drawn over it.
3. Adjust, with the preview updating as you go:
   - **Mode**: *Outline* (dark shapes on light, one colour), *Colours* (the image reduced to N
     colours; each colour is its own set of shapes and can be included or left out), or
     *Centre line* (single open lines along the middle of the strokes, for engraving).
   - **Threshold** (auto by default), **Invert**.
   - **Ignore specks** smaller than an area; **fill holes** smaller than an area.
   - **Detail**: how far the curves may stray from the pixel outline.
   - **Corners**: the angle below which a bend becomes a sharp corner rather than a curve.
   - **Curves**: lines only (polylines, best for some CAM), lines and arcs, or lines, arcs and
     splines (smoothest, fewest entities).
   - **Size**: width in model units (height follows), and where the image centre sits (taken
     from the canvas when tracing one).
   - **Add constraints** (on by default): horizontal, vertical, tangent, coincident, equal and
     concentric constraints where the traced geometry makes them unambiguous.
4. The dialog reports *N shapes (M holes), K curves, all closed* (or *N strokes, ≈ W mm wide*)
   and the constraints added — or names what was dropped.
5. OK adds the curves to the sketch as one undo step and selects the traced shapes (not their
   holes), so Extrude straight away makes the part. They are ordinary sketch geometry:
   dimension them, trim them, extrude them, flatten them into a DXF.

The same thing is a command, so the CLI, scripts and AI agents (MCP) get it too:

```json
{"cmd": "sketch.trace", "params": {"image": "logo.png", "width": 120, "mode": "outline"}}
```

## Pieces

### 1. `fox-trace` crate — the tracer (pure Rust, no OpenCASCADE)

Image in, closed contours out. No dependency on the sketch or engine crates, so it can also
become a standalone tool or a WebAssembly module later.

Pipeline:

| Step | What | Why it matters for CAD |
|------|------|------------------------|
| Load | `image` crate (PNG, JPEG, BMP, WebP); alpha composited on white; downscaled to a max side (default 2000 px) | Big phone photos stay fast |
| Pre-filter | Optional blur (box/gaussian, radius in px) | Removes JPEG noise before thresholding |
| Segment | *Outline*: grey levels, Otsu auto threshold or a given level, invert. *Colours*: k-means in Lab space to N colours (deterministic seeding), background = the colour most common along the border | Predictable, repeatable results |
| Clean | Remove regions smaller than *specks* px², fill holes smaller than *holes* px² | No dust in the cut file |
| Contours | Boundary following on pixel corners (marching squares with a fixed rule for diagonal-touching pixels, so outlines never touch or cross); outer boundaries counter-clockwise, holes clockwise | Every contour is closed by construction |
| Simplify | Split each contour at corners (angle test over a neighbourhood), then fit each run | Sharp corners stay sharp |
| Fit | Each run within *tolerance* px: one line if it fits, else an arc, else cubic Béziers (least squares, split on worst error); a whole contour that is a circle becomes one circle | Few, clean entities; real arcs and circles for CAM |
| Validate | Each contour checked for self-intersection after fitting; offending pieces refit as polylines. Last end point == first start point, exactly | Never hand the sketch a broken loop |

Output coordinates: pixels, x right, **y up**, origin at the image's bottom-left corner (so
"counter-clockwise" means what it does in the sketch).

```rust
pub struct Options { mode, invert, blur, specks, holes, tolerance, corner_angle, fit, max_side }
pub enum Mode { Outline { threshold: Option<u8> }, Colors { count: u8 } }
pub enum Fit { Lines, Arcs, Curves }
pub fn load(path) -> Result<Image, TraceError>
pub fn trace(image: &Image, options: &Options) -> Result<Trace, TraceError>

pub struct Trace { width, height, layers: Vec<Layer> }      // width/height after downscaling
pub struct Layer { color: [u8; 3], background: bool, contours: Vec<Contour> }
pub struct Contour { segs: Vec<Seg>, hole: bool }
pub enum Seg {
  Line { a, b },
  Arc { a, b, center, ccw },                                 // ccw: from a to b
  Cubic { a, c1, c2, b },
  Circle { center, radius },                                 // only ever a contour's sole segment
}
impl Trace { pub fn to_svg(&self) -> String }                // one closed <path> per contour
```

Tests use generated images: a filled square (4 lines, exact corners), a disc (one circle), a
ring (circle + hole), a rounded rectangle (lines + arcs), letters and a noisy photo-like blob
(closed, no self-intersections, area within 1 % of the pixel count), two touching diagonal
pixels (two separate contours), specks removed, holes filled, colour layers.

### 2. SVG in the exchange crate

Useful on its own (the roadmap already lists it) and for anyone who already has traced SVGs:

- **Import**: `<path>` (M L H V C S Q T A Z, relative and absolute), `<rect>` (with rounded
  corners), `<circle>`, `<ellipse>`, `<line>`, `<polyline>`, `<polygon>`, nested `<g>`
  transforms and `viewBox`/units. Quadratics become cubic splines, elliptical arcs become arcs
  (circular) or splines. Reached through `sketch.insert {from: {path: "x.svg"}}`, like DXF.
- **Export**: a sketch as SVG (one closed path per profile loop, open curves as paths), from
  `export` on a sketch, next to DXF.

### 3. Engine command: `sketch.trace`

```
{sketch?, image: path | canvas: feature, mode?: "outline"|"colors"|"centerline", threshold?,
 colors?, layers?: [index], invert?, blur?, specks?, holes?, tolerance?, corners?,
 fit?: "lines"|"arcs"|"curves", spurs?, constraints?: bool, width? | height? (expressions) | scale?,
 at?: [x, y], angle?}
→ {curves: [id], shapes, holes, layers: [{index, color, background, shapes, holes, strokes}],
   size: [w, h], pixels: [w, h], profiles, ink: [[x, y]], ink_profiles: [index], at, angle,
   strokes?, stroke_width?, constraints?: [id], constraint_counts?: {kind: n},
   constraint_warning?, warning?}
```

The trace becomes a temporary `Sketch` (as `sketch.insert` does with a DXF), and is merged with
`insert_from`, so it is one undo step and touches nothing else. Image pixels map to sketch
millimetres with the image centred on `at` (default the sketch origin), scaled to `width`
(default 100 mm) and turned by `angle`. `ink` is a point inside each traced shape and outside
its holes (found by classifying every resulting profile against the traced contours), so
`extrude {profiles: ink}` builds the shapes with their holes open. Lines share end points, arcs use the sketch's counter-clockwise convention,
cubic segments become 4-point control splines (exactly a Bézier, as text glyphs do).

`trace.info` runs the trace without changing the design and returns the counts and each
layer's colour, so the dialog can label the colour checkboxes and the CLI can preview.

### 4. Trace Image dialog (UI)

A hand-built dialog (it needs a file picker, sliders and colour swatches, which the declarative
forms don't have). It uses the existing preview machinery: each change withdraws the previous
`sketch.trace` and applies the new one; OK keeps it, Cancel withdraws it. Tracing runs in well
under 100 ms for a typical 1000 px image; larger images are downscaled first.

The image is drawn on the sketch plane under the preview (a textured quad from the plane's
corners, half transparent) so you can see how well the outline follows it.

### 5. Canvas, centre lines and constraints

**Canvas.** Insert > Canvas (`canvas.insert {image, plane, width?|height?, at?, angle?,
opacity?}`) keeps a picture on an origin or construction plane or a planar face as a timeline
feature, saved in the design. The browser lists canvases in a *Canvases* folder with a
visibility eye (`canvas.edit {visible}`); right-click offers Edit Canvas (opacity, size,
position, angle), Calibrate and Trace Canvas. Calibrate (`canvas.calibrate {feature, from, to,
distance}`) picks two points on the image and scales the canvas about the first so they end up
the typed distance apart. Trace Canvas (`sketch.trace {canvas}`) opens Trace Image on it — in
the sketch being edited, or a new sketch on the canvas's plane — and maps the image onto the
sketch where the canvas lies, at its calibrated width and angle. The sketch must lie on the
canvas's plane or a parallel one facing the same way.

**Centre lines** (`mode: "centerline"`, `fox_trace::Mode::Centerline`). For drawings and
single-line lettering to be engraved, V-carved or plotted:

| Step | What |
|------|------|
| Segment | as *Outline* (threshold, invert, specks, holes) |
| Thin | Zhang–Suen to a one-pixel skeleton, then the staircase pixels on diagonals removed (an L's inside pixel that connectivity doesn't need), so every skeleton pixel's neighbours are its path neighbours |
| Graph | free ends (one neighbour) and junctions (three or more, adjacent ones clustered) are nodes; the pixel runs between them are branches; what's left are loops (an `O`) |
| Prune | branches from a free end to a junction shorter than `spurs` (default 1.5 × the median stroke width) are thinning noise and go, shortest first, but a junction always keeps two branches, so a small `+` or `T` survives; branches left meeting two at a former junction are joined |
| Smooth and extend | a moving average over each branch (ends fixed); free ends are pushed back out along their direction to where the stroke ends (the skeleton stops half a stroke width short) |
| Fit | split at corners, then each run: a line within the tolerance, else an arc through its ends, else a least-squares cubic, else split at the worst point; a loop without corners that fits a circle is one circle |

Each `Stroke` has its segments (each starting exactly where the previous ends), `closed` and its
mean `width` in pixels (from a chamfer distance transform). Branches meeting at a junction end at
the junction's centre bit for bit, so `sketch.trace` writes them through one shared sketch point.
Centre-line curves are open: they make no profiles and the result has no `ink`; the command
reports `strokes` and the mean `stroke_width` in mm. SVG output strokes them, unfilled, at their
width.

**Constraints** (`Sketch::infer_constraints`, on by default, `constraints: false` to skip). Run on
the traced curves once they are in the sketch, so they are read in sketch coordinates (after
the trace's rotation), with the distance tolerance half the trace's own tolerance in mm:

- *horizontal / vertical*: lines within 1° of the axis;
- *tangent*: a line and an arc, or two arcs, meeting within 5° of smoothly at a point no other
  curve uses (two arcs on the same centre are concentric instead);
- *coincident*: end points closer than the tolerance that aren't already one point;
  *point on curve*: a free end within the tolerance of another line or arc's inside;
- *concentric*: arc and circle centres closer than the tolerance;
- *equal*: radii, and lengths of horizontal (or vertical) lines, within the tolerance — sorted
  runs whose spread stays inside the tolerance, chained (a = b, b = c).

Every inferred constraint is then checked by solving the sketch: if the solver can't satisfy
them all (a slightly sheared shape whose "equal" sides can't be once the rest is snapped), the
ones it names are dropped, then whole kinds, weakest first (equal, concentric, tangent, point on
curve, coincident, vertical, horizontal), so inference never leaves a sketch failing. A sketch
too big for the solver gets none and a `constraint_warning`. A sketch whose own constraints
already fail is left alone.

### 6. Later

- **Kerf and minimum-feature check**: flag details thinner than the cutter or laser kerf.

## Order of work

1. `fox-trace` with tests (in parallel with 2).
2. SVG import/export with tests.
3. `sketch.trace` and `trace.info` in the engine, with tests that check the traced sketch has the
   expected profiles (the strongest proof that everything closed).
4. The dialog, the image overlay, menu and toolbar entries, icon.
5. README, ROADMAP and an example (`examples/trace_logo.jsonl`).
6. Canvas tracing, centre lines and constraint inference, with tests in `fox-trace`
   (`tests_centerline.rs`), `fox-sketch` (`infer.rs`), the engine (`tests_trace.rs`) and the UI
   (`canvas_ui.rs`).
