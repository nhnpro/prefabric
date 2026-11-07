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
pub mod prefab;
pub mod scan;
mod store;
#[doc(hidden)]
pub mod synthetic;
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
}

/// Whether the index may start (the editor is ready); polled until it says yes.
pub type ReadyGate = Box<dyn Fn() -> bool + Send>;

const GATE_POLL: Duration = Duration::from_millis(250);

struct Shared {
    root: PathBuf,
    sink: SharedSink,
    status: Mutex<UnityIndexStatus>,
    /// Signalled when the status changes or the index is cancelled.
    changed: Condvar,
    cancelled: AtomicBool,
    /// The last complete scan; `None` until the first one ends.
    indexed: RwLock<Option<Arc<Indexed>>>,
}

struct Indexed {
    graph: graph::Graph,
    source: Arc<dyn graph::Source>,
    cache: graph::Cache,
}

impl Shared {
    fn set(&self, status: UnityIndexStatus) {
        let mut current = self.status.lock();
        if *current != status {
            *current = status;
            self.sink.emit(IdeEvent::UnityIndexState { indexing: status.indexing, files: status.files });
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
        let started = Instant::now();
        let Some((graph, source)) = build::full_scan(&self.root, store.as_ref(), &self.cancelled) else { return };
        tracing::info!(
            "unity index: {} assets ({} scanned) in {} ms",
            graph.file_count(),
            graph.scanned_count(),
            started.elapsed().as_millis()
        );
        let files = graph.file_count();
        *self.indexed.write() = Some(Arc::new(Indexed { graph, source, cache: graph::Cache::default() }));
        self.set(UnityIndexStatus { indexing: false, files });
        let mut status = self.status.lock();
        while !self.cancelled() {
            self.changed.wait(&mut status);
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
            status: Mutex::new(UnityIndexStatus { indexing: true, files: 0 }),
            changed: Condvar::new(),
            cancelled: AtomicBool::new(false),
            indexed: RwLock::new(None),
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
        f(&graph::Query { graph: &indexed.graph, source: &*indexed.source, cache: &indexed.cache })
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
        if let Some(handle) = self.worker.lock().take() {
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
mod tests;
