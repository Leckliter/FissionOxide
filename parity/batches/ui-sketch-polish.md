# Parity batch: ui-sketch-polish

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## inference lines (P2)
- key: `ui:sketch:inference-lines`
- Fusion: Dotted inference lines snap to horizontal, vertical and other points.
- FissionOxide: Drawing a second line whose end sits directly below the first line's end (same x within 1 px) shows no dotted vertical alignment guide and doesn't snap to it (the line came out at 355.2 degrees). Horizontal/vertical snapping of the line itself works (0 degree glyph). Fusion draws dotted inference lines to aligned points and snaps.
- Start here: `crates/ui/src/viewport.rs`

## p is project (P2)
- key: `ui:shortcuts:p-is-project`
- Fusion: In a sketch, P runs Project / Include; Point has no default key.
- FissionOxide: Pressing P in a sketch starts Point, while the S box lists Project with shortcut P. Fusion: P = Project / Include. Make P run Project in sketches, as listed; Point keeps no key or goes in the S box.
- Start here: `crates/ui/src/lib.rs`
