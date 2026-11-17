#!/usr/bin/env bash
#
# Builds the EdgeSteer macOS application bundle.
#
# The bundle contains three cooperating pieces:
#
#   Contents/MacOS/edgesteer-agent    menu-bar agent; owns the DNS engine
#   Contents/MacOS/edgesteer-engine   privileged DNS helper (port 53)
#   Contents/MacOS/EdgeSteer.app      the Flutter interface window
#
# The interface is a nested bundle so closing its window only ends the window
# process. The agent keeps the Rust DNS engine running, which is the whole point
# of the split.
#
# Usage: build-app.sh <rust-target> <version> <output-app>
#   RUST_BIN_DIR   directory holding the built Rust binaries (default:
#                  target/<rust-target>/release)

set -euo pipefail

if [[ $# -ne 3 ]]; then
  echo "usage: $0 <rust-target> <version> <output-app>" >&2
  exit 64
fi

target="$1"
version="$2"
output="$3"
repository_root="$(cd "$(dirname "$0")/../.." && pwd)"
bin_dir="${RUST_BIN_DIR:-$repository_root/target/$target/release}"
ui_app="$repository_root/ui/build/macos/Build/Products/Release/EdgeSteer.app"

for binary in edgesteer-agent edgesteer-engine; do
  if [[ ! -x "$bin_dir/$binary" ]]; then
    echo "missing built binary: $bin_dir/$binary" >&2
    exit 1
  fi
done
if [[ ! -d "$ui_app" ]]; then
  echo "missing built interface bundle: $ui_app" >&2
  echo "run 'flutter build macos --release' in ui/ first" >&2
  exit 1
fi

stage="$(mktemp -d "${TMPDIR:-/tmp}/edgesteer-app.XXXXXX")"
trap 'rm -rf "$stage"' EXIT
app="$stage/EdgeSteer.app"
resources="$app/Contents/Resources"
macos="$app/Contents/MacOS"

mkdir -p "$macos" "$resources"

install -m 755 "$bin_dir/edgesteer-agent" "$macos/edgesteer-agent"
install -m 755 "$bin_dir/edgesteer-engine" "$macos/edgesteer-engine"
install -m 644 "$repository_root/packaging/macos/Info.plist" "$app/Contents/Info.plist"
install -m 644 "$repository_root/packaging/macos/EdgeSteer.icns" "$resources/EdgeSteer.icns"
install -m 644 "$repository_root/README.md" "$repository_root/LICENSE" "$repository_root/config.example.json" "$resources/"
/usr/bin/ditto "$repository_root/docs" "$resources/docs"

# The Flutter interface keeps its own resources (assets and the Flutter engine)
# inside the nested bundle.
/usr/bin/ditto "$ui_app" "$macos/EdgeSteer.app"

/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $version" "$app/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $version" "$app/Contents/Info.plist"
/usr/libexec/PlistBuddy \
  -c "Set :CFBundleShortVersionString $version" \
  "$macos/EdgeSteer.app/Contents/Info.plist"
/usr/libexec/PlistBuddy \
  -c "Set :CFBundleVersion $version" \
  "$macos/EdgeSteer.app/Contents/Info.plist"

# Cargo and Flutter may leave signatures on individual executables. Sign the
# completed bundles so macOS validates each one as a single application.
/usr/bin/codesign --force --deep --sign - "$macos/EdgeSteer.app"
/usr/bin/codesign --force --deep --sign - "$app"

if [[ -n "${EDGE_STEER_APP_OUTPUT:-}" ]]; then
  rm -rf "$EDGE_STEER_APP_OUTPUT"
  /usr/bin/ditto "$app" "$EDGE_STEER_APP_OUTPUT"
fi

rm -rf "$output"
/usr/bin/ditto "$app" "$output"
