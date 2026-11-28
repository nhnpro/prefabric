//! Speedscope JSON profile parser (`dotnet-trace collect --format Speedscope`).
//!
//! Supports both `"evented"` profiles (`O`/`C` open/close events with timestamps — what
//! `dotnet-trace` emits) and `"sampled"` profiles (`samples` + `weights`).
//!
//! Frames in `shared.frames` are interned into [`TreeBuilder`] once up front so evented
//! and sampled streams walk tree nodes directly in O(1) per step without cloning strings.

use crate::ProfileFrameKind;
use crate::tree::{FrameKey, TreeBuilder};
use serde_json::Value;
use std::path::Path;

fn scale_to_ms(unit: &str) -> Option<f64> {
    match unit {
        "nanoseconds" => Some(1e-6),
        "microseconds" => Some(1e-3),
        "milliseconds" => Some(1.0),
        "seconds" => Some(1000.0),
        _ => None,
    }
}

fn intern_frames(b: &mut TreeBuilder, v: &Value, base: &Path) -> Vec<u32> {
    v.pointer("/shared/frames")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|f| {
            let name = f.get("name").and_then(Value::as_str).unwrap_or("(unknown)").trim();
            let name = if name.is_empty() { "(unknown)".to_owned() } else { name.to_owned() };
            let file = f.get("file").and_then(Value::as_str).filter(|s| !s.is_empty()).map(|raw| {
                let p = Path::new(raw);
                if p.is_absolute() { p.to_path_buf() } else { base.join(p) }
            });
            // Speedscope lines are 1-based when present.
            let line = file.as_ref().and(f.get("line").and_then(Value::as_u64)).map(|l| l.saturating_sub(1) as u32);
            b.frame(FrameKey { name, file, line, kind: ProfileFrameKind::Code })
        })
        .collect()
}

/// Adds all profiles from a Speedscope JSON document into `b`.
/// Returns `true` if any time-based profile unit (`ms`/`us`/`ns`/`s`) contributed samples.
pub(crate) fn add_speedscope(b: &mut TreeBuilder, json: &str, base: &Path) -> Result<bool, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let profiles = v.get("profiles").and_then(Value::as_array).ok_or("not a Speedscope profile: no profiles")?;
    let frame_ids = intern_frames(b, &v, base);
    let mut time_based = false;

    for profile in profiles {
        let kind = profile.get("type").and_then(Value::as_str).unwrap_or_default();
        let unit = profile.get("unit").and_then(Value::as_str).unwrap_or("none");
        let (is_time, scale) = match scale_to_ms(unit) {
            Some(s) => (true, s),
            None => (false, 1.0),
        };
        let mut added = 0.0;

        match kind {
            "sampled" => {
                let samples = profile.get("samples").and_then(Value::as_array);
                let weights = profile.get("weights").and_then(Value::as_array);
                if let Some(samples) = samples {
                    for (i, sample) in samples.iter().enumerate() {
                        let weight = weights.and_then(|w| w.get(i)).and_then(Value::as_f64).unwrap_or(1.0) * scale;
                        if weight <= 0.0 {
                            continue;
                        }
                        let mut node = 0u32;
                        let mut any = false;
                        for id in sample.as_array().into_iter().flatten() {
                            if let Some(fid) = id.as_u64().and_then(|idx| frame_ids.get(idx as usize)).copied() {
                                node = b.child(node, fid);
                                any = true;
                            }
                        }
                        if any {
                            b.add_self(node, weight);
                            added += weight;
                        }
                    }
                }
            }
            "evented" => {
                let Some(events) = profile.get("events").and_then(Value::as_array) else { continue };
                // Stack of `(speedscope_frame_idx, tree_node_id)` rooted below node 0.
                let mut stack: Vec<(usize, u32)> = Vec::new();
                let mut last_at: Option<f64> = None;
                for ev in events {
                    let Some(ev_type) = ev.get("type").and_then(Value::as_str) else { continue };
                    let Some(at) = ev.get("at").and_then(Value::as_f64) else { continue };
                    let Some(frame_idx) = ev.get("frame").and_then(Value::as_u64).map(|n| n as usize) else {
                        continue;
                    };
                    if let Some(prev) = last_at
                        && let Some(&(_, cur_node)) = stack.last()
                    {
                        let dt = (at - prev) * scale;
                        if dt > 0.0 {
                            b.add_self(cur_node, dt);
                            added += dt;
                        }
                    }
                    match ev_type {
                        "O" => {
                            if let Some(&fid) = frame_ids.get(frame_idx) {
                                let parent = stack.last().map_or(0, |&(_, n)| n);
                                let child = b.child(parent, fid);
                                stack.push((frame_idx, child));
                            }
                        }
                        "C" => {
                            if let Some(pos) = stack.iter().rposition(|&(f, _)| f == frame_idx) {
                                stack.truncate(pos);
                            } else {
                                stack.pop();
                            }
                        }
                        _ => {}
                    }
                    last_at = Some(at);
                }
            }
            _ => {}
        }

        if is_time && added > 0.0 {
            time_based = true;
        }
    }

    Ok(time_based)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ProfileMode;

    #[test]
    fn parses_evented_dotnet_trace_speedscope() {
        let json = r#"{
            "shared": {
                "frames": [
                    { "name": "MyApp!MyApp.Program.Main()", "file": "Program.cs", "line": 10 },
                    { "name": "MyApp!MyApp.Worker.Run()", "file": "/abs/Worker.cs", "line": 25 }
                ]
            },
            "profiles": [
                {
                    "type": "evented",
                    "name": "Thread (1)",
                    "unit": "milliseconds",
                    "events": [
                        { "type": "O", "at": 0.0, "frame": 0 },
                        { "type": "O", "at": 2.5, "frame": 1 },
                        { "type": "C", "at": 7.5, "frame": 1 },
                        { "type": "C", "at": 9.0, "frame": 0 }
                    ]
                }
            ]
        }"#;
        let mut b = TreeBuilder::new();
        let time_based = add_speedscope(&mut b, json, Path::new("/proj")).unwrap();
        assert!(time_based);
        let p = b.finish("dotnet-trace", "ms", ProfileMode::Cpu);
        assert!((p.total - 9.0).abs() < 1e-9);
        let main_idx = p.frames.iter().position(|f| f.name == "MyApp!MyApp.Program.Main()").unwrap();
        assert_eq!(p.frames[main_idx].file.as_deref(), Some(Path::new("/proj/Program.cs")));
        assert_eq!(p.frames[main_idx].line, Some(9));
        let main_stat = p.methods.iter().find(|m| m.frame == main_idx as u32).unwrap();
        assert!((main_stat.self_weight - 4.0).abs() < 1e-9);
        assert!((main_stat.total - 9.0).abs() < 1e-9);
        let worker_idx = p.frames.iter().position(|f| f.name == "MyApp!MyApp.Worker.Run()").unwrap() as u32;
        let worker_stat = p.methods.iter().find(|m| m.frame == worker_idx).unwrap();
        assert!((worker_stat.self_weight - 5.0).abs() < 1e-9);
    }

    #[test]
    fn parses_sampled_speedscope_with_microseconds() {
        let json = r#"{
            "shared": {
                "frames": [
                    { "name": "Main" },
                    { "name": "Compute" }
                ]
            },
            "profiles": [
                {
                    "type": "sampled",
                    "name": "CPU",
                    "unit": "microseconds",
                    "samples": [[0, 1], [0]],
                    "weights": [3000, 1000]
                }
            ]
        }"#;
        let mut b = TreeBuilder::new();
        let time_based = add_speedscope(&mut b, json, Path::new("/proj")).unwrap();
        assert!(time_based);
        let p = b.finish("dotnet-trace", "ms", ProfileMode::Cpu);
        assert!((p.total - 4.0).abs() < 1e-9);
    }
}
