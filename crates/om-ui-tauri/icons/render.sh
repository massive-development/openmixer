#!/usr/bin/env bash
# Renders the Tauri icon set from the two source drawings beside this script. Needs librsvg
# (rsvg-convert, or gdk-pixbuf-thumbnailer with the SVG loader), ImageMagick `convert` for the
# .ico, and Python 3 with Pillow for the .icns. ImageMagick's own SVG renderer is not used: it
# draws the arcs wrong.
#
# Usage: crates/om-ui-tauri/icons/render.sh
set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
large=$here/openmixer-icon.svg
small=$here/openmixer-icon-small.svg
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# render <svg> <pixels> <png>
render() {
  if command -v rsvg-convert >/dev/null; then
    rsvg-convert -w "$2" -h "$2" "$1" -o "$3"
  else
    gdk-pixbuf-thumbnailer -s "$2" "$1" "$3"
  fi
}

# The small drawing serves 32 px and below, the large one everything above (the comp's rule).
drawing() {
  if (($1 <= 32)); then echo "$small"; else echo "$large"; fi
}

render "$small" 32 "$here/32x32.png"
render "$large" 128 "$here/128x128.png"
render "$large" 256 "$here/128x128@2x.png"
render "$large" 512 "$here/icon.png"

ico=()
for size in 16 24 32 48 64 256; do
  render "$(drawing "$size")" "$size" "$work/ico-$size.png"
  ico+=("$work/ico-$size.png")
done
convert "${ico[@]}" "$here/icon.ico"

# macOS: the tile sits on Apple's 824/1024 grid with a clear margin (no shadow is drawn).
mac() {
  sed 's/viewBox="0 0 100 100"/viewBox="-12.136 -12.136 124.272 124.272"/' "$1" >"$work/mac.svg"
  render "$work/mac.svg" "$2" "$work/mac-$2.png"
}
for size in 32 64 128 256 512 1024; do
  mac "$(drawing "$size")" "$size"
done
python3 - "$work" "$here/icon.icns" <<'PY'
import sys
from PIL import Image

work, out = sys.argv[1], sys.argv[2]
images = [Image.open(f"{work}/mac-{size}.png").convert("RGBA") for size in (1024, 512, 256, 128, 64, 32)]
images[0].save(out, format="ICNS", append_images=images[1:])
PY
