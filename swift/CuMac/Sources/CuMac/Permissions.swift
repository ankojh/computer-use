import ApplicationServices
import CCu
import CoreGraphics
import Foundation

enum Permissions {
    static let settingsURLs: [String: String] = [
        "screen_recording": "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture",
        "accessibility": "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility",
        "input_monitoring": "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent",
    ]

    static func mask() -> UInt32 {
        var m: UInt32 = 0
        if CGPreflightScreenCaptureAccess() { m |= UInt32(CU_PERM_SCREEN) }
        if AXIsProcessTrusted() { m |= UInt32(CU_PERM_ACCESSIBILITY) }
        if CGPreflightListenEventAccess() { m |= UInt32(CU_PERM_LISTEN) }
        if CGPreflightPostEventAccess() { m |= UInt32(CU_PERM_POST) }
        return m
    }

    static func details() throws -> Data {
        let m = mask()
        func entry(_ bit: Int, _ key: String, _ why: String) -> [String: Any] {
            var e: [String: Any] = ["granted": m & UInt32(bit) != 0, "why": why]
            if let url = settingsURLs[key] { e["settings_url"] = url }
            return e
        }
        return try jsonData([
            "screen_recording": entry(Int(CU_PERM_SCREEN), "screen_recording", "capture screenshots (ScreenCaptureKit)"),
            "accessibility": entry(Int(CU_PERM_ACCESSIBILITY), "accessibility", "read UI elements; post input events"),
            "input_monitoring": entry(Int(CU_PERM_LISTEN), "input_monitoring", "detect human input (watchdog event tap)"),
            "post_events": entry(Int(CU_PERM_POST), "accessibility", "post synthetic mouse/keyboard events"),
            "process": ProcessInfo.processInfo.processName,
            "pid": Int(getpid()),
        ])
    }

    /// Triggers the system prompts for the requested permissions. Prompts are asynchronous;
    /// Screen Recording grants usually need a process restart to take effect.
    static func request(_ mask: UInt32) {
        if mask & UInt32(CU_PERM_SCREEN) != 0 { _ = CGRequestScreenCaptureAccess() }
        if mask & UInt32(CU_PERM_ACCESSIBILITY) != 0 {
            // String literal instead of kAXTrustedCheckOptionPrompt (a mutable global under Swift 6).
            _ = AXIsProcessTrustedWithOptions(["AXTrustedCheckOptionPrompt": true] as CFDictionary)
        }
        if mask & UInt32(CU_PERM_LISTEN) != 0 { _ = CGRequestListenEventAccess() }
        if mask & UInt32(CU_PERM_POST) != 0 { _ = CGRequestPostEventAccess() }
    }
}
