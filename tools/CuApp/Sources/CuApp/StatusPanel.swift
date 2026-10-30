import AppKit

/// The sole controls surface: native transient popover anchored to the status
/// button. No detached/draggable window, global click monitor, or input posting.
@MainActor
final class StatusPanel: NSObject, NSPopoverDelegate {
    var onClose: (() -> Void)?
    let item: NSStatusItem
    let popover = NSPopover()

    init(content: NSViewController, size: NSSize) {
        item = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        super.init()
        popover.contentViewController = content
        popover.contentSize = size
        popover.behavior = .transient
        popover.delegate = self
        popover.animates = true
        item.button?.target = self
        item.button?.action = #selector(toggle)
        item.button?.setAccessibilityLabel("Cu task controls")
    }

    @objc func toggle() {
        if popover.isShown { close() } else { show() }
    }

    func show() {
        guard let button = item.button, button.window != nil else { return }
        NSApp.activate(ignoringOtherApps: true)
        popover.show(relativeTo: button.bounds, of: button, preferredEdge: .minY)
        popover.contentViewController?.view.window?.makeKey()
    }

    func popoverShouldDetach(_ popover: NSPopover) -> Bool { false }
    func popoverDidClose(_ notification: Notification) { onClose?() }
    func close() { popover.performClose(nil) }
    func remove() {
        close()
        NSStatusBar.system.removeStatusItem(item)
    }
}
