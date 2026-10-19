# Plan: Fusion parity, then a fabrication suite

FissionOxide's goal is a parametric CAD app close enough to Fusion 360 that people can switch, which
then grows into a suite for hobby and small-shop fabrication: lasers, plasma tables, waterjets,
press brakes and CNC routers, supported by default rather than through paid extensions.

This plan orders the work. [ROADMAP.md](../ROADMAP.md) remains the detailed feature checklist;
this file says what comes first and how we know a phase is done.

## Where we stand (October 2026)

FissionOxide covers most of Fusion's design workspace in breadth (sketching, solids, surfaces, sheet
metal, assemblies, drawings, milling CAM, MCP), but each area is shallow: about 50k lines and one
scenario test per feature. The closest open-source project, SolidCraft (Rust, MIT/Apache, about
400k lines), is the opposite: deeper and much better tested, but on the young `truck` kernel, so
it has rewritten fillets, shells, offsets and STEP itself and still documents limits there. It has
no drawings, CAM or fabrication output.

Our advantages are OpenCASCADE (industrial fillets, shells, offsets, booleans, STEP and hidden-line
removal) and a head start on the manufacturing side. Our gaps are verification, stable references,
the viewport and interaction polish.

Parity does not mean all of Fusion. Simulation, generative design, rendering and T-spline forms are
out of scope until the fabrication suite ships. The target is the modelling loop people use every
day, plus everything a fabrication workflow sits on.

## Phase 0: prove what works

Before deepening anything, measure it.

1. **Green build and CI** on a fresh machine, including the OpenCASCADE build.
2. **Reference parts.** `tests/reference/` holds parts with independently known answers
   (closed-form volume, area, centre of mass, bounding box, face count). The first 83 are adapted
   from SolidCraft's oracle (see `tests/reference/README.md`). Every part is also checked for a
   valid solid and a STEP round trip. A baseline file ratchets: a part that passes must keep
   passing, and CI fails on any regression.
3. **Edit checks.** Parts carry variants ("set `width` to 80, expect this volume") so editing an
   early feature is tested, not only building.
4. **Robustness fuzzing.** Seeded random feature sequences, checked for valid solids and
   consistent volumes after every step.

*Done when* every reference part has a FissionOxide script, the pass rate is published in
`docs/reference-parts.md`, and CI runs the suite.

## Phase 1: the core modelling loop at Fusion quality

1. **Stable references.** Faces and edges are re-found after each rebuild by geometric signature.
   Replace this with OpenCASCADE's modification history (`BRepTools_History` and the
   generated/modified maps of each algorithm), keeping the signature as a fallback. This is
   what keeps a fillet attached when an early sketch changes.
2. **GPU viewport** on wgpu (egui already supports it), with real-time orbiting on large parts,
   edges, silhouettes and section views. The CPU rasterizer stays for headless rendering.
3. **Editing feel:** live preview during every feature dialog, drag manipulators (distance,
   angle), inline sketch dimensions, a command palette (search all commands), and a space mouse.
4. **Assembly joints solved,** not just placed: closed loops, contact, joint limits enforced while
   dragging.
5. **STEP with colours, names and assembly structure** (OpenCASCADE XCAF).
6. **Agent tooling:** MCP `batch` with rollback on failure, measurement tools and modelling guides
   for agents.

*Done when* the reference suite passes above 95%, every variant edit passes, and a set of
recorded Fusion tutorials can be followed step by step.

## Phase 2: sheet metal and drawings that shops accept

1. **Flat-pattern DXF that cutters accept as it is:** true arcs and circles, splines where needed,
   layers by role (cut, bend up, bend down, etch), bend notes, one file per body. Validate it
   against shop rules: no duplicate or open contours, a minimum feature size, and holes at a
   safe distance from bends.
2. **Material and bend tables:** K-factor and bend-deduction tables per material and thickness,
   a die-opening calculator, and bend tables exported as CSV.
3. **Sheet metal features:** lofted flanges, corner closure, and the flat pattern as an
   editable sketch.
4. **G2/G3 arcs** in CAM output. *Done:* the posts fit arcs to runs of short moves.

*Done when* the sheet metal reference parts pass and their DXFs pass the validator.

*Started:* `sheet.check` (and "Ready to cut" in the Flat Pattern dialog) checks a flat pattern
against common shop rules: hole size, material between holes and edges, holes near bends, sheet
size. Shop and service presets with their published limits come next.

## Phase 3: the fabrication suite

First milestone: **sheet-metal part → flat pattern → nested sheet → DXF or machine G-code.**

1. **2D cutting operations** for laser, plasma and waterjet: kerf compensation with true arcs
   (`cavalier_contours`), lead-ins and lead-outs on the scrap side, pierce settings, inside
   contours cut first, micro-joints (tabs), and shortest-path ordering between contours.
2. **Machine profiles and post processors,** each with a template: GRBL and FluidNC lasers (M4
   power that scales with speed), plasma with torch height control (Langmuir FireControl,
   LinuxCNC QtPlasmaC), Wazer waterjet G-code, and R12 DXF for OMAX. Real rapid moves and
   unlimited tools, which Fusion's personal licence removes.
3. **Nesting** of parts on sheets with `jagua-rs` / `sparrow`, keeping track of remnants.
   Fusion charges extra for this.
4. **Press brakes:** bend order with collision checks, backgauge positions, and a printable
   bend sheet.
5. **CNC routers:** 2.5D work (pocket, profile, drill, V-carve) with GRBL, Carbide Motion and Mach
   posts.
6. **Ruida controllers** (most CO2 lasers), then direct machine connections.

3D printing stays with existing slicers: export 3MF and hand off to the slicer.

## Status

| Phase | Item | State |
|---|---|---|
| 0 | Green build and CI | done: all 221 workspace tests pass on a fresh OpenCASCADE build |
| 0 | Reference parts harness | done: **77 of 83 pass**, see [reference-parts.md](reference-parts.md) |
| 0 | Edit checks | done: **44 of 44** checkable parts rebuild correctly when their first extrusion grows |
| 0 | Robustness fuzzing | done: 400 seeds (about 2,000 accepted steps) clean; it found an inside-out chamfer and a crash in OCCT's fillet, both fixed. CI runs 40 seeds |
| 1 | Stable references | started: split edges and split faces keep their features on every piece, and a sketch stays on its face when an inserted feature cuts a gap under its pick point (both used to jump silently to a look-alike); `tests/edits.rs` covers these. Next: OCCT modification history |

### Reference part failures to fix

Wrong results:

- **42** Where two patterned holes are exactly tangent, the plate has zero thickness along a line
  and each hole wall stays split in two there (the edge joins four faces, so faces can't be
  merged across it). Volume is right; low priority.
- **69, 75** Sheet metal corners. Not bugs: these expected values follow SolidCraft's own corner
  convention (walls stretched until their inner corners are a gap apart, open relief), which their
  notes say. What matters is Fusion's corner and relief behaviour, which needs references measured
  against Fusion before these cases mean anything.

Missing options:

- **78** A variable fillet whose radius runs down to zero (OpenCASCADE needs a radius above zero).
- **80, 81** Fillet radius that follows the room available, and a radius given as B-spline
  control values: SolidCraft options that Fusion doesn't have. Fusion's own variable fillet
  (radii at points along the edge) is the version worth adding.
