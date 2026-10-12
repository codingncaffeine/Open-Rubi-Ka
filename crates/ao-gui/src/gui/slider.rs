//! Native Slider_c (GUI 0x1014448a), extracted from the options slider port.
use super::*;

impl Gui {
    pub fn slider_value(&self, w: WindowId, name: &str) -> Option<f64> {
        match &self.tree.views[self.find(w, name)?].kind { Kind::Slider(s) => Some(s.value), _ => None }
    }

    /// `SetValue(value, false)`: observer updates do not emit a signal.
    pub fn set_slider_value(&mut self, w: WindowId, name: &str, value: f64) {
        if let Some(v) = self.find(w, name) { self.slider_set(v, value, false); }
    }

    pub fn set_slider_range(&mut self, w: WindowId, name: &str, min: f64, max: f64) {
        if !min.is_finite() || !max.is_finite() || max < min { return; }
        if let Some(v) = self.find(w, name) {
            if let Kind::Slider(s) = &mut self.tree.views[v].kind { s.min = min; s.max = max; s.value = s.value.clamp(min, max); }
        }
    }

    fn slider_set(&mut self, v: ViewId, value: f64, notify: bool) {
        if !value.is_finite() { return; }
        if let Kind::Slider(s) = &mut self.tree.views[v].kind {
            let value = value.clamp(s.min, s.max);
            if s.value == value { return; }
            s.value = value;
            if notify { self.slider_emit(v); }
        }
    }

    fn slider_emit(&mut self, v: ViewId) {
        if let (Some(window), Kind::Slider(s)) = (self.window_of(v), &self.tree.views[v].kind) {
            self.events.push(Event::SliderChanged { window, view: self.tree.views[v].name.clone(), value: s.value, final_change: s.grab.is_none() });
        }
    }

    pub(super) fn draw_slider(&self, out: &mut Vec<DrawCmd>, s: &SliderData, rect: Rect, alpha: f32) {
        let bg = s.background;
        let knob = s.knob;
        let (bw, bh) = self.gfx.size(bg);
        let (kw, kh) = self.gfx.size(knob);
        out.push(DrawCmd::Gfx { id: bg, src: [0.0, 0.0, bw as f32, bh as f32], dst: [rect.l, rect.t, rect.r + 1.0, rect.b + 1.0], tint: [255; 3], alpha });
        let x = rect.l + s.knob_x(rect.width() + 1.0, kw as f32);
        let y = rect.t + ((rect.height() + 1.0 - kh as f32) * 0.5).floor();
        out.push(DrawCmd::Gfx { id: knob, src: [0.0, 0.0, kw as f32, kh as f32], dst: [x, y, x + kw as f32, y + kh as f32], tint: self.map_color(0x1000000), alpha });
    }

    pub(super) fn slider_down(&mut self, v: ViewId, x: f32, y: f32) {
        let Some(w) = self.window_of(v) else { return };
        let o = self.origin(v);
        let pos = self.windows[w].as_ref().unwrap().pos;
        let frame = self.tree.views[v].frame;
        let r = Rect::new(o.0 + pos.0 as f32, o.1 + pos.1 as f32, o.0 + pos.0 as f32 + frame.width(), o.1 + pos.1 as f32 + frame.height());
        let Kind::Slider(data) = &self.tree.views[v].kind else { return };
        let (kw, kh) = self.gfx.size(data.knob);
        if let Kind::Slider(s) = &mut self.tree.views[v].kind {
            let left = r.l + s.knob_x(r.width() + 1.0, kw as f32);
            let top = r.t + ((r.height() + 1.0 - kh as f32) * 0.5).floor();
            if x >= left && x < left + kw as f32 && y >= top && y < top + kh as f32 {
                s.grab = Some(x - left);
                self.pressed = Some(v);
            }
        }
    }

    pub(super) fn drag_slider(&mut self, x: f32) {
        let Some(v) = self.pressed else { return };
        let Kind::Slider(s) = &self.tree.views[v].kind else { return };
        let Some(grab) = s.grab else { return };
        let Some(w) = self.window_of(v) else { return };
        let left = self.origin(v).0 + self.windows[w].as_ref().unwrap().pos.0 as f32;
        let travel = self.tree.views[v].frame.width() + 1.0 - self.gfx.size(s.knob).0 as f32;
        if travel > 0.0 { self.slider_set(v, f64::from(x - grab - left) * (s.max - s.min) / f64::from(travel) + s.min, true); }
    }

    pub(super) fn slider_up(&mut self, v: ViewId) {
        if let Kind::Slider(s) = &mut self.tree.views[v].kind {
            if s.grab.take().is_some() { self.slider_emit(v); }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_xml_slider_set_range_drag_and_signal() {
        let dir = crate::client_dir();
        if !dir.join("cd_image/gui/Default/Graphics.uvgi").exists() { return; }
        let mut g = Gui::new(&dir, None).unwrap();
        let w = g.open_window_xml("slider", r#"<root><View><Slider name="s" min_value="-100" max_value="100" default_value="0"/></View></root>"#, (0, 0), WindowSize::Fixed(128, 18)).unwrap();
        assert_eq!(g.slider_value(w, "s"), Some(0.0));
        g.set_slider_value(w, "s", 300.0);
        assert_eq!(g.slider_value(w, "s"), Some(100.0));
        assert!(g.events.is_empty());
        g.set_slider_range(w, "s", 0.0, 100.0);
        g.set_slider_value(w, "s", 0.0);
        g.input(InputEvent::MouseDown { x: 4.0, y: 5.0, button: MouseButton::Left });
        let events = g.input(InputEvent::MouseMove { x: 62.5, y: 5.0 });
        assert_eq!(g.slider_value(w, "s"), Some(50.0));
        assert!(matches!(events.last(), Some(Event::SliderChanged { value: 50.0, final_change: false, .. })));
        assert!(g.input(InputEvent::MouseMove { x: 62.5, y: 5.0 }).is_empty());
        g.input(InputEvent::MouseMove { x: 1000.0, y: 1000.0 });
        assert_eq!(g.slider_value(w, "s"), Some(100.0));
        let events = g.input(InputEvent::MouseUp { x: 1000.0, y: 1000.0, button: MouseButton::Left });
        assert!(matches!(events.last(), Some(Event::SliderChanged { value: 100.0, final_change: true, .. })));
        g.input(InputEvent::MouseMove { x: 0.0, y: 0.0 });
        assert_eq!(g.slider_value(w, "s"), Some(100.0));
        let mut cmds = Vec::new();
        let Kind::Slider(s) = &g.tree.views[g.find(w, "s").unwrap()].kind else { panic!() };
        g.draw_slider(&mut cmds, s, Rect::new(0.0, 0.0, 127.0, 17.0), 1.0);
        assert!(matches!(cmds[1], DrawCmd::Gfx { id: GfxId(0x91), dst: [117.0, 0.0, 128.0, 18.0], .. }));
    }
}
