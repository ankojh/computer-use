# Native app discovery and rejected-plan repair (version 13)

## Diagnosed runs — no task replay

- `20261029-180501-cb3fcfeb`: the planner chose Chrome/WhatsApp Web without inspecting the native app. It reported `needs_input` at a web login screen, and the independent review accepted that premature blocker. Five inputs were executed; no requested message was composed or sent.
- `20261029-180600-bd32f73d`: an initially guessed Spotlight focus identity failed safely (no text posted). The planner recovered and opened native WhatsApp. Saved observation 4 shows the signed-in native app and a matching-name conversation (not independent proof of the intended contact identity). The next reply failed `expected_focus` parsing before execution, ending the run. Its raw rejected reply was not retained, so the precise malformed action/field cannot be reconstructed. Four inputs were executed; no requested message was composed or sent.

These were planner/discovery and action-validation failures, not evidence of a missing app, failed Codex login, or new macOS permission requirement. Existing conversations in a screenshot are not evidence this task sent anything.

## Read-only native discovery

On a normal task start, the native frontend reads bounded `.app/Contents/Info.plist` metadata from `/Applications`, `~/Applications`, `/System/Applications`, and `/System/Applications/Utilities`:

- At most four roots, 512 entries/root, 256 unique bundle records, 64-KiB plist files. App/Contents/plist symlinks are not followed.
- Only app filename and bundle identifier are selected, with format/control characters stripped from names and bounded identifiers. No executable loading, app launch, scripting, app-container/document reads, login checks, account data or Spotlight service query.
- Whole-word task matching and narrow Chrome/WhatsApp aliases select at most eight candidates, max 4 KiB. Other app names, paths, task text and arbitrary plist values are not included.
- This is advisory private context for the subscription-backed planner and fresh completion verifier, recorded in private run events. It is **never** sent to Parallel or Jev, never an input authorization or proof of login/account/contact/delivery.
- Native-to-engine transport uses `CU_NATIVE_APPS_JSON`; it is removed from the spawned Codex process environment and supplied only as labelled prompt metadata. Invalid/oversized context is ignored. No API key/account fallback was added.
- Readiness, own-view renders and inert pointer diagnostics do not collect this inventory. CLI/web without this native context still use UI discovery; missing inventory is not evidence an app is absent. Apps outside these roots/limits or with unusual names may be missed.

Planner instructions prefer the native app for an application the task actually asks to **use**, unless a website/browser/profile is explicitly required. Apps mentioned only as research subjects or message contents do not imply an app launch. The planner must inspect fresh focus after activation, not invent a Spotlight/message field identity. A web login screen alone does not establish a blocker when the native route remains unchecked. The independent verifier gets the same candidate metadata and checks actual launch/screenshot evidence before accepting such a blocker; incomplete work can use the existing single completion-recovery allowance. This is evidence/instruction-based routing, not a formal guarantee the model always discovers the right app/account.

## Strict schema and bounded repair

- Disjoint action-schema branches require `expected_focus: null` on every non-type action. Only typing accepts an exact editable role/label. Nonblank label pattern and a 200-code-point schema bound fit within the retained 800-byte host guard limit.
- Host parsing still rejects malformed/misplaced guards. It does not strip guards, execute a valid action prefix, or execute any part of a rejected reply.
- A received but invalid plan becomes host-only `invalid_plan`, with sanitized validation code, no executable actions or raw rejected text. Usage, route and latency are retained in `model_response`; `plan_rejected` records zero execution and `summary.json` counts `plans_rejected`. This does not promise usage recovery from a cancelled/failed transport.
- The engine permits **one repair per run**, from a fresh observation, only within step, wall-time and estimated-cost budgets and with cancellation/human-interference checks. Stop takes precedence over polling the repair request. Time spent in host notifications does not extend the remaining wall budget.
- Any prior denied or explicitly approved input action blocks repair conservatively (including approved routine actions in confirm-all). This prevents the repair path from replaying an approved/uncertain Send or bypassing a refusal. A second invalid reply stops, with no repair storm or legacy-empty-plan success escape.
- Repair remains a normal tool-disabled Codex planning request, not authorization. Corrected input still passes the unchanged policy, exact focus guard where supplied, approval, focus/AX/display revalidation and completion gates. Protocol/auth/tool-call errors are not converted into plan repairs.

## Native WhatsApp submission safeguard

Bare Return/keypad Enter and typed newlines in native WhatsApp now require approval where the current control may submit a message, including missing AX element metadata. A positively identified search field remains routine, as do plain message typing, Shift–Return and navigation. This does not add a universal send detector for all websites/chat clients: existing best-effort AX/shortcut classification remains, and identity/delivery still need actual evidence.

Notification approval uses the existing launch/run/prompt-bound Approve/Deny path; these changes do not auto-approve anything or place message contents into OS notifications.

## Validation

112 Rust tests (26 core, 40 provider/helper/protocol, 45 fake-agent-loop, 1 bounded diagnostic), strict all-target Clippy; 70 native tests passed, 1 live metadata test skipped; backend, Node and signing regressions. Tests use synthetic bundle metadata, deterministic fake Codex subprocesses and `FakePlatform`, not live model inference or physical input. Coverage includes rejected-prefix exclusion, safe diagnostics, accounting, fresh-frame single repair, double-rejection stop, denied/approved barriers, Stop, deadlines and limits, legacy-completion rejection, inventory privacy, native-vs-web verifier context, and native Return/newline policy.

A separate read-only probe of this exact discovery implementation found the actual installed `WhatsApp` / `net.whatsapp.WhatsApp` metadata (`target/native-discovery-metadata-probe.log`). It did not activate WhatsApp, inspect accounts/chats, capture the desktop, request inference, or post input. This validates local discovery, not the planner's eventual task outcome.

Installed artifact: `target/Cu-native-discovery-staging/Cu.prepared`, using the existing certificate-backed requirements. See [STATUS.md](STATUS.md) for installation/readiness results. No WhatsApp interaction, task replay, message/Send, public-helper request, or extra model inference experiment was used for this update. Real end-to-end task behavior remains unverified.
