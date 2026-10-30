import CCu
import Foundation

/// Error carried back across the C ABI as a status code plus a thread-local message.
struct CuError: Error {
    let code: Int32
    let message: String

    static func fail(_ message: String) -> CuError { CuError(code: Int32(CU_ERR), message: message) }
    static func invalid(_ message: String) -> CuError { CuError(code: Int32(CU_ERR_INVALID_ARG), message: message) }
    static func permission(_ message: String) -> CuError { CuError(code: Int32(CU_ERR_PERMISSION), message: message) }
    static func timeout(_ message: String) -> CuError { CuError(code: Int32(CU_ERR_TIMEOUT), message: message) }
    static func aborted(_ message: String) -> CuError { CuError(code: Int32(CU_ERR_ABORTED), message: message) }
    static func unimplemented(_ what: String, milestone: String) -> CuError {
        CuError(code: Int32(CU_ERR_UNIMPLEMENTED), message: "\(what) is not implemented yet (milestone \(milestone))")
    }
}

private let lastErrorKey = "cu.lastError"

func setLastError(_ message: String) {
    Thread.current.threadDictionary[lastErrorKey] = message
}

func lastError() -> String {
    Thread.current.threadDictionary[lastErrorKey] as? String ?? ""
}

/// Runs `body`, translating thrown errors into a status code and recording the message.
func guarded(_ body: () throws -> Void) -> Int32 {
    do {
        try body()
        return Int32(CU_OK)
    } catch let e as CuError {
        setLastError(e.message)
        return e.code
    } catch {
        setLastError(String(describing: error))
        return Int32(CU_ERR)
    }
}

/// Copies `data` into a malloc'd buffer owned by the caller (released with `cu_buf_free`).
func makeBuf(_ data: Data) -> CuBuf {
    let len = data.count
    let ptr = malloc(max(len, 1))!.assumingMemoryBound(to: UInt8.self)
    data.copyBytes(to: ptr, count: len)
    return CuBuf(ptr: ptr, len: len)
}

func writeBuf(_ out: UnsafeMutablePointer<CuBuf>?, _ data: Data) throws {
    guard let out else { throw CuError.invalid("output buffer pointer is null") }
    out.pointee = makeBuf(data)
}

func jsonData(_ object: Any) throws -> Data {
    try JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
}

/// Monotonic nanoseconds (CLOCK_UPTIME_RAW), the timebase for every `*_ns` field.
func nowNs() -> UInt64 { clock_gettime_nsec_np(CLOCK_UPTIME_RAW) }

/// Sleeps in small slices so that `cu_release_all` can interrupt long waits.
func interruptibleSleep(ms: UInt32, epoch: UInt64) throws {
    let deadline = nowNs() + UInt64(ms) * 1_000_000
    while true {
        if InputState.shared.abortEpoch != epoch { throw CuError.aborted("aborted by cu_release_all") }
        let now = nowNs()
        if now >= deadline { return }
        usleep(UInt32(min(10_000, (deadline - now) / 1_000)))
    }
}

func sleepMs(_ ms: Double) { usleep(UInt32(ms * 1000)) }

/// Mutable box for bridging completion-handler APIs into synchronous calls.
final class ResultBox<T>: @unchecked Sendable {
    var value: T?
    var error: Error?
}

/// Calls a completion-handler API and blocks the current (non-main) thread until it completes.
func blocking<T>(timeoutSeconds: Double, _ what: String,
                 _ start: (@escaping @Sendable (T?, Error?) -> Void) -> Void) throws -> T {
    let box = ResultBox<T>()
    let sem = DispatchSemaphore(value: 0)
    start { value, error in
        box.value = value
        box.error = error
        sem.signal()
    }
    if sem.wait(timeout: .now() + timeoutSeconds) == .timedOut {
        throw CuError.timeout("\(what) timed out after \(timeoutSeconds)s")
    }
    if let error = box.error { throw error }
    guard let value = box.value else { throw CuError.fail("\(what) returned no result") }
    return value
}
