# Builds plugin.wasm for the LumenArt add-in. The app picks this folder up as a bundled plugin
# (in a checkout, and from plugins\ next to an installed fissionoxide.exe).
#   rustup target add wasm32-unknown-unknown   (once)
$ErrorActionPreference = 'Stop'
Push-Location $PSScriptRoot
try {
  cargo build --release --target wasm32-unknown-unknown
  if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
  Copy-Item target\wasm32-unknown-unknown\release\lumenart.wasm plugin.wasm -Force
  cargo run -q --release --manifest-path ..\..\Cargo.toml -p fissionoxide-cli -- plugin check .
  if ($LASTEXITCODE -ne 0) { throw "plugin check failed" }
} finally {
  Pop-Location
}
