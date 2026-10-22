# Parity batch: ui-animation-workspace

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Observed on 2026-10-22 (prober 18) with both windows 908 px wide; Fusion's Part Design "Untitled*",
FissionOxide built from `main` at `2d74c694`, where DESIGN > ANIMATION only prints "Animation isn't in
FissionOxide yet" at the lower left and stays in the current workspace. This is a new workspace:
start with the chrome and the two dialogs, then the timeline. See
`parity/reports/2026-10-22-workspaces.md`.

## Animation workspace toolbar (P1)
- key: `ui:animation:toolbar`
- Fusion: DESIGN > ANIMATION switches to one tab ANIMATION with panels STORYBOARD (New Storyboard), TRANSFORM (quick Transform Components, Auto Explode, Show/Hide; dropdown Transform Components (M) / Restore Home / Auto Explode: One Level / Auto Explode: All Levels (U) / Manual Explode (E) / Show/Hide / Appearance), ANNOTATION (Create Callout), VIEW (View, Ctrl+R), PUBLISH (Publish Video, P).
- FissionOxide: no Animation workspace; the click prints "Animation isn't in FissionOxide yet".
- Start here: `crates/ui/src/panels.rs:1005` (the `"Animation"` branch of `workspace_menu`), `crates/ui/src/panels.rs:1083` (`switch_workspace`), `crates/ui/src/menus.rs:968` (how the RENDER tab's groups are declared).

## Animation browser (P2)
- key: `ui:animation:browser`
- Fusion: the browser shows the design as an assembly root ("Untitled" with an assembly icon) holding one "Components" folder; bodies, sketches and construction are not listed.
- FissionOxide: no Animation workspace.
- Start here: `crates/ui/src/panels.rs` (browser tree).

## Animation timeline and storyboards (P1)
- key: `ui:animation:timeline`
- Fusion: the design timeline is replaced by an "ANIMATION TIMELINE" panel (collapsible header) with a time ruler 0..24 s, a "1.0" scale column at the left, a red keyframe marker at 0 and a grey playhead, a storyboard tab bar at the bottom ("Storyboard1" and a "+"), transport buttons (to start, play, to end), full-screen and settings icons, and a zoom - / slider / + at the lower right.
- FissionOxide: no Animation workspace.
- Start here: `crates/ui/src/panels.rs:3414` (`timeline`).

## Transform Components dialog (P2)
- key: `ui:animation:transform-components-dialog`
- Fusion: Transform Components (M) opens a docked "TRANSFORM COMPONENTS" dialog with one row Components "Select" (active), OK greyed and Cancel.
- FissionOxide: no Animation workspace.
- Start here: `crates/ui/src/dialogs.rs` (docked dialog pattern).

## New Storyboard dialog (P2)
- key: `ui:animation:new-storyboard-dialog`
- Fusion: New Storyboard opens a docked "NEW STORYBOARD" dialog with Storyboard Type "Clean" (dropdown), OK and Cancel.
- FissionOxide: no Animation workspace.
- Start here: `crates/ui/src/dialogs.rs`.
