use crate::{BindingKind, Guid, UnityIndex};
use ide_core::NullSink;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

const PROJECT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/unity-sample");
const PLAYER_CS: &str = "f8dbec04297b54fd18c802467ac277ea";
const PLAYER_PREFAB: &str = "79cb0651700894f0a8e50458c76fa759";

fn guid(hex: &str) -> Guid {
    Guid::parse(hex.as_bytes()).unwrap()
}

fn open(store: &Path) -> Arc<UnityIndex> {
    let index = UnityIndex::open(Path::new(PROJECT), store, Arc::new(NullSink), Box::new(|| true)).unwrap();
    assert!(index.wait_idle(Duration::from_secs(20)));
    index
}

type Usage = (String, String, Option<String>);

fn usages(index: &UnityIndex) -> Vec<Usage> {
    let mut out: Vec<Usage> = index
        .script_usages(guid(PLAYER_CS))
        .into_iter()
        .map(|u| (u.rel.to_string(), u.game_object.join("/"), u.through.map(|t| t.to_string())))
        .collect();
    out.sort();
    out
}

fn expected_usages() -> Vec<Usage> {
    let s = |rel: &str, path: &str, through: Option<&str>| (rel.to_owned(), path.to_owned(), through.map(Into::into));
    vec![
        s("Assets/Prefabs/Player Variant.prefab", "Player Variant", Some("Assets/Prefabs/Player.prefab")),
        s("Assets/Prefabs/Player.prefab", "Player", None),
        s("Assets/Prefabs/Squad.prefab", "Squad/Follower", Some("Assets/Prefabs/Player.prefab")),
        s("Assets/Prefabs/Squad.prefab", "Squad/Leader", Some("Assets/Prefabs/Player.prefab")),
        s("Assets/Scenes/Main.unity", "Hero", Some("Assets/Prefabs/Player Variant.prefab")),
        s("Assets/Scenes/Main.unity", "Squad/Follower", Some("Assets/Prefabs/Squad.prefab")),
        s("Assets/Scenes/Main.unity", "Squad/Leader", Some("Assets/Prefabs/Squad.prefab")),
    ]
}

#[test]
fn queries_the_fixture() {
    let store = tempfile::tempdir().unwrap();
    let index = open(store.path());
    assert_eq!(index.status().files, 13);
    let root = dunce::canonicalize(PROJECT).unwrap();
    let prefab = root.join("Assets/Prefabs/Player.prefab");
    assert_eq!(index.guid_of(&prefab), Some(guid(PLAYER_PREFAB)));
    assert_eq!(index.guid_of(Path::new("Assets/Scripts/Player.cs")), Some(guid(PLAYER_CS)));
    assert_eq!(index.path_of(guid(PLAYER_PREFAB)), Some(prefab));
    let mut referrers: Vec<_> =
        index.referrers(guid(PLAYER_PREFAB)).into_iter().map(|p| p.strip_prefix(&root).unwrap().to_owned()).collect();
    referrers.sort();
    let names = ["Assets/Prefabs/Player Variant.prefab", "Assets/Prefabs/Squad.prefab", "Assets/Scenes/Main.unity"];
    assert_eq!(referrers, names.map(std::path::PathBuf::from));

    assert_eq!(usages(&index), expected_usages());

    let speeds: Vec<_> = index
        .field_values(guid(PLAYER_CS), "speed")
        .into_iter()
        .map(|v| {
            (
                v.usage.game_object.join("/"),
                v.value.unwrap().to_string(),
                v.overridden_at.map(|(r, l)| (r.to_string(), l)),
            )
        })
        .collect();
    let at = |rel: &str, line| Some((rel.to_owned(), line));
    assert!(speeds.contains(&("Player".into(), "2".into(), None)));
    assert!(speeds.contains(&("Player Variant".into(), "5".into(), at("Assets/Prefabs/Player Variant.prefab", 54))));
    assert!(speeds.contains(&("Hero".into(), "7".into(), at("Assets/Scenes/Main.unity", 450))));
    assert!(speeds.contains(&("Squad/Follower".into(), "2".into(), None)));
    let coins: Vec<_> = index
        .field_values(guid(PLAYER_CS), "coins")
        .into_iter()
        .map(|v| (v.usage.rel.to_string(), v.usage.game_object.join("/"), v.value.unwrap().to_string()))
        .collect();
    assert!(coins.contains(&("Assets/Scenes/Main.unity".into(), "Squad/Follower".into(), "10".into())));

    let [event] = index.method_bindings("OnTriggerZoneEntered", Some("Game.Player")).try_into().unwrap();
    assert_eq!(
        (&*event.rel, event.line, event.owner.join("/")),
        ("Assets/Scenes/Main.unity", 303, "Trigger Zone".into())
    );
    assert!(matches!(event.kind, BindingKind::UnityEvent { ref field, .. } if &**field == "onEnter"));
    assert!(index.method_bindings("OnTriggerZoneEntered", Some("Game.Spawner")).is_empty());
    let [clip] = index.method_bindings("OnFootstep", None).try_into().unwrap();
    assert_eq!(
        (&*clip.rel, clip.kind),
        ("Assets/Animations/Walk.anim", BindingKind::AnimationEvent { time: "0.25".into() })
    );
    assert!(index.heap_bytes() > 0);
}

#[test]
fn a_restart_reads_the_store_and_answers_the_same() {
    let store = tempfile::tempdir().unwrap();
    drop(open(store.path()));
    let index = open(store.path());
    assert_eq!(usages(&index), expected_usages());
}

#[test]
fn works_without_a_store() {
    let dir = tempfile::tempdir().unwrap();
    // A file where the store's folder should be: the index runs in memory.
    let blocked = dir.path().join("store");
    std::fs::write(&blocked, "").unwrap();
    let index = open(&blocked);
    assert_eq!(usages(&index), expected_usages());
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// A copy of the fixture project, indexed.
fn copied() -> (tempfile::TempDir, tempfile::TempDir, Arc<UnityIndex>, std::path::PathBuf) {
    let project = tempfile::tempdir().unwrap();
    let store = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(project.path()).unwrap();
    copy_dir(Path::new(PROJECT), &root);
    let index = UnityIndex::open(&root, store.path(), Arc::new(NullSink), Box::new(|| true)).unwrap();
    assert!(index.wait_idle(Duration::from_secs(20)));
    (project, store, index, root)
}

fn speed_of(index: &UnityIndex, path: &str) -> Option<String> {
    index
        .field_values(guid(PLAYER_CS), "speed")
        .into_iter()
        .find(|v| v.usage.game_object.join("/") == path)
        .and_then(|v| v.value.map(Into::into))
}

/// Waits until `f` holds; the time it took.
fn until(f: impl Fn() -> bool) -> Duration {
    let started = std::time::Instant::now();
    while !f() {
        assert!(started.elapsed() < Duration::from_secs(10), "timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
    started.elapsed()
}

#[test]
fn a_saved_prefab_updates_the_queries() {
    let (_project, _store, index, root) = copied();
    let prefab = root.join("Assets/Prefabs/Player.prefab");
    let original = std::fs::read_to_string(&prefab).unwrap();
    let mut times = Vec::new();
    for speed in 10..20 {
        std::fs::write(&prefab, original.replace("  speed: 2\n", &format!("  speed: {speed}\n"))).unwrap();
        index.update(std::slice::from_ref(&prefab), &[]);
        times.push(until(|| speed_of(&index, "Player").as_deref() == Some(&speed.to_string())));
        // An override still wins over the new value.
        assert_eq!(speed_of(&index, "Hero").as_deref(), Some("7"));
    }
    times.sort();
    let median = times[times.len() / 2];
    eprintln!("prefab save → query: median {median:?}, max {:?}", times.last().unwrap());
    assert!(median < Duration::from_millis(50), "median {median:?}");

    // Saved again with the same content: answers don't change.
    std::fs::write(&prefab, original.replace("  speed: 2\n", "  speed: 19\n")).unwrap();
    index.update(std::slice::from_ref(&prefab), &[]);
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(speed_of(&index, "Player").as_deref(), Some("19"));
    assert_eq!(usages(&index).len(), 7);
}

#[test]
fn removed_added_and_moved_assets() {
    let (_project, _store, index, root) = copied();
    let variant = root.join("Assets/Prefabs/Player Variant.prefab");
    let variant_meta = root.join("Assets/Prefabs/Player Variant.prefab.meta");
    let saved = (std::fs::read(&variant).unwrap(), std::fs::read(&variant_meta).unwrap());
    std::fs::remove_file(&variant).unwrap();
    std::fs::remove_file(&variant_meta).unwrap();
    index.update(&[], &[variant.clone(), variant_meta.clone()]);
    until(|| usages(&index).len() == 5);
    assert!(usages(&index).iter().all(|(rel, ..)| !rel.contains("Variant")));
    until(|| index.status().files == 12);

    std::fs::write(&variant, &saved.0).unwrap();
    std::fs::write(&variant_meta, &saved.1).unwrap();
    index.update(&[variant.clone(), variant_meta], &[]);
    until(|| usages(&index) == expected_usages());

    // A new prefab using the script.
    let copy = root.join("Assets/Prefabs/Copy.prefab");
    std::fs::copy(root.join("Assets/Prefabs/Player.prefab"), &copy).unwrap();
    std::fs::write(root.join("Assets/Prefabs/Copy.prefab.meta"), "guid: 0123456789abcdef0123456789abcdef\n").unwrap();
    index.update(std::slice::from_ref(&copy), &[]);
    until(|| usages(&index).iter().any(|(rel, ..)| rel == "Assets/Prefabs/Copy.prefab"));

    // A folder moved out of the project: everything below it goes.
    let outside = tempfile::tempdir().unwrap();
    std::fs::rename(root.join("Assets/Prefabs"), outside.path().join("Prefabs")).unwrap();
    index.update(&[], &[root.join("Assets/Prefabs")]);
    until(|| usages(&index).is_empty());
    // And back in.
    std::fs::rename(outside.path().join("Prefabs"), root.join("Assets/Prefabs")).unwrap();
    index.update(&[root.join("Assets/Prefabs")], &[]);
    until(|| usages(&index).len() == 8);
    // Paths outside the asset roots are ignored.
    index.update(&[root.join("Library/x.prefab"), root.join("Assets/.hidden.prefab")], &[]);
}

#[test]
fn a_bulk_change_is_one_rescan() {
    let (_project, _store, index, root) = copied();
    let player = std::fs::read_to_string(root.join("Assets/Prefabs/Player.prefab")).unwrap();
    let dir = root.join("Assets/Generated");
    std::fs::create_dir_all(&dir).unwrap();
    let mut paths = Vec::new();
    for i in 0..300 {
        let path = dir.join(format!("P{i}.prefab"));
        std::fs::write(&path, &player).unwrap();
        std::fs::write(dir.join(format!("P{i}.prefab.meta")), format!("guid: {i:032x}\n")).unwrap();
        paths.push(path);
    }
    let started = std::time::Instant::now();
    for chunk in paths.chunks(50) {
        index.update(chunk, &[]);
    }
    // Queueing a branch switch's worth of changes doesn't block the caller (the watcher thread).
    let many: Vec<_> = (0..5_000).map(|i| root.join(format!("Assets/Other/F{i}.prefab"))).collect();
    let queued = std::time::Instant::now();
    index.update(&[], &many);
    assert!(queued.elapsed() < Duration::from_millis(50), "{:?}", queued.elapsed());
    until(|| index.status().indexing);
    // Queries keep answering from the previous index meanwhile.
    let asked = std::time::Instant::now();
    let _ = usages(&index);
    assert!(asked.elapsed() < Duration::from_millis(100));
    until(|| !index.status().indexing && usages(&index).len() == 307);
    eprintln!("300 new prefabs indexed in {:?}", started.elapsed());
}
