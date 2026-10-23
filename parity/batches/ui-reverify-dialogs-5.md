# Parity batch: ui-reverify-dialogs-5

Make FissionOxide match Autodesk Fusion for the items below. Each says what Fusion does, what
FissionOxide does now, and where to start. Keep changes scoped to them, add tests, and run
`cargo test` and `cargo clippy` before pushing.

Found on 2026-10-22 by the re-verification sweep (verifier 5) re-checking the SURFACE and SHEET METAL keys in the rebuilt app (`main` at `c6232d9f`). Both are reopened keys.
Every row is already in `parity/ui-features.jsonl` (do not run `ledger_from_batches.py` for this
batch); read each row's `history` before starting.

## Surface Sweep and Loft are missing (P1)
- key: `ui:surface:sweep-loft-missing`
- Fusion: SURFACE > CREATE > Sweep opens SWEEP (Feature and Analysis tabs; Type "Single Path", Profile "Select", Path "Select", Chain Selection checked, Orientation "Perpendicular"; no Operation). Loft opens LOFT (Feature and Analysis tabs; Profiles pick list with + and x, Guide Type two icon toggles (rails / centerline), Rails list, Chain Selection unchecked, Closed unchecked, Tangent Edges two icon toggles; no Operation). Both make surface bodies from open or closed curves.
- FissionOxide: SWEEP matches; LOFT opens with Chain Selection checked (Fusion unchecked) and Guide Type as a single icon where Fusion has two icon toggles (rails / centerline).
- Start here: `crates/ui/src/dialogs.rs:221`

## One Flange dialog with Type icons (P1)
- key: `ui:sheet:flange-dialog-open`
- Fusion: "FLANGE" opens with Type as three icon toggles (Flange (Base/Edge/Contour) lit, Hem, Lofted flange; hovering each shows its tooltip), an empty selection list with + and x below it, "Edges/Profiles" Select chip (lit), OK greyed, Cancel; cursor prompt "Select sheet metal edges or sketch geometry to use for flange". CREATE > Hem opens the same dialog titled "HEM" with the Hem icon lit and prompt "Select an edge to hem"; Lofted Flange likewise. Nothing is picked for you, even with one unconsumed profile.
- FissionOxide: FLANGE opens with the three Type icons, the list, Edges/Profiles lit, OK greyed and Fusion's prompt, but the list's remove button under it draws a missing-glyph box where Fusion has an x (the ✕ character); the same in BASE / EDGE FLANGE and HEM. EDGE FLANGE's header bar also stops about 45 px short of the dialog's right edge.
- Start here: `crates/ui/src/dialogs/flange.rs`
