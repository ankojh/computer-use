import Foundation

/// NDJSON may split in the middle of a line or a multi-byte UTF-8 character.
struct EngineEventStream {
    enum StreamError: Error { case lineTooLong }
    private var buffer = Data()
    private let limit = 1024 * 1024

    mutating func append(_ data: Data) throws -> [EngineEvent] {
        buffer.append(data)
        var events: [EngineEvent] = []
        while let newline = buffer.firstIndex(of: 0x0a) {
            let line = Data(buffer[..<newline])
            guard line.count <= limit else { throw StreamError.lineTooLong }
            buffer.removeSubrange(...newline)
            if let event = try? JSONDecoder().decode(EngineEvent.self, from: line) { events.append(event) }
        }
        guard buffer.count <= limit else { throw StreamError.lineTooLong }
        return events
    }
}
