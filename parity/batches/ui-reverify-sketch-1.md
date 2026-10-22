# Parity batch: ui-reverify-sketch-1

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Found on 2026-10-22 by the re-verification sweep (verifier 1) re-checking the sketch area's
`fixing-merged` keys in the rebuilt app (`main` at `ef887079`). Three are reopened keys (the fix
landed but part of Fusion's behaviour is still missing) and three are new rows seen along the way.
The rows are already in `parity/ui-features.jsonl` (do not run `ledger_from_batches.py` for this
batch); read each row's `history` before starting.

## Window-selected sketch geometry cannot be deleted (P1)
- key: `ui:sketch:window-delete-error`
- Fusion: a left-to-right window box over sketch curves, points and dimensions selects them, and Delete removes them (Fusion's side was not re-observed this session; it is Fusion's ordinary Delete).
- FissionOxide: in a sketch holding typed-dimension rectangles, circles, polygons, slots, an ellipse and an arc slot, a window box over everything followed by Delete printed "no point Point(d2) in this sketch" in red at the lower left and deleted nothing. Ctrl+A in the sketch selected nothing. A crossing box over one plain line deletes correctly.
- Start here: the error is `SketchError` at `crates/sketch/src/lib.rs:527`; the delete of a mixed selection (points that belong to dimensions, or points already removed with their curve) should skip ids that are gone instead of failing the whole delete. Look at the sketch delete in `crates/engine/src/commands.rs` and the selection Delete path in `crates/ui/src/viewport.rs`.

## Feature Options always first in the Sketch Palette (P2)
- key: `ui:sketch:feature-options-position`
- Fusion: Feature Options is the palette's first section, above Options, for every tool that has one.
- FissionOxide: Line, Rectangle and Circle show it at the top; Chamfer and Blend Curve add it below 3D Sketch, after Options.
- Start here: the two places that draw the section, `crates/ui/src/panels.rs:2807` and `:2881`; keep one, at the top.

## Palette shrinks back after a tool (P2)
- key: `ui:sketch:palette-stale-height`
- Fusion: when a tool with Feature Options ends, the palette is Options plus Finish Sketch again.
- FissionOxide: after the first rectangle tool ended, a blank band of about 20 px stayed between 3D Sketch and Finish Sketch for the rest of the sketch.
- Start here: the Sketch Palette window in `crates/ui/src/panels.rs` near `:2807` (egui keeps the window's last size: reset it or let it auto-shrink).

## Expression tooltip names the expression (P2)
- key: `ui:sketch:hud-parameter-expression` (reopened)
- Fusion: after "d1*2" drives a rectangle side, hovering its dimension shows "d21 : d1 * 2".
- FissionOxide: the expression drives the side and the edit box opens with "d1 * 2" behind a greyed fx, but the hover tooltip reads "d3 = 1.0 mm".
- Start here: the dimension hover tooltip in `crates/ui/src/viewport.rs` (the one fixed by `ui:sketch:dimension-hover-tooltip-style`): for a dimension whose expression is not a plain number, show `name : expression`.

## Dimension box menu lists recent values (P2)
- key: `ui:sketch:dim-box-menu` (reopened)
- Fusion: the vertical-ellipsis button of a dimension's edit box lists recent values ("0.2815284896 in", "17.220379275 deg", "0.5 in", "0.00 in"), a separator and Measure; picking a value fills the box.
- FissionOxide: the menu holds only Measure (with its ruler icon).
- Start here: the menu at `crates/ui/src/viewport.rs:2856`; keep a short list of values typed into heads-up and dimension boxes in the session.

## 3D Sketch hides the grid while a tool runs (P2)
- key: `ui:sketch:3d-sketch-manipulator` (reopened)
- Fusion: with 3D Sketch on, the Line tool shows the 3D manipulator at its start point and the sketch grid hides while the tool runs.
- FissionOxide: the manipulator (X/Y arrows and plane square) shows, but the sketch grid stays visible.
- Start here: `crates/ui/src/viewport.rs:970` already computes a hidden grid for `palette.sketch_3d && tool_3d`; check that the sketch grid (not only the 3D layout grid) reads it, and that it holds once the first point is placed.
