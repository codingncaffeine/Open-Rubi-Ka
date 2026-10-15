//! Offline ICC sky evidence, without login or a window:
//! `AOMAC_SHOT_DIR=/tmp/sky-before cargo test --release -p aomac icc_sky_screenshots -- --nocapture`
//! Repeat on the changed revision with `/tmp/sky-after`; matching filenames are comparison pairs.
//! No pixel baseline is asserted: a capture is evidence, not an invented aesthetic oracle.
use super::camera;
use ao_gui::{DrawList, Gui, InputEvent};
use ao_render::{Camera, Frontend, Host, Offscreen, Vec3};
use ao_scene::{Lens, Scene};
use std::path::PathBuf;

struct Shot {
    gui: Gui,
    scene: Option<Scene>,
    camera: Camera,
    lens: Lens,
}

impl Frontend for Shot {
    fn gui(&self) -> &Gui { &self.gui }
    fn input(&mut self, _: InputEvent, _: &mut Host) {}
    fn frame(&mut self, _: f32, _: (u32, u32), host: &mut Host) -> DrawList {
        if let Some(scene) = self.scene.take() { host.set_scene(scene); }
        host.camera = self.camera;
        host.lens = Some(self.lens);
        DrawList::default()
    }
}

#[test]
fn icc_sky_screenshots() -> anyhow::Result<()> {
    let Some(out) = std::env::var_os("AOMAC_SHOT_DIR").map(PathBuf::from) else { return Ok(()) };
    let dir = ao_gui::client_dir();
    if !dir.join("cd_image/rdb.db").exists() || !dir.join("cd_image/gui/Default/Graphics.uvgi").exists() {
        eprintln!("skipping ICC sky captures: no installed client at {}", dir.display());
        return Ok(());
    }
    std::fs::create_dir_all(&out)?;
    let store = ao_rdb::RecordStore::open(&dir)?;
    let mut shot = Shot {
        gui: Gui::new(&dir, None)?,
        scene: None,
        camera: Camera::look_at(Vec3::ZERO, -Vec3::Z),
        lens: Lens::default(),
    };
    let mut off = Offscreen::new(&shot, (1280, 800))?;
    for (time_name, time) in [("day2648.69", 2648.69), ("night0", 0.0)] {
        let scene = ao_formats::playfield::load_playfield_at(&store, &dir, 4582, time)?;
        assert_eq!(scene.day_time, time);
        assert!(!scene.sky.is_empty(), "ICC must supply the real sky layers");
        for instance in &scene.sky {
            assert!(instance.transform.iter().flatten().all(|v| v.is_finite()));
            let mesh = &scene.meshes[instance.mesh];
            assert!(!mesh.vertices.is_empty());
            assert!(mesh.vertices.iter().all(|v| v.pos.iter().all(|v| v.is_finite())));
        }
        let base = scene.lens.unwrap_or_default();
        // Offshore ICC sea camera: unobstructed sky, also exercised by liquid_real.
        // Fixed scene-space coordinates (AO world Z is negated), not a movable spawn heuristic.
        let eye = Vec3::new(1010.4, 24.0, -663.1);
        let heading = 2.4288;
        shot.scene = Some(scene);
        for size in [(1280, 800), (5120, 1440)] {
            off.resize(size);
            let aspect = size.0 as f32 / size.1 as f32;
            for wide in [false, true] {
                for scenery in [1.0, 2.0] {
                    // Identical preference mapping to Player::frame; do not substitute viewer defaults.
                    shot.lens = camera::lens(base, wide);
                    shot.lens.far = Some(camera::far_plane(camera::VIEW_DISTANCE) * scenery);
                    assert_eq!(shot.lens.near, camera::NEAR);
                    assert_eq!(shot.lens.far, Some(800.0 * scenery));
                    let vfov = shot.lens.vertical_fov(aspect);
                    let hfov = 2.0 * ((vfov * 0.5).tan() * aspect).atan();
                    assert!((hfov - camera::horizontal_fov(wide, aspect)).abs() < 1e-5);
                    for (heading_name, yaw) in [("forward", heading), ("right", heading + std::f32::consts::FRAC_PI_2)] {
                        for (view, pitch) in [("up", 60.0_f32.to_radians()), ("horizon", 0.0), ("down", -60.0_f32.to_radians())] {
                            shot.camera = Camera { pos: eye, yaw, pitch, roll: 0.0 };
                            let projection = glam::Mat4::perspective_rh(vfov, aspect, shot.lens.near, shot.lens.far.unwrap());
                            let view_matrix = glam::Mat4::look_to_rh(eye, shot.camera.forward(), shot.camera.up());
                            assert!((projection * view_matrix).is_finite());
                            // Freeze renderer time, clouds, movers and sky animations across the matrix.
                            let list = off.frame(&mut shot, 0.0);
                            let name = format!("icc4582-{time_name}-{}x{}-wide{}-scenery{scenery}-{heading_name}-{view}.png", size.0, size.1, u8::from(wide));
                            eprintln!("{name}: eye={eye:?}, yaw={yaw}, pitch={pitch}, lens={:?}", shot.lens);
                            off.png(&shot, &list, &out.join(name))?;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

/// Isolates each authored sky mesh without changing instance indices or animation metadata.
/// `AOMAC_SHOT_DIR=/tmp/sky-layers cargo test --release -p aomac icc_sky_layer_screenshots -- --nocapture`
#[test]
fn icc_sky_layer_screenshots() -> anyhow::Result<()> {
    let Some(out) = std::env::var_os("AOMAC_SHOT_DIR").map(PathBuf::from) else { return Ok(()) };
    let dir = ao_gui::client_dir();
    if !dir.join("cd_image/rdb.db").exists() || !dir.join("cd_image/gui/Default/Graphics.uvgi").exists() {
        eprintln!("skipping ICC sky layers: no installed client at {}", dir.display());
        return Ok(());
    }
    std::fs::create_dir_all(&out)?;
    let store = ao_rdb::RecordStore::open(&dir)?;
    let scene = ao_formats::playfield::load_playfield_at(&store, &dir, 4582, 2648.69)?;
    assert!(!scene.sky.is_empty(), "ICC must supply the real sky layers");
    let mut lens = camera::lens(scene.lens.unwrap_or_default(), false);
    lens.far = Some(camera::far_plane(camera::VIEW_DISTANCE));
    let mut shot = Shot {
        gui: Gui::new(&dir, None)?,
        scene: None,
        camera: Camera { pos: Vec3::new(1010.4, 24.0, -663.1), yaw: 2.4288, pitch: 0.0, roll: 0.0 },
        lens,
    };
    let mut off = Offscreen::new(&shot, (1280, 800))?;
    for (ordinal, instance) in scene.sky.iter().enumerate() {
        let mesh = &scene.meshes[instance.mesh];
        let (lo, hi) = mesh.vertices.iter().fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |(lo, hi), vertex| {
                let p = Vec3::from(vertex.pos);
                (lo.min(p), hi.max(p))
            },
        );
        eprintln!("layer{ordinal:02}: mesh={} vertices={} bounds={lo:?}..{hi:?} transform={:?}", instance.mesh, mesh.vertices.len(), instance.transform);
        for (material, submesh) in mesh.submeshes.iter().enumerate() {
            eprintln!("  material{material}: texture={:?} blend={:?} base={:?} emissive={:?} sky_fog={} indices={}",
                submesh.texture, submesh.blend, submesh.base_color, submesh.emissive, submesh.sky_fog, submesh.indices.len());
        }
        let mut isolated = scene.clone();
        for other in &scene.sky {
            if other.mesh != instance.mesh {
                isolated.meshes[other.mesh].submeshes.clear();
            }
        }
        shot.scene = Some(isolated);
        let list = off.frame(&mut shot, 0.0);
        off.png(&shot, &list, &out.join(format!("layer{ordinal:02}.png")))?;
    }
    Ok(())
}
