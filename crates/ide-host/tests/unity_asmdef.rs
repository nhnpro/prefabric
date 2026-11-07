//! Unity assembly definitions are validated with a bundled JSON schema by the JSON language
//! server. `#[ignore]`d: it needs `vscode-json-language-server` (`npm i -g vscode-langservers-extracted`).
//!
//! cargo test -p ide-host --test unity_asmdef -- --ignored

use ide_core::IdeEvent;
use ide_host::{Host, HostConfig, ops};
use std::sync::{Arc, Mutex};

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs vscode-json-language-server"]
async fn asmdef_files_are_validated_with_the_unity_schema() {
    let tmp = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(tmp.path()).unwrap().join("game");
    std::fs::create_dir_all(root.join("Assets")).unwrap();
    let file = root.join("Assets/Broken.asmdef");
    std::fs::write(&file, "{\n  \"rootNamespace\": \"Game\",\n  \"allowUnsafeCode\": \"yes\"\n}\n").unwrap();
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
    host.trust.decide(&root, true, false).unwrap();
    host.open_project("main", &root).unwrap();
    let _doc = ops::doc::open(&host, "main", file.clone()).await.unwrap().id;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
    let messages = loop {
        let latest = events.lock().unwrap().iter().rev().find_map(|e| match e {
            IdeEvent::Diagnostics { items, .. } if !items.is_empty() => {
                Some(items.iter().map(|d| d.message.clone()).collect::<Vec<_>>())
            }
            _ => None,
        });
        if let Some(messages) = latest {
            break messages;
        }
        assert!(std::time::Instant::now() < deadline, "no diagnostics");
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    };
    assert_eq!(messages, ["Missing property \"name\".", "Incorrect type. Expected \"boolean\"."]);
}
