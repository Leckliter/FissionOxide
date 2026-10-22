# Parity batch: ui-shortcut-extra-keys

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Found on 2026-10-22 (prober 19) by pressing keys on the canvas of both apps (FissionOxide `main` at
`c3d74430`, Fusion's Part Design with a body). Keys FissionOxide binds that Fusion does not, keys
Fusion binds that FissionOxide lacks, and Show/Hide details. `toggle_visibility` and the Home key are
in `crates/ui/src/lib.rs`, near `ui-shortcut-sketch-scope`'s edits: rebase if that batch lands first.
See `parity/reports/2026-10-22-keyboard-shortcuts.md`.

## J does nothing in a Part Design (P2)
- key: `ui:shortcuts:j-in-part-design`
- Fusion: in a Part Design (ASSEMBLE holds only Add To Assembly), J and Shift+J do nothing, in 3D and in a sketch.
- FissionOxide: J opens the JOINT dialog (Component 1, Component 2, Motion Rigid, Angle, Offset, Flip) with the prompt "Select a face or edge on the moving component, then one on the fixed one", in 3D and from inside a sketch.
- Start here: `ASSEMBLE_MORE` in `crates/ui/src/menus.rs:337` keeps J / Shift+J live in every document; drop them from the key table when the document is a Part Design (`command_key_table`, `crates/ui/src/lib.rs:1240`).

## Home does nothing on the canvas (P2)
- key: `ui:shortcuts:home-key`
- Fusion: pressing Home with the pointer on the canvas leaves the view as it is (after zooming in, Home changed nothing; F6 fits). Go Home is only on the ViewCube's home icon and menu.
- FissionOxide: Home goes to the home view (`lib.rs:1224`, listed as reserved "Home View" in `customize.rs:78`).
- Start here: remove the Home binding in `crates/ui/src/lib.rs:1224` and its `RESERVED` row in `crates/ui/src/customize.rs:61`.

## V with nothing selected is silent (P2)
- key: `ui:shortcuts:v-nothing-selected`
- Fusion: V with nothing selected does nothing and prints nothing, in 3D and in a sketch.
- FissionOxide: prints "Show/Hide: select bodies, faces or edges first" at the lower left, where it stays for minutes.
- Start here: the `self.hint(...)` in `toggle_visibility`, `crates/ui/src/lib.rs:1479`.

## V keeps the body selected (P2)
- key: `ui:shortcuts:v-keeps-selection`
- Fusion: with Body1 selected, V hides it but keeps it selected: the body draws as a translucent blue ghost, the status bar still reads "Body1" and the browser eye greys; V again shows it, still selected.
- FissionOxide: V hides the body and clears the selection, so a second V prints "Show/Hide: select bodies, faces or edges first" instead of showing it again.
- Start here: `toggle_visibility` in `crates/ui/src/lib.rs:1458` (planes and origin items already stay picked; do the same for bodies and draw a hidden selected body as a selection-blue ghost).

## Shift+S opens Scripts and Add-Ins (P2)
- key: `ui:shortcuts:shift-s-scripts`
- Fusion: Shift+S opens the "Scripts and Add-Ins" window (Browse / Favorites, Type, Status, Source filters, a Name / Run / Run on Startup / Version table, Close).
- FissionOxide: Shift+S opens the S toolbox ("Design Shortcuts"), as plain S does; the Scripts and Add-Ins entry has no key.
- Start here: give `"Scripts and Add-Ins..."` in `crates/ui/src/menus.rs:1107` the key `.k("Shift+S")`, and make the S toolbox match plain S only (`lib.rs:1163`).

## Shift+N toggles Display Component Colors (P2)
- key: `ui:shortcuts:shift-n-component-colors`
- Fusion: Shift+N turns Display Component Colors on (the body and the timeline icons take the component's colour, purple here) and off again.
- FissionOxide: Shift+N does nothing; INSPECT's "Display Component Colors" is a placeholder entry (`x(...)`).
- Start here: `crates/ui/src/menus.rs:307`; implement the toggle (component colour on bodies and timeline icons).

## Help menu has no Keyboard Shortcuts window (P2)
- key: `ui:chrome:help-menu-current`
- Fusion: the Help (?) menu lists: a "Search Help" field, Take a Tour, Learning and Documentation ▸ (Self-Paced Learning, Product Documentation Ctrl+/, Fusion API), Quick Setup (grey), Autodesk Assistant Shift+H, Community ▸ (Forum, Insider Program, Blog), Support and Diagnostics ▸ (Get Support, Annotate Log File, Diagnostic Log Files, Graphics Diagnostic, Clear Cache Data (grey), Service Utility, Report Performance Issue), Check for Updates, What's New, Release Notes, About. There is no Keyboard Shortcuts entry, and Preferences has no shortcut page; keys are seen on the menu rows and changed from each row's options.
- FissionOxide: Help has Learning and Documentation, Keyboard Shortcuts (a searchable window), What's New, Diagnostic Log, Check for Updates, a navigation line and About (row `ui:chrome:help-menu`).
- Start here: the Help menu in `crates/ui/src/panels.rs:302` (the window is `crates/ui/src/help.rs:99`); rebuild it to Fusion's list and submenus.
