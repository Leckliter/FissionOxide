# Parity probe: ribbon tooltips, pin/unpin, shortcut customisation (2026-10-21, prober 7)

Same session and documents as `2026-10-21-sketch-display-browser-rows.md`: FissionOxide built from
`main` at `42dec181` (window 1002 px wide), Fusion's window 907 px wide, so Fusion's ribbon was in
its responsive state (CREATE 5 icons, MODIFY 4). Every SOLID ribbon button was hovered for 1 s and
then 4 s in each app; then the CREATE dropdown's Extrude row was used for the row options, pin /
unpin, Change Keyboard Shortcut and the S toolbox.

## Method notes
- Fusion's first hover of the session (Create Sketch) showed nothing within 4 s; the second try,
  moved 2 px, worked. Move the pointer onto the canvas for half a second between buttons so each
  tooltip starts fresh.
- Fusion's tooltip panels can extend past the window's right edge (Select, Insert Mesh); zoom
  regions clipped at the window edge lose their text. The Select text is still unread.
- A `key` "s" with the CREATE dropdown open goes to the menu; close the menu with a canvas click
  first.
- Pin to Toolbar in Fusion is a checkbox; clicking it three times in a row (uncheck, check,
  uncheck) is easy to do by accident when the same batch is replayed. The Extrude icon is still not
  back in Fusion's CREATE panel at 907 px after re-checking it (Create Form holds the slot); not
  retried at full width.

## What matched
`ui:toolbar:create-dropdown-list` (25 entries, separators, E / H hints), `unpin-removes-icon`,
`panel-label-no-tooltip`, `shortcuts-toolbox-at-pointer`, `tooltip-timing` (about 1 s, then
about 3 s more). Verified: `ui:menus:row-options-glyph` (the ellipsis on a hovered row),
`ui:menus:tooltip-style` (dark panel right of the menu). Noted, left `fixing-merged`:
`ui:toolbar:responsive-collapse` (FissionOxide shows CREATE 6 / MODIFY 7 at 1002 px; the
collapse width was not probed).

## Differences, by priority

### P1
- `ui:toolbar:tooltip-style`: Fusion's ribbon tooltip is a dark rounded panel under the button,
  white bold title with the shortcut in parentheses ("Extrude (E)"), a description paragraph and a
  usage paragraph at once; FissionOxide's is a light panel with a one-line description and a grey
  "Shortcut: E" line. `crates/ui/src/menus.rs:1265`.
- `ui:toolbar:tooltip-expanded-illustration`: Fusion's expansion adds a large rendered illustration
  and "Press Ctrl+/ for more help." (commands without a picture show the help line at once);
  FissionOxide's adds its small ribbon icon and a usage line. `crates/ui/src/menus.rs:1265`,
  `crates/ui/src/tips.rs:866`.
- `ui:toolbar:tooltip-texts-solid`: every SOLID button's text differs; Fusion's full texts for the
  20 buttons are in the batch. `crates/ui/src/tips.rs:866`.

### P2
- `ui:toolbar:row-options-menu`: Fusion's ⋮ panel has two checkboxes (Pin to Toolbar, Pin to
  Shortcuts) and Change Keyboard Shortcut...; FissionOxide has four plain entries including a greyed
  "Pin to Panel" and "Add to Shortcuts". `crates/ui/src/customize.rs:296`.
- `ui:toolbar:change-shortcut-dialog`: Fusion's modal has a prompt, a lower-case key field, two
  hint lines, Reset to default at the left and Cancel / greyed OK; FissionOxide's floating window
  has a sentence, the key in upper case and OK / No Shortcut / Default / Cancel.
  `crates/ui/src/customize.rs:370`.
- `ui:toolbar:shortcuts-toolbox-style`: "Design Shortcuts" centred with a "Search" field versus
  "DESIGN SHORTCUTS" left-aligned with "Search commands". `crates/ui/src/toolbox.rs:134`.

## Retractions
None.

## Batches
| batch | items | keys |
|---|---|---|
| `ui-toolbar-tooltips` | 3 | tooltip-style, tooltip-expanded-illustration, tooltip-texts-solid |
| `ui-toolbar-customize-2` | 3 | row-options-menu, change-shortcut-dialog, shortcuts-toolbox-style |

## Not covered
- Tooltips on the other tabs (SURFACE, MESH, SHEET METAL, PLASTIC, MANAGE, UTILITIES) and on the
  SKETCH tab's buttons (the sketch MODIFY ones are in `ui:sketch:modify-tooltips`); the QAT, the
  application bar and the navigation bar buttons; Fusion's Select tooltip text.
- Pin to Shortcuts / Add to Shortcuts effect in the S toolbox; the re-pinned icon's place at full
  width; the Reset to default and OK flows of the shortcut dialogs; a shortcut clash message.
