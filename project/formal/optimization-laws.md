# Obligations for the performance revision

The Lean kernel is inherited unchanged from the submitted artifact. It verifies
expression specialization, not these Rust runtime changes or generated machine
code. The arguments below are explicit implementation obligations, supported by
bounded differential testing rather than machine-checked compiler correctness.

For a pattern p, input s and initial capture environment C, write O(p,s,C) for
the ordered list of successful full-match capture observations with unbounded
resources. Each optimization aims to preserve this ordered list. A language-only
equivalence is insufficient because later guards and backreferences observe C.

1. **Capture trail.** Each continuation records the undo-log length at creation.
   Every capture write while a continuation exists logs the prior slot value.
   Popping restores entries in reverse order down to that recorded length.
   Thus every resumed continuation observes exactly its saved capture state,
   including missing versus open versus completed captures. Writes made without
   any pending continuation need no undo entry.
2. **Range continuations.** A class scanner does not write captures. Its possible
   endpoints therefore share one capture snapshot. A single frame enumerates
   endpoints end-1,...,minimum for greedy repetition, or minimum+1,...,end for
   lazy repetition. Restoring the trail before every resume preserves both
   endpoint order and capture observations without one frame per byte.
3. **Forced endpoints.** A repeated class followed through zero-width operations
   by absolute end, or a disjoint consuming byte/class, can succeed only at its
   maximal endpoint. Every shorter endpoint fails its continuation. Removing
   those candidates preserves O even for a lazy repeat. Jumps, other repeats,
   backreferences and arbitrary emissions stop this conservative analysis.
4. **Disjoint alternatives.** If two nonnullable alternatives have disjoint
   first-byte sets, at most one can succeed at the current position. A byte
   dispatch can choose that branch without saving the other branch. A bounded
   recursive first-set analysis returns unknown when it cannot prove the sets.
5. **Borrowed expressions.** Selected capture lengths, ASCII transformations and
   comparisons can consume a subject slice directly. Missing/open captures and
   expression-byte limits must be checked in the same strict order. A dynamic
   lambda is lowered only when its body reads the same capture first; unused
   dynamic arguments and other unsupported forms retain the strict evaluator.
   Lower/upper transformations preserve byte length. No UTF-8 claim is made.
6. **Necessary literals.** A literal guaranteed by every successful path gives
   a sound rejection filter when absent from the subject. Passing this filter
   is not evidence of acceptance. The implementation enables it only around
   complex unbounded repetitions and keeps the remaining search intact.
7. **Native primitives.** Each inlined residual instruction charges the same
   instruction budget, performs the same bounds checks, and commits position
   only on success. Runtime offsets derive from repr(C) Rust fields. Subject
   loads are length-checked and may be unaligned. Capture writes, backreferences
   and compiled capture predicates use dedicated Rust helpers. Unsupported
   instructions retain the checked interpreter helper. The optimization backend
   is restricted to 64-bit little-endian hosts; execution was tested on x86-64.

Removing unsuccessful searches changes finite resource exhaustion relative to
the raw engine and submitted release. `Budget` remains distinct from `NoMatch`.
PE and JIT execute the same optimized residual program; tests additionally check
their finite-budget agreement. The necessary-literal filter and class scans do
work outside dispatch counting, as did the original class scanner. No wall-clock
or global-memory bound, extraction theorem, universal linear-time guarantee, or
proof of Cranelift is asserted.

In the paper's semantics, these changes inhabit the same behavioral fiber only
after forgetting finite resource observations. Directed cost annotations must be
retained when discussing performance; semantic equivalence alone implies no speedup.
