import Foundation
import ApplicationServices

struct PermissionSnapshot: Codable, Equatable {
    let screenRecording: Bool
    let accessibility: Bool
}

enum PermissionKind: String, CaseIterable {
    case accessibility, screenRecording
    var title: String { self == .accessibility ? "Accessibility" : "Screen Recording" }
    var pane: String { self == .accessibility ? "Privacy_Accessibility" : "Privacy_ScreenCapture" }
}

enum PermissionOperation: String {
    case check = "--permission-check"
    case accessibility = "--request-accessibility"
    case screenRecording = "--request-screen-recording"

    /// Run in a short-lived copy of this same signed executable, inside the same
    /// bundle, with Cu as the responsible app. A wedged TCC/XPC call must never
    /// block the menu bar, stop button, or the app's main run loop.
    static func runIfRequested(arguments: [String]) -> Bool {
        guard let operation = arguments.dropFirst().first.flatMap(Self.init(rawValue:)) else { return false }
        switch operation {
        case .check: break
        case .accessibility:
            _ = AXIsProcessTrustedWithOptions(["AXTrustedCheckOptionPrompt": true] as CFDictionary)
        case .screenRecording:
            _ = CGRequestScreenCaptureAccess()
        }
        let snapshot = PermissionSnapshot(screenRecording: CGPreflightScreenCaptureAccess(),
                                          accessibility: AXIsProcessTrusted())
        if let data = try? JSONEncoder().encode(snapshot) {
            FileHandle.standardOutput.write(data + Data([10]))
        }
        return true
    }
}

/// One bounded operation at a time. A timeout is an unknown result, not a denial.
/// The runner is injectable so hanging OS calls can be tested without touching TCC.
@MainActor
final class PermissionService {
    enum Failure: LocalizedError {
        case timedOut, invalidResponse, unavailable
        var errorDescription: String? {
            switch self {
            case .timedOut: return "macOS did not answer the permission check. Cu is still responsive. Open Privacy Settings, then check again or relaunch Cu."
            case .invalidResponse: return "Could not read macOS permission status. Check Privacy Settings and relaunch Cu if you changed a grant."
            case .unavailable: return "The installed Cu executable could not be found. Reinstall Cu in /Applications."
            }
        }
    }
    private let executable: String?
    private let environment: [String: String]
    private let arguments: (PermissionOperation) -> [String]
    private var current: (id: UUID, child: EngineProcess, timer: Timer)?

    init(executable: String? = Bundle.main.executableURL?.path,
         environment: [String: String] = ProcessInfo.processInfo.environment,
         arguments: @escaping (PermissionOperation) -> [String] = { [$0.rawValue] }) {
        self.executable = executable
        self.environment = environment
        self.arguments = arguments
    }

    @discardableResult
    func run(_ operation: PermissionOperation, timeout: TimeInterval? = nil,
             completion: @escaping (Result<PermissionSnapshot, Error>) -> Void) -> Bool {
        guard current == nil else { return false }
        guard let executable else { completion(.failure(Failure.unavailable)); return true }
        let child = EngineProcess()
        let id = UUID()
        var output = Data()
        let timer = Timer(timeInterval: timeout ?? (operation == .check ? 4 : 20), repeats: false) { [weak self] _ in
            Task { @MainActor in
                guard let self, self.current?.id == id else { return }
                self.current = nil
                child.stop(grace: 0.2)
                completion(.failure(Failure.timedOut))
            }
        }
        current = (id, child, timer)
        RunLoop.main.add(timer, forMode: .common)
        do {
            try child.start(executable: executable, arguments: arguments(operation), environment: environment,
                stdout: { data in output.append(data.prefix(max(0, 16384 - output.count))) },
                stderr: { _ in }, completion: { [weak self] code in
                    guard let self, self.current?.id == id else { return }
                    self.current?.timer.invalidate()
                    self.current = nil
                    guard code == 0, let snapshot = try? JSONDecoder().decode(PermissionSnapshot.self, from: output) else {
                        completion(.failure(Failure.invalidResponse))
                        return
                    }
                    completion(.success(snapshot))
                })
        } catch {
            timer.invalidate()
            current = nil
            completion(.failure(error))
        }
        return true
    }

    func cancel() {
        current?.timer.invalidate()
        current?.child.stop(grace: 0.2)
        current = nil
    }
}
