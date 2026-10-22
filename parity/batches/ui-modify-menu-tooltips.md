# Parity batch: ui-modify-menu-tooltips

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-22 (prober 9): SOLID > MODIFY in each app; Fusion in inches on a design with a
box, cylinder, sphere, torus and coil, FissionOxide in mm on two 20 mm boxes, built from `main` at
`ffa348e2`. See `parity/reports/2026-10-22-solid-modify.md`.

## SOLID MODIFY dropdown list (P2)
- key: `ui:toolbar:solid-modify-menu-list`
- Fusion: Press Pull Q, Fillet F, Chamfer, separator, Shell, Draft, Scale, Combine, Offset Face, Replace Face, Split Face, Split Body, Silhouette Split, separator, Move/Copy M, Align, Delete Del, Remove, Simplify ▸, separator, Physical Material, Appearance A, Manage Materials, separator, Change Parameters, Compute All Ctrl+B, separator, Bill of Materials. No separator between Press Pull and Fillet or between Remove and Simplify; no Direct Edit entry. Simplify ▸ lists Remove Features, Remove Faces, Replace with Primitives, each with its own icon.
- FissionOxide: Same entries, plus a separator after Press Pull, a separator between Remove and Simplify, and a separator and a "Direct Edit ▸" submenu (Move Face, Press Pull, Delete Face, Create Base Feature, Finish Base Feature, Capture Design History) after Bill of Materials; Simplify lists Remove Faces first, then Remove Features, both with the same icon.
- Start here: `crates/ui/src/menus.rs:160` (`SOLID_MODIFY`: drop the two separators and the Direct Edit entry; Fusion offers direct edits only in a design with history off) and `crates/ui/src/menus.rs:118` (`SIMPLIFY` order and icons).

## MODIFY dropdown tooltips (P2)
- key: `ui:toolbar:solid-modify-menu-tooltips`
- Fusion: Draft: "Applies a fixed or parting line draft angle to planar faces." / "Select a pull direction, parting tool, and faces to draft, then specify the draft angle." with two illustrations and "Press Ctrl+/ for more help." Compute All: "Computes all of the parameters, features, and assembly relationships in a design." / "Use at major milestones in the design process and after making significant changes to features in the Timeline, to ensure all assembly relationships are calculated properly." / "Press Ctrl+/ for more help."
- FissionOxide: Draft: "Tapers faces about a pull direction." / "Pick the pull direction and the faces, then set the angle. Moulded and cast parts need draft to release from the mould." Compute All: "Recomputes the whole timeline." / "Every feature is rebuilt in order. Use it after changing linked data or when something looks out of date." No illustration and no Ctrl+/ line.
- Start here: `crates/ui/src/tips.rs:1005` (Draft) and `crates/ui/src/tips.rs:1085` (Compute All): use Fusion's wording; check the other MODIFY rows (Scale at `:1010`, Combine, Offset Face, ...) against Fusion's while there.

## Compute All status message (P2)
- key: `ui:dialogs:compute-all-message`
- Fusion: Compute All on a small design shows no lower-left message; the model simply recomputes.
- FissionOxide: Shows "Computed in 0.0 s" at the lower left.
- Start here: `crates/ui/src/lib.rs:545` (report only errors, as Fusion).

## Parameters dialog details (P2)
- key: `ui:dialogs:parameters-dialog-details`
- Fusion: PARAMETERS has an "X" close at the top right and does not dim the window behind it. Favorites and User Parameters rows have no expand arrow (User Parameters has an fx icon); Model Parameters has a collapse arrow and one child row "(Unsaved)" (component icon, the document's name), collapsed.
- FissionOxide: A square glyph at the top right and the window behind dimmed; every section has a triangle; the document row reads "Untitled", expanded, with Box1, Box2, Draft1 under it.
- Start here: `crates/ui/src/params_ui.rs:408` (section rows, the document row's name and default collapse) and the window frame in `crates/ui/src/params_ui.rs`.

## responsive ribbon collapse, MODIFY count (P2)
- key: `ui:toolbar:responsive-collapse`
- Fusion: At a 907 px window: CREATE 5 icons ending with New Component (purple), MODIFY 4 (Press Pull, Fillet, Shell, Combine), every panel through SELECT visible.
- FissionOxide: Re-checked at the same width after the earlier fix: every panel through SELECT is visible and CREATE has 5 (its fifth is Rectangular Pattern), but MODIFY keeps 6 icons (Press Pull, Fillet, Chamfer, Combine, Split Body, Move/Copy).
- Start here: `crates/ui/src/menus.rs:1443` (`fit_counts`: Fusion takes MODIFY down to 4 at this width; match the per-panel counts at 907 px) and `crates/ui/src/menus.rs:1339`.
