import SwiftUI

private extension Color {
    init(hex: UInt32) {
        self.init(
            red: Double((hex >> 16) & 0xff) / 255,
            green: Double((hex >> 8) & 0xff) / 255,
            blue: Double(hex & 0xff) / 255
        )
    }

    static let pillBackground = Color(hex: 0x0d0d0d)
    static let barLive = Color(hex: 0xf9d852)
    static let barPaused = Color(hex: 0x4b5563)
    static let buttonFill = Color(hex: 0x1f1f1f)
    static let buttonHover = Color(hex: 0x2a2a2a)
    static let stopRed = Color(hex: 0xf87171)
    static let timerGray = Color(hex: 0x9ca3af)
    static let dotGray = Color(hex: 0x6b7280)
    static let okGreen = Color(hex: 0x34d399)
    static let retryIndigo = Color(hex: 0x818cf8)
}

/// The capsule. Its height always equals the panel's (see `PillController`),
/// so the view fills the window and the flexible expanded area soaks up the
/// difference while the window animates between collapsed and expanded.
struct PillView: View {
    @ObservedObject var model: PillModel
    /// Moves the panel while the body is dragged; a press without movement
    /// ends as `.openMeetings`.
    let onBodyDrag: (BodyDragEvent) -> Void
    let send: (PillAction) -> Void

    enum BodyDragEvent { case changed, ended }

    private typealias G = PillGeometry

    var body: some View {
        VStack(spacing: 0) {
            OatsLogo()
                .fill(Color.white, style: FillStyle(eoFill: true))
                .frame(width: G.logo, height: G.logo)
                .padding(.top, G.padding)
                .accessibilityHidden(true)
            switch model.phase {
            case .starting, .recording:
                capturing
            case .uploading:
                Spinner()
                    .frame(width: 16, height: 16)
                    .frame(height: G.band)
                    .padding(.top, G.padding)
                    .accessibilityLabel("Saving recording")
                Spacer(minLength: G.padding)
            case .success:
                statusMark("checkmark", color: .okGreen, label: "Recording saved")
                Spacer(minLength: G.padding)
            case .failed:
                failed
            }
        }
        .frame(width: G.width)
        .frame(maxHeight: .infinity, alignment: .top)
        .background(Color.pillBackground)
        .clipShape(RoundedRectangle(cornerRadius: G.cornerRadius, style: .continuous))
        .contentShape(RoundedRectangle(cornerRadius: G.cornerRadius, style: .continuous))
        .gesture(
            DragGesture(minimumDistance: 0, coordinateSpace: .global)
                .onChanged { _ in onBodyDrag(.changed) }
                .onEnded { _ in onBodyDrag(.ended) }
        )
        .accessibilityElement(children: .contain)
        .accessibilityLabel("oats recorder")
        .accessibilityAddTraits(.isButton)
        .accessibilityHint("Opens the Meetings window")
        .accessibilityAction { send(.openMeetings) }
    }

    @ViewBuilder private var capturing: some View {
        HStack(spacing: G.barGap) {
            ForEach(0..<3, id: \.self) { i in
                RoundedRectangle(cornerRadius: 2)
                    .fill(model.isPaused ? Color.barPaused : Color.barLive)
                    .frame(width: G.barWidth, height: G.band * G.barHeightFraction(model.bars[i]))
            }
        }
        .frame(height: G.band)
        .padding(.top, G.padding)
        .animation(.linear(duration: 0.075), value: model.bars)
        .accessibilityHidden(true)

        // Always present so the reveal can follow the window's height; the
        // flexible frame collapses to nothing while the pill is collapsed.
        VStack(spacing: G.controlGap) {
            Text(model.formattedDuration)
                .font(.system(size: 10, design: .monospaced))
                .foregroundColor(.timerGray)
                .frame(height: G.timer)
                .accessibilityLabel("Elapsed \(model.formattedDuration)")
            PillButton(label: model.pauseResumeLabel) { send(model.pauseResumeAction) } icon: {
                if model.isPaused {
                    Circle().fill(Color.white).frame(width: 11, height: 11)
                } else {
                    HStack(spacing: 3) {
                        RoundedRectangle(cornerRadius: 1).frame(width: 3.5, height: 12)
                        RoundedRectangle(cornerRadius: 1).frame(width: 3.5, height: 12)
                    }
                    .foregroundColor(.white)
                }
            }
            PillButton(label: "Stop and save recording") { send(.stop) } icon: {
                RoundedRectangle(cornerRadius: 2).fill(Color.stopRed).frame(width: 10, height: 10)
            }
        }
        .padding(.top, G.padding)
        .frame(minHeight: 0, maxHeight: .infinity, alignment: .top)
        .clipped()
        .opacity(model.isExpanded ? 1 : 0)
        .animation(.easeOut(duration: 0.15), value: model.isExpanded)
        .accessibilityHidden(!model.isExpanded)

        VStack(spacing: G.dividerGap) {
            Rectangle().fill(Color.white.opacity(0.08)).frame(width: 22, height: 1)
            Grid(horizontalSpacing: G.dotColumnGap, verticalSpacing: G.dotRowGap) {
                ForEach(0..<2, id: \.self) { _ in
                    GridRow {
                        ForEach(0..<3, id: \.self) { _ in
                            Circle().fill(Color.dotGray).frame(width: G.dot, height: G.dot)
                        }
                    }
                }
            }
        }
        .padding(.top, G.padding)
        .padding(.bottom, G.padding)
        .accessibilityHidden(true)
    }

    @ViewBuilder private var failed: some View {
        statusMark("xmark", color: .stopRed, label: "Upload failed")
        VStack(spacing: G.failedButtonGap) {
            PillButton(label: "Retry upload") { send(.retryUpload) } icon: {
                Image(systemName: "arrow.clockwise").font(.system(size: 13, weight: .bold)).foregroundColor(.retryIndigo)
            }
            PillButton(label: "Continue recording") { send(.continueRecording) } icon: {
                Circle().fill(Color.okGreen).frame(width: 10, height: 10)
            }
            PillButton(label: "Discard recording") { send(.discardRecording) } icon: {
                Image(systemName: "xmark").font(.system(size: 11, weight: .bold)).foregroundColor(.stopRed)
            }
        }
        .padding(.top, G.controlGap)
        Spacer(minLength: G.padding)
    }

    private func statusMark(_ symbol: String, color: Color, label: String) -> some View {
        Image(systemName: symbol)
            .font(.system(size: 15, weight: .heavy))
            .foregroundColor(color)
            .frame(height: G.band)
            .padding(.top, G.padding)
            .accessibilityLabel(label)
    }
}

/// A 34pt rounded-square control with a hover highlight and a tooltip.
private struct PillButton<Icon: View>: View {
    let label: String
    let action: () -> Void
    @ViewBuilder let icon: () -> Icon
    @State private var hovering = false

    var body: some View {
        Button(action: action) {
            icon()
                .frame(width: PillGeometry.button, height: PillGeometry.button)
                .background(
                    RoundedRectangle(cornerRadius: PillGeometry.buttonRadius, style: .continuous)
                        .fill(hovering ? Color.buttonHover : Color.buttonFill)
                )
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
        .help(label)
        .accessibilityLabel(label)
    }
}

private struct Spinner: View {
    @State private var spinning = false

    var body: some View {
        ZStack {
            Circle().stroke(Color.barPaused, lineWidth: 2)
            Circle()
                .trim(from: 0, to: 0.25)
                .stroke(Color.retryIndigo, style: StrokeStyle(lineWidth: 2, lineCap: .round))
                .rotationEffect(.degrees(spinning ? 360 : 0))
                .animation(.linear(duration: 0.8).repeatForever(autoreverses: false), value: spinning)
        }
        .onAppear { spinning = true }
    }
}
