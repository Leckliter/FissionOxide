# Parity batch: ui-sweep-loft-revolve

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-22 (prober 8) with a one-rectangle sketch beside an axis in each app, FissionOxide
built from `main` at `0c453dd1`. Both apps pick the only profile automatically when Revolve opens.
The rows are in `crates/ui/src/dialogs/solid.rs` (`revolve_type_rows` at 508, `sweep_rows` at 837,
`loft_rows` at 884).

## Revolve rows (P1)
- key: `ui:dialogs:revolve-fields`
- Fusion: REVOLVE: Profile (chip), Axis (Select chip, no default; the origin axes and planes are shown for the pick; prompt "Select the axis to revolve the profile around"), Project Axis (checked), then after the axis pick Extent Type ("Partial" dropdown), Angle ("360.0 deg" in a floating box with an angle handle on the arc), Direction ("One Side"), Operation, and "Objects To Cut" when cutting; OK disabled until the axis is picked.
- FissionOxide: Axis is a dropdown preset to "Z axis" (the preview appears at once), Type is a pair of Angle / Full radio buttons, there is no Project Axis or Extent Type row, no Objects To Cut, and no angle handle or floating box.
- Start here: `crates/ui/src/dialogs/solid.rs:508` (`revolve_type_rows`), `crates/ui/src/dialogs.rs:777` (drop the preset axis), `crates/ui/src/dialogs/manip.rs` (angle handle).

## Sweep rows (P1)
- key: `ui:dialogs:sweep-rows`
- Fusion: SWEEP has two tabs, Feature and Analysis. Feature: Type ("Single Path"; also Path + Guide Rail, Path + Guide Surface, Solid Sweep), Profile (Select chip), Path (Select chip), Chain Selection (checked), Orientation ("Perpendicular" dropdown), Operation; OK disabled; prompt "Specify a sweep type or select an object to sweep". Analysis: Analysis Type (four icon toggles) and Bodies (Select).
- FissionOxide: No tabs or Type; Profile is picked automatically, Path is a sketch dropdown, Guide rail a sketch dropdown, Guide surface a face chip, empty Twist angle and Taper angle fields, an Orientation "Parallel" checkbox; OK is enabled with a red "the sketch has no lines or arcs for the contour".
- Start here: `crates/ui/src/dialogs/solid.rs:837` (`sweep_rows`).

## Loft rows (P1)
- key: `ui:dialogs:loft-rows`
- Fusion: LOFT (a wider dialog) has Feature and Analysis tabs. Feature: Profiles (a select icon and a list box with + and x below), Guide Type (two icon toggles: rails, centerline), Rails (select icon and a list box with + and x), Chain Selection (checked), Closed (unchecked), Tangent Edges (two icon toggles), Operation; OK disabled. The origin planes and axes show while it is open.
- FissionOxide: LOFT: Profiles (one Select chip), "Ruled (straight between sections)" and "Closed (back to the first)" checkboxes, Centre line dropdown, Rails as one checkbox per sketch, Operation; no list boxes, Guide Type, Tangent Edges or Analysis tab.
- Start here: `crates/ui/src/dialogs/solid.rs:884` (`loft_rows`) and `crates/ui/src/dialogs.rs:3029`.

## Operation switches to Cut over a body (P1)
- key: `ui:dialogs:operation-auto-cut`
- Fusion: When a Revolve or Pipe preview starts inside an existing body, Operation switches itself to Cut, the preview turns translucent red, and an "Objects To Cut" group (collapsed) appears under Operation.
- FissionOxide: Operation stays "New body" and the preview is a solid grey body overlapping the old one.
- Start here: `crates/ui/src/dialogs.rs` (where Extrude chooses its default operation from the preview's overlap; apply it to Revolve, Sweep, Pipe and Loft, with the Objects To Cut group).
