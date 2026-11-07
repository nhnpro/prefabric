use super::*;

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/unity-sample/Assets");

fn fixture(rel: &str) -> Vec<u8> {
    std::fs::read(format!("{FIXTURE}/{rel}")).unwrap()
}

fn object(file: &ScannedFile, file_id: i64) -> &Object {
    file.objects.iter().find(|o| o.file_id == file_id).unwrap()
}

#[test]
fn reads_a_prefab() {
    let file = scan(&fixture("Prefabs/Player.prefab"));
    assert_eq!(file.objects.len(), 9);
    let player = object(&file, 1354967268023247473);
    assert_eq!((player.class_id, &*player.type_name, player.line), (1, "GameObject", 2));
    assert_eq!(player.name.as_deref(), Some("Player"));
    assert_eq!(player.components, [2647960798605896529, 6979874606233519545, 7936279571911340110, 8815640413858057345]);
    let transform = object(&file, 2647960798605896529);
    assert_eq!(transform.game_object, Some(1354967268023247473));
    assert_eq!((transform.father, &transform.children[..]), (None, &[3771186766817673773][..]));
    let child = object(&file, 3771186766817673773);
    assert_eq!((child.father, child.children.len()), (Some(2647960798605896529), 0));
    let behaviour = object(&file, 8815640413858057345);
    assert_eq!(&*behaviour.type_name, "MonoBehaviour");
    assert_eq!(behaviour.name, None);
    assert_eq!(
        behaviour.script,
        Some(ObjRef { file_id: 11500000, guid: Some(Guid::parse(b"f8dbec04297b54fd18c802467ac277ea").unwrap()) })
    );
    assert_eq!(behaviour.game_object, Some(1354967268023247473));
    assert!(file.objects.iter().all(|o| !o.stripped && o.prefab_instance.is_none() && o.source.is_none()));
}

#[test]
fn reads_stripped_objects_of_a_prefab_instance() {
    let file = scan(&fixture("Scenes/Main.unity"));
    let stripped: Vec<_> = file.objects.iter().filter(|o| o.stripped).collect();
    assert!(!stripped.is_empty());
    for o in &stripped {
        let instance = object(&file, o.prefab_instance.unwrap());
        assert_eq!((instance.class_id, &*instance.type_name), (1001, "PrefabInstance"));
        assert!(o.source.unwrap().guid.is_some());
    }
    let settings = object(&file, 1);
    assert_eq!(&*settings.type_name, "OcclusionCullingSettings");
}

#[test]
fn every_fixture_asset_scans() {
    for rel in ["Prefabs/Player Variant.prefab", "Prefabs/Squad.prefab", "Scenes/Main.unity"] {
        let file = scan(&fixture(rel));
        assert!(!file.objects.is_empty(), "{rel}");
        assert!(file.objects.iter().all(|o| !o.type_name.is_empty()), "{rel}");
    }
}

#[test]
fn handles_line_endings_quoting_and_negative_ids() {
    let text = "%YAML 1.1\r\n--- !u!1 &-42\r\nGameObject:\r\n  m_Name: 'It''s \"here\"'\r\n  m_Component:\r\n  - component: {fileID: -7}\r\n--- !u!1 &3\nGameObject:\n  m_Name: \"Caf\\u00e9 \\\"x\\\"\"\n--- !u!1 &4\nGameObject:\n  m_Name:   plain name  \n";
    let file = scan(text.as_bytes());
    assert_eq!(file.objects.len(), 3);
    assert_eq!(file.objects[0].file_id, -42);
    assert_eq!(file.objects[0].name.as_deref(), Some("It's \"here\""));
    assert_eq!(file.objects[0].components, [-7]);
    assert_eq!(file.objects[1].name.as_deref(), Some("Café \"x\""));
    assert_eq!(file.objects[2].name.as_deref(), Some("plain name"));
    assert_eq!(file.objects[2].line, 9);
}

#[test]
fn ignores_nested_properties_and_other_lists() {
    let text = "--- !u!114 &1\nMonoBehaviour:\n  m_Children:\n  - {fileID: 5}\n  items:\n  - {fileID: 6}\n  nested:\n    m_Name: inner\n    m_Script: {fileID: 1, guid: 00000000000000000000000000000001, type: 3}\n  m_Name: outer\n";
    let file = scan(text.as_bytes());
    let o = &file.objects[0];
    assert_eq!((o.children.as_slice(), o.name.as_deref(), o.script), (&[5][..], Some("outer"), None));
}

#[test]
fn references() {
    assert_eq!(reference(b"{fileID: 0}"), None);
    assert_eq!(reference(b"{fileID: 12}"), Some(ObjRef { file_id: 12, guid: None }));
    assert_eq!(
        reference(b"{fileID: 11500000, guid: f8dbec04297b54fd18c802467ac277ea, type: 3}")
            .unwrap()
            .guid
            .unwrap()
            .to_string(),
        "f8dbec04297b54fd18c802467ac277ea"
    );
    assert_eq!(reference(b"{fileID: 3, guid: nothex, type: 3}"), Some(ObjRef { file_id: 3, guid: None }));
    for bad in [&b""[..], b"{", b"}", b"{fileID:}", b"{fileID: -}", b"fileID: 3", b"{fileID: 99999999999999999999}"] {
        assert_eq!(reference(bad), None, "{}", String::from_utf8_lossy(bad));
    }
}

#[test]
fn headers_that_are_not_objects_are_skipped() {
    for text in
        ["--- !u!\n", "--- !u!x &1\n", "--- !u!1 &\n", "--- !u!1&1\n", "--- !u!-1 &1\n", "--- !u!99999999999 &1\n"]
    {
        assert!(scan(text.as_bytes()).objects.is_empty(), "{text:?}");
    }
}

/// xorshift: deterministic, so a failure reproduces.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

/// Mutated real assets and random bytes never panic the scanner (the `fuzz/` target does the same
/// with libFuzzer).
#[test]
fn never_panics_on_mutated_input() {
    let seeds: Vec<Vec<u8>> = ["Prefabs/Player.prefab", "Prefabs/Player Variant.prefab", "Scenes/Main.unity"]
        .iter()
        .map(|r| fixture(r))
        .collect();
    let tokens: [&[u8]; 8] = [b"--- !u!", b"\n", b"  ", b"- ", b"{fileID: ", b"guid: ", b"'", b"\xff\xfe"];
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for _ in 0..3000 {
        let mut input = seeds[rng.below(seeds.len())].clone();
        for _ in 0..1 + rng.below(16) {
            let at = rng.below(input.len() + 1);
            match rng.below(4) {
                0 if at < input.len() => input[at] = rng.next() as u8,
                1 => input.splice(at..at, tokens[rng.below(tokens.len())].iter().copied()).for_each(drop),
                2 => input.truncate(at),
                _ => {
                    let end = (at + rng.below(64)).min(input.len());
                    input.drain(at..end);
                }
            }
        }
        scan(&input);
    }
    for len in [0, 1, 7, 64, 4096] {
        let bytes: Vec<u8> = (0..len).map(|_| rng.next() as u8).collect();
        scan(&bytes);
    }
}
