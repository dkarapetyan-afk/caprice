## Abstract

We specify typed capture-dependent patterns, ordered match semantics, and a presheaf infinity-topos of static-information worlds and certified rewrites. We prove specialization preservation and conditional native simulation. A Lean kernel checks expression results. Rust artifacts implement a VM, partial evaluator, and Cranelift native control-flow JIT. Across 11 synthetic workloads, specialization improves median matching latency by 1.30 to 22.02 times over the raw VM (2.89 times geometric mean), while the helper-based JIT is slower than the specialized VM on every measured median.

Full proofs and artifacts accompany this text in `caprice-manuscript.pdf` and `caprice-v0.1.0.zip`.

## 1. Language and scope

Let $\Sigma=\{0,\ldots,255\}$ and $\mathrm{Str}=\Sigma^*$. Matching consumes the whole input. The language supports classes, literal bytes, concatenation, left-priority alternation, named captures `(?<x>p)`, backreferences `\k<x>`, greedy/lazy repetitions, and absolute anchors. The extensions are `${e}`, which consumes the literal bytes of an expression of type `Str`, and `(?{e})`, a zero-width Boolean guard.

```text
T ::= Str | Nat | Bool
e ::= "JSON string" | unsigned-integer | true | false | x
    | (cap name) | (param name) | (op e ... e)
    | (call (lambda (x T) e) e)
```

Lambdas are syntactically fully applied, first-order, lexically scoped, and nonrecursive. There is no host `eval`, implicit conversion, mutation, or I/O. Operators are `lower upper trim concat slice len eq contains starts ends lt le and or not`; arity and types are checked. `eq` requires operands of the same base type; comparisons and slices use natural numbers. All operands, including Boolean operands and lambda arguments, are strict.

Case conversion is ASCII-only; lengths and clamped half-open slices count bytes. Interpolated results are literal, never regex syntax. Static parameters have type `Str` and must all be supplied before Rust engine construction. Exact operator definitions appear in the full paper.

For example, the single-line pattern

```text
(?<x>[A-Za-z]+):${(upper (cap x))}(?{(le (len (cap x)) 5)})
```

accepts `Abc:ABC` and rejects `Abc:abc`.

Version 0.1 omits lookaround, recursive patterns, Unicode character semantics, first-class functions, and substring search. Unbounded repetition requires a statically nonnullable body; the conservative check can reject terminating patterns.

The general matching problem with backreferences is NP-complete when the pattern is part of the input [2]; partial evaluation does not abolish this boundary. PCRE2 already provides JIT and callouts [3,4]. Our contribution is the typed calculus, semantic organization, and reproducible implementation/evaluation.

## 2. Ordered operational semantics

For an input $w$ of length $n$, define $\mathrm{Span}_n=\{(a,b):0\leq a\leq b\leq n\}$. A capture environment is $\gamma:K\to\mathrm{Span}_n+\{\bot\}$ for a finite set of names $K$. A defined empty capture $(a,a)$ is distinct from $\bot$. Reading an unset or currently open capture gives `None`.

An expression has interpretation

$$ \llbracket e:T\rrbracket_{w,\gamma,\rho,\eta}\in\operatorname{Option}(\llbracket T\rrbracket). $$

Here $\rho$ supplies static parameters and $\eta$ supplies lambda variables. Constants give `Some`; a defined capture gives $w[a:b]$. Total primitive functions are lifted by strict left-to-right `Option` sequencing. Application first evaluates the argument and then the body with a lexical binding. A guard accepts only `Some(true)`; a defined interpolated string consumes exactly its bytes.

**Lemma 1 (typed termination).** Every well-typed expression terminates with a value of its declared type or `None`.

*Proof.* Induct on the finite expression AST. Primitives terminate on finite strings. Application evaluates finite nested subexpressions and contains no recursion; lexical bindings have their declared types. The claim concerns mathematical evaluation rather than physically unbounded allocation. $\square$

For fixed $w$, let $S_w$ contain states $(i,\gamma)$. Interpret a pattern as an ordered list of successor states, $R_p:S_w\to\operatorname{List}(S_w)$. Empty matching is a singleton; failure is the empty list. Alternation concatenates lists, while sequence uses list bind:

$$ R_{p;q}(s)=\operatorname{concat}_{t\in R_p(s)}R_q(t),\qquad
R_{p\mid q}(s)=R_p(s)\mathbin{+\!+}R_q(s). $$

Capturing records its start, clears its old binding while the body executes, and writes the successful span. Other bindings persist. Backtracking restores the full saved environment. Greedy repetition tries another body before its empty exit; lazy repetition tries the exit first. Bounded repetition decrements its bound. Consuming unbounded repetition terminates on finite subjects.

Let $\mathcal O(p,\rho,w)$ be the ordered list of capture environments from complete matches starting at zero with all captures unset, retaining multiplicity. Boolean acceptance is nonemptiness; the implementation returns the first environment. Equality of $\mathcal O$ is stronger than language equality.

**Counterexample 2 (first-match postfiltering).** On input `aa`,

```text
(?<x>a|aa)a?(?{(eq (cap x) "aa")})
```

first captures `a`, fails its predicate, then backtracks to capture `aa` and succeeds. Asking another engine once and rejecting its first capture would be wrong. The experimental host-predicate baselines below are valid only for selected patterns with unique full-match captures.

## 3. Higher-categorical semantics

Fix a finite parameter signature $M$. Worlds are partial maps $\sigma:M\rightharpoonup\Sigma^*$. There is a unique arrow $\tau\to\sigma$ when $\tau$ extends $\sigma$. This small category $W$ determines the presheaf infinity-topos

$$ \mathcal E=\operatorname{Fun}(W^{\mathrm{op}},\mathcal S). $$

If $\Omega_\sigma$ is the set of total assignments extending $\sigma$, then $\Omega_\tau\hookrightarrow\Omega_\sigma$ induces restriction of behaviors. Thus

$$ B(\sigma)=\operatorname{Set}(\Omega_\sigma\times\Sigma^*,\operatorname{List}(\mathrm{CapResult})) $$

is a discrete-valued object of $\mathcal E$. Capture results contain input-compatible spans. No extra topology or gluing assertion is assumed. The presheaf construction is standard [6]; neither the regex automaton nor the localization of its programs is asserted to be an infinity-topos.

Let $D_p$ be the family of successful finite derivations, with source and target maps giving a correspondence $S\leftarrow D_p\to S$. Subjects and parameter assignments are carried unchanged. Restricting completions makes these witness families presheaves. Sequence composes witnesses by homotopy pullback; alternatives use tagged coproducts. The prototype's fibers are discrete, so its homotopy pullbacks are ordinary pullbacks. Spans and their higher generalizations provide the surrounding correspondence calculus [7].

**Proposition 3 (observation forgets witnesses).** The image of $D_p\to S\times S$ is the successful transition relation. Its fibers are the $(-1)$-truncations of witness fibers. Further existential projection yields Boolean acceptance. Neither construction in general recovers ordered capture results.

*Proof.* The image factorization in an infinity-topos performs fiberwise propositional truncation. It remembers whether a transition witness exists, while existential projection also forgets its capture result. Reversing the alternatives in `(?<x>a|aa)a?` preserves acceptance of `aa` and changes its first capture. Priority therefore requires additional list/branch-order data beyond a bare span. $\square$

For each world, form a small category $\mathcal R_\sigma$ presented by finite programs, selected certified rewrite generators, and declared equations between rewrite paths. Certificates establish equality in $B(\sigma)$; they restrict to extending worlds. Let $C(\sigma)=\mathcal R_\sigma[\operatorname{Mor}^{-1}]$ be its infinity-groupoid completion, retaining the nerve's homotopy information rather than just connected components. Functorial localization makes $C$ a presheaf. Sound generators give a natural map $\operatorname{obs}:C\to B$, because their observation images are equal and hence invertible. Declared coherence equations require justification; for instance, constant folding at disjoint subtrees commutes syntactically.

For a program's behavior section $b_p:1\to B$, define its implementation fiber

$$ \mathcal F_p=1\mathbin{\times}_{B}C. $$

Its points describe compatible implementations, paths describe certified transformations, and higher paths describe the coherences retained by the presentation. We do not compute or assert a nontrivial higher homotopy group for the prototype. This construction specifies a place for proof-bearing optimization histories without confusing them with Boolean equivalence or execution cost.

## 4. Partial evaluation and native simulation

The specialization pipeline substitutes static parameters, folds closed operators, reduces lambdas with static arguments, removes constant guards, turns static interpolation into literals, fuses adjacent literals, and compresses class-only repetitions into a scanner with ordered alternatives. Dynamic lambda arguments remain strict. Oversized static string folds are left residual. An absolute end check is also hoisted before a contiguous trailing suffix of zero-width guards; under full-match observation, these state-preserving filters commute.

**Theorem 4 (expression preservation).** If $\rho$ extends $\sigma$, then

$$ \llbracket\operatorname{pe}_\sigma(e)\rrbracket_{w,\gamma,\rho,\eta}
=\llbracket e\rrbracket_{w,\gamma,\rho,\eta}. $$

*Proof.* Structural induction. Parameter replacement uses agreement with $\sigma$. Constant folding applies the same total operation to the same values. A static lambda argument permits value substitution; a dynamic argument retains strict evaluation before the body, including undefinedness. All remaining constructors follow from the induction hypotheses. Refusing a large fold retains a semantically equal residual expression. $\square$

**Theorem 5 (ordered matching preservation).** Ignoring operational budgets, specialization preserves $\mathcal O(p,\rho,w)$.

*Proof.* Theorem 4 preserves guard results and interpolated bytes. True guards and fused literals remove deterministic administrative steps. A class-repeat scanner enumerates precisely the same lengths in the same priority order, without changing captures. List bind and concatenation preserve these equalities. Induction on bounded counts and remaining input length handles repetitions. Moving the final end check across pure zero-width guards preserves the list of complete matches. Consequently acceptance and the first capture result are preserved. $\square$

The Lean artifact checks an intrinsically typed expression kernel with arbitrary total unary/binary primitives, capture reads, partial static assignments, and strict fully applied lambdas represented by higher-order abstract syntax. It proves specialization, staged semantic agreement, guard preservation, and the counterexample law that an unused missing argument still fails. Lean 4.19.0 checks it without `sorry` or additional axioms; the validation log lists standard Lean axioms. This is not verification of the Rust parser, bounded allocation, regex VM, or Cranelift.

The Rust VM uses backtracking frames containing a program counter, position, and capture snapshot. `raw` executes unspecialized bytecode; `pe` executes specialized bytecode; `jit` compiles that residual control-flow graph with Cranelift 0.125.3. Native blocks call Rust helpers, and a switch dispatches restored backtracking targets. String primitives and helper opcode dispatch remain in Rust: this is a native control-flow JIT, not a fully inlined string compiler.

**Theorem 6 (conditional lockstep simulation).** Assuming correct Cranelift lowering, ABI behavior, helper transitions, and executable lifetimes, the VM and JIT on the same residual program return equal match/capture, no-match, or budget results.

*Proof.* Relate instruction $j$ to native block $j$ with the same runtime state. Both execute the same helper. Successful transitions choose corresponding static edges; failed transitions restore the same frame and target. Exhaustion and acceptance terminate identically. Induction on dispatches establishes the result. This conditional proof does not replace machine-code or FFI verification. $\square$

Budget equivalence does not extend to PE versus raw execution: fusing many literal dispatches can change whether a fixed dispatch budget is exhausted. The API therefore distinguishes `Budget` from `NoMatch`. Defaults are one million dispatches, 100,000 frames, and 1 MiB per expression string. These are not a global memory or wall-clock bound. Source size, capture count, nesting, repetition counts, and expanded bytecode are also capped; details are in the full paper.

## 5. Validation and performance protocol

Nine Rust release tests pass. Nineteen patterns on all strings of length at most six over `ab-` give 20,767 cases checked against PCRE2 acceptance and capture spans. Four extended patterns on all strings of length at most five over `abAB-` give 15,624 cases checked across our modes. These 36,391 cases and targeted regressions are finite testing, not an end-to-end proof.

Measurements use one x86-64 Linux container on an AMD EPYC 9V74 host, pinned to CPU 0, with Rust 1.99.1 and a locked release build. Baselines are PCRE2 10.42 interpreted and JIT, Rust regex 1.12.2, and CPython 3.12.14. These are the tested versions, not a latest-version claim. There are 11 synthetic workloads of 256 inputs each, half accepted. All decisions are full byte matches. Unsupported backreference comparisons are omitted for Rust regex, whose documented regular-fragment behavior differs [5].

Three dynamic workloads use baseline regexes plus timed host predicates on unique full-match captures: case equality, uppercase field equality, or length at most 16. Counterexample 2 prohibits this translation in general. No callout baseline was run.

Three fresh processes each perform seven construction and nine warm matching samples per job. Matching order is shuffled deterministically, with calibrated batches targeting 20 ms. Tables use 27 matching and 21 construction samples. Raw data and IQRs are supplied. Compilation is excluded from warm matching. Python loop overhead is included; other loops are Rust. PCRE2 reuses match data; Caprice creates capture state per invocation. Frequency isolation, production corpora, memory, concurrency, Unicode, and Ruby/Onigmo were not evaluated.

## 6. Measured results

Median nanoseconds per decision; lower is better. PCRE-JIT means PCRE2 with JIT, Rust means Rust regex, and Python means CPython re. Host predicates are included where applicable; a dash denotes unsupported.

| Workload | Raw | PE | JIT | PCRE | PCRE-JIT | Rust | Python |
|:--------------------|-------:|-------:|-------:|-------:|-------:|-------:|-------:|
| Literal | 391.4 | 17.8 | 19.3 | 156.4 | 38.5 | 117.5 | 215.6 |
| Identifier | 611.8 | 451.1 | 480.9 | 197.9 | 48.8 | 48.7 | 211.0 |
| Fields | 827.5 | 543.2 | 597.6 | 137.7 | 41.7 | 55.4 | 247.0 |
| Alternation | 119.4 | 78.9 | 92.1 | 87.0 | 33.8 | 33.0 | 174.2 |
| Backref short | 236.9 | 163.0 | 174.0 | 79.2 | 40.2 | - | 185.4 |
| Backref long | 4226.0 | 3241.8 | 3384.2 | 192.5 | 170.7 | - | 677.3 |
| Static parameter | 252.9 | 43.5 | 48.8 | 101.4 | 34.5 | 39.5 | 187.5 |
| Lambda guard | 3875.4 | 632.7 | 677.1 | 102.9 | 56.9 | 107.8 | 273.0 |
| Dynamic interpolation | 408.6 | 312.9 | 329.6 | 155.0 | 95.6 | 158.4 | 315.7 |
| Length guard | 2681.7 | 664.4 | 714.8 | 104.1 | 55.4 | 119.0 | 247.7 |
| Constant lambda | 971.5 | 201.2 | 225.1 | 151.8 | 41.5 | 51.8 | 220.6 |

PE reduces median latency on all 11 workloads: 1.30 to 22.02 times, with an unweighted geometric mean of 2.89 times across these workloads. The JIT is 4.4% to 16.6% slower than PE by the measured medians. PE beats PCRE2 JIT on the fixed-literal workload (17.8 versus 38.5 ns), but PCRE2 JIT is faster on the other ten. Thus the evidence supports partial evaluation within this prototype and rejects a general speed-superiority claim for its current JIT.

The raw VM uses one instruction per literal byte; fusion explains its largest speedup. PE includes repeat fusion and end-check hoisting. Existing engines already optimize literals. The JIT retains allocation, capture copying, helper dispatch, and FFI costs.

For construction cost $C$, latency $t$, and $N$ uses, total cost is $C+Nt$. For JIT versus PE, the measured median compilation and matching costs are both higher in all 11 workloads, so no positive break-even count is demonstrated. Construction tables and code sizes appear in the full paper.

The separate cold adversarial probe is `(?:a|aa)*b` on $a^n$, for $n=8,12,16,20,24,28$. All three Caprice modes reach the one-million-dispatch budget at lengths 24 and 28 in each process run; the existing PCRE2 and Rust regex baselines reject without exhausting that budget or returning an error. PCRE2's required-byte optimization can reject this family immediately. This is not a claim that all backtracking engines fail on it, and JIT does not repair Caprice's worst-case search behavior.

## 7. Limits, cost, and reproducibility

Semantic equivalence is symmetric; improvement is not. Compilation cost and workload-indexed latency belong to the directed rewrite system before localization. Inverting a semantic rewrite does not turn its compilation time negative. Thus a correct implementation fiber can contain both faster and slower implementations.

The prototype has no adaptive JIT threshold, cache, native string lowering, SIMD, regular-fragment fallback, or machine-code certificates. The models permit partial static worlds; Rust currently requires all parameters and has no incremental certificate checker. The higher construction organizes evidence, rather than runtime homotopy computation. These synthetic results cannot rank engines universally.

The bundle includes sources, proofs, raw samples, statistics, metadata, native IR, and logs. Run `cargo test --release --locked`, build the binaries, then run `python3 scripts/run_benchmarks.py`; PCRE2 development libraries are required. `scripts/analyze.py` derives the statistics. Lean 4.19.0 checks `formal/Specialization.lean`; the README records host compatibility details.

**Authorship and automation.** Anonymous responsible author. AI assistance supported formulation, code, proofs, tests, benchmarks, and writing. Measurements are execution outputs. The author authorized submission and accepted the terms. No editorial acceptance, peer review, independent replication, or end-to-end verification is asserted.

## References

1. Jones, Gomard, Sestoft. *Partial Evaluation and Automatic Program Generation* (1993). <https://raspi.itu.dk/people/sestoft/pebook/pebook.html>.
2. Schmid. *Regular Expressions with Backreferences: Polynomial-Time Matching Techniques*, arXiv:1903.05896. <https://arxiv.org/abs/1903.05896>.
3. PCRE2. *JIT compiler support*. <https://www.pcre.org/current/doc/html/pcre2jit.html>.
4. PCRE2. *Callout specification*. <https://www.pcre.org/current/doc/html/pcre2callout.html>.
5. Rust regex 1.12.2. <https://docs.rs/regex/1.12.2/regex/>.
6. Lurie. *Higher Topos Theory* (2009). <https://www.math.ias.edu/~lurie/papers/HTT.pdf>.
7. Haugseng. *Iterated Spans and Classical Topological Field Theories* (2018). <https://arxiv.org/abs/1409.0837>.
8. Bytecode Alliance. *Cranelift JIT*. <https://docs.rs/cranelift-jit/0.125.3/cranelift_jit/>.
9. Python Software Foundation. *re*. <https://docs.python.org/3/library/re.html>.

The full paper supplies additional references, definitions, proofs, and compilation data. URLs accessed October 24, 2026, America/Los_Angeles.
