use super::*;
use ao_gui::{MenuItem, Modifiers, MouseButton};

#[test]
fn popup_dismissal_releases_chat_and_configuration_ownership() {
    let Some((mut gui, mut ch)) = rig((1280, 800)) else { return };
    let mut d = crate::play::dvalue::DValues::new(&ao_gui::client_dir());
    d.set_i64("chat_group_window", 1);
    ch.sync_config(&mut gui, &mut d);
    let config = ch.config.window.unwrap();
    let group = 0xe_0000_0005;
    ch.add_group(group, "Private", 0);
    ch.config_own_id(5);
    for owner in 0..3 {
        for dismissal in 0..4 {
            let event = match owner {
                0 => Event::ContextMenu { window: ch.frames[0].id, x: 20, y: 20, link: None },
                _ => Event::ListItemMouse {
                    window: config,
                    view: if owner == 1 { "windows" } else { "groups" }.into(),
                    id: if owner == 1 { ch.wins[0].cfg.window_name.clone() } else { group_ident(group) },
                    button: 2, clicks: 1, x: 20, y: 20,
                },
            };
            ch.event(&mut gui, &event);
            let generation = gui.menu_generation().expect("chat popup opens");
            assert!(ch.owns(&Event::MenuPicked { id: 0xdead_beef }));
            let events = match dismissal {
                0 => gui.input(InputEvent::MouseDown { x: 1200.0, y: 750.0, button: MouseButton::Left }),
                1 => gui.input(InputEvent::Key { key: Key::Escape, pressed: true, mods: Modifiers::default() }),
                2 => {
                    gui.close_menu();
                    gui.input(InputEvent::MouseMove { x: 1200.0, y: 750.0 })
                }
                _ => {
                    gui.open_menu((20, 20), (1280, 800), vec![MenuItem::entry(0xdead_beef, "Shop action")]);
                    gui.input(InputEvent::MouseMove { x: 1200.0, y: 750.0 })
                }
            };
            assert_eq!(events, vec![Event::MenuClosed { generation }]);
            for event in events { ch.event(&mut gui, &event); }
            assert!(!ch.owns(&Event::MenuPicked { id: 0xdead_beef }), "owner {owner}, dismissal {dismissal}");
            if dismissal != 3 {
                gui.open_menu((20, 20), (1280, 800), vec![MenuItem::entry(0xdead_beef, "Shop action")]);
            }
            let events = gui.input(InputEvent::MouseDown { x: 25.0, y: 23.0, button: MouseButton::Left });
            assert!(matches!(events.first(), Some(Event::MenuPicked { id: 0xdead_beef })));
            for event in events {
                assert!(!ch.owns(&event), "subsequent unrelated popup must reach its owner");
                ch.event(&mut gui, &event);
            }
        }
    }
    ch.close_all(&mut gui);
    let _ = std::fs::remove_dir_all(test_dir());
}

#[test]
fn replacement_closure_keeps_new_chat_context_and_pick_precedes_dismissal() {
    let Some((mut gui, mut ch)) = rig((1280, 800)) else { return };
    let window = ch.frames[0].id;
    for name in ["Old", "New"] {
        ch.event(&mut gui, &Event::ContextMenu { window, x: 20, y: 20, link: Some(format!("user://{name}")) });
    }
    let generation = gui.menu_generation().unwrap();
    let row = gui.font_height(ao_gui::FontId::Normal) as f32 + 2.0;
    let events = gui.input(InputEvent::MouseDown { x: 25.0, y: 22.0 + row * 1.5, button: MouseButton::Left });
    assert!(matches!(events.as_slice(), [Event::MenuClosed { generation: old }, Event::MenuPicked { .. }, Event::MenuClosed { generation: new }] if old != new && *new == generation));
    let mut outputs = vec![];
    for event in events { outputs.extend(ch.event(&mut gui, &event)); }
    assert_eq!(outputs, vec![WinOut::OpenTell("New".into())]);
    assert!(!ch.owns(&Event::MenuPicked { id: 0xdead_beef }));
    ch.close_all(&mut gui);
    let _ = std::fs::remove_dir_all(test_dir());
}
