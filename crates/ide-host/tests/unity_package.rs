//! Installing Prefabric's Unity package and generating project files with the real Unity editor
//! of the fixture's version (`fixtures/unity-sample`), then the C#, UXML and `.asmdef` support on
//! the result. `#[ignore]`d: it needs that Unity installed, Roslyn and lemminx.
//!
//! cargo test -p ide-host --test unity_package -- --ignored

use ide_core::{FileEdit, IdeEvent, Pos, Range, RequestId, TextEdit};
use ide_host::{Host, HostConfig, ops};
use ide_lsp::types::HierarchyDirection;
use ide_workspace::unity::UnityPackageStatus;
use std::path::{Path, PathBuf};
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
    assert!(ops::unity::generate_project_files(&host, WINDOW, RequestId::next()).await.is_err());

    let info = ops::unity::install_package(&host, WINDOW).await.unwrap().unity.unwrap();
    assert_eq!(info.package, Some(UnityPackageStatus::Current));
    let Err(err) = ops::unity::generate_project_files(&host, WINDOW, RequestId::next()).await else {
        panic!("generated in Restricted Mode")
    };
    assert!(err.message.contains("Restricted Mode"), "{err:?}");

    host.set_project_trust(&root, true, false).await.unwrap();
    let started = std::time::Instant::now();
    let info = ops::unity::generate_project_files(&host, WINDOW, RequestId::next()).await.unwrap().unity.unwrap();
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

    // UXML: generation writes Unity's schema, which lemminx completes elements from (associated
    // by pattern: UI Builder writes `noNamespaceSchemaLocation` without its `xsi:` prefix).
    assert!(root.join("UIElementsSchema/UnityEngine.UIElements.xsd").is_file());
    let uxml = root.join("Assets/UI/Menu.uxml");
    std::fs::create_dir_all(uxml.parent().unwrap()).unwrap();
    std::fs::write(
        &uxml,
        "<ui:UXML xmlns:ui=\"UnityEngine.UIElements\" xsi=\"http://www.w3.org/2001/XMLSchema-instance\" \
         noNamespaceSchemaLocation=\"../../UIElementsSchema/UIElements.xsd\">\n    <ui:\n</ui:UXML>\n",
    )
    .unwrap();
    let doc = ops::doc::open(&host, WINDOW, uxml.clone()).await.unwrap().id;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
    loop {
        let items = ops::lsp::lsp_completion(&host, WINDOW, RequestId(78), doc, Pos { line: 1, col: 8 }, None)
            .await
            .map(|l| l.items)
            .unwrap_or_default();
        if items.iter().any(|i| i.label == "ui:Button" || i.label == "Button") {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "no UXML element completion: {:?}", items.first());
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
    ops::doc::close(&host, WINDOW, doc).unwrap();
    std::fs::remove_file(&uxml).unwrap();

    // `.asmdef` navigation: definitions of the project and of the packages Unity resolved.
    let definitions = ops::unity::assembly_definitions(&host, WINDOW).await.unwrap();
    let runtime = definitions.iter().find(|d| d.name == "Game.Runtime").unwrap();
    let meta = std::fs::read_to_string(root.join("Assets/Scripts/Game.Runtime.asmdef.meta")).unwrap();
    assert!(meta.contains(&format!("guid: {}", runtime.guid.as_deref().unwrap())));
    let test_runner = definitions.iter().find(|d| d.name == "UnityEngine.TestRunner").unwrap();
    assert!(test_runner.path.starts_with(root.join("Library/PackageCache")), "{test_runner:?}");

    // Unity-aware rename (what the editor's Unity rename extensions add to Roslyn's edits): the
    // serialized `speed` becomes `velocity` with `[FormerlySerializedAs("speed")]`, then the
    // `Player` MonoBehaviour becomes `Runner` with its file. The script keeps its GUID, and Unity
    // still finds the component and its saved value in the prefab.
    let player = root.join("Assets/Scripts/Player.cs");
    let guid = |path: &Path| {
        let mut meta = path.as_os_str().to_os_string();
        meta.push(".meta");
        std::fs::read_to_string(meta).unwrap().lines().find(|l| l.starts_with("guid:")).unwrap().to_owned()
    };
    let original_guid = guid(&player);
    let prefab = root.join("Assets/Prefabs/Player.prefab");
    let saved = std::fs::read_to_string(&prefab).unwrap();
    assert!(saved.contains("  speed: 2\n"));
    std::fs::write(&prefab, saved.replace("  speed: 2\n", "  speed: 7\n")).unwrap();

    let rename = |file: PathBuf, find: &'static str, new_name: &'static str| {
        let host = host.clone();
        async move {
            let doc = ops::doc::open(&host, WINDOW, file).await.unwrap().id;
            let text = ops::doc::snapshot(&host, WINDOW, doc).await.unwrap().text;
            let (line, content) = text.lines().enumerate().find(|(_, l)| l.contains(find)).unwrap();
            let pos = Pos { line: line as u32, col: content.find(find).unwrap() as u32 };
            let renamed =
                ops::lsp::lsp_rename(&host, WINDOW, RequestId(77), doc, pos, new_name.to_owned()).await.unwrap();
            (renamed.edit, line as u32)
        }
    };
    let (mut edit, field_line) = rename(player.clone(), "speed = 2f", "velocity").await;
    let attribute = |line: u32, text: &str| TextEdit {
        range: Range { start: Pos { line, col: 0 }, end: Pos { line, col: 0 } },
        new_text: text.to_owned(),
    };
    let Some(FileEdit::Edit { edits, .. }) =
        edit.files.iter_mut().find(|f| matches!(f, FileEdit::Edit { path, .. } if path.ends_with("Player.cs")))
    else {
        panic!("no edit of Player.cs: {:?}", edit.files)
    };
    edits.push(attribute(field_line, "        [FormerlySerializedAs(\"speed\")]\n"));
    edits.push(attribute(1, "using UnityEngine.Serialization;\n"));
    ops::lsp::apply_workspace_edit(&host, WINDOW, edit).await.unwrap();

    let (mut edit, _) = rename(player.clone(), "Player : MonoBehaviour", "Runner").await;
    let runner = root.join("Assets/Scripts/Runner.cs");
    edit.files.push(FileEdit::Rename { from: player.clone().into(), to: runner.clone().into() });
    ops::lsp::apply_workspace_edit(&host, WINDOW, edit).await.unwrap();
    assert!(!player.exists());
    assert_eq!(guid(&runner), original_guid);
    assert!(std::fs::read_to_string(&prefab).unwrap().contains(original_guid.trim_start_matches("guid: ")));
    let source = std::fs::read_to_string(&runner).unwrap();
    assert!(source.contains("public class Runner : MonoBehaviour"), "{source}");
    assert!(
        source.contains("[FormerlySerializedAs(\"speed\")]\n        [SerializeField] private float velocity"),
        "{source}"
    );
    host.close_project(WINDOW);

    std::fs::write(
        root.join("Assets/Editor/RenameCheck.cs"),
        r#"using System.IO;
using UnityEditor;
using UnityEngine;

public static class RenameCheck
{
    public static void Run()
    {
        var prefab = AssetDatabase.LoadAssetAtPath<GameObject>("Assets/Prefabs/Player.prefab");
        var runner = prefab.GetComponent("Runner") as MonoBehaviour;
        var velocity = runner == null ? null : new SerializedObject(runner).FindProperty("velocity");
        File.WriteAllText("rename-check.txt", runner == null ? "no component" : velocity == null ? "no field" : runner.GetType().FullName + " " + velocity.floatValue);
    }
}
"#,
    )
    .unwrap();
    let version = ops::project::current(&host, WINDOW).and_then(|p| p.unity).and_then(|u| u.editor_version);
    let version = version.unwrap_or_else(|| "6000.6.8f1".to_owned());
    let editor = ide_workspace::unity::editor::find(&version).unwrap();
    let status = std::process::Command::new(editor)
        .args(["-batchmode", "-quit", "-nographics", "-projectPath"])
        .arg(&root)
        .args(["-executeMethod", "RenameCheck.Run", "-logFile"])
        .arg(tmp.path().join("rename-check.log"))
        .status()
        .unwrap();
    let log = std::fs::read_to_string(tmp.path().join("rename-check.log")).unwrap_or_default();
    assert!(status.success(), "{}", log.lines().filter(|l| l.contains("error")).collect::<Vec<_>>().join("\n"));
    assert_eq!(std::fs::read_to_string(root.join("rename-check.txt")).unwrap(), "Game.Runner 7");
}
