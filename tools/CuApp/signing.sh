#!/bin/bash
# Shared by build/install/setup. No key generation or ad-hoc fallback here.

cu_load_signing_identity() {
  local file="${CU_SIGNING_IDENTITY_FILE:-$HOME/.config/cu/signing-identity}"
  CU_SIGNING_IDENTITY="${CU_SIGNING_IDENTITY:-}"
  if [ -z "$CU_SIGNING_IDENTITY" ] && [ -f "$file" ]; then
    CU_SIGNING_IDENTITY="$(tr -d '\r\n' < "$file")"
  fi
  if ! [[ "$CU_SIGNING_IDENTITY" =~ ^[[:xdigit:]]{40}$ ]]; then
    echo 'Cu requires a persistent certificate SHA-1 fingerprint (40 hex characters), not ad-hoc signing.' >&2
    echo 'Run tools/CuApp/setup-signing.sh --create-local, or --use <existing-certificate-fingerprint>.' >&2
    return 1
  fi
  CU_SIGNING_IDENTITY="$(printf '%s' "$CU_SIGNING_IDENTITY" | tr '[:lower:]' '[:upper:]')"
  if ! /usr/bin/security find-identity -v -p codesigning | grep -F " $CU_SIGNING_IDENTITY " >/dev/null; then
    echo "Configured Cu signing identity is unavailable, expired, or untrusted: $CU_SIGNING_IDENTITY" >&2
    echo 'Restore/unlock that identity in Keychain; do not generate a new certificate for each build.' >&2
    return 1
  fi
}

cu_requirement() {
  # Pin the certificate, not the changing CodeDirectory hash. Do not weaken this
  # to an identifier-only requirement: another signer could impersonate Cu.
  printf 'identifier "%s" and certificate leaf = H"%s"' "$1" "$CU_SIGNING_IDENTITY"
}

cu_designated_requirement() {
  /usr/bin/codesign -d -r- "$1" 2>&1 | /usr/bin/awk '/^(# )?designated => / {sub(/^(# )?designated => /, ""); print}'
}

cu_verify_signed_code() {
  local path="$1" identifier="$2" expected actual
  /usr/bin/codesign --verify --strict "-R=$(cu_requirement "$identifier")" "$path" || return 1
  expected="$(cu_requirement "$identifier" | tr '[:upper:]' '[:lower:]')"
  actual="$(cu_designated_requirement "$path" | tr '[:upper:]' '[:lower:]')"
  if [ "$actual" != "$expected" ]; then
    echo "Refusing unstable or unexpected designated requirement: $path" >&2
    printf 'Expected: %s\nActual: %s\n' "$expected" "$actual" >&2
    return 1
  fi
}

cu_verify_bundle() {
  [ "$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$1/Contents/Info.plist")" = dev.ankojh.cu ] || return 1
  /usr/bin/codesign --verify --deep --strict "$1" || return 1
  cu_verify_signed_code "$1/Contents/Resources/cu-engine" dev.ankojh.cu.engine || return 1
  cu_verify_signed_code "$1" dev.ankojh.cu
}

cu_sign_code() {
  /usr/bin/codesign --force --sign "$CU_SIGNING_IDENTITY" --timestamp=none \
    --identifier "$2" --requirements "=designated => $(cu_requirement "$2")" "$1"
  cu_verify_signed_code "$1" "$2"
}
