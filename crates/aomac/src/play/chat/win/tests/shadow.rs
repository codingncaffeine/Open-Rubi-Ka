use super::*;
use ao_gui::{DrawCmd, Rect};

fn shadows(gui: &mut Gui, rect: Rect) -> usize {
    gui.frame(0.0).cmds.iter().filter(|cmd| matches!(cmd,
        DrawCmd::Glyph { dst, tint: [0, 0, 0], .. }
            if dst[0] as f32 >= rect.l && (dst[0] as f32) <= rect.r
                && dst[1] as f32 >= rect.t && (dst[1] as f32) <= rect.b
    )).count()
}

#[test]
fn chat_readability_shadow_tracks_rendered_alpha_not_configured_targets() {
    let Some((mut gui, mut ch)) = rig((1280, 800)) else { return };
    let fi = ch.wins[0].frame;
    ch.wins[0].cfg.fading = false;
    ch.wins[0].cfg.hide_input_when_inactive = false;
    ch.rebuild_frame(&mut gui, fi);
    ch.update(&mut gui, 0.0);
    let id = ch.frames[fi].id;
    gui.set_text(id, "text_0", "MMMM output");
    gui.set_text(id, "input_0", "MMMM editor");
    let output = gui.view_rect(id, "scroll_0").unwrap();
    let input = gui.view_rect(id, "input_0").unwrap();
    for level in [0.399, 0.4, 0.401, 0.05, 1.0] {
        ch.set_frame_alpha(&mut gui, fi, level);
        assert_eq!(shadows(&mut gui, output) > 0, level < 0.4, "output at {level}");
        assert_eq!(shadows(&mut gui, input) > 0, level < 0.4, "editor at {level}");
        gui.set_text(id, "input_0", "");
        gui.set_text_hint(id, "input_0", "MMMM hint");
        assert_eq!(shadows(&mut gui, input) > 0, level < 0.4, "prompt at {level}");
        gui.set_text(id, "input_0", "MMMM editor");
    }
    // Activation crosses the threshold before reaching its 0.8 target.
    ch.wins[0].alpha = 0.3;
    ch.wins[0].target = 0.3;
    ch.set_frame_alpha(&mut gui, fi, 0.3);
    ch.focus_input(&mut gui);
    ch.update(&mut gui, 0.02);
    assert!(gui.window_alpha(id) < 0.4);
    assert!(shadows(&mut gui, output) > 0);
    ch.update(&mut gui, 0.04);
    assert!(gui.window_alpha(id) > 0.4 && gui.window_alpha(id) < 0.8);
    assert_eq!(shadows(&mut gui, output), 0);

    // Existing and subsequently appended timed renderers toggle together, without resetting their age.
    gui.set_text_fade(id, "scroll_0", Some((8.0, 0.3)), "MMMM first");
    gui.add_fade_line(id, "scroll_0", "MMMM second");
    for level in [0.399, 0.4, 0.05] {
        ch.set_frame_alpha(&mut gui, fi, level);
        assert_eq!(gui.fade_line_count(id, "scroll_0"), 2);
        assert_eq!(shadows(&mut gui, output) > 0, level < 0.4, "timed lines at {level}");
    }
    gui.add_fade_line(id, "scroll_0", "MMMM third");
    assert!(shadows(&mut gui, output) > 0);
}
