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

    fn put(&self, rel: &str, record: Record, scan: Option<&ScannedFile>) {
        if let Err(e) = self.0.write(&[(rel, record, scan)], &[]) {
            tracing::warn!("unity index: {e}");
        }
    }

    fn remove(&self, rel: &str) {
        if let Err(e) = self.0.write(&[], &[rel]) {
            tracing::warn!("unity index: {e}");
        }
    }

    fn put_record(&self, rel: &str, record: Record) {
        if let Err(e) = self.0.write_record(rel, record) {
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

    fn put(&self, rel: &str, _: Record, scan: Option<&ScannedFile>) {
        match scan {
            Some(scan) => self.0.write().insert(rel.to_owned(), scan.clone()),
            None => self.0.write().remove(rel),
        };
    }

    fn remove(&self, rel: &str) {
        self.0.write().remove(rel);
    }

    fn put_record(&self, _: &str, _: Record) {}
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
    let in_roots = rel.starts_with("Assets/") || (rel.starts_with("Packages/") && rel.matches('/').count() >= 2);
    in_roots.then_some(rel)
}

/// Applies changes to the assets `rels` (files or folders, existing or not) to a scanned project.
pub(crate) fn apply(root: &Path, graph: &RwLock<Graph>, source: &dyn Source, cache: &Cache, rels: Vec<String>) {
    let mut files = Vec::new();
    for rel in rels {
        let abs = root.join(&rel);
        if abs.is_dir() {
            files.extend(walk::files_under(&abs, &rel));
        } else if abs.exists() || graph.read().record(&rel).is_some() {
            files.push(rel);
        } else {
            // A folder that went away: everything indexed below it.
            files.extend(graph.read().rels_under(&rel));
        }
    }
    files.sort();
    files.dedup();
    for rel in files {
        let abs = root.join(&rel);
        let known = graph.read().record(&rel);
        let Some(asset) = Stamp::of(&abs).filter(|_| abs.is_file()) else {
            if let Some(known) = known {
                let old = known.scanned.then(|| source.load(&rel)).flatten();
                graph.write().remove(&rel, old.as_ref());
                source.remove(&rel);
                cache.remove(&rel);
            }
            continue;
        };
        let meta = Stamp::of(&walk::meta_path(&abs)).unwrap_or_default();
        if known.is_some_and(|k| k.asset == asset && k.meta == meta) {
            continue;
        }
        let (record, scan) = read(&rel, &abs, asset, meta, known.as_ref());
        if record.scanned && scan.is_none() && known.is_some_and(|k| k.guid == record.guid) {
            // Saved without a change: only the stamps move.
            graph.write().touch(&rel, record);
            source.put_record(&rel, record);
            continue;
        }
        let old = known.filter(|k| k.scanned).and_then(|_| source.load(&rel));
        let scan = match scan {
            Some(scan) => Some(scan),
            // Same content, new GUID: the scan is the stored one.
            None if record.scanned => old.clone(),
            None => None,
        };
        graph.write().set(&rel, record, scan.as_ref(), old.as_ref());
        source.put(&rel, record, scan.as_ref());
        cache.remove(&rel);
    }
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
    let mut graph = Graph::default();
    let mut memory = HashMap::new();
    let mut put: Vec<(&str, Record, Option<&ScannedFile>)> = Vec::new();
    for (entry, outcome) in entries.iter().zip(&outcomes) {
        match outcome {
            Some(Outcome::Unchanged(record)) => {
                let scan = match (record.scanned, store) {
                    (true, Some(store)) => store.scan(&entry.rel).ok().flatten(),
                    _ => None,
                };
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
