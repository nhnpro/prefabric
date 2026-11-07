//! The GUID graph of a Unity project and its queries.
//!
//! Kept in memory: each asset's path and GUID, and reverse maps from what can be looked for (a
//! script, a prefab, any asset, a method name) to the files that use it. A file's objects (names,
//! hierarchy, overrides) stay in the store and load when a query needs them, through a small cache.

use crate::prefab::{self, Assets};
use crate::scan::{Guid, Object, ScannedFile};
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

pub(crate) type FileIdx = u32;

struct FileNode {
    rel: Arc<str>,
    guid: Option<Guid>,
}

#[derive(Default)]
pub(crate) struct Graph {
    files: Vec<FileNode>,
    by_guid: HashMap<Guid, FileIdx>,
    by_rel: HashMap<Arc<str>, FileIdx>,
    /// Script GUID → (file, MonoBehaviour fileID) using it.
    scripts: HashMap<Guid, Vec<(FileIdx, i64)>>,
    /// Prefab GUID → files with an instance of it.
    instances: HashMap<Guid, Vec<FileIdx>>,
    /// Any GUID → files referring to it.
    referrers: HashMap<Guid, Vec<FileIdx>>,
    /// UnityEvent method or animation event function → files calling it.
    methods: HashMap<Box<str>, Vec<FileIdx>>,
    scanned_files: u32,
}

fn push_unique(list: &mut Vec<FileIdx>, file: FileIdx) {
    if list.last() != Some(&file) {
        list.push(file);
    }
}

impl Graph {
    /// Adds an asset; `scan` when its objects were scanned (summarised here, not kept).
    pub(crate) fn add(&mut self, rel: &str, guid: Option<Guid>, scan: Option<&ScannedFile>) {
        let idx = FileIdx::try_from(self.files.len()).unwrap_or(FileIdx::MAX);
        let rel: Arc<str> = rel.into();
        self.by_rel.insert(rel.clone(), idx);
        if let Some(guid) = guid {
            self.by_guid.insert(guid, idx);
        }
        self.files.push(FileNode { rel, guid });
        let Some(scan) = scan else { return };
        self.scanned_files += 1;
        for o in &scan.objects {
            if let Some(script) = o.script.and_then(|s| s.guid)
                && !o.stripped
            {
                self.scripts.entry(script).or_default().push((idx, o.file_id));
            }
            if let Some(prefab) = o.source_prefab.and_then(|p| p.guid) {
                push_unique(self.instances.entry(prefab).or_default(), idx);
            }
            let calls = o.calls.iter().map(|c| &*c.method);
            let events = o.animation_events.iter().map(|e| &*e.function);
            let overridden = crate::events::override_calls(o);
            for method in calls.chain(events).chain(overridden.iter().map(|(_, c)| &*c.method)) {
                if method.is_empty() {
                    continue;
                }
                match self.methods.get_mut(method) {
                    Some(list) => push_unique(list, idx),
                    None => {
                        self.methods.insert(method.into(), vec![idx]);
                    }
                }
            }
        }
        for guid in &scan.references {
            push_unique(self.referrers.entry(*guid).or_default(), idx);
        }
    }

    pub(crate) fn file_count(&self) -> u32 {
        u32::try_from(self.files.len()).unwrap_or(u32::MAX)
    }

    pub(crate) fn scanned_count(&self) -> u32 {
        self.scanned_files
    }

    pub(crate) fn rel(&self, file: FileIdx) -> Option<&Arc<str>> {
        self.files.get(file as usize).map(|f| &f.rel)
    }

    pub(crate) fn guid_of(&self, rel: &str) -> Option<Guid> {
        self.files.get(*self.by_rel.get(rel)? as usize)?.guid
    }

    pub(crate) fn rel_of(&self, guid: Guid) -> Option<&Arc<str>> {
        self.rel(*self.by_guid.get(&guid)?)
    }

    pub(crate) fn referrers(&self, guid: Guid) -> impl Iterator<Item = &Arc<str>> {
        self.referrers.get(&guid).into_iter().flatten().filter_map(|&f| self.rel(f))
    }

    /// Heap held by the maps, roughly (for the bench's memory report).
    pub(crate) fn heap_bytes(&self) -> usize {
        use std::mem::size_of;
        let lists = |m: &HashMap<Guid, Vec<FileIdx>>| -> usize {
            m.values().map(|v| v.capacity() * size_of::<FileIdx>()).sum::<usize>()
                + m.capacity() * (size_of::<Guid>() + size_of::<Vec<FileIdx>>() + 8)
        };
        self.files.iter().map(|f| f.rel.len() + 16).sum::<usize>()
            + self.files.capacity() * size_of::<FileNode>()
            + self.by_guid.capacity() * (size_of::<Guid>() + size_of::<FileIdx>() + 8)
            + self.by_rel.capacity() * (size_of::<Arc<str>>() + size_of::<FileIdx>() + 8)
            + self.scripts.values().map(|v| v.capacity() * size_of::<(FileIdx, i64)>()).sum::<usize>()
            + self.scripts.capacity() * (size_of::<Guid>() + size_of::<Vec<(FileIdx, i64)>>() + 8)
            + lists(&self.instances)
            + lists(&self.referrers)
            + self.methods.iter().map(|(k, v)| k.len() + v.capacity() * size_of::<FileIdx>()).sum::<usize>()
            + self.methods.capacity() * (size_of::<Box<str>>() + size_of::<Vec<FileIdx>>() + 8)
    }
}

/// Where scanned files are read back from.
pub(crate) trait Source: Send + Sync {
    fn load(&self, rel: &str) -> Option<ScannedFile>;
}

const CACHE: usize = 256;

/// Scanned files loaded for queries, most recently used last.
#[derive(Default)]
pub(crate) struct Cache {
    entries: Mutex<VecDeque<(Arc<str>, Arc<ScannedFile>)>>,
}

impl Cache {
    fn get(&self, rel: &Arc<str>, source: &dyn Source) -> Option<Arc<ScannedFile>> {
        {
            let mut entries = self.entries.lock();
            if let Some(at) = entries.iter().position(|(r, _)| r == rel) {
                let entry = entries.remove(at)?;
                let file = entry.1.clone();
                entries.push_back(entry);
                return Some(file);
            }
        }
        let file = Arc::new(source.load(rel)?);
        let mut entries = self.entries.lock();
        if entries.len() >= CACHE {
            entries.pop_front();
        }
        entries.push_back((rel.clone(), file.clone()));
        Some(file)
    }
}

/// Files a query reads, by GUID, with the prefabs their instances come from.
struct Loaded(HashMap<Guid, Arc<ScannedFile>>);

impl Assets for Loaded {
    fn file(&self, guid: Guid) -> Option<&ScannedFile> {
        self.0.get(&guid).map(|f| &**f)
    }
}

/// A query: the graph plus loaded files.
pub(crate) struct Query<'a> {
    pub graph: &'a Graph,
    pub source: &'a dyn Source,
    pub cache: &'a Cache,
}

/// How a file uses a script: on one of its own GameObjects, or through an instance of a prefab
/// that does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptUsage {
    pub rel: Arc<str>,
    /// The component, as the file knows it.
    pub component: i64,
    /// Names from the root to the GameObject (`Squad/Follower`); empty when unknown.
    pub game_object: Vec<Box<str>>,
    /// 0-based line of the component, or of the prefab instance.
    pub line: u32,
    /// The prefab instanced to get the component, for a usage through an instance.
    pub through: Option<Arc<str>>,
}

/// A UnityEvent listener or animation event calling a method.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MethodBinding {
    pub rel: Arc<str>,
    pub line: u32,
    pub kind: BindingKind,
    /// The UnityEvent's owner GameObject, or the animation clip's name.
    pub owner: Vec<Box<str>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BindingKind {
    /// A UnityEvent field (`onEnter`) whose listener calls the method on `target_type`.
    UnityEvent {
        field: Box<str>,
        target_type: Option<Box<str>>,
    },
    AnimationEvent {
        time: Box<str>,
    },
}

/// A serialized field's value on one component using the script.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FieldValue {
    pub usage: ScriptUsage,
    pub value: Option<Box<str>>,
    /// The file and line of the override that sets it, when it is overridden.
    pub overridden_at: Option<(Arc<str>, u32)>,
}

/// Instances nested deeper than this aren't followed.
const MAX_NESTING: usize = 8;
/// Results of one query, at most.
const MAX_RESULTS: usize = 10_000;

const TRANSFORM: u32 = 4;
const RECT_TRANSFORM: u32 = 224;

impl Query<'_> {
    fn file(&self, idx: FileIdx) -> Option<Arc<ScannedFile>> {
        self.cache.get(self.graph.rel(idx)?, self.source)
    }

    /// `files` and, transitively, the prefabs their instances come from.
    fn load(&self, files: impl IntoIterator<Item = FileIdx>) -> Loaded {
        let mut loaded = HashMap::new();
        let mut queue: Vec<FileIdx> = files.into_iter().collect();
        let mut seen = HashSet::new();
        while let Some(idx) = queue.pop() {
            if !seen.insert(idx) || seen.len() > 512 {
                continue;
            }
            let (Some(guid), Some(file)) = (self.graph.files.get(idx as usize).and_then(|f| f.guid), self.file(idx))
            else {
                continue;
            };
            for o in &file.objects {
                if let Some(prefab) = o.source_prefab.and_then(|p| p.guid)
                    && let Some(&source) = self.graph.by_guid.get(&prefab)
                {
                    queue.push(source);
                }
            }
            loaded.insert(guid, file);
        }
        Loaded(loaded)
    }

    pub(crate) fn script_usages(&self, script: Guid) -> Vec<ScriptUsage> {
        let Some(direct) = self.graph.scripts.get(&script) else { return Vec::new() };
        let mut out = Vec::new();
        for &(idx, component) in direct {
            let (Some(file), Some(rel)) = (self.file(idx), self.graph.rel(idx)) else { continue };
            let Some(object) = file.object(component) else { continue };
            let path = object.game_object.map(|go| game_object_path(&file, go)).unwrap_or_default();
            out.push(ScriptUsage {
                rel: rel.clone(),
                component,
                game_object: path.clone(),
                line: object.line,
                through: None,
            });
            self.instance_usages(idx, component, &path, 0, &mut out);
            if out.len() >= MAX_RESULTS {
                break;
            }
        }
        out.truncate(MAX_RESULTS);
        out
    }

    /// Usages through instances of `prefab` (a file using the component `component` at `inner`, the
    /// path below the prefab's root).
    fn instance_usages(
        &self,
        prefab: FileIdx,
        component: i64,
        inner: &[Box<str>],
        depth: usize,
        out: &mut Vec<ScriptUsage>,
    ) {
        let Some(prefab_node) = self.graph.files.get(prefab as usize) else { return };
        let (Some(prefab_guid), true) = (prefab_node.guid, depth < MAX_NESTING) else { return };
        let below_root = inner.get(1..).unwrap_or_default();
        for &idx in self.graph.instances.get(&prefab_guid).into_iter().flatten() {
            let (Some(file), Some(rel), Some(guid)) = (self.file(idx), self.graph.rel(idx), file_guid(self.graph, idx))
            else {
                continue;
            };
            let loaded = self.load([idx]);
            let root = prefab_root(&loaded, prefab_guid);
            for instance in
                file.prefab_instances().filter(|i| i.source_prefab.and_then(|p| p.guid) == Some(prefab_guid))
            {
                if out.len() >= MAX_RESULTS {
                    return;
                }
                let mut path = instance.transform_parent.map(|t| transform_path(&file, t)).unwrap_or_default();
                let name = root
                    .and_then(|go| prefab::property(&loaded, guid, instance.file_id ^ go, "m_Name"))
                    .and_then(|v| v.value);
                path.push(name.or_else(|| inner.first().cloned()).unwrap_or_default());
                path.extend(below_root.iter().cloned());
                let here = instance.file_id ^ component;
                out.push(ScriptUsage {
                    rel: rel.clone(),
                    component: here,
                    game_object: path.clone(),
                    line: instance.line,
                    through: Some(prefab_node.rel.clone()),
                });
                self.instance_usages(idx, here, &path, depth + 1, out);
            }
        }
    }

    /// Listeners and animation events calling `method`; with `type_name` (`Game.Player`), only
    /// UnityEvent listeners whose target type is that class (animation events name no type).
    pub(crate) fn method_bindings(&self, method: &str, type_name: Option<&str>) -> Vec<MethodBinding> {
        let mut out = Vec::new();
        for &idx in self.graph.methods.get(method).into_iter().flatten() {
            let (Some(file), Some(rel)) = (self.file(idx), self.graph.rel(idx)) else { continue };
            let type_matches = |t: Option<&str>| match (type_name, t) {
                (Some(want), Some(t)) => t.split(',').next().map(str::trim) == Some(want),
                _ => true,
            };
            for o in &file.objects {
                let owner = || o.game_object.map(|go| game_object_path(&file, go)).unwrap_or_default();
                for c in o.calls.iter().filter(|c| &*c.method == method && type_matches(c.target_type.as_deref())) {
                    out.push(MethodBinding {
                        rel: rel.clone(),
                        line: c.line,
                        kind: BindingKind::UnityEvent { field: c.field.clone(), target_type: c.target_type.clone() },
                        owner: owner(),
                    });
                }
                for (_, c) in crate::events::override_calls(o) {
                    if &*c.method == method && type_matches(c.target_type.as_deref()) {
                        out.push(MethodBinding {
                            rel: rel.clone(),
                            line: c.line,
                            kind: BindingKind::UnityEvent { field: c.field, target_type: c.target_type },
                            owner: Vec::new(),
                        });
                    }
                }
                for e in o.animation_events.iter().filter(|e| &*e.function == method) {
                    out.push(MethodBinding {
                        rel: rel.clone(),
                        line: e.line,
                        kind: BindingKind::AnimationEvent { time: e.time.clone() },
                        owner: o.name.iter().cloned().collect(),
                    });
                }
            }
            if out.len() >= MAX_RESULTS {
                break;
            }
        }
        out.truncate(MAX_RESULTS);
        out
    }

    /// The value of the serialized field `field` on every component using `script`.
    pub(crate) fn field_values(&self, script: Guid, field: &str) -> Vec<FieldValue> {
        let usages = self.script_usages(script);
        let mut by_file: HashMap<Arc<str>, Loaded> = HashMap::new();
        let mut out = Vec::new();
        for usage in usages {
            let Some(&idx) = self.graph.by_rel.get(&usage.rel) else { continue };
            let Some(guid) = file_guid(self.graph, idx) else { continue };
            let loaded = by_file.entry(usage.rel.clone()).or_insert_with(|| self.load([idx]));
            let Some(value) = prefab::property(loaded, guid, usage.component, field) else { continue };
            let overridden_at = value.overridden_at.and_then(|(g, line)| Some((self.graph.rel_of(g)?.clone(), line)));
            out.push(FieldValue { usage, value: value.value, overridden_at });
        }
        out
    }
}

fn file_guid(graph: &Graph, idx: FileIdx) -> Option<Guid> {
    graph.files.get(idx as usize)?.guid
}

/// The root GameObject of a prefab: the one whose Transform has no parent, or for a variant, the
/// root of the prefab its root instance comes from, as the variant numbers it.
fn prefab_root(loaded: &Loaded, prefab: Guid) -> Option<i64> {
    let mut offset = 0i64;
    let mut guid = prefab;
    for _ in 0..MAX_NESTING {
        let file = loaded.file(guid)?;
        let own = file.objects.iter().find(|o| {
            matches!(o.class_id, TRANSFORM | RECT_TRANSFORM)
                && !o.stripped
                && o.father.is_none()
                && o.game_object.is_some()
        });
        if let Some(go) = own.and_then(|t| t.game_object) {
            return Some(offset ^ go);
        }
        let instance = file.prefab_instances().find(|i| i.transform_parent.is_none())?;
        offset ^= instance.file_id;
        guid = instance.source_prefab?.guid?;
    }
    None
}

fn transform_of(file: &ScannedFile, go: i64) -> Option<&Object> {
    file.objects.iter().find(|o| matches!(o.class_id, TRANSFORM | RECT_TRANSFORM) && o.game_object == Some(go))
}

/// Names from the root to `go`, as far as this file knows them.
pub(crate) fn game_object_path(file: &ScannedFile, go: i64) -> Vec<Box<str>> {
    match transform_of(file, go) {
        Some(t) => transform_path(file, t.file_id),
        None => file.object(go).and_then(|o| o.name.clone()).into_iter().collect(),
    }
}

fn transform_path(file: &ScannedFile, transform: i64) -> Vec<Box<str>> {
    let mut names = Vec::new();
    let mut next = Some(transform);
    while let Some(id) = next {
        if names.len() > 256 {
            break;
        }
        let Some(t) = file.object(id) else { break };
        let name = t.game_object.and_then(|go| file.object(go)).and_then(|o| o.name.clone());
        names.push(name.unwrap_or_default());
        next = t.father;
    }
    names.reverse();
    names
}

#[cfg(test)]
#[path = "graph_tests.rs"]
mod tests;
