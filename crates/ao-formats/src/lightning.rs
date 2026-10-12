//! The client's native lightning (Gamecode.dll two storm schedules + DisplaySystem.dll FX8
//! `VisualCloudlight_t` fan visual), gated entirely by `weather::State::lightning_enabled()`
//! (unchanged by this module: `f[9] > 0.0 && f[2] >= 0.4`).
//!
//! # Evidence (Gamecode.dll, DisplaySystem.dll)
//! * `FUN_100b921d` (ctor): two independent single-seed [`Schedule`]s — storm 1 seed `0x85c19`,
//!   period 600 s (`_DAT_10167f18`), gap 0.1..20 s (`_DAT_10160810`/`_DAT_10167f10`); storm 2
//!   seed `0x1e23a`, period 300 s (`_DAT_10167f14`), gap 0.1..2 s (`_DAT_10158794`).
//!   `FUN_100b15e8` → `FUN_100b104c(seed, period, gap_min, gap_max, 16, 16)`: the *same* per-
//!   window event-generation loop as `weather::Schedule` (one seed ⇒ `window == period`, a
//!   single-window schedule, not the weather schedule's three windows), 16 floats + 16 ints per
//!   event (weather draws only 8 ints; no decompiled lightning consumer reads the ints at all —
//!   kept for the native allocation shape, not for any effect on the floats, which are drawn
//!   first from the same per-event R250 stream).
//! * `FUN_100b0d1f`/`FUN_100b0f33` (advance/retire): the exact algorithm already ported as
//!   `Schedule::step`/`advance`. Unlike the weather schedule (driven by `GameDayTime`), the
//!   lightning schedules are driven directly by `fmod(GameTime_t::GetCurrentRealTime(), period)`
//!   — no day-time conversion, no window offset ([`Lightning::advance`]).
//! * Strike position (`FUN_100b1622` quantize, `FUN_100b16a7`/`FUN_100b17c7` X/Z — both newly
//!   decompiled for this port, not pre-resolved): `extent = tilemap +0x8264 tile size *
//!   playfield +0x50 scale * 6.0` (`_DAT_10158ba0`, confirmed `double 6.0` by raw file read).
//!   `candidate = floor(controlledCoord / extent) * extent + nextEventFloat() * extent`
//!   ([`quantize`]), wrapped by ±`extent` when `|candidate − controlledCoord| > extent * 0.5`
//!   (`_DAT_10155f00`, confirmed `double 0.5`) ([`wrap`]). `nextEventFloat` is `FUN_100b0e34`,
//!   i.e. `Schedule::draw_f`, called once for X then once for Z (two consecutive draws from the
//!   fired event's own 16-float array).
//! * `FUN_100b9850` (fire handler): `source = (strikeX, cameraY + 30 [`_DAT_101600e8`],
//!   strikeZ)`, `ground = source + (0, -70 [`_DAT_10167fa8`], 0)`. Starts FX8
//!   (`FUN_100afc50(8)`); on success plays `SM_Sandy_Env_NearLightning` with `volume = 1 − 0.5 ·
//!   distance / extent`, `delay = distance / 100` (`_DAT_10158670`, confirmed `double 100.0`)
//!   where `distance` is the horizontal camera-to-strike distance ([`Strike::audio`]). `distance
//!   < 25 m` (`_DAT_10167fa0`, confirmed `double 25.0`) additionally sends `AFCM::Send(9, 0xbf,
//!   cameraPos, (25.0 [`_DAT_10167f98`], 1.5 · (1 − distance / 25) [`_DAT_10160928`, confirmed
//!   `double 1.5`], 0))` — see "Not reproduced" for why no camera impulse is applied.
//! * FX8 = `VisualCloudlight_t` (DisplaySystem `FUN_10065675` ctor; `FUN_1006089d` sets render
//!   states 14=0, 27=1, 28=0, 137=0, 20=2; `FUN_100516ce` builds the initial 10-fan vertex/index
//!   buffer, 50 vertices / 120 indices). `FUN_100508bd` (`ActivateFX`, vtable+0x60) and
//!   `FUN_10050d3e` (vtable+0x54, the "additional companion instance" variant — newly decompiled
//!   for this port) both: seed a DisplaySystem-side [`R250`] (`FUN_1008445d`/`FUN_1008449f`,
//!   confirmed the same lagged-xor R250 algorithm as [`R250`] by raw decompile of the seed/next
//!   functions) — `FUN_100508bd` with the fired event's own seed (`FUN_100b0e88` ≡
//!   `Schedule::active_seed`), `FUN_10050d3e` with a fresh CRT `rand()` (not reproducible from
//!   the event — matches the client's own non-determinism, ported with [`CrtRand`] like the rest
//!   of this crate's native `rand()` uses) — leave fan slot 0 untouched (see "Not reproduced")
//!   and for fan slots 1..=9: `cx = rand() % 70 − 35` (`FUN_100508bd`) or `rand() % 30 − 15`
//!   (`FUN_10050d3e`), same formula for `cz`; `half = rand() % 20 + 25`; the 4 corners are
//!   `(cx∓half, 0, cz∓half)`, the 5th vertex is the center `(cx, 0, cz)` ([`Fan::vertices`]); a
//!   `rand() % 100 < 40` roll additionally starts a companion flash (undecoded `VisualEnvFX_t`
//!   kind 9, see "Not reproduced") at `source + (cx, 0, cz)` — ground gets the same offset in
//!   `FUN_100508bd`'s loop, but stays at the unmodified `ground` in `FUN_10050d3e`'s own
//!   probabilistic branch. These int draws are a signed 32-bit DisplaySystem R250 word
//!   reinterpreted as `i32` (`FUN_1008449f` returns the raw XOR'd word): the native `%` is C's
//!   truncating signed modulo, so e.g. `cx`/`cz` actually range -104..=34, not -35..=34, and the
//!   `< 40` roll succeeds roughly 70% of the time, not 40% — both reproduced exactly, quirks
//!   included.
//! * `FUN_100516ce`'s second loop (all 10 fan slots, including slot 0): zeroes the corner
//!   vertices' color (transparent corners) and rolls the center vertex's persistent RGB with
//!   plain `rand()` — 1-in-3 `(0, 255−rand()%128, 255)`, else `(255, 255−rand()%128, 200−rand()%
//!   128)` ([`fan_color`]). `FUN_10050c13` (draw): re-rolls only the center vertex's alpha byte
//!   every draw call — the flicker ([`flicker_alpha`]). `FUN_10050bc3`: lifetime starts at 1.0 s
//!   (`0x3f800000`, set at activation), counts down by `dt`, expires below 0.01 s (`_DAT_
//!   1008ae90`, confirmed `float 0.01`) ([`FAN_LIFETIME`], [`fan_expired`]).
//!
//! # Not reproduced (UNRESOLVED)
//! * Fan slot 0 (the strike's own "anchor" fan): neither `FUN_100516ce` (construction) nor
//!   `FUN_100508bd`/`FUN_10050d3e` (activation) ever write its 5 vertices — only slots 1..=9 are
//!   algorithmically positioned. Slot 0's position/size most likely come from the FX8 FXS
//!   resource template (loaded via `FXS_t::GetInstance()` in `FUN_10065675`), which was not
//!   traced. [`Strike::fans`] therefore holds only the 9 algorithmically-derived extra fans.
//! * FX kind 9 (`VisualEnvFX_t::StartFX(9)`), the companion lightning-bolt/flash visual paired
//!   with the strike and with each fan that rolls one: only the call site is known
//!   (`SetFXPos`/`SetFXGroundPos`/`StartFX`); `VisualEnvFX_t`'s own construction/geometry/
//!   material functions are a separate, untraced class.
//! * `FUN_10050d3e` (the FXS pool's "additional instance" path, vtable+0x54): the fan math is
//!   fully reproduced ([`additional_instance_fans`]), but **when** the native client summons it
//!   instead of reusing the primary FX8 instance (presumably FXS resource-pool occupancy — e.g.
//!   two strikes firing before the first's fans expire) was not traced; this is the FX9 pool/
//!   lifetime semantics gap flagged before starting this port.
//! * The `<25 m` `AFCM::Send(9, 0xbf, …)` camera impulse: the trigger condition and payload
//!   constants are confirmed (see above), but AFCM.dll dispatches message processors through a
//!   runtime `RegisterMessageProcessor` table (only the `AFCM::Send` overload *symbols* are
//!   visible in Gamecode.dll's import names; the module-9/0xbf consumer inside AFCM.dll that
//!   would turn this payload into an actual camera displacement is not decoded).
//!   `aomac::play::camera::apply_ground_shake` is an existing offset-the-eye primitive, but it
//!   backs a different native path (GC effect kind 3032 "GroundShake",
//!   `aomac::play::combat::effects_aux`); feeding it a guessed direction/magnitude here would not
//!   be faithful, so [`Strike::audio`] only reports `camera_impulse: bool` and applies nothing.
//! * Live wiring (`aomac::play::lightning::LightningFx`): `extent` comes from `Collision::tile_size()` (the same
//!   `Tilemap` already retained for terrain collision — no second rdb 1000009 load) through [`extent`]. `aomac`'s
//!   effect renderer (`aomac::play::combat::effects`) is bound 1:1 to RDB-templated, dynel-addressed GC effect
//!   classes — FX8/FX9 have no RDB template (pure native weather code) and are not bound to a dynel, so `LightningFx`
//!   draws fan quads as a small parallel list of transient additive actors alongside it instead of fabricating a
//!   template that does not exist. `SM_Sandy_Env_NearLightning`'s `delay = distance / 100` s is approximated as
//!   immediate playback: the one native per-frame delayed-command queue found in `ao_audio` (`game::Runtime::
//!   effect_delays`) backs a different system (AFCM103 authored effect sounds) with its own variant-selection and
//!   one-due-record-per-frame throttling, so reusing it here would misrepresent two different native subsystems as
//!   one, and a second lightning-only queue was judged disproportionate for a sub-second offset.

use crate::weather::{CrtRand, Schedule, R250};

/// `FUN_100b921d`: storm 1 seed `0x85c19` (`FUN_100b15e8(0x85c19, _DAT_10167f18, …)`), storm 2
/// seed `0x1e23a`.
const STORM_SEED: [u32; 2] = [0x0008_5c19, 0x0001_e23a];
/// `_DAT_10167f18` / `_DAT_10167f14`.
const STORM_PERIOD: [f32; 2] = [600.0, 300.0];
/// `_DAT_10160810`, shared by both storms.
const STORM_GAP_MIN: f32 = 0.1;
/// `_DAT_10167f10` / `_DAT_10158794`.
const STORM_GAP_MAX: [f32; 2] = [20.0, 2.0];
/// `FUN_100b104c(..., 16, 16)`.
const EVENT_INTS: usize = 16;

/// `_DAT_10158ba0`: tile size * playfield scale * 6.0 is the strike-position extent.
pub const EXTENT_FACTOR: f32 = 6.0;
/// `FUN_100b16a7`/`FUN_100b17c7` (newly decompiled for this port): `n3Playfield_t +0x50`, a runtime *object* field (int,
/// cast to float) — distinct from the on-disk `RDBPlayfield_t +0x50` `flags` field ([`crate::playfield::record::Record::flags`]).
/// No corresponding field exists in the decoded playfield record (whose own fields are version/id/name/tilemap/
/// zone_size/count/flags/vehicle_flags — none semantically a scale) or in `playfield::environment::Env`, and no
/// named tweak/cvar string ("Scale"-prefixed, searched in both N3.dll and Gamecode.dll) ties a scale to a playfield;
/// the setter of this runtime field was not traced. Treated as the engine default `1.0` for this port (every PRK
/// playfield observed uses it unscaled).
pub const PLAYFIELD_SCALE: f32 = 1.0;

/// `tile size * playfield scale * 6.0` (`_DAT_10158ba0`): the strike-position extent fed to [`Lightning::advance`] and
/// [`Strike::audio`]. `tile_size` is `ao_formats::playfield::collision::Collision::tile_size()` (outdoor only; `None`
/// there means `extent` should be `0.0`, which fires nothing).
pub fn extent(tile_size: f32) -> f32 {
    tile_size * PLAYFIELD_SCALE * EXTENT_FACTOR
}
/// `_DAT_10155f00`: the wrap threshold is half the extent.
const WRAP_HALF: f32 = 0.5;
/// `_DAT_101600e8`: the source sits this far above the camera.
const SOURCE_ABOVE_CAMERA: f32 = 30.0;
/// `_DAT_10167fa8`: the ground sits this far below the source.
const GROUND_BELOW_SOURCE: f32 = -70.0;
/// `_DAT_10158670`: audio delay divisor (metres per second of delay).
const AUDIO_DELAY_DIVISOR: f32 = 100.0;
/// `_DAT_10167fa0`: the camera-impulse distance gate.
pub const CAMERA_IMPULSE_DISTANCE: f32 = 25.0;

/// `FUN_100b1622`: `floor(coord / extent) * extent`.
pub fn quantize(coord: f32, extent: f32) -> f32 {
    (coord / extent).floor() * extent
}

/// `FUN_100b16a7`/`FUN_100b17c7`: wrap a candidate strike coordinate to within `extent / 2` of
/// `controlled`.
pub fn wrap(candidate: f32, controlled: f32, extent: f32) -> f32 {
    let diff = candidate - controlled;
    let half = extent * WRAP_HALF;
    if diff > half {
        candidate - extent
    } else if diff < -half {
        candidate + extent
    } else {
        candidate
    }
}

/// A fan-shaped cloud-light patch local to a strike's source.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fan {
    pub center: [f32; 2],
    pub half_size: f32,
}

impl Fan {
    /// The 5 local vertices (`y = 0`): the 4 corners then the center
    /// (`FUN_100508bd`/`FUN_10050d3e`'s vertex writes).
    pub fn vertices(&self) -> [[f32; 3]; 5] {
        let [cx, cz] = self.center;
        let h = self.half_size;
        [[cx - h, 0.0, cz - h], [cx + h, 0.0, cz - h], [cx - h, 0.0, cz + h], [cx + h, 0.0, cz + h], [cx, 0.0, cz]]
    }
}

/// Triangle-fan indices local to one [`Fan`]'s own 5 vertices: 4 triangles around the center
/// (index 4) covering the 4 corners in order.
pub const FAN_INDICES: [u32; 12] = [4, 0, 1, 4, 1, 3, 4, 3, 2, 4, 2, 0];

/// `FUN_100516ce`'s per-fan center-vertex color roll (`rand() % 3 == 0`): a 1-in-3 cyan-leaning
/// roll, else a warm roll. The alpha byte is re-rolled every draw call instead
/// ([`flicker_alpha`]), not included here.
pub fn fan_color(rng: &mut CrtRand) -> [u8; 3] {
    if rng.rand() % 3 == 0 {
        [0, 255u8.wrapping_sub((rng.rand() & 0x7f) as u8), 255]
    } else {
        let g = 255u8.wrapping_sub((rng.rand() & 0x7f) as u8);
        let b = 200u8.wrapping_sub((rng.rand() & 0x7f) as u8);
        [255, g, b]
    }
}

/// `FUN_10050c13`: the center vertex's alpha byte is re-rolled every draw call, the flicker.
pub fn flicker_alpha(rng: &mut CrtRand) -> u8 {
    (rng.rand() & 0xff) as u8
}

/// `FUN_10050bc3`: initial lifetime set at activation (`0x3f800000`).
pub const FAN_LIFETIME: f32 = 1.0;
/// `_DAT_1008ae90`.
const FAN_EXPIRE_EPSILON: f32 = 0.01;

/// `true` once a fan with this much lifetime left has expired and should be detached
/// (`FUN_10050bc3`: expires below 0.01 s, not at exactly 0).
pub fn fan_expired(remaining: f32) -> bool {
    remaining < FAN_EXPIRE_EPSILON
}

/// The `SM_Sandy_Env_NearLightning` cue (`FUN_100b9850`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioCue {
    pub distance: f32,
    pub volume: f32,
    pub delay: f32,
    /// `distance < 25 m`: the native client also sends an `AFCM` module-9 message `0xbf`
    /// ("camera impulse"-shaped payload) here; see the module doc for why nothing is applied.
    pub camera_impulse: bool,
}

/// One lightning strike (`FUN_100b9850`'s per-fired-event body).
#[derive(Clone, Debug, PartialEq)]
pub struct Strike {
    /// `(strikeX, cameraY + 30, strikeZ)`.
    pub source: [f32; 3],
    /// `source + (0, -70, 0)`.
    pub ground: [f32; 3],
    /// The 9 randomized extra fans (`FUN_100508bd`'s loop, fan slots 1..=9); fan slot 0 is not
    /// reproduced here, see the module doc.
    pub fans: [Fan; 9],
    /// Indices into [`Strike::fans`] whose `rand() % 100 < 40` roll also started a companion
    /// flash at `source + (fan.center.x, 0, fan.center.y)` (ground offset likewise).
    pub companions: Vec<usize>,
}

impl Strike {
    /// `FUN_100b9850`: `volume = 1 − 0.5 · distance / extent`, `delay = distance / 100`,
    /// `camera_impulse` when `distance < 25`. `camera` is the camera's world position; `extent`
    /// the same value passed to [`Lightning::advance`].
    pub fn audio(&self, camera: [f32; 3], extent: f32) -> AudioCue {
        let dx = self.source[0] - camera[0];
        let dz = self.source[2] - camera[2];
        let distance = (dx * dx + dz * dz).sqrt();
        AudioCue { distance, volume: 1.0 - WRAP_HALF * distance / extent, delay: distance / AUDIO_DELAY_DIVISOR, camera_impulse: distance < CAMERA_IMPULSE_DISTANCE }
    }
}

/// The two native storm schedules (`FUN_100b921d`). Callers must check
/// `weather::State::lightning_enabled()` themselves before calling [`Lightning::advance`] — this
/// struct does not read the weather state, matching how `Wind`/`Quake` are driven externally.
pub struct Lightning {
    storms: [Schedule; 2],
}

impl Default for Lightning {
    fn default() -> Self {
        Lightning { storms: std::array::from_fn(|i| Schedule::new(&[STORM_SEED[i]], STORM_PERIOD[i], STORM_GAP_MIN, STORM_GAP_MAX[i], EVENT_INTS)) }
    }
}

impl Lightning {
    /// `FUN_100b0d1f`/`FUN_100b0f33`: advance both schedules to `real_time` seconds (native
    /// `fmod(GameTime_t::GetCurrentRealTime(), period)`, no day-time conversion) and build a
    /// [`Strike`] for every event that fires in this step. `controlled` is the camera/player
    /// world position, `extent` is `tile size * playfield scale * 6.0` (`FUN_100b16a7`),
    /// `camera_y` the camera's own Y. `extent <= 0.0` (no tilemap loaded) fires nothing.
    pub fn advance(&mut self, real_time: f64, controlled: [f32; 3], extent: f32, camera_y: f32) -> Vec<Strike> {
        let mut strikes = Vec::new();
        if extent <= 0.0 {
            return strikes;
        }
        for storm in &mut self.storms {
            if storm.is_empty() {
                continue;
            }
            let t = (real_time as f32).rem_euclid(storm.period());
            while storm.step(t) {
                strikes.push(fire(storm, controlled, extent, camera_y));
            }
        }
        strikes
    }
}

/// `FUN_100508bd`/`FUN_10050d3e`'s shared fan-building loop: 9 fans (`cx`/`cz` from
/// `offset_range`/`offset_min`, `half` from `rand() % 20 + 25`), plus each fan's `rand() % 100 <
/// 40` companion-flash roll.
fn build_fans(mut next: impl FnMut() -> i32, offset_range: i32, offset_min: i32) -> ([Fan; 9], Vec<usize>) {
    let mut fans = [Fan { center: [0.0, 0.0], half_size: 0.0 }; 9];
    let mut companions = Vec::new();
    for (i, fan) in fans.iter_mut().enumerate() {
        let cx = (next() % offset_range + offset_min) as f32;
        let cz = (next() % offset_range + offset_min) as f32;
        let half = (next() % 20 + 25) as f32;
        *fan = Fan { center: [cx, cz], half_size: half };
        if next() % 100 < 40 {
            companions.push(i);
        }
    }
    (fans, companions)
}

/// `FUN_100b9850`'s per-fired-event body (strike position, source/ground, the 9 extra fans).
fn fire(storm: &mut Schedule, controlled: [f32; 3], extent: f32, camera_y: f32) -> Strike {
    let qx = quantize(controlled[0], extent);
    let qz = quantize(controlled[2], extent);
    let sx = wrap(qx + storm.draw_f() * extent, controlled[0], extent);
    let sz = wrap(qz + storm.draw_f() * extent, controlled[2], extent);
    let source = [sx, camera_y + SOURCE_ABOVE_CAMERA, sz];
    let ground = [sx, camera_y + SOURCE_ABOVE_CAMERA + GROUND_BELOW_SOURCE, sz];
    let mut gen = R250::new(storm.active_seed());
    let (fans, companions) = build_fans(|| gen.next_u32() as i32, 70, -35);
    Strike { source, ground, fans, companions }
}

/// `FUN_10050d3e`: the FXS-pool "additional instance" variant (vtable+0x54, called when the
/// primary FX8 instance from [`fire`] is still busy with an un-expired strike) — the same
/// fan-building algorithm, reseeded with a fresh CRT `rand()` (not reproducible from any
/// lightning event) and a narrower ±15 offset range. **When** the native client actually summons
/// this instead of reusing the primary one depends on `FXS_t`'s resource-pool occupancy, which
/// is not decompiled here (see the module doc's "Not reproduced"); this function only reproduces
/// the fan math so a future pool-aware caller can use it.
pub fn additional_instance_fans(crt: &mut CrtRand) -> ([Fan; 9], Vec<usize>) {
    let mut gen = R250::new(crt.rand() as u32);
    build_fans(|| gen.next_u32() as i32, 30, -15)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quantize_and_wrap_match_decompiled_formula() {
        // extent = tile size 10.0 * playfield scale 1 * 6.0 = 60.0 (_DAT_10158ba0).
        let extent = 60.0f32;
        assert_eq!(quantize(125.0, extent), 120.0);
        assert_eq!(quantize(-5.0, extent), -60.0);
        // within extent/2 (30.0, _DAT_10155f00) of controlled: unchanged.
        assert_eq!(wrap(110.0, 100.0, extent), 110.0);
        // candidate - controlled = 40 > half(30): wraps down by extent.
        assert_eq!(wrap(140.0, 100.0, extent), 80.0);
        // candidate - controlled = -40 < -half: wraps up by extent.
        assert_eq!(wrap(60.0, 100.0, extent), 120.0);
    }

    #[test]
    fn fan_vertices_are_the_axis_aligned_diamond_plus_center() {
        let fan = Fan { center: [10.0, -5.0], half_size: 25.0 };
        assert_eq!(fan.vertices(), [[-15.0, 0.0, -30.0], [35.0, 0.0, -30.0], [-15.0, 0.0, 20.0], [35.0, 0.0, 20.0], [10.0, 0.0, -5.0]]);
        // indices stay inside the 5 local vertices and use every one.
        assert!(FAN_INDICES.iter().all(|&i| i < 5));
        let mut used: Vec<u32> = FAN_INDICES.to_vec();
        used.sort_unstable();
        used.dedup();
        assert_eq!(used, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn source_ground_and_audio_cue_match_decompiled_constants() {
        let storm = &mut Lightning::default().storms[0];
        // force a deterministic fire: period 600, gap 0.1..20 guarantees events exist.
        assert!(!storm.is_empty());
        let t0 = storm.times().next().unwrap();
        assert!(storm.step(t0));
        let strike = fire(storm, [0.0, 0.0, 0.0], 60.0, 5.0);
        assert_eq!(strike.source[1], 5.0 + 30.0);
        assert_eq!(strike.ground, [strike.source[0], strike.source[1] - 70.0, strike.source[2]]);
        let cue = strike.audio([strike.source[0], strike.source[1], strike.source[2] + 20.0], 60.0);
        assert!((cue.distance - 20.0).abs() < 1e-4);
        assert!((cue.volume - (1.0 - 0.5 * 20.0 / 60.0)).abs() < 1e-4);
        assert!((cue.delay - 0.2).abs() < 1e-4);
        assert!(cue.camera_impulse); // 20.0 < 25.0
        let far = strike.audio([strike.source[0], strike.source[1], strike.source[2] + 40.0], 60.0);
        assert!(!far.camera_impulse); // 40.0 >= 25.0
    }

    #[test]
    fn fan_offsets_stay_in_the_decompiled_range_and_nine_fans_are_produced() {
        let mut storm = Schedule::new(&[0x85c19], 600.0, 0.1, 20.0, EVENT_INTS);
        let t0 = storm.times().next().unwrap();
        assert!(storm.step(t0));
        let strike = fire(&mut storm, [0.0, 0.0, 0.0], 60.0, 0.0);
        assert_eq!(strike.fans.len(), 9);
        for fan in &strike.fans {
            // `rand() % 70 - 35` on a signed 32-bit DisplaySystem R250 draw (not CrtRand, so the
            // dividend can be negative): truncating `%` gives -(70-1)..=(70-1), so -104..=34.
            assert!((-104.0..=34.0).contains(&fan.center[0]), "{fan:?}");
            assert!((-104.0..=34.0).contains(&fan.center[1]), "{fan:?}");
            // `rand() % 20 + 25`, same signed-draw reasoning: -(20-1)+25..=(20-1)+25 = 6..=44.
            assert!((6.0..=44.0).contains(&fan.half_size), "{fan:?}");
        }
        assert!(strike.companions.iter().all(|&i| i < 9));
    }

    #[test]
    fn fan_lifetime_and_flicker_match_decompiled_constants() {
        assert_eq!(FAN_LIFETIME, 1.0);
        assert!(!fan_expired(0.5));
        assert!(!fan_expired(0.01));
        assert!(fan_expired(0.0099));
        assert!(fan_expired(-1.0));
        let mut crt = CrtRand::default();
        for _ in 0..100 {
            let [r, g, b] = fan_color(&mut crt);
            assert!(r == 0 || r == 255);
            assert!((128..=255).contains(&g));
            if r == 255 {
                assert!((73..=200).contains(&b));
            } else {
                assert_eq!(b, 255);
            }
            let _ = flicker_alpha(&mut crt);
        }
    }

    #[test]
    fn additional_instance_fans_use_the_narrower_offset_range() {
        let mut crt = CrtRand::default();
        let (fans, companions) = additional_instance_fans(&mut crt);
        assert_eq!(fans.len(), 9);
        for fan in &fans {
            // `rand() % 30 - 15` on a signed 32-bit draw: -(30-1)-15..=(30-1)-15 = -44..=14.
            assert!((-44.0..=14.0).contains(&fan.center[0]), "{fan:?}");
            assert!((-44.0..=14.0).contains(&fan.center[1]), "{fan:?}");
            assert!((6.0..=44.0).contains(&fan.half_size), "{fan:?}");
        }
        assert!(companions.iter().all(|&i| i < 9));
        // two calls with the same CrtRand stream reseed R250 differently each time (fresh rand()
        // seed per FUN_10050d3e call) and so do not reproduce the same fans.
        let (fans2, _) = additional_instance_fans(&mut crt);
        assert_ne!(fans, fans2);
    }

    #[test]
    fn two_storms_fire_within_one_period_at_their_own_cadence() {
        let mut lightning = Lightning::default();
        let mut storm1_strikes = 0usize;
        let mut t = 0.0f64;
        while t < 600.0 {
            let strikes = lightning.advance(t, [0.0, 0.0, 0.0], 60.0, 0.0);
            for s in &strikes {
                assert!(s.source[1].is_finite() && s.ground[1].is_finite());
            }
            storm1_strikes += strikes.len();
            t += 0.5;
        }
        let storm2_strikes = lightning.storms[1].len();
        assert!(storm1_strikes > 0, "storm 1 never fired over a full period");
        assert!(storm2_strikes > 0, "storm 2 has no events");
        assert_eq!(lightning.storms[0].period(), 600.0);
        assert_eq!(lightning.storms[1].period(), 300.0);
    }

    #[test]
    fn zero_extent_fires_nothing() {
        let mut lightning = Lightning::default();
        assert!(lightning.advance(100.0, [0.0, 0.0, 0.0], 0.0, 0.0).is_empty());
    }

    #[test]
    fn extent_is_tile_size_times_scale_times_six() {
        assert_eq!(PLAYFIELD_SCALE, 1.0);
        assert_eq!(extent(10.0), 60.0);
        assert_eq!(extent(0.0), 0.0);
    }
}
