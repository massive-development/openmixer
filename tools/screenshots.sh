#!/usr/bin/env bash
# Regenerates docs/screenshots/*.png from the real GPUI window (Linux, X11).
#
# Never opens an audio device or MIDI hardware: every scene runs with --mock-device. Needs the
# GPUI build deps, a display (DISPLAY, default :0 as on WSLg) and ImageMagick
# (`import`, `convert`, `identify`) plus `xwininfo`. Run from anywhere:  tools/screenshots.sh [scene...]
# Scenes: main-dark main-daylight main-stems debug-panel settings-audio settings-appearance
# settings-midi settings-library main-automix main-record settings-recording settings-mapping (default: all). main-automix runs the automix demo
# (--demo-automix: click tracks, automix on, deck A playing) and is captured after a delay. main-stems loads deck A's track with its stems
# (the mock separator, --mock-stems) so the stem pads are live.
# main-dark also shows the library browser (the demo library is loaded in every scene).
#
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out="$root/docs/screenshots"
work="$(mktemp -d "${TMPDIR:-/tmp}/om-shots.XXXXXX")"
bin="$root/target/debug/openmixer"
pid=""
cleanup() {
  [[ -n "$pid" ]] && kill "$pid" 2>/dev/null || true
  rm -rf "$work"
}
trap cleanup EXIT

# Lint: every ImageMagick `import` call must pass -silent (without it each capture rings the X11
# bell). Comment lines are skipped.
if grep -nE '^[^#]*\bimport ' "${BASH_SOURCE[0]}" | grep -v -- '-silent' | grep -v 'lint-ok'; then
  echo "screenshots.sh: an import call lacks -silent" >&2
  exit 1
fi

export DISPLAY="${DISPLAY:-:0}"
export WAYLAND_DISPLAY=   # force X11
mkdir -p "$out"

# Build (no-op when up to date). One cargo process, low priority.
if [[ -z "${SKIP_BUILD:-}" ]]; then
  (cd "$root" && nice -n 19 env CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-3}" \
    cargo build -p openmixer)
fi

# The window has no title; it is the 1280x800 client window in the root tree.
find_window() {
  xwininfo -root -tree 2>/dev/null \
    | awk '/\(has no name\): \(\)  1280x800\+/ {print $1; exit}'
}

# capture NAME [env VAR=VAL ...] -- [app flags...]
capture() {
  local name="$1"; shift
  local envs=()
  while [[ "$1" != "--" ]]; do envs+=("$1"); shift; done
  shift
  local log="$work/$name.log" win="" i
  # Temp preferences, library, config and data dirs: the user's own files are never touched.
  mkdir -p "$work/xdg-config" "$work/xdg-data" "$work/lib-$name"
  env "${envs[@]}" OPENMIXER_PREFS="$work/prefs.json" OPENMIXER_LIBRARY_DB="$work/lib-$name/library.db" \
    XDG_CONFIG_HOME="$work/xdg-config" XDG_DATA_HOME="$work/xdg-data" \
    "$bin" --mock-device "$@" >"$log" 2>&1 &
  pid=$!
  for ((i = 0; i < 60; i++)); do   # up to 30 s for the window
    kill -0 "$pid" 2>/dev/null || { echo "$name: app exited early"; cat "$log"; return 1; }
    win="$(find_window)"; [[ -n "$win" ]] && break
    sleep 0.5
  done
  [[ -n "$win" ]] || { echo "$name: no window after 30 s"; return 1; }
  # A scene that needs time to pass (the automix demo's transition) waits SHOT_DELAY seconds first.
  sleep "${SHOT_DELAY:-0}"
  # Settle: two captures in a row with the same size and a non-black picture (up to 15 s).
  local prev="" cur ok=""
  for ((i = 0; i < 30; i++)); do
    import -silent -window "$win" "$work/$name.png" 2>/dev/null || true
    cur="$(identify -format '%wx%h %[fx:mean>0.02?1:0]' "$work/$name.png" 2>/dev/null || true)"
    [[ -n "$cur" && "$cur" == "$prev" && "$cur" == *" 1" ]] && { ok=1; break; }
    prev="$cur"; sleep 0.5
  done
  [[ -n "$ok" ]] || echo "$name: warning, picture never settled (black or changing)"
  import -silent -window "$win" "$work/$name.png"
  kill "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true; pid=""
  convert "$work/$name.png" -strip -resize '1600x>' "$out/$name.png"
  if (( $(stat -c %s "$out/$name.png") > 400000 )); then
    convert "$work/$name.png" -strip -resize '1600x>' -colors 128 "$out/$name.png"
  fi
  echo "$name: $(identify -format '%wx%h' "$out/$name.png"), $(stat -c %s "$out/$name.png") bytes"
}

declare -A scenes=(
  [main-dark]="OPENMIXER_THEME=openmixer.dark -- --demo-library"
  [main-daylight]="OPENMIXER_THEME=openmixer.daylight -- --demo-library"
  [main-stems]="OPENMIXER_THEME=openmixer.dark -- --demo-library --mock-stems"
  [debug-panel]="OPENMIXER_THEME=openmixer.dark -- --demo-library --debug-panel"
  [settings-audio]="OPENMIXER_THEME=openmixer.dark -- --demo-library --settings=audio"
  [settings-appearance]="OPENMIXER_THEME=openmixer.dark -- --demo-library --settings=appearance"
  [settings-midi]="OPENMIXER_THEME=openmixer.dark -- --demo-library --settings=midi"
  [settings-library]="OPENMIXER_THEME=openmixer.dark -- --demo-library --settings=library"
  [main-automix]="OPENMIXER_THEME=openmixer.dark -- --demo-automix"
  [main-record]="OPENMIXER_THEME=openmixer.dark -- --demo-record"
  [settings-recording]="OPENMIXER_THEME=openmixer.dark -- --demo-library --settings=recording"
  # The Mapping tab over the three fixture controller maps (--demo-map: temp map file, mock MIDI).
  [settings-mapping]="OPENMIXER_THEME=openmixer.dark -- --demo-map --settings=mapping"
)
# Seconds to wait before a scene is captured (the automix demo deck plays from the start).
declare -A delays=(
  [main-automix]=19
  [main-record]=4
)
order=(main-dark main-daylight main-stems debug-panel settings-audio settings-appearance settings-midi settings-library main-automix main-record settings-recording settings-mapping)
(( $# )) && order=("$@")
for name in "${order[@]}"; do
  [[ -n "${scenes[$name]:-}" ]] || { echo "unknown scene: $name"; exit 2; }
  # shellcheck disable=SC2086
  SHOT_DELAY="${delays[$name]:-0}" capture "$name" ${scenes[$name]}
done
