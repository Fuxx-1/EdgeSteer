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
binary="$repository_root/target/$target/release/edgesteer-ui"

if [[ ! -x "$binary" ]]; then
  echo "missing built application executable: $binary" >&2
  exit 1
fi

stage="$(mktemp -d "${TMPDIR:-/tmp}/edgesteer-dmg.XXXXXX")"
trap 'rm -rf "$stage"' EXIT
app="$stage/EdgeSteer.app"
resources="$app/Contents/Resources"
volume="$stage/volume"

mkdir -p "$app/Contents/MacOS" "$resources" "$volume"
install -m 755 "$binary" "$app/Contents/MacOS/edgesteer-ui"
if command -v xattr >/dev/null 2>&1; then
  xattr -c "$app/Contents/MacOS/edgesteer-ui" || true
fi
install -m 644 "$repository_root/packaging/macos/Info.plist" "$app/Contents/Info.plist"
install -m 644 "$repository_root/packaging/macos/EdgeSteer.icns" "$resources/EdgeSteer.icns"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $version" "$app/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $version" "$app/Contents/Info.plist"
install -m 644 "$repository_root/README.md" "$repository_root/LICENSE" "$repository_root/config.example.json" "$resources/"
/usr/bin/ditto "$repository_root/docs" "$resources/docs"

# Cargo may leave a signature on the executable itself. Sign the completed
# bundle so macOS validates its resources as one App instead of treating the
# copied binary as an incomplete signed application.
/usr/bin/codesign --force --deep --sign - "$app"

/usr/bin/ditto "$app" "$volume/EdgeSteer.app"
/bin/ln -s /Applications "$volume/Applications"

mkdir -p "$(dirname "$output")"
if [[ -n "${EDGE_STEER_APP_OUTPUT:-}" ]]; then
  /usr/bin/ditto "$app" "$EDGE_STEER_APP_OUTPUT"
fi
/usr/bin/hdiutil create -quiet -volname EdgeSteer -srcfolder "$volume" -ov -format UDZO "$output"
