#!/usr/bin/env bash
# Builds the installers for the OS it runs on: Linux .deb, .rpm and .AppImage; Windows .msi and
# setup .exe (run from Git Bash); macOS .app and .dmg. The app is the Tauri window
# (the default `gui-tauri` feature). Packages land in target/packages/<os>/.
#
#   tools/package.sh                   # this OS, every bundle type
#   tools/package.sh --bundles deb     # some bundle types (comma separated)
#   tools/package.sh --target aarch64-apple-darwin
#   tools/package.sh --docker          # Linux packages in a container (tools/package/Dockerfile)
#
# Signing, each step only when its keys are set (docs/features/tauri-trial.md, "Signing"):
#   OPENMIXER_GPG_KEY (+ OPENMIXER_GPG_PASSPHRASE)  GPG-signs SHA256SUMS-<os>.txt, and on Linux
#                                                   embeds a signature in the AppImage
#   APPLE_CERTIFICATE, APPLE_SIGNING_IDENTITY, ...  Developer ID signing and notarisation (Tauri);
#                                                   without them the .app is signed ad hoc
# Windows installers are signed in CI by SignPath, after this script. CI runs this on each OS
# (.github/workflows/package.yml).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WEB="$ROOT/crates/om-ui-tauri/web"
BUNDLES=""
TARGET=""
DOCKER=0

usage() {
    sed -n '2,/^set -e/p' "${BASH_SOURCE[0]}" | sed '$d; s/^# \{0,1\}//'
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
    if [ "$1" = linux ]; then echo "gui-tauri,realtime-dbus"; else echo "gui-tauri"; fi
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

# The AppImage tool embeds a GPG signature when SIGN is set (Tauri's Linux signing).
appimage_signing() {
    [ "$1" = linux ] && [ -n "${OPENMIXER_GPG_KEY:-}" ] || return 0
    export SIGN=1 SIGN_KEY="$OPENMIXER_GPG_KEY"
    export APPIMAGETOOL_SIGN_PASSPHRASE="${OPENMIXER_GPG_PASSPHRASE:-}"
}

# Where the bundler writes, under the cargo target folder.
bundle_dir() {
    echo "${CARGO_TARGET_DIR:-$ROOT/target}/${TARGET:+$TARGET/}release/bundle"
}

# Emptied first, so a bundle type left from an earlier run is not collected.
bundle() {
    rm -rf "$(bundle_dir)"
    local args=(bundle --ci --config tauri.bundle.json)
    [ -n "$BUNDLES" ] && args+=(--bundles "$BUNDLES")
    [ -n "$TARGET" ] && args+=(--target "$TARGET")
    (cd "$ROOT/crates/om-ui-tauri" && "$WEB/node_modules/.bin/tauri" "${args[@]}")
}

# Copies the installers (not the intermediate folders) out of target/<triple>/release/bundle.
collect() {
    local from
    from="$(bundle_dir)"
    local to="$ROOT/target/packages/$1"
    rm -rf "$to" && mkdir -p "$to"
    find "$from" -maxdepth 2 -type f \
        \( -name '*.deb' -o -name '*.rpm' -o -name '*.AppImage' -o -name '*.msi' \
        -o -name '*.exe' -o -name '*.dmg' \) -exec cp {} "$to/" \;
    # The .app is a folder; ship it zipped beside the .dmg.
    if [ -d "$from/macos" ]; then
        (cd "$from/macos" && for app in *.app; do ditto -c -k --keepParent "$app" "$to/$app.zip"; done)
    fi
    "$ROOT/tools/package/checksums.sh" "$to"
    echo "packages in $to:" && ls -1 "$to"
}

# The Linux packages in a Debian 12 container (glibc 2.36), so they run on that and newer, with the
# container's own target folder and node_modules so the host's stay untouched. The host's GPG
# keyring is not in the container, so the checksums are signed here afterwards; the AppImage carries
# no embedded signature from a Docker run.
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
        "$image" bash -c 'unset OPENMIXER_GPG_KEY; tools/package.sh ${0:+--bundles "$0"}; status=$?;
            chown -R "$HOST_IDS" target/docker target/packages crates/om-ui-tauri/web/dist; exit $status' \
        "$BUNDLES"
}

main() {
    parse_args "$@"
    if [ "$DOCKER" = 1 ]; then
        run_docker
        "$ROOT/tools/package/checksums.sh" "$ROOT/target/packages/linux"
        return
    fi
    local os
    os="$(host_os)"
    build_web
    build_binary "$os"
    appimage_signing "$os"
    bundle
    collect "$os"
}

main "$@"
