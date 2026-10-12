//! Gamecode Life_c catalog reader 0x1011ac84; SQLite payload excludes the DB object's three i32 header words.
use anyhow::{ensure, Context, Result};
use ao_rdb::RecordStore;

pub const CATALOG_TYPE: u32 = 1_010_025;
pub const MESH_TYPE: u32 = 1_010_001;

/// Native dispatcher 0x1011a3c9, in numeric order (not inferred from asset names).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum LifeKind { Bird, Rabbit, Butterfly, Fly, Beetle, TumbleWeed, Fish, Hoverbot, Trash, Cleanerbot, Pollen }
impl LifeKind {
    pub fn from_native(value: u16) -> Option<Self> {
        Some(match value { 0 => Self::Bird, 1 => Self::Rabbit, 2 => Self::Butterfly, 3 => Self::Fly, 4 => Self::Beetle, 5 => Self::TumbleWeed, 6 => Self::Fish, 7 => Self::Hoverbot, 8 => Self::Trash, 9 => Self::Cleanerbot, 10 => Self::Pollen, _ => return None })
    }
    pub fn supports_groups(self) -> bool { matches!(self, Self::Bird | Self::Fly | Self::Fish) }
}
#[derive(Clone, Debug, PartialEq)]
pub struct LifeAsset { pub name: String, pub kind: u16, pub mesh: u32, pub group_chance: f32 }

pub fn load_catalog(store: &RecordStore) -> Result<Vec<LifeAsset>> {
    let Some(id) = store.ids(CATALOG_TYPE)?.first().copied() else { return Ok(Vec::new()) };
    parse_catalog(&store.get(CATALOG_TYPE, id)?.context("missing wildlife catalog")?)
}

pub fn parse_catalog(data: &[u8]) -> Result<Vec<LifeAsset>> {
    let mut p = 0usize;
    let mut take = |n: usize| -> Result<&[u8]> {
        let end = p.checked_add(n).context("wildlife catalog offset overflow")?;
        let bytes = data.get(p..end).context("truncated wildlife catalog")?;
        p = end;
        Ok(bytes)
    };
    let count = u16::from_le_bytes(take(2)?.try_into()?) as usize;
    let mut assets = Vec::with_capacity(count.min(data.len() / 11));
    for _ in 0..count {
        // BinaryStream::operator>>(char*) reads through NUL; native destination is char[32].
        let mut name = Vec::new();
        loop {
            let byte = take(1)?[0];
            if byte == 0 { break; }
            ensure!(name.len() < 31, "wildlife catalog name exceeds native buffer");
            name.push(byte);
        }
        let kind = u16::from_le_bytes(take(2)?.try_into()?);
        ensure!(LifeKind::from_native(kind).is_some(), "unsupported native wildlife kind {kind}");
        let mesh = u32::from_le_bytes(take(4)?.try_into()?);
        let group_chance = f32::from_le_bytes(take(4)?.try_into()?);
        ensure!(group_chance.is_finite(), "non-finite wildlife group chance");
        assets.push(LifeAsset { name: String::from_utf8(name).context("invalid wildlife name")?, kind, mesh, group_chance });
    }
    Ok(assets)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn retail_catalog_and_native_kinds() {
        let client = std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join("Games/ProjectRubiKa/client");
        if !client.join("cd_image/rdb.db").exists() { return; }
        let store = RecordStore::open(&client).unwrap();
        let data = store.get(CATALOG_TYPE, 36258).unwrap().unwrap();
        let assets = parse_catalog(&data).unwrap();
        assert_eq!(assets.len(), 16);
        let expected = [
            ("Beetle", 4, 36085), ("Bird", 0, 36079), ("Bunny", 1, 36081),
            ("Butterfly", 2, 36083), ("Cockroach", 4, 36077), ("Dragonfly", 2, 36087),
            ("Fish", 6, 36089), ("Fly", 3, 36091), ("Frog", 1, 36093), ("Moth", 2, 36095),
            ("Garbagedrone", 9, 75382), ("Hoverdrone", 0, 75384), ("Minidrone", 9, 75386),
            ("Surveillancedrone", 7, 75390), ("Sweeperdrone", 9, 75392), ("Pollen", 10, 36083),
        ];
        for (asset, expected) in assets.iter().zip(expected) {
            assert_eq!((asset.name.as_str(), asset.kind, asset.mesh), expected);
        }
        assert!(parse_catalog(&data[..data.len() - 23]).is_err());
        for i in 0..11 { assert_eq!(LifeKind::from_native(i).unwrap() as u16, i); }
        assert!(LifeKind::from_native(11).is_none());
    }
}
