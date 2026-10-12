//! `DialogBox_c` confirmation / message boxes of the HUD windows (team invitation, perk train / untrain confirmations and the perk "Info" boxes).
//! Layout and button row are those of `DialogBoxView_c` 0x1012aa23 / `DialogButtons_c::_Initialize` 0x1012ad5f, read in docs/chat/dialogs.md §2
//! (the chat side keeps its own copy for `/messagebox`, `/org leave`, `/afk`); the dialog opens centred (`DialogBox_c::Go` + `Window::MoveToCenter`).

use ao_gui::{Event, Gui, WindowId, WindowSize};

/// One open dialog and what it was opened for (`tag` is the owner's meaning).
pub(super) struct Open<T> {
    win: WindowId,
    pub tag: T,
    escape: bool,
}

pub(super) struct Dialogs<T> {
    open: Vec<Open<T>>,
    escape: bool,
}

impl<T> Default for Dialogs<T> {
    fn default() -> Self {
        Self { open: vec![], escape: true }
    }
}

/// Wrap width of the body: `TextRenderer_c::SetAspectRatio(2.0)` asks for a text block twice as wide as high; the exact algorithm is not decoded
/// (**GUESS**, same as the chat dialogs): about `sqrt(2 * chars * 7 * 14)` px clamped to 160..=520.
fn wrap_width(body: &str) -> u32 {
    ((2.0 * body.chars().count() as f32 * 7.0 * 14.0).sqrt() as u32).clamp(160, 520)
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn xml(body: &str, buttons: &[String]) -> String {
    let w = wrap_width(body);
    // text borders (15, 5, 15, 20), button row with a leading spacer, 8 px left/right borders and a trailing spacer for a single button
    let mut row = String::from("<View view_layout=\"horizontal\" layout_borders=\"Rect(0,0,0,5)\"><HLayoutSpacer/>");
    for (i, b) in buttons.iter().enumerate() {
        row += &format!("<Button name=\"btn{i}\" label=\"{}\" layout_borders=\"Rect(8,0,8,0)\"/>", escape(b));
    }
    if buttons.len() == 1 {
        row += "<HLayoutSpacer/>";
    }
    row += "</View>";
    format!(
        "<root><View view_layout=\"vertical\"><TextView name=\"text\" feature_flags=\"TVF_MULTILINE|TVF_WORD_WRAP\" value=\"{}\" min_size=\"Point({w},-1)\" max_size=\"Point({w},-1)\" layout_borders=\"Rect(15,5,15,20)\"/>{row}</View></root>",
        escape(body)
    )
}

impl<T: Copy> Dialogs<T> {
    pub(super) fn configure_escape(&mut self, values: &super::dvalue::DValues) {
        self.escape = values.get_i64("esc_dialogs").unwrap_or(1) != 0;
    }

    pub(super) fn go(&mut self, gui: &mut Gui, screen: (u32, u32), tag: T, body: &str, buttons: &[String]) {
        self.go_named(gui, screen, "DialogBox", tag, body, buttons);
    }

    pub(super) fn go_named(&mut self, gui: &mut Gui, screen: (u32, u32), name: &str, tag: T, body: &str, buttons: &[String]) {
        let Ok(win) = gui.open_framed_window_xml(name, &xml(body, buttons), (0, 0), WindowSize::Preferred) else { return };
        gui.set_text(win, "text", body);
        gui.set_default_button(win, "btn0");
        gui.relayout_window(win);
        let (w, h) = gui.outer_size(win);
        gui.set_window_pos(win, ((screen.0 as i32 - w as i32) / 2, (screen.1 as i32 - h as i32) / 2));
        self.open.push(Open { win, tag, escape: self.escape });
    }

    pub(super) fn is_open(&self) -> bool {
        !self.open.is_empty()
    }

    /// The windows of the open dialogs (tests click their buttons).
    #[cfg(test)]
    pub(super) fn windows(&self) -> Vec<WindowId> {
        self.open.iter().map(|o| o.win).collect()
    }

    /// Replaces the body text of the open dialogs (`DialogBox_c` text view; the bind dialog shows the pressed key).
    pub(super) fn set_body(&self, gui: &mut Gui, body: &str) {
        for o in &self.open {
            gui.set_text(o.win, "text", body);
        }
    }

    pub(super) fn close_all(&mut self, gui: &mut Gui) {
        for o in self.open.drain(..) {
            gui.close_window(o.win);
        }
    }

    fn finish(&mut self, gui: &mut Gui, win: WindowId, button: i32) -> Option<(T, i32)> {
        let i = self.open.iter().position(|o| o.win == win)?;
        let o = self.open.remove(i);
        gui.close_window(win);
        Some((o.tag, button))
    }

    /// `(belongs to a dialog, (tag, pressed button; -1 = Esc / close))`.
    pub(super) fn event(&mut self, gui: &mut Gui, ev: &Event) -> (bool, Option<(T, i32)>) {
        let own = |w: &WindowId| self.open.iter().any(|o| o.win == *w);
        match ev {
            Event::Clicked { window, view, .. } if own(window) => {
                let n = view.strip_prefix("btn").and_then(|n| n.parse().ok()).unwrap_or(0);
                (true, self.finish(gui, *window, n))
            }
            // `esc_dialogs` defaults to true: `DialogBox_c::SlotEscPressed` = `SlotSelected(-1)`
            Event::Escape { window } if own(window) && self.open.iter().any(|o| o.win == *window && o.escape) => (true, self.finish(gui, *window, -1)),
            Event::Escape { window } if own(window) => (true, None),
            Event::CloseRequested { window } if own(window) => (true, self.finish(gui, *window, -1)),
            Event::TextChanged { window, .. } | Event::LinkClicked { window, .. } if own(window) => (true, None),
            _ => (false, None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dialog_escape_policy_is_captured_at_go() {
        let dir = ao_gui::client_dir();
        if !dir.join("cd_image/gui").exists() { return; }
        let mut gui = Gui::new(&dir, None).unwrap();
        let mut values = super::super::dvalue::DValues::new(&dir);
        let mut dialogs = Dialogs::default();
        values.set_i64("esc_dialogs", 0);
        dialogs.configure_escape(&values);
        dialogs.go(&mut gui, (800, 600), (), "Question", &["OK".into()]);
        let win = dialogs.windows()[0];
        values.set_i64("esc_dialogs", 1);
        dialogs.configure_escape(&values);
        assert_eq!(dialogs.event(&mut gui, &Event::Escape { window: win }), (true, None));
        assert!(dialogs.is_open());
        assert_eq!(dialogs.event(&mut gui, &Event::CloseRequested { window: win }), (true, Some(((), -1))));
        dialogs.go(&mut gui, (800, 600), (), "Question", &["OK".into()]);
        let win = dialogs.windows()[0];
        values.set_i64("esc_dialogs", 0);
        dialogs.configure_escape(&values);
        assert_eq!(dialogs.event(&mut gui, &Event::Escape { window: win }), (true, Some(((), -1))));
    }
}
