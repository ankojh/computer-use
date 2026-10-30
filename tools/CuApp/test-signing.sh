#!/bin/bash
# No installed app, desktop capture, input, or TCC access. Positive tests use
# the configured Keychain identity to sign disposable, never-launched fixtures.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
source "$HERE/signing.sh"
umask 077
WORK="$(mktemp -d "${TMPDIR:-/tmp}/cu-signing-tests.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT
for value in '' '-' not-a-fingerprint; do
  if (export HOME="$WORK/home" CU_SIGNING_IDENTITY="$value" CU_SIGNING_IDENTITY_FILE="$WORK/missing"; cu_load_signing_identity) >"$WORK/negative.log" 2>&1; then
    echo 'Missing/invalid identity was unexpectedly accepted.' >&2; exit 1
  fi
done
printf '%s\n' 'not-a-fingerprint' > "$WORK/invalid-config"
if (unset CU_SIGNING_IDENTITY; export CU_SIGNING_IDENTITY_FILE="$WORK/invalid-config"; cu_load_signing_identity) >"$WORK/negative.log" 2>&1; then
  echo 'Malformed identity configuration was unexpectedly accepted.' >&2; exit 1
fi
echo 'PASS: missing, ad-hoc, malformed identity/configuration fail closed.'
if [ "${1:-}" = --negative-only ]; then exit 0; fi
[ "$#" = 0 ] || { echo 'Usage: test-signing.sh [--negative-only]' >&2; exit 1; }
cu_load_signing_identity
FIRST="$WORK/first.prepared"
SECOND="$WORK/second.prepared"
mkdir -p "$FIRST/Contents/MacOS" "$FIRST/Contents/Resources"
cp /usr/bin/true "$FIRST/Contents/MacOS/Cu"
cp /usr/bin/true "$FIRST/Contents/Resources/cu-engine"
printf '%s\n' '<?xml version="1.0" encoding="UTF-8"?>' \
  '<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">' \
  '<plist version="1.0"><dict>' '<key>CFBundleIdentifier</key><string>dev.ankojh.cu</string>' \
  '<key>CFBundleExecutable</key><string>Cu</string>' '<key>CFBundlePackageType</key><string>APPL</string>' \
  '<key>CFBundleVersion</key><string>1</string>' '</dict></plist>' > "$FIRST/Contents/Info.plist"
cu_sign_code "$FIRST/Contents/Resources/cu-engine" dev.ankojh.cu.engine
cu_sign_code "$FIRST" dev.ankojh.cu
cu_verify_bundle "$FIRST"
ditto "$FIRST" "$SECOND"
/usr/libexec/PlistBuddy -c 'Set :CFBundleVersion 2' "$SECOND/Contents/Info.plist"
cp /usr/bin/false "$SECOND/Contents/Resources/cu-engine"
cu_sign_code "$SECOND/Contents/Resources/cu-engine" dev.ankojh.cu.engine
cu_sign_code "$SECOND" dev.ankojh.cu
cu_verify_bundle "$SECOND"
for part in '' '/Contents/Resources/cu-engine'; do
  FIRST_DR="$(cu_designated_requirement "$FIRST$part")"
  SECOND_DR="$(cu_designated_requirement "$SECOND$part")"
  [ "$FIRST_DR" = "$SECOND_DR" ]
  /usr/bin/codesign --verify --strict "-R=$FIRST_DR" "$SECOND$part"
  FIRST_HASH="$(/usr/bin/codesign -dv --verbose=4 "$FIRST$part" 2>&1 | awk -F= '/^CDHash=/{print $2}')"
  SECOND_HASH="$(/usr/bin/codesign -dv --verbose=4 "$SECOND$part" 2>&1 | awk -F= '/^CDHash=/{print $2}')"
  [ -n "$FIRST_HASH" ] && [ "$FIRST_HASH" != "$SECOND_HASH" ]
done
echo 'PASS: changed app and engine hashes retain identical requirements and satisfy the prior requirements.'
BAD="$WORK/bad.prepared"
ditto "$SECOND" "$BAD"
/usr/bin/codesign --force --sign - "$BAD" >/dev/null 2>&1
if cu_verify_bundle "$BAD" >"$WORK/negative.log" 2>&1; then
  echo 'Ad-hoc artifact was unexpectedly accepted.' >&2; exit 1
fi
/usr/bin/codesign --force --sign "$CU_SIGNING_IDENTITY" --timestamp=none \
  --identifier dev.ankojh.cu --requirements '=designated => identifier "dev.ankojh.cu"' "$BAD" >/dev/null 2>&1
if cu_verify_bundle "$BAD" >"$WORK/negative.log" 2>&1; then
  echo 'Identifier-only requirement was unexpectedly accepted.' >&2; exit 1
fi
echo 'PASS: ad-hoc and identifier-only signed bundles are rejected.'
