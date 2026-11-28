//! Unity test configurations: one per test assembly of the project (`.asmdef` referencing the Test
//! Framework), run in the connected Editor through the link instead of a process (E7).

use super::Emit;
use crate::Host;
use ide_core::{IdeError, IdeEvent, IdeResult, RunId, UnityLinkConnection};
use ide_run::{OutputChunk, RunConfig, RunInfo};
use ide_unity::link::{TestRun, test_console_text, test_exit_code};
use parking_lot::Mutex;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::OnceLock;

pub const KIND: &str = "unity-test";

/// The Test Framework's assemblies, by name and by the GUID an `.asmdef` may reference them with.
const TEST_RUNNER_REFERENCES: [&str; 4] = [
    "UnityEngine.TestRunner",
    "UnityEditor.TestRunner",
    "GUID:27619889b8ba8c24980f49ee34dbb44a",
    "GUID:0acc523941302664db1f4e527237feb3",
];

/// Replaces the `dotnet` configurations of the C# projects Unity generates (they can't run outside
/// the Editor) with a configuration per test assembly in `Assets/` and `Packages/`.
pub(super) fn adjust_detected(root: &Path, configs: &mut Vec<RunConfig>) {
    configs.retain(|c| {
        !(matches!(c.kind.as_str(), "dotnet" | "dotnet-test")
            && c.extras.params.get("project").is_some_and(|p| !p.contains(['/', '\\'])))
    });
    let mut found = Vec::new();
    for top in ["Assets", "Packages"] {
        let dir = root.join(top);
        if dir.is_dir() {
            collect_test_assemblies(root, &dir, &mut found);
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    for (name, dir, mode) in found {
        configs.push(test_config(&name, &dir, mode));
    }
}

fn collect_test_assemblies(root: &Path, dir: &Path, out: &mut Vec<(String, String, &'static str)>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let Ok(kind) = entry.file_type() else { continue };
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if kind.is_dir() {
            if !name_str.starts_with('.') && !name_str.ends_with('~') {
                collect_test_assemblies(root, &path, out);
            }
            continue;
        }
        if !kind.is_file()
            || !path.extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("asmdef"))
        {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let Some((assembly, mode)) = parse_test_assembly(&text) else { continue };
        let Ok(rel) = path.strip_prefix(root) else { continue };
        let rel_dir = rel.parent().unwrap_or(Path::new("")).to_string_lossy().replace('\\', "/");
        out.push((assembly, rel_dir, mode));
    }
}

/// Parses an `.asmdef` once: returns `(assembly_name, "editMode" | "playMode")` when it is a test assembly.
fn parse_test_assembly(asmdef: &str) -> Option<(String, &'static str)> {
    let json: Value = serde_json::from_str(asmdef.trim_start_matches('\u{feff}')).ok()?;
    let name = json.get("name").and_then(Value::as_str).filter(|n| !n.is_empty())?.to_owned();
    let mode = test_mode_json(&json)?;
    Some((name, mode))
}

#[cfg(test)]
fn test_mode(asmdef: &str) -> Option<&'static str> {
    let json: Value = serde_json::from_str(asmdef.trim_start_matches('\u{feff}')).ok()?;
    test_mode_json(&json)
}

/// `editMode` for a test assembly limited to the Editor, `playMode` for another one; none when the
/// assembly has no tests.
fn test_mode_json(json: &Value) -> Option<&'static str> {
    let strings = |key: &str| -> Vec<String> {
        json.get(key)
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).map(str::to_owned).collect())
            .unwrap_or_default()
    };
    let references = strings("references");
    let tests = references.iter().any(|r| TEST_RUNNER_REFERENCES.iter().any(|t| r.eq_ignore_ascii_case(t)))
        // The format before Unity 2019.2.
        || strings("optionalUnityReferences").iter().any(|r| r == "TestAssemblies");
    if !tests {
        return None;
    }
    Some(if strings("includePlatforms") == ["Editor"] { "editMode" } else { "playMode" })
}

fn test_config(assembly: &str, dir: &str, mode: &str) -> RunConfig {
    let mut config: RunConfig = serde_json::from_value(serde_json::json!({
        "id": format!("{KIND}:{assembly}"),
        "name": format!("{assembly} (Unity tests)"),
        "kind": KIND,
        "command": "unity",
        "args": [],
        "cwd": dir,
        "env": {},
        "detected": true,
        "isTest": true,
        "debugAdapter": null,
    }))
    .expect("a valid run configuration");
    config.extras.params = BTreeMap::from([("assembly".into(), assembly.into()), ("mode".into(), mode.into())]);
    config
}

/// Run ids of the test runs in progress, and the id the Editor knows each one by.
fn runs() -> &'static Mutex<HashMap<RunId, String>> {
    static RUNS: OnceLock<Mutex<HashMap<RunId, String>>> = OnceLock::new();
    RUNS.get_or_init(Mutex::default)
}

/// Starts the tests of `config` (its arguments: full names of tests or fixtures, none for all) in
/// the connected Editor. Output and `RunExited` follow as for a process.
pub(super) async fn start(
    host: &Host,
    window: &str,
    config: RunConfig,
    on_output: Emit<OutputChunk>,
) -> IdeResult<RunInfo> {
    let project = host.project(window)?;
    project.require_trust()?;
    let param = |key: &str| config.extras.params.get(key).filter(|v| !v.is_empty()).cloned();
    let run = TestRun {
        mode: param("mode").unwrap_or_else(|| "editMode".into()),
        assembly: param("assembly"),
        filter: config.args.clone(),
    };
    let id = RunId::next();
    // Unique across IDE sessions: the Editor outlives them.
    let key = format!("{}-{}-{}", std::process::id(), id.0, super::super::unity::nonce());
    let info = RunInfo { id, config_id: config.id, name: config.name, pid: None, running: true, pty: false };
    tokio::task::spawn_blocking(move || {
        let link = project.unity_link.as_ref().ok_or_else(|| IdeError::invalid("Not a Unity project"))?;
        if link.status().connection != UnityLinkConnection::Connected {
            return Err(IdeError::unavailable("Open the project in the Unity Editor to run its tests"));
        }
        runs().lock().insert(id, key.clone());
        let sink = project.sink.clone();
        let started = link.run_tests(&key, &run, move |update| {
            let (stdout, stderr) = test_console_text(&update);
            if !stdout.is_empty() {
                on_output(OutputChunk::Stdout { text: stdout });
            }
            if !stderr.is_empty() {
                on_output(OutputChunk::Stderr { text: stderr });
            }
            if let Some(code) = test_exit_code(&update) {
                runs().lock().remove(&id);
                sink.emit(IdeEvent::RunExited { run: id, code });
            }
        });
        if let Err(e) = started {
            runs().lock().remove(&id);
            return Err(e);
        }
        Ok(info)
    })
    .await
    .map_err(|e| IdeError::internal(e.to_string()))?
}

/// Cancels a Unity test run; `None` when `id` isn't one. A run the Editor no longer knows (a reload
/// lost it) ends right away.
pub(super) fn stop(host: &Host, window: &str, id: RunId) -> Option<IdeResult<()>> {
    let key = runs().lock().get(&id).cloned()?;
    let project = match host.project(window) {
        Ok(p) => p,
        Err(e) => return Some(Err(e)),
    };
    // Waits for the Editor's answer: off the command thread.
    std::thread::spawn(move || {
        let cancelled = project.unity_link.as_ref().is_some_and(|link| link.cancel_tests(&key));
        if !cancelled && runs().lock().remove(&id).is_some() {
            project.sink.emit(IdeEvent::RunExited { run: id, code: None });
        }
    });
    Some(Ok(()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assemblies_and_their_mode() {
        let edit = r#"{"name":"T","references":["Game","UnityEngine.TestRunner","UnityEditor.TestRunner"],"includePlatforms":["Editor"]}"#;
        assert_eq!(test_mode(edit), Some("editMode"));
        let play = r#"{"name":"T","references":["GUID:27619889b8ba8c24980f49ee34dbb44a"],"includePlatforms":[]}"#;
        assert_eq!(test_mode(play), Some("playMode"));
        let old = "\u{feff}{\"name\":\"T\",\"optionalUnityReferences\":[\"TestAssemblies\"]}";
        assert_eq!(test_mode(old), Some("playMode"));
        assert_eq!(test_mode(r#"{"name":"Game","references":[],"includePlatforms":["Editor"]}"#), None);
        assert_eq!(test_mode("not json"), None);
    }

    #[test]
    fn unity_projects_get_test_configs_instead_of_generated_dotnet_ones() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let write = |rel: &str, text: &str| {
            let path = root.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write(
            "Assets/Tests/EditMode/Game.Tests.asmdef",
            r#"{"name":"Game.Tests","references":["UnityEditor.TestRunner"],"includePlatforms":["Editor"]}"#,
        );
        write("Assets/Scripts/Game.asmdef", r#"{"name":"Game"}"#);
        write(
            "Library/PackageCache/com.x@1/Tests/X.Tests.asmdef",
            r#"{"name":"X.Tests","references":["UnityEngine.TestRunner"]}"#,
        );
        let dotnet = |project: &str| {
            let mut c = test_config("x", "", "editMode");
            c.kind = "dotnet-test".into();
            c.extras.params = BTreeMap::from([("project".into(), project.into())]);
            c
        };
        let mut configs = vec![dotnet("Game.Tests.csproj"), dotnet("tools/Tool.Tests/Tool.Tests.csproj")];
        adjust_detected(root, &mut configs);
        let ids: Vec<_> = configs.iter().map(|c| (c.id.as_str(), c.kind.as_str())).collect();
        assert_eq!(ids, [("unity-test:x", "dotnet-test"), ("unity-test:Game.Tests", KIND)]);
        let test = &configs[1];
        assert!(test.is_test);
        assert_eq!(test.cwd.as_ref().map(|c| c.0.clone()), Some("Assets/Tests/EditMode".into()));
        assert_eq!(test.extras.params.get("mode").map(String::as_str), Some("editMode"));
        assert_eq!(test.extras.params.get("assembly").map(String::as_str), Some("Game.Tests"));
    }
}
