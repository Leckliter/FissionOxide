# Parity batch: ui-selection-colors

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

All colours below were sampled from the screen on 2026-10-21 with both apps at their defaults
(Fusion: default appearance, light theme; FissionOxide: `main` at `2356d5a`). They are sRGB
values of a lit face, so match the *rule* (how the colour changes) and tune the constants
until the sampled values agree within a few units.

## face hover color (P1)
- key: `ui:canvas:face-hover-color`
- Fusion: Hovering a face lightens it and keeps its hue: the front face went from (162,162,155) to (190,190,185), the top face from (165,173,165) to (192,197,193). No blue tint. Edges of the hovered face are unchanged.
- FissionOxide: Hovering a face tints it blue: the front face went from (142,145,150) to (98,145,182). Hover is `mix(c, HOVER_COLOR, 0.6)` with `HOVER_COLOR = (128,190,238)`.
- Start here: `crates/engine/src/scene.rs:15` (`HOVER_COLOR`) and `:205` (the peek mix). Lighten instead of tinting (add about +28 per channel, or mix toward white by about 0.3).

## face selected color (P2)
- key: `ui:canvas:face-selected-color`
- Fusion: A selected face is a medium blue (81,132,205) on a front face and (83,138,210) on the top face, so the lighting still shows through slightly. A selected face under the pointer is lighter, (133,169,220).
- FissionOxide: A selected face is (28,109,168), darker and more saturated, and a selected face under the pointer shows only the hover tint (98,145,182), with no "selected and hovered" state.
- Start here: `crates/engine/src/scene.rs:16` (`SELECT_COLOR = (36,142,220)`), `:204` (`mix(c, SELECT_COLOR, 0.45)`), `:227`. Add a selected-and-hovered colour.

## edge hover color (P1)
- key: `ui:canvas:edge-hover-color`
- Fusion: Hovering an edge draws it as a 2 px black line (10,10,10); the plain edge is a 1 px light-grey silhouette (179,178,173).
- FissionOxide: Hovering an edge draws it 2 px light blue (128,190,238) over a dark plain edge (35,38,44).
- Start here: `crates/engine/src/scene.rs:304-306` (edge highlight `(HOVER_COLOR, 3.0)`) and `:17` (`EDGE_COLOR`). Plain edges should be a light grey silhouette on the body colour, hover black.

## edge selected color (P2)
- key: `ui:canvas:edge-selected-color`
- Fusion: A selected edge is bright blue (10,131,252) with a pale halo (214,235,255) one pixel wide on each side.
- FissionOxide: A selected edge is (36,142,220), 2 px, no halo.
- Start here: `crates/engine/src/scene.rs:304` (`(SELECT_COLOR, 3.0)`).

## body selected color (P2)
- key: `ui:canvas:body-selected-color`
- Fusion: Selecting a body (browser row or canvas) colours every face exactly like a selected face: (81,132,205) front, (83,138,210) top.
- FissionOxide: A selected body is a lighter, greyer blue than a selected face: (91,128,158) front, (113,160,197) top, because the body mix uses 0.45 of `SELECT_COLOR`.
- Start here: `crates/engine/src/scene.rs:204`. Use the same selected colour for bodies and faces.

## sketch curve priority (P1)
- key: `ui:canvas:sketch-curve-priority`
- Fusion: With a visible sketch on a body face, hovering a sketch curve highlights the curve (thicker, darker blue) and clicking selects it: status "1 Sketch Arc | Diameter : 0.50 in". The face underneath is not picked.
- FissionOxide: Hovering the same curve tints the whole face blue (122,181,226) and clicking selects the face (34,135,209); the curve cannot be picked over a face outside a sketch.
- Start here: `crates/ui/src/viewport.rs:500-523` (pick ordering: "preferred kinds win within the search radius", `sel_priority`). Sketch curves and points must win over faces within the pick radius, as vertices already win over edges.

## command pick colors (P2)
- key: `ui:canvas:command-pick-colors`
- Fusion: In Fillet, hovering an edge is the same black as plain hover and a picked edge the same bright blue with halo; the rest of the body keeps its colour. In Extrude, hovering a profile draws its outline black and lightens its fill to (192,197,193); a picked profile fills (83,138,210) with a red outline. The preview body keeps the body appearance.
- FissionOxide: Fillet hover is light blue and a picked edge (30,110,235); the whole body turns blue-grey (116,132,154) / (142,157,179) as soon as there is a preview. Extrude's hovered profile is (96,148,196) and picked (39,118,185), with no outline change.
- Start here: `crates/engine/src/scene.rs:245` (`mesh_of(&b.0, b.1, SELECT_COLOR, hover)` for previews), `:604-632` (profile hover/select), `:1289` (sketch curve widths). Related: `ui:canvas:preview-color` in batch ui-feature-manipulators.

## extrude all regions (P2)
- key: `ui:dialogs:extrude-all-regions`
- Fusion: E opens Extrude with nothing selected; the user picks profiles.
- FissionOxide: E opens Extrude with the chip "Select (all regions)" and immediately previews a 10 mm extrusion of every closed region on the visible sketch.
- Start here: `crates/ui/src/dialogs.rs:2172` and `Dialog::open` at `:658-673`. Start with no profiles (see also `ui:dialogs:extrude-auto-profile` in ui-dialog-docking).
