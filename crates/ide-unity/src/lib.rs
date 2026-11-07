//! Index of a Unity project's serialized assets (scenes, prefabs, ScriptableObjects…): which
//! GameObjects use a script, what a prefab overrides, which methods UnityEvents call.
//!
//! It runs beside the code index, in its own thread and its own store, and only in Unity projects.
//! It starts once the editor is ready (the code index has finished its first scan), so it never
//! competes with opening the project. Queries answer from what is indexed so far;
//! `IdeEvent::UnityIndexState` tells the UI when it is complete.

pub mod prefab;
pub mod scan;
mod store;

use ide_core::{IdeEvent, SharedSink};
use parking_lot::{Condvar, Mutex};
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
        let store = store::Store::open(&store_dir, &self.root);
        let files = store.as_ref().and_then(|s| s.file_count().ok()).unwrap_or(0);
        self.set(UnityIndexStatus { indexing: false, files: u32::try_from(files).unwrap_or(u32::MAX) });
        // Updates arrive here once the scanner exists; until then the worker idles until closed.
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
