# Persistent local signing and permission retention

Implemented and live-verified 2026-10-29. **Both macOS permissions survived the real
version 7 → 8 update without reset/reapproval. Version 9 is now installed and likewise
retained both grants**, including functional UI changes; see [CODEX_READINESS.md](CODEX_READINESS.md).

## Root cause

The old build script forced `codesign --sign -` (ad-hoc signing). Cu's designated
requirement was literally its changing CodeDirectory fingerprint:

```text
designated => cdhash H"e1257529c64c7efeb1f1fd3a3790a1f5b82ff826"
```

macOS TCC stored that requirement with the approval. A new build did not satisfy
it even though the path and bundle ID were unchanged. Logs explicitly showed
`Failed to match existing code requirement` for Accessibility and ScreenCapture.
Repeated toggling/signing/rebuilding was not a durable fix.

## New signing contract

- One long-lived **Cu Local Code Signing (ankojh)** certificate/private-key pair
  lives in the user's login Keychain. RSA-3072 / SHA-256, code-signing EKU,
  non-CA, approximately ten-year validity. User trust was approved for **Code
  Signing only**, not TLS/general trust; this was checked in the user trust record.
- Public SHA-1 fingerprint (the Keychain/codesign identity selector):
  `42049CB9AE1C7428797C11D3F9F678BA831700B8`.
- Selected fingerprint is saved mode 0600 in
  `~/.config/cu/signing-identity`. Private keys/passwords are not in the repository
  or that file. Key access is restricted to the system `codesign` tool, not `-A`.
- Both app and bundled engine are signed with explicit certificate-backed
  designated requirements. App requirement:

  ```text
  identifier "dev.ankojh.cu" and certificate leaf = H"42049cb9ae1c7428797c11d3f9f678ba831700b8"
  ```

  Engine identifier is `dev.ankojh.cu.engine`, with the same pinned certificate.
- CodeDirectory hashes can change; these requirements do not. A different signer
  cannot impersonate Cu just by reusing its bundle ID. Identifier-only requirements
  and ad-hoc artifacts are rejected.
- Missing, malformed, expired, inaccessible, or untrusted signing identity fails
  the build. **Never silently fall back to ad-hoc, regenerate a certificate per
  build, weaken the designated requirement, or reset TCC on routine updates.**

## Setup and routine builds

```sh
# One-time explicit setup on a NEW machine without a Cu identity:
tools/CuApp/setup-signing.sh --create-local

# Or select an existing valid Apple/local signing identity:
tools/CuApp/setup-signing.sh --use <40-hex-certificate-fingerprint>

# After setup, use the SAME identity for every build:
tools/CuApp/build-app.sh "$PWD/target/Cu-next/Cu.prepared"

# Quit idle Cu, then install the exact tested artifact; no rebuild/re-sign:
tools/CuApp/install-app.sh --prepared "$PWD/target/Cu-next/Cu.prepared"
open /Applications/Cu.app
```

`CU_SIGNING_IDENTITY` may explicitly select a fingerprint and
`CU_SIGNING_IDENTITY_FILE` may select a different configuration path. Do not use
these to rotate identities casually: installer guards still require the configured
certificate-backed requirement and compatibility with the current installed app.

The installer verifies both executables, exact designated requirements, copied
artifact hash, and compatibility with the prior requirement. It preserves the
previous app as `.disabled`, registers only the final installed path, and can
roll back on installation failure. Default builds use a non-`.app` staging suffix.
**It never resets TCC or requests/grants permissions.**

Actual identity migration (including the old ad-hoc build) requires explicit
`--allow-identity-change`, separate owner approval for any Cu-only permission reset,
and manual macOS consent. Do not put this flag on routine update commands.

### Keychain recovery

`--create-local` reuses an existing configured identity; it never replaces one.
A same-named partial/expired identity blocks generation, rather than producing an
accidental replacement key. If setup times out, inspect and finish trust/key-access
approval for that identity, then use `--use` to configure its fingerprint. In
Keychain Access, set **Code Signing → Always Trust**, leaving other trust settings
at their defaults. Enter passwords only in macOS dialogs, never in chat/scripts.

Raw PEM and PKCS#12 import compatibility issues were encountered during setup.
The final script uses macOS-compatible PKCS#12 transport, a random temporary
transport password (not the user's login password), mode-0600 files in a mode-0700
temporary directory, and bounded approval commands. Key material is removed after
import and on normal failure/exit. No temporary private material remains from this
migration. A hard-killed setup may need explicit cleanup/recovery, not key rotation.

Keep the Keychain identity or a securely exported backup. If it is actually lost,
expired, deleted, or replaced, an intentional identity migration and new approvals
may be necessary. Do not solve a locked Keychain by changing Cu's identity.

## Validation

```sh
tools/CuApp/test-signing.sh
# Missing/invalid identity checks only, without accessing a real signing key:
tools/CuApp/test-signing.sh --negative-only
```

Regression tests sign never-launched disposable app/engine fixtures. Both contents
and hashes change, while their requirements remain equal and the updated code
satisfies each prior requirement. Tests also reject ad-hoc and identifier-only
bundles. Default installer refusal of the version-6 → 7 identity change was
confirmed before the explicitly authorized migration.

70 Rust + 40 native + 7 backend tests, strict Clippy, optional web integration,
strict signatures, and own-view UI smoke checks passed. Logs are in
`target/stable-{rust,native,backend,web,signing}-tests.log`.

### Real TCC persistence test

1. With explicit owner approval, installed certificate-signed version **7** from
   `target/Cu-stable-staging/Cu.prepared`, hash
   `eb5bb0719dbef53f5d1e9b4aa838369f5d2e8598`.
2. Reset **only** Accessibility and ScreenCapture for `dev.ankojh.cu`, once.
   The owner manually approved both. Native Cu reported both true at **13:29:18**.
3. With separate owner approval, copied the signed bundle, changed only its bundle
   version to **8**, and re-signed its app seal with the SAME certificate. No source
   functionality or engine bytes changed. Hash became
   `a9c039dc83e0ca3b34e0481848b47ec761703ec0`; requirement stayed identical.
4. Installed version 8 using the ordinary installer, **without** a migration flag,
   TCC reset, permission request, or reapproval. Native Cu reported
   **`screen=true accessibility=true` at 13:31:23**. A fresh LaunchServices-launched
   helper independently returned both true. Live Codex account/catalog check passed.

Version-8 retention-test artifact: `target/Cu-stable-update-staging/Cu.prepared`.
Historical version-9 artifact: `target/Cu-status-staging/Cu.prepared`, hash
`937744222f351edbcd0c903d2843ff6fd3d73efe`; both grants survived without reset/reapproval.
Current version-10 artifact: `/Applications/Cu.app` / `target/Cu-speed-staging/Cu.prepared`,
hash `9a61c60bcb303e48aafcbb230a08e4f0748c99e7`. Same certificate/requirements; actual GUI
logged `screen=true accessibility=true` at **14:42:10**, Codex ready at **14:42:11**.
No migration flag, TCC reset/request, or reapproval. See [pointer/performance validation](PERFORMANCE_CURSOR.md).
Version 6 backup: `target/app-backups/20261029T132737.BksygT/Cu-system.disabled`.
Version 7 backup: `target/app-backups/20261029T133123.RRNiii/Cu-system.disabled`.

Use Cu's own setup checks/logs or a LaunchServices-launched helper for permission
verification. Launching its helper directly from a terminal can attribute Screen
Recording to Ghostty/Terminal instead; that is **not** proof of Cu's grant.

This fixes routine local build identity churn. It is not Developer ID notarization
or distribution signing, and cannot guarantee that macOS policy/OS changes, another
machine/user, changed identity, or certificate expiry will never require consent.
No actual computer-use task, research, or email was run during this validation.
