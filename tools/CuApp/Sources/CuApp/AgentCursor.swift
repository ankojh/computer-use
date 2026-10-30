import AppKit

/// Cursor feedback only; this panel cannot become an interactive controls window.
final class CursorPanel: NSPanel {
    override var canBecomeKey: Bool { false }
    override var canBecomeMain: Bool { false }
}

/// Logical points, not screenshot/device pixels; testable on stacked/Retina displays.
enum CursorGeometry {
    static func cocoaPoint(_ point: CursorFeedback.Point, quartz: CGRect, cocoa: CGRect) -> CGPoint? {
        guard point.valid, quartz.width > 0, quartz.height > 0, cocoa.width > 0, cocoa.height > 0,
              quartz.contains(CGPoint(x: point.x, y: point.y)) else { return nil }
        return CGPoint(x: cocoa.minX + (point.x - quartz.minX) * cocoa.width / quartz.width,
                       y: cocoa.maxY - (point.y - quartz.minY) * cocoa.height / quartz.height)
    }
}

/// A visual target, NOT a second input device. Never reads/follows the human's
/// cursor, warps/posts input, activates an app, or accepts clicks. Native posted
/// events drive brief movement highlights and independent click ripples, so the
/// next move or typing action cannot erase a click before it has been visible.
@MainActor
final class AgentCursor {
    static let windowTitle = "Cu Agent Cursor"
    static let size = NSSize(width: 218, height: 118)
    static let tip = NSPoint(x: 38, y: 38)
    static let pulseLifetime: TimeInterval = 0.75
    static let maximumPulses = 8
    let panel: CursorPanel
    private let glyph: AgentCursorView
    private let clock: () -> TimeInterval
    private var previous: CursorFeedback?
    private var timer: Timer?
    private var markerStart: TimeInterval = 0
    private var markerLifetime: TimeInterval?
    private struct Pulse {
        let panel: CursorPanel
        let glyph: AgentCursorView
        let start: TimeInterval
    }
    private var pulses: [Pulse] = []
    private var availablePulses: [Pulse] = []
    var pulsePanels: [CursorPanel] { pulses.map(\.panel) }

    init(clock: @escaping () -> TimeInterval = { ProcessInfo.processInfo.systemUptime }) {
        self.clock = clock
        glyph = AgentCursorView(frame: NSRect(origin: .zero, size: Self.size))
        panel = Self.makePanel(glyph)
    }

    private static func makePanel(_ glyph: AgentCursorView) -> CursorPanel {
        let panel = CursorPanel(contentRect: NSRect(origin: .zero, size: size),
                                styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        panel.title = windowTitle
        panel.level = .statusBar
        panel.isFloatingPanel = true
        panel.hidesOnDeactivate = false
        panel.ignoresMouseEvents = true
        panel.setAccessibilityElement(false)
        panel.setAccessibilityHidden(true)
        glyph.setAccessibilityElement(false)
        glyph.setAccessibilityHidden(true)
        panel.isMovable = false
        panel.isReleasedWhenClosed = false
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .ignoresCycle]
        panel.backgroundColor = .clear
        panel.isOpaque = false
        panel.hasShadow = false
        panel.sharingType = .none // SCK also excludes Cu's entire application.
        panel.animationBehavior = .none
        panel.contentView = glyph
        return panel
    }

    func update(_ feedback: CursorFeedback?) {
        guard let feedback else { hide(); return }
        guard feedback != previous else { return } // progress cannot restart an expired pulse
        guard let point = cocoaPoint(feedback.point) else { hide(); return }
        previous = feedback
        let now = clock()
        if feedback.isClick {
            // Recycle actual windows, not merely the active-array entries.
            // AppKit can retain ordered-out windows until its autorelease pool
            // drains; a fixed pool bounds allocation even in one large batch.
            let pulse: Pulse
            if let available = availablePulses.popLast() { pulse = available }
            else if pulses.count == Self.maximumPulses { pulse = pulses.removeFirst() }
            else {
                let view = AgentCursorView(frame: NSRect(origin: .zero, size: Self.size))
                pulse = Pulse(panel: Self.makePanel(view), glyph: view, start: now)
            }
            pulse.glyph.feedback = feedback
            pulse.glyph.progress = 0
            pulse.panel.alphaValue = 1
            position(pulse.panel, at: point)
            pulse.panel.orderFrontRegardless()
            pulses.append(Pulse(panel: pulse.panel, glyph: pulse.glyph, start: now))
        }
        glyph.feedback = feedback
        glyph.progress = 0
        markerStart = now
        markerLifetime = feedback.phase == .target ? nil : feedback.phase == .active ? 5 : 0.9
        position(panel, at: point)
        panel.alphaValue = 1
        panel.orderFrontRegardless()
        if timer == nil {
            timer = Timer.scheduledTimer(withTimeInterval: 1.0 / 30, repeats: true) { [weak self] _ in
                Task { @MainActor in self?.animate() }
            }
        }
    }

    private func position(_ panel: NSPanel, at point: CGPoint) {
        panel.setFrameOrigin(NSPoint(x: point.x - Self.tip.x,
                                     y: point.y - (Self.size.height - Self.tip.y)))
    }

    func animate() {
        let now = clock()
        if let lifetime = markerLifetime {
            let progress = max(0, min(1, (now - markerStart) / lifetime))
            glyph.progress = progress
            panel.alphaValue = min(1, (1 - progress) * 3)
            if progress >= 1 { panel.orderOut(nil) }
        }
        pulses.removeAll { pulse in
            let progress = max(0, min(1, (now - pulse.start) / Self.pulseLifetime))
            pulse.glyph.progress = progress
            pulse.panel.alphaValue = 1 - progress
            if progress >= 1 {
                pulse.panel.orderOut(nil)
                availablePulses.append(pulse)
                return true
            }
            return false
        }
        if (!panel.isVisible || markerLifetime == nil) && pulses.isEmpty { timer?.invalidate(); timer = nil }
    }

    func hide() {
        timer?.invalidate(); timer = nil
        previous = nil
        markerLifetime = nil
        panel.orderOut(nil)
        for pulse in pulses { pulse.panel.orderOut(nil) }
        availablePulses.append(contentsOf: pulses)
        pulses.removeAll()
    }

    private func cocoaPoint(_ point: CursorFeedback.Point) -> CGPoint? {
        for screen in NSScreen.screens {
            guard let id = screen.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? CGDirectDisplayID else { continue }
            if let converted = CursorGeometry.cocoaPoint(point, quartz: CGDisplayBounds(id), cocoa: screen.frame) { return converted }
        }
        return nil
    }
}

@MainActor
final class AgentCursorView: NSView {
    static func previewPNG(kind: CursorFeedback.Kind = .move) -> Data? {
        let view = AgentCursorView(frame: NSRect(origin: .zero, size: AgentCursor.size))
        view.feedback = .init(point: .init(x: 0, y: 0), phase: .posted, kind: kind, button: .left, count: 2)
        view.progress = kind == .click ? 0.35 : 0
        guard let bitmap = view.bitmapImageRepForCachingDisplay(in: view.bounds) else { return nil }
        view.cacheDisplay(in: view.bounds, to: bitmap)
        return bitmap.representation(using: .png, properties: [:])
    }
    var feedback = CursorFeedback(point: .init(x: 0, y: 0), phase: .posted, kind: .move) { didSet { needsDisplay = true } }
    var progress: Double = 0 { didSet { needsDisplay = true } }
    override var isFlipped: Bool { true }
    override var isOpaque: Bool { false }
    override func hitTest(_ point: NSPoint) -> NSView? { nil }

    override func draw(_ dirtyRect: NSRect) {
        let pending = feedback.phase == .target || feedback.phase == .active
        let click = feedback.isClick
        let color: NSColor = pending ? .systemOrange : click ? .systemGreen : feedback.kind == .buttonDown ? .systemPink : NSColor(red: 0, green: 0.91, blue: 1, alpha: 1)
        let tip = AgentCursor.tip
        let radius = click ? 14 + progress * 20 : 21 + progress * 7
        let ring = NSBezierPath(ovalIn: NSRect(x: tip.x - radius, y: tip.y - radius, width: radius * 2, height: radius * 2))
        color.withAlphaComponent(click ? 0.25 : 0.18).setFill(); ring.fill()
        NSColor.black.withAlphaComponent(0.75).setStroke(); ring.lineWidth = 6; ring.stroke()
        color.setStroke(); ring.lineWidth = click ? 4 : 3; ring.stroke()
        if click {
            let inner = NSBezierPath(ovalIn: NSRect(x: tip.x - 6, y: tip.y - 6, width: 12, height: 12))
            NSColor.white.setStroke(); inner.lineWidth = 2; inner.stroke()
            color.setFill(); inner.fill()
        } else {
            let arrow = NSBezierPath()
            arrow.move(to: tip)
            for point in [NSPoint(x: 0, y: 33), NSPoint(x: 8, y: 25), NSPoint(x: 15, y: 39),
                          NSPoint(x: 22, y: 35), NSPoint(x: 15, y: 21), NSPoint(x: 28, y: 21)] {
                arrow.line(to: NSPoint(x: tip.x + point.x, y: tip.y + point.y))
            }
            arrow.close()
            NSColor.black.setStroke(); arrow.lineWidth = 5; arrow.stroke()
            color.setFill(); arrow.fill()
            NSColor.white.setStroke(); arrow.lineWidth = 1.3; arrow.stroke()
        }
        let text = feedback.label as NSString
        let attributes: [NSAttributedString.Key: Any] = [.font: NSFont.boldSystemFont(ofSize: 12), .foregroundColor: color]
        let width = min(AgentCursor.size.width - 76, text.size(withAttributes: attributes).width + 14)
        let label = NSRect(x: 73, y: 47, width: width, height: 24)
        NSColor.black.withAlphaComponent(0.9).setFill()
        NSBezierPath(roundedRect: label, xRadius: 7, yRadius: 7).fill()
        text.draw(at: NSPoint(x: label.minX + 7, y: label.minY + 4), withAttributes: attributes)
    }
}
