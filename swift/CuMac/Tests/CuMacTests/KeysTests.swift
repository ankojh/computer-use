import CCu
import Testing
@testable import CuMac

@Suite struct KeysTests {
    @Test func modifiersAndNamedKeys() throws {
        let p = try Keys.parse("ctrl+opt+shift+cmd+return")
        #expect(p.mods == UInt32(CU_MOD_CTRL | CU_MOD_OPT | CU_MOD_SHIFT | CU_MOD_CMD))
        #expect(p.key == 0x24)
        #expect(try Keys.parse("forwarddelete").key == 0x75)
        #expect(try Keys.parse("f12").key == 0x6F)
        #expect(try Keys.parse("pagedown").key == 0x79)
        #expect(try Keys.parse("Return").key == 0x24) // named keys are case-insensitive
    }

    @Test func modifierOnly() throws {
        let p = try Keys.parse("shift")
        #expect(p.mods == UInt32(CU_MOD_SHIFT))
        #expect(p.key == nil)
    }

    @Test func charactersUseAnsiFallback() throws {
        // No layout map is built in tests (cu_init not called), so the US ANSI table applies.
        #expect(try Keys.parse("cmd+z").key == 0x06)
        #expect(try Keys.parse("cmd+Z").key == 0x06)
        #expect(try Keys.parse("cmd+.").key == 0x2F)
        #expect(try Keys.parse("ctrl+-").key == 0x1B)
    }

    @Test func shiftedSymbolsAddShift() throws {
        let p = try Keys.parse("cmd+?")
        #expect(p.key == 0x2C)
        #expect(p.mods == UInt32(CU_MOD_CMD | CU_MOD_SHIFT))
        let plus = try Keys.parse("cmd+plus")
        #expect(plus.key == 0x18)
        #expect(plus.mods & UInt32(CU_MOD_SHIFT) != 0)
    }

    @Test func errors() {
        #expect(throws: CuError.self) { try Keys.parse("") }
        #expect(throws: CuError.self) { try Keys.parse("cmd++") }
        #expect(throws: CuError.self) { try Keys.parse("hyper+x") }
        #expect(throws: CuError.self) { try Keys.parse("a+b") }
    }
}

@Suite struct GeometryTests {
    @Test func fitSizePreservesAspect() {
        #expect(fitSize(width: 3456, height: 2234, maxLongEdge: 1440) == (1440, 931))
        #expect(fitSize(width: 2234, height: 3456, maxLongEdge: 1440) == (931, 1440))
        #expect(fitSize(width: 800, height: 600, maxLongEdge: 1440) == (800, 600))
        #expect(fitSize(width: 800, height: 600, maxLongEdge: 0) == (800, 600))
    }
}
