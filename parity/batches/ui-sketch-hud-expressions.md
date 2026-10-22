# Parity batch: ui-sketch-hud-expressions

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-21 (prober 6) with the 2-Point Rectangle, Center Diameter Circle, 3-Point Arc
and Sketch Dimension tools, FissionOxide `main` at `d90d1e6`. Fusion was in inches, FissionOxide in
mm. The sketch already had a dimension named d1 (0.5 mm) in FissionOxide when the parameter
expression was typed.

## parameter expression in a heads-up box (P1)
- key: `ui:sketch:hud-parameter-expression`
- Fusion: Typing "d1*2" into a rectangle's heads-up box and Enter makes that side driven by the expression: the dimension reads "fx: 2.00", its edit box opens with "d1 * 2" and a greyed "fx" tag, and the tooltip names the new parameter ("d21 : d1 * 2").
- FissionOxide: "d1*2" typed into the focused height box shows with a padlock, but Enter ignores it: the rectangle takes the cursor's height and gets no height dimension at all (the width box "1/2" in the same rectangle worked and left a "0.50" dimension).
- Start here: `crates/ui/src/viewport.rs:1255` (`hud_value` evaluates against `app.s.model.params`, which does not hold the sketch's model parameters d1...), `crates/ui/src/viewport.rs:2893` (how an expression dimension is labelled; Fusion's label is "fx: 2.00").

## a letter starts typing into the first box (P1)
- key: `ui:sketch:hud-letter-first-key`
- Fusion: With the first heads-up box focused (as it is after the first click), any typed character replaces its text, letters included ("d1*2" goes straight in).
- FissionOxide: Digits typed right after the first click go into the width box, but a leading letter is swallowed ("d1*2" left the box at "19.25 mm" with no padlock, and the rectangle was made with the cursor width); after Tab the box takes letters.
- Start here: `crates/ui/src/viewport.rs:3292` (`take_hud_tab`) and the digit-only capture near `crates/ui/src/viewport.rs:3775` / the `digit keys consumed` test at `crates/ui/src/viewport.rs:5126`.

## expression normalised on Tab (P2)
- key: `ui:sketch:hud-expression-normalised`
- Fusion: Typing "1/2" shows "1/2" with a padlock; Tab rewrites the box to "( 1 / 2 ) * 1 in" (padlock kept) and moves to the next box.
- FissionOxide: The box keeps "1/2" after Tab.
- Start here: `crates/ui/src/viewport.rs:3269` (Tab commit of the typed text), `crates/engine/src/expr.rs` (a formatter for the parsed expression).

## rectangle heads-up layout (P2)
- key: `ui:sketch:rectangle-heads-up-layout`
- Fusion: After the first corner the first box ("0.948 in", text selected, blue underline) sits just left of the cursor and the second ("1.414 in") is centred under the bottom edge between extension lines; the prompt is "Specify size of rectangle" on one line.
- FissionOxide: The width box ("23.35 mm") sits above the top edge between extension lines and the height box ("15.40 mm") at the right side between extension lines; the prompt wraps onto two lines.
- Start here: `crates/ui/src/viewport.rs:3452` (rectangle box placement), the prompt box width near `crates/ui/src/viewport.rs:3835`.

## dimension box menu (P2)
- key: `ui:sketch:dim-box-menu`
- Fusion: The vertical-ellipsis button of a dimension's edit box opens a list of recent values ("0.2815284896 in", "17.220379275 deg", "0.5 in", "0.00 in"), a separator and "Measure" with a ruler icon; picking one fills the box.
- FissionOxide: The menu holds "Change Parameters" and "Delete".
- Start here: `crates/ui/src/viewport.rs:2342` (`dim_editor` menu).

## Enter on an empty dimension box (P2)
- key: `ui:sketch:dim-box-empty-enter`
- Fusion: Enter on an emptied box is refused: the box stays open and empty with a red underline until a value is typed or Escape closes it unchanged.
- FissionOxide: Enter closes the box, leaves the dimension unchanged and prints a red "... a number or an expression string" message at the bottom-left of the canvas.
- Start here: `crates/ui/src/viewport.rs:2360` (`commit` path of `dim_editor`), `crates/ui/src/viewport.rs:2330` (the text edit frame).

## 3-point arc third step (P2)
- key: `ui:sketch:three-point-arc-third-step`
- Fusion: After the two end points the arc previews through the pointer with a cross at each end and at the pointer, the prompt reads "Place point on arc" and no heads-up box is shown at that step.
- FissionOxide: The third step previews a full circle with a radius line from the centre, a radius box ("9.91 mm") and the prompt "Specify radius".
- Start here: `crates/ui/src/viewport.rs:3862` (`Tool::Arc3` prompts), `crates/ui/src/viewport.rs:219` (`hud_fields` gives `Arc3` a Radius box at step 2).

## circle prompt wording (P2)
- key: `ui:sketch:circle-diameter-prompt`
- Fusion: Center Diameter Circle's second prompt is "Specify diameter of circle" (the box "1.0.. in" sits at the right end of a diameter line with end ticks drawn through the circle).
- FissionOxide: The prompt is "Specify diameter" (the diameter line and box placement match).
- Start here: `crates/ui/src/viewport.rs:3841` (`sketch_prompt` for `Tool::Circle` / `Circle2P` / `Circle2T`; Fusion's 2-tangent prompt is "Specify radius of circle", see `ui:sketch:tangent-circle-radius-box`).
