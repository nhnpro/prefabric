//! Search Everywhere's Unity tab: GameObjects and prefab instances by name across scenes and
//! prefabs, and assets by file name, each with where it is (the hierarchy above it).

use crate::UnityIndex;
use crate::graph::{FileIdx, Query, game_object_path, transform_path};
use ide_core::IpcPath;
use nucleo_matcher::{Config, Matcher, Utf32Str, Utf32String};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum UnityHitKind {
    GameObject,
    /// An instance of a prefab in a scene or another prefab.
    PrefabInstance,
    /// An asset file (a prefab, a scene, a material…).
    Asset,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UnityHit {
    pub kind: UnityHitKind,
    pub name: String,
    /// Matched character indices of `name`.
    pub indices: Vec<u32>,
    /// Names from the root down to the GameObject's parent; empty for an asset or a root.
    pub parents: Vec<String>,
    /// The scene or prefab holding it, or the asset itself.
    pub path: IpcPath,
    /// 0-based line of the object in the file's YAML (0 for an asset).
    pub line: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UnitySearch {
    pub hits: Vec<UnityHit>,
    /// More matched than the limit.
    pub more: bool,
}

/// A candidate kept while scoring: what to resolve once it makes the cut.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct Candidate {
    score: u32,
    /// Shorter names first among equal scores.
    shorter: Reverse<usize>,
    file: Reverse<FileIdx>,
    /// Index into the file's named objects; `usize::MAX` for the file itself.
    object: Reverse<usize>,
}

const EXACT: u32 = 4000;
const PREFIX: u32 = 600;

fn boost(name: &str, lower_query: &str) -> u32 {
    let lower = name.to_lowercase();
    if lower == lower_query {
        EXACT
    } else if lower.starts_with(lower_query) {
        PREFIX
    } else {
        0
    }
}

impl UnityIndex {
    /// The best `limit` matches of `query` among GameObjects, prefab instances and assets.
    pub fn search(&self, query: &str, limit: usize) -> UnitySearch {
        let query = query.trim();
        if query.is_empty() || limit == 0 {
            return UnitySearch::default();
        }
        let root = self.root().to_path_buf();
        self.query(|q| search(q, query, limit, &root))
    }
}

fn search(q: &Query<'_>, query: &str, limit: usize, root: &std::path::Path) -> UnitySearch {
    let lower = query.to_lowercase();
    let needle = Utf32String::from(lower.as_str());
    let mut matcher = Matcher::new(Config::DEFAULT);
    let mut buf = Vec::new();
    let mut score = |name: &str, matcher: &mut Matcher| {
        matcher.fuzzy_match(Utf32Str::new(name, &mut buf), needle.slice(..)).map(|s| u32::from(s) + boost(name, &lower))
    };
    // A min-heap of the best `limit + 1`.
    let mut best: BinaryHeap<Reverse<Candidate>> = BinaryHeap::with_capacity(limit + 2);
    let mut matched = 0usize;
    let mut keep = |c: Candidate, best: &mut BinaryHeap<Reverse<Candidate>>| {
        matched += 1;
        best.push(Reverse(c));
        if best.len() > limit + 1 {
            best.pop();
        }
    };
    for (idx, rel, named) in q.graph.files_named() {
        let file_name = rel.rsplit('/').next().unwrap_or(rel);
        if let Some(s) = score(file_name, &mut matcher) {
            let c = Candidate {
                score: s,
                shorter: Reverse(file_name.len()),
                file: Reverse(idx),
                object: Reverse(usize::MAX),
            };
            keep(c, &mut best);
        }
        for (i, n) in named.iter().enumerate() {
            let name = instance_name(q, n);
            if let Some(s) = score(name, &mut matcher) {
                let c = Candidate { score: s, shorter: Reverse(name.len()), file: Reverse(idx), object: Reverse(i) };
                keep(c, &mut best);
            }
        }
    }
    let more = matched > limit;
    let mut ranked: Vec<Candidate> = best.into_iter().map(|Reverse(c)| c).collect();
    ranked.sort_by(|a, b| b.cmp(a));
    ranked.truncate(limit);
    let mut indices = Vec::new();
    let hits = ranked
        .into_iter()
        .filter_map(|c| {
            let hit = resolve(q, c.file.0, c.object.0, root)?;
            indices.clear();
            let mut buf = Vec::new();
            matcher.fuzzy_indices(Utf32Str::new(&hit.name, &mut buf), needle.slice(..), &mut indices);
            indices.sort_unstable();
            indices.dedup();
            Some(UnityHit { indices: indices.clone(), ..hit })
        })
        .collect();
    UnitySearch { hits, more }
}

/// An object's name; an instance without its own takes its prefab's.
fn instance_name<'a>(q: &'a Query<'_>, n: &'a crate::graph::Named) -> &'a str {
    if !n.name.is_empty() {
        return &n.name;
    }
    n.instance_of
        .and_then(|g| q.graph.rel_of(g))
        .map(|rel| {
            let file = rel.rsplit('/').next().unwrap_or(rel);
            file.strip_suffix(".prefab").unwrap_or(file)
        })
        .unwrap_or("")
}

fn resolve(q: &Query<'_>, idx: FileIdx, object: usize, root: &std::path::Path) -> Option<UnityHit> {
    let rel = q.graph.rel(idx)?;
    let path: IpcPath = root.join(&**rel).into();
    if object == usize::MAX {
        let name = rel.rsplit('/').next().unwrap_or(rel).to_string();
        return Some(UnityHit {
            kind: UnityHitKind::Asset,
            name,
            indices: Vec::new(),
            parents: Vec::new(),
            path,
            line: 0,
        });
    }
    let named = q.graph.named(idx).get(object)?;
    let name = instance_name(q, named).to_string();
    let file = q.file(idx);
    let o = file.as_ref().and_then(|f| f.object(named.file_id));
    let line = o.map_or(0, |o| o.line);
    let names = |v: Vec<Box<str>>| v.into_iter().map(String::from).collect::<Vec<_>>();
    let (kind, parents) = match (named.instance_of, &file) {
        (None, Some(f)) => {
            let mut p = names(game_object_path(f, named.file_id));
            p.pop();
            (UnityHitKind::GameObject, p)
        }
        (Some(_), Some(f)) => {
            let parent = o.and_then(|o| o.transform_parent);
            (UnityHitKind::PrefabInstance, parent.map(|t| names(transform_path(f, t))).unwrap_or_default())
        }
        (None, None) => (UnityHitKind::GameObject, Vec::new()),
        (Some(_), None) => (UnityHitKind::PrefabInstance, Vec::new()),
    };
    Some(UnityHit { kind, name, indices: Vec::new(), parents, path, line })
}
