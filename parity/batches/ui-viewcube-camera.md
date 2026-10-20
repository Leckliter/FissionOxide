# Parity batch: ui-viewcube-camera

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## face click (P1)
- key: `ui:viewcube:face-click`
- Fusion: Clicking a face (TOP, FRONT, ...) animates the camera to that standard view.
- FissionOxide: Face click goes to the right view but snaps instantly (Camera::set_view sets yaw/pitch directly). Fusion animates the camera over about 0.3 s.
- Start here: `crates/ui/src/viewport.rs`, `crates/render/src/lib.rs`

## edge click (P1)
- key: `ui:viewcube:edge-click`
- Fusion: Clicking one of the 12 edges animates to the 45-degree view between the two faces.
- FissionOxide: Only the 6 faces are hit-tested (viewcube() builds one quad per face). Clicking the band along an edge picks the nearest face. Fusion has 12 edge hot spots that go to the 45-degree view between two faces. StandardView has no edge or corner views.
- Start here: `crates/ui/src/viewport.rs`, `crates/render/src/lib.rs`

## corner click (P1)
- key: `ui:viewcube:corner-click`
- Fusion: Clicking one of the 8 corners animates to that isometric view.
- FissionOxide: Clicking the front-top-right corner went to TOP. Fusion has 8 corner hot spots, one isometric view each. Needs edge/corner hit zones (inset each face quad, so the outer band is edges and the corners are corners) plus arbitrary-direction views in Camera.
- Start here: `crates/ui/src/viewport.rs`, `crates/render/src/lib.rs`

## hover highlight (P1)
- key: `ui:viewcube:hover-highlight`
- Fusion: Hovering a face, edge or corner highlights that hot spot in blue before clicking.
- FissionOxide: Hover tints the whole face pale blue. Fusion tints only the hot spot under the pointer: the face centre, an edge strip, or a corner.
- Start here: `crates/ui/src/viewport.rs`, `crates/render/src/lib.rs`

## camera animation (P1)
- key: `ui:canvas:camera-animation`
- Fusion: Every camera change Fusion makes for you animates over about 0.3 s: ViewCube clicks, Home, Look At, entering and finishing a sketch, Fit.
- FissionOxide: All camera changes snap instantly: ViewCube faces, Home, entering a sketch (already at TOP 0.15 s after the click), Finish Sketch. Camera::set_view assigns yaw/pitch directly. Add a camera tween (yaw/pitch/target/distance, eased, ~300 ms) used by set_view, fit_view, look_at and sketch enter/exit.
- Start here: `crates/render/src/lib.rs`, `crates/ui/src/viewport.rs`, `crates/ui/src/lib.rs`

## rotate arrows (P2)
- key: `ui:viewcube:rotate-arrows`
- Fusion: With a face view active, curved arrows rotate the view 90 degrees about the view axis.
- FissionOxide: In a face view Fusion shows two curved arrows at the cube's top right that roll the view 90 degrees. FissionOxide shows only the flat face.
- Start here: `crates/ui/src/viewport.rs`, `crates/render/src/lib.rs`

## adjacent arrows (P2)
- key: `ui:viewcube:adjacent-arrows`
- Fusion: With a face view active, triangle arrows on each side roll to the adjacent face.
- FissionOxide: In a face view Fusion shows four triangles around the face that step to the neighbouring face. FissionOxide has none.
- Start here: `crates/ui/src/viewport.rs`, `crates/render/src/lib.rs`

## menu (P2)
- key: `ui:viewcube:menu`
- Fusion: The dropdown next to the cube: Go Home, Orthographic / Perspective, Set current view as Home or Front, Reset Front.
- FissionOxide: The button under the cube only toggles orthographic/perspective. Fusion opens a menu: Go Home, Orthographic, Perspective, Perspective with Ortho Faces, Set current view as Home (Fixed distance / Fit to view), Reset Home, Set current view as Front / Top, Reset Front.
- Start here: `crates/ui/src/viewport.rs`, `crates/render/src/lib.rs`

## compass ring (P2)
- key: `ui:viewcube:compass-ring`
- Fusion: The compass ring under the cube can be dragged to spin the view about up.
- FissionOxide: No compass ring. Fusion's ring under the cube drags to spin the view about up. Low priority.
- Start here: `crates/ui/src/viewport.rs`, `crates/render/src/lib.rs`

## fade when idle (P2)
- key: `ui:viewcube:fade-when-idle`
- Fusion: The cube is translucent until the pointer approaches.
- FissionOxide: Cube is always fully opaque. Fusion's cube is semi-transparent until the pointer comes near.
- Start here: `crates/ui/src/viewport.rs`, `crates/render/src/lib.rs`
