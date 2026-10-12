//! Live wiring of `ao_formats::lightning` into the zone flow: ticks the native storm schedules every frame, turns
//! fired [`ao_formats::lightning::Strike`]s into transient additive fan quads (FX8's `VisualCloudlight_t`; see the
//! module doc of `ao_formats::lightning` for why these are not routed through `combat::effects` — FX8/FX9 have no
//! RDB template and are not bound to a dynel). `SM_Sandy_Env_NearLightning`'s `delay = distance / 100` s is played
//! immediately (approximated, see the `ao_formats::lightning` module doc).

use std::sync::Arc;

use ao_formats::lightning::{self, Lightning, Strike};
use ao_formats::weather::CrtRand;
use ao_scene::{ActorFrame, Blend, Mesh, Scene, Submesh, Vertex, IDENTITY};

/// Model key of the shared 5-vertex fan mesh ([`fan_model`]); every strike's fans reuse it with a per-actor `skin`
/// override (dynamic world-space vertices, like `combat::effects`' own particle/mesh actors).
pub const FAN_MODEL: u64 = 0xfac3_0000_0000_0000;
/// High `id` range for fan actors, distinct from `combat::effects`' `ACTOR_BASE` (`0xe000_0000`) and real dynel ids.
const FAN_ACTOR_BASE: u32 = 0xd000_0000;

/// The shared fan submesh ([`lightning::FAN_INDICES`]): additive, untextured, two-sided (the native fan is a flat
/// quad meant to be seen from any angle while falling). Vertex positions are overridden per actor by [`LightningFx::actors`]'s `skin`.
pub fn fan_model() -> Scene {
    let mut sub = Submesh::new(lightning::FAN_INDICES.to_vec(), None);
    sub.blend = Blend::Additive;
    sub.two_sided = true;
    Scene { meshes: vec![Mesh { vertices: vec![Vertex::default(); 5], submeshes: vec![sub] }], ..Default::default() }
}

struct ActiveFan {
    id: u32,
    /// World positions of the 5 local vertices (`strike.source + Fan::vertices()`).
    verts: [[f32; 3]; 5],
    /// `fan_color`'s roll, applied to the center vertex (index 4); corners stay transparent.
    color: [u8; 3],
    /// `FAN_LIFETIME` counting down to `fan_expired`.
    life: f32,
}

/// Per-zone lightning state: the native storm schedules plus the render/audio side this port adds.
pub struct LightningFx {
    engine: Lightning,
    fans: Vec<ActiveFan>,
    /// Flicker/color RNG (`fan_color`/`flicker_alpha`'s own non-reproducible `CrtRand` draws, matching
    /// `lightning::additional_instance_fans`'s treatment of the same native non-determinism).
    rng: CrtRand,
    /// `FUN_100b0d1f`/`FUN_100b0f33`'s own free-running real-time clock, accumulated frame to frame.
    elapsed: f64,
    next_id: u32,
    model_uploaded: bool,
}

impl Default for LightningFx {
    fn default() -> Self {
        LightningFx { engine: Lightning::default(), fans: Vec::new(), rng: CrtRand::default(), elapsed: 0.0, next_id: 0, model_uploaded: false }
    }
}

impl LightningFx {
    /// One frame: advances the storms when `enabled` (`weather::State::lightning_enabled()`), ages and drops expired
    /// fans, and returns the strikes fired this frame (for the caller's own audio one-shot via
    /// `Strike::audio`/`ao_audio::Audio::play_lightning_cue`). `controlled` is the camera/player world position,
    /// `extent` is `ao_formats::lightning::extent(tile_size)` (`0.0` fires nothing), `camera_y` the camera's own Y.
    pub fn tick(&mut self, enabled: bool, dt: f32, controlled: [f32; 3], extent: f32, camera_y: f32) -> Vec<Strike> {
        self.elapsed += f64::from(dt);
        let strikes = if enabled { self.engine.advance(self.elapsed, controlled, extent, camera_y) } else { Vec::new() };
        for strike in &strikes {
            for fan in &strike.fans {
                let verts = fan.vertices().map(|[x, y, z]| [strike.source[0] + x, strike.source[1] + y, strike.source[2] + z]);
                let id = FAN_ACTOR_BASE | (self.next_id & 0x0fff_ffff);
                self.next_id = self.next_id.wrapping_add(1);
                self.fans.push(ActiveFan { id, verts, color: lightning::fan_color(&mut self.rng), life: lightning::FAN_LIFETIME });
            }
        }
        for fan in &mut self.fans {
            fan.life -= dt;
        }
        self.fans.retain(|f| !lightning::fan_expired(f.life));
        strikes
    }

    /// This frame's actor model upload, once per [`LightningFx`] instance (reset on zone load, matching
    /// `combat::effects`' own "upload once" `host.actor_models` precedent).
    pub fn take_model_upload(&mut self) -> Option<(u64, Scene)> {
        if self.model_uploaded {
            None
        } else {
            self.model_uploaded = true;
            Some((FAN_MODEL, fan_model()))
        }
    }

    /// This frame's fan quads, ready to push into `host.actors` (model [`FAN_MODEL`]).
    pub fn actors(&mut self) -> Vec<ActorFrame> {
        let Self { fans, rng, .. } = self;
        fans.iter()
            .map(|fan| {
                let alpha = lightning::flicker_alpha(rng);
                let center = [f32::from(fan.color[0]) / 255.0, f32::from(fan.color[1]) / 255.0, f32::from(fan.color[2]) / 255.0, f32::from(alpha) / 255.0];
                let verts: Vec<Vertex> =
                    fan.verts.iter().enumerate().map(|(i, &pos)| Vertex { pos, color: if i == 4 { center } else { [0.0; 4] }, ..Default::default() }).collect();
                ActorFrame { id: fan.id, model: FAN_MODEL, transform: IDENTITY, skin: Some(Arc::new(verts)), always: true, ..Default::default() }
            })
            .collect()
    }

    /// Active fan count, for tests.
    #[cfg(test)]
    pub fn fan_count(&self) -> usize {
        self.fans.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_fires_nothing_even_with_a_valid_extent() {
        let mut fx = LightningFx::default();
        let mut fired = 0usize;
        for _ in 0..2000 {
            fired += fx.tick(false, 0.5, [0.0, 0.0, 0.0], 60.0, 5.0).len();
        }
        assert_eq!(fired, 0);
        assert_eq!(fx.fan_count(), 0);
        assert!(fx.actors().is_empty());
    }

    #[test]
    fn enabled_eventually_fires_and_renders_nine_fans_per_strike() {
        let mut fx = LightningFx::default();
        let mut strikes = Vec::new();
        for _ in 0..2000 {
            strikes.extend(fx.tick(true, 0.5, [0.0, 0.0, 0.0], 60.0, 5.0));
            if !strikes.is_empty() {
                break;
            }
        }
        assert!(!strikes.is_empty(), "storm 1 (period 600 s) never fired over 1000 s");
        assert_eq!(fx.fan_count(), strikes.iter().map(|s| s.fans.len()).sum::<usize>(), "9 fans queued per strike, nothing expired yet");
        assert!(fx.fan_count() >= 9, "9 fans per strike should be queued");
        let actors = fx.actors();
        assert_eq!(actors.len(), fx.fan_count());
        for actor in &actors {
            assert_eq!(actor.model, FAN_MODEL);
            let skin = actor.skin.as_ref().unwrap();
            assert_eq!(skin.len(), 5);
            // corners transparent, center carries the flicker alpha
            for v in &skin[..4] {
                assert_eq!(v.color, [0.0; 4]);
            }
            assert!(skin[4].color[3] >= 0.0 && skin[4].color[3] <= 1.0);
        }
    }

    #[test]
    fn fans_expire_after_their_lifetime() {
        let mut fx = LightningFx::default();
        let mut strikes = Vec::new();
        for _ in 0..2000 {
            strikes.extend(fx.tick(true, 0.5, [0.0, 0.0, 0.0], 60.0, 5.0));
            if !strikes.is_empty() {
                break;
            }
        }
        assert!(fx.fan_count() > 0);
        fx.tick(false, lightning::FAN_LIFETIME + 0.1, [0.0, 0.0, 0.0], 60.0, 5.0);
        assert_eq!(fx.fan_count(), 0, "fans must expire after FAN_LIFETIME");
    }

    #[test]
    fn model_upload_happens_once() {
        let mut fx = LightningFx::default();
        assert!(fx.take_model_upload().is_some());
        assert!(fx.take_model_upload().is_none());
    }
}
