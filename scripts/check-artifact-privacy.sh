#!/usr/bin/env bash

# Inspect distributable binaries for build-machine paths and locally configured
# private identifiers. Use this after build-release.sh and before packaging an
# archive or disk image.

set -euo pipefail

if [[ "$#" -eq 0 ]]; then
    printf '%s\n' "usage: scripts/check-artifact-privacy.sh <artifact> [<artifact> ...]" >&2
    exit 2
fi

root="$(git rev-parse --show-toplevel)"
cd "$root"
failed=0

check_pattern() {
    local artifact="$1"
    local pattern="$2"
    local description="$3"
    local matches

    if matches="$(rg -a -o --no-messages -i -e "$pattern" -- "$artifact")"; then
        printf '%s\n' "artifact privacy check failed: $description in $artifact" >&2
        printf '%s\n' "$matches" >&2
        failed=1
    fi
}

for artifact in "$@"; do
    if [[ ! -f "$artifact" ]]; then
        printf '%s\n' "artifact privacy check failed: missing artifact $artifact" >&2
        failed=1
        continue
    fi

    check_pattern "$artifact" '(^|[^[:alnum:]_])/(Users|home)/[^/[:space:]]+' 'an absolute user-home path'
    check_pattern "$artifact" '[[:alpha:]]:[\\/]+Users[\\/]+[^\\/[:space:]]+' 'a Windows user-home path'
done

privacy_terms="$(git config --local --get-all edgesteer.privacyTerm 2>/dev/null || true)"
if [[ -n "$privacy_terms" ]]; then
    while IFS= read -r term; do
        [[ -n "$term" ]] || continue
        escaped_term="$(printf '%s' "$term" | sed 's/[][\\.^$*+?{}|()]/\\\\&/g')"
        for artifact in "$@"; do
            [[ -f "$artifact" ]] || continue
            check_pattern "$artifact" "$escaped_term" 'a locally configured private identifier'
        done
    done <<< "$privacy_terms"
fi

exit "$failed"
