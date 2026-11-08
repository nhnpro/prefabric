//! The GameObject tree of a scene or prefab, as Unity's Hierarchy window shows it: GameObjects
//! with their components, and prefab instances with their overrides and the prefab's own objects.

use crate::UnityIndex;
use crate::scan::{Guid, Object, ScannedFile, scan};
use ide_core::IpcPath;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashSet;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum HierarchyKind {
    GameObject,
    PrefabInstance,
    /// An object of an instanced prefab (it lives in the prefab, not in this file).
    FromPrefab,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HierarchyComponent {
    /// `Transform`, `BoxCollider`… or a script's class (its file name) for a MonoBehaviour.
    pub name: String,
    /// The script file of a MonoBehaviour.
    pub script: Option<IpcPath>,
    /// 0-based line in this file; `None` for a prefab's object.
    pub line: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HierarchyOverride {
    pub property: String,
    /// The new value, or the referenced object (`{fileID: …, guid: …}`) as written.
    pub value: String,
    /// The instanced object it changes, by name when known.
    pub target: Option<String>,
    pub line: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct HierarchyNode {
    pub kind: HierarchyKind,
    pub name: String,
    /// 0-based line in this file of the GameObject or instance (an instance's for a prefab's object).
    pub line: u32,
    /// The instanced prefab.
    pub prefab: Option<IpcPath>,
    pub components: Vec<HierarchyComponent>,
    pub overrides: Vec<HierarchyOverride>,
    pub children: Vec<HierarchyNode>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Hierarchy {
    pub roots: Vec<HierarchyNode>,
    /// Not a text-serialized scene or prefab.
    pub unreadable: bool,
}

const TRANSFORM: u32 = 4;
const RECT_TRANSFORM: u32 = 224;
const MONO_BEHAVIOUR: u32 = 114;
/// Prefabs instanced deeper than this aren't expanded.
const MAX_NESTING: usize = 8;
/// GameObjects shown, at most (a huge scene stays responsive).
const MAX_NODES: usize = 20_000;

fn is_transform(o: &Object) -> bool {
    matches!(o.class_id, TRANSFORM | RECT_TRANSFORM)
}

struct Builder<'a> {
    index: &'a UnityIndex,
    nodes: usize,
    /// Prefabs being expanded (a prefab that instances itself stops there).
    open: HashSet<Guid>,
}

/// One file being shown: its own objects, or a prefab's under an instance of it.
struct Level<'f> {
    file: &'f ScannedFile,
    /// Inside an instance: the instance's line in the shown file, and its name overrides.
    instance: Option<(u32, &'f [(i64, String)])>,
}

impl UnityIndex {
    /// The tree of the scene or prefab with content `bytes`; instanced prefabs are read from disk.
    pub fn hierarchy(&self, bytes: &[u8]) -> Hierarchy {
        if !bytes.starts_with(b"%YAML") {
            return Hierarchy { roots: Vec::new(), unreadable: true };
        }
        let file = scan(bytes);
        let mut b = Builder { index: self, nodes: 0, open: HashSet::new() };
        Hierarchy { roots: b.roots(&Level { file: &file, instance: None }, 0), unreadable: false }
    }
}

impl Builder<'_> {
    fn roots(&mut self, level: &Level<'_>, depth: usize) -> Vec<HierarchyNode> {
        let file = level.file;
        let mut roots: Vec<(u32, HierarchyNode)> = Vec::new();
        for o in &file.objects {
            let node = if is_transform(o) && !o.stripped && o.father.is_none() {
                self.game_object(level, o, depth)
            } else if o.source_prefab.is_some() && o.transform_parent.is_none() {
                self.instance(level, o, depth)
            } else {
                continue;
            };
            roots.extend(node.map(|n| (o.line, n)));
        }
        roots.sort_by_key(|(line, _)| *line);
        roots.into_iter().map(|(_, n)| n).collect()
    }

    /// The GameObject of `transform`, with its subtree.
    fn game_object(&mut self, level: &Level<'_>, transform: &Object, depth: usize) -> Option<HierarchyNode> {
        if self.nodes >= MAX_NODES {
            return None;
        }
        self.nodes += 1;
        let file = level.file;
        let go = transform.game_object.and_then(|id| file.object(id))?;
        let own = level.instance.is_none();
        let renamed = level.instance.and_then(|(_, names)| names.iter().find(|(id, _)| *id == go.file_id));
        let name = renamed.map(|(_, n)| n.clone()).or_else(|| go.name.as_deref().map(String::from)).unwrap_or_default();
        let components =
            go.components.iter().filter_map(|id| file.object(*id)).map(|c| self.component(c, own)).collect();
        let mut children = Vec::new();
        for id in &transform.children {
            let Some(child) = file.object(*id) else { continue };
            let node = match child.prefab_instance.filter(|_| child.stripped) {
                // A nested instance's root: the instance stands for it.
                Some(instance) => file.object(instance).and_then(|i| self.instance(level, i, depth)),
                None => self.game_object(level, child, depth),
            };
            children.extend(node);
        }
        Some(HierarchyNode {
            kind: if own { HierarchyKind::GameObject } else { HierarchyKind::FromPrefab },
            name,
            line: level.instance.map_or(go.line, |(line, _)| line),
            prefab: None,
            components,
            overrides: Vec::new(),
            children,
        })
    }

    fn component(&self, c: &Object, own: bool) -> HierarchyComponent {
        let script = (c.class_id == MONO_BEHAVIOUR)
            .then(|| c.script.and_then(|s| s.guid).and_then(|g| self.index.path_of(g)))
            .flatten();
        let name = script
            .as_ref()
            .and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| c.type_name.to_string());
        HierarchyComponent { name, script: script.map(Into::into), line: own.then_some(c.line) }
    }

    /// A prefab instance: its overrides, and the prefab's objects below it.
    fn instance(&mut self, level: &Level<'_>, o: &Object, depth: usize) -> Option<HierarchyNode> {
        if self.nodes >= MAX_NODES {
            return None;
        }
        self.nodes += 1;
        let guid = o.source_prefab.and_then(|p| p.guid);
        let prefab = guid.and_then(|g| self.index.path_of(g));
        let line = level.instance.map_or(o.line, |(line, _)| line);
        let source = match (guid, &prefab) {
            (Some(g), Some(p)) if depth < MAX_NESTING && !self.open.contains(&g) => {
                std::fs::read(p).ok().filter(|b| b.starts_with(b"%YAML")).map(|b| (g, scan(&b)))
            }
            _ => None,
        };
        // Renamings of the prefab's objects, by their id in the prefab.
        let names: Vec<(i64, String)> = o
            .modifications
            .iter()
            .filter(|m| &*m.property_path == "m_Name")
            .filter_map(|m| Some((m.target.file_id, m.value.as_deref()?.to_owned())))
            .collect();
        let target_name = |id: i64| -> Option<String> {
            let (_, src) = source.as_ref()?;
            names
                .iter()
                .find(|(t, _)| *t == id)
                .map(|(_, n)| n.clone())
                .or_else(|| src.object(id)?.name.as_deref().map(String::from))
                .or_else(|| {
                    let owner = src.object(id)?.game_object?;
                    let go = src.object(owner)?;
                    let name = names.iter().find(|(t, _)| *t == owner).map(|(_, n)| n.as_str());
                    Some(format!("{} ({})", name.or(go.name.as_deref())?, src.object(id)?.type_name))
                })
        };
        let overrides = if level.instance.is_some() {
            Vec::new()
        } else {
            o.modifications
                .iter()
                .map(|m| HierarchyOverride {
                    property: m.property_path.to_string(),
                    value: m
                        .value
                        .as_deref()
                        .map(String::from)
                        .or_else(|| {
                            m.object_reference.map(|r| match r.guid {
                                Some(g) => format!("{{fileID: {}, guid: {g}}}", r.file_id),
                                None => format!("{{fileID: {}}}", r.file_id),
                            })
                        })
                        .unwrap_or_default(),
                    target: target_name(m.target.file_id),
                    line: m.line,
                })
                .collect()
        };
        let prefab_ipc: Option<IpcPath> = prefab.as_ref().map(|p| p.clone().into());
        let stem = prefab.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().into_owned());
        let mut node = HierarchyNode {
            kind: HierarchyKind::PrefabInstance,
            name: String::new(),
            line,
            prefab: prefab_ipc,
            components: Vec::new(),
            overrides,
            children: Vec::new(),
        };
        let mut root_name = None;
        if let Some((g, src)) = &source {
            self.open.insert(*g);
            let inner = Level { file: src, instance: Some((line, &names)) };
            let mut roots = self.roots(&inner, depth + 1);
            self.open.remove(g);
            // The prefab's root becomes the instance; its objects hang below.
            if roots.len() == 1 {
                let root = roots.pop().expect("one root");
                root_name = Some(root.name);
                node.components = root.components;
                node.children = root.children;
            } else {
                node.children = roots;
            }
        }
        node.name = root_name
            .filter(|n| !n.is_empty())
            .or_else(|| names.first().map(|(_, n)| n.clone()))
            .or(stem)
            .unwrap_or_default();
        Some(node)
    }
}

/// The tree of the file at `path`.
pub fn read(index: &UnityIndex, path: &Path) -> std::io::Result<Hierarchy> {
    Ok(index.hierarchy(&std::fs::read(path)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ide_core::NullSink;
    use std::sync::Arc;
    use std::time::Duration;

    const PROJECT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/unity-sample");

    fn outline(nodes: &[HierarchyNode], depth: usize, out: &mut Vec<String>) {
        for n in nodes {
            let comps: Vec<&str> = n.components.iter().map(|c| c.name.as_str()).collect();
            let kind = match n.kind {
                HierarchyKind::GameObject => "",
                HierarchyKind::PrefabInstance => "@",
                HierarchyKind::FromPrefab => "~",
            };
            out.push(format!("{}{kind}{} [{}] {}", "  ".repeat(depth), n.name, comps.join(","), n.overrides.len()));
            outline(&n.children, depth + 1, out);
        }
    }

    fn tree(index: &UnityIndex, rel: &str) -> Vec<String> {
        let h = read(index, &Path::new(PROJECT).join(rel)).unwrap();
        assert!(!h.unreadable);
        let mut out = Vec::new();
        outline(&h.roots, 0, &mut out);
        out
    }

    #[test]
    fn shows_scenes_and_prefabs_with_nested_instances() {
        let store = tempfile::tempdir().unwrap();
        let index = UnityIndex::open(Path::new(PROJECT), store.path(), Arc::new(NullSink), Box::new(|| true)).unwrap();
        assert!(index.wait_idle(Duration::from_secs(20)));
        let squad = tree(&index, "Assets/Prefabs/Squad.prefab");
        let main = tree(&index, "Assets/Scenes/Main.unity");
        let variant = tree(&index, "Assets/Prefabs/Player Variant.prefab");
        assert_eq!(
            squad,
            [
                "Squad [Transform] 0",
                "  @Leader [Transform,CapsuleCollider,Animator,Player] 11",
                "    ~Model [Transform,MeshFilter,MeshRenderer] 0",
                "  @Follower [Transform,CapsuleCollider,Animator,Player] 12",
                "    ~Model [Transform,MeshFilter,MeshRenderer] 0",
            ]
        );
        assert_eq!(main[0], "@Squad [Transform] 11");
        assert_eq!(main[2], "    ~Model [Transform,MeshFilter,MeshRenderer] 0");
        assert!(main.contains(&"Trigger Zone [Transform,BoxCollider,Trigger] 0".to_string()));
        assert_eq!(variant[0], "@Player Variant [Transform,CapsuleCollider,Animator,Player] 12");

        let h = read(&index, &Path::new(PROJECT).join("Assets/Prefabs/Squad.prefab")).unwrap();
        let follower = &h.roots[0].children[1];
        assert_eq!(follower.prefab.as_ref().map(|p| p.ends_with("Player.prefab")), Some(true));
        let x = follower.overrides.iter().find(|o| o.property == "m_LocalPosition.x").unwrap();
        assert_eq!((x.value.as_str(), x.target.as_deref(), x.line), ("1.5", Some("Follower (Transform)"), 47));
        assert!(follower.children[0].components.iter().all(|c| c.line.is_none()));
        assert_eq!(follower.children[0].line, follower.line);
        let player = &follower.components[3];
        assert_eq!(player.script.as_ref().map(|p| p.ends_with("Player.cs")), Some(true));
    }

    #[test]
    fn binary_files_are_unreadable() {
        let store = tempfile::tempdir().unwrap();
        let index = UnityIndex::open(Path::new(PROJECT), store.path(), Arc::new(NullSink), Box::new(|| true)).unwrap();
        assert!(index.hierarchy(b"UnityFS\0").unreadable);
    }
}
