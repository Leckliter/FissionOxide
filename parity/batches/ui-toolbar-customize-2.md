# Parity batch: ui-toolbar-customize-2

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-21 (prober 7) from the SOLID > CREATE dropdown's Extrude row in each app
(FissionOxide built from `main` at `42dec181`). What already matches: the row shows a vertical
ellipsis at its right edge on hover, clicking it opens the options, unpinning removes the ribbon
icon at once, the S key opens a shortcuts toolbox at the pointer with the pinned icons and a search
field.

## row options menu (P2)
- key: `ui:toolbar:row-options-menu`
- Fusion: The options are a small white panel right of the row with two checkboxes, "Pin to Toolbar" and "Pin to Shortcuts" (both checked for Extrude by default), then the plain entry "Change Keyboard Shortcut...". Unchecking Pin to Toolbar removes the icon; at a narrow window the next pinned command (Create Form) takes its visible slot and re-checking does not bring Extrude back into view.
- FissionOxide: Four plain entries: "Unpin from Toolbar" (or "Pin to Toolbar"), a greyed "Pin to Panel" ("Not in FissionOxide yet"), "Add to Shortcuts" (or "Remove from Shortcuts"), "Change Keyboard Shortcut...". No checkboxes, an extra entry Fusion does not have, and "Add to" instead of "Pin to" Shortcuts.
- Start here: `crates/ui/src/customize.rs:296` (the options menu: two `checkbox` rows labelled Pin to Toolbar / Pin to Shortcuts, drop Pin to Panel), `crates/ui/src/customize.rs:302`.

## change keyboard shortcut dialog (P2)
- key: `ui:toolbar:change-shortcut-dialog`
- Fusion: A modal window titled "Change Keyboard Shortcut" with the Fusion icon: "Assign a new shortcut key for Extrude:", a text field holding the current key in lower case ("e") with a blue underline, "Hints:", "You can combine modifiers like Shift, Control, Alt/Option, and Command with an alphanumeric key.", "Leave blank to remove the shortcut.", then "Reset to default" at the left and Cancel / OK (OK greyed until the key changes) at the right.
- FissionOxide: A floating egui window "Change Keyboard Shortcut" with the command name "Extrude", "Press the new key, with Ctrl, Shift or Alt if you like.", the key "E" in upper case, and four buttons OK / No Shortcut / Default / Cancel in one row.
- Start here: `crates/ui/src/customize.rs:370` (`Change Keyboard Shortcut` window: Fusion's prompt, hints, a text field, Reset to default at the left, Cancel then a greyed OK at the right).

## shortcuts toolbox style (P2)
- key: `ui:toolbar:shortcuts-toolbox-style`
- Fusion: S opens a light panel at the pointer titled "Design Shortcuts" (title case, centred), a row of the pinned command icons, and a "Search" text field with a thin border.
- FissionOxide: The panel is titled "DESIGN SHORTCUTS" (upper case, left aligned) and its field reads "Search commands".
- Start here: `crates/ui/src/toolbox.rs:134` (the title) and `crates/ui/src/toolbox.rs:161` (the search hint text).
