//! The provider-agnostic agent loop (HANDOFF.md §4 "One step of the loop", §9.4).

use std::future::Future;
use std::time::{Duration, Instant};

use anyhow::Context;
use serde::Serialize;
use serde_json::{json, Value};
use tokio_util::sync::CancellationToken;

use crate::action::{Action, ResolvedAction, ScrollDelta};
use crate::coords::CoordMapping;
use crate::platform::{ObserveOpts, Platform, SettleOpts};
use crate::policy::{Decision, Policy, PolicyContext, Risk};
use crate::provider::Provider;
use crate::recorder::RunRecorder;
use crate::types::{
    ActionOutcome, AxElement, CallRef, CompletionReview, CompletionVerdict, ModelRoute,
    Observation, PointerFeedback, PointerPhase, SafetyCheck, StopKind, Turn, Usage,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStage {
    Planning,
    Executing,
    Researching,
    Judging,
    Verifying,
    Recovering,
}

/// Observed work counters, not an estimate of the percentage of the task done.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct TaskProgress {
    pub stage: RunStage,
    pub step: u32,
    pub actions_executed: u32,
}

/// The human in the loop: confirmations, pauses, and status output.
pub trait Supervisor: Send + Sync {
    /// Ask whether a risky (or, in confirm-all mode, any) action may run.
    fn confirm_action(&self, req: &ConfirmRequest) -> bool;
    /// Show provider safety checks verbatim; `true` only on explicit human approval.
    fn confirm_safety_checks(&self, checks: &[SafetyCheck]) -> bool;
    /// Human input was detected; everything is released. Resume or abort?
    fn human_input_paused(&self) -> PauseChoice;
    /// Progress output (model text, observe-mode descriptions).
    fn notify(&self, message: &str);
    fn task_progress(&self, _progress: TaskProgress) {}
    fn advisor_report(&self, report: &crate::types::AdvisorReport) {
        self.notify(&format!(
            "{} helper: {} ({} ms)",
            report.provider, report.status, report.latency_ms
        ));
    }
    fn completion_review(&self, review: &CompletionReview) {
        self.notify(&format!("Completion check: {}", review.summary));
    }
    /// Click-through visual feedback; no input or authorization side effects.
    fn pointer_feedback(&self, _feedback: Option<PointerFeedback>) {}
    /// Actual per-turn planner route, independent of action authorization.
    fn model_route(&self, route: &ModelRoute) {
        self.notify(&format!(
            "{} · {} reasoning — {}",
            route.model, route.reasoning, route.reason
        ));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PauseChoice {
    Resume,
    Abort,
}

pub struct ConfirmRequest<'a> {
    pub action: &'a Action,
    pub resolved: &'a ResolvedAction,
    pub reason: &'a str,
    pub element: Option<&'a AxElement>,
    pub focused_app: Option<&'a crate::types::AppInfo>,
}

impl ConfirmRequest<'_> {
    pub fn describe(&self) -> String {
        describe_resolved(self.resolved)
    }
}

#[derive(Debug, Clone)]
pub struct AgentLimits {
    /// Model turns.
    pub max_steps: u32,
    pub max_wall_time: Duration,
    pub max_cost_usd: f64,
    /// Cap for a single `wait` / `hold_key`.
    pub max_wait: Duration,
}

impl Default for AgentLimits {
    fn default() -> Self {
        AgentLimits {
            max_steps: 50,
            max_wall_time: Duration::from_secs(15 * 60),
            max_cost_usd: 5.0,
            max_wait: Duration::from_secs(30),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct AgentOptions {
    pub limits: AgentLimits,
    pub observe: ObserveOpts,
    pub zoom: Option<ObserveOpts>,
    pub settle: SettleOpts,
    /// PID of the terminal app running `cu` (hard-deny keyboard input into it).
    pub own_terminal_pid: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Done,
    NeedsInput,
    Refused,
    LimitHit,
    Aborted,
    SafetyDeclined,
    Error,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunSummary {
    pub status: RunStatus,
    pub message: Option<String>,
    pub final_text: Option<String>,
    pub steps: u32,
    pub actions_executed: u32,
    pub actions_failed: u32,
    pub plans_rejected: u32,
    pub actions_denied: u32,
    pub confirmations: u32,
    pub usage: Usage,
    pub wall_ms: u64,
    pub model_latency_ms: Percentiles,
    pub settle_ms: Percentiles,
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct Percentiles {
    pub count: usize,
    pub p50: u64,
    pub p95: u64,
    pub max: u64,
}

impl Percentiles {
    pub fn of(samples: &[u64]) -> Self {
        if samples.is_empty() {
            return Percentiles::default();
        }
        let mut s = samples.to_vec();
        s.sort_unstable();
        let at = |q: f64| s[((s.len() as f64 - 1.0) * q).round() as usize];
        Percentiles {
            count: s.len(),
            p50: at(0.5),
            p95: at(0.95),
            max: *s.last().unwrap(),
        }
    }
}

#[derive(Default)]
struct Stats {
    steps: u32,
    executed: u32,
    failed: u32,
    denied: u32,
    confirmations: u32,
    usage: Usage,
    model_latency: Vec<u64>,
    settle: Vec<u64>,
    final_text: Option<String>,
    evidence: Vec<Value>,
    observations: Vec<Observation>,
    verification_recoveries: u32,
    verification_required: bool,
    plans_rejected: u32,
    plan_repairs: u32,
}

impl Stats {
    fn has_replay_barrier(&self) -> bool {
        self.evidence.iter().any(|e| {
            e["outcome"] == "denied"
                || e["approved"] == true
                || e["consequential_attempted"] == true
        })
    }
}

/// Early exits from the loop.
enum Halt {
    Status(RunStatus, String),
    Error(anyhow::Error),
}

impl From<anyhow::Error> for Halt {
    fn from(e: anyhow::Error) -> Self {
        Halt::Error(e)
    }
}

/// Text Anthropic requires for every action after a failed one in the same batch.
pub const NOT_EXECUTED_TEXT: &str = "Not executed: an earlier computer action in this turn failed.";

pub struct Agent<'a> {
    pub provider: &'a mut dyn Provider,
    pub platform: &'a dyn Platform,
    pub policy: &'a Policy,
    pub supervisor: &'a dyn Supervisor,
    pub recorder: &'a RunRecorder,
    pub opts: AgentOptions,
    pub cancel: CancellationToken,
}

/// Releases held buttons/keys on every exit path, including panics.
struct ReleaseGuard<'a>(&'a dyn Platform);

impl Drop for ReleaseGuard<'_> {
    fn drop(&mut self) {
        let _ = self.0.release_all();
    }
}

/// Runs blocking platform calls without stalling the async runtime.
fn blocking<T>(f: impl FnOnce() -> T) -> T {
    match tokio::runtime::Handle::try_current() {
        Ok(h) if h.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread => {
            tokio::task::block_in_place(f)
        }
        _ => f(),
    }
}

async fn with_cancel<T>(
    cancel: &CancellationToken,
    fut: impl Future<Output = anyhow::Result<T>>,
) -> Result<T, Halt> {
    tokio::select! {
        biased;
        _ = cancel.cancelled() => Err(Halt::Status(RunStatus::Aborted, "interrupted".into())),
        r = fut => r.map_err(Halt::Error),
    }
}

impl<'a> Agent<'a> {
    pub async fn run(mut self, task: &str) -> RunSummary {
        let started = Instant::now();
        let platform = self.platform;
        let _guard = ReleaseGuard(platform);
        let mut stats = Stats::default();

        let (status, message) = match self.run_inner(task, started, &mut stats).await {
            Ok(()) => (RunStatus::Done, None),
            Err(Halt::Status(s, m)) => (s, Some(m)),
            Err(Halt::Error(e)) => (RunStatus::Error, Some(format!("{e:#}"))),
        };
        if status != RunStatus::Done {
            let _ = platform.release_all();
        }

        if status == RunStatus::NeedsInput {
            stats.final_text = message.clone().or(stats.final_text);
        }
        let summary = RunSummary {
            status,
            message,
            final_text: stats.final_text.clone(),
            steps: stats.steps,
            actions_executed: stats.executed,
            actions_failed: stats.failed,
            plans_rejected: stats.plans_rejected,
            actions_denied: stats.denied,
            confirmations: stats.confirmations,
            usage: stats.usage,
            wall_ms: started.elapsed().as_millis() as u64,
            model_latency_ms: Percentiles::of(&stats.model_latency),
            settle_ms: Percentiles::of(&stats.settle),
        };
        self.recorder.event(
            "done",
            serde_json::to_value(&summary).unwrap_or(Value::Null),
        );
        if let Err(e) = self.recorder.write_json("summary.json", &summary) {
            tracing::warn!("writing summary.json: {e:#}");
        }
        summary
    }

    async fn run_inner(
        &mut self,
        task: &str,
        started: Instant,
        stats: &mut Stats,
    ) -> Result<(), Halt> {
        let cancel = self.cancel.clone();
        let limits = self.opts.limits.clone();

        self.provider.set_control_mode(self.policy.mode);
        // The initial observation goes with the first request (saves a round trip).
        let mut obs = self.observe()?;
        stats.observations.push(obs.clone());
        self.recorder.event(
            "model_request",
            json!({ "step": 1, "frame": obs.frame_ref }),
        );
        self.progress(RunStage::Planning, 1, stats);
        self.supervisor.notify("Assessing and planning step 1…");
        let mut request_started = Instant::now();
        let mut turn = with_cancel(&cancel, self.provider.start(task, &obs)).await?;
        let mut last_action_end = Instant::now();

        loop {
            stats.steps += 1;
            let latency = request_started.elapsed().as_millis() as u64;
            stats.model_latency.push(latency);
            stats.usage += turn.usage;
            self.record_turn(&turn, latency, stats.steps);
            if let Some(route) = &turn.route {
                self.supervisor.model_route(route);
            }
            if let Some(text) = &turn.assistant_text {
                if !text.trim().is_empty() {
                    self.supervisor.notify(text);
                }
                stats.final_text = Some(text.clone());
            }

            if turn.stop == StopKind::InvalidPlan {
                stats.plans_rejected += 1;
                stats.verification_required = true;
                if !turn.actions.is_empty() {
                    return Err(anyhow::anyhow!(
                        "rejected replies must not contain executable actions"
                    )
                    .into());
                }
                let rejection = turn
                    .plan_rejection
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("missing plan rejection diagnostics"))?;
                self.recorder.event(
                    "plan_rejected",
                    json!({"step":stats.steps,"validation":rejection,"executed_from_reply":0}),
                );
                self.supervisor.pointer_feedback(None);
                let barrier = stats.has_replay_barrier();
                if barrier {
                    return Err(Halt::Status(RunStatus::NeedsInput,"The planner returned an invalid reply after a denied, approved or automatically attempted consequential action. No actions from that reply were executed, and automatic repair/replay is blocked. Inspect the outcome before continuing.".into()));
                }
                if stats.plan_repairs >= 1 {
                    return Err(Halt::Status(RunStatus::Error,"The planner returned a second invalid action plan. No actions from either rejected reply were executed; stopped rather than weakening the focus guard.".into()));
                }
                if stats.steps >= limits.max_steps || stats.usage.cost_usd > limits.max_cost_usd {
                    return Err(self.limit("step/cost limit before safe plan repair".into()));
                }
                self.check_interrupts(&mut last_action_end)?;
                let fresh = self.observe()?;
                let remaining = limits.max_wall_time.saturating_sub(started.elapsed());
                if remaining.is_zero() {
                    return Err(self.limit("wall-time limit before safe plan repair".into()));
                }
                stats.plan_repairs += 1;
                obs = fresh;
                stats.observations.push(obs.clone());
                self.progress(RunStage::Recovering, stats.steps + 1, stats);
                self.supervisor.notify("The planner reply was rejected before execution. Correcting its format once; no focus guard or approval is bypassed.");
                self.recorder.event("model_request", json!({"step":stats.steps+1,"frame":obs.frame_ref,"reason":"rejected_plan_repair"}));
                let remaining = limits.max_wall_time.saturating_sub(started.elapsed());
                if remaining.is_zero() {
                    return Err(
                        self.limit("wall-time limit before safe plan repair request".into())
                    );
                }
                request_started = Instant::now();
                turn = with_cancel(&cancel, async {
                    tokio::time::timeout(
                        remaining,
                        self.provider.replan_after_rejection(rejection, &obs),
                    )
                    .await
                    .context("safe plan repair exceeded the run deadline")?
                })
                .await?;
                continue;
            }
            if turn.stop == StopKind::Refusal {
                return Err(Halt::Status(
                    RunStatus::Refused,
                    turn.assistant_text.unwrap_or_default(),
                ));
            }
            if matches!(turn.stop, StopKind::VerifyCompletion | StopKind::NeedsInput) {
                stats.verification_required = true;
                if !turn.actions.is_empty() {
                    return Err(anyhow::anyhow!("terminal claim cannot execute actions").into());
                }
                self.supervisor.pointer_feedback(None);
                self.progress(RunStage::Verifying, stats.steps, stats);
                self.supervisor
                    .notify("Verifying the outcome against your task and recorded evidence…");
                let fresh = self.observe()?;
                let mut evidence = Vec::new();
                for seq in &turn.evidence_frames {
                    let frame = stats.observations.iter().find(|o| o.frame_seq == *seq)
                        .ok_or_else(|| Halt::Status(RunStatus::NeedsInput, format!("Completion evidence refers to unavailable observation {seq}; success is not verified.")))?;
                    if !evidence.iter().any(|o: &Observation| o.frame_seq == *seq) {
                        evidence.push(frame.clone());
                    }
                }
                if evidence.is_empty() {
                    evidence.extend(stats.observations.iter().rev().take(3).cloned());
                }
                evidence.push(fresh.clone());
                if serde_json::to_vec(&stats.evidence)
                    .map_err(anyhow::Error::from)?
                    .len()
                    > 262144
                {
                    return Err(Halt::Status(RunStatus::NeedsInput, "Completion evidence exceeds the verification bound; success is not verified.".into()));
                }
                let remaining = limits
                    .max_wall_time
                    .checked_sub(started.elapsed())
                    .ok_or_else(|| {
                        self.limit("wall-time limit before completion verification".into())
                    })?;
                let review = with_cancel(&cancel, async {
                    tokio::time::timeout(
                        remaining,
                        self.provider
                            .verify_completion(&turn, &fresh, &evidence, &stats.evidence),
                    )
                    .await
                    .map_err(|_| {
                        anyhow::anyhow!("completion verification exceeded the run deadline")
                    })?
                })
                .await?;
                let Some(mut review) = review else {
                    return Err(Halt::Status(
                        RunStatus::NeedsInput,
                        turn.assistant_text
                            .clone()
                            .unwrap_or_else(|| "Completion could not be verified.".into()),
                    ));
                };
                stats.usage += review.usage;
                stats.model_latency.push(review.latency_ms);
                if stats.usage.cost_usd > limits.max_cost_usd {
                    return Err(self.limit("cost limit during completion verification".into()));
                }
                let public_reports = crate::types::public_report_ids(&stats.evidence);
                let supported = !review.checks.is_empty()
                    && review.checks.iter().all(|check| {
                        check.verdict == CompletionVerdict::Verified
                            && !check.evidence.trim().is_empty()
                            && !check.frames.is_empty()
                            && check
                                .advisor_ids
                                .iter()
                                .all(|id| public_reports.contains(id))
                            && check
                                .frames
                                .iter()
                                .all(|id| evidence.iter().any(|o| o.frame_seq == *id))
                    });
                if review.verdict == CompletionVerdict::Verified && !supported {
                    review.verdict = CompletionVerdict::Incomplete;
                    review.summary =
                        "Completion is not verified: supporting requirement evidence is missing."
                            .into();
                }
                self.recorder.event(
                    "completion_verification",
                    serde_json::to_value(&review).map_err(anyhow::Error::from)?,
                );
                self.supervisor.completion_review(&review);
                if turn.stop == StopKind::VerifyCompletion
                    && review.verdict == CompletionVerdict::Verified
                    && supported
                {
                    stats.final_text = Some(review.summary);
                    return Ok(());
                }
                // Never automatically recover by replaying a denied or already
                // approved/automatically attempted consequential action (especially an uncertain Send).
                let unsafe_to_repeat = stats.has_replay_barrier();
                if review.verdict == CompletionVerdict::Incomplete
                    && stats.verification_recoveries < 1
                    && !unsafe_to_repeat
                    && stats.steps < limits.max_steps
                {
                    stats.verification_recoveries += 1;
                    self.progress(RunStage::Recovering, stats.steps + 1, stats);
                    obs = fresh;
                    stats.observations.push(obs.clone());
                    self.supervisor
                        .notify("Completion is not verified; continuing with the missing steps.");
                    self.recorder.event("model_request", json!({"step":stats.steps+1,"frame":obs.frame_ref,"reason":"completion_recovery"}));
                    request_started = Instant::now();
                    let remaining = limits
                        .max_wall_time
                        .checked_sub(started.elapsed())
                        .ok_or_else(|| {
                            self.limit("wall-time limit before completion recovery".into())
                        })?;
                    turn = with_cancel(&cancel, async {
                        tokio::time::timeout(
                            remaining,
                            self.provider.replan_after_review(&review, &obs),
                        )
                        .await
                        .map_err(|_| {
                            anyhow::anyhow!("completion recovery exceeded the run deadline")
                        })?
                    })
                    .await?;
                    continue;
                }
                stats.final_text = Some(review.summary.clone());
                return Err(Halt::Status(RunStatus::NeedsInput, review.summary));
            }
            if turn.actions.is_empty() {
                if stats.verification_required || turn.stop == StopKind::Continue {
                    return Err(Halt::Status(
                        RunStatus::NeedsInput,
                        "No verified completion was supplied; the task is not marked finished."
                            .into(),
                    ));
                }
                return Ok(());
            }
            if stats.usage.cost_usd > limits.max_cost_usd {
                return Err(self.limit(format!(
                    "estimated cost ${:.2} exceeds ${:.2}",
                    stats.usage.cost_usd, limits.max_cost_usd
                )));
            }

            let acknowledged = if turn.safety_checks.is_empty() {
                Vec::new()
            } else {
                let automatic = self.policy.mode == crate::policy::Mode::Auto;
                let approved = automatic
                    || blocking(|| self.supervisor.confirm_safety_checks(&turn.safety_checks));
                self.recorder.event(
                    "safety_checks",
                    json!({ "checks": turn.safety_checks, "approved": approved, "automatic": automatic }),
                );
                if !approved {
                    return Err(Halt::Status(
                        RunStatus::SafetyDeclined,
                        "the user declined a provider safety check".into(),
                    ));
                }
                if !automatic {
                    last_action_end = Instant::now();
                    self.platform.reset_human_input_baseline();
                }
                turn.safety_checks.clone()
            };

            if turn.actions.len() > 1
                && turn
                    .actions
                    .iter()
                    .any(|p| matches!(p.action, Action::PublicResearch | Action::UiJudgment))
            {
                return Err(anyhow::anyhow!("Helper requests must be a standalone batch; inspect their result before proposing input").into());
            }
            // Execute strictly in order; stop the batch at the first failure.
            let mut outcomes: Vec<(CallRef, ActionOutcome)> =
                Vec::with_capacity(turn.actions.len());
            let mut halted = false;
            let mut screenshot_last = None;
            self.progress(RunStage::Executing, stats.steps, stats);
            for (index, planned) in turn.actions.iter().enumerate() {
                if halted {
                    self.recorder.event(
                        "action_skipped",
                        json!({ "index": index, "call_ref": planned.call_ref }),
                    );
                    outcomes.push((planned.call_ref.clone(), ActionOutcome::NotExecuted));
                    continue;
                }
                self.check_interrupts(&mut last_action_end)?;
                let outcome = self
                    .execute_one(
                        index,
                        &planned.action,
                        &mut obs,
                        stats,
                        &mut last_action_end,
                        limits.max_wall_time.saturating_sub(started.elapsed()),
                    )
                    .await?;
                last_action_end = Instant::now();
                screenshot_last = match (&planned.action, &outcome) {
                    (Action::Screenshot, ActionOutcome::Ok { image: Some(o), .. }) => {
                        Some(o.as_ref().clone())
                    }
                    _ => None,
                };
                match &outcome {
                    ActionOutcome::Failed(_) => {
                        stats.failed += 1;
                        halted = true;
                    }
                    ActionOutcome::Denied(_) => {
                        stats.denied += 1;
                        halted = true;
                    }
                    _ => {}
                }
                outcomes.push((planned.call_ref.clone(), outcome));
                self.progress(RunStage::Executing, stats.steps, stats);
            }

            if stats.steps >= limits.max_steps {
                return Err(self.limit(format!("reached max steps ({})", limits.max_steps)));
            }
            if started.elapsed() > limits.max_wall_time {
                return Err(self.limit(format!(
                    "reached max wall time ({:?})",
                    limits.max_wall_time
                )));
            }

            // Fresh observation for the results (reuse a trailing screenshot).
            let latest = match screenshot_last {
                Some(o) => o,
                None => self.observe()?,
            };
            obs = latest.clone();
            stats.observations.push(latest.clone());
            self.recorder.event(
                "model_request",
                json!({ "step": stats.steps + 1, "frame": latest.frame_ref }),
            );
            self.progress(RunStage::Planning, stats.steps + 1, stats);
            self.supervisor
                .notify(&format!("Assessing and planning step {}…", stats.steps + 1));
            request_started = Instant::now();
            turn = with_cancel(
                &cancel,
                self.provider.continue_with(outcomes, &latest, acknowledged),
            )
            .await?;
        }
    }

    fn progress(&self, stage: RunStage, step: u32, stats: &Stats) {
        let progress = TaskProgress {
            stage,
            step,
            actions_executed: stats.executed,
        };
        self.recorder.event(
            "task_progress",
            serde_json::to_value(progress).unwrap_or(Value::Null),
        );
        self.supervisor.task_progress(progress);
    }

    fn limit(&self, msg: String) -> Halt {
        self.recorder.event("limit_hit", json!({ "message": msg }));
        Halt::Status(RunStatus::LimitHit, msg)
    }

    fn check_interrupts(&self, last_action_end: &mut Instant) -> Result<(), Halt> {
        if self.cancel.is_cancelled() {
            return Err(Halt::Status(RunStatus::Aborted, "interrupted".into()));
        }
        if blocking(|| self.platform.panic_requested())? {
            return Err(Halt::Status(
                RunStatus::Aborted,
                "panic hotkey pressed".into(),
            ));
        }
        if blocking(|| self.platform.human_input_since(*last_action_end))? {
            let _ = self.platform.release_all();
            self.recorder.event("human_input_pause", json!({}));
            let choice = blocking(|| self.supervisor.human_input_paused());
            self.recorder.event(
                "human_input_resume",
                json!({ "choice": format!("{choice:?}") }),
            );
            if choice == PauseChoice::Abort {
                return Err(Halt::Status(
                    RunStatus::Aborted,
                    "aborted after human input".into(),
                ));
            }
            *last_action_end = Instant::now();
            self.platform.reset_human_input_baseline();
        }
        Ok(())
    }

    fn observe(&self) -> anyhow::Result<Observation> {
        let mut o = blocking(|| self.platform.observe(&self.opts.observe))?;
        o.frame_ref = self.recorder.save_frame(&o.image).ok();
        self.recorder.event(
            "observation",
            json!({
                "frame": o.frame_ref,
                "frame_seq": o.frame_seq,
                "image": [o.image.width, o.image.height],
                "bytes": o.image.bytes.len(),
                "mapping": o.mapping,
                "cursor_i": o.cursor_i,
                "focused_app": o.focused.as_ref().and_then(|f| f.app.as_ref()),
            }),
        );
        Ok(o)
    }

    fn record_turn(&self, turn: &Turn, latency_ms: u64, step: u32) {
        let actions: Vec<Value> = turn
            .actions
            .iter()
            .map(|p| json!({ "call_ref": p.call_ref, "action": p.action }))
            .collect();
        self.recorder.event(
            "model_response",
            json!({
                "step": step,
                "provider": self.provider.name(),
                "model": turn.route.as_ref().map(|r| r.model.as_str()).unwrap_or_else(|| self.provider.model()),
                "route": turn.route,
                "latency_ms": latency_ms,
                "text": turn.assistant_text,
                "thinking": turn.thinking,
                "actions": actions,
                "safety_checks": turn.safety_checks,
                "usage": turn.usage,
                "stop": turn.stop,
                "plan_rejection": turn.plan_rejection,
                "native_apps": turn.native_apps,
                "evidence_frames": turn.evidence_frames,
            }),
        );
    }

    async fn execute_one(
        &mut self,
        index: usize,
        action: &Action,
        obs: &mut Observation,
        stats: &mut Stats,
        last_action_end: &mut Instant,
        remaining: Duration,
    ) -> Result<ActionOutcome, Halt> {
        self.recorder
            .event("action_start", json!({ "index": index, "action": action }));
        let t = Instant::now();
        let mut extra = json!({});
        let outcome = match action {
            Action::PublicResearch | Action::UiJudgment => {
                if self.policy.mode == crate::policy::Mode::Observe {
                    ActionOutcome::Ok {
                        text: Some("Observe mode: no external request was sent.".into()),
                        image: None,
                    }
                } else {
                    if remaining.is_zero() {
                        return Err(self.limit("wall-time limit before helper request".into()));
                    }
                    let kind = if matches!(action, Action::PublicResearch) {
                        crate::types::AdvisorKind::PublicResearch
                    } else {
                        crate::types::AdvisorKind::UiJudgment
                    };
                    let approved = self.policy.mode != crate::policy::Mode::ConfirmAll
                        || {
                            stats.confirmations += 1;
                            blocking(|| {
                                self.supervisor.confirm_safety_checks(&[SafetyCheck {
                            id: "external-helper".into(), code: Some("public_only_helper".into()),
                            message: Some("Allow this public-only helper request? Parallel receives only the separate public brief; Jev receives only host-generated role/boolean metadata. No task text, screenshots, email, recipients, or account identities.".into()),
                        }])
                            })
                        };
                    if self.policy.mode == crate::policy::Mode::ConfirmAll {
                        self.platform.reset_human_input_baseline();
                        *last_action_end = Instant::now();
                    }
                    extra["helper_approved"] = json!(approved);
                    if !approved {
                        ActionOutcome::Denied("The user declined external helper access.".into())
                    } else {
                        // Human approval must not extend the run's wall budget or
                        // resurrect a request after Stop while the prompt was open.
                        if self.cancel.is_cancelled() {
                            return Err(Halt::Status(
                                RunStatus::Aborted,
                                "interrupted before helper request".into(),
                            ));
                        }
                        let remaining = remaining.saturating_sub(t.elapsed());
                        if remaining.is_zero() {
                            return Err(self.limit("wall-time limit after helper approval".into()));
                        }
                        self.progress(
                            if kind == crate::types::AdvisorKind::PublicResearch {
                                RunStage::Researching
                            } else {
                                RunStage::Judging
                            },
                            stats.steps,
                            stats,
                        );
                        let report = with_cancel(&self.cancel, async {
                            tokio::time::timeout(
                                remaining.min(Duration::from_secs(15)),
                                self.provider.advise(kind, obs),
                            )
                            .await
                            .context("External helper deadline exceeded; continue with Luna")?
                        })
                        .await;
                        match report {
                            Ok(report) => {
                                stats.usage += report.usage;
                                self.supervisor.advisor_report(&report);
                                extra["advisor"] = serde_json::to_value(&report).map_err(anyhow::Error::from)?;
                                ActionOutcome::Ok { text: Some(extra["advisor"].to_string()), image: None }
                            }
                            Err(Halt::Status(s,m)) => return Err(Halt::Status(s,m)),
                            Err(_) => ActionOutcome::Failed("External helper unavailable or timed out. Continue with Luna; do not retry a consequential action.".into()),
                        }
                    }
                }
            }
            Action::Screenshot => {
                let o = self.observe()?;
                *obs = o.clone(); // the model now sees this screenshot; map later actions with it
                ActionOutcome::Ok {
                    text: None,
                    image: Some(Box::new(o)),
                }
            }
            Action::Zoom { region } => self.zoom(obs, *region)?,
            Action::CursorPosition => {
                let p = blocking(|| self.platform.cursor_position())?;
                let i = obs.mapping.points_to_image(p);
                extra = json!({ "p": p, "i": i });
                ActionOutcome::Ok {
                    text: Some(format!("X={}, Y={}", i.x.round(), i.y.round())),
                    image: None,
                }
            }
            Action::Wait { duration_ms } => match duration_ms {
                Some(ms) => {
                    let requested = Duration::from_millis(*ms);
                    let d = requested.min(self.opts.limits.max_wait);
                    tokio::select! {
                        _ = tokio::time::sleep(d) => {}
                        _ = self.cancel.cancelled() => return Err(Halt::Status(RunStatus::Aborted, "interrupted".into())),
                    }
                    let text =
                        (d < requested).then(|| format!("OK (wait capped to {}s)", d.as_secs()));
                    ActionOutcome::Ok { text, image: None }
                }
                None => {
                    self.settle(obs, stats)?;
                    ActionOutcome::ok()
                }
            },
            _ => self.execute_input(action, obs, stats, &mut extra, last_action_end)?,
        };
        let mut end = json!({
            "index": index,
            "outcome": outcome.kind(),
            "message": outcome.message(),
            "duration_ms": t.elapsed().as_millis() as u64,
        });
        if let (Value::Object(end), Value::Object(extra)) = (&mut end, extra) {
            end.extend(extra);
        }
        let mut evidence = end.clone();
        evidence["action"] = serde_json::to_value(action).map_err(anyhow::Error::from)?;
        evidence["observation_seq"] = json!(obs.frame_seq);
        stats.evidence.push(evidence);
        self.recorder.event("action_end", end);
        Ok(outcome)
    }

    fn zoom(
        &self,
        obs: &Observation,
        region: crate::action::RectI,
    ) -> anyhow::Result<ActionOutcome> {
        let rect = match obs.mapping.image_rect_to_points(region) {
            Ok(r) => r,
            Err(e) => return Ok(ActionOutcome::Failed(e.to_string())),
        };
        let opts = self.opts.zoom.unwrap_or(self.opts.observe);
        let image = match blocking(|| self.platform.zoom(rect, &opts)) {
            Ok(img) => img,
            Err(e) => return Ok(ActionOutcome::Failed(format!("zoom failed: {e:#}"))),
        };
        let mapping = CoordMapping {
            source: rect,
            image_w: image.width,
            image_h: image.height,
            ..obs.mapping
        };
        let frame_ref = self.recorder.save_frame(&image).ok();
        self.recorder
            .event("zoom", json!({ "frame": frame_ref, "region_points": rect }));
        // The active mapping stays that of the full screenshot (Anthropic docs).
        let zoomed = Observation {
            image,
            mapping,
            frame_seq: obs.frame_seq,
            cursor_i: None,
            focused: None,
            frame_ref,
        };
        Ok(ActionOutcome::Ok {
            text: None,
            image: Some(Box::new(zoomed)),
        })
    }

    fn settle(&self, obs: &Observation, stats: &mut Stats) -> anyhow::Result<()> {
        let opts = SettleOpts {
            after_frame_seq: obs.frame_seq,
            ..self.opts.settle
        };
        let report = blocking(|| self.platform.wait_settled(&opts))?;
        stats.settle.push(report.waited_ms);
        self.recorder.event("settle", serde_json::to_value(report)?);
        Ok(())
    }

    fn execute_input(
        &mut self,
        action: &Action,
        obs: &Observation,
        stats: &mut Stats,
        extra: &mut Value,
        last_action_end: &mut Instant,
    ) -> Result<ActionOutcome, Halt> {
        let mut resolved = match resolve(action, &obs.mapping) {
            Ok(r) => r,
            Err(e) => return Ok(ActionOutcome::Failed(e)),
        };
        let mut note = None;
        if let ResolvedAction::HoldKey { duration_ms, .. } = &mut resolved {
            let cap = self.opts.limits.max_wait.as_millis() as u64;
            if *duration_ms > cap {
                *duration_ms = cap;
                note = Some(format!("OK (hold capped to {}s)", cap / 1000));
            }
        }

        let target = resolved.target();
        if target.is_some() {
            let display = blocking(|| self.platform.display())?;
            if display.generation != obs.mapping.display_generation {
                return Ok(ActionOutcome::Failed(
                    "The display configuration changed since the last screenshot; take a new screenshot.".into(),
                ));
            }
        }
        let element = target.and_then(|p| blocking(|| self.platform.element_at(p)).ok().flatten());
        let focused = blocking(|| self.platform.focused()).ok().flatten();
        let ctx = PolicyContext {
            element: element.as_ref(),
            focused: focused.as_ref(),
            own_terminal_pid: self.opts.own_terminal_pid,
        };
        let (decision, risk) = self.policy.decide(&resolved, &ctx);
        let pointer = target.or_else(|| match &resolved {
            ResolvedAction::Click { .. }
            | ResolvedAction::Scroll { .. }
            | ResolvedAction::MouseButton { .. } => {
                blocking(|| self.platform.cursor_position()).ok()
            }
            _ => None,
        });

        *extra = json!({
            "resolved": resolved,
            "i": action_point(action),
            "p": target,
            "d": target.map(|p| obs.mapping.points_to_device(p)),
            "element": element.as_ref().map(|e| e.label()),
            "risk": match &risk { Risk::Safe => None, Risk::Risky(r) | Risk::Deny(r) => Some(r) },
        });
        self.recorder.event(
            "policy_decision",
            json!({
                "decision": format!("{decision:?}"),
                "risk": match &risk { Risk::Safe => None, Risk::Risky(r) | Risk::Deny(r) => Some(r) },
                "element": element,
                "focused_app": focused.as_ref().and_then(|f| f.app.as_ref()),
            }),
        );

        match decision {
            Decision::Allow => {}
            Decision::Simulate => {
                self.supervisor.pointer_feedback(None);
                self.supervisor
                    .notify(&format!("[observe] would {}", describe_resolved(&resolved)));
                return Ok(ActionOutcome::Ok {
                    text: Some("Observe mode: this action was not executed.".into()),
                    image: None,
                });
            }
            Decision::Deny(reason) => {
                self.supervisor.pointer_feedback(None);
                return Ok(ActionOutcome::Denied(format!(
                    "Denied by safety policy: {reason}."
                )));
            }
            Decision::Confirm(reason) => {
                self.supervisor.pointer_feedback(
                    pointer.map(|point| PointerFeedback::new(point, PointerPhase::Target)),
                );
                stats.confirmations += 1;
                let req = ConfirmRequest {
                    action,
                    resolved: &resolved,
                    reason: &reason,
                    element: element.as_ref(),
                    focused_app: focused.as_ref().and_then(|f| f.app.as_ref()),
                };
                let approved = blocking(|| self.supervisor.confirm_action(&req));
                extra["approved"] = json!(approved);
                self.recorder.event(
                    "confirmation",
                    json!({ "reason": reason, "approved": approved }),
                );
                // The human just used the keyboard/mouse to answer; that is not interference.
                *last_action_end = Instant::now();
                self.platform.reset_human_input_baseline();
                if !approved {
                    self.supervisor.pointer_feedback(None);
                    return Ok(ActionOutcome::Denied(format!(
                        "The user declined this action ({reason})."
                    )));
                }
                // The menu-bar popover can take focus while approving. The UI
                // restores it before replying; independently fail closed here if
                // focus/target identity changed. An approval is not transferable.
                let current_focus = blocking(|| self.platform.focused())?;
                let current_element = match target {
                    Some(p) => blocking(|| self.platform.element_at(p))?,
                    None => None,
                };
                let display_changed = target.is_some()
                    && blocking(|| self.platform.display())?.generation
                        != obs.mapping.display_generation;
                if current_focus != focused || current_element != element || display_changed {
                    self.supervisor.pointer_feedback(None);
                    return Ok(ActionOutcome::Denied(
                        "Focus, target, or display changed during approval; the approved action was not executed. Take a fresh observation.".into(),
                    ));
                }
            }
        }

        if let Action::Type {
            expected_focus: Some(expected),
            ..
        } = action
        {
            if !expected.matches(focused.as_ref()) {
                self.recorder
                    .event("focus_guard", json!({"matched":false,"expected":expected}));
                self.supervisor.pointer_feedback(None);
                return Ok(ActionOutcome::Failed(format!(
                    "Focus guard: expected {} '{}', but the current editable control does not match. No text was posted. Click the intended field explicitly and inspect a fresh observation; do not guess tab order.", expected.role, expected.label)));
            }
            self.recorder
                .event("focus_guard", json!({"matched":true,"expected":expected}));
        }
        if let Some(point) = pointer {
            self.supervisor
                .pointer_feedback(Some(PointerFeedback::new(point, PointerPhase::Active)));
        }
        // Auto mode grants execution, not a replay allowance. Mark before the
        // call because a failed/uncertain post may already have had an effect.
        if matches!(&risk, Risk::Risky(_)) {
            extra["consequential_attempted"] = json!(true);
        }
        if let Err(e) = blocking(|| {
            self.platform.execute_with_feedback(&resolved, &|event| {
                self.supervisor.pointer_feedback(Some(event))
            })
        }) {
            self.supervisor.pointer_feedback(None);
            return Ok(ActionOutcome::Failed(format!(
                "{} failed: {e:#}",
                action.kind()
            )));
        }
        stats.executed += 1;
        if action.changes_state() {
            self.settle(obs, stats)?;
        }
        Ok(ActionOutcome::Ok {
            text: note,
            image: None,
        })
    }
}

fn action_point(a: &Action) -> Option<crate::action::PointI> {
    match a {
        Action::MouseMove { at, .. } => Some(*at),
        Action::Click { at, .. } | Action::Scroll { at, .. } => *at,
        Action::Drag { path, .. } => path.first().copied(),
        _ => None,
    }
}

/// Maps an input action from image space to points. Errors are returned to the model.
pub fn resolve(action: &Action, m: &CoordMapping) -> Result<ResolvedAction, String> {
    let pt = |p| m.image_to_points(p).map_err(|e| e.to_string());
    Ok(match action {
        Action::MouseMove { at, mods } => ResolvedAction::MouseMove {
            at: pt(*at)?,
            mods: *mods,
        },
        Action::Click {
            at,
            button,
            count,
            mods,
        } => {
            if !(1..=3).contains(count) {
                return Err(format!("click count must be 1, 2, or 3 (got {count})"));
            }
            ResolvedAction::Click {
                at: at.map(pt).transpose()?,
                button: *button,
                count: *count,
                mods: *mods,
            }
        }
        Action::MouseDown { button } => ResolvedAction::MouseButton {
            button: *button,
            down: true,
        },
        Action::MouseUp { button } => ResolvedAction::MouseButton {
            button: *button,
            down: false,
        },
        Action::Drag { path, mods } => {
            if path.len() < 2 {
                return Err("drag needs at least 2 points".into());
            }
            ResolvedAction::Drag {
                path: path.iter().map(|p| pt(*p)).collect::<Result<_, _>>()?,
                mods: *mods,
            }
        }
        Action::Scroll { at, delta, mods } => {
            let delta = match *delta {
                ScrollDelta::Pixels { dx, dy } => {
                    let (dx, dy) = m.image_delta_to_points(dx, dy);
                    ScrollDelta::Pixels { dx, dy }
                }
                clicks => clicks,
            };
            ResolvedAction::Scroll {
                at: at.map(pt).transpose()?,
                delta,
                mods: *mods,
            }
        }
        Action::Type { text, .. } => ResolvedAction::Type { text: text.clone() },
        Action::Key { combo, repeat } => {
            if !(1..=100).contains(repeat) {
                return Err(format!("repeat must be 1..=100 (got {repeat})"));
            }
            ResolvedAction::Key {
                combo: *combo,
                repeat: *repeat,
            }
        }
        Action::HoldKey { combo, duration_ms } => ResolvedAction::HoldKey {
            combo: *combo,
            duration_ms: *duration_ms,
        },
        Action::PublicResearch
        | Action::UiJudgment
        | Action::Screenshot
        | Action::Zoom { .. }
        | Action::Wait { .. }
        | Action::CursorPosition => {
            return Err(format!("{} is not an input action", action.kind()))
        }
    })
}

pub fn describe_resolved(a: &ResolvedAction) -> String {
    let at = |p: &Option<crate::action::PointP>| match p {
        Some(p) => format!(" at ({:.1}, {:.1}) pt", p.x, p.y),
        None => " at the cursor".into(),
    };
    let mods = |m: &crate::action::Mods| {
        if m.is_empty() {
            String::new()
        } else {
            format!(" with {}", m.names().join("+"))
        }
    };
    match a {
        ResolvedAction::MouseMove { at: p, .. } => {
            format!("move the mouse to ({:.1}, {:.1}) pt", p.x, p.y)
        }
        ResolvedAction::Click {
            at: p,
            button,
            count,
            mods: m,
        } => {
            let kind = match count {
                2 => "double-click",
                3 => "triple-click",
                _ => "click",
            };
            format!("{button:?} {kind}{}{}", at(p), mods(m)).to_lowercase()
        }
        ResolvedAction::MouseButton { button, down } => {
            format!("{button:?} mouse {}", if *down { "down" } else { "up" }).to_lowercase()
        }
        ResolvedAction::Drag { path, mods: m } => {
            let (a, b) = (path[0], path[path.len() - 1]);
            format!(
                "drag from ({:.1}, {:.1}) to ({:.1}, {:.1}) pt{}",
                a.x,
                a.y,
                b.x,
                b.y,
                mods(m)
            )
        }
        ResolvedAction::Scroll { at: p, delta, .. } => format!("scroll {delta:?}{}", at(p)),
        ResolvedAction::Type { text } => {
            let preview: String = text.chars().take(80).collect();
            let more = if text.chars().count() > 80 { "…" } else { "" };
            format!("type {:?}{more}", preview)
        }
        ResolvedAction::Key { combo, repeat } => {
            if *repeat > 1 {
                format!("press {combo} ×{repeat}")
            } else {
                format!("press {combo}")
            }
        }
        ResolvedAction::HoldKey { combo, duration_ms } => {
            format!("hold {combo} for {duration_ms} ms")
        }
    }
}
