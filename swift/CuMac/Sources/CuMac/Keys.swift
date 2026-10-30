import Carbon
import CCu
import CoreGraphics
import Foundation

struct Modifier {
    let keyCode: CGKeyCode
    let flag: CGEventFlags
    let bit: UInt32
}

struct ParsedCombo {
    var mods: UInt32 = 0
    /// Main key; nil for modifier-only combos like "shift".
    var key: CGKeyCode?
}

/// Canonical key names (produced by cu-core's keymap) → macOS virtual key codes.
enum Keys {
    /// Press order: ctrl, opt, shift, cmd, fn. Released in reverse.
    static let modifierOrder: [UInt32] = [
        UInt32(CU_MOD_CTRL), UInt32(CU_MOD_OPT), UInt32(CU_MOD_SHIFT), UInt32(CU_MOD_CMD), UInt32(CU_MOD_FN),
    ]

    static func modifier(_ bit: UInt32) -> Modifier {
        switch bit {
        case UInt32(CU_MOD_CMD): return Modifier(keyCode: 0x37, flag: .maskCommand, bit: bit)
        case UInt32(CU_MOD_SHIFT): return Modifier(keyCode: 0x38, flag: .maskShift, bit: bit)
        case UInt32(CU_MOD_OPT): return Modifier(keyCode: 0x3A, flag: .maskAlternate, bit: bit)
        case UInt32(CU_MOD_CTRL): return Modifier(keyCode: 0x3B, flag: .maskControl, bit: bit)
        default: return Modifier(keyCode: 0x3F, flag: .maskSecondaryFn, bit: bit)
        }
    }

    static func flags(_ mods: UInt32) -> CGEventFlags {
        var f: CGEventFlags = []
        for bit in modifierOrder where mods & bit != 0 { f.insert(modifier(bit).flag) }
        return f
    }

    static let modifierNames: [String: UInt32] = [
        "cmd": UInt32(CU_MOD_CMD), "shift": UInt32(CU_MOD_SHIFT), "opt": UInt32(CU_MOD_OPT),
        "ctrl": UInt32(CU_MOD_CTRL), "fn": UInt32(CU_MOD_FN),
    ]

    static let named: [String: CGKeyCode] = [
        "return": 0x24, "tab": 0x30, "space": 0x31, "backspace": 0x33, "forwarddelete": 0x75,
        "escape": 0x35, "home": 0x73, "end": 0x77, "pageup": 0x74, "pagedown": 0x79,
        "left": 0x7B, "right": 0x7C, "down": 0x7D, "up": 0x7E, "capslock": 0x39, "help": 0x72,
        "keypadenter": 0x4C,
        "f1": 0x7A, "f2": 0x78, "f3": 0x63, "f4": 0x76, "f5": 0x60, "f6": 0x61, "f7": 0x62, "f8": 0x64,
        "f9": 0x65, "f10": 0x6D, "f11": 0x67, "f12": 0x6F, "f13": 0x69, "f14": 0x6B, "f15": 0x71,
        "f16": 0x6A, "f17": 0x40, "f18": 0x4F, "f19": 0x50, "f20": 0x5A,
        "plus": 0x18,  // '=' key; "plus" itself needs shift on US layouts (added in parse)
    ]

    /// US ANSI physical positions, used when the layout map has no entry.
    static let ansi: [Character: CGKeyCode] = [
        "a": 0x00, "s": 0x01, "d": 0x02, "f": 0x03, "h": 0x04, "g": 0x05, "z": 0x06, "x": 0x07,
        "c": 0x08, "v": 0x09, "b": 0x0B, "q": 0x0C, "w": 0x0D, "e": 0x0E, "r": 0x0F, "y": 0x10,
        "t": 0x11, "1": 0x12, "2": 0x13, "3": 0x14, "4": 0x15, "6": 0x16, "5": 0x17, "=": 0x18,
        "9": 0x19, "7": 0x1A, "-": 0x1B, "8": 0x1C, "0": 0x1D, "]": 0x1E, "o": 0x1F, "u": 0x20,
        "[": 0x21, "i": 0x22, "p": 0x23, "l": 0x25, "j": 0x26, "'": 0x27, "k": 0x28, ";": 0x29,
        "\\": 0x2A, ",": 0x2B, "/": 0x2C, "n": 0x2D, "m": 0x2E, ".": 0x2F, "`": 0x32,
    ]

    /// Shifted US ANSI symbols → unshifted base character.
    static let ansiShifted: [Character: Character] = [
        "!": "1", "@": "2", "#": "3", "$": "4", "%": "5", "^": "6", "&": "7", "*": "8", "(": "9",
        ")": "0", "_": "-", "+": "=", "{": "[", "}": "]", "|": "\\", ":": ";", "\"": "'", "<": ",",
        ">": ".", "?": "/", "~": "`",
    ]

    private static let lock = NSLock()
    /// Character → (key code, needs shift) for the current keyboard layout.
    nonisolated(unsafe) private static var layoutMap: [Character: (CGKeyCode, Bool)] = [:]

    /// Builds the layout map with UCKeyTranslate. Text Input Source APIs must run on the main thread.
    static func buildLayoutMap() {
        guard Thread.isMainThread else { return }
        guard let source = TISCopyCurrentKeyboardLayoutInputSource()?.takeRetainedValue(),
              let raw = TISGetInputSourceProperty(source, kTISPropertyUnicodeKeyLayoutData)
        else { return }
        let data = Unmanaged<CFData>.fromOpaque(raw).takeUnretainedValue()
        guard let bytes = CFDataGetBytePtr(data) else { return }

        var map: [Character: (CGKeyCode, Bool)] = [:]
        bytes.withMemoryRebound(to: UCKeyboardLayout.self, capacity: 1) { layout in
            // 0x00..<0x33 is the main typing block; skipping the keypad keeps "1" on the top row.
            for shift in [false, true] {
                for code in 0..<0x33 {
                    var dead: UInt32 = 0
                    var length = 0
                    var chars = [UniChar](repeating: 0, count: 4)
                    let modState: UInt32 = shift ? UInt32((shiftKey >> 8) & 0xFF) : 0
                    let status = UCKeyTranslate(layout, UInt16(code), UInt16(kUCKeyActionDown), modState,
                                                UInt32(LMGetKbdType()), OptionBits(kUCKeyTranslateNoDeadKeysBit),
                                                &dead, chars.count, &length, &chars)
                    guard status == noErr, length == 1,
                          let scalar = Unicode.Scalar(chars[0]) else { continue }
                    let ch = Character(scalar)
                    if map[ch] == nil { map[ch] = (CGKeyCode(code), shift) }
                }
            }
        }
        lock.withLock { layoutMap = map }
    }

    static func lookupChar(_ ch: Character) -> (CGKeyCode, Bool)? {
        if let hit = lock.withLock({ layoutMap[ch] }) { return hit }
        let lower = Character(ch.lowercased())
        if let hit = lock.withLock({ layoutMap[lower] }) { return (hit.0, hit.1) }
        if let code = ansi[lower] { return (code, false) }
        if let base = ansiShifted[ch], let code = ansi[base] { return (code, true) }
        return nil
    }

    /// Parses a canonical combo such as "cmd+shift+t". Exactly one non-modifier key is allowed.
    static func parse(_ combo: String) throws -> ParsedCombo {
        var parsed = ParsedCombo()
        let tokens = combo.split(separator: "+", omittingEmptySubsequences: false).map(String.init)
        guard !tokens.isEmpty, !tokens.contains(where: \.isEmpty) else {
            throw CuError.invalid("malformed key combo '\(combo)' (use 'plus' for the + key)")
        }
        for raw in tokens {
            let token = raw.count == 1 ? raw : raw.lowercased()
            if let bit = modifierNames[token.lowercased()] {
                parsed.mods |= bit
                continue
            }
            guard parsed.key == nil else {
                throw CuError.invalid("key combo '\(combo)' has more than one non-modifier key")
            }
            if token == "plus" {
                let (code, shift) = lookupChar("+") ?? (0x18, true)
                parsed.key = code
                if shift { parsed.mods |= UInt32(CU_MOD_SHIFT) }
            } else if let code = named[token] {
                parsed.key = code
            } else if token.count == 1, let ch = token.first {
                // Letters are case-insensitive in combos (OpenAI sends "A"); shifted symbols add shift.
                guard let (code, shift) = lookupChar(ch) else {
                    throw CuError.invalid("unknown key '\(raw)' in combo '\(combo)'")
                }
                parsed.key = code
                if shift, !ch.isLetter { parsed.mods |= UInt32(CU_MOD_SHIFT) }
            } else {
                throw CuError.invalid("unknown key '\(raw)' in combo '\(combo)'")
            }
        }
        return parsed
    }
}
