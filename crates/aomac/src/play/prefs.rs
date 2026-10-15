//! `~/Library/Application Support/aomac/prefs.txt`: `key=value` lines. Stands in for the original's `prefs/Prefs.xml`
//! `LauncherConfig` archive (docs/screens.md §3.3): remembered account names (never passwords), the selected account,
//! plus the `SelectedCharacter` row and the chosen server.

use std::path::PathBuf;

/// `IndependentPrefs` `CCSelectedBreed/Height/Size/Head/Profession` of the character creation (docs/screens.md §12).
#[derive(Clone, Default)]
pub struct CcPrefs {
    pub breed: i32,
    pub height: i32,
    pub size: i32,
    pub head: i32,
    pub profession: i32,
}

#[derive(Default)]
pub struct Prefs {
    pub cc: CcPrefs,
    /// `WasCharacterCreated`: the next creation starts from scratch.
    pub cc_created: bool,
    pub accounts: Vec<String>,
    pub selected_account: usize,
    pub selected_character: i32,
    pub server: String,
}

/// aomac's prefs directory (stand-in for the client's `prefs/`; also holds `CharacterViewer.xml`).
/// Empty environment values are unset; a nonempty override or HOME must be absolute, never cwd-relative.
pub fn dir() -> Option<PathBuf> {
    #[cfg(test)]
    if let Some(d) = TEST_DIR.with(|t| t.borrow().clone()) {
        return resolve_dir(Some(d.as_os_str()), None);
    }
    resolve_dir(std::env::var_os("AOMAC_PREFS_DIR").as_deref(), std::env::var_os("HOME").as_deref())
}

fn resolve_dir(override_dir: Option<&std::ffi::OsStr>, home: Option<&std::ffi::OsStr>) -> Option<PathBuf> {
    if let Some(d) = override_dir.filter(|d| !d.is_empty()) {
        let path = PathBuf::from(d);
        if path.is_absolute() {
            return Some(path);
        }
        eprintln!("prefs: AOMAC_PREFS_DIR must be absolute, got {}; preferences disabled", path.display());
        return None;
    }
    if let Some(h) = home.filter(|h| !h.is_empty()) {
        let path = PathBuf::from(h);
        if path.is_absolute() {
            return Some(path.join("Library/Application Support/aomac"));
        }
        eprintln!("prefs: HOME must be absolute, got {}; preferences disabled", path.display());
    } else {
        eprintln!("prefs: no absolute preferences directory: AOMAC_PREFS_DIR and HOME are unset or empty; preferences disabled");
    }
    None
}

#[cfg(test)]
thread_local! { static TEST_DIR: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) }; }

#[cfg(test)]
struct TestDir(PathBuf);

#[cfg(test)]
impl TestDir {
    fn new() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        loop {
            let path = std::env::temp_dir().join(format!("aomac-test-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
            match std::fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("cannot create test directory {}: {e}", path.display()),
            }
        }
    }
}

#[cfg(test)]
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(test)]
thread_local! { static TEST_SCRATCH: TestDir = TestDir::new(); }

/// Lazily created scratch directory unique to this process and test thread; removed when the thread exits.
/// Use [`set_test_dir`] to route preferences here without replacing an explicit override.
#[cfg(test)]
pub fn test_dir() -> PathBuf {
    TEST_SCRATCH.with(|d| d.0.clone())
}

/// Test scratch directory, isolated to the current test thread without mutating the process environment.
#[cfg(test)]
pub fn set_test_dir(d: impl Into<PathBuf>) {
    let d = d.into();
    TEST_DIR.with(|t| *t.borrow_mut() = Some(d));
}

/// Retail F12 screenshot folder and naming (RE: GUI.dll `FlowControlModule_t::SlotScreenshot` makes an unconditional
/// virtual call into `DisplaySystem.dll`'s backbuffer-capture routine with no explicit path, which falls back to
/// `BinaryIO.dll` `AnarchyPath_t::GetScreenshotsPath` = `%LOCALAPPDATA%\Funcom\Anarchy Online\Screenshots\`, creating
/// it if missing, then names the file `"%sscrn%04i.png"` against a process-lifetime counter that keeps incrementing
/// past any name already on disk, so it never overwrites). No chat feedback line exists anywhere in retail's
/// GUI/Gamecode/Interfaces/DisplaySystem modules for this command -- confirmed by `DumpRefs` across all four, the
/// only hits are the `#CameraScreenshot` command name and the `SlotScreenshot` symbol itself -- so aomac stays
/// silent too. aomac equivalent of the retail folder: the prefs app-support dir's `Screenshots` subfolder (not the
/// client install dir, which aomac never writes to).
#[cfg(not(test))]
static SCREENSHOT_COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
#[cfg(test)]
thread_local! { static SCREENSHOT_COUNTER: std::cell::Cell<u32> = const { std::cell::Cell::new(0) }; }

#[cfg(not(test))]
fn next_screenshot_index() -> u32 { SCREENSHOT_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed) }
#[cfg(test)]
fn next_screenshot_index() -> u32 { SCREENSHOT_COUNTER.with(|c| { let n = c.get(); c.set(n + 1); n }) }

/// Test-only: isolates the screenshot counter per thread like [`set_test_dir`] isolates the prefs dir.
#[cfg(test)]
pub fn reset_screenshot_counter() { SCREENSHOT_COUNTER.with(|c| c.set(0)); }

pub fn next_screenshot_path() -> Option<PathBuf> {
    let folder = dir()?.join("Screenshots");
    std::fs::create_dir_all(&folder).ok()?;
    loop {
        let path = folder.join(format!("scrn{:04}.png", next_screenshot_index()));
        if !path.exists() {
            return Some(path);
        }
    }
}

fn path() -> Option<PathBuf> {
    Some(dir()?.join("prefs.txt"))
}

impl Prefs {
    pub fn load() -> Self {
        let mut p = Prefs { selected_character: -1, cc: CcPrefs { height: 1, size: 1, ..Default::default() }, ..Default::default() };
        for line in path().and_then(|f| std::fs::read_to_string(f).ok()).unwrap_or_default().lines() {
            match line.split_once('=') {
                Some(("account", v)) if !v.is_empty() => p.accounts.push(v.into()),
                Some(("selected_account", v)) => p.selected_account = v.parse().unwrap_or(0),
                Some(("selected_character", v)) => p.selected_character = v.parse().unwrap_or(-1),
                Some(("server", v)) => p.server = v.into(),
                Some(("cc_breed", v)) => p.cc.breed = v.parse().unwrap_or(0),
                Some(("cc_height", v)) => p.cc.height = v.parse().unwrap_or(1),
                Some(("cc_size", v)) => p.cc.size = v.parse().unwrap_or(1),
                Some(("cc_head", v)) => p.cc.head = v.parse().unwrap_or(0),
                Some(("cc_profession", v)) => p.cc.profession = v.parse().unwrap_or(0),
                Some(("cc_created", v)) => p.cc_created = v == "1",
                _ => {}
            }
        }
        p
    }

    /// `SaveAccounts` equivalent: the selected account is the one in `current` (the name just logged in).
    pub fn remember(&mut self, current: &str) {
        if !self.accounts.iter().any(|a| a == current) {
            self.accounts.push(current.into());
        }
        self.selected_account = self.accounts.iter().position(|a| a == current).unwrap_or(0);
        self.save();
    }

    pub fn remove(&mut self, name: &str) {
        self.accounts.retain(|a| a != name);
        self.selected_account = 0;
        self.save();
    }

    pub fn save(&self) {
        let Some(f) = path() else { return };
        let mut text = String::new();
        for a in &self.accounts {
            text += &format!("account={}\n", a.replace('\n', ""));
        }
        text += &format!("selected_account={}\nselected_character={}\nserver={}\n", self.selected_account, self.selected_character, self.server.replace('\n', ""));
        let c = &self.cc;
        text += &format!("cc_breed={}\ncc_height={}\ncc_size={}\ncc_head={}\ncc_profession={}\ncc_created={}\n", c.breed, c.height, c.size, c.head, c.profession, u8::from(self.cc_created));
        if let Some(dir) = f.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Err(e) = std::fs::write(&f, text) {
            eprintln!("prefs: {}: {e}", f.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preference_bases_must_be_absolute_and_empty_values_are_unset() {
        use std::ffi::OsStr;
        let home = Some(OsStr::new("/isolated/home"));
        let expected = Some(PathBuf::from("/isolated/home/Library/Application Support/aomac"));
        assert_eq!(resolve_dir(None, home), expected);
        assert_eq!(resolve_dir(Some(OsStr::new("")), home), expected);
        assert_eq!(resolve_dir(Some(OsStr::new("/isolated/prefs")), None), Some(PathBuf::from("/isolated/prefs")));
        for unset in [None, Some(OsStr::new(""))] {
            assert_eq!(resolve_dir(None, unset), None);
            assert_eq!(resolve_dir(Some(OsStr::new("")), unset), None);
            assert_eq!(resolve_dir(Some(OsStr::new("/isolated/prefs")), unset), Some(PathBuf::from("/isolated/prefs")));
        }
        for relative in [".", "prefs", "~/prefs"] {
            assert_eq!(resolve_dir(Some(OsStr::new(relative)), home), None, "invalid override must not fall back to HOME");
            assert_eq!(resolve_dir(None, Some(OsStr::new(relative))), None);
        }
    }

    #[test]
    fn empty_override_saves_preferences_and_allocates_screenshots_under_home_not_cwd() {
        const CHILD: &str = "AOMAC_PREFS_SAVE_CHILD";
        if std::env::var_os(CHILD).is_some() {
            assert!(TEST_DIR.with(|t| t.borrow().is_none()), "exercise the real environment resolver");
            Prefs { server: "isolated-home-regression".into(), ..Default::default() }.save();
            let home = PathBuf::from(std::env::var_os("HOME").unwrap());
            let expected = home.join("Library/Application Support/aomac");
            assert_eq!(next_screenshot_path(), Some(expected.join("Screenshots/scrn0000.png")));
            assert!(std::fs::read_to_string(expected.join("prefs.txt")).unwrap().contains("server=isolated-home-regression\n"));
            return;
        }
        let root = test_dir();
        let cwd = root.join("cwd");
        let home = root.join("home");
        std::fs::create_dir(&cwd).unwrap();
        std::fs::create_dir(&home).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", std::thread::current().name().unwrap(), "--nocapture"])
            .current_dir(&cwd)
            .env(CHILD, "1")
            .env("AOMAC_PREFS_DIR", "")
            .env("HOME", &home)
            .output().unwrap();
        assert!(output.status.success(), "child failed: {output:?}");
        assert_eq!(std::fs::read_dir(&cwd).unwrap().count(), 0, "never write preferences or Screenshots in cwd");
        let expected = home.join("Library/Application Support/aomac");
        assert!(expected.join("prefs.txt").is_file());
        assert!(expected.join("Screenshots").is_dir());
    }

    #[test]
    fn parallel_test_directories_are_stable_and_cleaned_without_changing_native_environment() {
        let before = std::env::var_os("AOMAC_PREFS_DIR");
        let start = std::sync::Barrier::new(2);
        let paths = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..2).map(|_| {
                let before = &before;
                let start = &start;
                scope.spawn(move || {
                    let scratch = test_dir();
                    set_test_dir(&scratch);
                    std::fs::write(scratch.join("owned"), b"cleanup regression").unwrap();
                    start.wait();
                    assert_eq!(test_dir(), scratch);
                    assert_eq!(dir(), Some(scratch.clone()));
                    assert_eq!(std::env::var_os("AOMAC_PREFS_DIR"), *before);
                    scratch
                })
            }).collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect::<Vec<_>>()
        });
        assert_ne!(paths[0], paths[1]);
        assert!(paths.iter().all(|path| !path.exists()), "thread exit must remove its scratch directory and contents");
    }

    /// Naming/folder regression: a fresh folder gets created, names count up `scrn0000.png`, `scrn0001.png`, ...
    /// (retail's `"%sscrn%04i.png"`), and a name already on disk (from a previous run, matching retail's
    /// `stat64i32` check) is skipped rather than overwritten.
    #[test]
    fn screenshot_path_creates_the_folder_and_counts_up_skipping_existing_files() {
        let scratch = test_dir();
        set_test_dir(&scratch);
        reset_screenshot_counter();
        let first = next_screenshot_path().unwrap();
        assert_eq!(first, scratch.join("Screenshots/scrn0000.png"));
        assert!(scratch.join("Screenshots").is_dir(), "the Screenshots folder must be created, not assumed to exist");
        std::fs::write(&first, b"existing capture").unwrap(); // simulate the previous screenshot already on disk
        std::fs::write(scratch.join("Screenshots/scrn0001.png"), b"existing capture").unwrap();
        let next = next_screenshot_path().unwrap();
        assert_eq!(next, scratch.join("Screenshots/scrn0002.png"), "must skip past files already on disk, never overwrite");
    }
}

