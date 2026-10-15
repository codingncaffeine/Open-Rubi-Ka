//! Installed-client GPU regression: `cargo test --release -p aomac --test liquid_real -- --nocapture`.

#[test]
fn icc_sea_contains_alpha_blended_water_pixels() -> anyhow::Result<()> {
    let dir = ao_gui::client_dir();
    if !dir.join("cd_image/rdb.db").exists() {
        eprintln!("skipping ICC water pixels: no installed client at {}", dir.display());
        return Ok(());
    }
    let store = ao_rdb::RecordStore::open(&dir)?;
    let mut scene = ao_formats::playfield::load_playfield_at(&store, &dir, 4582, 2648.69)?;
    assert!(scene.liquids.iter().any(|surface| surface.style == 0), "ICC must contain native water surfaces");
    assert!(scene.meshes.iter().flat_map(|mesh| &mesh.submeshes).any(|sub| {
        sub.liquid && sub.texture.is_some_and(|key| key.rdb_type == 1_010_004 && key.id == 6329)
    }), "the real water5 texture must be bound to a liquid");
    let eye = [1010.4, 24.0, -663.1];
    let at = [1090.0, 15.0, -571.0];
    let out = std::env::temp_dir().join(format!("aomac-icc-water-{}", std::process::id()));
    std::fs::create_dir_all(&out)?;
    let wet_path = out.join("wet.png");
    let dry_path = out.join("dry.png");
    ao_render::render_to_png_at(&scene, eye, at, 640, 400, &wet_path, 0.0)?;
    // Remove only water draws, preserving every terrain/statel/sky/light and the camera.
    // Clearing the sources also prevents update_liquids from regenerating the removed indices.
    scene.liquids.clear();
    for mesh in &mut scene.meshes {
        mesh.submeshes.retain(|sub| !sub.liquid);
    }
    ao_render::render_to_png_at(&scene, eye, at, 640, 400, &dry_path, 0.0)?;
    let wet = image::open(&wet_path)?.to_rgba8();
    let dry = image::open(&dry_path)?.to_rgba8();
    std::fs::remove_file(wet_path)?;
    std::fs::remove_file(dry_path)?;
    std::fs::remove_dir(out)?;
    // Test actual visible water, not just decoded geometry or a synthetic stand-in.
    // SRCCOLOR/ONE (the stale pipeline slots 28/29) can only brighten the destination;
    // native water's two alpha passes also attenuate it. Ignore the upper sky half.
    let mut changed = 0;
    let mut attenuated = 0;
    for (water, background) in wet.pixels().zip(dry.pixels()).skip(640 * 200) {
        let delta: [i16; 3] = std::array::from_fn(|i| i16::from(water[i]) - i16::from(background[i]));
        changed += usize::from(delta.iter().any(|value| value.abs() > 8));
        attenuated += usize::from(delta.iter().sum::<i16>() < -24);
    }
    eprintln!("ICC sea: {changed} changed pixels, {attenuated} attenuated water pixels");
    assert!(changed > 1000, "real ICC water must cover visible pixels, got {changed}");
    assert!(attenuated > 100, "native water must alpha-blend, not use the sky's additive glow: {attenuated} pixels");
    Ok(())
}
