import Foundation

// The C ABI Rust drives the pill through (see src-tauri/src/recorder_pill/macos.rs).
// Every entry point except the version check must be called on the main thread.

/// Bumped whenever a signature below changes; Rust refuses a mismatched library.
@_cdecl("oats_pill_abi_version")
public func oatsPillAbiVersion() -> Int32 {
    1
}

public typealias OatsPillActionCallback = @convention(c) (Int32) -> Void

/// Creates the (hidden) pill. `onAction` receives `PillAction` raw values on
/// the main thread. A second call while a pill exists is a no-op.
@_cdecl("oats_pill_create")
public func oatsPillCreate(_ onAction: OatsPillActionCallback?) {
    guard let onAction else { return }
    MainActor.assumeIsolated {
        guard PillController.shared == nil else { return }
        PillController.shared = PillController { onAction($0.rawValue) }
    }
}

/// Applies one `recorder://state` broadcast. Unknown phase codes are ignored.
@_cdecl("oats_pill_update")
public func oatsPillUpdate(
    _ phase: Int32,
    _ isPaused: Bool,
    _ durationSeconds: UInt32,
    _ bars: UnsafePointer<Float>?,
    _ barCount: UInt32
) {
    guard let phase = PillPhase(rawValue: phase) else { return }
    let levels = bars.map { UnsafeBufferPointer(start: $0, count: Int(barCount)).map(Double.init) } ?? []
    MainActor.assumeIsolated {
        PillController.shared?.update(
            phase: phase,
            isPaused: isPaused,
            durationSeconds: durationSeconds,
            bars: levels
        )
    }
}

/// Shows (docked to the primary screen's right edge) or hides the pill.
@_cdecl("oats_pill_set_visible")
public func oatsPillSetVisible(_ visible: Bool) {
    MainActor.assumeIsolated {
        PillController.shared?.setVisible(visible)
    }
}

/// Tears the pill down; the next `oats_pill_create` builds a fresh one.
@_cdecl("oats_pill_destroy")
public func oatsPillDestroy() {
    MainActor.assumeIsolated {
        PillController.shared?.destroy()
        PillController.shared = nil
    }
}
