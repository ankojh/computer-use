import XCTest
@testable import CuApp

final class NativeAppDiscoveryTests: XCTestCase {
    func testTaskRelevantMetadataOnlyAndNoAppExecution() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at:root,withIntermediateDirectories:true)
        defer { try? FileManager.default.removeItem(at:root) }
        func bundle(_ name: String, _ id: String) throws {
            let contents = root.appendingPathComponent(name + ".app/Contents")
            try FileManager.default.createDirectory(at:contents,withIntermediateDirectories:true)
            let plist: [String:Any] = ["CFBundleIdentifier":id,"CFBundleName":"PRIVATE_UNUSED_NAME","PrivateCanary":"PRIVATE_ACCOUNT_CANARY"]
            let data = try PropertyListSerialization.data(fromPropertyList:plist,format:.xml,options:0)
            try data.write(to:contents.appendingPathComponent("Info.plist"))
        }
        try bundle("\u{200e}WhatsApp","net.whatsapp.WhatsApp")
        try bundle("OtherPrivateApp","private.unrelated")
        try bundle("Broken","bad\nidentifier")
        // No executable exists: discovery reads Info.plist, never loads a bundle.
        let text = NativeAppDiscovery.context(task:"Send PRIVATE_TASK_CANARY via whatspapp",roots:[root])
        let result = try JSONDecoder().decode([NativeAppRecord].self,from:Data(text.utf8))
        XCTAssertEqual(result,[NativeAppRecord(name:"WhatsApp",bundle_id:"net.whatsapp.WhatsApp")])
        for secret in ["PRIVATE_TASK_CANARY","PRIVATE_ACCOUNT_CANARY","PRIVATE_UNUSED_NAME","OtherPrivateApp",root.path] { XCTAssertFalse(text.contains(secret)) }
        XCTAssertEqual(NativeAppDiscovery.context(task:"Unrelated task",roots:[root]),"[]")
    }
    func testWholeWordMatchingAndBrowserAliases() {
        let mail = NativeAppRecord(name:"Mail",bundle_id:"com.apple.mail")
        XCTAssertFalse(NativeAppDiscovery.matches(mail,task:"Send an email"))
        XCTAssertTrue(NativeAppDiscovery.matches(mail,task:"Use Mail to inspect a draft"))
        XCTAssertTrue(NativeAppDiscovery.matches(NativeAppRecord(name:"Google Chrome",bundle_id:"com.google.Chrome"),task:"Use Chrome profile settings"))
        XCTAssertTrue(NativeAppDiscovery.matches(NativeAppRecord(name:"WhatsApp",bundle_id:"net.whatsapp.WhatsApp"),task:"Check Whats App"))
    }
    func testSymlinkBundlesAreNotTraversed() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at:root,withIntermediateDirectories:true)
        defer {try? FileManager.default.removeItem(at:root)}
        try FileManager.default.createSymbolicLink(at:root.appendingPathComponent("Alias.app"),withDestinationURL:URL(fileURLWithPath:"/Applications"))
        let outside = root.appendingPathComponent("Outside")
        try FileManager.default.createDirectory(at:outside,withIntermediateDirectories:true)
        let data = try PropertyListSerialization.data(fromPropertyList:["CFBundleIdentifier":"test.external"],format:.xml,options:0)
        try data.write(to:outside.appendingPathComponent("Info.plist"))
        let contentsAlias = root.appendingPathComponent("ContentsAlias.app")
        try FileManager.default.createDirectory(at:contentsAlias,withIntermediateDirectories:true)
        try FileManager.default.createSymbolicLink(at:contentsAlias.appendingPathComponent("Contents"),withDestinationURL:outside)
        let plistAlias = root.appendingPathComponent("PlistAlias.app/Contents")
        try FileManager.default.createDirectory(at:plistAlias,withIntermediateDirectories:true)
        try FileManager.default.createSymbolicLink(at:plistAlias.appendingPathComponent("Info.plist"),withDestinationURL:outside.appendingPathComponent("Info.plist"))
        XCTAssertEqual(NativeAppDiscovery.records(in:[root]),[])
    }
}
