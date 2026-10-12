//! Control-centre support of the GUI engine (`docs/gui.md` §10): criteria-driven visibility, views built from XML text,
//! the `CCMenuEntry_c` button art.

use super::*;
use crate::expr;

/// `CCMenuEntry_c` art ids (`SetGfx` in GUI.dll 0x10065b4e): (left, middle, right) of the raised, pressed and hover sets.
const RAISED: [u32; 3] = [0x9b, 0x9d, 0x9f];
const RAISED_GOLD: [u32; 3] = [0x9c, 0x9e, 0xa0];
const PRESSED: [u32; 3] = [0xa4, 0xa6, 0xa8];
const PRESSED_GOLD: [u32; 3] = [0xa5, 0xa7, 0xa9];
const HOVER: [u32; 3] = [0xa1, 0xa2, 0xa3];

impl Gui {
    /// `FUN_1006ee1e` (GUI.dll 0x1006ee1e): shows every view with an `activate_criteria`/`criteria` attribute iff its expression holds
    /// (`View::Show(CriteriaMonitor_c::Evaluate())`), then relayouts the window. `res` resolves `dvalue:` / `stat:` names.
    pub fn apply_criteria(&mut self, w: WindowId, res: &dyn Fn(&str, &str) -> Option<i64>) {
        let Some(Some(win)) = self.windows.get(w) else { return };
        let root = win.root;
        let mut stack = vec![root];
        let mut changed = false;
        while let Some(v) = stack.pop() {
            stack.extend(self.tree.views[v].children.iter().copied());
            if self.tree.views[v].criteria.is_empty() {
                continue;
            }
            let show = expr::truthy(&self.tree.views[v].criteria, res);
            if self.tree.views[v].visible != show {
                self.tree.views[v].visible = show;
                changed = true;
            }
        }
        if changed {
            self.relayout_window(w);
        }
    }

    /// Top-left of a window in screen pixels (for framed windows the client origin).
    pub fn window_pos(&self, w: WindowId) -> (i32, i32) {
        self.windows.get(w).and_then(|w| w.as_ref()).map_or((0, 0), |w| w.pos)
    }

    /// Resolves `id:GFX_NAME` (`ExpressionParser_c` skin ids) for [`Gui::apply_criteria`] resolvers.
    pub fn gfx_id(&self, name: &str) -> Option<u32> {
        self.gfx.id(name).map(|g| g.0)
    }

    /// Sets the dvalue state of a `CCMenuEntry` (toggle entries show the pressed art while active).
    pub fn set_cc_active(&mut self, w: WindowId, name: &str, active: bool) {
        if let Some(v) = self.find(w, name) {
            if let Kind::CcEntry(c) = &mut self.tree.views[v].kind {
                c.active = active;
            }
        }
    }

    /// `Button_c::MouseMove` 0x10127869 starts a 30000-us timer; `SlotFadeTimer` adds/subtracts 0.075.
    pub(super) fn tick_cc_fades(&mut self, dt: f32) {
        for v in &mut self.tree.views {
            match &mut v.kind {
                Kind::CcEntry(c) if c.icon.is_some() => tick_icon_fade(&mut c.fade, &mut c.fade_timer, c.hover, dt),
                Kind::Button(b) if b.background_icon.is_some() => tick_icon_fade(&mut b.background_icon_fade, &mut b.background_icon_timer, b.hover, dt),
                _ => {}
            }
        }
    }

    fn slice3(&self, out: &mut Vec<DrawCmd>, ids: [u32; 3], r: Rect, tint: [u8; 3], alpha: f32) {
        let [l, m, rt] = ids.map(GfxId);
        let (wl, wr) = (self.gfx.size(l).0 as f32, self.gfx.size(rt).0 as f32);
        self.push_gfx(out, m, Rect::new(r.l + wl, r.t, r.r - wr, r.b), tint, alpha);
        self.push_gfx(out, l, Rect::new(r.l, r.t, r.l + wl - 1.0, r.b), tint, alpha);
        self.push_gfx(out, rt, Rect::new(r.r - wr + 1.0, r.t, r.r, r.b), tint, alpha);
    }
    pub(super) fn draw_button_background_icon(&self, out: &mut Vec<DrawCmd>, icon: Option<GfxId>, fade: f32, r: Rect, tint: [u8; 3], alpha: f32) {
        if let Some(g) = icon {
            let (w, h) = self.gfx.size(g);
            let l = r.l + ((r.width() + 1.0 - w as f32) * 0.5).floor();
            let t = r.t + ((r.height() + 1.0 - h as f32) * 0.5).floor();
            // UpdateBgIconFade 0x10127bdd; LayoutLabel 0x101278d6: centered, no pressed displacement.
            let grey = 0x20 + ((1.0 - fade) * 223.0) as u32;
            self.push_gfx(out, g, Rect::new(l, t, l + w as f32 - 1.0, t + h as f32 - 1.0), mul(tint, [grey as u8; 3]), alpha * BUTTON_ALPHA);
        }
    }

    /// `Button_c` rendering (`StateChanged` 0x10128338) of a control-centre menu entry: raised/pressed 3-slice art tinted DEFAULT/SELECTED,
    /// the hover set tinted HOVER on top, and the same centered background icon consumer as ordinary Buttons.
    pub(super) fn draw_cc_entry(&mut self, out: &mut Vec<DrawCmd>, c: &CcEntryData, enabled: bool, r: Rect, tint: [u8; 3], alpha: f32) {
        let tint = mul(tint, if enabled { [255; 3] } else { [0x90; 3] });
        let pressed = c.pressed || (c.toggle && c.active);
        let (set, col) = match (pressed, c.golden) {
            (true, false) => (PRESSED, 0x2000000),
            (true, true) => (PRESSED_GOLD, 0x2000000),
            (false, false) => (RAISED, 0x1000000),
            (false, true) => (RAISED_GOLD, 0x1000000),
        };
        let t = mul(tint, self.map_color(col));
        self.slice3(out, set, r, t, alpha * BUTTON_ALPHA);
        if c.hover && enabled {
            let t = mul(tint, self.map_color(0x3000000));
            self.slice3(out, HOVER, r, t, alpha * BUTTON_ALPHA);
        }
        self.draw_button_background_icon(out, c.icon, c.fade, r, tint, alpha);
        // the label view is shown iff fade > 0.8 (`_DAT_101c5778`) once an icon exists; label-only entries always show it
        if !c.label.is_empty() && (c.icon.is_none() || c.fade > 0.8) {
            let s = text::string_size(&mut self.fonts, &self.colors, FontId::Normal, &c.label);
            let (lw, lh) = (s.x + 1.0, s.y + 1.0);
            let lx = r.l + ((r.width() + 1.0 - lw) * 0.5).floor();
            let ly = r.t + ((r.height() + 1.0 - lh) * 0.5).floor();
            let lt = mul(tint, self.map_color(col));
            self.draw_string(out, FontId::Normal, &c.label, lx as i32, ly as i32, lt, alpha, false);
        }
    }
}

fn tick_icon_fade(fade: &mut f32, timer: &mut f32, hover: bool, dt: f32) {
    if (hover && *fade >= 1.0) || (!hover && *fade <= 0.0) {
        *timer = 0.0;
        return;
    }
    *timer += dt;
    while *timer >= 0.03 {
        *timer -= 0.03;
        *fade = (*fade + if hover { 0.075 } else { -0.075 }).clamp(0.0, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::tick_icon_fade;
    #[test]
    fn native_background_icon_timer_ticks() {
        let (mut fade, mut timer) = (0.0, 0.0);
        tick_icon_fade(&mut fade, &mut timer, true, 0.02);
        assert_eq!(fade, 0.0);
        tick_icon_fade(&mut fade, &mut timer, true, 0.011);
        assert_eq!(fade, 0.075);
        tick_icon_fade(&mut fade, &mut timer, false, 0.03);
        assert_eq!(fade, 0.0);
    }
}
