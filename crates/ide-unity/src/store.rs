//! Persistence of the Unity index in its own redb file, apart from the code index: a change to the
//! code index's grammars doesn't discard it, and it can be rebuilt without touching the other.

use crate::scan::{Guid, ScannedFile};
use crate::walk::Stamp;
use parking_lot::RwLock;
#[cfg(test)]
use redb::ReadableTableMetadata;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition, TableError};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// Asset path → [`Record`].
pub(crate) const FILES: TableDefinition<&str, &[u8]> = TableDefinition::new("files");
/// Asset path → its [`ScannedFile`], for scanned assets.
const SCANS: TableDefinition<&str, &[u8]> = TableDefinition::new("scans");
const META: TableDefinition<&str, &[u8]> = TableDefinition::new("meta");
/// Bump when a stored record or its encoding changes.
const SCHEMA: &str = "prefabric-unity-4";

type Result<T> = std::result::Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

/// What was indexed of an asset, to skip it while neither it nor its `.meta` changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Record {
    pub asset: Stamp,
    pub meta: Stamp,
    pub guid: Option<Guid>,
    pub scanned: bool,
    /// Hash of a scanned asset's content: a save that changes nothing isn't scanned again.
    pub hash: u64,
    /// Why a serialized asset isn't scanned.
    pub problem: crate::problems::Problem,
}

/// Pages kept in memory: redb's default (1 GiB) would hold most of a large project's index.
const CACHE_BYTES: usize = 32 << 20;

fn create(file: &Path) -> std::result::Result<Database, redb::DatabaseError> {
    Database::builder().set_cache_size(CACHE_BYTES).create(file)
}

pub(crate) struct Store {
    db: RwLock<Database>,
}

impl Store {
    /// Opens (or creates) `dir/unity.redb`. Records of another schema or root are discarded.
    /// `None` when the database can't be used (another window has it open): the index then
    /// lives in memory only.
    pub(crate) fn open(dir: &Path, root: &Path) -> Option<Self> {
        if let Err(e) = std::fs::create_dir_all(dir) {
            tracing::warn!("unity index {}: {e}", dir.display());
            return None;
        }
        let file = dir.join("unity.redb");
        let db = match create(&file) {
            Ok(db) => db,
            Err(redb::DatabaseError::DatabaseAlreadyOpen) => {
                tracing::warn!("unity index {} is in use; indexing in memory only", file.display());
                return None;
            }
            Err(e) => {
                tracing::warn!("unity index {} unreadable ({e}); rebuilding", file.display());
                let _ = std::fs::remove_file(&file);
                create(&file).map_err(|e| tracing::warn!("unity index: {e}")).ok()?
            }
        };
        let store = Self { db: RwLock::new(db) };
        let stamp = format!("{SCHEMA}\n{}", root.display());
        match store.meta("stamp") {
            Ok(Some(v)) if v == stamp.as_bytes() => {}
            _ => {
                if let Err(e) = store.reset(&stamp) {
                    tracing::warn!("unity index reset failed: {e}");
                    return None;
                }
            }
        }
        Some(store)
    }

    fn meta(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let txn = self.db.read().begin_read().map_err(err)?;
        let table = match txn.open_table(META) {
            Ok(t) => t,
            Err(TableError::TableDoesNotExist(_)) => return Ok(None),
            Err(e) => return Err(err(e)),
        };
        Ok(table.get(key).map_err(err)?.map(|v| v.value().to_vec()))
    }

    fn reset(&self, stamp: &str) -> Result<()> {
        let txn = self.db.read().begin_write().map_err(err)?;
        for table in [FILES, SCANS] {
            match txn.delete_table(table) {
                Ok(_) | Err(TableError::TableDoesNotExist(_)) => {}
                Err(e) => return Err(err(e)),
            }
        }
        {
            let mut meta = txn.open_table(META).map_err(err)?;
            meta.insert("stamp", stamp.as_bytes()).map_err(err)?;
            txn.open_table(FILES).map_err(err)?;
            txn.open_table(SCANS).map_err(err)?;
        }
        txn.commit().map_err(err)
    }

    /// Every stored record.
    pub(crate) fn records(&self) -> Result<HashMap<String, Record>> {
        let txn = self.db.read().begin_read().map_err(err)?;
        let table = txn.open_table(FILES).map_err(err)?;
        let mut out = HashMap::new();
        for entry in table.iter().map_err(err)? {
            let (k, v) = entry.map_err(err)?;
            if let Ok(record) = postcard::from_bytes(v.value()) {
                out.insert(k.value().to_owned(), record);
            }
        }
        Ok(out)
    }

    pub(crate) fn scan(&self, rel: &str) -> Result<Option<ScannedFile>> {
        let txn = self.db.read().begin_read().map_err(err)?;
        let table = txn.open_table(SCANS).map_err(err)?;
        Ok(table.get(rel).map_err(err)?.and_then(|v| postcard::from_bytes(v.value()).ok()))
    }

    /// A flag kept with the index (a notice already shown).
    pub(crate) fn flag(&self, key: &str) -> bool {
        self.meta(key).ok().flatten().is_some()
    }

    pub(crate) fn set_flag(&self, key: &str) -> Result<()> {
        let txn = self.db.read().begin_write().map_err(err)?;
        txn.open_table(META).map_err(err)?.insert(key, &b"1"[..]).map_err(err)?;
        txn.commit().map_err(err)
    }

    pub(crate) fn write_record(&self, rel: &str, record: Record) -> Result<()> {
        let txn = self.db.read().begin_write().map_err(err)?;
        {
            let bytes = postcard::to_allocvec(&record).map_err(err)?;
            txn.open_table(FILES).map_err(err)?.insert(rel, bytes.as_slice()).map_err(err)?;
        }
        txn.commit().map_err(err)
    }

    /// Stores `put` (a scan of `None` drops the stored one) and forgets `removed`, in one transaction.
    pub(crate) fn write(&self, put: &[(&str, Record, Option<&ScannedFile>)], removed: &[&str]) -> Result<()> {
        let txn = self.db.read().begin_write().map_err(err)?;
        {
            let mut files = txn.open_table(FILES).map_err(err)?;
            let mut scans = txn.open_table(SCANS).map_err(err)?;
            let mut buf = Vec::new();
            for (rel, record, scan) in put {
                buf = postcard::to_extend(record, std::mem::take(&mut buf)).map_err(err)?;
                files.insert(*rel, buf.as_slice()).map_err(err)?;
                buf.clear();
                match scan {
                    Some(scan) => {
                        buf = postcard::to_extend(*scan, std::mem::take(&mut buf)).map_err(err)?;
                        scans.insert(*rel, buf.as_slice()).map_err(err)?;
                        buf.clear();
                    }
                    None => {
                        scans.remove(*rel).map_err(err)?;
                    }
                }
            }
            for rel in removed {
                files.remove(*rel).map_err(err)?;
                scans.remove(*rel).map_err(err)?;
            }
        }
        txn.commit().map_err(err)
    }

    /// Number of stored file records.
    #[cfg(test)]
    pub(crate) fn file_count(&self) -> Result<u64> {
        let txn = self.db.read().begin_read().map_err(err)?;
        match txn.open_table(FILES) {
            Ok(t) => t.len().map_err(err),
            Err(TableError::TableDoesNotExist(_)) => Ok(0),
            Err(e) => Err(err(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reopens_and_discards_another_root() {
        let dir = tempfile::tempdir().unwrap();
        {
            let store = Store::open(dir.path(), Path::new("/a")).unwrap();
            let txn = store.db.read().begin_write().unwrap();
            txn.open_table(FILES).unwrap().insert("Assets/A.prefab", &b"x"[..]).unwrap();
            txn.commit().unwrap();
            assert_eq!(store.file_count().unwrap(), 1);
        }
        assert_eq!(Store::open(dir.path(), Path::new("/a")).unwrap().file_count().unwrap(), 1);
        assert_eq!(Store::open(dir.path(), Path::new("/b")).unwrap().file_count().unwrap(), 0);
    }

    #[test]
    fn rebuilds_a_damaged_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("unity.redb"), b"not a database").unwrap();
        assert_eq!(Store::open(dir.path(), Path::new("/a")).unwrap().file_count().unwrap(), 0);
    }
}
