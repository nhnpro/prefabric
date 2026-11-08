//! Code Vision for Unity: what the assets do with a script's class, methods and serialized fields,
//! summarised above their declarations.

use crate::UnityIndex;
use crate::usages::{is_class, symbol};
use ide_core::{OutlineItem, SymbolKind};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::BTreeSet;
use std::path::Path;

/// One declaration's summary, at its name (0-based line, UTF-16 column).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UnityLens {
    pub line: u32,
    pub col: u32,
    pub summary: LensSummary,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum LensSummary {
    /// Prefabs, scenes and other assets with a component of the class.
    #[serde(rename_all = "camelCase")]
    Class { prefabs: u32, scenes: u32, other: u32 },
    /// UnityEvent listeners and animation events calling the method; `files`: the assets' names,
    /// at most a few, out of `file_count`.
    #[serde(rename_all = "camelCase")]
    Method { unity_events: u32, animation_events: u32, files: Vec<String>, file_count: u32 },
    /// The serialized field's distinct values on the script's own components (the Inspector's),
    /// and how many prefab instances override it.
    #[serde(rename_all = "camelCase")]
    Field { values: Vec<String>, overrides: u32 },
}

const NAMED_FILES: usize = 3;
const NAMED_VALUES: usize = 3;

impl UnityIndex {
    /// The lenses of a C# file whose syntax outline is `outline`; only declarations assets use.
    pub fn lenses(&self, path: &Path, outline: &[OutlineItem]) -> Vec<UnityLens> {
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else { return Vec::new() };
        let mut out = Vec::new();
        let mut parents = Vec::new();
        self.class_lenses(path, stem, outline, &mut parents, &mut out);
        out.sort_by_key(|l| (l.line, l.col));
        out
    }

    fn class_lenses<'a>(
        &self,
        path: &Path,
        stem: &str,
        items: &'a [OutlineItem],
        parents: &mut Vec<&'a OutlineItem>,
        out: &mut Vec<UnityLens>,
    ) {
        for item in items {
            match item.kind {
                SymbolKind::Namespace => {}
                _ if is_class(item) => {
                    if let Some(class) = symbol(parents, item, stem) {
                        self.members(path, item, &class, out);
                    }
                }
                _ => continue,
            }
            parents.push(item);
            self.class_lenses(path, stem, &item.children, parents, out);
            parents.pop();
        }
    }

    /// Lenses of `class` (declared by `item`) and of its methods and fields.
    fn members(&self, path: &Path, item: &OutlineItem, class: &crate::usages::CodeSymbol, out: &mut Vec<UnityLens>) {
        let at = |i: &OutlineItem| (i.selection_range.start.line, i.selection_range.start.col);
        let script = if class.script_class { self.guid_of(path) } else { None };
        if let Some(script) = script {
            let (mut prefabs, mut scenes, mut other) = (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
            for u in self.script_usages(script) {
                let set = if u.rel.ends_with(".prefab") {
                    &mut prefabs
                } else if u.rel.ends_with(".unity") {
                    &mut scenes
                } else {
                    &mut other
                };
                set.insert(u.rel);
            }
            if !(prefabs.is_empty() && scenes.is_empty() && other.is_empty()) {
                let (line, col) = at(item);
                let n = |s: BTreeSet<_>| s.len() as u32;
                out.push(UnityLens {
                    line,
                    col,
                    summary: LensSummary::Class { prefabs: n(prefabs), scenes: n(scenes), other: n(other) },
                });
            }
        }
        for m in item.children.iter().filter(|c| c.kind == SymbolKind::Method) {
            let bindings = self.method_bindings(&m.name, Some(&class.class));
            if bindings.is_empty() {
                continue;
            }
            let unity_events =
                bindings.iter().filter(|b| matches!(b.kind, crate::BindingKind::UnityEvent { .. })).count();
            let files: BTreeSet<&str> = bindings.iter().map(|b| b.rel.rsplit('/').next().unwrap_or(&b.rel)).collect();
            let (line, col) = at(m);
            out.push(UnityLens {
                line,
                col,
                summary: LensSummary::Method {
                    unity_events: unity_events as u32,
                    animation_events: (bindings.len() - unity_events) as u32,
                    file_count: files.len() as u32,
                    files: files.into_iter().take(NAMED_FILES).map(String::from).collect(),
                },
            });
        }
        let Some(script) = script else { return };
        let fields: Vec<&OutlineItem> = item.children.iter().filter(|c| c.kind == SymbolKind::Field).collect();
        if fields.is_empty() {
            return;
        }
        let names: Vec<&str> = fields.iter().map(|f| f.name.as_str()).collect();
        for (field, values) in fields.iter().zip(self.fields_values(script, &names)) {
            let overrides = values.iter().filter(|v| v.overridden_at.is_some()).count() as u32;
            let own: BTreeSet<&str> = values
                .iter()
                .filter(|v| v.usage.through.is_none() && v.overridden_at.is_none())
                .filter_map(|v| v.value.as_deref())
                .collect();
            if own.is_empty() && overrides == 0 {
                continue;
            }
            let (line, col) = at(field);
            out.push(UnityLens {
                line,
                col,
                summary: LensSummary::Field {
                    values: own.into_iter().take(NAMED_VALUES + 1).map(String::from).collect(),
                    overrides,
                },
            });
        }
    }
}
