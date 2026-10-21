# Parity batch: ui-sketch-tangent-circles-dims

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

Observed on 2026-10-21 (prober 4) on a sketch with two non-parallel lines and a third parallel
line; FissionOxide `main` at `ef41107` (rebuilt after the wave-7 CREATE fixers merged).

## tangent circle radius box (P2)
- key: `ui:sketch:tangent-circle-radius-box`
- Fusion: After the second tangent pick of 2-Tangent Circle the circle follows the pointer with a floating box "0.141 in" (value selected), the prompt "Specify radius of circle" and the status "2 Sketch Curves | Min Distance : 0.206 in"; typing sets the radius.
- FissionOxide: The circle follows the pointer with the prompt "Specify diameter", no floating box and no status readout.
- Start here: `crates/ui/src/viewport.rs:3824` (`Tool::Circle2T` prompts), `crates/ui/src/viewport.rs:1916` (the 2-tangent preview; add the heads-up radius box like the Circle tool's diameter box).

## tangent circle result (P2)
- key: `ui:sketch:tangent-circle-result`
- Fusion: The placed 2-tangent circle carries only two tangent glyphs (each beside the circle on its line) and a centre point; no dimension.
- FissionOxide: The placed circle gets an "R5.19" radius dimension it was never asked for, and the tangent glyphs stack next to the horizontal glyph at the line's midpoint.
- Start here: `crates/ui/src/viewport.rs:1916` (`sketch.tangent_circle` call: do not add a radius dimension) and the glyph placement noted in `ui:sketch:fillet-result-glyphs`.

## three-tangent prompts (P2)
- key: `ui:sketch:three-tangent-prompts`
- Fusion: 3-Tangent Circle prompts "Select first tangent of circle", "Select second tangent of circle", "Select third tangent on circle"; the result circle is drawn like any other curve.
- FissionOxide: The prompt is "Select line, circle, or arc" for all three picks, and the result circle is drawn in a lighter blue than the other curves until the next action.
- Start here: `crates/ui/src/viewport.rs:3829` (`Tool::Circle3T` prompt by pick count), `crates/ui/src/viewport.rs:1932` (result selection/colour).

## angular dimension text (P2)
- key: `ui:sketch:angular-dimension-text`
- Fusion: An angular dimension previews and reads "17.2°" (one decimal, the degree sign) with the prompt "Select location for dimension"; the edit box after placing shows the full value "17.220379275 deg" selected.
- FissionOxide: The preview reads "17.3°" but the placed dimension reads "17.2954°" (four decimals); the prompt is "Select additional sketch geometry or location to place dimension" (Fusion uses that wording, minus "to place", only for diameter/length picks); the edit box shows "17.2954000529 deg".
- Start here: `crates/ui/src/viewport.rs:2151` (dimension label formatting: angles to one decimal), `crates/ui/src/viewport.rs:3909` (prompt).

## diameter dimension placement (P2)
- key: `ui:sketch:diameter-dimension-placement`
- Fusion: A diameter dimension's text "Ø0.282" sits where the pointer clicked, joined to the circle by a leader line; its edit box shows the full "0.2815284896 in"; the prompt before placing is "Select additional sketch geometry or location for dimension".
- FissionOxide: The text "Ø10.38" is drawn horizontally through the circle's centre inside the circle with arrow heads, wherever the pointer clicked; the edit box shows "10.38 mm" (display precision).
- Start here: `crates/ui/src/viewport.rs:2889` (diameter dimension painter), `crates/ui/src/viewport.rs:2540` (label glyph) and the placement point handling used by linear dimensions.
