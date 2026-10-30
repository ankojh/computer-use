import AppKit
import Combine
import OSLog

@MainActor
final class AppModel: ObservableObject {
    @Published var draft = ""
    @Published var publicResearchBrief = ""
    @Published var useHybridHelpers = true
    @Published var mode: ControlMode = .auto
    @Published private(set) var approvalRevealNonce: UInt64 = 0
    @Published var notificationMessage = "Task notifications are being checked…"
    @Published private(set) var completionRevealNonce: UInt64 = 0
    func revealCompletion(runID: UUID) -> Bool {
        guard session.id == runID, session.ended else { return false }
        completionRevealNonce &+= 1
        return true
    }
    var enableNotifications: (() -> Void)?
    func revealApproval(runID: UUID, promptID: UInt64) -> Bool {
        guard session.id == runID, session.phase == .waiting, session.pending?.id == promptID else { return false }
        approvalRevealNonce &+= 1
        return true
    }
    func openNotificationSettings() {
        if let url = URL(string: "x-apple.systempreferences:com.apple.Notifications-Settings.extension?id=dev.ankojh.cu") { NSWorkspace.shared.open(url) }
    }
    @Published var showAgentCursor = UserDefaults.standard.object(forKey: "ShowAgentCursor") as? Bool ?? true {
        didSet { UserDefaults.standard.set(showAgentCursor, forKey: "ShowAgentCursor") }
    }
    @Published private(set) var session = Session()
    @Published private(set) var screenReady = false
    @Published private(set) var inputReady = false
    @Published private(set) var codexReady = false
    @Published private(set) var checking = false
    @Published private(set) var permissionsChecking = false
    @Published private(set) var permissionRequest: PermissionKind?
    @Published private(set) var permissionError: String?
    @Published private(set) var permissionKnown = false
    @Published private(set) var setupMessage = "Checking this Mac…"
    private let permissions = PermissionService()
    private let permissionsLog = Logger(subsystem: "dev.ankojh.cu", category: "Permissions")
    @Published private(set) var codexMessage = "Checking Codex sign-in…"
    var permissionsBusy: Bool { permissionsChecking || permissionRequest != nil }
    var onStart: (() -> Void)?
    var onFinish: (() -> Void)?
    @Published private(set) var respondingToApproval = false
    var prepareResponse: ((Int32?, @escaping (Bool) -> Void) -> Void)?
    var closePanel: (() -> Void)?
    var quit: (() -> Void)?
    private var process: EngineProcess?
    private var probe: EngineProcess?
    private var deadline: Timer?
    private var eventStream = EngineEventStream()
    private var errorBuffer = Data()

    init() {
        let defaultMode = mode.rawValue
        Logger(subsystem: "dev.ankojh.cu", category: "ControlMode")
            .notice("Native default control mode: \(defaultMode, privacy: .public)")
    }

    var engineURL: URL? { Bundle.main.url(forResource: "cu-engine", withExtension: nil) }
    var environment: [String: String] {
        var env = ProcessInfo.processInfo.environment
        let saved = Bundle.main.url(forResource: "launch-path", withExtension: "txt")
            .flatMap { try? String(contentsOf: $0, encoding: .utf8) }?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        env["PATH"] = ["\(home)/.local/bin", "\(home)/.cargo/bin", env["PATH"] ?? "", saved,
                       "/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin", "/usr/sbin", "/sbin"]
            .filter { !$0.isEmpty }.joined(separator: ":")
        return env
    }
    var codexPath: String? {
        environment["PATH"]?.split(separator: ":").map { "\($0)/codex" }
            .first { FileManager.default.isExecutableFile(atPath: $0) }
    }
    var canStart: Bool {
        let text = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        return !session.active && !checking && !permissionsBusy && permissionKnown && permissionError == nil
            && engineURL != nil && screenReady && codexReady
            && (mode == .observe || inputReady) && !text.isEmpty && text.count <= 4000
            && (!useHybridHelpers || publicResearchBrief.count <= 2000)
    }

    func checkReadiness() {
        checkPermissions()
        guard !checking else { return }
        guard let engineURL else {
            setupMessage = "The native engine is missing. Rebuild with tools/CuApp/build-app.sh."
            return
        }
        guard codexPath != nil else {
            codexReady = false
            codexMessage = "Install the Codex CLI, then run codex login in Terminal."
            updateSetupMessage()
            return
        }
        checking = true
        codexMessage = "Checking Codex sign-in…"
        updateSetupMessage()
        do {
            probe = try CodexReadinessProbe.start(executable: engineURL.path, environment: environment) { [weak self] result in
                guard let self else { return }
                self.checking = false
                self.probe = nil
                self.codexReady = result.ready
                self.codexMessage = result.message
                self.updateSetupMessage()
            }
        } catch {
            checking = false
            probe = nil
            codexReady = false
            codexMessage = "Could not check Codex: \(error.localizedDescription)"
            updateSetupMessage()
        }
    }

    func checkPermissions() {
        guard !permissionsBusy, !session.active else { return }
        permissionsChecking = true
        permissionError = nil
        updateSetupMessage()
        permissions.run(.check) { [weak self] result in self?.applyPermissions(result) }
    }

    func requestPermission(_ kind: PermissionKind) {
        guard !permissionsBusy, !session.active else { return }
        permissionRequest = kind
        permissionsLog.notice("Requesting \(kind.title, privacy: .public) for \(Bundle.main.bundlePath, privacy: .public)")
        permissionError = nil
        updateSetupMessage()
        permissions.run(kind == .accessibility ? .accessibility : .screenRecording) { [weak self] result in
            self?.applyPermissions(result)
        }
        // Settings always stays accessible, even if the OS prompt/helper stalls.
        openPrivacy(kind.pane)
    }

    private func applyPermissions(_ result: Result<PermissionSnapshot, Error>) {
        permissionsChecking = false
        permissionRequest = nil
        switch result {
        case .success(let snapshot):
            if !permissionKnown || screenReady != snapshot.screenRecording || inputReady != snapshot.accessibility {
                permissionsLog.notice("macOS permissions: screen=\(snapshot.screenRecording, privacy: .public) accessibility=\(snapshot.accessibility, privacy: .public) app=\(Bundle.main.bundlePath, privacy: .public)")
            }
            screenReady = snapshot.screenRecording
            inputReady = snapshot.accessibility
            permissionKnown = true
            permissionError = nil
        case .failure(let error):
            permissionsLog.error("Permission helper failed: \(error.localizedDescription, privacy: .public)")
            permissionKnown = false
            permissionError = error.localizedDescription
        }
        updateSetupMessage()
    }

    private func updateSetupMessage() {
        if let error = permissionError { setupMessage = error }
        else if let kind = permissionRequest { setupMessage = "Waiting for macOS \(kind.title). Enable Cu in Privacy Settings; no need to keep clicking the button." }
        else if !permissionKnown { setupMessage = "Checking macOS permissions…" }
        else if !screenReady || !inputReady { setupMessage = "Enable Cu in Privacy Settings. Status refreshes when you return. If macOS asks, quit and reopen /Applications/Cu.app." }
        else if !codexReady { setupMessage = codexMessage }
        else { setupMessage = "Native engine ready · Codex subscription connected" }
    }

    func revealApp() {
        NSWorkspace.shared.activateFileViewerSelecting([Bundle.main.bundleURL])
    }

    func openPrivacy(_ pane: String) {
        if let url = URL(string: "x-apple.systempreferences:com.apple.preference.security?\(pane)") {
            NSWorkspace.shared.open(url)
        }
    }

    func copyLogin() {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString("codex login", forType: .string)
    }

    func start() {
        guard canStart, let engineURL else { return }
        let text = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        launch(engineURL: engineURL, task: text,
               arguments: Self.taskArguments(task: text, mode: mode, hybrid: useHybridHelpers, publicBrief: publicResearchBrief), timeout: 15 * 60)
    }

    /// Arguments, not shell text. Never copy the private task into the public brief.
    static func taskArguments(task: String, mode: ControlMode, hybrid: Bool, publicBrief: String) -> [String] {
        var arguments = ["run", task, "--provider", "codex", "--mode", mode.rawValue, "--ui"]
        if hybrid {
            arguments.append("--hybrid")
            let brief = publicBrief.trimmingCharacters(in: .whitespacesAndNewlines)
            if !brief.isEmpty { arguments += ["--public-research", brief] }
        }
        return arguments
    }

    /// Explicit launch-argument diagnostic only; never a planner capability.
    func startPointerTest(layout: String) {
        guard !session.active, let engineURL else { return }
        launch(engineURL: engineURL, task: "Inert CuTarget pointer feedback test",
               arguments: ["pointer-test", "--layout", layout], timeout: 12)
    }

    private func launch(engineURL: URL, task: String, arguments: [String], timeout: TimeInterval) {
        session.begin(task: task)
        respondingToApproval = false
        let runID = session.id
        eventStream = EngineEventStream()
        errorBuffer = Data()
        let child = EngineProcess()
        process = child
        onStart?()
        do {
            var childEnvironment = environment
            if arguments.first == "run" {
                childEnvironment["CU_NATIVE_APPS_JSON"] = NativeAppDiscovery.context(task: task)
            }
            try child.start(executable: engineURL.path,
                arguments: arguments,
                environment: childEnvironment,
                stdout: { [weak self] data in self?.receive(data, runID: runID) },
                stderr: { [weak self] data in
                    guard let self, self.session.id == runID else { return }
                    self.errorBuffer.append(data)
                    if self.errorBuffer.count > 8000 { self.errorBuffer = self.errorBuffer.suffix(8000) }
                }, completion: { [weak self] code in
                    guard let self, self.session.id == runID, self.session.active else { return }
                    self.deadline?.invalidate()
                    self.deadline = nil
                    self.process = nil
                    self.session.finish(runID: runID, code: code,
                                        error: String(decoding: self.errorBuffer, as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines))
                    if TaskInputCompletion.shouldClear(session: self.session, runID: runID, draft: self.draft) {
                        self.draft = ""
                    }
                    if self.session.phase == .signInNeeded {
                        self.codexReady = false
                        self.codexMessage = self.session.failureReason ?? "Run codex login to sign in again."
                        self.updateSetupMessage()
                    }
                    self.onFinish?()
                })
            // This remains a bounded personal agent, never an unattended background loop.
            deadline = Timer.scheduledTimer(withTimeInterval: timeout, repeats: false) { [weak self] _ in
                Task { @MainActor in self?.stop() }
            }
        } catch {
            process = nil
            session.finish(runID: runID, code: 1, error: "Could not start the native engine: \(error.localizedDescription)")
            onFinish?()
        }
    }

    private func receive(_ data: Data, runID: UUID) {
        guard session.id == runID else { return }
        do {
            for event in try eventStream.append(data) { session.receive(event, runID: runID) }
        } catch {
            session.append("Engine output exceeded the protocol limit; stopping safely.")
            stop()
        }
    }

    func approve(runID: UUID, promptID: UInt64, approved: Bool) {
        guard !respondingToApproval, session.id == runID, session.pending?.id == promptID,
              session.phase == .waiting else { return }
        respondingToApproval = true
        let finish: (Bool) -> Void = { [weak self] restored in
            guard let self, self.session.id == runID else { return }
            self.respondingToApproval = false
            guard self.session.phase == .waiting, self.session.pending?.id == promptID else { return }
            guard restored else {
                self.session.append("Could not restore the original app focus. Stopping without executing the approved action.")
                self.stop()
                return
            }
            self.deliverApproval(runID: runID, promptID: promptID, approved: approved)
        }
        guard let prepareResponse else { finish(false); return }
        prepareResponse(session.pending?.restoreAppPID, finish)
    }

    private func deliverApproval(runID: UUID, promptID: UInt64, approved: Bool) {
        guard session.respond(runID: runID, promptID: promptID) else { return }
        guard let data = try? JSONSerialization.data(withJSONObject: ["id": promptID, "approved": approved]),
              process?.send(data + Data([0x0a])) == true else {
            session.append("Could not deliver the approval. Stopping instead.")
            stop()
            return
        }
        session.append(approved ? "Approved by you." : "Declined by you.")
    }

    func stop() {
        guard session.active else { return }
        session.stop()
        process?.stop()
    }

    func shutdown() {
        permissions.cancel()
        probe?.stop(grace: 0.2)
        stop()
    }

    func openLogs() {
        let url = FileManager.default.homeDirectoryForCurrentUser.appendingPathComponent(".cu/runs", isDirectory: true)
        try? FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
        NSWorkspace.shared.open(url)
    }
}
