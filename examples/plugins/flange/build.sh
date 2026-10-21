#!/bin/sh
# Builds plugin.wasm and packs flange.foxplugin next to this script.
#   rustup target add wasm32-unknown-unknown   (once)
set -e
cd "$(dirname "$0")"
cargo build --release --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/flange_plugin.wasm plugin.wasm
cargo run -q --manifest-path ../../../Cargo.toml -p fissionoxide-cli -- plugin pack . flange.foxplugin
