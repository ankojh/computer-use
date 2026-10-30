# Menu-bar panel and authentication diagnosis — 2026-10-29

> Historical version-6 report. The interface remains in version **9**; the permission
> blocker below was resolved with persistent signing and a real update-retention test.
> Both grants are now ready. See [SIGNING.md](SIGNING.md).

## Why the latest tasks stopped

Both runs `20261029-102612-5b4fb409` and `20261029-102637-9a98a8fc`
failed at `Codex account/read` with:

```text
workspace routing discovery unauthorized (401)
```

They executed **zero planning steps and zero actions**, asked zero confirmations,
and ended in approximately 1.8 / 1.5 seconds. This was an authentication/workspace
routing error, not refusal to perform the task, completion verification, or a
macOS permission error. The installed version was still 4; version 5 had only
been prepared. No research/email work was retried during the diagnosis.

`codex login status` reported a cached ChatGPT sign-in despite the live rejection.
An initial live account check reproduced the 401; a refresh returned no usable
account. Subsequent fresh-process live checks succeeded, including checks with
the prepared and installed engine. Recovery is observed, but it is not proof
that future workspace authentication can never expire or fail.

## Implemented and installed in version 6

- One native `NSPopover`, transient and non-detachable, anchored below the menu-bar
  button. No separate task-controls window or draggable floating task panel.
  Opening requires a click/reopen (or explicit `--show-menu`); approvals do not
  automatically open/activate a window over the task.
- Task entry, setup, approvals, completion evidence, activity, run logs, and Quit
  live in the same panel. Starting dismisses it and restores the previous app.
- The menu-bar icon/`!` indicates a pending approval. Stop is always in the panel
  header; global Control–Option–Escape remains registered. Closing does not stop.
- New progress strip: current stage (planning, executing, verifying, recovering),
  current planning step, and successfully posted input-action count. Host-generated
  `task_progress` events drive it; model text cannot alter the counters or approve
  an action. Final summary counts override an attempted step on failure. No guessed
  percentage, total-task estimate, or synthetic milestone completion is shown.
- `cu codex-status` performs a live account/catalog check and validates fixed model
  selection. It starts no model thread, makes no inference, captures no screen,
  posts no input, and skips the Mac keyboard-layout initializer. Total connection
  check is bounded to 20 seconds; native subprocess handling is bounded too.
- An explicit RPC authentication rejection gets **one** account refresh. No retry
  on arbitrary protocol/tool errors, no API-key fallback, no account switching,
  no automatic browser login, and no resetting/deleting local credentials.
- Runtime authentication errors show **Sign-in needed**, not generic Needs attention.
  The exact cause and sign-in/workspace guidance remain visible after summary counts.
  Preflight failure disables Start rather than repeatedly failing real tasks.
- Popovers can take keyboard focus, unlike the removed nonactivating overlay.
  Before replying to a prompt, native Cu closes the panel, restores the host-recorded
  focused PID (previous app for pause/safety prompts), and observes that exact focus
  within one monotonic second. A missing app/failed restore stops without approving.
  The engine independently denies the action if focus/AX target/display identity
  changed during approval. Agent keyboard input into Cu itself is hard-denied.
- Version-5 completion verification/profile instructions are now active in the
  installed binary. Send/consequential approvals remain required. The cursor marker
  is still optional, click-through, and nonactivating; it is not a controls panel.

## Evidence and deployment

- **70 Rust tests:** 21 core, 20 provider/protocol, 29 agent-loop; physical-input and
  subscription inference tests remain opt-in. Logs: `target/menu-rust-tests.log`.
- **40 native tests:** state/protocol, transient panel contract, monotonic focus wait,
  stale-run safety, persistent auth error cause, progress/approval separation.
  Log: `target/menu-swift-tests.log`.
- **7 Swift backend tests**, optional web integration, and strict Clippy pass.
  Logs: `target/menu-backend-tests.log`, `target/menu-web-tests.log`.
- Prepared and installed own-view smoke checks show `popover=true`, `transient=true`,
  menu action present, engine present, and stop hotkey registered. Only Cu's own
  view was rendered, never the desktop; no task/capture/input was started.
- Exact tested artifact copied without rebuilding/re-signing:
  `target/Cu-menu-staging/Cu.prepared` → `/Applications/Cu.app`.
  CDHash: **`e1257529c64c7efeb1f1fd3a3790a1f5b82ff826`**, strict verification passed.
- Prior version 4 preserved at
  `target/app-backups/20261029T105336.KorhaV/Cu-system.disabled`.
- The installed live Codex check reported `ready=true` after replacement.
- **Blocker at version-6 installation (subsequently resolved):** the new ad-hoc signature reported `screen=false
  accessibility=false` at **10:53:38**. The operator approved installation after
  being warned this could require reapproval. **No permission reset/request was
  performed.** The panel exposes setup controls, but tasks cannot start until the
  user restores the required grants. Do not describe the full workflow as verified.

## Limits

No live profile-switching/research/email task was attempted. Own-view/unit/protocol
checks do not establish full real-desktop success. The final review is evidence-based
model reasoning, not a formal guarantee. Focus/AX identity checks reduce approval
transfer risk but do not provide an atomic screenshot/content hash guarantee while
another app updates asynchronously. Native stop is not an engine watchdog. The
optional web frontend has no native focus-restoration handshake; an altered target
will now be denied by the engine rather than executed under an old approval.
