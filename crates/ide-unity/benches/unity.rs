//! Full scan and queries of a generated project, and the heap the graph holds.
//! `cargo bench -p ide-unity` (A8 adds budgets).

use ide_core::NullSink;
use ide_unity::{Guid, UnityIndex, synthetic};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn main() {
    let (scripts, prefabs, scenes, instances) = (500, 4_000, 500, 20);
    let project = tempfile::tempdir().unwrap();
    let assets = synthetic::write_project(project.path(), scripts, prefabs, scenes, instances).unwrap();
    let store = tempfile::tempdir().unwrap();
    for pass in ["cold", "warm"] {
        let started = Instant::now();
        let index = UnityIndex::open(project.path(), store.path(), Arc::new(NullSink), Box::new(|| true)).unwrap();
        assert!(index.wait_idle(Duration::from_secs(600)));
        println!("unity/full_scan_{pass}: {assets} assets in {:.1} ms", started.elapsed().as_secs_f64() * 1e3);
        if pass == "warm" {
            let script = Guid::parse(synthetic::script_guid(7).as_bytes()).unwrap();
            let started = Instant::now();
            let usages = index.script_usages(script).len();
            println!("unity/script_usages: {usages} usages in {:.2} ms", started.elapsed().as_secs_f64() * 1e3);
            let started = Instant::now();
            let values = index.field_values(script, "speed").len();
            println!("unity/field_values: {values} values in {:.2} ms", started.elapsed().as_secs_f64() * 1e3);
            let started = Instant::now();
            let bindings = index.method_bindings("OnHit7", None).len();
            println!("unity/method_bindings: {bindings} bindings in {:.2} ms", started.elapsed().as_secs_f64() * 1e3);
            println!("unity/graph_heap: {:.1} KiB", index.heap_bytes() as f64 / 1024.0);
        }
    }
}
