//! GroupControlWindow_c / GroupControlView_c / ChatWindowOptionsView_c (GUI
//! 100ad888 / 100ad160 / 100a16bc). Configuration is character-local.
use super::*;
use crate::play::dvalue::{DValues, Variant};
use ao_gui::view::ListItem;

const FOLDERS: [&str; 3] = ["PublicChannels", "PrivateChannels", "CombatMessages"];
const FOLDER_KEYS: [&str; 3] = ["is_normal_groups_folder_open", "is_private_groups_folder_open", "is_combat_groups_folder_open"];

#[derive(Default)]
pub(super) struct ConfigWindow {
    pub(super) window: Option<WindowId>,
    pub(super) open_request: Option<bool>,
    pub(super) selected: String,
    folders: [bool; 3],
    scroll: f32,
    frame: Option<[f32; 4]>,
    escape: bool,
    close: bool,
    hide_other: bool,
    rows: Vec<(u64, String, bool, bool, bool)>,
    documents: Vec<(String, String)>,
    own_id: u32,
    menu_doc: Option<usize>,
    menu_group: Option<u64>,
    pending: Vec<WinOut>,
    archive_dirty: bool,
}

pub(super) fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
}

impl ConfigWindow {
    fn load(&mut self, d: &DValues) {
        let Some(Variant::Archive(src)) = d.get("ChatGroupWindowConfig") else { return };
        let Ok(root) = xml::parse(src) else { return };
        for e in &root.children {
            let Some(value) = e.attr("value") else { continue };
            match e.attr("name").unwrap_or_default() {
                "selected_chat_window" => self.selected = value.trim_matches('"').to_owned(),
                "group_list_scroll_offset" => self.scroll = value.parse().unwrap_or(0.0),
                "WindowFrame" => {
                    let v: Vec<f32> = value.trim_start_matches("Rect(").trim_end_matches(')').split(',').filter_map(|s| s.trim().parse().ok()).collect();
                    if v.len() == 4 { self.frame = Some([v[0], v[1], v[2], v[3]]); }
                }
                name => {
                    if let Some(i) = FOLDER_KEYS.iter().position(|k| *k == name) { self.folders[i] = value == "true"; }
                }
            }
        }
    }

    fn archive(&self) -> String {
        let mut s = format!("<Archive name=\"ChatGroupWindowConfig\" code=\"0\"><String name=\"selected_chat_window\" value=\"&quot;{}&quot;\"/>", escape(&self.selected));
        for (key, open) in FOLDER_KEYS.iter().zip(self.folders) { let _ = write!(s, "<Bool name=\"{key}\" value=\"{open}\"/>"); }
        let _ = write!(s, "<Float name=\"group_list_scroll_offset\" value=\"{}\"/>", self.scroll);
        if let Some([l,t,r,b]) = self.frame { let _ = write!(s, "<Rect name=\"WindowFrame\" value=\"Rect({l},{t},{r},{b})\"/>"); }
        s + "</Archive>"
    }
}

impl ChatWindows {
    pub(crate) fn config_own_id(&mut self, id: u32) { self.config.own_id = id; }
    pub(super) fn config_clear_menu(&mut self) { self.config.menu_doc = None; self.config.menu_group = None; }

    pub(super) fn config_take_outputs(&mut self) -> Vec<WinOut> { std::mem::take(&mut self.config.pending) }

    fn config_show_document(&mut self, gui: &mut Gui, doc: usize, open: bool) {
        let fi = self.wins[doc].frame;
        for &d in &self.frames[fi].docs { self.wins[d].cfg.open = open; }
        gui.set_window_visible(self.frames[fi].id, open);
        if open { if let Some(index) = self.frames[fi].docs.iter().position(|&d|d == doc) { self.select_tab(gui,fi,index); } }
        self.dirty = true;
    }

    // GUI `100a1074`: the configuration list's window context menu.
    fn config_window_menu(&mut self, gui: &mut Gui, doc: usize, x: i32, y: i32) {
        use ao_gui::MenuItem;
        let c = &self.wins[doc].cfg;
        let fi = self.wins[doc].frame;
        let style_allowed = self.frames[fi].docs.len() == 1 || self.frames[fi].docs[0] != doc;
        let items = vec![
            MenuItem { enabled: style_allowed, ..MenuItem::submenu(&self.label("WindowStyle"), vec![
                MenuItem::check(100 << 16,&self.label("StyleNormal"),c.visual_mode == 0),
                MenuItem::check(100 << 16 | 2,&self.label("StyleBorderless"),c.visual_mode == 2),
            ]) },
            MenuItem::separator(),
            MenuItem::check(101 << 16,&self.label("ShowChatWindow"),c.open),
            MenuItem::check(102 << 16,&self.label("ShowLogWndow"),self.config_logs.contains_key(&c.window_name)),
            MenuItem::check(103 << 16,&self.label("LogMessages"),c.logged),
            MenuItem::separator(),
            MenuItem::entry(104 << 16,&self.label("RenameWindow")),
            MenuItem::entry(105 << 16,&self.label("RemoveWindow")),
        ];
        self.menu = None;
        self.config.menu_group = None;
        self.config.menu_doc = Some(doc);
        gui.open_menu((x,y), (self.screen.0 as i32,self.screen.1 as i32), items);
    }

    // GUI `100a0dbf`: rights derive from private-group owner identity and native active byte +54.
    fn config_group_menu(&mut self, gui: &mut Gui, group: u64, x: i32, y: i32) {
        use ao_gui::MenuItem;
        let private = group >> 32 == 0xe && !self.inactive_groups.contains(&group);
        let owner = group as u32 == self.config.own_id;
        let items = vec![
            MenuItem { enabled: private && owner, ..MenuItem::entry(106 << 16,&self.label("DisbandPrivateGroup").replace("%s",&self.group_name(group))) },
            MenuItem { enabled: private && !owner, ..MenuItem::entry(107 << 16,&self.label("LeavePrivateGroup").replace("%s",&self.group_name(group))) },
        ];
        self.menu = None;
        self.config.menu_doc = None;
        self.config.menu_group = Some(group);
        gui.open_menu((x,y), (self.screen.0 as i32,self.screen.1 as i32), items);
    }

    fn config_menu_picked(&mut self, gui: &mut Gui, id: u32, arg: u32) {
        if let Some(doc) = self.config.menu_doc.take() {
            match id {
                100 => {
                    let fi = self.wins[doc].frame;
                    self.menu = Some(menu::Ctx { frame: fi, groups: vec![], user: None });
                    let _ = self.menu_picked(gui, menu::OP_MODE << 16 | arg);
                }
                101 => self.config_show_document(gui,doc,!self.wins[doc].cfg.open),
                102 => self.config_operation(gui,doc,"ShowMessageLog"),
                103 => self.config_operation(gui,doc,"LogMessages"),
                104 => self.config_operation(gui,doc,"RenameWindow"),
                105 => self.config_operation(gui,doc,"RemoveWindow"),
                _ => {}
            }
        } else if let Some(group) = self.config.menu_group.take() {
            if group >> 32 == 0xe && !self.inactive_groups.contains(&group) {
                match id {
                    106 if group as u32 == self.config.own_id => self.config.pending.push(WinOut::DisbandGroup),
                    107 if group as u32 != self.config.own_id => self.config.pending.push(WinOut::LeaveGroup(group as u32)),
                    _ => {}
                }
            }
        }
    }

    pub fn sync_config(&mut self, gui: &mut Gui, d: &mut DValues) {
        if let Some(open) = self.config.open_request.take() { d.set_i64("chat_group_window", i64::from(open)); }
        if let Some(win) = self.config.window { self.capture_config(gui,win); }
        if self.config.archive_dirty {
            self.config.archive_dirty = false;
            d.set("ChatGroupWindowConfig", Variant::Archive(self.config.archive()));
        }
        if self.config.close {
            self.config.close = false;
            d.set_i64("chat_group_window", 0);
        }
        if !d.flag("chat_group_window") {
            if let Some(win) = self.config.window.take() {
                self.capture_config(gui, win);
                self.config_clear_menu();
                gui.close_menu();
                gui.close_window(win);
                d.set("ChatGroupWindowConfig", Variant::Archive(self.config.archive()));
                self.config.archive_dirty = false;
            }
            return;
        }
        if self.config.window.is_none() {
            self.config.load(d);
            self.config.escape = d.get_i64("esc_chatconfig").unwrap_or(1) != 0;
            let labels = |k: &str| escape(&self.label(k));
            let mut src = String::from("<View view_layout=\"horizontal\"><View layout_borders=\"Rect(5,5,5,5)\"><TextView value=\"Chat Windows\" layout_borders=\"Rect(5,5,5,5)\"/><BorderView><ScrollView name=\"windows_scroll\"><StringListView name=\"windows\"/></ScrollView></BorderView></View><View layout_borders=\"Rect(0,5,5,5)\">");
            let _ = write!(src, "<TextView value=\"{}\" layout_borders=\"Rect(5,5,5,5)\"/><BorderView><ScrollView name=\"groups_scroll\" min_size=\"Point(-1,50)\"><StringListView name=\"groups\"/></ScrollView></BorderView><CheckBox name=\"HideChannelsInOtherWnd\" label=\"{}\" layout_borders=\"Rect(5,5,5,0)\"/><BorderView layout_borders=\"Rect(0,5,0,0)\"><View>", labels("ChatChannels"), labels("HideChannelsInOtherWnd"));
            src += "<View view_layout=\"horizontal\"><View>";
            for key in ["NewWindow", "RenameWindow"] { let _ = write!(src, "<Button name=\"{key}\" label=\"{}\" width_group=\"Buttons\" layout_borders=\"Rect(5,5,0,{})\"/>", labels(key), if key == "RenameWindow" { 5 } else { 0 }); }
            src += "</View><HLayoutSpacer/><View>";
            for key in ["CloneWindow", "RemoveWindow"] { let _ = write!(src, "<Button name=\"{key}\" label=\"{}\" width_group=\"Buttons\" layout_borders=\"Rect(5,5,5,0)\"/>", labels(key)); }
            src += "</View></View>";
            src += "</View></BorderView></View></View>";
            let outer = self.config.frame.map(|r| place(Some(r), self.screen)).unwrap_or(((self.screen.0 as i32 - 500)/2, (self.screen.1 as i32 - 300)/2, 500, 300));
            match gui.open_tabbed_window_xml("ChatGroupWindow", &self.label("ChatConfig"), &src, (outer.0,outer.1), WindowSize::Fixed(outer.2,outer.3)) {
                Ok(win) => { self.config.window = Some(win); gui.list_set_multi(win,"groups",true); self.config.documents.clear(); self.config.rows.clear(); },
                Err(e) => { eprintln!("chat configuration: {e:#}"); return; }
            }
        }
        self.refresh_config(gui);
    }

    fn selected_config_doc(&self) -> Option<usize> {
        self.wins.iter().position(|w| w.cfg.window_name == self.config.selected)
    }

    fn capture_config(&mut self, gui: &Gui, win: WindowId) {
        let previous = (self.config.scroll,self.config.frame,self.config.folders);
        self.config.scroll = gui.scroll_offset(win,"groups_scroll");
        if let Some((x,y,w,h)) = gui.window_outer_frame(win) { self.config.frame = Some([x as f32,y as f32,(x+w as i32-1) as f32,(y+h as i32-1) as f32]); }
        for (i,key) in FOLDERS.iter().enumerate() { if let Some(row) = gui.list_item(win,"groups",key) { self.config.folders[i] = row.open; } }
        self.config.archive_dirty |= previous != (self.config.scroll,self.config.frame,self.config.folders);
    }

    fn refresh_config(&mut self, gui: &mut Gui) {
        let Some(win) = self.config.window else { return };
        let documents_changed = self.wins.len() != self.config.documents.len() || self.wins.iter().zip(&self.config.documents).any(|(w,(id,name))| w.cfg.window_name != *id || w.cfg.name != *name);
        if documents_changed {
            let docs = self.window_list();
            gui.list_clear(win,"windows");
            for (id,name) in &docs { gui.list_add(win,"windows",None,ListItem::new(id,name,0,0)); }
            if !docs.iter().any(|(id,_)| *id == self.config.selected) { self.config.selected = docs.first().map(|(id,_)|id.clone()).unwrap_or_default(); }
            gui.list_select(win,"windows",&self.config.selected,true,false);
            self.config.documents = docs;
        }
        let Some(doc) = self.selected_config_doc() else { return };
        let elsewhere = |id| self.wins.iter().enumerate().any(|(i,w)| i != doc && w.cfg.shows(id));
        let rows_changed = self.groups.len() != self.config.rows.len() || self.config.rows.iter().any(|(id,name,on,other,active)| self.groups.get(id) != Some(name) || self.wins[doc].cfg.shows(*id) != *on || (self.config.hide_other && elsewhere(*id) != *other) || *active == self.inactive_groups.contains(id));
        if rows_changed {
            let mut rows: Vec<_> = self.groups.iter().map(|(&id,name)|(id,name.clone(),self.wins[doc].cfg.shows(id),elsewhere(id),!self.inactive_groups.contains(&id))).collect();
            rows.sort_by(|a,b|a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
            let scroll = if self.config.rows.is_empty() { self.config.scroll } else { gui.scroll_offset(win,"groups_scroll") };
            gui.batch_updates(|gui| {
                gui.list_clear(win,"groups");
                for (i,key) in FOLDERS.iter().enumerate() { let mut row=ListItem::new(key,&self.label(key),0xd5,0xd4); row.folder=true; row.open=self.config.folders[i]; row.selectable=false; gui.list_add(win,"groups",None,row); }
                for (id,name,on,other,active) in &rows {
                    let folder = if *id >> 32 == 0 && (0x4200_0000..0x4200_001a).contains(id) { 2 } else if *id >> 32 == 0xe { 1 } else { 0 };
                    if self.config.hide_other && !on && *other { continue; }
                    let mut row=ListItem::new(&group_ident(*id),name,0x117,0); row.selected=*on; row.hide_icon=!*on;
                    row.color_a = if *active { 0xf4f4f4 } else { 0xf66666 }; row.color_b = row.color_a;
                    gui.list_add(win,"groups",Some(FOLDERS[folder]),row);
                }
            });
            gui.set_scroll_offset(win,"groups_scroll",scroll);
            self.config.rows=rows;
        }
        gui.set_checked(win,"HideChannelsInOtherWnd",self.config.hide_other);
    }

    pub(super) fn config_owns(&self, ev: &Event) -> bool {
        if matches!(ev, Event::MenuPicked {..}) { return self.config.menu_doc.is_some() || self.config.menu_group.is_some(); }
        let window = match ev { Event::Clicked{window,..}|Event::ListSelected{window,..}|Event::ListItemMouse{window,..}|Event::Escape{window}|Event::CloseRequested{window}|Event::WindowFrame{window} => *window, _=>return false };
        self.config.window==Some(window)
    }

    pub(super) fn config_event(&mut self,gui:&mut Gui,ev:&Event)->bool {
        if !self.config_owns(ev) { return false; }
        if let Event::MenuPicked { id, .. } = ev {
            self.config_menu_picked(gui, id >> 16, id & 0xffff);
            self.refresh_config(gui);
            return true;
        }
        let win=self.config.window.unwrap();
        match ev {
            Event::CloseRequested{..} => self.config.close=true,
            Event::Escape{..} if self.config.escape => self.config.close=true,
            Event::ListSelected{view,id,selected,..} if view=="windows" && *selected => { self.config.selected=id.clone(); self.config.rows.clear(); self.config.archive_dirty=true; },
            Event::ListSelected{view,id,selected,..} if view=="groups" => { if let (Some(doc),Some(group))=(self.selected_config_doc(),parse_ident(id)) { let name=self.wins[doc].cfg.window_name.clone(); self.subscribe_group(&name,group,*selected); self.config.rows.clear(); } },
            Event::Clicked{view,..} if view=="HideChannelsInOtherWnd" => { self.config.hide_other=gui.checked(win,view); self.config.rows.clear(); },
            Event::ListItemMouse { view,id,button:1,clicks:2,.. } if view == "windows" => {
                if let Some(doc) = self.wins.iter().position(|w|w.cfg.window_name == *id) { self.config_show_document(gui,doc,true); }
            }
            Event::ListItemMouse { view,id,button:2,x,y,.. } if view == "windows" => {
                if let Some(doc) = self.wins.iter().position(|w|w.cfg.window_name == *id) { self.config_window_menu(gui,doc,*x,*y); }
            }
            Event::ListItemMouse { view,id,button:2,x,y,.. } if view == "groups" => {
                if let Some(group) = parse_ident(id) { self.config_group_menu(gui,group,*x,*y); }
            }
            Event::Clicked{view,..} => { if let Some(doc)=self.selected_config_doc() { self.config_operation(gui,doc,view); } },
            _=>{}
        }
        self.capture_config(gui,win);
        self.refresh_config(gui);
        true
    }
}
