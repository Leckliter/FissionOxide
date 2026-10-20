# Parity batch: ui-components

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## component (P2)
- key: `ui:context-menus:component`
- Fusion: Right-click a component in the browser: Activate, New Component, Insert, Save Copy As, Show/Hide, Isolate, Physical Material, Properties.
- FissionOxide: Component menu: Rename, Ground, Duplicate, Physical Material, Delete Component. Missing Fusion's Activate Component, New Component, Show/Hide, Isolate, Save Copy As, Open, Properties, Find in Window, Create Selection Set.
- Start here: `crates/ui/src/panels.rs`

## new component (P2)
- key: `ui:assemble:new-component`
- Fusion: New Component dialog: Standard/Sheet Metal type, name, parent, Empty component / From bodies, Activate.
- FissionOxide: NEW COMPONENT has Name and Bodies only. Fusion: Type (Standard / Sheet Metal), Name, Part Number, Description, Parent component, Empty Component vs From Bodies, Activate checkbox.
- Start here: `crates/ui/src/dialogs.rs`

## assemble menu (P2)
- key: `ui:assemble:assemble-menu`
- Fusion: ASSEMBLE dropdown: New Component, Joint, As-built Joint, Joint Origin, Rigid Group, Drive Joints, Motion Link, Enable Contact Sets, Motion Study.
- FissionOxide: ASSEMBLE has New Component, Joint J, Drive Joints, Motion Study, Duplicate Component, Joint Origin, Rigid Group, Motion Link, Contact Set, Insert Component, Cam Follower. Missing Fusion's As-built Joint (joint two components where they already sit, Shift+J).
- Start here: `crates/ui/src/panels.rs`
