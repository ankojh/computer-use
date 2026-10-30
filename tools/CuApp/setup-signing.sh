#!/bin/bash
# Explicit one-time local identity setup. Never called implicitly by a build.
set -euo pipefail
umask 077
HERE="$(cd "$(dirname "$0")" && pwd)"
source "$HERE/signing.sh"
FILE="${CU_SIGNING_IDENTITY_FILE:-$HOME/.config/cu/signing-identity}"
NAME="Cu Local Code Signing ($(id -un))"
KEYCHAIN="$(/usr/bin/security default-keychain -d user | /usr/bin/awk -F'"' '{print $2}')"
[ -n "$KEYCHAIN" ] || { echo 'No user default Keychain found.' >&2; exit 1; }

save_identity() {
  cu_load_signing_identity
  mkdir -p "$(dirname "$FILE")"
  local temporary
  temporary="$(mktemp "$(dirname "$FILE")/.cu-signing.XXXXXX")"
  printf '%s\n' "$CU_SIGNING_IDENTITY" > "$temporary"
  chmod 600 "$temporary"
  mv "$temporary" "$FILE"
  printf 'Cu signing identity: %s\nConfiguration: %s\n' "$CU_SIGNING_IDENTITY" "$FILE"
}

if [ -f "$FILE" ]; then
  # Even a missing/expired identity must not trigger accidental key rotation.
  if [ "$#" = 1 ] && [ "$1" = --create-local ]; then
    cu_load_signing_identity
    echo 'Reusing configured identity; no new key/certificate created.'
    exit 0
  fi
  echo "Signing is already configured at $FILE. Refusing to replace the identity implicitly." >&2
  exit 1
fi
if [ "$#" = 2 ] && [ "$1" = --use ]; then
  CU_SIGNING_IDENTITY="$2"
  save_identity
  exit 0
fi
if [ "$#" != 1 ] || [ "$1" != --create-local ]; then
  echo 'Usage: setup-signing.sh --create-local | --use <certificate-sha1-fingerprint>' >&2
  exit 1
fi
mkdir -p "$(dirname "$FILE")"
LOCK="$(dirname "$FILE")/.signing-setup.lock"
mkdir "$LOCK" 2>/dev/null || { echo "Signing setup is already active (or left a lock at $LOCK); inspect it before retrying." >&2; exit 1; }
WORK=""
cleanup() {
  if [ -n "$WORK" ]; then
    rm -f "$WORK/private.pem" "$WORK/certificate.pem" "$WORK/identity.p12" "$WORK/openssl.cnf"
    rmdir "$WORK" 2>/dev/null || true
  fi
  rmdir "$LOCK" 2>/dev/null || true
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
if /usr/bin/security find-certificate -c "$NAME" "$KEYCHAIN" >/dev/null 2>&1; then
  echo "A certificate named '$NAME' already exists. Restore/trust it and configure its fingerprint with --use; do not generate a replacement." >&2
  exit 1
fi
WORK="$(mktemp -d "${TMPDIR:-/tmp}/cu-signing.XXXXXX")"
security_approval() {
  # Leave time for real user approval, but ensure this script can clean up its
  # temporary key material instead of relying on the caller's hard timeout.
  python3 - "$@" <<'PY'
import subprocess, sys
try:
    result = subprocess.run(['/usr/bin/security', *sys.argv[1:]], timeout=90)
    sys.exit(result.returncode)
except subprocess.TimeoutExpired:
    print('Keychain approval timed out. An imported identity may remain; inspect/trust it instead of generating another.', file=sys.stderr)
    sys.exit(1)
PY
}
printf 'Creating %s in the user Keychain. macOS may request approval.\n' "$NAME"
printf '%s\n' '[req]' 'prompt = no' 'distinguished_name = subject' 'x509_extensions = code_signing' \
  '[subject]' "CN = $NAME" '[code_signing]' 'basicConstraints = critical, CA:FALSE' \
  'keyUsage = critical, digitalSignature' 'extendedKeyUsage = critical, codeSigning' \
  'subjectKeyIdentifier = hash' > "$WORK/openssl.cnf"
/usr/bin/openssl req -x509 -newkey rsa:3072 -sha256 -nodes -days 3650 \
  -config "$WORK/openssl.cnf" -keyout "$WORK/private.pem" -out "$WORK/certificate.pem"
# Both temporary containers are mode 0600 inside a mode-0700 directory. PKCS#12
# is used because macOS does not reliably import raw OpenSSL private PEMs. Its
# randomly generated transport password is NOT the user/Keychain password and
# is never logged or saved. Explicit legacy transport algorithms avoid macOS
# PKCS#12 compatibility issues; the signing certificate itself uses RSA/SHA-256.
# No private key is written into the repository or config.
export CU_PKCS12_PASSWORD="$(/usr/bin/openssl rand -hex 32)"
/usr/bin/openssl pkcs12 -export -inkey "$WORK/private.pem" -in "$WORK/certificate.pem" \
  -name "$NAME" -keypbe PBE-SHA1-3DES -certpbe PBE-SHA1-3DES -macalg sha1 \
  -passout env:CU_PKCS12_PASSWORD -out "$WORK/identity.p12"
security_approval import "$WORK/identity.p12" -k "$KEYCHAIN" -f pkcs12 -P "$CU_PKCS12_PASSWORD" -T /usr/bin/codesign
unset CU_PKCS12_PASSWORD
rm -f "$WORK/private.pem" "$WORK/identity.p12"
# User domain, code-signing policy only. No system/TLS/general root trust and no -A key ACL.
security_approval add-trusted-cert -r trustRoot -p codeSign -k "$KEYCHAIN" "$WORK/certificate.pem"
CU_SIGNING_IDENTITY="$(/usr/bin/openssl x509 -in "$WORK/certificate.pem" -noout -fingerprint -sha1 | cut -d= -f2 | tr -d ':')"
save_identity
printf 'Private key stays in Keychain; temporary key material removed on exit.\n'
