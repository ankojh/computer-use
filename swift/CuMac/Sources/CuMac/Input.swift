import ApplicationServices
import CCu
import CoreGraphics
import Foundation

/// Tag stored in `.eventSourceUserData` of every event we post ("CUAGENT1"), so the watchdog can
/// tell our events from the human's.
let cuEventTag: Int64 = 0x4355_4147_454E_5431

/// Tracks held buttons/keys so that aborts always release them, and an abort epoch that
/// interrupts long-running operations (typing, drags, holds).
final class InputState: @unchecked Sendable {
    static let shared = InputState()

    private let lock = NSLock()
    private var heldButtons: Set<Int32> = []
    private var heldKeys: [CGKeyCode] = []
    private var epoch: UInt64 = 0
    let source: CGEventSource? = CGEventSource(stateID: .hidSystemState)

    var abortEpoch: UInt64 { lock.withLock { epoch } }

    func setButton(_ button: Int32, down: Bool) {
        lock.withLock { if down { heldButtons.insert(button) } else { heldButtons.remove(button) } }
    }

    func setKey(_ code: CGKeyCode, down: Bool) {
        lock.withLock {
            if down { heldKeys.append(code) } else { heldKeys.removeAll { $0 == code } }
        }
    }

    var anyButtonHeld: Int32? { lock.withLock { heldButtons.sorted().first } }

    /// Bumps the abort epoch and returns everything that is still held.
    func takeAllHeld() -> (buttons: [Int32], keys: [CGKeyCode]) {
        lock.withLock {
            epoch += 1
            let held = (heldButtons.sorted(), Array(heldKeys.reversed()))
            heldButtons.removeAll()
            heldKeys.removeAll()
            return held
        }
    }
}

enum Input {
    static func ensureCanPost() throws {
        guard CGPreflightPostEventAccess() || AXIsProcessTrusted() else {
            throw CuError.permission("posting input events requires Accessibility permission (run `cu doctor`)")
        }
    }

    static func post(_ event: CGEvent) {
        event.setIntegerValueField(.eventSourceUserData, value: cuEventTag)
        // Track held state BEFORE a synchronous observer can run. A stop must
        // still release a posted down event if the feedback pipe is congested.
        if let sample = InputPointerFeedback.sample(event), sample.kind == CU_POINTER_DOWN || sample.kind == CU_POINTER_UP {
            InputState.shared.setButton(sample.button, down: sample.kind == CU_POINTER_DOWN)
        }
        event.post(tap: .cghidEventTap)
        InputPointerFeedback.posted(event)
    }

    // MARK: Mouse

    struct ButtonTypes {
        let down: CGEventType
        let up: CGEventType
        let dragged: CGEventType
        let cg: CGMouseButton
        let number: Int64
    }

    static func buttonTypes(_ button: Int32) throws -> ButtonTypes {
        switch button {
        case Int32(CU_BTN_LEFT):
            return ButtonTypes(down: .leftMouseDown, up: .leftMouseUp, dragged: .leftMouseDragged, cg: .left, number: 0)
        case Int32(CU_BTN_RIGHT):
            return ButtonTypes(down: .rightMouseDown, up: .rightMouseUp, dragged: .rightMouseDragged, cg: .right, number: 1)
        case Int32(CU_BTN_MIDDLE), Int32(CU_BTN_BACK), Int32(CU_BTN_FORWARD):
            let n = Int64(button) // middle = 2, back = 3, forward = 4
            return ButtonTypes(down: .otherMouseDown, up: .otherMouseUp, dragged: .otherMouseDragged,
                               cg: CGMouseButton(rawValue: UInt32(n))!, number: n)
        default:
            throw CuError.invalid("unknown mouse button \(button)")
        }
    }

    static func mouseEvent(_ type: CGEventType, at p: CGPoint, button: ButtonTypes, flags: CGEventFlags,
                           clickState: Int64 = 1) throws -> CGEvent {
        guard let e = CGEvent(mouseEventSource: InputState.shared.source, mouseType: type,
                              mouseCursorPosition: p, mouseButton: button.cg) else {
            throw CuError.fail("could not create mouse event")
        }
        e.flags = flags
        e.setIntegerValueField(.mouseEventClickState, value: clickState)
        if button.number >= 2 { e.setIntegerValueField(.mouseEventButtonNumber, value: button.number) }
        return e
    }

    /// Moves the cursor; emits dragged events if a button is held so that drags made of
    /// down → move → up (Anthropic left_mouse_down/up) work.
    static func move(to p: CGPoint) throws {
        try ensureCanPost()
        if let held = InputState.shared.anyButtonHeld {
            let t = try buttonTypes(held)
            post(try mouseEvent(t.dragged, at: p, button: t, flags: []))
        } else {
            post(try mouseEvent(.mouseMoved, at: p, button: try buttonTypes(Int32(CU_BTN_LEFT)), flags: []))
        }
    }

    static func click(at p: CGPoint, button: Int32, count: Int32, mods: UInt32) throws {
        guard (1...3).contains(count) else { throw CuError.invalid("click count must be 1...3, got \(count)") }
        let t = try buttonTypes(button)
        // Always move first: hover state matters for many apps.
        try move(to: p)
        sleepMs(15)
        let flags = try pressModifiers(mods)
        defer { releaseModifiers(mods) }
        for i in 1...Int64(count) {
            post(try mouseEvent(t.down, at: p, button: t, flags: flags, clickState: i))
            sleepMs(12)
            post(try mouseEvent(t.up, at: p, button: t, flags: flags, clickState: i))
            if i < Int64(count) { sleepMs(30) }
        }
    }

    static func button(_ button: Int32, down: Bool) throws {
        try ensureCanPost()
        let t = try buttonTypes(button)
        let p = try cursorPosition()
        post(try mouseEvent(down ? t.down : t.up, at: p, button: t, flags: []))
    }

    static func drag(_ path: [CGPoint], mods: UInt32) throws {
        guard path.count >= 2 else { throw CuError.invalid("drag needs at least 2 points") }
        let epoch = InputState.shared.abortEpoch
        let t = try buttonTypes(Int32(CU_BTN_LEFT))
        try move(to: path[0])
        sleepMs(15)
        let flags = try pressModifiers(mods)
        defer { releaseModifiers(mods) }
        post(try mouseEvent(t.down, at: path[0], button: t, flags: flags))
        var last = path[0]
        do {
            try interruptibleSleep(ms: 50, epoch: epoch) // some apps need a short hold before dragging
            for target in path.dropFirst() {
                let dist = hypot(target.x - last.x, target.y - last.y)
                let steps = max(1, Int((dist / 8).rounded(.up)))
                for s in 1...steps {
                    let f = Double(s) / Double(steps)
                    let p = CGPoint(x: last.x + (target.x - last.x) * f, y: last.y + (target.y - last.y) * f)
                    post(try mouseEvent(t.dragged, at: p, button: t, flags: flags))
                    try interruptibleSleep(ms: 5, epoch: epoch)
                }
                last = target
            }
        } catch {
            post(try mouseEvent(t.up, at: last, button: t, flags: flags))
            throw error
        }
        sleepMs(15)
        post(try mouseEvent(t.up, at: last, button: t, flags: flags))
    }

    /// Positive dy scrolls content down (reveals what is below), positive dx scrolls right.
    /// ⚠ verify sign conventions empirically with CuTarget (HANDOFF.md §7.4): CGEvent wheel1 > 0
    /// is "wheel away from the user" (content scrolls up), so we negate.
    static func scroll(at p: CGPoint, dx: Double, dy: Double, unit: Int32, mods: UInt32) throws {
        let units: CGScrollEventUnit
        let maxPerEvent: Double
        switch unit {
        case Int32(CU_SCROLL_LINE): units = .line; maxPerEvent = 1
        case Int32(CU_SCROLL_PIXEL): units = .pixel; maxPerEvent = 40
        default: throw CuError.invalid("unknown scroll unit \(unit)")
        }
        let epoch = InputState.shared.abortEpoch
        try move(to: p)
        sleepMs(10)
        let flags = try pressModifiers(mods)
        defer { releaseModifiers(mods) }

        // Split into small events; many apps ignore one huge delta.
        var remX = dx, remY = dy
        while remX.magnitude >= 0.5 || remY.magnitude >= 0.5 {
            let stepY = (remY.magnitude >= 0.5) ? (min(remY.magnitude, maxPerEvent) * (remY < 0 ? -1 : 1)).rounded() : 0
            let stepX = (remX.magnitude >= 0.5) ? (min(remX.magnitude, maxPerEvent) * (remX < 0 ? -1 : 1)).rounded() : 0
            guard let e = CGEvent(scrollWheelEvent2Source: InputState.shared.source, units: units, wheelCount: 2,
                                  wheel1: Int32(-stepY), wheel2: Int32(-stepX), wheel3: 0) else {
                throw CuError.fail("could not create scroll event")
            }
            e.flags = flags
            post(e)
            remY -= stepY
            remX -= stepX
            try interruptibleSleep(ms: unit == Int32(CU_SCROLL_LINE) ? 15 : 8, epoch: epoch)
        }
    }

    // MARK: Keyboard

    static func keyEvent(_ code: CGKeyCode, down: Bool, flags: CGEventFlags) throws -> CGEvent {
        guard let e = CGEvent(keyboardEventSource: InputState.shared.source, virtualKey: code, keyDown: down) else {
            throw CuError.fail("could not create key event")
        }
        e.flags = flags
        return e
    }

    /// Posts modifier key-downs (and sets flags on each) so apps that check either signal work.
    static func pressModifiers(_ mods: UInt32) throws -> CGEventFlags {
        try ensureCanPost()
        var flags: CGEventFlags = []
        for bit in Keys.modifierOrder where mods & bit != 0 {
            let m = Keys.modifier(bit)
            flags.insert(m.flag)
            post(try keyEvent(m.keyCode, down: true, flags: flags))
            InputState.shared.setKey(m.keyCode, down: true)
        }
        if mods != 0 { sleepMs(5) }
        return flags
    }

    static func releaseModifiers(_ mods: UInt32) {
        var flags = Keys.flags(mods)
        for bit in Keys.modifierOrder.reversed() where mods & bit != 0 {
            let m = Keys.modifier(bit)
            flags.remove(m.flag)
            if let e = try? keyEvent(m.keyCode, down: false, flags: flags) { post(e) }
            InputState.shared.setKey(m.keyCode, down: false)
        }
    }

    static func key(_ combo: String, repeatCount: UInt32) throws {
        guard (1...100).contains(repeatCount) else { throw CuError.invalid("repeat must be 1...100, got \(repeatCount)") }
        let parsed = try Keys.parse(combo)
        let epoch = InputState.shared.abortEpoch
        let flags = try pressModifiers(parsed.mods)
        defer { releaseModifiers(parsed.mods) }
        guard let code = parsed.key else { return } // modifier-only combo: press + release
        for i in 0..<repeatCount {
            post(try keyEvent(code, down: true, flags: flags))
            sleepMs(8)
            post(try keyEvent(code, down: false, flags: flags))
            if i + 1 < repeatCount { try interruptibleSleep(ms: 15, epoch: epoch) }
        }
    }

    static func keyHold(_ combo: String, durationMs: UInt32) throws {
        let parsed = try Keys.parse(combo)
        let epoch = InputState.shared.abortEpoch
        let flags = try pressModifiers(parsed.mods)
        defer { releaseModifiers(parsed.mods) }
        if let code = parsed.key {
            post(try keyEvent(code, down: true, flags: flags))
            InputState.shared.setKey(code, down: true)
            defer {
                if let e = try? keyEvent(code, down: false, flags: flags) { post(e) }
                InputState.shared.setKey(code, down: false)
            }
            try interruptibleSleep(ms: durationMs, epoch: epoch)
        } else {
            try interruptibleSleep(ms: durationMs, epoch: epoch)
        }
    }

    /// Types text with unicode-string key events in chunks of ≤ 20 UTF-16 units. Newlines and tabs
    /// are sent as Return/Tab key presses because unicode newlines are not reliably interpreted.
    static func type(_ text: String) throws {
        try ensureCanPost()
        let epoch = InputState.shared.abortEpoch
        var chunk: [UniChar] = []

        func flush() throws {
            guard !chunk.isEmpty else { return }
            for down in [true, false] {
                let e = try keyEvent(0, down: down, flags: [])
                chunk.withUnsafeBufferPointer { buf in
                    e.keyboardSetUnicodeString(stringLength: buf.count, unicodeString: buf.baseAddress)
                }
                post(e)
            }
            chunk.removeAll(keepingCapacity: true)
            try interruptibleSleep(ms: 3, epoch: epoch)
        }

        func press(_ code: CGKeyCode) throws {
            try flush()
            post(try keyEvent(code, down: true, flags: []))
            sleepMs(5)
            post(try keyEvent(code, down: false, flags: []))
            try interruptibleSleep(ms: 5, epoch: epoch)
        }

        for ch in text {
            switch ch {
            case "\n", "\r\n", "\r": try press(0x24)
            case "\t": try press(0x30)
            default:
                let units = Array(String(ch).utf16) // never split a grapheme / surrogate pair
                if chunk.count + units.count > 20 { try flush() }
                chunk.append(contentsOf: units)
            }
        }
        try flush()
    }

    static func releaseAll() {
        let held = InputState.shared.takeAllHeld()
        let p = (try? cursorPosition()) ?? .zero
        for b in held.buttons {
            if let t = try? buttonTypes(b), let e = try? mouseEvent(t.up, at: p, button: t, flags: []) { post(e) }
        }
        for code in held.keys {
            if let e = try? keyEvent(code, down: false, flags: []) { post(e) }
        }
    }
}
