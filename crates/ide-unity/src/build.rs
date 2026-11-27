//! A full scan of the project: every asset's GUID, and the objects of the serialized ones. An
//! asset whose file and `.meta` keep their size and modification time is taken from the store
//! instead of being read again.

use crate::graph::{Cache, Graph, Source};
use crate::problems::{self, Problem};
use crate::scan::{self, ScannedFile};
use crate::store::{Record, Store};
use crate::walk::{self, Stamp};
use parking_lot::RwLock;
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Scanned files read back from the store.
pub(crate) struct StoreSource(pub Arc<Store>);

impl Source for StoreSource {
    fn load(&self, rel: &str) -> Option<ScannedFile> {
        self.0.scan(rel).ok().flatten()
    }

    fn commit(&self, put: &[(&str, Record, Option<&ScannedFile>)], touched: &[(&str, Record)], removed: &[&str]) {
        if let Err(e) = self.0.commit(put, touched, removed) {
            tracing::warn!("unity index: {e}");
        }
    }
}

/// Scanned files kept in memory, when the store can't be used.
pub(crate) struct MemorySource(pub RwLock<HashMap<String, ScannedFile>>);

impl Source for MemorySource {
    fn load(&self, rel: &str) -> Option<ScannedFile> {
        self.0.read().get(rel).cloned()
    }

    fn commit(&self, put: &[(&str, Record, Option<&ScannedFile>)], _: &[(&str, Record)], removed: &[&str]) {
        if put.is_empty() && removed.is_empty() {
            return;
        }
        let mut map = self.0.write();
        for (rel, _, scan) in put {
            match scan {
                Some(scan) => {
                    map.insert((*rel).to_owned(), (*scan).clone());
                }
                None => {
                    map.remove(*rel);
                }
            }
        }
        for rel in removed {
            map.remove(*rel);
        }
    }
}

enum Outcome {
    Unchanged(Record),
    Changed(Record, Option<ScannedFile>),
}

/// Reads an asset (and its `.meta`) unless the stored record still matches.
fn index_one(entry: &walk::Entry, stored: Option<&Record>) -> Option<Outcome> {
    let asset = Stamp::of(&entry.abs)?;
    let meta_path = walk::meta_path(&entry.abs);
    let meta = Stamp::of(&meta_path).unwrap_or_default();
    if let Some(r) = stored
        && r.asset == asset
        && r.meta == meta
    {
        return Some(Outcome::Unchanged(*r));
    }
    let (record, scanned) = read(&entry.rel, &entry.abs, asset, meta, None);
    Some(Outcome::Changed(record, scanned))
}

/// Reads an asset's GUID and, for a serialized one, its objects. With `known` (what is indexed),
/// content of the same hash isn't scanned again: the scan is then `None` though `scanned` is set.
pub(crate) fn read(
    rel: &str,
    abs: &Path,
    asset: Stamp,
    meta: Stamp,
    known: Option<&Record>,
) -> (Record, Option<ScannedFile>) {
    let guid = std::fs::read(walk::meta_path(abs)).ok().and_then(|m| walk::meta_guid(&m));
    let bytes = walk::is_scanned(rel).then(|| std::fs::read(abs).ok()).flatten();
    let hash = bytes.as_deref().map_or(0, content_hash);
    let problem = bytes.as_deref().map_or(Problem::None, problems::problem);
    let readable = bytes.filter(|_| problem == Problem::None);
    let record = Record { asset, meta, guid, scanned: readable.is_some(), hash, problem };
    if record.scanned && known.is_some_and(|k| k.scanned && k.hash == hash) {
        return (record, None);
    }
    let scanned = readable.map(|b| scan::scan(&b));
    (record, scanned)
}

/// The asset a changed path is (a `.meta` stands for its asset), relative to `root` with `/`;
/// `None` outside the asset roots or in what Unity ignores.
pub(crate) fn asset_rel(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let mut parts = Vec::new();
    for c in rel.components() {
        let name = c.as_os_str().to_str()?;
        if walk::ignored(name) {
            return None;
        }
        parts.push(name);
    }
    let rel = parts.join("/");
    let rel = rel.strip_suffix(".meta").map(str::to_owned).unwrap_or(rel);
    // `Packages/<name>` itself: an embedded package added or removed as a whole.
    let in_roots = rel.starts_with("Assets/") || (rel.starts_with("Packages/") && rel.len() > "Packages/".len());
    in_roots.then_some(rel)
}

enum PendingOp {
    Remove { rel: String, old: Option<ScannedFile> },
    Touch { rel: String, record: Record },
    Set { rel: String, record: Record, scan: Option<ScannedFile>, old: Option<ScannedFile> },
}

/// Applies changes to the assets `rels` (files or folders, existing or not) to a scanned project;
/// whether anything queries see changed (not only file stamps).
pub(crate) fn apply(root: &Path, graph: &RwLock<Graph>, source: &dyn Source, cache: &Cache, rels: Vec<String>) -> bool {
    let mut files = Vec::new();
    for rel in rels {
        let abs = root.join(&rel);
        // `Packages/manifest.json` and the like aren't assets; a package is a folder.
        let package = rel.starts_with("Packages/") && rel.matches('/').count() == 1;
        if abs.is_dir() {
            files.extend(walk::files_under(&abs, &rel));
            files.extend(graph.read().rels_under(&rel));
        } else if package {
            files.extend(graph.read().rels_under(&rel));
        } else if abs.exists() || graph.read().record(&rel).is_some() {
            files.push(rel);
        } else {
            // A folder that went away: everything indexed below it.
            files.extend(graph.read().rels_under(&rel));
        }
    }
    files.sort();
    files.dedup();
    let known_records: Vec<(String, Option<Record>)> = {
        let g = graph.read();
        files
            .into_iter()
            .map(|rel| {
                let known = g.record(&rel);
                (rel, known)
            })
            .collect()
    };
    let mut ops = Vec::new();
    for (rel, known) in known_records {
        let abs = root.join(&rel);
        let Some(asset) = Stamp::of(&abs).filter(|_| abs.is_file()) else {
            if let Some(known) = known {
                let old = known.scanned.then(|| source.load(&rel)).flatten();
                ops.push(PendingOp::Remove { rel, old });
            }
            continue;
        };
        let meta = Stamp::of(&walk::meta_path(&abs)).unwrap_or_default();
        if known.is_some_and(|k| k.asset == asset && k.meta == meta) {
            continue;
        }
        let (record, scan) = read(&rel, &abs, asset, meta, known.as_ref());
        if scan.is_none()
            && known
                .is_some_and(|k| k.guid == record.guid && k.scanned == record.scanned && k.problem == record.problem)
        {
            // Saved without a change (or a file that isn't scanned, like a script): only the stamps move.
            ops.push(PendingOp::Touch { rel, record });
            continue;
        }
        let old = known.filter(|k| k.scanned).and_then(|_| source.load(&rel));
        let scan = match scan {
            Some(scan) => Some(scan),
            // Same content, new GUID: the scan is the stored one.
            None if record.scanned => old.clone(),
            None => None,
        };
        ops.push(PendingOp::Set { rel, record, scan, old });
    }
    if ops.is_empty() {
        return false;
    }
    let mut put = Vec::new();
    let mut touched = Vec::new();
    let mut removed = Vec::new();
    for op in &ops {
        match op {
            PendingOp::Remove { rel, .. } => removed.push(rel.as_str()),
            PendingOp::Touch { rel, record } => touched.push((rel.as_str(), *record)),
            PendingOp::Set { rel, record, scan, .. } => put.push((rel.as_str(), *record, scan.as_ref())),
        }
    }
    // Persist in one batch before acquiring `graph.write()`, then evict stale cache entries under the lock.
    source.commit(&put, &touched, &removed);
    let mut changed = false;
    let mut g = graph.write();
    for op in &ops {
        match op {
            PendingOp::Remove { rel, old } => {
                g.remove(rel, old.as_ref());
                cache.remove(rel);
                changed = true;
            }
            PendingOp::Touch { rel, record } => {
                g.touch(rel, *record);
            }
            PendingOp::Set { rel, record, scan, old } => {
                g.set(rel, *record, scan.as_ref(), old.as_ref());
                cache.remove(rel);
                changed = true;
            }
        }
    }
    changed
}

pub(crate) fn content_hash(bytes: &[u8]) -> u64 {
    let hash = blake3::hash(bytes);
    let mut first = [0u8; 8];
    first.copy_from_slice(&hash.as_bytes()[..8]);
    u64::from_le_bytes(first)
}

/// Records written per transaction.
const BATCH: usize = 2_000;

/// Scans the project at `root`; `None` when cancelled.
pub(crate) fn full_scan(
    root: &Path,
    store: Option<&Arc<Store>>,
    cancelled: &AtomicBool,
) -> Option<(Graph, Arc<dyn Source>)> {
    let stored =
        store.and_then(|s| s.records().map_err(|e| tracing::warn!("unity index: {e}")).ok()).unwrap_or_default();
    let entries = walk::assets(root);
    let outcomes: Vec<Option<Outcome>> = entries
        .par_iter()
        .map(|e| if cancelled.load(Ordering::Relaxed) { None } else { index_one(e, stored.get(&e.rel)) })
        .collect();
    if cancelled.load(Ordering::Acquire) {
        return None;
    }
    let mut unchanged_scans = if let Some(store) = store {
        let rels: Vec<&str> = entries
            .iter()
            .zip(&outcomes)
            .filter_map(|(e, o)| match o {
                Some(Outcome::Unchanged(r)) if r.scanned => Some(e.rel.as_str()),
                _ => None,
            })
            .collect();
        store.scans(&rels).map_err(|e| tracing::warn!("unity index: {e}")).unwrap_or_default()
    } else {
        HashMap::new()
    };
    let mut graph = Graph::default();
    let mut memory = HashMap::new();
    let mut put: Vec<(&str, Record, Option<&ScannedFile>)> = Vec::new();
    for (entry, outcome) in entries.iter().zip(&outcomes) {
        match outcome {
            Some(Outcome::Unchanged(record)) => {
                let scan = record.scanned.then(|| unchanged_scans.remove(&entry.rel)).flatten();
                graph.set(&entry.rel, *record, scan.as_ref(), None);
            }
            Some(Outcome::Changed(record, scan)) => {
                graph.set(&entry.rel, *record, scan.as_ref(), None);
                if store.is_some() {
                    put.push((&entry.rel, *record, scan.as_ref()));
                } else if let Some(scan) = scan {
                    memory.insert(entry.rel.clone(), scan.clone());
                }
            }
            None => {}
        }
    }
    if let Some(store) = store {
        let present: std::collections::HashSet<&str> = entries.iter().map(|e| e.rel.as_str()).collect();
        let removed: Vec<&str> = stored.keys().map(String::as_str).filter(|r| !present.contains(r)).collect();
        for (i, chunk) in put.chunks(BATCH).enumerate() {
            let removed: &[&str] = if i == 0 { &removed } else { &[] };
            if let Err(e) = store.write(chunk, removed) {
                tracing::warn!("unity index: {e}");
            }
        }
        if put.is_empty()
            && !removed.is_empty()
            && let Err(e) = store.write(&[], &removed)
        {
            tracing::warn!("unity index: {e}");
        }
        return Some((graph, Arc::new(StoreSource(store.clone()))));
    }
    Some((graph, Arc::new(MemorySource(RwLock::new(memory)))))
}

#[cfg(test)]
mod tests {
    use super::asset_rel;
    use std::path::Path;

    #[test]
    fn changed_paths_map_to_assets() {
        let root = Path::new("/p");
        let rel = |p: &str| asset_rel(root, &root.join(p));
        assert_eq!(rel("Assets/A.prefab.meta").as_deref(), Some("Assets/A.prefab"));
        assert_eq!(
            rel("Packages/com.x.tools/Runtime/B.prefab").as_deref(),
            Some("Packages/com.x.tools/Runtime/B.prefab")
        );
        // A package folder added or removed as a whole.
        assert_eq!(rel("Packages/com.x.tools").as_deref(), Some("Packages/com.x.tools"));
        assert_eq!(rel("Packages"), None);
        assert_eq!(rel("Library/x.asset"), None);
    }
}
