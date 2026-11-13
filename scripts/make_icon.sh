#!/usr/bin/env bash
# Regenerates assets/AppIcon.icns from scripts/make_icon.swift.
set -euo pipefail
cd "$(dirname "$0")/.."

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
swift scripts/make_icon.swift "$tmp/icon_1024.png"

set_dir="$tmp/AppIcon.iconset"
mkdir -p "$set_dir"
for s in 16 32 128 256 512; do
  sips -z $s $s "$tmp/icon_1024.png" --out "$set_dir/icon_${s}x${s}.png" >/dev/null
  sips -z $((s * 2)) $((s * 2)) "$tmp/icon_1024.png" --out "$set_dir/icon_${s}x${s}@2x.png" >/dev/null
done

mkdir -p assets
iconutil -c icns "$set_dir" -o assets/AppIcon.icns
cp "$tmp/icon_1024.png" assets/AppIcon.png
echo "wrote assets/AppIcon.icns"
