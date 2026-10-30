import AppKit
import XCTest
@testable import CuApp

@MainActor
final class AgentCursorTests: XCTestCase {
    func testRetinaUsesLogicalPointsAndFlipsY() {
        let frame = CGRect(x: 0, y: 0, width: 1728, height: 1117)
        XCTAssertEqual(CursorGeometry.cocoaPoint(.init(x: 864, y: 100), quartz: frame, cocoa: frame), CGPoint(x: 864, y: 1017))
    }
    func testNegativeOriginAndVerticallyStackedScreen() {
        let left = CGRect(x: -1440, y: 0, width: 1440, height: 900)
        XCTAssertEqual(CursorGeometry.cocoaPoint(.init(x: -100, y: 100), quartz: left, cocoa: left), CGPoint(x: -100, y: 800))
        XCTAssertEqual(CursorGeometry.cocoaPoint(.init(x: 100, y: -100),
            quartz: CGRect(x: 0, y: -900, width: 1920, height: 900),
            cocoa: CGRect(x: 0, y: 1117, width: 1920, height: 900)), CGPoint(x: 100, y: 1217))
    }
    func testRejectsNonfiniteAndOffscreenPoints() {
        let frame = CGRect(x: 0, y: 0, width: 1728, height: 1117)
        XCTAssertNil(CursorGeometry.cocoaPoint(.init(x: .infinity, y: 100), quartz: frame, cocoa: frame))
        XCTAssertNil(CursorGeometry.cocoaPoint(.init(x: 100, y: .nan), quartz: frame, cocoa: frame))
        XCTAssertNil(CursorGeometry.cocoaPoint(.init(x: -1, y: 0), quartz: frame, cocoa: frame))
    }
    func testMarkerIsClickThroughNonactivatingAndNotAnInputDevice() {
        _ = NSApplication.shared
        let marker = AgentCursor()
        XCTAssertTrue(marker.panel.ignoresMouseEvents)
        XCTAssertFalse(marker.panel.canBecomeKey)
        XCTAssertFalse(marker.panel.canBecomeMain)
        XCTAssertTrue(marker.panel.isAccessibilityHidden())
        XCTAssertFalse(marker.panel.isVisible)
        XCTAssertEqual(marker.panel.sharingType, .none)
        XCTAssertNil(marker.panel.contentView?.hitTest(.zero))
        marker.hide()
    }
    func testRepeatedNativePositionRearmsMovementAndProgressDoesNotRestartFade() {
        _ = NSApplication.shared
        var now = 0.0
        let marker = AgentCursor(clock: { now })
        let point = CursorFeedback.Point(x: 200, y: 200)
        let first = CursorFeedback(point: point, phase: .posted, kind: .move, sequence: 1)
        marker.update(first)
        XCTAssertTrue(marker.panel.isVisible)
        now = 1; marker.animate()
        XCTAssertFalse(marker.panel.isVisible)
        marker.update(first)
        XCTAssertFalse(marker.panel.isVisible)
        marker.update(.init(point: point, phase: .posted, kind: .move, sequence: 2))
        XCTAssertTrue(marker.panel.isVisible)
        marker.hide()
    }
    func testClicksSurviveSubsequentMovesAndExpireWithoutDelayingInput() {
        _ = NSApplication.shared
        var now = 0.0
        let marker = AgentCursor(clock: { now })
        marker.update(.init(point: .init(x: 200, y: 200), phase: .posted, kind: .click, sequence: 1, button: .right, count: 2))
        XCTAssertEqual(marker.pulsePanels.count, 1)
        let click = marker.pulsePanels[0]
        XCTAssertTrue(click.ignoresMouseEvents)
        XCTAssertTrue(click.isAccessibilityHidden())
        XCTAssertFalse(click.canBecomeKey)
        XCTAssertEqual(click.sharingType, .none)
        marker.update(.init(point: .init(x: 300, y: 300), phase: .posted, kind: .move, sequence: 2))
        XCTAssertTrue(click.isVisible)
        XCTAssertNotEqual(click.frame.origin, marker.panel.frame.origin)
        now = 0.8; marker.animate()
        XCTAssertTrue(marker.pulsePanels.isEmpty)
        XCTAssertFalse(click.isVisible)
        marker.update(.init(point: .init(x: 350, y: 350), phase: .posted, kind: .click, sequence: 3, button: .left, count: 1))
        XCTAssertTrue(marker.pulsePanels.first === click)
        XCTAssertEqual(click.alphaValue, 1)
        XCTAssertEqual((click.contentView as? AgentCursorView)?.feedback.label, "Left click")
        marker.hide()
    }
    func testPulsePoolIsBoundedAndPauseClearsEveryWindow() {
        _ = NSApplication.shared
        let existingWindows = NSApp.windows.filter { $0.title == AgentCursor.windowTitle }.count
        let marker = AgentCursor()
        for sequence in 1...20 {
            marker.update(.init(point: .init(x: 200, y: 200), phase: .posted, kind: .click, sequence: UInt64(sequence), button: .left, count: 1))
        }
        XCTAssertEqual(marker.pulsePanels.count, AgentCursor.maximumPulses)
        XCTAssertLessThanOrEqual(NSApp.windows.filter { $0.title == AgentCursor.windowTitle }.count, existingWindows + AgentCursor.maximumPulses + 1)
        let panels = marker.pulsePanels
        marker.update(nil)
        XCTAssertFalse(marker.panel.isVisible)
        XCTAssertTrue(panels.allSatisfy { !$0.isVisible })
        XCTAssertTrue(marker.pulsePanels.isEmpty)
    }
    func testDistinctLabelsAndPostedEventDecode() throws {
        let event = try JSONDecoder().decode(EngineEvent.self, from: Data(#"{"type":"cursor","cursor":{"point":{"x":200,"y":300},"phase":"posted","kind":"click","sequence":15,"button":"right","count":2}}"#.utf8))
        XCTAssertEqual(event.cursor?.label, "Right click ×2")
        XCTAssertTrue(event.cursor?.isClick == true)
        XCTAssertEqual(CursorFeedback(point: .init(x: 0, y: 0), phase: .target).label, "Cu target")
        XCTAssertNotEqual(AgentCursorView.previewPNG(kind: .move), AgentCursorView.previewPNG(kind: .click))
    }
    func testOffscreenPreviewRendersOnlyOurOwnGlyph() {
        _ = NSApplication.shared
        let png = AgentCursorView.previewPNG()
        XCTAssertNotNil(png)
        XCTAssertNotNil(png.flatMap(NSImage.init(data:)))
    }
}
