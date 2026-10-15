//! `TimerBarBase_c` / `GameTimerBar_c` (GUI.dll ctor `FUN_100512ae` 0x100512ae): the progress bar `TimerSystemModule_t::CreateTimer`
//! opens for a timed action (the `/camp` "Logout" countdown, `hud_use.rs`; the own item-action delay, `hud_stats.rs`;
//! the per-hand Attack round, the Reload bar, the own Nano-cast bar and the Special-attack bar, `combat/glue.rs` +
//! `hud_use.rs` + `dynels.rs`, docs/zone/combat-timers.md). A `PowerBar`
//! (`GFX_GUI_TIMERBAR_EMPTY`/`_FULL`) tinted by the timer's kind (`RenderWindow_t::SetColor`), with a `TOOLTIP`-font label solid
//! black (`TextLine_t::SetDefaultColor(this, 0)` 0x100223ef: the colour field is a plain RGB int, not a palette index -- 0 is
//! black, not white).
//!
//! Position/alignment (raw disassembly of `FUN_100512ae`, not just its decompile, which drops two of the six `TextLine_t::TextLine_t`
//! 0x100223ff arguments): the ctor passes `IPoint(0, 1)` and `TextOutputFlags_e` **0x800**, then immediately overwrites the label's
//! own `TextOutput_t` rect to the *whole bar* (`IRect(0, 0, barWidth, barHeight)`, same offsets `SetDefaultColor` touches). Given
//! that rect, `TextOutput_t::Alignment` 0x100231d8 tests the flags: `0x400` = left (`x = rect.left`), **`0x800` = centre**
//! (`x = (rect.width - textWidth) / 2 + rect.left`), `0x1000` = right. `AdjustHotspot` 0x10023247 (vertical/hotspot shift) is a
//! no-op for 0x800, so `y` stays the literal `1` -- centred horizontally, flush to the top (not vertically centred).
//!
//! `TimerSystemModule_t::StartTimerBar` 0x10051a27 (text.mdb category 10000) colours the bar by `TimerEntry_t` kind: Attack
//! `0xaaffaa` "Timer_Attack", Special `0xaaffff` "Timer_Special", Nano `0xffaaaa` "Timer_Nano", **Item `0xaaaaff` "Timer_Item"**
//! (= "Item", not an "Equipping <name>" label; shown only when the duration is at least [`MIN_SECONDS`], `_DAT_101a9f9c`), Reload
//! untinted `0xffffff`. The hotbar icon-fader recharge overlay (`hud_bar.rs`/`hud_special.rs`) is a separate,
//! additional indicator, not a replacement for any of these bars.

use ao_gui::{GfxId, Gui, WindowId, WindowSize};

/// `RenderWindow_t::SetColor` tint of the item-action bar (`StartTimerBar` type 4).
pub(super) const COLOR_ITEM: u32 = 0xaaaaff;
/// text.mdb 10000 "Timer_Item".
pub(super) const LABEL_ITEM: &str = "Item";
/// `StartTimerBar` type 1 tint; text.mdb 10000 "Timer_Attack" = "Attack". One bar per wielded hand while fighting,
/// filling over the initiative-scaled AttackDelay+RechargeDelay round time (`combat::arms::item_round_seconds`,
/// `combat/glue.rs`); corroborated by the kind-2 "Special" bar's own Gamecode evidence (below).
pub(super) const COLOR_ATTACK: u32 = 0xaaffaa;
pub(super) const LABEL_ATTACK: &str = "Attack";
/// `StartTimerBar` type 2 tint; text.mdb 10000 "Timer_Special" = "Special". GC `FUN_1003b947` (gated by
/// `FUN_1003bac9`'s wield-state check), reads `AttackDelay`(0x126) + `RechargeDelay`(0xd2) of the special's own list
/// item (Brawl/Dimach), or, when the `DummyItemBase_t` RTTI cast fails, the wielded weapon (Burst/Fling/Full Auto --
/// live-confirmed, `docs/captures/zone_special_burst_fixr_ithaca.rec`: Aomacfixr's rifle Burst(148) is absent from
/// its own `SpecialAttackWeaponIIR_t` list). `Dynels::start_special_bar`, `combat::arms::special_or_wielded_item`.
pub(super) const COLOR_SPECIAL: u32 = 0xaaffff;
pub(super) const LABEL_SPECIAL: &str = "Special";
/// `StartTimerBar` type 5: untinted (`uVar4 = 0xffffff`, no `LDBface::GetTextPtr` call precedes the fallthrough, but
/// text.mdb 10000 "Timer_Reload" = "Reload" matches the kind's own name). Duration: the ranged hand's raw
/// `RechargeDelay` stat (0xd2/210) alone, no initiative scaling -- traced to `ReloadIIR_t` 0x26515E61's apply
/// (`Dynels::start_reload_bar`, docs/zone/combat-timers.md §3), not a guess.
pub(super) const COLOR_RELOAD: u32 = 0xffffff;
pub(super) const LABEL_RELOAD: &str = "Reload";
/// `StartTimerBar` type 3 tint; text.mdb 10000 "Timer_Nano" = "Nano program". The own character's active cast,
/// duration = `nano_cast_delay` (already ported), triggered by `CastNanoSpellIIR_t` apply (`Dynels::nano_bar_progress`,
/// docs/zone/combat-timers.md §4).
pub(super) const COLOR_NANO: u32 = 0xffaaaa;
pub(super) const LABEL_NANO: &str = "Nano program";
/// `_DAT_101a9f9c`: durations shorter than this never open a bar.
pub(super) const MIN_SECONDS: f32 = 0.5;
/// `RenderWindow_t`'s fixed screen anchor for free-position slot `n` (`IPoint(0x28, (n + 2) * 0x14)`, ctor `FUN_100512ae`
/// 0x100512ae): `TimerSystemModule_t::FindNextFreePos` stacks concurrent bars every 20 px starting at y = 40, taking the
/// lowest slot index not already in use by an open bar (`hud_stats::free_timer_slot`).
pub(super) fn pos_slot(n: i32) -> (i32, i32) {
    (0x28, (n + 2) * 0x14)
}

fn xml(bw: u32, bh: u32, label: &str, color: u32) -> String {
    format!(
        "<root><View view_layout=\"stacked\" min_size=\"Point({bw},{bh})\" max_size=\"Point({bw},{bh})\">\
         <PowerBar name=\"bar\" bg_gfx=\"GFX_GUI_TIMERBAR_EMPTY\" full_gfx=\"GFX_GUI_TIMERBAR_FULL\" direction=\"right\" color=\"{color:#x}\"/>\
         <View view_layout=\"horizontal\" h_alignment=\"center\" v_alignment=\"top\">\
         <TextView name=\"label\" value=\"{label}\" font=\"TOOLTIP\" color=\"0x000000\" layout_borders=\"Rect(0,1,0,0)\"/>\
         </View></View></root>"
    )
}

/// Size of the bar art (`IPoint(0x80, 0x10)` = 128 x 16 in the ctor, matching `GFX_GUI_TIMERBAR_EMPTY`'s own size).
pub(super) fn bar_size(gui: &Gui) -> (u32, u32) {
    gui.gfx_id("GFX_GUI_TIMERBAR_EMPTY").map(GfxId).map_or((128, 16), |g| gui.gfx().size(g))
}

/// Opens a borderless timer bar window at `pos` tinted `color` with `label`; `name` only labels warnings.
pub(super) fn open(gui: &mut Gui, name: &str, label: &str, color: u32, pos: (i32, i32)) -> anyhow::Result<WindowId> {
    let (bw, bh) = bar_size(gui);
    let w = gui.open_window_xml(name, &xml(bw, bh, label, color), pos, WindowSize::Fixed(bw, bh))?;
    Ok(w)
}

pub(super) fn set_progress(gui: &mut Gui, w: WindowId, value: f32) {
    gui.set_progress(w, "bar", value);
}
