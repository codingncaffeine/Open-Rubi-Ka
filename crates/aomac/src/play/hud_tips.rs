//! TipSystem_t::SlotFrameProcess 10053edd, TipView 100568ac/100562e3/10056806.
use super::{chat::extract_section, dvalue::DValues};
use ao_gui::{xml, Event, Gui, WindowId, WindowSize};
use ao_formats::screens::TextDb;
use std::{collections::BTreeSet, path::{Path, PathBuf}};

struct Definition { event: String, data: String, file: String }
struct Page { id: String, file: String, sections: usize }
pub(super) struct Tips {
    dir: PathBuf,
    definitions: Vec<Definition>,
    disabled: BTreeSet<String>,
    loaded: Option<PathBuf>,
    pages: Vec<Page>,
    current: usize,
    window: Option<WindowId>,
    pending: Option<WindowId>,
    dialogs: super::hud_dialog::Dialogs<()>,
    texts: Option<TextDb>,
    screen: (u32,u32),
    esc: bool,
    elapsed: f32,
    minute: u32,
}
fn matches(pattern: &str, value: &str) -> bool {
    let p = pattern.to_ascii_lowercase();
    let v = value.to_ascii_lowercase();
    if p.is_empty() { true }
    else if let Some(p) = p.strip_prefix('*') {
        if let Some(p) = p.strip_suffix('*') { v.contains(p) } else { v.ends_with(p) }
    } else if let Some(p) = p.strip_suffix('*') { v.starts_with(p) }
    else { p == v }
}
impl Tips {
    pub(super) fn new(client: &Path) -> anyhow::Result<Self> {
        let dir = client.join("cd_image/text/tips");
        let root = xml::parse(&std::fs::read_to_string(dir.join("TipHeader.xml"))?)?;
        let definitions = root.children.iter().filter(|e| e.name == "Tip").map(|e| {
            Ok(Definition { event: e.attr("event").ok_or_else(|| anyhow::anyhow!("Tip without event"))?.into(), data: e.attr("data").unwrap_or("").into(), file: e.attr("file").ok_or_else(|| anyhow::anyhow!("Tip without file"))?.into() })
        }).collect::<anyhow::Result<Vec<_>>>()?;
        Ok(Self { dir, definitions, disabled: BTreeSet::new(), loaded: None, pages: vec![], current: 0, window: None, pending: None, dialogs: Default::default(), texts: TextDb::load(client).ok(), screen: (0,0), esc: false, elapsed: 0.0, minute: 0 })
    }
    pub(super) fn window(&self) -> Option<WindowId> { self.window }
    fn load_disabled(&mut self, values: &DValues) {
        let Some(dir) = values.char_dir() else { return };
        if self.loaded.as_deref() == Some(dir) { return }
        self.disabled.clear();
        let path = dir.join("DisabledTipsMap.xml");
        match std::fs::read_to_string(&path) {
            Ok(text) => match xml::parse(&text) {
                Ok(root) => self.disabled.extend(root.children.iter().filter(|e| e.name == "DisabledTip").filter_map(|e| e.attr("tip_id").map(str::to_owned))),
                Err(e) => eprintln!("tips: {}: {e:#}", path.display()),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {},
            Err(e) => eprintln!("tips: {}: {e}", path.display()),
        }
        self.loaded = Some(dir.into());
    }
    pub(super) fn trigger(&mut self, gui: &mut Gui, values: &mut DValues, screen: (u32,u32), event: &str, data: &str) {
        self.load_disabled(values);
        if !values.flag("IsTipEnabled") { return }
        let mut triggered = false;
        for def in &self.definitions {
            if def.event != event || !matches(&def.data, data) { continue }
            let id = format!("{}{}", def.event, def.data);
            if self.disabled.contains(&id) { continue }
            triggered = true;
            if self.pages.get(self.current).is_some_and(|p| p.id == id) { continue }
            self.pages.push(Page { id, file: def.file.clone(), sections: 1 });
            self.current = self.pages.len() - 1;
        }
        if !triggered { return }
        values.set_i64("tip_window", 1);
        self.open(gui,values,screen);
    }
    fn open(&mut self, gui: &mut Gui, values: &DValues, screen: (u32,u32)) {
        self.screen = screen;
        if let Some(w) = self.pending.take() { gui.close_window(w); }
        if self.window.is_none() {
            // TipView vtable+0x80 (100571bd): inclusive Point(399,299).
            match gui.open_tabbed_window("TipView", "Tips", (0,0), WindowSize::Fixed(400,300)) {
                Ok(w) => {
                    let (width,height) = gui.outer_size(w);
                    gui.set_window_pos(w, ((screen.0 as i32-width as i32)/2,(screen.1 as i32-height as i32)/2));
                    self.window = Some(w);
                    self.esc = values.flag("esc_tips");
                }
                Err(e) => eprintln!("tips: {e:#}"),
            }
        }
        self.show(gui,screen);
    }
    fn sync_pending(&mut self, gui: &mut Gui, values: &DValues) {
        if self.window.is_some() || self.pages.is_empty() || !values.flag("IsTipEnabled") {
            if let Some(w) = self.pending.take() { gui.close_window(w); }
        } else if self.pending.is_none() {
            // TipButtonWindow_c 10055ded: same art for all three states.
            let xml = "<root><Button name=\"TipButton\" gfxid_raised=\"GFX_GUI_NEW_TIP\" gfxid_pressed=\"GFX_GUI_NEW_TIP\" gfxid_hover=\"GFX_GUI_NEW_TIP\"/></root>";
            match gui.open_window_xml("TipButton",xml,(self.screen.0 as i32-60,self.screen.1 as i32-453),WindowSize::Preferred) {
                Ok(w) => self.pending = Some(w),
                Err(e) => eprintln!("tips: pending button: {e:#}"),
            }
        }
    }
    fn show(&self, gui: &mut Gui, _screen: (u32,u32)) {
        let Some(w) = self.window else { return };
        let Some(page) = self.pages.get(self.current) else { return };
        let text = match std::fs::read(self.dir.join(&page.file)) {
            Ok(bytes) => bytes.into_iter().map(char::from).collect::<String>(),
            Err(e) => { eprintln!("tips: {}: {e}", page.file); gui.set_text(w,"BrowserView","Failed to load Tip File"); return },
        };
        if let Some((_,tail)) = text.split_once("<title>") {
            if let Some((title,_)) = tail.split_once("</title>") {
                gui.set_window_tabs(w,&[title.to_owned()],0);
            }
        }
        let total = text.to_ascii_lowercase().matches("<section>").count();
        let mut body = String::new();
        for n in 0..page.sections.min(total) {
            body.push_str("<div><font color=CCInfoHeader>o</font><font color=CCInfoHeadline> ");
            body.push_str(&extract_section(&text,n as i32));
            body.push_str("</font></div>");
        }
        if page.sections < total { body.push_str(&format!("<a href=\"NextSubTip\">Next Section... ({}/{total})</a>",page.sections)); }
        if self.pages.len() > 1 {
            body.push_str("<br>");
            if self.current > 0 { body.push_str("<a href=\"PrevTip\">Previous Tip</a> "); }
            if self.current + 1 < self.pages.len() { body.push_str("<a href=\"NextTip\">Next Tip</a>"); }
        }
        gui.set_text(w,"BrowserView",&body);
        gui.set_checked(w,"DisableOneTipCheckBox",self.disabled.contains(&page.id));
    }
    fn close(&mut self, gui: &mut Gui, values: &mut DValues) {
        if let Some(w) = self.window.take() {
            if gui.checked(w,"DisableTipsCheckBox") {
                let text = |key| self.texts.as_ref().and_then(|t| t.by_key(10000,key)).unwrap_or_default();
                self.dialogs.configure_escape(values);
                self.dialogs.go_named(gui,self.screen,"DisableTips",(),&text("ReallyDisableTips"),&[text("MsgBox_Yes"),text("MsgBox_No")]);
            }
            gui.close_window(w);
        }
        values.set_i64("tip_window",0);
        self.pages.retain(|p| !self.disabled.contains(&p.id));
        self.current = self.pages.len().saturating_sub(1);
        self.sync_pending(gui,values);
    }
    pub(super) fn escape(&mut self, gui: &mut Gui, values: &mut DValues) -> bool {
        if self.window.is_some() && self.esc { self.close(gui,values); true } else { false }
    }
    pub(super) fn update(&mut self, gui: &mut Gui, values: &mut DValues, screen: (u32,u32), dt: f32) {
        self.screen = screen;
        if self.window.is_none() && values.flag("tip_window") && !self.pages.is_empty() { self.open(gui,values,screen); }
        self.sync_pending(gui,values);
        if self.window.is_some() && !values.flag("tip_window") { self.close(gui,values); }
        if values.flag("IsTipEnabled") {
            self.elapsed += dt;
            let minute = (self.elapsed / 60.0) as u32;
            if minute > self.minute { self.minute = minute; self.trigger(gui,values,screen,"AfterMinutesOfPlay",&minute.to_string()); }
        }
    }
    pub(super) fn event(&mut self, gui: &mut Gui, values: &mut DValues, screen: (u32,u32), event: &Event) -> (bool,Option<String>) {
        let (mine,answer) = self.dialogs.event(gui,event);
        if mine {
            // TipSystem_t::SlotDisableTipsDlg 10053334: only button index 0.
            if answer.is_some_and(|(_,button)| button == 0) { values.set_i64("IsTipEnabled",0); }
            self.sync_pending(gui,values);
            return (true,None);
        }
        if matches!(event, Event::Clicked { window,view,.. } if Some(*window) == self.pending && view == "TipButton") {
            // 10055877; DValue listener100558bd destroys the pending button.
            values.set_i64("tip_window",1);
            self.open(gui,values,screen);
            return (true,None);
        }
        let Some(w) = self.window else { return (false,None) };
        match event {
            Event::CloseRequested { window } if *window == w => self.close(gui,values),
            Event::LinkClicked { window,href,.. } if *window == w => {
                match href.as_str() {
                    "NextSubTip" => self.pages[self.current].sections += 1,
                    "PrevTip" => self.current = self.current.saturating_sub(1),
                    "NextTip" => self.current = (self.current + 1).min(self.pages.len()-1),
                    _ => return (true,Some(href.clone())),
                }
                self.show(gui,screen);
            }
            Event::Clicked { window,view,.. } if *window == w && view == "DisableOneTipCheckBox" => {
                let id = self.pages[self.current].id.clone();
                if gui.checked(w,view) { self.disabled.insert(id); } else { self.disabled.remove(&id); }
                if let Some(dir) = values.char_dir() {
                    let children = self.disabled.iter().map(|id| xml::Element { name: "DisabledTip".into(), attrs: [("tip_id".into(),id.clone())].into(), children: vec![] }).collect();
                    let root = xml::Element { name: "Root".into(), attrs: Default::default(), children };
                    if let Err(e) = std::fs::write(dir.join("DisabledTipsMap.xml"),super::dvalue::element_xml(&root)) { eprintln!("tips: saving disabled tips: {e}"); }
                }
            }
            _ => return (false,None),
        }
        (true,None)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn native_tip_pattern_matching() {
        assert!(matches("*Startup Maps*","My Startup Maps Item"));
        assert!(matches("*Basic Board","Omni Basic Board"));
        assert!(matches("Startup Maps*","Startup Maps Kit"));
        assert!(!matches("Startup Maps*","Other Startup Maps"));
        assert!(matches("","anything"));
    }
    #[test]
    fn player_tip_uses_native_file_and_samples_escape_at_creation() {
        let dir = ao_gui::client_dir();
        if !dir.join("cd_image/text/tips/TipHeader.xml").exists() { return }
        let mut gui = Gui::new(&dir,None).unwrap();
        let mut values = DValues::new(&dir);
        let mut tips = Tips::new(&dir).unwrap();
        values.set_i64("IsTipEnabled",1);
        values.set_i64("esc_tips",0);
        tips.trigger(&mut gui,&mut values,(1280,800),"OnSelectingPlayer","");
        let window = tips.window.unwrap();
        assert_eq!(gui.window_size(window),(400,300));
        assert!(gui.text(window,"BrowserView").contains("invite this player"));
        values.set_i64("esc_tips",1);
        assert!(!tips.escape(&mut gui,&mut values));
        tips.event(&mut gui,&mut values,(1280,800),&Event::CloseRequested { window });
        tips.trigger(&mut gui,&mut values,(1280,800),"OnSelectingPlayer","");
        assert!(tips.escape(&mut gui,&mut values));
        assert!(tips.window.is_none() && !values.flag("tip_window"));
    }
    #[test]
    fn disabled_tip_checkbox_persists_native_identity() {
        let dir = ao_gui::client_dir();
        if !dir.join("cd_image/text/tips/TipHeader.xml").exists() { return }
        let mut gui = Gui::new(&dir,None).unwrap();
        let mut values = DValues::new(&dir);
        let root = std::env::temp_dir().join(format!("aomac-tip-disable-{}",std::process::id()));
        std::fs::create_dir_all(root.join("account/Char123")).unwrap();
        values.open_user(&root,"account",123);
        values.set_i64("IsTipEnabled",1);
        let mut tips = Tips::new(&dir).unwrap();
        tips.trigger(&mut gui,&mut values,(1280,800),"OnSelectingPlayer","");
        let window = tips.window.unwrap();
        gui.set_checked(window,"DisableOneTipCheckBox",true);
        tips.event(&mut gui,&mut values,(1280,800),&Event::Clicked { window,view:"DisableOneTipCheckBox".into(),item:None });
        let saved = std::fs::read_to_string(root.join("account/Char123/DisabledTipsMap.xml")).unwrap();
        assert!(saved.contains("tip_id=\"OnSelectingPlayer\""));
        tips.event(&mut gui,&mut values,(1280,800),&Event::CloseRequested { window });
        let mut restored = Tips::new(&dir).unwrap();
        restored.trigger(&mut gui,&mut values,(1280,800),"OnSelectingPlayer","");
        assert!(restored.window.is_none());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn close_confirms_global_disable_and_pending_button_reopens_history() {
        let dir = ao_gui::client_dir();
        if !dir.join("cd_image/text/tips/TipHeader.xml").exists() { return }
        let mut gui = Gui::new(&dir,None).unwrap();
        let mut values = DValues::new(&dir);
        values.set_i64("IsTipEnabled",1);
        let mut tips = Tips::new(&dir).unwrap();
        tips.trigger(&mut gui,&mut values,(1280,800),"OnSelectingPlayer","");
        let window = tips.window.unwrap();
        gui.set_checked(window,"DisableTipsCheckBox",true);
        tips.event(&mut gui,&mut values,(1280,800),&Event::CloseRequested { window });
        assert!(values.flag("IsTipEnabled"));
        let pending = tips.pending.unwrap();
        assert_eq!(gui.window_pos(pending),(1220,347));
        let dialog = tips.dialogs.windows()[0];
        tips.event(&mut gui,&mut values,(1280,800),&Event::Clicked { window:dialog,view:"btn1".into(),item:None });
        assert!(values.flag("IsTipEnabled"));
        tips.event(&mut gui,&mut values,(1280,800),&Event::Clicked { window:pending,view:"TipButton".into(),item:None });
        assert!(values.flag("tip_window") && tips.pending.is_none());
        let window = tips.window.unwrap();
        assert!(gui.text(window,"BrowserView").contains("invite this player"));
        gui.set_checked(window,"DisableTipsCheckBox",true);
        tips.event(&mut gui,&mut values,(1280,800),&Event::CloseRequested { window });
        let dialog = tips.dialogs.windows()[0];
        tips.event(&mut gui,&mut values,(1280,800),&Event::CloseRequested { window:dialog });
        assert!(values.flag("IsTipEnabled"));
        let pending = tips.pending.unwrap();
        tips.event(&mut gui,&mut values,(1280,800),&Event::Clicked { window:pending,view:"TipButton".into(),item:None });
        let window = tips.window.unwrap();
        gui.set_checked(window,"DisableTipsCheckBox",true);
        tips.event(&mut gui,&mut values,(1280,800),&Event::CloseRequested { window });
        let dialog = tips.dialogs.windows()[0];
        tips.event(&mut gui,&mut values,(1280,800),&Event::Clicked { window:dialog,view:"btn0".into(),item:None });
        assert!(!values.flag("IsTipEnabled") && tips.pending.is_none());
    }
}
