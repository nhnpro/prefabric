//! UnityEvent listeners a prefab instance sets through overrides. A listener added to (or changed
//! on) an instance's component is written as overrides of its properties:
//! `<field>.m_PersistentCalls.m_Calls.Array.data[<i>].m_MethodName`, `….m_Target`…

use crate::scan::{EventCall, ObjRef, Object};

const CALLS: &str = ".m_PersistentCalls.m_Calls.Array.data[";

/// The listeners `instance` (a PrefabInstance) sets, with the object of its prefab they belong to.
/// A listener whose method isn't overridden is left out: it is the prefab's own, found there.
pub fn override_calls(instance: &Object) -> Vec<(ObjRef, EventCall)> {
    let mut out: Vec<(ObjRef, usize, EventCall)> = Vec::new();
    for m in &instance.modifications {
        let Some((field, rest)) = m.property_path.split_once(CALLS) else { continue };
        let Some((index, property)) = rest.split_once("].") else { continue };
        let Ok(index) = index.parse::<usize>() else { continue };
        let at = match out.iter().position(|(t, i, c)| *t == m.target && *i == index && &*c.field == field) {
            Some(at) => at,
            None => {
                let call = EventCall {
                    field: field.into(),
                    target: None,
                    target_type: None,
                    method: "".into(),
                    mode: 0,
                    call_state: 0,
                    line: m.line,
                };
                out.push((m.target, index, call));
                out.len() - 1
            }
        };
        let call = &mut out[at].2;
        let small = || m.value.as_deref().and_then(|v| v.parse().ok()).unwrap_or(0);
        match property {
            "m_Target" => call.target = m.object_reference,
            "m_TargetAssemblyTypeName" => call.target_type = m.value.clone(),
            "m_MethodName" => call.method = m.value.clone().unwrap_or_default(),
            "m_Mode" => call.mode = small(),
            "m_CallState" => call.call_state = small(),
            _ => {}
        }
    }
    out.into_iter().filter(|(_, _, c)| !c.method.is_empty()).map(|(t, _, c)| (t, c)).collect()
}

#[cfg(test)]
#[path = "events_tests.rs"]
mod tests;
