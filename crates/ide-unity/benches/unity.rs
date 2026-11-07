//! `cargo bench -p ide-unity`: the asset index of a generated 50,000-asset Unity project — full
//! scans (cold store, warm store), a saved prefab reaching the queries, the queries — and the heap
//! the index holds, written to `target/perf/extra/unity.json` for `pnpm perf:rust`.

use criterion::{BatchSize, Criterion, black_box, criterion_group, criterion_main};
use ide_core::NullSink;
use ide_unity::synthetic::{self, Shape};
use ide_unity::{Guid, UnityIndex};
use std::alloc::{GlobalAlloc, Layout, System};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// Bytes allocated and not yet freed.
struct Counting;

static LIVE: AtomicUsize = AtomicUsize::new(0);

// SAFETY: forwards to the system allocator, only counting sizes.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        LIVE.fetch_add(layout.size(), Ordering::Relaxed);
        // SAFETY: the caller's contract is passed on unchanged.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        // SAFETY: as above.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

const SHAPE: Shape = Shape { scripts: 2_000, prefabs: 30_000, scenes: 1_000, instances: 50, others: 17_000 };

fn open(project: &Path, store: &Path) -> Arc<UnityIndex> {
    let index = UnityIndex::open(project, store, Arc::new(NullSink), Box::new(|| true)).unwrap();
    assert!(index.wait_idle(Duration::from_secs(600)));
    index
}

fn guid(hex: String) -> Guid {
    Guid::parse(hex.as_bytes()).unwrap()
}

fn write_extra(metrics: &[(&str, f64)]) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/perf/extra");
    let _ = std::fs::create_dir_all(&dir);
    let body: Vec<String> = metrics.iter().map(|(k, v)| format!("  \"{k}\": {v:.1}")).collect();
    let _ = std::fs::write(dir.join("unity.json"), format!("{{\n{}\n}}\n", body.join(",\n")));
}

fn bench(c: &mut Criterion) {
    let project = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(project.path()).unwrap();
    let started = Instant::now();
    assert_eq!(synthetic::write_project(&root, SHAPE).unwrap(), 50_000);
    eprintln!("generated {} assets in {:?}", SHAPE.assets(), started.elapsed());

    let mut g = c.benchmark_group("unity_50k");
    g.sample_size(10);
    g.measurement_time(Duration::from_secs(20));
    g.bench_function("full_scan_cold", |b| {
        b.iter_batched(
            || tempfile::tempdir().unwrap(),
            |store| drop(open(&root, store.path())),
            BatchSize::PerIteration,
        )
    });
    let store = tempfile::tempdir().unwrap();
    drop(open(&root, store.path()));
    g.bench_function("full_scan_warm", |b| b.iter(|| drop(open(&root, store.path()))));

    // The heap the index holds once a project is open and scanned (warm, nothing queried yet).
    let before = LIVE.load(Ordering::Relaxed);
    let index = open(&root, store.path());
    let heap = LIVE.load(Ordering::Relaxed).saturating_sub(before);
    eprintln!(
        "unity_50k index heap: {:.1} MiB (graph {:.1} MiB)",
        heap as f64 / 1048576.0,
        index.heap_bytes() as f64 / 1048576.0
    );
    write_extra(&[("unity_50k/index_heap_kib", heap as f64 / 1024.0)]);

    let script = guid(synthetic::script_guid(7));
    let prefab = root.join(synthetic::prefab_path(7));
    let original = std::fs::read_to_string(&prefab).unwrap();
    let mut speed = 1_000;
    g.bench_function("update_saved_prefab", |b| {
        b.iter(|| {
            speed += 1;
            let text = speed.to_string();
            std::fs::write(&prefab, original.replace("  speed: 7\n", &format!("  speed: {speed}\n"))).unwrap();
            index.update(std::slice::from_ref(&prefab), &[]);
            while !index.field_values(script, "speed").iter().any(|v| v.value.as_deref() == Some(text.as_str())) {
                std::thread::sleep(Duration::from_micros(200));
            }
        })
    });
    g.sample_size(20);
    g.measurement_time(Duration::from_secs(5));
    g.bench_function("script_usages", |b| b.iter(|| black_box(index.script_usages(script).len())));
    g.bench_function("field_values", |b| b.iter(|| black_box(index.field_values(script, "speed").len())));
    g.bench_function("method_bindings", |b| b.iter(|| black_box(index.method_bindings("OnHit7", None).len())));
    let prefab_guid = guid(synthetic::prefab_guid(7));
    g.bench_function("referrers", |b| b.iter(|| black_box(index.referrers(prefab_guid).len())));
    g.finish();
}

criterion_group!(benches, bench);
criterion_main!(benches);
