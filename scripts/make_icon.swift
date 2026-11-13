// Renders the 1024px app icon PNG. Usage: swift scripts/make_icon.swift <out.png>
import AppKit

let out = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "icon_1024.png"
let size: CGFloat = 1024
// Apple's macOS grid: 824pt body centered in 1024 canvas.
let body = NSRect(x: 100, y: 100, width: 824, height: 824)

guard let rep = NSBitmapImageRep(
    bitmapDataPlanes: nil, pixelsWide: Int(size), pixelsHigh: Int(size),
    bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
    colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
) else { fatalError("bitmap") }

NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)

let shadow = NSShadow()
shadow.shadowColor = NSColor.black.withAlphaComponent(0.3)
shadow.shadowOffset = NSSize(width: 0, height: -12)
shadow.shadowBlurRadius = 24
shadow.set()

let squircle = NSBezierPath(roundedRect: body, xRadius: 185, yRadius: 185)
NSGradient(colors: [
    NSColor(srgbRed: 0.31, green: 0.55, blue: 1.0, alpha: 1),
    NSColor(srgbRed: 0.45, green: 0.30, blue: 0.95, alpha: 1),
])!.draw(in: squircle, angle: -60)
NSShadow().set()

// 2x2 tile grid ("workspace of tools"); top-left tile is solid, the rest translucent.
let tile: CGFloat = 220
let gap: CGFloat = 64
let origin = (size - (tile * 2 + gap)) / 2
for row in 0..<2 {
    for col in 0..<2 {
        let rect = NSRect(
            x: origin + CGFloat(col) * (tile + gap),
            y: origin + CGFloat(1 - row) * (tile + gap),
            width: tile, height: tile)
        let solid = row == 0 && col == 0
        NSColor.white.withAlphaComponent(solid ? 1.0 : 0.55).setFill()
        NSBezierPath(roundedRect: rect, xRadius: 56, yRadius: 56).fill()
    }
}

NSGraphicsContext.restoreGraphicsState()
try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: out))
print("wrote \(out)")
