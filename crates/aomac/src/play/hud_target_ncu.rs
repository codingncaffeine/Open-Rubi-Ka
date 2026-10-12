//! TargetNCUView_c (GUI 0x100dd171), target routing 0x100dcda0.
use super::{dvalue::{element_xml, DValues, Variant, CAT_CHAR}, hud_listview::{self as lv, Column, Hit, ListView, ListWindow, MenuTexts, Mode, Row, Spec}, hud_nanodb::NanoDb, hud_wincfg::{ListCfg, Store, WinCfgs}, zone::Zone};
use ao_formats::screens::{TextDb, CAT_GUI};
use ao_gui::{xml::{self, Element}, Event, Gui, InputEvent, MenuItem, Modifiers, MouseButton};
use std::path::Path;

const KEYS: [&str; 2] = ["ShowNanoTargetNCU", "ShowFightTargetNCU"];
const CONFIGS: [&str; 2] = ["NanoTargetNCUWindowConfig", "FightTargetNCUWindowConfig"];
const TITLES: [&str; 2] = ["Nano Target NCU", "Fight Target NCU"];

pub(super) struct TargetNcu {
    db: NanoDb,
    texts: TextDb,
    windows: [Option<ListWindow>; 2],
    built: [Option<(i32, u32, i64, Mode, usize)>; 2],
    clock: f32,
    targets: [Option<i32>; 2],
    urls: Vec<String>,
    remove: Option<i32>,
    context: Option<i32>,
    borderless: [bool; 2],
    alt_right: bool,
}
impl TargetNcu {
    pub fn new(dir: &Path) -> anyhow::Result<Self> {
        Ok(Self { db: NanoDb::new(dir), texts: TextDb::load(dir)?, windows: [None, None], built: [None, None], clock: 0.0, targets: [None, None], urls: vec![], remove: None, context: None, borderless: [false; 2], alt_right: false })
    }
    pub fn update(&mut self, gui: &mut Gui, d: &mut DValues, cfg: &mut WinCfgs, zone: &Zone, screen: (u32, u32), dt: f32) {
        self.clock += dt;
        for i in 0..2 {
            let enabled = d.flag("cc_section1") && d.flag(KEYS[i]);
            if !enabled {
                if let Some(w) = &self.windows[i] { gui.set_window_visible(w.win, false); }
                continue;
            }
            if self.windows[i].is_none() {
                let columns: Vec<_> = [(1, "Tab_Name", 200.0), (12, "Tab_Remaning", 55.0)].into_iter().map(|(id, key, width)| Column { id, label: self.texts.by_key(CAT_GUI, key).unwrap_or_default(), width }).collect();
                let mut view = ListView::new(Mode::Grid, columns.clone(), 3, 0, (12, false));
                view.spacing = 5.0;
                view.icon_height = Some(24.0);
                if let Some(Variant::Archive(text)) = d.get(CONFIGS[i]) {
                    if let Ok(root) = xml::parse(text) {
                        self.borderless[i] = root.children.iter().any(|c| c.attr("name") == Some("borderless") && c.attr("value") == Some("true"));
                        if let Some(list) = root.children.iter().find(|c| c.attr("name") == Some("listview_config")) {
                            let saved = ListCfg::parse(list);
                            if let Some(list) = saved.list { view.mode = if list { Mode::List } else { Mode::Grid }; }
                            if let Some((column, order)) = saved.list_sort { view.sort = (column as u32, order != 0); }
                            if !saved.columns.is_empty() {
                                view.columns = saved.columns.iter().enumerate().filter_map(|(index, &(id, width))| {
                                    if saved.col_flags.get(index).copied().unwrap_or(0) & 1 != 0 { return None; }
                                    let mut column = columns.iter().find(|c| c.id == id as u32)?.clone();
                                    column.width = width;
                                    Some(column)
                                }).collect();
                            }
                        }
                    }
                }
                let texts = MenuTexts { list_mode: self.texts.by_key(CAT_GUI, "ListMode").unwrap_or_default(), auto_arrange: self.texts.by_key(CAT_GUI, "AutoArrange").unwrap_or_default() };
                match ListWindow::open(gui, Spec { name: "target_ncu_window", title: TITLES[i], pos: ((screen.0 as i32 - 167) / 2, (screen.1 as i32 - 112) / 2), client: (157, 81), top_xml: "", top_h: 0.0, texts, menu_base: 0x7800 + i as u32 * 0x100, screen }, view, columns, None) {
                    Ok(w) => {
                        gui.set_window_frame(w.win, true, true);
                        if self.borderless[i] { set_borderless(gui, &w, TITLES[i], true); }
                        cfg.attach_hud(gui, d, w.win, Store::Value(CONFIGS[i]), screen);
                        self.windows[i] = Some(w);
                    }
                    Err(e) => { eprintln!("hud: target NCU: {e:#}"); continue; }
                }
            }
            gui.set_window_visible(self.windows[i].as_ref().expect("opened").win, true);
            let target = if i == 0 { zone.selected_target().filter(|t| t.kind == 50000).map(|t| t.instance) } else { zone.fight_target.get(&(zone.char_id as i32)).copied() };
            self.targets[i] = target;
            let nanos = target.and_then(|id| if id == zone.char_id as i32 { Some(&zone.nanos) } else { zone.world.target_nanos(id) });
            let w = self.windows[i].as_mut().expect("opened");
            w.screen = screen;
            let client = gui.window_size(w.win);
            if gui.view_frame(w.win, "lv_scroll").is_none_or(|rect| (rect.width() + 1.0) as u32 != client.0 || (rect.height() + 1.0) as u32 != client.1) {
                gui.set_view_pref_size(w.win, "lv_scroll", (client.0 as f32, client.1 as f32), (client.0 as f32, client.1 as f32));
            }
            w.view.cols = ((client.0.saturating_sub(12) + 5) / 45).max(1) as usize;
            gui.set_visible(w.win, "lv_scroll", target.is_some());
            let sig = (target.unwrap_or(0), nanos.map_or(0, |n| n.serial), nanos.map_or(0, |n| n.time as i64), w.view.mode, w.view.cols);
            if self.built[i] == Some(sig) { continue; }
            let mut rows = Vec::new();
            if let Some(nanos) = nanos {
                for b in &nanos.buffs {
                    let Some(info) = self.db.info(gui, b.nano).cloned() else { continue };
                    let remaining = lv::hms(b.remaining_cs(nanos.time));
                    let cells = w.view.columns.iter().map(|c| match c.id { 1 => info.name.clone(), 12 => remaining.clone(), _ => String::new() }).collect();
                    rows.push(Row { key: b.nano, icon: info.icon, cells, tip_title: info.name.clone(), tip_body: format!("<br>NCU usage:&nbsp;{}<br>Remaining:&nbsp;{}", info.stat(super::hud_nanodb::stat::NCU).unwrap_or(0), remaining) });
                }
            }
            w.view.set_rows(rows);
            w.view.paint(gui, w.win, 40.0);
            self.built[i] = Some(sig);
        }
    }
    pub fn event(&mut self, gui: &mut Gui, d: &mut DValues, ev: &Event, own: i32) -> bool {
        for i in 0..2 {
            let Some(w) = self.windows[i].as_mut() else { continue };
            match ev {
                Event::CanvasPress { window, button: MouseButton::Right, .. } if *window == w.win && self.alt_right => {
                    self.borderless[i] = !self.borderless[i];
                    set_borderless(gui, w, TITLES[i], self.borderless[i]);
                    save_list(d, CONFIGS[i], w, self.borderless[i]);
                    return true;
                }
                Event::MenuPicked { id: 0x7b00 } => {
                    if let Some(nano) = self.context.take() { self.urls.push(format!("itemid://53019/{nano}")); }
                    return true;
                }
                Event::CloseRequested { window } if *window == w.win => { d.set_i64(KEYS[i], 0); return true; }
                Event::FrameIcon { window, x, y } if *window == w.win => { w.open_menu(gui, (*x, *y)); return true; }
                Event::MenuPicked { id } if w.owns_menu(*id) => { w.picked(*id); save_list(d, CONFIGS[i], w, self.borderless[i]); self.built[i] = None; return true; }
                _ => {}
            }
            match w.view.event(gui, w.win, ev, self.clock, 40.0) {
                Some(Hit::Double(nano)) if self.targets[i] == Some(own) => { self.remove = Some(nano); return true; }
                Some(Hit::Context(nano, at)) => {
                    if self.alt_right {
                        self.borderless[i] = !self.borderless[i];
                        set_borderless(gui, w, TITLES[i], self.borderless[i]);
                        save_list(d, CONFIGS[i], w, self.borderless[i]);
                        return true;
                    }
                    self.context = Some(nano);
                    gui.open_menu(at, (w.screen.0 as i32, w.screen.1 as i32), vec![MenuItem::entry(0x7b00, &self.texts.by_key(CAT_GUI, "LookAtNano").unwrap_or_default())]);
                    return true;
                }
                Some(_) => return true,
                None => {}
            }
        }
        false
    }
    /// Alt + right down changes style without changing the outer frame (0x100dcefb/0x100dcc61).
    pub fn input(&mut self, gui: &Gui, ev: &InputEvent, mods: Modifiers) {
        if matches!(ev, InputEvent::MouseDown { button: MouseButton::Right, .. }) {
            self.alt_right = mods.alt && !gui.interacting();
        }
    }
    pub fn take_urls(&mut self) -> Vec<String> { std::mem::take(&mut self.urls) }
    pub fn take_remove(&mut self) -> Option<i32> { self.remove.take() }
    pub fn close(&mut self, gui: &mut Gui) {
        for w in &mut self.windows { if let Some(w) = w.take() { w.close(gui); } }
    }
}

fn save_list(d: &mut DValues, name: &str, w: &ListWindow, borderless: bool) {
    let mut root = match d.get(name) {
        Some(Variant::Archive(text)) => xml::parse(text).ok(),
        _ => None,
    }.unwrap_or_else(|| Element { name: "Archive".into(), attrs: vec![("name".into(), name.into()), ("code".into(), "0".into())], children: vec![] });
    let index = root.children.iter().position(|c| c.attr("name") == Some("listview_config")).unwrap_or_else(|| {
        root.children.push(Element { name: "Archive".into(), attrs: vec![("name".into(), "listview_config".into()), ("code".into(), "0".into())], children: vec![] });
        root.children.len() - 1
    });
    ListCfg { list: Some(w.view.mode == Mode::List), columns: w.all.iter().map(|c| (c.id as i32, w.view.columns.iter().find(|v| v.id == c.id).map_or(c.width, |v| v.width))).collect(), col_flags: w.all.iter().map(|c| u32::from(!w.view.columns.iter().any(|v| v.id == c.id))).collect(), list_sort: Some((w.view.sort.0 as i32, i32::from(w.view.sort.1))), grid_sort: None }.write_into(&mut root.children[index]);
    root.children.retain(|c| c.attr("name") != Some("borderless"));
    root.children.push(Element { name: "Bool".into(), attrs: vec![("name".into(), "borderless".into()), ("value".into(), borderless.to_string())], children: vec![] });
    let value = Variant::Archive(element_xml(&root));
    if d.exists(name) { d.set(name, value); } else { d.add(name, value, true, CAT_CHAR, None, None, false); }
}

fn set_borderless(gui: &mut Gui, w: &ListWindow, title: &str, borderless: bool) {
    let outer = gui.window_outer_frame(w.win);
    gui.set_window_dock_frame(w.win, (!borderless).then_some(title));
    if let Some(outer) = outer { gui.set_window_outer_frame(w.win, outer); }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn target_ncu_preferences_select_real_target_buff_lists() {
        let dir = ao_gui::client_dir();
        if !dir.join("cd_image/gui").exists() { return; }
        let mut gui = Gui::new(&dir, None).unwrap();
        let mut d = DValues::new(&dir);
        let mut cfg = WinCfgs::new(&dir);
        let mut view = TargetNcu::new(&dir).unwrap();
        let mut zone = Zone::new(7);
        zone.target = Some(7);
        zone.nanos.buffs.push(super::super::own_nanos::Buff { nano: 163449, total_cs: 10000, ..Default::default() });
        d.set_i64("cc_section1", 1);
        d.set_i64(KEYS[0], 1);
        d.set_i64(KEYS[1], 1);
        view.update(&mut gui, &mut d, &mut cfg, &zone, (1280, 800), 0.0);
        let nano = view.windows[0].as_ref().unwrap();
        let fight = view.windows[1].as_ref().unwrap();
        assert_ne!(nano.win, fight.win);
        assert_eq!(view.targets, [Some(7), None]);
        assert!(nano.view.rows().iter().any(|row| row.key == 163449));
        assert!(fight.view.rows().is_empty());
        let saved = nano.win;
        d.set_i64(KEYS[0], 0);
        view.update(&mut gui, &mut d, &mut cfg, &zone, (1280, 800), 0.0);
        assert!(!gui.window_visible(saved));
        d.set_i64(KEYS[0], 1);
        view.update(&mut gui, &mut d, &mut cfg, &zone, (1280, 800), 0.0);
        assert_eq!(view.windows[0].as_ref().unwrap().win, saved);
        assert!(gui.window_visible(saved));
        zone.tick(15.0);
        view.update(&mut gui, &mut d, &mut cfg, &zone, (1280, 800), 0.0);
        let nano = view.windows[0].as_ref().unwrap();
        assert_eq!(nano.view.rows().iter().find(|row| row.key == 163449).unwrap().cells[1], lv::hms(8500));
        let frame = gui.window_outer_frame(saved).unwrap();
        view.input(&gui, &InputEvent::MouseDown { x: (frame.0 + 20) as f32, y: (frame.1 + 40) as f32, button: MouseButton::Right }, Modifiers { alt: true, ..Default::default() });
        view.event(&mut gui, &mut d, &Event::CanvasPress { window: saved, view: "grid".into(), x: 10.0, y: 10.0, button: MouseButton::Right, clicks: 1 }, 7);
        assert!(view.borderless[0]);
        assert_eq!(gui.window_outer_frame(saved).unwrap(), frame);
        assert!(matches!(d.get(CONFIGS[0]), Some(Variant::Archive(text)) if text.contains("borderless") && text.contains("true")));
    }
}
