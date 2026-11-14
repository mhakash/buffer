// usage: mouse move X Y | mouse click X Y   (points, global coords)
import CoreGraphics
import Foundation
let a = CommandLine.arguments
let p = CGPoint(x: Double(a[2])!, y: Double(a[3])!)
func post(_ t: CGEventType) { CGEvent(mouseEventSource: nil, mouseType: t, mouseCursorPosition: p, mouseButton: .left)?.post(tap: .cghidEventTap) }
post(.mouseMoved)
if a[1] == "click" { usleep(80000); post(.leftMouseDown); usleep(60000); post(.leftMouseUp) }
