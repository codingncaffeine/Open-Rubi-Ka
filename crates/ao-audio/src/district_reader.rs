//! GameData 100049be / 1000640f / 10009def. Walk every variable list; never search for headers.
use anyhow::{ensure, Context, Result};
use crate::district::{District, Districts};

struct Reader<'a> { data: &'a [u8], pos: usize }
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(n).context("district offset overflow")?;
        let b = self.data.get(self.pos..end).context("truncated district stream")?;
        self.pos = end;
        Ok(b)
    }
    fn byte(&mut self) -> Result<u8> { Ok(self.take(1)?[0]) }
    fn short(&mut self) -> Result<u16> { Ok(u16::from_le_bytes(self.take(2)?.try_into()?)) }
    fn int(&mut self) -> Result<u32> { Ok(u32::from_le_bytes(self.take(4)?.try_into()?)) }
    fn float(&mut self) -> Result<f32> { let f = f32::from_bits(self.int()?); ensure!(f.is_finite(), "non-finite district coordinate"); Ok(f) }
    fn string(&mut self) -> Result<String> {
        let n = self.short()? as usize;
        ensure!(n < 0x8000, "invalid district string length");
        Ok(String::from_utf8(self.take(n)?.to_vec()).context("invalid district string")?)
    }
    fn spawn(&mut self, rotation: bool) -> Result<()> {
        let x = self.float()?; let y = self.float()?; let z = self.float()?; let radius = self.float()?;
        // Native GD 0x1000a464 requires x,z > 0, but real records (e.g. pf 111 "District" hash "CLLN", pf 953's
        // first plain spawn point) carry an origin-centred (0, y, 0) point with otherwise well-formed metadata.
        ensure!(((x > 0.0 && z > 0.0) || (x == 0.0 && z == 0.0)) && y >= 0.0 && radius >= 0.0, "invalid district spawn point");
        if rotation {
            // Native GD 0x1000a29a (RotationSpawnPoint_t): angle < 360, angle width <= 360.
            ensure!(self.short()? < 360 && self.short()? <= 360, "invalid spawn rotation");
        }
        Ok(())
    }
    fn criteria(&mut self, sized: bool) -> Result<()> {
        let word = self.int()?;
        let n = if sized { ensure!(word >= 1009 && word % 1009 == 0, "invalid spell criteria count"); word / 1009 - 1 } else { word };
        ensure!(n <= 1000, "too many spell criteria");
        for _ in 0..n { self.take(8)?; ensure!(self.int()? < 0x91, "invalid spell criterion operator"); }
        Ok(())
    }
    fn spell(&mut self) -> Result<()> {
        let function = self.int()?;
        self.int()?;
        ensure!(self.int()? == ao_net::n3::spells::FORMAT_VERSION as u32, "invalid spell format version");
        self.criteria(false)?;
        self.take(16)?; // four native standard arguments
        for &(kind, _) in ao_net::n3::spells::format_of(function) {
            if kind == 1 { let n = self.int()? as usize; self.take(n)?; } else { self.int()?; }
        }
        if function == 0xcf2f {
            self.int()?;
            let marker = self.int()?;
            if marker >= 100 { self.take(16)?; } else { self.int()?; }
        }
        if function == 0xcf17 { self.criteria(true)?; }
        ensure!(function != 0xcf20, "district spell expression stream is unsupported");
        Ok(())
    }
    fn hash_spawn(&mut self, ver: u16) -> Result<()> {
        self.spawn(true)?;
        let hash = self.int()?;
        let min = self.short()?; let max = self.short()?; let chance = self.byte()?;
        let flags = self.byte()?;
        self.short()?; // respawn seconds
        if ver >= 7 { self.int()?; }
        // Native GD 0x1000640f: hash nonzero, min/max level in 1..=1000, chance <= 100.
        ensure!(hash != 0 && (1..=1000).contains(&min) && (1..=1000).contains(&max) && chance <= 100, "invalid hash spawn metadata");
        if flags & 1 != 0 { self.take(4)?; }
        if flags & 2 != 0 { let n = self.byte()?; for _ in 0..n { self.spawn(true)?; } }
        if flags & 4 != 0 {
            let n = self.int()?; ensure!(n <= 100, "too many hash spawn lists");
            for _ in 0..n {
                let ty = self.int()?; let id = self.int()?; let word = self.int()?;
                ensure!(word >= 1009 && word % 1009 == 0, "invalid hash spawn list count");
                let count = word / 1009 - 1;
                if ty == 2 {
                    ensure!(id <= 28 && (1..=100).contains(&count), "invalid hash spawn spell list");
                    for _ in 0..count { self.spell()?; }
                } else {
                    ensure!(ty == 32 && id == 55 && self.int()? == 1, "invalid hash spawn tag list");
                    ensure!(count as usize <= self.data.len().saturating_sub(self.pos) / 10, "tag list exceeds stream");
                    for _ in 0..count { ensure!(self.int()? == 1, "invalid hash spawn tag version"); self.string()?; self.int()?; }
                }
            }
        }
        Ok(())
    }
}

pub(crate) fn parse(data: &[u8]) -> Result<Districts> {
    let mut r = Reader { data, pos: 0 };
    let ver = r.short()?;
    ensure!((5..=7).contains(&ver), "unsupported district version {ver}");
    let nz = r.int()? as usize; let nd = r.byte()? as usize;
    // A playfield may legitimately define zero zones/districts (e.g. unused system/admin playfields).
    ensure!(nz <= data.len(), "invalid district counts");
    let mut districts = Vec::with_capacity(nd);
    for _ in 0..nd {
        let centre = [r.float()?, r.float()?, r.float()?];
        ensure!(centre.iter().all(|&f| f >= 0.0), "negative district centre");
        let name = if ver >= 6 { r.string()? } else { let b = r.take(32)?; String::from_utf8(b[..b.iter().position(|&c| c == 0).unwrap_or(32)].to_vec()).context("invalid legacy district name")? };
        ensure!(!name.is_empty() && name.len() <= 64, "invalid district name");
        let sound_id = r.short()?;
        let mut music = [0; 8]; for id in &mut music { *id = r.short()?; }
        let (npc_lvl, lc_lvl) = if ver >= 6 { ((r.short()?, r.short()?), (r.short()?, r.short()?)) } else { r.take(4)?; ((r.byte()? as u16, r.byte()? as u16), (0, 0)) };
        // Native GD 0x49be: each pair independently has min <= max <= 1024; min/max are not required to be zero together.
        ensure!([npc_lvl, lc_lvl].iter().all(|&(a, b)| a <= b && b <= 1024), "invalid district level ranges");
        let respawn_pct = r.byte()?; let respawn_time = r.int()? as i32;
        ensure!(respawn_pct <= 100 && respawn_time >= -1, "invalid district respawn metadata");
        let fight_mode = r.byte()?; ensure!(fight_mode < 5, "invalid district fight mode");
        let hashes = r.byte()? as usize;
        let legacy = if ver < 6 { r.byte()? as usize } else { 0 };
        let nw = r.byte()?; let points = r.byte()?; let hash_points = r.byte()?;
        r.take((hashes + legacy) * 6)?; // ACGHash i32 + count u16
        let mut wildlife = Vec::with_capacity(nw as usize);
        for _ in 0..nw { wildlife.push((r.short()?, r.short()?)); }
        for _ in 0..points { r.spawn(false)?; }
        for _ in 0..hash_points { r.hash_spawn(ver)?; }
        districts.push(District { name, sound_id, music, npc_lvl, lc_lvl, centre, fight_mode, wildlife });
    }
    let zone_to_district = r.take(nz)?.to_vec();
    ensure!(zone_to_district.iter().all(|&i| (i as usize) < nd), "district zone index out of range");
    // ResourceDB conversion leaves a 12-byte zero suffix after the native stream.
    let remaining = &data[r.pos..];
    ensure!(remaining.is_empty() || (remaining.len() == 12 && remaining.iter().all(|&b| b == 0)), "unexpected district suffix");
    Ok(Districts { zone_to_district, districts })
}
