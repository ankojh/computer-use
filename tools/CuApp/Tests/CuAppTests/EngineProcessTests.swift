import XCTest
import Darwin
@testable import CuApp

final class EngineProcessTests: XCTestCase {
    func testDirectArgumentsNeverGoThroughAShell() throws {
        let complete = expectation(description: "native process exited")
        let child = EngineProcess()
        var output = Data()
        let task = "text with 'quotes', $(echo not-executed), ; and 👋"
        try child.start(executable: "/usr/bin/printf", arguments: ["%s", task], environment: [:],
            stdout: { output.append($0) }, stderr: { _ in XCTFail("unexpected stderr") }, completion: { code in
                XCTAssertEqual(code, 0)
                XCTAssertEqual(String(decoding: output, as: UTF8.self), task)
                complete.fulfill()
            })
        wait(for: [complete], timeout: 5)
    }

    func testPipeApprovalAndEOF() throws {
        let complete = expectation(description: "EOF releases the child")
        let child = EngineProcess()
        var output = Data()
        try child.start(executable: "/bin/cat", arguments: [], environment: [:],
            stdout: { output.append($0) }, stderr: { _ in }, completion: { code in
                XCTAssertEqual(code, 0)
                XCTAssertEqual(String(decoding: output, as: UTF8.self), "{\"id\":1,\"approved\":false}\n")
                complete.fulfill()
            })
        XCTAssertTrue(child.send(Data("{\"id\":1,\"approved\":false}\n".utf8)))
        child.closeInput()
        wait(for: [complete], timeout: 5)
        XCTAssertFalse(child.send(Data("late".utf8)))
    }

    func testStopKillsStubbornProcessGroupIncludingDescendants() throws {
        let ready = expectation(description: "descendant started")
        let complete = expectation(description: "stopped")
        let child = EngineProcess()
        var output = ""
        var descendant: pid_t = 0
        // Only a fixed test fixture uses a shell. Production launches cu directly.
        try child.start(executable: "/bin/sh",
            arguments: ["-c", "trap '' INT TERM; sleep 60 & echo $!; wait"],
            environment: ["PATH": "/usr/bin:/bin"], stdout: { data in
                output += String(decoding: data, as: UTF8.self)
                if descendant == 0, let pid = Int32(output.trimmingCharacters(in: .whitespacesAndNewlines)) {
                    descendant = pid
                    ready.fulfill()
                }
            }, stderr: { _ in }, completion: { code in
                XCTAssertNotEqual(code, 0)
                complete.fulfill()
            })
        wait(for: [ready], timeout: 5)
        child.stop(grace: 0.1)
        wait(for: [complete], timeout: 5)
        XCTAssertGreaterThan(descendant, 0)
        // A reparented zombie may briefly exist, but it must not be running.
        let state = Process()
        state.executableURL = URL(fileURLWithPath: "/bin/ps")
        state.arguments = ["-o", "stat=", "-p", String(descendant)]
        let pipe = Pipe()
        state.standardOutput = pipe
        try state.run()
        state.waitUntilExit()
        let status = String(decoding: pipe.fileHandleForReading.readDataToEndOfFile(), as: UTF8.self)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        XCTAssertTrue(status.isEmpty || status.hasPrefix("Z"), "Descendant still running: \(status)")
    }

    func testLaunchFailureIsReported() {
        let child = EngineProcess()
        XCTAssertThrowsError(try child.start(executable: "/nonexistent/cu-test", arguments: [], environment: [:],
                                             stdout: { _ in }, stderr: { _ in }, completion: { _ in }))
    }
}
