import AppKit

/// Owns the one pill panel: applies state from Rust, keeps the panel frame in
/// step with the capsule's height, and turns clicks and drags into actions.
@MainActor
final class PillController {
    static var shared: PillController?

    let model = PillModel()
    private let panel = PillPanel()
    private let hosting: PillHostingView
    private let sendAction: (PillAction) -> Void
    private var drag: ClickDragTracker?
    /// The height the panel is at or animating toward.
    private var targetHeight: CGFloat = 0

    init(send: @escaping (PillAction) -> Void) {
        sendAction = send
        hosting = PillHostingView(rootView: PillView(model: model, onBodyDrag: { _ in }, send: { _ in }))
        hosting.rootView = PillView(
            model: model,
            onBodyDrag: { [weak self] in self?.bodyDrag($0) },
            send: { [weak self] in self?.sendAction($0) }
        )
        hosting.onHover = { [weak self] in self?.hover($0) }
        panel.contentView = hosting
        syncFrame(animated: false)
    }

    func update(phase: PillPhase, isPaused: Bool, durationSeconds: UInt32, bars: [Double]) {
        model.apply(phase: phase, isPaused: isPaused, durationSeconds: durationSeconds, bars: bars)
        syncFrame(animated: false)
    }

    /// Showing always re-docks to the primary screen's right edge, like the
    /// webview pill did whenever the Meetings window stopped covering for it.
    func setVisible(_ visible: Bool) {
        if visible {
            guard !panel.isVisible else { return }
            let screen = NSScreen.screens.first ?? NSScreen.main
            if let screen {
                panel.setFrame(PillGeometry.dockFrame(in: screen.visibleFrame, height: model.height), display: false)
            }
            panel.orderFrontRegardless()
            panel.invalidateShadow()
        } else {
            drag = nil
            model.setHovering(false)
            panel.orderOut(nil)
            syncFrame(animated: false)
        }
    }

    func destroy() {
        panel.orderOut(nil)
        panel.close()
    }

    private func hover(_ hovering: Bool) {
        model.setHovering(hovering)
        syncFrame(animated: true)
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
            if wasClick { sendAction(.openMeetings) }
        }
    }

    /// The panel hugs the capsule, so transparent space never blocks clicks
    /// meant for the app underneath. Compared against the last target rather
    /// than the live frame: state updates arrive many times a second and must
    /// not cut a running hover animation short.
    private func syncFrame(animated: Bool) {
        let height = model.height
        guard height != targetHeight else { return }
        targetHeight = height
        let visible = (panel.screen ?? NSScreen.screens.first)?.visibleFrame ?? .infinite
        let target = PillGeometry.resized(panel.frame, toHeight: height, within: visible)
        guard animated, panel.isVisible else {
            panel.setFrame(target, display: true)
            panel.invalidateShadow()
            return
        }
        NSAnimationContext.runAnimationGroup { context in
            context.duration = PillGeometry.animation
            context.timingFunction = CAMediaTimingFunction(name: .easeOut)
            panel.animator().setFrame(target, display: true)
        } completionHandler: { [weak self] in
            MainActor.assumeIsolated { self?.panel.invalidateShadow() }
        }
    }
}
