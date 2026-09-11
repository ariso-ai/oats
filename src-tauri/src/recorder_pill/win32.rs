//! The Windows recorder pill: a layered, topmost popup that never activates,
//! drawn with Direct2D/DirectWrite into a premultiplied bitmap and handed to
//! `UpdateLayeredWindow`. Geometry, hit-testing and hover come from
//! [`super::layout`]; this file only talks to Win32.
//!
//! Everything runs on Tauri's main thread (the one pumping this window's
//! messages). The window state lives in a thread-local and is never borrowed
//! across a call that can re-enter the window procedure: handlers return the
//! action to dispatch and the procedure runs it after the borrow ends.

use std::cell::RefCell;
use std::sync::OnceLock;
use std::time::Instant;

use tauri::AppHandle;
use windows::core::{w, Result, PCWSTR, PWSTR};
use windows::Win32::Foundation::{
    COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM,
};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_BEZIER_SEGMENT, D2D1_COLOR_F, D2D1_FIGURE_BEGIN_FILLED,
    D2D1_FIGURE_BEGIN_HOLLOW, D2D1_FIGURE_END_CLOSED, D2D1_FIGURE_END_OPEN,
    D2D1_FILL_MODE_ALTERNATE, D2D1_PIXEL_FORMAT, D2D_RECT_F, D2D_SIZE_F,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1DCRenderTarget, ID2D1Factory, ID2D1PathGeometry,
    ID2D1SolidColorBrush, ID2D1StrokeStyle, D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
    D2D1_ARC_SEGMENT, D2D1_ARC_SIZE_LARGE, D2D1_ARC_SIZE_SMALL, D2D1_CAP_STYLE_ROUND,
    D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_ELLIPSE, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_FEATURE_LEVEL_DEFAULT, D2D1_LINE_JOIN_ROUND, D2D1_RENDER_TARGET_PROPERTIES,
    D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_RENDER_TARGET_USAGE_NONE, D2D1_ROUNDED_RECT,
    D2D1_STROKE_STYLE_PROPERTIES, D2D1_SWEEP_DIRECTION_CLOCKWISE,
    D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE,
};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat, DWRITE_FACTORY_TYPE_SHARED,
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_NORMAL,
    DWRITE_MEASURING_MODE_NATURAL, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT_CENTER,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetMonitorInfoW,
    MonitorFromPoint, MonitorFromWindow, SelectObject, AC_SRC_ALPHA, AC_SRC_OVER, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, BLENDFUNCTION, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ, HMONITOR,
    MONITORINFO, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{
    InitCommonControlsEx, ICC_WIN95_CLASSES, INITCOMMONCONTROLSEX, TOOLTIPS_CLASSW, TTF_SUBCLASS,
    TTM_ADDTOOLW, TTM_DELTOOLW, TTS_ALWAYSTIP, TTS_NOPREFIX, TTTOOLINFOW, WM_MOUSELEAVE,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, GetDpiForWindow, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetCursorPos, GetWindowRect, KillTimer,
    LoadCursorW, RegisterClassExW, SendMessageW, SetTimer, SetWindowPos, ShowWindow,
    UpdateLayeredWindow, CW_USEDEFAULT, HWND_TOPMOST, IDC_ARROW, MA_NOACTIVATE, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SW_HIDE, SW_SHOWNOACTIVATE, ULW_ALPHA,
    WINDOW_STYLE, WM_CAPTURECHANGED, WM_DPICHANGED, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MOUSEACTIVATE, WM_MOUSEMOVE, WM_TIMER, WNDCLASSEXW, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};
use windows_numerics::{Matrix3x2, Vector2};

use super::layout::{self, ClickDragTracker, HeightAnimation, Hit, Rect};
use super::{PillAction, PillPhase, PillState};

const CLASS_NAME: PCWSTR = w!("OatsRecorderPill");
const FRAME_TIMER: usize = 1;
const FRAME_MS: u32 = 16;
const SPINNER_PERIOD_MS: f32 = 800.0;

/// The app clicks dispatch into. Set on first create.
static APP: OnceLock<AppHandle> = OnceLock::new();

thread_local! {
    static PILL: RefCell<Option<Pill>> = const { RefCell::new(None) };
    static CLASS_REGISTERED: RefCell<bool> = const { RefCell::new(false) };
}

fn on_main(app: &AppHandle, f: impl FnOnce() + Send + 'static) {
    if let Err(error) = app.run_on_main_thread(f) {
        eprintln!("recorder pill: main-thread dispatch failed: {error}");
    }
}

pub(super) fn create(app: &AppHandle) {
    APP.get_or_init(|| app.clone());
    on_main(app, || {
        if PILL.with(|p| p.borrow().is_some()) {
            return;
        }
        match Pill::new() {
            Ok(pill) => PILL.with(|p| *p.borrow_mut() = Some(pill)),
            Err(error) => eprintln!("recorder pill: failed to create the window: {error}"),
        }
    });
}

pub(super) fn update(app: &AppHandle, state: PillState) {
    on_main(app, move || {
        with_pill(|pill| pill.apply(state));
    });
}

pub(super) fn set_visible(app: &AppHandle, visible: bool) {
    on_main(app, move || {
        with_pill(|pill| pill.set_visible(visible));
    });
}

pub(super) fn destroy(app: &AppHandle) {
    on_main(app, || {
        // Take it out first: DestroyWindow re-enters the window procedure.
        if let Some(pill) = PILL.with(|p| p.borrow_mut().take()) {
            pill.teardown();
        }
    });
}

fn with_pill<R>(f: impl FnOnce(&mut Pill) -> R) -> Option<R> {
    PILL.with(|p| p.try_borrow_mut().ok()?.as_mut().map(f))
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_MOUSEACTIVATE {
        // Clicking the pill must never pull focus from the user's app.
        return LRESULT(MA_NOACTIVATE as isize);
    }
    let handled = with_pill(|pill| {
        if pill.hwnd != hwnd {
            return None;
        }
        pill.handle(msg, wparam, lparam)
    });
    match handled {
        Some(Some(Handled { result, action })) => {
            if let (Some(action), Some(app)) = (action, APP.get()) {
                super::dispatch(app, action);
            }
            result
        }
        // Not ours, not handled, or a re-entrant message while the pill is
        // borrowed (e.g. from SetWindowPos inside a handler).
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

struct Handled {
    result: LRESULT,
    action: Option<PillAction>,
}

impl Handled {
    fn done() -> Option<Self> {
        Some(Self { result: LRESULT(0), action: None })
    }
    fn act(action: Option<PillAction>) -> Option<Self> {
        Some(Self { result: LRESULT(0), action })
    }
}

/// What a left press landed on.
#[derive(Clone, Copy)]
enum Press {
    Button(PillAction),
    Body(ClickDragTracker),
}

struct Pill {
    hwnd: HWND,
    tooltip: Option<HWND>,
    tool_ids: Vec<usize>,
    tool_signature: Option<(PillPhase, bool, bool)>,
    renderer: Renderer,
    state: PillState,
    hovering: bool,
    hot: Option<PillAction>,
    press: Option<Press>,
    tracking_leave: bool,
    timer_on: bool,
    visible: bool,
    anim: HeightAnimation,
    epoch: Instant,
    dpi: u32,
}

impl Pill {
    fn new() -> Result<Self> {
        let instance: HINSTANCE = unsafe { GetModuleHandleW(None)? }.into();
        register_class(instance)?;
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                CLASS_NAME,
                w!("oats recorder"),
                WS_POPUP,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                1,
                1,
                None,
                None,
                Some(instance),
                None,
            )?
        };
        let dpi = unsafe { GetDpiForWindow(hwnd) }.max(96);
        let state = PillState {
            phase: PillPhase::Starting,
            paused: false,
            duration_s: 0,
            bars: [0.0; 3],
        };
        let renderer = match Renderer::new() {
            Ok(renderer) => renderer,
            Err(error) => {
                let _ = unsafe { DestroyWindow(hwnd) };
                return Err(error);
            }
        };
        let mut pill = Self {
            hwnd,
            tooltip: create_tooltip(hwnd, instance),
            tool_ids: Vec::new(),
            tool_signature: None,
            renderer,
            anim: HeightAnimation::settled(layout::capsule_height(state.phase, false)),
            state,
            hovering: false,
            hot: None,
            press: None,
            tracking_leave: false,
            timer_on: false,
            visible: false,
            epoch: Instant::now(),
            dpi,
        };
        pill.render();
        Ok(pill)
    }

    fn teardown(self) {
        unsafe {
            if let Some(tooltip) = self.tooltip {
                let _ = DestroyWindow(tooltip);
            }
            let _ = DestroyWindow(self.hwnd);
        }
        // `self.renderer` releases its GDI objects in Drop.
    }

    fn now_ms(&self) -> f32 {
        self.epoch.elapsed().as_secs_f32() * 1000.0
    }

    fn scale(&self) -> f32 {
        self.dpi as f32 / 96.0
    }

    fn expanded(&self) -> bool {
        self.hovering && matches!(self.state.phase, PillPhase::Starting | PillPhase::Recording)
    }

    fn apply(&mut self, state: PillState) {
        self.state = state;
        self.retarget();
        self.render();
    }

    fn set_visible(&mut self, visible: bool) {
        if visible == self.visible {
            return;
        }
        self.visible = visible;
        if visible {
            self.dock();
            // Paint before showing so the first visible frame is the pill.
            self.retarget();
            self.render();
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_SHOWNOACTIVATE);
                let _ = SetWindowPos(
                    self.hwnd,
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
        } else {
            self.hovering = false;
            self.hot = None;
            self.press = None;
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
            self.retarget();
        }
    }

    /// WM_DPICHANGED sent while a handler holds the pill (our own
    /// SetWindowPos) can't be handled, so re-read the DPI after moving.
    fn refresh_dpi(&mut self) {
        let dpi = unsafe { GetDpiForWindow(self.hwnd) }.max(96);
        if dpi != self.dpi {
            self.dpi = dpi;
            self.tool_signature = None;
            self.render();
        }
    }

    /// Right edge of the primary monitor's work area, like the webview pill.
    fn dock(&mut self) {
        let monitor = unsafe { MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY) };
        let Some(work) = work_area(monitor) else { return };
        let mut dpi_x = 96;
        let mut dpi_y = 96;
        let _ = unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y) };
        let scale = dpi_x.max(96) as f32 / 96.0;
        let (x, y) = layout::dock_origin(scaled(work, 1.0 / scale));
        self.move_to((x * scale).round() as i32, (y * scale).round() as i32);
        // Moving onto a monitor with another scale delivers WM_DPICHANGED,
        // but pick the new DPI up now too so the first frame is crisp.
        self.dpi = unsafe { GetDpiForWindow(self.hwnd) }.max(96);
    }

    fn move_to(&self, x: i32, y: i32) {
        unsafe {
            let _ = SetWindowPos(self.hwnd, None, x, y, 0, 0, SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
        }
    }

    fn window_origin(&self) -> (i32, i32) {
        let mut rect = RECT::default();
        let _ = unsafe { GetWindowRect(self.hwnd, &mut rect) };
        (rect.left, rect.top)
    }

    fn retarget(&mut self) {
        let target = layout::capsule_height(self.state.phase, self.expanded());
        let now = self.now_ms();
        self.anim.retarget(target, now);
        self.ensure_timer();
    }

    /// Frames tick while the capsule animates or the spinner spins.
    fn ensure_timer(&mut self) {
        let needed = self.visible
            && (self.anim.is_running(self.now_ms()) || self.state.phase == PillPhase::Uploading);
        if needed && !self.timer_on {
            unsafe { SetTimer(Some(self.hwnd), FRAME_TIMER, FRAME_MS, None) };
            self.timer_on = true;
        } else if !needed && self.timer_on {
            let _ = unsafe { KillTimer(Some(self.hwnd), FRAME_TIMER) };
            self.timer_on = false;
        }
    }

    fn logical(&self, lparam: LPARAM) -> (f32, f32) {
        let x = (lparam.0 & 0xffff) as u16 as i16 as f32;
        let y = ((lparam.0 >> 16) & 0xffff) as u16 as i16 as f32;
        (x / self.scale(), y / self.scale())
    }

    fn hit(&self, point: (f32, f32)) -> Hit {
        let height = self.anim.height_at(self.now_ms());
        layout::hit_test(self.state.phase, self.state.paused, height, self.expanded(), point)
    }

    fn handle(&mut self, msg: u32, wparam: WPARAM, lparam: LPARAM) -> Option<Handled> {
        match msg {
            WM_MOUSEMOVE => {
                if let Some(Press::Body(mut tracker)) = self.press {
                    let mut cursor = POINT::default();
                    let _ = unsafe { GetCursorPos(&mut cursor) };
                    if let Some((x, y)) = tracker.drag_to((cursor.x, cursor.y)) {
                        self.move_to(x, y);
                        self.refresh_dpi();
                    }
                    self.press = Some(Press::Body(tracker));
                    return Handled::done();
                }
                if !self.tracking_leave {
                    let mut track = TRACKMOUSEEVENT {
                        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                        dwFlags: TME_LEAVE,
                        hwndTrack: self.hwnd,
                        dwHoverTime: 0,
                    };
                    self.tracking_leave = unsafe { TrackMouseEvent(&mut track) }.is_ok();
                }
                let point = self.logical(lparam);
                let hovering = layout::hovering(self.state.phase, self.hovering, point);
                let hot = match self.hit(point) {
                    Hit::Button(action) => Some(action),
                    _ => None,
                };
                if hovering != self.hovering || hot != self.hot {
                    self.hovering = hovering;
                    self.hot = hot;
                    self.retarget();
                    self.render();
                }
                Handled::done()
            }
            WM_MOUSELEAVE => {
                self.tracking_leave = false;
                if self.press.is_none() {
                    self.hovering = false;
                    self.hot = None;
                    self.retarget();
                    self.render();
                }
                Handled::done()
            }
            WM_LBUTTONDOWN => {
                let point = self.logical(lparam);
                self.press = match self.hit(point) {
                    Hit::Outside => None,
                    Hit::Button(action) => Some(Press::Button(action)),
                    Hit::Body => {
                        let mut cursor = POINT::default();
                        let _ = unsafe { GetCursorPos(&mut cursor) };
                        Some(Press::Body(ClickDragTracker::new(
                            (cursor.x, cursor.y),
                            self.window_origin(),
                            layout::DRAG_THRESHOLD * self.scale(),
                        )))
                    }
                };
                if self.press.is_some() {
                    unsafe { SetCapture(self.hwnd) };
                }
                Handled::done()
            }
            WM_LBUTTONUP => {
                let press = self.press.take();
                let _ = unsafe { ReleaseCapture() };
                let action = match press {
                    Some(Press::Button(action)) => {
                        (self.hit(self.logical(lparam)) == Hit::Button(action)).then_some(action)
                    }
                    Some(Press::Body(tracker)) if tracker.is_click => Some(PillAction::OpenMeetings),
                    Some(Press::Body(_)) => {
                        self.clamp_on_screen();
                        self.refresh_dpi();
                        None
                    }
                    None => None,
                };
                Handled::act(action)
            }
            WM_CAPTURECHANGED => {
                // Capture taken away mid-press (e.g. a system dialog): abandon it.
                self.press = None;
                Handled::done()
            }
            WM_TIMER if wparam.0 == FRAME_TIMER => {
                self.render();
                self.ensure_timer();
                Handled::done()
            }
            WM_DPICHANGED => {
                self.dpi = ((wparam.0 & 0xffff) as u32).max(96);
                // SAFETY: WM_DPICHANGED's lParam points at the suggested RECT.
                let suggested = unsafe { *(lparam.0 as *const RECT) };
                self.move_to(suggested.left, suggested.top);
                self.tool_signature = None;
                self.render();
                Handled::done()
            }
            _ => None,
        }
    }

    /// Keep room to expand after a drag: a pill parked under the top edge
    /// would otherwise grow its controls off-screen.
    fn clamp_on_screen(&self) {
        let monitor = unsafe { MonitorFromWindow(self.hwnd, MONITOR_DEFAULTTONEAREST) };
        let Some(work) = work_area(monitor) else { return };
        let scale = self.scale();
        let (x, y) = self.window_origin();
        let (cx, cy) = layout::clamped_origin(
            (x as f32 / scale, y as f32 / scale),
            scaled(work, 1.0 / scale),
        );
        let (cx, cy) = ((cx * scale).round() as i32, (cy * scale).round() as i32);
        if (cx, cy) != (x, y) {
            self.move_to(cx, cy);
        }
    }

    fn render(&mut self) {
        if !self.visible {
            return;
        }
        let now = self.now_ms();
        let height = self.anim.height_at(now);
        let frame = Frame {
            state: &self.state,
            layout: layout::layout(self.state.phase, self.state.paused, height),
            reveal: reveal_fraction(self.state.phase, height),
            hot: self.hot,
            spinner_turns: now / SPINNER_PERIOD_MS,
        };
        if let Err(error) = self.renderer.draw(self.hwnd, self.dpi, &frame) {
            eprintln!("recorder pill: render failed: {error}");
        }
        self.sync_tooltips(height);
    }

    /// One tooltip per live button, rebuilt only when the set of buttons (or
    /// their positions) changes.
    fn sync_tooltips(&mut self, height: f32) {
        let Some(tooltip) = self.tooltip else { return };
        let settled = !self.anim.is_running(self.now_ms());
        let signature = (self.state.phase, self.state.paused, self.expanded() && settled);
        if self.tool_signature == Some(signature) || !settled {
            return;
        }
        self.tool_signature = Some(signature);
        for id in self.tool_ids.drain(..) {
            let mut info = tool_info(self.hwnd, id);
            unsafe {
                SendMessageW(tooltip, TTM_DELTOOLW, None, Some(LPARAM(&mut info as *mut _ as isize)));
            }
        }
        let layout = layout::layout(self.state.phase, self.state.paused, height);
        let live = self.state.phase == PillPhase::Failed || signature.2;
        if !live {
            return;
        }
        let scale = self.scale();
        for (id, button) in layout.buttons.iter().enumerate() {
            let mut text: Vec<u16> = button.label.encode_utf16().chain(Some(0)).collect();
            let mut info = tool_info(self.hwnd, id);
            info.uFlags = TTF_SUBCLASS;
            info.rect = RECT {
                left: (button.rect.x * scale) as i32,
                top: (button.rect.y * scale) as i32,
                right: (button.rect.right() * scale) as i32,
                bottom: (button.rect.bottom() * scale) as i32,
            };
            info.lpszText = PWSTR(text.as_mut_ptr());
            // The control copies the text during TTM_ADDTOOLW.
            unsafe {
                SendMessageW(tooltip, TTM_ADDTOOLW, None, Some(LPARAM(&mut info as *mut _ as isize)));
            }
            self.tool_ids.push(id);
        }
    }
}

fn register_class(instance: HINSTANCE) -> Result<()> {
    if CLASS_REGISTERED.with(|r| *r.borrow()) {
        return Ok(());
    }
    let class = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(wndproc),
        hInstance: instance,
        hCursor: unsafe { LoadCursorW(None, IDC_ARROW)? },
        lpszClassName: CLASS_NAME,
        ..Default::default()
    };
    if unsafe { RegisterClassExW(&class) } == 0 {
        return Err(windows::core::Error::from_thread());
    }
    CLASS_REGISTERED.with(|r| *r.borrow_mut() = true);
    Ok(())
}

fn create_tooltip(owner: HWND, instance: HINSTANCE) -> Option<HWND> {
    let init = INITCOMMONCONTROLSEX {
        dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
        dwICC: ICC_WIN95_CLASSES,
    };
    unsafe {
        let _ = InitCommonControlsEx(&init);
        CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_NOACTIVATE,
            TOOLTIPS_CLASSW,
            PCWSTR::null(),
            WS_POPUP | WINDOW_STYLE(TTS_ALWAYSTIP | TTS_NOPREFIX),
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            Some(owner),
            None,
            Some(instance),
            None,
        )
        .ok()
    }
}

fn tool_info(hwnd: HWND, id: usize) -> TTTOOLINFOW {
    TTTOOLINFOW {
        // The pre-v6 size: accepted by every comctl32 the app may load.
        cbSize: std::mem::offset_of!(TTTOOLINFOW, lpReserved) as u32,
        hwnd,
        uId: id,
        ..Default::default()
    }
}

fn work_area(monitor: HMONITOR) -> Option<Rect> {
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if !unsafe { GetMonitorInfoW(monitor, &mut info) }.as_bool() {
        return None;
    }
    let r = info.rcWork;
    Some(Rect::new(r.left as f32, r.top as f32, (r.right - r.left) as f32, (r.bottom - r.top) as f32))
}

fn scaled(r: Rect, by: f32) -> Rect {
    Rect::new(r.x * by, r.y * by, r.w * by, r.h * by)
}

/// How far the hover controls have slid in, 0…1, for their fade.
fn reveal_fraction(phase: PillPhase, height: f32) -> f32 {
    let collapsed = layout::capsule_height(phase, false);
    let expanded = layout::capsule_height(phase, true);
    if expanded <= collapsed {
        return 0.0;
    }
    ((height - collapsed) / (expanded - collapsed)).clamp(0.0, 1.0)
}

struct Frame<'a> {
    state: &'a PillState,
    layout: layout::Layout,
    reveal: f32,
    hot: Option<PillAction>,
    spinner_turns: f32,
}

const fn rgb(hex: u32) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: ((hex >> 16) & 0xff) as f32 / 255.0,
        g: ((hex >> 8) & 0xff) as f32 / 255.0,
        b: (hex & 0xff) as f32 / 255.0,
        a: 1.0,
    }
}

fn with_alpha(mut color: D2D1_COLOR_F, alpha: f32) -> D2D1_COLOR_F {
    color.a *= alpha;
    color
}

const PILL_BACKGROUND: D2D1_COLOR_F = rgb(0x0d0d0d);
const BAR_LIVE: D2D1_COLOR_F = rgb(0xf9d852);
const BAR_PAUSED: D2D1_COLOR_F = rgb(0x4b5563);
const BUTTON_FILL: D2D1_COLOR_F = rgb(0x1f1f1f);
const BUTTON_HOVER: D2D1_COLOR_F = rgb(0x2a2a2a);
const STOP_RED: D2D1_COLOR_F = rgb(0xf87171);
const TIMER_GRAY: D2D1_COLOR_F = rgb(0x9ca3af);
const DOT_GRAY: D2D1_COLOR_F = rgb(0x6b7280);
const OK_GREEN: D2D1_COLOR_F = rgb(0x34d399);
const RETRY_INDIGO: D2D1_COLOR_F = rgb(0x818cf8);
const WHITE: D2D1_COLOR_F = rgb(0xffffff);

fn v(x: f32, y: f32) -> Vector2 {
    Vector2 { X: x, Y: y }
}

fn d2d_rect(r: Rect) -> D2D_RECT_F {
    D2D_RECT_F { left: r.x, top: r.y, right: r.right(), bottom: r.bottom() }
}

fn rounded(r: Rect, radius: f32) -> D2D1_ROUNDED_RECT {
    D2D1_ROUNDED_RECT { rect: d2d_rect(r), radiusX: radius, radiusY: radius }
}

fn center(r: Rect) -> (f32, f32) {
    (r.x + r.w / 2.0, r.y + r.h / 2.0)
}

/// Device-independent drawing resources plus the bitmap the layered window
/// is updated from. Drawing happens in logical pixels (the render target's
/// DPI is the window's), so layout units map straight through.
struct Renderer {
    factory: ID2D1Factory,
    target: ID2D1DCRenderTarget,
    brush: ID2D1SolidColorBrush,
    round: ID2D1StrokeStyle,
    timer_text: IDWriteTextFormat,
    logo: ID2D1PathGeometry,
    surface: Option<Surface>,
}

/// A 32-bpp premultiplied DIB selected into a memory DC.
struct Surface {
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    size: (i32, i32),
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.previous);
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.dc);
        }
    }
}

impl Renderer {
    fn new() -> Result<Self> {
        unsafe {
            let factory: ID2D1Factory = D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)?;
            let target = factory.CreateDCRenderTarget(&D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 0.0,
                dpiY: 0.0,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            })?;
            target.SetAntialiasMode(D2D1_ANTIALIAS_MODE_PER_PRIMITIVE);
            // ClearType needs an opaque background; the pill sits on alpha.
            target.SetTextAntialiasMode(D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE);
            let brush = target.CreateSolidColorBrush(&WHITE, None)?;
            let round = factory.CreateStrokeStyle(
                &D2D1_STROKE_STYLE_PROPERTIES {
                    startCap: D2D1_CAP_STYLE_ROUND,
                    endCap: D2D1_CAP_STYLE_ROUND,
                    dashCap: D2D1_CAP_STYLE_ROUND,
                    lineJoin: D2D1_LINE_JOIN_ROUND,
                    miterLimit: 10.0,
                    ..Default::default()
                },
                None,
            )?;
            let dwrite: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;
            let timer_text = dwrite.CreateTextFormat(
                w!("Consolas"),
                None,
                DWRITE_FONT_WEIGHT_NORMAL,
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                10.0,
                w!(""),
            )?;
            timer_text.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER)?;
            timer_text.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            let logo = build_logo(&factory)?;
            Ok(Self { factory, target, brush, round, timer_text, logo, surface: None })
        }
    }

    fn surface(&mut self, size: (i32, i32)) -> Result<&Surface> {
        if self.surface.as_ref().map(|s| s.size) != Some(size) {
            self.surface = None;
            unsafe {
                let dc = CreateCompatibleDC(None);
                let info = BITMAPINFO {
                    bmiHeader: BITMAPINFOHEADER {
                        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                        biWidth: size.0,
                        biHeight: -size.1, // top-down
                        biPlanes: 1,
                        biBitCount: 32,
                        biCompression: BI_RGB.0,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                let mut bits = std::ptr::null_mut();
                let bitmap = match CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0) {
                    Ok(bitmap) => bitmap,
                    Err(error) => {
                        let _ = DeleteDC(dc);
                        return Err(error);
                    }
                };
                let previous = SelectObject(dc, bitmap.into());
                self.surface = Some(Surface { dc, bitmap, previous, size });
            }
        }
        Ok(self.surface.as_ref().expect("surface was just created"))
    }

    fn draw(&mut self, hwnd: HWND, dpi: u32, frame: &Frame) -> Result<()> {
        let scale = dpi as f32 / 96.0;
        let (panel_w, panel_h) = layout::panel_size();
        let size = ((panel_w * scale).ceil() as i32, (panel_h * scale).ceil() as i32);
        let dc = self.surface(size)?.dc;
        unsafe {
            self.target.BindDC(dc, &RECT { left: 0, top: 0, right: size.0, bottom: size.1 })?;
            self.target.SetDpi(dpi as f32, dpi as f32);
            self.target.BeginDraw();
            self.target.Clear(Some(&D2D1_COLOR_F { r: 0.0, g: 0.0, b: 0.0, a: 0.0 }));
            self.paint(frame)?;
            self.target.EndDraw(None, None)?;

            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            UpdateLayeredWindow(
                hwnd,
                None,
                None,
                Some(&SIZE { cx: size.0, cy: size.1 }),
                Some(dc),
                Some(&POINT { x: 0, y: 0 }),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            )
        }
    }

    fn fill(&self, color: D2D1_COLOR_F) -> &ID2D1SolidColorBrush {
        unsafe { self.brush.SetColor(&color) };
        &self.brush
    }

    fn paint(&self, frame: &Frame) -> Result<()> {
        let t = &self.target;
        let l = &frame.layout;
        let capsule = l.capsule;
        let radius = layout::CORNER_RADIUS.min(capsule.h / 2.0);

        // Soft drop shadow (0 6px 20px rgba(0,0,0,.5)): stacked translucent
        // rounded rects, since a DC render target has no blur effect.
        const LAYERS: usize = 10;
        for i in (0..LAYERS).rev() {
            let grow = i as f32 * 1.6;
            let rect = Rect::new(capsule.x - grow, capsule.y + 6.0 - grow, capsule.w + 2.0 * grow, capsule.h + 2.0 * grow);
            let alpha = 0.06 * (1.0 - i as f32 / LAYERS as f32);
            unsafe { t.FillRoundedRectangle(&rounded(rect, radius + grow), self.fill(with_alpha(rgb(0), alpha))) };
        }
        unsafe { t.FillRoundedRectangle(&rounded(capsule, radius), self.fill(PILL_BACKGROUND)) };

        // Logo: geometry is built in a 24×24 box at the origin.
        unsafe {
            t.SetTransform(&Matrix3x2::translation(l.logo.x, l.logo.y));
            t.FillGeometry(&self.logo, self.fill(WHITE), None);
            t.SetTransform(&Matrix3x2::identity());
        }

        match frame.state.phase {
            PillPhase::Starting | PillPhase::Recording => self.paint_capturing(frame)?,
            PillPhase::Uploading => self.paint_spinner(l.band, frame.spinner_turns)?,
            PillPhase::Success => self.paint_check(l.band),
            PillPhase::Failed => {
                self.paint_cross(l.band, 5.0, 2.8, STOP_RED);
                for button in &l.buttons {
                    self.paint_button(button.rect, frame.hot == Some(button.action), 1.0);
                    let c = center(button.rect);
                    match button.action {
                        PillAction::RetryUpload => self.paint_retry(c)?,
                        PillAction::ContinueRecording => unsafe {
                            t.FillEllipse(
                                &D2D1_ELLIPSE { point: v(c.0, c.1), radiusX: 5.0, radiusY: 5.0 },
                                self.fill(OK_GREEN),
                            )
                        },
                        _ => self.paint_cross(button.rect, 4.0, 1.8, STOP_RED),
                    }
                }
            }
        }
        Ok(())
    }

    fn paint_capturing(&self, frame: &Frame) -> Result<()> {
        let t = &self.target;
        let l = &frame.layout;
        let state = frame.state;
        let band = l.band;
        let bar_color = if state.paused { BAR_PAUSED } else { BAR_LIVE };
        let total = layout::BAR_WIDTH * 3.0 + layout::BAR_GAP * 2.0;
        for (i, level) in state.bars.iter().enumerate() {
            let h = layout::BAND * layout::bar_height_fraction(*level);
            let x = band.x + (band.w - total) / 2.0 + i as f32 * (layout::BAR_WIDTH + layout::BAR_GAP);
            let rect = Rect::new(x, band.y + (band.h - h) / 2.0, layout::BAR_WIDTH, h);
            unsafe { t.FillRoundedRectangle(&rounded(rect, 1.5), self.fill(bar_color)) };
        }

        if let Some(area) = l.expanded_area.filter(|a| a.h > 0.0) {
            let alpha = frame.reveal;
            unsafe { t.PushAxisAlignedClip(&d2d_rect(area), D2D1_ANTIALIAS_MODE_PER_PRIMITIVE) };
            if let Some(timer) = l.timer {
                let secs = state.duration_s;
                let text: Vec<u16> = format!("{:02}:{:02}", secs / 60, secs % 60).encode_utf16().collect();
                unsafe {
                    t.DrawText(
                        &text,
                        &self.timer_text,
                        &d2d_rect(timer),
                        self.fill(with_alpha(TIMER_GRAY, alpha)),
                        D2D1_DRAW_TEXT_OPTIONS_NONE,
                        DWRITE_MEASURING_MODE_NATURAL,
                    )
                };
            }
            for button in &l.buttons {
                self.paint_button(button.rect, frame.hot == Some(button.action), alpha);
                let (cx, cy) = center(button.rect);
                match button.action {
                    PillAction::Pause => {
                        for x in [cx - 5.0, cx + 1.5] {
                            let bar = Rect::new(x, cy - 6.0, 3.5, 12.0);
                            unsafe { t.FillRoundedRectangle(&rounded(bar, 1.0), self.fill(with_alpha(WHITE, alpha))) };
                        }
                    }
                    PillAction::Resume => unsafe {
                        t.FillEllipse(
                            &D2D1_ELLIPSE { point: v(cx, cy), radiusX: 5.5, radiusY: 5.5 },
                            self.fill(with_alpha(WHITE, alpha)),
                        )
                    },
                    _ => {
                        let square = Rect::new(cx - 5.0, cy - 5.0, 10.0, 10.0);
                        unsafe { t.FillRoundedRectangle(&rounded(square, 2.0), self.fill(with_alpha(STOP_RED, alpha))) };
                    }
                }
            }
            unsafe { t.PopAxisAlignedClip() };
        }

        if let Some(divider) = l.divider {
            unsafe { t.FillRectangle(&d2d_rect(divider), self.fill(with_alpha(WHITE, 0.08))) };
        }
        for dot in &l.dots {
            let (cx, cy) = center(*dot);
            unsafe {
                t.FillEllipse(
                    &D2D1_ELLIPSE { point: v(cx, cy), radiusX: dot.w / 2.0, radiusY: dot.h / 2.0 },
                    self.fill(DOT_GRAY),
                )
            };
        }
        Ok(())
    }

    fn paint_button(&self, rect: Rect, hot: bool, alpha: f32) {
        let color = if hot { BUTTON_HOVER } else { BUTTON_FILL };
        unsafe {
            self.target.FillRoundedRectangle(
                &rounded(rect, layout::BUTTON_RADIUS),
                self.fill(with_alpha(color, alpha)),
            )
        };
    }

    fn paint_check(&self, band: Rect) {
        let (cx, cy) = center(band);
        let points = [v(cx - 5.5, cy + 0.5), v(cx - 1.5, cy + 4.5), v(cx + 6.0, cy - 4.5)];
        unsafe {
            for pair in points.windows(2) {
                self.target.DrawLine(pair[0], pair[1], self.fill(OK_GREEN), 2.8, &self.round);
            }
        }
    }

    fn paint_cross(&self, around: Rect, half: f32, width: f32, color: D2D1_COLOR_F) {
        let (cx, cy) = center(around);
        unsafe {
            self.target.DrawLine(v(cx - half, cy - half), v(cx + half, cy + half), self.fill(color), width, &self.round);
            self.target.DrawLine(v(cx - half, cy + half), v(cx + half, cy - half), self.fill(color), width, &self.round);
        }
    }

    /// A clockwise arrow: an open 300° arc with an arrowhead at its end.
    fn paint_retry(&self, (cx, cy): (f32, f32)) -> Result<()> {
        let r = 5.5_f32;
        let start = (-60.0_f32).to_radians();
        let end = 240.0_f32.to_radians();
        let at = |a: f32| v(cx + r * a.cos(), cy + r * a.sin());
        unsafe {
            let arc = self.factory.CreatePathGeometry()?;
            let sink = arc.Open()?;
            sink.BeginFigure(at(start), D2D1_FIGURE_BEGIN_HOLLOW);
            sink.AddArc(&D2D1_ARC_SEGMENT {
                point: at(end),
                size: D2D_SIZE_F { width: r, height: r },
                rotationAngle: 0.0,
                sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                arcSize: D2D1_ARC_SIZE_LARGE,
            });
            sink.EndFigure(D2D1_FIGURE_END_OPEN);
            sink.Close()?;
            self.target.DrawGeometry(&arc, self.fill(RETRY_INDIGO), 2.0, &self.round);

            // Arrowhead pointing along the clockwise tangent at the arc's end.
            let tip = at(end);
            let (tx, ty) = (-end.sin(), end.cos());
            let (nx, ny) = (end.cos(), end.sin());
            let head = self.factory.CreatePathGeometry()?;
            let sink = head.Open()?;
            sink.BeginFigure(v(tip.X + tx * 3.0, tip.Y + ty * 3.0), D2D1_FIGURE_BEGIN_FILLED);
            sink.AddLine(v(tip.X + nx * 3.0 - tx, tip.Y + ny * 3.0 - ty));
            sink.AddLine(v(tip.X - nx * 3.0 - tx, tip.Y - ny * 3.0 - ty));
            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
            sink.Close()?;
            self.target.FillGeometry(&head, self.fill(RETRY_INDIGO), None);
        }
        Ok(())
    }

    fn paint_spinner(&self, band: Rect, turns: f32) -> Result<()> {
        let (cx, cy) = center(band);
        let r = 7.0_f32;
        unsafe {
            self.target.DrawEllipse(
                &D2D1_ELLIPSE { point: v(cx, cy), radiusX: r, radiusY: r },
                self.fill(BAR_PAUSED),
                2.0,
                &self.round,
            );
            let start = turns.fract() * std::f32::consts::TAU;
            let end = start + std::f32::consts::FRAC_PI_2;
            let at = |a: f32| v(cx + r * a.cos(), cy + r * a.sin());
            let arc = self.factory.CreatePathGeometry()?;
            let sink = arc.Open()?;
            sink.BeginFigure(at(start), D2D1_FIGURE_BEGIN_HOLLOW);
            sink.AddArc(&D2D1_ARC_SEGMENT {
                point: at(end),
                size: D2D_SIZE_F { width: r, height: r },
                rotationAngle: 0.0,
                sweepDirection: D2D1_SWEEP_DIRECTION_CLOCKWISE,
                arcSize: D2D1_ARC_SIZE_SMALL,
            });
            sink.EndFigure(D2D1_FIGURE_END_OPEN);
            sink.Close()?;
            self.target.DrawGeometry(&arc, self.fill(RETRY_INDIGO), 2.0, &self.round);
        }
        Ok(())
    }
}

/// The oats mark scaled into a 24×24 box at the origin, even-odd filled.
fn build_logo(factory: &ID2D1Factory) -> Result<ID2D1PathGeometry> {
    let vb = layout::LOGO_VIEWBOX;
    let s = (layout::LOGO / vb.w).min(layout::LOGO / vb.h);
    let (ox, oy) = ((layout::LOGO - vb.w * s) / 2.0, (layout::LOGO - vb.h * s) / 2.0);
    let map = |x: f32, y: f32| v(ox + (x - vb.x) * s, oy + (y - vb.y) * s);
    unsafe {
        let geometry = factory.CreatePathGeometry()?;
        let sink = geometry.Open()?;
        sink.SetFillMode(D2D1_FILL_MODE_ALTERNATE);
        let mut open = false;
        for data in layout::LOGO_PATHS {
            for cmd in layout::parse_path(data) {
                match cmd {
                    layout::PathCmd::Move(x, y) => {
                        if open {
                            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                        }
                        sink.BeginFigure(map(x, y), D2D1_FIGURE_BEGIN_FILLED);
                        open = true;
                    }
                    layout::PathCmd::Line(x, y) => sink.AddLine(map(x, y)),
                    layout::PathCmd::Curve([x1, y1, x2, y2, x, y]) => sink.AddBezier(&D2D1_BEZIER_SEGMENT {
                        point1: map(x1, y1),
                        point2: map(x2, y2),
                        point3: map(x, y),
                    }),
                    layout::PathCmd::Close => {
                        if open {
                            sink.EndFigure(D2D1_FIGURE_END_CLOSED);
                            open = false;
                        }
                    }
                }
            }
        }
        sink.Close()?;
        Ok(geometry)
    }
}
