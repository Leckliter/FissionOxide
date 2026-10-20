#!/usr/bin/env python3
"""Seeds parity/ui-features.jsonl: the interaction-level Fusion behaviours the API catalog can't
see (hover highlights, ViewCube hot spots, menus, drag gestures, ...).

    py tools/parity-loop/seed_ui.py

Each line: {"key", "area", "name", "fusion", "status", "history"}. `fusion` says what Fusion does
so the probe knows what to look for. The discovery pass of the loop appends more entries with
`loop.py add`. Re-running keeps every existing entry's status, history and notes.
"""

import json
import os

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
OUT = os.path.join(ROOT, "parity", "ui-features.jsonl")

SEED = {
  "viewcube": [
    ("face-click", "Clicking a face (TOP, FRONT, ...) animates the camera to that standard view."),
    ("edge-click", "Clicking one of the 12 edges animates to the 45-degree view between the two faces."),
    ("corner-click", "Clicking one of the 8 corners animates to that isometric view."),
    ("hover-highlight", "Hovering a face, edge or corner highlights that hot spot in blue before clicking."),
    ("home-button", "The house icon above the cube returns to the Home view, animated."),
    ("rotate-arrows", "With a face view active, curved arrows rotate the view 90 degrees about the view axis."),
    ("adjacent-arrows", "With a face view active, triangle arrows on each side roll to the adjacent face."),
    ("drag-orbit", "Dragging the cube orbits the model freely."),
    ("menu", "The dropdown next to the cube: Go Home, Orthographic / Perspective, Set current view as Home or Front, Reset Front."),
    ("compass-ring", "The compass ring under the cube can be dragged to spin the view about up."),
    ("fade-when-idle", "The cube is translucent until the pointer approaches."),
    ("axis-triad", "A small XYZ triad sits at the cube's lower-left corner."),
  ],
  "navbar": [
    ("orbit", "The bottom navigation bar has Orbit (free / constrained), Look At, Pan, Zoom, Fit."),
    ("display-settings", "Display Settings flyout: visual style, environment, effects, object visibility, camera, ground plane."),
    ("grid-snaps", "Grid and Snaps flyout: layout grid, snap to grid, grid settings, incremental move."),
    ("viewports", "Viewports flyout: single / multiple views."),
    ("look-at", "Look At, then a face, rotates the camera normal to it."),
  ],
  "canvas": [
    ("hover-face", "Hovering a face highlights it (blue tint) before any click."),
    ("hover-edge", "Hovering an edge thickens and highlights it."),
    ("hover-vertex", "Hovering a vertex shows a point highlight."),
    ("hover-sketch", "Hovering a sketch curve highlights it; profiles highlight on hover when a command wants a profile."),
    ("click-select", "Click selects; selected entities draw in a stronger blue than hover."),
    ("ctrl-shift-select", "Ctrl or Shift+click adds to or removes from the selection."),
    ("window-select", "Left-to-right drag is window select, right-to-left is crossing select, with different box styles."),
    ("select-other", "Press and hold the left button shows Select Other, a list of the entities under the pointer."),
    ("middle-pan", "Middle drag pans; Shift+middle drag orbits; the wheel zooms toward the pointer."),
    ("double-click-loop", "Double-clicking an edge selects its tangent chain or loop."),
    ("origin-planes", "Commands that need a plane show the origin planes as translucent squares that highlight on hover."),
    ("tooltip-on-hover", "Hovering an entity during a command shows a small context tooltip (Select a plane or planar face)."),
    ("empty-click-clears", "Clicking empty space clears the selection."),
    ("escape-clears", "Escape cancels the command, then clears the selection."),
  ],
  "marking-menu": [
    ("ring", "Right-click opens an 8-slot ring: Repeat, Delete, Press Pull, Redo, Hole, Sketch, Move/Copy, Undo."),
    ("overflow", "A list under the ring: OK/Cancel, Pan, Zoom, Orbit options, Show All, Workspace, recent commands with shortcuts."),
    ("context-aware", "Ring and list change for faces, edges, bodies, sketches and browser items."),
    ("flick", "Right-drag in a direction runs that slot without waiting for the menu (marking gesture)."),
    ("hover-preview", "Hovering a slot highlights it and shows the command's tooltip."),
  ],
  "toolbar": [
    ("workspace-switcher", "Workspace dropdown at the left (Design, Generative, Render, ...)."),
    ("tabs", "Tabs per workspace: SOLID, SURFACE, MESH, SHEET METAL, PLASTIC, UTILITIES, MANAGE, TOOLS."),
    ("panel-dropdowns", "Each panel name (CREATE, MODIFY, ...) opens a dropdown with every command."),
    ("pin-to-toolbar", "Commands in dropdowns can be pinned to the panel or the toolbar."),
    ("rich-tooltip", "Hovering a command shows a tooltip; resting longer expands it with an illustration and description."),
    ("contextual-tab", "Entering a sketch switches to a SKETCH tab with FINISH SKETCH at the right."),
    ("qat", "Quick access toolbar: data panel, File menu, Save, Undo, Redo."),
  ],
  "browser": [
    ("tree", "Browser tree: document settings, named views, origin, bodies, sketches, components."),
    ("eye-toggle", "Each node has a visibility eye that toggles on click."),
    ("hover-highlight", "Hovering a browser node highlights the entity in the canvas."),
    ("context-menu", "Right-click on a node gives a node-specific menu (Rename, Delete, Create Components from Bodies, ...)."),
    ("rename-dblclick", "Double-click or slow-click a name to rename it in place."),
    ("drag-reorder", "Bodies and components can be dragged between folders and components."),
    ("activate-component", "The radio button next to a component activates it."),
    ("collapse", "The browser panel collapses to a narrow strip and can be pinned."),
  ],
  "timeline": [
    ("icons", "Each feature is an icon in the timeline at the bottom; hovering highlights the feature in the canvas."),
    ("dblclick-edit", "Double-clicking an icon reopens the feature's dialog."),
    ("rollback-marker", "Dragging the marker rolls the model back; features after it grey out."),
    ("drag-reorder", "Dragging an icon reorders features, with a red line where it can't go."),
    ("context-menu", "Right-click: Edit Feature, Edit Profile Sketch, Suppress, Delete, Roll History Marker Here, Create Group."),
    ("playback", "Playback buttons: to beginning, step back, play, step forward, to end."),
    ("error-badges", "Failed features turn red or yellow with a warning tooltip."),
  ],
  "sketch": [
    ("heads-up-input", "Drawing a line or rectangle shows length and angle boxes that follow the cursor; Tab cycles them."),
    ("inference-lines", "Dotted inference lines snap to horizontal, vertical and other points."),
    ("constraint-glyphs", "Constraints draw as glyphs next to geometry; hovering a glyph highlights the constrained items."),
    ("dimension-leaders", "Dimensions draw with extension lines, arrows and the value; double-click edits the value."),
    ("fully-constrained-color", "Fully constrained curves turn black; underconstrained curves stay blue."),
    ("sketch-palette", "Sketch Palette: Options (look at, grid, snap, slice, profile, points, dimensions, constraints) and Finish Sketch."),
    ("drag-geometry", "Dragging underconstrained geometry moves it live while constraints hold."),
    ("profile-shading", "Closed profiles shade light orange."),
    ("finish-returns-view", "Finish Sketch animates the camera back to the view before the sketch."),
  ],
  "dialogs": [
    ("command-dialog", "Feature dialogs dock top-right of the canvas with OK/Cancel, a collapsible header and a help icon."),
    ("selection-rows", "Selection rows show a count (1 selected) with an X to clear and a blue arrow when active."),
    ("live-preview", "Features preview live while the dialog is open."),
    ("manipulators", "Arrows and rotation handles in the canvas drag values (extrude distance, fillet radius)."),
    ("value-expressions", "Value boxes take expressions and units (10 mm + 2 in, d1 * 2)."),
    ("enter-ok", "Enter commits, Escape cancels."),
  ],
  "shortcuts": [
    ("s-toolbox", "S opens Design Shortcuts: recent commands plus a search box."),
    ("keys", "Single keys: E extrude, F fillet, Q press pull, H hole, D dimension, L line, R rectangle, C circle, M move, J joint, I measure, X construction, V visibility."),
    ("customize", "Shortcuts can be reassigned from each command's ... menu."),
  ],
  "chrome": [
    ("data-panel", "Data panel (grid icon) slides in with projects and files."),
    ("document-tabs", "Document tabs across the top, with + for a new design."),
    ("notifications", "Job status, notification bell, help and profile at the top right."),
    ("status-bar", "Selection info such as 1 Profile | Area: 2.00 in^2 at the lower right."),
    ("text-commands", "Text Commands panel at the bottom."),
    ("comments", "Comments panel at the lower left."),
  ],
}


def main():
  old = {}
  if os.path.exists(OUT):
    with open(OUT, encoding="utf-8") as f:
      old = {e["key"]: e for e in map(json.loads, filter(str.strip, f))}
  out = dict(old)
  for area, items in SEED.items():
    for slug, fusion in items:
      key = f"ui:{area}:{slug}"
      if key not in out:
        out[key] = {"key": key, "area": area, "name": slug.replace("-", " "), "fusion": fusion, "status": "untested", "history": []}
  with open(OUT, "w", encoding="utf-8", newline="\n") as f:
    for e in out.values():
      f.write(json.dumps(e) + "\n")
  print(f"{len(out)} UI features ({len(out) - len(old)} new) -> {os.path.relpath(OUT, ROOT)}")


if __name__ == "__main__":
  main()
