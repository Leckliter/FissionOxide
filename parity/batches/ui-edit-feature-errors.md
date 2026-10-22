# Parity batch: ui-edit-feature-errors

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Observed on 2026-10-22 (prober 10): double-clicking timeline icons (Box and Fillet in Fusion,
Extrude and Fillet in FissionOxide) and forcing a fillet to fail (a 5 in fillet after the box
height went to 0.05 in in Fusion; a 100 mm radius in FissionOxide). FissionOxide built from `main`
at `2a6b280b`. The "EDIT FEATURE" title is already `ui:dialogs:edit-feature-title` in
`ui-primitive-dialogs`. See `parity/reports/2026-10-22-manipulators.md`.

## Editing a feature rolls the timeline back (P1)
- key: `ui:timeline:edit-rollback`
- Fusion: Edit Feature on a timeline item rolls the design back to it while the dialog is open: the marker moves to just after the item, the later items are drawn faded, and the bodies and browser rows they made disappear from the canvas and browser; OK or Cancel rolls forward again.
- FissionOxide: Edit Extrude leaves the marker at the end; Fillet1 (after the extrude) stays applied in the canvas during the edit.
- Start here: `crates/ui/src/panels.rs:3737` (`edit_feature`: roll to the item, remember the old marker) and `crates/ui/src/dialogs.rs:1084` (`Dialog::edit`), restoring on OK and Cancel.

## Edit dialogs keep their selection rows live (P1)
- key: `ui:dialogs:edit-selection-rows`
- Fusion: The edit dialog is the creation dialog filled in: Fillet's set row reads "1 Edge | 5 in | 5 in | Tangent (G...)", Edges/Faces/Features is a live Select chip and Tangent Chain is enabled; Box keeps its Placement chip; the picks can be changed.
- FissionOxide: Edit Fillet shows "1 (unchanged)" as the set row's label and as plain Edges/Faces/Features text, with Tangent chain greyed; Edit Extrude shows "Profiles (unchanged)"; the picks cannot be changed.
- Start here: `crates/ui/src/dialogs/solid.rs:180`, `crates/ui/src/dialogs/solid.rs:236`, `crates/ui/src/dialogs/extrude.rs:93` (the "(unchanged)" labels) and `crates/ui/src/dialogs.rs:1084`.

## Edit shows the same manipulators (P2)
- key: `ui:dialogs:edit-manipulators`
- Fusion: Edit Feature shows the command's handles as during creation: Box its blue height arrow, four outlined side arrows and the focused box "1 in"; Fillet its radius arrow and setback handle.
- FissionOxide: Edit Extrude shows the distance arrow and box but no taper handle (`Dialog::handles` returns nothing while editing).
- Start here: `crates/ui/src/dialogs/manip.rs:269` (`handles`: drop the `editing` early return).

## A failing value greys OK and shows the error toast (P1)
- key: `ui:dialogs:failing-value-ok-disabled`
- Fusion: When the typed value makes the feature fail (fillet radius 5 in), OK greys out and a toast at the canvas's lower right shows a red circled X, bold "1 error(s)", the message "The fillet/chamfer could not be created at the requested size. Try adjusting the size, deselecting some of the edges (try disabling Tangent Chain), or using multiple separate operations." and a "More Info" link; a small red X badge sits at the right end of the status bar. A valid value (0.01) clears both and enables OK.
- FissionOxide: The error is red text inside the dialog ("fillet: the fillet could not be computed (radius too large?)") and OK stays enabled; OK commits the failed feature (the fillet vanishes from the model).
- Start here: `crates/ui/src/dialogs.rs:2348` (`ok_enabled`: false while the live preview failed) and `crates/ui/src/panels.rs:3157` (the warning toast: add the error variant with the message and the status-bar badge).

## Error toast after a recompute (P1)
- key: `ui:timeline:error-toast`
- Fusion: When an upstream edit leaves a later feature failed (Box height 0.05 in under a 5 in Fillet1), a toast at the canvas's lower right shows the red X, "1 error(s)", the feature name "Fillet1" and "More Info", and a red X badge stays at the status bar's right end; the timeline icon gets a pink fill with a red border.
- FissionOxide: The timeline icon gets the pink fill and red border (matches) and its tooltip carries the message, but there is no toast and no status-bar badge.
- Start here: `crates/ui/src/panels.rs:3157` (toast) and `crates/ui/src/panels.rs:2919` (status-bar badge), fed by the timeline state at `crates/ui/src/panels.rs:3405`.
