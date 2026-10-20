# Parity batch: ui-canvas-input

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## wheel zoom direction (P1)
- key: `ui:canvas:wheel-zoom-direction`
- Fusion: With default preferences (Reverse zoom direction off), rolling the wheel toward you (down) zooms in and rolling it away (up) zooms out, toward the cursor.
- FissionOxide: FissionOxide zooms the other way: wheel down zooms out (viewport.rs navigate(): factor = exp(-scroll.y*0.0015*zoom_sign), with reverse_zoom false). Fusion with Reverse zoom unchecked (checked on this machine's Preferences) zooms in on wheel down. Flip the default sign so the unchecked default matches Fusion, and keep the checkbox meaning 'reverse from Fusion's default'. Users with reverse_zoom already saved need their setting to keep the direction they chose.
- Start here: `crates/ui/src/viewport.rs`, `crates/ui/src/preferences.rs`

## double middle fit (P1)
- key: `ui:canvas:double-middle-fit`
- Fusion: Double-clicking the middle mouse button (wheel) fits the whole model in view, animated, like Fit (F6).
- FissionOxide: Double middle-click does nothing in FissionOxide. viewport.rs has no handler for a middle-button double-click; the existing double_clicked() checks are for the primary button (edge loop, text edit).
- Start here: `crates/ui/src/viewport.rs`

## copy paste (P1)
- key: `ui:shortcuts:copy-paste`
- Fusion: Ctrl+C then Ctrl+V on a component pastes a new occurrence (Arm:2) and opens the Initial Position dialog (Move Type, Set Pivot, X/Y/Z Distance, X/Y/Z Angle) with the triad, to place it; Cancel removes the paste. Ctrl+Shift+V (Paste New) makes an independent copy. On bodies and on sketch curves inside a sketch, Ctrl+V pastes a copy and opens Move/Copy.
- FissionOxide: FissionOxide has no copy/paste. Selecting Body1 and pressing Ctrl+C, Ctrl+V does nothing; lib.rs only binds V (without Ctrl). Fusion places pasted components and bodies with a triad dialog. The engine already has component.copy (Duplicate Component) and the Move dialog to build on.
- Start here: `crates/ui/src/lib.rs`, `crates/engine/src/commands.rs`

## space repeat (P2)
- key: `ui:shortcuts:space-repeat`
- Fusion: With no command running, Space repeats the last command (Preferences > General > 'Spacebar repeats last command', on by default). Enter does too.
- FissionOxide: Space does nothing in FissionOxide's canvas (lib.rs binds no Key::Space). The marking menu already has Repeat, so wire Space (and Enter when no dialog is open) to the same action, behind a preference matching Fusion's.
- Start here: `crates/ui/src/lib.rs`
