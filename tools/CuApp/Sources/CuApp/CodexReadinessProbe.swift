import Foundation
import OSLog

/// stdout is one JSON status document. stderr is diagnostic text, never part of
/// that document. A warning (even one labelled ERROR by Codex) is not auth failure.
struct CodexStatusOutput {
    private(set) var stdout = Data()
    private(set) var stderr = Data()
    private(set) var stdoutTruncated = false
    private let stdoutLimit = 16384
    private let stderrLimit = 8192

    mutating func appendStdout(_ data: Data) {
        let remaining = max(0, stdoutLimit - stdout.count)
        if data.count > remaining { stdoutTruncated = true }
        stdout.append(data.prefix(remaining))
    }

    mutating func appendStderr(_ data: Data) {
        stderr.append(data)
        if stderr.count > stderrLimit { stderr.removeFirst(stderr.count - stderrLimit) }
    }

    func result(exitCode: Int32, timedOut: Bool = false) -> CodexReadiness {
        if timedOut {
            return CodexReadiness(ready: false, message: "The live Codex connection check timed out. Check connectivity and refresh; a timeout alone does not mean your sign-in is invalid.")
        }
        if stdoutTruncated {
            return CodexReadiness(ready: false, message: "Codex status output exceeded the protocol limit. Refresh the connection; this is not evidence of a sign-in failure.")
        }
        if let status = try? JSONDecoder().decode(CodexReadiness.self, from: stdout) {
            if exitCode != 0 && status.ready {
                return CodexReadiness(ready: false, message: "Codex reported ready but its check exited with status \(exitCode). Refresh the connection before starting a task.")
            }
            let message = status.message.trimmingCharacters(in: .whitespacesAndNewlines)
            return CodexReadiness(ready: exitCode == 0 && status.ready,
                                  message: message.isEmpty ? (status.ready ? "Codex subscription connected (live check verified)" : "Codex is not ready. Refresh the connection to check again.") : status.message)
        }
        let diagnostic = String(decoding: stderr, as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines)
        let detail = diagnostic.isEmpty ? "" : " Diagnostic output: \(diagnostic.suffix(1200))"
        return CodexReadiness(ready: false, message: "Could not read the Codex status response (exit \(exitCode)). Refresh the connection; this is not evidence of a sign-in failure.\(detail)")
    }
}

@MainActor
enum CodexReadinessProbe {
    private static let log = Logger(subsystem: "dev.ankojh.cu", category: "CodexReadiness")

    /// Direct CLI transport shared by native UI and regression tests. No shell,
    /// model turn, screenshot, desktop input, permission request, or login change.
    static func start(executable: String, arguments: [String] = ["codex-status"],
                      environment: [String: String], timeout: TimeInterval = 25,
                      completion: @escaping (CodexReadiness) -> Void) throws -> EngineProcess {
        let child = EngineProcess()
        var output = CodexStatusOutput()
        var timedOut = false
        try child.start(executable: executable, arguments: arguments, environment: environment,
            stdout: { output.appendStdout($0) },
            stderr: { output.appendStderr($0) },
            completion: { code in
                let status = output.result(exitCode: code, timedOut: timedOut)
                log.notice("Live Codex check: ready=\(status.ready, privacy: .public) exit=\(code, privacy: .public) timedOut=\(timedOut, privacy: .public) stdoutBytes=\(output.stdout.count, privacy: .public) stderrBytes=\(output.stderr.count, privacy: .public)")
                completion(status)
            })
        DispatchQueue.main.asyncAfter(deadline: .now() + max(0, timeout)) { [weak child] in
            guard let child else { return }
            timedOut = true
            child.stop(grace: 1)
        }
        return child
    }
}
