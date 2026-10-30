import Foundation

/// Read-only bundle metadata, never app activation, scripting, account inspection,
/// Spotlight queries, executable loading, or access to app containers/documents.
struct NativeAppRecord: Codable, Equatable {
    let name: String
    let bundle_id: String
}
struct NativeAppDiscovery {
    static func matches(_ app: NativeAppRecord, task: String) -> Bool {
        func words(_ text: String) -> [String] {
            text.folding(options: [.caseInsensitive, .diacriticInsensitive], locale: Locale(identifier: "en_US_POSIX"))
                .split(whereSeparator: { !$0.isLetter && !$0.isNumber }).map(String.init)
        }
        let tokens = words(task)
        let name = words(app.name)
        guard !name.isEmpty else { return false }
        if tokens.joined(separator: " ").contains(name.joined(separator: " ")) && name.joined().count >= 4 {
            // Match words/phrases, not e.g. Mail inside email.
            let padded = " " + tokens.joined(separator: " ") + " "
            if padded.contains(" " + name.joined(separator: " ") + " ") { return true }
        }
        if app.bundle_id == "net.whatsapp.WhatsApp" {
            return tokens.contains("whatsapp") || tokens.contains("whatspapp") || tokens.joined(separator:" ").contains("whats app")
        }
        if app.bundle_id == "com.google.Chrome" { return tokens.contains("chrome") }
        return false
    }
    static func records(in roots: [URL]) -> [NativeAppRecord] {
        let fm = FileManager.default
        var records: [NativeAppRecord] = []
        var seen = Set<String>()
        for root in roots.prefix(4) {
            let entries = (try? fm.contentsOfDirectory(at: root, includingPropertiesForKeys: [.isDirectoryKey, .isSymbolicLinkKey], options: [.skipsHiddenFiles])) ?? []
            for entry in entries.sorted(by: { $0.lastPathComponent < $1.lastPathComponent }).prefix(512) where entry.pathExtension.lowercased() == "app" {
                guard let info = try? entry.resourceValues(forKeys: [.isDirectoryKey,.isSymbolicLinkKey]), info.isDirectory == true, info.isSymbolicLink != true else { continue }
                let contents = entry.appendingPathComponent("Contents")
                guard let directory = try? contents.resourceValues(forKeys:[.isDirectoryKey,.isSymbolicLinkKey]), directory.isDirectory == true, directory.isSymbolicLink != true else { continue }
                let plist = contents.appendingPathComponent("Info.plist")
                guard let attrs = try? plist.resourceValues(forKeys:[.isRegularFileKey,.isSymbolicLinkKey,.fileSizeKey]), attrs.isRegularFile == true, attrs.isSymbolicLink != true, (attrs.fileSize ?? Int.max) <= 65536,
                      let data = try? Data(contentsOf:plist), data.count <= 65536,
                      let object = try? PropertyListSerialization.propertyList(from:data,format:nil) as? [String:Any],
                      let id = object["CFBundleIdentifier"] as? String, !id.isEmpty, id.utf8.count <= 200,
                      id.allSatisfy({$0.isASCII && ($0.isLetter || $0.isNumber || ".-_".contains($0))}), seen.insert(id).inserted else { continue }
                let name = entry.deletingPathExtension().lastPathComponent
                    .unicodeScalars.filter { !CharacterSet.controlCharacters.contains($0) && $0.properties.generalCategory != .format }.map(String.init).joined()
                guard !name.isEmpty, name.utf8.count <= 200 else { continue }
                records.append(NativeAppRecord(name:name,bundle_id:id))
                if records.count >= 256 { return records }
            }
        }
        return records
    }
    static func context(task: String, roots: [URL]? = nil) -> String {
        let home = FileManager.default.homeDirectoryForCurrentUser
        let paths = roots ?? [URL(fileURLWithPath:"/Applications"), home.appendingPathComponent("Applications"), URL(fileURLWithPath:"/System/Applications"), URL(fileURLWithPath:"/System/Applications/Utilities")]
        let matches = records(in:paths).filter { Self.matches($0,task:task) }.prefix(8)
        guard let data = try? JSONEncoder().encode(Array(matches)), data.count <= 4096 else { return "[]" }
        return String(decoding:data,as:UTF8.self)
    }
}
