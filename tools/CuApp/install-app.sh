#!/bin/bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
source "$ROOT/tools/CuApp/signing.sh"
DEST="/Applications/Cu.app"
OLD="$HOME/Applications/Cu.app"
REGISTER="/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister"
PREPARED=""
ALLOW_IDENTITY_CHANGE=0
while [ "$#" -gt 0 ]; do
  case "$1" in
    --prepared)
      [ "$#" -ge 2 ] || { echo '--prepared requires a bundle path' >&2; exit 1; }
      PREPARED="$2"; shift 2 ;;
    --allow-identity-change) ALLOW_IDENTITY_CHANGE=1; shift ;;
    *) echo 'Usage: install-app.sh [--prepared BUNDLE] [--allow-identity-change]' >&2; exit 1 ;;
  esac
done
cu_load_signing_identity
unregister() {
  if ! "$REGISTER" -u "$1"; then printf 'Note: no prior launch registration for %s\n' "$1" >&2; fi
}
for APP in "$DEST" "$OLD"; do
  if /bin/ps -axo comm= | /usr/bin/grep -Fx -- "$APP/Contents/MacOS/Cu" >/dev/null; then
    echo "Quit Cu before installing (running copy: $APP). No app was changed." >&2
    exit 1
  fi
done
[ -w /Applications ] || { echo '/Applications is not writable; this script never runs sudo.' >&2; exit 1; }
if [ -d "$DEST" ]; then
  [ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$DEST/Contents/Info.plist")" = dev.ankojh.cu ] || {
    echo "Refusing to replace a different app at $DEST" >&2; exit 1;
  }
  if ! cu_verify_signed_code "$DEST" dev.ankojh.cu >/dev/null 2>&1; then
    if [ "$ALLOW_IDENTITY_CHANGE" != 1 ]; then
      echo 'Existing Cu has a different/unstable signing identity. Explicit --allow-identity-change is required for this one-time permission migration.' >&2
      exit 1
    fi
    echo 'Explicit signing-identity migration: manual macOS grant renewal may be necessary.'
  fi
fi
if [ -n "$PREPARED" ]; then
  STAGE="$PREPARED"
else
  STAGE="$ROOT/target/Cu-install-staging/Cu.prepared"
  "$ROOT/tools/CuApp/build-app.sh" "$STAGE"
fi
cu_verify_bundle "$STAGE"
EXPECTED_HASH="$(/usr/bin/codesign -dv --verbose=4 "$STAGE" 2>&1 | /usr/bin/awk -F= '/^CDHash=/{print $2}')"
[ -n "$EXPECTED_HASH" ] || { echo 'Missing prepared signature hash.' >&2; exit 1; }
mkdir -p "$ROOT/target/app-backups"
BACKUPS="$(mktemp -d "$ROOT/target/app-backups/$(date +%Y%m%dT%H%M%S).XXXXXX")"
SYSTEM_STAGE="$(mktemp -d /Applications/.cu-install.XXXXXX)"
NEW_INSTALLED=0
cleanup() {
  local status="$1"
  trap - EXIT
  if [ "$status" != 0 ]; then
    if [ "$NEW_INSTALLED" = 1 ] && [ -d "$DEST" ]; then
      mv "$DEST" "$BACKUPS/Cu-failed.disabled"
    fi
    if [ -d "$BACKUPS/Cu-system.disabled" ] && [ ! -e "$DEST" ]; then
      mv "$BACKUPS/Cu-system.disabled" "$DEST"
      "$REGISTER" -f "$DEST" || true
      echo 'Installation failed; previous installed Cu restored.' >&2
    fi
  fi
  rmdir "$SYSTEM_STAGE" 2>/dev/null || true
  exit "$status"
}
trap 'cleanup "$?"' EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
# Copy and verify without rebuilding or re-signing the reviewed artifact.
ditto "$STAGE" "$SYSTEM_STAGE/Cu.app"
cu_verify_bundle "$SYSTEM_STAGE/Cu.app"
if [ -d "$DEST" ] && [ "$ALLOW_IDENTITY_CHANGE" != 1 ]; then
  PRIOR_REQUIREMENT="$(cu_designated_requirement "$DEST")"
  [ -n "$PRIOR_REQUIREMENT" ] || { echo 'Cannot establish prior identity.' >&2; exit 1; }
  /usr/bin/codesign --verify --strict "-R=$PRIOR_REQUIREMENT" "$SYSTEM_STAGE/Cu.app"
fi
if [ -d "$DEST" ]; then
  unregister "$DEST"
  mv "$DEST" "$BACKUPS/Cu-system.disabled"
fi
mv "$SYSTEM_STAGE/Cu.app" "$DEST"
NEW_INSTALLED=1
cu_verify_bundle "$DEST"
[ "$(/usr/bin/codesign -dv --verbose=4 "$DEST" 2>&1 | /usr/bin/awk -F= '/^CDHash=/{print $2}')" = "$EXPECTED_HASH" ] || {
  echo 'Installed artifact differs from prepared artifact.' >&2; exit 1;
}
"$REGISTER" -f "$DEST"
if [ -d "$OLD" ] && [ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$OLD/Contents/Info.plist")" = dev.ankojh.cu ]; then
  unregister "$OLD"
  mv "$OLD" "$BACKUPS/Cu-user.disabled"
fi
printf '\nInstalled exact verified artifact: %s\nIdentity: %s\nPrevious copies: %s\n' "$DEST" "$CU_SIGNING_IDENTITY" "$BACKUPS"
printf 'Launch with: open "%s" (click Cu in the menu bar for controls)\n' "$DEST"
echo 'No TCC reset or permission request was performed. Keep this identity for future updates.'
