# Parity probe: Data Panel (2026-10-21, prober 7)

Same session as the two earlier reports of the day. Fusion's Data Panel (Personal licence, hub
"Collin Leckliter", projects FNG Customs, LumenArt, National Manufacturing Co, Default Project)
against FissionOxide's local hub (`main` at `42dec181`, one design "FissionOxide" in Default
Project). Fusion's window 907 px wide, FissionOxide's 1002 px.

## Method notes
- Fusion's panel opened into the last location (the Assets library); the home icon ("Return to
  project list") shows the project list. A single click on a project card only selects it; a
  double-click opens it.
- Fusion's search icon turns the header into a "Search All Data" bar; Escape does not leave it
  (the "<-" does), and clicks meant for the Data/People tabs landed in the empty search panel.
- FissionOxide's "New project" and "New folder" prompts ignore Escape; they stayed open under the
  later screenshots until their Cancel buttons were clicked.
- Fusion's panel was closed at the end so its ribbon returned to the full width.

## What matched
`ui:chrome:data-panel-pushes-ui`, `data-panel-refresh-tooltip`, `data-panel-card-no-tooltip`,
`data-panel-search-results`. The earlier `ui:chrome:data-panel` match row (the panel exists with
projects and files) stands; `ui:chrome:data-panel-reopens` was not re-checked (the panel stayed
closed until its button was used this session, which is the fixed behaviour).

## Differences, by priority

### P1
- `ui:chrome:data-panel-header`: hub dropdown, capacity "6 of 10", Refresh, Search (a bar that
  replaces the header), Close; no title. FissionOxide: "DATA PANEL" title, Local hub / Hub toggle,
  four tabs, an inline search field. `crates/ui/src/data_panel.rs:212`.
- `ui:chrome:data-panel-project-list`: Fusion's home is a project list with "ALL PROJECTS v"
  filters, a "New Project" button, cards with descriptions and cloud badges, LIBRARIES and a Filter
  field; FissionOxide has a project dropdown with "+" and "...". `crates/ui/src/data_panel.rs:349`.
- `ui:chrome:data-panel-project-view`: Data | People tabs, blue Upload, New Folder (inline name),
  Settings gear (sort, grid/list, offline cache), breadcrumb with "Open on the Web", empty text;
  FissionOxide: text buttons New folder / Upload / Save here and floating prompts.
  `crates/ui/src/data_panel.rs:349`.
- `ui:chrome:data-panel-file-menu`: 12 entries (Open ... Create Version) against 6.
  `crates/ui/src/data_panel.rs:491`.

### P2
- `ui:chrome:data-panel-tabs`: Fusion has no Recent / Activity / Trash tabs.
- `ui:chrome:data-panel-file-row`: Fusion's list row with date, Editable / Read-Only dropdown and
  a details expander (Uses / Used In / Drawings); FissionOxide's card with "v1 - 22 h ago" and a
  details block with Lock and versions. `crates/ui/src/data_panel.rs:491`.
- `ui:chrome:data-panel-upload-dialog`: Fusion's Upload window (Select Files, Drag and Drop,
  Location, Change Location) against the Windows file dialog. `crates/ui/src/data_panel.rs:991`.
- `ui:chrome:data-panel-button-tooltip`: "Data Panel" against "Show Data Panel".

## Retractions
None.

## Batches
| batch | items | keys |
|---|---|---|
| `ui-data-panel-2` | 8 | data-panel-header, data-panel-project-list, data-panel-project-view, data-panel-tabs, data-panel-file-row, data-panel-file-menu, data-panel-upload-dialog, data-panel-button-tooltip |

## Not covered
- Fusion's grid view, a folder's context menu, drag and drop into the panel, the Trash (no design
  was moved there), version history on a design with several versions (the Personal panel shows an
  Editable / Read-Only dropdown where the version selector sits), the Filter field, the People tab's
  web link, the hub dropdown's list, Fusion's search results layout.
