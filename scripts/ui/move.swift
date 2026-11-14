// move PID X Y : move that app's first window, nothing else.
import ApplicationServices
let a = CommandLine.arguments
let app = AXUIElementCreateApplication(pid_t(a[1])!)
var wins: CFTypeRef?
AXUIElementCopyAttributeValue(app, kAXWindowsAttribute as CFString, &wins)
guard let w = (wins as? [AXUIElement])?.first else { print("no window"); exit(1) }
var p = CGPoint(x: Double(a[2])!, y: Double(a[3])!)
print(AXUIElementSetAttributeValue(w, kAXPositionAttribute as CFString, AXValueCreate(.cgPoint, &p)!) == .success)
