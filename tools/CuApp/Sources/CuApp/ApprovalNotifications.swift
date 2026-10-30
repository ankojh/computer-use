import Foundation
import UserNotifications
import OSLog

/// No task text, recipients, typed text, or AX labels enter notification content.
/// The full request remains in the attached panel, opened by the default action.
struct ApprovalNotice: Equatable {
    let launchID: UUID
    let runID: UUID
    let promptID: UInt64
    let kind: String
    let topic: String
    var identifier: String { "cu.approval.\(launchID.uuidString).\(runID.uuidString).\(promptID)" }
    var userInfo: [String: String] { ["launch": launchID.uuidString, "run": runID.uuidString, "prompt": String(promptID)] }
    var title: String { kind == "pause" ? "Cu paused — resume?" : "Cu — \(topic)" }
    var body: String {
        kind == "pause" ? "Human input paused the task. Resume or stop here, or click to inspect Cu."
            : "A consequential action needs your approval. Expand for Approve / Deny, or click to inspect the exact request. No response means Cu keeps waiting."
    }
    static func topic(for approval: Approval) -> String {
        if approval.kind == "pause" { return "Task paused" }
        if approval.kind == "safety" { return "Safety review required" }
        // Classify locally, but never copy arbitrary/private engine text to OS notifications.
        let reason = String(approval.message.split(separator: "\n", maxSplits: 1).first ?? "").lowercased()
        if ["password", "secure", "secret", "token", "key or", "card number"].contains(where: reason.contains) { return "Sensitive input approval" }
        if ["send", "submit", "reply"].contains(where: reason.contains) { return "Send / submit approval" }
        if ["delete", "trash", "erase", "discard", "remove"].contains(where: reason.contains) { return "Deletion approval" }
        if ["pay", "purchase", "checkout", "buy", "transfer"].contains(where: reason.contains) { return "Payment approval" }
        if ["publish", "post", "share"].contains(where: reason.contains) { return "Publishing / sharing approval" }
        return "Action approval required"
    }
}

enum ApprovalNotificationIntent: Equatable {
    case open(UUID, UInt64)
    case respond(UUID, UInt64, Bool)
}

/// Pure, run/prompt/launch-scoped routing, independently testable without macOS permission.
struct ApprovalNotificationState {
    static let approve = "CU_APPROVE"
    static let deny = "CU_DENY"
    let launchID = UUID()
    private(set) var current: ApprovalNotice?
    private var consumed = false

    mutating func update(_ session: Session) {
        let notice = session.phase == .waiting ? session.pending.map {
            ApprovalNotice(launchID: launchID, runID: session.id, promptID: $0.id,
                           kind: $0.kind, topic: ApprovalNotice.topic(for: $0))
        } : nil
        if notice != current { current = notice; consumed = false }
    }
    mutating func route(identifier: String, info: [String: String], action: String) -> ApprovalNotificationIntent? {
        guard !consumed, let current, identifier == current.identifier, info == current.userInfo else { return nil }
        if action == UNNotificationDefaultActionIdentifier { return .open(current.runID, current.promptID) }
        guard action == Self.approve || action == Self.deny else { return nil } // Dismiss is NOT Deny/Approve.
        consumed = true
        return .respond(current.runID, current.promptID, action == Self.approve)
    }
    var canPresent: Bool { current != nil && !consumed }
}

@MainActor
final class ApprovalNotifications: NSObject, UNUserNotificationCenterDelegate {
    static let requestsPermissionAtLaunch = false
    private let center: UNUserNotificationCenter
    private let log = Logger(subsystem: "dev.ankojh.cu", category: "ApprovalNotifications")
    private var state = ApprovalNotificationState()
    private var completionState = CompletionNotificationState()
    private var completionSubmittedID: String?
    private var submittedID: String?
    private var allowed = false
    private var requesting = false
    var onIntent: ((ApprovalNotificationIntent) -> Void)?
    var onStatus: ((String) -> Void)?
    var onCompletionOpen: ((UUID) -> Void)?

    static func categories() -> Set<UNNotificationCategory> {
        let action = UNNotificationCategory(identifier: "CU_APPROVAL", actions: [
            UNNotificationAction(identifier: ApprovalNotificationState.approve, title: "Approve", options: [.authenticationRequired]),
            UNNotificationAction(identifier: ApprovalNotificationState.deny, title: "Deny", options: [.authenticationRequired, .destructive])
        ], intentIdentifiers: [], hiddenPreviewsBodyPlaceholder: "Cu needs your approval", options: [])
        let pause = UNNotificationCategory(identifier: "CU_PAUSE", actions: [
            UNNotificationAction(identifier: ApprovalNotificationState.approve, title: "Resume", options: [.authenticationRequired]),
            UNNotificationAction(identifier: ApprovalNotificationState.deny, title: "Stop task", options: [.authenticationRequired, .destructive])
        ], intentIdentifiers: [], hiddenPreviewsBodyPlaceholder: "Cu is paused", options: [])
        let completion = UNNotificationCategory(identifier: "CU_COMPLETION", actions: [], intentIdentifiers: [], hiddenPreviewsBodyPlaceholder: "Cu task status", options: [])
        return [action, pause, completion]
    }
    init(center: UNUserNotificationCenter = .current()) {
        self.center = center
        super.init()
        center.delegate = self
        center.setNotificationCategories(Self.categories())
        // Remove notices from prior launches; they can never authorize this launch.
        let prefix = "cu.approval.\(state.launchID.uuidString)."
        let completionPrefix = "cu.completion.\(completionState.launchID.uuidString)."
        center.getDeliveredNotifications { [weak self] notices in
            let old = notices.map(\.request.identifier).filter { ($0.hasPrefix("cu.approval.") && !$0.hasPrefix(prefix)) || ($0.hasPrefix("cu.completion.") && !$0.hasPrefix(completionPrefix)) }
            Task { @MainActor in self?.center.removeDeliveredNotifications(withIdentifiers: old) }
        }
        center.getPendingNotificationRequests { [weak self] requests in
            let old = requests.map(\.identifier).filter { ($0.hasPrefix("cu.approval.") && !$0.hasPrefix(prefix)) || ($0.hasPrefix("cu.completion.") && !$0.hasPrefix(completionPrefix)) }
            Task { @MainActor in self?.center.removePendingNotificationRequests(withIdentifiers: old) }
        }
    }
    func prepare() {
        // Optional notifications are enabled only by the explicit panel button.
        // Default automatic action execution needs no notification authorization.
        refresh(requestIfNeeded: Self.requestsPermissionAtLaunch)
    }
    func requestPermission() { refresh(requestIfNeeded: true) }
    func refresh(requestIfNeeded: Bool = false) {
        center.getNotificationSettings { [weak self] settings in
            Task { @MainActor in
                guard let self else { return }
                self.log.notice("Notification settings: authorization=\(settings.authorizationStatus.rawValue, privacy: .public) alerts=\(settings.alertSetting == .enabled, privacy: .public)")
                switch settings.authorizationStatus {
                case .authorized, .provisional:
                    self.allowed = true
                    self.log.notice("Approval notifications authorized; alerts=\(settings.alertSetting == .enabled, privacy: .public)")
                    self.onStatus?(settings.alertSetting == .enabled ? "Task notifications enabled · finished tasks show a banner" : "macOS notification alerts are off · enable Cu alerts in Notification Settings; Done remains in the menu bar")
                    self.submitCurrent()
                    self.submitCompletion()
                case .notDetermined where requestIfNeeded && !self.requesting:
                    self.requesting = true
                    self.log.notice("Explicitly requesting one-time task-notification authorization")
                    UserDefaults.standard.set(true, forKey: "ApprovalNotificationPermissionAsked")
                    do {
                        _ = try await self.center.requestAuthorization(options: [.alert, .sound])
                        self.requesting = false
                        self.refresh()
                    } catch {
                        self.requesting = false
                        self.onStatus?("Could not enable notifications. Check macOS Notification Settings.")
                    }
                default:
                    self.allowed = false
                    self.log.notice("Notifications not authorized; optional notifications unavailable")
                    self.onStatus?(settings.authorizationStatus == .notDetermined
                        ? "Completion alerts are not enabled · press Enable completion alerts for the one-time macOS permission."
                        : "Completion alerts are blocked by macOS · open Notification settings → Cu → Allow notifications. The menu bar still shows Done.")
                }
            }
        }
    }
    func update(_ session: Session) {
        let old = state.current?.identifier
        state.update(session)
        if old != state.current?.identifier {
            if let old { remove(old) }
            submittedID = nil
        }
        let oldCompletion = completionState.current?.identifier
        completionState.update(session)
        if oldCompletion != completionState.current?.identifier {
            if let oldCompletion { remove(oldCompletion) }
            completionSubmittedID = nil
        }
        // Settings may have changed while Cu stayed in the background. Re-read
        // on a new prompt/outcome, without requesting authorization again.
        if (old != state.current?.identifier && state.current != nil)
            || (oldCompletion != completionState.current?.identifier && completionState.current != nil) {
            refresh()
        }
        submitCurrent()
        submitCompletion()
    }
    private func submitCompletion() {
        guard allowed, completionState.canPresent, let notice = completionState.current,
              completionSubmittedID != notice.identifier else { return }
        completionSubmittedID = notice.identifier
        let content = UNMutableNotificationContent()
        content.title = notice.title
        content.body = notice.body
        content.categoryIdentifier = "CU_COMPLETION"
        content.userInfo = notice.userInfo
        content.sound = .default
        content.interruptionLevel = .active
        center.add(UNNotificationRequest(identifier:notice.identifier,content:content,trigger:nil)) { [weak self] error in
            Task { @MainActor in
                guard let self else { return }
                if self.completionState.current != notice || !self.completionState.canPresent { self.remove(notice.identifier) }
                else if error != nil {
                    self.log.error("macOS rejected the task-outcome notification")
                    self.onStatus?("macOS could not accept the task notification. Check Cu in the menu bar for its outcome.")
                } else {
                    self.log.notice("Task-outcome notification accepted by macOS: \(notice.kind.rawValue, privacy: .public)")
                }
            }
        }
    }
    private func submitCurrent() {
        guard allowed, state.canPresent, let notice = state.current, submittedID != notice.identifier else { return }
        submittedID = notice.identifier
        let content = UNMutableNotificationContent()
        content.title = notice.title
        content.body = notice.body
        content.categoryIdentifier = notice.kind == "pause" ? "CU_PAUSE" : "CU_APPROVAL"
        content.userInfo = notice.userInfo
        content.sound = .default
        content.interruptionLevel = .active // Does not override Focus or use critical-alert entitlement.
        center.add(UNNotificationRequest(identifier: notice.identifier, content: content, trigger: nil)) { [weak self] error in
            Task { @MainActor in
                guard let self else { return }
                if self.state.current != notice || !self.state.canPresent { self.remove(notice.identifier) }
                else if error != nil { self.onStatus?("macOS could not deliver the approval notification. Open Cu to review; nothing was approved.") }
            }
        }
    }
    private func remove(_ id: String) {
        center.removePendingNotificationRequests(withIdentifiers: [id])
        center.removeDeliveredNotifications(withIdentifiers: [id])
    }
    func shutdown() {
        if let id = state.current?.identifier { remove(id) }
        state = ApprovalNotificationState()
        if let id = completionState.current?.identifier { remove(id) }
        completionState = CompletionNotificationState()
    }
    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter, willPresent notification: UNNotification, withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void) {
        Task { @MainActor in
            let valid = (self.state.canPresent && self.state.current?.identifier == notification.request.identifier)
                || (self.completionState.canPresent && self.completionState.current?.identifier == notification.request.identifier)
            completionHandler(valid ? [.banner, .list, .sound] : [])
        }
    }
    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse, withCompletionHandler completionHandler: @escaping () -> Void) {
        Task { @MainActor in
            let info = response.notification.request.content.userInfo as? [String: String] ?? [:]
            if let runID = self.completionState.route(identifier: response.notification.request.identifier, info: info, action: response.actionIdentifier) {
                self.remove(response.notification.request.identifier)
                self.onCompletionOpen?(runID)
            } else if let intent = self.state.route(identifier: response.notification.request.identifier, info: info, action: response.actionIdentifier) {
                if case .respond = intent { self.remove(response.notification.request.identifier) }
                self.onIntent?(intent)
            } else if response.notification.request.identifier.hasPrefix("cu.approval.") || response.notification.request.identifier.hasPrefix("cu.completion.") {
                self.remove(response.notification.request.identifier)
            }
            completionHandler()
        }
    }
}
