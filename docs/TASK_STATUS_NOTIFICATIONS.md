# Task outcome notifications, menu-bar progress and input clearing (version 16)

## Diagnosis

The two recent runs `20261030-142852-13b9e7d3` and `20261030-143006-ce2c1424`
finished with backend status `done`, independent verification `verified`, and zero
confirmation prompts. The old native controller only emitted approval/pause notifications;
it never emitted task-completion notifications. Actual GUI logs also showed macOS notification
authorization unavailable. These are separate causes, neither evidence those tasks failed.

## Menu-bar status

While working, Cu's menu-bar item now displays the current planning/research/verification
stage/step and the number of posted input actions. Hover for the full stage/counters; open
the attached panel for details. No guessed percentage, predicted total or action checklist:
remaining work is **Unknown — steps adapt as Cu works** until success is verified.

After verified success and successful engine process exit the menu bar persistently shows
**Cu ✓ Done**, the panel shows the completed planning-step/input counts and **Remaining: 0**.
It stays visible until a new task starts. Needs input, Error, Stopped and Unverified are distinct,
never presented as success or zero remaining work. A nonzero process exit cannot become success
even if a `done` summary was received. The live process must exit before terminal UI/alerts.

## Native outcome notifications

The existing single UNUserNotificationCenter delegate now handles a separate `CU_COMPLETION`
category, with no Approve/Deny actions. A completion notice contains only a fixed outcome,
host planning-step/input counts and opaque launch/run IDs—never task text, message contents,
recipients, result summaries, error bodies, AX labels or screenshots.

- Verified complete: **Cu — task finished ✓**. Other endings use distinct needs-input/error/
  stopped/unverified titles and never say the task succeeded.
- A default notification click only opens that exact current terminal run's results in the
  attached panel. It cannot approve, resume, send, or start a task.
- One notice per current terminal state; duplicate updates do not resend. A new task, stale
  launch/run, forged identity, unknown action or duplicate response cannot control a current task.
  Late submissions are cleaned up. Approval routing remains separately scoped and unchanged.
- Normal sound/banner/list presentation, including while Cu is foreground. No critical-alert
  entitlement, Focus bypass or guarantee of macOS banner delivery. Acceptance by macOS is logged
  as acceptance, not falsely claimed as delivery.

The operator explicitly requested completion alerts. The one-time update launch flag
`--enable-completion-notifications` requests authorization if not determined. Normal launches
still do not automatically request permission. Enable completion alerts and Notification settings
are always visible, including in auto mode. If macOS already denied permission, it must be enabled
manually in **System Settings → Notifications → Cu → Allow notifications**, with alerts enabled.
The controller logs the actual authorization status (0 undetermined, 1 denied, 2 authorized,
3 provisional) and alert setting to distinguish these cases. It re-reads settings on new task
outcomes/prompts without requesting permission, so enabling alerts externally while Cu is in the
background does not leave a stale disabled cache. Done/menu-bar results work without
notification authorization. Send/action approvals remain **off** by default; notification permission
is not a change to action policy.

## Input clearing

After verified success and successful process exit, the original task editor clears. The result,
evidence and run logs remain available. Failure, Needs input, Stop, unverified output, missing
summary or nonzero exit preserve the task for correction. A different/edited draft, stale run ID
or duplicate process callback cannot clear a newer task. The optional public research brief is
not automatically changed. No task is automatically replayed or started after clearing.

## Validation/deployment

119 Rust tests, strict all-target Clippy; 81 native tests passed, one live metadata test skipped;
backend/Node/signing regressions. Ten new pure-state tests cover process-exit/verification gating,
nonzero exits, input preservation, unknown remaining work, persistent Done, safe notification
content, launch/run binding, single-use body clicks, stale/new tasks, no approval actions and
retained auto default. No model/helper inference experiment, desktop input, task replay or Send.

Prepared artifact: `target/Cu-completion-status-staging/Cu.prepared`, same persistent certificate.
Actual installation, readiness, authorization and residual banner-delivery status: [STATUS.md](STATUS.md).
Logs: `target/completion-status-{rust-tests,clippy,native-tests,backend-tests,node-tests,signing-tests,build,install,gui-readiness}.log`.
