# Attack / Reload / Nano HUD timer bars

Code: `play/combat/arms.rs` (`round_delay`, `item_round_seconds`, `attack_slots`), `play/dynels.rs` (`update_attack_bars`,
`restart_attack_bar`, `attack_bar_progress`, `start_reload_bar`, `reload_bar_progress`, `nano_bar_progress`,
`nano_cast_delay`), `play/combat/glue.rs` (per-frame wiring), `play/hud_use.rs` (Reload trigger), `play/hud_stats.rs`
(`update_attack_timers`, `update_reload_timer`, `update_nano_timer`, `free_timer_slot`), `play/hud_timerbar.rs`
(`COLOR_*`/`LABEL_*` per kind, `pos_slot`).

RE tooling: three new scripts were added to `~/aomac-ghidra/scripts` for this investigation (reusable): `FindScalar.java`
(scan every instruction for a literal operand value), `DumpAsm.java` (raw disassembly of an address range, needed
whenever the decompiler drops register-passed arguments), `ListSymRange.java` (every symbol in an address range, used
to resolve an unnamed vtable slot to its owning class by position).

## 1. The widget and the `TimerEntry_t` construction chain (hard evidence)

`hud_timerbar.rs` already carries the faithful RE of the generic timer-bar widget (`TimerBarBase_c`/`GameTimerBar_c`,
GUI.dll) from the "Logout" countdown and the Item wear/unwear delay bar. `TimerSystemModule_t::StartTimerBar`
[GUI 0x10051a27] colours/labels the bar by `TimerEntry_t+0x14` kind: 1 Attack (`0xaaffaa` "Attack"), 2 Special
(`0xaaffff` "Special"), 3 Nano (`0xffaaaa` "Nano program"), 4 Item (`0xaaaaff` "Item"), 5 Reload (untinted `0xffffff`
"Reload") -- English strings read directly from the installed `cd_image/text/text.mdb` (`TextDb::by_key(10000, key)`).
Position: `TimerSystemModule_t::FindNextFreePos` [GUI 0x10051779] stacks concurrent bars 20 px apart from
`IPoint(0x28, (n+2)*0x14)`, taking the lowest free slot (`hud_timerbar::pos_slot`, `HudStats::free_timer_slot`).

**Every `TimerEntry_t` in Gamecode.dll is unambiguously identifiable**, independent of which kind it ends up as: its
shared_ptr control block (`FUN_1003d9ec`) is built with `boost::detail::sp_counted_impl_p<TimerEntry_t>::vftable`
(RTTI-typed, not a guess -- `~/aomac-ghidra/re.sh Gamecode DumpRefs.java "TimerEntry_t"` finds this exact mangled
name). Its only wrapper, `FUN_1003da5d` (assign + wrap + a second helper), is called by exactly one place,
`FUN_1003d7ce` -- the actual "construct `TimerEntry_t`, emit `GlobalSignals+0x1f8`" (`StartTimerBar`'s own signal,
confirmed via `ShutdownMessage`'s `SignalBase_c::_Disconnect((SignalBase_c*)(GlobalSignals+0x1f8), ...)`) helper.
`DumpCallers 1003d7ce` finds **exactly six** call sites, each independently traced below (§2-§5); there is no other
`TimerEntry_t` constructor and no other emitter in either DLL -- `FUN_1009b415`/`FUN_1009b37e`, which looked like a
second "kind-parameterised" constructor from their allocation-size pattern, turned out on closer reading to be plain
`WeaponItem_t`/`DummyWeapon_t` member construction (`FUN_1009c0ca` sets `*extraout_ECX = WeaponItem_t::vftable`), an
unrelated coincidence of object size, not a second TimerBar path.

## 2. Attack (kind 1): unchanged, now corroborated

Duration = `combat::arms::item_round_seconds` (initiative-scaled `AttackDelay`(294) + `RechargeDelay`(210), the same
formula the item-info DPS tooltip already used, `chat::item_info_combat::delay`/`FUN_1006772b`/`FUN_10067849`).
Per-hand (`Armory::attack_slots`, GC 0x10068969), restarts on each swing (`CombatEvent::Hit`/`Miss`'s `slot`).
No direct Gamecode caller of kind 1 specifically was traced among the six `FUN_1003d7ce` sites (none of the six
match an ordinary-attack context); this remains the pre-existing, evidence-grounded reconstruction. It is now
corroborated, not merely analogous: kind 2 "Special" (§4) uses the *exact same two stats, summed, from the exact
same `(**vtable+0x3c)(statId, 2)` effective-stat accessor*, on a sibling (special-attack) weapon-like item, which is
strong indirect confirmation that an ordinary weapon's round bar uses the identical formula.

## 3. Reload (kind 5): traced to its exact caller, not a guess

* **Message class**: `ReloadIIR_t` 0x26515E61 (present in `ao_net::n3::outgoing`'s known-class table, not yet
  decoded into the `Misc`/`World` enums). Its apply is `FUN_10076780` (a bare vtable `Execute` slot, confirmed by
  `DumpCallers` on it resolving to a `<no function>` DATA ref -- a vtable entry, not a direct call) -> `FUN_10069274`.
* `FUN_10069274(ctrl, weaponIdentity, old, newCount, flag)`: reads the weapon's stat `0x1a` (26, the same
  current-ammo stat `chat/item_info.rs::ammo` already reads as "count"), and if it is present (`-1 < count`), **sets**
  stat `0x1a` to `newCount` (the clip refilling), calls `FUN_10068a89(weapon)`, then prints `Feedback_Reloading`.
* `FUN_10068a89` is the one of the six `FUN_1003d7ce` callers that matters here. Its very first instructions
  (`~/aomac-ghidra re.sh Gamecode DumpAsm.java 10068a89 10068ac7`) push the literal pair `(0xd2, 2)` into the item's
  effective-stat accessor (`(**vtable+0x3c)(statId, mode)`, the same accessor used for `(0x1a,2)` ammo and `(0x126,2)`
  AttackDelay elsewhere) before constructing the `TimerEntry_t` -- i.e. **the duration passed to kind 5 is the
  weapon's own `RechargeDelay` stat (210/0xd2), alone, with no `AttackDelay` term and no initiative scaling** (no
  `GameTime_t`/initiative-slope code runs anywhere in this function). `Dynels::start_reload_bar` now uses exactly
  `item.recharge as f32 / 100.0` (the hundredths-to-seconds divisor is `StartTimerBar`'s own kind-4 branch constant
  `_DAT_101adfa8 = 100.0`, read directly from GUI.dll).
* **Trigger, as ported**: this client has no captured `ReloadIIR_t` to pin the wire layout from, so the bar is still
  opened client-side when the player sends the Reload special action (`hud_use.rs` 0x6e, action id `RELOAD` ->
  `CharacterActionIIR_t` 0xd2, `FUN_1006949a`/`FUN_10029e66`, unchanged by this pass) rather than on a decoded
  `ReloadIIR_t`. The *duration value* is no longer a guess; the exact server-side trigger timing (whether retail
  opens the bar on send or waits for `ReloadIIR_t`'s echo) remains unresolved for lack of a capture.

## 4. Nano (kind 3): reuses an already-correct, independently-traced formula

* **Trigger**: `CastNanoSpellIIR_t` 0x25314D6D apply [GC 0x10072306] -> `FUN_10051754(spell, &target, 1, flag, 1, 1,
  &source, 0, 0)` (the exact call already documented in `docs/zone/dynel.md` §5, independently of this
  investigation) -> tail `FUN_10050ed9`, whose address range (`asm 10050f85-10051064`) is *already* the one labelled
  in `dynels.rs` as `nano_cast_delay`'s own GC evidence. Its caller chain is unambiguous: `DumpCallers 10051754`
  finds the nano apply (0x10072306), the `CharacterActionIIR_t` dispatch's own case for `identity_b`-bearing nano
  actions, and `FUN_100a9543`/`FUN_10071be4` (a `BuffIIR_c`-adjacent context, same shared helper reused for buffs).
* **Duration**: `nano_cast_delay(template.stat(0x126), template.stat(0x20b), stat(who, 0x95), stat(who, 0x33),
  template.stat(0))` was *already* ported (`dynels.rs`, predating this change) and already drives the nano casting
  animation's `NanoCast.remaining` countdown -- nano templates reuse the item stat schema, `AttackDelay`(294) as
  "cast time" and `AttackDelayCap`(523, stat `0x20b`) as its floor, scaled by the caster's own stat `0x95`
  initiative and `0x33` AggDef. `NanoCast` now also stores `total` (the value `remaining` started at), so
  `Dynels::nano_bar_progress(own)` can report `(1 - remaining/total, total)` without recomputing the formula.
* **Port**: `hud_stats::update_nano_timer` opens/updates/closes the "Nano program" bar for the own character's
  active, non-instant, unreleased cast. Regression `combat_timer_bars_stack_and_show_mid_fight` casts the real
  installed Body Boost (template 29091, the same fixture `dynels.rs`'s own nano tests already use) through the real
  `CastNanoSpellIIR_t` apply path (`Dynels::on_message` + `nano_visual_frame`) and checks the bar opens with label
  "Nano program" at a free stacking slot alongside the Attack/Reload bars, shown in the offscreen screenshot.

## 5. Special (kind 2): traced live, implemented

One of the six `FUN_1003d7ce` callers, `FUN_1003b947` (gated by `FUN_1003bac9(state)` on `state ∈ {3, 5, 0x20}`,
called twice within itself depending on whether a `DummyItemBase_t` RTTI cast of the relevant item succeeds), reads
`AttackDelay`(0x126) and `RechargeDelay`(0xd2) via the same `(**vtable+0x3c)(statId, 2)` effective-stat accessor as
Reload, sums them for an internal `< 0x1f` (31 hundredths) refusal check, and passes both raw values into
`FUN_1003d7ce`. The exact `CharSecSpecAttackIIR_t`/`SpecialAttackInfoIIR_t` apply addresses in `docs/zone/combat-
log.md` §2.2 never reference this function, so the receive-side trigger could not be pinned down from static RE
alone -- resolved instead with a live capture.

**Live capture** (`docs/captures/zone_special_burst_fixr_ithaca.rec`, headless `live_walk`, muted, within the 5-minute
live-session budget): `AOMAC_LIVE_CHAR=Aomacfixr AOMAC_NET_TRACE=<path> AOMAC_LIVE_STEPS='goto=hunt,Q=0.1,wait=5,
M=0.1,wait=10,goto=hunt,Q=0.1,wait=5,L=0.1,wait=10' cargo test --release -p aomac live_walk -- --ignored --nocapture`
(`M`/`L` are Aomacfixr's saved keybinds for Burst/Fling, per the existing `arm=burst:1:special,M=0.1,...` recipe in
this same file's own doc comment). Aomacfixr's rifle landed a real Burst: `CharSecSpecAttackIIR_t` send
`{target: 1080207, special: 148}` immediately followed by the server's `SpecialAttackInfoIIR_t`
`{slot: 6, damage: 43, target: 1080207, special: 148}`. **Critically, the SAME capture's own
`SpecialAttackWeaponIIR_t` re-announcement of Aomacfixr's own list** (sent right after, matching retail's existing
documented "fight-start re-announce" pattern) **carries only `{100 bare hands, 144 Dimach, 142 Brawl, 1}` -- no 148**.
Burst is therefore **not** a special-list item for a rifle; it must read the *wielded weapon* directly, exactly
matching `FUN_1003b947`'s other branch (the `DummyItemBase_t` RTTI cast failing falls through to reading the
controller's own slot item instead). `combat::arms::special_or_wielded_item` implements this: try the special's own
list key first (Brawl/Dimach-style `DummyItemBase_t` entries), else the wielded hand (`attack_slots`, Burst/Fling/
Full Auto).

**Trigger, as ported**: client-predicted like Attack/Reload/Item -- `combat/glue.rs`'s `fight_special` (hotbar/
action-menu) and `fight_cmd` (keyboard, the path the live capture's own `M` key used) both call
`Dynels::start_special_bar(own, id)` the moment `combat::module::is_special_id(id)` recognises the id as one of the
special-attack actions (`Module::special_action`'s own `Command::Special` id set, shared via a single function so
the two cannot drift). Duration = `(AttackDelay + RechargeDelay) / 100` of `special_or_wielded_item`, no initiative
scaling (matching Reload's own evidence: `FUN_1003b947`'s refusal check and `FUN_1003d7ce` construction both use the
raw stat sum, no `GameTime_t`/initiative-slope code in this function either).

The hotbar icon-fader recharge overlay (`CharacterActionIIR_t` action `0x14`, `hud_bar.rs`/`hud_special.rs`, already
ported before this investigation) is a **separate, additional** indicator -- it existed independently of this
`TimerEntry_t` mechanism and is not a substitute for the kind-2 bar.

## 6. Regressions

* `combat::arms::tests::round_delay_matches_the_item_info_dps_cell` / `item_round_seconds_sums_attack_and_recharge_in_
  hundredths` / `attack_slots_follow_wielded_hands`: the Attack-bar formula against the tooltip's own fixture.
* `dynels::tests::attack_and_reload_bars_track_hands_swings_and_fight_state`: the state machine (open on fight
  start, restart on swing, ignore an untracked slot, clear on fight stop) and the exact Reload duration (raw
  `RechargeDelay`/100, pinned to `5.0` for stat `0xd2 = 500`, no initiative term, no `AttackDelay` sum) and the
  Special bar's fall-through to the wielded weapon for Burst(148), pinned to `10.0` for AttackDelay=RechargeDelay=500.
* `combat::arms::tests::special_or_wielded_item_falls_back_to_the_weapon`: the special-list-then-wielded-weapon
  resolution order.
* `combat::glue::health_tests::captured_fight_drives_the_attack_bar`: replays `docs/captures/zone_fight_ithaca.rec`
  through a real `Module` + `Zone`, checking the Attack bar for the own character's bare hands opens, restarts once
  per own swing and clears on `StopFight`.
* `combat::glue::health_tests::captured_burst_drives_the_special_bar`: replays the live Burst capture through a real
  `Zone`, checking the sent `CharSecSpecAttackIIR_t`/received `SpecialAttackInfoIIR_t` special ids match, Burst is
  confirmed absent from the own special list, and the bar's duration matches the real installed Solar-Powered
  Assault Rifle's `AttackDelay`+`RechargeDelay` exactly.
* `hud_stats::tests::combat_timer_bars_stack_and_show_mid_fight`: two Attack bars (dual hands), a Reload bar and a
  real Body-Boost-driven Nano bar, each at a distinct `FindNextFreePos` slot with the retail label text, and an
  offscreen screenshot (`combat-timer-bars.png`).
