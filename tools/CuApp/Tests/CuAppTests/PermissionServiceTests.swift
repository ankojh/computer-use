import XCTest
@testable import CuApp

@MainActor
final class PermissionServiceTests: XCTestCase {
    private let json = #"{"screenRecording":true,"accessibility":false}"#

    func testPermissionStatusComesFromBoundedHelper() async {
        let service = PermissionService(executable: "/usr/bin/printf", environment: [:], arguments: { _ in ["%s", self.json] })
        let done = expectation(description: "status")
        XCTAssertTrue(service.run(.check) { result in
            switch result {
            case .success(let snapshot):
                XCTAssertTrue(snapshot.screenRecording)
                XCTAssertFalse(snapshot.accessibility)
            case .failure(let error): XCTFail("Unexpected error: \(error)")
            }
            done.fulfill()
        })
        await fulfillment(of: [done], timeout: 3)
    }

    func testHangingPermissionCallTimesOutWithoutBlockingMainQueue() async {
        let service = PermissionService(executable: "/bin/sh", environment: ["PATH": "/bin:/usr/bin"],
                                        arguments: { _ in ["-c", "trap '' INT TERM; sleep 60 & wait"] })
        let responsive = expectation(description: "main queue remains responsive")
        let timeout = expectation(description: "bounded timeout")
        timeout.assertForOverFulfill = true
        XCTAssertTrue(service.run(.check, timeout: 0.1) { result in
            if case .failure(let error) = result {
                XCTAssertTrue(error is PermissionService.Failure)
                XCTAssertTrue(error.localizedDescription.contains("still responsive"))
            } else { XCTFail("A timeout is not a permission grant or denial") }
            timeout.fulfill()
        })
        DispatchQueue.main.async { responsive.fulfill() }
        await fulfillment(of: [responsive, timeout], timeout: 3)
        // The forced termination callback must not complete the request again.
        let reaped = expectation(description: "late child completion ignored")
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.4) { reaped.fulfill() }
        await fulfillment(of: [reaped], timeout: 2)
    }

    func testRepeatedRequestsAreCoalesced() async {
        let service = PermissionService(executable: "/bin/sleep", environment: [:], arguments: { _ in ["60"] })
        let done = expectation(description: "timeout")
        XCTAssertTrue(service.run(.accessibility, timeout: 0.05) { _ in done.fulfill() })
        XCTAssertFalse(service.run(.accessibility) { _ in XCTFail("Must not duplicate an OS prompt") })
        XCTAssertFalse(service.run(.check) { _ in XCTFail("Must not race a pending request") })
        await fulfillment(of: [done], timeout: 2)
    }

    func testCancelledHelperCannotOverwriteNewerResult() async {
        let json = self.json
        let service = PermissionService(executable: "/bin/sh", environment: ["PATH": "/bin:/usr/bin"], arguments: { operation in
            operation == .check ? ["-c", "sleep 60"] : ["-c", "printf '%s' \"$1\"", "fixture", json]
        })
        XCTAssertTrue(service.run(.check) { _ in XCTFail("Cancelled result must be discarded") })
        service.cancel()
        let done = expectation(description: "new result")
        XCTAssertTrue(service.run(.accessibility) { result in
            XCTAssertEqual(try? result.get(), PermissionSnapshot(screenRecording: true, accessibility: false))
            done.fulfill()
        })
        await fulfillment(of: [done], timeout: 3)
    }

    func testMalformedHelperResponseIsNotADenial() async {
        let service = PermissionService(executable: "/usr/bin/printf", environment: [:], arguments: { _ in ["invalid"] })
        let done = expectation(description: "invalid response")
        service.run(.check) { result in
            guard case .failure = result else { XCTFail("Must report unknown, not false permissions"); done.fulfill(); return }
            done.fulfill()
        }
        await fulfillment(of: [done], timeout: 3)
    }

    func testMissingInstalledExecutableReportsFailure() {
        let service = PermissionService(executable: nil)
        var received = false
        service.run(.check) { result in
            if case .failure = result { received = true }
        }
        XCTAssertTrue(received)
    }
}
