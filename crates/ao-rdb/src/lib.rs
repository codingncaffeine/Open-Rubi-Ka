//! Read access to the PRK client's resource database (`cd_image/rdb.db`).
//!
//! PRK ships the Funcom ResourceDatabase converted to SQLite: one table per record type,
//! named `rdb_<type>`, schema `(id INTEGER PRIMARY KEY, version INTEGER, data BLOB)`.

use std::path::Path;
use std::cell::Cell;

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags, OptionalExtension};

pub struct RecordStore {
    conn: Connection,
    other_texture_type: Cell<u32>,
    ground_texture_type: Cell<u32>,
}

impl RecordStore {
    /// Opens `<client_dir>/cd_image/rdb.db` read-only.
    pub fn open(client_dir: &Path) -> Result<Self> {
        let path = client_dir.join("cd_image").join("rdb.db");
        let conn = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .with_context(|| format!("opening {}", path.display()))?;
        Ok(Self { conn, other_texture_type: Cell::new(1_010_004), ground_texture_type: Cell::new(1_010_006) })
    }

    /// DisplaySystem 0x10076a24: ordinary textures select a resource family, not a resize.
    pub fn set_other_texture_quality(&self, quality: i32) -> bool {
        let ty = other_texture_type(quality);
        self.other_texture_type.replace(ty) != ty
    }

    pub fn other_texture_quality(&self) -> i32 {
        match self.other_texture_type.get() { 1_010_017 => 1, 1_010_016 => 2, _ => 3 }
    }

    pub fn other_texture_type(&self) -> u32 { self.other_texture_type.get() }

    /// Native ground resource families: low, medium, high.
    pub fn set_ground_texture_quality(&self, quality: i32) -> bool {
        let ty = match quality { 0 => 1_010_022, 1 => 1_010_021, _ => 1_010_006 };
        self.ground_texture_type.replace(ty) != ty
    }

    fn texture_type(&self, ty: u32) -> u32 {
        match ty {
            1_010_004 => self.other_texture_type.get(),
            1_010_006 => self.ground_texture_type.get(),
            _ => ty,
        }
    }

    /// Record types present in the database, ascending.
    pub fn types(&self) -> Result<Vec<u32>> {
        let mut stmt = self
            .conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name LIKE 'rdb_%'")?;
        let mut types = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .filter_map(|n| n.ok()?.strip_prefix("rdb_")?.parse().ok())
            .collect::<Vec<u32>>();
        types.sort_unstable();
        Ok(types)
    }

    /// Record ids of `rdb_type`, ascending. Empty if the type has no table.
    pub fn ids(&self, rdb_type: u32) -> Result<Vec<u32>> {
        if !self.has_type(rdb_type)? {
            return Ok(Vec::new());
        }
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT id FROM rdb_{rdb_type} ORDER BY id"))?;
        let ids = stmt
            .query_map([], |r| r.get::<_, u32>(0))?
            .collect::<rusqlite::Result<Vec<u32>>>()?;
        Ok(ids)
    }

    /// Raw record payload, or `None` if the type or id is absent.
    pub fn get(&self, rdb_type: u32, id: u32) -> Result<Option<Vec<u8>>> {
        let rdb_type = self.texture_type(rdb_type);
        if !self.has_type(rdb_type)? {
            return Ok(None);
        }
        self.conn
            .query_row(
                &format!("SELECT data FROM rdb_{rdb_type} WHERE id = ?1"),
                [id],
                |r| r.get::<_, Vec<u8>>(0),
            )
            .optional()
            .with_context(|| format!("reading rdb record {rdb_type}:{id}"))
    }

    /// Like [`RecordStore::get`] with the record's `version` column (the data format selector of the DbObject).
    pub fn get_versioned(&self, rdb_type: u32, id: u32) -> Result<Option<(u32, Vec<u8>)>> {
        let rdb_type = self.texture_type(rdb_type);
        if !self.has_type(rdb_type)? {
            return Ok(None);
        }
        self.conn
            .query_row(&format!("SELECT version, data FROM rdb_{rdb_type} WHERE id = ?1"), [id], |r| Ok((r.get::<_, u32>(0)?, r.get::<_, Vec<u8>>(1)?)))
            .optional()
            .with_context(|| format!("reading rdb record {rdb_type}:{id}"))
    }

    fn has_type(&self, rdb_type: u32) -> Result<bool> {
        Ok(self
            .conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name = ?1",
                [format!("rdb_{rdb_type}")],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }
}

fn other_texture_type(quality: i32) -> u32 {
    match quality {
        1 => 1_010_017,
        2 => 1_010_016,
        _ => 1_010_004,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn native_other_texture_quality_selects_asset_family() {
        assert_eq!(super::other_texture_type(0), 1_010_004);
        assert_eq!(super::other_texture_type(1), 1_010_017);
        assert_eq!(super::other_texture_type(2), 1_010_016);
        assert_eq!(super::other_texture_type(-1), 1_010_004);
        assert_eq!(super::other_texture_type(3), 1_010_004);
    }

    #[test]
    fn texture_preferences_route_real_record_reads_without_changing_other_types() {
        use super::*;
        let conn = Connection::open_in_memory().unwrap();
        for ty in [1_010_004u32, 1_010_017, 1_010_016, 1_010_006, 1_010_021, 1_010_022, 1_010_011] {
            conn.execute_batch(&format!("CREATE TABLE rdb_{ty} (id INTEGER PRIMARY KEY, version INTEGER, data BLOB);")).unwrap();
            conn.execute(&format!("INSERT INTO rdb_{ty} VALUES (7, ?1, ?2)"), rusqlite::params![ty, ty.to_le_bytes().to_vec()]).unwrap();
        }
        let store = RecordStore { conn, other_texture_type: Cell::new(1_010_004), ground_texture_type: Cell::new(1_010_006) };
        for (quality, ty) in [(1, 1_010_017u32), (2, 1_010_016), (3, 1_010_004)] {
            store.set_other_texture_quality(quality);
            assert_eq!(store.get(1_010_004, 7).unwrap().unwrap(), ty.to_le_bytes());
            assert_eq!(store.get_versioned(1_010_004, 7).unwrap().unwrap().0, ty);
            assert_eq!(store.get_versioned(1_010_011, 7).unwrap().unwrap().0, 1_010_011);
        }
        for (quality, ty) in [(0, 1_010_022), (1, 1_010_021), (2, 1_010_006)] {
            store.set_ground_texture_quality(quality);
            assert_eq!(store.get_versioned(1_010_006, 7).unwrap().unwrap().0, ty);
        }
    }
}
