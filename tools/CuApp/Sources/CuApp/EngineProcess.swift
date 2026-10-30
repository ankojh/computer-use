import Foundation
import Darwin

/// Direct native child transport: no shell, HTTP server, Node bridge, or WebView.
/// Each child owns a POSIX process group so stopping/quit also reaps its planner.
final class EngineProcess: @unchecked Sendable {
    private let lock = NSLock()
    private var pid: pid_t = 0
    private var input: FileHandle?

    func start(executable: String, arguments: [String], environment: [String: String],
               stdout: @escaping (Data) -> Void, stderr: @escaping (Data) -> Void,
               completion: @escaping (Int32) -> Void) throws {
        signal(SIGPIPE, SIG_IGN)
        let inPipe = Pipe(), outPipe = Pipe(), errPipe = Pipe()
        var actions: posix_spawn_file_actions_t?
        var attributes: posix_spawnattr_t?
        posix_spawn_file_actions_init(&actions)
        posix_spawnattr_init(&attributes)
        defer {
            posix_spawn_file_actions_destroy(&actions)
            posix_spawnattr_destroy(&attributes)
        }
        for (source, target) in [(inPipe.fileHandleForReading.fileDescriptor, STDIN_FILENO),
                                 (outPipe.fileHandleForWriting.fileDescriptor, STDOUT_FILENO),
                                 (errPipe.fileHandleForWriting.fileDescriptor, STDERR_FILENO)] {
            posix_spawn_file_actions_adddup2(&actions, source, target)
        }
        for pipe in [inPipe, outPipe, errPipe] {
            posix_spawn_file_actions_addclose(&actions, pipe.fileHandleForReading.fileDescriptor)
            posix_spawn_file_actions_addclose(&actions, pipe.fileHandleForWriting.fileDescriptor)
        }
        posix_spawnattr_setpgroup(&attributes, 0)
        // GUI apps may inherit ignored signals; the engine must receive Ctrl-C normally.
        var defaults = sigset_t()
        sigemptyset(&defaults)
        for s in [SIGINT, SIGTERM, SIGPIPE] { sigaddset(&defaults, s) }
        posix_spawnattr_setsigdefault(&attributes, &defaults)
        var mask = sigset_t()
        sigemptyset(&mask)
        posix_spawnattr_setsigmask(&attributes, &mask)
        posix_spawnattr_setflags(&attributes, Int16(POSIX_SPAWN_SETPGROUP | POSIX_SPAWN_SETSIGDEF | POSIX_SPAWN_SETSIGMASK | POSIX_SPAWN_CLOEXEC_DEFAULT))
        let argv = ([executable] + arguments).map { strdup($0) } + [nil]
        let env = environment.sorted { $0.key < $1.key }.map { strdup("\($0.key)=\($0.value)") } + [nil]
        defer { for p in argv + env { if let p { free(p) } } }
        var child: pid_t = 0
        let result = argv.withUnsafeBufferPointer { args in
            env.withUnsafeBufferPointer { vars in
                posix_spawn(&child, executable, &actions, &attributes,
                            UnsafeMutablePointer(mutating: args.baseAddress!),
                            UnsafeMutablePointer(mutating: vars.baseAddress!))
            }
        }
        guard result == 0 else { throw POSIXError(POSIXErrorCode(rawValue: result) ?? .EIO) }
        lock.lock()
        pid = child
        input = inPipe.fileHandleForWriting
        lock.unlock()
        try? inPipe.fileHandleForReading.close()
        try? outPipe.fileHandleForWriting.close()
        try? errPipe.fileHandleForWriting.close()

        let readers = DispatchGroup()
        for (handle, callback) in [(outPipe.fileHandleForReading, stdout), (errPipe.fileHandleForReading, stderr)] {
            readers.enter()
            DispatchQueue.global(qos: .utility).async {
                defer { try? handle.close(); readers.leave() }
                var buffer = [UInt8](repeating: 0, count: 16384)
                while true {
                    let count = buffer.withUnsafeMutableBytes { bytes in
                        Darwin.read(handle.fileDescriptor, bytes.baseAddress!, bytes.count)
                    }
                    if count < 0 && errno == EINTR { continue }
                    guard count > 0 else { break }
                    let data = Data(buffer.prefix(count))
                    DispatchQueue.main.async { callback(data) }
                }
            }
        }
        DispatchQueue.global(qos: .utility).async { [self] in
            var status: Int32 = 0
            while waitpid(child, &status, 0) == -1 && errno == EINTR {}
            lock.lock()
            // The parent has exited. No orphan planner may retain our pipes or keep running.
            kill(-child, SIGKILL)
            pid = 0
            try? input?.close()
            input = nil
            lock.unlock()
            readers.wait()
            let code = (status & 0x7f) == 0 ? (status >> 8) & 0xff : 128 + (status & 0x7f)
            DispatchQueue.main.async { completion(code) }
        }
    }

    @discardableResult
    func send(_ data: Data) -> Bool {
        lock.lock()
        defer { lock.unlock() }
        guard let input else { return false }
        do { try input.write(contentsOf: data); return true }
        catch { return false }
    }

    func closeInput() {
        lock.lock()
        try? input?.close()
        input = nil
        lock.unlock()
    }

    func stop(grace: TimeInterval = 3) {
        lock.lock()
        let stoppingPID = pid
        // Signal the engine itself first so it can release held keys and cancel
        // Codex cleanly. EOF unblocks any synchronous confirmation read.
        if pid > 0 { kill(pid, SIGINT) }
        try? input?.close()
        input = nil
        lock.unlock()
        DispatchQueue.global(qos: .utility).asyncAfter(deadline: .now() + grace) { [self] in
            lock.lock()
            if pid > 0, pid == stoppingPID { kill(-pid, SIGKILL) }
            lock.unlock()
        }
    }
}
