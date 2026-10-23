# 2026-10-23: incremental re-verification, verifier 10

Built `main` at `588d30a9` (no build fix needed; `cargo build --release -p fissionoxide`, 3 m 19 s) and
re-checked the 43 checkable keys the wave-42..45 fixers and the legacy-leftovers fixer had marked
`fixing-merged`, in the running app, against each row's `fusion` text. FissionOxide ran at x 0..1030
(screenshot px) with the Data Panel closed; it did not hang. Fusion was used once, for the new Patch row
(Untitled*(4), nothing saved).

Result: **39 verified, 1 reopened, 3 left `fixing-merged`, 2 new rows**, one new batch
`ui-reverify-misc-1` (wave 47, `map-wave47.json`).

## Verified (39)

| Area | Keys |
|---|---|
| chrome / toolbar / documents | `ui:toolbar:workspace-menu-style`, `ui:documents:tab-overflow-menu`, `ui:chrome:unit-display-hide-trailing-zeroes`, `ui:chrome:preferences-unrecorded-pages` |
| shortcuts | `ui:shortcuts:sketch-keys-from-3d` (L, C, R, T), `ui:shortcuts:offset-from-3d-dialog` |
| sketch | `ui:sketch:dim-box-measure-fill`, `ui:sketch:dim-edit-tooltip-style` |
| dialogs | `ui:dialogs:chamfer-fields-2`, `ui:dialogs:chamfer-corner-geometry`, `ui:dialogs:move-rows-after-pick`, `ui:dialogs:checkbox-label-column`, `ui:dialogs:angle-box-precision`, `ui:dialogs:copy-ghost-preview`, `ui:dialogs:replace-face-plane-target`, `ui:dialogs:remove-components` |
| surface | `ui:surface:sweep-loft-missing`, `ui:surface:patch-interior-rails` |
| kernel tests (Windows) | `ui:sheet:butt-corner-relief-flat`, `ui:sketch:text-reference-parts-font`, `ui:sketch:reference-parts-other-failures` (`cargo test -p fox-engine --test reference_parts` passes, 260 s; the butt-corner test passes) |
| mesh (lprism.stl) | reduce, smooth, remesh, shell, combine, convert-mesh, scale-mesh, face-groups dialogs, `ui:mesh:body-menu-greyed-entries`, `ui:mesh:sharp-edge-shading` |
| render | `ui:render:texture-map-controls`, `ui:render:flatten-ground`, `ui:render:appearance-not-drawn`, `ui:render:in-canvas-body-shading`, `ui:render:light-editing` |
| manufacture / navbar / insert | `ui:manufacture:navbar-cam-icons`, `ui:navbar:user-coordinate-systems`, `ui:insert:canvas-calibrate` |

Each row's history has what was seen. Highlights: Smooth previews in about a second and Cancel restores the
mesh; Remesh at the defaults keeps the L-prism; the oak appearance draws at once in DESIGN and RENDER;
chamfer's Miter and Blend corners differ visibly from Chamfer's flat patch.

## Reopened (1)

- `ui:manufacture:solid-holder-submenu` (P2): the Milling Tool Holder entry and icon are right, but its tooltip
  has no holder picture (text only, with "Not in FissionOxide yet"), and the Solid Holder submenu closes while
  the tooltip is drawn over the MANAGE list. Pointers: `crates/ui/src/tips.rs:2412`,
  `crates/ui/src/ribbon_tip.rs:61`.

## New rows (2)

- `ui:surface:patch-sketch-boundary` (P2): Fusion's Patch takes a closed sketch circle ("Curve 1", preview, OK
  lit, seen this session); FissionOxide selects the circle but Boundary Edges stays empty and OK grey.
- `ui:chrome:preferences-inert-controls-greyed` (P3): the Manufacture, Network and Preview Features pages
  draw every control greyed; Fusion's are live.

## Left `fixing-merged` (3, plus the usual 4)

- `ui:browser:drag-reorder`: needs a component holding a body; Part and Hybrid Designs here offer no New
  Component and an Assembly Design's component can't take a body from the UI.
- `ui:animation:commands`: menus and Create Callout checked; Transform Components and the explodes need
  components holding bodies.
- `ui:manufacture:show-tool-on-cursor`: Ctrl+T greys and ungreys with Show Tool; the tool following the pointer
  needs a toolpath.
- Unchanged: `ui:marking-menu:flick`, `ui:chrome:comments-expand`, `ui:sketch:vertical-on-horizontal-line`,
  `ui:timeline:feature-owner-component`.

## Seen but not recorded (no Fusion check this session)

- A surface Loft between two coplanar parallel sketch lines lists both profiles but previews nothing (OK grey).
- Triple-clicking a dialog value box ("0.00 mm") does not select the whole text: typing "4" gave "0.4 mm".
- Dragging an appearance swatch from the Appearance window onto a body did nothing here (double-clicking the
  swatch with the body selected applied it); the drag may be the automation tool's, so it needs a hand check.
- The CANVAS and CALIBRATE windows are plain small windows (OK/Cancel at their left, no visible distance box)
  rather than Fusion-style command dialogs; Fusion's were not looked at this session.
- Hybrid Design's ASSEMBLE menu holds only "Add To Assembly" (no New Component).

## Batch

| Batch | Items | Keys |
|---|---|---|
| `ui-reverify-misc-1` | 3 | `ui:manufacture:solid-holder-submenu`, `ui:surface:patch-sketch-boundary`, `ui:chrome:preferences-inert-controls-greyed` |
