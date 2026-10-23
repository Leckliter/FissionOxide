# Parity batch: ui-reverify-mesh-1

Make FissionOxide match Autodesk Fusion for the items below. Each says what Fusion does, what
FissionOxide does now, and where to start. Keep changes scoped to them, add tests, and run
`cargo test` and `cargo clippy` before pushing.

Found on 2026-10-22 by the re-verification sweep (verifier 6) re-checking the MESH keys with an inserted
`lprism.stl` (a 30 x 10 x 5 mm L-prism; any small closed STL will do) on `main` at `dadbcad3`, then again on
`d2202676`. All five rows are reopened; three matched on `dadbcad3` and lost their Preview row when
`ce56bfa` (batch `ui-reverify-dialogs-1`, row `ui:dialogs:preview-checkbox-extra`) took the Preview check box
off every form: `Dialog::has_preview_box` (`crates/ui/src/dialogs/live.rs:233`) now keeps it only for Fold,
Convert to Sheet Metal, Rip and Corner Closure. Fusion's command dialogs have no Preview box, but its MESH
dialogs do (Repair and Tessellate still have theirs). Keep the solid dialogs without it.
Every row is already in `parity/ui-features.jsonl` (do not run `ledger_from_batches.py` for this
batch); read each row's `history` before starting.

## Remesh preview and Preview row (P1)
- key: `ui:mesh:remesh-dialog`
- Fusion: REMESH: Body/Face Groups "1 selected", Type (Adaptive, Uniform), Method (Density, Edge Length), Density "0.25" with a slider, Shape Preservation "0.50" with a slider, Preserve Sharp Edges, Preserve Boundaries and Preview check boxes (all unticked). Nothing previews until Preview is ticked, and the remeshed prism keeps its shape.
- FissionOxide: rows now stack (fixed by ce56bfa), but there is no Preview row, and as soon as the dialog opens the prism is drawn as a shapeless melted blob (the default Density 0.25 / Shape Preservation 0.50 result).
- Start here: `crates/ui/src/dialogs/live.rs:233`, `crates/model/src/meshops.rs:129`

## Reduce Preview row (P2)
- key: `ui:mesh:reduce-dialog`
- Fusion: REDUCE ends with a Preview check box (unticked) under Proportion's slider.
- FissionOxide: rows match and stack, but no Preview row.
- Start here: `crates/ui/src/dialogs/live.rs:233`, `crates/ui/src/forms.rs:2106`

## Smooth Preview row and the freeze on opening (P2)
- key: `ui:mesh:smooth-dialog`
- Fusion: SMOOTH: Body/Face Groups, Smoothness "1.00" with a slider, Preview (unticked); opens at once.
- FissionOxide: no Preview row; opening Smooth on the small prism freezes the app for 5-10 s (window black / not responding) before the dialog appears, on both builds.
- Start here: `crates/ui/src/forms.rs:2124`, `crates/model/src/regen/mesh_edit.rs`

## mesh Shell Preview row (P2)
- key: `ui:mesh:shell-dialog`
- Fusion: SHELL (mesh): Body, Thickness with a slider, Preview (unticked).
- FissionOxide: no Preview row, and the shell previews while nothing is ticked.
- Start here: `crates/ui/src/forms.rs:2196`

## mesh Combine Preview row (P2)
- key: `ui:mesh:combine-dialog`
- Fusion: COMBINE (mesh) ends with Preview after Algorithm.
- FissionOxide: no Preview row.
- Start here: `crates/ui/src/forms.rs:2219`
