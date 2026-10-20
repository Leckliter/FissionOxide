# Parity batch: ui-timeline

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## rollback marker (P1)
- key: `ui:timeline:rollback-marker`
- Fusion: Dragging the marker rolls the model back; features after it grey out.
- FissionOxide: The marker is drawn but can't be dragged: marker() allocates it with Sense::hover (panels.rs, 'The history marker'). Rolling back works only through 'Roll Timeline Marker Here' or the playback buttons. In Fusion you drag the marker and later features grey out.
- Start here: `crates/ui/src/panels.rs`

## drag reorder (P1)
- key: `ui:timeline:drag-reorder`
- Fusion: Dragging an icon reorders features, with a red line where it can't go.
- FissionOxide: Icons can't be dragged and the engine has no timeline reorder command. Fusion lets you drag a feature earlier or later, with a red marker where dependencies forbid it.
- Start here: `crates/ui/src/panels.rs`, `crates/engine/src/commands.rs`

## context menu (P2)
- key: `ui:timeline:context-menu`
- Fusion: Right-click: Edit Feature, Edit Profile Sketch, Suppress, Delete, Roll History Marker Here, Create Group.
- FissionOxide: Has Edit Feature, Delete, Rename, Roll Timeline Marker Here, Suppress Features, Find in Window. Missing from Fusion's menu: Edit Profile Sketch, Create Group / Move to Group, Find in Browser.
- Start here: `crates/ui/src/panels.rs`
