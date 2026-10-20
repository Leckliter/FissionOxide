# Parity batch: ui-hole-and-pattern

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## hole fields (P1)
- key: `ui:dialogs:hole-fields`
- Fusion: Hole dialog: Placement (single/at points), Face, Extent, Hole Type (Simple/Counterbore/Countersink), Tap Type, Drill Point, diameter/depth fields drawn on a hole diagram.
- FissionOxide: Hole only drills at a sketch's points ('the sketch has no points to drill'). Fusion's default is a single hole placed by clicking a face, positioned with offset dimensions to edges, plus multiple holes at sketch points. Also missing Tap Type, Drill Point and the hole diagram with diameter/depth fields.
- Start here: `crates/ui/src/dialogs.rs`, `crates/ui/src/dialogs/solid.rs`, `crates/ui/src/forms.rs`

## pattern fields (P1)
- key: `ui:dialogs:pattern-fields`
- Fusion: Rectangular Pattern dialog: Pattern Type (Faces/Bodies/Features/Components), Objects, Directions, Distribution (Extent/Spacing), Quantity and Distance per direction, Suppress instances via canvas checkboxes.
- FissionOxide: RECTANGULAR PATTERN has Bodies, Direction (axis dropdown), Count, Spacing, Second direction, Join instances. Missing Pattern Type (Faces / Bodies / Features / Components), Distribution (Extent / Spacing), picking a direction from an edge, Symmetric, per-instance suppress checkboxes and drag arrows in the canvas. Engine options for several of these are in the driver session's open PRs (#84 pattern extent/symmetric, #86 two-side revolve, #91 chamfer, #97 full-round fillet); wire the dialog to them once merged.
- Start here: `crates/ui/src/dialogs.rs`, `crates/ui/src/dialogs/solid.rs`, `crates/ui/src/forms.rs`
