//! Headless harness input planning; all value changes still pass through GUI input/events.
use super::*;

impl HudOptions {
    pub(in crate::play) fn probe_value(&self, d: &DValues, name: &str) -> f64 {
        let opt = self.pages.iter().flat_map(Page::opts).find(|o| o.var == name).expect("unknown native option");
        if opt.ctl == Ctl::Check { f64::from(u8::from(opt.checked(d))) } else { opt.value(d) }
    }

    pub(in crate::play) fn probe_inputs(&mut self, gui: &mut Gui, d: &mut DValues, name: &str, value: f64) -> Vec<InputEvent> {
        assert!(value.is_finite(), "option value must be finite");
        self.open(gui, d);
        let w = self.win.as_ref().expect("Options window");
        let c = w.controls.iter().find(|c| c.opt.var == name).expect("unknown native option");
        let (id, page, opt, control, leaf) = (w.id, c.page, c.opt.clone(), c.name.clone(), w.leaves[c.page].clone());
        gui.select_child(id, TABS, Some(0));
        gui.list_select(id, TREE, &leaf, true, false);
        // Use the same category event handler as StringListView, never set the bound store.
        self.event(gui, &Event::ListSelected { window: id, view: TREE.into(), id: leaf, selected: true }, d);
        self.update(gui, d, &|_, _| None);
        let view = match &opt.ctl {
            Ctl::Radio(values) => {
                assert!(values.iter().any(|(_, v)| f64::from(*v) == value), "invalid native radio value");
                format!("{control}_{}", value as i32)
            }
            _ => control.clone(),
        };
        let pg = format!("pg{page}");
        let r = gui.view_rect(id, &view).expect("option control rectangle");
        let viewport = gui.view_rect(id, &pg).expect("option viewport");
        gui.set_scroll_offset(id, &pg, gui.scroll_offset(id, &pg) + (r.t - viewport.t).max(0.0));
        let r = gui.view_rect(id, &view).expect("scrolled option rectangle");
        assert!(gui.is_enabled(id, &view), "native option is disabled: {name}");
        eprintln!("option category={} variable={name} control={view} native_type={:?} store={:?}", self.pages[page].label, opt.ty, opt.kind);
        let y = (r.t + r.b) * 0.5;
        let (from, to) = match opt.ctl {
            Ctl::Slider => {
                let (lo, hi) = opt.range(d);
                assert!(value >= lo && value <= hi && hi > lo, "option outside native range");
                let travel = (r.r - r.l + 1.0 - 11.0).max(1.0);
                // Int bindings truncate the continuous native slider value; aim inside its integer interval.
                let target = if opt.ty == model::Ty::Int || matches!(d.get(name), Some(Variant::Int(_))) { (value + if value < 0.0 { -0.5 } else { 0.5 }).clamp(lo, hi) } else { value };
                (r.l + ((opt.value(d) - lo) * f64::from(travel) / (hi - lo)).floor() as f32 + 5.0,
                 r.l + ((target - lo) * f64::from(travel) / (hi - lo)) as f32 + 5.0)
            }
            Ctl::Check => {
                assert!(value == 0.0 || value == 1.0, "check value must be 0 or 1");
                if opt.checked(d) == (value != 0.0) { return vec![]; }
                ((r.l + r.r) * 0.5, (r.l + r.r) * 0.5)
            }
            Ctl::Radio(_) => ((r.l + r.r) * 0.5, (r.l + r.r) * 0.5),
        };
        vec![InputEvent::MouseMove { x: from, y }, InputEvent::MouseDown { x: from, y, button: MouseButton::Left }, InputEvent::MouseMove { x: to, y }, InputEvent::MouseUp { x: to, y, button: MouseButton::Left }]
    }
}
