use super::*;
use crate::scan::scan;

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/unity-sample/Assets");

fn guid(hex: &str) -> Guid {
    Guid::parse(hex.as_bytes()).unwrap()
}

const PLAYER: &str = "79cb0651700894f0a8e50458c76fa759";
const VARIANT: &str = "80147db3ebd7b43a585f319f06c99381";
const SQUAD: &str = "046e90e86723d4340ac603f82a340f4c";
const SCENE: &str = "785ab778687f74d0285516dd608851ca";

/// Player.prefab's objects.
const PLAYER_GO: i64 = 1354967268023247473;
const PLAYER_BEHAVIOUR: i64 = 8815640413858057345;
/// The variant's PrefabInstance of Player, Squad's instances (Follower, Leader), the scene's
/// instance of Squad and of the variant (Hero).
const VARIANT_INSTANCE: i64 = 2063827559162485253;
const FOLLOWER: i64 = 1037567035831799824;
const LEADER: i64 = 5649960808104458441;
const SCENE_SQUAD: i64 = 600556987;
const HERO_BEHAVIOUR_STRIPPED: i64 = 1133812325;

fn assets() -> HashMap<Guid, ScannedFile> {
    ["Prefabs/Player.prefab", "Prefabs/Player Variant.prefab", "Prefabs/Squad.prefab", "Scenes/Main.unity"]
        .iter()
        .map(|rel| {
            let meta = std::fs::read_to_string(format!("{FIXTURE}/{rel}.meta")).unwrap();
            let hex = meta.lines().find_map(|l| l.strip_prefix("guid: ")).unwrap();
            (guid(hex), scan(&std::fs::read(format!("{FIXTURE}/{rel}")).unwrap()))
        })
        .collect()
}

fn value(assets: &HashMap<Guid, ScannedFile>, file: &str, id: i64, path: &str) -> (Option<String>, Option<Guid>) {
    let v = property(assets, guid(file), id, path).unwrap_or_else(|| panic!("{path} of {id} in {file}"));
    (v.value.map(Into::into), v.overridden_at.map(|(g, _)| g))
}

#[test]
fn scans_prefab_instances() {
    let assets = assets();
    let variant = &assets[&guid(VARIANT)];
    let instance = variant.object(VARIANT_INSTANCE).unwrap();
    assert_eq!(instance.source_prefab.unwrap().guid, Some(guid(PLAYER)));
    assert_eq!((instance.modifications.len(), instance.transform_parent), (12, None));
    let speed = instance.modifications.last().unwrap();
    assert_eq!(
        (&*speed.property_path, speed.value.as_deref(), speed.target.file_id, speed.line),
        ("speed", Some("5"), PLAYER_BEHAVIOUR, 54)
    );
    let squad = &assets[&guid(SQUAD)];
    assert_eq!(squad.object(FOLLOWER).unwrap().transform_parent, Some(8109665369720403721));
    assert_eq!(squad.prefab_instances().count(), 2);
    let behaviour = assets[&guid(PLAYER)].object(PLAYER_BEHAVIOUR).unwrap();
    assert_eq!(behaviour.properties, [("speed".into(), "2".into()), ("coins".into(), "0".into())]);
}

#[test]
fn a_variant_overrides_its_base() {
    let assets = assets();
    let id = VARIANT_INSTANCE ^ PLAYER_BEHAVIOUR;
    let origin = resolve(&assets, guid(VARIANT), id).unwrap();
    assert_eq!((origin.file, origin.object.file_id), (guid(PLAYER), PLAYER_BEHAVIOUR));
    assert_eq!(
        origin.hops,
        [Hop { file: guid(VARIANT), instance: VARIANT_INSTANCE, source: guid(PLAYER), source_id: PLAYER_BEHAVIOUR }]
    );
    assert_eq!(value(&assets, VARIANT, id, "speed"), (Some("5".into()), Some(guid(VARIANT))));
    assert_eq!(value(&assets, VARIANT, id, "coins"), (Some("0".into()), None));
    assert_eq!(value(&assets, PLAYER, PLAYER_BEHAVIOUR, "speed"), (Some("2".into()), None));
    let go = VARIANT_INSTANCE ^ PLAYER_GO;
    assert_eq!(value(&assets, VARIANT, go, "m_Name"), (Some("Player Variant".into()), Some(guid(VARIANT))));
}

#[test]
fn a_scene_instance_of_a_variant_overrides_both() {
    let assets = assets();
    let origin = resolve(&assets, guid(SCENE), HERO_BEHAVIOUR_STRIPPED).unwrap();
    assert_eq!((origin.file, origin.object.file_id, origin.hops.len()), (guid(PLAYER), PLAYER_BEHAVIOUR, 2));
    assert_eq!(value(&assets, SCENE, HERO_BEHAVIOUR_STRIPPED, "speed"), (Some("7".into()), Some(guid(SCENE))));
    assert_eq!(value(&assets, SCENE, HERO_BEHAVIOUR_STRIPPED, "coins"), (Some("0".into()), None));
    // The scene's override hides the variant's.
    let all = overrides(&assets, guid(SCENE), HERO_BEHAVIOUR_STRIPPED);
    let found: Vec<_> = all.iter().map(|(g, m)| (*g, &*m.property_path, m.value.as_deref())).collect();
    assert_eq!(found, [(guid(SCENE), "speed", Some("7"))]);
}

#[test]
fn nested_prefab_overrides_reach_the_scene() {
    let assets = assets();
    // The Follower's behaviour and GameObject, as the scene's Squad instance has them.
    let behaviour = SCENE_SQUAD ^ (FOLLOWER ^ PLAYER_BEHAVIOUR);
    let origin = resolve(&assets, guid(SCENE), behaviour).unwrap();
    assert_eq!(origin.object.file_id, PLAYER_BEHAVIOUR);
    assert_eq!(origin.hops.iter().map(|h| h.file).collect::<Vec<_>>(), [guid(SCENE), guid(SQUAD)]);
    assert_eq!(value(&assets, SCENE, behaviour, "coins"), (Some("10".into()), Some(guid(SQUAD))));
    assert_eq!(value(&assets, SCENE, behaviour, "speed"), (Some("2".into()), None));
    let follower = SCENE_SQUAD ^ (FOLLOWER ^ PLAYER_GO);
    let leader = SCENE_SQUAD ^ (LEADER ^ PLAYER_GO);
    assert_eq!(value(&assets, SCENE, follower, "m_Name"), (Some("Follower".into()), Some(guid(SQUAD))));
    assert_eq!(value(&assets, SCENE, leader, "m_Name"), (Some("Leader".into()), Some(guid(SQUAD))));
    assert_eq!(value(&assets, SCENE, SCENE_SQUAD ^ 5281235478733098455, "m_Name").0.as_deref(), Some("Squad"));
}

#[test]
fn unknown_objects_and_cycles_resolve_to_nothing() {
    let assets = assets();
    assert!(resolve(&assets, guid(SCENE), 12345).is_none());
    assert!(property(&assets, guid(PLAYER), PLAYER_BEHAVIOUR, "missing").is_none());
    let looping = "--- !u!1001 &7\nPrefabInstance:\n  m_SourcePrefab: {fileID: 100100000, guid: 00000000000000000000000000000001, type: 3}\n";
    let mut cyclic = HashMap::new();
    cyclic.insert(Guid(1), scan(looping.as_bytes()));
    assert!(resolve(&cyclic, Guid(1), 8).is_none());
}

#[test]
fn resolves_negative_file_ids_and_skips_removed_objects() {
    let base_guid = Guid(0x11);
    let inst_guid = Guid(0x22);
    let neg_id: i64 = -7493120387424395000;
    let base_yaml = format!(
        "%YAML 1.1\n\
         --- !u!1 &10\nGameObject:\n  m_Name: Root\n  m_Component:\n  - component: {{fileID: {neg_id}}}\n  - component: {{fileID: 20}}\n\
         --- !u!114 &{neg_id}\nMonoBehaviour:\n  m_GameObject: {{fileID: 10}}\n  speed: 4\n\
         --- !u!114 &20\nMonoBehaviour:\n  m_GameObject: {{fileID: 10}}\n  coins: 1\n\
         --- !u!1 &30\nGameObject:\n  m_Name: Child\n  m_Component:\n  - component: {{fileID: 31}}\n\
         --- !u!114 &31\nMonoBehaviour:\n  m_GameObject: {{fileID: 30}}\n  hp: 9\n"
    );
    let inst_id: i64 = 100;
    let inst_yaml = format!(
        "%YAML 1.1\n\
         --- !u!1001 &{inst_id}\nPrefabInstance:\n  m_Modification:\n    m_RemovedComponents:\n    - {{fileID: 20, guid: {base_guid}, type: 3}}\n    m_RemovedGameObjects:\n    - {{fileID: 30, guid: {base_guid}, type: 3}}\n  m_SourcePrefab: {{fileID: 100100000, guid: {base_guid}, type: 3}}\n"
    );
    let mut files = HashMap::new();
    files.insert(base_guid, scan(base_yaml.as_bytes()));
    files.insert(inst_guid, scan(inst_yaml.as_bytes()));

    let mapped_neg = inst_id ^ neg_id;
    let origin = resolve(&files, inst_guid, mapped_neg).expect("negative fileID should resolve");
    assert_eq!((origin.file, origin.object.file_id), (base_guid, neg_id));
    assert_eq!(property(&files, inst_guid, mapped_neg, "speed").and_then(|v| v.value).as_deref(), Some("4"));

    // Removed component and component belonging to a removed GameObject must not resolve.
    assert!(resolve(&files, inst_guid, inst_id ^ 20).is_none());
    assert!(resolve(&files, inst_guid, inst_id ^ 30).is_none());
    assert!(resolve(&files, inst_guid, inst_id ^ 31).is_none());
}
