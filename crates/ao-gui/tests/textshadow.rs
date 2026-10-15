use ao_gui::{DrawCmd, Gui, WindowSize};

#[test]
fn readability_shadow_keeps_text_local_alpha_and_timed_line_age() {
    let Ok(mut gui) = Gui::new(&ao_gui::client_dir(), None) else { return };
    let xml = r#"<root><View view_layout="vertical"><ScrollView name="scroll" max_size="Point(16000,16000)">
        <ScrollViewChild view_layout="vertical" max_size="Point(16000,16000)">
        <TextView name="text" view_flags="0x80" font="CHAT" max_size="Point(16000,-1)"
            feature_flags="TVF_WORD_WRAP|TVF_MULTILINE|TVF_ENABLE_SHADOW"/>
        </ScrollViewChild></ScrollView></View></root>"#;
    let id = gui.open_window_xml("shadow", xml, (10, 10), WindowSize::Fixed(300, 200)).unwrap();
    gui.set_view_alpha(id, "text", 0.25);
    gui.set_window_alpha(id, 0.05);
    gui.set_text(id, "text", "MMMM local");
    gui.set_text_shadow(id, "text", true);
    let glyphs = |gui: &mut Gui, dt| gui.frame(dt).cmds.into_iter().filter_map(|c| match c {
        DrawCmd::Glyph { tint, alpha, .. } => Some((tint, alpha)), _ => None,
    }).collect::<Vec<_>>();
    let local = glyphs(&mut gui, 0.0);
    assert!(local.iter().any(|(c, _)| *c == [0; 3]));
    assert!(local.iter().any(|(c, _)| *c != [0; 3]));
    assert!(local.iter().all(|(_, a)| *a == 0.25));
    gui.set_text_fade(id, "scroll", Some((8.0, 0.3)), "MMMM first");
    gui.add_fade_line(id, "scroll", "MMMM second");
    gui.set_text_shadow(id, "scroll", true);
    let timed = glyphs(&mut gui, 8.15);
    assert!(timed.iter().any(|(c, _)| *c == [0; 3]));
    assert!(timed.iter().all(|(_, a)| (*a - 0.5).abs() < 0.02));
    gui.set_text_shadow(id, "scroll", false);
    let unshadowed = glyphs(&mut gui, 0.0);
    assert!(!unshadowed.is_empty());
    assert!(unshadowed.iter().all(|(c, a)| *c != [0; 3] && (*a - 0.5).abs() < 0.02));
    assert_eq!(gui.fade_line_count(id, "scroll"), 2);
}
