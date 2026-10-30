//! The interface the agent loop uses to observe and drive a machine (implemented by `cu-mac`).

use std::time::Instant;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::action::{PointI, PointP, RectP, ResolvedAction};
use crate::types::{AxElement, AxSummary, EncodedImage, ImageFormat, Observation};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DisplayInfo {
    pub display_id: u32,
    pub bounds: RectP,
    pub pixel_w: u32,
    pub pixel_h: u32,
    pub scale: f64,
    pub generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ObserveOpts {
    pub max_long_edge: u32,
    pub format: ImageFormat,
    pub jpeg_quality: f32,
    /// Also query the focused app/element (AX).
    pub include_focused: bool,
}

impl Default for ObserveOpts {
    fn default() -> Self {
        ObserveOpts {
            max_long_edge: 1440,
            format: ImageFormat::Jpeg,
            jpeg_quality: 0.78,
            include_focused: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SettleOpts {
    pub quiet_ms: u32,
    pub timeout_ms: u32,
    /// Fraction of tiles that may change while still counting as "quiet".
    pub threshold: f32,
    pub after_frame_seq: u64,
    /// Fixed wait used when the platform has no change detection yet.
    pub fallback_ms: u32,
}

impl Default for SettleOpts {
    fn default() -> Self {
        SettleOpts {
            quiet_ms: 150,
            timeout_ms: 2000,
            threshold: 0.002,
            after_frame_seq: 0,
            fallback_ms: 300,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SettleReport {
    pub waited_ms: u64,
    pub timed_out: bool,
    /// `true` when change detection was unavailable and a fixed sleep was used.
    pub fixed: bool,
    pub settled_seq: Option<u64>,
}

/// Blocking platform calls; the agent invokes them via `block_in_place`.
pub trait Platform: Send + Sync {
    fn display(&self) -> anyhow::Result<DisplayInfo>;
    fn observe(&self, opts: &ObserveOpts) -> anyhow::Result<Observation>;
    /// Crops `region` (points) at native resolution and fits it into `max_long_edge`.
    fn zoom(&self, region: RectP, opts: &ObserveOpts) -> anyhow::Result<EncodedImage>;
    /// Executes an action already mapped to points.
    fn execute(&self, action: &ResolvedAction) -> anyhow::Result<()>;
    /// Observe only this execution's posted pointer events. No callback may
    /// authorize or inject input. Native platforms override with event-level data.
    fn execute_with_feedback(
        &self,
        action: &ResolvedAction,
        feedback: &dyn Fn(crate::PointerFeedback),
    ) -> anyhow::Result<()> {
        self.execute(action)?;
        if let Some(point) = action.final_cursor() {
            let mut event = crate::PointerFeedback::new(point, crate::PointerPhase::Complete);
            event.kind = match action {
                ResolvedAction::Click { button, count, .. } => {
                    event.button = Some(*button);
                    event.count = *count;
                    crate::types::PointerKind::Click
                }
                ResolvedAction::MouseMove { .. } => crate::types::PointerKind::Move,
                ResolvedAction::Drag { .. } => crate::types::PointerKind::Drag,
                _ => crate::types::PointerKind::Action,
            };
            feedback(event);
        }
        Ok(())
    }
    fn wait_settled(&self, opts: &SettleOpts) -> anyhow::Result<SettleReport>;
    fn cursor_position(&self) -> anyhow::Result<PointP>;
    fn element_at(&self, p: PointP) -> anyhow::Result<Option<AxElement>>;
    fn focused(&self) -> anyhow::Result<Option<AxSummary>>;
    /// Whether the human touched the mouse/keyboard since `t`. Reporting `true` resets the baseline.
    fn human_input_since(&self, t: Instant) -> anyhow::Result<bool>;
    /// Forget any input seen so far (after the human answered a prompt or resumed a pause).
    fn reset_human_input_baseline(&self) {}
    fn panic_requested(&self) -> anyhow::Result<bool>;
    /// Releases every held button/key and interrupts in-flight input operations.
    fn release_all(&self) -> anyhow::Result<()>;
}

/// Extension point for later (OCR / AX / MLX grounding). Not implemented in v1.
#[async_trait]
pub trait Grounder: Send + Sync {
    async fn locate(&self, obs: &Observation, query: &str) -> anyhow::Result<Option<PointI>>;
}
