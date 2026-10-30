import CCu
import CoreGraphics
import Foundation

/// Main-display geometry. The generation counter increases whenever bounds or pixel size change,
/// detected by comparing a signature on every query (works without a run loop, unlike
/// `CGDisplayRegisterReconfigurationCallback`).
final class DisplayTracker: @unchecked Sendable {
    static let shared = DisplayTracker()

    private let lock = NSLock()
    private var lastSignature: [Double] = []
    private var generation: UInt64 = 0

    func current() throws -> CuDisplayInfo {
        let id = CGMainDisplayID()
        let bounds = CGDisplayBounds(id)
        // CGDisplayPixelsWide/High can be misleading for scaled modes; the mode has the real size.
        guard let mode = CGDisplayCopyDisplayMode(id) else {
            throw CuError.fail("CGDisplayCopyDisplayMode returned nil for display \(id)")
        }
        let pixelW = mode.pixelWidth
        let pixelH = mode.pixelHeight
        guard bounds.width > 0, bounds.height > 0, pixelW > 0, pixelH > 0 else {
            throw CuError.fail("display \(id) reports an empty size")
        }
        let scale = Double(pixelW) / Double(bounds.width)

        let signature = [Double(id), bounds.origin.x, bounds.origin.y, bounds.width, bounds.height,
                         Double(pixelW), Double(pixelH)]
        let gen: UInt64 = lock.withLock {
            if signature != lastSignature {
                if !lastSignature.isEmpty { generation += 1 }
                lastSignature = signature
            }
            return generation
        }

        return CuDisplayInfo(
            display_id: id,
            bounds_points: CuRect(x: bounds.origin.x, y: bounds.origin.y, w: bounds.width, h: bounds.height),
            pixel_w: UInt32(pixelW),
            pixel_h: UInt32(pixelH),
            scale: scale,
            generation: gen
        )
    }
}

func cursorPosition() throws -> CGPoint {
    guard let event = CGEvent(source: nil) else { throw CuError.fail("CGEvent(source: nil) failed") }
    return event.location
}
