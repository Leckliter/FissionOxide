# Parity batch: ui-browser-body-rows

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Observed on 2026-10-22 (prober 17) with both windows 908 px wide; Fusion's Part Design had Body1,
FissionOxide's (built from `main` at `f1f9ed56`) Body1..3 from a rectangular pattern.
`ui:surface:body-context-menu` (batch `ui-surface-results`, fixing) asks for the same menu on
surface bodies through the same `body_menu`: rebase onto it first; if the solid body row already
matches, set this key `fixing-merged` with that commit. See
`parity/reports/2026-10-22-timeline-browser.md`.

## Solid body row's right-click menu (P1)
- key: `ui:browser:body-row-menu`
- Fusion: right-click on Body1's row: Move/Copy (M) / Move to Group / Create Selection Set / (separator) Configure / (separator) Physical Material / Appearance (A) / Texture Map Controls / Properties / (separator) Save As Mesh / (separator) Copy (Ctrl+C) / Cut (Ctrl+X) / Delete (Del) / Remove / Rename / (separator) Display Detail Control / (separator) Show/Hide (V) / Selectable/Unselectable / Opacity Control > / Isolate / Find in Window; Physical Material is enabled; there is no Create Components from Bodies.
- FissionOxide: Move/Copy (M) / Create Components from Bodies / Physical Material (greyed) / Appearance (A) / Properties / Save As Mesh / Delete / Remove / Rename / Show/Hide (V) / Isolate / Find in Window / Selectable/Unselectable / Opacity Control >: no Move to Group, Create Selection Set, Configure, Texture Map Controls, Copy, Cut or Display Detail Control; Isolate and Find in Window come before Selectable/Unselectable and Opacity Control.
- Start here: `crates/ui/src/panels.rs:2032` (`body_menu`).

## Folder eye when its children are hidden (P2)
- key: `ui:browser:folder-eye-children-hidden`
- Fusion: hiding Body1 (the only body) greys Body1's eye; the Bodies folder's own eye is not drawn at all until the pointer is on the folder row, where it shows open (the folder itself is still visible).
- FissionOxide: hiding Body2 (of three) turns the Bodies folder's eye into the struck-through hidden eye at once.
- Start here: `crates/ui/src/panels.rs:1961` (the Bodies folder's `all_visible` eye); the Sketches and Construction folders share the pattern.

## Construction folder starts collapsed (P2)
- key: `ui:browser:construction-folder-collapsed`
- Fusion: the first offset plane adds a "Construction" folder (with an eye) after Sketches, collapsed; Plane1 is inside it.
- FissionOxide: the folder appears expanded with Plane1 showing, and without an eye.
- Start here: `crates/ui/src/panels.rs:2293` (`tree_row(..., Some(true), None, ...)` and `is_open(ui, id, true)`: default closed, add the eye; `ui:browser:construction-folder-eyes` in batch `ui-construction-display` covers the eye's behaviour).
