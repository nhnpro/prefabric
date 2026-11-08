//! Index of a Unity project's serialized assets (scenes, prefabs, ScriptableObjects…): which
//! GameObjects use a script, what a prefab overrides, which methods UnityEvents call.
//!
//! It runs beside the code index, in its own thread and its own store, and only in Unity projects.
//! It starts once the editor is ready (the code index has finished its first scan), so it never
//! competes with opening the project. Queries answer from what is indexed so far;
//! `IdeEvent::UnityIndexState` tells the UI when it is complete.

mod build;
pub mod events;
mod graph;
pub mod lenses;
pub mod prefab;
pub mod problems;
pub mod scan;
pub mod search;
mod store;
#[doc(hidden)]
pub mod synthetic;
pub mod usages;
mod walk;

pub use graph::{BindingKind, FieldValue, MethodBinding, ScriptUsage};
pub use scan::Guid;

use ide_core::{IdeEvent, SharedSink};
use parking_lot::{Condvar, Mutex, RwLock};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UnityIndexStatus {
    /// Waiting to start or scanning.
    pub indexing: bool,
    /// Indexed asset files.
    pub files: u32,
    /// Counts the times the index got new content (a scan, saved assets): what was derived from
    /// it (code lenses…) is out of date when it changes.
    pub revision: u32,
}

/// Whether the index may start (the editor is ready); polled until it says yes.
pub type ReadyGate = Box<dyn Fn() -> bool + Send>;

const GATE_POLL: Duration = Duration::from_millis(250);
/// Changes are applied once no other arrives for this long (an editor's save writes several files).
const SETTLE: Duration = Duration::from_millis(15);
/// More changed assets than this (a branch switch, a reimport) make one batched rescan, once no
/// change arrives for `BULK_SETTLE`.
const BULK: usize = 256;
const BULK_SETTLE: Duration = Duration::from_millis(300);

#[derive(Default)]
struct Pending {
    /// Changed assets, relative to the root.
    rels: std::collections::HashSet<String>,
    last: Option<Instant>,
}

struct Shared {
    root: PathBuf,
    sink: SharedSink,
    status: Mutex<UnityIndexStatus>,
    /// Signalled when the status changes or the index is cancelled.
    changed: Condvar,
    cancelled: AtomicBool,
    /// The last complete scan; `None` until the first one ends.
    indexed: RwLock<Option<Arc<Indexed>>>,
    pending: Mutex<Pending>,
    /// Signalled when changes arrive or the index is cancelled.
    wake: Condvar,
    /// The binary-assets notice was shown in this session.
    binary_reported: AtomicBool,
    /// The conflicted assets last reported.
    conflicts_reported: Mutex<Vec<Arc<str>>>,
}

/// Assets the index can't read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Problems {
    pub serialization: Option<problems::SerializationMode>,
    /// Serialized assets in Unity's binary format.
    pub binary_assets: u32,
    /// Assets skipped for their merge-conflict markers.
    pub conflicted: Vec<PathBuf>,
}

struct Indexed {
    graph: RwLock<graph::Graph>,
    source: Arc<dyn graph::Source>,
    cache: graph::Cache,
}

impl Shared {
    /// The index is `indexing`, or got new content (bumping the revision).
    fn set(&self, indexing: bool, files: u32) {
        let mut current = self.status.lock();
        let revision = if indexing { current.revision } else { current.revision.wrapping_add(1) };
        let status = UnityIndexStatus { indexing, files, revision };
        if *current != status {
            *current = status;
            self.sink.emit(IdeEvent::UnityIndexState { indexing, files, revision });
        }
        self.changed.notify_all();
    }

    fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    fn run(&self, store_dir: PathBuf, ready: ReadyGate) {
        while !ready() {
            if self.cancelled() {
                return;
            }
            let mut status = self.status.lock();
            self.changed.wait_for(&mut status, GATE_POLL);
        }
        if self.cancelled() {
            return;
        }
        let store = store::Store::open(&store_dir, &self.root).map(Arc::new);
        if !self.rescan(store.as_ref()) {
            return;
        }
        self.report(store.as_deref());
        while let Some(rels) = self.next_changes() {
            if rels.len() > BULK {
                let files = self.status.lock().files;
                self.set(true, files);
                if !self.rescan(store.as_ref()) {
                    return;
                }
                self.report(store.as_deref());
                continue;
            }
            let Some(indexed) = self.indexed.read().clone() else { continue };
            build::apply(&self.root, &indexed.graph, &*indexed.source, &indexed.cache, rels);
            let files = indexed.graph.read().file_count();
            self.set(false, files);
            self.report(store.as_deref());
        }
    }

    fn problems(&self) -> Problems {
        let Some(indexed) = self.indexed.read().clone() else { return Problems::default() };
        let (binary_assets, conflicted) = indexed.graph.read().problems();
        Problems {
            serialization: problems::serialization_mode(&self.root),
            binary_assets,
            conflicted: conflicted.iter().map(|rel| self.root.join(&**rel)).collect(),
        }
    }

    /// Tells the user about binary assets (once per project) and conflicted ones (when they change).
    fn report(&self, store: Option<&store::Store>) {
        use ide_core::event::NotifyLevel;
        let Some(indexed) = self.indexed.read().clone() else { return };
        let (binary, conflicted) = indexed.graph.read().problems();
        let mode = problems::serialization_mode(&self.root);
        let binary_project = binary > 0
            || matches!(mode, Some(problems::SerializationMode::ForceBinary | problems::SerializationMode::Unreadable));
        const SHOWN: &str = "binary-notice";
        if binary_project && !self.binary_reported.swap(true, Ordering::AcqRel) && !store.is_some_and(|s| s.flag(SHOWN))
        {
            self.sink.emit(IdeEvent::Notify {
                level: NotifyLevel::Info,
                title: problems::BINARY_TITLE.into(),
                body: Some(problems::BINARY_BODY.into()),
            });
            if let Some(store) = store
                && let Err(e) = store.set_flag(SHOWN)
            {
                tracing::warn!("unity index: {e}");
            }
        }
        let mut reported = self.conflicts_reported.lock();
        if *reported != conflicted {
            if !conflicted.is_empty() {
                let files: Vec<&str> = conflicted.iter().map(|r| &**r).collect();
                self.sink.emit(IdeEvent::Notify {
                    level: NotifyLevel::Warning,
                    title: problems::CONFLICT_TITLE.into(),
                    body: Some(problems::conflict_body(&files)),
                });
            }
            *reported = conflicted;
        }
    }

    /// A full scan replacing the index; `false` when cancelled.
    fn rescan(&self, store: Option<&Arc<store::Store>>) -> bool {
        let started = Instant::now();
        let Some((graph, source)) = build::full_scan(&self.root, store, &self.cancelled) else { return false };
        tracing::info!(
            "unity index: {} assets ({} scanned) in {} ms",
            graph.file_count(),
            graph.scanned_count(),
            started.elapsed().as_millis()
        );
        let files = graph.file_count();
        *self.indexed.write() =
            Some(Arc::new(Indexed { graph: RwLock::new(graph), source, cache: graph::Cache::default() }));
        self.set(false, files);
        true
    }

    /// Waits for changes, then until they settle; `None` when cancelled.
    fn next_changes(&self) -> Option<Vec<String>> {
        let mut pending = self.pending.lock();
        loop {
            if self.cancelled() {
                return None;
            }
            match pending.last {
                None => {
                    self.wake.wait(&mut pending);
                }
                Some(last) => {
                    let quiet = if pending.rels.len() > BULK { BULK_SETTLE } else { SETTLE };
                    let waited = last.elapsed();
                    if waited >= quiet {
                        pending.last = None;
                        return Some(pending.rels.drain().collect());
                    }
                    self.wake.wait_for(&mut pending, quiet - waited);
                }
            }
        }
    }
}

pub struct UnityIndex {
    shared: Arc<Shared>,
    worker: Mutex<Option<JoinHandle<()>>>,
}

impl UnityIndex {
    /// Starts the index of the Unity project at `root`, persisted in `store_dir`. It waits for
    /// `ready` before reading anything.
    pub fn open(root: &Path, store_dir: &Path, sink: SharedSink, ready: ReadyGate) -> std::io::Result<Arc<Self>> {
        let root = dunce::canonicalize(root).unwrap_or_else(|_| root.to_owned());
        let shared = Arc::new(Shared {
            root,
            sink,
            status: Mutex::new(UnityIndexStatus { indexing: true, files: 0, revision: 0 }),
            changed: Condvar::new(),
            cancelled: AtomicBool::new(false),
            indexed: RwLock::new(None),
            pending: Mutex::new(Pending::default()),
            wake: Condvar::new(),
            binary_reported: AtomicBool::new(false),
            conflicts_reported: Mutex::new(Vec::new()),
        });
        let worker = {
            let shared = shared.clone();
            let store_dir = store_dir.to_owned();
            std::thread::Builder::new()
                .name("prefabric-unity-index".into())
                .spawn(move || shared.run(store_dir, ready))?
        };
        Ok(Arc::new(Self { shared, worker: Mutex::new(Some(worker)) }))
    }

    pub fn root(&self) -> &Path {
        &self.shared.root
    }

    pub fn status(&self) -> UnityIndexStatus {
        *self.shared.status.lock()
    }

    fn query<T: Default>(&self, f: impl FnOnce(&graph::Query<'_>) -> T) -> T {
        let Some(indexed) = self.shared.indexed.read().clone() else { return T::default() };
        let graph = indexed.graph.read();
        f(&graph::Query { graph: &graph, source: &*indexed.source, cache: &indexed.cache })
    }

    /// Files changed on disk (from the project's watcher): the assets among them are indexed
    /// again in the background.
    pub fn update(&self, changed: &[PathBuf], removed: &[PathBuf]) {
        let rels: Vec<String> =
            changed.iter().chain(removed).filter_map(|p| build::asset_rel(&self.shared.root, p)).collect();
        if rels.is_empty() {
            return;
        }
        let mut pending = self.shared.pending.lock();
        pending.rels.extend(rels);
        pending.last = Some(Instant::now());
        self.shared.wake.notify_all();
    }

    fn rel<'a>(&self, path: &'a Path) -> Option<std::borrow::Cow<'a, str>> {
        let rel = path.strip_prefix(&self.shared.root).ok().or_else(|| path.is_relative().then_some(path))?;
        let rel = rel.to_str()?;
        Some(if std::path::MAIN_SEPARATOR == '/' { rel.into() } else { rel.replace('\\', "/").into() })
    }

    /// The GUID of the asset at `path` (absolute, or relative to the project).
    pub fn guid_of(&self, path: &Path) -> Option<Guid> {
        let rel = self.rel(path)?;
        self.query(|q| q.graph.guid_of(&rel))
    }

    /// The asset with this GUID.
    pub fn path_of(&self, guid: Guid) -> Option<PathBuf> {
        self.query(|q| q.graph.rel_of(guid).map(|rel| self.shared.root.join(&**rel)))
    }

    /// Assets referring to `guid`.
    pub fn referrers(&self, guid: Guid) -> Vec<PathBuf> {
        self.query(|q| q.graph.referrers(guid).map(|rel| self.shared.root.join(&**rel)).collect())
    }

    /// Components using the script `script` (the GUID of its `.cs` file), directly or through
    /// prefab instances.
    pub fn script_usages(&self, script: Guid) -> Vec<ScriptUsage> {
        self.query(|q| q.script_usages(script))
    }

    /// UnityEvent listeners and animation events calling `method` (of the class `type_name`, a
    /// full name, when given).
    pub fn method_bindings(&self, method: &str, type_name: Option<&str>) -> Vec<MethodBinding> {
        self.query(|q| q.method_bindings(method, type_name))
    }

    /// The serialized field `field` on every component using `script`, overrides applied.
    pub fn field_values(&self, script: Guid, field: &str) -> Vec<FieldValue> {
        self.query(|q| q.field_values(script, field))
    }

    /// [`Self::field_values`] of several fields of `script`, in the order of `fields`.
    pub fn fields_values(&self, script: Guid, fields: &[&str]) -> Vec<Vec<FieldValue>> {
        self.query(|q| q.fields_values(script, fields))
    }

    /// Assets the index can't read: binary ones, and ones with merge conflicts.
    pub fn problems(&self) -> Problems {
        self.shared.problems()
    }

    /// Heap held by the in-memory graph, roughly.
    pub fn heap_bytes(&self) -> usize {
        self.query(|q| q.graph.heap_bytes())
    }

    /// Blocks until the index is complete (tests, benchmarks). `false` on timeout.
    pub fn wait_idle(&self, timeout: Duration) -> bool {
        let deadline = Instant::now() + timeout;
        let mut status = self.shared.status.lock();
        while status.indexing {
            if self.shared.changed.wait_until(&mut status, deadline).timed_out() {
                return !status.indexing;
            }
        }
        true
    }
}

impl Drop for UnityIndex {
    fn drop(&mut self) {
        self.shared.cancelled.store(true, Ordering::Release);
        {
            let _status = self.shared.status.lock();
            self.shared.changed.notify_all();
        }
        {
            let _pending = self.shared.pending.lock();
            self.shared.wake.notify_all();
        }
        if let Some(handle) = self.worker.lock().take() {
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
mod tests;
