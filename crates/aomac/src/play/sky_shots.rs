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

/// One landmark-inferred view of the user's current-client screenshot, not retail visual proof.
/// `AOMAC_SHOT_DIR=/tmp/icc-retail cargo test --release -p aomac icc_retail_sky_screenshots -- --nocapture`
/// Optional scene-space calibration: AOMAC_SKY_EYE=x,y,z; AOMAC_SKY_YAW/PITCH (radians);
/// AOMAC_SKY_TIME (game seconds), AOMAC_SKY_WIDE=0|1. Reuse identical values before/after.
#[test]
fn icc_retail_sky_screenshots() -> anyhow::Result<()> {
    use std::fmt::Write;
    let Some(out) = std::env::var_os("AOMAC_SHOT_DIR").map(PathBuf::from) else { return Ok(()) };
    let dir = ao_gui::client_dir();
    anyhow::ensure!(dir.join("cd_image/rdb.db").exists(), "no installed client at {}", dir.display());
    let scalar = |name: &str, default: f32| -> anyhow::Result<f32> {
        let value = std::env::var(name).map_or(Ok(default), |s| s.parse::<f32>())?;
        anyhow::ensure!(value.is_finite(), "{name} must be finite");
        Ok(value)
    };
    // [INFERENCE] Calibrated against the user's current-client screenshot landmarks.
    // Baseline camera-view-i: wreck mast ~(-3,+3), shuttle antenna ~(-5,0),
    // overhang right edge ~(+7,0) pixels relative to the 2090x1083 screenshot.
    // Foreground shoreline/grass still differs; this is not an exact recovered camera.
    let mut eye = Vec3::new(933.5, 26.0, -748.0);
    if let Ok(value) = std::env::var("AOMAC_SKY_EYE") {
        let xyz = value.split(',').map(str::parse::<f32>).collect::<Result<Vec<_>, _>>()?;
        anyhow::ensure!(xyz.len() == 3 && xyz.iter().all(|v| v.is_finite()), "AOMAC_SKY_EYE must be finite x,y,z");
        eye = Vec3::new(xyz[0], xyz[1], xyz[2]);
    }
    let yaw = scalar("AOMAC_SKY_YAW", 3.765)?;
    let pitch = scalar("AOMAC_SKY_PITCH", 0.046)?;
    let time = scalar("AOMAC_SKY_TIME", 2648.69)?;
    anyhow::ensure!((0.0..6480.0).contains(&time), "AOMAC_SKY_TIME must be in 0..6480");
    let wide = scalar("AOMAC_SKY_WIDE", 0.0)?;
    anyhow::ensure!(wide == 0.0 || wide == 1.0, "AOMAC_SKY_WIDE must be 0 or 1");
    let store = ao_rdb::RecordStore::open(&dir)?;
    let scene = ao_formats::playfield::load_playfield_at(&store, &dir, 4582, time)?;
    assert!(!scene.sky.is_empty(), "ICC must supply authored sky layers");
    let mut lens = camera::lens(scene.lens.unwrap_or_default(), wide == 1.0);
    lens.far = Some(camera::far_plane(camera::VIEW_DISTANCE));
    let mut shot = Shot {
        gui: Gui::new(&dir, None)?,
        scene: None,
        camera: Camera { pos: eye, yaw, pitch, roll: 0.0 },
        lens,
    };
    let mut report = format!(
        "reference=user current-client Screenshot 2026-10-15 at 16.50.21.png (NOT retail visual proof)\nsize=2090x1083\ncamera=INFERRED, not recovered from screenshot\n\
         eye={eye:?} yaw={yaw} pitch={pitch} direction={:?} lens={lens:?}\n\
         time={time} (game seconds, NOT screenshot/chat wall time)\n\
         default_camera_source=offscreen landmark calibration camera-view-i; initial area located from docs/zone/camera.md ICC idle capture\n\
         default_camera_residuals_approx_pixels= wreck_mast(-3,+3), shuttle_antenna(-5,0), overhang_right_edge(+7,0)\n\
         default_camera_limit=foreground grass/shoreline differs; no exact camera or saved preferences recovered\n\
         screenshot_landmarks=left sea wreck; raised shuttle right; grass beach; overhang upper-left\n\
         native_ICC_game_time=2648.69, authored FrozenTime override via FXS duplicate-object replacement; independent of wall16:50\n\
         scene_environment={:?}\n\
         Combined frame contains installed static playfield and sky, not captured avatar/NPCs/HUD.\n",
        shot.camera.forward(), scene.environment,
    );
    let names = ao_formats::character::NameTable::load(&store)?;
    let tweaks = ao_formats::playfield::Tweaks::load(&dir, 4582, true)
        .ok_or_else(|| anyhow::anyhow!("missing ICC tweaks"))?;
    let objects = tweaks.objects();
    writeln!(report, "\nAuthored pre-render objects (including objects hidden at this game time):")?;
    for (index, object) in objects.iter().enumerate().filter(|(_, o)| {
        o.field("Priority").is_some_and(|p| p.trim() == "e_RenderPriority_PreRendering")
    }) {
        let mesh = object.string("Mesh");
        let record = mesh.and_then(|name| names.id(ao_formats::mesh::MESH_TYPE, name));
        writeln!(report, "object{index}: name={} fxid={} mesh={mesh:?} record={record:?} texture={:?} sort={:?} visibility={:?} states={:?}",
            object.name, object.fxid(), object.texture, object.field("Sorting"), object.field("Visibility"), object.states)?;
        writeln!(report, "  authored fields={:?}", object.fields)?;
    }
    let mut off = Offscreen::new(&shot, (2090, 1083))?;
    std::fs::create_dir_all(&out)?;
    shot.scene = Some(scene.clone());
    let list = off.frame(&mut shot, 0.0);
    off.png(&shot, &list, &out.join("icc-retail-combined.png"))?;
    // Keep instance indices and animation metadata intact. Empty world draw geometry rather
    // than removing instances that movers/LOD reference.
    let mut sky_only = scene.clone();
    sky_only.liquids.clear();
    sky_only.ground_detail.clear();
    for (index, mesh) in sky_only.meshes.iter_mut().enumerate() {
        if !scene.sky.iter().any(|instance| instance.mesh == index) {
            mesh.submeshes.clear();
        }
    }
    shot.scene = Some(sky_only.clone());
    let list = off.frame(&mut shot, 0.0);
    off.png(&shot, &list, &out.join("icc-retail-sky.png"))?;
    for (ordinal, instance) in scene.sky.iter().enumerate() {
        let mesh = &scene.meshes[instance.mesh];
        let (lo, hi) = mesh.vertices.iter().fold(
            (Vec3::splat(f32::INFINITY), Vec3::splat(f32::NEG_INFINITY)),
            |(lo, hi), v| (lo.min(Vec3::from(v.pos)), hi.max(Vec3::from(v.pos))),
        );
        assert!(lo.is_finite() && hi.is_finite());
        writeln!(report, "\nlayer{ordinal:02}: mesh={} vertices={} bounds={lo:?}..{hi:?} transform={:?}",
            instance.mesh, mesh.vertices.len(), instance.transform)?;
        let object_index = scene.sky_pose_objects.iter().find_map(|&(instance, object)| {
            (instance == ordinal).then_some(object)
        }).ok_or_else(|| anyhow::anyhow!("layer{ordinal:02} has no authored object mapping"))?;
        let object = objects.get(object_index)
            .ok_or_else(|| anyhow::anyhow!("layer{ordinal:02} object index {object_index} is out of range"))?;
        let source_mesh = object.string("Mesh");
        let record = source_mesh.and_then(|name| names.id(ao_formats::mesh::MESH_TYPE, name));
        writeln!(report, "  authored object{object_index}: name={} fxid={} mesh={source_mesh:?} record={record:?} sorting={:?}",
            object.name, object.fxid(), object.field("Sorting"))?;
        writeln!(report, "  authored transform: Rotation={:?} Scale={:?} ScaleType={:?} Position={:?} PositionType={:?} MeshIndex={:?}",
            object.field("Rotation"), object.field("Scale"), object.field("ScaleType"),
            object.field("Position"), object.field("PositionType"), object.field("MeshIndex"))?;
        writeln!(report, "  Geometry above is baked; authored formulas and factory-frame transforms are not the identity instance transform.")?;
        for (material, submesh) in mesh.submeshes.iter().enumerate() {
            writeln!(report, "  material{material}: texture={:?} blend={:?} base={:?} emissive={:?} sky_fog={} fog_color={:?} indices={}",
                submesh.texture, submesh.blend, submesh.base_color, submesh.emissive,
                submesh.sky_fog, submesh.sky_fog_color, submesh.indices.len())?;
        }
        let mut isolated = sky_only.clone();
        for (index, other) in scene.sky.iter().enumerate() {
            if index != ordinal {
                anyhow::ensure!(other.mesh != instance.mesh, "shared sky mesh cannot be isolated by clearing submeshes");
                isolated.meshes[other.mesh].submeshes.clear();
            }
        }
        shot.scene = Some(isolated);
        let list = off.frame(&mut shot, 0.0);
        off.png(&shot, &list, &out.join(format!("icc-retail-layer{ordinal:02}.png")))?;
    }
    std::fs::write(out.join("icc-retail-objects.txt"), &report)?;
    eprintln!("{report}");
    Ok(())
}
