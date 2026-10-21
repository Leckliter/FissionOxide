# Fixer brief

You have one batch file, `parity/batches/<name>.md`. Make FissionOxide match Autodesk Fusion
for every item in it. FissionOxide is a Rust workspace (`crates/ui` is the egui front end,
`crates/engine` the command engine and scene, `crates/sketch` the sketch solver,
`crates/kernel` the OpenCASCADE bridge). Read `CONTRIBUTING.md` first.

## Rules
1. Fusion is the reference. Each item says what Fusion does; match that, not an improvement.
   If an item needs engine work the kernel cannot do, implement the UI side, grey the control,
   and say so in the ledger note.
2. Keep the change to the batch's items. If you find another gap, add a row to
   `parity/ui-features.jsonl` (key `ui:<area>:<slug>`, status `mismatch`, your note) instead
   of fixing it.
3. Add a test for every behaviour that can be tested without a screen (`crates/ui/src/*_tests.rs`
   and `input_tests.rs` show how the UI is driven headlessly; `crates/engine/src/tests.rs` for
   commands).
4. Build and check: `cargo build --release -p fissionoxide`, `cargo test --workspace
   --no-fail-fast`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --check`.
   `fuzz_features::fuzz_found_bugs` at seed 1022 and the fox-kernel test binary's exit crash are
   known failures on `main`; nothing else may fail. On Windows set `FOX_OCCT_DIR` to the main
   checkout's `target/OCCT` if the kernel needs it; in the cloud environment the build script
   fetches OCCT.
5. Record: in `parity/ui-features.jsonl` set each key's `status` to `fixing-merged`, append a
   history entry (`at`, `status`, `note` with what you changed and the commit), and keep
   `files` current. In `parity/batches/index.jsonl` set the batch to `done` with the commit
   hash in `pr`. A prober verifies it in the app and sets `verified`.
6. Ship: commit with a message that names the batch and the items, then
   `git pull --rebase origin main`, re-run the tests if the rebase touched your files, and
   `git push origin main`. Resolve conflicts yourself; other fixers push to `main` at the same
   time. Never force-push.
7. End with a short report (items done, items left with why, commit hash), then archive this
   session (`archive_session` on yourself where the tool exists; otherwise just end).
