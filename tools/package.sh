#!/usr/bin/env bash
# Builds the installers for the OS it runs on: Linux .deb, .rpm and .AppImage; Windows .msi and
# setup .exe (run from Git Bash); macOS .app and .dmg. The app is the Tauri window with no GPUI
# (`--no-default-features --features tauri-default`). Packages land in target/packages/<os>/.
#
#   tools/package.sh                   # this OS, every bundle type
#   tools/package.sh --bundles deb     # some bundle types (comma separated)
#   tools/package.sh --target aarch64-apple-darwin
#   tools/package.sh --docker          # Linux packages in a container (tools/package/Dockerfile)
#
# The packages are unsigned. CI runs this on each OS (.github/workflows/package.yml).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WEB="$ROOT/crates/om-ui-tauri/web"
BUNDLES=""
TARGET=""
DOCKER=0

usage() {
    sed -n '2,11p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'
    exit "${1:-0}"
}

parse_args() {
    while [ $# -gt 0 ]; do
        case "$1" in
            --bundles) BUNDLES="${2:?--bundles needs a list}"; shift 2 ;;
            --target) TARGET="${2:?--target needs a triple}"; shift 2 ;;
            --docker) DOCKER=1; shift ;;
            -h|--help) usage 0 ;;
            *) echo "package.sh: unknown argument $1" >&2; usage 1 ;;
        esac
    done
}

host_os() {
    case "$(uname -s)" in
        Linux) echo linux ;;
        Darwin) echo macos ;;
        MINGW*|MSYS*|CYGWIN*) echo windows ;;
        *) echo "package.sh: unsupported OS $(uname -s)" >&2; exit 1 ;;
    esac
}

# Real-time audio priority over D-Bus exists only on Linux (see om-audio).
features() {
    if [ "$1" = linux ]; then echo "tauri-default,realtime-dbus"; else echo "tauri-default"; fi
}

# The front end is embedded at compile time, so it is built first.
build_web() {
    (cd "$WEB" && npm ci && npm run build)
}

build_binary() {
    local args=(build --release --locked -p openmixer --no-default-features --features "$(features "$1")")
    [ -n "$TARGET" ] && args+=(--target "$TARGET")
    (cd "$ROOT" && cargo "${args[@]}")
}

bundle() {
    local args=(bundle --ci --config tauri.bundle.json)
    [ -n "$BUNDLES" ] && args+=(--bundles "$BUNDLES")
    [ -n "$TARGET" ] && args+=(--target "$TARGET")
    (cd "$ROOT/crates/om-ui-tauri" && "$WEB/node_modules/.bin/tauri" "${args[@]}")
}

# Copies the installers (not the intermediate folders) out of target/<triple>/release/bundle.
collect() {
    local target_dir="${CARGO_TARGET_DIR:-$ROOT/target}"
    local from="$target_dir/${TARGET:+$TARGET/}release/bundle"
    local to="$ROOT/target/packages/$1"
    rm -rf "$to" && mkdir -p "$to"
    find "$from" -maxdepth 2 -type f \
        \( -name '*.deb' -o -name '*.rpm' -o -name '*.AppImage' -o -name '*.msi' \
        -o -name '*.exe' -o -name '*.dmg' \) -exec cp {} "$to/" \;
    # The .app is a folder; ship it zipped beside the .dmg.
    if [ -d "$from/macos" ]; then
        (cd "$from/macos" && for app in *.app; do ditto -c -k --keepParent "$app" "$to/$app.zip"; done)
    fi
    echo "packages in $to:" && ls -1 "$to"
}

# The Linux packages in a Debian 12 container (glibc 2.36), so they run on that and newer, with the
# container's own target folder and node_modules so the host's stay untouched.
run_docker() {
    local image=openmixer-package
    docker build -t "$image" -f "$ROOT/tools/package/Dockerfile" "$ROOT/tools/package"
    docker run --rm \
        -v "$ROOT:/src" \
        -v openmixer-package-cargo:/usr/local/cargo/registry \
        -v openmixer-package-rustup:/usr/local/rustup \
        -v openmixer-package-node-modules:/src/crates/om-ui-tauri/web/node_modules \
        -e CARGO_TARGET_DIR=/src/target/docker \
        ${CARGO_BUILD_JOBS:+-e CARGO_BUILD_JOBS="$CARGO_BUILD_JOBS"} \
        -e HOST_IDS="$(id -u):$(id -g)" \
        "$image" bash -c 'tools/package.sh ${0:+--bundles "$0"}; status=$?;
            chown -R "$HOST_IDS" target/docker target/packages crates/om-ui-tauri/web/dist; exit $status' \
        "$BUNDLES"
}

main() {
    parse_args "$@"
    if [ "$DOCKER" = 1 ]; then
        run_docker
        return
    fi
    local os
    os="$(host_os)"
    build_web
    build_binary "$os"
    bundle
    collect "$os"
}

main "$@"
