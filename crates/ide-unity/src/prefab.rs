//! Prefab instances: which object of which prefab an instance's object stands for (through
//! variants and nested prefabs), and the value a property has once every override applies.
//!
//! Unity doesn't write the objects of a prefab instance into the file that uses it, only overrides
//! of them. An object reached through an instance has the fileID `instance ^ source` (both 63-bit),
//! where `instance` is the PrefabInstance and `source` the object's fileID in the prefab; a
//! `stripped` object stands for one explicitly, when something in the file refers to it.

use crate::scan::{Guid, Modification, ObjRef, Object, ScannedFile};
use std::collections::{HashMap, HashSet};

/// Scanned assets by GUID.
pub trait Assets {
    fn file(&self, guid: Guid) -> Option<&ScannedFile>;
}

impl Assets for HashMap<Guid, ScannedFile> {
    fn file(&self, guid: Guid) -> Option<&ScannedFile> {
        self.get(&guid)
    }
}

/// Nested deeper than this is a cycle (a broken project).
const MAX_DEPTH: usize = 32;

/// One step from a file into the prefab one of its instances comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hop {
    /// The file holding the PrefabInstance.
    pub file: Guid,
    /// The PrefabInstance's fileID.
    pub instance: i64,
    /// The prefab, and the object's fileID in it.
    pub source: Guid,
    pub source_id: i64,
}

/// The object that actually serializes an object of a file, and the instances it was reached
/// through, outermost first. Without hops, the object is in the file itself.
#[derive(Debug)]
pub struct Origin<'a> {
    pub file: Guid,
    pub object: &'a Object,
    pub hops: Vec<Hop>,
}

impl ScannedFile {
    pub fn object(&self, file_id: i64) -> Option<&Object> {
        self.objects.iter().find(|o| o.file_id == file_id)
    }

    pub fn prefab_instances(&self) -> impl Iterator<Item = &Object> {
        self.objects.iter().filter(|o| o.source_prefab.is_some())
    }
}

/// Where the object `file_id` of `file` comes from.
pub fn resolve(assets: &impl Assets, file: Guid, file_id: i64) -> Option<Origin<'_>> {
    let mut hops = Vec::new();
    let (file, object) = walk(assets, file, file_id, &mut hops, &mut HashSet::new())?;
    Some(Origin { file, object, hops })
}

fn walk<'a>(
    assets: &'a impl Assets,
    file: Guid,
    file_id: i64,
    hops: &mut Vec<Hop>,
    seen: &mut HashSet<(Guid, i64)>,
) -> Option<(Guid, &'a Object)> {
    // A pair tried before failed then, or is a cycle now.
    if hops.len() > MAX_DEPTH || !seen.insert((file, file_id)) {
        return None;
    }
    let scanned = assets.file(file)?;
    if let Some(object) = scanned.object(file_id) {
        if !object.stripped {
            return Some((file, object));
        }
        let (instance, source) = (object.prefab_instance?, object.source?);
        let source_file = source.guid?;
        hops.push(Hop { file, instance, source: source_file, source_id: source.file_id });
        return walk(assets, source_file, source.file_id, hops, seen);
    }
    for instance in scanned.prefab_instances() {
        let Some(source_file) = instance.source_prefab.and_then(|p| p.guid) else { continue };
        let source_id = file_id ^ instance.file_id;
        if source_id <= 0 {
            continue;
        }
        hops.push(Hop { file, instance: instance.file_id, source: source_file, source_id });
        let mark = hops.len();
        if let Some(found) = walk(assets, source_file, source_id, hops, seen) {
            return Some(found);
        }
        hops.truncate(mark - 1);
    }
    None
}

/// A property's value on an object, once every override applies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PropertyValue {
    pub value: Option<Box<str>>,
    pub object_reference: Option<ObjRef>,
    /// The file and line of the override that sets it; `None` when it is the object's own value.
    pub overridden_at: Option<(Guid, u32)>,
}

/// The value of `path` (`speed`, `m_Name`, `m_LocalPosition.x`…) on the object `file_id` of `file`:
/// the outermost override wins, then the value serialized in the object. `None` when the object is
/// unknown, or nothing sets the property and it isn't one of the values the scanner keeps.
pub fn property(assets: &impl Assets, file: Guid, file_id: i64, path: &str) -> Option<PropertyValue> {
    let origin = resolve(assets, file, file_id)?;
    for hop in &origin.hops {
        if let Some(m) = override_of(assets, hop, path) {
            return Some(PropertyValue {
                value: m.value.clone(),
                object_reference: m.object_reference,
                overridden_at: Some((hop.file, m.line)),
            });
        }
    }
    let object = origin.object;
    let value = match path {
        "m_Name" => object.name.clone(),
        _ => Some(object.properties.iter().find(|(k, _)| &**k == path)?.1.clone()),
    };
    Some(PropertyValue { value, object_reference: None, overridden_at: None })
}

/// The last override of `path` the hop's instance makes on its object (Unity keeps one per path).
fn override_of<'a>(assets: &'a impl Assets, hop: &Hop, path: &str) -> Option<&'a Modification> {
    let instance = assets.file(hop.file)?.object(hop.instance)?;
    instance
        .modifications
        .iter()
        .rev()
        .find(|m| m.target.file_id == hop.source_id && m.target.guid == Some(hop.source) && &*m.property_path == path)
}

/// Every override applying to the object `file_id` of `file`, outermost first: what the
/// Inspector shows in bold. An inner override hidden by an outer one of the same path is left out.
pub fn overrides(assets: &impl Assets, file: Guid, file_id: i64) -> Vec<(Guid, &Modification)> {
    let Some(origin) = resolve(assets, file, file_id) else { return Vec::new() };
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for hop in &origin.hops {
        let Some(instance) = assets.file(hop.file).and_then(|f| f.object(hop.instance)) else { continue };
        for m in instance.modifications.iter().rev() {
            if m.target.file_id == hop.source_id
                && m.target.guid == Some(hop.source)
                && seen.insert(m.property_path.clone())
            {
                out.push((hop.file, m));
            }
        }
    }
    out
}

#[cfg(test)]
#[path = "prefab_tests.rs"]
mod tests;
