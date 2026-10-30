import Foundation

enum ControlMode: String, CaseIterable, Identifiable {
    case auto, confirmRisky = "confirm-risky", confirmAll = "confirm-all", observe
    var id: String { rawValue }
    var title: String {
        switch self {
        case .confirmRisky: return "Critical actions only"
        case .confirmAll: return "Confirm every action"
        case .observe: return "Observe only"
        case .auto: return "Allow all actions"
        }
    }
    var detail: String {
        switch self {
        case .confirmRisky: return "Routine navigation and typing run automatically. Sending, deleting, paying, publishing, sensitive input and other consequential actions still need approval. Taking over the mouse or keyboard still pauses Cu."
        case .confirmAll: return "You approve each action before your Mac is controlled."
        case .observe: return "Looks at your screen and describes actions, without posting input."
        case .auto: return "Default: no Cu action approvals, including Send. Sending, deleting, purchases, sensitive typing and command execution run automatically. Relaunch keeps this default. Stop, human-interference pause, hard denials, focus checks and macOS-required permissions still apply."
        }
    }
}

struct Approval: Decodable, Identifiable, Equatable {
    let id: UInt64
    let kind: String
    let message: String
    let restoreAppPID: Int32?
}

struct RunSummary: Decodable {
    let status: String
    let message: String?
    let final_text: String?
    let steps: Int
    let actions_executed: Int
    let wall_ms: UInt64
}

struct ModelRoute: Decodable {
    let model: String
    let reasoning: String
    let tier: String
    let reason: String
    let router_model: String?
    let router_reasoning: String?
    let router_latency_ms: UInt64
    let planner_latency_ms: UInt64
    var label: String { "\(model) · \(reasoning) reasoning" }
}

struct CursorFeedback: Decodable, Equatable {
    struct Point: Decodable, Equatable {
        let x: Double
        let y: Double
        var valid: Bool { x.isFinite && y.isFinite }
    }
    enum Phase: String, Decodable { case target, active, complete, posted }
    enum Kind: String, Decodable { case action, move, drag, buttonDown = "button_down", buttonUp = "button_up", click, scroll }
    enum Button: String, Decodable { case left, right, middle, back, forward }
    let point: Point
    let phase: Phase
    let kind: Kind?
    let sequence: UInt64?
    let button: Button?
    let count: Int?

    init(point: Point, phase: Phase, kind: Kind? = nil, sequence: UInt64? = nil, button: Button? = nil, count: Int? = nil) {
        self.point = point; self.phase = phase; self.kind = kind
        self.sequence = sequence; self.button = button; self.count = count
    }
    var isClick: Bool { kind == .click && [.posted, .complete].contains(phase) }
    var label: String {
        if phase == .target || (phase == .active && (kind == nil || kind == .action)) { return "Cu target" }
        let name = button.map { $0.rawValue.capitalized } ?? "Mouse"
        switch kind {
        case .buttonDown: return "\(name) down"
        case .click:
            let clicks = max(1, min(3, count ?? 1))
            return "\(name) click\(clicks > 1 ? " ×\(clicks)" : "")"
        case .buttonUp: return "\(name) up"
        case .drag: return "Cu drag"
        case .scroll: return "Cu scroll"
        default: return "Cu move"
        }
    }
}

struct CompletionReview: Decodable {
    struct Check: Decodable {
        let requirement: String
        let verdict: String
        let evidence: String
    }
    let verdict: String
    let summary: String
    let model: String
    let reasoning: String
    let latency_ms: UInt64
    let checks: [Check]
}

struct AdvisorReport: Decodable, Identifiable {
    struct Source: Decodable { let url: String; let title: String }
    let id: String
    let provider: String
    let kind: String
    let status: String
    let summary: String
    let latency_ms: UInt64
    let requests_used: Int
    let sources: [Source]
}

struct CodexReadiness: Decodable {
    let ready: Bool
    let message: String
}

struct TaskProgress: Decodable {
    enum Stage: String, Decodable {
        case planning, executing, researching, judging, verifying, recovering
        var title: String {
            switch self {
            case .planning: return "Planning"
            case .executing: return "Executing"
            case .researching: return "Researching the public web"
            case .judging: return "Checking UI metadata"
            case .verifying: return "Verifying the outcome"
            case .recovering: return "Continuing missing work"
            }
        }
    }
    let stage: Stage
    let step: Int
    let actions_executed: Int
}

struct EngineEvent: Decodable {
    let advisor: AdvisorReport?
    let restore_app_pid: Int32?
    let progress: TaskProgress?
    let verification: CompletionReview?
    let cursor: CursorFeedback?
    let route: ModelRoute?
    let type: String
    let id: UInt64?
    let kind: String?
    let message: String?
    let summary: RunSummary?
}

struct Activity: Identifiable {
    let id = UUID()
    let date = Date()
    let text: String
}

/// A run-scoped state machine. A new UUID for every task prevents old callbacks and
/// stale approval buttons from controlling a later task (prompt IDs restart at 1).
struct Session {
    enum Phase: String {
        case idle = "Idle", running = "Working", waiting = "Needs approval"
        case stopping = "Stopping", done = "Finished", aborted = "Stopped", error = "Task error"
        case signInNeeded = "Sign-in needed"
        case needsInput = "Needs your input"
    }
    private(set) var id = UUID()
    private(set) var phase: Phase = .idle
    private(set) var task = ""
    private(set) var started: Date?
    private(set) var pending: Approval?
    private(set) var summary: RunSummary?
    private(set) var route: ModelRoute?
    private(set) var cursor: CursorFeedback?
    private(set) var verification: CompletionReview?
    private(set) var advisors: [AdvisorReport] = []
    private(set) var activity: [Activity] = []
    private(set) var progress: TaskProgress?
    private(set) var failureReason: String?
    var step: Int { summary?.steps ?? progress?.step ?? 0 }
    var actionsExecuted: Int { summary?.actions_executed ?? progress?.actions_executed ?? 0 }
    var stageLabel: String {
        switch phase {
        case .running: return progress?.stage.title ?? "Connecting"
        default: return phase.rawValue
        }
    }
    static func isAuthenticationError(_ message: String) -> Bool {
        let text = message.lowercased()
        return text.contains("codex") && (text.contains("401") || text.contains("authentication rejected")
            || text.contains("subscription sign-in is required"))
    }
    var active: Bool { [.running, .waiting, .stopping].contains(phase) }
    var verifiedSuccess: Bool { phase == .done && summary?.status == "done" && verification?.verdict == "verified" }
    var ended: Bool { started != nil && !active && phase != .idle }
    var latest: String { activity.last?.text ?? "Your Mac, with you in control." }

    mutating func begin(task: String) {
        self = Session()
        id = UUID()
        self.task = task
        started = Date()
        phase = .running
        append("Connecting to Codex…")
    }

    mutating func append(_ text: String) {
        activity.append(Activity(text: String(text.prefix(16000))))
        if activity.count > 200 { activity.removeFirst(activity.count - 200) }
    }

    mutating func receive(_ event: EngineEvent, runID: UUID) {
        guard id == runID, active, phase != .stopping else { return }
        switch event.type {
        case "advisor":
            guard let report = event.advisor else { return }
            advisors.removeAll { $0.id == report.id }
            advisors.append(report)
            if advisors.count > 8 { advisors.removeFirst(advisors.count - 8) }
            append("\(report.provider.capitalized) helper: \(report.status) · \(report.latency_ms) ms. Not action approval.")
        case "task_progress":
            guard let progress = event.progress, progress.step >= 0, progress.actions_executed >= 0 else { return }
            self.progress = progress
        case "confirmation":
            guard let id = event.id, let kind = event.kind, let message = event.message else { return }
            pending = Approval(id: id, kind: kind, message: message, restoreAppPID: event.restore_app_pid)
            if kind != "action" { cursor = nil }
            phase = .waiting
            append(kind == "pause" ? "Paused for human input." : "Waiting for your approval.")
        case "verification":
            guard let review = event.verification else { return }
            verification = review
            append("Completion check · \(review.model) · \(review.reasoning): \(review.summary)")
        case "cursor":
            cursor = event.cursor.flatMap { $0.point.valid ? $0 : nil }
        case "route":
            guard let route = event.route else { return }
            self.route = route
            append("\(route.label) — \(route.reason)")
        case "progress":
            if let message = event.message, !message.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { append(message) }
        case "done":
            guard let summary = event.summary else { return }
            self.summary = summary
            pending = nil
            cursor = nil
            // Remain active until the process actually exits; no overlapping children.
            if let text = summary.final_text, !text.isEmpty { append(text) }
            if let text = summary.message, !text.isEmpty, text != summary.final_text { append(text) }
        default: break
        }
    }

    mutating func respond(runID: UUID, promptID: UInt64) -> Bool {
        guard id == runID, phase == .waiting, pending?.id == promptID else { return false }
        pending = nil
        cursor = nil
        phase = .running
        return true
    }

    mutating func stop() {
        guard active else { return }
        phase = .stopping
        pending = nil
        cursor = nil
        append("Stopping and releasing input…")
    }

    mutating func finish(runID: UUID, code: Int32, error: String) {
        guard id == runID, active else { return }
        pending = nil
        cursor = nil
        if phase == .stopping {
            phase = .aborted
            append("Task stopped.")
        } else if let summary {
            switch summary.status {
            case "done":
                if code == 0 { phase = .done }
                else {
                    phase = .error
                    failureReason = "Engine reported completion but exited with code \(code); success is not confirmed."
                }
            case "needs_input": phase = .needsInput
            case "aborted": phase = .aborted
            default: phase = Self.isAuthenticationError(summary.message ?? "") ? .signInNeeded : .error
            }
            append("\(summary.steps) steps · \(summary.actions_executed) actions · \(summary.wall_ms / 1000)s")
            if phase != .done, let reason = failureReason ?? summary.message ?? summary.final_text, !reason.isEmpty {
                failureReason = reason
                append(reason)
            }
        } else {
            phase = Self.isAuthenticationError(error) ? .signInNeeded : .error
            let reason = error.isEmpty ? "Engine exited (\(code)). Check permissions and Codex sign-in." : error
            failureReason = reason
            append(reason)
        }
    }
}
