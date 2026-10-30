# Completion verification and profile recovery

Implemented 2026-10-29 after inspecting run `20261029-003331-55d09610`.
That run stopped at an unverified Chrome profile, did no research or email
composition, but was incorrectly recorded as `done` because actions were empty.

## New Codex outcome contract

| Planner status | Actions | Engine behavior |
|---|---|---|
| `continue` | Required, at most eight | Existing policy/execution/observation loop |
| `complete` | Empty; evidence observation IDs required | Independent verification, never immediate success |
| `needs_input` | Empty; explanation required | Review whether the block is real or recoverable; never immediate success |

Malformed combinations, missing status, unsupported actions, and invented observation
IDs fail closed. Mock scripts keep their legacy default for existing tests, but a
run that enters verification cannot escape it through a later legacy empty turn.

## Reasoned verification

- A fresh ephemeral, tool-disabled Codex thread uses the current planner model with
  **at least medium reasoning** (low/medium/high cap). With the current configuration
  this is **Luna-medium**, not Astra; no per-turn classifier is re-enabled.
- It receives the original task, the terminal claim labelled as a claim, actual
  action outcomes/AX target labels/approvals, a fresh screenshot, and up to six
  host-recorded screenshots cited by the planner (recent observations for a blocked
  claim without citations). All content remains untrusted data, not instructions.
- Every check has a requirement, verdict, concise evidence, and valid observation
  IDs. Version 11 additionally permits `advisor_ids` for host-recorded successful Parallel
  research reports containing actual URL/excerpt pairs. These supplement, never replace,
  the screenshot-reference requirement. A current screenshot may supply run/desktop context,
  but public facts must be grounded in the source excerpts, not attributed to that image.
  Fabricated IDs, Jev judgments and unavailable/excerpt-free reports are not public-source
  evidence. Helpers cannot establish profile/account/recipient/draft/Send outcomes; normal
  desktop/receipt evidence remains necessary. See [HYBRID.md](HYBRID.md).
  No private reasoning trace is requested or recorded.
- `verified` requires nonempty, supported requirement checks. Contradictory or
  unsupported verified checks are downgraded. A blocked claim cannot directly
  become success. Review/schema/time-out errors cannot report completion.
- `incomplete` allows **one bounded recovery/replan** for ordinary missing work.
  A second failure becomes `needs_input`, not a loop. No recovery is allowed after
  any denied action or any approved consequential action, even if execution later
  failed: an uncertain Send must not be repeated automatically.
- `needs_input` ends with the real question/reason and a non-success exit code.
  Native controls show **Needs your input**, retain the reason in the overlay,
  and expose the latest brief requirement/evidence checks.
- The review has a 45-second deadline. The run's remaining wall-time budget and
  cancellation also bound verification/recovery. Token usage includes review calls;
  model latency statistics now include these separately logged model calls too.

For research/email, the verifier checks source-page evidence, requested Chrome
profile, exact resolved recipient/no additional recipients, relevant draft content
and sender, and matching post-send receipt or Sent-message evidence. A draft, a
Send click, or a planner assertion is not proof that the send completed.

This is an evidence-based model review, **not a formal identity or delivery
guarantee**. Live profile switching, research quality, and email UI receipts still
need real workflow validation. No real research/email task was retried during this
implementation.

## Navigation and identity changes

- In confirm-risky mode, exact single `cmd+tab`, `cmd+shift+tab`, and `cmd+space`
  shortcuts no longer ask merely because Settings/a sensitive app is focused.
  Hard denials still win, and confirm-all still asks as explicitly selected.
- Send/delete/submit and other consequential operations still require approval.
  Common `cmd+return` / `ctrl+return` send/submit shortcuts are also risky; activating
  a consequential focused control with Return/Space is not treated as harmless.
- Planner instructions prefer Spotlight over repeated Cmd–Tab and require
  inspecting available Chrome profiles before asking the user to switch manually.
- Chrome profile and website sender are distinct: requesting the CMU Chrome
  profile does **not** automatically require a CMU-domain Gmail sender. An explicitly
  requested sender must still be verified independently.

## Original version-5 validation

- 64 Rust tests, 31 native UI/state tests, 7 Swift backend tests; strict Clippy and
  optional web bridge integration passed. `target/completion-validation.log`
  contains the combined validation output.
- Deterministic protocol tests confirm a fresh tool-free verifier context, medium
  reasoning when planning is low, no classifier calls in fixed mode, labelled
  historical/current images, and rejection of executable actions in reviews.
- Regression tests cover false empty-action success, real `needs_input`, fresh
  screenshots, fabricated evidence IDs, bounded recovery, no legacy fallback,
  approved/failed/denied Send replay prevention, and navigation versus Send approvals.
- A three-call live test used only a generated 1px image and Luna-medium. It accepted
  a supported read-only dimension answer (review **2922 ms**) and rejected an
  unsupported research/email completion claim (review **4187 ms**, `incomplete`).
  This validates the protocol/gate, not real desktop task quality.

```sh
# Opt-in subscription test; never captures the desktop or posts input:
cargo test -p cu-providers codex_live_completion_review_without_input -- --ignored --nocapture
```

## Deployment status (updated after persistent signing)

Current installed version **14** retains version 13's private task-relevant native-app metadata to the planner and
fresh verifier. Unless a browser is explicitly required, a web login wall with an untried native
route is incomplete work, not a genuine blocker. The existing one-shot completion recovery can
inspect that route, still never after a denied or approved consequential action. A separate single
repair of an entirely rejected, unexecuted plan preserves validation usage/latency and the same
no-replay barrier; it cannot bypass completion evidence. See [APP_DISCOVERY_RECOVERY.md](APP_DISCOVERY_RECOVERY.md).
Version 14 additionally offers opt-in [Allow all actions](ALLOW_ALL.md). Known risky automatic
input is marked before execution, including uncertain/failed posts; both completion and rejected-plan
recovery honor that no-replay barrier even without a human approval. The selected mode does not
waive identity, screenshot, receipt or completion requirements.
The older deployment records below are retained as history.

- Version **5** was prepared but not installed; its original artifact remains at
  `target/Cu-completion-staging/Cu.prepared`, hash `c143494ee949ac33755fd51b9465bab7de7b409a`.
- Version **6**, including these completion checks plus the attached menu-bar panel,
  live authentication check, progress counters, and approval focus revalidation,
  was installed with explicit operator approval, then superseded by versions **9/10**
  with the same functionality and persistent local signing.
- The exact tested version-6 artifact is `target/Cu-menu-staging/Cu.prepared`;
  its historical CDHash is `e1257529c64c7efeb1f1fd3a3790a1f5b82ff826`.
  Version 4 is backed up at `target/app-backups/20261029T105336.KorhaV/Cu-system.disabled`.
- **Setup blocker resolved:** replaced ad-hoc signing with a persistent certificate,
  performed one explicitly authorized Cu-only permission migration, and verified that
  a real same-certificate version 7 → 8 update retained both grants without another
  reset/request/reapproval. Version-8 test artifact: `target/Cu-stable-update-staging/Cu.prepared`,
  then-installed hash `a9c039dc83e0ca3b34e0481848b47ec761703ec0`. Native app logged both
  grants true at **13:31:23**. Live Codex account/catalog check succeeds.
  See [signing and live update proof](SIGNING.md).
- Version 9 additionally fixes native Codex status parsing: warnings on stderr no
  longer invalidate stdout JSON or falsely suggest a login failure. GUI readiness
  and both grants are verified true, with the same certificate and no reset/reapproval.
  Historical version-9 hash: `937744222f351edbcd0c903d2843ff6fd3d73efe`.
  See [CODEX_READINESS.md](CODEX_READINESS.md).
- Historical **version 10** introduced native mouse-event pulses and optional exact AX typing-focus
  guards. Hash `9a61c60bcb303e48aafcbb230a08e4f0748c99e7`; both GUI grants and Codex ready at
  **14:42:10–11**, without reapproval. 76 Rust, 56 native (including live metadata), 9 backend,
  strict Clippy, Node/web and signing tests pass. Only inert test-window pointer input was exercised;
  no research/email task replay or Send. See [performance/cursor validation](PERFORMANCE_CURSOR.md)
  and [menu-bar revision details](MENU_BAR.md).
