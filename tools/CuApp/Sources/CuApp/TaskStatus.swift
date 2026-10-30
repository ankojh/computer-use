import Foundation

/// Host-observed counters, never a guessed percentage or a promise of future steps.
struct TaskStatusPresentation {
    let session: Session
    var remaining: String {
        if session.verifiedSuccess { return "0 — verified complete" }
        if session.phase == .idle { return "Not started" }
        return session.active ? "Unknown — steps adapt as Cu works" : "Unfinished / not verified"
    }
    var title: String {
        if session.verifiedSuccess { return " Cu ✓ Done" }
        switch session.phase {
        case .idle: return " Cu"
        case .running:
            let stage: String
            switch session.progress?.stage {
            case .verifying: stage = "Verify"
            case .recovering: stage = "Recover"
            case .researching: stage = "Research"
            case .judging: stage = "Check"
            default: stage = "Step"
            }
            return " Cu · \(stage) \(session.step) · \(session.actionsExecuted) inputs"
        case .waiting: return session.pending?.kind == "pause" ? " Cu · Paused" : " Cu · Approval"
        case .stopping: return " Cu · Stopping"
        case .aborted: return " Cu · Stopped"
        case .needsInput: return " Cu · Needs input"
        case .error, .signInNeeded: return " Cu · Error"
        case .done: return " Cu · Unverified"
        }
    }
    var symbol: String {
        if session.verifiedSuccess { return "checkmark.circle.fill" }
        if session.pending != nil { return "hand.raised.fill" }
        if [.error,.signInNeeded,.needsInput].contains(session.phase) { return "exclamationmark.circle" }
        return session.active ? "sparkles" : "cursorarrow.rays"
    }
    var tooltip: String {
        "Cu — \(session.verifiedSuccess ? "Done — verified" : session.stageLabel) · Planning step \(session.step) · \(session.actionsExecuted) posted input actions · Remaining: \(remaining)"
    }
}

/// Protect new/edited drafts and stale callbacks. Only the submitted task clears.
struct TaskInputCompletion {
    static func shouldClear(session: Session, runID: UUID, draft: String) -> Bool {
        session.id == runID && session.verifiedSuccess && draft.trimmingCharacters(in:.whitespacesAndNewlines) == session.task
    }
}
