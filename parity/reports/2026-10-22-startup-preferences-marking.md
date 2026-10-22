# 2026-10-22, prober 20: start-up and documents, Preferences pages, marking menus

FissionOxide `main` at `379c2ae2` (release build), Fusion Personal with four unsaved tabs
("Untitled*", "Untitled*(1)", "Untitled(2)" Assembly Design, "Untitled*(3)" sheet metal). Both
windows at screenshot x 0..908. Nothing was saved to Fusion's cloud; Fusion was not quit (its window
close was cancelled at the Save dialog) and its update card was not answered.

## What matched
- A tab right-click opens no menu and a tab shows no tooltip, in both (`ui:documents:tab-right-click`).
- Closing the active rightmost tab shows its left neighbour; untouched tabs close without asking
  (`ui:documents:close-activates-left`).
- File > New... is the same "What do you want to design?" chooser as "+" (`ui:documents:file-new-chooser`).
- Open Recovered Documents (0) is greyed in both (`ui:documents:recovered-documents-entry`; FissionOxide's
  count is hard-coded and its recovery backups are never offered, not exercised in Fusion).
- A timeline item's right-click is a plain list without a ring in both (`ui:marking-menu:timeline-item-list`).

## Differences

### P1
- `ui:documents:window-close-unsaved`: FissionOxide's window X quit at once with an unsaved sketch;
  Fusion's opens the Save dialog for the active unsaved tab, and Cancel keeps everything open.
- `ui:marking-menu:ring-slot-order`: the ring's slots are one position off. Fusion: N Repeat, NE Press
  Pull, E Redo, SE Hole, S Sketch v, SW Move/Copy, W Undo, NW Delete. FissionOxide: N Repeat, NE Delete,
  E Press Pull, SE Redo, S Hole, SW Sketch, W Move/Copy, NW Undo. The earlier "match" row
  (`ui:marking-menu:ring`) compared the list of names, not where each sits.
- `ui:browser:bodies-folder-eye-hides`: FissionOxide's Bodies folder eye greys the rows but the body stays
  drawn; Fusion hides the bodies.

### P2
- Tabs: no minimum width, scrolling or "v" overflow list (`ui:documents:tab-overflow-menu`; at six tabs the
  lock glyph overlaps the X); no Ctrl+Tab / Ctrl+Shift+Tab (`ui:documents:ctrl-tab-switch`); window title
  carries the user name (`ui:chrome:window-title-format`); the chooser's size, tiles, descriptions, inline
  labels, Type tooltips and Drawing options (`ui:documents:new-chooser-details`).
- Application bar: Fusion's house icon opens a Home page (Start working, Recent, ...) that FissionOxide lacks
  (`ui:chrome:home-start-page`); Check for Updates goes to a Job Status card in Fusion and a centred window in
  FissionOxide, whose Job Status and "Recent activity" icons are inert (`ui:chrome:check-updates-job-status`);
  the profile menu has two of Fusion's six entries (`ui:chrome:profile-menu`).
- Preferences: tree nodes (Design / Manufacture / Electronics / Graphics / Unit and Value Display children,
  one "Simulation and Generative Design" node, Default Units' children and order), the window's size, icon,
  X, label alignment and remembered page, and every page's rows (`ui:chrome:preferences-*`, eight rows).
  FissionOxide has rows only on General, Design, Graphics and Default Units > Design.
- Marking menus: a right-click over a selected body re-picks the face (`ui:marking-menu:selected-body`); a
  vertex and a construction plane get the empty-canvas list (`ui:marking-menu:vertex`,
  `ui:marking-menu:construction-plane`); inside a sketch Fusion showed the 3D ring and a list with Line /
  2-Point Rectangle / Center Diameter Circle where FissionOxide has a sketch ring and a four-entry list
  (`ui:marking-menu:in-sketch-ring`, seen once, recheck); sketch-point lists differ
  (`ui:marking-menu:sketch-point`); the selection readout in a sketch is drawn over the navigation bar
  (`ui:canvas:readout-beside-palette`).

## Re-verified
- `ui:assemble:assembly-design-document`: verified (Assembly Design tab with ASSEMBLY / MANAGE / UTILITIES).
- `ui:assemble:assembly-new-component`: verified (Parent, Design Type, Part Type, External1, Default Project, Activate).
- `ui:insert:file-chooser`: reopened (empty PROJECT list, "Default Project" heading drawn twice), now in
  `ui-appbar-home-updates`.
- `ui:marking-menu:ring-at-cursor`: verified. `ui:chrome:preferences-layout`: verified (size and look in
  `ui:chrome:preferences-window-look`).
- `ui:chrome:preferences-general-settings`: reopened (ten of Fusion's General rows still missing), now in
  `ui-preferences-pages`. `ui:context-menus:face-list-contents`: reopened (no OK / Cancel at the top), now in
  `ui-marking-menu-targets`.

## Batches (wave 35, `parity/batches/map-wave35.json`)
| Batch | Items | Files |
|---|---|---|
| ui-documents-window-tabs | 5 (one P1) | documents.rs, lib.rs, panels.rs window_title, apps/fissionoxide/src/main.rs; after ui-documents-tabs-close |
| ui-appbar-home-updates | 4 + reopened ui:insert:file-chooser (one P1) | panels.rs, updates.rs, viewport.rs / scene.rs, insert_window.rs |
| ui-preferences-pages | 8 + reopened ui:chrome:preferences-general-settings | preferences.rs |
| ui-marking-menu-targets | 7 + reopened ui:context-menus:face-list-contents (one P1) | marking.rs, viewport.rs, panels.rs readout_anchor |

## Not covered
- Fusion's start-up screen at launch and closing Fusion's last tab (Fusion holds the owner's unsaved tabs
  and was not quit); FissionOxide starts in a new Untitled design with the Data Panel open and puts its
  window at a fixed spot each launch.
- Fusion's Update Now / Update Later (not pressed); a real recovered document in either app.
- Fusion's Electronics Preferences sub-pages (names only), most of Manufacture's long page, the Preview
  Features list contents.
- Marking menus on an edge-vertex without a coincident sketch point in Fusion's Part Design (done on the
  sheet metal flange), on construction axes and points, a body in FissionOxide outside a selection, and a
  sketch point outside a sketch in FissionOxide.
