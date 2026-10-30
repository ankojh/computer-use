import AppKit
import XCTest
@testable import CuApp

@MainActor
final class StatusPanelTests: XCTestCase {
    func testControlsAreAHiddenTransientMenuBarPopoverNotADetachedWindow() {
        _ = NSApplication.shared
        let content = NSViewController()
        content.view = NSView(frame: NSRect(x: 0, y: 0, width: 520, height: 640))
        let panel = StatusPanel(content: content, size: NSSize(width: 520, height: 640))
        defer { panel.remove() }
        XCTAssertEqual(panel.popover.behavior, .transient)
        XCTAssertFalse(panel.popover.isShown)
        XCTAssertFalse(panel.popoverShouldDetach(panel.popover))
        XCTAssertTrue(panel.popover.delegate === panel)
        XCTAssertNil(panel.item.menu)
        XCTAssertTrue(panel.item.button?.target === panel)
        XCTAssertEqual(panel.item.button?.action, #selector(StatusPanel.toggle))
        XCTAssertTrue(panel.popover.contentViewController === content)
        XCTAssertEqual(panel.popover.contentSize, NSSize(width: 520, height: 640))
    }
}
