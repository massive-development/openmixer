#!/usr/bin/env bash
# Writes SHA256SUMS-<os>.txt for the packages in a target/packages/<os>/ folder and, when
# OPENMIXER_GPG_KEY names a secret key in the keyring, a detached ASCII signature beside it
# (SHA256SUMS-<os>.txt.asc). OPENMIXER_GPG_PASSPHRASE unlocks the key without a prompt.
#
#   tools/package/checksums.sh target/packages/linux
#
# Run again after anything rewrites the packages (CI signs the Windows installers afterwards).
set -euo pipefail

DIR="${1:?usage: checksums.sh <packages folder>}"
SUMS="SHA256SUMS-$(basename "$DIR").txt"

sha256() {
    if command -v sha256sum >/dev/null; then sha256sum "$@"; else shasum -a 256 "$@"; fi
}

# Every package in the folder, not earlier sums or signatures.
write_sums() {
    local files=()
    for f in *; do
        case "$f" in SHA256SUMS*) ;; *) [ -f "$f" ] && files+=("$f") ;; esac
    done
    [ ${#files[@]} -gt 0 ] || { echo "checksums.sh: no packages in $DIR" >&2; exit 1; }
    sha256 "${files[@]}" > "$SUMS"
}

sign_sums() {
    local args=(--batch --yes --armor --detach-sign --local-user "$OPENMIXER_GPG_KEY")
    if [ -n "${OPENMIXER_GPG_PASSPHRASE:-}" ]; then
        args+=(--pinentry-mode loopback --passphrase-fd 0)
        printf '%s' "$OPENMIXER_GPG_PASSPHRASE" | gpg "${args[@]}" "$SUMS"
    else
        gpg "${args[@]}" "$SUMS"
    fi
}

cd "$DIR"
rm -f SHA256SUMS*
write_sums
if [ -n "${OPENMIXER_GPG_KEY:-}" ]; then
    sign_sums
    echo "signed $SUMS with $OPENMIXER_GPG_KEY"
fi
echo "wrote $DIR/$SUMS"
