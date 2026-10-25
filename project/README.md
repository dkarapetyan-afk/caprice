# Caprice performance experiment v0.3.0

Read `caprice-performance-note.pdf` for measured results and implementation scope.
Across 44 synthetic workloads, Boolean matching improves **1.75x** and
capture-output calls **1.63x** geometrically versus v0.2. Native
Boolean medians beat PCRE2 10.49 on **36/44** workloads.
The comparison interleaves both versions and PCRE2 in each timing sample.

## New implementation

- Conservative affine capture-length inference: `N = d + k*l`.
- Budget-aware necessary length bounds from selected strict guards.
- Explicit native capture slots; fused class capture operations and undo writes.
- Ordered factoring of contiguous literal prefixes, with capture/guard barriers.
- Specialized native checks for small negated byte sets.
- Bounded process-local `EngineCache` keyed by pattern, parameters and mode.

The surface language remains full-subject byte matching with ASCII transforms.
Unsupported analysis falls back to the original residual execution. Native mode
requires a 64-bit little-endian host and was measured on x86-64. Limits remain
dispatch/stack/expression budgets; scans are not charged per byte and finite
exhaustion can change across optimizer versions.

```rust
use caprice::{cache::EngineCache, expr::Params, Mode, Scratch};
fn main() -> Result<(), String> {
    let mut cache = EngineCache::new(16);
    let engine = cache.get_or_compile(r"(?<x>[ab]{0,16})\k<x>", &Params::new(), Mode::Jit)?;
    let mut scratch = Scratch::default();
    assert!(engine.is_match_with(b"abab", &mut scratch)?);
    Ok(())
}
```

`run_with(input, limits, scratch)` materializes the public capture vector on
success. Inspect `scratch.captures()` only after `MatchStatus::Match` from this
method. `is_match_with` uses native slots directly. Generic expressions refresh
a public view before strict evaluation. A reused match state may retain memory.
The cache bounds retained entries; caller-held `Rc` owners can keep evicted
engines alive. The cache is thread-local. Hits avoid construction; misses still
perform a complete compilation, and automatic tier selection is future work.

## Original and development corpus results

| Workload | v0.2 ns | v0.3 ns | PCRE2 ns | Gain vs v0.2 |
|---|---:|---:|---:|---:|
| Literal | 15.5 | 15.3 | 33.4 | 1.01x |
| Identifier | 37.7 | 33.8 | 47.4 | 1.12x |
| Key/value fields | 43.3 | 30.5 | 37.4 | 1.42x |
| Alternation | 19.8 | 19.7 | 29.3 | 1.00x |
| Short backreference | 29.3 | 23.1 | 42.2 | 1.27x |
| Long backreference | 119.5 | 116.9 | 243.2 | 1.02x |
| Static parameter | 24.2 | 18.0 | 33.1 | 1.34x |
| Lambda guard | 37.8 | 27.5 | 36.6 | 1.37x |
| Dynamic interpolation | 34.3 | 30.2 | 49.0 | 1.14x |
| Length guard | 39.2 | 26.8 | 36.6 | 1.46x |
| Constant lambda | 27.7 | 26.3 | 35.6 | 1.05x |
| 16-byte literal | 16.2 | 16.1 | 27.1 | 1.01x |
| 128-byte literal | 22.3 | 22.4 | 29.6 | 0.99x |
| 512-byte literal | 24.2 | 24.8 | 37.9 | 0.98x |
| Overlapping alternatives | 35.4 | 19.2 | 30.1 | 1.85x |
| Disjoint alternatives | 32.7 | 33.2 | 37.9 | 0.99x |
| Eight captures | 71.4 | 45.9 | 53.2 | 1.55x |
| Backref without delimiter | 172.3 | 24.0 | 56.1 | 7.17x |
| Long backref, mixed failures | 110.6 | 103.6 | 145.3 | 1.07x |
| Lazy repeat with delimiter | 50.4 | 38.8 | 50.9 | 1.30x |
| Repeated capture | 66.0 | 57.0 | 37.9 | 1.16x |
| Long capture guard | 95.7 | 24.1 | 69.8 | 3.98x |
| Interpolation, varied lengths | 52.8 | 47.9 | 61.9 | 1.10x |
| Wide byte class | 93.1 | 93.2 | 204.2 | 1.00x |
| Negated byte class | 92.4 | 72.6 | 59.7 | 1.27x |
| 1 KiB class scan | 653.9 | 638.9 | 740.2 | 1.02x |
| Length guard, larger captures | 113.3 | 26.7 | 82.7 | 4.24x |
| Captures across alternatives | 32.7 | 30.2 | 33.5 | 1.09x |


## Fresh validation corpus

The engine was frozen before these timings were observed. They are synthetic,
and include large inputs, multiple capture lengths and generic guard fallback.

| Workload | v0.2 ns | v0.3 ns | PCRE2 ns | Gain vs v0.2 |
|---|---:|---:|---:|---:|
| Three copies of a capture | 1046.7 | 47.7 | 335.6 | 21.96x |
| Affine length with affixes | 1011.9 | 41.2 | 221.7 | 24.53x |
| Nested capture | 45.2 | 50.0 | 53.7 | 0.90x |
| Two unknown capture lengths | 278.9 | 240.2 | 140.2 | 1.16x |
| Fixed and variable captures | 558.6 | 42.8 | 180.5 | 13.05x |
| Strict length lambda | 48.6 | 28.1 | 43.8 | 1.73x |
| ASCII-folded equality guard | 70.9 | 24.6 | 64.0 | 2.88x |
| Interpolation with affixes | 43.6 | 39.8 | 57.5 | 1.10x |
| Shared-prefix tree | 72.9 | 22.8 | 29.7 | 3.20x |
| Capture barriers in prefixes | 61.4 | 57.8 | 33.9 | 1.06x |
| Repeated captures, 1-16 | 193.8 | 157.2 | 69.6 | 1.23x |
| Empty repeated capture | 107.1 | 87.9 | 44.9 | 1.22x |
| Negated class, control bytes | 303.1 | 160.5 | 244.6 | 1.89x |
| Class scan through 8 KiB | 2001.4 | 1995.9 | 1959.0 | 1.00x |
| Static parameter with backref | 890.6 | 41.5 | 246.8 | 21.44x |
| Generic compound guard | 444.6 | 438.8 | 36.2 | 1.01x |


Generic compound guards remain 12.11x PCRE2 plus the timed host predicate;
this baseline uses a uniquely determined full capture. It is not a substitute
for general backtracking-aware guard semantics. Nested captures regress
10.6% versus v0.2 on the fresh case. Tiny differences in other workloads are
reported as medians without significance claims. The historical workload ID
`class_scan_4096` contains 1,024-1,280 byte subjects, not 4 KiB subjects.

## Verification

20 release Rust tests and Clippy with warnings denied pass. Tests retain the
earlier exhaustive cases and add affine lengths, prefix order, full capture
spans, undo restoration, byte boundaries, mandatory expression-limit errors,
cache keys, LRU behavior and ownership through eviction.

`formal/AffineLengths.lean` checks five arithmetic lemmas in Lean 4.19.0:
uniqueness, quotient recovery, divisibility, and rejection by remainder/upper
bound. Reported axioms are `[propext]`. This does not verify the Rust extractor,
native backend or the full matcher. `formal/Revision3.md` states the remaining
semantic obligations, including the behavioral-fiber interpretation and directed
cost annotations. The original expression proof is retained separately.

## Reproduce

Requirements: Rust compatible with Cargo.lock, Python 3, PCRE2 8-bit development
library, a C toolchain, and Linux taskset. Report generation also needs ReportLab
and DejaVu fonts under `/usr/share/fonts/truetype/dejavu`.

```sh
cargo build --release --locked --example compare
cargo test --release --locked -- --nocapture
cargo clippy --release --locked --all-targets -- -D warnings
bash scripts/build_pcre2_10_49.sh /absolute/path/to/pcre2-build
python3 scripts/make_revision3_corpus.py
python3 scripts/measure_revision3.py /absolute/path/to/pcre2-build/install/lib
python3 scripts/analyze_revision3.py
python3 scripts/build_revision3_note.py
```

The comparison example imports `baseline-v0.2.0` as a dev-dependency alias.
Both versions use the same compiler, optimization profile, dependency versions
and shared PCRE2 library. Eight modes include Boolean JIT/PE, capture-output JIT,
ordinary PCRE2 JIT, and direct PCRE2 JIT. Three fresh processes per corpus use 9
randomized timing samples and 7 construction samples, targeting 20 ms per timing
sample on one logical CPU. Every aggregate verifies acceptance counts. Shared
fragment capture spans are additionally checked before timing.

Cache-hit timing is separate: capacity 16, one resident pattern, 50,000 lookups
per sample including Rc return/drop. Cold construction uses fresh Engine::new
instances and excludes EngineCache. Host load and frequency are uncontrolled.

For allocation counts, build the `allocations` binary and pass each corpus JSON.
All 44 cache-hit paths and 43/44 warmed Boolean paths allocate zero Rust heap
objects. The generic guard fallback makes 2,032 allocations over 256 calls.

```sh
cargo build --release --locked --bin allocations
target/release/allocations benchmarks/revision3-corpus.json
lean formal/AffineLengths.lean
```

`scripts/lean-proc-compat.c` is a bundled optional host-specific
workaround when a Lean executable cannot resolve `/proc/<pid>/exe`; the proof
kernel is unchanged. The artifact records the checked proof output.

## Files and provenance

- `benchmarks/revision3-summary.csv` and `revision3-analysis.json`: current results.
- `benchmarks/compare-*`: raw current campaign; frozen source/data manifest.
- `performance-changes.patch`: review aid from v0.2 to v0.3; full sources govern.
- `baseline-v0.2.0`: immutable baseline source used in the comparison binary.
- `baseline-v0.1.0`, historical campaign data and `history/v0.2-performance-note.pdf`:
  earlier work. Historical source hashes resolve against the matching baseline
  directory where current files differ. Earlier scripts remain for that history.
- `MANIFEST.json`: digests of every packaged file except itself.

This performance follow-up updates the experimental artifact. The submitted
journal manuscript is a separate artifact; no journal upload is performed here.
