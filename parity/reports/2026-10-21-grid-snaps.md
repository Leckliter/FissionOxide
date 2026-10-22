# Parity probe: grid and snaps (2026-10-21, prober 7)

Same session as the earlier reports of the day; FissionOxide `main` at `42dec181`. The navigation
bar's Grid and Snaps menu, its Grid Settings and Set Increments, and Layout Grid in the 3D view
were compared. The sketch grid itself was covered by the sketch display probes.

## Method notes
- Fusion's Grid and Snaps menu closes when a dialog opens from it; FissionOxide's stays open under
  its submenus. Both were read with the pointer resting on each row.

## What matched
`ui:navbar:grid-menu-list` (six entries, same order and separator; `ui:navbar:grid-snaps` was
verified earlier).

## Differences (all P2)
- `ui:navbar:grid-menu-defaults`: Fusion's 3D view has no grid until Layout Grid is checked, the
  lock is greyed until then, Incremental Move is on, and the two command rows carry icons;
  FissionOxide shows the grid by default, enables the lock, leaves Incremental Move off and has no
  icons. `crates/ui/src/navbar.rs:366`.
- `ui:navbar:grid-settings-dialog`: a docked GRID SETTINGS dialog (Adaptive / Fixed, Reference
  Numbers, OK / Cancel) against a submenu with a spacing value. `crates/ui/src/navbar.rs:375`.
- `ui:navbar:set-increments-dialog`: a docked SET INCREMENTS dialog (Adaptive / Fixed, OK /
  Cancel) against a submenu with Linear and Angular values. `crates/ui/src/navbar.rs:393`.

## Retractions
None.

## Batches
| batch | items | keys |
|---|---|---|
| `ui-grid-snaps-2` | 3 | grid-menu-defaults, grid-settings-dialog, set-increments-dialog |

## Not covered
- Snap to Grid while sketching, Layout Grid Lock's effect while zooming, the Fixed spacing and
  increment fields in Fusion, Incremental Move during a manipulator drag, Reference Numbers off.
