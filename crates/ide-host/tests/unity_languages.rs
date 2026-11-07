//! Unity's JSON, CSS and YAML dialects with their language servers: assembly definitions are
//! validated with a bundled schema; USS gets no vendor-prefix warnings for `-unity-*`
//! properties; Unity's YAML assets get no "unresolved tag" errors. `#[ignore]`d: they need
//! `npm i -g vscode-langservers-extracted yaml-language-server`.
//!
//! cargo test -p ide-host --test unity_languages -- --ignored

use ide_core::IdeEvent;
use ide_host::{Host, HostConfig, ops};
use std::sync::{Arc, Mutex};

/// Opens `file` (written with `text`) in a trusted project at `root` and returns the messages of
/// its first non-empty diagnostics, or none after `wait` when it stays clean.
async fn diagnostics(root: &std::path::Path, file: &str, text: &str, wait: std::time::Duration) -> Vec<String> {
    let tmp = tempfile::tempdir().unwrap();
    let file = root.join(file);
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, text).unwrap();
    let events: Arc<Mutex<Vec<IdeEvent>>> = Arc::default();
    let app = tmp.path().join("app");
    let host = Arc::new(Host::new(HostConfig {
        config_dir: app.join("config"),
        data_dir: app.join("data"),
        local_data_dir: app.join("data"),
        languages: Arc::new(ide_language::LanguageRegistry::load(None).unwrap()),
        syntax: Arc::default(),
        sink_for: Box::new({
            let events = events.clone();
            move |_| {
                let events = events.clone();
                Arc::new(move |e: IdeEvent| events.lock().unwrap().push(e))
            }
        }),
        owner: |w| w,
        runtime: tokio::runtime::Handle::current(),
        project_environment: false,
    }));
    host.trust.decide(root, true, false).unwrap();
    host.open_project("main", root).unwrap();
    ops::doc::open(&host, "main", file).await.unwrap();
    let deadline = std::time::Instant::now() + wait;
    loop {
        let latest = events.lock().unwrap().iter().rev().find_map(|e| match e {
            IdeEvent::Diagnostics { items, .. } if !items.is_empty() => {
                Some(items.iter().map(|d| d.message.clone()).collect::<Vec<_>>())
            }
            _ => None,
        });
        if let Some(messages) = latest {
            return messages;
        }
        if std::time::Instant::now() > deadline {
            return Vec::new();
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs vscode-json-language-server"]
async fn asmdef_files_are_validated_with_the_unity_schema() {
    let tmp = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(tmp.path()).unwrap().join("game");
    let text = "{\n  \"rootNamespace\": \"Game\",\n  \"allowUnsafeCode\": \"yes\"\n}\n";
    let messages = diagnostics(&root, "Assets/Broken.asmdef", text, std::time::Duration::from_secs(60)).await;
    assert_eq!(messages, ["Missing property \"name\".", "Incorrect type. Expected \"boolean\"."]);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs vscode-css-language-server"]
async fn uss_files_keep_css_checks_without_vendor_prefix_warnings() {
    let tmp = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(tmp.path()).unwrap().join("game");
    let text = ".title {\n  -unity-font-style: bold;\n  -unity-text-align: middle-center;\n  colr: red;\n}\n";
    let messages = diagnostics(&root, "Assets/Menu.uss", text, std::time::Duration::from_secs(60)).await;
    assert_eq!(messages, ["Unknown property: 'colr'"]);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs yaml-language-server"]
async fn unity_yaml_assets_have_no_unresolved_tag_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(tmp.path()).unwrap().join("game");
    let prefab = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/unity-sample/Assets/Prefabs/Player.prefab"
    ))
    .unwrap();
    let quiet = diagnostics(&root, "Assets/Player.prefab", &prefab, std::time::Duration::from_secs(15)).await;
    assert_eq!(quiet, Vec::<String>::new());
    // Other YAML keeps its checks.
    let other = diagnostics(&root, "config.yaml", "a: [1\n", std::time::Duration::from_secs(60)).await;
    assert!(!other.is_empty());
}
