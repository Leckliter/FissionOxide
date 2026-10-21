# Parity run, 2026-10-21 (prober 4, second part): sketch CREATE leftovers

Follows `2026-10-21-sketch-modify-tools.md` in the same session. FissionOxide was rebuilt from
`main` at `ef41107` (after the wave-7 CREATE fixers merged) and relaunched; both apps got a fresh
XY sketch with a horizontal line, a diagonal line below it and a shorter horizontal line above.
Probed: 2-Tangent and 3-Tangent Circle, angular and diameter Sketch Dimension, Text after the
frame, Mirror, Circular and Rectangular Pattern.

## Headline

| | Count |
|---|---|
| New differences recorded | 10 |
| P1 | 2 |
| P2 | 8 |
| New batches | 2 (`map-wave10.json`) |
| Match rows | 5 |
| Re-verified | 6 verified: `ui:sketch:diameter-glyph`, `ui:sketch:text-tool-flow`, `ui:sketch:mirror-dialog`, `ui:sketch:circular-pattern-dialog`, `ui:sketch:rectangular-pattern-dialog`, `ui:sketch:dimension-placement-flow` |

## What matched

- 2-Tangent Circle: Feature Options icons, both prompts, bold picked lines, prompt loop.
- Text's first stage (Type row, OK disabled, "Place first corner of text frame").
- Dimension edit box with the value selected and a menu; Enter accepts; the prompt returns.
- MIRROR, CIRCULAR PATTERN and RECTANGULAR PATTERN open with the same two picker rows and OK
  disabled.
- The diameter glyph renders now (`ui:sketch:diameter-glyph` verified).

## P1

1. **Text frame flow** (`ui:sketch:text-frame-flow`): Fusion takes two clicks with a dashed
   rubber-band frame and "Place second corner of text frame", then focuses the Text box over a blue
   "Sample Text" with a rotate handle; FissionOxide needs a drag, has no rubber band, leaves the box
   unfocused (typed text is lost) and shows a black "FissionOxide" placeholder.
2. **Mirror line picker** (`ui:sketch:mirror-line-picker`): after the objects pick the prompt
   says "Select the mirror line" but the Objects picker stays active, so the line lands in Objects
   and OK stays disabled.

## P2

- 2-Tangent Circle: no heads-up radius box ("Specify radius of circle", "0.141 in") and no
  status; the result gets an unasked "R5.19" dimension
  (`ui:sketch:tangent-circle-radius-box`, `ui:sketch:tangent-circle-result`).
- 3-Tangent Circle prompts "Select line, circle, or arc" three times; the result draws lighter
  (`ui:sketch:three-tangent-prompts`).
- Angular dimension reads "17.2954°" after placing (Fusion "17.2°"); diameter dimension is drawn
  through the centre regardless of the placement click (Fusion: text at the click with a leader)
  (`ui:sketch:angular-dimension-text`, `ui:sketch:diameter-dimension-placement`).
- TEXT rows: Fusion's multi-line Text with fx, Arial, Typeface icons, Height "0.394 in", Flip and
  an Alignment icon grid vs FissionOxide's single line, Ubuntu Light, Bold/Italic/Underline
  checkboxes, alignment dropdowns and an Angle row (`ui:sketch:text-dialog-rows`).
- CIRCULAR PATTERN: Distribution/Suppression/Quantity 3 with a grey preview and a floating
  quantity box vs Quantity 6 / Total Angle / Symmetric with a blue preview
  (`ui:sketch:circular-pattern-rows`).
- RECTANGULAR PATTERN: fixed-width dialog with Distribution, Distance 0.00 in and Direction
  dropdowns, OK disabled until a direction vs a dialog that widens to 500 px with Distance Type,
  20 mm defaults, an inline second-direction row and OK enabled at once
  (`ui:sketch:rectangular-pattern-rows`).

## Observations without a row

- Fusion's OFFSET opens with Chain Selection checked when nothing is pre-selected (seen when a
  stray O opened it), unchecked with a pre-selected curve.
- Fusion's canvas shortcuts (F, O, X ...) fire while the Text frame is pending: a drag's release
  does not place the second corner, so typing there rotated the view and opened OFFSET.

## Batches

| Batch | Items | Files |
|---|---|---|
| `ui-sketch-tangent-circles-dims` | 5 | viewport.rs |
| `ui-sketch-text-mirror-patterns` | 5 | forms.rs, dialogs.rs |

## Not covered

- Mirror and pattern previews' glyphs after OK in Fusion beyond the symmetry glyphs; Circular
  Pattern's Distribution options; Rectangular Pattern after a direction pick; text on path; the
  Text box's fx button; Project / Include was not revisited.
