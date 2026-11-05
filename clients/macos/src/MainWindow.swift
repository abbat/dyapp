import AppKit
import SwiftUI

/// Where the window goes on the displays connected now; the first one is the primary display.
enum WindowPlacement {
    private static let titleBarHeight: CGFloat = 28

    static func restored(_ frame: CGRect, on screens: [CGRect]) -> CGRect {
        let titleBar = CGRect(x: frame.minX, y: frame.maxY - titleBarHeight, width: frame.width, height: titleBarHeight)
        let visible = screens.map { area($0.intersection(frame)) }.reduce(0, +)
        if visible * 4 >= area(frame), screens.contains(where: { $0.intersects(titleBar) }) { return frame }
        return centred(frame.size, on: screens)
    }

    static func centred(_ size: CGSize, on screens: [CGRect]) -> CGRect {
        guard let primary = screens.first else { return CGRect(origin: .zero, size: size) }
        let width = min(size.width, primary.width)
        let height = min(size.height, primary.height)
        return CGRect(x: primary.midX - width / 2, y: primary.midY - height / 2, width: width, height: height)
    }

    private static func area(_ rect: CGRect) -> CGFloat {
        rect.width * rect.height
    }
}

/// AppKit asks a delegate about selectors off the main actor, and only `NSObject` calls touch this one.
private struct ForwardedDelegate: @unchecked Sendable {
    let object: NSObjectProtocol?
}

/// Answers the close button and Cmd+W itself and leaves everything else to SwiftUI's delegate,
/// because SwiftUI drops a closed window together with the state of its views.
@MainActor
private final class WindowCloseProxy: NSObject, NSWindowDelegate {
    private let inner: ForwardedDelegate
    private let close: (NSWindow) -> Void

    init(inner: NSWindowDelegate?, close: @escaping (NSWindow) -> Void) {
        self.inner = ForwardedDelegate(object: inner)
        self.close = close
    }

    func windowShouldClose(_ sender: NSWindow) -> Bool {
        close(sender)
        return false
    }

    override nonisolated func responds(to selector: Selector?) -> Bool {
        super.responds(to: selector) || inner.object?.responds(to: selector) == true
    }

    override nonisolated func forwardingTarget(for selector: Selector?) -> Any? {
        inner.object?.responds(to: selector) == true ? inner.object : nil
    }
}

/// The one main window, its frame on relaunch, and the menu-bar icon that can bring it back.
@MainActor
final class MainWindowController: NSObject, NSApplicationDelegate {
    private static let defaultSize = CGSize(width: 1_024, height: 720)
    private var window: NSWindow?
    private var closeProxy: WindowCloseProxy?
    private var statusItem: NSStatusItem?

    /// UI tests start from the default frame: each slot root saves under its own name.
    private var frameName: String {
        #if DEBUG
        if let root = ProcessInfo.processInfo.arguments.first(where: { $0.hasPrefix("--ui-test-slot-root=") }) {
            return "main-" + root.dropFirst("--ui-test-slot-root=".count)
        }
        #endif
        return "main"
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        UserDefaults.standard.register(defaults: [MenuBarSection.showKey: true])
        applyIconSetting()
        MailNotice.showWindow = { [weak self] in self?.showWindow() }
        _ = NotificationCenter.default.addObserver(
            forName: UserDefaults.didChangeNotification, object: nil, queue: .main
        ) { [weak self] _ in
            MainActor.assumeIsolated { self?.applyIconSetting() }
        }
    }

    func applicationShouldHandleReopen(_ sender: NSApplication, hasVisibleWindows flag: Bool) -> Bool {
        showWindow()
        return false
    }

    func adopt(_ window: NSWindow) {
        guard self.window == nil else { return }
        self.window = window
        let proxy = WindowCloseProxy(inner: window.delegate) { [weak self] in self?.hide($0) }
        window.delegate = proxy
        closeProxy = proxy
        // SwiftUI places a new window after it joins the view: a saved x came back centred at its saved size.
        Task { [weak self] in self?.place(window) }
    }

    func applicationWillTerminate(_ notification: Notification) {
        // The autosave name alone left nothing to restore on a relaunch after Cmd+Q.
        window?.saveFrame(usingName: frameName)
    }

    private func place(_ window: NSWindow) {
        let screens = NSScreen.screens.map(\.visibleFrame)
        let frame = window.setFrameUsingName(frameName)
            ? WindowPlacement.restored(window.frame, on: screens)
            : WindowPlacement.centred(Self.defaultSize, on: screens)
        window.setFrame(frame, display: false)
        window.setFrameAutosaveName(frameName)
    }

    @objc
    func showWindow() {
        NSApp.setActivationPolicy(.regular)
        NSApp.activate(ignoringOtherApps: true)
        if window?.isMiniaturized == true { window?.deminiaturize(nil) }
        window?.makeKeyAndOrderFront(nil)
    }

    @objc
    func quit() {
        NSApp.terminate(nil)
    }

    private func hide(_ window: NSWindow) {
        // shortcut: full screen is left without waiting for its animation, unchecked on a real
        // display; wait for windowDidExitFullScreen if the empty Space stays behind.
        if window.styleMask.contains(.fullScreen) { window.toggleFullScreen(nil) }
        window.orderOut(nil)
        // The Dock icon goes only when the menu-bar icon is there to bring the window back.
        if UserDefaults.standard.bool(forKey: MenuBarSection.closeKey), statusItem != nil {
            NSApp.setActivationPolicy(.accessory)
        }
    }

    private func applyIconSetting() {
        let show = UserDefaults.standard.bool(forKey: MenuBarSection.showKey)
        if show, statusItem == nil {
            statusItem = makeStatusItem()
        } else if !show, let item = statusItem {
            NSStatusBar.system.removeStatusItem(item)
            statusItem = nil
        }
    }

    private func makeStatusItem() -> NSStatusItem {
        let item = NSStatusBar.system.statusItem(withLength: NSStatusItem.squareLength)
        item.button?.image = NSImage(systemSymbolName: "heart.fill", accessibilityDescription: "DYApp")
        item.button?.setAccessibilityIdentifier("menubar-icon")
        let menu = NSMenu()
        menu.addItem(menuItem("tray.open", id: "menubar-open", action: #selector(showWindow)))
        menu.addItem(menuItem("tray.quit", id: "menubar-quit", action: #selector(quit)))
        item.menu = menu
        return item
    }

    private func menuItem(_ key: String, id: String, action: Selector) -> NSMenuItem {
        let item = NSMenuItem(title: NSLocalizedString(key, comment: ""), action: action, keyEquivalent: "")
        item.target = self
        item.setAccessibilityIdentifier(id)
        return item
    }
}

/// Hands over the hosting window as soon as the view joins it.
struct WindowReader: NSViewRepresentable {
    final class Probe: NSView {
        var found: ((NSWindow) -> Void)?

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            if let window { found?(window) }
        }
    }

    let found: (NSWindow) -> Void

    func makeNSView(context: Context) -> Probe {
        let probe = Probe()
        probe.found = found
        return probe
    }

    func updateNSView(_ view: Probe, context: Context) {}
}

/// Settings for the menu-bar icon; `MainWindowController` reads the same defaults.
struct MenuBarSection: View {
    static let showKey = "menuBarIcon"
    static let closeKey = "closeToMenuBar"
    @AppStorage(Self.showKey) private var show = true
    @AppStorage(Self.closeKey) private var close = false

    var body: some View {
        Section {
            Toggle("tray.show", isOn: Binding(
                get: { show },
                set: {
                    show = $0
                    if !$0 { close = false }
                }
            ))
            .accessibilityIdentifier("tray-show")
            Toggle("tray.close", isOn: $close)
                .disabled(!show)
                .accessibilityIdentifier("tray-close")
        } header: {
            Text("tray.title")
        }
    }
}
