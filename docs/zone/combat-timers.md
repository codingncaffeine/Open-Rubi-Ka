# Attack / Reload / Nano / Item HUD timer bars

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
name). `FUN_1003da5d` (assign + wrap) has **three** callers: `FUN_1003d7ce`, plus weapon-slot constructors
`FUN_1009b37e` (dummy weapon, called by `FUN_10082512`) and `FUN_1009b415` (wielded weapon, called by
`FUN_1009c0ca`). The latter two retain an entry in the slot object rather than constructing one on each start.
The earlier assertion that `FUN_1003d7ce` was its only caller missed the Attack path.
`FUN_1003d7ce` is the construct-and-start helper; `FUN_1003d6b1` registers/starts an existing entry, sets its
kind at `+0x14`, and calls `FUN_1003d85b` to emit the start signal for the client character.
`DumpCallers 1003d7ce` finds six call sites. The kind argument must be checked at each caller:
the item-use state machine `FUN_1003b947` passes literal **4 (Item)**, not 2 (Special).
The generic GUI's kind-2 label alone does not identify a special-attack trigger.

## 2. Attack (kind 1): confirmed-fight trigger traced

**Starting an attack request is not starting its timer.** Gamecode `FUN_10067c34` only sets pending byte
`ctrl+0x79`, constructs `AttackIIR_t`, and calls `SendIIRToObservers`; it has no timer call.
Received `AttackIIR_t::Execute` (`FUN_1007c545`) calls `FUN_10069c68(target, 0)`. After `CanAttack`
(`FUN_10069556`) accepts the target, this calls `FUN_100681e5`, which visits the weapon slots and calls
`FUN_10067de8(slot, force)`. That function gets the slot's retained entry with `FUN_1006b0a3` and starts it
through **`FUN_1003d6b1(..., kind = 1)`**. `DumpAsm 10067de8 10067e8d` pins `EBX = 1`
(`0x10067e1c`/`0x10067e1e`), `PUSH EBX` (`0x10067e60`), then the recharge/attack durations,
the entry shared pointer, and the call at `0x10067e88`. At `0x10069d9d`–`0x10069db6`, fight setup
forces a fresh start when no kind-1 timer exists (`FUN_1003ceb9`); the slot helper otherwise respects its
retained entry's flags. Opening the first entry belongs to accepted fight setup, before the first
`AttackInfoIIR_t`, not a sent request or its first hit.
The same setup is refreshed during fighting equipment changes (`FUN_1006a700`, `FUN_1006a772`).
`StopFight`'s `FUN_10068b7f` emits the own character's timer-stop signal at `GlobalSignals+0x1fc`.

The port already follows that trigger end to end: keys/action shortcuts reach `Module::default_attack`,
which sets `AttackGate` and queues the request only. `Combat::on_frame` starts combat for received `Attack`
(or a full update's active target); `Module::attacking()` reads that confirmed state. `combat/glue.rs`
passes it to `Dynels::update_attack_bars`; `HudStats::update_attack_timers` mirrors those entries into windows.
Server refusal actions `0x93`/`0x76` do not start combat or bars. No behavioral send-vs-confirm change is
warranted by this trace. The reported refused-Attack association was an inference; its deleted logs/screenshots
cannot establish otherwise, and the surviving successful fights do not supply retail widget timing.

Offscreen live acceptance (2026-10-15, Aomacfixr 33588, ICC4582) corroborates the ordering:
sent Attack at **18017 ms**, received Attack at **18222 ms**, first own hit at **19203 ms**.
Before sending, `attacking=false`, Attack entries empty; the post-confirmation/pre-hit sample had
`attacking=true`, slot6 progress **0.35511562**, total **2.2333333 s**, and the inspected frame showed
Attack. After the second hit/StopFight it was absent. No rejected server request was exercised live;
the refusal regression covers that path. Relevant original frames are retained in
`docs/captures/zone_tab_rifle_fixr_ithaca.rec` (a combat-only subset, no login credentials).

**Remaining reconstruction, not established by this trigger trace:** the port displays a summed
initiative-scaled `AttackDelay`(294) + `RechargeDelay`(210) cycle (`item_round_seconds`) and resets it on
each own `CombatEvent::Hit`/`Miss`. Retail `FUN_10067de8` separately obtains those two durations through
`FUN_10067bef` -> `FUN_1006772b` and `FUN_10067c0d` -> `FUN_10067849`; its entry flags and cycle update
are not yet traced into equivalent HUD progress. This investigation establishes the opening trigger, not
the phase/restart model. The former special-attack corroboration was wrong: `FUN_1003b947` is item use (§5).

## 3. Reload (kind 5): traced to its exact caller, not a guess

* **Message class**: `ReloadIIR_t` 0x26515E61 (present in `ao_net::n3::outgoing`'s known-class table, not yet
  decoded into the `Misc`/`World` enums). Its apply is `FUN_10076780` (a bare vtable `Execute` slot, confirmed by
  `DumpCallers` on it resolving to a `<no function>` DATA ref -- a vtable entry, not a direct call) -> `FUN_10069274`.
* `FUN_10069274(ctrl, weaponIdentity, old, newCount, flag)`: reads the weapon's stat `0x1a` (26, the same
  current-ammo stat `chat/item_info.rs::ammo` already reads as "count"), and if it is present (`-1 < count`), **sets**
  stat `0x1a` to `newCount` (the clip refilling), calls `FUN_10068a89(weapon)`, then prints `Feedback_Reloading`.
* `FUN_10068a89` is one of the six `FUN_1003d7ce` callers and explicitly passes literal **kind 5**.
  Its first instructions (`~/aomac-ghidra re.sh Gamecode DumpAsm.java 10068a89 10068ac7`) push `(0xd2, 2)` into the item's
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
  investigation) -> tail `FUN_10050ed9`, which creates the cast timer for an active **non-instant** cast.
  Its address range (`asm 10050f85-10051064`) is already `nano_cast_delay`'s GC evidence. `DumpCallers 10051754`
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

## 5. Item (kind 4), not a special-attack timer (bug 12)

Fresh `Gamecode DumpFn 1003b947` shows **both** `FUN_1005863c(..., 4)` and
`FUN_1003d7ce(..., 4)`. `DumpAsm` pins the kind-4 pushes at `0x1003b9cf` before the timer call at
`0x1003b9e5`, and `0x1003ba5b` before the timer call at `0x1003ba71`.
This is the item-use state machine (also traced in `interact.md`), not a
weapon-special timer. The previous documentation incorrectly inferred a Special bar and a special-list/
wielded-weapon timer fallback from this function. That inference and the timer-only implementation introduced
by `4b054bf` have been removed from both hotbar/action-menu and keyboard dispatch.

`docs/captures/zone_special_burst_fixr_ithaca.rec` remains network evidence: the player sends
`CharSecSpecAttackIIR_t {target: 1080207, special: 148}` and receives
`SpecialAttackInfoIIR_t {slot: 6, damage: 43, target: 1080207, special: 148}`.
The capture proves that Burst exchange, not a timer widget or its duration. No timer fallback can be inferred
from Burst's absence in the captured special-weapon item list.

Actual special attacks, their animation/item resolution and the hotbar icon-fader recharge feed
(`CharacterActionIIR_t` action `0x14`, `hud_bar.rs`/`hud_special.rs`) remain intact.
The existing kind-4 Item wear/unwear bar remains intact as well.

## 6. Regressions

* `combat::arms::tests::round_delay_matches_the_item_info_dps_cell` / `item_round_seconds_sums_attack_and_recharge_in_
  hundredths` / `attack_slots_follow_wielded_hands`: the Attack-bar formula against the tooltip's own fixture.
* `dynels::tests::attack_and_reload_bars_track_hands_swings_and_fight_state`: the state machine (open on fight
  start, restart on swing, ignore an untracked slot, clear on fight stop) and the exact Reload duration (raw
  `RechargeDelay`/100, pinned to `5.0` for stat `0xd2 = 500`, no initiative term, no `AttackDelay` sum).
* `combat::glue::health_tests::captured_fight_drives_the_attack_bar`: replays `docs/captures/zone_fight_ithaca.rec`
  through a real `Module` + `Zone`, checking that the outgoing Attack has no bar, the received own Attack
  opens it before any own hit/miss, and the existing swing/reset/stop reconstruction remains covered.
* `combat::module::tests::attack_key_round_trip` / `server_refusal_prints_text_and_clears_the_gate`: exercise
  actual command send/pending, confirmation before any hit, stop, and refusal through the timer state.
  These assertions use message-stat weapons and do not require the installed client database.
  The capture regression above does require that database for its existing bare-hands item fixture.
* `combat::glue::health_tests::captured_burst_matches_the_server_result`: preserves the capture's matching sent
  `CharSecSpecAttackIIR_t`/received `SpecialAttackInfoIIR_t` special ids without asserting timer behaviour.
* `hud_stats::tests::combat_timer_bars_stack_and_show_mid_fight`: dispatches actual Burst/Brawl actions and checks
  that only two Attack bars, Reload and a real Body-Boost-driven Nano bar occupy the stack, with native labels
  and distinct slots; no Special timer is added. Its existing offscreen screenshot is `combat-timer-bars.png`.
* `hud_stats::tests::ncu_belt_equip_starts_and_counts_down_the_item_timer_bar`: retains Item-bar coverage.
