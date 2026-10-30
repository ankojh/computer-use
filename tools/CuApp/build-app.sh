#!/bin/bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
CARGO="${CARGO:-$HOME/.cargo/bin/cargo}"
DEST="${1:-$ROOT/target/Cu.app}"
source "$ROOT/tools/CuApp/signing.sh"
cu_load_signing_identity
# Rebuilding a running bundle confuses macOS's code identity and TCC grants.
if /bin/ps -axo comm= | /usr/bin/grep -Fx -- "$DEST/Contents/MacOS/Cu" >/dev/null; then
  echo "Quit Cu before rebuilding $DEST" >&2
  exit 1
fi
"$CARGO" build --manifest-path "$ROOT/Cargo.toml" --release
swift build --package-path "$ROOT/tools/CuApp" -c release
BIN="$(swift build --package-path "$ROOT/tools/CuApp" -c release --show-bin-path)"
mkdir -p "$DEST/Contents/MacOS" "$DEST/Contents/Resources"
cp "$BIN/CuApp" "$DEST/Contents/MacOS/Cu"
cp "$ROOT/target/release/cu" "$DEST/Contents/Resources/cu-engine"
printf '%s\n' "$PATH" > "$DEST/Contents/Resources/launch-path.txt"
cp "$ROOT/tools/CuApp/Info.plist" "$DEST/Contents/Info.plist"
ICONSET="$ROOT/target/Cu.iconset"
mkdir -p "$ICONSET"
swift "$ROOT/tools/CuApp/make-icon.swift" "$ICONSET/icon_512x512@2x.png"
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" "$ICONSET/icon_512x512@2x.png" --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
  double=$((size * 2))
  if [ "$size" != 512 ]; then
    sips -z "$double" "$double" "$ICONSET/icon_512x512@2x.png" --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
  fi
done
iconutil -c icns "$ICONSET" -o "$DEST/Contents/Resources/Cu.icns"
chmod +x "$DEST/Contents/MacOS/Cu" "$DEST/Contents/Resources/cu-engine"
# Persistent certificate-backed designated requirements survive code changes.
# Missing/expired/untrusted identities fail closed, never fall back to ad-hoc.
cu_sign_code "$DEST/Contents/Resources/cu-engine" dev.ankojh.cu.engine
cu_sign_code "$DEST" dev.ankojh.cu
cu_verify_bundle "$DEST"
printf '\nBuilt native app: %s\nLaunch: open "%s"\n' "$DEST" "$DEST"
