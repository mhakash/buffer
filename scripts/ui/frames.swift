// frames IN.mov OUTDIR t1 t2 ... : write PNG frames at the given seconds.
import AVFoundation
import AppKit
let a = CommandLine.arguments
let gen = AVAssetImageGenerator(asset: AVURLAsset(url: URL(fileURLWithPath: a[1])))
gen.requestedTimeToleranceBefore = .zero; gen.requestedTimeToleranceAfter = .zero
gen.maximumSize = CGSize(width: 900, height: 900)
for t in a[3...] {
    let time = CMTime(seconds: Double(t)!, preferredTimescale: 600)
    guard let img = try? gen.copyCGImage(at: time, actualTime: nil) else { print("fail \(t)"); continue }
    let rep = NSBitmapImageRep(cgImage: img)
    try! rep.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: "\(a[2])/t\(t).png"))
}
print("ok")
