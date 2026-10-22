# Parity probe: File menu dialogs (Save, Open, Export, 3D Print, Capture Image, View) (2026-10-21, prober 7)

Same session as the earlier reports of the day; an unsaved design in each app (FissionOxide `main`
at `42dec181`). Every File menu entry that opens something was opened and cancelled in both apps.
Fusion's current File menu has no Open Recent, New Drawing or Insert into Design entries (the
coverage line names them from an older menu), so those were not probed.

## Method notes
- Fusion's modal web dialogs (Save, Open, Export) hand keyboard focus to a helper window the
  desktop control cannot reach ("Unidentified app"): `key` calls fail while one is open, and
  `open_application` on Fusion started a second instance ("Fusion is already open"). Close them
  with their Cancel buttons by mouse, not with Escape.
- Fusion's first click on "Open..." right after the menu opened only showed the entry's tooltip;
  the second click opened the dialog.
- FissionOxide's floating windows ignore Escape and stack at the same place; cancel each one.

## What matched
`ui:chrome:file-menu-tooltip`, `file-menu-greyed-cloud-entries`, `capture-image-options`; the
existing `ui:chrome:save-dialog` match stands for the first-save flow.

## Differences, by priority

### P1
- `ui:chrome:open-dialog`: Fusion's own Open window (project tree with Samples, NAME / LAST
  UPDATED table, "Open from my computer...") against the Windows Open dialog.
  `crates/ui/src/lib.rs:378`.
- `ui:chrome:export-dialog`: Fusion's Export window (Name, a Type dropdown of 14 formats with
  the unsupported ones greyed, Location with "...", a help link) against an Export submenu of eight
  formats that go straight to the Windows save dialog. `crates/ui/src/panels.rs:282`.
- `ui:chrome:windows-ignore-escape`: Save, 3D Print, Capture Image, New project and New folder
  windows stay open on Escape. `crates/ui/src/capture.rs:198`.

### P2
- `ui:chrome:file-menu-items` (reopened): Save As enabled on an unsaved design, Export... as a
  submenu, separator placement, no entry tooltips. `crates/ui/src/panels.rs:243`.
- `ui:chrome:save-dialog-layout`: Fusion's wide modal with Name and Location only against a small
  window with Description and Milestone rows and buttons at the left.
  `crates/ui/src/data_panel.rs:1024`.
- `ui:chrome:view-submenu`: Fusion's nine Hide/Show entries with Ctrl+Alt shortcuts against four
  checkboxes plus greyed Explode and Render Components. `crates/ui/src/panels.rs:296`.
- `ui:chrome:3d-print-dialog`: Fusion's docked 3D PRINT with a Preparation Type row against a
  large floating window with Refinement and Format. `crates/ui/src/capture.rs:275`.
- `ui:chrome:capture-image-dialog`: Fusion's Image Options (width and height editable with a
  unit dropdown, Resolution, Lock Aspect Ratio, OK only) against a large window with greyed
  sizes and OK / Cancel. `crates/ui/src/capture.rs:198`.

## Retractions
None.

## Batches
| batch | items | keys |
|---|---|---|
| `ui-file-dialogs` | 8 | file-menu-items (reopened), open-dialog, export-dialog, save-dialog-layout, view-submenu, 3d-print-dialog, capture-image-dialog, windows-ignore-escape |

## Not covered
- New... (opens a new tab in both), Upload... from the File menu (the Data Panel's Upload was
  covered), completing a Save or Export, the Save As flow on a saved design, Save As Latest, Create
  Version, Fusion's "Open from my computer" picker, the Export Location "..." picker, the
  Performance Overlay and Reset to Default Layout entries.
