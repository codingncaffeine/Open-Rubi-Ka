//! NotificationWindow_c GUI 0x1000bd86; AddText 0x1000bc49, links 0x1000b9e5.
//! Visibility controls chrome, not the notification stream (0x1000b683).
use super::{dvalue::{element_xml, DValues, Variant, CAT_CHAR}, hud_wincfg::{Store, WinCfgs}};
use ao_formats::screens::{TextDb, CAT_GUI};
use ao_gui::{xml::{self, Element}, Event, Gui, MenuItem, ViewHandle, WindowId, WindowSize};
use std::{collections::VecDeque, path::Path};

const MENU: u32 = 0x7a00;
const XML: &str = "<root><View name=\"notifications\" view_layout=\"vertical\"><VLayoutSpacer min_size=\"0\" max_size=\"16000\" weight=\"1\"/></View></root>";
struct Line { name: String, handle: ViewHandle, age: f32 }
pub(super) struct Notifications {
    window: Option<WindowId>,
    frame: bool,
    clickable: bool,
    lines: VecDeque<Line>,
    next: u64,
    texts: TextDb,
    urls: Vec<String>,
}
fn permitted_url(href: &str) -> bool {
    ["itemref://", "charref://", "text://"].iter().any(|prefix| href.get(..prefix.len()).is_some_and(|s| s.eq_ignore_ascii_case(prefix)))
}
impl Notifications {
    pub fn new(dir: &Path) -> anyhow::Result<Self> {
        Ok(Self { window: None, frame: true, clickable: true, lines: VecDeque::new(), next: 0, texts: TextDb::load(dir)?, urls: vec![] })
    }
    pub fn update(&mut self, gui: &mut Gui, d: &mut DValues, cfg: &mut WinCfgs, screen: (u32, u32), dt: f32) {
        if !d.flag("notification_window") {
            if let Some(w) = self.window.take() { cfg.detach_hud(gui, d, w); gui.close_window(w); self.lines.clear(); }
            return;
        }
        if self.window.is_none() {
            self.clickable = match d.get("NotificationWindowConfig") {
                Some(Variant::Archive(text)) => xml::parse(text).ok().and_then(|root| root.children.iter().find(|c| c.attr("name") == Some("ClickableLinks")).and_then(|c| c.attr("value")).map(|v| v == "true")).unwrap_or(true),
                _ => true,
            };
            match gui.open_tabbed_window_xml("NotificationWindow", "Notifications", XML, (0, 0), WindowSize::Fixed(screen.0.saturating_sub(10), (screen.1 / 3).saturating_sub(31))) {
                Ok(w) => {
                    gui.set_window_frame(w, true, true);
                    gui.set_window_context(w, true);
                    cfg.attach_hud(gui, d, w, Store::Value("NotificationWindowConfig"), screen);
                    self.window = Some(w);
                    self.frame = true;
                }
                Err(e) => { eprintln!("hud: notifications: {e:#}"); return; }
            }
        }
        let w = self.window.expect("opened");
        let frame = d.flag("NotificationWindowVisibility");
        if frame != self.frame {
            let outer = gui.window_outer_frame(w);
            gui.set_window_dock_frame(w, frame.then_some("Notifications"));
            if let Some(outer) = outer { gui.set_window_outer_frame(w, outer); }
            self.frame = frame;
        }
        for line in &mut self.lines {
            line.age += dt;
            gui.set_view_alpha(w, &line.name, ((8.0 - line.age) / 3.0).clamp(0.0, 1.0));
        }
        while self.lines.front().is_some_and(|line| line.age >= 8.0) {
            let line = self.lines.pop_front().expect("nonempty");
            gui.remove_view_in(line.handle, &line.name);
        }
    }
    /// Native GlobalSignals+0x298 slot; no retail emitter found in GUI/Gamecode/Interfaces.
    #[allow(dead_code)]
    pub fn add_text(&mut self, gui: &mut Gui, text: &str) {
        let Some(w) = self.window else { return };
        let name = format!("notification{}", self.next);
        self.next = self.next.wrapping_add(1);
        let xml = format!("<root><TextView name=\"{name}\" font=\"CC17\" value=\"{}\"/></root>", super::hud_listview::esc(text));
        let Ok(handle) = gui.add_view_xml(w, "notifications", &name, &xml) else { return };
        self.lines.push_back(Line { name, handle, age: 0.0 });
        // 0x1000bc49 deletes the oldest message when preferred content exceeds the client.
        while self.lines.len() > 1 {
            let Some(last) = self.lines.back().and_then(|line| gui.view_frame(w, &line.name)) else { break };
            if last.b < gui.window_size(w).1 as f32 { break; }
            let line = self.lines.pop_front().expect("nonempty");
            gui.remove_view_in(line.handle, &line.name);
        }
    }
    pub fn event(&mut self, gui: &mut Gui, d: &mut DValues, ev: &Event, screen: (u32, u32)) -> bool {
        let Some(w) = self.window else { return false };
        match ev {
            Event::CloseRequested { window } if *window == w => { d.set_i64("NotificationWindowVisibility", 0); true }
            Event::LinkClicked { window, href, .. } if *window == w => { if self.clickable && permitted_url(href) { self.urls.push(href.clone()); } true }
            Event::FrameIcon { window, x, y } if *window == w => {
                let label = |key| self.texts.by_key(CAT_GUI, key).unwrap_or_default();
                gui.open_menu((*x, *y), (screen.0 as i32, screen.1 as i32), vec![MenuItem::entry(MENU, &label("ResetPosition")), MenuItem::entry(MENU + 1, &label("HideFrame")), MenuItem::check(MENU + 2, &label("ClickableLinks"), self.clickable)]);
                true
            }
            Event::MenuPicked { id } if *id >= MENU && *id <= MENU + 2 => {
                match *id - MENU {
                    0 => gui.set_window_outer_frame(w, (0, 0, screen.0, screen.1 / 3)),
                    1 => d.set_i64("NotificationWindowVisibility", 0),
                    _ => { self.clickable = !self.clickable; self.save_clickable(d); }
                }
                true
            }
            _ => false,
        }
    }
    pub fn take_urls(&mut self) -> Vec<String> { std::mem::take(&mut self.urls) }
    fn save_clickable(&self, d: &mut DValues) {
        let name = "NotificationWindowConfig";
        let mut root = match d.get(name) { Some(Variant::Archive(text)) => xml::parse(text).ok(), _ => None }
            .unwrap_or_else(|| Element { name: "Archive".into(), attrs: vec![("name".into(), name.into()), ("code".into(), "0".into())], children: vec![] });
        root.children.retain(|c| c.attr("name") != Some("ClickableLinks"));
        root.children.push(Element { name: "Bool".into(), attrs: vec![("name".into(), "ClickableLinks".into()), ("value".into(), self.clickable.to_string())], children: vec![] });
        let value = Variant::Archive(element_xml(&root));
        if d.exists(name) { d.set(name, value); } else { d.add(name, value, true, CAT_CHAR, None, None, false); }
    }
    pub fn close(&mut self, gui: &mut Gui) { if let Some(w) = self.window.take() { gui.close_window(w); } }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn notification_links_use_only_the_retail_info_schemes() {
        assert!(permitted_url("ITEMREF://1/2/3"));
        assert!(permitted_url("charref://50000/7"));
        assert!(permitted_url("text://help"));
        assert!(!permitted_url("file://help"));
        assert!(!permitted_url("script://attack"));
    }
    #[test]
    fn visibility_changes_chrome_without_dropping_native_notifications() {
        let dir = ao_gui::client_dir();
        if !dir.join("cd_image/gui").exists() { return; }
        let mut gui = Gui::new(&dir, None).unwrap();
        let mut d = DValues::new(&dir);
        let mut cfg = WinCfgs::new(&dir);
        let mut notices = Notifications::new(&dir).unwrap();
        d.set_i64("notification_window", 1);
        d.set_i64("NotificationWindowVisibility", 1);
        notices.update(&mut gui, &mut d, &mut cfg, (1280, 800), 0.0);
        notices.add_text(&mut gui, "<a href=\"text://test\">Native text</a>");
        let window = notices.window.unwrap();
        let frame = gui.window_outer_frame(window).unwrap();
        d.set_i64("NotificationWindowVisibility", 0);
        notices.update(&mut gui, &mut d, &mut cfg, (1280, 800), 1.5);
        assert!(!notices.frame);
        assert_eq!(gui.window_outer_frame(window).unwrap(), frame);
        assert_eq!(notices.lines.len(), 1);
        let line = &notices.lines[0];
        assert_eq!(gui.view_alpha(window, &line.name), Some(1.0));
        assert!(gui.text(window, &line.name).contains("<a href="));
        notices.update(&mut gui, &mut d, &mut cfg, (1280, 800), 5.0);
        assert_eq!(gui.view_alpha(window, &notices.lines[0].name), Some(0.5));
        notices.update(&mut gui, &mut d, &mut cfg, (1280, 800), 1.5);
        assert!(notices.lines.is_empty());
        d.set_i64("notification_window", 0);
        notices.update(&mut gui, &mut d, &mut cfg, (1280, 800), 0.0);
        assert!(notices.window.is_none());
    }
}
