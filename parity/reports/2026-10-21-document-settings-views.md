# Parity probe: Document Settings, units, Named Views, Comments (2026-10-21, prober 7)

Same session as the earlier reports of the day; FissionOxide `main` at `42dec181`. The browser's
Document Settings rows (Units, Part Design), the Named Views folder and the COMMENTS header were
hovered, right-clicked and used in both apps.

## Method notes
- FissionOxide's Data Panel was still open when the first pass started, so the browser rows sat
  240 px to the right and the clicks landed on the panel's Upload and "+" controls (a file dialog
  and a New project prompt opened; both cancelled). Close the Data Panel before probing the
  browser.
- A right-click while a Windows file dialog is open is refused by the desktop control; cancel the
  dialog first.
- Desktop focus was taken twice by an invisible window of the remote-desktop host
  (`remoting_desktop`); `open_application` for FissionOxide started a second instance (killed)
  but gave the focus back.

## What matched
`ui:browser:units-row-menu`, `named-views-default-rows`, `named-views-folder-menu`,
`named-view-no-row-menu`.

## Differences (all P2)
- `ui:browser:units-dialog`: Fusion's docked CHANGE ACTIVE UNITS (Unit System dropdown, Length,
  Mass, Set as Default, OK / Cancel) against a popup radio list of five length units.
  `crates/ui/src/panels/browser_rows.rs:488`.
- `ui:browser:row-edit-pencils`: the hover pencil sits on Units in FissionOxide and on Part Design
  in Fusion. `crates/ui/src/panels.rs:1453`.
- `ui:browser:convert-design-type` (fox-missing): no Part Design menu; Fusion's Convert Design Type
  and CONVERT UNTITLED dialog. `crates/ui/src/panels.rs:1472`.
- `ui:browser:new-named-view-inline-name`: "Named View 1" with a status message against
  "NamedView" in an inline editor. `crates/ui/src/panels.rs:1485`.
- `ui:browser:named-view-icon`: a filled square against Fusion's camera-view glyph.
- `ui:chrome:comments-expand` (reopened): a floating Comments window against Fusion's docked panel
  with the unsaved-design text. `crates/ui/src/comments.rs:46`.

## Retractions
None.

## Batches
| batch | items | keys |
|---|---|---|
| `ui-document-settings-views` | 6 | units-dialog, row-edit-pencils, convert-design-type, new-named-view-inline-name, named-view-icon, comments-expand (reopened) |

## Not covered
- Changing the unit and its effect on dimension readouts and the Units row text; Set as Default;
  renaming, updating or deleting a named view; restoring a named view that differs from the current
  camera in FissionOxide; comments on a saved document (adding, replying, screenshots).
