import SwiftUI

private let accent = Color(red: 0.12, green: 0.47, blue: 0.39)

struct MenuBarView: View {
    @ObservedObject var model: AppModel
    let hotkeyAvailable: Bool
    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 12) {
                Image(systemName: "cursorarrow.rays")
                    .font(.system(size: 25, weight: .medium))
                    .foregroundStyle(.white)
                    .frame(width: 48, height: 48)
                    .background(accent.gradient, in: RoundedRectangle(cornerRadius: 13))
                VStack(alignment: .leading, spacing: 3) {
                    Text("Cu").font(.title2.weight(.semibold))
                    Text("Native computer use").font(.caption).foregroundStyle(.secondary)
                }
                Spacer()
                PhaseBadge(phase: model.session.phase)
                if model.session.active {
                    Button(role: .destructive, action: model.stop) { Image(systemName: "stop.fill") }
                        .disabled(model.session.phase == .stopping)
                        .help(hotkeyAvailable ? "Stop task (⌃⌥Esc)" : "Stop task")
                }
                Button { model.closePanel?() } label: { Image(systemName: "xmark") }
                    .buttonStyle(.plain).help("Close panel — task keeps running")
            }
            .padding(18)
            TaskProgressStrip(session: model.session)
            Divider()
            ScrollViewReader { proxy in
            ScrollView {
                VStack(alignment: .leading, spacing: 22) {
                    if let approval = model.session.pending {
                        ApprovalCard(model: model, approval: approval, runID: model.session.id).id("pendingApproval")
                    }
                    if let reason = model.session.failureReason {
                        VStack(alignment: .leading, spacing: 6) {
                            Label(model.session.phase.rawValue, systemImage: "exclamationmark.circle").font(.headline)
                            Text(reason).font(.callout).textSelection(.enabled)
                            if model.session.phase == .signInNeeded {
                                Text("Sign in again in Terminal with codex login, then refresh the connection below. If it persists, verify access to your Codex workspace.")
                                    .font(.caption)
                            }
                        }.padding(14).background(Color.orange.opacity(0.08), in: RoundedRectangle(cornerRadius: 12))
                    }
                    setup
                    VStack(alignment: .leading, spacing: 10) {
                        Text("What should we do?").font(.headline)
                        ZStack(alignment: .topLeading) {
                            TextEditor(text: $model.draft)
                                .font(.body)
                                .scrollContentBackground(.hidden)
                                .padding(8)
                                .frame(height: 96)
                                .disabled(model.session.active)
                                .accessibilityLabel("Task")
                            if model.draft.isEmpty {
                                Text("Describe a task for this Mac…")
                                    .foregroundStyle(.tertiary).padding(.horizontal, 13).padding(.vertical, 16)
                                    .allowsHitTesting(false)
                            }
                        }
                        .background(.background, in: RoundedRectangle(cornerRadius: 10))
                        .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(.quaternary))
                        if model.draft.count > 4000 {
                            Text("Keep the task under 4,000 characters.").font(.caption).foregroundStyle(.red)
                        }
                        Toggle("Use hybrid helpers · public data only", isOn: $model.useHybridHelpers)
                            .disabled(model.session.active)
                        if model.useHybridHelpers {
                            VStack(alignment: .leading, spacing: 6) {
                                Text("Public research brief (optional)").font(.callout.weight(.semibold))
                                TextField("Public topic/questions only—never email or account details", text: $model.publicResearchBrief)
                                    .textFieldStyle(.roundedBorder)
                                    .disabled(model.session.active)
                                    .accessibilityLabel("Public research brief shared with Parallel")
                                Text("Only this separate field is shared with Parallel. Nothing is copied from your private task or screen. Leave blank for browser-only research. Jev receives role/boolean metadata only; its API is separately metered. Helpers run only when requested, with bounded calls/timeouts.")
                                    .font(.caption2).foregroundStyle(.secondary)
                                if model.publicResearchBrief.count > 2000 { Text("Keep the public brief under 2,000 characters.").font(.caption).foregroundStyle(.red) }
                            }
                        }
                        Picker("Control", selection: $model.mode) {
                            ForEach(ControlMode.allCases) { Text($0.title).tag($0) }
                        }
                        .disabled(model.session.active)
                        if model.mode == .auto {
                            Label("Actions run without approval", systemImage: "exclamationmark.triangle.fill")
                                .font(.caption.weight(.semibold)).foregroundStyle(.orange)
                        }
                        Text(model.mode.detail).font(.caption).foregroundStyle(.secondary)
                        VStack(alignment: .leading, spacing: 5) {
                            Text(model.notificationMessage).font(.caption2).foregroundStyle(.secondary)
                            HStack {
                                Button("Enable notifications") { model.enableNotifications?() }.buttonStyle(.link)
                                Button("Notification settings", action: model.openNotificationSettings).buttonStyle(.link)
                            }.font(.caption2)
                        }
                        Toggle("Highlight Cu’s shared cursor", isOn: $model.showAgentCursor)
                            .font(.caption)
                            .help("Bright cyan marks Cu’s actions; orange marks a target awaiting approval. Visual only—your mouse movement still pauses Cu.")
                        HStack {
                            Label("Planner: ChatGPT subscription", systemImage: "lock.shield")
                                .font(.caption).foregroundStyle(.secondary)
                            Spacer()
                            if model.session.active {
                                Button(role: .destructive, action: model.stop) {
                                    Label("Stop task", systemImage: "stop.fill")
                                }
                                .disabled(model.session.phase == .stopping)
                                if hotkeyAvailable { Text("⌃⌥Esc").font(.caption).foregroundStyle(.secondary) }
                            } else {
                                Button(action: model.start) { Label("Start task", systemImage: "arrow.up") }
                                    .buttonStyle(.borderedProminent).tint(accent)
                                    .keyboardShortcut(.return, modifiers: .command)
                                    .disabled(!model.canStart)
                            }
                        }
                    }
                    ForEach(model.session.advisors.suffix(4)) { report in
                        VStack(alignment: .leading, spacing: 5) {
                            Label("\(report.provider.capitalized) · \(report.status) · \(report.latency_ms) ms", systemImage: "network")
                                .font(.callout.weight(.medium))
                            Text(report.summary).font(.caption)
                            Text("Helper HTTP requests this run: \(report.requests_used) · at most 12/run · not input approval")
                                .font(.caption2).foregroundStyle(.secondary)
                            ForEach(report.sources.prefix(3), id: \.url) { source in
                                Text("\(source.title)\n\(source.url)").font(.caption2).textSelection(.enabled)
                            }
                        }
                    }
                    if let route = model.session.route {
                        VStack(alignment: .leading, spacing: 5) {
                            Label("Last plan: \(route.label)", systemImage: "arrow.triangle.branch").font(.callout.weight(.medium))
                            Text(route.reason).font(.caption).foregroundStyle(.secondary)
                            Text("Router \(route.router_latency_ms) ms · Planner \(route.planner_latency_ms) ms")
                                .font(.caption2).foregroundStyle(.tertiary)
                        }.textSelection(.enabled)
                    }
                    if let review = model.session.verification {
                        VStack(alignment: .leading, spacing: 6) {
                            Label("Completion check · \(review.model) · \(review.reasoning)",
                                  systemImage: review.verdict == "verified" ? "checkmark.shield" : "exclamationmark.bubble")
                                .font(.callout.weight(.medium))
                            Text(review.summary).font(.callout)
                            ForEach(Array(review.checks.enumerated()), id: \.offset) { _, check in
                                Text("\(check.verdict == "verified" ? "✓" : "•") \(check.requirement): \(check.evidence)")
                                    .font(.caption).foregroundStyle(.secondary)
                            }
                        }.textSelection(.enabled)
                    }
                    if !model.session.activity.isEmpty {
                        VStack(alignment: .leading, spacing: 10) {
                            HStack {
                                Text("Activity").font(.headline)
                                Spacer()
                                if model.session.active { ProgressView().controlSize(.small) }
                            }
                            ForEach(model.session.activity.suffix(25)) { item in
                                HStack(alignment: .top, spacing: 10) {
                                    Circle().fill(accent.opacity(0.65)).frame(width: 5, height: 5).padding(.top, 6)
                                    Text(item.text).font(.callout).textSelection(.enabled).frame(maxWidth: .infinity, alignment: .leading)
                                }
                            }
                        }
                    }
                }
                .padding(18)
            }
            .onChange(of: model.approvalRevealNonce) { _, _ in proxy.scrollTo("pendingApproval", anchor: .top) }
            .onChange(of: model.session.pending?.id) { _, prompt in
                if prompt != nil { proxy.scrollTo("pendingApproval", anchor: .top) }
            }
            }
            Divider()
            HStack {
                Label("ChatGPT subscription", systemImage: "lock.shield")
                Spacer()
                Button("Run logs", action: model.openLogs).buttonStyle(.link)
                Button("Quit Cu") { model.quit?() }.buttonStyle(.link)
            }
            .font(.caption).foregroundStyle(.secondary).padding(.horizontal, 18).padding(.vertical, 13)
        }
        .frame(minWidth: 500, minHeight: 400)
        .background(Color(nsColor: .windowBackgroundColor))
    }

    private var setup: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 14) {
                ReadinessChip(title: "Screen", ready: model.permissionKnown ? model.screenReady : nil)
                ReadinessChip(title: "Accessibility", ready: model.permissionKnown ? model.inputReady : nil)
                ReadinessChip(title: "Codex", ready: model.checking ? nil : model.codexReady)
                Spacer()
                Button(action: model.checkReadiness) {
                    Image(systemName: "arrow.clockwise")
                }.buttonStyle(.plain).disabled(model.checking || model.permissionsBusy).help("Check setup again")
            }
            Text(model.setupMessage).font(.caption).foregroundStyle(.secondary).textSelection(.enabled)
            if !model.codexReady, model.setupMessage != model.codexMessage {
                Text(model.codexMessage).font(.caption).foregroundStyle(.orange).textSelection(.enabled)
            }
            if !model.permissionKnown || !model.screenReady || !model.inputReady {
                HStack {
                    if !model.inputReady {
                        Button("Enable Accessibility…") { model.requestPermission(.accessibility) }
                    }
                    if !model.screenReady {
                        Button("Enable Screen Recording…") { model.requestPermission(.screenRecording) }
                    }
                    if model.permissionsBusy { ProgressView().controlSize(.small) }
                }.disabled(model.permissionsBusy || model.session.active)
                HStack {
                    Menu("Privacy settings") {
                        Button("Screen Recording…") { model.openPrivacy("Privacy_ScreenCapture") }
                        Button("Accessibility…") { model.openPrivacy("Privacy_Accessibility") }
                    }.fixedSize()
                    Button("Show app in Finder", action: model.revealApp)
                }
                Text("Grant access to Cu, not Terminal. If Cu is missing or an old entry will not enable, use + in Privacy Settings to select /Applications/Cu.app. Relaunch if macOS asks.")
                    .font(.caption2).foregroundStyle(.secondary)
            }
            Text("App: \(Bundle.main.bundlePath)").font(.caption2).foregroundStyle(.tertiary).textSelection(.enabled)
            if !model.codexReady {
                Button("Copy ‘codex login’ for Terminal", action: model.copyLogin).font(.caption)
            }
        }
        .padding(14)
        .background(.quinary, in: RoundedRectangle(cornerRadius: 12))
    }
}

struct ReadinessChip: View {
    let title: String
    let ready: Bool?
    var body: some View {
        Label(title, systemImage: ready == true ? "checkmark.circle.fill" : ready == false ? "exclamationmark.circle" : "ellipsis.circle")
            .font(.caption.weight(.medium)).foregroundStyle(ready == true ? accent : ready == false ? .orange : .secondary)
            .accessibilityLabel("\(title): \(ready == true ? "ready" : ready == false ? "setup needed" : "checking or unavailable")")
    }
}

struct PhaseBadge: View {
    let phase: Session.Phase
    private var color: Color {
        switch phase {
        case .waiting, .error, .signInNeeded, .needsInput: return .orange
        case .stopping, .aborted: return .secondary
        default: return accent
        }
    }
    var body: some View {
        HStack(spacing: 5) {
            Circle().fill(color).frame(width: 6, height: 6)
            Text(phase.rawValue).font(.caption.weight(.medium))
        }
        .foregroundStyle(color).padding(.horizontal, 9).padding(.vertical, 5)
        .background(color.opacity(0.1), in: Capsule())
    }
}

struct ApprovalCard: View {
    @ObservedObject var model: AppModel
    let approval: Approval
    let runID: UUID
    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Label(approval.kind == "pause" ? "Resume this task?" : "Your approval is needed", systemImage: "hand.raised.fill")
                .font(.headline).foregroundStyle(.orange)
            ScrollView {
                Text(approval.message).font(.callout).textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }.frame(minHeight: 48, maxHeight: 130)
            HStack {
                Button("Decline", role: .cancel) {
                    model.approve(runID: runID, promptID: approval.id, approved: false)
                }
                Spacer()
                Button(approval.kind == "pause" ? "Resume" : "Allow once") {
                    model.approve(runID: runID, promptID: approval.id, approved: true)
                }.buttonStyle(.borderedProminent).tint(accent)
            }
        }
        .padding(14)
        .background(Color.orange.opacity(0.07), in: RoundedRectangle(cornerRadius: 12))
        .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(Color.orange.opacity(0.25)))
        .disabled(model.respondingToApproval)
    }
}

struct TaskProgressStrip: View {
    let session: Session
    var body: some View {
        HStack(spacing: 12) {
            if session.active, session.phase != .waiting, session.phase != .stopping {
                ProgressView().controlSize(.small)
            } else {
                Image(systemName: session.phase == .done ? "checkmark.circle.fill" : "circle.dotted")
                    .foregroundStyle(accent)
            }
            Text(session.stageLabel).font(.caption.weight(.semibold)).lineLimit(1)
            Spacer(minLength: 0)
            Text("Step \(session.step)")
            Text("\(session.actionsExecuted) input actions")
        }
        .font(.caption).monospacedDigit()
        .padding(.horizontal, 18).padding(.vertical, 11)
        .background(accent.opacity(0.07))
        .accessibilityElement(children: .combine)
        .help("Observed planning step and successfully posted input actions. The total task size is unknown; this is not a completion percentage.")
    }
}
