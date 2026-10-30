# Handoff: bounded-context learning and guarded recipes for Cu

**Status:** design proposal only; not implemented or authorized for implementation by this document.
**Audience:** owner and a future implementing engineer/agent.
**Scope:** help Cu improve at recurring tasks without growing planner context or constantly asking the user for feedback.

This document records the direction discussed with the owner. Numeric budgets, interfaces, storage layouts, and rollout thresholds below are proposed starting points, not existing functionality or final product decisions. Reinspect current source before implementation: the project is under active development.

## 1. Executive decision

Build a small, versioned **recipe book**, not a growing memory prompt.

- Keep run recordings outside planner context.
- Start with a few manually reviewed guarded recipes for repeated tasks.
- Supply only the relevant recipe stage and a small amount of applicable guidance.
- Obtain feedback primarily from observable outcomes, not repeated user ratings.
- Automatically propose improvements; initially approve changes in occasional batches.
- Evaluate changes in a resettable sandbox before promotion.
- Keep the old version available for rollback.
- Preserve existing host policy, approvals, completion verification, and uncertainty handling.

The first milestone is not autonomous recursive self-improvement (RSI). It is:

> A recurring task becomes more reliable after an evidence-backed recipe improvement, while the planner's learned-context budget remains fixed.

Analogy: logs are cooking recordings, recipes are tested procedures, and context is the kitchen counter. Only the recipe needed now belongs on the counter.

## 2. User goals and constraints

### Agreed direction

1. Avoid accumulating every failure, lesson, and skill in the prompt.
2. Avoid asking the user for feedback after each action or task.
3. Prefer a small practical system over fine-tuning or autonomous self-modification.
4. Learn from result checks, guarded failures, and successful recoveries.
5. Retain user involvement for genuine ambiguity, subjective preferences, consequential approvals, and occasional change review.
6. Produce a handoff for later consideration; do not implement runtime changes now.

### Non-goals for the first release

- Model weight training or fine-tuning.
- Arbitrary executable skills generated and deployed by a model.
- Automatic changes to safety rules, permissions, account identity, or approval policy.
- General-purpose vector memory infrastructure before simple scoped lookup proves insufficient.
- Replaying real emails, purchases, deletions, or production-file mutations as training experiments.
- An additional reflection/model call after every desktop action.
- Autonomous promotion of recipe changes.
- Claiming broad RSI or general intelligence improvement from a handful of task wins.

## 3. Current foundation and implementation touchpoints

These observations come from source inspection during the discussion, not a comprehensive audit. Paths are relative to the repository root.

| Existing component | Relevant behavior | Proposed use |
|---|---|---|
| `crates/cu-core/src/recorder.rs` | `RunRecorder` writes `events.jsonl`, frames, and JSON summaries; creates private directories/files | Evidence references and learning-attribution events |
| `crates/cu-core/src/agent.rs` | Agent loop, run summaries, actual action evidence, independent completion review and bounded recovery | Record recipe stages and outcomes; preserve policy and no-replay rules |
| `crates/cu-core/src/provider.rs` | `Provider::start`, `continue_with`, completion review and recovery interfaces | Eventually pass bounded host-selected recipe context without leaking provider-specific logic into recipes |
| `crates/cu-core/src/types.rs` | Observations, AX summaries, action outcomes, completion review types | Reuse evidence concepts; add explicit unknown and checker provenance for learning |
| `crates/cu-providers/src/codex.rs` | Persistent planner conversation within a run; detailed operational instructions | Inject selected stage guidance; later support safe bounded planning episodes |
| `crates/cu-core/src/policy.rs` | Host policy and approval boundary | Must remain authoritative for every recipe action |
| `crates/cu-core/src/testing.rs` | In-memory platform for agent tests | Deterministic unit and loop tests |
| `tools/CuTarget` | Inert event-receiving test app | Extend or complement with resettable workflow fixtures; do not assume it already implements a mail workflow |
| `tools/decision-bench.mjs` and its tests | Synthetic advisory decision fixtures | Starting point for decision regression tests, not end-to-end task quality proof |

Read current [completion contract](COMPLETION.md), [status](STATUS.md), and [hybrid helper boundaries](HYBRID.md) before changing runtime behavior. In particular, public helper evidence cannot establish private account identity or desktop delivery.

Important distinctions in current code:

- `ActionOutcome::Ok` is not semantic task success: an input may have been posted to the wrong place.
- A completion model's verdict is a useful signal, not objective ground truth.
- A `Done` summary alone should not promote a recipe or create a trusted lesson.
- The existing completion recovery explicitly avoids replay after denied or approved consequential actions. Recipe recovery must not weaken it.
- The current planner conversation grows within a run. Bounded learned context does not, by itself, bound that conversation.

## 4. Architecture: four separate layers

```text
Existing run recordings
        |
        v
Offline bounded evidence extraction
        |
        v
Candidate improvements (not visible to normal planner)
        |
        v
Deduplication + safe evaluation + owner review
        |
        v
Versioned active recipe store
        |
        v
Host scope matching + fixed-budget context selection
        |
        v
Current recipe stage -> normal planner -> unchanged policy -> action
        |                                                    |
        +---------------- observed result checks <-----------+
```

### Layer A: raw experience

Run events, screenshots, actual action outcomes, checker results, and completion evidence. Retain outside the prompt under an explicit privacy/retention policy.

### Layer B: candidates

Evidence-linked hypotheses about a failure and a possible improvement. These are untrusted and never automatically retrieved for live tasks.

### Layer C: active recipes

Reviewed, versioned procedures with scope, preconditions, stages, checks, and bounded recovery rules. They are advisory/procedural knowledge, not authority.

### Layer D: working context

Original task and constraints, current observation, active stage, a few relevant facts, and recent outcomes. Learned material has a hard budget independent of stored history size.

Storage may grow subject to retention limits. Planner context must not grow proportionally with it.

## 5. What a recipe is

A recipe is a parameterized, guarded procedure. It is not a coordinate macro, a transcript, a user preference, or a giant essay.

Example first recipe: **compose an email draft, without sending**.

Inputs:

- Requested account/profile constraints, if any.
- Recipient, subject, and body supplied for this task.
- Whether ambiguity requires user clarification.

Stages:

1. Inspect intended application/account context.
2. Open or identify a compose draft.
3. Resolve and commit exactly one intended recipient.
4. Explicitly target and populate Subject.
5. Explicitly target and populate Body.
6. Inspect the draft against requested fields.
7. Return verified draft status, incomplete, or unknown.

Guard examples:

- Do not assume a Chrome profile proves the website sender identity.
- Do not assume fixed tab order through recipient chips and form fields.
- Use exact AX focus guards only when the host actually knows the relevant identity.
- If the UI does not match the stage, yield back to general planning; do not improvise a hidden macro.
- Never include Send in this first recipe.

Draft creation still changes application state and may autosave remotely. It is not a harmless dry run merely because it excludes Send. Test initially in a sandbox.

### Execution levels

**First:** the host selects one stage, and the existing planner performs it through normal actions. A stage must have bounded attempts and explicit checks; it is not just extra prose.

**Later:** reliable stages may become tested host-managed state machines or deterministic primitives. Their actions still traverse normal policy, target validation, approvals, cancellation, and recording.

Do not implement a second privileged execution path for recipes.

## 6. Strict context management

### Proposed learned-context budget

Start with a configurable ceiling of approximately 1,000 tokens:

| Allocation | Illustrative maximum |
|---|---:|
| Active recipe stage | 400 |
| Relevant validated guidance | 400 |
| Version/scope/status metadata | 200 |

These are design defaults to test, not measured optimal values. Specify which tokenizer/accounting implementation is used; if exact accounting is unavailable, use a conservative bound and test worst-case strings. Token counts and byte limits are not interchangeable.

Rules:

1. At most one active recipe per planning episode initially.
2. Load the current stage, not the full cookbook or full recipe history.
3. Replace obsolete stage context; do not append new stage instructions indefinitely.
4. Retrieve zero extra lessons when applicability is weak.
5. Never evict original task constraints or mandatory safety instructions to fit learned content.
6. If content exceeds budget, drop optional guidance or fall back to ordinary planning. Do not truncate guards into a different meaning.
7. Record selected and omitted item IDs, versions, budget usage, and selection reasons without duplicating sensitive content.
8. No additional router call per action just to choose a recipe. Start with deterministic scope matching.

### Selection

Use structured eligibility before relevance ranking:

- Recipe active and not expired/revoked.
- Task intent/stage compatible.
- App bundle ID or domain compatible where independently available.
- Required observed capabilities present.
- Current UI conditions support its preconditions.
- No conflict with task-specific requirements.

Unknown required scope means ineligible. Inferred task stage is not an established fact; guards must independently validate applicability. Semantic search can be added later as a candidate finder, never as the sole eligibility gate.

Do not assume full-page AX data or browser DOM access already exists. The current observation provides bounded focus information and screenshots; any richer inspection capability is separate work.

### Active conversation versus learned context

A persistent provider thread may still retain previously injected stages, even if the host removes them from its next request. Merely changing a context field does not truly remove old tokens.

For the first bounded recipe, cap its stages and run duration, and report both current selected-context size and cumulative injected material. Do not claim total-context bounds until provider history handling supports them.

A later checkpoint/fresh-thread feature can bound long runs, but requires a separate safety design:

- Preserve the exact original task and constraints.
- Preserve a host-owned ledger of consequential attempts, approvals, denials, and uncertain outcomes.
- Carry references to evidence and outstanding requirements.
- Restore active recipe ID/version/stage and pending conditions.
- Reobserve focus/UI; do not assume the old screen still applies.
- Never rehydrate unresolved approvals as permission to execute.
- Never let a lossy model summary erase an uncertain Send or a denial.

Keep this out of the first implementation unless provider context constraints make it necessary. Until then, bound run length and fail/yield explicitly rather than pretending compaction is solved.

## 7. Feedback without constant user involvement

### Feedback priority

1. **Deterministic semantic checks:** test-app state, exact structured field values, sandbox output artifacts.
2. **Host execution checks:** focus mismatch, target changed, action denied/failed/not executed. These describe execution, not always task semantics.
3. **Visual/model checks:** screenshot and completion review against explicit requirements. Useful but fallible; preserve checker identity and evidence.
4. **User feedback:** necessary for ambiguity, subjective quality, explicit preferences, and approvals—not routine outcome labelling.

Every result check returns `pass`, `fail`, or `unknown`, with method and evidence. Do not average an unknown into success. A skipped action is not a failed attempt; a denial is not a problem to route around.

### Example automatic learning signal

```text
Stage: populate subject
Attempt: type with an expected-focus guard
Outcome: failed guard; no text posted
Recovery: inspect and explicitly target Subject
Check: actual subject matches requested subject
Candidate: strengthen field-targeting step for this scoped recipe
```

Record this as an observation plus a hypothesis. The sequence does not prove that targeting was the sole causal factor or that the change generalizes.

### Cases that still require user involvement

- Multiple plausible recipients or missing identity requirements.
- A consequential action needing the existing approval.
- Unclear delivery after a send attempt; do not resend to obtain cleaner feedback.
- Subjective preferences such as tone or quality beyond objective requirements.
- Occasional review of evaluated recipe revisions.

No response from the user is not positive feedback. Approval is permission for a particular action, not certification that a recipe is correct.

### Trigger extraction selectively

Use post-run/offline extraction, not synchronous reflection after every action. Initial triggers:

- A new guard failure or repeated failure fingerprint.
- A successful bounded recovery with observable evidence.
- A failed completion check with a clearly identified missing condition.
- An explicit user correction.

Exclude truncated, missing-evidence, ambiguous, and unrelated infrastructure failures from automatic procedural conclusions. They can still be diagnostic records.

## 8. Proposed records and storage

Use a simple local versioned JSON store initially; a database/vector index is not required for a handful of recipes. This example layout is new design, not current functionality:

```text
~/.cu/learning/
  candidates/<candidate-id>.json
  recipes/<recipe-id>/<version>.json
  evaluations/<evaluation-id>/report.json
  active.json
  review-decisions.jsonl
```

Reuse private file/directory conventions. Use atomic writes, schema validation, one writer/appropriate locking, immutable recipe versions, and an atomic active pointer. On corruption or unavailable storage, disable learned guidance and preserve normal safe execution; if already partway through a procedure, yield and reobserve rather than silently restart it.

Pin each active run to a recipe version. Promotion or rollback changes future selection; explicit emergency revocation must stop/yield an in-flight unsafe recipe rather than silently switching stages underneath it.

### Minimum recipe fields

- `schema_version`, `recipe_id`, `version`, `status`.
- `intent`, structured `scope`, required capabilities.
- Typed input names and constraints; never embed last run's private input values.
- Preconditions, stage IDs, stage guidance, check IDs.
- Allowed transitions, bounded attempts, yield/stop conditions.
- Risk classification; no independent approval authority.
- Creation/provenance references, evaluation reference, review decision.
- `supersedes`, expiry/revalidation conditions.

Use an allowlisted declarative schema. No arbitrary shell/code strings, dynamic imports, model-defined tools, or unrestricted selectors in the first version.

### Minimum candidate fields

- ID and schema version.
- Source run IDs and host evidence references.
- Scope and failure fingerprint.
- Observed facts, separated from hypothesized explanation.
- Proposed recipe diff and expected benefit.
- Contradicting evidence and uncertainties.
- Deduplication/supersession relationship.
- Evaluation and review status.

### Minimum outcome record

- Run, recipe version, stage, attempt, and check ID.
- `pass | fail | unknown` and diagnostic reason.
- Checker type/version and reference to observations/action outcomes.
- Whether text/input was actually posted.
- Safety/approval/uncertainty flags from the host ledger.
- Timing/usage where available; do not invent subscription dollar cost.

Do not store private chain-of-thought. Store concise evidence and testable hypotheses.

## 9. Learning lifecycle and growth control

```text
candidate -> deduplicated -> evaluated -> reviewed -> active
     |             |             |          |          |
     +---------- rejected -------+----------+          v
                                            superseded / retired / revoked
```

Promotion should usually modify an existing recipe, not create another global lesson.

Consolidation example: four incidents involving recipient chips and tab order become one scoped field-targeting improvement, with four evidence references outside context.

Growth controls:

- Deduplicate by scoped failure/behavior fingerprints; make extraction idempotent.
- Limit candidate generation per run and extraction cost/time.
- Merge repeated evidence into an existing candidate.
- Keep incompatible scopes separate; do not overgeneralize a Gmail rule to all forms.
- Expire or revalidate UI-dependent knowledge after relevant changes.
- Retire unused/contradicted versions; retain a compact audit trail.
- Add an explicit retention/storage quota for screenshots and candidates.
- Distinguish missing/deleted evidence from successful evidence. If source evidence expires, do not claim it remains independently inspectable.

New evidence that conflicts with an active recipe should trigger review or quarantine, not an automatic global rewrite.

## 10. Evaluation and promotion

### First evaluation environment

Implement a resettable compose-form fixture (in CuTarget or a separate inert app). It should expose independent semantic state for assertions while exercising real planner-facing observations. Do not give the planner the answer key.

Cover:

- Normal flow.
- Wrong initial focus.
- Recipient chip changing tab order.
- Different window sizes and field positions.
- Ambiguous recipient suggestions.
- Missing expected control or unsupported UI state.
- Delayed rendering/loading.
- Unexpected modal or changed app focus.
- Stale AX label/recipe scope.
- Prompt-injection-like text inside visible content.
- Denied/uncertain consequential action handling in separate safety fixtures.

Where real posted input is needed, run only in an explicitly authorized inert environment. Offline extraction never needs live desktop input.

### Comparison protocol

Compare three useful conditions where feasible:

1. Base planner without recipes.
2. Current active recipe.
3. Candidate revised recipe.

Hold model, reasoning configuration, environment, task inputs, and budgets fixed. Reset state between attempts, vary execution order, and repeat to expose nondeterminism. Keep held-out variations separate from candidate generation. Version the evaluation suite and checkers.

### Metrics

Primary:

- Independently checked task success.
- False completion claims.
- Unsafe attempts and approval/no-replay boundary violations.
- `unknown` outcomes and unnecessary user interruptions.

Secondary:

- Recurrence of the targeted failure.
- Model calls, reported token usage, wall time.
- Selected and cumulative learned-context size.
- Recovery attempts and yield rate.
- Extraction/evaluation overhead, measured separately from live task latency.

Do not optimize for fewer approvals, screenshots, or checks at the expense of safety. Separate necessary user input from avoidable interruptions. A correct refusal or safe yield is not the same as task success, but must not be penalized into unsafe behavior.

### Initial promotion gate

Manual batch approval is required. The review should show:

- Small readable recipe diff.
- Evidence-linked failure and hypothesized fix.
- Baseline/candidate results and held-out coverage.
- Safety regressions, unknowns, and sample counts.
- Evaluation configuration and rollback target.

No promotion for invalid scope/schema, missing evidence, unresolved safety regression, or an improvement claim based only on the proposing model's own success judgment. No safety regressions observed in a finite test suite is not proof of safety.

Choose numerical success/regression thresholds before running the comparison; do not invent statistical confidence from a tiny sample. A failed gate leaves the old recipe active. Later automatic promotion is a separate owner decision requiring substantially stronger evidence and narrowly defined eligibility.

## 11. Safety and privacy invariants

1. Recipes never authorize actions. Every input follows existing policy and approval checks.
2. No learning-driven changes to permissions, auth, account selection, or hard denials.
3. Never retry a denied action by another route.
4. Never repeat an uncertain consequential action to verify success or improve a recipe.
5. Existing more-conservative recovery restrictions win over recipe suggestions.
6. Source pages, screenshots, task text, and proposed lessons are untrusted data, not instructions to the learning system.
7. Validate provenance IDs against host records; reject invented observations/checks.
8. Keep recipient identities, message bodies, tokens, and account identifiers out of generalized recipes.
9. Local storage does not imply local inference: extraction using Codex or another model may transmit selected evidence externally. Establish an explicit extraction privacy setting/consent; honor the existing public-helper boundaries.
10. Redact/minimize evidence before model extraction where possible. Never silently route private screenshots or tasks through public research helpers.
11. Store no credentials, bearer tokens, or private reasoning traces.
12. Support disabling learning, deleting candidates, retiring recipes, and applying a retention policy independently of normal execution.
13. Pin versions and record attribution so regressions can be reproduced and rolled back.
14. Candidate extraction/evaluation failure must not alter a completed task's result or silently activate a fallback recipe.

## 12. Phased implementation plan

### Phase 0 — select one workflow and define success

- Start with draft composition in a synthetic environment, excluding Send.
- Write objective stage checks and independent finish-line assertions.
- Define safety failures, unknown outcomes, and held-out variations.
- Select context accounting and initial budgets.

Deliverable: fixture specification and acceptance tests. No runtime learning.

### Phase 1 — one manually authored guarded recipe

- Add a typed, validated recipe schema and a tiny local registry.
- Implement deterministic eligibility and one active recipe stage.
- Pass bounded guidance through the normal provider/agent path.
- Record stage transitions, checks, versions, and budget use.
- Keep recipe use opt-in and default behavior unchanged.

Deliverable: baseline versus recipe evaluation. Demonstrate an actual reliability benefit, not merely more infrastructure.

### Phase 2 — automatic evidence collection and proposals

- Add selective post-run extraction from authorized run artifacts.
- Separate observed facts from hypotheses.
- Deduplicate and propose diffs to the existing recipe.
- No candidate injection or automatic promotion.

Deliverable: a reviewable evidence-backed proposal generated without asking the user to label every run.

### Phase 3 — safe evaluation and batch review

- Run candidate versus active/baseline in the resettable fixture.
- Generate a compact review report.
- Add explicit promote/disable/rollback operations with atomic version pointers.
- Keep the user out of routine checks; involve them only in occasional promotion decisions and genuine task questions.

Deliverable: one full improvement cycle with evidence, held-out tests, review, promotion, and rollback.

### Phase 4 — more recipes and selective compilation

- Add only workflows justified by repeated use and failures.
- Move stable procedures into host state machines where it reduces reasoning safely.
- Add stronger retrieval/indexing only when simple scoped selection is insufficient.
- Consider safe episode checkpoints separately if long-run context growth is a demonstrated problem.

Deliverable: more capability without larger learned-context limits.

### Deferred

Automatic promotion, online optimization, fine-tuning, autonomous code changes, and production experimentation require separate design and owner approval. Do not smuggle them into the first release under the label “learning.”

## 13. Suggested interfaces (not implemented)

Possible CLI shape:

```text
cu recipes list
cu recipes inspect <recipe-id> --version <version>
cu learn extract <run-id>
cu learn candidates
cu learn inspect <candidate-id>
cu eval recipe <candidate-id> --suite <suite-id>
cu learn promote <candidate-id> --evaluation <evaluation-id>
cu recipes disable <recipe-id>
cu recipes rollback <recipe-id> --to <version>
```

Command names are illustrative. Favor a simple CLI/report workflow before building native review UI. Promotion must validate evaluation provenance and review authority; it must not just copy arbitrary model-written JSON into the active registry.

Useful events:

```text
recipe_selected
recipe_stage_started
recipe_check_result
recipe_stage_yielded
recipe_finished
learning_candidate_created
learning_candidate_deduplicated
recipe_evaluation_completed
recipe_promoted
recipe_revoked
```

Record enough metadata to answer “Which recipe changed this behavior?” without sending the entire recipe history to the planner.

## 14. Required tests

### Schema/store

- Unknown fields/actions, invalid transitions, cycles without bounds, oversized content, path traversal, and unsupported versions rejected.
- Atomic promotion/rollback and concurrent-writer behavior.
- Corrupt/missing store yields safe fallback; active stages do not restart silently.
- Private file modes and sensitive-data exclusion.
- Idempotent extraction and deduplication.

### Selection/context

- Ineligible and stale recipes not selected.
- Unknown precondition means yield, not assumed applicability.
- Candidate/rejected recipes never retrieved.
- Budget remains fixed as stored knowledge grows.
- No truncation of guards; optional content dropped safely.
- Old stages do not silently accumulate through application-level injection; provider-history limits reported honestly.

### Agent/policy

- All recipe actions use normal policy/approval/cancellation paths.
- Focus changes and guard failures prevent incorrect typing.
- Denied and uncertain consequential actions never replayed.
- Policy restrictions override recipe recovery.
- Missing checks cannot claim recipe success.
- Aborted/partial runs do not become positive training examples.
- Version pinning, emergency revoke, and stage yielding behave correctly.

### Learning/evaluation

- Unsupported evidence IDs rejected.
- Model success claims alone cannot label objective success.
- Prompt injection in screenshots, source material, or candidates cannot alter authority.
- Held-out cases not used to generate the candidate being evaluated.
- Candidate that improves speed but worsens correctness/safety fails the gate.
- Small/ambiguous evaluation results remain inconclusive rather than auto-promoted.

## 15. Open decisions for the future implementation session

Resolve these before coding the relevant phase; they are not prerequisites to retaining this handoff.

1. Exact first workflow and whether to extend CuTarget or create a dedicated fixture.
2. Whether existing AX/focus observations suffice for stage checks, or additional safe inspection APIs are needed.
3. Learned-context accounting method and budget; bounds on cumulative provider history.
4. Extraction privacy consent, model choice, evidence minimization, and cost limits.
5. Retention limits and deletion semantics for recordings versus derived artifacts.
6. Evaluation sample counts, performance thresholds, and treatment of inconclusive results.
7. CLI-only review versus later native UI integration.
8. Whether any future low-risk promotions may become automatic; default is no.

## 16. Acceptance criteria for the first useful release

- [ ] One repeated workflow has a guarded recipe with explicit checks and bounded recovery.
- [ ] Routine feedback comes from observed outcomes without per-step user questions.
- [ ] At least one evidence-backed improvement is proposed automatically.
- [ ] Candidates remain outside live planner context until evaluated and approved.
- [ ] Baseline/current/candidate comparison includes held-out variations and independent result checks.
- [ ] Learned context remains within its fixed budget as stored records increase.
- [ ] Current selected-context bounds are distinguished from total conversation history.
- [ ] No safety/approval/no-replay policy is weakened.
- [ ] Private task values do not leak into reusable recipes or public helpers.
- [ ] Version attribution, disable, rollback, and safe storage-failure behavior work.
- [ ] A user can review improvements occasionally rather than supervise learning continuously.

## 17. Instructions for the next implementing agent

1. Read this proposal and current project instructions; inspect the latest code and related docs.
2. Confirm the owner now wants implementation; this handoff alone is not that permission.
3. Inspect working-tree changes and coordinate with ongoing work. Do not overwrite unrelated changes or assume the repository state is clean.
4. Propose the smallest Phase 0/1 slice and resolve its open decisions with the owner.
5. Keep the initial scope to one recipe, a resettable fixture, bounded context, and objective checks.
6. Do not run live desktop input, send email, reset permissions, change accounts, install an app build, or transmit private run evidence without the applicable explicit authorization.
7. Run relevant offline tests and report exactly what was and was not validated.
8. Ask permission before committing or pushing, following project identity policy.

**Bottom line:** Cu should acquire better procedures, not a bigger prompt. Start with automatic checks, automatic proposals, and occasional human-reviewed promotion—not constant feedback and not autonomous deployment.
