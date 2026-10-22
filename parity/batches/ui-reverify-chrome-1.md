# Parity batch: ui-reverify-chrome-1

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Found on 2026-10-22 by the re-verification sweep (verifier 2) re-checking the chrome area's
`fixing-merged` keys in the rebuilt app (`main` at `76f67b2f`). All eight are reopened keys (the
fix landed but part of Fusion's behaviour is still missing). The rows are already in
`parity/ui-features.jsonl` (do not run `ledger_from_batches.py` for this batch); read each row's
`history` before starting.

## Dark theme darkens only Preferences, and cannot be undone (P1)
- key: `ui:chrome:dark-theme`
- Fusion: Preferences > General > Theme: Dark turns the whole UI dark (application bar, toolbar, browser, panels, dialogs, canvas background).
- FissionOxide: Dark + Apply / OK darkens only the Preferences dialog; everything else stays light. In the dark dialog the heading bar turns into a blank white strip and the Theme dropdown no longer opens, so the theme cannot be set back from the UI (the verifier edited `Theme=light` in `%LOCALAPPDATA%\FissionOxide\settings.ini` with the app closed).
- Start here: `crates/ui/src/theme.rs:162` (`Theme::Dark => egui::ThemePreference::Dark`) and the app's own colour constants in `crates/ui/src/theme.rs`, which are light-only.

## A project opens with one click (P1)
- key: `ui:chrome:data-panel-project-list`
- Fusion: one click on a project card opens the project.
- FissionOxide: a click only outlines the card; a double-click opens it only on the icon or bare frame; a double-click on the card's name or description selects that word as text.
- Start here: `crates/ui/src/data_panel.rs:765` (`r.clicked()` picks, `r.double_clicked()` opens) and the labels inside `card_frame` (`:662`), which take the clicks: make them non-selectable.

## Text Commands panel (P2)
- key: `ui:chrome:text-commands`
- Fusion: a TEXT COMMANDS bar under the timeline; File > View reads "Hide Text Commands Ctrl+Alt+C".
- FissionOxide: no bar, and File > View shows "Show Text Commands" greyed.
- Start here: `crates/ui/src/text_commands.rs` and `crates/ui/src/file_menu.rs:145`.

## File > View extras (P2)
- key: `ui:chrome:view-submenu`
- Fusion: the nine Hide/Show entries and Reset to Default Layout, nothing else.
- FissionOxide: the nine match apart from the greyed Text Commands entry, then a separator and greyed "Explode" and "Render Components...".
- Start here: `crates/ui/src/explode_ui.rs:87` and `:102`; move them to where Fusion has them (the ANIMATION workspace) or drop them from this submenu.

## Data Panel list view rows (P2)
- key: `ui:chrome:data-panel-file-row`
- Fusion: in list view a row is a thumbnail, an orange badge before the bold name, the date, a dropdown button at the right and a "v" expander for the details card.
- FissionOxide: at the panel's 242 px width every row collapses into a narrow column: the name is cut to one letter, the date wraps a few characters per line, the badge overlaps it; the dropdown and expander are not reachable. Grid view is fine.
- Start here: `crates/ui/src/data_panel.rs:1241` (`list_row`).

## Open window's project tree (P2)
- key: `ui:chrome:open-dialog`
- Fusion: the tree lists Default Project, New Project and a bold Samples group (Workshops & Events, CAM Samples, ...) down the left pane.
- FissionOxide: Default Project is in the tree, but "+ New Project", "Samples" and every sample group name are drawn on top of each other in one spot beside the breadcrumb.
- Start here: `crates/ui/src/file_dialogs.rs:243` (the sample list) and the tree layout above it.

## Export formats Fusion writes (P2)
- key: `ui:chrome:export-dialog`
- Fusion: Autodesk Inventor 2025 (*.ipt) and FBX (*.fbx) are live in the Type list; Fusion Archive (*.f3d) is the default.
- FissionOxide: IPT and FBX are greyed, and the default is FissionOxide's own archive with .f3d greyed.
- Start here: `crates/ui/src/file_dialogs.rs:561` and `:564`, and the exporters in `crates/engine`.

## Missing-glyph boxes (P2)
- key: `ui:chrome:arrow-glyph-boxes`
- Fusion: no missing-glyph boxes anywhere.
- FissionOxide: boxes where the Data Panel's hub-name and ALL PROJECTS down arrows, the Upload window's close X, the Preferences window's close button and a mark after PARAMETERS' "Parameter" column header should be.
- Start here: `crates/ui/src/data_panel.rs:100` and `:500` (header), the Upload and Preferences window titles, the PARAMETERS table header; draw these as painted icons (as `Icon::Close` already is for PARAMETERS' X) or use characters the bundled fonts have.
