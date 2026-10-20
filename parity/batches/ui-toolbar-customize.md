# Parity batch: ui-toolbar-customize

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## pin to toolbar (P2)
- key: `ui:toolbar:pin-to-toolbar`
- Fusion: Commands in dropdowns can be pinned to the panel or the toolbar.
- FissionOxide: Dropdown rows have no options button. Fusion shows a '...' on hover with Pin to Toolbar, Pin to Panel, Add to Shortcuts, Change Keyboard Shortcut. Nothing in crates/ui handles pinning.
- Start here: `crates/ui/src/panels.rs`

## rich tooltip (P2)
- key: `ui:toolbar:rich-tooltip`
- Fusion: Hovering a command shows a tooltip; resting longer expands it with an illustration and description.
- FissionOxide: Tooltip is 'Extrude / Shortcut: E' and doesn't grow. Fusion shows name, description and shortcut, and after resting longer an illustration with a fuller explanation. Fusion's tooltip text is in parity/catalog.json, which could feed this.
- Start here: `crates/ui/src/panels.rs`

## customize (P2)
- key: `ui:shortcuts:customize`
- Fusion: Shortcuts can be reassigned from each command's ... menu.
- FissionOxide: No way to change a shortcut. Fusion: Change Keyboard Shortcut from a command's '...' menu, or Preferences.
- Start here: `crates/ui/src/panels.rs`
