# Parity batch: ui-reverify-dialogs-7

Make FissionOxide match Autodesk Fusion for the items below. Each says what Fusion does, what
FissionOxide does now, and where to start. Keep changes scoped to them, add tests, and run
`cargo test` and `cargo clippy` before pushing.

Found on 2026-10-23 by the incremental re-verification pass (verifier 7) on `main` at `905a4701`. Both
rows are reopened (from `ui-reverify-dialogs-4` and `ui-reverify-dialogs-5`). Every row is already in
`parity/ui-features.jsonl` (do not run `ledger_from_batches.py` for this batch); read each row's
`history` before starting.

## ui-reverify-dialogs-7 (P1)

- key: `ui:dialogs:replace-face-plane-target`
- Fusion: REPLACE FACE's Target Faces takes a construction plane clicked in the canvas.
- FissionOxide: Plane1's browser row works (the row fills, OK moves the face onto the plane), but in the canvas Plane1 (an Offset Plane 10 mm off a box face, drawn beside the box and clear of it) neither hovers nor picks while Target Faces is active, tried three times. Once the row already held Plane1 the plane did hover, so check `hover_cplane` while an empty plane-taking row is active.
- Start here: `crates/ui/src/viewport.rs:523` (`hover_cplane` click), `crates/ui/src/dialogs/rows.rs:41` (`row_takes_plane`)

- key: `ui:surface:sweep-loft-missing`
- Fusion: SURFACE > Loft's Rails list takes rails (and the Centerline toggle a centerline), and the surface loft follows them.
- FissionOxide: SWEEP and LOFT's rows match, but Loft's Rails / Centerline Select chip is greyed: the surface loft takes no rails or centerline yet.
- Start here: `crates/ui/src/dialogs/rsl.rs:67` (Guide Type), `crates/ui/src/dialogs/surface.rs:502` (the command sent), `crates/engine/src/surface_create.rs:135` (`surface.loft`)
