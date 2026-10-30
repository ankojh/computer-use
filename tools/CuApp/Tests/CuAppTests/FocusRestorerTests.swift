import XCTest
@testable import CuApp

@MainActor
final class FocusRestorerTests: XCTestCase {
    func testAlreadyFocusedDoesNotActivateAnything() {
        var result: Bool?
        FocusRestorer.restore(pid: 42, activate: { XCTFail("Already focused"); return false },
                              frontmost: { 42 }, completion: { result = $0 })
        XCTAssertEqual(result, true)
    }

    func testFailedActivationAndInvalidPIDFailClosed() {
        for pid in [Int32(0), Int32(42)] {
            var result: Bool?
            FocusRestorer.restore(pid: pid, activate: { false }, frontmost: { nil }, completion: { result = $0 })
            XCTAssertEqual(result, false)
        }
    }

    func testSuccessfulActivationRequestIsNotProofOfFocus() {
        var result: Bool?
        FocusRestorer.restore(pid: 42, activate: { true }, frontmost: { 7 }, timeout: 0, completion: { result = $0 })
        XCTAssertEqual(result, false)
    }

    func testWaitsForObservedFocusBeforeReleasingResponse() {
        let complete = expectation(description: "Observed focus")
        var current: Int32 = 7
        FocusRestorer.restore(pid: 42, activate: { true }, frontmost: { current }, completion: { restored in
            XCTAssertTrue(restored)
            XCTAssertEqual(current, 42)
            complete.fulfill()
        })
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.04) { current = 42 }
        wait(for: [complete], timeout: 2)
    }
}
