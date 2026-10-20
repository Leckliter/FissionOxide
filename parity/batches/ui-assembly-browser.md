# Parity batch: ui-assembly-browser

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## joint glyphs (P1)
- key: `ui:assemble:joint-glyphs`
- Fusion: Joints show glyphs in the canvas and a Joints folder in the browser; right-click a joint for Drive Joints, Motion Link, Edit Joint Limits, Lock.
- FissionOxide: Fusion: a revolute joint draws as a blue stem with a flag (axis direction) and a ring at the joint origin, drawn over the geometry, and it lights up when you hover the joint in the browser or canvas. FissionOxide draws a small black ring-with-dot at the origin, with no axis stem or flag, and hovering Joint1 in the browser doesn't highlight it. Clicking the glyph should select the joint.
- Start here: `crates/render/src/lib.rs`, `crates/ui/src/viewport.rs`, `crates/ui/src/panels.rs`

## joint context menu (P1)
- key: `ui:assemble:joint-context-menu`
- Fusion: Right-click a joint (browser or glyph): Create Selection Set, Configure, Drive Joints, Edit Joint, Edit Motion Limits, Lock Motion, Suppress, Animate Joint, Animate Joint Relationships, Select Components, Delete (Del), Rename, Show/Hide (V), Find in Window, Find in Timeline.
- FissionOxide: FissionOxide's joint menu has only Drive Joint, Motion Study, Rename and Delete. Missing: Edit Joint (reopen the joint dialog), Edit Motion Limits, Lock Motion, Suppress, Animate Joint, Animate Joint Relationships, Select Components, Show/Hide, Find in Window, Find in Timeline.
- Start here: `crates/ui/src/panels.rs`

## assembly structure (P1)
- key: `ui:browser:assembly-structure`
- Fusion: With components, the browser lists each occurrence under the root as Name:1 (component icon, eye, activate radio, pin badge if grounded). A component's bodies appear only inside it. Joints live under a Relationships folder: Relationships > Joints > Revolute 1. Joints are named by type (Revolute 1, Rigid 1, Slider 1).
- FissionOxide: FissionOxide groups components in a Components folder, shows them as 'A (grounded)' / 'B' with no :1 occurrence suffix, and lists their bodies twice: inside the component and again in the root Bodies folder. The Joints folder sits at the root rather than under Relationships, and joints are named Joint1 instead of by type (Revolute 1).
- Start here: `crates/ui/src/panels.rs`

## open file new tab (P2)
- key: `ui:chrome:open-file-new-tab`
- Fusion: Opening a design while another is open (even a modified one) opens it in a new document tab; the existing tab is untouched.
- FissionOxide: Opening joint.fox from outside while an unsaved Untitled was active gave a dialog 'Untitled has unsaved changes. Discard them and open joint.fox?' and replaced the document. FissionOxide has document tabs, so it should open a new tab as Fusion does.
- Start here: `crates/ui/src/app.rs`
