# Parity batch: ui-workspace-menu

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Observed on 2026-10-22 (prober 18) with both windows 908 px wide; FissionOxide built from `main` at
`2d74c694`. The menu's entries and order already match (DESIGN, GENERATIVE DESIGN, RENDER,
ANIMATION, SIMULATION, MANUFACTURE, DRAWING >, ELECTRONICS >). Generative Design and Simulation are
`blocked` on Fusion's Personal licence (greyed rows with a blue upgrade badge that open an upgrade
card); leave FissionOxide's padlock and its workspaces as they are. `ui-electronics-workspace` edits
the ELECTRONICS submenu in the same function; rebase onto it if it lands first. See
`parity/reports/2026-10-22-workspaces.md`.

## Workspace menu row look (P2)
- key: `ui:workspaces:menu-row-style`
- Fusion: rows about 26 px tall (screenshot px) in a semi-bold upper-case face, the list about 110 px wide hugging the labels, with the ▸ arrows for DRAWING and ELECTRONICS close to the text; the current workspace has no fill.
- FissionOxide: rows about 18 px tall in regular weight, a 200 px wide list, and the current workspace filled blue.
- Start here: `crates/ui/src/panels.rs:1027` (`workspace_row`) and `crates/ui/src/panels.rs:978` (`WIDTH`).

## Workspace rows have rich tooltips (P2)
- key: `ui:workspaces:menu-tooltips`
- Fusion: resting on a row for about 1.5 s opens a dark tooltip to its right: bold title "<Name> Workspace" ("Render Workspace", "Generative Design Workspace"), one line of text ("Generates realistic renderings of the design.", "Creates multiple designs that meet your geometric, performance, and manufacturing requirements.") and a picture.
- FissionOxide: no tooltip on the rows.
- Start here: `crates/ui/src/panels.rs:1027` (`workspace_row`), tooltip texts beside `crates/ui/src/tips.rs:639`.

## Workspace button label size (P2)
- key: `ui:workspaces:button-label-size`
- Fusion: the toolbar's workspace button shows "MANUFACTURE ▾", "RENDER ▾", "ANIMATION ▾" in the same semi-bold size as "DESIGN ▾", the caret a few px after the text.
- FissionOxide: MANUFACTURE, ELECTRONICS, GENERATIVE and SIMULATION drop to a 10 px font with the caret pressed against the text ("MANUFACTURE▾"); DRAWING 12.5 px.
- Start here: `crates/ui/src/panels.rs:958` (the `FontId::proportional` size choice).
