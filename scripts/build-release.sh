#!/usr/bin/env bash

# Build distributable binaries without retaining the build machine's paths in
# Rust source-location metadata. The mappings use stable, non-user-specific
# prefixes and are computed locally rather than committed as absolute paths.

set -euo pipefail

repository_root="$(cd "$(dirname "$0")/.." && pwd -P)"
cd "$repository_root"

encoded_flags="${CARGO_ENCODED_RUSTFLAGS:-}"
separator=$'\x1f'

append_flag() {
    if [[ -n "$encoded_flags" ]]; then
        encoded_flags+="$separator"
    fi
    encoded_flags+="$1"
}

append_mapping() {
    local source="$1"
    local destination="$2"
    [[ -n "$source" ]] || return 0
    append_flag "--remap-path-prefix=${source}=${destination}"

    if command -v cygpath >/dev/null 2>&1; then
        local windows_source
        windows_source="$(cygpath -w "$source")"
        if [[ "$windows_source" != "$source" ]]; then
            append_flag "--remap-path-prefix=${windows_source}=${destination}"
        fi
    fi
}

if [[ -n "${HOME:-}" ]]; then
    append_mapping "$HOME" "/build-home"
fi

if [[ -n "${CARGO_HOME:-}" ]]; then
    append_mapping "$CARGO_HOME" "/cargo"
elif [[ -n "${HOME:-}" ]]; then
    append_mapping "$HOME/.cargo" "/cargo"
fi

# rustc resolves overlapping remaps in declaration order, so keep the project
# mapping last and avoid reducing it to the broader home-directory mapping.
append_mapping "$repository_root" "/edgesteer"

export CARGO_ENCODED_RUSTFLAGS="$encoded_flags"
unset RUSTFLAGS
export EDGESTEER_LIVE_MANIFEST_PATH="/edgesteer"

cargo build --locked --release "$@"
