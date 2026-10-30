import Foundation

/// Wait for observed focus, not merely a successful activation request, before
/// releasing an approval to the engine. Dependencies are injectable for tests.
@MainActor
enum FocusRestorer {
    static func restore(pid: Int32, activate: () -> Bool, frontmost: @escaping () -> Int32?,
                        timeout: TimeInterval = 1, completion: @escaping (Bool) -> Void) {
        guard pid > 0 else { completion(false); return }
        if frontmost() == pid { completion(true); return }
        guard activate() else { completion(false); return }
        poll(pid: pid, deadline: ProcessInfo.processInfo.systemUptime + max(0, timeout), frontmost: frontmost, completion: completion)
    }

    private static func poll(pid: Int32, deadline: TimeInterval, frontmost: @escaping () -> Int32?,
                             completion: @escaping (Bool) -> Void) {
        if frontmost() == pid { completion(true); return }
        guard ProcessInfo.processInfo.systemUptime < deadline else { completion(false); return }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.02) {
            poll(pid: pid, deadline: deadline, frontmost: frontmost, completion: completion)
        }
    }
}
