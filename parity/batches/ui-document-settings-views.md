# Parity batch: ui-document-settings-views

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-21 (prober 7) on the browser's Document Settings rows, the Named Views folder
and the COMMENTS header of an unsaved design in each app; FissionOxide built from `main` at
`42dec181`. What already matches: the Units row's right-click menu ("Change Active Units"), the
four default named views TOP / FRONT / RIGHT / HOME, the folder menu ("New Named View"), and a
named view row having no context menu.

## units dialog (P2)
- key: `ui:browser:units-dialog`
- Fusion: Change Active Units opens a docked dialog "CHANGE ACTIVE UNITS": a "Unit System" dropdown ("inch (in), ouncemass"), greyed Length ("inch (in)") and Mass ("ouncemass") dropdowns, a greyed "Set as Default" checkbox, OK (greyed until a change) and Cancel; while it is open the Units row is highlighted with a pencil icon and the status bar reads "Units". Escape closes it.
- FissionOxide: Change Active Units opens a small popup list "Change Active Units" with radio rows Millimeter (mm), Centimeter (cm), Meter (m), Inch (in), Foot (ft) and no OK / Cancel; Escape closes it.
- Start here: `crates/ui/src/panels/browser_rows.rs:488` (`units_menu`) and the popup it opens; make it a docked dialog with the Unit System dropdown (length and mass pairs), Length, Mass, Set as Default, OK / Cancel.

## row edit pencils (P2)
- key: `ui:browser:row-edit-pencils`
- Fusion: Hovering the "Part Design" row shows a pencil icon at its right (the Convert Design Type entry); hovering the Units row shows no icon.
- FissionOxide: Hovering the Units row shows a pencil at its right; hovering Part Design shows nothing.
- Start here: `crates/ui/src/panels.rs:1453` (the Units row) and `crates/ui/src/panels.rs:1472` (the Part Design row): move the hover pencil from Units to Part Design.

## convert design type (P2)
- key: `ui:browser:convert-design-type`
- Fusion: Right-click on "Part Design" (or its pencil) lists "Convert Design Type" with a pencil icon; it opens a docked dialog "CONVERT UNTITLED" with a "Design Type" dropdown ("Part" with an icon), a greyed Convert and Cancel; the status bar reads "Design Experience".
- FissionOxide: Right-click on Part Design opens nothing.
- Start here: `crates/ui/src/panels.rs:1472` (the Part Design row): add the menu entry and a dialog whose dropdown lists the design types FissionOxide has (Part, Sheet Metal ...), greying the rest.

## new named view inline name (P2)
- key: `ui:browser:new-named-view-inline-name`
- Fusion: New Named View adds a row "NamedView" under HOME with its name in an inline editor (selected, ready to retype); Escape keeps "NamedView"; nothing is printed in the status bar.
- FissionOxide: New Named View adds "Named View 1" with no editor and prints "Saved the view as 'Named View 1'" at the lower left.
- Start here: `crates/ui/src/panels.rs:1485` (`New Named View`): name it "NamedView" and begin the browser's inline rename (`browser_edit::begin`), drop the status message.

## named view icon (P2)
- key: `ui:browser:named-view-icon`
- Fusion: Named view rows carry a small camera-view icon (a framed picture glyph) left of the name.
- FissionOxide: The rows carry a small filled-square glyph.
- Start here: `crates/ui/src/icons.rs` (`Icon::Views` or the icon used by the named view rows in `crates/ui/src/panels.rs:1479`).

## comments expand (P2)
- key: `ui:chrome:comments-expand`
- Fusion: The "+" at the right of the COMMENTS header (and the header itself) expands a docked panel above the header, inside the browser column, whose header reads "COMMENTS" with a "-" collapse button and whose body reads "Save this document to add comments, screenshots, and attachments." for an unsaved design; the browser shrinks to make room.
- FissionOxide: Clicking the COMMENTS header opens a small floating "Comments" window at the lower left with a "+" button and "No comments yet"; the next click on the header closes it.
- Start here: `crates/ui/src/comments.rs:46` (the floating window with `float_header`): dock it above the COMMENTS header in the browser column, show Fusion's unsaved-design text, and add the "-" collapse.
