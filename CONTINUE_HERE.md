# Development checkpoint

## Current state

The latest implementation is **project/**, version **0.3.0**. Cargo.lock is
retained exactly. The v0.2 baseline used by examples/compare.rs is a path
dev-dependency in project/baseline-v0.2.0. The original v0.1 baseline is also
retained; original-v0.1.0/ contains the full submitted release.

The whole-subject byte matcher supports named captures, backreferences, strict
typed first-order applied lambdas, guards and literal string interpolation.
Its string operations use byte lengths/slices and ASCII case conversion.
Resource exhaustion is an explicit Budget outcome.

v0.3 implements:

1. Conservative affine capture-length inference, N = d + k*l.
2. Budget-aware necessary bounds from selected length and equality guards.
3. Stable native capture slots and fused atomic capture/class scans.
4. Ordered factoring of contiguous literal prefixes.
5. Native small-negated-set checks and a bounded compiled-engine LRU cache.

20 release Rust tests and strict Clippy passed. Five additional Lean arithmetic
lemmas passed in Lean 4.19.0 with reported axioms [propext]. Proofs do not establish
correctness of the Rust compiler or native backend.

## Most recent results

The frozen comparison interleaves v0.2, v0.3 and PCRE2 10.49 in each process.
Three processes per corpus produced 9,504 matching rows, 7,392 construction
rows and 1,188 cache-lookup rows across 44 synthetic workloads.

- Boolean matching: 1.75x geometric-mean speedup over v0.2.
- Capture-producing calls: 1.63x geometric-mean speedup over v0.2.
- v0.3 lower Boolean medians than PCRE2 on 36/44 workloads.
- Delimiter-free backreference: 172.3 ns -> 24.0 ns.
- Generic compound guard: 438.8 ns versus a 36.2 ns PCRE2 + unique-capture predicate.
- Cold JIT construction: 219-1,764 microseconds; warm one-key cache hits: 18-26 ns.
- Nested captures regress about 10.6% on the fresh case; all regressions are retained.

Read project/benchmarks/revision3-analysis.json for complete medians, quartiles,
process medians and input ranges. The historical ID class_scan_4096 actually
contains 1,024-1,280 byte subjects. Synthetic scores are not application throughput.

## Next implementation priorities

1. Compile the generic typed expression language to a residual IR. Remove
   temporary strings, argument vectors and environment-map overhead while
   preserving strict argument evaluation, missing/open captures, lexical lambda
   scoping and byte limits. Start with the compound guard fallback case.
2. Reduce continuation and undo costs for repeated captures and alternatives
   whose leading capture writes prevent literal-prefix factoring.
3. Extend affine constraints to several unknown capture lengths, preserving the
   order of feasible endpoints rather than choosing an arbitrary solution.
4. Select PE/native scanners by measured shape and add compilation tiering.
   EngineCache is explicit reuse, not automatic profiling or tier selection.

## Navigation

- src/expr.rs: reference expression evaluator and specialization.
- src/fast_expr.rs: selected borrowed capture-expression operations.
- src/length.rs: conservative affine analyzer and exact-length scanner metadata.
- src/pattern.rs: parser, AST specialization, bytecode compiler and capture fusion.
- src/lib.rs: VM, slots, undo/range continuations, budgets and public API.
- src/jit.rs: native operations and checked runtime fallbacks.
- src/cache.rs: bounded process-local compiled-engine reuse.
- examples/compare.rs: interleaved version/engine comparison, including capture output.
- formal/Revision3.md: semantic obligations and higher-categorical interpretation.
- formal/AffineLengths.lean: checked arithmetic kernel.

The original manuscript submission remains a separate historical artifact.
publication/caprice-submission-receipt.json records its status at submission time;
its public status URL is the authority for a later review/publication claim.
No private journal session or submission credentials are needed for local work.
