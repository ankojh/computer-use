import Foundation
import UserNotifications

/// A fixed outcome and host counts only. Never task text, recipients or results.
struct CompletionNotice: Equatable {
    enum Kind: String { case done, needsInput, error, stopped, unverified }
    let launchID: UUID
    let runID: UUID
    let kind: Kind
    let steps: Int
    let inputs: Int
    var identifier: String { "cu.completion.\(launchID.uuidString).\(runID.uuidString)" }
    var userInfo: [String:String] { ["launch":launchID.uuidString,"run":runID.uuidString] }
    var title: String {
        switch kind {
        case .done: return "Cu — task finished ✓"
        case .needsInput: return "Cu — task needs your input"
        case .error: return "Cu — task ended with an error"
        case .stopped: return "Cu — task stopped"
        case .unverified: return "Cu — completion not verified"
        }
    }
    var body: String {
        let outcome = kind == .done ? "Verified complete. Remaining: 0." : "The task is not verified complete."
        return "\(outcome) \(steps) planning steps · \(inputs) posted input actions. Click to view Cu's results."
    }
    static func make(session: Session, launchID: UUID) -> CompletionNotice? {
        guard session.ended else { return nil }
        let kind: Kind
        switch session.phase {
        case .done: kind = session.verifiedSuccess ? .done : .unverified
        case .needsInput: kind = .needsInput
        case .error, .signInNeeded: kind = .error
        case .aborted: kind = .stopped
        default: return nil
        }
        return CompletionNotice(launchID:launchID,runID:session.id,kind:kind,steps:max(0,session.step),inputs:max(0,session.actionsExecuted))
    }
}

/// At most one notice for the current terminal run. A new task invalidates it.
struct CompletionNotificationState {
    let launchID = UUID()
    private(set) var current: CompletionNotice?
    private var consumed = false
    mutating func update(_ session: Session) {
        let notice = CompletionNotice.make(session:session,launchID:launchID)
        if notice != current { current = notice; consumed = false }
    }
    mutating func route(identifier: String, info: [String:String], action: String) -> UUID? {
        guard !consumed, let current, identifier == current.identifier, info == current.userInfo,
              action == UNNotificationDefaultActionIdentifier else { return nil }
        consumed = true
        return current.runID // Open results only; cannot approve anything.
    }
    var canPresent: Bool { current != nil && !consumed }
}
