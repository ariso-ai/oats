import AppKit

/// Owns the one pill panel: applies state from Rust, places the panel, and
/// turns pointer movement, clicks and drags into hover and actions.
@MainActor
final class PillController {
    static var shared: PillController?

    let model = PillModel()
    private let panel = PillPanel()
    private let hosting: PillHostingView
    private let sendAction: (PillAction) -> Void
    private var drag: ClickDragTracker?

    init(send: @escaping (PillAction) -> Void) {
        sendAction = send
        hosting = PillHostingView(rootView: PillView(model: model, onBodyDrag: { _ in }, send: { _ in }))
        hosting.rootView = PillView(
            model: model,
            onBodyDrag: { [weak self] in self?.bodyDrag($0) },
            send: { [weak self] in self?.sendAction($0) }
        )
        hosting.onPointer = { [weak self] point in
            guard let self else { return }
            if let point { self.model.pointerMoved(to: point) } else { self.model.pointerExited() }
        }
        panel.contentView = hosting
        panel.setContentSize(PillGeometry.panelSize)
    }

    func update(phase: PillPhase, isPaused: Bool, durationSeconds: UInt32, bars: [Double]) {
        model.apply(phase: phase, isPaused: isPaused, durationSeconds: durationSeconds, bars: bars)
    }

    /// Showing always re-docks to the primary screen's right edge, like the
    /// webview pill did whenever the Meetings window stopped covering for it.
    func setVisible(_ visible: Bool) {
        if visible {
            guard !panel.isVisible else { return }
            if let screen = NSScreen.screens.first ?? NSScreen.main {
                panel.setFrameOrigin(PillGeometry.dockOrigin(in: screen.visibleFrame))
            }
            panel.orderFrontRegardless()
        } else {
            drag = nil
            model.pointerExited()
            panel.orderOut(nil)
        }
    }

    func destroy() {
        panel.orderOut(nil)
        panel.close()
    }

    private func bodyDrag(_ event: PillView.BodyDragEvent) {
        switch event {
        case .changed:
            let pointer = NSEvent.mouseLocation
            guard drag != nil else {
                drag = ClickDragTracker(mouseDown: pointer, windowOrigin: panel.frame.origin)
                return
            }
            if let origin = drag?.drag(to: pointer) {
                panel.setFrameOrigin(origin)
            }
        case .ended:
            let wasClick = drag?.isClick ?? false
            drag = nil
            if wasClick {
                sendAction(.openMeetings)
            } else if let visible = (panel.screen ?? NSScreen.screens.first)?.visibleFrame {
                // Keep room to expand: a pill parked under the menu bar would
                // otherwise grow its controls off-screen.
                panel.setFrameOrigin(PillGeometry.clampedOrigin(panel.frame.origin, within: visible))
            }
        }
    }
}
