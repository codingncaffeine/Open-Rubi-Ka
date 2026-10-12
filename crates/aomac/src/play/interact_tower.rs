//! TowerBuildMenuView_c (GUI 1005c35b), Interfaces 1000a707 use interception.
//! Native checklist tables: GUI 1005a723; AF request: GC 100294a7; B0 result: GC 1005d213/100125c4.
use super::{dvalue::DValues, zone::Zone};
use ao_formats::{dynel_visual::{item_template, ITEM_TEMPLATE_TYPE}, screens::TextDb};
use ao_gui::{Event, Gui, WindowId, WindowSize};
use ao_net::{msg::Identity, n3::{action, world::{AcgItem, CharacterAction}}};
use ao_rdb::RecordStore;

pub(super) enum TowerOut { Handled, Request(Vec<u8>), Build(Identity) }

// Ordered native checklist entries: flag, LDB 503 key, /showfile target.
const CONTROLLER: &[(u32, &str, &str)] = &[
    (0x20,"LC_CheckOrg","BuildMenu_Controller_InOrg.html"),
    (0x80,"LC_CheckOrgRank","BuildMenu_Controller_HighOrgRank.html"),
    (0x8000,"LC_CheckOrgControllerCount","BuildMenu_Controller_WithinTowerLimit.html"),
    (0x1000,"LC_CheckLCEnabled","BuildMenu_Controller_PositionIsLegal.html"),
    (0x400000,"LC_CheckLCDisabled","BuildMenu_Controller_Approved.html"),
    (0x40,"LC_CheckSameOrg","BuildMenu_Controller_OrgOwn.html"),
    (0x400,"LC_CheckExistingController","BuildMenu_Controller_OneController.html"),
    (0x2000,"LC_CheckPeaceState","BuildMenu_Controller_Aftermath.html"),
    (0x40000,"LC_CheckTowerLevel","BuildMenu_Controller_WithinRange.html"),
    (0x10000,"LC_CheckControllerType","BuildMenu_Controller_UniqueType.html"),
    (0x80000,"LC_CheckSkillReq","BuildMenu_Controller_Skills.html"),
    (0x200,"LC_CheckExp","BuildMenu_Controller_Skills.html"),
    (0x800000,"LC_CheckFlying","BuildMenu_Player_Flying.html"),
];
const TOWER: &[(u32, &str, &str)] = &[
    (0x20,"LC_CheckOrg","BuildMenu_Tower_InOrg.html"),
    (0x1000,"LC_CheckLCEnabled","BuildMenu_Tower_PositionIsLegal.html"),
    (0x400000,"LC_CheckLCDisabled","BuildMenu_Tower_Approved.html"),
    (0x100,"LC_CheckControllerPresent","BuildMenu_Tower_ControllerInDistrict.html"),
    (0x200000,"LC_CheckDefenceShield","BuildMenu_Tower_ControllerShield.html"),
    (0x40,"LC_CheckSameOrg","BuildMenu_Tower_OrgOwn.html"),
    (0x40000,"LC_CheckTowerLevel","BuildMenu_Tower_DistrictLevelRange.html"),
    (0x4000,"LC_CheckTowerCount","BuildMenu_Tower_TowerDistrictLimit.html"),
    (4,"LC_CheckTowerProximity","BuildMenu_Tower_TooCrowded.html"),
    (2,"LC_CheckTowerDistance","BuildMenu_Tower_TooClose.html"),
    (0x20000,"LC_CheckCharTowerCount","BuildMenu_Tower_OwnTooMany.html"),
    (0x100000,"LC_CheckCharTowerUnique","BuildMenu_Tower_Unique.html"),
    (0x80000,"LC_CheckSkillReq","BuildMenu_Tower_SkillRequirement.html"),
    (0x200,"LC_CheckExp","BuildMenu_Tower_SkillRequirement.html"),
    (0x800000,"LC_CheckFlying","BuildMenu_Player_Flying.html"),
];
const DISABLER: &[(u32, &str, &str)] = &[
    (8,"LC_CheckFightMode","WarMenu_NoFighting.html"),
    (0x10,"LC_CheckControllerPresent","WarMenu_NoController.html"),
    (2,"LC_CheckDefenceShield","WarMenu_NotEnabled.html"),
    (4,"LC_CheckDisablerLevel","WarMenu_TooLowLevel.html"),
    (0x40,"LC_CheckPvPLevelRange","WarMenu_TooHighLevel.html"),
    (0x80000,"LC_CheckSkillReq","WarMenu_SkillRequirement.html"),
];

pub(super) struct TowerUi {
    win: Option<WindowId>,
    item: Identity,
    acg: AcgItem,
    kind: i32,
    escape_next: bool,
    escape: bool,
    build_enabled: bool,
    position: [f32; 3],
    urls: Vec<String>,
    store: Option<RecordStore>,
    notices: Vec<String>,
    config: super::hud_wincfg::WinCfgs,
    tracked_win: Option<WindowId>,
}
impl Default for TowerUi {
    fn default() -> Self {
        let dir = ao_gui::client_dir();
        Self { win: None, item: Identity::default(), acg: AcgItem::default(), kind: 0, escape_next: true, escape: true, build_enabled: false, position: [0.0;3], urls: vec![], notices: vec![], store: RecordStore::open(&dir).ok(), config: super::hud_wincfg::WinCfgs::new(&dir), tracked_win: None }
    }
}
fn esc(text: &str) -> String { text.replace('&',"&amp;").replace('<',"&lt;").replace('>',"&gt;").replace('"',"&quot;") }
fn checklist_request(own: Identity, item: AcgItem) -> Vec<u8> {
    action::character_action(own.instance, &CharacterAction { action: 0xaf, param: 0, identity_a: Identity {kind:item.low_id,instance:item.high_id}, identity_b: Identity {kind:item.level,instance:0}, text:String::new() })
}
fn can_build(owned: bool, flags: u32, checks: &[(u32, &str, &str)]) -> bool {
    owned && checks.iter().all(|(mask, _, _)| flags & mask == 0)
}
impl TowerUi {
    pub(super) fn configure_escape(&mut self, d: &DValues) { self.escape_next = d.get_i64("esc_towerbuildmenu").unwrap_or(1) != 0; }
    pub(super) fn sync_config(&mut self, gui: &mut Gui, d: &mut DValues, screen: (u32, u32)) {
        if self.tracked_win != self.win {
            if let Some(old) = self.tracked_win.take() { self.config.detach_hud(gui, d, old); }
            if let Some(win) = self.win {
                gui.set_window_frame(win, true, false);
                self.config.attach_hud(gui, d, win, super::hud_wincfg::Store::Value("TowerBuildMenuViewConfig"), screen);
                self.tracked_win = Some(win);
            }
        }
        self.config.update(gui, d);
    }
    pub(super) fn close_all(&mut self, gui: &mut Gui) { if let Some(w)=self.win.take() { gui.close_window(w); } self.build_enabled=false; }
    pub(super) fn classify(&self, zone: &Zone, id: Identity) -> anyhow::Result<Option<(AcgItem,i32,String)>> {
        let acg = if id.kind < 50000 {
            let Some(entry)=zone.inventory.get(&(id.instance as u32)) else {return Ok(None)};
            entry.item
        } else {
            let Some(low)=zone.world.stat_of(id.kind,id.instance,23).filter(|v|*v>0) else {return Ok(None)};
            AcgItem {low_id:low,high_id:low,level:zone.world.stat_of(id.kind,id.instance,54).unwrap_or(1)}
        };
        let Some(store)=self.store.as_ref() else {return Ok(None)};
        let Some(t)=item_template(store,acg.low_id as u32)? else {return Ok(None)};
        let tower=zone.world.stat_of(id.kind,id.instance,0x184).or_else(||t.stat(0x184)).unwrap_or(0);
        if tower != 0 && tower != 0x499602d2 { return Ok(Some((acg,tower,t.name.unwrap_or_default()))); }
        let Some(record)=store.get(ITEM_TEMPLATE_TYPE,acg.low_id as u32)? else {return Ok(None)};
        if super::chat::item_is_shield_disabler(&record)? { Ok(Some((acg,0,t.name.unwrap_or_default()))) } else {Ok(None)}
    }
    pub(super) fn open(&mut self, gui: &mut Gui, screen:(u32,u32), zone:&Zone, id:Identity, text:&TextDb) -> anyhow::Result<Option<Vec<u8>>> {
        let Some((acg,kind,name))=self.classify(zone,id)? else {return Ok(None)};
        if self.win.is_none() {
            let xml="<root><View view_layout=\"vertical\"><View view_layout=\"horizontal\"><HLayoutSpacer/><TextView name=\"name\" layout_borders=\"Rect(5,5,5,0)\"/><HLayoutSpacer/></View><View view_layout=\"horizontal\"><HLayoutSpacer/><CanvasView name=\"item\" min_size=\"Point(48,48)\" max_size=\"Point(48,48)\" layout_borders=\"Rect(10,10,10,0)\"/><HLayoutSpacer/></View><BorderView layout_borders=\"Rect(10,10,10,10)\"><TextView name=\"checks\" feature_flags=\"TVF_MULTILINE|TVF_WORD_WRAP|TVF_ACCEPT_MOUSE_INPUT\" layout_borders=\"Rect(5,5,5,5)\" v_scrollbar_mode=\"AUTO\"/></BorderView><View view_layout=\"horizontal\"><HLayoutSpacer/><Button name=\"check\" layout_borders=\"Rect(5,0,5,5)\"/><Button name=\"build\" layout_borders=\"Rect(0,0,5,5)\"/></View></View></root>";
            let w=gui.open_window_xml(&text.by_key(10000,"BuildMenu").unwrap_or_default(),xml,(0,0),WindowSize::Fixed(409,369))?;
            gui.set_window_dock_frame(w,Some(&text.by_key(10000,"BuildMenu").unwrap_or_default()));
            let (width,height)=gui.outer_size(w);
            gui.set_window_pos(w,((screen.0 as i32-width as i32)/2,(screen.1 as i32-height as i32)/2));
            self.escape=self.escape_next; self.win=Some(w);
        }
        self.item=id; self.acg=acg; self.kind=kind;
        self.position=zone.own().map_or([0.0;3],|d|d.pos);
        let w=self.win.unwrap();
        gui.set_text(w,"name",&esc(&name));
        gui.set_text(w,"check",&text.by_key(503,"LC_ButtonCheck").unwrap_or_default());
        gui.set_text(w,"build",&text.by_key(503,if kind==0 {"LC_ButtonDisableShield"} else {"LC_ButtonBuild"}).unwrap_or_default());
        gui.set_text(w,"checks",&text.by_key(503,if kind==0 {"LC_WarCheckList"} else if kind==1 {"LC_ControllerCheckList"} else {"LC_TowerCheckList"}).unwrap_or_default());
        if let Some(store)=self.store.as_ref() {
            if let Some(t)=item_template(store,acg.low_id as u32)? {
                if let Some((gfx,width,height))=t.stat(79).filter(|v|*v>0).and_then(|v|super::hud_bar::icon_image(gui,store,v as u32)) {
                    gui.set_canvas(w,"item",vec![ao_gui::view::CanvasItem::Image {id:gfx,src:[0.0,0.0,width as f32,height as f32],dst:[0.0,0.0,48.0,48.0],alpha:1.0}]);
                }
            }
        }
        self.build_enabled=false;
        gui.set_enabled(w,"build",false);
        gui.set_enabled(w,"check",false);
        Ok(Some(checklist_request(Identity {kind:50000,instance:zone.char_id as i32},acg)))
    }
    pub(super) fn result(&mut self, gui:&mut Gui, a:&CharacterAction, text:&TextDb, zone:&Zone) {
        let Some(w)=self.win else {return};
        if a.action!=0xb0 {return;}
        let flags=a.identity_b.instance as u32;
        let checks=if self.kind==0 {DISABLER} else if self.kind==1 {CONTROLLER} else {TOWER};
        let mut body=String::new();
        for &(mask,key,file) in checks {
            let marker=if flags&mask==0 {"GFX_GUI_CHECKLIST_MARKER_POSITIVE"} else {"GFX_GUI_CHECKLIST_MARKER_NEGATIVE"};
            body+=&format!("<div indent=wrapped><img src=tdb://id:{marker}> <a href=\"/showfile {file}\">{}</a></div>",esc(&text.by_key(503,key).unwrap_or_default()));
        }
        let owned=self.item.kind<50000 && zone.inventory.contains_key(&(self.item.instance as u32));
        if !owned {body+=&esc(&text.by_key(10000,"OnlySimulatedCheck").unwrap_or_default());}
        self.build_enabled=can_build(owned,flags,checks);
        if flags != 0 && can_build(true, flags, checks) {
            self.notices.push(format!("All checks are ok, but some data checks failed. This should not happen, please report a bug with your current position. And the value ({flags:08x}). You will fail to build here."));
        }
        gui.set_text(w,"checks",&body); gui.set_enabled(w,"build",self.build_enabled);
    }
    pub(super) fn update(&mut self, gui:&mut Gui, zone:&Zone, check_ready:bool) {
        let Some(w)=self.win else {return};
        let pos=zone.own().map_or([0.0;3],|d|d.pos);
        if pos!=self.position {self.position=pos;self.build_enabled=false;gui.set_enabled(w,"build",false);}
        if self.item.kind<50000 && !zone.inventory.contains_key(&(self.item.instance as u32)) {self.close_all(gui);return;}
        gui.set_enabled(w,"check",check_ready);
    }
    pub(super) fn event(&mut self, gui:&mut Gui, ev:&Event, own:Identity) -> Option<TowerOut> {
        let w=self.win?;
        match ev {
            Event::Escape {window} if *window==w && self.escape => self.close_all(gui),
            Event::CloseRequested {window} if *window==w => self.close_all(gui),
            Event::Clicked {window,view,..} if *window==w && view=="check" => {
                gui.set_text(w,"checks","");self.build_enabled=false;gui.set_enabled(w,"build",false);gui.set_enabled(w,"check",false);
                return Some(TowerOut::Request(checklist_request(own,self.acg)));
            }
            Event::Clicked {window,view,..} if *window==w && view=="build" && self.build_enabled => {
                let id=self.item;self.close_all(gui);
                return Some(TowerOut::Build(id));
            }
            Event::LinkClicked {window,href,..} if *window==w => self.urls.push(href.clone()),
            _=>return None,
        }
        Some(TowerOut::Handled)
    }
    pub(super) fn take_urls(&mut self)->Vec<String> {std::mem::take(&mut self.urls)}
    pub(super) fn take_notices(&mut self) -> Vec<String> { std::mem::take(&mut self.notices) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checklist_request_preserves_all_acg_words() {
        let own=Identity {kind:50000,instance:7};
        let bytes=checklist_request(own,AcgItem {low_id:1,high_id:2,level:3});
        let frame=ao_net::n3::outgoing::n3_frame(0,7,bytes);
        let decoded=ao_net::n3::decode(&frame).unwrap();
        let ao_net::n3::N3::World(ao_net::n3::world::World::CharacterAction(a))=decoded.body else {panic!("action")};
        assert_eq!((a.action,a.param,a.identity_a,a.identity_b),(0xaf,0,Identity {kind:1,instance:2},Identity {kind:3,instance:0}));
    }
    #[test]
    fn native_checklist_requires_ownership_and_every_displayed_predicate() {
        for checks in [CONTROLLER, TOWER, DISABLER] {
            assert!(can_build(true, 0, checks));
            assert!(!can_build(false, 0, checks));
            for (mask, _, _) in checks { assert!(!can_build(true, *mask, checks)); }
            // Retail reports undisplayed bits separately, without inventing a checklist row.
            assert!(can_build(true, 0x80000000, checks));
        }
    }

    #[test]
    fn city_policy_only_captures_for_native_city_interface_models() {
        let mut values=DValues::default();
        values.set_i64("esc_cityterminal",0);
        assert_eq!(super::super::interact::Interact::city_terminal_constructor_policy(&values,None),None);
        assert_eq!(super::super::interact::Interact::city_terminal_constructor_policy(&values,Some(0)),None);
        assert_eq!(super::super::interact::Interact::city_terminal_constructor_policy(&values,Some(1)),Some(false));
        values.set_i64("esc_cityterminal",1);
        assert_eq!(super::super::interact::Interact::city_terminal_constructor_policy(&values,Some(2)),Some(true));
    }
}
