// C ABI between the Rust core and the CuMac Swift layer.
//
// Rules (HANDOFF.md §8.1):
// - Every function returns int32_t status: 0 = ok, negative = CU_ERR_*.
// - On error, the calling thread's last error message is available via cu_last_error().
// - CuBuf memory is malloc'd by Swift and must be released with cu_buf_free().
// - All functions may be called from any non-main thread unless noted.
// - Coordinates are global points (top-left of the main display, y down) unless noted.
//
// Mirrored by hand in crates/cu-mac-sys/src/lib.rs; bump CU_ABI_VERSION on any change.
#pragma once

#include <stdint.h>
#include <stddef.h>

#define CU_ABI_VERSION 2

typedef struct { uint8_t *ptr; size_t len; } CuBuf;
typedef struct { double x, y, w, h; } CuRect;
typedef struct { double x, y; } CuPoint;

typedef struct {
  uint32_t display_id;
  CuRect   bounds_points;
  uint32_t pixel_w, pixel_h;
  double   scale;
  uint64_t generation;
} CuDisplayInfo;

typedef struct {
  uint64_t frame_seq;
  uint64_t captured_at_ns;         // CLOCK_UPTIME_RAW nanoseconds
  uint32_t image_w, image_h;       // size of the encoded image we return
  CuRect   source_rect_points;     // region of the display the image covers (full display or crop)
  double   scale;
  uint64_t display_generation;
} CuFrameInfo;

// Layout guards; the Rust mirror asserts the same sizes.
_Static_assert(sizeof(CuRect) == 32, "CuRect layout");
_Static_assert(sizeof(CuDisplayInfo) == 64, "CuDisplayInfo layout");
_Static_assert(sizeof(CuFrameInfo) == 72, "CuFrameInfo layout");

enum { CU_OK = 0, CU_ERR = -1, CU_ERR_INVALID_ARG = -2, CU_ERR_PERMISSION = -3, CU_ERR_TIMEOUT = -4,
       CU_ERR_UNIMPLEMENTED = -5, CU_ERR_ABORTED = -6 };
enum { CU_FMT_JPEG = 0, CU_FMT_PNG = 1 };
enum { CU_BTN_LEFT = 0, CU_BTN_RIGHT = 1, CU_BTN_MIDDLE = 2, CU_BTN_BACK = 3, CU_BTN_FORWARD = 4 };
enum { CU_MOD_CMD = 1, CU_MOD_SHIFT = 2, CU_MOD_OPT = 4, CU_MOD_CTRL = 8, CU_MOD_FN = 16 };
enum { CU_SCROLL_LINE = 0, CU_SCROLL_PIXEL = 1 };
enum { CU_PERM_SCREEN = 1, CU_PERM_ACCESSIBILITY = 2, CU_PERM_LISTEN = 4, CU_PERM_POST = 8 };

int32_t cu_abi_version(void);
int32_t cu_last_error(CuBuf *out);
void    cu_buf_free(CuBuf buf);
// Must be called once from the process main thread before anything else (builds the keyboard
// layout map; Text Input Source APIs require the main thread).
int32_t cu_init(void);

// permissions & display
int32_t cu_permissions(uint32_t *mask_out, CuBuf *details_json_out);
int32_t cu_request_permissions(uint32_t mask);
int32_t cu_display_info(CuDisplayInfo *out);
int32_t cu_cursor_position(CuPoint *out);

// capture
int32_t cu_capture_start(uint32_t fps, int32_t shows_cursor);
int32_t cu_capture_stop(void);
// Without a running stream this takes a one-shot screenshot (SCScreenshotManager).
// max_long_edge == 0 keeps the native (device pixel) size.
int32_t cu_frame_encode(uint32_t max_long_edge, int32_t fmt, float quality,
                        const CuRect *crop_points_or_null, CuFrameInfo *info_out, CuBuf *bytes_out);
int32_t cu_wait_settled(uint32_t quiet_ms, uint32_t timeout_ms, float change_threshold,
                        uint64_t after_frame_seq, uint64_t *settled_seq_out);
int32_t cu_changed_since(uint64_t frame_seq, CuBuf *rects_json_out);

// Thread-scoped observation of our posted mouse events only. The borrowed
// callback/context must outlive synchronous input calls; clear it before return.
// It cannot intercept/modify events, and never reports human input.
enum { CU_POINTER_MOVE = 0, CU_POINTER_DRAG = 1, CU_POINTER_DOWN = 2, CU_POINTER_UP = 3 };
typedef void (*CuPointerCallback)(void *context, int32_t kind, double x, double y,
                                 int32_t button, int32_t click_count);
void cu_set_pointer_callback(CuPointerCallback callback_or_null, void *context_or_null);

// input — scroll deltas: positive dy scrolls content down (reveals what is below), positive dx right.
int32_t cu_mouse_move(double x, double y);
int32_t cu_mouse_click(double x, double y, int32_t button, int32_t count, uint32_t mods);
int32_t cu_mouse_button(int32_t button, int32_t down);                 // at current position
int32_t cu_mouse_drag(const CuPoint *path, size_t n, uint32_t mods);
int32_t cu_scroll(double x, double y, double dx, double dy, int32_t unit, uint32_t mods);
int32_t cu_key(const char *combo_utf8, uint32_t repeat);               // canonical combo, "cmd+shift+t"
int32_t cu_key_hold(const char *combo_utf8, uint32_t duration_ms);
int32_t cu_type_text(const char *text_utf8);
int32_t cu_release_all(void);                                          // panic: release buttons/modifiers

// watchdog
int32_t cu_watchdog_start(void);
int32_t cu_watchdog_human_input_since(uint64_t since_ns, int32_t *yes_out);
int32_t cu_watchdog_panic_requested(int32_t *yes_out);

// accessibility
int32_t cu_ax_focused(CuBuf *json_out);                 // focused app + focused element
int32_t cu_ax_element_at(double x, double y, CuBuf *json_out);
int32_t cu_ax_tree(uint32_t max_depth, uint32_t max_nodes, CuBuf *json_out);

// recording
int32_t cu_record_start(const char *path_utf8, uint32_t fps, uint32_t bitrate_kbps);
int32_t cu_record_stop(void);

// metal extras (later milestones)
int32_t cu_template_store(const CuRect *region_points, uint32_t *template_id_out);
int32_t cu_template_find(uint32_t template_id, const CuRect *search_points_or_null,
                         float min_score, CuRect *found_points_out, float *score_out);
