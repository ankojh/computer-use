import Testing
@testable import CuMac

@Suite struct CaptureFilterTests {
    @Test func onlyCuFeedbackIsExcluded() {
        #expect(isCuFeedbackApplication("dev.ankojh.cu"))
        #expect(!isCuFeedbackApplication("com.google.Chrome"))
        #expect(!isCuFeedbackApplication("dev.ankojh.cu.lookalike"))
        #expect(!isCuFeedbackApplication("dev.ankojh.cu.engine"))
        #expect(!isCuFeedbackApplication(""))
    }
}
