import AppKit
import SwiftUI

/// A floating, non-activating panel: it never becomes key or pulls oats to the
/// front, so clicking the pill leaves focus in whatever app the user is in.
final class PillPanel: NSPanel {
    init() {
        super.init(
            contentRect: NSRect(x: 0, y: 0, width: PillGeometry.width, height: 1),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        isFloatingPanel = true
        level = .floating
        // Follow the user across Spaces and over fullscreen meeting apps.
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary, .ignoresCycle]
        isOpaque = false
        backgroundColor = .clear
        // The shadow follows the capsule's alpha; re-derived after each resize.
        hasShadow = true
        hidesOnDeactivate = false
        becomesKeyOnlyIfNeeded = true
        isReleasedWhenClosed = false
        animationBehavior = .none
        // oats is usually not the active app while recording; keep tooltips.
        allowsToolTipsWhenApplicationIsInactive = true
    }

    override var canBecomeKey: Bool { false }
    override var canBecomeMain: Bool { false }
}

/// Hosts the SwiftUI pill. Takes the first click (the panel is never key) and
/// reports hover with an always-active tracking area, since oats is rarely
/// the frontmost app while the pill is up.
final class PillHostingView: NSHostingView<PillView> {
    var onHover: ((Bool) -> Void)?
    private var hoverArea: NSTrackingArea?

    required init(rootView: PillView) {
        super.init(rootView: rootView)
        // The controller owns the window size; don't let SwiftUI constrain it.
        sizingOptions = []
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) is not supported")
    }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let hoverArea { removeTrackingArea(hoverArea) }
        let area = NSTrackingArea(
            rect: .zero,
            options: [.mouseEnteredAndExited, .activeAlways, .inVisibleRect],
            owner: self,
            userInfo: nil
        )
        addTrackingArea(area)
        hoverArea = area
    }

    override func mouseEntered(with event: NSEvent) {
        super.mouseEntered(with: event)
        if event.trackingArea === hoverArea { onHover?(true) }
    }

    override func mouseExited(with event: NSEvent) {
        super.mouseExited(with: event)
        if event.trackingArea === hoverArea { onHover?(false) }
    }
}
