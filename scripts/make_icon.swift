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

// Three list lines; the first is "checked".
let lineColor = NSColor.white.withAlphaComponent(0.9)
for (ix, y) in [640.0, 500.0, 360.0].enumerated() {
    let circle = NSBezierPath(ovalIn: NSRect(x: 250, y: y - 40, width: 80, height: 80))
    circle.lineWidth = 22
    lineColor.setStroke()
    if ix == 0 {
        NSColor.white.setFill()
        // Match the outer edge of the stroked circles (stroke adds lineWidth/2 outside).
        NSBezierPath(ovalIn: NSRect(x: 239, y: y - 51, width: 102, height: 102)).fill()
        let check = NSBezierPath()
        check.move(to: NSPoint(x: 268, y: y))
        check.line(to: NSPoint(x: 284, y: y - 18))
        check.line(to: NSPoint(x: 314, y: y + 20))
        check.lineWidth = 16
        check.lineCapStyle = .round
        check.lineJoinStyle = .round
        NSColor(srgbRed: 0.38, green: 0.42, blue: 0.98, alpha: 1).setStroke()
        check.stroke()
    } else {
        circle.stroke()
    }
    let bar = NSBezierPath(
        roundedRect: NSRect(x: 380, y: y - 22, width: ix == 0 ? 300 : 390, height: 44),
        xRadius: 22, yRadius: 22)
    (ix == 0 ? NSColor.white.withAlphaComponent(0.55) : lineColor).setFill()
    bar.fill()
}

NSGraphicsContext.restoreGraphicsState()
try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: out))
print("wrote \(out)")
