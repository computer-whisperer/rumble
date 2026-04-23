#!/usr/bin/env bash
#
# Render the rumble-widgets gallery for every theme and save to /tmp.
# Used for eyeballing theme work against reference/mumble_screenshots/.
#
# Usage: scripts/screenshot_gallery.sh [theme ...]
#   themes: modern luna luna-dark mumble mumble-dark   (default: all five)

set -euo pipefail

cd "$(dirname "$0")/.."

themes=("$@")
if [ ${#themes[@]} -eq 0 ]; then
    themes=(modern luna luna-dark mumble mumble-dark)
fi

# Build once so the per-theme invocations don't re-compile.
cargo build -p rumble-widgets --example screenshot --quiet

for theme in "${themes[@]}"; do
    out="/tmp/gallery_${theme//-/_}.png"
    cargo run -p rumble-widgets --example screenshot --quiet -- \
        --theme "$theme" --out "$out"
done
