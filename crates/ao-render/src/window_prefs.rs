//! Native macOS window size/position persistence (bug: the client forgot the window size between launches).
//!
//! Retail stores the chosen display resolution in its account-scoped `Login.cfg` (`IndependentPrefs_t` type 0 "login prefs",
//! `aomac/src/play/dvalue/indep.rs`), loaded by `Preferences_t::LoadLoginPrefs` before the character list but after the account
//! name is known. The native OS window this crate opens exists before any account is chosen (there is no login screen yet for
//! `IndependentPrefs_t` to key a file on), so this mirrors the same idea -- an aomac-own prefs file, outside the client install
//! (skill rule: never write inside the client dir) -- next to `play/prefs.rs`'s own store (same `AOMAC_PREFS_DIR` override, so an
//! agent's isolated prefs dir never touches the user's real `~/Library/Application Support/aomac/Window.cfg`).
//!
//! Audit items 6/7 (aomac Enhanced, not retail): also remembers which display the window was on (by name, since display IDs
//! are not stable across reboots) and whether it was fullscreen, so a restart restores the same monitor/fullscreen state
//! and (`viewer.rs::State::new`) clamps the saved position onto whichever monitor is actually available this time.

use std::io::Write;
use std::path::PathBuf;

/// The last saved native window; `pos`/`monitor` are `None` until an initial placement is known (winit chooses the
/// very first window's spot; nothing is lost if monitor's name can't be read back, the position is just unclamped).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Saved {
    /// Logical (not physical) pixels: a size saved on one display must still look the same size when restored onto a
    /// display with a different backing scale (audit item 7), not visually double/halve.
    pub width: u32,
    pub height: u32,
    /// Physical pixels (an on-screen position, unlike size, is already DPI-unified: `MonitorHandle::position()`).
    pub pos: Option<(i32, i32)>,
    pub monitor: Option<String>,
    pub fullscreen: bool,
}

/// `play/prefs.rs::dir()`'s own directory pick, duplicated here since `ao-render` cannot depend on the `aomac` binary crate:
/// `AOMAC_PREFS_DIR` first (agents' isolated prefs dirs), else `~/Library/Application Support/aomac`.
fn dir() -> Option<PathBuf> {
    if let Some(d) = std::env::var_os("AOMAC_PREFS_DIR") {
        return Some(d.into());
    }
    Some(PathBuf::from(std::env::var_os("HOME")?).join("Library/Application Support/aomac"))
}

fn path() -> Option<PathBuf> {
    Some(dir()?.join("Window.cfg"))
}

/// One save per line: `width height`, `x y` (blank line if no saved position yet), `fullscreen 0|1`, then an optional
/// `monitor <name>` line (the name is taken verbatim to the end of the line/file, since display names may contain spaces).
fn parse(text: &str) -> Option<Saved> {
    let mut lines = text.lines();
    let mut n = lines.next()?.split_whitespace().filter_map(|t| t.parse::<i64>().ok());
    let (w, h) = (n.next()?, n.next()?);
    if w <= 0 || h <= 0 {
        return None;
    }
    let pos = lines.next().and_then(|l| {
        let mut n = l.split_whitespace().filter_map(|t| t.parse::<i64>().ok());
        match (n.next(), n.next()) {
            (Some(x), Some(y)) => Some((x as i32, y as i32)),
            _ => None,
        }
    });
    let fullscreen = lines.next().is_some_and(|l| l.trim() == "fullscreen 1");
    let monitor = lines.next().and_then(|l| l.strip_prefix("monitor ")).map(str::to_string);
    Some(Saved { width: w as u32, height: h as u32, pos, monitor, fullscreen })
}

fn format_line(s: &Saved) -> String {
    let mut out = format!("{} {}\n", s.width, s.height);
    out += &s.pos.map_or_else(String::new, |(x, y)| format!("{x} {y}"));
    out += "\n";
    out += if s.fullscreen { "fullscreen 1\n" } else { "fullscreen 0\n" };
    if let Some(name) = &s.monitor {
        out += &format!("monitor {name}\n");
    }
    out
}

/// The last saved native window, if any valid save exists.
pub fn load() -> Option<Saved> {
    parse(&std::fs::read_to_string(path()?).ok()?)
}

/// Saves the current native window state for the next launch.
pub fn save(s: &Saved) {
    let Some(p) = path() else { return };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(mut f) = std::fs::File::create(&p) {
        let _ = f.write_all(format_line(s).as_bytes());
    }
}

/// Clamps `pos` so a `size`-sized window stays fully within a monitor at `monitor_pos`/`monitor_size` (pulls a window
/// saved on a now-missing/resized display back on screen, audit item 7). Pure geometry, no winit types, so it is
/// directly testable; `viewer.rs::State::new` supplies the real `MonitorHandle` fields.
pub fn clamp_to_monitor(pos: (i32, i32), size: (u32, u32), monitor_pos: (i32, i32), monitor_size: (u32, u32)) -> (i32, i32) {
    let max_x = monitor_pos.0 + monitor_size.0 as i32 - size.0.min(monitor_size.0) as i32;
    let max_y = monitor_pos.1 + monitor_size.1 as i32 - size.1.min(monitor_size.1) as i32;
    (pos.0.clamp(monitor_pos.0, max_x.max(monitor_pos.0)), pos.1.clamp(monitor_pos.1, max_y.max(monitor_pos.1)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_size_position_fullscreen_and_monitor_and_rejects_garbage() {
        let full = Saved { width: 1600, height: 900, pos: Some((40, 60)), monitor: Some("Built-in Retina Display".into()), fullscreen: true };
        assert_eq!(parse(&format_line(&full)), Some(full));
        let bare = Saved { width: 1280, height: 800, pos: None, monitor: None, fullscreen: false };
        assert_eq!(parse(&format_line(&bare)), Some(bare));
        assert_eq!(parse(""), None, "no save yet");
        assert_eq!(parse("not numbers"), None);
        assert_eq!(parse("0 800"), None, "a zero/negative dimension is never saved, never trusted either");
    }

    #[test]
    fn clamp_pulls_an_off_screen_window_fully_onto_a_smaller_or_relocated_monitor() {
        // window saved at (1800, 50) 1600x900 on a display that's gone; the new monitor is 1920x1080 at (0, 0)
        assert_eq!(clamp_to_monitor((1800, 50), (1600, 900), (0, 0), (1920, 1080)), (320, 50));
        // a monitor smaller than the window: pinned to its origin, never negative into a monitor to its left
        assert_eq!(clamp_to_monitor((-500, -500), (1600, 900), (100, 100), (800, 600)), (100, 100));
        // already fully on screen: untouched
        assert_eq!(clamp_to_monitor((50, 50), (800, 600), (0, 0), (1920, 1080)), (50, 50));
    }
}
