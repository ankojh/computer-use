# Critical-only approvals and actionable macOS notifications

## Behavior (introduced in version 12, retained in version 14)

Current deployment: [version 14](ALLOW_ALL.md), with opt-in **Allow all actions**. That selection skips Cu action/provider-check approvals; Critical actions only remains the default. Stop, human pause, hard denials and macOS permissions remain. At 18:54:43 on 2026-10-29 the actual GUI explicitly logged notifications **not authorized**. Enable Cu in macOS Settings → Notifications for Approve/Deny banners. Accessibility, Screen Recording and Codex readiness are verified true independently; missing notification authorization does not block the planner or panel approval. Version 13 also approval-gates native WhatsApp bare Return/keypad Enter and typed newlines where they may submit a message; ordinary typing and known search navigation remain routine.

The native default is **Critical actions only** (`confirm-risky`), not confirm-all. Routine GUI navigation and ordinary typing run without a confirmation. Explicit Send/submit, payment, publishing/sharing, deletion, credential/secret entry, account/permission changes and other consequential or ambiguous high-risk operations remain guarded. Unknown terminal command execution remains approval-gated; this change does not make arbitrary shell commands safe.

On this Mac, `~/.config/cu/config.toml` explicitly selects `safety.mode = "confirm-risky"` and `cmd_w_is_risky = false`: ordinary window/tab closing is automatic. Other protective defaults remain unchanged. Confirm-every-action is still available if explicitly selected. Since version 14, **Allow all actions** is an explicit opt-in in Control; it is not the default. Approval descriptions below apply to confirmation modes, not automatically allowed actions.

Two conservative false-positive fixes preserve the existing hard denials:

- Pointer motion and scrolling are routine even when a sensitive app is focused; they do not activate controls. Confirm-all still confirms them.
- Clicking to focus a known editable field does not trigger Send merely because its help explains a send shortcut. Explicit consequential titles/descriptions/identifiers still trigger approval, even on an editable role. Pressing Return/Space on a consequential focused control, send shortcuts, secure/secret typing and real consequential buttons retain their checks.

The label/risk rules are best-effort, not a formal guarantee that every consequential action is recognized. Cu-control input and Privacy & Security toggle hard denials remain; human interference still pauses the task.

## No menu-bar hunt for each approval

A pending request posts one native macOS notification:

- **Approve / Deny** for an action or safety review.
- **Resume / Stop task** for a human-interference pause.
- **Clicking the notification body only opens the attached Cu panel**, scrolled to that exact pending request. It never approves an action.
- Explicit response buttons do not require opening Cu first. They use the same existing response path, focus restoration and engine-side target/display/AX revalidation as panel approval.
- Buttons require authentication when macOS is locked. Dismissing a banner or doing nothing does not approve, deny, or resume.

macOS may show action buttons only after hovering/expanding the notification. Notification style, Focus/Do Not Disturb, preview preferences, delivery and sound remain controlled by macOS. Cu does not request the special critical-alert entitlement or override Focus. The menu-bar indicator remains a fallback.

## One-time system permission

macOS notification authorization is needed once; it cannot be bypassed. The first normal version-12 launch requests it if undetermined and not previously requested. It is not requested for each command, each approval, normal subsequent launches, smoke renders or the inert pointer diagnostic.

If denied, Cu does not repeatedly ask or disable ordinary engine readiness. The panel provides **Enable notifications** and **Notification settings**; macOS may require manually enabling Cu after denial. Until allowed, critical actions still wait safely and can be reviewed in Cu. Existing Accessibility and Screen Recording grants are reused; no TCC reset, grant migration or new account/login is performed.

## Scope, privacy and focus

Notification identities bind the app-launch nonce, run UUID and full decimal prompt ID. Old launches/runs, reused prompt numbers in another run, malformed/forged identities, unknown action identifiers and duplicate responses cannot control a later/current task. Notices are withdrawn on answer, replacement, Stop and completion, with cleanup for late async delivery and old-launch notices.

No raw task, recipients, email, typed text, secret preview, AX label or screenshot is stored in notification content. Only a fixed category summary and opaque IDs are sent to Notification Center. Click the body to inspect exact details in the attached panel before deciding when needed. Approving directly is still an explicit choice about the pending action, not a blanket approval of future commands.

Fallback target focus is captured when the prompt arrives, before Notification Center can activate; it is never dynamically replaced with the app that happens to be frontmost at notification-click time. Notification Center/Control Center activations are excluded from previous-app tracking. Missing/failed focus restoration stops rather than posting an approved input. The engine independently checks the original target afterward. This is not an atomic page-content or delivery guarantee.

## Version-12 validation and deployment history

The following PID/hash/initial-request findings are historical; see [STATUS.md](STATUS.md) for version 13 and the current explicit not-authorized notification status.

- 98 Rust tests passed, including routine navigation/field-focus behavior, explicit consequential labels, send shortcuts, deletion guards and unchanged hard denials; strict all-target Clippy passed.
- 67 native tests passed, one opt-in live Codex metadata test skipped. New tests cover stale launch/run/prompt identity, stop/completion/answer cleanup, max UInt64 IDs, single-use responses, body click versus approval, dismiss/unknown actions, private-content exclusion, default mode and authentication-required action options.
- Nine Swift backend tests, three Node tests and persistent-signing regressions passed.
- Exact signed prepared artifact: `target/Cu-approval-staging/Cu.prepared`, installed as version 12 at `/Applications/Cu.app` with the same persistent certificate.
- Actual installed GUI PID 36128 logged Screen/Accessibility true at 17:54:56 and Codex ready at 17:54:57 (2026-10-29). The one-time notification authorization request was logged at 17:54:56; a user grant/delivery has not yet been verified. App CDHash `db8ce93c710350703efc673ec151fe5105029d14`, engine `5772d3431fb0c537818e34f77606608222fc6241`; strict signatures and exact prepared/installed binaries verified.
- Actual notification delivery/appearance depends on the user granting macOS authorization and OS notification settings. Routing/action options are tested locally; no live consequential action, desktop task/email replay, Send, pointer input or additional Jev/Parallel inference was performed for this change.

Logs: `target/approval-{rust-tests,clippy,native-tests,backend-tests,node-tests,signing-tests,build,install}.log`. GUI metadata: `target/approval-gui-readiness.log`. GUI permission/readiness/notification-authorization metadata is inspected separately; do not equate permission grant or an accepted notification request with a successfully executed desktop action.
