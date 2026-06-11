//! Pins the floating recorder ("waveform" window) to the right edge of the
//! library ("Meetings") window while a recording is on-going.
//!
//! Tauri emits no minimize/restore window events, so a small watcher task
//! polls instead: it exits when the waveform window is gone, idles while the
//! library window is absent or minimized (letting the user drag the recorder
//! freely), and otherwise keeps the recorder glued to the library's right
//! edge — re-attaching automatically when the library is restored.

use tauri::{AppHandle, Manager, PhysicalPosition};

const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(200);

/// Target outer position for the recorder: flush against the library
/// window's right edge, vertically centered. All values in physical pixels.
fn attach_position(
    lib_pos: (i32, i32),
    lib_size: (u32, u32),
    wave_size: (u32, u32),
) -> (i32, i32) {
    let x = lib_pos.0 + lib_size.0 as i32;
    let y = lib_pos.1 + (lib_size.1 as i32 - wave_size.1 as i32) / 2;
    (x, y)
}

/// Keep the waveform window attached to the library window for the lifetime
/// of the recording. Spawned when the waveform window is created.
pub(crate) fn spawn_watcher(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(POLL_INTERVAL).await;
            // Recording over (window closed/destroyed) — watcher is done.
            let Some(wave) = app.get_webview_window("waveform") else {
                return;
            };
            // No library window, or minimized: leave the recorder free.
            let Some(lib) = app.get_webview_window("library") else {
                continue;
            };
            if lib.is_minimized().unwrap_or(false) {
                continue;
            }
            let (Ok(lib_pos), Ok(lib_size), Ok(wave_pos), Ok(wave_size)) = (
                lib.outer_position(),
                lib.outer_size(),
                wave.outer_position(),
                wave.outer_size(),
            ) else {
                continue;
            };
            let (x, y) = attach_position(
                (lib_pos.x, lib_pos.y),
                (lib_size.width, lib_size.height),
                (wave_size.width, wave_size.height),
            );
            if (wave_pos.x, wave_pos.y) != (x, y) {
                let _ = wave.set_position(PhysicalPosition::new(x, y));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attach_position_is_flush_right_and_vertically_centered() {
        // Library at (100, 50), 900x600; recorder 92x284.
        // x: flush to the right edge = 100 + 900.
        // y: centered = 50 + (600 - 284) / 2 = 208.
        assert_eq!(
            attach_position((100, 50), (900, 600), (92, 284)),
            (1000, 208)
        );
    }

    #[test]
    fn attach_position_centers_a_recorder_taller_than_the_library() {
        // Library 100 tall at y=0, recorder 284 tall: y = (100 - 284) / 2 = -92.
        assert_eq!(attach_position((0, 0), (300, 100), (92, 284)), (300, -92));
    }
}
