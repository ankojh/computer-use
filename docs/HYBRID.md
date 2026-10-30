# Public-only hybrid helpers

## Deployment status

Hybrid helpers were introduced and validated in version 11, and remain in the currently installed [version 15](ALLOW_ALL.md). Native app inventory is private planner/verifier context only, never helper input. The following records the version-11 deployment/smoke history, not the current app PID/hash.

Version 11 was installed at `/Applications/Cu.app` from `target/Cu-hybrid-staging/Cu.prepared`, using the existing certificate without permission resets/requests or login changes. Actual GUI PID 19874 logged both grants true at 16:29:37 and live Codex readiness true at 16:29:38 on 2026-10-29. The app is idle; no desktop task/email was rerun. Final app CDHash: `b2502f307d6c12faaffbe2739cfa4ffb27006d77`; engine CDHash: `7b91c826d665d93f706a812f63f15ad68d450617`. Previous-copy backup: `target/app-backups/20261029T162936.2HlTS4`.

A freshly authorized public/synthetic smoke passed: Parallel public search + decision with six retained sources in **1,508 ms** (four HTTP attempts including MCP handshake), Jev metadata judgment in **127 ms** (one HTTP attempt). Five host-counted HTTP sends, no application-level retries, no desktop capture/input or private task/email/account context. Subsequent final-build hardening also explicitly disables reqwest's default protocol-NACK retries; the smoke was not repeated. [Saved summary](benchmarks/20261029-hybrid-smoke.json); raw public reports in `target/hybrid-live-smoke.jsonl`. These are transport/synthetic results, not real-task speed or quality claims. The earlier benchmark allowance and this one-shot smoke allowance are exhausted; do not repeat inference experiments without new authorization.

## Routing by need, not an extra classifier

The fixed Luna-medium subscription planner remains in charge of desktop planning. It can propose one of two **argument-free, standalone** helper actions:

| Capability | Host dispatch | Allowed outbound context |
|---|---|---|
| `public_research` | Parallel public MCP `web_search` then `web_decisions` | Only the separately entered public brief, plus fixed host instructions/criteria. No arbitrary planner query. |
| `ui_judgment` | Jev `jev-latest` | Closed projection: allowlisted role category, focus-present boolean, enabled boolean/null, secure boolean. No label/value/title/app/PID/window/image. Secure input is handled locally without calling Jev. |
| Neither needed / unavailable / uncertain | Luna and normal browser path | Existing desktop observations and approval policy. |

There is no helper classifier on every planning turn and no automatic fan-out to both vendors. Luna chooses whether a capability is useful; the host deterministically chooses its provider. Jev's tiny metadata judgment is advisory, often unnecessary when local checks suffice, and is not a demonstrated useful fast path. It does not compare intended Subject/To/Body identities. Exact AX typing guards remain local and independent.

A helper must be the only action in its batch; mixed helper/input batches fail before input or a helper request. The host takes the usual fresh observation afterward. A missing public brief disables Parallel. Jev without a configured usable key is unavailable; Start/readiness still depend on Codex and normal permissions, not the helpers.

## Privacy and consent

The native panel has a **Use hybrid helpers · public data only** toggle and an optional **Public research brief** field. Nothing is copied from the private task into it. Starting with a nonempty field explicitly shares that field with Parallel. Leave it blank to keep research on the browser/desktop path; turn helpers off to prevent both new services from being called.

Only put information you knowingly intend to make public in that field. Obvious email addresses, credential-like values, local addresses, nonpublic/local URLs, URL credentials/query/fragment data and some account/login URLs are rejected before execution. This is **not a general PII detector, sanitizer, DNS/private-network proof, or guarantee that arbitrary text is safe**. Names and private prose cannot reliably be inferred from free text. Never paste private correspondence, recipients, account identity, logged-in page contents, or secrets there.

Parallel receives no cookie jar or browser profile. It retrieves on its own public-web infrastructure. Cu does not install an MCP server into Codex, alter Codex configuration, sign up, log in, or switch accounts. Private/logged-in browsing and all desktop/email actions remain on the normal computer-use path. Returned excerpts, source titles and judgments are untrusted data, not instructions. Outbound helper bodies have no free-text planner argument, so returned instructions cannot change the authorized public brief or copy private desktop content into a helper call.

Jev's key is read inside the engine from the explicitly configured dotenv path, or `TYPESAFE_API_KEY` when no file is selected. A selected missing/invalid file does not fall back to an ambient credential/account. Dotenv iteration does not export variables. No key/session/header/raw error body is included in reports or logs; helper key environment variables are removed from spawned Codex environments. No API-key fallback for the **main subscription planner** is added.

## Limits, timing, caching and billing

- Default **12 HTTP attempts total per run**, shared across both services. Configurable downward only. Failed requests and MCP initialization/notification count.
- One Parallel research operation normally uses **four HTTP attempts**: initialize, initialized notification, public search, public decision. There is no tool-listing or authenticated research call.
- Default 10-second per-request timeout (500–15,000 ms configurable); additionally the agent imposes **15 seconds for the helper operation after approval or the remaining run wall budget, whichever is smaller**, with cancellation. Human approval consumes the global run budget and cannot extend it; budget and Stop are rechecked before any helper request.
- No automatic transport retries (`reqwest::retry::never()`, requiring reqwest ≥0.12.28) or redirects; a rate limit/access/format error becomes unavailable rather than a login flow.
- Per-run cache by the fixed public brief or closed metadata state, including failures. A cancellation placeholder prevents the same partially completed operation from being silently replayed. Cache hits reuse the report ID, add no network/token usage, and are marked in report data.
- Responses are bounded to 512 KiB. At most six deduplicated public HTTPS sources, two 2,000-character excerpts/source, and 200-character titles reach the planner. URLs with credentials, queries, fragments, ports, local/IP-literal hosts or known account/login pages are not retained. This conservative filter can omit legitimate sources.
- Jev probability output must have exactly the allowed keys and valid normalized probabilities; selected probability below 0.9 is unavailable, **not a calibrated safety guarantee**.
- Parallel's public no-key MCP is currently documented as free at low rate limits. Jev uses its existing, separately metered API credential. OpenAI subscription access does not cover that bill. Pricing is not estimated: the legacy usage `cost_usd: 0` means unpriced here, not proof of free use or no consumption. The dollar-cost limit cannot account for unknown helper fees; the separate hard request cap applies regardless.
- No end-to-end speed, research-quality, accuracy or safety improvement has been measured for this integration. Helper calls add a planner round trip, even when the external HTTP call itself is fast.

## Actions, approval and completion remain separate

Helpers have no platform/input handle and cannot approve, click, type, switch profiles, resolve email recipients, send, or establish delivery. A positive Jev judgment never bypasses policy or an exact AX guard. Observe mode sends no helper requests. Confirm-all asks for the helper access too; declining prevents the request. Normal hard denials, confirm-risky consequential approvals, confirm-all, human-interference pause and Stop remain.

Successful host-recorded Parallel reports with real URL/excerpt pairs can be cited by ID in the independent completion verifier's `advisor_ids`. Fabricated, unavailable, Jev-only and excerpt-free reports are rejected as public-source evidence. These references **supplement**, not replace, the existing actual screenshot-reference requirement; a screenshot used for run/desktop context is not itself proof of public facts. Public source evidence may support research requirements, but not a requested profile, sender/recipient, draft or Send. Those require desktop/action/receipt evidence as before. The verifier remains a model/evidence check, not a formal identity or delivery guarantee. No recovery replay after denied or approved consequential actions is introduced.

Observed research/metadata stages, source URLs, helper status, total attempts and helper timing are shown in the attached native panel and recorded in action events. They are not executed input counts or completion percentages.

## Configuration and CLI

Native version 11 enables the helper option by default, with an empty public brief. Helpers are requested only as needed during a user-started task. Existing version 10 and the optional web frontend do not automatically enable the new CLI flags.

```toml
# Existing planner selection is unchanged:
[codex]
routing = "fixed"
model = "gpt-6-luna"
reasoning = "medium"

[hybrid]
# Optional: an existing dotenv file, NOT the secret itself. No shell sourcing.
jev_env_file = "/absolute/path/to/existing/.env"
max_requests = 12
timeout_ms = 10000
```

```sh
# This starts a REAL desktop task; not an inert smoke/test command.
cu run 'Your private desktop task' --provider codex --hybrid \
  --public-research 'Only the public topic/questions you explicitly want shared'
# Omit --public-research: no Parallel request. Omit --hybrid: neither helper enabled.
```

`codex-status` still performs subscription/catalog metadata checks only: no helper calls or inference. There is no new account dependency for basic readiness.

## Validation and explicit live-smoke boundary

Version-11 local validation: **96 Rust tests passed**, strict all-target Clippy passed; **58 native tests passed, 1 live metadata test skipped**; **9 Swift backend tests**, **3 Node tests**, and persistent-signing regressions passed. Prepared signed artifact: `target/Cu-hybrid-staging/Cu.prepared` (version 11), with the existing certificate-pinned requirement. Strict installed bundle signatures and identical prepared/installed executables are verified. Actual GUI grants/readiness and the separately authorized live synthetic helper results are recorded above.

Offline tests cover literal/separate CLI arguments, stale-run native reports, no approval/completion authority, observed stages, metadata canaries, public payload canaries, strict argument-free plans, mixed-batch rejection, observe/confirm-all denial, positive advice not authorizing Send, bounded response/request counts, Stop/budget expiry during approval preventing network access, malformed/refused results, normalized probabilities, RPC IDs, cancellation/no replay, failure caching, source-ID validation, and screenshot requirements.

HTTP tests use loopback fixture servers and synthetic credentials only. They make no real inference or physical input. Existing Rust, native, backend, optional web and signing regressions continue to apply.

For a **newly authorized** smoke only:

```sh
# Fixed public Rust brief + synthetic role metadata. No Codex, desktop capture/input,
# browser, login, private tasks/email/screenshots, account changes, or retries.
# At most one Parallel search + one Parallel decision + one Jev request,
# maximum five HTTP attempts including the MCP handshake.
CU_JEV_ENV_FILE=/absolute/path/to/existing/.env \
  cargo run -p cu-providers --example hybrid_smoke -- --live-public-only
```

The example refuses to run without its explicit opt-in flag. Do not interpret transport/synthetic smoke success as a successful real desktop/email task.

Official interfaces: [Parallel MCP](https://docs.parallel.ai/integrations/mcp/quickstart.md), [Parallel Decisions](https://docs.parallel.ai/decisions/decisions-quickstart.md), [TypeSafe/Jev](https://docs.typesafe.ai/introduction).
