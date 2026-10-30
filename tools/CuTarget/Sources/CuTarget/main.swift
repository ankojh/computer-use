// CuTarget: a window of known targets that logs every event it receives.
//
// Output (directory from $CUTARGET_DIR, default /tmp/cutarget):
//   layout.json   — frames of every target in global CG points (top-left origin), rewritten on move
//   events.jsonl  — one JSON object per received event (truncated at launch)
//
// Targets: square buttons of 8…64 pt ("btn-<size>"), a text field ("text"), a secure field
// ("secure"), a scroll view ("scroll"), and a drag pad ("drag").

import AppKit

let cuEventTag: Int64 = 0x4355_4147_454E_5431 // must match Input.swift
let outDir = URL(fileURLWithPath: ProcessInfo.processInfo.environment["CUTARGET_DIR"] ?? "/tmp/cutarget")

final class Log {
    static let shared = Log()
    private let handle: FileHandle
    private let start = Date()

    init() {
        try? FileManager.default.createDirectory(at: outDir, withIntermediateDirectories: true)
        let url = outDir.appendingPathComponent("events.jsonl")
        FileManager.default.createFile(atPath: url.path, contents: nil)
        handle = try! FileHandle(forWritingTo: url)
    }

    func write(_ object: [String: Any]) {
        var o = object
        o["t_ms"] = Int(Date().timeIntervalSince(start) * 1000)
        guard let data = try? JSONSerialization.data(withJSONObject: o, options: [.sortedKeys]) else { return }
        handle.write(data)
        handle.write(Data("\n".utf8))
    }
}

/// Converts a view's bounds to global CG points (top-left origin of the primary display).
func globalRect(_ view: NSView) -> CGRect? {
    guard let window = view.window, let primary = NSScreen.screens.first else { return nil }
    let r = window.convertToScreen(view.convert(view.bounds, to: nil))
    return CGRect(x: r.origin.x, y: primary.frame.height - (r.origin.y + r.height), width: r.width, height: r.height)
}

func rectJSON(_ r: CGRect) -> [String: Double] {
    ["x": r.origin.x, "y": r.origin.y, "w": r.width, "h": r.height]
}

func modifierNames(_ f: NSEvent.ModifierFlags) -> [String] {
    var names: [String] = []
    if f.contains(.control) { names.append("ctrl") }
    if f.contains(.option) { names.append("opt") }
    if f.contains(.shift) { names.append("shift") }
    if f.contains(.command) { names.append("cmd") }
    if f.contains(.function) { names.append("fn") }
    return names
}

/// A plain colored square; hit-testing identifies it by `identifier`.
final class TargetView: NSView {
    let color: NSColor
    init(id: String, color: NSColor) {
        self.color = color
        super.init(frame: .zero)
        identifier = NSUserInterfaceItemIdentifier(id)
    }
    required init?(coder: NSCoder) { fatalError() }
    override func draw(_ dirtyRect: NSRect) {
        color.setFill()
        bounds.fill()
    }
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
    // Accept mouse events so they are not passed to the window background.
    override func mouseDown(with event: NSEvent) {}
    override func rightMouseDown(with event: NSEvent) {}
    override func otherMouseDown(with event: NSEvent) {}
}

final class TallDocument: NSView {
    override var isFlipped: Bool { true }
    override func draw(_ dirtyRect: NSRect) {
        for i in 0..<100 {
            ("Line \(i)" as NSString).draw(at: NSPoint(x: 8, y: CGFloat(i) * 24 + 4), withAttributes: [.font: NSFont.systemFont(ofSize: 14)])
        }
    }
}

final class Controller: NSObject, NSApplicationDelegate, NSWindowDelegate, NSTextFieldDelegate {
    var window: NSWindow!
    var targets: [NSView] = []
    var scroll: NSScrollView!

    func applicationDidFinishLaunching(_ note: Notification) {
        let content = NSView(frame: NSRect(x: 0, y: 0, width: 900, height: 640))
        window = NSWindow(contentRect: NSRect(x: 100, y: 100, width: 900, height: 640),
                          styleMask: [.titled, .closable, .miniaturizable], backing: .buffered, defer: false)
        window.title = "CuTarget"
        window.contentView = content
        window.delegate = self

        // Buttons, bottom-left origin in Cocoa content coordinates.
        var x: CGFloat = 20
        for size in [8, 12, 16, 24, 32, 48, 64] {
            let v = TargetView(id: "btn-\(size)", color: .systemBlue)
            v.frame = NSRect(x: x, y: 640 - 40 - CGFloat(size), width: CGFloat(size), height: CGFloat(size))
            content.addSubview(v)
            targets.append(v)
            x += CGFloat(size) + 30
        }

        let text = NSTextField(frame: NSRect(x: 20, y: 480, width: 400, height: 28))
        text.identifier = NSUserInterfaceItemIdentifier("text")
        text.delegate = self
        content.addSubview(text)
        targets.append(text)

        let secure = NSSecureTextField(frame: NSRect(x: 20, y: 440, width: 400, height: 28))
        secure.identifier = NSUserInterfaceItemIdentifier("secure")
        secure.delegate = self
        content.addSubview(secure)
        targets.append(secure)

        scroll = NSScrollView(frame: NSRect(x: 20, y: 20, width: 400, height: 400))
        scroll.identifier = NSUserInterfaceItemIdentifier("scroll")
        scroll.hasVerticalScroller = true
        scroll.hasHorizontalScroller = true
        scroll.documentView = TallDocument(frame: NSRect(x: 0, y: 0, width: 800, height: 2400))
        scroll.contentView.postsBoundsChangedNotifications = true
        NotificationCenter.default.addObserver(self, selector: #selector(scrolled), name: NSView.boundsDidChangeNotification, object: scroll.contentView)
        content.addSubview(scroll)
        targets.append(scroll)

        let drag = TargetView(id: "drag", color: .systemGreen)
        drag.frame = NSRect(x: 460, y: 20, width: 420, height: 400)
        content.addSubview(drag)
        targets.append(drag)

        NSEvent.addLocalMonitorForEvents(matching: [
            .leftMouseDown, .leftMouseUp, .rightMouseDown, .rightMouseUp, .otherMouseDown, .otherMouseUp,
            .leftMouseDragged, .mouseMoved, .scrollWheel, .keyDown, .keyUp, .flagsChanged,
        ]) { [weak self] event in
            self?.log(event)
            return event
        }
        window.acceptsMouseMovedEvents = true
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        writeLayout()
        Log.shared.write(["kind": "ready"])
    }

    func targetID(at location: CGPoint?) -> String? {
        guard let location, let primary = NSScreen.screens.first, let content = window.contentView else { return nil }
        let cocoa = NSPoint(x: location.x, y: primary.frame.height - location.y)
        let inWindow = window.convertPoint(fromScreen: cocoa)
        var v = content.hitTest(content.convert(inWindow, from: nil))
        while let view = v {
            if let id = view.identifier?.rawValue, targets.contains(where: { $0.identifier?.rawValue == id }) { return id }
            v = view.superview
        }
        return nil
    }

    func log(_ e: NSEvent) {
        let cg = e.cgEvent
        let loc = cg?.location
        var o: [String: Any] = [
            "kind": "event",
            "type": String(describing: e.type),
            "mods": modifierNames(e.modifierFlags),
            "tagged": cg?.getIntegerValueField(.eventSourceUserData) == cuEventTag,
        ]
        if let loc { o["x"] = loc.x; o["y"] = loc.y }
        if let id = targetID(at: loc) { o["target"] = id }
        switch e.type {
        case .leftMouseDown, .leftMouseUp, .rightMouseDown, .rightMouseUp, .otherMouseDown, .otherMouseUp:
            o["click_count"] = e.clickCount
            o["button"] = e.buttonNumber
        case .scrollWheel:
            o["dx"] = e.scrollingDeltaX
            o["dy"] = e.scrollingDeltaY
            o["precise"] = e.hasPreciseScrollingDeltas
            o["inverted"] = e.isDirectionInvertedFromDevice
        case .keyDown, .keyUp:
            o["key_code"] = Int(e.keyCode)
            o["chars"] = e.characters ?? ""
            o["chars_ignoring_mods"] = e.charactersIgnoringModifiers ?? ""
            o["repeat"] = e.isARepeat
        case .flagsChanged:
            o["key_code"] = Int(e.keyCode)
        default: break
        }
        Log.shared.write(o)
    }

    func controlTextDidChange(_ note: Notification) {
        guard let field = note.object as? NSTextField else { return }
        let id = field.identifier?.rawValue ?? "?"
        if field is NSSecureTextField {
            Log.shared.write(["kind": "text", "target": id, "length": field.stringValue.count])
        } else {
            Log.shared.write(["kind": "text", "target": id, "value": field.stringValue])
        }
    }

    @objc func scrolled() {
        let o = scroll.contentView.bounds.origin
        Log.shared.write(["kind": "scroll_position", "target": "scroll", "x": o.x, "y": o.y])
    }

    func windowDidMove(_ note: Notification) { writeLayout() }

    func writeLayout() {
        var frames: [String: Any] = [:]
        for t in targets {
            if let id = t.identifier?.rawValue, let r = globalRect(t) { frames[id] = rectJSON(r) }
        }
        var layout: [String: Any] = ["targets": frames, "pid": Int(getpid())]
        if let content = window.contentView, let r = globalRect(content) { layout["window_content"] = rectJSON(r) }
        let data = try! JSONSerialization.data(withJSONObject: layout, options: [.prettyPrinted, .sortedKeys])
        try? data.write(to: outDir.appendingPathComponent("layout.json"), options: .atomic)
    }
}

let app = NSApplication.shared
let controller = Controller()
app.delegate = controller
app.setActivationPolicy(.regular)
app.run()
