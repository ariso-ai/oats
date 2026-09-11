//! FFI to the Swift recorder pill (`src-tauri/recorder-pill/macos`), linked in
//! as a static library by `build.rs`. AppKit is main-thread only, so every
//! call hops onto the main thread first.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use tauri::AppHandle;

use super::{PillAction, PillState};

/// Must equal `oatsPillAbiVersion()` in Bridge.swift.
const ABI_VERSION: i32 = 1;

type ActionCallback = extern "C" fn(i32);

unsafe extern "C" {
    fn oats_pill_abi_version() -> i32;
    fn oats_pill_create(on_action: ActionCallback);
    fn oats_pill_update(phase: i32, paused: bool, duration_s: u32, bars: *const f32, bar_count: u32);
    fn oats_pill_set_visible(visible: bool);
    fn oats_pill_destroy();
}

/// The app the Swift callback dispatches into. Set on first create.
static APP: OnceLock<AppHandle> = OnceLock::new();
/// False when the linked library speaks another ABI; the pill then stays off.
static ABI_OK: AtomicBool = AtomicBool::new(false);

extern "C" fn on_action(code: i32) {
    let (Some(app), Some(action)) = (APP.get(), PillAction::from_code(code)) else {
        return;
    };
    super::dispatch(app, action);
}

fn on_main(app: &AppHandle, f: impl FnOnce() + Send + 'static) {
    if !ABI_OK.load(Ordering::SeqCst) {
        return;
    }
    if let Err(error) = app.run_on_main_thread(f) {
        eprintln!("recorder pill: main-thread dispatch failed: {error}");
    }
}

pub(super) fn create(app: &AppHandle) {
    APP.get_or_init(|| app.clone());
    let version = unsafe { oats_pill_abi_version() };
    if version != ABI_VERSION {
        eprintln!("recorder pill: Swift ABI {version} != expected {ABI_VERSION}; pill disabled");
        return;
    }
    ABI_OK.store(true, Ordering::SeqCst);
    on_main(app, || unsafe { oats_pill_create(on_action) });
}

pub(super) fn update(app: &AppHandle, state: PillState) {
    on_main(app, move || unsafe {
        oats_pill_update(
            state.phase as i32,
            state.paused,
            state.duration_s,
            state.bars.as_ptr(),
            state.bars.len() as u32,
        )
    });
}

pub(super) fn set_visible(app: &AppHandle, visible: bool) {
    on_main(app, move || unsafe { oats_pill_set_visible(visible) });
}

pub(super) fn destroy(app: &AppHandle) {
    on_main(app, || unsafe { oats_pill_destroy() });
}

#[cfg(test)]
mod tests {
    #[test]
    fn linked_swift_library_speaks_this_abi() {
        assert_eq!(unsafe { super::oats_pill_abi_version() }, super::ABI_VERSION);
    }
}
