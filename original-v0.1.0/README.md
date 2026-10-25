# Caprice 0.1.0

A Rust research implementation of full byte matching with named captures,
backreferences, typed literal interpolation, and fully applied string lambdas.
The anonymous paper and measured results are in `paper/` and `benchmarks/`.

## Build and run

Tested: Linux x86-64, Rust 1.99.1, PCRE2 10.42, Cranelift 0.125.3.
Install a Rust toolchain and PCRE2 development libraries (Ubuntu:
`apt-get install libpcre2-dev pkg-config`). The locked Cargo dependencies
are fetched from crates.io. No journal credential is needed to build or run.

```sh
cargo build --release --locked
cargo test --release --locked
cargo run --release --bin caprice -- jit '(?<x>[a-z]+)-\k<x>' 'abc-abc'
cargo run --release --bin caprice -- pe '(?<x>[A-Za-z]+):${(upper (cap x))}' 'Abc:ABC'
cargo run --release --bin caprice -- pe '${(param prefix)}(?<id>[0-9]{4})' 'tenant:1234' 'prefix=tenant:'
cargo run --release --bin caprice -- jit '(?<x>a|aa)a?(?{(eq (cap x) "aa")})' 'aa'
```

Modes: `raw` = unspecialized VM; `pe` = specialized VM; `jit` = native
control-flow compilation of the specialized program. All perform full matching.
The CLI prints JSON with `match`, `no_match`, or `budget`. Successful capture
values are returned as byte arrays with half-open spans. Invalid compilation
or CLI arguments produce a nonzero exit; a valid matching decision prints JSON.

## Language

- Named capture `(?<name>...)`, noncapturing group `(?:...)`, backreference
  `\k<name>`, left-priority `|`, greedy/lazy `* + ? {m} {m,n} {m,}`.
- ASCII classes and byte ranges, dot except LF, absolute `^` and `$`.
  `\d` is `[0-9]`, `\w` is ASCII alphanumerics plus underscore,
  `\s` is bytes 9, 10, 11, 12, 13, 32. Escaped metacharacters, `\n`, `\r`,
  `\t`, and two-digit `\xHH` are supported.
- `${e}` matches the literal bytes computed by `e : Str`. The result is never
  parsed as regex source. `(?{e})` is a zero-width Boolean guard.
- Expression syntax is parenthesized and typed: `(upper (cap name))`,
  `(eq (len (cap name)) 4)`, `(param prefix)`,
  `(call (lambda (x Str) (eq (lower x) "admin")) (cap name))`.
- Operators: `lower upper trim concat slice len eq contains starts ends lt le
  and or not`. `trim` follows Rust's ASCII whitespace predicate, removing bytes
  9, 10, 12, 13, 32. String literals use JSON syntax; lengths and slices are bytes.
- Arguments are strict. An unset capture is distinct from an empty capture and
  makes its branch fail. Guards can trigger backtracking. Lambdas have lexical
  scope, base-type arguments, no recursion, and no first-class function values.
- All static parameters must be supplied at engine construction. Captured input
  remains dynamic. The Lean model additionally supports partial static worlds.

Unsupported syntax is rejected. This is not a drop-in PCRE, Python, Ruby, or
Unicode regex implementation. It has no host-language execution or I/O.

## Semantics and validation

`formal/Specialization.lean` checks expression-specialization preservation,
staged semantic agreement, guard preservation, and strictness of missing
lambda arguments. It is an abstract typed kernel, not a verification of the
Rust implementation or generated machine code. Check with Lean 4.19.0:

```sh
lean formal/Specialization.lean
```

The validation host exposes `/proc/self/exe` but not `/proc/<pid>/exe`.
The optional compatibility shim in `scripts/lean-proc-compat.c` redirects only
that equivalent own-process path for Lean 4.19.0 executable discovery:

```sh
cc -shared -fPIC scripts/lean-proc-compat.c -ldl -o /tmp/lean-proc-compat.so
LD_PRELOAD=/tmp/lean-proc-compat.so lean formal/Specialization.lean
```

It does not modify the checker, proofs, or logical axioms. Normal hosts do not
need it. The successful Lean log lists only standard Lean axioms and no `sorryAx`.

Nine Rust tests include 20,767 shared cases checked against PCRE2 capture
semantics and 15,624 extended cases compared across all three modes, plus
targeted regressions. The mathematical paper states which results are proved,
which are conditional, and which are supported by finite tests.

## Reproduce the measurements

```sh
python3 scripts/make_corpus.py
cargo build --release --locked
python3 scripts/run_benchmarks.py
python3 scripts/analyze.py
```

The runner uses `taskset` on Linux, three sequential process runs, 11 workloads,
256 inputs per workload, nine timing samples per process, and seven construction
samples per process. Python 3 is needed for the secondary CPython `re` baseline.
Matplotlib and NumPy are optional for the exported figure. Raw CSV files and
the environment record are included; `summary.csv` includes medians and IQRs.
Unsupported Rust regex backreference comparisons are omitted. Dynamic baseline
predicates are used only when the full-match captures are unique.

The JIT compiles native control flow and calls Rust instruction helpers. It
does not inline primitive operators, eliminate the helper opcode dispatch,
or implement a DFA. PE is faster than the raw VM on this corpus; JIT is slower
than PE by every measured workload median. These are single-host synthetic
measurements, not universal engine rankings.

## Files

- `src/expr.rs`: typed expression parser, evaluator, specializer.
- `src/pattern.rs`: pattern parser, nullability, specializer, bytecode compiler.
- `src/lib.rs`: capture/backtracking runtime, limits, public engine interface.
- `src/jit.rs`: Cranelift native control-flow lowering and executable lifetime.
- `src/baseline.rs`: PCRE2 adapter for tests and benchmarks.
- `src/benchmark.rs`, `scripts/bench_python.py`: measurement harnesses.
- `formal/`: checked expression kernel and rigorous syntax specification.
- `paper/`: full manuscript, journal-length version, typeset PDF, and figures.
- `benchmarks/`: raw data, derived statistics, environment, validation logs.

The PDF is self-contained. The HTML uses adjacent CSS/figure files and loads
MathJax 3.2.2 from its CDN, so HTML mathematical rendering needs network access.

Default budgets: one million VM dispatches, 100,000 frames, 1 MiB per expression
string. These do not constitute a global memory or wall-clock bound. The core
is a research prototype; it has no persistent JIT cache, adaptive threshold,
SIMD primitive lowering, streaming search, or regular-fragment fallback.

## Authorship

Anonymous responsible author. AI assistance was used for formulation,
implementation, proofs, testing, benchmarking, and manuscript preparation.
Submission does not imply journal acceptance, peer review, or independent
replication. No credentials are included in this bundle.
