import CCu
import CoreGraphics
import Foundation
import XCTest
@testable import CuMac

private final class SampleSink: @unchecked Sendable {
    private let lock = NSLock()
    private var storage: [InputPointerFeedback.Sample] = []
    var samples: [InputPointerFeedback.Sample] { lock.withLock { storage } }
    func append(_ sample: InputPointerFeedback.Sample) { lock.withLock { storage.append(sample) } }
}
private let sampleCallback: CuPointerCallback = { context, kind, x, y, button, count in
    guard let context else { return }
    Unmanaged<SampleSink>.fromOpaque(context).takeUnretainedValue().append(.init(kind: kind, x: x, y: y, button: button, count: count))
}
final class PointerFeedbackTests: XCTestCase {
    // Construct events ONLY; never call Input.post or CGEvent.post in unit tests.
    func testNativeMoveDragAndRepeatedClickMetadata() throws {
        let sink = SampleSink()
        InputPointerFeedback.install(sampleCallback, context: Unmanaged.passUnretained(sink).toOpaque())
        defer { InputPointerFeedback.install(nil, context: nil); withExtendedLifetime(sink) {} }
        let move = try XCTUnwrap(CGEvent(mouseEventSource: nil, mouseType: .mouseMoved, mouseCursorPosition: CGPoint(x: -20, y: 80), mouseButton: .left))
        InputPointerFeedback.posted(move); InputPointerFeedback.posted(move)
        let drag = try XCTUnwrap(CGEvent(mouseEventSource: nil, mouseType: .leftMouseDragged, mouseCursorPosition: CGPoint(x: 150, y: -50), mouseButton: .left))
        InputPointerFeedback.posted(drag)
        let click = try XCTUnwrap(CGEvent(mouseEventSource: nil, mouseType: .rightMouseUp, mouseCursorPosition: CGPoint(x: 300, y: 250), mouseButton: .right))
        click.setIntegerValueField(.mouseEventClickState, value: 2)
        InputPointerFeedback.posted(click)
        XCTAssertEqual(sink.samples.map(\.kind), [0, 0, 1, 3])
        XCTAssertEqual(sink.samples.last?.button, 1)
        XCTAssertEqual(sink.samples.last?.count, 2)
        XCTAssertEqual(sink.samples[2].y, -50)
        InputPointerFeedback.install(nil, context: nil)
        InputPointerFeedback.posted(click)
        XCTAssertEqual(sink.samples.count, 4)
    }
    func testObserverIsThreadLocalAndNeverReportsKeyboardData() throws {
        let sink = SampleSink()
        InputPointerFeedback.install(sampleCallback, context: Unmanaged.passUnretained(sink).toOpaque())
        defer { InputPointerFeedback.install(nil, context: nil); withExtendedLifetime(sink) {} }
        let keyboard = try XCTUnwrap(CGEvent(keyboardEventSource: nil, virtualKey: 0, keyDown: true))
        InputPointerFeedback.posted(keyboard)
        let semaphore = DispatchSemaphore(value: 0)
        Thread.detachNewThread {
            if let move = CGEvent(mouseEventSource: nil, mouseType: .mouseMoved, mouseCursorPosition: .zero, mouseButton: .left) { InputPointerFeedback.posted(move) }
            semaphore.signal()
        }
        XCTAssertEqual(semaphore.wait(timeout: .now() + 2), .success)
        XCTAssertTrue(sink.samples.isEmpty)
    }
}
