import Cocoa
import FlutterMacOS

/// The Flutter interface is a window process owned by the EdgeSteer menu-bar
/// agent. Closing the window must end only this process: the agent and the Rust
/// DNS engine keep running, so DNS service never depends on the window.
@main
class AppDelegate: FlutterAppDelegate {
  override func applicationShouldTerminateAfterLastWindowClosed(_ sender: NSApplication) -> Bool {
    return true
  }

  override func applicationSupportsSecureRestorableState(_ app: NSApplication) -> Bool {
    return true
  }

  override func applicationShouldHandleReopen(
    _ sender: NSApplication,
    hasVisibleWindows flag: Bool
  ) -> Bool {
    // A second launch from the menu bar focuses the existing window instead of
    // starting a second interface process.
    if let window = sender.windows.first {
      window.makeKeyAndOrderFront(nil)
    }
    sender.activate(ignoringOtherApps: true)
    return true
  }
}
