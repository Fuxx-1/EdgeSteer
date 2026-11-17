#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 3 ]]; then
  echo "usage: $0 <rust-target> <version> <output-dmg>" >&2
  exit 64
fi

target="$1"
version="$2"
output="$3"
repository_root="$(cd "$(dirname "$0")/../.." && pwd)"

stage="$(mktemp -d "${TMPDIR:-/tmp}/edgesteer-dmg.XXXXXX")"
trap 'rm -rf "$stage"' EXIT
volume="$stage/volume"
mkdir -p "$volume"

# build-app.sh stages the Rust agent, the privileged DNS helper, and the nested
# Flutter interface bundle into one application.
"$repository_root/packaging/macos/build-app.sh" "$target" "$version" "$stage/EdgeSteer.app"

/usr/bin/ditto "$stage/EdgeSteer.app" "$volume/EdgeSteer.app"
/bin/ln -s /Applications "$volume/Applications"

mkdir -p "$(dirname "$output")"
/usr/bin/hdiutil create -quiet -volname EdgeSteer -srcfolder "$volume" -ov -format UDZO "$output"
