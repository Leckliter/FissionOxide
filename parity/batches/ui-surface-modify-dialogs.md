# Parity batch: ui-surface-modify-dialogs

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Observed on 2026-10-22 (prober 13) in an unsaved Part Design document in each app (Fusion in
inches, FissionOxide in mm), FissionOxide built from `main` at `de1a1c5a`, from the SURFACE tab's
MODIFY panel. As in batch ui-surface-create-dialogs: Fusion's dialogs have no Preview checkbox, pick
rows are "Select" chips, OK stays disabled until the picks are made, distance rows appear after the
first pick with an arrow and a focused floating value box. See
`parity/reports/2026-10-22-surface-tab.md`.

## Trim picks the tool first (P1)
- key: `ui:surface:trim-rows`
- Fusion: TRIM opens with one row, Trim Tool ("Select"; a body, sketch curve or plane), OK disabled. With a tool picked the pieces it cuts are listed for removal; picking Plane1, which crosses no surface, shows the red toast "1 error(s) / Cutting Tool does not intersect any surface! / More Info" and keeps OK disabled.
- FissionOxide: TRIM opens with Surfaces ("Select", the bodies to cut first), Trimming tool (dropdown "XY plane"), Keep the other side (checkbox), Pieces to remove (checkbox "Pick in the viewport"), Preview; Trim with Body and Trim with Sketch are separate commands.
- Start here: `crates/ui/src/forms.rs:1572` (TRIM), `crates/ui/src/forms.rs:2221` (TRIM_BODY) and TRIM_SKETCH after it.

## Extend rows (P1)
- key: `ui:surface:extend-rows`
- Fusion: EXTEND has Feature and Analysis tabs: Edges ("Select"), Tangent Chain (checked), Type ("Natural"; list Natural, Perpendicular, Tangent), Edge Alignment ("Free Edges"; list Free Edges, Align Edges); OK disabled. After an edge pick a Distance "0.00 in" row appears under Tangent Chain, with an arrow on the edge, a focused "0.00 in" box and the hint "Select extension type or specify distance".
- FissionOxide: EXTEND: Edges ("Select"), Distance "5 mm" (shown before any pick), Extend type ("Natural"; Natural, Tangent, Perpendicular in that order), Close corners (checkbox), Preview; no tabs, no Tangent Chain or Edge Alignment.
- Start here: `crates/ui/src/forms.rs:2182` (EXTEND).

## Untrim rows (P2)
- key: `ui:surface:untrim-rows`
- Fusion: UNTRIM: Faces ("Select"), Type ("All Edges"; list All Edges, Internal Edges, External Edges, Manual).
- FissionOxide: UNTRIM: Faces, "Loop type" ("All loops", "Internal loops", "External loops"; no Manual).
- Start here: `crates/ui/src/forms.rs:2202` (UNTRIM).

## Stitch rows (P1)
- key: `ui:surface:stitch-rows`
- Fusion: STITCH: Stitch Surfaces ("Select"), Tolerance "0.004 in" with a slider under it, a collapsed "Stitch Result" group and an open "Unstitched Edges" group holding an Edge / Max Gap table with a magnifier and a select icon under it.
- FissionOxide: STITCH: Bodies ("Select"), Tolerance "0.01 mm", Preview; no slider, no Stitch Result or Unstitched Edges groups.
- Start here: `crates/ui/src/forms.rs:1170` (STITCH).

## Unstitch and Reverse Normal pick faces (P2)
- key: `ui:surface:unstitch-reverse-rows`
- Fusion: UNSTITCH: Faces / Bodies ("Select"), Chain Selection (checked). REVERSE NORMAL: Faces ("Select").
- FissionOxide: UNSTITCH: Bodies ("Select"), Preview. REVERSE NORMAL: Bodies ("Select"), Preview.
- Start here: `crates/ui/src/forms.rs:1184` (UNSTITCH), `crates/ui/src/forms.rs:1195` (REVERSE_NORMAL).
