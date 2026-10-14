//! Native macOS window size/position persistence (bug: the client forgot the window size between launches).
//!
//! Retail stores the chosen display resolution in its account-scoped `Login.cfg` (`IndependentPrefs_t` type 0 "login prefs",
//! `aomac/src/play/dvalue/indep.rs`), loaded by `Preferences_t::LoadLoginPrefs` before the character list but after the account
//! name is known. The native OS window this crate opens exists before any account is chosen (there is no login screen yet for
//! `IndependentPrefs_t` to key a file on), so this mirrors the same idea -- an aomac-own prefs file, outside the client install
//! (skill rule: never write inside the client dir) -- at `~/Library/Application Support/aomac/Window.cfg`: one line
//! `width height [x y]` (position is written once known; a first run omits it and the OS places the window).

use std::io::Write;
use std::path::PathBuf;

/// `(width, height, position)`.
type Saved = (u32, u32, Option<(i32, i32)>);

fn path() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join("Library/Application Support/aomac/Window.cfg"))
}

/// Parses the saved `width height [x y]` line; `None` for anything malformed or non-positive.
fn parse(text: &str) -> Option<Saved> {
    let mut n = text.split_whitespace().filter_map(|t| t.parse::<i64>().ok());
    let (w, h) = (n.next()?, n.next()?);
    if w <= 0 || h <= 0 {
        return None;
    }
    let pos = match (n.next(), n.next()) {
        (Some(x), Some(y)) => Some((x as i32, y as i32)),
        _ => None,
    };
    Some((w as u32, h as u32, pos))
}

fn format_line(width: u32, height: u32, pos: Option<(i32, i32)>) -> String {
    match pos {
        Some((x, y)) => format!("{width} {height} {x} {y}\n"),
        None => format!("{width} {height}\n"),
    }
}

/// `(width, height, position)` of the last saved native window, if any valid save exists.
pub fn load() -> Option<Saved> {
    parse(&std::fs::read_to_string(path()?).ok()?)
}

/// Saves the current native window size (and position, once known) for the next launch.
pub fn save(width: u32, height: u32, pos: Option<(i32, i32)>) {
    let Some(p) = path() else { return };
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(mut f) = std::fs::File::create(&p) {
        let _ = f.write_all(format_line(width, height, pos).as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_size_and_position_and_rejects_garbage() {
        assert_eq!(parse(&format_line(1600, 900, Some((40, 60)))), Some((1600, 900, Some((40, 60)))));
        assert_eq!(parse(&format_line(1280, 800, None)), Some((1280, 800, None)));
        assert_eq!(parse(""), None, "no save yet");
        assert_eq!(parse("not numbers"), None);
        assert_eq!(parse("0 800"), None, "a zero/negative dimension is never saved, never trusted either");
        assert_eq!(parse("1280 800 extra garbage"), Some((1280, 800, None)), "malformed position is just dropped, not fatal");
    }
}
