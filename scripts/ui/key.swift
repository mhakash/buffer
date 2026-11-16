// usage: key text "abc" | key code N   (N = macOS virtual key code: 36 return, 48 tab, 51 delete, 53 esc, 123–126 arrows)
import CoreGraphics
import Foundation
let a = CommandLine.arguments
func post(_ code: CGKeyCode, _ s: String?) {
    for down in [true, false] {
        let e = CGEvent(keyboardEventSource: nil, virtualKey: code, keyDown: down)!
        if let s = s { let u = Array(s.utf16); e.keyboardSetUnicodeString(stringLength: u.count, unicodeString: u) }
        e.post(tap: .cghidEventTap); usleep(30000)
    }
}
if a[1] == "text" { for ch in a[2] { post(0, String(ch)) } }
// Esc must carry its unicode string or macOS ignores it (see AGENTS.md).
else { let c = CGKeyCode(a[2])!; post(c, c == 53 ? "\u{1b}" : nil) }
