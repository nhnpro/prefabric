//! A full scan of the project: every asset's GUID, and the objects of the serialized ones. An
//! asset whose file and `.meta` keep their size and modification time is taken from the store
//! instead of being read again.

use crate::graph::{Graph, Source};
use crate::scan::{self, ScannedFile};
use crate::store::{Record, Store};
use crate::walk::{self, Stamp};
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
}

/// Scanned files kept in memory, when the store can't be used.
pub(crate) struct MemorySource(pub HashMap<String, ScannedFile>);

impl Source for MemorySource {
    fn load(&self, rel: &str) -> Option<ScannedFile> {
        self.0.get(rel).cloned()
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
    let guid = std::fs::read(&meta_path).ok().and_then(|m| walk::meta_guid(&m));
    let scanned = walk::is_scanned(&entry.rel)
        .then(|| std::fs::read(&entry.abs).ok())
        .flatten()
        .filter(|bytes| bytes.starts_with(b"%YAML"))
        .map(|bytes| scan::scan(&bytes));
    Some(Outcome::Changed(Record { asset, meta, guid, scanned: scanned.is_some() }, scanned))
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
                graph.add(&entry.rel, record.guid, scan.as_ref());
            }
            Some(Outcome::Changed(record, scan)) => {
                graph.add(&entry.rel, record.guid, scan.as_ref());
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
    Some((graph, Arc::new(MemorySource(memory))))
}
