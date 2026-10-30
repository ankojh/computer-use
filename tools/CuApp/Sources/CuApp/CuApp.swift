import AppKit
import SwiftUI
import Combine
import Carbon

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    let model = AppModel()
    private var statusPanel: StatusPanel!
    private var subscription: AnyCancellable?
    private var workspaceObserver: NSObjectProtocol?
    private var previousApp: NSRunningApplication?
    private var previousPrompt: String?
    private var promptRestorePID: Int32?
    private var approvalNotifications: ApprovalNotifications?
    private var quitting = false
    private var hotkey: EventHotKeyRef?
    private var eventHandler: EventHandlerRef?
    private var permissionTimer: Timer?
    private let agentCursor = AgentCursor()
    private var pointerTestEvents: [[String: Any]] = []
    private var pointerTestLastSequence: UInt64?
    private var pointerTestPulses = 0
    private var pointerTestLayout: String? {
        let args = CommandLine.arguments
        guard let index = args.firstIndex(of: "--cursor-test-layout"), index + 1 < args.count else { return nil }
        return args[index + 1]
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)
        installMainMenu()
        previousApp = NSWorkspace.shared.frontmostApplication
        if previousApp?.processIdentifier == getpid() { previousApp = nil }
        workspaceObserver = NSWorkspace.shared.notificationCenter.addObserver(
            forName: NSWorkspace.didActivateApplicationNotification, object: nil, queue: .main
        ) { [weak self] note in
            guard let app = note.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication,
                  app.processIdentifier != getpid(),
                  !["com.apple.notificationcenterui", "com.apple.controlcenter", "com.apple.SystemUIServer", "com.apple.loginwindow"].contains(app.bundleIdentifier ?? "") else { return }
            Task { @MainActor in self?.previousApp = app }
        }
        installHotkey()
        let height = min(740, max(400, (NSScreen.main?.visibleFrame.height ?? 820) - 80))
        statusPanel = StatusPanel(content: NSHostingController(rootView: MenuBarView(model: model, hotkeyAvailable: hotkey != nil)),
                                  size: NSSize(width: 520, height: height))
        statusPanel.onClose = { [weak self] in
            DispatchQueue.main.async {
                guard let self, NSWorkspace.shared.frontmostApplication?.processIdentifier == getpid() else { return }
                NSApp.deactivate()
                self.previousApp?.activate(options: [])
            }
        }
        model.closePanel = { [weak self] in self?.statusPanel.close() }
        model.quit = { NSApp.terminate(nil) }
        model.prepareResponse = { [weak self] expectedPID, done in
            // Capture fallback focus when the prompt arrives, BEFORE Notification
            // Center can activate. A notification never changes the input target.
            guard let self, let pid = expectedPID ?? self.promptRestorePID else { done(false); return }
            self.statusPanel.close()
            NSApp.deactivate()
            FocusRestorer.restore(pid: pid,
                activate: { NSRunningApplication(processIdentifier: pid)?.activate(options: []) ?? false },
                frontmost: { NSWorkspace.shared.frontmostApplication?.processIdentifier }, completion: done)
        }
        model.onStart = { [weak self] in
            guard let self else { return }
            self.statusPanel.close()
            NSApp.deactivate()
            self.previousApp?.activate(options: [])
        }
        model.onFinish = { [weak self] in
            guard let self else { return }
            if let layout = self.pointerTestLayout {
                let report: [String: Any] = ["phase": self.model.session.phase.rawValue,
                    "status": self.model.session.summary?.status ?? "error",
                    "actions": self.model.session.actionsExecuted,
                    "feedback": self.pointerTestEvents, "click_pulses": self.pointerTestPulses,
                    "message": self.model.session.failureReason ?? self.model.session.summary?.message ?? ""]
                let url = URL(fileURLWithPath: layout).deletingLastPathComponent().appendingPathComponent("cursor-feedback.json")
                if let data = try? JSONSerialization.data(withJSONObject: report, options: [.prettyPrinted, .sortedKeys]) { try? data.write(to: url, options: .atomic) }
                NSApp.terminate(nil)
            } else if self.quitting { NSApp.reply(toApplicationShouldTerminate: true) }
        }
        subscription = model.$session.combineLatest(model.$showAgentCursor).sink { [weak self] session, showCursor in
            guard let self else { return }
            self.updateStatus(session)
            self.approvalNotifications?.update(session)
            self.agentCursor.update((showCursor || self.pointerTestLayout != nil) && session.active ? session.cursor : nil)
            if self.pointerTestLayout != nil, let feedback = session.cursor,
               feedback.phase == .posted, feedback.sequence != self.pointerTestLastSequence {
                self.pointerTestLastSequence = feedback.sequence
                self.pointerTestEvents.append(["sequence": feedback.sequence ?? 0, "kind": feedback.kind?.rawValue ?? "",
                    "x": feedback.point.x, "y": feedback.point.y, "count": feedback.count ?? 0])
                if feedback.isClick, !self.agentCursor.pulsePanels.isEmpty {
                    self.pointerTestPulses += 1
                    if self.pointerTestPulses == 1, let view = self.agentCursor.pulsePanels.last?.contentView { self.snapshot(view, name: "cursor-click") }
                }
            }
            let prompt = session.pending.map { "\(session.id):\($0.id)" }
            if prompt != self.previousPrompt {
                self.promptRestorePID = session.pending?.restoreAppPID ?? (prompt == nil ? nil : self.previousApp?.processIdentifier)
            }
            if prompt != nil, prompt != self.previousPrompt, let button = self.statusPanel.item.button {
                // Indicate approval in the menu bar. Never open/activate a window
                // over the target app automatically while the agent is working.
                NSAccessibility.post(element: button, notification: .announcementRequested,
                                     userInfo: [.announcement: "Cu needs your approval; use the notification or open Cu", .priority: NSAccessibilityPriorityLevel.high.rawValue])
            }
            self.previousPrompt = prompt
        }
        let smoke = CommandLine.arguments.contains("--smoke-test")
        if !smoke {
            if pointerTestLayout == nil {
                let notices = ApprovalNotifications()
                notices.onStatus = { [weak self] message in self?.model.notificationMessage = message }
                notices.onIntent = { [weak self] intent in
                    guard let self else { return }
                    switch intent {
                    case .open(let runID, let promptID):
                        if self.model.revealApproval(runID: runID, promptID: promptID) { self.statusPanel.show() }
                    case .respond(let runID, let promptID, let approved):
                        self.model.approve(runID: runID, promptID: promptID, approved: approved)
                    }
                }
                approvalNotifications = notices
                model.enableNotifications = { [weak notices] in notices?.requestPermission() }
                notices.update(model.session)
                notices.prepare()
            }
            permissionTimer = Timer.scheduledTimer(withTimeInterval: 2, repeats: true) { [weak self] _ in
                Task { @MainActor in
                    guard let self, self.statusPanel.popover.isShown, self.model.permissionError == nil,
                          !self.model.screenReady || !self.model.inputReady else { return }
                    self.model.checkPermissions()
                }
            }
            if pointerTestLayout == nil { model.checkReadiness() }
        }
        if let layout = pointerTestLayout {
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.8) { [weak self] in self?.model.startPointerTest(layout: layout) }
        }
        if smoke || CommandLine.arguments.contains("--show-menu") || CommandLine.arguments.contains("--show-panel") {
            // The status button is attached to its menu-bar window after launch.
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) { [weak self] in self?.statusPanel.show() }
        }
        if smoke {
            DispatchQueue.main.asyncAfter(deadline: .now() + 1) { [self] in
                print("Cu native smoke test: popover=\(statusPanel.popover.isShown), transient=\(statusPanel.popover.behavior == .transient), menu=\(statusPanel.item.button?.action != nil), detachedControls=false, engine=\(model.engineURL != nil), hotkey=\(hotkey != nil)")
                snapshot(statusPanel.popover.contentViewController!.view, name: "menu-panel")
                NSApp.terminate(nil)
            }
        }
    }

    func applicationDidBecomeActive(_ notification: Notification) {
        if !CommandLine.arguments.contains("--smoke-test") {
            model.checkPermissions()
            approvalNotifications?.refresh()
        }
    }
    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        statusPanel.show()
        return true
    }
    func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool { false }
    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        guard model.session.active else { model.shutdown(); return .terminateNow }
        guard !quitting else { return .terminateLater }
        let alert = NSAlert()
        alert.messageText = "Stop the task and quit Cu?"
        alert.informativeText = "Quitting stops the engine and its planner. Closing the menu-bar panel does not stop the task."
        alert.addButton(withTitle: "Stop and Quit")
        alert.addButton(withTitle: "Keep Working")
        guard alert.runModal() == .alertFirstButtonReturn else { return .terminateCancel }
        guard model.session.active else { model.shutdown(); return .terminateNow }
        quitting = true
        model.shutdown()
        return .terminateLater
    }
    func applicationWillTerminate(_ notification: Notification) {
        agentCursor.hide()
        approvalNotifications?.shutdown()
        statusPanel?.remove()
        permissionTimer?.invalidate()
        if let hotkey { UnregisterEventHotKey(hotkey) }
        if let eventHandler { RemoveEventHandler(eventHandler) }
        if let workspaceObserver { NSWorkspace.shared.notificationCenter.removeObserver(workspaceObserver) }
    }
    private func updateStatus(_ session: Session) {
        let problem = [.error, .signInNeeded, .needsInput].contains(session.phase)
        let symbol = session.pending != nil ? "hand.raised.fill" : problem ? "exclamationmark.circle" : session.active ? "sparkles" : "cursorarrow.rays"
        let image = NSImage(systemSymbolName: symbol, accessibilityDescription: "Cu: \(session.stageLabel)")
        image?.isTemplate = true
        statusPanel.item.button?.image = image
        statusPanel.item.button?.title = session.pending != nil ? " !" : ""
        statusPanel.item.button?.toolTip = "Cu — \(session.stageLabel) · Step \(session.step) · \(session.actionsExecuted) input actions"
    }
    /// Accessory apps still need a responder-chain Edit menu for the task editor.
    private func installMainMenu() {
        let main = NSMenu()
        let app = NSMenu()
        let appItem = NSMenuItem(title: "Cu", action: nil, keyEquivalent: "")
        appItem.submenu = app
        main.addItem(appItem)
        app.addItem(withTitle: "Quit Cu", action: #selector(NSApplication.terminate(_:)), keyEquivalent: "q")
        let edit = NSMenu(title: "Edit")
        let editItem = NSMenuItem(title: "Edit", action: nil, keyEquivalent: "")
        editItem.submenu = edit
        main.addItem(editItem)
        for (title, action, key) in [("Undo", "undo:", "z"), ("Redo", "redo:", "Z"),
                                     ("Cut", "cut:", "x"), ("Copy", "copy:", "c"),
                                     ("Paste", "paste:", "v"), ("Select All", "selectAll:", "a")] {
            edit.addItem(withTitle: title, action: Selector(action), keyEquivalent: key)
        }
        NSApp.mainMenu = main
    }
    /// Only renders our own views. No desktop capture or input events.
    private func snapshot(_ view: NSView, name: String) {
        view.layoutSubtreeIfNeeded()
        guard let bitmap = view.bitmapImageRepForCachingDisplay(in: view.bounds) else { return }
        view.cacheDisplay(in: view.bounds, to: bitmap)
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent("cu-native-smoke", isDirectory: true)
        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        if let png = bitmap.representation(using: .png, properties: [:]) {
            let url = directory.appendingPathComponent("\(name).png")
            try? png.write(to: url)
            print("Native view snapshot: \(url.path)")
        }
    }
    private func installHotkey() {
        var type = EventTypeSpec(eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyPressed))
        let callback: EventHandlerUPP = { _, _, context in
            guard let context else { return OSStatus(eventNotHandledErr) }
            let owner = Unmanaged<AppDelegate>.fromOpaque(context).takeUnretainedValue()
            Task { @MainActor in owner.model.stop() }
            return noErr
        }
        let installed = InstallEventHandler(GetApplicationEventTarget(), callback, 1, &type,
                                           Unmanaged.passUnretained(self).toOpaque(), &eventHandler)
        guard installed == noErr else { return }
        let id = EventHotKeyID(signature: 0x63754170, id: 1)
        if RegisterEventHotKey(UInt32(kVK_Escape), UInt32(controlKey | optionKey), id,
                               GetApplicationEventTarget(), 0, &hotkey) != noErr { hotkey = nil }
    }
}

@main
struct CuApplication {
    @MainActor static func main() {
        if CommandLine.arguments.count == 3, ["--render-cursor", "--render-click"].contains(CommandLine.arguments[1]) {
            _ = NSApplication.shared
            guard let png = AgentCursorView.previewPNG(kind: CommandLine.arguments[1] == "--render-click" ? .click : .move) else { exit(1) }
            do { try png.write(to: URL(fileURLWithPath: CommandLine.arguments[2])) }
            catch { fputs("Cursor preview failed: \(error)\n", stderr); exit(1) }
            return
        }
        if PermissionOperation.runIfRequested(arguments: CommandLine.arguments) { return }
        let app = NSApplication.shared
        let delegate = AppDelegate()
        app.delegate = delegate
        withExtendedLifetime(delegate) { app.run() }
    }
}
