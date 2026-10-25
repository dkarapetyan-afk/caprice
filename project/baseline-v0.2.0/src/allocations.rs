//! Allocation counting is run separately from timing, after one corpus warmup.
use caprice::{Engine, Mode, Scratch};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    hint::black_box,
    sync::atomic::{AtomicUsize, Ordering},
};
struct Counting;
static COUNT: AtomicUsize = AtomicUsize::new(0);
#[global_allocator]
static ALLOC: Counting = Counting;
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 {
        COUNT.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(l) }
    }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 {
        COUNT.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc_zeroed(l) }
    }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 {
        COUNT.fetch_add(1, Ordering::Relaxed);
        unsafe { System.realloc(p, l, n) }
    }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) {
        unsafe { System.dealloc(p, l) }
    }
}
fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or("benchmarks/corpus.json".into());
    let corpus: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    println!("workload,engine,inputs,allocations");
    for w in corpus.as_array().unwrap() {
        let params = w["params"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str().unwrap().as_bytes().to_vec()))
            .collect();
        let e = Engine::new(w["pattern"].as_str().unwrap(), params, Mode::Jit).unwrap();
        let inputs: Vec<_> = w["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["text"].as_str().unwrap().as_bytes())
            .collect();
        for reuse in [false, true] {
            let mut scratch = Scratch::default();
            for s in &inputs {
                black_box(e.is_match_with(s, &mut scratch).unwrap());
            }
            COUNT.store(0, Ordering::SeqCst);
            for s in &inputs {
                black_box(
                    if reuse {
                        e.is_match_with(s, &mut scratch)
                    } else {
                        e.is_match(s)
                    }
                    .unwrap(),
                );
            }
            let count = COUNT.load(Ordering::SeqCst);
            println!(
                "{},{},{},{}",
                w["id"].as_str().unwrap(),
                if reuse { "jit-reuse" } else { "jit" },
                inputs.len(),
                count
            );
        }
    }
}
