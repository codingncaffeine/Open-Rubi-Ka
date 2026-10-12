//! Liquid polygons: the first part of the `RDBPlayfieldAnarchy_t` tail that follows the zone / room
//! list of a playfield record (rdb 1000001), see `record::Record::tail`.
//!
//! ```text
//! u32 n; n x { u32 kind, u32 nv, nv x f32[3] vertex (AO world x,y,z), u32 nt, nt x u16[3] triangle }
//! ```
//! This is the `n3WaterData_t` array that `PlayfieldAnarchy_t::GetWaters` returns (count @ +0x98, data @ +0x9c;
//! `n3Zone_t::SetWater` N3 @0x1001abf2 keeps the highest water level per zone). The record parses with this
//! layout in all 601 playfields (outdoor and dungeon); the data after it is `environment`.
//!
//! `kind` is the `VisualWater_t::Create` liquid type (DisplaySystem @0x1003ad24): bit 0 = sloped/flowing
//! polygon (river, fall), `kind >> 1` selects the liquid. The liquid colours are the under-water fog colours of
//! `FUN_100b7273` (Gamecode, the camera-in-liquid check): `AddFog(r,g,b,density)` per `kind >> 1`.

use std::collections::hash_map::Entry;

use anyhow::{ensure, Result};
use ao_rdb::RecordStore;
use ao_scene::{Blend, Instance, Mesh, Scene, Submesh, TextureKey, Vertex, IDENTITY};

use super::record::Rd;
use crate::character::NameTable;
use crate::texture::decode_texture;

const TEXTURES: u32 = 1_010_004;

#[derive(Debug, Clone, PartialEq)]
pub struct Water {
    pub kind: u32,
    pub verts: Vec<[f32; 3]>,
    pub tris: Vec<[u16; 3]>,
}

pub fn parse(r: &mut Rd) -> Result<Vec<Water>> {
    let n = r.u32()? as usize;
    ensure!(n < 100_000, "implausible liquid count {n}");
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let kind = r.u32()?;
        let nv = r.u32()? as usize;
        ensure!(nv < 100_000, "implausible liquid vertex count {nv}");
        let verts = (0..nv).map(|_| r.vec3()).collect::<Result<Vec<_>>>()?;
        let nt = r.u32()? as usize;
        ensure!(nt < 100_000, "implausible liquid triangle count {nt}");
        let mut tris = Vec::with_capacity(nt);
        for _ in 0..nt {
            let t = [r.u16()?, r.u16()?, r.u16()?];
            ensure!(t.iter().all(|&i| (i as usize) < nv), "liquid triangle index out of range");
            tris.push(t);
        }
        out.push(Water { kind, verts, tris });
    }
    Ok(out)
}

/// Retained independent lava/star stages.
/// Water/slime/mud/acid/shallow and native default-kind materials use `native_mesh`.
struct Liquid {
    /// Vertex colour ARGB bytes (sRGB-ish, as D3D: the alpha is the vertex alpha, the water textures are JPEG).
    argb: u32,
    texture: &'static str,
    /// Metres per texture repeat (40 star, 8 lava).
    scale: f32,
}

const WATER_TEX: &str = "water5.png";

fn legacy_liquid(kind: u32) -> Option<Liquid> {
    let (argb, texture, scale) = match kind >> 1 {
        1 => (0xffff_0000, WATER_TEX, 8.0),
        7 => (0xffff_ffff, "stars01.png", 40.0),
        _ => return None,
    };
    Some(Liquid { argb, texture, scale })
}

/// Crust of the lava: `env_lava2.png` (RGBA, alpha 60..255) blended over the lava with its own alpha; the client multiplies the
/// alphas of two copies at 75 m and 250 m (`FUN_1003c356`, blob 0x7e8), we draw one at 75 m with the mean of the product.
const CRUST_TEX: &str = "env_lava2.png";
const CRUST_SCALE: f32 = 75.0;
const CRUST_ALPHA: f32 = 0.84;

fn argb_to_linear(argb: u32) -> [f32; 4] {
    let b = |s: u32| (argb >> s & 0xff) as f32 / 255.0;
    let c = |s| super::environment::srgb_to_linear(b(s));
    [c(16), c(8), c(0), b(24)]
}

fn texture(store: &RecordStore, names: &NameTable, scene: &mut Scene, name: &str) -> Option<TextureKey> {
    let key = TextureKey { rdb_type: TEXTURES, id: names.id(TEXTURES, name)? };
    if let Entry::Vacant(e) = scene.textures.entry(key) {
        e.insert(decode_texture(&store.get(TEXTURES, key.id).ok()??).ok()?);
    }
    Some(key)
}

/// Emits the client's five style lists in native draw order. RealisticWater controls
/// geometry refinement of styles 0/2/4 only; lava and star water keep their independent stages.
pub fn emit(store: &RecordStore, waters: &[Water], scene: &mut Scene) -> Result<()> {
    let names = NameTable::load(store)?;
    for style in [0, 2, 1, 3, 4] {
        let source: Vec<_> = waters.iter().filter(|w| native_style(w.kind) == Some(style) && !w.tris.is_empty()).collect();
        if source.is_empty() { continue; }
        let mesh_index = scene.meshes.len();
        let mesh = if matches!(style, 0 | 2 | 4) {
            let scale = if style == 2 { 12.0 } else if style == 4 { 160.0 } else { 40.0 };
            let triangles = source.iter().flat_map(|w| w.tris.iter().map(|t| ao_scene::liquid::SourceTriangle {
                kind: w.kind,
                vertices: t.map(|i| { let p = w.verts[i as usize]; Vertex { pos: [p[0], p[1], -p[2]], uv: [p[0] / scale, p[2] / scale], ..Default::default() } }),
            })).collect();
            let surface = ao_scene::liquid::Surface::new(mesh_index, style, triangles).map_err(anyhow::Error::msg)?;
            let Some(camera) = surface.initial_camera() else { continue };
            let mesh = native_mesh(&surface, camera, &mut |name| texture(store, &names, scene, name))?;
            scene.liquids.push(surface);
            mesh
        } else {
            let Some(mesh) = legacy_mesh(source, &mut |name| texture(store, &names, scene, name)) else { continue };
            mesh
        };
        scene.meshes.push(mesh);
        scene.instances.push(Instance { mesh: mesh_index, transform: IDENTITY });
    }
    Ok(())
}

fn native_style(kind: u32) -> Option<u32> {
    ao_scene::liquid::style(kind)
}

fn native_mesh(surface: &ao_scene::liquid::Surface, camera: [f32; 3], tex: &mut dyn FnMut(&str) -> Option<TextureKey>) -> Result<Mesh> {
    let mut mesh = Mesh::default();
    let (mut indices, mut colors) = (Vec::new(), Vec::new());
    surface.geometry(camera, false, &mut mesh.vertices, &mut indices, &mut colors);
    let name = if surface.style == 2 { "toxicwater.png" } else { WATER_TEX };
    let key = tex(name).ok_or_else(|| anyhow::anyhow!("missing native liquid texture {name}"))?;
    let mut sub = Submesh::new(indices, Some(key));
    sub.liquid = true;
    sub.two_sided = true;
    sub.blend = Blend::AlphaBlend;
    match surface.style {
        0 => { sub.specular = [0.505f32.powf(2.2); 3]; sub.shininess = 10.0; }
        2 => sub.emissive = [1.0; 3], // native LIGHTING off: texture × authored vertex colour
        4 => sub.emissive = [0.2f32, 0.3, 0.4].map(|c| c.powf(2.2)),
        _ => unreachable!("native_mesh only handles styles 0/2/4"),
    }
    if surface.style != 4 {
        let base = mesh.vertices.len() as u32;
        mesh.vertices.extend_from_within(..);
        let mut ripple = sub.clone();
        ripple.indices.iter_mut().for_each(|i| *i += base);
        mesh.submeshes.extend([sub, ripple]);
    } else {
        mesh.submeshes.push(sub);
    }
    Ok(mesh)
}

/// `tex` resolves a texture name to a scene texture.
fn legacy_mesh<'a>(waters: impl IntoIterator<Item = &'a Water>, tex: &mut dyn FnMut(&str) -> Option<TextureKey>) -> Option<Mesh> {
    let mut mesh = Mesh::default();
    let mut crust: Vec<u32> = Vec::new();
    let mut by_kind: Vec<(u32, Vec<u32>)> = Vec::new();
    for w in waters.into_iter().filter(|w| !w.tris.is_empty()) {
        let key = (w.kind >> 1).min(10);
        let Some(l) = legacy_liquid(w.kind) else { continue };
        let base = mesh.vertices.len() as u32;
        mesh.vertices.extend(w.verts.iter().map(|v| Vertex { pos: [v[0], v[1], -v[2]], uv: [v[0] / l.scale, v[2] / l.scale], ..Default::default() }));
        let idx = by_kind.iter().position(|(k, _)| *k == key).unwrap_or_else(|| {
            by_kind.push((key, Vec::new()));
            by_kind.len() - 1
        });
        by_kind[idx].1.extend(w.tris.iter().flatten().map(|&i| base + i as u32));
        if key == 1 {
            // the crust layer has its own uv scale: a second copy of the vertices
            let cbase = mesh.vertices.len() as u32;
            mesh.vertices.extend(w.verts.iter().map(|v| Vertex { pos: [v[0], v[1], -v[2]], uv: [v[0] / CRUST_SCALE, v[2] / CRUST_SCALE], ..Default::default() }));
            crust.extend(w.tris.iter().flatten().map(|&i| cbase + i as u32));
        }
    }
    if mesh.vertices.is_empty() {
        return None;
    }
    for (key, indices) in by_kind {
        let l = legacy_liquid(key << 1).expect("kind drawn above");
        let mut s = Submesh::new(indices, tex(l.texture));
        s.two_sided = true;
        s.liquid = true; // `VisualLiquid_t` render list 4
        let c = argb_to_linear(l.argb);
        match key {
            // lava: opaque, unlit red water5 + the crust; `prelit` = unlit in the renderer (vertex colour is white)
            1 => {
                s.prelit = true;
                s.base_color = [c[0], c[1], c[2], 1.0];
                mesh.submeshes.push(s);
                let mut crust = Submesh::new(std::mem::take(&mut crust), tex(CRUST_TEX));
                crust.blend = Blend::AlphaBlend;
                crust.two_sided = true;
                crust.liquid = true;
                crust.prelit = true;
                crust.base_color = [1.0, 1.0, 1.0, CRUST_ALPHA];
                mesh.submeshes.push(crust);
                continue;
            }
            // star water: the client projects a star field and mist layers with `ONE, INVSRCCOLOR` [not decoded in detail]
            7 => s.blend = Blend::Additive,
            _ => unreachable!("legacy_liquid only emits lava or star water"),
        }
        mesh.submeshes.push(s);
    }
    Some(mesh)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Vec<u8> {
        let mut b = Vec::new();
        b.extend(1u32.to_le_bytes()); // one liquid
        b.extend(2u32.to_le_bytes()); // kind 2: lava
        b.extend(3u32.to_le_bytes());
        for v in [[0.0f32, 5.0, 0.0], [10.0, 5.0, 0.0], [0.0, 5.0, 10.0]] {
            for c in v {
                b.extend(c.to_le_bytes());
            }
        }
        b.extend(1u32.to_le_bytes());
        for i in [0u16, 1, 2] {
            b.extend(i.to_le_bytes());
        }
        b
    }

    #[test]
    fn parses_liquid_and_builds_mesh() {
        let d = fixture();
        let mut r = Rd::new(&d, 0);
        let w = parse(&mut r).unwrap();
        assert_eq!(r.o, d.len());
        assert_eq!(w, vec![Water { kind: 2, verts: vec![[0.0, 5.0, 0.0], [10.0, 5.0, 0.0], [0.0, 5.0, 10.0]], tris: vec![[0, 1, 2]] }]);
        let m = legacy_mesh(w.iter(), &mut |n| Some(TextureKey { rdb_type: TEXTURES, id: n.len() as u32 })).unwrap();
        assert_eq!(m.vertices.len(), 6); // 3 lava vertices + 3 crust copies
        assert_eq!(m.vertices[2].pos, [0.0, 5.0, -10.0]); // z negated
        assert_eq!(m.vertices[2].uv, [0.0, 10.0 / 8.0]); // z metres / lava scale
        assert_eq!(m.vertices[5].uv, [0.0, 10.0 / 75.0]); // crust scale
        assert_eq!(m.submeshes.len(), 2);
        assert_eq!(m.submeshes[0].blend, Blend::Opaque);
        assert_eq!(m.submeshes[0].base_color, [1.0, 0.0, 0.0, 1.0]); // lava = red
        assert_eq!(m.submeshes[1].blend, Blend::AlphaBlend);
        assert!(m.submeshes.iter().all(|s| s.liquid), "every liquid submesh is in render list 4");
        assert_eq!(m.submeshes[1].indices, vec![3, 4, 5]);
    }

    #[test]
    fn authored_water_style_has_two_native_layers() {
        let Some(dir) = std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join("Games/ProjectRubiKa/client")).filter(|dir| dir.join("cd_image/rdb.db").exists()) else { return };
        let store = RecordStore::open(&dir).unwrap();
        let data = store.get(super::super::RECORD, 566).unwrap().unwrap();
        let record = super::super::record::parse(&data).unwrap();
        let waters = parse(&mut Rd::new(&data, record.tail)).unwrap();
        let mut scene = Scene::default();
        emit(&store, &waters, &mut scene).unwrap();
        let surface = scene.liquids.iter().find(|surface| surface.style == 0).expect("Newland authored water");
        let mesh = &scene.meshes[surface.mesh];
        assert_eq!(mesh.submeshes.len(), 2, "base and inverse-alpha ripple, not one approximate pass");
        let count = mesh.vertices.len() / 2;
        assert_eq!(mesh.submeshes[1].indices, mesh.submeshes[0].indices.iter().map(|i| i + count as u32).collect::<Vec<_>>());
        for sub in &mesh.submeshes {
            assert_eq!(sub.base_color, [1.0; 4], "packed source colour must not be tinted twice");
            assert_eq!(sub.texture, Some(TextureKey { rdb_type: TEXTURES, id: 6329 }));
            assert_eq!(sub.uv_scroll, [0.0; 2], "native UVs are animated by the liquid stage");
            assert_eq!(sub.shininess, 10.0);
            assert_eq!(sub.specular, [0.505f32.powf(2.2); 3]);
            assert!(sub.liquid && sub.two_sided);
        }
        assert!(mesh.vertices[..count].iter().all(|v| v.normal[1] >= 0.0));
        assert!(mesh.vertices[..count].iter().any(|v| v.color[3] > 0.0 && v.color[3] < 1.0));
        assert!(scene.liquids.iter().all(|surface| matches!(surface.style, 0 | 2 | 4)), "RealisticWater never owns lava/star geometry");
    }

    #[test]
    fn native_default_kinds_have_water_material_and_two_layers() {
        use ao_scene::liquid::{SourceTriangle, Surface};
        for kind in [338, 960] {
            let source = SourceTriangle { kind, vertices: [[0.0,0.0,0.0],[80.0,0.0,0.0],[0.0,0.0,-80.0]].map(|pos| Vertex {pos,uv:[pos[0]/40.0,-pos[2]/40.0],..Default::default()}) };
            let surface = Surface::new(0, native_style(kind).unwrap(), vec![source]).unwrap();
            let key = TextureKey { rdb_type: TEXTURES, id: 6329 };
            let mesh = native_mesh(&surface, surface.initial_camera().unwrap(), &mut |_| Some(key)).unwrap();
            assert_eq!(mesh.submeshes.len(), 2, "native default kind {kind} must not use the legacy single-pass stage");
            let count = mesh.vertices.len() / 2;
            assert_eq!(mesh.submeshes[1].indices, mesh.submeshes[0].indices.iter().map(|index| index + count as u32).collect::<Vec<_>>());
            for sub in &mesh.submeshes {
                assert_eq!(sub.texture, Some(key));
                assert_eq!(sub.shininess, 10.0);
                assert_eq!(sub.specular, [0.505f32.powf(2.2); 3]);
                assert!(sub.liquid && sub.two_sided);
            }
        }
    }

    #[test]
    fn kinds_follow_create_switch() {
        assert_eq!(argb_to_linear(0xcc00_186a)[3], 0.8); // water: alpha 0xcc
        assert!(native_style(16).is_none() && native_style(17).is_none());
        assert_eq!(native_style(1), native_style(0));
        assert_eq!(native_style(7), Some(2));
        assert_eq!(native_style(338), Some(0), "native default style consumes RealisticWater despite allocation-dependent colour");
        assert_eq!(native_style(960), Some(0));
    }

    #[test]
    fn rejects_bad_triangle_index() {
        let mut d = fixture();
        let n = d.len();
        d[n - 2] = 9; // last index 9 >= 3 vertices
        assert!(parse(&mut Rd::new(&d, 0)).is_err());
    }
}
