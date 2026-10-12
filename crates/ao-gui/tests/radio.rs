use ao_gui::{DrawCmd, Event, Gui, InputEvent, MouseButton, WindowSize};

#[test]
fn native_radio_view_label_paints_and_selects() {
    let Ok(mut g) = Gui::new(&ao_gui::client_dir(), None) else { return };
    let xml = r#"<root><RadioButtonGroup name="group"><RadioButton name="research" value="7"><View name="row" view_layout="horizontal"><PowerBar name="progress" bg_gfx="id:224" full_gfx="id:225" min_size="Point(80,10)" max_size="Point(80,10)" borders="Rect(5,0,0,0)"/><TextView name="label" value="Research" min_size="Point(90,12)" max_size="Point(90,12)" borders="Rect(5,1,1,1)"/></View></RadioButton></RadioButtonGroup></root>"#;
    let w = g.open_window_xml("radio", xml, (10, 10), WindowSize::Fixed(240, 40)).unwrap();
    let radio = g.view_rect(w, "research").unwrap();
    let row = g.view_rect(w, "row").unwrap();
    let bar = g.view_rect(w, "progress").unwrap();
    let label = g.view_rect(w, "label").unwrap();
    assert_eq!(row.l, radio.l + 11.0);
    assert_eq!(bar.width(), 80.0);
    assert_eq!(bar.height(), 12.0); // GfxId 225's real atlas image height; max_size is a floor when growing here, not a ceiling
    assert_eq!(label.width(), 90.0);
    assert_eq!(label.height(), 12.0);
    let draw = g.frame(0.0);
    assert!(draw.cmds.iter().any(|c| matches!(c, DrawCmd::Glyph { .. })));
    // Clicking either composed child selects and reports the enclosing radio, not its label.
    assert!(draw.cmds.iter().any(|c| matches!(c, DrawCmd::Gfx { id: ao_gui::GfxId(225), .. })));
    for r in [bar, label] {
        let (x, y) = (r.l + 2.0, r.t + 2.0);
        g.set_radio_value(w, "group", 0);
        g.input(InputEvent::MouseDown { x, y, button: MouseButton::Left });
        let events = g.input(InputEvent::MouseUp { x, y, button: MouseButton::Left });
        assert_eq!(g.radio_value(w, "group"), Some(7));
        assert!(events.iter().any(|e| matches!(e, Event::Clicked { view, .. } if view == "research")));
    }
}
