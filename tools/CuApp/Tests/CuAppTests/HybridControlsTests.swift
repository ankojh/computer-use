import Foundation
import XCTest
@testable import CuApp

@MainActor
final class HybridControlsTests: XCTestCase {
    func testAllowAllIsAnExplicitRunArgumentAndDoesNotChangeDefault() {
        let model = AppModel()
        XCTAssertEqual(model.mode, .confirmRisky)
        let args = AppModel.taskArguments(task: "Synthetic task", mode: .auto, hybrid: false, publicBrief: "")
        let index = args.firstIndex(of: "--mode")!
        XCTAssertEqual(args[index + 1], "auto")
        XCTAssertEqual(model.mode, .confirmRisky)
    }
    func testPublicBriefIsSeparateAndNeverFilledFromPrivateTask() {
        let task = "PRIVATE_EMAIL recipient@example.com; compose only"
        let noBrief = AppModel.taskArguments(task: task, mode: .confirmRisky, hybrid: true, publicBrief: "")
        XCTAssertTrue(noBrief.contains("--hybrid"))
        XCTAssertFalse(noBrief.contains("--public-research"))
        let withBrief = AppModel.taskArguments(task: task, mode: .confirmRisky, hybrid: true, publicBrief: "Rust public ownership documentation")
        let index = withBrief.firstIndex(of: "--public-research")!
        XCTAssertEqual(withBrief[index + 1], "Rust public ownership documentation")
        XCTAssertEqual(withBrief.filter { $0.contains("PRIVATE_EMAIL") }.count, 1) // Private planner argument ONLY.
        let disabled = AppModel.taskArguments(task: task, mode: .observe, hybrid: false, publicBrief: "Rust public docs")
        XCTAssertFalse(disabled.contains("--public-research"))
        XCTAssertFalse(disabled.contains("--hybrid"))
    }
    func testHelperReportIsRunScopedAndCannotApproveOrComplete() throws {
        var session = Session(); session.begin(task: "Private task")
        let id = session.id
        let report = try JSONDecoder().decode(EngineEvent.self, from: Data(#"{"type":"advisor","advisor":{"id":"public-1","provider":"parallel","kind":"public_research","status":"ok","summary":"Public sources only, not delivery","latency_ms":500,"requests_used":4,"sources":[{"url":"https://doc.rust-lang.org/book/","title":"Rust"}]}}"#.utf8))
        session.receive(report, runID: UUID())
        XCTAssertTrue(session.advisors.isEmpty)
        session.receive(report, runID: id)
        XCTAssertEqual(session.advisors.count, 1)
        XCTAssertEqual(session.phase, .running)
        XCTAssertNil(session.summary)
        XCTAssertNil(session.pending)
        XCTAssertEqual(session.actionsExecuted, 0)
        session.receive(report, runID: id)
        XCTAssertEqual(session.advisors.count, 1)
        session.stop(); session.receive(report, runID: id)
        XCTAssertEqual(session.phase, .stopping)
    }
    func testPublicResearchAndMetadataStagesAreObservedNotPercentages() throws {
        var session = Session(); session.begin(task: "Task")
        let id = session.id
        for (stage, label) in [("researching", "Researching the public web"), ("judging", "Checking UI metadata")] {
            let event = try JSONDecoder().decode(EngineEvent.self, from: Data("{\"type\":\"task_progress\",\"progress\":{\"stage\":\"\(stage)\",\"step\":2,\"actions_executed\":0}}".utf8))
            session.receive(event, runID: id)
            XCTAssertEqual(session.stageLabel, label)
            XCTAssertEqual(session.actionsExecuted, 0)
        }
    }
}
