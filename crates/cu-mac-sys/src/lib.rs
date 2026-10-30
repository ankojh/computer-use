//! Raw bindings to the CuMac Swift static library. Mirrors `swift/CuMac/include/cu.h` by hand;
//! keep the two in sync and bump [`CU_ABI_VERSION`] on any change.
#![allow(non_camel_case_types)]

use std::os::raw::c_char;

pub const CU_ABI_VERSION: i32 = 2;

pub const CU_POINTER_MOVE: i32 = 0;
pub const CU_POINTER_DRAG: i32 = 1;
pub const CU_POINTER_DOWN: i32 = 2;
pub const CU_POINTER_UP: i32 = 3;
pub type CuPointerCallback = unsafe extern "C" fn(*mut std::ffi::c_void, i32, f64, f64, i32, i32);

#[repr(C)]
#[derive(Debug)]
pub struct CuBuf {
    pub ptr: *mut u8,
    pub len: usize,
}

impl Default for CuBuf {
    fn default() -> Self {
        CuBuf {
            ptr: std::ptr::null_mut(),
            len: 0,
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CuRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CuPoint {
    pub x: f64,
    pub y: f64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct CuDisplayInfo {
    pub display_id: u32,
    pub bounds_points: CuRect,
    pub pixel_w: u32,
    pub pixel_h: u32,
    pub scale: f64,
    pub generation: u64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct CuFrameInfo {
    pub frame_seq: u64,
    pub captured_at_ns: u64,
    pub image_w: u32,
    pub image_h: u32,
    pub source_rect_points: CuRect,
    pub scale: f64,
    pub display_generation: u64,
}

pub const CU_OK: i32 = 0;
pub const CU_ERR: i32 = -1;
pub const CU_ERR_INVALID_ARG: i32 = -2;
pub const CU_ERR_PERMISSION: i32 = -3;
pub const CU_ERR_TIMEOUT: i32 = -4;
pub const CU_ERR_UNIMPLEMENTED: i32 = -5;
pub const CU_ERR_ABORTED: i32 = -6;

pub const CU_FMT_JPEG: i32 = 0;
pub const CU_FMT_PNG: i32 = 1;

pub const CU_BTN_LEFT: i32 = 0;
pub const CU_BTN_RIGHT: i32 = 1;
pub const CU_BTN_MIDDLE: i32 = 2;
pub const CU_BTN_BACK: i32 = 3;
pub const CU_BTN_FORWARD: i32 = 4;

pub const CU_MOD_CMD: u32 = 1;
pub const CU_MOD_SHIFT: u32 = 2;
pub const CU_MOD_OPT: u32 = 4;
pub const CU_MOD_CTRL: u32 = 8;
pub const CU_MOD_FN: u32 = 16;

pub const CU_SCROLL_LINE: i32 = 0;
pub const CU_SCROLL_PIXEL: i32 = 1;

pub const CU_PERM_SCREEN: u32 = 1;
pub const CU_PERM_ACCESSIBILITY: u32 = 2;
pub const CU_PERM_LISTEN: u32 = 4;
pub const CU_PERM_POST: u32 = 8;

extern "C" {
    pub fn cu_abi_version() -> i32;
    pub fn cu_last_error(out: *mut CuBuf) -> i32;
    pub fn cu_buf_free(buf: CuBuf);
    pub fn cu_init() -> i32;

    pub fn cu_permissions(mask_out: *mut u32, details_json_out: *mut CuBuf) -> i32;
    pub fn cu_request_permissions(mask: u32) -> i32;
    pub fn cu_display_info(out: *mut CuDisplayInfo) -> i32;
    pub fn cu_cursor_position(out: *mut CuPoint) -> i32;

    pub fn cu_capture_start(fps: u32, shows_cursor: i32) -> i32;
    pub fn cu_capture_stop() -> i32;
    pub fn cu_frame_encode(
        max_long_edge: u32,
        fmt: i32,
        quality: f32,
        crop_points_or_null: *const CuRect,
        info_out: *mut CuFrameInfo,
        bytes_out: *mut CuBuf,
    ) -> i32;
    pub fn cu_wait_settled(
        quiet_ms: u32,
        timeout_ms: u32,
        change_threshold: f32,
        after_frame_seq: u64,
        settled_seq_out: *mut u64,
    ) -> i32;
    pub fn cu_changed_since(frame_seq: u64, rects_json_out: *mut CuBuf) -> i32;

    pub fn cu_set_pointer_callback(
        callback: Option<CuPointerCallback>,
        context: *mut std::ffi::c_void,
    );
    pub fn cu_mouse_move(x: f64, y: f64) -> i32;
    pub fn cu_mouse_click(x: f64, y: f64, button: i32, count: i32, mods: u32) -> i32;
    pub fn cu_mouse_button(button: i32, down: i32) -> i32;
    pub fn cu_mouse_drag(path: *const CuPoint, n: usize, mods: u32) -> i32;
    pub fn cu_scroll(x: f64, y: f64, dx: f64, dy: f64, unit: i32, mods: u32) -> i32;
    pub fn cu_key(combo_utf8: *const c_char, repeat: u32) -> i32;
    pub fn cu_key_hold(combo_utf8: *const c_char, duration_ms: u32) -> i32;
    pub fn cu_type_text(text_utf8: *const c_char) -> i32;
    pub fn cu_release_all() -> i32;

    pub fn cu_watchdog_start() -> i32;
    pub fn cu_watchdog_human_input_since(since_ns: u64, yes_out: *mut i32) -> i32;
    pub fn cu_watchdog_panic_requested(yes_out: *mut i32) -> i32;

    pub fn cu_ax_focused(json_out: *mut CuBuf) -> i32;
    pub fn cu_ax_element_at(x: f64, y: f64, json_out: *mut CuBuf) -> i32;
    pub fn cu_ax_tree(max_depth: u32, max_nodes: u32, json_out: *mut CuBuf) -> i32;

    pub fn cu_record_start(path_utf8: *const c_char, fps: u32, bitrate_kbps: u32) -> i32;
    pub fn cu_record_stop() -> i32;

    pub fn cu_template_store(region_points: *const CuRect, template_id_out: *mut u32) -> i32;
    pub fn cu_template_find(
        template_id: u32,
        search_points_or_null: *const CuRect,
        min_score: f32,
        found_points_out: *mut CuRect,
        score_out: *mut f32,
    ) -> i32;
}

// Same guards as the `_Static_assert`s in cu.h.
const _: () = assert!(std::mem::size_of::<CuRect>() == 32);
const _: () = assert!(std::mem::size_of::<CuDisplayInfo>() == 64);
const _: () = assert!(std::mem::size_of::<CuFrameInfo>() == 72);
