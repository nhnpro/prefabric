//! Installing Prefabric's Unity package and generating project files with the real Unity editor
//! of the fixture's version (`fixtures/unity-sample`). `#[ignore]`d: it needs that Unity installed.
//!
//! cargo test -p ide-host --test unity_package -- --ignored

use ide_core::{IdeEvent, Pos};
use ide_host::{Host, HostConfig, ops};
use ide_lsp::types::HierarchyDirection;
use ide_workspace::unity::UnityPackageStatus;
use std::path::Path;
use std::sync::Arc;

const WINDOW: &str = "main";

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let name = entry.file_name();
        if matches!(name.to_str(), Some("Library" | "Temp" | "Logs" | "UserSettings")) {
            continue;
        }
        let target = to.join(&name);
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn host(tmp: &Path) -> Arc<Host> {
    let app = tmp.join("app");
    Arc::new(Host::new(HostConfig {
        config_dir: app.join("config"),
        data_dir: app.join("data"),
        local_data_dir: app.join("data"),
        languages: Arc::new(ide_language::LanguageRegistry::load(None).unwrap()),
        syntax: Arc::default(),
        sink_for: Box::new(|_| Arc::new(|_: IdeEvent| {})),
        owner: |w| w,
        runtime: tokio::runtime::Handle::current(),
        project_environment: false,
    }))
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the fixture's Unity version installed"]
async fn installs_the_package_and_generates_project_files() {
    let tmp = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(tmp.path()).unwrap().join("unity-sample");
    copy_dir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/unity-sample"), &root);
    for entry in std::fs::read_dir(&root).unwrap().flatten() {
        if matches!(entry.path().extension().and_then(|e| e.to_str()), Some("csproj" | "slnx" | "sln")) {
            std::fs::remove_file(entry.path()).unwrap();
        }
    }
    std::fs::write(
        root.join("Assets/Scripts/Character.cs"),
        "using UnityEngine;\n\nnamespace Game\n{\n    public abstract class Character : MonoBehaviour {}\n}\n",
    )
    .unwrap();
    std::fs::write(
        root.join("Assets/Scripts/Hero.cs"),
        "namespace Game\n{\n    public class Hero : Character\n    {\n    }\n}\n",
    )
    .unwrap();
    let host = host(tmp.path());
    host.trust.decide(&root, false, false).unwrap();
    host.open_project(WINDOW, &root).unwrap();

    let info = ops::project::current(&host, WINDOW).unwrap().unity.unwrap();
    assert_eq!(info.package, Some(UnityPackageStatus::Missing));
    // Generating needs the package, and runs project code: not in Restricted Mode.
    assert!(ops::unity::generate_project_files(&host, WINDOW).await.is_err());

    let info = ops::unity::install_package(&host, WINDOW).await.unwrap().unity.unwrap();
    assert_eq!(info.package, Some(UnityPackageStatus::Current));
    let Err(err) = ops::unity::generate_project_files(&host, WINDOW).await else {
        panic!("generated in Restricted Mode")
    };
    assert!(err.message.contains("Restricted Mode"), "{err:?}");

    host.set_project_trust(&root, true, false).await.unwrap();
    let started = std::time::Instant::now();
    let info = ops::unity::generate_project_files(&host, WINDOW).await.unwrap().unity.unwrap();
    eprintln!("generated in {:?}", started.elapsed());
    assert_eq!(info.solutions, ["unity-sample.slnx"]);
    assert!(info.projects.contains(&"Game.Runtime.csproj".to_owned()), "{:?}", info.projects);
    let csproj = std::fs::read_to_string(root.join("Game.Runtime.csproj")).unwrap();
    assert!(csproj.contains("Microsoft.Unity.Analyzers.dll"));
    assert!(csproj.contains("<LangVersion>9.0</LangVersion>"));

    // Unity message completion walks a script's bases declared in other files with Roslyn's type
    // hierarchy, which needs the generated projects (and Roslyn installed). Roslyn lists only
    // supertypes with source: `Hero : Character` names Character, `Character : MonoBehaviour` none.
    let supertypes = |file: &'static str, class: &'static str| {
        let host = host.clone();
        let root = root.clone();
        async move {
            let doc = ops::doc::open(&host, WINDOW, root.join(file)).await.unwrap().id;
            let text = ops::doc::snapshot(&host, WINDOW, doc).await.unwrap().text;
            let (line, content) =
                text.lines().enumerate().find(|(_, l)| l.contains(&format!("class {class}"))).unwrap();
            let pos = Pos { line: line as u32, col: content.find(class).unwrap() as u32 };
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(240);
            loop {
                let items = ops::lsp::lsp_type_hierarchy(&host, WINDOW, doc, pos, HierarchyDirection::Incoming)
                    .await
                    .unwrap_or_default();
                if !items.is_empty() || std::time::Instant::now() > deadline {
                    break items.into_iter().map(|i| i.name).collect::<Vec<_>>();
                }
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
        }
    };
    assert_eq!(supertypes("Assets/Scripts/Hero.cs", "Hero").await, ["Character"]);
}
