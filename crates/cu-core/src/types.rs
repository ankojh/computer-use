//! Observations, model turns, and action outcomes shared by the loop, providers, and platforms.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::action::{Action, PointI, RectP};
use crate::coords::CoordMapping;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImageFormat {
    #[default]
    Jpeg,
    Png,
}

impl ImageFormat {
    pub fn media_type(self) -> &'static str {
        match self {
            ImageFormat::Jpeg => "image/jpeg",
            ImageFormat::Png => "image/png",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            ImageFormat::Jpeg => "jpg",
            ImageFormat::Png => "png",
        }
    }
}

/// Encoded image bytes, cheap to clone (shared).
#[derive(Debug, Clone)]
pub struct EncodedImage {
    pub bytes: Arc<Vec<u8>>,
    pub format: ImageFormat,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppInfo {
    pub pid: i32,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub bundle_id: Option<String>,
}

/// One accessibility element, as returned by the Swift layer.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct AxElement {
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub subrole: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub help: Option<String>,
    #[serde(default)]
    pub identifier: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub secure: bool,
    #[serde(default)]
    pub frame_points: Option<RectP>,
    #[serde(default)]
    pub app: Option<AppInfo>,
}

impl AxElement {
    /// "AXButton 'Delete'" style label for logs and confirmation prompts.
    pub fn label(&self) -> String {
        let role = self.role.as_deref().unwrap_or("?");
        let name = self
            .title
            .as_deref()
            .or(self.description.as_deref())
            .unwrap_or("");
        let app = self
            .app
            .as_ref()
            .and_then(|a| a.name.as_deref())
            .map(|n| format!(" in {n}"))
            .unwrap_or_default();
        if name.is_empty() {
            format!("{role}{app}")
        } else {
            format!("{role} '{name}'{app}")
        }
    }

    /// All human-readable text on the element, for risk matching.
    pub fn texts(&self) -> impl Iterator<Item = &str> {
        [&self.title, &self.description, &self.help, &self.identifier]
            .into_iter()
            .filter_map(|s| s.as_deref())
    }
}

/// Exact focused-control expectation for typing. AX text is untrusted: this
/// prevents accidental wrong-field entry, not malicious UI or sender spoofing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedFocus {
    pub role: String,
    pub label: String,
}

impl ExpectedFocus {
    pub fn valid(&self) -> bool {
        matches!(
            self.role.as_str(),
            "AXTextField" | "AXTextArea" | "AXComboBox" | "AXSearchField"
        ) && !self.label.trim().is_empty()
            && self.label.len() <= 800
    }

    pub fn matches(&self, focused: Option<&AxSummary>) -> bool {
        let Some(focus) = focused else { return false };
        let Some(element) = focus.element.as_ref() else {
            return false;
        };
        self.valid() && !focus.secure_input && !element.secure && element.enabled != Some(false)
            && element.role.as_deref() == Some(self.role.as_str())
            // Never match a value/help string, substrings, or inferred tab order.
            && [&element.title, &element.description, &element.identifier].iter()
                .filter_map(|s| s.as_deref())
                .any(|s| s.trim().eq_ignore_ascii_case(self.label.trim()))
    }
}

/// Focused app/window/element summary.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct AxSummary {
    #[serde(default)]
    pub app: Option<AppInfo>,
    #[serde(default)]
    pub window_title: Option<String>,
    #[serde(default)]
    pub element: Option<AxElement>,
    #[serde(default)]
    pub secure_input: bool,
}

impl AxSummary {
    pub fn bundle_id(&self) -> Option<&str> {
        self.app.as_ref().and_then(|a| a.bundle_id.as_deref())
    }
}

#[derive(Debug, Clone)]
pub struct Observation {
    pub image: EncodedImage,
    pub mapping: CoordMapping,
    pub frame_seq: u64,
    pub cursor_i: Option<PointI>,
    pub focused: Option<AxSummary>,
    /// Run-folder path of the saved frame (set by the recorder).
    pub frame_ref: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdvisorKind {
    PublicResearch,
    UiJudgment,
}

/// Data returned by a host-owned helper, never input authority. All source text
/// and model judgments are untrusted. Only successful public research can be cited.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvisorReport {
    pub id: String,
    pub kind: AdvisorKind,
    pub provider: String,
    pub status: String,
    pub summary: String,
    pub latency_ms: u64,
    pub requests_used: u32,
    pub sources: Vec<PublicSource>,
    pub data: serde_json::Value,
    pub usage: Usage,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicSource {
    pub url: String,
    pub title: String,
    pub excerpts: Vec<String>,
}
impl AdvisorReport {
    pub fn valid_public_evidence(&self) -> bool {
        self.kind == AdvisorKind::PublicResearch
            && self.provider == "parallel"
            && self.status == "ok"
            && !self.id.is_empty()
            && !self.sources.is_empty()
            && self.sources.len() <= 6
            && self.sources.iter().all(|s| !s.excerpts.is_empty())
    }
}

/// Successful public-source reports actually recorded by the host. A planner
/// cannot fabricate an ID or cite a Jev judgment as evidence of delivery/research.
pub fn public_report_ids(actions: &[serde_json::Value]) -> Vec<String> {
    actions
        .iter()
        .filter(|e| e["outcome"] == "ok" && e["action"]["type"] == "public_research")
        .filter_map(|e| serde_json::from_value::<AdvisorReport>(e["advisor"].clone()).ok())
        .filter(AdvisorReport::valid_public_evidence)
        .map(|r| r.id)
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeAppInfo {
    pub name: String,
    pub bundle_id: String,
}

/// Sanitized host validation diagnostics, never the rejected reply/typed text.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanRejection {
    pub code: String,
    pub summary: String,
}

/// Provider's identifier for one action (Anthropic `tool_use_id`, OpenAI `call_id` + index).
pub type CallRef = String;

#[derive(Debug, Clone, PartialEq)]
pub struct PlannedAction {
    pub call_ref: CallRef,
    pub action: Action,
}

#[derive(Debug, Clone)]
pub enum ActionOutcome {
    Ok {
        text: Option<String>,
        image: Option<Box<Observation>>,
    },
    Failed(String),
    NotExecuted,
    Denied(String),
}

impl ActionOutcome {
    pub fn ok() -> Self {
        ActionOutcome::Ok {
            text: None,
            image: None,
        }
    }

    pub fn is_ok(&self) -> bool {
        matches!(self, ActionOutcome::Ok { .. })
    }

    pub fn kind(&self) -> &'static str {
        match self {
            ActionOutcome::Ok { .. } => "ok",
            ActionOutcome::Failed(_) => "failed",
            ActionOutcome::NotExecuted => "not_executed",
            ActionOutcome::Denied(_) => "denied",
        }
    }

    pub fn message(&self) -> Option<&str> {
        match self {
            ActionOutcome::Ok { text, .. } => text.as_deref(),
            ActionOutcome::Failed(m) | ActionOutcome::Denied(m) => Some(m),
            ActionOutcome::NotExecuted => None,
        }
    }
}

/// OpenAI `pending_safety_checks` entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SafetyCheck {
    pub id: String,
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    /// Estimated by the provider from its price table.
    pub cost_usd: f64,
}

impl std::ops::AddAssign for Usage {
    fn add_assign(&mut self, o: Usage) {
        self.input_tokens += o.input_tokens;
        self.output_tokens += o.output_tokens;
        self.cache_read_tokens += o.cache_read_tokens;
        self.cache_write_tokens += o.cache_write_tokens;
        self.cost_usd += o.cost_usd;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopKind {
    /// The model finished (no actions).
    #[default]
    Done,
    /// More actions remain; never a completion claim.
    Continue,
    /// Completion is claimed, but the engine must verify it before success.
    VerifyCompletion,
    /// The task is blocked, not successfully completed.
    NeedsInput,
    /// Entire reply rejected before execution. Host may permit ONE bounded repair.
    InvalidPlan,
    /// The model refused; stop with its message.
    Refusal,
}

/// Visual feedback only; never authorizes an action or controls the OS pointer.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PointerPhase {
    Target,
    Active,
    Complete,
    /// Emitted after a native tagged mouse event is posted (not a UI receipt).
    Posted,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PointerKind {
    #[default]
    Action,
    Move,
    Drag,
    ButtonDown,
    ButtonUp,
    /// A posted up event within a Click action, not a drag/button release.
    Click,
    Scroll,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PointerFeedback {
    /// Resolved global Quartz points, NOT screenshot pixels or device pixels.
    pub point: crate::PointP,
    pub phase: PointerPhase,
    #[serde(default)]
    pub kind: PointerKind,
    #[serde(default)]
    pub sequence: u64,
    #[serde(default)]
    pub button: Option<crate::Button>,
    #[serde(default)]
    pub count: u8,
}

impl PointerFeedback {
    pub fn new(point: crate::PointP, phase: PointerPhase) -> Self {
        static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Self {
            point,
            phase,
            kind: PointerKind::Action,
            sequence: SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            button: None,
            count: 0,
        }
    }
}

/// Actual planner selection for this turn, plus the separate classifier cost.
/// The reason is a short routing explanation, not private chain-of-thought.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRoute {
    pub model: String,
    pub reasoning: String,
    pub tier: String,
    pub reason: String,
    pub router_model: Option<String>,
    pub router_reasoning: Option<String>,
    pub router_latency_ms: u64,
    pub planner_latency_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionVerdict {
    Verified,
    Incomplete,
    NeedsInput,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompletionCheck {
    pub requirement: String,
    pub verdict: CompletionVerdict,
    pub evidence: String,
    pub frames: Vec<u64>,
    /// Only host-recorded successful Parallel public-source report IDs, never Jev judgments.
    #[serde(default)]
    pub advisor_ids: Vec<String>,
}

/// Brief evidence-based findings, never a private reasoning trace or an action plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionReview {
    pub verdict: CompletionVerdict,
    pub summary: String,
    pub checks: Vec<CompletionCheck>,
    pub model: String,
    pub reasoning: String,
    pub latency_ms: u64,
    pub usage: Usage,
}

#[derive(Debug, Clone, Default)]
pub struct Turn {
    pub plan_rejection: Option<PlanRejection>,
    pub native_apps: Vec<NativeAppInfo>,
    pub route: Option<ModelRoute>,
    /// Host-recorded observation sequence IDs offered as completion evidence.
    pub evidence_frames: Vec<u64>,
    /// In order; empty alone does not establish completion for verified providers.
    pub actions: Vec<PlannedAction>,
    pub assistant_text: Option<String>,
    pub thinking: Option<String>,
    pub safety_checks: Vec<SafetyCheck>,
    pub usage: Usage,
    pub stop: StopKind,
}

#[cfg(test)]
mod focus_tests {
    use super::*;
    #[test]
    fn expected_focus_requires_editable_role_and_nonempty_exact_label() {
        for role in ["AXButton", "AXGroup", "", "AXSecureTextField"] {
            assert!(!ExpectedFocus {
                role: role.into(),
                label: "Subject".into()
            }
            .valid());
        }
        assert!(!ExpectedFocus {
            role: "AXTextField".into(),
            label: " ".into()
        }
        .valid());
        let expected = ExpectedFocus {
            role: "AXTextField".into(),
            label: "Subject".into(),
        };
        assert!(!expected.matches(None));
        let mut focus = AxSummary {
            element: Some(AxElement {
                role: Some("AXTextField".into()),
                title: Some("Subject".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(expected.matches(Some(&focus)));
        focus.element.as_mut().unwrap().role = Some("AXTextArea".into());
        assert!(!expected.matches(Some(&focus)));
        focus.element.as_mut().unwrap().role = Some("AXTextField".into());
        focus.element.as_mut().unwrap().enabled = Some(false);
        assert!(!expected.matches(Some(&focus)));
        focus.element.as_mut().unwrap().enabled = Some(true);
        focus.secure_input = true;
        assert!(!expected.matches(Some(&focus)));
    }
    #[test]
    fn repeated_pointer_positions_have_distinct_sequences() {
        let point = crate::PointP { x: -20., y: 100. };
        let first = PointerFeedback::new(point, PointerPhase::Posted);
        let second = PointerFeedback::new(point, PointerPhase::Posted);
        assert!(second.sequence > first.sequence);
        assert_eq!(first.point, second.point);
    }
}
