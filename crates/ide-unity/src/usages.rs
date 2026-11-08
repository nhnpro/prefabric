//! Find Usages in assets: the scenes, prefabs and clips that use a script's class, call one of its
//! methods or set one of its serialized fields — what the code index can't see.

use crate::{BindingKind, UnityIndex};
use ide_core::{IpcPath, OutlineItem, Pos, Range, SymbolKind};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::Path;

/// A symbol declared in a C# file, as asset usages know it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeSymbol {
    /// The declaring class's full name as Unity serializes it (`Game.Player`, `Game.Outer+Inner`).
    pub class: String,
    /// The class is the file's script class: it has the file's name, so assets use it by the file's GUID.
    pub script_class: bool,
    pub member: Member,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Member {
    Class,
    Method(String),
    Field(String),
}

impl CodeSymbol {
    pub fn name(&self) -> &str {
        match &self.member {
            Member::Class => self.class.rsplit(['.', '+']).next().unwrap_or(&self.class),
            Member::Method(n) | Member::Field(n) => n,
        }
    }
}

/// How an asset uses the symbol.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum AssetUsageKind {
    /// A component of the script on a GameObject.
    Component,
    /// A UnityEvent listener calling the method.
    #[serde(rename_all = "camelCase")]
    UnityEvent { field: String },
    /// An animation event calling the method at `time` seconds.
    AnimationEvent { time: String },
    /// The serialized field's value on a component; `overridden` when a prefab instance sets it.
    #[serde(rename_all = "camelCase")]
    FieldValue { value: Option<String>, overridden: bool },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AssetUsage {
    pub path: IpcPath,
    /// 0-based line in the asset's YAML.
    pub line: u32,
    /// The GameObject from its root (`Squad/Follower`), or the animation clip; empty when unknown.
    pub game_object: Vec<String>,
    /// The prefab (relative to the project) whose instance brings the component.
    pub through: Option<String>,
    /// The component's file ID (as text: IDs exceed 2^53): tells apart same-named GameObjects of
    /// one instance. `None` for a method binding.
    pub component: Option<String>,
    pub usage: AssetUsageKind,
}

impl UnityIndex {
    /// Assets using `symbol`, declared in the script at `script`; sorted by file, then line.
    pub fn asset_usages(&self, script: &Path, symbol: &CodeSymbol) -> Vec<AssetUsage> {
        let root = self.root();
        let guid = if symbol.script_class { self.guid_of(script) } else { None };
        let names = |v: Vec<Box<str>>| v.into_iter().map(String::from).collect::<Vec<_>>();
        let mut out: Vec<AssetUsage> = match &symbol.member {
            Member::Class => guid
                .map(|g| self.script_usages(g))
                .unwrap_or_default()
                .into_iter()
                .map(|u| AssetUsage {
                    path: root.join(&*u.rel).into(),
                    line: u.line,
                    game_object: names(u.game_object),
                    through: u.through.map(|t| t.to_string()),
                    component: Some(u.component.to_string()),
                    usage: AssetUsageKind::Component,
                })
                .collect(),
            Member::Method(name) => self
                .method_bindings(name, Some(&symbol.class))
                .into_iter()
                .map(|b| AssetUsage {
                    path: root.join(&*b.rel).into(),
                    line: b.line,
                    game_object: names(b.owner),
                    through: None,
                    component: None,
                    usage: match b.kind {
                        BindingKind::UnityEvent { field, .. } => AssetUsageKind::UnityEvent { field: field.into() },
                        BindingKind::AnimationEvent { time } => AssetUsageKind::AnimationEvent { time: time.into() },
                    },
                })
                .collect(),
            Member::Field(name) => guid
                .map(|g| self.field_values(g, name))
                .unwrap_or_default()
                .into_iter()
                // Where the value is set: the script's own components and the overrides, not
                // every instance inheriting it.
                .filter(|v| v.usage.through.is_none() || v.overridden_at.is_some())
                .map(|v| {
                    let (rel, line) = v.overridden_at.clone().unwrap_or((v.usage.rel.clone(), v.usage.line));
                    AssetUsage {
                        path: root.join(&*rel).into(),
                        line,
                        game_object: names(v.usage.game_object),
                        through: v.usage.through.map(|t| t.to_string()),
                        component: Some(v.usage.component.to_string()),
                        usage: AssetUsageKind::FieldValue {
                            value: v.value.map(String::from),
                            overridden: v.overridden_at.is_some(),
                        },
                    }
                })
                .collect(),
        };
        out.sort_by(|a, b| a.path.cmp(&b.path).then(a.line.cmp(&b.line)));
        out.dedup();
        out
    }
}

/// The class, method or field named at `pos` of a C# file: its declaration, or a mention of one
/// of the enclosing classes' members (or of a class of the file). `outline` is the file's.
pub fn symbol_at(path: &Path, text: &str, outline: &[OutlineItem], pos: Pos) -> Option<CodeSymbol> {
    let word = word_at(text, pos)?;
    let stem = path.file_stem()?.to_str()?;
    // Ancestors of the innermost item containing `pos`, outermost first.
    let mut chain: Vec<&OutlineItem> = Vec::new();
    let mut items = outline;
    while let Some(item) = items.iter().find(|i| contains(i.range, pos)) {
        chain.push(item);
        items = &item.children;
    }
    // The declaration whose name the caret is on.
    if let Some(at) = chain.iter().rposition(|i| contains_name(i.selection_range, pos) && i.name == word) {
        return symbol(&chain[..at], chain[at], stem);
    }
    // A member of an enclosing class, innermost first.
    for at in (0..chain.len()).rev() {
        if !is_class(chain[at]) {
            continue;
        }
        if let Some(m) = chain[at].children.iter().find(|c| c.name == word && !is_class(c)) {
            return symbol(&chain[..=at], m, stem);
        }
    }
    // A class of the file.
    find_class(outline, &word, &mut Vec::new()).and_then(|(parents, class)| symbol(&parents, class, stem))
}

fn find_class<'a>(
    items: &'a [OutlineItem],
    name: &str,
    parents: &mut Vec<&'a OutlineItem>,
) -> Option<(Vec<&'a OutlineItem>, &'a OutlineItem)> {
    for item in items {
        if is_class(item) && item.name == name {
            return Some((parents.clone(), item));
        }
        if matches!(item.kind, SymbolKind::Namespace | SymbolKind::Class | SymbolKind::Struct) {
            parents.push(item);
            if let Some(found) = find_class(&item.children, name, parents) {
                return Some(found);
            }
            parents.pop();
        }
    }
    None
}

pub(crate) fn is_class(i: &OutlineItem) -> bool {
    matches!(i.kind, SymbolKind::Class | SymbolKind::Struct)
}

/// `item` declared under `parents` (outermost first).
pub(crate) fn symbol(parents: &[&OutlineItem], item: &OutlineItem, stem: &str) -> Option<CodeSymbol> {
    let (class_parents, class, member) = if is_class(item) {
        (parents, item, Member::Class)
    } else {
        let member = match item.kind {
            SymbolKind::Method => Member::Method(item.name.clone()),
            SymbolKind::Field => Member::Field(item.name.clone()),
            _ => return None,
        };
        let (class, rest) = parents.split_last()?;
        if !is_class(class) {
            return None;
        }
        (rest, *class, member)
    };
    let mut full = String::new();
    let mut nested = false;
    for p in class_parents {
        match p.kind {
            SymbolKind::Namespace => {
                full.push_str(&p.name);
                full.push('.');
            }
            SymbolKind::Class | SymbolKind::Struct => {
                full.push_str(&p.name);
                full.push('+');
                nested = true;
            }
            _ => return None,
        }
    }
    full.push_str(&class.name);
    Some(CodeSymbol { class: full, script_class: !nested && class.name == stem, member })
}

fn contains(r: Range, p: Pos) -> bool {
    r.start <= p && p < r.end
}

/// Inclusive of the end: the caret right after a name is on it.
fn contains_name(r: Range, p: Pos) -> bool {
    r.start <= p && p <= r.end
}

/// The identifier at or just before the UTF-16 position `pos`.
fn word_at(text: &str, pos: Pos) -> Option<String> {
    let line = text.lines().nth(pos.line as usize)?;
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    let mut units = 0u32;
    let mut at = chars.len();
    for (i, (_, c)) in chars.iter().enumerate() {
        if units >= pos.col {
            at = i;
            break;
        }
        units += c.len_utf16() as u32;
    }
    let ident = |c: char| c.is_alphanumeric() || c == '_';
    let mut start = at;
    while start > 0 && ident(chars[start - 1].1) {
        start -= 1;
    }
    let mut end = at;
    while end < chars.len() && ident(chars[end].1) {
        end += 1;
    }
    if start == end {
        return None;
    }
    let word: String = chars[start..end].iter().map(|(_, c)| c).collect();
    (!word.starts_with(|c: char| c.is_ascii_digit())).then_some(word)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(
        name: &str,
        kind: SymbolKind,
        range: (u32, u32),
        name_at: (u32, u32),
        children: Vec<OutlineItem>,
    ) -> OutlineItem {
        OutlineItem {
            name: name.into(),
            detail: None,
            kind,
            range: Range::new(Pos::new(range.0, 0), Pos::new(range.1, 0)),
            selection_range: Range::new(
                Pos::new(name_at.0, name_at.1),
                Pos::new(name_at.0, name_at.1 + name.len() as u32),
            ),
            children,
            visibility: None,
        }
    }

    const TEXT: &str = "namespace Game {\n  public class Player {\n    float speed;\n    void Hit() { speed++; }\n    class Inner { void Go() {} }\n  }\n}\n";

    fn outline() -> Vec<OutlineItem> {
        let inner = item(
            "Inner",
            SymbolKind::Class,
            (4, 5),
            (4, 10),
            vec![item("Go", SymbolKind::Method, (4, 5), (4, 23), vec![])],
        );
        let player = item(
            "Player",
            SymbolKind::Class,
            (1, 6),
            (1, 15),
            vec![
                item("speed", SymbolKind::Field, (2, 3), (2, 10), vec![]),
                item("Hit", SymbolKind::Method, (3, 4), (3, 9), vec![]),
                inner,
            ],
        );
        vec![item("Game", SymbolKind::Namespace, (0, 7), (0, 10), vec![player])]
    }

    #[test]
    fn finds_the_symbol_at_the_caret() {
        let path = Path::new("/p/Assets/Player.cs");
        let at = |line, col| symbol_at(path, TEXT, &outline(), Pos::new(line, col));
        let class = CodeSymbol { class: "Game.Player".into(), script_class: true, member: Member::Class };
        assert_eq!(at(1, 17), Some(class.clone()));
        let field = CodeSymbol { member: Member::Field("speed".into()), ..class.clone() };
        assert_eq!(at(2, 12), Some(field.clone()));
        // A mention of the field in a method body.
        assert_eq!(at(3, 19), Some(field));
        assert_eq!(at(3, 10), Some(CodeSymbol { member: Member::Method("Hit".into()), ..class.clone() }));
        let go = at(4, 24).unwrap();
        assert_eq!((go.class.as_str(), go.script_class), ("Game.Player+Inner", false));
        assert_eq!(go.name(), "Go");
        assert_eq!(at(1, 3), None, "a keyword");
        // A different file name: not the script class.
        assert!(!symbol_at(Path::new("/p/Other.cs"), TEXT, &outline(), Pos::new(1, 17)).unwrap().script_class);
    }

    #[test]
    fn reads_the_word_in_utf16_columns() {
        assert_eq!(word_at("a 😀 bé_c", Pos::new(0, 6)).as_deref(), Some("bé_c"));
        assert_eq!(word_at("x = @class;", Pos::new(0, 6)).as_deref(), Some("class"));
        assert_eq!(word_at("  ", Pos::new(0, 1)), None);
    }
}
