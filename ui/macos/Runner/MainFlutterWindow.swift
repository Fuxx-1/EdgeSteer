import Cocoa
import FlutterMacOS

class MainFlutterWindow: NSWindow {
  override func awakeFromNib() {
    let flutterViewController = FlutterViewController()
    self.contentViewController = flutterViewController

    // Flutter restores the previous frame through NSWindow state restoration.
    // Disabling it keeps the configured size authoritative instead of letting a
    // stale saved frame shrink the window below its content.
    self.isRestorable = false
    self.minSize = NSSize(width: 900, height: 620)

    // The interface talks to the agent over its authenticated loopback control
    // protocol. No engine, system DNS, or configuration access crosses this
    // channel, so there is no method channel to register.
    RegisterGeneratedPlugins(registry: flutterViewController)

    super.awakeFromNib()

    // The Flutter view installs its own frame during the first layout pass, so
    // apply the configured size after that pass instead of before it.
    DispatchQueue.main.async { [weak self] in
      guard let self else { return }
      self.setFrame(self.windowFrame(), display: true)
    }
  }

  /// Opens at the configured size, clamped to the visible screen so the window
  /// never starts larger than the display it appears on.
  ///
  /// `NSScreen.main` follows the key window, which on a multi-display setup can
  /// be the smaller screen. Prefer the screen under the pointer, then the
  /// largest available display, so the window opens where the user is looking.
  private func windowFrame() -> NSRect {
    let preferred = NSSize(width: 1180, height: 820)
    let pointer = NSEvent.mouseLocation
    let pointerScreen = NSScreen.screens.first { NSMouseInRect(pointer, $0.frame, false) }
    let largestScreen = NSScreen.screens.max {
      $0.visibleFrame.width * $0.visibleFrame.height
        < $1.visibleFrame.width * $1.visibleFrame.height
    }
    guard let screen = pointerScreen ?? largestScreen ?? NSScreen.main else {
      return NSRect(origin: frame.origin, size: preferred)
    }
    let visible = screen.visibleFrame
    let size = NSSize(
      width: max(900, min(preferred.width, visible.width - 80)),
      height: max(620, min(preferred.height, visible.height - 80))
    )
    let origin = NSPoint(
      x: visible.midX - size.width / 2,
      y: visible.midY - size.height / 2
    )
    return NSRect(origin: origin, size: size)
  }
}
