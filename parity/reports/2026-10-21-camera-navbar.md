# Parity probe: camera, navigation bar, projection, mouse presets (2026-10-21, prober 7)

Same session as the earlier reports of the day; FissionOxide `main` at `42dec181`. The navigation
bar buttons and their tooltips, the wheel, the Orbit / Pan / Zoom tools with left drags, Fit and
Zoom Window, Display Settings > Camera, and the Preferences mouse-setup rows were compared.

## Method notes
- Middle-button drags are not available to the desktop control, so the mouse presets' own
  orbit/pan gestures were not driven; the navigation bar tools with a left drag stood in.
- Fusion's navigation bar tooltips sit below the bar (zoom the band y 640..730); its Zoom Window
  dropdown opens below the bar too.
- Fusion's Preferences opened on the last page (Manufacture); the General page needs a scroll of
  five wheel notches to reach the mouse rows. Escape in Fusion closes only the open dropdown; in
  FissionOxide it closed the whole Preferences window.

## What matched
`ui:canvas:wheel-zoom-direction` (forward zooms in about the pointer), `ui:navbar:tool-drags`,
`ui:navbar:camera-submenu` (Orthographic / Perspective / Perspective with Ortho Faces),
`ui:chrome:preferences-orbit-type` (row and default). Verified: `ui:navbar:display-settings-2`
(the menu now lists Fusion's eight entries with the Camera radios).

## Differences (all P2)
- `ui:navbar:tooltips`: one-word Fusion tooltips against FissionOxide's sentences.
  `crates/ui/src/navbar.rs`.
- `ui:navbar:zoom-window-button`: Fusion's fifth button is Zoom Window with a dropdown "Zoom
  Window / Fit F6" (icons); FissionOxide's is Fit with "Fit / Zoom Window".
  `crates/ui/src/navbar.rs:110`.
- `ui:chrome:preferences-mouse-setup`: Fusion lists Fusion, Alias, Inventor, PowerMill,
  SOLIDWORKS, Tinkercad plus camera pivot, ViewCube size, gesture and Spacebar rows; FissionOxide
  lists Fusion, SolidWorks, Inventor, Alias with three explanatory rows.
  `crates/ui/src/preferences.rs:109`.
- `ui:chrome:preferences-default-orientation`: Y up (Fusion) against Z up.
- `ui:chrome:preferences-escape`: Escape closes the whole Preferences window in FissionOxide.

## Retractions
None.

## Batches
| batch | items | keys |
|---|---|---|
| `ui-camera-navbar` | 5 | tooltips, zoom-window-button, preferences-mouse-setup, preferences-default-orientation, preferences-escape |

## Not covered
- The presets' middle-button gestures, animation timing beyond the earlier verified 0.3 s, zoom
  speed per wheel notch (both zoomed a similar amount for three notches), Free Orbit, Zoom Window
  drags, Look At on a face (verified earlier), Reverse zoom direction, Fit on an off-centre view.
