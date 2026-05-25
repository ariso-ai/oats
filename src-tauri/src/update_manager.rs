//! In-app update orchestration. Owns scheduling, persistent state
//! (skip / snooze / auto-check toggle), and command handling. See
//! `docs/superpowers/specs/2026-05-25-in-app-updates-design.md`.

use serde::{Deserialize, Serialize};

/// User-facing snapshot of update state. Returned by `update_get_state`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct UpdateState {
    pub last_check_unix: Option<i64>,
    pub latest_known: Option<UpdateInfo>,
    pub auto_check_enabled: bool,
    pub skipped_version: Option<String>,
    pub snoozed_until_unix: Option<i64>,
}

impl Default for UpdateState {
    fn default() -> Self {
        Self {
            last_check_unix: None,
            latest_known: None,
            auto_check_enabled: true,
            skipped_version: None,
            snoozed_until_unix: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct UpdateInfo {
    pub version: String,
    pub notes: String,
    pub mandatory: bool,
}

/// Predicate: should the background scheduler trigger a check now?
///
/// Returns true iff:
/// - auto-check is enabled, AND
/// - no check has happened in the last 24h (or never), AND
/// - we are not currently snoozed
pub fn should_check(state: &UpdateState, now_unix: i64) -> bool {
    if !state.auto_check_enabled {
        return false;
    }
    let last_ok = match state.last_check_unix {
        None => true,
        Some(t) => now_unix - t > 24 * 60 * 60,
    };
    let snooze_ok = match state.snoozed_until_unix {
        None => true,
        Some(t) => now_unix > t,
    };
    last_ok && snooze_ok
}

/// After a check returns `new_version`, decide whether to clear the
/// persisted skip. Skip is cleared when the new version is strictly
/// greater than the skipped one (semver-ish string compare for simple
/// `MAJOR.MINOR.PATCH`; the actual updater uses real semver internally).
pub fn skip_cleared_by(skipped: &Option<String>, new_version: &str) -> bool {
    match skipped {
        None => false,
        Some(s) => version_gt(new_version, s),
    }
}

/// Returns true if `a > b` under simple dotted-integer comparison.
/// Both strings expected to be `N.N.N` (optionally with a `-suffix`
/// that is ignored for comparison purposes).
fn version_gt(a: &str, b: &str) -> bool {
    let parse = |v: &str| -> Vec<u32> {
        v.split('-').next().unwrap_or("")
            .split('.')
            .map(|p| p.parse::<u32>().unwrap_or(0))
            .collect()
    };
    let pa = parse(a);
    let pb = parse(b);
    for i in 0..pa.len().max(pb.len()) {
        let x = pa.get(i).copied().unwrap_or(0);
        let y = pb.get(i).copied().unwrap_or(0);
        if x != y {
            return x > y;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st() -> UpdateState {
        UpdateState::default()
    }

    const NOW: i64 = 1_800_000_000;
    const DAY: i64 = 24 * 60 * 60;

    #[test]
    fn should_check_when_never_checked_and_auto_on() {
        assert!(should_check(&st(), NOW));
    }

    #[test]
    fn should_not_check_when_auto_off() {
        let s = UpdateState {
            auto_check_enabled: false,
            ..Default::default()
        };
        assert!(!should_check(&s, NOW));
    }

    #[test]
    fn should_not_check_when_recently_checked() {
        let s = UpdateState {
            last_check_unix: Some(NOW - 10 * 60),
            ..Default::default()
        };
        assert!(!should_check(&s, NOW));
    }

    #[test]
    fn should_check_when_check_is_25h_old() {
        let s = UpdateState {
            last_check_unix: Some(NOW - 25 * 60 * 60),
            ..Default::default()
        };
        assert!(should_check(&s, NOW));
    }

    #[test]
    fn should_not_check_while_snoozed() {
        let s = UpdateState {
            snoozed_until_unix: Some(NOW + 60 * 60),
            ..Default::default()
        };
        assert!(!should_check(&s, NOW));
    }

    #[test]
    fn should_check_after_snooze_expires() {
        let s = UpdateState {
            snoozed_until_unix: Some(NOW - 60),
            ..Default::default()
        };
        assert!(should_check(&s, NOW));
    }

    #[test]
    fn skip_cleared_by_higher_version() {
        let skipped = Some("0.3.0".to_string());
        assert!(skip_cleared_by(&skipped, "0.3.1"));
        assert!(skip_cleared_by(&skipped, "0.4.0"));
        assert!(skip_cleared_by(&skipped, "1.0.0"));
    }

    #[test]
    fn skip_not_cleared_by_same_or_lower() {
        let skipped = Some("0.3.0".to_string());
        assert!(!skip_cleared_by(&skipped, "0.3.0"));
        assert!(!skip_cleared_by(&skipped, "0.2.9"));
    }

    #[test]
    fn skip_cleared_when_none_returns_false() {
        assert!(!skip_cleared_by(&None, "1.0.0"));
    }
}
