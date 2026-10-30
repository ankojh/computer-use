// The only public surface of CuMac: @_cdecl functions mirroring include/cu.h.

import CCu
import CoreGraphics
import Foundation

// MARK: ABI basics

@_cdecl("cu_abi_version")
public func cu_abi_version() -> Int32 { CU_ABI_VERSION }

@_cdecl("cu_last_error")
public func cu_last_error(_ out: UnsafeMutablePointer<CuBuf>?) -> Int32 {
    guard let out else { return Int32(CU_ERR_INVALID_ARG) }
    out.pointee = makeBuf(Data(lastError().utf8))
    return Int32(CU_OK)
}

@_cdecl("cu_buf_free")
public func cu_buf_free(_ buf: CuBuf) {
    free(buf.ptr)
}

@_cdecl("cu_init")
public func cu_init() -> Int32 {
    guarded {
        guard Thread.isMainThread else { throw CuError.invalid("cu_init must be called on the main thread") }
        Keys.buildLayoutMap()
        _ = try DisplayTracker.shared.current()
    }
}

// MARK: Permissions & display

@_cdecl("cu_permissions")
public func cu_permissions(_ maskOut: UnsafeMutablePointer<UInt32>?, _ details: UnsafeMutablePointer<CuBuf>?) -> Int32 {
    guarded {
        maskOut?.pointee = Permissions.mask()
        if let details { details.pointee = makeBuf(try Permissions.details()) }
    }
}

@_cdecl("cu_request_permissions")
public func cu_request_permissions(_ mask: UInt32) -> Int32 {
    guarded { Permissions.request(mask) }
}

@_cdecl("cu_display_info")
public func cu_display_info(_ out: UnsafeMutablePointer<CuDisplayInfo>?) -> Int32 {
    guarded {
        guard let out else { throw CuError.invalid("out is null") }
        out.pointee = try DisplayTracker.shared.current()
    }
}

@_cdecl("cu_cursor_position")
public func cu_cursor_position(_ out: UnsafeMutablePointer<CuPoint>?) -> Int32 {
    guarded {
        guard let out else { throw CuError.invalid("out is null") }
        let p = try cursorPosition()
        out.pointee = CuPoint(x: p.x, y: p.y)
    }
}

// MARK: Capture

@_cdecl("cu_capture_start")
public func cu_capture_start(_ fps: UInt32, _ showsCursor: Int32) -> Int32 {
    guarded { throw CuError.unimplemented("continuous capture (SCStream)", milestone: "M7") }
}

@_cdecl("cu_capture_stop")
public func cu_capture_stop() -> Int32 {
    guarded { throw CuError.unimplemented("continuous capture (SCStream)", milestone: "M7") }
}

@_cdecl("cu_frame_encode")
public func cu_frame_encode(_ maxLongEdge: UInt32, _ fmt: Int32, _ quality: Float,
                            _ crop: UnsafePointer<CuRect>?, _ infoOut: UnsafeMutablePointer<CuFrameInfo>?,
                            _ bytesOut: UnsafeMutablePointer<CuBuf>?) -> Int32 {
    guarded {
        guard let infoOut, let bytesOut else { throw CuError.invalid("output pointer is null") }
        let frame = try captureAndEncode(maxLongEdge: maxLongEdge, format: fmt, quality: quality,
                                         cropPoints: crop?.pointee)
        infoOut.pointee = frame.info
        bytesOut.pointee = makeBuf(frame.bytes)
    }
}

@_cdecl("cu_wait_settled")
public func cu_wait_settled(_ quietMs: UInt32, _ timeoutMs: UInt32, _ threshold: Float, _ afterSeq: UInt64,
                            _ settledOut: UnsafeMutablePointer<UInt64>?) -> Int32 {
    guarded { throw CuError.unimplemented("settle detection", milestone: "M7") }
}

@_cdecl("cu_changed_since")
public func cu_changed_since(_ seq: UInt64, _ out: UnsafeMutablePointer<CuBuf>?) -> Int32 {
    guarded { throw CuError.unimplemented("change detection", milestone: "M7") }
}

// MARK: Input

@_cdecl("cu_mouse_move")
public func cu_mouse_move(_ x: Double, _ y: Double) -> Int32 {
    guarded { try Input.move(to: CGPoint(x: x, y: y)) }
}

@_cdecl("cu_mouse_click")
public func cu_mouse_click(_ x: Double, _ y: Double, _ button: Int32, _ count: Int32, _ mods: UInt32) -> Int32 {
    guarded { try Input.click(at: CGPoint(x: x, y: y), button: button, count: count, mods: mods) }
}

@_cdecl("cu_mouse_button")
public func cu_mouse_button(_ button: Int32, _ down: Int32) -> Int32 {
    guarded { try Input.button(button, down: down != 0) }
}

@_cdecl("cu_mouse_drag")
public func cu_mouse_drag(_ path: UnsafePointer<CuPoint>?, _ n: Int, _ mods: UInt32) -> Int32 {
    guarded {
        guard let path, n >= 2 else { throw CuError.invalid("drag needs at least 2 points") }
        let points = UnsafeBufferPointer(start: path, count: n).map { CGPoint(x: $0.x, y: $0.y) }
        try Input.drag(points, mods: mods)
    }
}

@_cdecl("cu_scroll")
public func cu_scroll(_ x: Double, _ y: Double, _ dx: Double, _ dy: Double, _ unit: Int32, _ mods: UInt32) -> Int32 {
    guarded { try Input.scroll(at: CGPoint(x: x, y: y), dx: dx, dy: dy, unit: unit, mods: mods) }
}

@_cdecl("cu_key")
public func cu_key(_ combo: UnsafePointer<CChar>?, _ repeatCount: UInt32) -> Int32 {
    guarded {
        guard let combo else { throw CuError.invalid("combo is null") }
        try Input.key(String(cString: combo), repeatCount: repeatCount)
    }
}

@_cdecl("cu_key_hold")
public func cu_key_hold(_ combo: UnsafePointer<CChar>?, _ durationMs: UInt32) -> Int32 {
    guarded {
        guard let combo else { throw CuError.invalid("combo is null") }
        try Input.keyHold(String(cString: combo), durationMs: durationMs)
    }
}

@_cdecl("cu_type_text")
public func cu_type_text(_ text: UnsafePointer<CChar>?) -> Int32 {
    guarded {
        guard let text else { throw CuError.invalid("text is null") }
        try Input.type(String(cString: text))
    }
}

@_cdecl("cu_release_all")
public func cu_release_all() -> Int32 {
    guarded { Input.releaseAll() }
}

// MARK: Watchdog (M5)

@_cdecl("cu_watchdog_start")
public func cu_watchdog_start() -> Int32 {
    guarded { throw CuError.unimplemented("watchdog event tap", milestone: "M5") }
}

@_cdecl("cu_watchdog_human_input_since")
public func cu_watchdog_human_input_since(_ sinceNs: UInt64, _ yesOut: UnsafeMutablePointer<Int32>?) -> Int32 {
    guarded { throw CuError.unimplemented("watchdog event tap", milestone: "M5") }
}

@_cdecl("cu_watchdog_panic_requested")
public func cu_watchdog_panic_requested(_ yesOut: UnsafeMutablePointer<Int32>?) -> Int32 {
    guarded { throw CuError.unimplemented("panic hotkey", milestone: "M5") }
}

// MARK: Accessibility

@_cdecl("cu_ax_focused")
public func cu_ax_focused(_ out: UnsafeMutablePointer<CuBuf>?) -> Int32 {
    guarded { try writeBuf(out, try AX.focusedJSON()) }
}

@_cdecl("cu_ax_element_at")
public func cu_ax_element_at(_ x: Double, _ y: Double, _ out: UnsafeMutablePointer<CuBuf>?) -> Int32 {
    guarded { try writeBuf(out, try AX.elementAtJSON(x: x, y: y)) }
}

@_cdecl("cu_ax_tree")
public func cu_ax_tree(_ maxDepth: UInt32, _ maxNodes: UInt32, _ out: UnsafeMutablePointer<CuBuf>?) -> Int32 {
    guarded { throw CuError.unimplemented("AX tree snapshot", milestone: "later") }
}

// MARK: Recording (M8)

@_cdecl("cu_record_start")
public func cu_record_start(_ path: UnsafePointer<CChar>?, _ fps: UInt32, _ bitrateKbps: UInt32) -> Int32 {
    guarded { throw CuError.unimplemented("session recording", milestone: "M8") }
}

@_cdecl("cu_record_stop")
public func cu_record_stop() -> Int32 {
    guarded { throw CuError.unimplemented("session recording", milestone: "M8") }
}

// MARK: Template matching (M9)

@_cdecl("cu_template_store")
public func cu_template_store(_ region: UnsafePointer<CuRect>?, _ idOut: UnsafeMutablePointer<UInt32>?) -> Int32 {
    guarded { throw CuError.unimplemented("template matching", milestone: "M9") }
}

@_cdecl("cu_template_find")
public func cu_template_find(_ id: UInt32, _ search: UnsafePointer<CuRect>?, _ minScore: Float,
                             _ foundOut: UnsafeMutablePointer<CuRect>?, _ scoreOut: UnsafeMutablePointer<Float>?) -> Int32 {
    guarded { throw CuError.unimplemented("template matching", milestone: "M9") }
}
