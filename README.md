# cu — native macOS computer-use agent

A model looks at the screen, decides on mouse/keyboard actions, and `cu` executes them on the real
Mac. Rust core + Swift macOS layer joined by a C ABI. See [HANDOFF.md](HANDOFF.md) for the full
design and [docs/STATUS.md](docs/STATUS.md) for what is implemented.

## Build

```sh
# Rust stable (rustup) + Xcode / Swift 6
cargo build            # also builds swift/CuMac via crates/cu-mac-sys/build.rs
./target/debug/cu --version
```

## Test

```sh
cargo test --workspace                     # unit + agent-loop tests, no permissions needed
(cd swift/CuMac && swift test)             # Swift key parsing / geometry tests

# Live tests: post real input events into tools/CuTarget. Grant Accessibility + Screen Recording
# to your terminal first, and keep your hands off the mouse/keyboard while they run.
CU_LIVE_TESTS=1 cargo test -p cu-mac --test live_cutarget -- --ignored --nocapture --test-threads=1
```

## Use

Permissions attach to the *responsible process*: run `cu` from your terminal app and grant the
permissions to that app.

```sh
cu doctor [--request] [--open] [--input-test]
cu display | cu cursor
cu screenshot --out shot.jpg [--max 1440] [--png]
cu click 864 558 [--space points|image] [--button left] [--count 2] [--mods cmd+shift]
cu move 100 100 | cu drag 100 100 400 300
cu type "hello" | cu key "cmd+space" [--repeat 3] | cu scroll 500 400 --dy 3 [--pixels]
cu ax focused | cu ax at 500 400
cu run --provider mock --script examples/observe-demo.json --mode observe "demo"
```

Runs are logged under `~/.cu/runs/<timestamp>-<id>/` (`run.json`, `events.jsonl`, `frames/`,
`summary.json`). Configuration lives in `~/.config/cu/config.toml` (HANDOFF.md §14).

## Layout

```
crates/cu-core        actions, coordinate mapping, keymap, policy, recorder, agent loop
crates/cu-providers   Codex subscription + mock; optional public-only Parallel/Jev helpers
crates/cu-mac-sys     build.rs (builds + links Swift) and raw extern "C" decls
crates/cu-mac         safe wrapper implementing cu_core::Platform
crates/cu-cli         the `cu` binary
swift/CuMac           Swift static library; include/cu.h is the ABI
tools/CuTarget        AppKit test app that logs every event it receives
tools/CuApp           native SwiftUI/AppKit menu-bar app with attached popover
```

## Native Mac app (menu bar + attached panel)

No browser, WebView, or web server is needed. The native app bundles the existing Rust/Swift
engine and talks to `cu run --ui` directly over stdin/stdout. The Codex CLI must be installed and
signed in with your ChatGPT subscription (`codex login`).

```sh
# One-time local signing setup; reuses an existing configured identity.
# macOS may require Keychain approval. No Apple developer membership needed.
tools/CuApp/setup-signing.sh --create-local
# Build/install locally (Swift 6 + Rust stable). Quit Cu before installing.
# Reuses that identity, installs one copy, and preserves the old app in target/app-backups.
tools/CuApp/install-app.sh
open /Applications/Cu.app
# Optional: expand the attached panel immediately.
# open /Applications/Cu.app --args --show-menu
```

- **Menu bar only:** click Cu to expand a transient native popover directly below its icon. Click
  outside or close it to dismiss. There is no separate controls window, draggable task overlay,
  or automatic popup over your work. Closing the panel does not stop a task.
- **Public-only hybrid helpers (version 11):** fixed Luna-medium remains
  the desktop planner. It requests Parallel only for a separately entered public research brief,
  Jev only for content-free role/boolean judgments, or neither. No automatic fan-out or private
  task/screenshot/email sharing. Bounded/cached calls, normal approvals and independent completion
  review remain. See [configuration, privacy limits and validation](docs/HYBRID.md).
- **Task controls:** task editor, confirm-risky / confirm-all / observe modes, readiness checks,
  activity, final answer, explicit approvals, run logs, and Quit all live in this attached panel.
  **Allow all actions** is an explicit opt-in in Control; Critical actions only remains the default.
  Starting closes it and restores the previous app. See [Allow all](docs/ALLOW_ALL.md).
- **Progress strip:** current execution stage, planning step, and successfully posted input count.
  These are host-observed counters, not a guessed completion percentage or an estimate of total work.
- **Allow all actions (version 14):** choose **Control → Allow all actions** to skip Cu action approvals,
  including Send, deletion, purchases and commands. No extra confirmation popup. Stop, human pause,
  hard denials, focus checks, limits and completion verification remain; macOS permissions are still
  required. The choice lasts while selected in this app session; relaunch defaults to Critical actions
  only. Known automatic consequential attempts also block recovery replay. [Details](docs/ALLOW_ALL.md).
- **Native app discovery and plan repair (version 13):** task-relevant, read-only installed-app metadata
  helps the private planner check the native app before giving up at a web login screen. Strict action
  schemas and one budgeted correction of an unexecuted invalid reply retain focus/approval safeguards;
  any denied/approved input blocks automatic repair. Native WhatsApp Return/newlines are guarded as
  possible Send. The app inventory and private task/screenshots are never sent to public helpers. [Details](docs/APP_DISCOVERY_RECOVERY.md).
- **Approvals (introduced in version 12):** **Critical actions only** is the default; routine inputs run automatically.
  A native notification offers **Approve / Deny**, or **Resume / Stop task** after human interference.
  Clicking its body opens the exact pending request in the attached panel, not a blanket approval.
  Allow Cu's one-time macOS notification request; OS settings/Focus control delivery. The menu-bar
  `!` remains a fallback. Stale/duplicate notifications cannot control another task. Notifications
  exclude private task/typed/email contents. See [approval details](docs/APPROVAL_NOTIFICATIONS.md).
  Before releasing a response, the app closes the popover and waits up to one second for the original app's focus. The engine
  independently checks focus/target/display identity before executing an approved action. Changed
  targets fail closed. Agent keyboard input into Cu itself is denied.
- **Live Codex readiness:** uses `cu codex-status` to check the account and model catalog, not just
  cached `codex login status`. One authentication refresh is allowed on a 401; persistent rejection
  becomes an actionable sign-in/workspace error. JSON stdout stays separate from diagnostic stderr;
  unrelated warnings cannot masquerade as a failed login. See [the native status fix](docs/CODEX_READINESS.md).
  This check makes no model turn or desktop input.
- **Agent cursor:** brief cyan movement pulses follow Cu's posted mouse events, including drag
  samples and implicit moves before clicks. Pink button-down and distinct green click ripples show
  the button and multi-click count. A subsequent move/typing action does not erase a click ripple;
  movements fade in ~0.9s and click ripples in 0.75s, without adding input sleeps. Orange **Cu target**
  marks a pending approval, not an executed click. Feedback is click-through, nonactivating, AX-hidden,
  and excluded from model screenshots. Pause/stop/completion clear it. Toggle **Highlight Cu’s shared
  cursor** in controls. This is **not an independent second mouse**: human movement still pauses Cu.
  See [performance findings, provider comparison and cursor validation](docs/PERFORMANCE_CURSOR.md).
- **Stop:** persistent panel-header button or **Control–Option–Escape** globally (if the shortcut is available).
  The app closes the approval pipe, interrupts the engine to release held input, and force-kills its
  process group after three seconds if needed. Quitting during a task asks before stopping it.
- **Permissions:** use the separate **Enable Accessibility…** and **Enable Screen Recording…** buttons
  and enable the installed **Cu** in System Settings → Privacy & Security. Permission calls run in
  bounded helpers using the same signed app executable, never on the UI thread. Checks time out after
  4 seconds; explicit requests after 20 seconds. A timeout is shown as unknown, not a denial. Repeated
  requests are coalesced. Status refreshes on return from Settings and periodically while setup is
  incomplete; relaunch if macOS requests it. The app never starts a task on launch.

If a Cu permission entry refuses to enable, inspect the permission logs before changing anything.
The earlier ad-hoc builds left stale signing requirements; removing/re-adding an entry did not always
resolve that. The one-time move to persistent signing may need a **Cu-only**, explicitly authorized
reset and manual reapproval. Do not reset TCC for routine updates. The installer preserves old
bundles with a non-`.app` suffix, unregisters old locations, and registers the final installed path.
It does **not** reset TCC, grant permissions, or change grants for other apps.
The app shows its actual bundle path and has **Show app in Finder** to help select the right copy.
Read permission diagnostics with `log show --last 10m --predicate 'subsystem == "dev.ankojh.cu"'`.

The app now uses a **persistent local Keychain signing certificate**, not ad-hoc signing. Both app
and engine pin that certificate in their designated requirements. Missing identities fail closed;
there is no ad-hoc fallback. Keep this Keychain identity for future builds. A real version 7 → 8
update retained **both macOS permissions without reapproval**. See [signing setup and proof](docs/SIGNING.md).
Developer ID/notarization would still be needed for normal distribution to other Macs.

The build captures `PATH` as a fallback for GUI launches and also checks standard CLI locations.
Rebuild after moving CLI installations. This app and the optional web dashboard are independent
frontends: do not run tasks in both at once. Live model-driven workflows still need manual
verification; native UI/protocol/signing checks do not prove research or email task quality.

```sh
swift test --package-path tools/CuApp
# Starts no task, posts no input, renders only the app's own views, then exits.
"/Applications/Cu.app/Contents/MacOS/Cu" --smoke-test
```

### Completion verification (new builds)

Codex now distinguishes `continue`, `complete`, and `needs_input`. An empty action list is not
proof of success. Terminal claims are checked in a fresh, tool-free context using the current model
with **at least medium reasoning**, actual action outcomes, cited historical screenshots, and a
fresh screenshot. With the selected fixed configuration this uses **Luna-medium**, without
re-enabling a per-turn classifier. Brief requirement/evidence findings are logged and shown in
native controls; private reasoning traces are not requested.

Recoverable missing work gets one bounded replan. Genuine blocks are shown as **Needs your input**,
not Finished. Approved/denied consequential actions are never automatically replayed by completion
recovery, including an uncertain Send. Research/email verification checks source evidence, profile,
recipient/content, and a matching post-send receipt—not merely a draft or a proposed Send click.
Chrome profiles and website sender accounts are treated as distinct identities.

In confirm-risky mode, exact single Cmd–Tab, Cmd–Shift–Tab, and Cmd–Space navigation no longer asks
merely because Settings is focused. Send/submit/delete approvals and hard denials remain; confirm-all
still confirms navigation. macOS grants are not reset or requested by this change.

See [the verification contract, validation, and deployment status](docs/COMPLETION.md). This is a
reasoned evidence check, not a formal guarantee of identity, research quality, or email delivery.
Version 10 is installed with these checks, the attached menu-bar UI, persistent signing, the
corrected Codex status parser, native mouse-event pulses and optional exact AX typing-focus guards. GUI readiness and both grants are verified ready after the
update, without reapproval. See [current status](docs/STATUS.md).

## Mobile dashboard (optional)

The dashboard adds a responsive task screen, streamed progress, remote confirmations, a stop
button, and an on-demand screen preview. The Rust/Swift engine still executes all actions through
its existing policy. Node.js 20+ and the Codex CLI must be on the Mac's PATH; the dashboard has no
npm dependencies and is embedded in the `cu` binary.

```sh
cargo build --release
codex login                  # sign in with Adam's ChatGPT account, on the Mac
./target/release/cu doctor --request
./target/release/cu serve     # private preview on this Mac at port 8787
```

For a phone on the same trusted Wi-Fi, explicitly bind to the Mac's LAN address, for example
`cu serve --host 192.168.1.20`, or use `--host 0.0.0.0` to listen on every IPv4 interface. Use the
private URL printed by `cu serve` and add the page to your phone's Home Screen (Safari Share menu
on iPhone; Chrome menu on Android). A new server start creates a new connection code. The Mac must
stay awake and `cu serve` must keep running. The page retains its pairing only for the browser
session; paste the connection code from the private link if a Home Screen launch needs pairing.
For access outside your Wi-Fi, use a private VPN or an authenticated HTTPS reverse proxy. Direct
LAN HTTP does not encrypt screenshots or tasks; don't expose the port to the public internet.

Only a paired browser can read screen previews, see tasks, start or stop runs, or respond to an
approval. The private URL is a bearer capability: anyone with it has the same control. It is
removed from the address bar on connection, then replaced by an HttpOnly, SameSite=Strict cookie.
The bridge rejects cross-origin requests, isolates approvals by run and prompt IDs, bounds request
sizes and event history, and allows only one active task. Closing or stopping the server aborts
its active run. This is a personal dashboard, without separate users or roles.

## Codex subscription, dynamic models, and reasoning

The native app, `cu run`, and optional dashboard default to Codex using the signed-in ChatGPT
subscription. The adapter uses the [Codex app-server protocol](https://developers.openai.com/codex/app-server/),
verifies ChatGPT login, and rejects API-key-only authentication. It does not extract or expose
OAuth tokens. Use `codex login` to select the desired account; account ownership cannot be inferred
from a successful sign-in.

**Dynamic routing is the default and is reconsidered on every planning turn.** A separate fast
OpenAI classifier sees the original task, current screenshot/focus, the latest actual action results,
and up to three bounded recent proposed plans. It chooses a tier, not arbitrary execution settings:

| Next-step difficulty | Preferred planner | Reasoning |
|---|---|---|
| Routine, clear mechanical step | `gpt-6-luna` | low |
| Deliberate planning, account/profile checks, ambiguous UI | `gpt-6-astra` | medium |
| Complex reasoning or error recovery | `gpt-6-astra` | high |

The model can change up or down on each turn. A failed action imposes a host-side high-reasoning
floor, even if the classifier asks for routine. Only screenshot-capable models in the signed-in
catalog are eligible. If preferred models are unavailable, selection falls back to available vision
models (preferring Sol for stronger planning); unsupported reasoning may move upward within the
low/medium/high cap, never downward or to max/ultra/delegation. A missing explicit model pin is an
error. Malformed/failed/timed-out routing stops the run without executing a guessed plan.

The router uses a fresh ephemeral context for each decision, released after use. The planner keeps
one conversation across model switches; every `turn/start` explicitly overrides both model and
reasoning, so an old high effort cannot accidentally carry into a low-effort turn. Routing has a
20-second deadline; planning has a 120-second deadline. Cancellation and the run's wall-time bound
still apply. **Every-turn routing adds one inference call per planning turn**; it is not guaranteed
to be faster. A synthetic 1px test measured about 2.3–2.4s routing overhead, not a real desktop benchmark.

The attached menu-bar panel shows the last completed plan's model, reasoning, and short routing explanation.
`events.jsonl` records them in each `model_response.route`, with separate router/planner latencies
and router identity. `run.json` describes initial configuration, not the final resolved model.
Token usage includes classifier and planner when reported by Codex; subscription dollar cost is not
estimated (`cost_usd: 0` is not evidence of zero usage). No private reasoning traces are requested.

Every input still passes through the unchanged policy/approval engine; a stronger model cannot
approve its own actions or bypass a denial. Explicit profile/account prerequisites are emphasized
in planner instructions, but routing itself is **not** a deterministic identity-verification guard.
Batching up to eight predictable actions and the existing 300 ms settle waits remain.

```sh
cu run --provider codex --routing dynamic "task"
# Pin the planner, while still varying reasoning each turn:
cu run --provider codex --model gpt-6-astra "task"
# No router call; use a fixed model and reasoning:
cu run --provider codex --model gpt-6-astra --reasoning medium "task"
```

Or configure `~/.config/cu/config.toml`:

```toml
[provider]
default = "codex"
[codex]
routing = "dynamic"             # default; alternatively "fixed"
# model = "gpt-6-astra"         # optional planner pin
# router_model = "gpt-6-luna"   # optional classifier pin
reasoning = "medium"            # used only with routing = "fixed"
screenshot_max_long_edge = 1440
```

The dashboard uses confirm-risky mode by default, with confirm-all and observe options. It never
selects auto mode. Screen Recording and Accessibility must be granted on the Mac to the terminal
that launches `cu serve`. Missing permissions appear as setup instructions in the dashboard.

```sh
node --test crates/cu-cli/web/server.test.cjs
# Optional: real routing + planning replies (including high-reasoning recovery)
# using only a synthetic 1px image; posts no computer input.
cargo test -p cu-providers codex_live_plans_without_input -- --ignored --nocapture
```

An opt-in, no-input [synthetic latency benchmark](docs/benchmarks/2026-10-28.md) compared fixed
Luna-low, Luna-medium, and Astra-medium across 18 planner calls. Planner medians were 2.35s,
2.63s, and 4.13s respectively. Both Luna configurations looked faster in this small sample;
low versus medium was close. This is not a real desktop quality/latency guarantee. Benchmarking
itself does not change the installed app or active configuration.

No Jev or Parallel service is involved in this route: both classifier and planner use Codex's
ChatGPT subscription. An opt-in, synthetic-only [12-request/provider comparison](docs/PERFORMANCE_CURSOR.md)
measured Jev/Parallel advisory decisions but did not establish a reliable fast path. Subsequently,
explicitly authorized [version-11 public-only helpers](docs/HYBRID.md) were added separately from
that routing system: Parallel public-brief research, optional Jev metadata advice, and Luna fallback.
Fixed Luna-medium remains the desktop planner; no real-task speedup is claimed.
