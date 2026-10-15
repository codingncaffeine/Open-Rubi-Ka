use super::*;
use super::super::menu::{OP_BEHIND, OP_ON_TOP, OP_OPEN_CHAT, OP_SEND_TELL, OP_SLIDER_ACTIVE, OP_SLIDER_INACTIVE};

#[test]
fn transparency_slider_events_preview_then_restore_on_release_or_close() {
    let Some((mut gui, mut ch)) = rig((1280, 800)) else { return };
    let w = ch.frames[0].id;
    ch.test_pick(&mut gui, 0, OP_MODE, 2);
    gui.clear_focus();
    ch.update(&mut gui, 1.0);
    assert!(!ch.wins[0].active);
    ch.event(&mut gui, &Event::ContextMenu { window: w, x: 10, y: 10, link: None });
    // Exercise the actual two retail slider entries through GUI press/drag dispatch.
    // Open their submenu directly so the check does not depend on translated ancestor label widths.
    let sliders = ch.window_menu(0)[1].sub.clone();
    assert_eq!(sliders.iter().map(|s| s.id).collect::<Vec<_>>(), [OP_SLIDER_INACTIVE << 16, OP_SLIDER_ACTIVE << 16]);
    ch.menu_generation = Some(gui.open_menu((10, 10), (1280, 800), sliders));
    let row = gui.font_height(ao_gui::FontId::Normal) as f32 + 2.0;
    let y = 12.0 + row * 1.5;
    for input in [
        InputEvent::MouseDown { x: 100.0, y, button: ao_gui::MouseButton::Left },
        InputEvent::MouseMove { x: 75.0, y },
    ] {
        let events = gui.input(input);
        let value = events.iter().find_map(|event| match event {
            Event::MenuSlider { id, value, finished: false } if *id == OP_SLIDER_ACTIVE << 16 => Some(*value),
            _ => None,
        }).expect("the second slider must emit its active-alpha event");
        for event in &events {
            ch.event(&mut gui, event);
        }
        ch.update(&mut gui, 0.016);
        assert_eq!(gui.window_alpha(w), value);
        assert_eq!(ch.wins[0].cfg.alpha_active, value);
        assert!(!ch.wins[0].active);
    }
    let preview = ch.wins[0].alpha;
    let events = gui.input(InputEvent::MouseUp { x: 75.0, y, button: ao_gui::MouseButton::Left });
    assert_eq!(events, [Event::MenuSlider { id: OP_SLIDER_ACTIVE << 16, value: preview, finished: true }]);
    for event in &events {
        ch.event(&mut gui, event);
    }
    assert!(gui.menu_open());
    assert!(ch.menu.is_some());
    assert_eq!(ch.wins[0].target, ch.wins[0].cfg.alpha_inactive);
    assert_eq!(ch.wins[0].rate, (ch.wins[0].target - preview).abs() / FADE_OUT);
    ch.update(&mut gui, 1.0);
    assert_eq!(gui.window_alpha(w), ch.wins[0].cfg.alpha_inactive, "release restores inactive alpha without closing");
    // Inactive window: even the second (active) slider previews its own value immediately.
    for (op, value) in [(OP_SLIDER_ACTIVE, 0.95), (OP_SLIDER_INACTIVE, 0.15), (OP_SLIDER_ACTIVE, 0.65)] {
        ch.event(&mut gui, &Event::MenuSlider { id: op << 16, value, finished: false });
        assert_eq!(gui.window_alpha(w), value);
        ch.update(&mut gui, 0.016);
        assert_eq!(gui.window_alpha(w), value, "menu update must not restore the inactive alpha");
    }
    assert_eq!((ch.wins[0].cfg.alpha_inactive, ch.wins[0].cfg.alpha_active), (0.15, 0.65));
    gui.close_menu();
    for event in gui.input(InputEvent::MouseMove { x: 0.0, y: 0.0 }) {
        ch.event(&mut gui, &event);
    }
    ch.update(&mut gui, 1.0);
    assert_eq!(gui.window_alpha(w), 0.15, "closing the menu restores the inactive alpha");
    ch.focus_input(&mut gui);
    ch.update(&mut gui, 1.0);
    assert!(ch.wins[0].active);
    ch.event(&mut gui, &Event::ContextMenu { window: w, x: 10, y: 10, link: None });
    let sliders = ch.window_menu(0)[1].sub.clone();
    ch.menu_generation = Some(gui.open_menu((10, 10), (1280, 800), sliders));
    let y = 12.0 + row * 0.5;
    for input in [
        InputEvent::MouseDown { x: 100.0, y, button: ao_gui::MouseButton::Left },
        InputEvent::MouseMove { x: 75.0, y },
    ] {
        let events = gui.input(input);
        let value = events.iter().find_map(|event| match event {
            Event::MenuSlider { id, value, finished: false } if *id == OP_SLIDER_INACTIVE << 16 => Some(*value),
            _ => None,
        }).expect("the first slider must emit its inactive-alpha preview");
        for event in &events {
            ch.event(&mut gui, event);
        }
        ch.update(&mut gui, 0.016);
        assert!(ch.wins[0].active);
        assert_eq!(gui.window_alpha(w), value);
        assert_eq!(ch.wins[0].cfg.alpha_inactive, value);
    }
    let preview = ch.wins[0].alpha;
    let events = gui.input(InputEvent::MouseUp { x: 75.0, y, button: ao_gui::MouseButton::Left });
    assert_eq!(events, [Event::MenuSlider { id: OP_SLIDER_INACTIVE << 16, value: preview, finished: true }]);
    for event in &events {
        ch.event(&mut gui, event);
    }
    assert!(gui.menu_open());
    assert!(ch.menu.is_some());
    assert_eq!(ch.wins[0].target, 0.65);
    assert_eq!(ch.wins[0].rate, (0.65 - preview).abs() / FADE_IN);
    ch.update(&mut gui, 1.0);
    assert_eq!(gui.window_alpha(w), 0.65, "release restores active alpha without closing");
    ch.event(&mut gui, &Event::MenuSlider { id: OP_SLIDER_INACTIVE << 16, value: 0.25, finished: false });
    ch.update(&mut gui, 1.0);
    assert_eq!(gui.window_alpha(w), 0.25, "inactive slider must also preview while active");
    gui.close_menu();
    for event in gui.input(InputEvent::MouseMove { x: 0.0, y: 0.0 }) {
        ch.event(&mut gui, &event);
    }
    ch.update(&mut gui, 1.0);
    assert_eq!(gui.window_alpha(w), 0.65, "closing the menu restores the active alpha");
    gui.clear_focus();
    ch.update(&mut gui, 1.0);
    ch.focus_input(&mut gui);
    ch.update(&mut gui, 1.0);
    assert_eq!(gui.window_alpha(w), 0.65, "next activation restores the configured active value");
    gui.clear_focus();
    ch.update(&mut gui, 1.0);
    assert_eq!(gui.window_alpha(w), 0.25);
    let _ = std::fs::remove_dir_all(test_dir());
}

#[test]
fn always_behind_and_on_top_are_exclusive_but_disable_only_their_own_flag() {
    let Some((mut gui, mut ch)) = rig((1280, 800)) else { return };
    ch.wins[0].cfg.frontmost = false;
    ch.wins[0].cfg.backmost = false;
    for (op, expected) in [
        (OP_BEHIND, (true, false)),
        (OP_ON_TOP, (false, true)),
        (OP_ON_TOP, (false, false)),
        (OP_ON_TOP, (false, true)),
        (OP_BEHIND, (true, false)),
        (OP_BEHIND, (false, false)),
    ] {
        ch.test_pick(&mut gui, 0, op, 0);
        assert_eq!((ch.wins[0].cfg.backmost, ch.wins[0].cfg.frontmost), expected);
    }
    // A legacy config can contain both bits: disabling one must leave the other alone.
    for (op, expected) in [(OP_BEHIND, (false, true)), (OP_ON_TOP, (true, false))] {
        ch.wins[0].cfg.backmost = true;
        ch.wins[0].cfg.frontmost = true;
        ch.test_pick(&mut gui, 0, op, 0);
        assert_eq!((ch.wins[0].cfg.backmost, ch.wins[0].cfg.frontmost), expected);
    }
    let _ = std::fs::remove_dir_all(test_dir());
}

#[test]
fn talk_to_requires_subscription_and_preserves_server_readonly_flags() {
    let Some((mut gui, mut ch)) = rig((1280, 800)) else { return };
    let channel = 3u64 << 32 | 1;
    ch.add_group(channel, "Clan OOC", 2);
    assert!(!ch.can_talk_to(0, channel));
    assert!(!ch.can_talk_to(0, G_SYSTEM));
    assert!(ch.can_talk_to(0, G_VICINITY));
    ch.add_group(channel, "Clan OOC", 0);
    assert!(ch.can_talk_to(0, channel), "a later group announcement replaces the flags");
    let name = ch.wins[0].cfg.window_name.clone();
    ch.subscribe_group(&name, channel, false);
    assert!(!ch.can_talk_to(0, channel));
    ch.subscribe_group(&name, channel, true);
    assert!(ch.can_talk_to(0, channel));
    ch.update(&mut gui, 0.0);
    let _ = std::fs::remove_dir_all(test_dir());
}

#[test]
fn subscription_menu_folders_follow_native_identity_boundaries() {
    for (group, folder) in [
        (G_SYSTEM, 0),
        (G_VICINITY, 0),
        (0x4200_0000, 2),
        (0x4200_0019, 2),
        (0x4200_001a, 0),
        (0x4200_001b, 0),
        (0xe_0000_0001, 1),
        (0x3_4200_0001, 0),
    ] {
        assert_eq!(super::super::config::group_folder(group), folder, "{group:#x}");
    }
}

#[test]
fn autosubscribe_keeps_offline_entries_and_send_tell_prefills_instead_of_opening_chat() {
    let Some((mut gui, mut ch)) = rig((1280, 800)) else { return };
    let offline = 0x3_ffff_ffff;
    ch.wins[0].cfg.set.push(offline);
    ch.test_pick(&mut gui, 0, OP_AUTOSUB, 0);
    assert!(ch.wins[0].cfg.set.contains(&offline));
    ch.test_pick(&mut gui, 0, OP_AUTOSUB, 0);
    assert!(ch.wins[0].cfg.set.contains(&offline));
    let window = ch.frames[0].id;
    ch.event(&mut gui, &Event::ContextMenu { window, x: 10, y: 10, link: Some("user://Bob".into()) });
    assert_eq!(ch.event(&mut gui, &Event::MenuPicked { id: OP_OPEN_CHAT << 16 }), vec![WinOut::OpenTell("Bob".into())]);
    ch.event(&mut gui, &Event::ContextMenu { window, x: 10, y: 10, link: Some("user://Bob".into()) });
    assert!(ch.event(&mut gui, &Event::MenuPicked { id: OP_SEND_TELL << 16 }).is_empty());
    assert_eq!(gui.text(window, &ch.wins[0].input()), "/tell Bob ");
    assert_eq!(gui.focused_view().as_deref(), Some(ch.wins[0].input().as_str()));
    gui.close_menu();
    let _ = std::fs::remove_dir_all(test_dir());
}
