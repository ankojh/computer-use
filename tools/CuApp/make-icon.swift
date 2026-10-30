import AppKit

// Generate a native SF Symbol app icon without any downloaded assets.
let image = NSImage(size: NSSize(width: 1024, height: 1024))
image.lockFocus()
let rect = NSRect(x: 82, y: 82, width: 860, height: 860)
let shape = NSBezierPath(roundedRect: rect, xRadius: 190, yRadius: 190)
let gradient = NSGradient(starting: NSColor(calibratedRed: 0.10, green: 0.37, blue: 0.31, alpha: 1),
                          ending: NSColor(calibratedRed: 0.28, green: 0.65, blue: 0.56, alpha: 1))!
gradient.draw(in: shape, angle: 90)
let configuration = NSImage.SymbolConfiguration(pointSize: 440, weight: .medium)
    .applying(NSImage.SymbolConfiguration(paletteColors: [.white]))
let symbol = NSImage(systemSymbolName: "cursorarrow.rays", accessibilityDescription: nil)!
    .withSymbolConfiguration(configuration)!
symbol.draw(in: NSRect(x: 272, y: 272, width: 480, height: 480))
image.unlockFocus()
let bitmap = NSBitmapImageRep(data: image.tiffRepresentation!)!
try bitmap.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: CommandLine.arguments[1]))
