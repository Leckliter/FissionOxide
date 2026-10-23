# Parity batch: ui-reverify-chrome-2

Make FissionOxide match Autodesk Fusion for the items below. Each says what Fusion does, what
FissionOxide does now, and where to start. Keep changes scoped to them, add tests, and run
`cargo test` and `cargo clippy` before pushing.

Found on 2026-10-22 by the re-verification sweep (verifier 4) re-checking the chrome keys in the
rebuilt app (`main` at `ff53fda7`), with Fusion's Preferences open beside it. Four are reopened keys
and one is new; every row is already in `parity/ui-features.jsonl` (do not run
`ledger_from_batches.py` for this batch); read each row's `history` before starting.

## Preferences > General defaults (P2)
- key: `ui:chrome:preferences-general-settings`
- Fusion (seen 2026-10-22): User language "English | English"; Start-up experience "Default Design Type"; "Offline cache time period (days)" a spinner "15"; Purpose for using Fusion "Design"; "Create new shortcuts when updating Fusion" unchecked; Automatic save on close checked; "Automatic recovery backup interval (minutes)" a spinner "5"; Use gesture-based view navigation checked. The other rows match.
- FissionOxide: "English"; "Home"; "Offline cache period" dropdown "14 days"; "Commercial"; "Create new shortcuts when updating" checked; Automatic save on close unchecked; "Automatic recovery backup interval" dropdown "5 minutes"; gestures unchecked.
- Start here: `crates/ui/src/preferences.rs` (General page rows; test near line 2078).

## Preferences > Unit and Value Display: General precision default (P3)
- key: `ui:chrome:preferences-unit-display-page`
- Fusion: General precision defaults to "0.123".
- FissionOxide: "0.12" with nothing stored in settings.ini.
- Start here: `crates/ui/src/preferences.rs:274` and `:382` (the precision list and its default).

## Preferences window size and the tree on open (P2)
- key: `ui:chrome:preferences-window-look`
- Fusion: on the same display the window measures 748 x 447 screenshot px (about 986 x 589 screen px at the 1.3187 screenshot scale); opening on General shows General's children expanded.
- FissionOxide: 579 x 360 screenshot px (`SIZE` is 748 x 447 points, which are screen px here), and General opens collapsed.
- Start here: `crates/ui/src/preferences.rs:1097` (`SIZE`: the recorded size was in screenshot px, so scale it by 1.3187), and the tree's expand-on-open.

## Profile menu width (P3)
- key: `ui:chrome:profile-menu`
- Fusion: the avatar menu is a narrow list (about 108 screenshot px) under the avatar.
- FissionOxide: the entries match but the menu is about 453 screenshot px wide (x 447..900).
- Start here: `crates/ui/src/panels.rs:354` (`profile_menu`).

## Home page shows the design's Unsaved banner (P2)
- key: `ui:chrome:home-unsaved-banner`
- Fusion: the Home page shows nothing of the design underneath.
- FissionOxide: the "Unsaved: Changes may be lost  Save" banner is drawn over the Home page's "What's new" card.
- Start here: `crates/ui/src/panels.rs:3121` (`unsaved_banner`, called from `crates/ui/src/lib.rs:2283`): skip it while the Home view is shown.
