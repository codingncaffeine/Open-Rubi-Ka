use super::*;
use super::super::menu::{OP_SLIDER_ACTIVE, OP_SLIDER_INACTIVE};
use ao_gui::{DrawCmd, DrawList, Rect};

fn inside(r: Rect, x: f32, y: f32) -> bool {
    x >= r.l && x <= r.r && y >= r.t && y <= r.b
}

fn assert_chat_alpha(gui: &mut Gui, w: WindowId, input_alpha: f32) -> DrawList {
    let output = gui.view_rect(w, "scroll_0").unwrap();
    let input = gui.view_rect(w, "input_0").unwrap();
    let scroll = gui.view_rect(w, "scroll_0").unwrap();
    let background = gui.view_rect(w, "text_border_0").unwrap();
    let window_alpha = gui.window_alpha(w);
    let list = gui.frame(0.0);
    let (mut output_count, mut input_count, mut backgrounds, mut scrollbars) = (0, 0, 0, 0);
    for cmd in &list.cmds {
        match cmd {
            DrawCmd::Glyph { dst, alpha, .. } => {
                if inside(output, dst[0] as f32, dst[1] as f32) {
                    assert_eq!(*alpha, 1.0, "regular chat output must reset inherited alpha");
                    output_count += 1;
                } else if inside(input, dst[0] as f32, dst[1] as f32) {
                    assert_eq!(*alpha, input_alpha, "editor/prompt keeps its own alpha");
                    input_count += 1;
                }
            }
            DrawCmd::Gfx { dst, alpha, .. } => {
                if dst[0] == background.l && dst[1] == background.t && dst[2] > background.r {
                    assert_eq!(*alpha, window_alpha, "chat background must still fade");
                    backgrounds += 1;
                }
                if dst[0] > scroll.r - 20.0 && dst[0] < scroll.r && inside(scroll, dst[0], dst[1]) {
                    assert_eq!(*alpha, window_alpha, "scrollbar must still fade");
                    scrollbars += 1;
                }
            }
            _ => {}
        }
    }
    assert!(output_count > 0 && input_count > 0 && backgrounds > 0 && scrollbars > 0,
        "nonvacuous output/editor/background/scrollbar checks: {output_count}/{input_count}/{backgrounds}/{scrollbars}");
    list
}

#[test]
fn chat_output_editor_and_hint_reset_window_alpha_across_sliders_and_focus() {
    let Some((mut gui, mut ch)) = rig((1280, 800)) else { return };
    ch.wins[0].cfg.fading = false;
    ch.wins[0].cfg.hide_input_when_inactive = false;
    ch.test_pick(&mut gui, 0, OP_MODE, 2);
    let w = ch.wins[0].id;
    gui.set_text(w, "text_0", "MMMM output");
    gui.set_text(w, "input_0", "MMMM editor");
    gui.clear_focus();
    ch.update(&mut gui, 1.0);
    ch.event(&mut gui, &Event::ContextMenu { window: w, x: 10, y: 10, link: None });
    for (op, value) in [(OP_SLIDER_INACTIVE, 0.1), (OP_SLIDER_ACTIVE, 0.2)] {
        ch.event(&mut gui, &Event::MenuSlider { id: op << 16, value, finished: false });
        assert_eq!(gui.window_alpha(w), value);
        assert_chat_alpha(&mut gui, w, 1.0);
        gui.set_text(w, "input_0", "");
        gui.set_text_hint(w, "input_0", "MMMM hint");
        assert_chat_alpha(&mut gui, w, 0.6);
        gui.set_text(w, "input_0", "MMMM editor");
    }
    gui.close_menu();
    for event in gui.input(InputEvent::MouseMove { x: 1200.0, y: 750.0 }) {
        ch.event(&mut gui, &event);
    }
    ch.update(&mut gui, 1.0);
    assert!((gui.window_alpha(w) - 0.1).abs() < 1e-6);
    ch.focus_input(&mut gui);
    ch.update(&mut gui, 0.0);
    for _ in 0..4 {
        ch.update(&mut gui, 0.05);
        assert_chat_alpha(&mut gui, w, 1.0);
    }
    assert!((gui.window_alpha(w) - 0.2).abs() < 1e-6);
    gui.clear_focus();
    ch.update(&mut gui, 0.0);
    for _ in 0..5 {
        ch.update(&mut gui, 0.2);
        assert_chat_alpha(&mut gui, w, 1.0);
    }
    assert!((gui.window_alpha(w) - 0.1).abs() < 1e-6);
    // The same XML constructor is used when changing visual mode/rebuilding the tab.
    ch.test_pick(&mut gui, 0, OP_MODE, 0);
    ch.test_pick(&mut gui, 0, OP_MODE, 2);
    let w = ch.wins[0].id;
    gui.set_text(w, "text_0", "MMMM rebuilt output");
    gui.set_text(w, "input_0", "MMMM rebuilt editor");
    gui.set_window_alpha(w, 0.05);
    assert_chat_alpha(&mut gui, w, 1.0);
}

#[test]
fn chat_low_window_alpha_preserves_rendered_text_pixels() {
    let size = (1280, 800);
    let Some((mut gui, mut ch)) = rig(size) else { return };
    ch.wins[0].cfg.fading = false;
    ch.wins[0].cfg.hide_input_when_inactive = false;
    ch.test_pick(&mut gui, 0, OP_MODE, 2);
    let w = ch.wins[0].id;
    gui.set_text(w, "text_0", "<font color=#FFFFFF>MMMM output</font>");
    gui.clear_focus();
    struct Fe(Gui);
    impl ao_render::Frontend for Fe {
        fn gui(&self) -> &Gui { &self.0 }
        fn input(&mut self, ev: InputEvent, _: &mut ao_render::Host) { self.0.input(ev); }
        fn frame(&mut self, dt: f32, _: (u32, u32), _: &mut ao_render::Host) -> DrawList { self.0.frame(dt) }
    }
    let mut fe = Fe(gui);
    let mut offscreen = ao_render::Offscreen::new(&fe, size).unwrap();
    let dir = test_dir().join("opacity-pixels");
    std::fs::create_dir_all(&dir).unwrap();
    let output = fe.0.view_rect(w, "scroll_0").unwrap();
    let input = fe.0.view_rect(w, "input_0").unwrap();
    for hint in [false, true] {
        fe.0.set_text(w, "input_0", if hint { "" } else { "MMMM editor" });
        fe.0.set_text_hint(w, "input_0", "MMMM hint");
        let mut images = Vec::new();
        for alpha in [0.05, 1.0] {
            fe.0.set_window_alpha(w, alpha);
            let mut actual = fe.0.frame(0.0);
            actual.cmds.insert(0, DrawCmd::Solid { dst: [0.0, 0.0, size.0 as f32, size.1 as f32], color: [80, 120, 160], alpha: 1.0 });
            let mut expected = actual.clone();
            let mut glyphs = 0;
            for cmd in &mut expected.cmds {
                if let DrawCmd::Glyph { dst, alpha, .. } = cmd {
                    if inside(output, dst[0] as f32, dst[1] as f32) {
                        *alpha = 1.0;
                        glyphs += 1;
                    } else if inside(input, dst[0] as f32, dst[1] as f32) {
                        *alpha = if hint { 0.6 } else { 1.0 };
                        glyphs += 1;
                    }
                }
            }
            assert!(glyphs > 0);
            let render = |offscreen: &mut ao_render::Offscreen, list: &DrawList, name: &str| {
                let path = dir.join(name);
                offscreen.png(&fe, list, &path).unwrap();
                let pixels = image::open(&path).unwrap().to_rgba8();
                std::fs::remove_file(path).unwrap();
                pixels
            };
            let actual_pixels = render(&mut offscreen, &actual, "actual.png");
            let expected_pixels = render(&mut offscreen, &expected, "expected.png");
            assert_eq!(actual_pixels, expected_pixels, "window alpha {alpha}, hint={hint}: rendered pixels must match independently opaque text (prompt 0.6)");
            let mut blank = actual.clone();
            blank.cmds.retain(|cmd| !matches!(cmd, DrawCmd::Glyph { dst, .. } if inside(output, dst[0] as f32, dst[1] as f32) || inside(input, dst[0] as f32, dst[1] as f32)));
            let blank_pixels = render(&mut offscreen, &blank, "blank.png");
            assert!(actual_pixels.pixels().zip(blank_pixels.pixels()).any(|(a, b)| a != b), "text must contribute real pixels");
            images.push(actual_pixels);
        }
        assert!(images[0].pixels().zip(images[1].pixels()).any(|(a, b)| a != b), "background/scrollbar pixels must still respond to window alpha");
        // Fully covered white output strokes are unchanged even over the fading background.
        assert!((output.t.max(0.0) as u32..=(output.b as u32).min(size.1 - 1)).any(|y|
            (output.l.max(0.0) as u32..=(output.r as u32).min(size.0 - 1)).any(|x|
                images[0].get_pixel(x, y).0 == [255; 4] && images[1].get_pixel(x, y).0 == [255; 4])),
            "opaque chat strokes must remain white at low window alpha");
    }
    std::fs::remove_dir(dir).unwrap();
}
