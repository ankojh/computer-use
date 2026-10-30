import XCTest
@testable import CuApp

final class EngineEventStreamTests: XCTestCase {
    func testEveryByteCanArriveSeparatelyIncludingUnicode() throws {
        var stream = EngineEventStream()
        let wire = Data("{\"type\":\"progress\",\"message\":\"Hello 👋\"}\n{\"type\":\"confirmation\",\"id\":42,\"kind\":\"action\",\"message\":\"Send?\"}\n".utf8)
        var events: [EngineEvent] = []
        for byte in wire { events += try stream.append(Data([byte])) }
        XCTAssertEqual(events.count, 2)
        XCTAssertEqual(events[0].message, "Hello 👋")
        XCTAssertEqual(events[1].id, 42)
    }

    func testMalformedLinesCannotBecomeApprovals() throws {
        var stream = EngineEventStream()
        let events = try stream.append(Data("garbage\n{\"type\":\"confirmation\"}\n".utf8))
        var session = Session()
        session.begin(task: "Task")
        for event in events { session.receive(event, runID: session.id) }
        XCTAssertNil(session.pending)
        XCTAssertEqual(session.phase, .running)
    }

    func testBoundsCompleteAndIncompleteLines() {
        var incomplete = EngineEventStream()
        XCTAssertThrowsError(try incomplete.append(Data(repeating: 65, count: 1024 * 1024 + 1)))
        var complete = EngineEventStream()
        XCTAssertThrowsError(try complete.append(Data(repeating: 65, count: 1024 * 1024 + 1) + Data([10])))
    }

    func testWireApprovalRoundTripAndSummary() throws {
        let finished = expectation(description: "fake native engine exited")
        var stream = EngineEventStream()
        var session = Session()
        session.begin(task: "Synthetic task — no real computer input")
        let runID = session.id
        let child = EngineProcess()
        // A deterministic wire-protocol fixture: waits for a native approval, then exits.
        let fixture = #"""
        printf '%s\n' '{"type":"confirmation","id":7,"kind":"action","message":"Synthetic action?"}'
        IFS= read -r reply
        printf '%s\n' "$reply" >&2
        printf '%s\n' '{"type":"done","summary":{"status":"done","message":null,"final_text":"Synthetic result","steps":1,"actions_executed":0,"wall_ms":5}}'
        """#
        var response = Data()
        try child.start(executable: "/bin/sh", arguments: ["-c", fixture], environment: [:], stdout: { data in
            do {
                for event in try stream.append(data) {
                    session.receive(event, runID: runID)
                    if let prompt = session.pending {
                        XCTAssertEqual(prompt.id, 7)
                        XCTAssertTrue(session.respond(runID: runID, promptID: prompt.id))
                        XCTAssertTrue(child.send(Data("{\"id\":7,\"approved\":false}\n".utf8)))
                    }
                }
            } catch { XCTFail("Unexpected protocol error: \(error)") }
        }, stderr: { response.append($0) }, completion: { code in
            session.finish(runID: runID, code: code, error: "")
            XCTAssertEqual(code, 0)
            XCTAssertEqual(session.phase, .done)
            XCTAssertEqual(session.summary?.final_text, "Synthetic result")
            XCTAssertEqual(String(decoding: response, as: UTF8.self), "{\"id\":7,\"approved\":false}\n")
            finished.fulfill()
        })
        wait(for: [finished], timeout: 5)
    }
}
