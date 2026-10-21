# Parity run, 2026-10-21 (second pass): sketch tools and Modify commands

Same method as the morning report (`2026-10-21.md`), on the rebuilt `main` (`2356d5a`, which
includes the Manufacture work): Fusion and FissionOxide driven in turn, each window placed left
of the Claude overlay. The session this time went deeper than the first pass: a sketch on the
body's top face with a circle, a diameter edit, a line, Trim, Offset, a Horizontal constraint and
a circumscribed polygon; then Chamfer, Shell, Press Pull, Move/Copy and Hole on the body.

## Headline

| | Count |
|---|---|
| New differences recorded | 15 |
| P1 | 2 |
| P2 | 13 |
| New batches | 3 (`map-wave5.json`) |
| Retracted on the spot | 1 (face shading under a sketch is flat in Fusion too) |

## What matched

- Create Sketch on a face: the face highlights, the view looks at it, the grid draws on it.
- Circle with the heads-up diameter box, `10 Enter`; double-clicking the dimension opens an
  editor with the value selected and Enter commits (Fusion's box adds units and the parameter
  name, see below).
- Line heads-up boxes, Trim with a hover highlight and the coincident glyphs it leaves,
  Offset with a live preview and a typed distance, Horizontal/Vertical on a free line.
- Chamfer, Shell, Press Pull, Move and Hole all open with a live preview and the right
  selection filter; Shell's inside/outside direction, Move's triad and Hole's diagram are there.

## P1

### 1. The polygon tool does nothing after the centre click
- **Fusion:** a polygon and its circle follow the cursor, with radius and sides heads-up boxes,
  Feature Options in the palette and the prompt "Place point on polygon"; `.3 Tab 5 Enter`
  makes a pentagon with a radius dimension.
- **FissionOxide:** only a rubber line and a coordinate readout; typing is ignored; Enter and
  Escape leave nothing.
- **Start here:** [viewport.rs:950](../../crates/ui/src/viewport.rs) (`Tool::Polygon if n == 2`),
  the preview at line 2208, the hint at 2542, [menus.rs:562](../../crates/ui/src/menus.rs).

### 2. Horizontal refuses a line that touches a circle
- **Fusion:** applies the constraint and rotates the line about its coincident end.
- **FissionOxide:** "that constraint conflicts with the others (or over-constrains the sketch)"
  on a trimmed line with one end coincident on the circle and 6 degrees of freedom free. The same
  constraint on a free line works.
- **Start here:** [commands.rs:3244](../../crates/engine/src/commands.rs) and the solver it calls
  in `crates/sketch`.

## P2

- **Diameter dimension** renders "□12" (no ⌀ glyph in the font) as a diagonal inside the
  circle; Fusion draws "⌀0.40" horizontally through the centre.
  [viewport.rs:2059](../../crates/ui/src/viewport.rs).
- **Line heads-up boxes** show "14.76" and "336.7876°"; Fusion "0.492 in" and "336.8 deg".
  [viewport.rs:1053](../../crates/ui/src/viewport.rs).
- **Dimension editor** shows "10"; Fusion "0.4 in" with a ⋮ menu and the tooltip "d7 : 0.4 in".
  [viewport.rs:1575](../../crates/ui/src/viewport.rs).
- **Offset** leaves no offset dimension; Fusion adds one.
  [dialogs.rs:1340](../../crates/ui/src/dialogs.rs).
- **Trim hover** is red; Fusion's is dark blue with "Select curve section to trim".
- **Sketch CREATE** has an extra "Trace Image" row (Fusion also has Midpoint Line, so that one
  is fine). [menus.rs:600](../../crates/ui/src/menus.rs).
- **The Data Panel reopened by itself** during the sketch session.
  [lib.rs:306](../../crates/ui/src/lib.rs) has a `(None, None) => app.data.toggle()` arm worth
  checking first.
- **OFFSET dialog** lacks Chain Selection, Direction and Flip.
- **CHAMFER** lacks the selection-set table, Tangent Chain, Corner Type and the manipulator;
  Fusion's pick of one vertical edge took its tangent chain (2 edges).
  [dialogs.rs:728](../../crates/ui/src/dialogs.rs).
- **SHELL** lacks the Shell Type icon pair and the floating thickness box, and shows a greyed
  "Round corners" row Fusion does not have. [dialogs.rs:729](../../crates/ui/src/dialogs.rs).
- **PRESS PULL on a face** stays PRESS PULL; Fusion retitles it OFFSET FACE with an Offset
  Type row. [dialogs.rs:133](../../crates/ui/src/dialogs.rs).
- **MOVE / ROTATE** has a text Move Type dropdown and no Set Pivot; Fusion's MOVE/COPY has five
  icons and Set Pivot. [dialogs.rs:126](../../crates/ui/src/dialogs.rs).
- **HOLE** uses text dropdowns and radios, one References row and Flip/Preview checkboxes;
  Fusion uses icon rows under a Shape Settings group, two Reference rows and editable diagram
  fields, and adds no sketch row to the browser (FissionOxide adds "Sketch3").
  [dialogs.rs:768](../../crates/ui/src/dialogs.rs).

## Batches

| Batch | Items | Main files |
|---|---|---|
| ui-sketch-tools-2 | 8 | viewport.rs, menus.rs, lib.rs |
| ui-sketch-solver | 1 | engine commands.rs, crates/sketch |
| ui-feature-dialogs-2 | 6 | dialogs.rs |

ui-feature-dialogs-2 overlaps ui-feature-manipulators from the morning wave on the floating
value box; run it after that one merges, or have it reuse the same helper.

## Not covered this pass

Mirror, sketch fillet, slots, splines, text, constraints other than Horizontal, Combine (needs a
second body), Scale, Draft, Split Body, patterns, construction planes, and everything outside
the Design workspace.
