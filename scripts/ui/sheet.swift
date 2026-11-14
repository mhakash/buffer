// sheet OUT.png cols img1 img2 ... : grid contact sheet, each cell labelled with its filename.
import AppKit
let a = CommandLine.arguments
let cols = Int(a[2])!
let imgs = a[3...].map { NSImage(contentsOfFile: $0)! }
let names = a[3...].map { URL(fileURLWithPath: $0).deletingPathExtension().lastPathComponent }
let w = 450.0, h = w * imgs[0].size.height / imgs[0].size.width
let rows = (imgs.count + cols - 1) / cols
let out = NSImage(size: NSSize(width: w * Double(cols), height: (h + 18) * Double(rows)))
out.lockFocus()
NSColor.white.setFill(); NSRect(origin: .zero, size: out.size).fill()
for (i, img) in imgs.enumerated() {
    let x = Double(i % cols) * w, y = out.size.height - Double(i / cols + 1) * (h + 18)
    img.draw(in: NSRect(x: x, y: y, width: w, height: h))
    (names[i] as NSString).draw(at: NSPoint(x: x + 4, y: y + h + 2), withAttributes: [.font: NSFont.systemFont(ofSize: 12)])
}
out.unlockFocus()
let rep = NSBitmapImageRep(data: out.tiffRepresentation!)!
try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: a[1]))
