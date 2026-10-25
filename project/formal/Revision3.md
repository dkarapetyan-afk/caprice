# Semantic obligations for revision 0.3

Let O(p,s,C) be the ordered successful full-match capture observations with
unbounded resources. The intended correctness condition remains O(p,s,C) =
O(opt(p),s,C). Resource counters are directed cost annotations; their finite
values need not be preserved across optimizer versions. The PE/JIT tests compare
finite budgets on the same residual program.

## Affine capture lengths

The analyzer accepts a conservative straight-line fragment with one variable
capture consisting of a repeated byte class. Literals and fixed repetitions
have constant length. Completed captures have recorded affine lengths;
backreferences and supported length-preserving emissions reuse those lengths.
Sequence adds affine forms. Unsupported alternation, a second unknown length,
complex repetition, unknown capture state, or expression causes analysis to fail
and the ordinary residual program to remain in use. Natural-number arithmetic
is checked for overflow during analysis.

For any successful full match in the analyzed fragment, extraction should give
N = d + k*l, where N is the subject length, l the variable capture length, and
k > 0. Therefore l = (N-d)/k is the sole admissible length, with N >= d and
(N-d) mod k = 0. The scanner verifies the original class and repetition bounds
at that exact length. Every other endpoint fails full-match observation.

`AffineLengths.lean` checks uniqueness, quotient recovery, divisibility, and
sound arithmetic rejection by remainder or upper bound. This is an arithmetic
kernel; extraction from Rust ASTs and correspondence to operational semantics
remain implementation obligations supported by finite differential tests.

Selected strict guards add necessary bounds: len(x) <= b, len(x) < b, and
equality of x/lower(x)/upper(x) with a constant string. The original guard is
retained. An early rejection from its length bound is enabled only when the
runtime expression-byte limit covers both the inferred capture length and the
constant. A smaller limit executes the downstream guard, retaining its
mandatory Limit outcome on a structurally matching subject. The analyzer does
not hoist arbitrary guard expressions, unused lambda arguments, or captures
from another branch.

## Capture representation and fusion

A native capture slot is a repr(C) pair (start,end). MAX start denotes missing;
a finite start with MAX end denotes open; a finite ordered pair denotes a
completed span. This gives a representation map to the public Option span
type. Subject/capture pointer layout and offsets come from the hosting Rust
build, and no generated store depends on Rust Option layout. The vector is
initialized before native entry and never resized while native code uses it.

The Boolean API consumes the native representation directly. The capture-output
API materializes public spans after success. Unsupported expressions receive a
refreshed public capture view before strict evaluation. Capture undo entries
store entire native slots, including missing/open states. Their restoration is
unchanged by the representation map.

SaveStart; AtomicRun; SaveEnd can fuse when no control-flow edge enters the
interior. The class operation contains no expression or capture read. A fused
operation commits the completed span only when the scan succeeds, and records
one old slot when a continuation is pending. This is observational refinement,
not instruction-by-instruction equality. Branch targets are explicitly remapped
after fusion. If the program has any potential pending continuation, fused
captures use the checked runtime helper. Otherwise generated stores need no
undo record.

## Ordered prefix factoring

Only contiguous alternatives with a common leading literal are factored.
Captures and guards are barriers. Removing the common literal from each branch
leaves alternatives in their original order, including empty suffixes. Recursive
factoring builds a prefix tree without regrouping noncontiguous alternatives.
The existing conservative first-byte analysis can then remove disjoint choices.

## Compilation reuse

The process-local LRU key is the exact source pattern, complete static parameter
map, and mode. The cache holds immutable engines behind Rc. A returned owner
keeps native code and its program alive through eviction or clearing. Capacity
bounds retained entries; externally retained engines can outlive that bound.
The cache is thread-local, and a hit is neither an optimization of a cold miss
nor a reduction of first-compilation work.

## Higher semantics

In the submitted behavioral-presheaf model, the proposed rewrites must lie over
the same ordered observation in the behavioral fiber after forgetting finite
resource annotations. Slot conversion supplies a representation correspondence;
fusion and endpoint pruning are refinements of execution traces. Their cost
comparisons remain directed. Arithmetic uniqueness alone establishes neither
the semantic-fiber claim nor a performance improvement; extraction arguments,
representation invariants, native ABI assumptions and measured costs remain
separate evidence.
