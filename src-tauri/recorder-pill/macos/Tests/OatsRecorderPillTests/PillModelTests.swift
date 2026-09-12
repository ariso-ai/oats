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

    func testPanelFitsTheTallestCapsulePlusShadowRoom() {
        let tallest = max(
            PillGeometry.height(for: .recording, expanded: true),
            PillGeometry.height(for: .failed, expanded: false)
        )
        XCTAssertEqual(PillGeometry.panelSize.width, PillGeometry.width + 2 * PillGeometry.shadowPad)
        XCTAssertEqual(PillGeometry.panelSize.height, tallest + 2 * PillGeometry.shadowPad)
    }

    func testCapsuleIsBottomAnchoredAndCenteredInThePanel() {
        let rect = PillGeometry.capsuleRect(height: 90)
        XCTAssertEqual(rect, NSRect(x: PillGeometry.shadowPad, y: PillGeometry.shadowPad, width: PillGeometry.width, height: 90))
        // Growing keeps the bottom edge where it is.
        XCTAssertEqual(PillGeometry.capsuleRect(height: 190).minY, rect.minY)
    }

    func testDocksCollapsedCapsuleToTheRightEdgeVerticallyCentered() {
        let visible = NSRect(x: 0, y: 40, width: 1440, height: 860)
        let origin = PillGeometry.dockOrigin(in: visible)
        let capsule = PillGeometry.capsuleRect(height: PillGeometry.height(for: .recording, expanded: false))
            .offsetBy(dx: origin.x, dy: origin.y)
        XCTAssertEqual(capsule.maxX, visible.maxX - PillGeometry.screenMargin)
        XCTAssertEqual(capsule.midY, visible.midY)
    }

    func testDockOriginRespectsAnOffsetScreen() {
        let visible = NSRect(x: -1920, y: 0, width: 1920, height: 1080)
        let origin = PillGeometry.dockOrigin(in: visible)
        let capsule = PillGeometry.capsuleRect(height: 90).offsetBy(dx: origin.x, dy: origin.y)
        XCTAssertEqual(capsule.maxX, -PillGeometry.screenMargin)
    }

    func testClampKeepsTheTallestCapsuleOnScreen() {
        let visible = NSRect(x: 0, y: 0, width: 1440, height: 900)
        let tallest = PillGeometry.panelSize.height - 2 * PillGeometry.shadowPad
        func capsule(at origin: NSPoint) -> NSRect {
            PillGeometry.capsuleRect(height: tallest).offsetBy(dx: origin.x, dy: origin.y)
        }
        // Dragged up under the menu bar: expanding must not go off the top.
        XCTAssertEqual(capsule(at: PillGeometry.clampedOrigin(NSPoint(x: 600, y: 850), within: visible)).maxY, visible.maxY)
        // Dragged below the bottom edge, and off either side.
        XCTAssertEqual(capsule(at: PillGeometry.clampedOrigin(NSPoint(x: 600, y: -200), within: visible)).minY, visible.minY)
        XCTAssertEqual(capsule(at: PillGeometry.clampedOrigin(NSPoint(x: -300, y: 300), within: visible)).minX, visible.minX)
        XCTAssertEqual(capsule(at: PillGeometry.clampedOrigin(NSPoint(x: 1500, y: 300), within: visible)).maxX, visible.maxX)
        // Anywhere inside is left alone.
        XCTAssertEqual(PillGeometry.clampedOrigin(NSPoint(x: 600, y: 300), within: visible), NSPoint(x: 600, y: 300))
    }

    func testHoverFollowsThePointerOverTheCapsuleOnly() {
        let model = PillModel()
        model.apply(phase: .recording, isPaused: false, durationSeconds: 1, bars: [])
        let collapsed = PillGeometry.capsuleRect(height: model.height)
        // Over the transparent space above the collapsed capsule: no hover.
        model.pointerMoved(to: NSPoint(x: collapsed.midX, y: collapsed.maxY + 20))
        XCTAssertFalse(model.isHovering)
        // Onto the capsule: expands.
        model.pointerMoved(to: NSPoint(x: collapsed.midX, y: collapsed.midY))
        XCTAssertTrue(model.isExpanded)
        // Up into the revealed controls: the expanded capsule contains it, so
        // hover holds instead of collapsing under the pointer.
        model.pointerMoved(to: NSPoint(x: collapsed.midX, y: collapsed.maxY + 20))
        XCTAssertTrue(model.isExpanded)
        // Into the shadow margin beside it: collapses.
        model.pointerMoved(to: NSPoint(x: collapsed.minX - 4, y: collapsed.midY))
        XCTAssertFalse(model.isHovering)
    }

    func testPointerLeavingThePanelEndsHover() {
        let model = PillModel()
        model.apply(phase: .recording, isPaused: false, durationSeconds: 1, bars: [])
        let rect = PillGeometry.capsuleRect(height: model.height)
        model.pointerMoved(to: NSPoint(x: rect.midX, y: rect.midY))
        XCTAssertTrue(model.isHovering)
        model.pointerExited()
        XCTAssertFalse(model.isHovering)
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
