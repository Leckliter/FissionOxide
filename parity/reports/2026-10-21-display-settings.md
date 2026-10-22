# Parity probe: display settings (2026-10-21, prober 7)

Same session as the earlier reports of the day; FissionOxide `main` at `42dec181`. Every
submenu of the navigation bar's Display Settings was opened in both apps, the visual style
shortcuts and full screen were tried, and Ground Plane Offset was opened.

## Method notes
- The taskbar order of the two apps swapped during the session (Fusion moved to the right
  button); a taskbar click meant for FissionOxide landed on Fusion once. Check the screenshot
  after every switch.
- Fusion's Display Settings menu stays open through the Ctrl+4..9 shortcuts and through full
  screen; two Escapes did not close it.
- Fusion's long submenus (Object Visibility) run below the window; zoom the band down to y 790.

## What matched
`ui:navbar:display-settings-structure`, `ui:navbar:full-screen-toggle`; the Camera submenu
(`ui:navbar:camera-submenu`, earlier today).

## Differences (all P2)
- `ui:navbar:visual-style-list`: six Fusion rows with Ctrl+4..9 (working) against eight rows in
  another order, a different default and no shortcuts. `crates/ui/src/navbar.rs`.
- `ui:navbar:environment-list`: Fusion's Theme row plus two headed groups of three against a flat
  list of sixteen. `crates/ui/src/navbar.rs:125`.
- `ui:navbar:effects-list`: Fusion's preset row and seven effects (four on) against three unchecked
  boxes; Graphics Preset defaults to Custom in Fusion and Performance in FissionOxide.
  `crates/ui/src/navbar.rs:285`.
- `ui:navbar:object-visibility-list`: fourteen rows against five. `crates/ui/src/navbar.rs:288`.
- `ui:navbar:ground-plane-offset-dialog`: a docked Adaptive checkbox dialog against a large
  window with an offset field. `crates/ui/src/navbar.rs:335`.

## Retractions
None.

## Batches
| batch | items | keys |
|---|---|---|
| `ui-display-settings-3` | 5 | visual-style-list, environment-list, effects-list, object-visibility-list, ground-plane-offset-dialog |

## Not covered
- The rendered look of each visual style and environment, toggling each effect and visibility
  row, the ground plane offset value, the greyed "Graphics Preset: Custom" row's behaviour when a
  preset is chosen.
