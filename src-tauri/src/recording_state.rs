//! Shared "is a recording in progress" flag, set when a recorder window opens
//! (manual via tray or auto via the mic monitor) and cleared when it stops or
//! is destroyed. The mic monitor reads it to suppress auto-triggers — and to
//! avoid self-triggering off a manual recording — regardless of which PID
//! macOS attributes our own capture to.

use std::sync::Mutex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RecordingSource {
    Manual,
    Auto,
}

#[derive(Default)]
pub struct RecordingState {
    inner: Mutex<Option<RecordingSource>>,
}

impl RecordingState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&self, source: RecordingSource) {
        *self.inner.lock().unwrap() = Some(source);
    }

    pub fn clear(&self) {
        *self.inner.lock().unwrap() = None;
    }

    pub fn is_active(&self) -> bool {
        self.inner.lock().unwrap().is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_inactive() {
        assert!(!RecordingState::new().is_active());
    }

    #[test]
    fn set_marks_active_clear_resets() {
        let s = RecordingState::new();
        s.set(RecordingSource::Manual);
        assert!(s.is_active());
        s.set(RecordingSource::Auto);
        assert!(s.is_active());
        s.clear();
        assert!(!s.is_active());
    }
}
