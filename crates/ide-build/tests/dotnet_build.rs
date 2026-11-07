//! `dotnet build` of a broken copy of `fixtures/csharp-app`: its compiler errors come back as
//! build messages at their source position. Needs a .NET SDK on PATH:
//! `cargo test -p ide-build --test dotnet_build -- --ignored`.

use ide_build::BuildModel;
use ide_build::compile::{self, BuildKind, BuildSystem};
use ide_core::{CancellationToken, Severity};
use std::path::{Path, PathBuf};

fn copy(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let name = e.file_name();
        if matches!(name.to_str(), Some("bin" | "obj")) {
            continue;
        }
        if e.file_type().unwrap().is_dir() {
            copy(&e.path(), &to.join(&name));
        } else {
            std::fs::copy(e.path(), to.join(&name)).unwrap();
        }
    }
}

#[test]
#[ignore = "needs a .NET SDK"]
fn compiler_errors_become_messages() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("csharp-app");
    copy(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/csharp-app"), &root);
    let file = root.join("src/Tasks.Core/TaskItem.cs");
    let text = std::fs::read_to_string(&file).unwrap();
    let broken = text.replace(
        "    public TaskItem Complete() => this with { Done = true };",
        "    public TaskItem Complete() => this with { Done = finished };",
    );
    assert_ne!(broken, text);
    std::fs::write(&file, broken).unwrap();

    let p = compile::plan(&root, &BuildModel::default(), BuildKind::Build, None, None).unwrap();
    assert_eq!(p.system, BuildSystem::DotNet);
    let result = compile::run(&root, &p, &CancellationToken::new(), &|_| {}).unwrap();
    assert!(!result.success);
    let errors: Vec<_> = result.messages.iter().filter(|m| m.severity == Severity::Error).collect();
    assert_eq!(errors.len(), 1, "{:#?}", result.messages);
    let e = errors[0];
    assert_eq!(e.code.as_deref(), Some("CS0103"));
    let path = PathBuf::from(e.path.clone().unwrap());
    assert_eq!(dunce::canonicalize(path).unwrap(), dunce::canonicalize(&file).unwrap());
    let range = e.range.unwrap();
    assert_eq!((range.start.line, range.start.col, range.end.col), (12, 53, 61));
    // The fixture's deliberate warning (CS0219 in TaskStore.cs), reported once.
    let warnings: Vec<_> = result.messages.iter().filter(|m| m.severity == Severity::Warning).collect();
    assert_eq!(warnings.iter().map(|w| w.code.as_deref()).collect::<Vec<_>>(), [Some("CS0219")]);
    assert_eq!(compile::diagnostics(&result.messages).len(), 2);
}
