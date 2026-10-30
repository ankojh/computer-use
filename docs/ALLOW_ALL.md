# Allow all actions — default since version 15

## Current behavior

**Allow all actions is now the native default**, including after relaunch. Starting a task
passes `--mode auto` without selecting a mode or acknowledging another popup. This Mac's
`~/.config/cu/config.toml` also explicitly sets `[safety] mode = "auto"`, so normal CLI
runs using that config agree. Other custom CLI configs/explicit mode flags still control
their own behavior; the shared library fallback for an unconfigured CLI remains cautious.

Send, deletion, purchases, sensitive typing and command execution run **without Cu action
approval prompts**. Critical actions only, Confirm every action and Observe only remain
available as explicit overrides. Mode selection stays locked during an active task.
A selected cautious override lasts for that app instance; a fresh launch defaults to auto.

Notifications are optional and not needed for automatic action execution. Normal startup only
reads authorization status. Version 16 adds [completion alerts/live progress](TASK_STATUS_NOTIFICATIONS.md)
with visible Enable completion alerts / Notification settings controls, including in auto mode.
The operator-requested update enablement can explicitly request the one-time macOS authorization;
ordinary launches do not prompt. Actual authorization is currently denied, so the owner must enable
Cu in macOS Notification Settings. This never enables Send or other action approvals.

The app logs `Native default control mode: auto` at startup. Native tests check the actual
default-generated engine arguments and fresh instances, not just availability of the option.

## Why version 14 still prompted

Version 14 added auto as an opt-in, but retained/reinstated Critical actions only by default.
The operator subsequently explicitly requested no Cu approval prompts by default.

Both `20261030-011735-f85bd9d1` and `20261030-132459-314977db` actually used `confirm-risky`.
The latter was waiting on a native WhatsApp Send button, not missing macOS permissions.
For the version-15 update that exact waiting engine was cancelled without approving Send
or replaying the task. Its synchronous stdin confirmation needed the documented second
Ctrl-C exit; no Send confirmation/action completion followed the pending policy decision.
The unsent draft may remain. The task was not automatically restarted under auto.

## Retained boundaries

Stop / Control–Option–Escape, human-interference pause, hard denials, optional exact
AX typing-focus guards, normal target/display resolution, run limits, private/public
helper boundaries, tool-disabled subscription authentication, and evidence-based
completion remain. There is no approval-popup focus restoration when no approval popup
occurs; ordinary execution/focus checks remain. This is not atomic UI or identity assurance.
macOS-required Accessibility/Screen Recording permissions cannot be bypassed.
Genuine missing information/login, ambiguous account/contact or uncertain delivery can
still require input. Auto does not authorize unrelated work or waive receipt evidence.

The actual host policy is supplied to the planner and verifier so needing action approval
alone is not a blocker in auto mode. Provider safety-check approval requests are acknowledged
automatically, logged as `automatic: true`; no human approval/baseline reset is fabricated.
Known risky input is marked `consequential_attempted` before execution, including potentially
partial/failed execution. Completion recovery and rejected-plan repair honor that barrier
without human approval. Risk detection remains best-effort, not universal detection.

## Validation and deployment

119 Rust tests, strict all-target Clippy, 71 native tests passed (one live metadata test
skipped), plus backend/Node/signing regressions. A synthetic native WhatsApp Send-button
regression explicitly verifies zero approval calls; other tests cover automatic risky input,
provider checks, no consequential replay, hard denials, exact focus guard, human pause, Stop,
default run arguments, fresh-launch default and no notification-authorization request.
No live auto-mode task, message/Send, physical input test or inference experiment was run.

Prepared artifact: `target/Cu-auto-default-staging/Cu.prepared`, signed with the existing
certificate. Installation/startup mode/readiness results: [STATUS.md](STATUS.md).
Logs: `target/auto-default-{rust-tests,clippy,native-tests,backend-tests,node-tests,signing-tests,build,install,gui-readiness}.log`.
