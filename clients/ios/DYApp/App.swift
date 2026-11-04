import SwiftUI

/// Main app entry point for DYApp iOS
@main
struct DYAppApp: App {
  init() {
    NetworkType.startReporting()
  }

  var body: some Scene {
    WindowGroup {
      LockGate()
    }
  }
}
