import Foundation
import XCTest
import UserNotifications
@testable import CuApp

final class CompletionStatusTests: XCTestCase {
    private func event(_ value: [String:Any]) throws -> EngineEvent {
        try JSONDecoder().decode(EngineEvent.self,from:JSONSerialization.data(withJSONObject:value))
    }
    private func session(status:String="done",exit:Int32?=0,verified:Bool=true) throws -> Session {
        var s=Session();s.begin(task:"PRIVATE_TASK_CANARY")
        let id=s.id
        if verified {
            s.receive(try event(["type":"verification","verification":["verdict":"verified","summary":"PRIVATE_RESULT_CANARY","model":"synthetic","reasoning":"medium","latency_ms":1,"checks":[["requirement":"Synthetic requirement","verdict":"verified","evidence":"Synthetic evidence"]]]]),runID:id)
        }
        s.receive(try event(["type":"done","summary":["status":status,"steps":6,"actions_executed":5,"wall_ms":1234,"final_text":"PRIVATE_RESULT_CANARY","message":"PRIVATE_ERROR_CANARY"]]),runID:id)
        if let exit {s.finish(runID:id,code:exit,error:"")}
        return s
    }
    func testSummaryAndReviewCannotFinishOrClearBeforeProcessExit() throws {
        let s=try session(exit:nil)
        XCTAssertTrue(s.active);XCTAssertFalse(s.verifiedSuccess)
        XCTAssertNil(CompletionNotice.make(session:s,launchID:UUID()))
        XCTAssertFalse(TaskInputCompletion.shouldClear(session:s,runID:s.id,draft:s.task))
        XCTAssertFalse(TaskStatusPresentation(session:s).title.contains("Done"))
    }
    func testVerifiedSuccessHasPersistentDoneZeroRemainingAndClearsOnlySubmittedDraft() throws {
        let s=try session()
        XCTAssertTrue(s.verifiedSuccess)
        let presentation=TaskStatusPresentation(session:s)
        XCTAssertEqual(presentation.title," Cu ✓ Done")
        XCTAssertTrue(presentation.remaining.hasPrefix("0"))
        XCTAssertTrue(presentation.tooltip.contains("6"));XCTAssertTrue(presentation.tooltip.contains("5"))
        XCTAssertTrue(TaskInputCompletion.shouldClear(session:s,runID:s.id,draft:"  PRIVATE_TASK_CANARY\n"))
        XCTAssertFalse(TaskInputCompletion.shouldClear(session:s,runID:s.id,draft:"A newer edited task"))
        XCTAssertFalse(TaskInputCompletion.shouldClear(session:s,runID:UUID(),draft:s.task))
        let notice=try XCTUnwrap(CompletionNotice.make(session:s,launchID:UUID()))
        XCTAssertEqual(notice.kind,.done);XCTAssertEqual(notice.steps,6);XCTAssertEqual(notice.inputs,5)
    }
    func testNonzeroExitCannotBecomeSuccessNotificationOrClearInput() throws {
        let s=try session(exit:1)
        XCTAssertEqual(s.phase,.error);XCTAssertFalse(s.verifiedSuccess)
        XCTAssertTrue(s.failureReason?.contains("exited with code 1") == true)
        XCTAssertFalse(TaskInputCompletion.shouldClear(session:s,runID:s.id,draft:s.task))
        XCTAssertEqual(CompletionNotice.make(session:s,launchID:UUID())?.kind,.error)
        XCTAssertFalse(TaskStatusPresentation(session:s).title.contains("Done"))
    }
    func testUnverifiedFailureNeedsInputAndStoppedNeverClaimDoneOrClearDraft() throws {
        for (status,verified,kind) in [("done",false,CompletionNotice.Kind.unverified),("error",true,.error),("needs_input",true,.needsInput),("aborted",true,.stopped)] {
            let s=try session(status:status,verified:verified)
            XCTAssertFalse(s.verifiedSuccess)
            XCTAssertEqual(CompletionNotice.make(session:s,launchID:UUID())?.kind,kind)
            XCTAssertFalse(TaskInputCompletion.shouldClear(session:s,runID:s.id,draft:s.task))
            XCTAssertFalse(TaskStatusPresentation(session:s).remaining.hasPrefix("0"))
        }
    }
    func testCompletionNoticeNeverContainsTaskResultsErrorsOrApprovalActions() throws {
        for status in ["done","error","needs_input"] {
            let s=try session(status:status)
            let n=try XCTUnwrap(CompletionNotice.make(session:s,launchID:UUID()))
            let text=n.title+n.body+n.userInfo.values.joined()
            XCTAssertFalse(text.contains("PRIVATE_"));XCTAssertFalse(text.contains("Approve"))
            XCTAssertEqual(Set(n.userInfo.keys),Set(["launch","run"]))
        }
    }
    func testNotificationClickOnlyOpensExactCurrentRunAndIsSingleUse() throws {
        let s=try session();var state=CompletionNotificationState();state.update(s)
        let n=try XCTUnwrap(state.current)
        XCTAssertNil(state.route(identifier:n.identifier,info:n.userInfo,action:ApprovalNotificationState.approve))
        XCTAssertNil(state.route(identifier:n.identifier,info:n.userInfo,action:UNNotificationDismissActionIdentifier))
        var forged=n.userInfo;forged["run"]=UUID().uuidString
        XCTAssertNil(state.route(identifier:n.identifier,info:forged,action:UNNotificationDefaultActionIdentifier))
        XCTAssertNil(state.route(identifier:"wrong",info:n.userInfo,action:UNNotificationDefaultActionIdentifier))
        XCTAssertEqual(state.route(identifier:n.identifier,info:n.userInfo,action:UNNotificationDefaultActionIdentifier),s.id)
        state.update(s) // A UI rerender cannot revive a consumed notice.
        XCTAssertNil(state.route(identifier:n.identifier,info:n.userInfo,action:UNNotificationDefaultActionIdentifier))
    }
    func testOldLaunchAndNewTaskInvalidateCompletionNotice() throws {
        var s=try session();var first=CompletionNotificationState();first.update(s)
        let old=try XCTUnwrap(first.current)
        var newLaunch=CompletionNotificationState();newLaunch.update(s)
        XCTAssertNil(newLaunch.route(identifier:old.identifier,info:old.userInfo,action:UNNotificationDefaultActionIdentifier))
        s.begin(task:"Next task");first.update(s)
        XCTAssertNil(first.current)
        XCTAssertNil(first.route(identifier:old.identifier,info:old.userInfo,action:UNNotificationDefaultActionIdentifier))
        XCTAssertEqual(s.phase,.running)
    }
    func testLiveProgressReportsUnknownRemainingAndObservedCounts() throws {
        var s=Session();XCTAssertEqual(TaskStatusPresentation(session:s).remaining,"Not started")
        s.begin(task:"Synthetic task");let id=s.id
        s.receive(try event(["type":"task_progress","progress":["stage":"executing","step":4,"actions_executed":7]]),runID:id)
        let p=TaskStatusPresentation(session:s)
        XCTAssertTrue(p.title.contains("Step 4"));XCTAssertTrue(p.title.contains("7 inputs"))
        XCTAssertTrue(p.remaining.contains("Unknown"));XCTAssertFalse(p.tooltip.contains("%"))
        s.receive(try event(["type":"task_progress","progress":["stage":"verifying","step":5,"actions_executed":7]]),runID:id)
        XCTAssertTrue(TaskStatusPresentation(session:s).title.contains("Verify"))
        XCTAssertFalse(s.verifiedSuccess)
    }
    func testStoppedWithSuccessSummaryDoesNotClearOrNotifyDone() throws {
        var s=try session(exit:nil);s.stop();s.finish(runID:s.id,code:0,error:"")
        XCTAssertEqual(s.phase,.aborted);XCTAssertFalse(s.verifiedSuccess)
        XCTAssertEqual(CompletionNotice.make(session:s,launchID:UUID())?.kind,.stopped)
        XCTAssertFalse(TaskInputCompletion.shouldClear(session:s,runID:s.id,draft:s.task))
    }
    @MainActor func testCompletionCategoryHasNoApproveButtonsAndDefaultStaysAutomatic() {
        let category=ApprovalNotifications.categories().first {$0.identifier=="CU_COMPLETION"}
        XCTAssertNotNil(category);XCTAssertTrue(category?.actions.isEmpty == true)
        XCTAssertEqual(AppModel().mode,.auto)
        XCTAssertFalse(ApprovalNotifications.requestsPermissionAtLaunch)
    }
}
