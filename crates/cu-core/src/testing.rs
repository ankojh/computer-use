//! In-memory platform for agent-loop tests: records actions instead of posting events.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crate::action::{PointP, RectP, ResolvedAction};
use crate::coords::{fit_size, CoordMapping};
use crate::platform::{DisplayInfo, ObserveOpts, Platform, SettleOpts, SettleReport};
use crate::types::{AxElement, AxSummary, EncodedImage, ImageFormat, Observation};

pub struct FakePlatform {
    pub display: Mutex<DisplayInfo>,
    pub executed: Mutex<Vec<ResolvedAction>>,
    /// `execute` call index (0-based) → error message.
    pub fail_on: Mutex<HashMap<usize, String>>,
    /// Report human input before these `execute` call indices.
    pub human_input_before: Mutex<Vec<usize>>,
    /// Simulate a display reconfiguration right after this `execute` call index.
    pub bump_generation_after: Mutex<Option<usize>>,
    pub elements: Mutex<Vec<(RectP, AxElement)>>,
    pub focused: Mutex<Option<AxSummary>>,
    pub cursor: Mutex<PointP>,
    pub releases: AtomicUsize,
    pub observations: AtomicU64,
    pub settles: AtomicUsize,
    pub panic: Mutex<bool>,
    calls: AtomicUsize,
}

impl Default for FakePlatform {
    fn default() -> Self {
        Self::new(
            RectP {
                x: 0.0,
                y: 0.0,
                w: 1728.0,
                h: 1117.0,
            },
            2.0,
        )
    }
}

impl FakePlatform {
    pub fn new(bounds: RectP, scale: f64) -> Self {
        FakePlatform {
            display: Mutex::new(DisplayInfo {
                display_id: 1,
                bounds,
                pixel_w: (bounds.w * scale) as u32,
                pixel_h: (bounds.h * scale) as u32,
                scale,
                generation: 0,
            }),
            executed: Mutex::default(),
            fail_on: Mutex::default(),
            human_input_before: Mutex::default(),
            bump_generation_after: Mutex::default(),
            elements: Mutex::default(),
            focused: Mutex::default(),
            cursor: Mutex::new(PointP { x: 0.0, y: 0.0 }),
            releases: AtomicUsize::new(0),
            observations: AtomicU64::new(0),
            settles: AtomicUsize::new(0),
            panic: Mutex::new(false),
            calls: AtomicUsize::new(0),
        }
    }

    pub fn executed(&self) -> Vec<ResolvedAction> {
        self.executed.lock().unwrap().clone()
    }

    fn image(w: u32, h: u32) -> EncodedImage {
        EncodedImage {
            bytes: Arc::new(b"fake-png".to_vec()),
            format: ImageFormat::Png,
            width: w,
            height: h,
        }
    }
}

impl Platform for FakePlatform {
    fn display(&self) -> anyhow::Result<DisplayInfo> {
        Ok(*self.display.lock().unwrap())
    }

    fn observe(&self, opts: &ObserveOpts) -> anyhow::Result<Observation> {
        let d = self.display()?;
        let seq = self.observations.fetch_add(1, Ordering::SeqCst) + 1;
        let (w, h) = fit_size(d.pixel_w, d.pixel_h, opts.max_long_edge);
        let mut mapping = CoordMapping::full_display(d.bounds, w, h, d.scale);
        mapping.display_generation = d.generation;
        mapping.frame_seq = seq;
        let cursor = *self.cursor.lock().unwrap();
        Ok(Observation {
            image: Self::image(w, h),
            cursor_i: Some(mapping.points_to_image(cursor)),
            mapping,
            frame_seq: seq,
            focused: self.focused.lock().unwrap().clone(),
            frame_ref: None,
        })
    }

    fn zoom(&self, region: RectP, opts: &ObserveOpts) -> anyhow::Result<EncodedImage> {
        let d = self.display()?;
        let (w, h) = fit_size(
            (region.w * d.scale) as u32,
            (region.h * d.scale) as u32,
            opts.max_long_edge,
        );
        Ok(Self::image(w, h))
    }

    fn execute(&self, action: &ResolvedAction) -> anyhow::Result<()> {
        let n = self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some(msg) = self.fail_on.lock().unwrap().get(&n) {
            anyhow::bail!("{msg}");
        }
        if let Some(p) = action.final_cursor() {
            *self.cursor.lock().unwrap() = p;
        }
        self.executed.lock().unwrap().push(action.clone());
        if *self.bump_generation_after.lock().unwrap() == Some(n) {
            self.display.lock().unwrap().generation += 1;
        }
        Ok(())
    }

    fn wait_settled(&self, _opts: &SettleOpts) -> anyhow::Result<SettleReport> {
        self.settles.fetch_add(1, Ordering::SeqCst);
        Ok(SettleReport {
            waited_ms: 0,
            timed_out: false,
            fixed: false,
            settled_seq: None,
        })
    }

    fn cursor_position(&self) -> anyhow::Result<PointP> {
        Ok(*self.cursor.lock().unwrap())
    }

    fn element_at(&self, p: PointP) -> anyhow::Result<Option<AxElement>> {
        Ok(self
            .elements
            .lock()
            .unwrap()
            .iter()
            .find(|(r, _)| r.contains(p))
            .map(|(_, e)| e.clone()))
    }

    fn focused(&self) -> anyhow::Result<Option<AxSummary>> {
        Ok(self.focused.lock().unwrap().clone())
    }

    fn human_input_since(&self, _t: Instant) -> anyhow::Result<bool> {
        let n = self.calls.load(Ordering::SeqCst);
        let mut list = self.human_input_before.lock().unwrap();
        if let Some(pos) = list.iter().position(|&i| i == n) {
            list.remove(pos);
            return Ok(true);
        }
        Ok(false)
    }

    fn panic_requested(&self) -> anyhow::Result<bool> {
        Ok(*self.panic.lock().unwrap())
    }

    fn release_all(&self) -> anyhow::Result<()> {
        self.releases.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}
