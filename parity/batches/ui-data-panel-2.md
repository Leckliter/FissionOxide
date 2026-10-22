# Parity batch: ui-data-panel-2

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-21 (prober 7) with Fusion's Data Panel (Personal licence, the "Collin
Leckliter" hub) and FissionOxide's local hub (`main` at `42dec181`). Both panels slide in at the
left and push the browser and canvas right (Fusion about 300 px wide, FissionOxide about 240 px);
both refresh buttons read "Refresh"; neither shows a tooltip on a design card. Everything else
differs; the whole panel is `crates/ui/src/data_panel.rs` (`show` at line 212).

## data panel header (P1)
- key: `ui:chrome:data-panel-header`
- Fusion: The header row is the hub name with a down arrow ("Collin Leckliter v", tooltip "Fusion Hub with Collaborative Editing"), a capacity counter with a document icon ("6 of 10", tooltip "Capacity"), a Refresh icon, a Search icon (tooltip "Search"; clicking turns the header into a "<- [Search All Data] [magnifier] x" bar and empties the list until a term is typed; the "<-" leaves it, Escape does not) and an X that closes the panel. No panel title.
- FissionOxide: A "DATA PANEL" title with a refresh and an X ("Close the data panel"), a "Local hub" / "Hub" toggle pair, and an always-visible "Search designs, versions, milestones" field below the project row.
- Start here: `crates/ui/src/data_panel.rs:212` (`show`: the header), `crates/ui/src/data_panel.rs:280` (`hub_location`: make it the hub dropdown), the search field at `crates/ui/src/data_panel.rs:349`.

## data panel project list (P1)
- key: `ui:chrome:data-panel-project-list`
- Fusion: The panel's home is a project list: a "Back >" link at the top right, "ALL PROJECTS v" (a dropdown: All Projects, Joined by Me, Pinned, Owned by Me, Shared with Me) with a blue "New Project" button, then cards with an icon, a bold name and a grey description ("My Recent Data / A list of what you've recently been working on", "My Editable Documents / A list of your editable documents", team projects with a cloud badge at the right, "Admin Project / This project is for team members with Admin privileges in <hub>", "Default Project / This comes with Fusion and is where your work is stored until you create or join another project.", "New Project"), a "LIBRARIES" section ("Assets") and a "Filter" field at the bottom. A single click selects a card (blue border), a double-click opens it; the home icon in a project's breadcrumb ("Return to project list") comes back here.
- FissionOxide: No list: a "Default Project" dropdown of project names with a "+" (tooltip "New project": a floating "New project" prompt with a name field, OK, Cancel) and a "..." menu (Rename project..., Delete project).
- Start here: `crates/ui/src/data_panel.rs:349` (`files_tab`: the project dropdown row), `crates/ui/src/data_panel.rs:798` (`prompt_window`).

## data panel project view (P1)
- key: `ui:chrome:data-panel-project-view`
- Fusion: Inside a project: "Data" | "People" tabs (People shows an illustration and a "Manage members" card: "View and add members to this folder with the Fusion web client."), a blue "Upload" button, "New Folder" and a Settings gear (tooltip "Settings": radio Sort by Last Updated / Sort by Name, radio View as Grid / View as List, then View Offline Cache and Cache This Project), a breadcrumb "home > Default Project" with an "Open on the Web" icon at the right, and the empty text "Saved and Uploaded files will appear here" centred in grey. New Folder adds a folder row at once with its name "New Folder" selected in an inline editor.
- FissionOxide: A "Default Project" breadcrumb link, then three text buttons "New folder", "Upload", "Save here" (tooltip "Save the open design as a new design in this folder"); no People tab, no gear, no Open on the Web; New folder opens a floating "New folder" prompt (name field, OK, Cancel) that Escape does not close.
- Start here: `crates/ui/src/data_panel.rs:349` (`files_tab`), `crates/ui/src/data_panel.rs:798` (`prompt_window`: close on Escape, or rename inline as Fusion does).

## data panel tabs (P2)
- key: `ui:chrome:data-panel-tabs`
- Fusion: There are no Recent, Activity or Trash tabs: recent designs are the "My Recent Data" project in the list, Trash is reached from a design's "Move to Trash" entry, and the only tabs are Data | People inside a project.
- FissionOxide: Four tabs Files / Recent / Activity / Trash at the top (Recent: the design cards with their project; Activity: "Leckliter created FissionOxide 22 h ago" lines; Trash: "The trash is empty.").
- Start here: `crates/ui/src/data_panel.rs:252` (the tab row), `crates/ui/src/data_panel.rs:741` (`activity_tab`), `crates/ui/src/data_panel.rs:761` (`trash_tab`): fold Recent into a "My Recent Data" project card and Activity/Trash into the project list or drop them.

## data panel file row (P2)
- key: `ui:chrome:data-panel-file-row`
- Fusion: In list view a design row is a thumbnail, an orange design badge before the bold name, the date "12/15/2025 1:34 AM", a small dropdown button at the right (its menu: "Editable" / "Read-Only" with a check on the current mode) and a "v" expander under it that opens a details card: the name, "Fusion Design", a "View Details on Web" link, "Last updated 12/15/2025 1:34 AM by Collin Leckliter", "Stored in LumenArt > Products", tabs "Uses | Used In | Drawings" and "This item does not reference any other designs."
- FissionOxide: A card with a thumbnail, the name and "v1 - 22 h ago"; a click selects it (blue card) and opens a details block under the list: the name with Insert / Open buttons, "Not locked" with a Lock button, "1 versions" and a version row ("v1 latest", "Welcome to FissionOxide", "2026-10-21 04:41 - Leckliter", Open / Milestone / comment buttons).
- Start here: `crates/ui/src/data_panel.rs:491` (`item_rows`), `crates/ui/src/data_panel.rs:589` (`details`).

## data panel file menu (P1)
- key: `ui:chrome:data-panel-file-menu`
- Fusion: Right-click on a design lists Open, Insert into Current Design, Create Drawing, Show in Location, Share Link, Import New Change, Rename, Move, Copy, Move to Trash, Add to Offline Cache, Create Version (greyed).
- FissionOxide: Open, Insert into Current Design, Rename..., Copy, Download..., | Delete.
- Start here: `crates/ui/src/data_panel.rs:491` (`item_rows`: the card's `context_menu`); grey the cloud-only entries (Share Link, Import New Change, Add to Offline Cache, Create Version) as the browser's root menu does.

## data panel upload dialog (P2)
- key: `ui:chrome:data-panel-upload-dialog`
- Fusion: Upload opens a white modal "Upload" window: a blue "Select Files" button, "or", a "Drag and Drop Here" box with an illustration, "Location:" with the project name and a "Change Location" link, Cancel and a disabled Upload button.
- FissionOxide: Upload opens the Windows file dialog "Upload to the data panel" at once.
- Start here: `crates/ui/src/data_panel.rs:991` (`upload_dialog`): an Upload window first, the file picker from its Select Files button.

## data panel button tooltip (P2)
- key: `ui:chrome:data-panel-button-tooltip`
- Fusion: The grid button in the application bar shows the tooltip "Data Panel".
- FissionOxide: The tooltip reads "Show Data Panel".
- Start here: `crates/ui/src/panels.rs` (the application bar's grid button tooltip string).
