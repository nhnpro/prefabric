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
