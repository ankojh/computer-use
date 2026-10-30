# Native Codex readiness stream fix — 2026-10-29

## Confirmed bug

Version 8's `AppModel.checkReadiness()` appended **both stdout and stderr** into
one buffer and decoded it as a single JSON document. The installed engine actually
returned exit 0 and `{"ready":true,...}` on stdout, while Codex emitted an unrelated
`Ignoring unknown features requirement ultrafast_mode` diagnostic on stderr (tagged
ERROR by its logger). The combined bytes were not JSON. Cu then incorrectly showed:

```text
Live Codex connection check failed or timed out. Check connectivity and run
codex login in Terminal to sign in again with your ChatGPT subscription.
```

The failure was reproduced with the installed engine: stdout alone decoded and
reported ready; merged stderr/stdout did not. This was a native status-parser bug,
not evidence of failed login or timeout. No login/configuration change was needed.

## Fix installed in version 9

- Shared native `CodexReadinessProbe` launches the engine directly and separates
  stdout JSON from stderr diagnostics. The same transport is exercised in tests.
- Stdout is bounded to 16 KiB and truncation fails closed. Diagnostic stderr keeps
  a bounded 8 KiB tail; warnings cannot corrupt or consume the JSON buffer.
- Ready requires both valid structured `ready=true` and process exit 0. A nonzero
  exit cannot report ready just because the last JSON claimed success.
- Actual structured authentication failures preserve their exact reason. Invalid
  protocol data and timeout have distinct messages and do not prescribe login
  without evidence. Timeout remains bounded with process-group cancellation.
- OSLog category `CodexReadiness` records ready/exit/timeout/byte counts, not raw
  account credentials or output. No global Codex logging/features were disabled.

Sources:
`tools/CuApp/Sources/CuApp/{CodexReadinessProbe.swift,AppModel.swift}`.
Regression suite:
`tools/CuApp/Tests/CuAppTests/CodexReadinessProbeTests.swift`.

## Validation

- **52 native tests passed**, including 12 new readiness tests and an opt-in live
  metadata-only test through the native transport. Covers stderr in both arrival
  orders, split UTF-8, structured 401 preservation, malformed/oversized output,
  nonzero exits, actual pipes, launch failure, and actual deadline cancellation.
- **70 Rust tests, 7 Swift backend tests**, strict Clippy, web integration, and
  persistent-signing regressions passed. No inference or physical-input test ran.
- Installed the exact tested version-9 artifact from
  `target/Cu-status-staging/Cu.prepared` without a signing-identity migration.
  CDHash: `937744222f351edbcd0c903d2843ff6fd3d73efe`.
- Same persistent certificate and designated requirement as versions 7/8:
  `42049CB9AE1C7428797C11D3F9F678BA831700B8`. No TCC reset/request or reapproval.
- **Actual GUI verification**, not merely a terminal engine check:
  ```text
  13:49:45 Cu[61898] macOS permissions: screen=true accessibility=true
  13:49:47 Cu[61898] Live Codex check: ready=true exit=0 timedOut=false stdoutBytes=98 stderrBytes=326
  ```
- Version 8 backup:
  `target/app-backups/20261029T134945.FNBH91/Cu-system.disabled`.
- Own-view smoke test initially had the transient popover dismissed; immediate
  repeat showed popover/transient/menu/engine/hotkey ready and rendered only Cu's
  view. No task, desktop capture, input, email, login, or permission change occurred.

Logs: `target/codex-status-{native-live-full,live-tests,rust-tests,backend-tests,
web-tests,signing-tests,build,install}.log` (some names include the full `tests` suffix).

```sh
# Default native suite skips the live check:
swift test --package-path tools/CuApp
# Opt-in live subscription metadata only; no model turn or desktop access:
CU_LIVE_CODEX_STATUS_TEST=1 swift test --package-path tools/CuApp
log show --last 10m --predicate 'subsystem == "dev.ankojh.cu" AND category == "CodexReadiness"'
```

This verifies readiness/parsing, not successful completion of a real computer-use
workflow. Genuine future network, authentication, or workspace errors can still occur.
