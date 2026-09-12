import AppKit
import Combine

/// The recorder's lifecycle as broadcast in `recorder://state`. Raw values are
/// the C ABI codes (see `Bridge.swift` and `recorder_pill::macos`).
enum PillPhase: Int32 {
    case starting = 0
    case recording = 1
    case uploading = 2
    case success = 3
    case failed = 4

    /// Hovering reveals the timer and controls only while capture is live.
    var canExpand: Bool { self == .starting || self == .recording }
}

/// What a click on the pill asks the app to do. Raw values are the C ABI codes.
enum PillAction: Int32 {
    case openMeetings = 0
    case pause = 1
    case resume = 2
    case stop = 3
    case retryUpload = 4
    case continueRecording = 5
    case discardRecording = 6
}

/// Everything the pill renders, fed from Rust through `oats_pill_update`.
@MainActor
final class PillModel: ObservableObject {
    @Published private(set) var phase: PillPhase = .starting
    @Published private(set) var isPaused = false
    @Published private(set) var durationSeconds: UInt32 = 0
    /// Exactly three levels in 0…1, left to right.
    @Published private(set) var bars: [Double] = [0, 0, 0]
    @Published private(set) var isHovering = false

    var isExpanded: Bool { isHovering && phase.canExpand }

    var height: CGFloat { PillGeometry.height(for: phase, expanded: isExpanded) }

    var formattedDuration: String {
        String(format: "%02d:%02d", durationSeconds / 60, durationSeconds % 60)
    }

    var pauseResumeLabel: String { isPaused ? "Resume recording" : "Pause recording" }
    var pauseResumeAction: PillAction { isPaused ? .resume : .pause }

    func apply(phase: PillPhase, isPaused: Bool, durationSeconds: UInt32, bars: [Double]) {
        self.phase = phase
        self.isPaused = isPaused
        self.durationSeconds = durationSeconds
        let clamped = bars.prefix(3).map { $0.isFinite ? min(1, max(0, $0)) : 0 }
        self.bars = clamped + Array(repeating: 0, count: 3 - clamped.count)
    }

    func setHovering(_ hovering: Bool) {
        isHovering = hovering
    }

    /// Hover is decided by the pointer's position (panel coordinates) against
    /// the capsule as currently sized, not by the panel's rectangular tracking
    /// area, so the transparent space above a collapsed pill doesn't expand it.
    /// The expanded capsule contains the collapsed one, so this can't flap.
    func pointerMoved(to point: NSPoint) {
        let hovering = PillGeometry.capsuleRect(height: height).contains(point)
        if hovering != isHovering { isHovering = hovering }
    }

    func pointerExited() {
        if isHovering { isHovering = false }
    }
}

/// Sizes and placement, shared by the SwiftUI layout, hover hit-testing and
/// the panel frame.
///
/// The panel never resizes: resizing a window makes AppKit re-derive its
/// tracking areas and report the resting pointer as exited, which collapsed
/// the pill mid-expansion and made it flap. Instead the panel is sized for the
/// tallest capsule and the capsule grows inside it, bottom-anchored. The
/// window server routes clicks on fully transparent pixels to the window
/// underneath, so the empty space above a short capsule never blocks clicks.
enum PillGeometry {
    static let width: CGFloat = 48
    static let cornerRadius: CGFloat = 24
    static let screenMargin: CGFloat = 16
    /// Room around the capsule for its drop shadow.
    static let shadowPad: CGFloat = 22

    static let padding: CGFloat = 7
    static let logo: CGFloat = 24
    /// The band the bars travel through; status marks reuse it.
    static let band: CGFloat = 22
    static let barWidth: CGFloat = 3
    static let barGap: CGFloat = 4
    static let button: CGFloat = 34
    static let buttonRadius: CGFloat = 8
    static let controlGap: CGFloat = 8
    static let timer: CGFloat = 12
    static let failedButtonGap: CGFloat = 6
    static let dot: CGFloat = 3.2
    static let dotRowGap: CGFloat = 2.4
    static let dotColumnGap: CGFloat = 3.2
    static let dividerGap: CGFloat = 6
    static var handle: CGFloat { 1 + dividerGap + dot * 2 + dotRowGap }
    /// Timer, Pause and Stop plus their spacing, revealed on hover.
    static var expandedArea: CGFloat { timer + controlGap + button + controlGap + button }
    static let animation: TimeInterval = 0.18

    static func height(for phase: PillPhase, expanded: Bool) -> CGFloat {
        let head = padding + logo + padding + band
        switch phase {
        case .starting, .recording:
            let extra = expanded ? expandedArea + padding : 0
            return head + extra + padding + handle + padding
        case .uploading, .success:
            return head + padding
        case .failed:
            return head + controlGap + button + 2 * (failedButtonGap + button) + padding
        }
    }

    /// The fixed panel: the tallest capsule any phase needs, plus shadow room.
    static var panelSize: NSSize {
        let tallest = max(height(for: .recording, expanded: true), height(for: .failed, expanded: false))
        return NSSize(width: width + 2 * shadowPad, height: tallest + 2 * shadowPad)
    }

    /// The capsule within the panel, in panel (bottom-left origin) coordinates.
    static func capsuleRect(height: CGFloat) -> NSRect {
        NSRect(x: shadowPad, y: shadowPad, width: width, height: height)
    }

    /// Panel origin that puts the capsule against the right edge of
    /// `visibleFrame`, the collapsed recording pill vertically centered; taller
    /// states extend upward from the same bottom edge.
    static func dockOrigin(in visibleFrame: NSRect) -> NSPoint {
        let collapsed = height(for: .recording, expanded: false)
        return NSPoint(
            x: visibleFrame.maxX - screenMargin - width - shadowPad,
            y: visibleFrame.midY - collapsed / 2 - shadowPad
        )
    }

    /// Pulls a dragged panel back so even the tallest capsule stays inside
    /// `visibleFrame` (only the shadow margin may hang off-screen).
    static func clampedOrigin(_ origin: NSPoint, within visibleFrame: NSRect) -> NSPoint {
        let capsule = NSSize(width: width, height: panelSize.height - 2 * shadowPad)
        let x = min(max(origin.x, visibleFrame.minX - shadowPad), visibleFrame.maxX - capsule.width - shadowPad)
        let y = min(max(origin.y, visibleFrame.minY - shadowPad), visibleFrame.maxY - capsule.height - shadowPad)
        return NSPoint(x: x, y: y)
    }

    /// Mirrors `barHeightPercent` in src/views/waveformBars.ts: levels are
    /// analyser bytes / 255, so speech lives in a narrow band well up the
    /// scale. Returns the bar's fraction of the band.
    static func barHeightFraction(_ level: Double) -> Double {
        let silence = 0.25, full = 0.7, curve = 0.65, floor = 0.08
        let normalized = min(1, max(0, (level - silence) / (full - silence)))
        return floor + pow(normalized, curve) * (1 - floor)
    }
}

/// Separates a click on the pill body (open Meetings) from a drag (move the
/// pill). Points are in screen coordinates.
struct ClickDragTracker {
    static let threshold: CGFloat = 3

    private let mouseDown: NSPoint
    private let windowOrigin: NSPoint
    private(set) var isClick = true

    init(mouseDown: NSPoint, windowOrigin: NSPoint) {
        self.mouseDown = mouseDown
        self.windowOrigin = windowOrigin
    }

    /// The window origin that keeps the pill under the pointer, or nil while
    /// the press is still within the click threshold.
    mutating func drag(to point: NSPoint) -> NSPoint? {
        let dx = point.x - mouseDown.x
        let dy = point.y - mouseDown.y
        if isClick && hypot(dx, dy) <= Self.threshold { return nil }
        isClick = false
        return NSPoint(x: windowOrigin.x + dx, y: windowOrigin.y + dy)
    }
}
