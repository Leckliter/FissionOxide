# Parity batch: ui-sketch-trim-extend-break

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-21 (prober 4) on a sketch with a circle crossing a rectangle's right edge and
a short line ending near its left edge; FissionOxide `main` at `feea62e`.

## trim prompt (P2)
- key: `ui:sketch:trim-prompt`
- Fusion: Trim's pointer prompt is "Select curve section to trim" (also shown after a drag).
- FissionOxide: "Select curve to trim".
- Start here: `crates/ui/src/viewport.rs:3781`.

## extend preview (P2)
- key: `ui:sketch:extend-preview`
- Fusion: Hovering a line with Extend draws the line bold blue and the piece that will be added in light red from its end to the curve it will meet, with the prompt "Select curve to extend"; the click adds exactly that piece.
- FissionOxide: Hovering shows no preview of the extension (only the prompt); the click extends correctly.
- Start here: `crates/ui/src/viewport.rs:998` (Trim/Extend hover: Trim already previews the piece in dark purple; Extend should preview the added piece in a light red).

## break preview (P2)
- key: `ui:sketch:break-preview`
- Fusion: Break's prompt is "Select curve to Break"; hovering a curve draws it blue and marks every break point with a small red x (two on the rectangle's right edge where the circle crosses it); the click splits it there.
- FissionOxide: Prompt "Select curve to break"; hovering shows neither a highlight nor the break points; the click splits correctly.
- Start here: `crates/ui/src/viewport.rs:1777` (`Tool::Break`), `crates/ui/src/viewport.rs:3783` (prompt).

## removed-constraints warning toast (P2)
- key: `ui:sketch:modify-warning-toast`
- Fusion: When a Chamfer, Trim or Break drops constraints or dimensions, a toast appears at the canvas's lower right: a warning icon, bold "1 warning(s)" and "Constraints and/or dimensions were removed during operation."; a warning icon also appears at the right end of the status bar until the next command.
- FissionOxide: Nothing is shown when a trim or break removes constraints.
- Start here: the trim/break/chamfer results in `crates/engine/src/commands.rs` (report removed constraints) and a toast painter near the status readout in `crates/ui/src/panels.rs:2880`.
