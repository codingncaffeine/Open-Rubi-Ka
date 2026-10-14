//! `TimerBarBase_c` / `GameTimerBar_c` (GUI.dll ctor `FUN_100512ae` 0x100512ae): the progress bar `TimerSystemModule_t::CreateTimer`
//! opens for a timed action (the `/camp` "Logout" countdown, `hud_use.rs`; the own item-action delay, `hud_stats.rs`). A `PowerBar`
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
//! untinted `0xffffff`. Only the Item bar is ported (this client has no hotbar attack/special/nano/reload cooldown bars yet).

use ao_gui::{GfxId, Gui, WindowId, WindowSize};

/// `RenderWindow_t::SetColor` tint of the item-action bar (`StartTimerBar` type 4).
pub(super) const COLOR_ITEM: u32 = 0xaaaaff;
/// text.mdb 10000 "Timer_Item".
pub(super) const LABEL_ITEM: &str = "Item";
/// `_DAT_101a9f9c`: durations shorter than this never open a bar.
pub(super) const MIN_SECONDS: f32 = 0.5;
/// `RenderWindow_t`'s fixed screen anchor for free-position slot 0 (`IPoint(0x28, (0 + 2) * 0x14)`, ctor `FUN_100512ae`
/// 0x100512ae): `TimerSystemModule_t::FindNextFreePos` stacks concurrent bars every 20 px starting at y = 40; this port only
/// ever drives one bar at a time, so it always gets slot 0.
pub(super) const POS_SLOT0: (i32, i32) = (40, 40);

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
