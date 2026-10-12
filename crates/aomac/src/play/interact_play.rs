//! The `Play` side of the interaction layer: the hooks `flow.rs` calls (zone frames, GUI events, mouse clicks, the per-frame outbox).

use super::hud_target::pick_ray;
use super::interact::Interact;
use super::interact_use::pick_objects;
use super::Play;
use ao_gui::{Event, InputEvent, MouseButton};
use ao_net::msg::Identity;
use ao_net::n3::outgoing::DYNEL_CHAR;
use ao_net::frame::Frame;
use ao_render::Host;

impl Play {
    /// A new zone connection / playfield: the old dialogue window is gone (`StopPlayfield`), the state starts over.
    pub(super) fn interact_reset(&mut self) {
        if let Some(i) = self.interact.as_mut() {
            i.close_all(&mut self.gui);
        }
        let mut i = Interact::new(self.zone.char_id, self.size);
        if let Some(hud) = self.hud.as_ref() { i.configure_escape(&hud.dvalues); }
        i.set_client_dir(self.dir.clone());
        i.trade.texts = super::interact_chat::ChatTexts::load(&self.text);
        i.ptrade.set_texts(self.text.by_key(10000, "MsgBox_Yes").unwrap_or_default(), self.text.by_key(10000, "MsgBox_No").unwrap_or_default());
        self.interact = Some(i);
    }

    pub(super) fn interact_zone_frame(&mut self, f: &Frame) {
        if let Some(i) = self.interact.as_mut() {
            if let Some(hud) = self.hud.as_ref() {
                i.configure_escape(&hud.dvalues);
                i.mission.load_config(&hud.dvalues);
            }
            i.on_frame(&mut self.gui, f, &mut self.zone);
        }
    }

    pub(super) fn interact_event(&mut self, e: &Event) -> bool {
        if let (Some(i), Some(hud)) = (self.interact.as_mut(), self.hud.as_ref()) { i.configure_escape(&hud.dvalues); }
        self.interact.as_mut().is_some_and(|i| i.event(&mut self.gui, e, &self.zone))
    }

    /// Per frame: the frames the player's actions produced go to the zone server.
    pub(super) fn interact_frame(&mut self) {
        let Some(i) = self.interact.as_mut() else { return };
        let mut check_ready = true;
        if let Some(hud) = self.hud.as_mut() {
            i.configure_escape(&hud.dvalues);
            check_ready = hud.actions.list.progress(0xa1).is_none();
            for item in hud.take_tower_uses() { i.queue_tower_use(&self.zone, item); }
        }
        for item in std::mem::take(&mut i.tower_uses) {
            match i.tower.open(&mut self.gui, self.size, &self.zone, item, &self.text) {
                Ok(Some(payload)) => i.send(payload),
                Ok(None) => {}
                Err(error) => eprintln!("tower build menu: {error:#}"),
            }
        }
        if let Some(reply) = i.tower_reply.take() { i.tower.result(&mut self.gui, &reply, &self.text, &self.zone); }
        i.tower.update(&mut self.gui, &self.zone, check_ready);
        i.resize(self.size);
        i.tick(self.time);
        if let Some(hud) = self.hud.as_mut() {
            i.mission.load_config(&hud.dvalues);
            i.mission.save_config(&mut hud.dvalues);
            i.tower.sync_config(&mut self.gui, &mut hud.dvalues, self.size);
            i.use_ui.loot.update_tooltips(&mut self.gui, &self.zone, &self.text, hud.dvalues.flag("ItemDescriptionTooltips"));
        }
        for t in i.take_notices().into_iter().chain(i.tower.take_notices()) {
            if let Some(c) = self.chat.as_mut() {
                c.system_line(&mut self.gui, &t, 12);
            }
        }
        for key in i.take_feedback() {
            if let Some(c) = self.chat.as_mut() {
                c.feedback(&mut self.gui, key, &self.text);
            }
        }
        i.show_confirms(&mut self.gui, &self.text, &self.zone);
        for key in i.ptrade.take_feedback() {
            if let Some(c) = self.chat.as_mut() {
                c.feedback(&mut self.gui, key, &self.text);
            }
        }
        for url in i.mission.take_urls() {
            if let Some(chat) = self.chat.as_mut() {
                chat.show_url(&mut self.gui, &self.zone, &self.text, &url);
            }
        }
        for url in i.tower.take_urls() {
            if let Some(chat) = self.chat.as_mut() {
                chat.show_url(&mut self.gui, &self.zone, &self.text, &url);
            }
        }
        if let Some(marker) = i.mission.take_marker() {
            if let Some(hud) = self.hud.as_mut() {
                hud.set_mission(Some(marker));
            }
        }
        for f in i.take_outbox() {
            if let Some(s) = &self.session {
                s.send_zone(f);
            }
        }
        self.interact_trade_frame();
        self.interact_ptrade_frame();
        self.interact_shop_frame();
        self.interact_bank_frame();
    }

    /// A plain left click that selected character `id`: the second one on the same dynel within [`ao_gui::DOUBLE_CLICK_TIME`] is a double click,
    /// which runs the default action when the option `DoubleclickAction` is on (`LoginPrefs.xml`: true; `FUN_1002c2ee`).
    pub(super) fn interact_left_click(&mut self, id: i32) {
        let now = self.time;
        let id = Identity { kind: DYNEL_CHAR, instance: id };
        if let Some(i) = self.interact.as_mut() {
            if i.double_click(id, now) {
                i.default_action_on(&self.zone, id);
            }
        }
    }

    /// A mouse button released without a look (`Controls` emits `Click`) on the world. The object under the pointer is `GetObjectUnderColLine`: the
    /// current target when it is under the pointer, else the nearest hit (characters and objects, [`pick_objects`]).
    /// * Right button (`ActionViewMouseHandler_c` release `FUN_1002c469` [GUI]): a character -> `N3Msg_DefaultActionOnDynel`, anything else ->
    ///   `N3Msg_UseItem(id, false)`.
    /// * Left button: the HUD selects from the same merged hit list; a second click on a non-character runs its default action.
    pub(super) fn interact_mouse(&mut self, ev: &InputEvent, host: &Host) {
        if let Some(i) = self.interact.as_mut() {
            i.shop.quick = host.mods.shift || host.mods.ctrl;
            if matches!(ev, InputEvent::MouseDown { button: MouseButton::Left, .. }) {
                i.use_ui.loot.info_click = host.mods.shift || host.mods.ctrl;
            }
            if let InputEvent::MouseMove { x, y } = ev {
                i.trade.mouse = (*x, *y);
            }
        }
        // an inventory item released over the world (`FUN_100cb081` [GUI], docs/zone/interact.md §8.6): the object under the pointer is what it is used on
        if let InputEvent::MouseUp { x, y, button: MouseButton::Left } = *ev {
            if self.hud.as_ref().is_some_and(|h| h.item_dragging()) {
                let under = (!self.gui.wants_mouse(x, y)).then(|| {
                    let ray = pick_ray(&host.camera, &host.lens.unwrap_or_default(), (self.size.0 as f32, self.size.1 as f32), (x, y));
                    pick_objects(&ray, &self.zone, self.player.as_ref()).first().copied()
                });
                if let Some(h) = self.hud.as_mut() {
                    h.set_world_under(under.flatten());
                }
            }
        }
        let InputEvent::MouseUp { x, y, button: button @ (MouseButton::Left | MouseButton::Right) } = *ev else { return };
        let clicked = self.player.as_mut().is_some_and(|p| p.take_clicks().contains(&button));
        if !clicked || self.gui.wants_mouse(x, y) {
            return;
        }
        let ray = pick_ray(&host.camera, &host.lens.unwrap_or_default(), (self.size.0 as f32, self.size.1 as f32), (x, y));
        let list = pick_objects(&ray, &self.zone, self.player.as_ref());
        let target = self.zone.selected_target();
        let Some(id) = target.filter(|t| list.contains(t)).or_else(|| list.first().copied()) else { return };
        let (now, zone) = (self.time, &self.zone);
        let Some(i) = self.interact.as_mut() else { return };
        match button {
            MouseButton::Right if id.kind == DYNEL_CHAR => {
                i.default_action_on(zone, id);
            }
            MouseButton::Right => {
                i.use_item(zone, id, false);
            }
            _ if !host.mods.shift && id.kind != DYNEL_CHAR && i.double_click(id, now) => {
                i.default_action_on(zone, id);
            }
            _ => {}
        }
    }
}
