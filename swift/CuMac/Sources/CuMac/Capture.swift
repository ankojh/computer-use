import CCu
import CoreGraphics
import Foundation
import ScreenCaptureKit

/// One-shot capture via SCScreenshotManager (macOS 14+). Used for `cu screenshot` and as the
/// fallback before the continuous SCStream (milestone M7) exists.
enum OneShotCapture {
    private static let lock = NSLock()
    nonisolated(unsafe) private static var frameSeq: UInt64 = 0

    static func nextSeq() -> UInt64 {
        lock.withLock {
            frameSeq += 1
            return frameSeq
        }
    }

    /// Captures the main display at native device-pixel resolution.
    static func captureMainDisplay(showsCursor: Bool, display: CuDisplayInfo) throws -> CGImage {
        guard CGPreflightScreenCaptureAccess() else {
            throw CuError.permission("Screen Recording permission is not granted (run `cu doctor`)")
        }
        let content: SCShareableContent = try blocking(timeoutSeconds: 5, "SCShareableContent") { done in
            SCShareableContent.getExcludingDesktopWindows(false, onScreenWindowsOnly: true) { content, error in
                done(content, error)
            }
        }
        guard let scDisplay = content.displays.first(where: { $0.displayID == display.display_id }) else {
            throw CuError.fail("main display \(display.display_id) not found in shareable content")
        }

        // Never put the native controls or highlighted cursor into the model's
        // screenshots. Window sharingType alone does not reliably exclude SCK.
        let cu = content.applications.filter { isCuFeedbackApplication($0.bundleIdentifier) }
        let filter = SCContentFilter(display: scDisplay, excludingApplications: cu, exceptingWindows: [])
        let config = SCStreamConfiguration()
        config.width = Int(display.pixel_w)
        config.height = Int(display.pixel_h)
        config.pixelFormat = kCVPixelFormatType_32BGRA
        config.showsCursor = showsCursor
        config.capturesAudio = false

        return try blocking(timeoutSeconds: 5, "SCScreenshotManager.captureImage") { done in
            SCScreenshotManager.captureImage(contentFilter: filter, configuration: config) { image, error in
                done(image, error)
            }
        }
    }
}

/// Exact bundle match: do not exclude browsers, other apps, or lookalike IDs.
func isCuFeedbackApplication(_ bundleID: String) -> Bool { bundleID == "dev.ankojh.cu" }

struct EncodedFrame {
    let info: CuFrameInfo
    let bytes: Data
}

/// Captures, optionally crops (crop rect in global points), downsizes to `maxLongEdge`
/// preserving aspect ratio, and encodes.
func captureAndEncode(maxLongEdge: UInt32, format: Int32, quality: Float, cropPoints: CuRect?,
                      showsCursor: Bool = true) throws -> EncodedFrame {
    let display = try DisplayTracker.shared.current()
    let capturedAt = nowNs()
    let native = try OneShotCapture.captureMainDisplay(showsCursor: showsCursor, display: display)
    let seq = OneShotCapture.nextSeq()

    let b = display.bounds_points
    // The captured image may not exactly match the mode's pixel size; derive scale from the image.
    let sx = Double(native.width) / b.w
    let sy = Double(native.height) / b.h

    var sourcePoints = b
    var image = native
    if let crop = cropPoints {
        let px = CGRect(x: (crop.x - b.x) * sx, y: (crop.y - b.y) * sy, width: crop.w * sx, height: crop.h * sy)
            .integral
            .intersection(CGRect(x: 0, y: 0, width: native.width, height: native.height))
        guard !px.isNull, px.width >= 1, px.height >= 1 else {
            throw CuError.invalid("crop rect \(crop) is outside the display")
        }
        guard let cropped = native.cropping(to: px) else { throw CuError.fail("CGImage cropping failed") }
        image = cropped
        // Report the region actually covered after rounding to whole device pixels.
        sourcePoints = CuRect(x: b.x + px.origin.x / sx, y: b.y + px.origin.y / sy,
                              w: px.width / sx, h: px.height / sy)
    }

    let (w, h) = fitSize(width: image.width, height: image.height, maxLongEdge: Int(maxLongEdge))
    if w != image.width || h != image.height {
        image = try ImageOps.resize(image, width: w, height: h)
    }
    let bytes = try ImageOps.encode(image, format: format, quality: quality)

    let info = CuFrameInfo(
        frame_seq: seq,
        captured_at_ns: capturedAt,
        image_w: UInt32(image.width),
        image_h: UInt32(image.height),
        source_rect_points: sourcePoints,
        scale: display.scale,
        display_generation: display.generation
    )
    return EncodedFrame(info: info, bytes: bytes)
}

/// Fits `width × height` inside a square of `maxLongEdge`, preserving aspect ratio.
/// `maxLongEdge == 0` or an already-small image keeps the original size.
func fitSize(width: Int, height: Int, maxLongEdge: Int) -> (Int, Int) {
    let long = max(width, height)
    guard maxLongEdge > 0, long > maxLongEdge else { return (width, height) }
    let s = Double(maxLongEdge) / Double(long)
    if width >= height {
        return (maxLongEdge, max(1, Int((Double(height) * s).rounded())))
    }
    return (max(1, Int((Double(width) * s).rounded())), maxLongEdge)
}
