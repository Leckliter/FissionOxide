# Parity batch: ui-appbar-home-updates

Make FissionOxide match Autodesk Fusion for each item below. Each item says what Fusion
does, what FissionOxide does now, and where to start. Keep changes scoped to these items, add tests
where the behaviour is testable, and run `cargo test` and `cargo clippy` before pushing.

Found on 2026-10-22 (prober 20) while probing start-up, the update flow and the application bar
(FissionOxide `main` at `379c2ae2`). The application bar is `crates/ui/src/panels.rs:95` to
`:160`. See `parity/reports/2026-10-22-startup-preferences-marking.md`.

Also reopened in this batch (an existing row, not added by the script): `ui:insert:file-chooser`.
FissionOxide's Decal now opens the "Insert" window, but its PROJECT list is empty and the
"Default Project" heading is drawn twice, offset, over the file table's title; Fusion lists the
account's projects (Default Project selected) with one heading. Start in
`crates/ui/src/insert_window.rs:160` to `:230`.

## Home page from the house icon (P2)
- key: `ui:chrome:home-start-page`
- Fusion: the first application-bar icon is a house (tooltip "Home"). It opens a Home view in place of the design (title "Home - Autodesk Fusion ..."): a left column (a "Home" chip, "Fusion", the hub "Collin Leckliter v", "Open..." and "New..." buttons, My Fusion / Projects / Samples), and a page with a search bar ("Search by design properties or file name in current hub", Search), "Hi Collin" with a pencil, two promotion cards, "Start working" (+ New Design in blue, New Part Design, New Project, Open, Upload), "Recent" (list / grid toggle, a search field, filter and settings icons, a table Name / Location / Owner) and "Ways to explore". Clicking a document tab returns to the design.
- FissionOxide: the first icon is the FissionOxide logo (inert); there is no Home view, and the app starts in a new Untitled design with the Data Panel open.
- Start here: the logo in `crates/ui/src/panels.rs:95`; a Home view drawn over the canvas area from the data panel's project and recent-file data (`crates/ui/src/data_panel.rs`).

## Check for Updates reports in Job Status (P2)
- key: `ui:chrome:check-updates-job-status`
- Fusion: Help > Check for Updates opens no window; the Job Status icon gets a "1" badge. Clicking Job Status opens a "View Job Status" panel under it: a gift icon, bold "A new version of Fusion has landed." then "Click Update Now to begin. Click Update Later to delay the update for 3 days.", a "Check out what's new" link with an external-link icon, a blue Update Now and an Update Later button, a separator and a "Working online" toggle (on). Fusion's icons right of the tabs are: an upload/blue icon, Autodesk Assistant, Job Status, the bell, Help, the profile.
- FissionOxide: Check for Updates opens a centred "Software Update" window (version, a red error line, Try Again, Open Releases Page, an Updates dropdown, a link). The Job Status icon (a gauge, tooltip "Job Status: all changes are local") and an extra "Recent activity" clock icon do nothing when clicked.
- Start here: `crates/ui/src/updates.rs:40` and `:85`, and the two inert `bar_icon` calls at `crates/ui/src/panels.rs:146`; make Job Status a panel (like the Notifications panel in `crates/ui/src/notifications.rs`) that carries the update card and the online toggle, and drop "Recent activity".

## Profile menu entries (P2)
- key: `ui:chrome:profile-menu`
- Fusion: the avatar menu lists Autodesk Account, Preferences, My Profile, Share Feedback, a separator, Subscribe Now, a separator, and Sign Out (with an icon).
- FissionOxide: "Local user" and Preferences.
- Start here: `avatar` in `crates/ui/src/panels.rs:339`; add the entries (greyed with a reason where FissionOxide has no account service).

## The Bodies folder eye hides the bodies (P1)
- key: `ui:browser:bodies-folder-eye-hides`
- Fusion: clicking the Bodies folder's eye hides every body in the canvas (the rows grey); clicking it again shows them.
- FissionOxide: the folder's eye turns off and Body1's row greys, but the body stays drawn in the canvas.
- Start here: `only_bodies: Some(Vec::new())` at `crates/ui/src/viewport.rs:834` (set from `app.ctx_menu.bodies_hidden`, `crates/ui/src/panels.rs:1983`); the scene's consumer probably treats an empty list as no filter (`crates/engine/src/scene.rs`).
