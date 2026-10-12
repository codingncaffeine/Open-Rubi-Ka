//! Native Life placement geometry (Gamecode 1011a72c / 1011aece / 1011b49c).
use anyhow::{Context, Result};
use ao_rdb::RecordStore;
use super::{ground, record, water, RECORD, TILEMAP};

pub struct WildlifeGeometry {
    pub zones_x: usize,
    pub zones_z: usize,
    pub zone_edge: f32,
    /// Native AO-space liquid triangles in authored order, n3WaterData_t's first three vectors.
    pub water_triangles: Vec<([[f32; 3]; 3], u32)>,
}

pub fn wildlife_geometry(store: &RecordStore, id: u32) -> Result<WildlifeGeometry> {
    let raw = store.get(RECORD, id)?.context("missing wildlife playfield resource")?;
    let rec = record::parse(&raw)?;
    let mut tail = record::Rd::new(&raw, rec.tail);
    let waters = water::parse(&mut tail)?;
    let water_triangles = waters.iter().flat_map(|w| w.tris.iter().map(|t| (t.map(|i| w.verts[i as usize]), w.kind))).collect();
    let (zones_x, zones_z, zone_edge) = if rec.is_outdoor() {
        let bytes = store.get(TILEMAP, rec.tilemap)?.context("missing wildlife terrain")?;
        let tm = ground::parse(&bytes)?;
        anyhow::ensure!(rec.zone_size > 0, "wildlife zone size is zero");
        (tm.cells_x / rec.zone_size as usize, tm.cells_z / rec.zone_size as usize, tm.cell_size * rec.zone_size as f32)
    } else {
        // Native room playfields have no outdoor zone grid from which Life selects positions.
        (0, 0, 0.0)
    };
    Ok(WildlifeGeometry { zones_x, zones_z, zone_edge, water_triangles })
}
