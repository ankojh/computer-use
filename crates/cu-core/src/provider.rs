//! The model-side interface. Each provider owns its conversation state (Anthropic: the full
//! `messages` list; OpenAI: `previous_response_id`), so the agent loop stays provider-agnostic.

use async_trait::async_trait;

use crate::types::{ActionOutcome, CallRef, Observation, SafetyCheck, Turn};

#[async_trait]
pub trait Provider: Send {
    /// Effective host policy for this run; advisory to the planner, never model-set.
    fn set_control_mode(&mut self, _mode: crate::policy::Mode) {}

    /// Start the conversation with the user task and an initial observation.
    async fn start(&mut self, task: &str, obs: &Observation) -> anyhow::Result<Turn>;

    /// Report outcomes for every action of the previous turn (same order) and get the next turn.
    /// `latest` is a fresh observation taken after the batch.
    async fn continue_with(
        &mut self,
        outcomes: Vec<(CallRef, ActionOutcome)>,
        latest: &Observation,
        acknowledged: Vec<SafetyCheck>,
    ) -> anyhow::Result<Turn>;

    /// Explicit host-mediated helpers. No model-supplied outbound text is accepted.
    async fn advise(
        &mut self,
        _kind: crate::types::AdvisorKind,
        _obs: &Observation,
    ) -> anyhow::Result<crate::types::AdvisorReport> {
        anyhow::bail!("Public helpers unavailable; continue with the desktop planner")
    }

    /// Independent, tool-free review of a terminal claim using actual outcomes
    /// and host-selected screenshots. None means verification is unsupported.
    async fn verify_completion(
        &mut self,
        _claim: &Turn,
        _latest: &Observation,
        _evidence: &[Observation],
        _actions: &[serde_json::Value],
    ) -> anyhow::Result<Option<crate::CompletionReview>> {
        Ok(None)
    }

    /// Host-authorized single repair of an entirely rejected, unexecuted reply.
    /// The host denies this after any denied/approved consequential action.
    async fn replan_after_rejection(
        &mut self,
        _rejection: &crate::types::PlanRejection,
        _latest: &Observation,
    ) -> anyhow::Result<Turn> {
        anyhow::bail!("provider does not support safe plan repair")
    }

    /// A failed completion check may request bounded recovery, never authorization.
    async fn replan_after_review(
        &mut self,
        _review: &crate::CompletionReview,
        _latest: &Observation,
    ) -> anyhow::Result<Turn> {
        anyhow::bail!("provider does not support completion recovery")
    }

    fn name(&self) -> &str;

    fn model(&self) -> &str {
        ""
    }
}
