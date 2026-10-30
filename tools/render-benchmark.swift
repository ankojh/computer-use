#!/usr/bin/swift
// Synthetic screens only. Never captures a display or posts input.
import AppKit

let directory = CommandLine.arguments.dropFirst().first ?? "target/benchmark-fixtures"
try FileManager.default.createDirectory(atPath: directory, withIntermediateDirectories: true)
for (name, shifted, personal) in [("compose", false, false), ("shifted", true, false), ("wrong-profile", false, true)] {
    let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: 1152, pixelsHigh: 720,
        bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
        colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0)!
    let graphics = NSGraphicsContext(bitmapImageRep: bitmap)!
    graphics.cgContext.translateBy(x: 0, y: 720)
    graphics.cgContext.scaleBy(x: 1, y: -1)
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(cgContext: graphics.cgContext, flipped: true)
    func box(_ rect: NSRect, _ color: NSColor) { color.setFill(); NSBezierPath(rect: rect).fill() }
    func text(_ value: String, _ point: NSPoint, _ size: CGFloat = 20, _ color: NSColor = .black) {
        (value as NSString).draw(at: point, withAttributes: [.font: NSFont.systemFont(ofSize: size), .foregroundColor: color])
    }
    box(NSRect(x: 0, y: 0, width: 1152, height: 720), .white)
    box(NSRect(x: 0, y: 0, width: 1152, height: 72), NSColor(white: 0.90, alpha: 1))
    text("SYNTHETIC BENCHMARK — no real browser or account", NSPoint(x: 24, y: 12), 18)
    text("Chrome profile: \(personal ? "Personal" : "CMU")", NSPoint(x: 24, y: 42), 18)
    text("mail.example.test — Demo Mail", NSPoint(x: 24, y: 90), 26)
    text("Signed in: \(personal ? "personal@example.test" : "demo@cmu.edu")", NSPoint(x: 620, y: 94), 20)
    let button = NSRect(x: shifted ? 800 : 32, y: shifted ? 540 : 150, width: 180, height: 64)
    box(button, NSColor(red: 0.78, green: 0.87, blue: 1, alpha: 1))
    text("Compose", NSPoint(x: button.minX + 30, y: button.minY + 18), 24)
    text("Inbox", NSPoint(x: 32, y: 270), 22)
    text("No messages. This is a generated UI fixture.", NSPoint(x: 280, y: 300), 23, .darkGray)
    NSGraphicsContext.restoreGraphicsState()
    try bitmap.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: directory).appendingPathComponent("\(name).png"))
}
print("Generated 1152×720 fixtures in \(directory)")
