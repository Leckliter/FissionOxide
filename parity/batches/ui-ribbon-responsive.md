# Parity batch: ui-ribbon-responsive

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before opening the PR.

## responsive collapse (P1)
- key: `ui:toolbar:responsive-collapse`
- Fusion: At a 920 px window the ribbon shrinks each panel to fewer icons (SOLID: CREATE 5, MODIFY 4) so every panel through SELECT stays visible; the dropdown still lists everything.
- FissionOxide: Panels keep their full icon count. At 920 px INSERT is clipped at the right edge and ASSEMBLE and SELECT are off-screen, with no scroll and no way to reach them except the Design Shortcuts box.
- Start here: `crates/ui/src/panels.rs:759` (`toolbar`) and the panel width at 1007. Measure the row, then drop pinned icons from the right of each panel (largest panel first) until it fits, keeping at least one icon per panel.

## sketch panel icons (P2)
- key: `ui:toolbar:sketch-panel-icons`
- Fusion: At full width the SKETCH tab's CREATE panel shows 7 icons, MODIFY 5 and CONSTRAINTS all 12 constraints; icons are 32 px with a generous hit area. Clicking anywhere on the big FINISH SKETCH button (icon or label area above the panel name) finishes the sketch.
- FissionOxide: CREATE 6, MODIFY 4, CONSTRAINTS 8, at 22 px. FINISH SKETCH's hit area is only the small icon: a click between the icon and the "FINISH SKETCH" label does nothing.
- Start here: `crates/ui/src/icons.rs:1966` (`icon_button` size and hit rect) and the SKETCH tab panel list in `crates/ui/src/menus.rs`.

## tab style (P2)
- key: `ui:toolbar:tab-style`
- Fusion: The active ribbon tab is bold black text with a black underline; the SKETCH contextual tab is the same.
- FissionOxide: The active tab is blue text with a blue underline, and the SKETCH tab sits on a light-blue box.
- Start here: `crates/ui/src/panels.rs:759` (`tab_row`) and `crates/ui/src/theme.rs`.

## workspace menu style (P2)
- key: `ui:toolbar:workspace-menu-style`
- Fusion: The DESIGN switcher lists DESIGN, GENERATIVE DESIGN (lock icon), RENDER, ANIMATION, SIMULATION (lock icon), MANUFACTURE, DRAWING ▸, ELECTRONICS ▸ in upper case, with a "Change Workspace" tooltip.
- FissionOxide: Title-case "Design, Generative Design, Render, Animation, Simulation, Manufacture, Drawing" with the unbuilt ones greyed, no lock icons, no Electronics, no submenu for Drawing.
- Start here: `crates/ui/src/panels.rs:57` (the list) and 803 (`workspace_switcher`).
