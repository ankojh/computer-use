import CCu
import CoreGraphics
import Foundation

/// A synchronous, thread-local observer, not an event tap. It never follows the
/// human pointer, controls input, or survives the input call that borrowed it.
final class InputPointerFeedback {
    private static let key = "dev.ankojh.cu.input-pointer-observer"
    private let callback: CuPointerCallback
    private let context: UnsafeMutableRawPointer?

    private init(_ callback: @escaping CuPointerCallback, _ context: UnsafeMutableRawPointer?) {
        self.callback = callback
        self.context = context
    }
    static func install(_ callback: CuPointerCallback?, context: UnsafeMutableRawPointer?) {
        if let callback { Thread.current.threadDictionary[key] = InputPointerFeedback(callback, context) }
        else { Thread.current.threadDictionary.removeObject(forKey: key) }
    }

    struct Sample: Equatable {
        let kind: Int32
        let x: Double
        let y: Double
        let button: Int32
        let count: Int32
    }
    static func sample(_ event: CGEvent) -> Sample? {
        let kind: Int32
        let button: Int32
        switch event.type {
        case .mouseMoved: kind = Int32(CU_POINTER_MOVE); button = -1
        case .leftMouseDragged: kind = Int32(CU_POINTER_DRAG); button = 0
        case .rightMouseDragged: kind = Int32(CU_POINTER_DRAG); button = 1
        case .otherMouseDragged: kind = Int32(CU_POINTER_DRAG); button = Int32(event.getIntegerValueField(.mouseEventButtonNumber))
        case .leftMouseDown: kind = Int32(CU_POINTER_DOWN); button = 0
        case .rightMouseDown: kind = Int32(CU_POINTER_DOWN); button = 1
        case .otherMouseDown: kind = Int32(CU_POINTER_DOWN); button = Int32(event.getIntegerValueField(.mouseEventButtonNumber))
        case .leftMouseUp: kind = Int32(CU_POINTER_UP); button = 0
        case .rightMouseUp: kind = Int32(CU_POINTER_UP); button = 1
        case .otherMouseUp: kind = Int32(CU_POINTER_UP); button = Int32(event.getIntegerValueField(.mouseEventButtonNumber))
        default: return nil // No keyboard data, human events, or fabricated click success.
        }
        let count = (kind == CU_POINTER_DOWN || kind == CU_POINTER_UP)
            ? Int32(clamping: event.getIntegerValueField(.mouseEventClickState)) : 0
        return Sample(kind: kind, x: event.location.x, y: event.location.y, button: button, count: count)
    }
    static func posted(_ event: CGEvent) {
        guard let observer = Thread.current.threadDictionary[key] as? InputPointerFeedback,
              let sample = sample(event) else { return }
        observer.callback(observer.context, sample.kind, sample.x, sample.y, sample.button, sample.count)
    }
}

@_cdecl("cu_set_pointer_callback")
public func cu_set_pointer_callback(_ callback: CuPointerCallback?, _ context: UnsafeMutableRawPointer?) {
    InputPointerFeedback.install(callback, context: context)
}
