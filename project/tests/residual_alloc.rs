//! Reused match state must not allocate while a residual guard or interpolation runs.
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
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        COUNT.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        COUNT.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new: usize) -> *mut u8 {
        COUNT.fetch_add(1, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, new) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}
fn quiet(mode: Mode, pattern: &str, inputs: &[&[u8]], accept: fn(&[u8]) -> bool) {
    let engine = Engine::new(pattern, caprice::expr::Params::new(), mode).unwrap();
    let mut scratch = Scratch::default();
    for input in inputs {
        assert_eq!(
            engine.is_match_with(input, &mut scratch).unwrap(),
            accept(input),
            "{mode:?} {pattern} {input:?}"
        );
    }
    COUNT.store(0, Ordering::SeqCst);
    for input in inputs {
        assert_eq!(
            black_box(engine.is_match_with(input, &mut scratch).unwrap()),
            accept(input)
        );
    }
    assert_eq!(
        COUNT.load(Ordering::SeqCst),
        0,
        "{mode:?} {pattern} allocated during reused matches"
    );
}
#[test]
fn compound_guard_and_interpolation_allocate_nothing_on_reuse() {
    let mut texts = vec![Vec::new()];
    for n in 0..=32 {
        texts.push(vec![b'a'; n]);
        texts.push(vec![b'b'; n]);
    }
    texts.push(b"ab".to_vec());
    texts.push(b"ba".repeat(8));
    let inputs: Vec<&[u8]> = texts.iter().map(Vec::as_slice).collect();
    let compound = r#"(?<x>[ab]{0,32})(?{(and (le (len (cap x)) 16) (not (eq (cap x) "b")))})"#;
    let lambda = r#"(?<x>[ab]{0,32})(?{(call (lambda (v Str) (and (le (len v) 16) (not (eq v "b")))) (cap x))})"#;
    let accept = |input: &[u8]| input.len() <= 16 && input != b"b";
    for mode in [Mode::Raw, Mode::Pe, Mode::Jit] {
        quiet(mode, compound, &inputs, accept);
        quiet(mode, lambda, &inputs, accept);
        quiet(
            mode,
            r#"(?<x>ab)${(slice (concat (upper (cap x)) "cd") 1 3)}"#,
            &[b"abBc"],
            |_| true,
        );
    }
}
