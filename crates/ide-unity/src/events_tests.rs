use super::*;
use crate::prefab::resolve;
use crate::scan::{Guid, ScannedFile, scan};
use std::collections::HashMap;

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/unity-sample/Assets");

fn guid(hex: &str) -> Guid {
    Guid::parse(hex.as_bytes()).unwrap()
}

fn load(rels: &[&str]) -> HashMap<Guid, ScannedFile> {
    rels.iter()
        .map(|rel| {
            let meta = std::fs::read_to_string(format!("{FIXTURE}/{rel}.meta")).unwrap();
            let hex = meta.lines().find_map(|l| l.strip_prefix("guid: ")).unwrap();
            (guid(hex), scan(&std::fs::read(format!("{FIXTURE}/{rel}")).unwrap()))
        })
        .collect()
}

#[test]
fn finds_the_scene_s_unityevent_binding() {
    let assets = load(&["Scenes/Main.unity", "Prefabs/Player Variant.prefab", "Prefabs/Player.prefab"]);
    let scene = guid("785ab778687f74d0285516dd608851ca");
    let trigger = assets[&scene].object(951019560).unwrap();
    assert_eq!(trigger.script.unwrap().guid, Some(guid("04b2d0a4ed19d4d44b32547d2728f356")));
    let [call] = trigger.calls.as_slice() else { panic!("{:?}", trigger.calls) };
    assert_eq!(
        (&*call.field, call.target_type.as_deref(), &*call.method, call.mode, call.call_state, call.line),
        ("onEnter", Some("Game.Player, Game.Runtime"), "OnTriggerZoneEntered", 1, 2, 303)
    );
    // The target is the Hero's Player behaviour: Player.cs (through the variant).
    let target = call.target.unwrap();
    assert_eq!((target.file_id, target.guid), (1133812325, None));
    let origin = resolve(&assets, scene, target.file_id).unwrap();
    assert_eq!(origin.object.script.unwrap().guid, Some(guid("f8dbec04297b54fd18c802467ac277ea")));
    // No other object of the fixture has listeners.
    let listeners: usize = assets.values().flat_map(|f| &f.objects).map(|o| o.calls.len()).sum();
    assert_eq!(listeners, 1);
}

#[test]
fn finds_animation_events_and_the_clip_s_animators() {
    let assets = load(&["Animations/Walk.anim", "Animations/Player.controller", "Prefabs/Player.prefab"]);
    let clip_guid = guid("0ba21d426251e43d1967f952f727bc4f");
    let clip = &assets[&clip_guid].objects[0];
    assert_eq!((clip.class_id, clip.name.as_deref()), (74, Some("Walk")));
    let events: Vec<_> = clip.animation_events.iter().map(|e| (&*e.time, &*e.function, e.line)).collect();
    assert_eq!(events, [("0.25", "OnFootstep", 88)]);
    // Clip ← controller state ← the prefab's Animator.
    let controller = guid("ad14f8745eb414a6b8990304d59c866b");
    let state = assets[&controller].objects.iter().find(|o| o.motion.is_some()).unwrap();
    assert_eq!(state.motion.unwrap().guid, Some(clip_guid));
    let player = &assets[&guid("79cb0651700894f0a8e50458c76fa759")];
    let animator = player.objects.iter().find(|o| o.controller.is_some()).unwrap();
    assert_eq!(
        (animator.controller.unwrap().guid, animator.game_object),
        (Some(controller), Some(1354967268023247473))
    );
}

#[test]
fn reads_nested_and_listed_events() {
    let text = "--- !u!114 &1\nMonoBehaviour:\n  m_Script: {fileID: 11500000, guid: 00000000000000000000000000000001, type: 3}\n  settings:\n    onDone:\n      m_PersistentCalls:\n        m_Calls:\n        - m_Target: {fileID: 5}\n          m_MethodName: Finish\n          m_Arguments:\n            m_StringArgument: m_MethodName\n          m_CallState: 2\n        - m_Target: {fileID: 6}\n          m_MethodName: Log\n  buttons:\n  - onClick:\n      m_PersistentCalls:\n        m_Calls: []\n  - onClick:\n      m_PersistentCalls:\n        m_Calls:\n        - m_Target: {fileID: 0}\n          m_MethodName: Missing\n  after: 1\n";
    let file = scan(text.as_bytes());
    let calls: Vec<_> = file.objects[0]
        .calls
        .iter()
        .map(|c| (&*c.field, c.target.map(|t| t.file_id), &*c.method, c.call_state))
        .collect();
    assert_eq!(
        calls,
        [
            ("settings.onDone", Some(5), "Finish", 2),
            ("settings.onDone", Some(6), "Log", 0),
            ("buttons.onClick", None, "Missing", 0),
        ]
    );
    assert_eq!(file.objects[0].properties, [("after".into(), "1".into())]);
}

#[test]
fn reads_listeners_set_by_overrides() {
    let text = "--- !u!1001 &9\nPrefabInstance:\n  m_Modification:\n    m_Modifications:\n    - target: {fileID: 42, guid: 00000000000000000000000000000002, type: 3}\n      propertyPath: onEnter.m_PersistentCalls.m_Calls.Array.data[0].m_MethodName\n      value: Open\n      objectReference: {fileID: 0}\n    - target: {fileID: 42, guid: 00000000000000000000000000000002, type: 3}\n      propertyPath: onEnter.m_PersistentCalls.m_Calls.Array.data[0].m_Target\n      value: \n      objectReference: {fileID: 77}\n    - target: {fileID: 42, guid: 00000000000000000000000000000002, type: 3}\n      propertyPath: onEnter.m_PersistentCalls.m_Calls.Array.data[1].m_CallState\n      value: 0\n      objectReference: {fileID: 0}\n    - target: {fileID: 42, guid: 00000000000000000000000000000002, type: 3}\n      propertyPath: speed\n      value: 3\n      objectReference: {fileID: 0}\n  m_SourcePrefab: {fileID: 100100000, guid: 00000000000000000000000000000002, type: 3}\n";
    let file = scan(text.as_bytes());
    let calls = override_calls(&file.objects[0]);
    let [(target, call)] = calls.as_slice() else { panic!("{calls:?}") };
    assert_eq!(target.file_id, 42);
    assert_eq!(
        (&*call.field, &*call.method, call.target.map(|t| t.file_id), call.line),
        ("onEnter", "Open", Some(77), 4)
    );
}
