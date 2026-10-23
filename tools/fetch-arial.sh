#!/bin/bash
# Installs Arial (arial.ttf, from Microsoft's freely distributable core fonts package, the file
# Debian's ttf-mscorefonts-installer fetches) into ~/.local/share/fonts, where sketch text finds
# it. The text reference parts (tests/reference/253-256) were measured in Fusion in Arial; without
# it their text draws in a stand-in with other outlines and they fail. Windows and macOS have Arial
# already. Needs curl and cabextract; does nothing when Arial is installed.
set -euo pipefail

DEST="${HOME}/.local/share/fonts"
URL="https://downloads.sourceforge.net/corefonts/arial32.exe"
SHA256="85297a4d146e9c87ac6f74822734bdee5f4b2a722d7eaa584b7f2cbf76f478f6"

if [ -f "$DEST/arial.ttf" ]; then
  exit 0
fi
for tool in curl cabextract sha256sum; do
  if ! command -v "$tool" >/dev/null; then
    echo "fetch-arial: $tool is needed" >&2
    exit 1
  fi
done
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
curl -fsSL --retry 4 --retry-delay 2 -o "$tmp/arial32.exe" "$URL"
echo "$SHA256  $tmp/arial32.exe" | sha256sum -c --quiet -
cabextract -q -L -d "$tmp" -F arial.ttf "$tmp/arial32.exe"
mkdir -p "$DEST"
install -m 644 "$tmp/arial.ttf" "$DEST/arial.ttf"
echo "fetch-arial: installed $DEST/arial.ttf"
