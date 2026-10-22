# Parity batch: ui-pattern-mirror-misc

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-22 (prober 8) by opening each command from SOLID > CREATE in both apps,
FissionOxide built from `main` at `0c453dd1`. Mirror and Circular Pattern were driven to their
preview in Fusion (cylinder mirrored about a box face; torus patterned about the cylinder).

## Mirror rows (P1)
- key: `ui:dialogs:mirror-rows`
- Fusion: MIRROR: Object Type ("Bodies" dropdown with icon), Objects (Select chip), Mirror Plane (Select chip, no default), OK disabled, prompt "Select bodies to mirror"; after both picks an Operation row ("New Body") appears and the prompt reads "Hold Ctrl to modify object selection or mirror plane". The origin planes show while it is open.
- FissionOxide: MIRROR: Plane first, a dropdown preset to "XY plane", then Type, Bodies (taking the current selection), and a "Join to original" checkbox instead of Operation; the preview appears at once.
- Start here: `crates/ui/src/dialogs.rs:778` (`Kind::Mirror` start) and `crates/ui/src/dialogs.rs:2578` (rows).

## pattern dialogs share a Type row (P1)
- key: `ui:dialogs:pattern-type-switch`
- Fusion: Rectangular, Circular and Pattern on Path open one dialog whose first row, Type, is three icon toggles (rectangular, circular, path) that switch between them; CIRCULAR PATTERN then has Object Type ("Bodies"), Objects (Select), Axis (Select), OK disabled, prompt "Select solid or surface bodies in the canvas or browser to pattern"; after the picks Distribution ("Full"), Suppress, Quantity (3, floating box with a spinner).
- FissionOxide: Separate dialogs without a Type row; CIRCULAR PATTERN opens with Bodies "all bodies" and Axis preset to "Z axis", so every body is patterned at once, with Count, Total angle, Symmetric and Join instances rows.
- Start here: `crates/ui/src/dialogs.rs:4511` ("all bodies" default) and `crates/ui/src/dialogs.rs:937` (circular defaults); the three pattern dialogs in `crates/ui/src/dialogs/hole_pattern.rs`.

## Pattern on Path rows (P2)
- key: `ui:dialogs:path-pattern-rows`
- Fusion: Pattern on Path is the pattern dialog with Type set to path: Object Type, Objects, Path (Select chip), then Distribution, Quantity, Distance, Orientation after the picks.
- FissionOxide: PATTERN ON PATH: Bodies, Path sketch (dropdown), "Path curves (default all)", Quantity, Distance type, Distance, "Start point (0 to 1)", Flip, Symmetric, Orientation, "Join to the original", Preview.
- Start here: `crates/ui/src/forms.rs:2609` (`PATH_PATTERN`).

## ghost previews for copies (P2)
- key: `ui:dialogs:copy-ghost-preview`
- Fusion: Mirror and pattern previews draw each new copy as a translucent grey ghost with dark silhouette edges; the picked originals turn blue.
- FissionOxide: Copies preview as solid grey bodies, indistinguishable from real ones.
- Start here: `crates/ui/src/preview.rs` (a ghost style for mirror and pattern instances).

## Create Base Feature is a mode (P2)
- key: `ui:dialogs:base-feature-mode`
- Fusion: Create Base Feature opens no dialog: the ribbon switches to BASE FEATURE SOLID / BASE FEATURE SURFACE tabs with a green FINISH BASE FEATURE button, and a base feature icon is added to the timeline; Finish returns to the SOLID tab.
- FissionOxide: An empty dialog titled "BASE FEATURE" with OK and Cancel.
- Start here: `crates/ui/src/config_ui.rs:57` (BASE FEATURE) and `:68` (FINISH BASE FEATURE): enter the mode from the CREATE entry.

## Joint Origin rows (P2)
- key: `ui:dialogs:joint-origin-rows`
- Fusion: JOINT ORIGIN opens with Origin Mode (three icon toggles: simple, between two faces, two edge intersection) and Snap (Select chip), OK disabled; the angle, offset and flip rows come after the snap.
- FissionOxide: JOINT ORIGIN: Face, Angle, Offset X, Offset Y, Offset Z, Flip, Name; no Origin Mode or Snap.
- Start here: `crates/ui/src/assembly_forms.rs:14` (`JOINT_ORIGIN`).
