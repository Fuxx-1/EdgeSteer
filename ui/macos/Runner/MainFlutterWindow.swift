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

    // The interface draws its own window controls in the top bar, so the native
    // title bar must not paint a second set. A transparent title bar with
    // hidden buttons leaves the traffic lights' behaviour to the app while the
    // whole window becomes content, which is what puts them beside the logo
    // instead of in a separate strip above it.
    self.titlebarAppearsTransparent = true
    self.titleVisibility = .hidden
    self.styleMask.insert(.fullSizeContentView)
    for button in [NSWindow.ButtonType.closeButton,
                   .miniaturizeButton,
                   .zoomButton] {
      self.standardWindowButton(button)?.isHidden = true
    }
    self.isMovableByWindowBackground = true

    // The interface talks to the agent over its authenticated loopback control
    // protocol. No engine, system DNS, or configuration access crosses this
    // channel, so there is no method channel to register.
    RegisterGeneratedPlugins(registry: flutterViewController)

    // Window controls only. The interface draws the traffic lights itself, so
    // it needs a way to drive the real window behaviour behind them.
    let windowChannel = FlutterMethodChannel(
      name: "io.edgesteer/window",
      binaryMessenger: flutterViewController.engine.binaryMessenger)
    windowChannel.setMethodCallHandler { [weak self] call, result in
      guard let self else {
        result(FlutterError(code: "no_window", message: nil, details: nil))
        return
      }
      switch call.method {
      case "close":
        self.performClose(nil)
        result(nil)
      case "minimize":
        self.miniaturize(nil)
        result(nil)
      case "zoom":
        self.zoom(nil)
        result(nil)
      default:
        result(FlutterMethodNotImplemented)
      }
    }

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
    // Clamp the origin into the visible frame. A stale saved frame or a display
    // change can otherwise place the window partly or fully off-screen, where
    // the window server never shows it and the app looks like it failed to open.
    let origin = NSPoint(
      x: max(visible.minX, min(visible.midX - size.width / 2,
                            visible.maxX - size.width)),
      y: max(visible.minY, min(visible.midY - size.height / 2,
                            visible.maxY - size.height))
    )
    return NSRect(origin: origin, size: size)
  }
}
