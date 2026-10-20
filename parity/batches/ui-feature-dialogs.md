# Parity batch: ui-feature-dialogs

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## revolve fields (P1)
- key: `ui:dialogs:revolve-fields`
- Fusion: Revolve dialog: Profile, Axis, Type (Angle/To/Full), Angle, Direction (One Side/Two Sides/Symmetric), Operation; a rotation manipulator in the canvas.
- FissionOxide: Revolve has Profiles, Axis (a dropdown of X/Y/Z, not a pick of a line or edge), Angle, Operation. Missing Fusion's Type (Angle / To / Full), Direction (One Side / Two Sides / Symmetric), picking any line/edge/axis, and the rotation manipulator. Engine options for several of these are in the driver session's open PRs (#84 pattern extent/symmetric, #86 two-side revolve, #91 chamfer, #97 full-round fillet); wire the dialog to them once merged.
- Start here: `crates/ui/src/dialogs.rs`, `crates/ui/src/dialogs/solid.rs`, `crates/ui/src/forms.rs`

## chamfer fields (P1)
- key: `ui:dialogs:chamfer-fields`
- Fusion: Chamfer dialog: Edges, Chamfer Type (Equal distance/Two distances/Distance and angle), Distance, Tangent Chain, Corner Type.
- FissionOxide: Chamfer has only Edges and Distance. Missing Chamfer Type (Equal distance / Two distances / Distance and angle), per-row distances, Tangent Chain, Corner Type, Flip. Engine options for several of these are in the driver session's open PRs (#84 pattern extent/symmetric, #86 two-side revolve, #91 chamfer, #97 full-round fillet); wire the dialog to them once merged.
- Start here: `crates/ui/src/dialogs.rs`, `crates/ui/src/dialogs/solid.rs`, `crates/ui/src/forms.rs`

## fillet fields (P2)
- key: `ui:dialogs:fillet-fields`
- Fusion: Fillet dialog: Type (Fillet/Rule Fillet/Full Round), selection rows each with a radius, Tangent Chain, Corner Type (Rolling Ball/Setback), radius manipulator.
- FissionOxide: Fillet has selection rows each with a radius (+ to add a row), Tangent chain, Radius type (Constant / Chord length) and Corner type. Missing the Type dropdown (Fillet / Rule Fillet / Full Round) and Variable radius type. Engine options for several of these are in the driver session's open PRs (#84 pattern extent/symmetric, #86 two-side revolve, #91 chamfer, #97 full-round fillet); wire the dialog to them once merged.
- Start here: `crates/ui/src/dialogs.rs`, `crates/ui/src/dialogs/solid.rs`, `crates/ui/src/forms.rs`

## shell fields (P2)
- key: `ui:dialogs:shell-fields`
- Fusion: Shell dialog: Faces/Body, Tangent Chain, Inside/Outside thickness, Direction (Inside/Outside/Both).
- FissionOxide: Shell has Faces, Thickness and Round corners. Missing Direction (Inside / Outside / Both), Tangent Chain, picking a whole body for a hollow shell.
- Start here: `crates/ui/src/dialogs.rs`, `crates/ui/src/dialogs/solid.rs`, `crates/ui/src/forms.rs`
