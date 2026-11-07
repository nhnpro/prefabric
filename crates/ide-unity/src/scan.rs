//! Byte-level reading of Unity's serialized YAML (scenes, prefabs, assets).
//!
//! A file is a list of documents, one per object: `--- !u!<class> &<fileId>` (`stripped` when
//! the object only stands for one in a prefab instance), the object's type on the next line, then
//! its properties indented by two spaces. Only the properties that tie objects together are read,
//! line by line and without building a YAML tree, so files of any size scan in one pass; lines that
//! don't fit the format are skipped. Never panics on any input.

use memchr::memchr;
use std::fmt;

/// An asset's GUID: 32 hex digits in the `.meta` file and in references.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Guid(pub u128);

impl Guid {
    pub fn parse(hex: &[u8]) -> Option<Self> {
        if hex.len() != 32 {
            return None;
        }
        let mut value = 0u128;
        for &b in hex {
            value = (value << 4) | u128::from((b as char).to_digit(16)?);
        }
        Some(Self(value))
    }
}

impl fmt::Display for Guid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:032x}", self.0)
    }
}

impl fmt::Debug for Guid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Guid({self})")
    }
}

/// `{fileID: <id>, guid: <guid>, type: <n>}`; without a GUID it points into the same file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ObjRef {
    pub file_id: i64,
    pub guid: Option<Guid>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Object {
    /// Unity's class id: 1 GameObject, 4 Transform, 114 MonoBehaviour, 1001 PrefabInstance…
    pub class_id: u32,
    pub file_id: i64,
    /// Stands for an object of a prefab instance (`--- !u!… stripped`).
    pub stripped: bool,
    /// 0-based line of the `---` header.
    pub line: u32,
    /// `GameObject`, `MonoBehaviour`…
    pub type_name: Box<str>,
    pub name: Option<Box<str>>,
    /// A MonoBehaviour's or ScriptableObject's script.
    pub script: Option<ObjRef>,
    /// A component's GameObject.
    pub game_object: Option<i64>,
    /// A Transform's parent Transform.
    pub father: Option<i64>,
    /// A Transform's child Transforms.
    pub children: Vec<i64>,
    /// A GameObject's components.
    pub components: Vec<i64>,
    /// The prefab instance an object belongs to (`m_PrefabInstance`).
    pub prefab_instance: Option<i64>,
    /// The object of the source prefab this one comes from (`m_CorrespondingSourceObject`).
    pub source: Option<ObjRef>,
    /// A PrefabInstance's prefab.
    pub source_prefab: Option<ObjRef>,
    /// A PrefabInstance's parent Transform in this file.
    pub transform_parent: Option<i64>,
    /// A PrefabInstance's overrides, in file order.
    pub modifications: Vec<Modification>,
    /// Components and GameObjects of the prefab a PrefabInstance removes.
    pub removed: Vec<ObjRef>,
    /// A MonoBehaviour's serialized fields written on one line (`speed: 2`), in file order.
    pub properties: Vec<(Box<str>, Box<str>)>,
}

/// An override of a PrefabInstance: `propertyPath` of `target` (an object of the source prefab) is
/// `value`, or `objectReference` for a reference field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Modification {
    pub target: ObjRef,
    pub property_path: Box<str>,
    pub value: Option<Box<str>>,
    pub object_reference: Option<ObjRef>,
    /// 0-based line of the `- target:` item.
    pub line: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScannedFile {
    pub objects: Vec<Object>,
}

/// Which top-level list the following `- ` items belong to.
#[derive(Clone, Copy, PartialEq, Eq)]
enum List {
    None,
    Children,
    Components,
    /// Inside a PrefabInstance's `m_Modification`.
    Modification,
}

/// Which list of `m_Modification` the following four-space-indented `- ` items belong to.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ModList {
    None,
    Modifications,
    Removed,
}

pub fn scan(bytes: &[u8]) -> ScannedFile {
    let mut file = ScannedFile::default();
    let mut current: Option<Object> = None;
    let mut expect_type = false;
    let mut list = List::None;
    let mut mod_list = ModList::None;
    for (number, line) in lines(bytes) {
        if let Some(rest) = line.strip_prefix(b"--- !u!") {
            file.objects.extend(current.take());
            current = header(rest, number);
            expect_type = current.is_some();
            list = List::None;
            continue;
        }
        let Some(object) = current.as_mut() else { continue };
        if expect_type {
            expect_type = false;
            if let Some(name) = line.strip_suffix(b":")
                && !name.is_empty()
                && !name.starts_with(b" ")
            {
                object.type_name = String::from_utf8_lossy(name).into();
                continue;
            }
        }
        let Some(body) = line.strip_prefix(b"  ") else {
            list = List::None;
            continue;
        };
        if let Some(nested) = body.strip_prefix(b"  ") {
            if list == List::Modification {
                modification_line(object, nested, number, &mut mod_list);
            }
            continue;
        }
        if let Some(item) = body.strip_prefix(b"- ") {
            match list {
                List::Children => object.children.extend(reference(item).map(|r| r.file_id)),
                List::Components => {
                    if let Some(r) = item.strip_prefix(b"component:").and_then(reference) {
                        object.components.push(r.file_id);
                    }
                }
                List::None | List::Modification => {}
            }
            continue;
        }
        list = List::None;
        let Some(colon) = memchr(b':', body) else { continue };
        let (key, value) = (&body[..colon], trim(&body[colon + 1..]));
        match key {
            b"m_SourcePrefab" => object.source_prefab = reference(value),
            b"m_Modification" if value.is_empty() => {
                list = List::Modification;
                mod_list = ModList::None;
            }
            b"m_Name" => object.name = scalar(value),
            b"m_Script" => object.script = reference(value),
            b"m_GameObject" => object.game_object = reference(value).map(|r| r.file_id),
            b"m_Father" => object.father = reference(value).map(|r| r.file_id),
            b"m_PrefabInstance" => object.prefab_instance = reference(value).map(|r| r.file_id),
            b"m_CorrespondingSourceObject" => object.source = reference(value),
            b"m_Children" if value.is_empty() => list = List::Children,
            b"m_Component" if value.is_empty() => list = List::Components,
            _ if object.class_id == MONO_BEHAVIOUR && !value.is_empty() && !key.starts_with(b"m_") => {
                if let Some(value) = scalar(value) {
                    object.properties.push((String::from_utf8_lossy(key).into(), value));
                }
            }
            _ => {}
        }
    }
    file.objects.extend(current);
    file
}

const MONO_BEHAVIOUR: u32 = 114;

/// A line of `m_Modification`, without its first four spaces of indentation.
fn modification_line(object: &mut Object, line: &[u8], number: u32, list: &mut ModList) {
    if let Some(item) = line.strip_prefix(b"- ") {
        match *list {
            ModList::Modifications => {
                if let Some(target) = item.strip_prefix(b"target:").and_then(reference) {
                    object.modifications.push(Modification {
                        target,
                        property_path: "".into(),
                        value: None,
                        object_reference: None,
                        line: number,
                    });
                }
            }
            ModList::Removed => object.removed.extend(reference(item)),
            ModList::None => {}
        }
        return;
    }
    if let Some(field) = line.strip_prefix(b"  ") {
        // A property of the last `- target:` item.
        if *list != ModList::Modifications || field.starts_with(b" ") {
            return;
        }
        let Some(m) = object.modifications.last_mut() else { return };
        let Some(colon) = memchr(b':', field) else { return };
        let value = trim(&field[colon + 1..]);
        match &field[..colon] {
            b"propertyPath" => m.property_path = scalar(value).unwrap_or_default(),
            b"value" => m.value = scalar(value),
            b"objectReference" => m.object_reference = reference(value),
            _ => {}
        }
        return;
    }
    *list = ModList::None;
    let Some(colon) = memchr(b':', line) else { return };
    let value = trim(&line[colon + 1..]);
    match &line[..colon] {
        b"m_TransformParent" => object.transform_parent = reference(value).map(|r| r.file_id),
        b"m_Modifications" if value.is_empty() => *list = ModList::Modifications,
        b"m_RemovedComponents" | b"m_RemovedGameObjects" if value.is_empty() => *list = ModList::Removed,
        _ => {}
    }
}

/// Lines without their ending (`\n` or `\r\n`), with their 0-based numbers.
fn lines(bytes: &[u8]) -> impl Iterator<Item = (u32, &[u8])> {
    let mut rest = bytes;
    let mut number = 0u32;
    std::iter::from_fn(move || {
        if rest.is_empty() {
            return None;
        }
        let (line, next) = match memchr(b'\n', rest) {
            Some(end) => (&rest[..end], &rest[end + 1..]),
            None => (rest, &rest[rest.len()..]),
        };
        rest = next;
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        let item = (number, line);
        number = number.saturating_add(1);
        Some(item)
    })
}

/// `<class> &<fileId>[ stripped]`, after `--- !u!`.
fn header(rest: &[u8], line: u32) -> Option<Object> {
    let (class_id, rest) = number(rest)?;
    let rest = rest.strip_prefix(b" &")?;
    let (file_id, rest) = number(rest)?;
    Some(Object {
        class_id: u32::try_from(class_id).ok()?,
        file_id,
        stripped: trim(rest) == b"stripped",
        line,
        ..Object::default()
    })
}

/// A leading (optionally negative) integer, and what follows it.
fn number(bytes: &[u8]) -> Option<(i64, &[u8])> {
    let digits_from = usize::from(bytes.first() == Some(&b'-'));
    let len = bytes[digits_from..].iter().take_while(|b| b.is_ascii_digit()).count();
    if len == 0 {
        return None;
    }
    let end = digits_from + len;
    let value = std::str::from_utf8(&bytes[..end]).ok()?.parse().ok()?;
    Some((value, &bytes[end..]))
}

/// `{fileID: <id>[, guid: <guid>, type: <n>]}`; `None` for a null reference (`fileID: 0`).
pub(crate) fn reference(value: &[u8]) -> Option<ObjRef> {
    let value = trim(value);
    let inner = value.strip_prefix(b"{")?;
    let inner = inner.strip_suffix(b"}").unwrap_or(inner);
    let mut file_id = None;
    let mut guid = None;
    for part in inner.split(|&b| b == b',') {
        let part = trim(part);
        if let Some(v) = part.strip_prefix(b"fileID:") {
            file_id = number(trim(v)).map(|(n, _)| n);
        } else if let Some(v) = part.strip_prefix(b"guid:") {
            guid = Guid::parse(trim(v));
        }
    }
    match file_id? {
        0 => None,
        file_id => Some(ObjRef { file_id, guid }),
    }
}

/// A one-line YAML scalar: plain, `'single'` (`''` escapes a quote) or `"double"` quoted.
/// `None` when empty.
pub(crate) fn scalar(value: &[u8]) -> Option<Box<str>> {
    let value = trim(value);
    let text = match value.first() {
        Some(b'\'') => {
            let body = &value[1..];
            let body = body.strip_suffix(b"'").unwrap_or(body);
            String::from_utf8_lossy(body).replace("''", "'")
        }
        Some(b'"') => {
            let body = &value[1..];
            let body = body.strip_suffix(b"\"").unwrap_or(body);
            unescape(&String::from_utf8_lossy(body))
        }
        _ => String::from_utf8_lossy(value).into_owned(),
    };
    (!text.is_empty()).then(|| text.into())
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                out.extend(u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32));
            }
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

pub(crate) fn trim(bytes: &[u8]) -> &[u8] {
    bytes.trim_ascii()
}

#[cfg(test)]
#[path = "scan_tests.rs"]
mod tests;
