//! Platform-free geometry for the natively drawn pill on Windows: sizes,
//! element positions, hit-testing, hover, click-vs-drag, and docking. Mirrors
//! `PillGeometry`/`PillModel` in the Swift pill so both platforms look and
//! behave the same. Kept free of Win32 so it is unit-tested on every host.
//!
//! Units are logical (96-DPI) pixels with a top-left origin, in panel
//! coordinates unless noted. The panel never resizes (see the spec): it fits
//! the tallest capsule plus shadow room, and the capsule is bottom-anchored
//! inside it.

// Off Windows this module is compiled only for its tests, so drawing
// constants the Win32 renderer uses look unused there.
#![cfg_attr(not(target_os = "windows"), allow(dead_code))]

use super::{PillAction, PillPhase};

pub(crate) const WIDTH: f32 = 48.0;
pub(crate) const CORNER_RADIUS: f32 = 24.0;
pub(crate) const SCREEN_MARGIN: f32 = 16.0;
pub(crate) const SHADOW_PAD: f32 = 22.0;
pub(crate) const PADDING: f32 = 7.0;
pub(crate) const LOGO: f32 = 24.0;
pub(crate) const BAND: f32 = 22.0;
pub(crate) const BAR_WIDTH: f32 = 3.0;
pub(crate) const BAR_GAP: f32 = 4.0;
pub(crate) const BUTTON: f32 = 34.0;
pub(crate) const BUTTON_RADIUS: f32 = 8.0;
pub(crate) const CONTROL_GAP: f32 = 8.0;
pub(crate) const TIMER: f32 = 12.0;
pub(crate) const FAILED_BUTTON_GAP: f32 = 6.0;
pub(crate) const DOT: f32 = 3.2;
pub(crate) const DOT_ROW_GAP: f32 = 2.4;
pub(crate) const DOT_COLUMN_GAP: f32 = 3.2;
pub(crate) const DIVIDER_GAP: f32 = 6.0;
pub(crate) const DIVIDER_WIDTH: f32 = 22.0;
pub(crate) const HANDLE: f32 = 1.0 + DIVIDER_GAP + DOT * 2.0 + DOT_ROW_GAP;
/// Timer, Pause and Stop plus their spacing, revealed on hover.
pub(crate) const EXPANDED_AREA: f32 = TIMER + CONTROL_GAP + BUTTON + CONTROL_GAP + BUTTON;
pub(crate) const ANIMATION_MS: f32 = 180.0;
pub(crate) const DRAG_THRESHOLD: f32 = 3.0;
/// Top of the band (bars / status mark) within the capsule.
const BAND_TOP: f32 = PADDING + LOGO + PADDING;
/// First pixel below the band, where the expanded area / failed buttons start.
const HEAD: f32 = BAND_TOP + BAND;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub(crate) const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
    pub(crate) fn contains(&self, (px, py): (f32, f32)) -> bool {
        px >= self.x && px < self.x + self.w && py >= self.y && py < self.y + self.h
    }
    pub(crate) fn offset(&self, dx: f32, dy: f32) -> Self {
        Self::new(self.x + dx, self.y + dy, self.w, self.h)
    }
    pub(crate) fn bottom(&self) -> f32 {
        self.y + self.h
    }
    pub(crate) fn right(&self) -> f32 {
        self.x + self.w
    }
}

fn can_expand(phase: PillPhase) -> bool {
    matches!(phase, PillPhase::Starting | PillPhase::Recording)
}

/// The capsule's height for a state (expansion only applies while capturing).
pub(crate) fn capsule_height(phase: PillPhase, expanded: bool) -> f32 {
    match phase {
        PillPhase::Starting | PillPhase::Recording => {
            let extra = if expanded { EXPANDED_AREA + PADDING } else { 0.0 };
            HEAD + extra + PADDING + HANDLE + PADDING
        }
        PillPhase::Uploading | PillPhase::Success => HEAD + PADDING,
        PillPhase::Failed => {
            HEAD + CONTROL_GAP + BUTTON + 2.0 * (FAILED_BUTTON_GAP + BUTTON) + PADDING
        }
    }
}

/// The fixed panel: the tallest capsule any phase needs, plus shadow room.
pub(crate) fn panel_size() -> (f32, f32) {
    let tallest = capsule_height(PillPhase::Recording, true)
        .max(capsule_height(PillPhase::Failed, false));
    (WIDTH + 2.0 * SHADOW_PAD, tallest + 2.0 * SHADOW_PAD)
}

/// The capsule within the panel, bottom-anchored so it grows upward.
pub(crate) fn capsule_rect(height: f32) -> Rect {
    let (_, panel_h) = panel_size();
    Rect::new(SHADOW_PAD, panel_h - SHADOW_PAD - height, WIDTH, height)
}

/// A clickable control: where it is, what it does, and its tooltip.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Button {
    pub rect: Rect,
    pub action: PillAction,
    pub label: &'static str,
}

/// Where every element sits for one frame, in panel coordinates. `height` is
/// the capsule's current (possibly mid-animation) height; the expanded area is
/// whatever room is left between the band and the drag handle, clipped.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Layout {
    pub capsule: Rect,
    pub logo: Rect,
    pub band: Rect,
    /// Space revealed between band and handle while capturing (clip to it).
    pub expanded_area: Option<Rect>,
    pub timer: Option<Rect>,
    pub buttons: Vec<Button>,
    pub divider: Option<Rect>,
    pub dots: Vec<Rect>,
}

pub(crate) fn layout(phase: PillPhase, paused: bool, height: f32) -> Layout {
    let capsule = capsule_rect(height);
    let (cx, cy) = (capsule.x, capsule.y);
    let centered = |w: f32, y: f32, h: f32| Rect::new(cx + (WIDTH - w) / 2.0, cy + y, w, h);
    let logo = centered(LOGO, PADDING, LOGO);
    let band = centered(WIDTH, BAND_TOP, BAND);
    let mut out = Layout {
        capsule,
        logo,
        band,
        expanded_area: None,
        timer: None,
        buttons: Vec::new(),
        divider: None,
        dots: Vec::new(),
    };
    match phase {
        PillPhase::Starting | PillPhase::Recording => {
            let handle_top = height - PADDING - HANDLE;
            let area_h = (handle_top - PADDING - HEAD).max(0.0);
            out.expanded_area = Some(Rect::new(cx, cy + HEAD, WIDTH, area_h));
            let timer_top = HEAD + PADDING;
            out.timer = Some(centered(WIDTH, timer_top, TIMER));
            let pause_top = timer_top + TIMER + CONTROL_GAP;
            let (pause_action, pause_label) = if paused {
                (PillAction::Resume, "Resume recording")
            } else {
                (PillAction::Pause, "Pause recording")
            };
            out.buttons.push(Button {
                rect: centered(BUTTON, pause_top, BUTTON),
                action: pause_action,
                label: pause_label,
            });
            out.buttons.push(Button {
                rect: centered(BUTTON, pause_top + BUTTON + CONTROL_GAP, BUTTON),
                action: PillAction::Stop,
                label: "Stop and save recording",
            });
            out.divider = Some(centered(DIVIDER_WIDTH, handle_top, 1.0));
            let grid_w = DOT * 3.0 + DOT_COLUMN_GAP * 2.0;
            let first_row = handle_top + 1.0 + DIVIDER_GAP;
            for row in 0..2 {
                for col in 0..3 {
                    out.dots.push(Rect::new(
                        cx + (WIDTH - grid_w) / 2.0 + col as f32 * (DOT + DOT_COLUMN_GAP),
                        cy + first_row + row as f32 * (DOT + DOT_ROW_GAP),
                        DOT,
                        DOT,
                    ));
                }
            }
        }
        PillPhase::Uploading | PillPhase::Success => {}
        PillPhase::Failed => {
            let mut top = HEAD + CONTROL_GAP;
            for (action, label) in [
                (PillAction::RetryUpload, "Retry upload"),
                (PillAction::ContinueRecording, "Continue recording"),
                (PillAction::DiscardRecording, "Discard recording"),
            ] {
                out.buttons.push(Button { rect: centered(BUTTON, top, BUTTON), action, label });
                top += BUTTON + FAILED_BUTTON_GAP;
            }
        }
    }
    out
}

/// What the pointer is over.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Hit {
    Button(PillAction),
    Body,
    Outside,
}

/// Buttons are only live while visible: Pause/Stop need the pill expanded (and
/// fully revealed), the failed-upload buttons are always shown.
pub(crate) fn hit_test(phase: PillPhase, paused: bool, height: f32, expanded: bool, point: (f32, f32)) -> Hit {
    let layout = layout(phase, paused, height);
    if !layout.capsule.contains(point) {
        return Hit::Outside;
    }
    let buttons_live = phase == PillPhase::Failed
        || (can_expand(phase) && expanded && height >= capsule_height(phase, true));
    if buttons_live {
        if let Some(button) = layout.buttons.iter().find(|b| b.rect.contains(point)) {
            return Hit::Button(button.action);
        }
    }
    Hit::Body
}

/// Hover comes from the pointer against the capsule at its *target* height:
/// once hovering, the expanded capsule contains the collapsed one, so moving
/// up into the revealed controls keeps it open instead of flapping.
pub(crate) fn hovering(phase: PillPhase, currently_hovering: bool, point: (f32, f32)) -> bool {
    let expanded = currently_hovering && can_expand(phase);
    capsule_rect(capsule_height(phase, expanded)).contains(point)
}

/// Separates a click on the pill body (open Meetings) from a drag (move the
/// pill). Points and origins are in screen pixels.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ClickDragTracker {
    down: (i32, i32),
    origin: (i32, i32),
    threshold: f32,
    pub is_click: bool,
}

impl ClickDragTracker {
    /// `threshold` is `DRAG_THRESHOLD` scaled to the monitor.
    pub(crate) fn new(down: (i32, i32), origin: (i32, i32), threshold: f32) -> Self {
        Self { down, origin, threshold, is_click: true }
    }

    /// The window origin that keeps the pill under the pointer, or `None`
    /// while the press is still within the click threshold.
    pub(crate) fn drag_to(&mut self, point: (i32, i32)) -> Option<(i32, i32)> {
        let (dx, dy) = (point.0 - self.down.0, point.1 - self.down.1);
        if self.is_click && ((dx * dx + dy * dy) as f32).sqrt() <= self.threshold {
            return None;
        }
        self.is_click = false;
        Some((self.origin.0 + dx, self.origin.1 + dy))
    }
}

/// Panel origin (logical px, screen space) docking the capsule to the right
/// edge of `work`, with the collapsed recording pill vertically centered.
pub(crate) fn dock_origin(work: Rect) -> (f32, f32) {
    let (_, panel_h) = panel_size();
    let collapsed = capsule_height(PillPhase::Recording, false);
    let capsule_top = work.y + (work.h - collapsed) / 2.0;
    (
        work.right() - SCREEN_MARGIN - WIDTH - SHADOW_PAD,
        capsule_top + collapsed + SHADOW_PAD - panel_h,
    )
}

/// Pulls a dragged panel back so even the tallest capsule stays inside `work`
/// (only the shadow margin may hang off-screen).
pub(crate) fn clamped_origin((x, y): (f32, f32), work: Rect) -> (f32, f32) {
    // The panel is the tallest capsule inset by SHADOW_PAD on every side.
    let (panel_w, panel_h) = panel_size();
    (
        x.clamp(work.x - SHADOW_PAD, work.right() - panel_w + SHADOW_PAD),
        y.clamp(work.y - SHADOW_PAD, work.bottom() - panel_h + SHADOW_PAD),
    )
}

/// Mirrors `barHeightPercent` in src/views/waveformBars.ts: levels are
/// analyser bytes / 255, so speech lives in a narrow band well up the scale.
/// Returns the bar's fraction of the band.
pub(crate) fn bar_height_fraction(level: f32) -> f32 {
    const SILENCE: f32 = 0.25;
    const FULL: f32 = 0.7;
    const CURVE: f32 = 0.65;
    const FLOOR: f32 = 0.08;
    let normalized = ((level - SILENCE) / (FULL - SILENCE)).clamp(0.0, 1.0);
    FLOOR + normalized.powf(CURVE) * (1.0 - FLOOR)
}

/// Cubic ease-out, close to SwiftUI's `.easeOut` used on macOS.
pub(crate) fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

/// The capsule's height sliding between two states.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct HeightAnimation {
    from: f32,
    to: f32,
    start_ms: f32,
}

impl HeightAnimation {
    pub(crate) fn settled(height: f32) -> Self {
        Self { from: height, to: height, start_ms: 0.0 }
    }

    /// Retarget from wherever the capsule is right now.
    pub(crate) fn retarget(&mut self, to: f32, now_ms: f32) {
        if to == self.to {
            return;
        }
        self.from = self.height_at(now_ms);
        self.to = to;
        self.start_ms = now_ms;
    }

    pub(crate) fn target(&self) -> f32 {
        self.to
    }

    pub(crate) fn height_at(&self, now_ms: f32) -> f32 {
        let t = (now_ms - self.start_ms) / ANIMATION_MS;
        self.from + (self.to - self.from) * ease_out(t)
    }

    pub(crate) fn is_running(&self, now_ms: f32) -> bool {
        self.from != self.to && now_ms - self.start_ms < ANIMATION_MS
    }
}

/// The white oats mark from src/assets/oats-tray-white.svg (same data as
/// `LogoPath.swift`): ring outline + inner hole, then the three face bars,
/// filled together with the even-odd rule.
pub(crate) const LOGO_VIEWBOX: Rect = Rect::new(69.0, 30.0, 204.0, 228.0);
pub(crate) const LOGO_PATHS: [&str; 4] = [
    "M128.007 50.1915C149.478 38.9382 174.281 36.0041 197.806 42.1485C215.411 46.7569 234.104 61.963 245.979 75.5811C251.536 81.95 255.316 89.0095 258.095 96.9317C265.706 118.697 266.501 138.943 256.041 159.879C250.276 171.409 242.51 181.247 233.172 190.637L249.793 213.23C250.742 214.524 250.673 216.043 249.707 217.251C230.134 234.028 212.2 244.229 186.569 249.165C170.587 252.255 154.604 250.77 139.381 244.919C125.297 239.499 112.68 231.266 102.134 220.53C87.0833 205.186 80.7146 186.425 77.418 165.506C75.6747 154.46 75.5013 143.776 77.1064 132.678C78.7116 121.58 81.5254 110.222 85.3916 99.2618C89.6548 87.2144 96.2482 76.8063 105.103 67.6759C111.937 60.6339 119.239 54.7998 128.007 50.1915Z M139.691 65.415C154.776 58.5284 171.122 56.9577 187.398 60.0126C213.046 64.8281 235.345 81.4324 239.471 108.22C242.502 130.414 235.761 176.355 208.634 187.009L207.989 187.254L225.974 212.401C220.243 216.509 214.651 220.014 208.403 223.293C180.27 235.202 149.788 238.447 125.192 217.769C112.023 206.689 102.772 191.845 98.336 174.947C92.4331 152.475 93.5726 128.311 101.581 106.546C108.382 88.0432 121.914 73.5444 139.691 65.415Z",
    "M121.588 110.824C122.236 106.889 125.643 104.035 129.631 104.015C133.557 103.996 136.949 106.732 137.587 110.606C138.6 116.756 139.885 126.3 139.929 135.354C139.974 144.408 138.782 153.964 137.829 160.124C137.228 164.003 133.864 166.772 129.938 166.791C125.949 166.811 122.515 163.99 121.828 160.061C120.533 152.653 118.677 141.078 118.649 135.458C118.622 129.837 120.365 118.245 121.588 110.824Z",
    "M158.289 96.682C158.696 92.9849 161.815 90.2015 165.535 90.1833C169.216 90.1654 172.337 92.8623 172.768 96.5186C173.767 104.993 175.343 120.627 175.415 135.356C175.487 150.086 174.063 165.734 173.148 174.218C172.753 177.878 169.658 180.605 165.976 180.623C162.257 180.642 159.11 177.889 158.667 174.196C157.445 164.008 155.243 144.443 155.199 135.455C155.155 126.466 157.167 106.882 158.289 96.682Z",
    "M193.625 110.824C194.273 106.889 197.68 104.035 201.668 104.015C205.594 103.996 208.986 106.732 209.624 110.606C210.637 116.756 211.922 126.3 211.966 135.354C212.011 144.408 210.819 153.964 209.866 160.124C209.265 164.003 205.901 166.772 201.975 166.791C197.986 166.811 194.552 163.99 193.865 160.061C192.57 152.653 190.714 141.078 190.686 135.458C190.659 129.837 192.402 118.245 193.625 110.824Z",
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum PathCmd {
    Move(f32, f32),
    Line(f32, f32),
    Curve([f32; 6]),
    Close,
}

/// Parses the absolute `M`/`L`/`C`/`Z` subset the mark uses.
pub(crate) fn parse_path(data: &str) -> Vec<PathCmd> {
    let mut out = Vec::new();
    let mut op = ' ';
    let mut nums: Vec<f32> = Vec::new();
    let flush = |op: char, nums: &mut Vec<f32>, out: &mut Vec<PathCmd>| {
        match (op, nums.len()) {
            ('M', 2) => out.push(PathCmd::Move(nums[0], nums[1])),
            ('L', 2) => out.push(PathCmd::Line(nums[0], nums[1])),
            ('C', 6) => out.push(PathCmd::Curve([nums[0], nums[1], nums[2], nums[3], nums[4], nums[5]])),
            _ => {}
        }
        nums.clear();
    };
    for token in data.split_inclusive(|c: char| "MLCZ ,".contains(c)) {
        let (number, tail) = match token.char_indices().last() {
            Some((i, c)) if "MLCZ ,".contains(c) => (&token[..i], Some(c)),
            _ => (token, None),
        };
        if let Ok(value) = number.parse::<f32>() {
            nums.push(value);
        }
        if let Some(c @ ('M' | 'L' | 'C' | 'Z')) = tail {
            flush(op, &mut nums, &mut out);
            op = c;
            if c == 'Z' {
                out.push(PathCmd::Close);
            }
        }
    }
    flush(op, &mut nums, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn heights_match_the_swift_pill() {
        assert!(close(capsule_height(PillPhase::Recording, false), 89.8));
        assert!(close(capsule_height(PillPhase::Starting, false), 89.8));
        assert!(close(capsule_height(PillPhase::Recording, true), 192.8));
        assert!(close(capsule_height(PillPhase::Uploading, false), 67.0));
        assert!(close(capsule_height(PillPhase::Success, true), 67.0));
        assert!(close(capsule_height(PillPhase::Failed, false), 189.0));
        assert!(close(capsule_height(PillPhase::Failed, true), 189.0));
    }

    #[test]
    fn panel_fits_the_tallest_capsule_plus_shadow_room() {
        let (w, h) = panel_size();
        assert!(close(w, WIDTH + 2.0 * SHADOW_PAD));
        assert!(close(h, 192.8 + 2.0 * SHADOW_PAD));
    }

    #[test]
    fn capsule_is_bottom_anchored() {
        let (_, h) = panel_size();
        let short = capsule_rect(67.0);
        let tall = capsule_rect(189.0);
        assert!(close(short.bottom(), h - SHADOW_PAD));
        assert!(close(tall.bottom(), short.bottom()));
        assert!(close(short.x, SHADOW_PAD));
    }

    #[test]
    fn expanded_recording_layout_stacks_timer_pause_stop_then_handle() {
        let h = capsule_height(PillPhase::Recording, true);
        let l = layout(PillPhase::Recording, false, h);
        let c = l.capsule;
        assert!(close(l.logo.y - c.y, 7.0));
        assert!(close(l.band.y - c.y, 38.0));
        let timer = l.timer.unwrap();
        assert!(close(timer.y - c.y, 67.0));
        assert_eq!(l.buttons.len(), 2);
        assert_eq!(l.buttons[0].action, PillAction::Pause);
        assert_eq!(l.buttons[0].label, "Pause recording");
        assert!(close(l.buttons[0].rect.y - c.y, 87.0));
        assert_eq!(l.buttons[1].action, PillAction::Stop);
        assert!(close(l.buttons[1].rect.y - c.y, 129.0));
        assert!(close(l.buttons[1].rect.x - c.x, 7.0));
        let divider = l.divider.unwrap();
        assert!(close(divider.y - c.y, 170.0));
        assert_eq!(l.dots.len(), 6);
        assert!(close(l.dots.last().unwrap().bottom(), c.bottom() - PADDING));
    }

    #[test]
    fn collapsed_layout_leaves_no_room_for_the_controls() {
        let l = layout(PillPhase::Recording, false, capsule_height(PillPhase::Recording, false));
        assert!(close(l.expanded_area.unwrap().h, 0.0));
        assert!(close(l.divider.unwrap().y - l.capsule.y, 67.0));
    }

    #[test]
    fn paused_layout_offers_resume() {
        let l = layout(PillPhase::Recording, true, capsule_height(PillPhase::Recording, true));
        assert_eq!(l.buttons[0].action, PillAction::Resume);
        assert_eq!(l.buttons[0].label, "Resume recording");
    }

    #[test]
    fn failed_layout_has_retry_continue_discard() {
        let l = layout(PillPhase::Failed, false, capsule_height(PillPhase::Failed, false));
        let actions: Vec<_> = l.buttons.iter().map(|b| (b.action, b.label)).collect();
        assert_eq!(
            actions,
            [
                (PillAction::RetryUpload, "Retry upload"),
                (PillAction::ContinueRecording, "Continue recording"),
                (PillAction::DiscardRecording, "Discard recording"),
            ]
        );
        assert!(close(l.buttons[0].rect.y - l.capsule.y, 68.0));
        assert!(close(l.buttons[2].rect.bottom(), l.capsule.bottom() - PADDING));
        assert!(l.divider.is_none() && l.dots.is_empty());
    }

    fn center(r: Rect) -> (f32, f32) {
        (r.x + r.w / 2.0, r.y + r.h / 2.0)
    }

    #[test]
    fn hit_testing_finds_live_buttons_only() {
        let full = capsule_height(PillPhase::Recording, true);
        let l = layout(PillPhase::Recording, false, full);
        let stop = center(l.buttons[1].rect);
        assert_eq!(hit_test(PillPhase::Recording, false, full, true, stop), Hit::Button(PillAction::Stop));
        // Mid-animation the controls are still sliding in: not clickable yet.
        assert_eq!(hit_test(PillPhase::Recording, false, full - 10.0, true, stop), Hit::Body);
        assert_eq!(hit_test(PillPhase::Recording, false, full, true, center(l.logo)), Hit::Body);
        // Above the collapsed capsule is transparent panel space.
        let collapsed = capsule_height(PillPhase::Recording, false);
        let above = (center(l.logo).0, capsule_rect(collapsed).y - 10.0);
        assert_eq!(hit_test(PillPhase::Recording, false, collapsed, false, above), Hit::Outside);

        let failed = capsule_height(PillPhase::Failed, false);
        let fl = layout(PillPhase::Failed, false, failed);
        assert_eq!(
            hit_test(PillPhase::Failed, false, failed, false, center(fl.buttons[2].rect)),
            Hit::Button(PillAction::DiscardRecording)
        );
    }

    #[test]
    fn hover_follows_the_capsule_without_flapping() {
        let collapsed = capsule_rect(capsule_height(PillPhase::Recording, false));
        let above = (collapsed.x + 24.0, collapsed.y - 20.0);
        let inside = (collapsed.x + 24.0, collapsed.y + 40.0);
        assert!(!hovering(PillPhase::Recording, false, above));
        assert!(hovering(PillPhase::Recording, false, inside));
        // Once hovering, the revealed controls above count as inside.
        assert!(hovering(PillPhase::Recording, true, above));
        // Beside the capsule, in the shadow margin: not hovering.
        assert!(!hovering(PillPhase::Recording, true, (collapsed.x - 4.0, collapsed.y + 40.0)));
        // Status phases never expand, so the space above stays outside.
        let status = capsule_rect(capsule_height(PillPhase::Uploading, false));
        assert!(!hovering(PillPhase::Uploading, true, (status.x + 24.0, status.y - 20.0)));
    }

    #[test]
    fn small_movement_is_a_click_larger_is_a_drag() {
        let mut t = ClickDragTracker::new((10, 10), (500, 300), 3.0);
        assert_eq!(t.drag_to((12, 12)), None);
        assert!(t.is_click);
        assert_eq!(t.drag_to((14, 10)), Some((504, 300)));
        assert!(!t.is_click);
        // Once dragging, every movement follows the pointer.
        assert_eq!(t.drag_to((11, 9)), Some((501, 299)));
    }

    #[test]
    fn docks_the_collapsed_capsule_to_the_right_edge_vertically_centered() {
        let work = Rect::new(0.0, 0.0, 1920.0, 1040.0);
        let (x, y) = dock_origin(work);
        let capsule = capsule_rect(capsule_height(PillPhase::Recording, false)).offset(x, y);
        assert!(close(capsule.right(), 1920.0 - SCREEN_MARGIN));
        assert!(close(capsule.y + capsule.h / 2.0, 520.0));
        // A monitor to the left of the primary one.
        let (x, _) = dock_origin(Rect::new(-1920.0, 0.0, 1920.0, 1080.0));
        assert!(close(capsule_rect(89.8).offset(x, 0.0).right(), -SCREEN_MARGIN));
    }

    #[test]
    fn clamp_keeps_the_tallest_capsule_on_screen() {
        let work = Rect::new(0.0, 0.0, 1440.0, 900.0);
        let tallest = panel_size().1 - 2.0 * SHADOW_PAD;
        let capsule = |(x, y): (f32, f32)| capsule_rect(tallest).offset(x, y);
        assert!(close(capsule(clamped_origin((600.0, -300.0), work)).y, 0.0));
        assert!(close(capsule(clamped_origin((600.0, 2000.0), work)).bottom(), 900.0));
        assert!(close(capsule(clamped_origin((-300.0, 300.0), work)).x, 0.0));
        assert!(close(capsule(clamped_origin((1500.0, 300.0), work)).right(), 1440.0));
        assert_eq!(clamped_origin((600.0, 300.0), work), (600.0, 300.0));
    }

    #[test]
    fn bar_curve_matches_the_web_pill() {
        assert!(close(bar_height_fraction(0.0), 0.08));
        assert!(close(bar_height_fraction(0.25), 0.08));
        assert!(close(bar_height_fraction(0.7), 1.0));
        assert!(close(bar_height_fraction(1.0), 1.0));
        assert!(close(bar_height_fraction(0.475), 0.08 + 0.5_f32.powf(0.65) * 0.92));
    }

    #[test]
    fn height_animation_eases_to_its_target() {
        let mut a = HeightAnimation::settled(90.0);
        assert!(!a.is_running(0.0));
        a.retarget(190.0, 1000.0);
        assert!(close(a.height_at(1000.0), 90.0));
        assert!(a.is_running(1090.0));
        let mid = a.height_at(1090.0);
        assert!(mid > 140.0 && mid < 190.0, "ease-out is past halfway at half time: {mid}");
        assert!(close(a.height_at(1180.0), 190.0));
        assert!(!a.is_running(1180.0));
        // Retargeting mid-flight starts from the current height.
        a.retarget(90.0, 1090.0);
        a.retarget(190.0, 1100.0);
        assert!(a.height_at(1100.0) < 190.0);
        assert!(close(a.target(), 190.0));
    }

    #[test]
    fn parses_the_logo_path_subset() {
        assert_eq!(
            parse_path("M1 2L3 4C5 6 7 8 9 10Z"),
            [
                PathCmd::Move(1.0, 2.0),
                PathCmd::Line(3.0, 4.0),
                PathCmd::Curve([5.0, 6.0, 7.0, 8.0, 9.0, 10.0]),
                PathCmd::Close,
            ]
        );
        for data in LOGO_PATHS {
            let cmds = parse_path(data);
            assert!(cmds.len() > 5);
            assert_eq!(cmds.last(), Some(&PathCmd::Close));
            assert!(matches!(cmds[0], PathCmd::Move(..)));
        }
        // The ring has two subpaths (outline + hole).
        let ring = parse_path(LOGO_PATHS[0]);
        assert_eq!(ring.iter().filter(|c| matches!(c, PathCmd::Move(..))).count(), 2);
    }
}
