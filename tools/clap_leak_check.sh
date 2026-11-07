#!/usr/bin/env bash
# Runs the CLAP leak test under valgrind (ADR 0007, decision 3). Needs valgrind; slow, so few iterations.
# Usage: tools/clap_leak_check.sh [iterations]
set -euo pipefail

iterations="${1:-15}"
binary="$(cargo test -p om-plugins --test clap_leak --no-run --message-format=json \
  | python3 -c 'import json,sys
for line in sys.stdin:
    m = json.loads(line)
    if m.get("reason") == "compiler-artifact" and m.get("executable") and m["target"]["name"] == "clap_leak":
        print(m["executable"])')"

CLAP_LEAK_ITERATIONS="$iterations" valgrind --leak-check=full \
  --errors-for-leak-kinds=definite,indirect --error-exitcode=9 \
  "$binary" --test-threads=1
