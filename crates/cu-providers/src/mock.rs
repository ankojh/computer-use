//! Scripted provider: replays a list of turns from JSON. Used to test the executor, policy, and
//! recorder without network or cost, and to reproduce a run deterministically.
//!
//! ```json
//! {
//!   "name": "demo",
//!   "turns": [
//!     {"text": "Clicking the button", "actions": [{"type": "click", "at": {"x": 100, "y": 200}}]},
//!     {"actions": [{"type": "type", "text": "hello"}, {"type": "screenshot"}]}
//!   ],
//!   "final_text": "All done."
//! }
//! ```
//!
//! Coordinates are in image space of the screenshot the mock "saw", like a real model's.

use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::Context;
use async_trait::async_trait;
use cu_core::{
    Action, ActionOutcome, CallRef, Observation, PlannedAction, Provider, SafetyCheck, StopKind,
    Turn,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Script {
    #[serde(default)]
    pub repair_delay_ms: u64,
    #[serde(default)]
    pub name: Option<String>,
    pub turns: Vec<ScriptTurn>,
    #[serde(default)]
    pub final_text: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScriptTurn {
    #[serde(default)]
    pub plan_rejection: Option<cu_core::types::PlanRejection>,
    #[serde(default)]
    pub usage: cu_core::Usage,
    #[serde(default)]
    pub advisor: Option<cu_core::types::AdvisorReport>,
    #[serde(default)]
    pub stop: StopKind,
    #[serde(default)]
    pub evidence_frames: Vec<u64>,
    #[serde(default)]
    pub verification: Option<cu_core::CompletionReview>,
    #[serde(default)]
    pub route: Option<cu_core::ModelRoute>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub actions: Vec<Action>,
    #[serde(default)]
    pub safety_checks: Vec<SafetyCheck>,
    #[serde(default)]
    pub refusal: bool,
}

impl Script {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let s =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        serde_json::from_str(&s).with_context(|| format!("parsing mock script {}", path.display()))
    }
}

/// What the provider was told after each turn (for assertions).
#[derive(Debug, Clone)]
pub struct Received {
    pub outcomes: Vec<(CallRef, String, Option<String>)>,
    pub latest_frame_seq: u64,
    pub acknowledged: Vec<String>,
}

pub struct MockProvider {
    script: Script,
    next: usize,
    task: Option<String>,
    pub received: Arc<Mutex<Vec<Received>>>,
    pub helper_calls: Arc<Mutex<Vec<cu_core::types::AdvisorKind>>>,
    pub repair_calls: Arc<Mutex<Vec<(String, u64)>>>,
}

impl MockProvider {
    pub fn new(script: Script) -> Self {
        MockProvider {
            script,
            next: 0,
            task: None,
            received: Arc::default(),
            helper_calls: Arc::default(),
            repair_calls: Arc::default(),
        }
    }

    pub fn task(&self) -> Option<&str> {
        self.task.as_deref()
    }

    fn next_turn(&mut self) -> Turn {
        let i = self.next;
        self.next += 1;
        let Some(t) = self.script.turns.get(i) else {
            return Turn {
                assistant_text: self.script.final_text.clone(),
                ..Default::default()
            };
        };
        Turn {
            plan_rejection: t.plan_rejection.clone(),
            native_apps: Vec::new(),
            actions: t
                .actions
                .iter()
                .enumerate()
                .map(|(j, a)| PlannedAction {
                    call_ref: format!("mock-{i}-{j}"),
                    action: a.clone(),
                })
                .collect(),
            assistant_text: t.text.clone(),
            evidence_frames: t.evidence_frames.clone(),
            thinking: None,
            route: t.route.clone(),
            safety_checks: t.safety_checks.clone(),
            usage: t.usage,
            stop: if t.refusal { StopKind::Refusal } else { t.stop },
        }
    }
}

#[async_trait]
impl Provider for MockProvider {
    async fn advise(
        &mut self,
        kind: cu_core::types::AdvisorKind,
        _: &Observation,
    ) -> anyhow::Result<cu_core::types::AdvisorReport> {
        self.helper_calls.lock().unwrap().push(kind);
        self.script
            .turns
            .get(self.next.saturating_sub(1))
            .and_then(|t| t.advisor.clone())
            .context("mock helper unavailable")
    }
    async fn start(&mut self, task: &str, _obs: &Observation) -> anyhow::Result<Turn> {
        self.task = Some(task.to_string());
        Ok(self.next_turn())
    }

    async fn continue_with(
        &mut self,
        outcomes: Vec<(CallRef, ActionOutcome)>,
        latest: &Observation,
        acknowledged: Vec<SafetyCheck>,
    ) -> anyhow::Result<Turn> {
        self.received.lock().unwrap().push(Received {
            outcomes: outcomes
                .iter()
                .map(|(r, o)| {
                    (
                        r.clone(),
                        o.kind().to_string(),
                        o.message().map(String::from),
                    )
                })
                .collect(),
            latest_frame_seq: latest.frame_seq,
            acknowledged: acknowledged.into_iter().map(|c| c.id).collect(),
        });
        Ok(self.next_turn())
    }

    async fn verify_completion(
        &mut self,
        _: &Turn,
        _: &Observation,
        _: &[Observation],
        _: &[serde_json::Value],
    ) -> anyhow::Result<Option<cu_core::CompletionReview>> {
        Ok(self
            .script
            .turns
            .get(self.next.saturating_sub(1))
            .and_then(|t| t.verification.clone()))
    }
    async fn replan_after_rejection(
        &mut self,
        rejection: &cu_core::types::PlanRejection,
        latest: &Observation,
    ) -> anyhow::Result<Turn> {
        self.repair_calls
            .lock()
            .unwrap()
            .push((rejection.code.clone(), latest.frame_seq));
        if self.script.repair_delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(
                self.script.repair_delay_ms,
            ))
            .await;
        }
        Ok(self.next_turn())
    }
    async fn replan_after_review(
        &mut self,
        _: &cu_core::CompletionReview,
        _: &Observation,
    ) -> anyhow::Result<Turn> {
        Ok(self.next_turn())
    }
    fn name(&self) -> &str {
        "mock"
    }

    fn model(&self) -> &str {
        self.script.name.as_deref().unwrap_or("script")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_script_parses() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/observe-demo.json");
        let script = Script::load(&path).unwrap();
        assert_eq!(script.turns.len(), 2);
        assert_eq!(script.turns[0].actions.len(), 4);
    }
}
