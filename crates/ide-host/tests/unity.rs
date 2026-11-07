use ide_core::{NullSink, RequestId};
use ide_host::{Host, HostConfig, ops};
use std::{sync::Arc, time::Duration};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn opens_unity_without_generated_solutions_and_excludes_caches_from_the_index() {
    let temp = tempfile::tempdir().unwrap();
    let base = dunce::canonicalize(temp.path()).unwrap();
    let root = base.join("Game");
    for name in ["Assets/Scripts", "Packages", "ProjectSettings", "Library/PackageCache", "Temp", "Logs"] {
        std::fs::create_dir_all(root.join(name)).unwrap();
    }
    std::fs::write(root.join("ProjectSettings/ProjectVersion.txt"), "m_EditorVersion: 6000.0.62f1\n").unwrap();
    std::fs::write(root.join("Assets/Scripts/Player.cs"), "class Player {}\n").unwrap();
    std::fs::write(root.join("Packages/manifest.json"), "{}\n").unwrap();
    std::fs::write(root.join("Library/PackageCache/Cached.cs"), "class Cached {}\n").unwrap();
    std::fs::write(root.join("Temp/Temporary.cs"), "class Temporary {}\n").unwrap();
    std::fs::write(root.join("Logs/Editor.log"), "log\n").unwrap();
    let host = Host::new(HostConfig {
        config_dir: base.join("config"),
        data_dir: base.join("data"),
        local_data_dir: base.join("data"),
        languages: Arc::new(ide_language::LanguageRegistry::load(None).unwrap()),
        syntax: Arc::default(),
        sink_for: Box::new(|_| Arc::new(NullSink)),
        owner: |w| w,
        runtime: tokio::runtime::Handle::current(),
        project_environment: false,
    });
    let project = host.open_project("main", &root).unwrap();
    let info = ops::project::info(&project);
    assert_eq!(info.name, "Game");
    let unity = info.unity.unwrap();
    assert_eq!(unity.editor_version.as_deref(), Some("6000.0.62f1"));
    assert!(unity.projects.is_empty() && unity.solutions.is_empty());
    assert!(project.index.wait_idle(Duration::from_secs(20)));
    assert_eq!(project.index.find_files("Player.cs", 100).len(), 1);
    // The asset index starts after the code index, in its own store.
    let assets = project.unity.as_ref().expect("a Unity project has an asset index");
    assert!(assets.wait_idle(Duration::from_secs(20)));
    assert!(!ops::unity::index_status(&host, "main").unwrap().unwrap().indexing);
    assert!(base.join("data/unity-index").read_dir().unwrap().next().is_some());
    let listed = project.index.files_matching(&project.root, |rel| rel.ends_with("Player.cs"), 10).unwrap();
    assert_eq!(listed.len(), 1);
    for name in ["Cached.cs", "Temporary.cs", "Editor.log"] {
        assert!(project.index.find_files(name, 100).is_empty(), "indexed generated file {name}");
    }
    let hits = Arc::new(std::sync::Mutex::new(Vec::new()));
    let out = hits.clone();
    let query = serde_json::from_value(serde_json::json!({
        "pattern": "class", "isRegex": false, "caseSensitive": false, "wholeWord": false,
        "include": [], "exclude": [], "scope": { "kind": "project" }, "maxMatches": 100,
    }))
    .unwrap();
    ops::search::start(
        &host,
        "main",
        RequestId::next(),
        query,
        Arc::new(move |event| {
            if let ops::search::SearchEvent::File { file } = event {
                out.lock().unwrap().push(file.rel_path);
            }
        }),
    )
    .await
    .unwrap();
    assert_eq!(*hits.lock().unwrap(), ["Assets/Scripts/Player.cs"]);
    let listing = ops::fs::read_dir(&host, "main", root.clone()).await.unwrap();
    assert!(listing.iter().any(|e| e.name == "Library" && e.excluded));
    assert!(project.exclude_globs().contains(&"/Obj".to_owned()));

    // Project metadata refreshes without reopening when Unity generates its solution.
    std::fs::write(root.join("Game.sln"), "").unwrap();
    std::fs::write(root.join("Assembly-CSharp.csproj"), "").unwrap();
    let unity = ops::project::info(&project).unity.unwrap();
    assert_eq!(unity.solutions, ["Game.sln"]);
    assert_eq!(unity.projects, ["Assembly-CSharp.csproj"]);

    // Desktop commands and remote RPC share these host operations.
    let original = root.join("Assets/Scripts/Player.cs");
    let renamed = root.join("Assets/Scripts/Hero.cs");
    let copied = root.join("Assets/Scripts/HeroCopy.cs");
    let meta = |path: &std::path::Path| -> std::path::PathBuf {
        let mut name = path.as_os_str().to_os_string();
        name.push(".meta");
        name.into()
    };
    let original_metadata = "fileFormatVersion: 2\nguid: 11111111111111111111111111111111\n";
    std::fs::write(meta(&original), original_metadata).unwrap();
    ops::fs::rename(&host, original.clone(), renamed.clone()).await.unwrap();
    assert!(!original.exists() && !meta(&original).exists());
    assert_eq!(std::fs::read_to_string(meta(&renamed)).unwrap(), original_metadata);
    ops::fs::copy(&host, renamed.clone(), copied.clone()).await.unwrap();
    assert!(copied.exists());
    assert_ne!(std::fs::read_to_string(meta(&copied)).unwrap(), original_metadata);
    assert_eq!(std::fs::read_to_string(meta(&renamed)).unwrap(), original_metadata);
    host.close_project("main");

    let ordinary = base.join("ConsoleApp");
    std::fs::create_dir(&ordinary).unwrap();
    std::fs::write(ordinary.join("App.csproj"), "").unwrap();
    let project = host.open_project("main", &ordinary).unwrap();
    assert!(ops::project::info(&project).unity.is_none());
    assert!(!project.exclude_globs().contains(&"/Library".to_owned()));
    host.close_project("main");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn other_projects_have_no_asset_index() {
    let temp = tempfile::tempdir().unwrap();
    let base = dunce::canonicalize(temp.path()).unwrap();
    let root = base.join("App");
    std::fs::create_dir_all(root.join("Assets")).unwrap();
    std::fs::write(root.join("App.csproj"), "<Project />\n").unwrap();
    let host = Host::new(HostConfig {
        config_dir: base.join("config"),
        data_dir: base.join("data"),
        local_data_dir: base.join("data"),
        languages: Arc::new(ide_language::LanguageRegistry::load(None).unwrap()),
        syntax: Arc::default(),
        sink_for: Box::new(|_| Arc::new(NullSink)),
        owner: |w| w,
        runtime: tokio::runtime::Handle::current(),
        project_environment: false,
    });
    let project = host.open_project("main", &root).unwrap();
    assert!(project.unity.is_none());
    assert_eq!(ops::unity::index_status(&host, "main").unwrap(), None);
    assert!(!base.join("data/unity-index").exists());
}

fn copy_dir(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn finds_asset_usages_of_the_symbol_at_the_caret() {
    use ide_core::Pos;
    use ide_unity::usages::AssetUsageKind;
    let temp = tempfile::tempdir().unwrap();
    let base = dunce::canonicalize(temp.path()).unwrap();
    let root = base.join("Game");
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/unity-sample");
    for dir in ["Assets", "ProjectSettings", "Packages"] {
        copy_dir(&fixture.join(dir), &root.join(dir));
    }
    let host = Host::new(HostConfig {
        config_dir: base.join("config"),
        data_dir: base.join("data"),
        local_data_dir: base.join("data"),
        languages: Arc::new(ide_language::LanguageRegistry::load(None).unwrap()),
        syntax: Arc::new(std::sync::OnceLock::from(Arc::new(ide_language::Syntax::new()))),
        sink_for: Box::new(|_| Arc::new(NullSink)),
        owner: |w| w,
        runtime: tokio::runtime::Handle::current(),
        project_environment: false,
    });
    let project = host.open_project("main", &root).unwrap();
    assert!(project.unity.as_ref().unwrap().wait_idle(Duration::from_secs(30)));
    let script = root.join("Assets/Scripts/Player.cs");
    let text = std::fs::read_to_string(&script).unwrap();
    let doc = ops::doc::open(&host, "main", script).await.unwrap().id;
    let at = |needle: &str| {
        let line = text.lines().position(|l| l.contains(needle)).unwrap();
        let col = text.lines().nth(line).unwrap().find(needle).unwrap();
        Pos::new(line as u32, col as u32 + 1)
    };
    let usages = |pos| {
        let host = &host;
        async move {
            ops::unity::asset_usages(host, "main", doc, pos)
                .await
                .unwrap()
                .unwrap_or_else(|| panic!("no symbol at {pos:?}"))
        }
    };
    let rel = |u: &ide_unity::usages::AssetUsage| u.path.strip_prefix(&root).unwrap().to_string_lossy().into_owned();

    let class = usages(Pos::new(at("class Player").line, at("class Player").col + 6)).await;
    assert_eq!(class.symbol, "Player");
    let files: Vec<String> = class.usages.iter().map(rel).collect();
    for f in ["Assets/Prefabs/Player.prefab", "Assets/Scenes/Main.unity"] {
        assert!(files.iter().any(|x| x == f), "{f} in {files:?}");
    }
    assert!(class.usages.iter().all(|u| u.usage == AssetUsageKind::Component));
    let follower = class.usages.iter().find(|u| rel(u) == "Assets/Prefabs/Squad.prefab").unwrap();
    assert_eq!(
        (follower.game_object.join("/"), follower.through.as_deref()),
        ("Squad/Follower".into(), Some("Assets/Prefabs/Player.prefab"))
    );

    let handler = usages(at("OnTriggerZoneEntered")).await;
    assert!(handler.usages.iter().any(|u| matches!(&u.usage, AssetUsageKind::UnityEvent { .. })), "{handler:#?}");
    let footstep = usages(at("OnFootstep")).await;
    assert!(
        footstep
            .usages
            .iter()
            .any(|u| matches!(&u.usage, AssetUsageKind::AnimationEvent { .. })
                && rel(u) == "Assets/Animations/Walk.anim"),
        "{footstep:#?}"
    );
    let speed = usages(at("speed = 2f")).await;
    assert_eq!(speed.symbol, "speed");
    let set: Vec<(String, Option<&str>, bool)> = speed
        .usages
        .iter()
        .map(|u| match &u.usage {
            AssetUsageKind::FieldValue { value, overridden } => (rel(u), value.as_deref(), *overridden),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(
        set,
        [
            ("Assets/Prefabs/Player Variant.prefab".to_owned(), Some("5"), true),
            ("Assets/Prefabs/Player.prefab".to_owned(), Some("2"), false),
            ("Assets/Scenes/Main.unity".to_owned(), Some("7"), true),
        ]
    );
    // A local variable isn't a symbol assets use.
    assert!(ops::unity::asset_usages(&host, "main", doc, at("var unused")).await.unwrap().is_none());
    host.close_project("main");
}
