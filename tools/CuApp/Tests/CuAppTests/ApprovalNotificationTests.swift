import Foundation
import UserNotifications
import XCTest
@testable import CuApp

final class ApprovalNotificationTests: XCTestCase {
    private func waiting(kind: String = "action", prompt: UInt64 = 1, message: String = "clicking Send") throws -> Session {
        var session = Session(); session.begin(task: "PRIVATE_TASK person@example.com")
        let data = try JSONSerialization.data(withJSONObject: ["type":"confirmation", "id":prompt, "kind":kind, "message":message, "restore_app_pid":42])
        session.receive(try JSONDecoder().decode(EngineEvent.self, from:data), runID: session.id)
        return session
    }
    func testDefaultClickOnlyOpensAndExplicitApproveIsSingleUse() throws {
        let session = try waiting()
        var state = ApprovalNotificationState(); state.update(session)
        let notice = try XCTUnwrap(state.current)
        XCTAssertEqual(state.route(identifier:notice.identifier, info:notice.userInfo, action:UNNotificationDefaultActionIdentifier), .open(session.id,1))
        XCTAssertEqual(session.phase,.waiting)
        XCTAssertEqual(state.route(identifier:notice.identifier, info:notice.userInfo, action:ApprovalNotificationState.approve), .respond(session.id,1,true))
        XCTAssertNil(state.route(identifier:notice.identifier, info:notice.userInfo, action:ApprovalNotificationState.approve))
        XCTAssertNil(state.route(identifier:notice.identifier, info:notice.userInfo, action:ApprovalNotificationState.deny))
        XCTAssertFalse(state.canPresent)
    }
    func testDismissAndUnknownActionsNeverApproveOrDeny() throws {
        var state = ApprovalNotificationState(); state.update(try waiting())
        let notice = try XCTUnwrap(state.current)
        for action in [UNNotificationDismissActionIdentifier,"", "untrusted-action"] {
            XCTAssertNil(state.route(identifier:notice.identifier, info:notice.userInfo, action:action))
        }
        XCTAssertTrue(state.canPresent)
        XCTAssertNotNil(state.route(identifier:notice.identifier, info:notice.userInfo, action:ApprovalNotificationState.deny))
    }
    func testOldRunWithReusedPromptIDAndOldAppLaunchCannotControlNewTask() throws {
        var state = ApprovalNotificationState(); let old = try waiting(); state.update(old)
        let notice = try XCTUnwrap(state.current)
        let next = try waiting(); state.update(next)
        XCTAssertNil(state.route(identifier:notice.identifier, info:notice.userInfo, action:ApprovalNotificationState.approve))
        XCTAssertNotEqual(old.id,next.id)
        var newLaunch = ApprovalNotificationState(); newLaunch.update(old)
        XCTAssertNil(newLaunch.route(identifier:notice.identifier, info:notice.userInfo, action:ApprovalNotificationState.approve))
    }
    func testStopCompletionAndAlreadyAnsweredPromptInvalidateNotice() throws {
        for outcome in 0..<3 {
            var session = try waiting(); var state = ApprovalNotificationState(); state.update(session)
            let notice = try XCTUnwrap(state.current)
            switch outcome {
            case 0: session.stop()
            case 1: session.finish(runID:session.id, code:1,error:"Stopped")
            default: XCTAssertTrue(session.respond(runID:session.id,promptID:1))
            }
            state.update(session)
            XCTAssertNil(state.current)
            XCTAssertNil(state.route(identifier:notice.identifier,info:notice.userInfo,action:ApprovalNotificationState.approve))
        }
    }
    func testMalformedAndForgedIdentityIsRejectedWithoutConsumingRealRequest() throws {
        var state = ApprovalNotificationState(); state.update(try waiting())
        let notice = try XCTUnwrap(state.current)
        for key in ["launch","run","prompt","extra"] {
            var info = notice.userInfo; info[key] = "forged"
            XCTAssertNil(state.route(identifier:notice.identifier, info:info, action:ApprovalNotificationState.approve))
        }
        XCTAssertNil(state.route(identifier:"wrong",info:notice.userInfo,action:ApprovalNotificationState.approve))
        XCTAssertNotNil(state.route(identifier:notice.identifier,info:notice.userInfo,action:ApprovalNotificationState.approve))
    }
    func testPrivateContentAndSecretsNeverEnterNotification() throws {
        let secret = "sk-SYNTHETIC_PRIVATE_ONLY_12345678"
        let session = try waiting(message:"typed text looks like an API key or private key\ntype \(secret) recipient@example.com PRIVATE_BODY")
        var state = ApprovalNotificationState();state.update(session)
        let notice = try XCTUnwrap(state.current)
        let text = notice.title + notice.body + notice.userInfo.values.joined()
        XCTAssertFalse(text.contains(secret));XCTAssertFalse(text.contains("PRIVATE"));XCTAssertFalse(text.contains("example.com"))
        XCTAssertTrue(notice.title.contains("Sensitive"))
    }
    func testPauseAndMaxPromptIDsAreNotAutoResumedOrRounded() throws {
        let session = try waiting(kind:"pause",prompt:UInt64.max)
        var state = ApprovalNotificationState();state.update(session)
        let notice = try XCTUnwrap(state.current)
        XCTAssertEqual(notice.userInfo["prompt"],String(UInt64.max))
        XCTAssertEqual(notice.kind,"pause")
        XCTAssertEqual(state.route(identifier:notice.identifier,info:notice.userInfo,action:ApprovalNotificationState.deny),.respond(session.id,UInt64.max,false))
    }
    @MainActor func testNotificationButtonsRequireAuthenticationAndDoNotOpenControlsToApprove() {
        let categories=ApprovalNotifications.categories()
        XCTAssertEqual(categories.count,3)
        XCTAssertEqual(categories.first { $0.identifier == "CU_COMPLETION" }?.actions.count,0)
        for category in categories where category.identifier != "CU_COMPLETION" {
            XCTAssertEqual(category.actions.count,2)
            for action in category.actions {
                XCTAssertTrue(action.options.contains(.authenticationRequired))
                XCTAssertFalse(action.options.contains(.foreground))
            }
        }
    }
    @MainActor func testDefaultModeIsAllowAllWithoutNotificationPermissionAndStaleRevealDoesNothing() {
        let model = AppModel()
        XCTAssertEqual(model.mode,.auto)
        XCTAssertEqual(model.mode.title,"Allow all actions")
        XCTAssertFalse(ApprovalNotifications.requestsPermissionAtLaunch)
        XCTAssertFalse(model.revealApproval(runID:UUID(),promptID:1))
        XCTAssertEqual(model.approvalRevealNonce,0)
    }
}
