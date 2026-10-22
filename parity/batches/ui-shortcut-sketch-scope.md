# Parity batch: ui-shortcut-sketch-scope

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Found on 2026-10-22 (prober 19) by pressing every letter on the canvas of both apps, in 3D with and
without a selection and inside an open sketch. FissionOxide built from `main` at `c3d74430`; Fusion's
document was a Part Design with a body and sketches. All four items live in the key dispatch in
`crates/ui/src/lib.rs` (`command_key_table`, `command_keys`, `launch`); take them together. See
`parity/reports/2026-10-22-keyboard-shortcuts.md`.

## Sketch keys work outside a sketch (P1)
- key: `ui:shortcuts:sketch-keys-from-3d`
- Fusion: in 3D with nothing selected, the sketch keys start their sketch tool and ask for a plane: C (Center Diameter Circle), L (Line), R (2-Point Rectangle), T (Trim) and P (Project) show the cursor prompt "Select a plane or planar face"; O opens the OFFSET dialog (Curves, Chain Selection, Direction, Flip) with the same prompt. Picking a plane creates the sketch with that tool running.
- FissionOxide: C, L, R, T, P and O do nothing outside a sketch (no prompt, no dialog); they only work after Create Sketch.
- Start here: `command_key_table` in `crates/ui/src/lib.rs:1240` takes keys only from `toolbox::workspace_tabs(self)`, which holds the SKETCH tab only while a sketch is open; when no sketch is open, fall back to the sketch scope's keys and run Create Sketch's plane pick with the tool queued (the plane-pick flow is `Cmd::CreateSketch` in `crates/ui/src/panels.rs`).

## A sketch key with a planar face selected sketches on that face (P1)
- key: `ui:shortcuts:sketch-key-on-face`
- Fusion: with a planar body face selected, C creates a new sketch on that face at once (the browser gains Sketch5, the view turns to look at the face, the SKETCH tab opens) with Center Diameter Circle running and the prompt "Place center point". L, R and the other sketch keys do the same with their tools.
- FissionOxide: C with a face selected does nothing; the face stays selected.
- Start here: the same dispatch as `ui:shortcuts:sketch-keys-from-3d`; reuse the Create Sketch path that takes a preselected planar face.

## X with nothing selected toggles the construction linetype (P2)
- key: `ui:shortcuts:x-construction-mode`
- Fusion: in a sketch with nothing selected, X turns on the Sketch Palette's Linetype "Construction" toggle (it stays highlighted after Escape, so the next curves are drawn as construction); X again turns it off. With curves selected X toggles them (unchanged).
- FissionOxide: X with nothing selected does nothing.
- Start here: `toggle_construction` in `crates/ui/src/lib.rs:1816` (called from `lib.rs:1232`); when the selection is empty, flip the palette's construction linetype instead.

## A 3D command run from a sketch keeps the sketch open (P1)
- key: `ui:shortcuts:3d-command-keeps-sketch`
- Fusion: in an open sketch, A (Appearance), E (Extrude), F (Fillet), H (Hole) and Q (Press Pull) open their 3D command over the sketch: the SOLID tab and the 3D view show, the sketch stays open (still being edited in the browser), and Escape or Cancel returns to the sketch with the SKETCH tab and the sketch view. I (Measure) runs without leaving the sketch view.
- FissionOxide: the same keys finish the sketch first (the comment at `lib.rs:1230` says so); Escape then leaves you in 3D with the sketch closed, so a mistaken E costs an Edit Sketch.
- Start here: `launch` in `crates/ui/src/lib.rs:1447` calls `finish_sketch()` for any command the SKETCH tab does not offer; instead suspend the sketch while the command runs and restore it on cancel (finish it only when the command commits, as Fusion does on OK).
