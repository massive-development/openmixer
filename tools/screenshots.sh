#!/usr/bin/env bash
# Captures the real GPUI window (Linux, X11) into $OUT_DIR (see below).
#
# Never opens an audio device or MIDI hardware: every scene runs with --mock-device. Needs the
# GPUI build deps, a display (DISPLAY, default :0 as on WSLg) and ImageMagick
# (`import`, `convert`, `identify`) plus `xwininfo`. Run from anywhere:  tools/screenshots.sh [scene...]
# Output goes to $OUT_DIR, default a scratch folder (/tmp/om-scenes), never docs/screenshots: set
# OUT_DIR=docs/screenshots only to refresh the tracked captures on purpose. Another window of the
# same size (another agent's run) may be open: the script captures the newest window that appeared
# after it started its own process, by id, and never one that was already there.
# Staged-state scenes (--demo-scene, DEMO / TEST ONLY flags, see crates/openmixer/src/demo_scene.rs):
# sampler-open-{dark,daylight}  sampler drawer open, pads loaded, the Bass loop pad playing
# cues-loop-{dark,daylight}     deck A: cues 1-5 set, playing from cue 3, a one-beat loop and its hatch
# meters-lit-{dark,daylight}    deck A looped and hot: channel and master meters lit, clip lamp
# stems-page                    deck A on its STEMS tab with the mock separator's stems loaded
# track-preview                 deck A plays your own file: OPENMIXER_DEMO_TRACK=/path/to/song.flac
#                               tools/screenshots.sh track-preview (linked into a scratch folder, never
#                               copied; waits SHOT_DELAY, default 15 s, for the analysis and waveform)
# platter-{dark,daylight}       deck A playing with the platter toggled on
# Scenes: main-dark main-daylight main-stems debug-panel settings-audio settings-appearance
# settings-midi settings-library main-automix main-record settings-recording settings-mapping main-waveform (default: all). main-waveform is the demo loop on deck A after the analysis finishes (OPENMIXER_WAVE_STYLE picks the style; OUT_DIR overrides the output folder). main-automix runs the automix demo
# (--demo-automix: click tracks, automix on, deck A playing) and is captured after a delay. main-stems loads deck A's track with its stems
# (the mock separator, --mock-stems) so the stem pads are live.
# main-dark also shows the library browser (the demo library is loaded in every scene).
#
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
out="${OUT_DIR:-${TMPDIR:-/tmp}/om-scenes}"
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

# The window has no title; it is a 1280x800 client window in the root tree (the app opens at
# 1920x1080 by default, so every capture pins OPENMIXER_WINDOW=1280x800). Other 1280x800 windows
# may be open (another agent's run), so the ids present before the app starts are remembered and
# the newest id that is not among them is the app's window.
list_windows() {
  xwininfo -root -tree 2>/dev/null \
    | awk '/\(has no name\): \(\)  1280x800\+/ {print $1}'
}
# find_window BEFORE: newest window id (numeric order) in list_windows that is not in BEFORE.
find_window() {
  local before="$1" id best="" bestn=-1
  for id in $(list_windows); do
    grep -qxF "$id" <<<"$before" && continue
    if (( id > bestn )); then best="$id"; bestn=$((id)); fi
  done
  echo "$best"
}

# capture NAME [env VAR=VAL ...] -- [app flags...]
capture() {
  local name="$1"; shift
  local envs=()
  while [[ "$1" != "--" ]]; do envs+=("$1"); shift; done
  shift
  local log="$work/$name.log" win="" i before
  before="$(list_windows)"
  # Temp preferences, library, config and data dirs: the user's own files are never touched.
  mkdir -p "$work/xdg-config" "$work/xdg-data" "$work/lib-$name"
  env "${envs[@]}" OPENMIXER_WINDOW=1280x800 OPENMIXER_PREFS="$work/prefs.json" OPENMIXER_LIBRARY_DB="$work/lib-$name/library.db" \
    XDG_CONFIG_HOME="$work/xdg-config" XDG_DATA_HOME="$work/xdg-data" \
    "$bin" --mock-device "$@" >"$log" 2>&1 &
  pid=$!
  for ((i = 0; i < 60; i++)); do   # up to 30 s for the window
    kill -0 "$pid" 2>/dev/null || { echo "$name: app exited early"; cat "$log"; return 1; }
    win="$(find_window "$before")"; [[ -n "$win" ]] && break
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
  [main-waveform]="OPENMIXER_THEME=openmixer.dark -- --demo-library"
  [main-record]="OPENMIXER_THEME=openmixer.dark -- --demo-record"
  [sampler-open-dark]="OPENMIXER_THEME=openmixer.dark -- --demo-scene=sampler"
  [sampler-open-daylight]="OPENMIXER_THEME=openmixer.daylight -- --demo-scene=sampler"
  [cues-loop-dark]="OPENMIXER_THEME=openmixer.dark -- --demo-scene=cues"
  [cues-loop-daylight]="OPENMIXER_THEME=openmixer.daylight -- --demo-scene=cues"
  [meters-lit-dark]="OPENMIXER_THEME=openmixer.dark -- --demo-scene=meters"
  [meters-lit-daylight]="OPENMIXER_THEME=openmixer.daylight -- --demo-scene=meters"
  [stems-page]="OPENMIXER_THEME=openmixer.dark -- --demo-scene=stems"
  [platter-dark]="OPENMIXER_THEME=openmixer.dark -- --demo-scene=platter"
  [platter-daylight]="OPENMIXER_THEME=openmixer.daylight -- --demo-scene=platter"
  [track-preview]="OPENMIXER_THEME=openmixer.dark -- --demo-scene=track"
  [settings-recording]="OPENMIXER_THEME=openmixer.dark -- --demo-library --settings=recording"
  # The Mapping tab over the three fixture controller maps (--demo-map: temp map file, mock MIDI).
  [settings-mapping]="OPENMIXER_THEME=openmixer.dark -- --demo-map --settings=mapping"
)
# Seconds to wait before a scene is captured (the automix demo deck plays from the start).
declare -A delays=(
  [main-automix]=19
  [main-waveform]=8
  [main-record]=4
  [sampler-open-dark]=3
  [sampler-open-daylight]=3
  [cues-loop-dark]=2
  [cues-loop-daylight]=2
  [meters-lit-dark]=3
  [meters-lit-daylight]=3
  [stems-page]=3
  [platter-dark]=3
  [platter-daylight]=3
  [track-preview]=15
)
# track-preview needs OPENMIXER_DEMO_TRACK, so it is not in the default set.
order=(main-dark main-daylight main-stems debug-panel settings-audio settings-appearance settings-midi settings-library main-automix main-record settings-recording settings-mapping sampler-open-dark sampler-open-daylight cues-loop-dark cues-loop-daylight meters-lit-dark meters-lit-daylight stems-page platter-dark platter-daylight)
(( $# )) && order=("$@")
for name in "${order[@]}"; do
  [[ -n "${scenes[$name]:-}" ]] || { echo "unknown scene: $name"; exit 2; }
  if [[ "$name" == track-preview && -z "${OPENMIXER_DEMO_TRACK:-}" ]]; then
    echo "track-preview: set OPENMIXER_DEMO_TRACK=/path/to/an/audio/file"; exit 2
  fi
  # shellcheck disable=SC2086
  SHOT_DELAY="${SHOT_DELAY:-${delays[$name]:-0}}" capture "$name" ${scenes[$name]}
done
