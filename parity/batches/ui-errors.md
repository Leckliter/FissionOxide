# Parity batch: ui-errors

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## review warning (P1)
- key: `ui:timeline:review-warning`
- Fusion: A failed feature's timeline icon gets a red frame, and a red error badge appears at the canvas's lower right. Right-clicking the icon adds Review Warning (red icon) to the menu (Create Selection Set, Edit Feature, Configure, Delete, Rename, Roll Timeline Marker Here, Convert to DM Feature, Suppress Features, Review warning, Find in Browser, Find in Window). It opens an Error window with a tree, Fillet1 > Compute Failed > an explanatory message (for example 'The fillet/chamfer could not be created where edges meet at a corner... Try adjusting the size...'), a copy button and Close. Clicking the badge opens the same window for all failures.
- FissionOxide: FissionOxide marks failed features in the timeline (ui:timeline:error-badges match), but the timeline menu has no Review Warning item, there's no Error window with the per-feature message tree and copy button, and the canvas has no error badge that lists every failure. The engine's error strings should feed the window's messages, phrased like Fusion's. Scene: a box with all edges filleted by parameter filletR, then filletR raised from 2 mm to 8 mm.
- Start here: `crates/ui/src/panels.rs`, `crates/engine/src/features.rs`
