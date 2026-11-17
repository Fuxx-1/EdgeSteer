#!/bin/sh

# Reject common personal identifiers before they enter first-party source.
# Extra project-specific terms stay in local Git config so the denylist itself
# never exposes them: git config --local --add edgesteer.privacyTerm VALUE

set -eu

scope="tracked"
treeish=""
check_identity=0

usage() {
    printf '%s\n' "usage: scripts/check-privacy.sh [--tracked|--staged|--tree REV] [--check-identity]" >&2
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --tracked)
            scope="tracked"
            ;;
        --staged)
            scope="staged"
            ;;
        --tree)
            [ "$#" -ge 2 ] || {
                usage
                exit 2
            }
            scope="tree"
            treeish="$2"
            shift
            ;;
        --check-identity)
            check_identity=1
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            usage
            exit 2
            ;;
    esac
    shift
done

root="$(git rev-parse --show-toplevel)"
cd "$root"

if [ "$scope" = "staged" ]; then
    files="$(git diff --cached --name-only --diff-filter=ACMR -- . ':(exclude)third_party/**')"
elif [ "$scope" = "tracked" ]; then
    files="$(git ls-files -- . ':(exclude)third_party/**')"
else
    git rev-parse --verify "${treeish}^{tree}" >/dev/null
    files=""
fi

if [ "$scope" != "tree" ] && [ -z "$files" ]; then
    exit 0
fi

failed=0

scan_pattern() {
    pattern="$1"
    description="$2"
    matches=0

    if [ "$scope" = "tree" ]; then
        if git grep --color=never --line-number -i -E "$pattern" "$treeish" -- . ':(exclude)third_party/**'; then
            matches=1
        else
            status=$?
            if [ "$status" -ne 1 ]; then
                printf '%s\n' "privacy check failed: could not scan commit $treeish" >&2
                exit "$status"
            fi
        fi
    else
        while IFS= read -r file; do
            [ -f "$file" ] || continue
            if rg --color=never --line-number --no-heading --no-messages -i -e "$pattern" -- "$file"; then
                matches=1
            fi
        done <<EOF
$files
EOF
    fi

    if [ "$matches" -eq 1 ]; then
        printf '%s\n' "privacy check failed: found $description" >&2
        failed=1
    fi
}

scan_pattern '(^|[^[:alnum:]_])/(Users|home)/[^/[:space:]]+' 'an absolute user-home path'
scan_pattern '[[:alnum:]._%+-]+@[[:alnum:].-]+\.[[:alpha:]]{2,}' 'an email address'
scan_pattern '[[:alpha:]]:[\\/]+Users[\\/]+[^\\/[:space:]]+' 'a Windows user-home path'

privacy_terms="$(git config --local --get-all edgesteer.privacyTerm 2>/dev/null || true)"
if [ -n "$privacy_terms" ]; then
    while IFS= read -r term; do
        [ -n "$term" ] || continue
        scan_pattern "$(printf '%s' "$term" | sed 's/[][\\.^$*+?{}|()]/\\\\&/g')" 'a locally configured private identifier'
    done <<EOF
$privacy_terms
EOF
fi

if [ "$check_identity" -eq 1 ]; then
    expected_name="$(git config --local --get edgesteer.publicAuthorName 2>/dev/null || true)"
    expected_email="$(git config --local --get edgesteer.publicAuthorEmail 2>/dev/null || true)"

    if [ -z "$expected_name" ] || [ -z "$expected_email" ]; then
        printf '%s\n' "privacy check failed: configure a neutral public Git identity for this repository" >&2
        exit 1
    fi

    if [ "$scope" = "tree" ]; then
        author_name="$(git show --no-patch --format=%an "$treeish")"
        author_email="$(git show --no-patch --format=%ae "$treeish")"
        committer_name="$(git show --no-patch --format=%cn "$treeish")"
        committer_email="$(git show --no-patch --format=%ce "$treeish")"
        if [ "$author_name" != "$expected_name" ] || [ "$author_email" != "$expected_email" ] || [ "$committer_name" != "$expected_name" ] || [ "$committer_email" != "$expected_email" ]; then
            printf '%s\n' "privacy check failed: commit $treeish does not use the configured neutral public identity" >&2
            failed=1
        fi
    else
        author_name="$(git config --local --get user.name 2>/dev/null || true)"
        author_email="$(git config --local --get user.email 2>/dev/null || true)"
        if [ "$author_name" != "$expected_name" ] || [ "$author_email" != "$expected_email" ]; then
            printf '%s\n' "privacy check failed: local Git identity does not match the configured neutral public identity" >&2
            failed=1
        fi
    fi
fi

exit "$failed"
