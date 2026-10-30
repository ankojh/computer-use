import AppKit
import ApplicationServices
import Carbon
import CCu
import Foundation

/// Accessibility queries. Attribute names are string literals: the kAX* constants are imported as
/// globals that Swift 6 strict concurrency rejects.
enum AX {
    static let messagingTimeout: Float = 0.25

    static func ensureTrusted() throws {
        guard AXIsProcessTrusted() else {
            throw CuError.permission("Accessibility permission is not granted (run `cu doctor`)")
        }
    }

    static func systemWide() -> AXUIElement {
        let el = AXUIElementCreateSystemWide()
        AXUIElementSetMessagingTimeout(el, messagingTimeout)
        return el
    }

    static func attr(_ el: AXUIElement, _ name: String) -> CFTypeRef? {
        var value: CFTypeRef?
        guard AXUIElementCopyAttributeValue(el, name as CFString, &value) == .success else { return nil }
        return value
    }

    static func string(_ el: AXUIElement, _ name: String, limit: Int = 200) -> String? {
        guard let v = attr(el, name) else { return nil }
        let s: String?
        if let str = v as? String { s = str }
        else if let num = v as? NSNumber { s = num.stringValue }
        else { s = nil }
        guard let s, !s.isEmpty else { return nil }
        return s.count > limit ? String(s.prefix(limit)) + "…" : s
    }

    static func bool(_ el: AXUIElement, _ name: String) -> Bool? {
        (attr(el, name) as? NSNumber)?.boolValue
    }

    static func element(_ el: AXUIElement, _ name: String) -> AXUIElement? {
        guard let v = attr(el, name), CFGetTypeID(v) == AXUIElementGetTypeID() else { return nil }
        return unsafeDowncast(v, to: AXUIElement.self)
    }

    static func frame(_ el: AXUIElement) -> [String: Double]? {
        guard let pv = attr(el, "AXPosition"), CFGetTypeID(pv) == AXValueGetTypeID(),
              let sv = attr(el, "AXSize"), CFGetTypeID(sv) == AXValueGetTypeID() else { return nil }
        var p = CGPoint.zero
        var s = CGSize.zero
        guard AXValueGetValue(unsafeDowncast(pv, to: AXValue.self), .cgPoint, &p),
              AXValueGetValue(unsafeDowncast(sv, to: AXValue.self), .cgSize, &s) else { return nil }
        return ["x": p.x, "y": p.y, "w": s.width, "h": s.height]
    }

    static func app(pid: pid_t) -> [String: Any] {
        let running = NSRunningApplication(processIdentifier: pid)
        return [
            "pid": Int(pid),
            "name": running?.localizedName as Any? ?? NSNull(),
            "bundle_id": running?.bundleIdentifier as Any? ?? NSNull(),
        ]
    }

    static func describe(_ el: AXUIElement) -> [String: Any] {
        AXUIElementSetMessagingTimeout(el, messagingTimeout)
        var d: [String: Any] = [:]
        d["role"] = string(el, "AXRole") ?? NSNull()
        d["subrole"] = string(el, "AXSubrole") ?? NSNull()
        d["title"] = string(el, "AXTitle") ?? NSNull()
        d["description"] = string(el, "AXDescription") ?? NSNull()
        d["help"] = string(el, "AXHelp") ?? NSNull()
        d["identifier"] = string(el, "AXIdentifier") ?? NSNull()
        let secure = (d["subrole"] as? String) == "AXSecureTextField"
        d["secure"] = secure
        // Never read the value of secure fields.
        d["value"] = secure ? NSNull() : (string(el, "AXValue") ?? NSNull())
        d["enabled"] = bool(el, "AXEnabled") ?? NSNull()
        d["frame_points"] = frame(el) ?? NSNull()
        var pid: pid_t = 0
        if AXUIElementGetPid(el, &pid) == .success { d["app"] = app(pid: pid) }
        return d
    }

    static func focusedJSON() throws -> Data {
        try ensureTrusted()
        let sys = systemWide()
        var result: [String: Any] = ["secure_input": IsSecureEventInputEnabled()]
        var focusedElement: AXUIElement?
        if let appEl = element(sys, "AXFocusedApplication") {
            AXUIElementSetMessagingTimeout(appEl, messagingTimeout)
            var pid: pid_t = 0
            result["app"] = AXUIElementGetPid(appEl, &pid) == .success ? app(pid: pid) : NSNull()
            result["window_title"] = element(appEl, "AXFocusedWindow").flatMap { string($0, "AXTitle") } ?? NSNull()
            focusedElement = element(appEl, "AXFocusedUIElement")
        } else {
            result["app"] = NSNull()
            result["window_title"] = NSNull()
        }
        result["element"] = (focusedElement ?? element(sys, "AXFocusedUIElement")).map(describe) ?? NSNull()
        return try jsonData(result)
    }

    static func elementAtJSON(x: Double, y: Double) throws -> Data {
        try ensureTrusted()
        var el: AXUIElement?
        let err = AXUIElementCopyElementAtPosition(systemWide(), Float(x), Float(y), &el)
        guard err == .success, let el else {
            return try jsonData(["element": NSNull(), "ax_error": Int(err.rawValue)])
        }
        return try jsonData(["element": describe(el)])
    }
}
