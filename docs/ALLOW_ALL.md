# Opt-in Allow all actions (version 14)

## Use

Open Cu's menu-bar panel, choose **Control → Allow all actions**, then start a task.
No extra per-task or per-action confirmation dialog is added. A visible warning
explains that actions run without approval. Selection is locked during an active task.
It applies to tasks started while selected in this app session; relaunching Cu returns
to **Critical actions only**. The local default/config remains `confirm-risky`.

## What changes

- Existing engine `auto` policy is now available in the native selector. Consequential
  actions—including Send, deletion, purchases, sensitive typing and command execution—
  execute without Cu asking for action approval. Understand the task before starting it.
- Provider safety-check approval requests are acknowledged automatically in this mode,
  with `automatic: true` logged; no human approval or input-baseline reset is fabricated.
- The host supplies the actual run policy to the planner and independent verifier.
  In auto mode, needing action approval alone is not a reason to stop and ask the user.
  The model cannot change the policy by declaring a different mode.
- Notification authorization is not needed for automatic action execution. Human-pause
  notifications/panel controls remain available.

## What does not change

Stop / Control–Option–Escape, human-interference pause, hard denials, optional exact
AX typing-focus guards, normal target/display resolution, run limits, private/public
helper boundaries, tool-disabled subscription authentication, and evidence-based
completion still apply. There is no approval-popup focus restoration when no approval
popup occurs; the ordinary execution/focus checks remain. This is not atomic UI or
identity assurance. macOS-required permissions cannot be bypassed.

Genuine missing information, login, ambiguous recipient/account or uncertain delivery
can still produce Needs your input. Auto mode does not invent an answer, authorize
unrelated work, or remove hard-denied controls.

Known risky input is marked `consequential_attempted` immediately before execution,
including potentially partial/failed execution. Both rejected-plan repair and
completion recovery honor this barrier even without human approval. This prevents
those recovery paths from replaying a known automatically attempted Send/payment/etc.
The existing denied/approved barriers remain. Risk detection remains best-effort,
not a guarantee every consequential action is identified.

## Validation/deployment

118 Rust tests, strict all-target Clippy, 71 native tests passed (one live metadata
test skipped), backend/Node/signing regressions. Synthetic tests cover prompt-free
risky input/provider checks, no replay after successful or uncertain automatic
consequential attempts, hard denials, exact focus guard, human pause, Stop, mode
arguments/defaults, and the effective policy in planner/verifier protocol messages.
No physical desktop input, WhatsApp task replay, Send or live inference experiment
was used. Real auto-mode task behavior remains unverified.

Prepared artifact: `target/Cu-allow-all-staging/Cu.prepared`, signed with the existing
certificate; exact installation/readiness results are recorded in [STATUS.md](STATUS.md).
Logs: `target/allow-all-{rust-tests,clippy,native-tests,backend-tests,node-tests,signing-tests,build,install,gui-readiness}.log`.
