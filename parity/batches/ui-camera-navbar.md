# Parity batch: ui-camera-navbar

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-21 (prober 7) on the navigation bar, the wheel, the Orbit / Pan / Zoom tools,
the Display Settings > Camera submenu and the Preferences mouse-setup rows of each app;
FissionOxide built from `main` at `42dec181`. What already matches: the wheel zooms in on a
forward roll and out on a backward roll, about the point under the pointer; the Orbit tool turns
the view with a left drag, the Pan tool moves it, the Zoom tool zooms out on an upward drag; the
Display Settings menu lists Visual Style, Environment, Graphics Preset, Effects, Object Visibility,
Camera, Ground Plane Offset, Enter Full Screen Ctrl+Shift+F; Camera lists Orthographic,
Perspective (default), Perspective with Ortho Faces as radio rows and the switch works; the
Preferences rows "Mouse setup (pan, zoom, orbit)" (Fusion) and "Default Orbit type"
(Constrained Orbit / Free Orbit) exist in both.

## navbar tooltips (P2)
- key: `ui:navbar:tooltips`
- Fusion: The navigation bar buttons show one-word dark tooltips: "Orbit", "Look At", "Pan", "Zoom", "Zoom Window", "Display Settings", "Grid and Snaps", "Viewports".
- FissionOxide: "Constrained Orbit: left-drag to orbit (Esc to stop)", "Look At: the sketch plane or a selected planar face", "Pan: left-drag to pan (Esc to stop)", "Zoom: left-drag to zoom (Esc to stop)", "Fit (F6)", then "Display Settings", "Grid and Snaps", "Viewports".
- Start here: `crates/ui/src/navbar.rs` (the `on_hover_text` strings of the five tool buttons).

## navbar fifth button (P2)
- key: `ui:navbar:zoom-window-button`
- Fusion: The fifth button is Zoom Window (a magnifier in a box); its dropdown lists "Zoom Window" and "Fit F6", each with an icon; clicking the button starts the Zoom Window tool (a "Zoom Window" label shows at the bottom of the canvas).
- FissionOxide: The fifth button is Fit (tooltip "Fit (F6)") and its dropdown lists "Fit" then "Zoom Window" without icons or the F6 hint.
- Start here: `crates/ui/src/navbar.rs:110` (the Fit button and its dropdown: make the button Zoom Window, order the dropdown Zoom Window / Fit F6 with icons).

## preferences mouse setup list (P2)
- key: `ui:chrome:preferences-mouse-setup`
- Fusion: General > "Mouse setup (pan, zoom, orbit)" lists Fusion, Alias, Inventor, PowerMill, SOLIDWORKS, Tinkercad; the page also has "Reverse zoom direction", "Enable camera pivot", "ViewCube size" (Automatic), "Use gesture-based view navigation", "Use Windows touchpad for gestures" and "Spacebar repeats last command" under the orbit type.
- FissionOxide: The dropdown lists Fusion, SolidWorks, Inventor, Alias (no PowerMill or Tinkercad, SolidWorks not in capitals) and prints three explanatory rows (Orbit "Shift+Middle drag, Right drag", Pan "Middle drag, Shift+Right drag", Zoom "Wheel") Fusion does not show; "Reverse zoom direction" is greyed; no camera pivot, ViewCube size or gesture rows.
- Start here: `crates/ui/src/preferences.rs:109` (`NavPreset` names and order), `crates/ui/src/preferences.rs:910` (the rows after Mouse setup).

## preferences default orientation (P2)
- key: `ui:chrome:preferences-default-orientation`
- Fusion: "Default modeling orientation" defaults to "Y up".
- FissionOxide: The row defaults to "Z up".
- Start here: `crates/ui/src/preferences.rs` (the default of the modeling orientation preference; the Y-up default also drives `ui:sketch:look-at-sketch-up`).

## preferences escape (P2)
- key: `ui:chrome:preferences-escape`
- Fusion: Escape while a dropdown in Preferences is open closes the dropdown only; the Preferences window stays until OK or Cancel.
- FissionOxide: Escape on the open Mouse setup dropdown closed the whole Preferences window (the next click landed on the canvas).
- Start here: `crates/ui/src/preferences.rs` (the window's Escape handling: close the open popup first, the window only when none is open).
