//! Consumers of the options that have no window of their own: `MiscOptionsMonitor_c` (GUI.dll ctor `FUN_100bff11`, created by `OptionPanelModule_c::Initialize` 0xc2a58 and
//! living for the whole session) and `SoundOptionsMonitor_c` (0x100c4cc9). docs/gui.md "Options window".
//!
//! * `ToggleAllEffects` (observer `FUN_100bfa27`): every change sets the 13 effect prefs (`RealisticWater`, `Wildlife`, `IsSpaceShipsShown` (char prefs), `RealisticClouds`,
//!   `SimpleClouds`, `Shadows`, `BuffsFX`, `TracersFX`, `NanoEffectFX`, `MuzzleFlashFX`, `EnvironmentFX`, `OthersFX`, `SmoothAnimations`) to its bool (`SetPrefInt(name, b, type, true)`).
//! * `AutoTargetMOB` / `AutoTargetPvP` / `DisableXPGain` are bits 1 / 3 / 4 of the own stat `0x15d` (read once at construction); a change runs
//!   `N3Msg_EventFeedback(0x3c, {0,0}, {0, stat with the bit set / cleared})` (`FUN_100bfb7e` / `100bfc3a` / `100bfbdc`), Gamecode 0x1001cbb4: a new value sends a
//!   `CharacterActionIIR_t` of action `0xa5` (identity b `{0, value}`) and stores it in the stat.
//! * `VisualFlags` = the whole stat `0x2a1`, `VisiblePVPTitle` = `stat >> 7 & 3`; a change (`FUN_100bfd64` / `FUN_100bfe01`, the title with `stat & !0x180 | (v & 3) << 7`) runs
//!   `N3Msg_EventFeedback(0x46, ..)`: the same message with action `0xa6` (the one `/rp` sends). Gamecode 0x1001cbb4 refuses a bit-5 change while the fight controller is non-idle or stat 0x19a is nonzero;
//!   GUI 0x100bfd64 then restores the DValue to the stat.
//! * `OpenNanoWindow`: while true the slot `FUN_100bfc98` is connected to `GlobalSignals+0x10c` and sets the DValue `nano_window` (opens the Programs window) -- the emitter of that
//!   signal is UNRESOLVED, see docs.

use super::super::dvalue::{DValues, Kind, Variant, CAT_VARIABLES};
use super::super::zone::Zone;
use ao_net::frame::Frame;
use ao_net::msg::Identity;
use ao_net::n3::action::{character_action, simple};
use ao_net::n3::outgoing::n3_frame;

/// `FUN_100bfa27`: `(name, char prefs)`.
pub const EFFECT_PREFS: [(&str, bool); 13] = [
    ("RealisticWater", false),
    ("Wildlife", false),
    ("IsSpaceShipsShown", true),
    ("RealisticClouds", false),
    ("SimpleClouds", false),
    ("Shadows", false),
    ("BuffsFX", false),
    ("TracersFX", false),
    ("NanoEffectFX", false),
    ("MuzzleFlashFX", false),
    ("EnvironmentFX", false),
    ("OthersFX", false),
    ("SmoothAnimations", false),
];

/// Own stat of the server-side option flags (`N3Msg_GetSkill(0x15d, 2)`).
pub const STAT_OPTION_FLAGS: u32 = 0x15d;
/// Own stat `VisualFlags`.
pub const STAT_VISUAL_FLAGS: u32 = 0x2a1;
/// `(DValue, bit of stat 0x15d)`.
const FLAG_BITS: [(&str, u32); 3] = [("AutoTargetMOB", 1), ("AutoTargetPvP", 3), ("DisableXPGain", 4)];
/// Bits 7 and 8 of `VisualFlags`: `VisiblePVPTitle`.
const TITLE_MASK: i32 = 0x180;

#[derive(Default)]
pub(in crate::play) struct Live {
    effects: Option<bool>,
    flags: Option<i32>,
    visual: Option<i32>,
    visual_request: Option<i32>,
}

fn set_bool(d: &mut DValues, name: &str, v: bool) {
    if d.exists(name) {
        d.set_i64(name, i64::from(v));
    } else {
        d.add(name, Variant::Bool(v), false, CAT_VARIABLES, None, None, false);
    }
}

impl Live {
    /// Per frame: run changed observers, append outgoing frames, and return whether the native social-visual refusal needs feedback.
    pub fn apply(&mut self, d: &mut DValues, zone: &mut Zone, out: &mut Vec<Frame>) -> bool {
        // ToggleAllEffects
        let all = d.flag("ToggleAllEffects");
        if self.effects.is_some_and(|e| e != all) {
            for (n, chr) in EFFECT_PREFS {
                d.prefs.set_int(n, i32::from(all), if chr { Kind::Char } else { Kind::Login });
            }
        }
        self.effects = Some(all);

        let id = zone.char_id as i32;
        // stat 0x15d <-> AutoTargetMOB / AutoTargetPvP / DisableXPGain
        if let Some(stat) = zone.stat(STAT_OPTION_FLAGS) {
            if self.flags != Some(stat) {
                // the server (or the first sight) set the stat: the check boxes follow it (`SetDValue` at construction)
                for (n, bit) in FLAG_BITS {
                    set_bool(d, n, stat >> bit & 1 != 0);
                }
                self.flags = Some(stat);
            } else {
                let mut want = stat;
                for (n, bit) in FLAG_BITS {
                    want = if d.flag(n) { want | 1 << bit } else { want & !(1 << bit) };
                }
                if want != stat {
                    out.push(event_feedback(id, 0xa5, want));
                    zone.stats.insert(STAT_OPTION_FLAGS, want);
                    self.flags = Some(want);
                }
            }
        }
        // stat 0x2a1 <-> VisualFlags / VisiblePVPTitle
        if let Some(stat) = zone.stat(STAT_VISUAL_FLAGS) {
            if self.visual != Some(stat) {
                d.set_i64("VisualFlags", i64::from(stat));
                d.set_i64("VisiblePVPTitle", i64::from(stat >> 7 & 3));
                self.visual = Some(stat);
                self.visual_request = None;
            } else {
                let vf = d.get_i64("VisualFlags").unwrap_or(i64::from(stat)) as i32;
                let title = d.get_i64("VisiblePVPTitle").unwrap_or(i64::from(stat >> 7 & 3)) as i32;
                let want = if vf != self.visual_request.unwrap_or(stat) { vf } else { vf & !TITLE_MASK | (title & 3) << 7 };
                if want != stat && self.visual_request != Some(want) {
                    if (want ^ stat) & 0x20 != 0 && (zone.fight_target.contains_key(&id) || zone.stat(0x19a).unwrap_or(0) != 0) {
                        d.set_i64("VisualFlags", i64::from(stat));
                        self.visual_request = None;
                        return true;
                    }
                    out.push(event_feedback(id, 0xa6, want));
                    d.set_i64("VisualFlags", i64::from(want));
                    d.set_i64("VisiblePVPTitle", i64::from(want >> 7 & 3));
                    self.visual_request = Some(want);
                }
                if want == stat {
                    self.visual_request = None;
                }
            }
        }
        false
    }
}

/// `N3Msg_EventFeedback` of a changed value: `CharacterActionIIR_t(action, a = {0,0}, b = {0, value})` (Gamecode 0x1001cbb4, `FUN_1007253f`).
pub fn event_feedback(char_id: i32, action: i32, value: i32) -> Frame {
    n3_frame(0, char_id as u32, character_action(char_id, &simple(action, Identity::default(), Identity { kind: 0, instance: value })))
}

/// `SoundOptionsMonitor_c` (`SlotPrefMasterVolumeChanged` 0x100c47cf, `SlotPrefSoundFXOnChanged` ..., `SlotPrefBattlemusicModeChanged`): the sound prefs of the client's audio engine.
pub fn audio_prefs(d: &DValues) -> ao_audio::Prefs {
    let f = |n: &str, def: f32| match d.get(n) {
        Some(Variant::Float(v)) => *v,
        Some(Variant::Int(v)) => *v as f32,
        _ => def,
    };
    ao_audio::Prefs {
        sound_on: d.get("SoundOnOff").is_none_or(|_| d.flag("SoundOnOff")),
        master: f("MasterVolume", 1.0),
        fx_on: d.get("SoundFXOn").is_none_or(|_| d.flag("SoundFXOn")),
        fx: f("FXVolume", 1.0),
        music_on: d.get("MusicOn").is_none_or(|_| d.flag("MusicOn")),
        music: f("MusicVolume", 1.0),
        voice_on: d.get("VoiceSndFxOn").is_none_or(|_| d.flag("VoiceSndFxOn")),
        voice: f("VoiceSndFxVolume", 1.0),
        battlemusic_mode: d.get_i64("BattlemusicMode").unwrap_or(3) as i32,
    }
}

/// Sound preferences needed before the HUD exists (character selection/creation and
/// CharacterLoggedInMessage). These are the same Main/Login archives loaded by open_user.
pub fn login_audio_prefs(client: &std::path::Path, prefs: Option<&std::path::Path>, account: Option<&str>) -> ao_audio::Prefs {
    use super::super::dvalue::{CAT_LOGIN, CAT_MAIN};
    let mut d = DValues::new(client);
    if let Some(root) = prefs {
        if let Ok(text) = std::fs::read_to_string(root.join("Prefs.xml")) {
            d.load_config(&text, CAT_MAIN, false);
        }
        if let Some(account) = account {
            if let Ok(text) = std::fs::read_to_string(root.join(account).join("Prefs.xml")) {
                d.load_config(&text, CAT_LOGIN, false);
            }
        }
    }
    audio_prefs(&d)
}


#[cfg(test)]
mod tests {
    use super::*;
    use ao_net::n3::action::parse_character_action;

    #[test]
    fn login_sound_preferences_load_before_a_character_hud_exists() {
        let client = ao_gui::client_dir();
        if !client.join("cd_image/gui/Default/LoginPrefs.xml").exists() { return; }
        let root = std::env::temp_dir().join(format!("aomac-login-audio-prefs-{}", std::process::id()));
        std::fs::create_dir_all(root.join("account")).unwrap();
        std::fs::write(root.join("Prefs.xml"), r#"<Root><Value name="SoundOnOff" value="false"/></Root>"#).unwrap();
        std::fs::write(root.join("account/Prefs.xml"), r#"<Root><Value name="MasterVolume" value="0.25"/><Value name="MusicOn" value="false"/><Value name="VoiceSndFxVolume" value="0.5"/></Root>"#).unwrap();
        let p = login_audio_prefs(&client, Some(&root), Some("account"));
        assert_eq!((p.sound_on, p.master, p.music_on, p.voice), (false, 0.25, false, 0.5));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn every_sound_option_restores_from_its_retail_archive() {
        use super::super::super::dvalue::{CAT_CHAR, CAT_LOGIN, CAT_MAIN};
        let client = ao_gui::client_dir();
        if !client.join("cd_image/gui/Default/LoginPrefs.xml").exists() { return; }
        let mut d = DValues::new(&client);
        for key in ["SoundOnOff", "SoundFXOn", "MusicOn", "VoiceSndFxOn",
            "VoiceSndFxHearVicinityOn", "VoiceSndFxHearTeamOn",
            "VoiceSndFxHearGuildOn", "VoiceSndFxHearFriendsOn"] {
            d.set_i64(key, i64::from(!d.flag(key)));
        }
        for key in ["MasterVolume", "FXVolume", "MusicVolume", "VoiceSndFxVolume"] {
            d.set(key, Variant::Float(0.25));
        }
        d.set_i64("BattlemusicMode", 1);
        d.set_i64("VoiceSndFxType", 3);
        let mut restored = DValues::new(&client);
        for cat in [CAT_MAIN, CAT_LOGIN, CAT_CHAR] {
            assert!(restored.load_config(&d.save_config(cat), cat, false));
        }
        for key in ["SoundOnOff", "MasterVolume", "SoundFXOn", "FXVolume", "MusicOn",
            "MusicVolume", "BattlemusicMode", "VoiceSndFxOn", "VoiceSndFxVolume",
            "VoiceSndFxHearVicinityOn", "VoiceSndFxHearTeamOn", "VoiceSndFxHearGuildOn",
            "VoiceSndFxHearFriendsOn", "VoiceSndFxType"] {
            assert_eq!(restored.get(key), d.get(key), "{key}");
        }
        let p = audio_prefs(&restored);
        assert_eq!((p.master, p.fx, p.music, p.voice, p.battlemusic_mode), (0.25, 0.25, 0.25, 0.25, 1));
    }
    fn store() -> DValues {
        let mut d = DValues::default();
        d.load_config(
            r#"<Root><Value name="ToggleAllEffects" value="true"/><Value name="VisualFlags" value="31"/><Value name="VisiblePVPTitle" value="0"/><Value name="OpenNanoWindow" value="false"/><Value name="VoiceSndFxVolume" value="0.5" min="0.0" max="1.0"/></Root>"#,
            super::super::super::dvalue::CAT_CHAR,
            true,
        );
        d.prefs = super::super::super::dvalue::IndepPrefs::with_defaults();
        d
    }

    fn zone() -> Zone {
        let mut z = Zone { char_id: 77, ..Default::default() };
        z.stats.insert(STAT_OPTION_FLAGS, 0b01010);
        z.stats.insert(STAT_VISUAL_FLAGS, 31 | 1 << 7);
        z
    }

    #[test]
    fn social_visual_flag_refuses_fighting_and_attack_stat_only() {
        let mut d = store();
        let mut z = zone();
        let mut live = Live::default();
        let mut out = Vec::new();
        assert!(!live.apply(&mut d, &mut z, &mut out));
        let original = z.stat(STAT_VISUAL_FLAGS).unwrap();
        z.fight_target.insert(77, 88);
        d.set_i64("VisualFlags", i64::from(original ^ 0x20));
        assert!(live.apply(&mut d, &mut z, &mut out));
        assert_eq!(d.get_i64("VisualFlags"), Some(i64::from(original)));
        assert!(out.is_empty());
        // Other visual bits remain editable in combat.
        d.set_i64("VisualFlags", i64::from(original ^ 4));
        assert!(!live.apply(&mut d, &mut z, &mut out));
        assert_eq!(out.len(), 1);
        z.fight_target.clear();
        z.stats.insert(0x19a, 1);
        let original = z.stat(STAT_VISUAL_FLAGS).unwrap();
        d.set_i64("VisualFlags", i64::from(original ^ 0x20));
        assert!(live.apply(&mut d, &mut z, &mut out));
        assert_eq!(out.len(), 1);
        z.stats.insert(0x19a, 0);
        d.set_i64("VisualFlags", i64::from(original ^ 0x20));
        assert!(!live.apply(&mut d, &mut z, &mut out));
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn server_stats_fill_the_options_and_changes_send_event_feedback() {
        let (mut d, mut z, mut live, mut out) = (store(), zone(), Live::default(), vec![]);
        live.apply(&mut d, &mut z, &mut out);
        // 0x15d = 0b01010: bit 1 (auto target monsters) and bit 3 (players) set, bit 4 (disable XP) clear
        assert!(d.flag("AutoTargetMOB") && d.flag("AutoTargetPvP") && !d.flag("DisableXPGain"));
        assert_eq!((d.get_i64("VisualFlags"), d.get_i64("VisiblePVPTitle")), (Some(31 | 128), Some(1)));
        assert!(out.is_empty(), "reading the stats sends nothing");
        // the user ticks "disable XP gain" and unticks auto target monsters
        d.set_i64("DisableXPGain", 1);
        d.set_i64("AutoTargetMOB", 0);
        live.apply(&mut d, &mut z, &mut out);
        assert_eq!(z.stat(STAT_OPTION_FLAGS), Some(0b11000));
        assert_eq!(out.len(), 1);
        let (_, a) = parse_character_action(&out[0].payload).unwrap();
        assert_eq!((a.action, a.identity_a, a.identity_b), (0xa5, Identity::default(), Identity { kind: 0, instance: 0b11000 }));
        // no further message while nothing changes
        live.apply(&mut d, &mut z, &mut out);
        assert_eq!(out.len(), 1);
        // a visual flag bit goes out with action 0xa6 and the whole stat value
        d.set_i64("VisualFlags", i64::from(31 | 128) & !4);
        live.apply(&mut d, &mut z, &mut out);
        let (_, a) = parse_character_action(&out[1].payload).unwrap();
        assert_eq!((a.action, a.identity_b.instance), (0xa6, (31 | 128) & !4));
        assert_eq!(z.stat(STAT_VISUAL_FLAGS), Some(31 | 128), "visual stats wait for the server echo");
        live.apply(&mut d, &mut z, &mut out);
        assert_eq!(out.len(), 2, "unchanged pending visual request is not resent");
        // the PvP title radio replaces bits 7..8 only
        d.set_i64("VisiblePVPTitle", 3);
        live.apply(&mut d, &mut z, &mut out);
        let (_, a) = parse_character_action(&out[2].payload).unwrap();
        assert_eq!(a.identity_b.instance, 31 & !4 | 3 << 7);
        assert_eq!(d.get_i64("VisualFlags"), Some(i64::from(31 & !4 | 3 << 7)), "the VisualFlags DValue holds the requested value");
        // a server update wins over the DValues
        z.stats.insert(STAT_VISUAL_FLAGS, 31 & !4 | 3 << 7);
        z.stats.insert(STAT_OPTION_FLAGS, 0);
        live.apply(&mut d, &mut z, &mut out);
        assert!(!d.flag("DisableXPGain") && !d.flag("AutoTargetPvP"));
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn toggle_all_effects_sets_every_effect_pref() {
        let (mut d, mut z, mut live, mut out) = (store(), Zone::default(), Live::default(), vec![]);
        live.apply(&mut d, &mut z, &mut out);
        assert_eq!(d.prefs.get_int("Wildlife", Kind::Login), Some(1), "the first sight changes nothing");
        d.set_i64("ToggleAllEffects", 0);
        live.apply(&mut d, &mut z, &mut out);
        for (n, chr) in EFFECT_PREFS {
            assert_eq!(d.prefs.get_int(n, if chr { Kind::Char } else { Kind::Login }), Some(0), "{n}");
        }
        d.set_i64("ToggleAllEffects", 1);
        live.apply(&mut d, &mut z, &mut out);
        assert_eq!(d.prefs.get_int("IsSpaceShipsShown", Kind::Char), Some(1));
        assert_eq!(d.prefs.get_int("SmoothAnimations", Kind::Login), Some(1));
    }

    #[test]
    fn sound_prefs_follow_the_dvalues() {
        let mut d = store();
        d.load_config(r#"<Root><Value name="SoundOnOff" value="true"/><Value name="MasterVolume" value="0.5"/><Value name="MusicOn" value="false"/><Value name="BattlemusicMode" value="1"/></Root>"#, super::super::super::dvalue::CAT_LOGIN, true);
        let p = audio_prefs(&d);
        assert_eq!((p.sound_on, p.master, p.music_on, p.fx_on, p.battlemusic_mode), (true, 0.5, false, true, 1));
        assert_eq!((p.voice_on, p.voice), (true, 0.5));
        d.set_i64("SoundOnOff", 0);
        assert!(!audio_prefs(&d).sound_on, "the master switch mutes the voices too");
    }
}
