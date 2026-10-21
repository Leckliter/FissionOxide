# LumenArt Tool Suite: porting spec

What the LumenArt Fusion 360 add-in (`LumenArtToolSuite`, 2026.01.21) does, command by command,
written down so it can be rebuilt as a FissionOxide plugin without the Python source. The plugin
system itself is planned in [PLUGINS.md](PLUGINS.md).

All sizes are inches unless marked. Fusion's API works in cm; several tools mix the two (noted as
bugs below). Material is 0.063" (16 ga aluminium) throughout.

## The sign workflow

1. **Prepare.** Import artwork (SVG/DXF; the Illustrator script `MergeSingleOutline.jsx` first
   outlines strokes and unions everything into one cut silhouette). Clean it (Line Cleaner,
   Sketch Cleaner) and size it (Auto Scale).
2. **Hardware.** Drop standard cutouts from the LumenArt Library into the sketch: keyholes,
   studs, E12 socket, LED strip slot, part identifier, tab heads and slots.
3. **Layers.** Create Component, once per layer: selected profiles → component `NN[ - name]`
   holding one 0.063" body. The user then stacks the layers at different Z (standoffs).
4. **Tabs.** Quick Tab clicks 0.25" tab stubs onto layer edges; Auto Flange stretches each stub
   over the gap to the layer below and marks its bend line; Auto Tab cuts receiver slots and adds
   tab heads (Slot Projector + Tab Builder).
5. **Finish and ship.** Prismatic Colors makes powder-coat appearances; Cut File Collector lays
   every flat pattern out in one `Cut File` sketch; SCS Validator checks it for SendCutSend;
   Auto Export writes renders, part data, sketch data, dimensions and the DXF in one go.

## Core features and the add-in

General tools become part of FissionOxide; only the sign-specific ones stay in the LumenArt
add-in. With the add-in on, its **LumenArt** tab still lists every step, built-in ones included
(see [PLUGINS.md](PLUGINS.md#an-add-ins-toolbar-tab-can-hold-built-in-commands)), with
grey-and-yellow LumenArt icons numbered in the order they are used.

| LumenArt tool | Where it goes |
|---|---|
| Line Cleaner | Core: Sketch → Remove Duplicates |
| Sketch Cleaner | Core: Sketch → Clean Up (break projection links, delete constraints, splines to arcs/lines, stray points) |
| Auto Scale | Core: Scale to Size / Fit in Box in sketch Scale |
| Entity Inspector | Core: Inspect → Properties |
| Dimensions Capture | Core: overall size in Properties and Measure |
| SCS Validator | Core: cut-service check with shop profiles (SendCutSend, OSH Cut, Ponoko, your own) on `sheet.check_dxf` |
| Sketch Analyzer (area, cut length, pierces, pieces) | Core: cut estimate; LumenArt counts tabs |
| Cut File Collector | Core: flat patterns of all parts on one sheet / one DXF |
| Render Product / Components / Exploded | Core: exploded view and batch render (one image per component); LumenArt names the files |
| LumenArt Library | Core: sketch library (insert DXF/SVG/.fox parts where you click); LumenArt ships its parts as a library pack |
| Create Component | Core: new component option on Extrude; LumenArt numbers the layers `01`, `02`, … and stacks them |
| Quick Tab, Auto Flange, Slot Projector, Tab Builder, Auto Tab | Add-in: the layered-sign tab system, as live timeline features. A generic core Tab and Slot (as in SolidWorks) may follow. |
| Auto Export | Add-in: LumenArt's folder layout, product label and master export |
| Prismatic Colors | Example plugin (needs `network`) |

## Toolbar of the original

Tab **LumenArt**, panels in this order:

| Panel     | Commands |
|-----------|----------|
| PREPARE   | Line Cleaner, Sketch Cleaner, Auto Scale |
| TABS      | Quick Tab, Slot Projector, Tab Builder |
| EXPORT    | Render Product, Render Components, Render Exploded |
| AUTOMATE  | Create Component, Auto Flange, Auto Tab, Cut File Collector, Auto Export |
| ANALYZE   | Sketch Analyzer, Dimensions Capture, Entity Inspector, SCS Validator |
| MATERIALS | Prismatic Colors, LumenArt Library |

## Shared conventions

- **Output root:** the user's Downloads folder, `Downloads/<Label>/` with `Photos/`, `Parts/`,
  `Sketch/`, `File/`; the cut DXF goes in the root. In FissionOxide this becomes the output
  folder the user picks (the `write-files` permission).
- **Label:** first match of `[A-Za-z]{2,}-\d{3,6}(\.\d+)?` (upper-cased) in the document name,
  then the root component name, then sketch names; else a 4-digit token; else the sanitised
  document name (`<>:"/\|?*` → space). The original computes this two different ways; use one.
- **Component names:** `NN` or `NN - name`; the two-digit prefix is the layer/part number.
- **Attributes** (group `LumenArt`): Create Component writes `Sequence`, `BaseSketchName`,
  `ThicknessIn`, `CreatedAt` on the component and body; Slot Projector writes
  `SlotProjectorVersion`, `CutDepthMode`, `TabComponent` on its cut; Tab Builder writes `TabID`,
  `TabSizeIn`, `SourceComponent`, `CenterWorldXYZ`, `LongAxisWorld`, `Version`. Nothing reads them
  today: tools find each other's geometry by size. The port should tag and find by attribute.
- **Magic sizes:** tab stub end face 0.25 × 0.063; tab head 0.25 shoulder, 0.110 × 0.0675 neck,
  0.170 × 0.075 head; receiver slot 0.200 × 0.085; flange allowance +0.025 (−0.003 clearance,
  +0.028 bend).

## Commands

### Line Cleaner (PREPARE)
Inputs: sketch; tolerance Exact 1e-5 / Tight 1e-4 / Standard 1e-3 (default) / Loose 1e-2 /
Custom (value, default 0.001"); include construction (off); preview (on).
Removes duplicates, keeping the first of each pair, same curve type only:
- lines: both ends match (either direction); a shared single endpoint is never a duplicate
  (corner); otherwise overlapping if an endpoint of each lies within tol of the other line;
- arcs: same centre, radius, and end points (either direction); circles: centre and radius;
- splines: same fit points in order.
Points match per axis within tol. Preview shows counts by type and asks before deleting.

### Sketch Cleaner (PREPARE)
Inputs (defaults): sketch (empty = all in component); rebuild every curve (on); detach projected
geometry (on); delete constraints referencing projections (on); delete all constraints (on);
purge reference points (on); delete unattached points (on); construction → normal (off); merge
gaps (off, 0.001"); explode splines to polylines (off, 64 segments, 4–512); export DXF (off,
name). Order: rebuild/detach → constraints → construction → points → merge gaps → explode →
export. Makes imported or projected sketches DXF-clean.

### Auto Scale (PREPARE)
Needs a sketch in edit. Actions: Scale to Width (36"), Scale to Height (18"), Fit in Box (presets
19.5 × 19.5 "20x20 box", 36 × 10, Custom; Margin Mode: box = outer (20 × 20) − 2 × padding
(0.25)). Anchor: centroid of samples (default) or origin. Optionally only selected entities.
Size = axis-aligned box of points sampled on the curves. Fit in Box tries rotations 0–90° in
1° steps and keeps the best factor `min(boxW/w, boxH/h)` — but never applies the rotation, so the
result can overflow at 0°. Scale uniformly about the anchor (`sketch.transform`).

### Create Component (AUTOMATE)
Inputs: profiles (1+, one sketch); name (optional); thickness (0.063"). Next number = smallest
unused `NN` among top-level components. New component `NN` / `NN - name`, one extrude of all the
profiles by the thickness along the sketch normal, new body. Writes the `LumenArt` attributes,
hides the new component, keeps the sketch visible, and reopens for the next layer.

### Quick Tab (TABS)
Interactive: walks the solid bodies sorted by (component, body); click an edge to place a tab;
"Next Body / Finish". At the clicked point P on the edge, tangent T, outward normal N:
rectangle from P ± 0.125·T (clamped to the edge ends), shifted 0.005 outward, 0.125 deep inward;
joined to the body, 0.063 thick. Result: a stub 0.005 proud of the edge, 0.25 long, whose end
face is 0.25 × 0.063. (Original takes N from the edge's first face and assumes a plate parallel
to XY.)

### Auto Flange (AUTOMATE)
Finds every planar 4-edge face 0.25 × 0.063 (±0.0001). For each: the component below is the
highest body whose top is below the face centre and which contains it in plan (port: cast a ray
down). gap = centre Z − top Z. Extrude the face outward by gap + 0.025, joined. Before that, the
bottom 0.25 edge is drawn as a construction line in the component's `Bend Lines` sketch: where
the tab bends 90° down to reach the layer below. No bend radius is modelled.

### Slot Projector (TABS)
Detects 0.25 × 0.063 (±0.010) planar faces whose 0.063 side is horizontal (tabs on parts standing
up). On each: a 0.200 × 0.085 rectangle centred on the face, long side along the face's long
side, extrude-cut 0.100 along the face normal. Inputs exist for depth mode (to receiver / fixed),
direction and dry run, but only dry run works; the 0.1" cut is hard-coded. A receiver ray cast
(10" along ∓Z, nearest facing face) and an idempotency check are written but never called; the
port should use them.

### Tab Builder (TABS)
Faces: picked, or auto-detected flange ends (planar, 4 edges, two ≈0.25 ±0.010, two 0.02–2.0
long, not a 0.25 × 0.063 stub face; de-duplicated). For each: free edge (no other face, or not
next to the bend cylinder) vs. mount edge; import `tab_head.dxf` onto the face, rotate so the
head points from the mount edge to the free edge, shoulder midpoint on the free edge midpoint;
join-extrude profile 0 by 0.063 (flip option). Writes the TabID attributes.

### Auto Tab (AUTOMATE)
No inputs. Slot Projector over all components (0.200 × 0.085, 0.100 deep, tol 0.010), then Tab
Builder on every detected flange face (0.063). Slots first, while the stub faces still exist.

### Cut File Collector (AUTOMATE)
Input: max row width (48"). New root sketch `Cut File` (`Cut File 02`, … if taken). For every
component with a sheet-metal body: flat pattern (made from the largest planar face if missing),
its outline copied into the sketch with its box's bottom-left at the cursor, fixed. Rows of
parts 0.25 apart, wrapping at the max width, growing downward. (Bug: the wrap test runs after
placing the part that overflows.) Then: right-click → Save as DXF.

### Sketch Analyzer (ANALYZE)
Inputs: sketch (pre-selects `Cut File`); main piece min area (0.75 in²); TXT/CSV/JSON/DXF and
debug checkboxes. Per profile: area, box, edge count. Main piece = area ≥ threshold, everything
else a hole. Tabs = chains of three lines 0.063, 0.17, 0.063 (±0.02), joined end to start within
0.01 cm. Cut length = all non-construction curve lengths. Material and thickness from the
component. Main piece count is replaced by the number of components when there are any (port:
count from geometry). Writes `Sketch/<Label> - Sketch Data.{txt,csv,json}` (JSON root
`lumenart_sketch_analyzer` v1.3.0: material, settings, results {total_area, total_cut_length,
main_pieces, tabs, holes_cutouts, total_profiles}, details) and `<doc name>.dxf`.

### Dimensions Capture (ANALYZE)
Overall width, height, thickness of the visible model: in the frame of the largest planar face
(width and height across it, thickness along its normal), else sorted box extents. Writes
`File/<Label> - Dimensions.{txt,json,csv}` (JSON `lumenart_dimensions` v1.2.0: width_in,
height_in, thickness_in, mode planar|bbox, bbox_cm).

### Entity Inspector (ANALYZE)
Debug tool: dumps type, id, and geometry (area, length, box, normal, centre, radius, volume, …)
of the selected faces, edges, vertices, bodies, components, sketch curves and points to
`<doc>_EntityDump/<doc>_entities_<timestamp>.{json,txt}`.

### SCS Validator (ANALYZE)
Inputs: cut method Laser (default) / Waterjet / CNC Router; thickness (0.125"); export JSON.
Minimum feature: laser 0.5 × thickness, waterjet 0.070, router 0.125. Per visible sketch:
text entities → error (convert to outlines); size < 0.1 cm or > 1000 cm → warning; curves in no
profile → error (open contour); overlapping profile boxes → error (port: don't flag holes inside a
part); smallest circle, arc or line under the minimum → warning; more than one profile → info.
Report PASS / WARNINGS / ERRORS; optional JSON `scs_validation` v1.0.0. FissionOxide's
`sheet.check_dxf` already covers most of this and more.

### Render Product / Components / Exploded (EXPORT)
All 1080 × 1080 PNG, orthographic, shaded, sketches hidden, everything shown, fit.
- **Product:** looking straight down −Z. `Photos/<Label> - Render Product.png`.
- **Components:** each part on its own (others hidden), camera 15° around, 20° up. Image
  `Parts/<Label>.<NN>.png` (or `<Label> - <name>.png`). Manifest `Parts/<Label> - Part Data.
  {json,csv,txt}`: part number, identifier NN, name, area in² (for powder coat), appearance,
  path.
- **Exploded:** parts sorted by Z, the i-th lifted i × Z-range × 20 / N; camera 45° around, 30°
  up, 3.5 × size; restored afterwards. `Photos/<Label> - Render Exploded.png`.

### Auto Export (AUTOMATE)
No inputs. Runs Render Product, Render Components, Render Exploded, Sketch Analyzer (on `Cut
File`, else the first sketch) with the DXF, and Dimensions Capture, then writes
`<Label> - Master Export.json` (`lumenart_master_export` v1.0.0) pointing at every output.

### Prismatic Colors (MATERIALS)
Input: a prismaticpowders.com product URL. Product code `[A-Z]+-\d+` from the URL; downloads the
page for the name and the swatch colour (first non-UI hex/rgb on the page, else a name → colour
table, else #808080); finish gloss/satin/matte from the name or gloss units (≥85 gloss, 30–84
satin, <30 matte → roughness 0.1 / 0.35 / 0.7). Creates appearance `<Name> - <Code>`; doesn't
apply it. Needs the `network` permission.

### LumenArt Library (MATERIALS)
Places library parts into the active sketch. The code reads `.f3d` files that don't ship; the
real library is DXF (inches, centred on the origin):

| Part | Geometry |
|------|----------|
| `10.32_stud` | Ø0.189 and Ø0.562 circles (#10-32 weld stud) |
| `stud_pattern` | 3 × 3 grid at 0.5" pitch of Ø0.189/0.271/0.375 circles, 1" square with diagonals |
| `8_inch_keyholes`, `16_inch_keyholes` | two 0.2"-wide keyholes at ±4" / ±8", centre line, Ø0.25 centre mark |
| `back_bracing` | 1.25 × 0.30 slot |
| `e12_socket` | Ø1.15 hole with an anti-rotation key |
| `light_strip` | 0.984 × 0.315 (25 × 8 mm) LED strip slot |
| `part_identifier` | 0.21 × 0.35 arrow |
| `tab_head`, `tab_slot` | the tab head and 0.200 × 0.085 slot |

The port should let the user pick where the part goes (`sketch.insert` with a transform).

## Bugs in the original to fix, not copy

1. Line and Sketch Cleaner compare inch tolerances with cm coordinates.
2. Sketch Cleaner's merge gaps moves points the wrong way (doubles the gap).
3. Auto Scale only moves line endpoints (arcs, circles and splines don't scale), its angle step
   defaults to 1 rad, and Fit in Box doesn't apply the rotation it chose.
4. Quick Tab takes the normal from the edge's first face and only works on plates parallel to XY.
5. Slot Projector ignores depth mode, direction, receiver and existing slots.
6. Cut File Collector wraps rows one part late and its clean-up deletes the user's sketches named
   `Sketch…`.
7. Sketch Analyzer's tab chains are direction-dependent, its JSON swaps width and height, and its
   main piece count is the component count.
8. SCS Validator flags holes as overlapping profiles.
9. Auto Export's part images are named differently from Render Components'.
10. Render Exploded moves nested components twice.
11. LumenArt Library reads `.f3d` files that don't exist; Slot Projector and Auto Tab look for the
    tab id `LumenArt` instead of `LumenArtTab`.
