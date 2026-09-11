import AppKit
import XCTest
@testable import OatsRecorderPill

@MainActor
final class PillModelTests: XCTestCase {
    func testFormatsDurationAsMinutesAndSeconds() {
        let model = PillModel()
        model.apply(phase: .recording, isPaused: false, durationSeconds: 0, bars: [])
        XCTAssertEqual(model.formattedDuration, "00:00")
        model.apply(phase: .recording, isPaused: false, durationSeconds: 65, bars: [])
        XCTAssertEqual(model.formattedDuration, "01:05")
        model.apply(phase: .recording, isPaused: false, durationSeconds: 3_600, bars: [])
        XCTAssertEqual(model.formattedDuration, "60:00")
    }

    func testAlwaysHoldsThreeClampedBars() {
        let model = PillModel()
        model.apply(phase: .recording, isPaused: false, durationSeconds: 1, bars: [0.5])
        XCTAssertEqual(model.bars, [0.5, 0, 0])
        model.apply(phase: .recording, isPaused: false, durationSeconds: 1, bars: [-1, 2, 0.25, 0.9])
        XCTAssertEqual(model.bars, [0, 1, 0.25])
        model.apply(phase: .recording, isPaused: false, durationSeconds: 1, bars: [.nan, 0.5, 0.5])
        XCTAssertEqual(model.bars, [0, 0.5, 0.5])
    }

    func testHoverExpandsOnlyWhileCapturing() {
        let model = PillModel()
        model.setHovering(true)
        for phase in [PillPhase.starting, .recording] {
            model.apply(phase: phase, isPaused: false, durationSeconds: 1, bars: [])
            XCTAssertTrue(model.isExpanded, "\(phase) should expand on hover")
        }
        for phase in [PillPhase.uploading, .success, .failed] {
            model.apply(phase: phase, isPaused: false, durationSeconds: 1, bars: [])
            XCTAssertFalse(model.isExpanded, "\(phase) must not expand")
        }
        model.apply(phase: .recording, isPaused: false, durationSeconds: 1, bars: [])
        model.setHovering(false)
        XCTAssertFalse(model.isExpanded)
    }

    func testPauseLabelTracksPausedState() {
        let model = PillModel()
        model.apply(phase: .recording, isPaused: false, durationSeconds: 1, bars: [])
        XCTAssertEqual(model.pauseResumeLabel, "Pause recording")
        XCTAssertEqual(model.pauseResumeAction, .pause)
        model.apply(phase: .recording, isPaused: true, durationSeconds: 1, bars: [])
        XCTAssertEqual(model.pauseResumeLabel, "Resume recording")
        XCTAssertEqual(model.pauseResumeAction, .resume)
    }

    func testHeightGrowsWhenExpandedAndFitsEachPhase() {
        let collapsed = PillGeometry.height(for: .recording, expanded: false)
        let expanded = PillGeometry.height(for: .recording, expanded: true)
        XCTAssertGreaterThan(expanded, collapsed)
        // Status phases are shorter than the recording pill; failed stacks
        // three 34pt buttons under the status mark.
        XCTAssertLessThan(PillGeometry.height(for: .uploading, expanded: false), collapsed)
        XCTAssertEqual(
            PillGeometry.height(for: .success, expanded: false),
            PillGeometry.height(for: .uploading, expanded: false)
        )
        XCTAssertGreaterThan(PillGeometry.height(for: .failed, expanded: false), 3 * PillGeometry.button)
        // Expansion is meaningless outside capture.
        XCTAssertEqual(
            PillGeometry.height(for: .failed, expanded: true),
            PillGeometry.height(for: .failed, expanded: false)
        )
    }

    func testDocksCollapsedPillToTheRightEdgeVerticallyCentered() {
        let visible = NSRect(x: 0, y: 40, width: 1440, height: 860)
        let frame = PillGeometry.dockFrame(in: visible, height: 120)
        XCTAssertEqual(frame.maxX, visible.maxX - PillGeometry.screenMargin)
        XCTAssertEqual(frame.width, PillGeometry.width)
        XCTAssertEqual(frame.height, 120)
        let collapsed = PillGeometry.height(for: .recording, expanded: false)
        // The collapsed pill is centered; taller states grow upward from it.
        XCTAssertEqual(frame.minY, visible.midY - collapsed / 2)
    }

    func testDockFrameRespectsAnOffsetScreen() {
        let visible = NSRect(x: -1920, y: 0, width: 1920, height: 1080)
        let frame = PillGeometry.dockFrame(in: visible, height: 90)
        XCTAssertEqual(frame.maxX, -PillGeometry.screenMargin)
    }

    func testResizeKeepsTheBottomEdgeFixed() {
        let frame = NSRect(x: 100, y: 200, width: 48, height: 90)
        let screen = NSRect(x: 0, y: 0, width: 1440, height: 900)
        let grown = PillGeometry.resized(frame, toHeight: 190, within: screen)
        XCTAssertEqual(grown, NSRect(x: 100, y: 200, width: 48, height: 190))
    }

    func testResizeNearTheScreenTopGrowsDownInstead() {
        // Dragged up against the menu bar: growing upward would go off-screen.
        let screen = NSRect(x: 0, y: 0, width: 1440, height: 900)
        let frame = NSRect(x: 100, y: 800, width: 48, height: 90)
        let grown = PillGeometry.resized(frame, toHeight: 190, within: screen)
        XCTAssertEqual(grown, NSRect(x: 100, y: 710, width: 48, height: 190))
    }

    func testBarHeightMatchesTheWebPillCurve() {
        // Mirrors src/views/waveformBars.ts barHeightPercent.
        XCTAssertEqual(PillGeometry.barHeightFraction(0), 0.08, accuracy: 1e-9)
        XCTAssertEqual(PillGeometry.barHeightFraction(0.25), 0.08, accuracy: 1e-9)
        XCTAssertEqual(PillGeometry.barHeightFraction(0.7), 1, accuracy: 1e-9)
        XCTAssertEqual(PillGeometry.barHeightFraction(1), 1, accuracy: 1e-9)
        let mid = 0.08 + pow(0.5, 0.65) * 0.92
        XCTAssertEqual(PillGeometry.barHeightFraction(0.475), mid, accuracy: 1e-9)
    }
}

final class ClickDragTrackerTests: XCTestCase {
    func testSmallMovementIsAClick() {
        var tracker = ClickDragTracker(mouseDown: NSPoint(x: 10, y: 10), windowOrigin: NSPoint(x: 500, y: 300))
        XCTAssertNil(tracker.drag(to: NSPoint(x: 12, y: 12)))
        XCTAssertTrue(tracker.isClick)
    }

    func testMovementPastThresholdDragsTheWindowWithThePointer() {
        var tracker = ClickDragTracker(mouseDown: NSPoint(x: 10, y: 10), windowOrigin: NSPoint(x: 500, y: 300))
        XCTAssertEqual(tracker.drag(to: NSPoint(x: 14, y: 10)), NSPoint(x: 504, y: 300))
        XCTAssertFalse(tracker.isClick)
        // Once dragging, every movement follows the pointer — even back inside
        // the threshold — and the gesture can no longer become a click.
        XCTAssertEqual(tracker.drag(to: NSPoint(x: 11, y: 9)), NSPoint(x: 501, y: 299))
        XCTAssertFalse(tracker.isClick)
    }
}

final class LogoPathTests: XCTestCase {
    func testParsesAbsoluteMoveLineCurveClose() {
        let path = LogoPath.parse("M1 2L3 4C5 6 7 8 9 10Z")
        XCTAssertEqual(path, [
            .move(CGPoint(x: 1, y: 2)),
            .line(CGPoint(x: 3, y: 4)),
            .curve(CGPoint(x: 5, y: 6), CGPoint(x: 7, y: 8), CGPoint(x: 9, y: 10)),
            .close,
        ])
    }

    func testLogoOutlineParsesCompletely() {
        for data in LogoPath.oatsMark {
            let commands = LogoPath.parse(data)
            XCTAssertFalse(commands.isEmpty)
            XCTAssertEqual(commands.last, .close)
        }
    }
}
