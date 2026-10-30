import Accelerate
import CCu
import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

/// CPU image operations (vImage + ImageIO). The Metal path (milestone M7) replaces resize for
/// streamed frames; this stays as the fallback and test baseline.
enum ImageOps {
    static func bgraFormat() throws -> vImage_CGImageFormat {
        guard let space = CGColorSpace(name: CGColorSpace.sRGB),
              let format = vImage_CGImageFormat(
                  bitsPerComponent: 8,
                  bitsPerPixel: 32,
                  colorSpace: space,
                  bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedFirst.rawValue
                      | CGBitmapInfo.byteOrder32Little.rawValue))
        else { throw CuError.fail("could not create BGRA image format") }
        return format
    }

    static func resize(_ image: CGImage, width: Int, height: Int) throws -> CGImage {
        var format = try bgraFormat()
        let src = try vImage.PixelBuffer<vImage.Interleaved8x4>(cgImage: image, cgImageFormat: &format)
        let dst = vImage.PixelBuffer<vImage.Interleaved8x4>(width: width, height: height)
        src.scale(destination: dst)
        guard let out = dst.makeCGImage(cgImageFormat: format) else {
            throw CuError.fail("vImage could not create the resized image")
        }
        return out
    }

    static func encode(_ image: CGImage, format: Int32, quality: Float) throws -> Data {
        let type: UTType
        switch format {
        case Int32(CU_FMT_JPEG): type = .jpeg
        case Int32(CU_FMT_PNG): type = .png
        default: throw CuError.invalid("unknown image format \(format)")
        }
        let data = NSMutableData()
        guard let dest = CGImageDestinationCreateWithData(data as CFMutableData, type.identifier as CFString, 1, nil) else {
            throw CuError.fail("CGImageDestinationCreateWithData failed")
        }
        var props: [CFString: Any] = [:]
        if type == .jpeg {
            props[kCGImageDestinationLossyCompressionQuality] = Double(min(max(quality, 0.05), 1.0))
        }
        CGImageDestinationAddImage(dest, image, props as CFDictionary)
        guard CGImageDestinationFinalize(dest) else { throw CuError.fail("image encoding failed") }
        return data as Data
    }
}
