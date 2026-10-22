# 2026-10-22, prober 16: PLASTIC, MANAGE and UTILITIES tabs

Fusion (Personal licence) and FissionOxide built from `main` at `c94d76ac`, both windows 908 px wide
at the left of the screen (Fusion maximised once to read the full-width PLASTIC ribbon). Fusion's
"Untitled*" document (inches, one body, never saved); FissionOxide's new empty design (mm).

## PLASTIC tab

On this account Fusion's PLASTIC tab carries SOLID's menus with its own quick icons; Plastic Rules,
Boss, Snap Fit, Rest, Lip/Groove and Rule Fillet are not offered anywhere (menus, the browser's
component menu, Preferences > Preview Features). They are recorded `blocked`
(`ui:plastic:plastic-features`): a prober on an account that shows them should probe them.

Matches: panels and quick icons at full width (`ui:plastic:panels-quick-icons`), the CREATE list
(`ui:plastic:menu-lists`). Shared differences already open: MODIFY's extra Direct Edit
(`ui:toolbar:solid-modify-menu-list`), the 908 px collapse (MODIFY 6 icons against Fusion's 4,
`ui:toolbar:responsive-collapse`). New: the tooltip texts of Web, Rib, Create Form, Split Body,
Move/Copy and Bill of Materials (`ui:plastic:tooltip-texts`).

## MANAGE tab

Same single BOM panel and dropdown (`ui:manage:tab-panel`). Fusion's Bill of Materials is a wide
palette with Structured / Working dropdowns, link, download, Column Settings and a Refresh BOM /
Compare to Model menu, a table header and an empty state for unsaved designs; FissionOxide's is a
small "No components" dialog (`ui:manage:bom-palette`, `ui:manage:bom-toolbar-menus`, P1/P2).

## UTILITIES tab

Dropdown lists match except the ADD-INS shortcut hint and icon and INSPECT's extra Properties
(`ui:utilities:menu-lists`, notes on `ui:menus:inspect-properties-extra`). Differences:

- P1 `ui:utilities:scripts-addins-window`: Fusion's three-pane Scripts and Add-Ins window against
  FissionOxide's small Plugins window (`crates/ui/src/plugins_ui.rs:98`).
- P1 `ui:utilities:make-3d-print`: MAKE > 3D Print opens a Save As for STL instead of the 3D PRINT
  dialog that File > 3D Print opens (`crates/ui/src/panels.rs:761`).
- P1 reopened `ui:dialogs:manage-materials`: the Material Browser's library rows overlap on one line.
- P2 `ui:utilities:scripts-addins-shortcut`: Shift+S opens the S toolbox, not Scripts and Add-Ins.
- P2 `ui:utilities:plugin-details-glyphs`: "→" draws as a box in the plugin details.
- P2 `ui:utilities:tooltip-texts`: eight UTILITIES tooltips.
- P2 reopened `ui:chrome:status-dev-messages`: Compute All writes "Computed in 0.0 s".

Blocked: Manage Threads Library (subscription card). Match: Compute All (`ui:utilities:compute-all`).

## Verified

`ui:toolbar:tooltip-style`, `ui:toolbar:tooltip-expanded-illustration` (the picture is a drawn
stand-in), `ui:toolbar:tooltip-texts-solid` (Extrude), `ui:dialogs:rib-rows`, `ui:dialogs:web-rows`,
`ui:chrome:3d-print-dialog` (File menu path).

## Batches (wave 31, `parity/batches/map-wave31.json`)

| Batch | Items | Files |
|---|---|---|
| ui-manage-bom-palette | 2 | dialogs.rs (Kind::Bom), menus.rs |
| ui-utilities-addins-make | 4 | plugins_ui.rs, panels.rs, customize.rs, menus.rs |
| ui-utilities-tips-status | 4 | tips.rs, material_ui.rs, lib.rs |

## Not covered

The plastic commands (blocked), SELECT's dropdown on these tabs (shared with SOLID, overflows under
the overlay), the Marketplace entry (opens a web page), running a script or add-in in Fusion, Fusion's
BOM with a saved design (rows, Parts Only view), Fusion's Draft / Accessibility / Interference dialogs
from UTILITIES (the same commands as SOLID's INSPECT, probed by prober 12), FissionOxide's BOM with
components.
