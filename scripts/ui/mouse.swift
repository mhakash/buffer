// usage: mouse move X Y | mouse click X Y | mouse rclick X Y | mouse drag X Y X2 Y2   (points, global coords)
import CoreGraphics
import Foundation
let a = CommandLine.arguments
let p = CGPoint(x: Double(a[2])!, y: Double(a[3])!)
func post(_ t: CGEventType, _ at: CGPoint = p, _ b: CGMouseButton = .left) { CGEvent(mouseEventSource: nil, mouseType: t, mouseCursorPosition: at, mouseButton: b)?.post(tap: .cghidEventTap) }
post(.mouseMoved)
if a[1] == "click" { usleep(80000); post(.leftMouseDown); usleep(60000); post(.leftMouseUp) }
if a[1] == "rclick" { usleep(80000); post(.rightMouseDown, p, .right); usleep(60000); post(.rightMouseUp, p, .right) }
if a[1] == "drag" {
    let end = CGPoint(x: Double(a[4])!, y: Double(a[5])!)
    usleep(80000); post(.leftMouseDown); usleep(80000)
    for i in 1...20 {
        let t = Double(i) / 20
        post(.leftMouseDragged, CGPoint(x: p.x + (end.x - p.x) * t, y: p.y + (end.y - p.y) * t))
        usleep(16000)
    }
    usleep(60000); post(.leftMouseUp, end)
}
