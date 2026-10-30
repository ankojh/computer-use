import XCTest
@testable import CuApp

@MainActor
final class CodexReadinessProbeTests: XCTestCase {
    private let healthy = Data(#"{"ready":true,"message":"Subscription connected 👋"}"#.utf8)
    private let warning = Data("\u{1b}[31mERROR Ignoring unknown features requirement ultrafast_mode\u{1b}[0m\n".utf8)

    func testStderrWarningsCannotCorruptHealthyJSONInEitherArrivalOrder() {
        for warningFirst in [true, false] {
            var output = CodexStatusOutput()
            if warningFirst { output.appendStderr(warning) }
            output.appendStdout(healthy)
            if !warningFirst { output.appendStderr(warning) }
            let result = output.result(exitCode: 0)
            XCTAssertTrue(result.ready)
            XCTAssertEqual(result.message, "Subscription connected 👋")
        }
    }

    func testSplitUTF8AndInterleavedDiagnosticsRemainIndependent() {
        var output = CodexStatusOutput()
        for byte in healthy {
            output.appendStdout(Data([byte]))
            output.appendStderr(warning)
        }
        XCTAssertTrue(output.result(exitCode: 0).ready)
        XCTAssertEqual(output.result(exitCode: 0).message, "Subscription connected 👋")
    }

    func testActualAuthenticationErrorIsPreserved() {
        var output = CodexStatusOutput()
        output.appendStdout(Data(#"{"ready":false,"message":"Codex authentication rejected (401); run codex login."}"#.utf8))
        output.appendStderr(warning)
        for code in [Int32(0), Int32(1)] {
            let result = output.result(exitCode: code)
            XCTAssertFalse(result.ready)
            XCTAssertEqual(result.message, "Codex authentication rejected (401); run codex login.")
        }
    }

    func testNonzeroExitCannotReportReady() {
        var output = CodexStatusOutput()
        output.appendStdout(healthy)
        XCTAssertFalse(output.result(exitCode: 130).ready)
        XCTAssertTrue(output.result(exitCode: 130).message.contains("130"))
    }

    func testMalformedOrMissingStatusDoesNotInventAnAuthenticationFailure() {
        for data in [Data(), Data("not JSON".utf8), Data(#"{"ready":true}"#.utf8)] {
            var output = CodexStatusOutput()
            output.appendStdout(data)
            output.appendStderr(Data("Network diagnostic".utf8))
            let result = output.result(exitCode: 1)
            XCTAssertFalse(result.ready)
            XCTAssertTrue(result.message.contains("Could not read"))
            XCTAssertTrue(result.message.contains("Network diagnostic"))
            XCTAssertFalse(result.message.contains("codex login"))
        }
    }

    func testOutputIsBoundedAndTruncatedStdoutFailsClosed() {
        var output = CodexStatusOutput()
        output.appendStdout(healthy)
        output.appendStdout(Data(repeating: 32, count: 100000))
        output.appendStderr(Data(repeating: 65, count: 100000))
        output.appendStderr(Data("last diagnostic".utf8))
        XCTAssertEqual(output.stdout.count, 16384)
        XCTAssertEqual(output.stderr.count, 8192)
        XCTAssertTrue(String(decoding: output.stderr, as: UTF8.self).hasSuffix("last diagnostic"))
        XCTAssertTrue(output.stdoutTruncated)
        XCTAssertFalse(output.result(exitCode: 0).ready)
        XCTAssertTrue(output.result(exitCode: 0).message.contains("protocol limit"))
    }

    func testTimeoutNeverUsesAnOldReadyResultOrAsksForLogin() {
        var output = CodexStatusOutput()
        output.appendStdout(healthy)
        let result = output.result(exitCode: 130, timedOut: true)
        XCTAssertFalse(result.ready)
        XCTAssertTrue(result.message.contains("timed out"))
        XCTAssertFalse(result.message.contains("codex login"))
    }

    func testNativeTransportWithRealStdoutAndStderrPipes() throws {
        for warningFirst in [true, false] {
            let completed = expectation(description: "Separate real pipes")
            let diagnostic = "printf '%s\\n' 'ERROR Ignoring unknown features requirement ultrafast_mode' >&2"
            let status = "printf '%s\\n' '{\"ready\":true,\"message\":\"Live check verified\"}'"
            let script = warningFirst ? "\(diagnostic); \(status)" : "\(status); \(diagnostic)"
            // Only fixed test fixtures use a shell. Production launches cu directly.
            let child = try CodexReadinessProbe.start(executable: "/bin/sh", arguments: ["-c", script], environment: [:]) { result in
                XCTAssertTrue(result.ready)
                XCTAssertEqual(result.message, "Live check verified")
                completed.fulfill()
            }
            withExtendedLifetime(child) { wait(for: [completed], timeout: 5) }
        }
    }

    func testNativeTransportPreservesStructuredFailureWithWarnings() throws {
        let completed = expectation(description: "Real failure")
        let child = try CodexReadinessProbe.start(executable: "/bin/sh", arguments: ["-c",
            "printf '%s\\n' 'warning' >&2; printf '%s\\n' '{\"ready\":false,\"message\":\"Workspace rejected (401)\"}'; exit 1"], environment: [:]) { result in
            XCTAssertFalse(result.ready)
            XCTAssertEqual(result.message, "Workspace rejected (401)")
            completed.fulfill()
        }
        withExtendedLifetime(child) { wait(for: [completed], timeout: 5) }
    }

    func testNativeTransportDeadlineIsNotReportedAsInvalidLogin() throws {
        let completed = expectation(description: "Bounded status check")
        let child = try CodexReadinessProbe.start(executable: "/bin/sleep", arguments: ["2"], environment: [:], timeout: 0.03) { result in
            XCTAssertFalse(result.ready)
            XCTAssertTrue(result.message.contains("timed out"))
            XCTAssertFalse(result.message.contains("codex login"))
            completed.fulfill()
        }
        withExtendedLifetime(child) { wait(for: [completed], timeout: 3) }
    }

    func testLaunchFailureIsNotConvertedToLoginAdvice() {
        XCTAssertThrowsError(try CodexReadinessProbe.start(executable: "/nonexistent/cu-status", environment: [:], completion: { _ in
            XCTFail("A failed launch cannot produce a status result")
        }))
    }

    func testLiveInstalledEngineThroughNativeStatusTransport() throws {
        guard ProcessInfo.processInfo.environment["CU_LIVE_CODEX_STATUS_TEST"] == "1" else {
            throw XCTSkip("Opt-in live metadata-only check; no model turn or desktop access")
        }
        let completed = expectation(description: "Live installed engine through native probe")
        var env = ProcessInfo.processInfo.environment
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        env["PATH"] = "\(home)/.local/bin:\(home)/.cargo/bin:\(env["PATH"] ?? "")"
        let child = try CodexReadinessProbe.start(executable: "/Applications/Cu.app/Contents/Resources/cu-engine", environment: env) { result in
            XCTAssertTrue(result.ready, result.message)
            completed.fulfill()
        }
        withExtendedLifetime(child) { wait(for: [completed], timeout: 28) }
    }
}
