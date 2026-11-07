//! Persistence of the Unity index in its own redb file, apart from the code index: a change to the
//! code index's grammars doesn't discard it, and it can be rebuilt without touching the other.

use parking_lot::RwLock;
use redb::{Database, ReadableDatabase, ReadableTableMetadata, TableDefinition, TableError};
use std::path::Path;

pub(crate) const FILES: TableDefinition<&str, &[u8]> = TableDefinition::new("files");
const META: TableDefinition<&str, &[u8]> = TableDefinition::new("meta");
/// Bump when a stored record or its encoding changes.
const SCHEMA: &str = "prefabric-unity-1";

type Result<T> = std::result::Result<T, String>;

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
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
        let db = match Database::create(&file) {
            Ok(db) => db,
            Err(redb::DatabaseError::DatabaseAlreadyOpen) => {
                tracing::warn!("unity index {} is in use; indexing in memory only", file.display());
                return None;
            }
            Err(e) => {
                tracing::warn!("unity index {} unreadable ({e}); rebuilding", file.display());
                let _ = std::fs::remove_file(&file);
                Database::create(&file).map_err(|e| tracing::warn!("unity index: {e}")).ok()?
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
        match txn.delete_table(FILES) {
            Ok(_) | Err(TableError::TableDoesNotExist(_)) => {}
            Err(e) => return Err(err(e)),
        }
        {
            let mut meta = txn.open_table(META).map_err(err)?;
            meta.insert("stamp", stamp.as_bytes()).map_err(err)?;
            txn.open_table(FILES).map_err(err)?;
        }
        txn.commit().map_err(err)
    }

    /// Number of stored file records.
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
