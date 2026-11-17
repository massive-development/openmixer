#!/usr/bin/env bash
# Measures one window's CPU and memory for the Tauri trial (ADR 0009, plan Task 11). Linux only.
#
# Usage: tools/ui-bench.sh <gpui|tauri> <seconds> [extra openmixer args...]
#        tools/ui-bench.sh --self-test
#
# Starts the already-built binary in OPENMIXER_BIN (default target/debug/openmixer; this script
# never builds) with --mock-device and the extra args (tauri adds --ui=tauri --ui-bench=<seconds>),
# waits a 5 s warm-up, then samples utime + stime and VmRSS summed over the app and every
# descendant (WebKit helpers are children) for <seconds>. Prints one line of JSON; for tauri it
# merges in the app's own frame report (OPENMIXER_BENCH_OUT, default a temp file). The app is
# always stopped by pid on exit. --self-test checks the arithmetic on a fake /proc tree.
set -euo pipefail

PROC_ROOT="${PROC_ROOT:-/proc}"
WARM_UP_S=5
# How long a tauri run may take after the window to write its own report.
REPORT_WAIT_S=30

die() {
  echo "ui-bench: $*" >&2
  exit 1
}

# Milliseconds since the epoch.
now_ms() {
  local ns
  ns=$(date +%s%N)
  echo $((ns / 1000000))
}

# "ppid ticks" of one process (utime + stime), or nothing once it is gone. The command name may
# hold spaces and parentheses, so the fields are read after its last ") ".
stat_of() {
  local line f
  { read -r line <"$PROC_ROOT/$1/stat"; } 2>/dev/null || return 0
  line=${line##*) }
  read -r -a f <<<"$line"
  echo "${f[1]} $((f[11] + f[12]))"
}

# Resident memory of one process in kB (0 for kernel threads or a process that is gone).
rss_of() {
  local key value rest
  { while read -r key value rest; do
    if [[ $key == VmRSS: ]]; then
      echo "$value"
      return 0
    fi
  done <"$PROC_ROOT/$1/status"; } 2>/dev/null || true
  echo 0
}

# "ticks rss_kb processes" summed over $1 and all its descendants; "0 0 0" when $1 is gone.
sample() {
  local root=$1 dir pid fields
  local -A ticks=() children=()
  for dir in "$PROC_ROOT"/[0-9]*; do
    pid=${dir##*/}
    fields=$(stat_of "$pid")
    [[ -n $fields ]] || continue
    ticks[$pid]=${fields#* }
    children[${fields%% *}]+=" $pid"
  done
  [[ -n ${ticks[$root]+x} ]] || { echo "0 0 0"; return 0; }
  local -a queue=("$root")
  local sum=0 rss=0 count=0 i=0
  while ((i < ${#queue[@]})); do
    pid=${queue[i]}
    i=$((i + 1))
    sum=$((sum + ticks[$pid]))
    rss=$((rss + $(rss_of "$pid")))
    count=$((count + 1))
    # shellcheck disable=SC2206 # the list is space-separated pids
    queue+=(${children[$pid]:-})
  done
  echo "$sum $rss $count"
}

# Percent of one core: tick delta over the wall time, at clk ticks a second.
cpu_percent() {
  local t0=$1 t1=$2 clk=$3 ms=$4
  awk -v d=$((t1 - t0)) -v clk="$clk" -v ms="$ms" \
    'BEGIN { if (ms <= 0) print "0.0"; else printf "%.1f\n", d * 100000 / (clk * ms) }'
}

# kB to MB with one decimal.
mb_of() {
  awk -v kb="$1" 'BEGIN { printf "%.1f\n", kb / 1024 }'
}

# Writes a fake process: pid, ppid, utime, stime, rss in kB (empty for none), command name.
fake_proc() {
  local root=$1 pid=$2 ppid=$3 ut=$4 st=$5 rss=$6 comm=$7
  mkdir -p "$root/$pid"
  echo "$pid ($comm) S $ppid $pid $pid 0 -1 4194304 10 0 0 0 $ut $st 0 0 20 0 1 0 100 1000 50" \
    >"$root/$pid/stat"
  if [[ -n $rss ]]; then
    printf 'Name:\t%s\nVmRSS:\t%s kB\nThreads:\t1\n' "$comm" "$rss" >"$root/$pid/status"
  else
    printf 'Name:\t%s\nThreads:\t1\n' "$comm" >"$root/$pid/status"
  fi
}

expect() {
  if [[ $2 != "$3" ]]; then
    echo "ui-bench self-test FAILED: $1: expected '$3', got '$2'" >&2
    exit 1
  fi
}

self_test() {
  local root
  root=$(mktemp -d)
  # Expanded now: the trap runs after this function's locals are gone.
  # shellcheck disable=SC2064
  trap "rm -rf '$root'" EXIT
  PROC_ROOT=$root
  fake_proc "$root" 1 0 5 5 100 init
  fake_proc "$root" 100 1 50 25 2048 openmixer
  fake_proc "$root" 101 100 300 100 1024 "WebKit (Web) Process"
  fake_proc "$root" 102 101 20 30 "" kworker
  fake_proc "$root" 200 1 999 999 4096 unrelated
  mkdir -p "$root/self"
  expect "tree of the app" "$(sample 100)" "525 3072 3"
  expect "subtree" "$(sample 101)" "450 1024 2"
  expect "a process that is gone" "$(sample 300)" "0 0 0"
  expect "one core for 2 s" "$(cpu_percent 0 200 100 2000)" "100.0"
  expect "two cores for 4 s" "$(cpu_percent 525 1325 100 4000)" "200.0"
  expect "a third of a core, 250 Hz clock" "$(cpu_percent 0 250 250 3000)" "33.3"
  expect "no wall time" "$(cpu_percent 0 10 100 0)" "0.0"
  expect "memory" "$(mb_of 3072)" "3.0"
  # The child gains 150 ticks between samples.
  fake_proc "$root" 101 100 400 150 1024 "WebKit (Web) Process"
  expect "second sample" "$(sample 100)" "675 3072 3"
  echo "ui-bench self-test ok"
}

# The app's own report on one line without its outer braces, or nothing.
report_fields() {
  [[ -s $1 ]] || return 0
  tr -d '\n' <"$1" | tr -s ' ' | sed -e 's/^ *{ *//' -e 's/ *} *$//'
}

usage() {
  echo "usage: tools/ui-bench.sh <gpui|tauri> <seconds> [extra openmixer args...] | --self-test" >&2
  exit 2
}

if [[ ${1:-} == --self-test ]]; then
  self_test
  exit 0
fi
if [[ $(uname -s) != Linux ]]; then
  echo "ui-bench: Linux only (it reads /proc); on $(uname -s) measure with the OS's own tools" >&2
  exit 2
fi
(($# >= 2)) || usage
ui=$1
seconds=$2
shift 2
[[ $ui == gpui || $ui == tauri ]] || usage
[[ $seconds =~ ^[0-9]+$ ]] && ((seconds >= 1 && seconds <= 600)) || die "seconds must be 1 to 600"

repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
bin=${OPENMIXER_BIN:-$repo/target/debug/openmixer}
[[ -x $bin ]] || die "no binary at $bin (build it first; this script never runs cargo)"
clk=$(getconf CLK_TCK)
work=$(mktemp -d)
args=(--mock-device "$@")
if [[ $ui == tauri ]]; then
  args=(--ui=tauri "--ui-bench=$seconds" "${args[@]}")
  export OPENMIXER_BENCH_OUT=${OPENMIXER_BENCH_OUT:-$work/bench.json}
  # Only a report written after this marker belongs to this run.
  touch "$work/started"
fi

pid=""
stop_app() {
  if [[ -n $pid ]] && kill -0 "$pid" 2>/dev/null; then
    kill "$pid" 2>/dev/null || true
    for _ in 1 2 3 4 5 6 7 8 9 10; do
      kill -0 "$pid" 2>/dev/null || break
      sleep 0.5
    done
    kill -9 "$pid" 2>/dev/null || true
  fi
  rm -rf "$work"
}
trap stop_app EXIT
trap 'exit 130' INT TERM

WAYLAND_DISPLAY= "$bin" "${args[@]}" >"$work/app.log" 2>&1 &
pid=$!
sleep "$WARM_UP_S"
kill -0 "$pid" 2>/dev/null || die "the app exited during the warm-up: $(tail -n 5 "$work/app.log")"

read -r ticks0 _ _ < <(sample "$pid")
start=$(now_ms)
last="$ticks0 0 0"
last_at=$start
end=$((start + seconds * 1000))
# Sample every second so a tauri run that exits on time still has its last reading.
while (($(now_ms) < end)); do
  left=$((end - $(now_ms)))
  sleep "$(awk -v ms="$left" 'BEGIN { printf "%.3f", (ms > 1000 ? 1000 : ms) / 1000 }')"
  kill -0 "$pid" 2>/dev/null || break
  at=$(now_ms)
  reading=$(sample "$pid")
  [[ $reading == "0 0 0" ]] && break
  last=$reading
  last_at=$at
done
read -r ticks1 rss_kb processes <<<"$last"
elapsed=$((last_at - start))

extra=""
if [[ $ui == tauri ]]; then
  for _ in $(seq 1 "$REPORT_WAIT_S"); do
    kill -0 "$pid" 2>/dev/null || break
    sleep 1
  done
  if [[ $OPENMIXER_BENCH_OUT -nt $work/started ]]; then
    fields=$(report_fields "$OPENMIXER_BENCH_OUT")
    [[ -n $fields ]] && extra=", $fields"
  else
    echo "ui-bench: the app wrote no frame report: $(tail -n 5 "$work/app.log")" >&2
  fi
fi

printf '{"ui": "%s", "seconds": %d, "sampledSeconds": %s, "cpuPercentOneCore": %s, "rssMb": %s, "processes": %d%s}\n' \
  "$ui" "$seconds" "$(awk -v ms="$elapsed" 'BEGIN { printf "%.1f", ms / 1000 }')" \
  "$(cpu_percent "$ticks0" "$ticks1" "$clk" "$elapsed")" "$(mb_of "$rss_kb")" "$processes" "$extra"
