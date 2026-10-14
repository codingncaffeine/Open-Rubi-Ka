# Attack / Reload HUD timer bars

Code: `play/combat/arms.rs` (`round_delay`, `item_round_seconds`, `attack_slots`), `play/dynels.rs` (`update_attack_bars`,
`restart_attack_bar`, `attack_bar_progress`, `start_reload_bar`, `reload_bar_progress`), `play/combat/glue.rs` (per-frame
wiring), `play/hud_use.rs` (Reload trigger), `play/hud_stats.rs` (`update_attack_timers`, `update_reload_timer`,
`free_timer_slot`), `play/hud_timerbar.rs` (`COLOR_ATTACK`/`LABEL_ATTACK`, `COLOR_RELOAD`/`LABEL_RELOAD`, `pos_slot`).

## 1. The widget: `TimerBarBase_c` / `TimerSystemModule_t` (already ported)

`hud_timerbar.rs` already carries the faithful RE of the generic timer-bar widget from the "Logout" countdown
(`/camp`, `hud_use.rs`) and the item wear/unwear delay bar (`hud_stats.rs`). Relevant evidence, re-confirmed here:

* **Kind table** (`TimerSystemModule_t::StartTimerBar` [GUI 0x10051a27], `TimerEntry_t+0x14`): 1 = Attack (tint
  `0xaaffaa`, text.mdb 10000 `Timer_Attack` = "Attack"), 2 = Special (`0xaaffff`, `Timer_Special` = "Special"), 3 = Nano
  (`0xffaaaa`, `Timer_Nano` = "Nano program"), 4 = Item (`0xaaaaff`, `Timer_Item` = "Item", only shown when the duration
  is at least [`hud_timerbar::MIN_SECONDS`] = 0.5 s, `_DAT_101a9f9c`), 5 = Reload (untinted `0xffffff`, `Timer_Reload` =
  "Reload"). The English strings were read directly from the installed `cd_image/text/text.mdb` (`TextDb::by_key(10000,
  key)`), not guessed.
* **Position**: `TimerSystemModule_t::FindNextFreePos` [GUI 0x10051779] stacks concurrent bars 20 px apart starting at
  `IPoint(0x28, (n + 2) * 0x14)` = (40, 40 + 20n), taking the lowest free slot index. `hud_timerbar::pos_slot(n)` and
  `HudStats::free_timer_slot` (a linear scan over the slots held by the open Item/Attack/Reload bars) now implement the
  real multi-bar stacking instead of a single hard-coded slot 0.
* **Only `StartTimerBar` kind 4 ("Item") and the public `CreateTimer` overload ("Logout") were traced to their actual
  Gamecode trigger.** Kind 1/2/3/5 are only reachable through `TimerSystemModule_t::StartTimerBar`, itself only called
  from its own `InitialiseMessage`/`ShutdownMessage` (signal connect/disconnect, GUI `DumpCallers 0x10051a27`); the
  `weak_ptr<TimerEntry_t>` that the signal actually carries is constructed by whoever emits it, which was **not**
  found by searching GUI.dll for `TimerEntry_t` constructors, `CreateTimer`/`StartTimerBar` callers, or matching
  Gamecode.dll strings (`AttackDelay`/`RechargeDelay`/`ReloadTime`/`Reload` do not appear as literal Gamecode strings;
  `~/aomac-ghidra/re.sh Gamecode DumpRefs.java "AttackDelay"` etc. are empty). **[UNRESOLVED]** exact emission site of
  the Attack/Special/Nano/Reload `TimerEntry_t`s; see §4.

## 2. Attack bar: design and evidence used

* **Duration formula (hard evidence, already ported)**: `chat::item_info_combat::delay` (the item-info tooltip's DPS
  cell) already implements `FUN_1006772b`/`FUN_10067849`: hundredths of a second, `raw_stat + 75 - initiative_slope -
  AggDef`, clamped to an item cap (stat 523 `AttackDelayCap` / 524 `RechargeDelayCap`) and a 1.0 s floor;
  `initiative <= 1200` scales ×0.5, else `(initiative - 1200) / 6 + 600`. `combat::arms::round_delay` is the same
  formula, factored out so both the tooltip and the HUD bar share one source of truth (`round_delay_matches_the_
  item_info_dps_cell` pins the tooltip's own fixture against it). `item_round_seconds(item, stats)` sums
  `round_delay(AttackDelay=stat 294)` + `round_delay(RechargeDelay=stat 210, recharge=true)`, in seconds.
* **Which stat scales which**: the item's own `InitiativeSkill` (stat 440) names which of the character's skills
  scales `AttackDelay`; `RechargeDelay` always uses the *same* value if it already lies in 0..=1200, else the
  `FUN_10067849`-observed fallback skill 118 (Physical Initiative) — this is the tooltip's own established behaviour,
  reused verbatim.
* **Per-hand**: `Armory::attack_slots` (GC 0x10068969, already used by `in_attack_range`) is hands 8/6 if either is
  wielded, else bare hands 0 — the same slot set `in_attack_range` already treats as "the attacking weapon(s)".
  Dual-wielding therefore opens two concurrent Attack bars, one per hand, each keyed to that hand's own weapon.
* **Start / restart**: there is no client-side round timer in this engine (docs/zone/combat-net.md §3: "no timer and
  no timeout… no client-side attack repeat" — rounds are entirely server-driven). The Attack bar is the client's own
  *visual prediction* of the round, exactly like the already-ported Item bar is a cosmetic prediction of an
  equip/unequip delay that the server alone actually resolves (`hud_stats::update_item_timer`'s own comment). It opens
  while `combat::module::Module::attacking()` is true, one per currently wielded attack slot, and **restarts at every
  swing of that hand** — `CombatEvent::Hit`/`Miss` both carry `attacker`/`slot` (`combat/notes.rs::hit_of`, already
  used to drive the swing animation/sound of that exact slot), so a swing that just landed or missed is read as "the
  weapon fired, the round starts over" without inventing a second signal.
* **[GUESS]**: that retail really restarts kind-1 "Attack" per swing rather than running once per whole fight, and
  that AttackDelay+RechargeDelay (the DPS formula's own "round time") is the bar's total, not AttackDelay alone. Both
  follow directly from what the `StartTimerBar` kind table and the DPS formula already prove client-side, but the
  exact Gamecode call site that opens/closes/updates a kind-1 bar was not found (§1/§4), so this is the best-evidenced
  reconstruction, not a traced call.

## 3. Reload bar: design, evidence, and the open gap

* **Trigger (hard evidence)**: the hotbar Reload special action (id `0x6e`, `combat::action::RELOAD`,
  `combat-net.md` §5.3's provider table) calls `FUN_1006949a` [GC], gated by `FUN_10029e66` (a null-check on the fight
  controller's `+4` field; always true off the current no-fight-controller stub), which sends a bare
  `CharacterActionIIR_t` action `0xd2` with every identity/param zero (`hud_use.rs`, unchanged from before this
  change). No duration or ammo count travels on this message.
* **No `ReloadTime` stat exists in this client's data.** Searched: `ao-formats/data/stat_names.txt` (full file, no
  `Reload`/`Clip` entry besides the already-mapped ammo-count/clip-size pair stats 26/212 used by
  `chat/item_info.rs::ammo`), `~/aomac-ghidra/re.sh Gamecode DumpRefs.java "ReloadTime" "Reload"` (empty — no literal
  string), the action-0xd2 apply path (`FUN_1005d0d8`'s decompile, 1264 lines, no `0xd2`/`210` case) and `gui.md`
  §10.7's own prior note ("UNRESOLVED: … Reload (stat 0x1a guess)" — that guess is about which stat feeds the hotbar
  icon's disabled-overlay progress, not a timer duration, and was never resolved either).
* **[GUESS] duration**: `Dynels::start_reload_bar` uses the *same* `item_round_seconds` the Attack bar uses, applied
  to the first wielded hand that carries ammo (`item.ammo != -1`, stat `AmmoType` 420/`0x1a4`). This is the only
  evidenced weapon timing this client's rdb carries; it is explicitly not a traced `ReloadTime` value. The bar is
  purely a client-side cosmetic prediction — like the Attack and Item bars — and does not gate anything server-side;
  it self-closes once its predicted duration elapses (`Dynels::update_attack_bars`).
* **Not implemented**: an authoritative close on an ammo-count `StatIIR` (stat 26 reaching the clip's max again) once
  the server actually finishes the reload, because no live reload capture exists to confirm the server announces it
  that way rather than via a full item-stat replace. Left as a predicted-duration close only, labelled here so a
  future capture can correct it.

## 4. Not ported: Special / Nano kinds

Kinds 2 ("Special") and 3 ("Nano") were not wired to a bar. The special-attack recharge countdown already has its own,
separately-RE'd mechanism — the hotbar icon's disabled overlay (`GFX_GUI_ICONFADER32` shrink + `%2ds`/`%2dm` text,
fed by the relayed `CharacterActionIIR_t` action `0x14` "recharge feed", `hud_bar.rs` §"Recharge feed" /
`hud_actionwin.rs`, already implemented before this change) — and no second, independent "Special" `TimerBarBase_c`
bar was found triggered anywhere in the traced call graph. No client-visible nano cast-time bar was found either
(nano casting in this engine's traffic is `CastNanoSpell`/`BuffIIR_c`, applied atomically, with no observed
in-progress state to drive a fill). Both remain [UNRESOLVED GUESS: not present] until a capture or further Gamecode
trace shows otherwise.

## 5. Regressions

* `combat::arms::tests::round_delay_matches_the_item_info_dps_cell` / `item_round_seconds_sums_attack_and_recharge_in_
  hundredths` / `attack_slots_follow_wielded_hands`: the shared formula against the tooltip's own fixture, and the
  per-hand slot selection.
* `dynels::tests::attack_and_reload_bars_track_hands_swings_and_fight_state`: the state machine in isolation (open on
  fight start, restart on swing, ignore an untracked slot, clear on fight stop, Reload bar progress/self-close).
* `combat::glue::health_tests::captured_fight_drives_the_attack_bar`: replays `docs/captures/zone_fight_ithaca.rec`
  (the only real fight capture where the own character — Aomacvolk, bare-handed — actually fights) through a real
  `Module` + `Zone` (real rdb if installed, skips otherwise), checking the Attack bar for slot 0 opens with a sane
  initiative-scaled duration, restarts exactly once per own swing (hits + misses), and clears once the fight stops.
