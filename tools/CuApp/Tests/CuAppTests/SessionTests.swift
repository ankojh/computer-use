import XCTest
@testable import CuApp

final class SessionTests: XCTestCase {
    private func event(_ json: String) throws -> EngineEvent {
        try JSONDecoder().decode(EngineEvent.self, from: Data(json.utf8))
    }

    func testApprovalIsExplicitAndScopedToRunAndPrompt() throws {
        var session = Session()
        session.begin(task: "First task")
        let first = session.id
        session.receive(try event(#"{"type":"confirmation","id":1,"kind":"action","message":"Send?"}"#), runID: first)
        XCTAssertEqual(session.phase, .waiting)
        XCTAssertFalse(session.respond(runID: UUID(), promptID: 1))
        XCTAssertFalse(session.respond(runID: first, promptID: 2))
        XCTAssertTrue(session.respond(runID: first, promptID: 1))
        XCTAssertFalse(session.respond(runID: first, promptID: 1))
        session.stop()
        session.finish(runID: first, code: 130, error: "")
        session.begin(task: "Second task")
        let second = session.id
        XCTAssertNotEqual(first, second)
        session.receive(try event(#"{"type":"confirmation","id":1,"kind":"action","message":"Delete?"}"#), runID: second)
        XCTAssertFalse(session.respond(runID: first, promptID: 1))
        session.receive(try event(#"{"type":"progress","message":"stale"}"#), runID: first)
        XCTAssertFalse(session.activity.contains { $0.text == "stale" })
        XCTAssertEqual(session.pending?.message, "Delete?")
    }

    func testStopClearsApprovalAndLateEventsCannotUndoStop() throws {
        var session = Session()
        session.begin(task: "Task")
        let id = session.id
        let confirmation = try event(#"{"type":"confirmation","id":1,"kind":"pause","message":"Resume?"}"#)
        session.receive(confirmation, runID: id)
        session.stop()
        session.receive(confirmation, runID: id)
        XCTAssertNil(session.pending)
        XCTAssertFalse(session.respond(runID: id, promptID: 1))
        XCTAssertEqual(session.phase, .stopping)
        XCTAssertTrue(session.active)
        session.finish(runID: id, code: 130, error: "interrupted")
        XCTAssertEqual(session.phase, .aborted)
        XCTAssertFalse(session.active)
    }

    func testDoneWaitsForProcessExitAndRetainsFinalAnswer() throws {
        var session = Session()
        session.begin(task: "Task")
        let id = session.id
        session.receive(try event(#"{"type":"done","summary":{"status":"done","message":null,"final_text":"All set","steps":2,"actions_executed":3,"wall_ms":1001}}"#), runID: id)
        XCTAssertTrue(session.active)
        XCTAssertEqual(session.summary?.final_text, "All set")
        session.finish(runID: id, code: 0, error: "")
        XCTAssertEqual(session.phase, .done)
        XCTAssertFalse(session.active)
    }

    func testExitWithoutSummaryIsAnErrorEvenWithZeroStatus() {
        var session = Session()
        session.begin(task: "Task")
        session.finish(runID: session.id, code: 0, error: "Missing Screen Recording permission")
        XCTAssertEqual(session.phase, .error)
        XCTAssertEqual(session.latest, "Missing Screen Recording permission")
    }

    func testActivityAndMessageSizesAreBounded() {
        var session = Session()
        for _ in 0..<250 { session.append(String(repeating: "a", count: 17000)) }
        XCTAssertEqual(session.activity.count, 200)
        XCTAssertEqual(session.latest.count, 16000)
    }

    func testRouteIsVisibleButCannotApproveAnAction() throws {
        var session = Session()
        session.begin(task: "Task")
        let id = session.id
        session.receive(try event(#"{"type":"confirmation","id":1,"kind":"action","message":"Send?"}"#), runID: id)
        session.receive(try event(#"{"type":"route","route":{"model":"gpt-6-astra","reasoning":"high","tier":"complex","reason":"Verify sender identity.","router_model":"gpt-6-luna","router_reasoning":"low","router_latency_ms":500,"planner_latency_ms":2000}}"#), runID: id)
        XCTAssertEqual(session.route?.model, "gpt-6-astra")
        XCTAssertEqual(session.route?.reasoning, "high")
        XCTAssertTrue(session.latest.contains("Verify sender identity"))
        XCTAssertEqual(session.phase, .waiting)
        XCTAssertEqual(session.pending?.id, 1)
        session.begin(task: "Another task")
        XCTAssertNil(session.route)
    }

    func testEmptyProgressCannotEraseCurrentRoutingMessage() throws {
        var session = Session()
        session.begin(task: "Task")
        session.append("Using Astra")
        session.receive(try event(#"{"type":"progress","message":""}"#), runID: session.id)
        XCTAssertEqual(session.latest, "Using Astra")
    }

    func testCursorMetadataCannotApproveAndClearsOnPauseAndStop() throws {
        var session = Session()
        session.begin(task: "Task")
        let id = session.id
        let cursor = try event(#"{"type":"cursor","cursor":{"point":{"x":120,"y":180},"phase":"target"}}"#)
        session.receive(cursor, runID: id)
        XCTAssertEqual(session.cursor?.point.x, 120)
        session.receive(try event(#"{"type":"confirmation","id":3,"kind":"action","message":"Send?"}"#), runID: id)
        session.receive(cursor, runID: id)
        XCTAssertEqual(session.phase, .waiting)
        XCTAssertEqual(session.pending?.id, 3)
        session.receive(try event(#"{"type":"confirmation","id":4,"kind":"pause","message":"Mouse moved"}"#), runID: id)
        XCTAssertNil(session.cursor)
        session.receive(cursor, runID: id)
        session.stop()
        XCTAssertNil(session.cursor)
        session.receive(cursor, runID: id)
        XCTAssertNil(session.cursor)
    }

    func testCursorClearAndStaleRunEvents() throws {
        var session = Session()
        session.begin(task: "Task")
        let id = session.id
        let cursor = try event(#"{"type":"cursor","cursor":{"point":{"x":120,"y":180},"phase":"active"}}"#)
        session.receive(cursor, runID: id)
        XCTAssertNotNil(session.cursor)
        session.receive(try event(#"{"type":"cursor","cursor":null}"#), runID: id)
        XCTAssertNil(session.cursor)
        session.begin(task: "Next")
        session.receive(cursor, runID: id)
        XCTAssertNil(session.cursor)
        session.receive(cursor, runID: session.id)
        session.finish(runID: session.id, code: 1, error: "Synthetic failure")
        XCTAssertNil(session.cursor)
    }

    func testNeedsInputIsNotFinishedAndKeepsTheQuestionVisible() throws {
        var session = Session()
        session.begin(task: "Research and send")
        let id = session.id
        session.receive(try event(#"{"type":"done","summary":{"status":"needs_input","message":"Which RSI do you mean?","final_text":"Which RSI do you mean?","steps":4,"actions_executed":5,"wall_ms":50000}}"#), runID: id)
        XCTAssertTrue(session.active)
        session.finish(runID: id, code: 1, error: "")
        XCTAssertEqual(session.phase, .needsInput)
        XCTAssertFalse(session.active)
        XCTAssertEqual(session.latest, "Which RSI do you mean?")
        XCTAssertNil(session.pending)
    }

    func testVerificationCannotApproveOrFinishAnActionAndIsRunScoped() throws {
        var session = Session()
        session.begin(task: "Task")
        let id = session.id
        session.receive(try event(#"{"type":"confirmation","id":7,"kind":"action","message":"Send?"}"#), runID: id)
        let review = try event(#"{"type":"verification","verification":{"verdict":"verified","summary":"Evidence checked.","model":"gpt-6-luna","reasoning":"medium","latency_ms":1200,"checks":[{"requirement":"Outcome","verdict":"verified","evidence":"Receipt"}]}}"#)
        session.receive(review, runID: id)
        XCTAssertEqual(session.verification?.reasoning, "medium")
        XCTAssertEqual(session.phase, .waiting)
        XCTAssertEqual(session.pending?.id, 7)
        session.begin(task: "Next")
        session.receive(review, runID: id)
        XCTAssertNil(session.verification)
    }

    func testAuthenticationFailureIsSpecificAndTheCauseIsNotReplacedByCounts() throws {
        var session = Session()
        session.begin(task: "Task")
        let id = session.id
        session.receive(try event(#"{"type":"done","summary":{"status":"error","message":"Codex account/read: workspace routing discovery unauthorized (401)","final_text":null,"steps":0,"actions_executed":0,"wall_ms":1491}}"#), runID: id)
        session.finish(runID: id, code: 1, error: "")
        XCTAssertEqual(session.phase, .signInNeeded)
        XCTAssertTrue(session.latest.contains("401"))
        XCTAssertEqual(session.failureReason, session.latest)
        XCTAssertEqual(session.step, 0)
        XCTAssertEqual(session.actionsExecuted, 0)
    }

    func testStructuredProgressIsRunScopedAndCannotApproveOrFinish() throws {
        var session = Session()
        session.begin(task: "Task")
        let id = session.id
        let progress = try event(#"{"type":"task_progress","progress":{"stage":"executing","step":2,"actions_executed":3}}"#)
        session.receive(progress, runID: id)
        XCTAssertEqual(session.stageLabel, "Executing")
        XCTAssertEqual(session.step, 2)
        XCTAssertEqual(session.actionsExecuted, 3)
        session.receive(try event(#"{"type":"confirmation","id":1,"kind":"action","message":"Send?","restore_app_pid":42}"#), runID: id)
        session.receive(progress, runID: id)
        XCTAssertEqual(session.phase, .waiting)
        XCTAssertEqual(session.pending?.restoreAppPID, 42)
        XCTAssertEqual(session.stageLabel, "Needs approval")
        session.stop()
        session.receive(progress, runID: id)
        XCTAssertEqual(session.phase, .stopping)
        session.finish(runID: id, code: 130, error: "")
        session.begin(task: "Next")
        session.receive(progress, runID: id)
        XCTAssertNil(session.progress)
        XCTAssertEqual(session.step, 0)
        XCTAssertEqual(session.actionsExecuted, 0)
    }

    func testFinalSummaryOverridesAttemptedStepAndGenericErrorsKeepTheirCause() throws {
        var session = Session()
        session.begin(task: "Task")
        let id = session.id
        session.receive(try event(#"{"type":"task_progress","progress":{"stage":"planning","step":1,"actions_executed":0}}"#), runID: id)
        session.receive(try event(#"{"type":"done","summary":{"status":"error","message":"Connection failed","final_text":null,"steps":0,"actions_executed":0,"wall_ms":1000}}"#), runID: id)
        session.finish(runID: id, code: 1, error: "")
        XCTAssertEqual(session.phase, .error)
        XCTAssertEqual(session.step, 0)
        XCTAssertEqual(session.latest, "Connection failed")
    }

    func testLiveReadinessIsStructuredNotCachedLoginText() throws {
        let data = Data(#"{"ready":false,"message":"Codex authentication rejected (401)"}"#.utf8)
        let result = try JSONDecoder().decode(CodexReadiness.self, from: data)
        XCTAssertFalse(result.ready)
        XCTAssertTrue(result.message.contains("401"))
        XCTAssertThrowsError(try JSONDecoder().decode(CodexReadiness.self, from: Data("Logged in using ChatGPT".utf8)))
    }

    func testNativeModesExposeExplicitAllowAllWithoutChangingDefaultOrder() {
        XCTAssertEqual(ControlMode.allCases.map(\.rawValue), ["confirm-risky", "confirm-all", "observe", "auto"])
        XCTAssertEqual(ControlMode.auto.title, "Allow all actions")
        XCTAssertTrue(ControlMode.auto.detail.contains("run automatically"))
        XCTAssertTrue(ControlMode.auto.detail.contains("hard denials"))
        XCTAssertTrue(ControlMode.auto.detail.contains("relaunch defaults to Critical actions only"))
    }
}
