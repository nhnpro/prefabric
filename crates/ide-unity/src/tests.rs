use super::*;
use std::sync::atomic::AtomicBool;

fn recording() -> (SharedSink, Arc<Mutex<Vec<IdeEvent>>>) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = {
        let events = events.clone();
        Arc::new(move |e: IdeEvent| events.lock().push(e)) as SharedSink
    };
    (sink, events)
}

#[test]
fn waits_for_the_editor_then_reports_its_state() {
    let project = tempfile::tempdir().unwrap();
    let store = tempfile::tempdir().unwrap();
    let (sink, events) = recording();
    let ready = Arc::new(AtomicBool::new(false));
    let index = {
        let ready = ready.clone();
        UnityIndex::open(project.path(), store.path(), sink, Box::new(move || ready.load(Ordering::Acquire))).unwrap()
    };
    assert!(!index.wait_idle(Duration::from_millis(400)));
    assert_eq!(index.status(), UnityIndexStatus { indexing: true, files: 0, revision: 0 });
    assert!(!store.path().join("unity.redb").exists(), "nothing is read before the editor is ready");
    ready.store(true, Ordering::Release);
    assert!(index.wait_idle(Duration::from_secs(5)));
    assert_eq!(index.status(), UnityIndexStatus { indexing: false, files: 0, revision: 1 });
    assert!(store.path().join("unity.redb").exists());
    let states: Vec<_> = events
        .lock()
        .iter()
        .filter_map(|e| match e {
            IdeEvent::UnityIndexState { indexing, files, .. } => Some((*indexing, *files)),
            _ => None,
        })
        .collect();
    assert_eq!(states, [(false, 0)]);
}

#[test]
fn closes_while_waiting_for_the_editor() {
    let project = tempfile::tempdir().unwrap();
    let store = tempfile::tempdir().unwrap();
    let (sink, _) = recording();
    let index = UnityIndex::open(project.path(), store.path(), sink, Box::new(|| false)).unwrap();
    let started = Instant::now();
    drop(index);
    assert!(started.elapsed() < Duration::from_secs(2));
}
