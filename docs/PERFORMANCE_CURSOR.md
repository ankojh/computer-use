# Performance investigation and mouse-event feedback — 2026-10-29

## Observed bottlenecks, not estimated progress

Read-only analysis of run `20261029-135552-2e3dde1e`:

| Measurement | Observed |
|---|---:|
| Wall time | 177,554 ms |
| Planner calls | 13 |
| Planner total | 123,852 ms (69.8% of wall time) |
| Planner median / p95 / max | 7,665 / 16,028 / 21,109 ms |
| Planner turns 9–11 correcting compose fields | 51,629 ms, **included** in planner total |
| Automatic settle waits | 9,600 ms |
| Explicit waits | 8,600 ms |
| Human pause / Send response | approximately 28 seconds |
| Executed / failed / denied | 32 / 0 / 1 |

The software counted misplaced typing as successfully posted input, not a semantic success.
The run reached research/drafting, but **Send was declined and the run was aborted**; the email
was not sent by this run. This investigation did not retry that task or alter its draft.

Most delay was planning/recovery, not cursor travel. The earlier easy synthetic planner benchmark
(~2.6 s Luna-medium) was not representative of this multi-step desktop run. No end-to-end speedup
percentage is claimed for this update; the new browser/email path still needs a separately
user-started task to measure it. Human approval time is not a performance defect to bypass.

## Jev / Parallel experiment

Owner authorized up to 12 synthetic requests per provider, using available credentials, with no
private screenshots/email/run data. `tools/decision-bench.mjs` made **12 inference requests each**:

| Provider | Median client wall time | p95 | Expected advisory choices |
|---|---:|---:|---:|
| Jev (`jev-latest`, existing TypeSafe key) | 79.5 ms | 133 ms | 6/12 |
| Parallel Decisions (public no-key MCP) | 512.5 ms | 596 ms | 6/12 |

Parallel's documented free public `web_decisions` endpoint required no account, key, MCP config
installation, or login. MCP initialization/listing is outside the per-inference timings. Jev
credentials were loaded inside the benchmark process only, sent only to the official TypeSafe
endpoint, never printed or committed. No automatic retries. Unknown server/API failures stop a
provider's lane rather than consuming repeated calls.

The same small fixture/question set covered field matching, unknown focus, secure focus, declined
Send, uncertain Send, a draft without a receipt, and injected value text. Both services mostly chose
`inspect` instead of identifying matched/mismatched ordinary fields. The measured 6/12 is **this
prompt/fixture result**, not a general model-quality rating. It is insufficient evidence of a useful
live fast path. These are text/JSON advisory decisions, **not screenshot-coordinate plans or a
safety benchmark**; comparing their latency to full Codex planning is not apples-to-apples.

**At the time of this benchmark neither provider was integrated.** Subsequently the operator
explicitly authorized [version-11 public-only helper integration](HYBRID.md), with a separate public
research brief and content-free metadata boundary; its distinct five-attempt live smoke is not a
rerun of this benchmark. Fixed Luna-medium, subscription auth, no per-turn classifier, approval
policy, screenshot/evidence review and no-consequential-replay rules remain. No real-task speedup
has been established. An extra fast call is only worthwhile if it removes slower work, not if
added to every turn.

- Exact synthetic fixtures, question, timings and probabilities: [benchmark JSON](benchmarks/20261029-decisions.json).
- [Jev SDK docs](https://docs.typesafe.ai/).
- [Parallel Decisions](https://docs.parallel.ai/decisions/decisions-quickstart),
  [question types](https://docs.parallel.ai/decisions/question-types),
  [public MCP](https://docs.parallel.ai/integrations/mcp/quickstart).

```sh
# Opt-in network use. Potentially billable Jev / authenticated Parallel API usage.
# Only the fixed synthetic fixtures are accepted; no arbitrary private state input.
node tools/decision-bench.mjs --live-synthetic --env-file ../jev-exp/.env \
  --output target/decision-bench.json
# Offline protocol/fixture tests, no credentials/network:
node --test tools/decision-bench.test.mjs
```

## Local reliability/speed changes

- Instruct the planner to commit the resolved recipient, explicitly click Subject and Message Body,
  and type them separately—not guess tab counts through recipient chips or flatten fields into
  one tab/newline-delimited string. A wrong-field issue calls for refocusing, not repeated content rewrites.
- Optional `expected_focus` on `type` supplies an exact editable AX `role` and `label`.
  The host compares them against the **current** focus already fetched for policy, before posting
  text. Mismatch/missing/disabled/noneditable focus fails without typing and halts the remaining
  batch; the provider gets a fresh observation. No new inference or AX query is added by the guard.
- Match only exact title/description/identifier, case-insensitively; never a substring, value, or help
  text. Missing identity is not a match. This is an optional reliability guard, **not** formal
  sender/profile identity verification or defense against malicious UI labels.
- Prefer the focused application's AX focused element, with the system-wide element as fallback.
- Explain existing post-input settling and automatic post-batch screenshots, so the planner avoids
  habitual extra waits/redundant screenshots. The fixed **300 ms** settling fallback itself remains;
  waits are not stripped from returned plans and safety/approval timing is not shortened.

Example engine action (other action fields omitted):

```json
{"type":"type","text":"Synthetic subject","expected_focus":{"role":"AXTextField","label":"Subject"}}
```

Unknown/unsupported AX identity can use `null`, with explicit focus inspection instead. Other
providers and existing mock scripts remain compatible with ordinary typing. No real Gmail focus
identity or task-speed claim is inferred from synthetic guard tests.

## Brief movement and click feedback

Owner chose **brief pulses**, not a persistent last-position marker:

- Cyan arrow/ring on each posted Cu mouse move, including click/scroll's implicit move and drag
  interpolation samples. Separate monotonically unique sequences rearm repeated same-position moves.
- Pink button-down and separate green click ripple, with left/right/etc. button and multi-click count.
  Drag/button releases are labelled `up`, not falsely presented as click actions.
- Click ripples live independently for 0.75 seconds; subsequent movement/typing cannot erase them.
  Movement feedback fades in approximately 0.9 seconds. No artificial delay is added to production input.
- At most eight **actual recycled click windows**, plus the one movement marker—even inside one large
  event batch before AppKit drains its autorelease pool. Clock/animation work stays on the UI thread.
- All feedback remains click-through, nonactivating, AX-hidden, excluded with Cu from SCK capture,
  and clears on pause/stop/completion. Shared physical cursor and human-interference pause remain.
  Orange pending approval feedback does not mean input was posted.

The Swift input layer calls a thread-local, synchronous observer **after** posting tagged CGEvents;
ABI version is now **2**. Rust borrows callback context only for synchronous execution and clears it
on success/error/unwind. It cannot authorize or modify input. Held-button state is recorded before
the callback can run, so a congested feedback pipe cannot hide a held down event from stop cleanup.
A posted event is not evidence that a website processed it or a message was delivered.

Own-view renders (not desktop captures):

![Movement](native-agent-cursor.png)
![Click](native-agent-click.png)

## Validation and opt-in live diagnostic

76 Rust tests; strict Clippy; 56 native tests including opt-in live metadata readiness (default:
55 passed, one skipped); 9 Swift backend tests across XCTest/Swift Testing; Node/web and persistent
signing regressions. Coverage includes exact/missing/spoofed focus labels, halted typing batches,
posted-event parsing, repeated positions, click lifetime across moves, a fixed real-window pool,
thread-local callback cleanup, no keyboard metadata, and Retina/negative/stacked geometry.

The owner-authorized physical test used **only inert CuTarget squares**: seven actions, including
repeated same-position movement, left single/double clicks, and a right click. Native GUI received
15 posted feedback events and rendered four click ripples. CuTarget independently logged the same
15 tagged events, eight button down/up receipts, correct counts and ≤1 point coordinate error.
No typing, scrolling, desktop capture, model inference, browser interaction or email replay.

The first validator rejected AppKit's numeric `NSEventType(rawValue: …)` format. It was corrected and
**the same saved records** were revalidated successfully, without repeating physical input.
Artifacts: `target/speed-pointer-live-artifacts`, `target/speed-pointer-verified.log`.
The subsequent fixed-window-pool refinement is covered by synthetic stress tests; the posted-input
engine is unchanged from that live check.

```sh
# Real input: ONLY with explicit permission. Quit idle Cu; never interrupts a task.
swift build --package-path tools/CuTarget -c release
python3 tools/pointer-smoke.py --live-inert-targets
# Offline evidence verification, no new input:
python3 tools/pointer-smoke.py --verify-artifacts target/speed-pointer-live-artifacts
```

The hidden engine diagnostic revalidates target PID/app/window, owned bounded layout, unchanged
inert square geometry, current AX target, policy, human pointer interference, cancellation and
an eight-second bound. It never asks a model or captures the desktop. It is not exposed as a
planner tool, and cannot approve consequential input.
