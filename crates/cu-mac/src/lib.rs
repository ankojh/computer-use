//! Safe wrapper over `cu-mac-sys`, implementing `cu_core::Platform` for the main display.

use std::ffi::CString;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use cu_core::platform::{DisplayInfo, ObserveOpts, Platform, SettleOpts, SettleReport};
use cu_core::{
    AxElement, AxSummary, Button, CoordMapping, EncodedImage, ImageFormat, KeyCombo, Mods,
    Observation, PointP, RectP, ResolvedAction, ScrollDelta,
};
use cu_mac_sys as sys;
use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
#[error("{message} (cu status {code})")]
pub struct MacError {
    pub code: i32,
    pub message: String,
}

impl MacError {
    pub fn is_unimplemented(&self) -> bool {
        self.code == sys::CU_ERR_UNIMPLEMENTED
    }

    pub fn is_permission(&self) -> bool {
        self.code == sys::CU_ERR_PERMISSION
    }
}

pub type Result<T, E = MacError> = std::result::Result<T, E>;

/// Owned Swift-allocated buffer.
struct Buf(sys::CuBuf);

impl Buf {
    fn new() -> Self {
        Buf(sys::CuBuf::default())
    }

    fn bytes(&self) -> &[u8] {
        if self.0.ptr.is_null() {
            &[]
        } else {
            // SAFETY: Swift allocated `len` bytes at `ptr` and transferred ownership to us.
            unsafe { std::slice::from_raw_parts(self.0.ptr, self.0.len) }
        }
    }
}

impl Drop for Buf {
    fn drop(&mut self) {
        if !self.0.ptr.is_null() {
            // SAFETY: allocated by Swift's malloc; released exactly once.
            unsafe { sys::cu_buf_free(std::mem::take(&mut self.0)) }
        }
    }
}

fn last_error() -> String {
    let mut b = Buf::new();
    // SAFETY: valid out pointer.
    if unsafe { sys::cu_last_error(&mut b.0) } != sys::CU_OK {
        return "unknown error".into();
    }
    String::from_utf8_lossy(b.bytes()).into_owned()
}

fn check(status: i32) -> Result<()> {
    if status == sys::CU_OK {
        Ok(())
    } else {
        Err(MacError {
            code: status,
            message: last_error(),
        })
    }
}

fn json<T: for<'de> Deserialize<'de>>(b: &Buf) -> Result<T> {
    serde_json::from_slice(b.bytes()).map_err(|e| MacError {
        code: sys::CU_ERR,
        message: format!("bad JSON from CuMac: {e}"),
    })
}

fn cstring(s: &str) -> Result<CString> {
    CString::new(s).map_err(|_| MacError {
        code: sys::CU_ERR_INVALID_ARG,
        message: "text contains a NUL byte".into(),
    })
}

struct PointerCallbackContext<'a> {
    callback: &'a dyn Fn(cu_core::PointerFeedback),
    is_click: bool,
}

unsafe extern "C" fn pointer_callback(
    context: *mut std::ffi::c_void,
    kind: i32,
    x: f64,
    y: f64,
    button: i32,
    count: i32,
) {
    if context.is_null() || !x.is_finite() || !y.is_finite() {
        return;
    }
    // SAFETY: installed only for a synchronous execution on this same thread;
    // the scope clears Swift's thread-local callback before context is dropped.
    let context = unsafe { &*(context as *const PointerCallbackContext<'_>) };
    let mut feedback =
        cu_core::PointerFeedback::new(PointP { x, y }, cu_core::PointerPhase::Posted);
    feedback.kind = match kind {
        sys::CU_POINTER_MOVE => cu_core::types::PointerKind::Move,
        sys::CU_POINTER_DRAG => cu_core::types::PointerKind::Drag,
        sys::CU_POINTER_DOWN => cu_core::types::PointerKind::ButtonDown,
        sys::CU_POINTER_UP => cu_core::types::PointerKind::ButtonUp,
        _ => return,
    };
    if context.is_click && feedback.kind == cu_core::types::PointerKind::ButtonUp {
        feedback.kind = cu_core::types::PointerKind::Click;
    }
    feedback.button = match button {
        0 => Some(Button::Left),
        1 => Some(Button::Right),
        2 => Some(Button::Middle),
        3 => Some(Button::Back),
        4 => Some(Button::Forward),
        _ => None,
    };
    feedback.count = u8::try_from(count).unwrap_or(0);
    // Visual feedback is not input authority. Never unwind through the C ABI.
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        (context.callback)(feedback)
    }));
}

struct PointerCallbackScope;
impl Drop for PointerCallbackScope {
    fn drop(&mut self) {
        // SAFETY: clear only this thread's synchronous observer.
        unsafe { sys::cu_set_pointer_callback(None, std::ptr::null_mut()) };
    }
}

pub fn abi_version() -> i32 {
    // SAFETY: no arguments.
    unsafe { sys::cu_abi_version() }
}

/// Checks the ABI version and initializes the Swift layer. Must run on the process main thread.
pub fn init() -> Result<()> {
    let v = abi_version();
    if v != sys::CU_ABI_VERSION {
        return Err(MacError {
            code: sys::CU_ERR,
            message: format!(
                "CuMac ABI version {v} does not match the Rust bindings ({})",
                sys::CU_ABI_VERSION
            ),
        });
    }
    // SAFETY: no arguments; Swift verifies it is on the main thread.
    check(unsafe { sys::cu_init() })
}

#[derive(Debug, Clone)]
pub struct Permissions {
    pub screen_recording: bool,
    pub accessibility: bool,
    pub input_monitoring: bool,
    pub post_events: bool,
    pub details: serde_json::Value,
}

pub fn permissions() -> Result<Permissions> {
    let mut mask = 0u32;
    let mut b = Buf::new();
    // SAFETY: valid out pointers.
    check(unsafe { sys::cu_permissions(&mut mask, &mut b.0) })?;
    Ok(Permissions {
        screen_recording: mask & sys::CU_PERM_SCREEN != 0,
        accessibility: mask & sys::CU_PERM_ACCESSIBILITY != 0,
        input_monitoring: mask & sys::CU_PERM_LISTEN != 0,
        post_events: mask & sys::CU_PERM_POST != 0,
        details: json(&b)?,
    })
}

pub fn request_permissions(mask: u32) -> Result<()> {
    // SAFETY: plain value.
    check(unsafe { sys::cu_request_permissions(mask) })
}

fn rect(r: sys::CuRect) -> RectP {
    RectP {
        x: r.x,
        y: r.y,
        w: r.w,
        h: r.h,
    }
}

pub fn display_info() -> Result<DisplayInfo> {
    let mut d = sys::CuDisplayInfo::default();
    // SAFETY: valid out pointer.
    check(unsafe { sys::cu_display_info(&mut d) })?;
    Ok(DisplayInfo {
        display_id: d.display_id,
        bounds: rect(d.bounds_points),
        pixel_w: d.pixel_w,
        pixel_h: d.pixel_h,
        scale: d.scale,
        generation: d.generation,
    })
}

pub fn cursor() -> Result<PointP> {
    let mut p = sys::CuPoint::default();
    // SAFETY: valid out pointer.
    check(unsafe { sys::cu_cursor_position(&mut p) })?;
    Ok(PointP { x: p.x, y: p.y })
}

#[derive(Debug, Clone, Copy)]
pub struct FrameInfo {
    pub frame_seq: u64,
    pub captured_at_ns: u64,
    pub image_w: u32,
    pub image_h: u32,
    pub source: RectP,
    pub scale: f64,
    pub display_generation: u64,
}

/// Captures (one-shot until M7), optionally crops to `crop` (points), fits into `max_long_edge`
/// (0 = native), and encodes.
pub fn capture(
    max_long_edge: u32,
    format: ImageFormat,
    quality: f32,
    crop: Option<RectP>,
) -> Result<(FrameInfo, Vec<u8>)> {
    let fmt = match format {
        ImageFormat::Jpeg => sys::CU_FMT_JPEG,
        ImageFormat::Png => sys::CU_FMT_PNG,
    };
    let crop = crop.map(|r| sys::CuRect {
        x: r.x,
        y: r.y,
        w: r.w,
        h: r.h,
    });
    let crop_ptr = crop.as_ref().map_or(std::ptr::null(), |r| r as *const _);
    let mut info = sys::CuFrameInfo::default();
    let mut b = Buf::new();
    // SAFETY: crop_ptr is null or points to a live CuRect; out pointers are valid.
    check(unsafe {
        sys::cu_frame_encode(max_long_edge, fmt, quality, crop_ptr, &mut info, &mut b.0)
    })?;
    Ok((
        FrameInfo {
            frame_seq: info.frame_seq,
            captured_at_ns: info.captured_at_ns,
            image_w: info.image_w,
            image_h: info.image_h,
            source: rect(info.source_rect_points),
            scale: info.scale,
            display_generation: info.display_generation,
        },
        b.bytes().to_vec(),
    ))
}

/// Raw input primitives (global points).
pub mod input {
    use super::*;

    pub fn button_code(b: Button) -> i32 {
        match b {
            Button::Left => sys::CU_BTN_LEFT,
            Button::Right => sys::CU_BTN_RIGHT,
            Button::Middle => sys::CU_BTN_MIDDLE,
            Button::Back => sys::CU_BTN_BACK,
            Button::Forward => sys::CU_BTN_FORWARD,
        }
    }

    pub fn mouse_move(p: PointP) -> Result<()> {
        // SAFETY: plain values.
        check(unsafe { sys::cu_mouse_move(p.x, p.y) })
    }

    pub fn click(p: PointP, button: Button, count: u8, mods: Mods) -> Result<()> {
        // SAFETY: plain values.
        check(unsafe { sys::cu_mouse_click(p.x, p.y, button_code(button), count as i32, mods.0) })
    }

    pub fn mouse_button(button: Button, down: bool) -> Result<()> {
        // SAFETY: plain values.
        check(unsafe { sys::cu_mouse_button(button_code(button), down as i32) })
    }

    pub fn drag(path: &[PointP], mods: Mods) -> Result<()> {
        let pts: Vec<sys::CuPoint> = path
            .iter()
            .map(|p| sys::CuPoint { x: p.x, y: p.y })
            .collect();
        // SAFETY: pts outlives the call; n matches its length.
        check(unsafe { sys::cu_mouse_drag(pts.as_ptr(), pts.len(), mods.0) })
    }

    /// Positive dy scrolls content down; `pixels` selects pixel vs. line (wheel click) units.
    pub fn scroll(p: PointP, dx: f64, dy: f64, pixels: bool, mods: Mods) -> Result<()> {
        let unit = if pixels {
            sys::CU_SCROLL_PIXEL
        } else {
            sys::CU_SCROLL_LINE
        };
        // SAFETY: plain values.
        check(unsafe { sys::cu_scroll(p.x, p.y, dx, dy, unit, mods.0) })
    }

    pub fn key(combo: &KeyCombo, repeat: u8) -> Result<()> {
        let c = cstring(&combo.canonical())?;
        // SAFETY: c is a valid NUL-terminated string for the duration of the call.
        check(unsafe { sys::cu_key(c.as_ptr(), repeat as u32) })
    }

    pub fn key_hold(combo: &KeyCombo, duration_ms: u64) -> Result<()> {
        let c = cstring(&combo.canonical())?;
        // SAFETY: as above.
        check(unsafe { sys::cu_key_hold(c.as_ptr(), duration_ms.min(u32::MAX as u64) as u32) })
    }

    pub fn type_text(text: &str) -> Result<()> {
        let c = cstring(text)?;
        // SAFETY: as above.
        check(unsafe { sys::cu_type_text(c.as_ptr()) })
    }

    pub fn release_all() -> Result<()> {
        // SAFETY: no arguments.
        check(unsafe { sys::cu_release_all() })
    }
}

pub fn ax_focused() -> Result<AxSummary> {
    let mut b = Buf::new();
    // SAFETY: valid out pointer.
    check(unsafe { sys::cu_ax_focused(&mut b.0) })?;
    json(&b)
}

pub fn ax_element_at(p: PointP) -> Result<Option<AxElement>> {
    #[derive(Deserialize)]
    struct Hit {
        element: Option<AxElement>,
    }
    let mut b = Buf::new();
    // SAFETY: valid out pointer.
    check(unsafe { sys::cu_ax_element_at(p.x, p.y, &mut b.0) })?;
    Ok(json::<Hit>(&b)?.element)
}

/// Fixed sleep used while settle detection (M7) is unavailable.
fn sleep_ms(ms: u32) -> u64 {
    std::thread::sleep(Duration::from_millis(ms as u64));
    ms as u64
}

/// The real Mac. Human-input detection uses the cursor-delta fallback until the event-tap
/// watchdog lands (M5): if the cursor is not where our last event left it, the human moved it.
#[derive(Default)]
pub struct MacPlatform {
    baseline_cursor: Mutex<Option<PointP>>,
}

/// Cursor movement beyond this (points) counts as human input.
const CURSOR_TOLERANCE: f64 = 2.0;

impl MacPlatform {
    pub fn new() -> Arc<Self> {
        Arc::new(MacPlatform::default())
    }

    fn current_or(p: Option<PointP>) -> anyhow::Result<PointP> {
        Ok(match p {
            Some(p) => p,
            None => cursor()?,
        })
    }
}

impl Platform for MacPlatform {
    fn display(&self) -> anyhow::Result<DisplayInfo> {
        Ok(display_info()?)
    }

    fn observe(&self, opts: &ObserveOpts) -> anyhow::Result<Observation> {
        let (info, bytes) = capture(opts.max_long_edge, opts.format, opts.jpeg_quality, None)?;
        let mapping = CoordMapping {
            source: info.source,
            display_bounds: info.source,
            image_w: info.image_w,
            image_h: info.image_h,
            scale: info.scale,
            display_generation: info.display_generation,
            frame_seq: info.frame_seq,
        };
        let cursor_i = cursor().ok().map(|p| mapping.points_to_image(p));
        let focused = if opts.include_focused {
            ax_focused().ok()
        } else {
            None
        };
        Ok(Observation {
            image: EncodedImage {
                bytes: Arc::new(bytes),
                format: opts.format,
                width: info.image_w,
                height: info.image_h,
            },
            mapping,
            frame_seq: info.frame_seq,
            cursor_i,
            focused,
            frame_ref: None,
        })
    }

    fn zoom(&self, region: RectP, opts: &ObserveOpts) -> anyhow::Result<EncodedImage> {
        let (info, bytes) = capture(
            opts.max_long_edge,
            opts.format,
            opts.jpeg_quality,
            Some(region),
        )?;
        Ok(EncodedImage {
            bytes: Arc::new(bytes),
            format: opts.format,
            width: info.image_w,
            height: info.image_h,
        })
    }

    fn execute(&self, action: &ResolvedAction) -> anyhow::Result<()> {
        match action {
            ResolvedAction::MouseMove { at, mods } => {
                anyhow::ensure!(
                    mods.is_empty(),
                    "modifier keys during a mouse move are not supported"
                );
                input::mouse_move(*at)?
            }
            ResolvedAction::Click {
                at,
                button,
                count,
                mods,
            } => input::click(Self::current_or(*at)?, *button, *count, *mods)?,
            ResolvedAction::MouseButton { button, down } => input::mouse_button(*button, *down)?,
            ResolvedAction::Drag { path, mods } => input::drag(path, *mods)?,
            ResolvedAction::Scroll { at, delta, mods } => {
                let p = Self::current_or(*at)?;
                match *delta {
                    ScrollDelta::Clicks { dx, dy } => {
                        input::scroll(p, dx as f64, dy as f64, false, *mods)?
                    }
                    ScrollDelta::Pixels { dx, dy } => input::scroll(p, dx, dy, true, *mods)?,
                }
            }
            ResolvedAction::Type { text } => input::type_text(text)?,
            ResolvedAction::Key { combo, repeat } => input::key(combo, *repeat)?,
            ResolvedAction::HoldKey { combo, duration_ms } => input::key_hold(combo, *duration_ms)?,
        }
        if let Some(p) = action.final_cursor() {
            *self.baseline_cursor.lock().unwrap() = Some(p);
        }
        Ok(())
    }

    fn execute_with_feedback(
        &self,
        action: &ResolvedAction,
        feedback: &dyn Fn(cu_core::PointerFeedback),
    ) -> anyhow::Result<()> {
        let mut context = PointerCallbackContext {
            callback: feedback,
            is_click: matches!(action, ResolvedAction::Click { .. }),
        };
        // SAFETY: borrowed context remains alive throughout execute, on this
        // thread only. No callback survives the scope, including error returns.
        unsafe {
            sys::cu_set_pointer_callback(
                Some(pointer_callback),
                (&mut context as *mut PointerCallbackContext<'_>).cast(),
            )
        };
        let _scope = PointerCallbackScope;
        self.execute(action)
    }

    fn wait_settled(&self, opts: &SettleOpts) -> anyhow::Result<SettleReport> {
        let t = Instant::now();
        let mut seq = 0u64;
        // SAFETY: valid out pointer.
        let status = unsafe {
            sys::cu_wait_settled(
                opts.quiet_ms,
                opts.timeout_ms,
                opts.threshold,
                opts.after_frame_seq,
                &mut seq,
            )
        };
        match check(status) {
            Ok(()) => Ok(SettleReport {
                waited_ms: t.elapsed().as_millis() as u64,
                timed_out: false,
                fixed: false,
                settled_seq: Some(seq),
            }),
            Err(e) if e.code == sys::CU_ERR_TIMEOUT => Ok(SettleReport {
                waited_ms: t.elapsed().as_millis() as u64,
                timed_out: true,
                fixed: false,
                settled_seq: None,
            }),
            Err(e) if e.is_unimplemented() => {
                let waited_ms = sleep_ms(opts.fallback_ms);
                Ok(SettleReport {
                    waited_ms,
                    timed_out: false,
                    fixed: true,
                    settled_seq: None,
                })
            }
            Err(e) => Err(e.into()),
        }
    }

    fn cursor_position(&self) -> anyhow::Result<PointP> {
        Ok(cursor()?)
    }

    fn element_at(&self, p: PointP) -> anyhow::Result<Option<AxElement>> {
        Ok(ax_element_at(p)?)
    }

    fn focused(&self) -> anyhow::Result<Option<AxSummary>> {
        Ok(Some(ax_focused()?))
    }

    fn human_input_since(&self, _t: Instant) -> anyhow::Result<bool> {
        let now = cursor()?;
        let mut baseline = self.baseline_cursor.lock().unwrap();
        let moved = match *baseline {
            Some(b) => (now.x - b.x).hypot(now.y - b.y) > CURSOR_TOLERANCE,
            None => false,
        };
        *baseline = Some(now);
        Ok(moved)
    }

    fn reset_human_input_baseline(&self) {
        *self.baseline_cursor.lock().unwrap() = cursor().ok();
    }

    fn panic_requested(&self) -> anyhow::Result<bool> {
        let mut yes = 0i32;
        // SAFETY: valid out pointer.
        match check(unsafe { sys::cu_watchdog_panic_requested(&mut yes) }) {
            Ok(()) => Ok(yes != 0),
            Err(e) if e.is_unimplemented() => Ok(false), // panic hotkey lands with the event tap (M5)
            Err(e) => Err(e.into()),
        }
    }

    fn release_all(&self) -> anyhow::Result<()> {
        Ok(input::release_all()?)
    }
}
