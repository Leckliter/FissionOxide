# Parity batch: ui-sketch-constraint-flow

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-21 (prober 5) on an XY sketch holding two separate lines and a small circle;
FissionOxide `main` at `d99603d` (binary built after `788352e`). Fusion's CONSTRAINTS dropdown has
the same thirteen entries in the same order as FissionOxide's (Horizontal/Vertical, Coincident,
Tangent, Equal, Parallel, Perpendicular, Fix/UnFix, MidPoint, Concentric, Collinear, Symmetry,
Curvature, Polygon), so this batch is about what happens after the click.

## constraints are pick commands (P0)
- key: `ui:sketch:constraint-pick-command`
- Fusion: Every CONSTRAINTS entry starts a command: the panel's icon lights up blue, the pointer prompt reads "Select sketch objects or change constraint type" (Horizontal/Vertical: "Select sketch objects or constraint type"), the first pick draws the point filled dark blue or the curve bold blue and changes the prompt to "Select other geometries" (Symmetry: "Select second sketch element", then "Select a symmetry line"), the constraint applies on the last pick, the prompt returns to the first text and the command stays active until Escape. A pre-selection is used as the first picks. Picking another constraint icon in the ribbon changes the constraint type without leaving the command.
- FissionOxide: The entry is a one-shot action on the current selection (`apply_constraint`): with nothing selected it prints "'coincident' needs 2 point(s)" in red at the bottom-left and nothing else happens; there is no prompt, no pick mode, no icon highlight, and building the selection needs Shift+click or a window box first.
- Start here: `crates/ui/src/panels.rs:3419` (`apply_constraint`), `crates/ui/src/menus.rs:642` (`SKETCH_CONSTRAINTS` map to `Cmd::Constraint`), `crates/ui/src/viewport.rs:1034` (curve tool clicks; add a constraint pick tool with prompts like the create tools' prompts), `crates/engine/src/commands.rs:3293` (`constrain` takes points and curves in pick order).

## stale red error after a constraint applies (P2)
- key: `ui:sketch:constraint-stale-error`
- Fusion: A refused pick leaves no message; the status bar and prompt simply keep their text.
- FissionOxide: The red "'coincident' needs 2 point(s)" / "'polygon' needs 3 curve(s)" line at the bottom-left of the canvas stays after the constraint later succeeds and through the next commands until another message replaces it.
- Start here: `crates/ui/src/lib.rs:512` (`hint`) and where the red error text is painted in `crates/ui/src/panels.rs`; clear it on the next successful command or after a few seconds.

## fix and unfix (P1)
- key: `ui:sketch:fix-unfix-flow`
- Fusion: Fix/UnFix is the same pick command: picking a line turns it green (sampled about (216,231,185) on a thin line) with no lock glyph; picking the green line again in the same command unfixes it (blue again). The prompt after a pick reads "Select sketch objects to constrain".
- FissionOxide: Fix turns the line black and draws a padlock glyph above-right of each end point; unfixing needs the line selected again and the menu entry again.
- Start here: `crates/ui/src/panels.rs:3400-3470` (`unfix`, the `fix` branch of `apply_constraint`), `crates/engine/src/scene.rs` (fixed-curve colour), `crates/ui/src/viewport.rs:2906` (`glyph_anchor`; no glyph for `Fix`).

## curvature and polygon refusals (P2)
- key: `ui:sketch:curvature-polygon-refusal`
- Fusion: Curvature accepts the first line as a pick but silently refuses a second line (status stays "1 Sketch Line | Length : 0.489 in", prompt "Select sketch objects to constrain"); Polygon's prompt is "Select a closed profile with equal sides and angles to co..." (cut off at the window edge) and a single line pick is ignored.
- FissionOxide: Curvature on two selected lines prints "Curvature needs a spline" in red; Polygon prints "'polygon' needs 3 curve(s)".
- Start here: `crates/engine/src/commands.rs:3326-3395` (`constrain` messages), the pick command from the first item (filter picks instead of erroring).

## constraint menu icons (P2)
- key: `ui:sketch:constraint-menu-icons`
- Fusion: The dropdown icons are red/orange line art: Horizontal/Vertical a red bar with ticks, Coincident an "L" corner with a point, Tangent a circle with a tangent line, Equal two red bars, Parallel two slanted bars, Perpendicular an "L", Fix/UnFix a red padlock, MidPoint a triangle, Concentric two rings, Collinear two offset bars, Symmetry "[ ]" brackets, Curvature a curve with a tangent tick, Polygon a pentagon; the hovered row shows a vertical-ellipsis button at its right edge.
- FissionOxide: Purple icons with different shapes (Coincident a chevron, Tangent a "U", Symmetry three dots, Collinear a single slanted line), and the hovered row shows a small square at its right edge instead of the ellipsis.
- Start here: `crates/ui/src/icons.rs:861-900` (`Icon::Coincident`, `Icon::Tangent`, ...), `crates/ui/src/menus.rs:642`.
