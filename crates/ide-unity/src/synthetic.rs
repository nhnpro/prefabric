//! Generated Unity projects for benchmarks (`benches/`); not a stable API.

use std::fmt::Write as _;
use std::path::Path;

fn guid(kind: u8, i: usize) -> String {
    format!("{kind:02x}{i:030x}")
}

fn meta(path: &Path, guid: &str) -> std::io::Result<()> {
    let mut name = path.as_os_str().to_os_string();
    name.push(".meta");
    std::fs::write(name, format!("fileFormatVersion: 2\nguid: {guid}\n"))
}

/// The GUID of the generated script `i`.
pub fn script_guid(i: usize) -> String {
    guid(1, i)
}

/// Writes a project with `scripts` scripts, `prefabs` prefabs (each a GameObject with a Transform,
/// a child and a MonoBehaviour with a UnityEvent listener) and `scenes` scenes, each holding
/// `instances` prefab instances with overrides. Returns the number of asset files.
pub fn write_project(
    root: &Path,
    scripts: usize,
    prefabs: usize,
    scenes: usize,
    instances: usize,
) -> std::io::Result<usize> {
    for dir in ["Assets/Scripts", "Assets/Prefabs", "Assets/Scenes", "ProjectSettings"] {
        std::fs::create_dir_all(root.join(dir))?;
    }
    std::fs::write(root.join("ProjectSettings/ProjectVersion.txt"), "m_EditorVersion: 6000.0.0f1\n")?;
    for i in 0..scripts {
        let path = root.join(format!("Assets/Scripts/Behaviour{i}.cs"));
        std::fs::write(
            &path,
            format!("public class Behaviour{i} : UnityEngine.MonoBehaviour {{ public int speed; }}\n"),
        )?;
        meta(&path, &script_guid(i))?;
    }
    let header = "%YAML 1.1\n%TAG !u! tag:unity3d.com,2011:\n";
    for p in 0..prefabs {
        let script = script_guid(p % scripts.max(1));
        let mut text = String::from(header);
        let _ = write!(
            text,
            "--- !u!1 &100\nGameObject:\n  m_Component:\n  - component: {{fileID: 200}}\n  - component: {{fileID: 300}}\n  m_Name: Prefab{p}\n\
             --- !u!4 &200\nTransform:\n  m_GameObject: {{fileID: 100}}\n  m_Children:\n  - {{fileID: 500}}\n  m_Father: {{fileID: 0}}\n\
             --- !u!114 &300\nMonoBehaviour:\n  m_GameObject: {{fileID: 100}}\n  m_Script: {{fileID: 11500000, guid: {script}, type: 3}}\n  m_Name: \n  speed: {p}\n  onHit:\n    m_PersistentCalls:\n      m_Calls:\n      - m_Target: {{fileID: 300}}\n        m_TargetAssemblyTypeName: Behaviour{s}, Assembly-CSharp\n        m_MethodName: OnHit{s}\n        m_Mode: 1\n        m_CallState: 2\n\
             --- !u!1 &400\nGameObject:\n  m_Component:\n  - component: {{fileID: 500}}\n  m_Name: Child\n\
             --- !u!4 &500\nTransform:\n  m_GameObject: {{fileID: 400}}\n  m_Children: []\n  m_Father: {{fileID: 200}}\n",
            s = p % scripts.max(1),
        );
        let path = root.join(format!("Assets/Prefabs/Prefab{p}.prefab"));
        std::fs::write(&path, text)?;
        meta(&path, &guid(2, p))?;
    }
    for s in 0..scenes {
        let mut text = String::from(header);
        for i in 0..instances {
            let prefab = guid(2, (s * instances + i) % prefabs.max(1));
            let id = 1_000 + i * 10;
            let _ = write!(
                text,
                "--- !u!1001 &{id}\nPrefabInstance:\n  m_Modification:\n    m_TransformParent: {{fileID: 0}}\n    m_Modifications:\n    - target: {{fileID: 100, guid: {prefab}, type: 3}}\n      propertyPath: m_Name\n      value: Instance{i}\n      objectReference: {{fileID: 0}}\n    - target: {{fileID: 300, guid: {prefab}, type: 3}}\n      propertyPath: speed\n      value: {i}\n      objectReference: {{fileID: 0}}\n    m_RemovedComponents: []\n  m_SourcePrefab: {{fileID: 100100000, guid: {prefab}, type: 3}}\n"
            );
        }
        let path = root.join(format!("Assets/Scenes/Scene{s}.unity"));
        std::fs::write(&path, text)?;
        meta(&path, &guid(3, s))?;
    }
    Ok(scripts + prefabs + scenes)
}
