//! HelpButtonWindow_c 10071d87/10072169 and TipSystem_t 10053edd; native XML, not /tipoftheday.
use super::dvalue::DValues;
use super::hud_wincfg::{Store, WinCfgs};
use ao_gui::{Event, Gui, WindowId, WindowSize};

#[derive(Default)]
pub(super) struct HelpButton {
    window: Option<WindowId>,
    /// `InputConfig_t::SetStaticInputMode(0x39)` in `StartHelpLayerMessage` 1001bd39.
    help_mode: bool,
}
impl HelpButton {
    pub(super) fn help_mode(&self) -> bool { self.help_mode }
    pub(super) fn update(&mut self, gui: &mut Gui, values: &DValues, cfg: &mut WinCfgs, screen: (u32, u32)) {
        if !values.flag("ShowHelpButtonWindow") {
            if let Some(w) = self.window.take() { gui.close_window(w); }
        } else if self.window.is_none() {
            // The original has a literal text button, not GFX_GUI_BUTTON_HELP (10071d87).
            // SetBorders(0,20,0,0), float101b4e08, in10071d87.
            let xml = "<root><View><Button name=\"Help\" label=\"Help\" layout_borders=\"Rect(0,20,0,0)\"/></View></root>";
            match gui.open_framed_window_xml("HelpButtonWindow", xml, (0, 0), WindowSize::Preferred) {
                Ok(w) => {
                    gui.set_window_frame(w, true, false);
                    cfg.attach_hud(gui, values, w, Store::Value("HelpButtonWindowConfig"), screen);
                    self.window = Some(w);
                }
                Err(e) => eprintln!("hud: help button: {e:#}"),
            }
        }
    }
    pub(super) fn event(&mut self, gui: &mut Gui, values: &mut DValues, event: &Event) -> Option<bool> {
        match event {
            Event::Clicked { window, view, .. } if Some(*window) == self.window && view == "Help" => {
                self.help_mode = true;
                Some(true)
            }
            Event::CloseRequested { window } if Some(*window) == self.window => {
                gui.close_window(*window);
                self.window = None;
                // HelpButtonWindow destructor 10072064.
                values.set_i64("ShowHelpButtonWindow", 0);
                Some(false)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_help_button_option_click_and_close() {
        let dir = ao_gui::client_dir();
        if !dir.join("cd_image/gui/Default/Graphics.uvgi").exists() { return; }
        let mut gui = Gui::new(&dir, None).unwrap();
        let mut values = DValues::new(&dir);
        let mut cfg = WinCfgs::new(&dir);
        let mut help = HelpButton::default();
        values.set_i64("ShowHelpButtonWindow", 1);
        help.update(&mut gui, &values, &mut cfg, (1280, 800));
        let window = help.window.unwrap();
        assert_eq!(help.event(&mut gui, &mut values, &Event::Clicked { window, view: "Help".into(), item: None }), Some(true));
        assert!(help.help_mode());
        assert_eq!(help.event(&mut gui, &mut values, &Event::CloseRequested { window }), Some(false));
        assert!(!values.flag("ShowHelpButtonWindow"));
        // Closing the floater does not clear a static input mode.
        assert!(help.help_mode());
        help.update(&mut gui, &values, &mut cfg, (1280, 800));
        assert!(help.window.is_none());
    }
}
