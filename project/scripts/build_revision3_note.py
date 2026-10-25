#!/usr/bin/env python3
"""Render the latest performance note from the frozen revision-3 campaign."""
import json,html,statistics,shutil
from pathlib import Path
from reportlab.platypus import SimpleDocTemplate,Paragraph,Spacer,Table,TableStyle,PageBreak
from reportlab.lib.styles import getSampleStyleSheet,ParagraphStyle
from reportlab.lib import colors
from reportlab.pdfbase import pdfmetrics
from reportlab.pdfbase.ttfonts import TTFont
root=Path(__file__).resolve().parents[1];a=json.loads((root/'benchmarks/revision3-analysis.json').read_text())
rows=a['rows'];byid={r['workload']:r for r in rows};out=root.parent/'deliverables';out.mkdir(exist_ok=True)
pdf=out/'caprice-performance-note.pdf'
history=root/'history';history.mkdir(exist_ok=True)
old=root.parent/'caprice-perf/caprice-performance-note.pdf'
if old.exists() and not (history/'v0.2-performance-note.pdf').exists():shutil.copy2(old,history/'v0.2-performance-note.pdf')
fontroot=Path('/usr/share/fonts/truetype/dejavu')
for name,file in [('RC3','DejaVuSans.ttf'),('RC3-Bold','DejaVuSans-Bold.ttf')]:pdfmetrics.registerFont(TTFont(name,str(fontroot/file)))
pdfmetrics.registerFontFamily('RC3',normal='RC3',bold='RC3-Bold',italic='RC3',boldItalic='RC3-Bold')
navy=colors.HexColor('#132C42');teal=colors.HexColor('#087F83');gray=colors.HexColor('#526471');pale=colors.HexColor('#EFF5F7')
s=getSampleStyleSheet()
for name,size,leading,font,color in [('T',23,28,'RC3-Bold',navy),('Deck',11.2,16,'RC3',gray),('H',13,17,'RC3-Bold',navy),('B',9.8,14,'RC3',navy),('Small',8.5,12,'RC3',gray),('Cell',8.5,11,'RC3',navy),('TH',8.1,10,'RC3-Bold',colors.white)]:
    s.add(ParagraphStyle(name=name,fontName=font,fontSize=size,leading=leading,textColor=color,spaceAfter=9 if name not in ['Cell','TH'] else 0))
story=[]
def p(t,style='B'):return Paragraph(t,s[style])
def add(t,style='B'):story.append(p(t,style))
def heading(t):story.append(Spacer(1,5));add(t,'H')
def table(headers,values,widths):
    t=Table([[p(html.escape(h),'TH') for h in headers]]+[[p(html.escape(str(v)),'Cell') for v in row] for row in values],colWidths=widths,repeatRows=1,hAlign='LEFT')
    t.setStyle(TableStyle([('BACKGROUND',(0,0),(-1,0),navy),('VALIGN',(0,0),(-1,-1),'TOP'),('LEFTPADDING',(0,0),(-1,-1),7),('RIGHTPADDING',(0,0),(-1,-1),7),('TOPPADDING',(0,0),(-1,-1),6),('BOTTOMPADDING',(0,0),(-1,-1),6),('ROWBACKGROUNDS',(0,1),(-1,-1),[colors.white,pale]),('LINEBELOW',(0,0),(-1,0),0.7,teal)]));story.append(t);story.append(Spacer(1,8))
names={'literal':'Literal','identifier':'Identifier','fields':'Key/value fields','alternation':'Alternation','backref_short':'Short backreference','backref_long':'Long backreference','static_parameter':'Static parameter','lambda_guard':'Lambda guard','dynamic_interpolation':'Dynamic interpolation','length_guard':'Length guard','constant_lambda':'Constant lambda','literal_16':'16-byte literal','literal_128':'128-byte literal','literal_512':'512-byte literal','overlapping_alternatives':'Overlapping alternatives','disjoint_alternatives':'Disjoint alternatives','eight_captures':'Eight captures','backref_no_delimiter':'Backref without delimiter','backref_long_mixed_failures':'Long backref, mixed failures','lazy_delimiter':'Lazy repeat with delimiter','repeated_capture':'Repeated capture','guard_long':'Long capture guard','emit_lengths':'Interpolation, varied lengths','wide_class':'Wide byte class','negated_class':'Negated byte class','class_scan_4096':'1 KiB class scan','length_guard_scale':'Length guard, larger captures','branch_captures':'Captures across alternatives','r3_triple_backref':'Three copies of a capture','r3_affine_prefix_suffix':'Affine length with affixes','r3_nested_capture':'Nested capture','r3_two_unknown_captures':'Two unknown capture lengths','r3_fixed_capture':'Fixed and variable captures','r3_strict_length_lambda':'Strict length lambda','r3_folded_guard':'ASCII-folded equality guard','r3_emission_with_affixes':'Interpolation with affixes','r3_prefix_tree':'Shared-prefix tree','r3_capture_prefix_barriers':'Capture barriers in prefixes','r3_repeated_capture_scale':'Repeated captures, 1-16','r3_empty_repeated_capture':'Empty repeated capture','r3_negated_class_controls':'Negated class, control bytes','r3_class_scan_large':'Class scan through 8 KiB','r3_static_parameter_affine':'Static parameter with backref','r3_generic_guard_fallback':'Generic compound guard'}
def suite_table(group):
    table(['Workload','v0.2 JIT\nns/match','v0.3 JIT\nns/match','PCRE2\nns/match','Gain over\nv0.2'],[[names[r['workload']],f"{r['v02_ns']:,.1f}",f"{r['v03_ns']:,.1f}",f"{r['pcre_ns']:,.1f}",f"{r['speedup']:.2f}x"] for r in rows if r['suite']==group],[192,84,84,84,84])
overall=a['overall'];base=a['suites']['base'];dev=a['suites']['development'];fresh=a['suites']['fresh']
add('Caprice performance: revision 0.3','T')
add('Affine lengths, fused captures, prefix factoring and compilation reuse | Anonymous author','Deck')
add(f'<b>{overall["speedup_geomean"]:.2f}x faster Boolean matching</b> and <b>{overall["output_speedup_geomean"]:.2f}x faster capture-output calls</b> than v0.2, geometrically across 44 synthetic workloads. Boolean medians improve on {overall["faster_than_v02"]}/44 workloads and beat PCRE2 10.49 on {overall["faster_than_pcre"]}/44. These are measured point estimates; compilation and generic expressions remain important gaps.')
heading('The original 11 workloads')
add(f'All {base["v03_faster_than_pcre"]} v0.3 medians are below PCRE2 direct-JIT medians. Geometric-mean gain over v0.2: {base["speedup_geomean"]:.2f}x. Both versions and PCRE2 reuse match storage.','Small')
suite_table('base')
add('Times pool 27 samples from three fresh processes. Engines interleave in randomized order inside each process. Ratios compare workload medians and are then combined geometrically; they are not application-weighted throughput or significance claims.','Small')
story.append(PageBreak())
add('The next 17 development workloads','T')
add(f'{dev["speedup_geomean"]:.2f}x geometric-mean gain over v0.2; lower medians than PCRE2 on {dev["v03_faster_than_pcre"]}/17 workloads. These workloads guided this revision.','Deck')
suite_table('development')
r=byid['backref_no_delimiter'];g=byid['guard_long'];l=byid['length_guard_scale']
add(f'The largest previous gap, delimiter-free backreferences, falls from {r["v02_ns"]:.1f} ns to {r["v03_ns"]:.1f} ns. Long guards improve {g["speedup"]:.2f}x and larger length-guard subjects improve {l["speedup"]:.2f}x. Repeated captures remain {byid["repeated_capture"]["v03_over_pcre"]:.2f}x PCRE2 latency; negated classes remain {byid["negated_class"]["v03_over_pcre"]:.2f}x.')
add('The historical ID class_scan_4096 contains 1,024-1,280 byte subjects, including the appended failure byte. The table labels its actual scale.','Small')
story.append(PageBreak())
add('Fresh validation: 16 workloads','T')
add(f'{fresh["speedup_geomean"]:.2f}x faster than v0.2 geometrically; {fresh["v03_faster_than_pcre"]}/16 lower medians than PCRE2. The engine was frozen before seeing these timings.','Deck')
suite_table('fresh')
f=byid['r3_generic_guard_fallback'];n=byid['r3_nested_capture']
add(f'The generic compound guard still takes {f["v03_ns"]:.1f} ns versus PCRE2 plus its unique-capture host predicate at {f["pcre_ns"]:.1f} ns ({f["v03_over_pcre"]:.2f}x). Nested captures regress {(1/n["speedup"]-1)*100:.1f}% relative to v0.2. These results are retained without retuning the engine against this corpus.','Small')
story.append(PageBreak())
add('Why the transformations are valid','T')
heading('One unknown capture length')
add('For a conservative straight-line fragment, the compiler derives N = d + k*l, where N is subject length, d is fixed length, and l is the sole unknown capture length. Completed captures, backreferences and supported length-preserving emissions contribute affine forms. With k &gt; 0, a full match requires N &gt;= d, zero remainder, and the unique length l = (N-d)/k. The original class and repetition bounds are then checked at that endpoint.')
add('Alternation, a second unknown capture length, complex repetition and unsupported expressions stop this analysis. Selected length/equality guards give necessary bounds, while the original guard remains. Early rejection is conditioned on enough expression-byte budget; smaller budgets still execute the strict downstream guard.')
heading('Captures and ordered alternatives')
add('An explicit repr(C) start/end slot represents missing, open and completed captures using MAX sentinels. Native code uses Rust-computed offsets, with an initialized, fixed-length slot vector. The public capture-output API materializes Option spans after success; the Boolean API uses slots directly. Generic expressions receive a refreshed public view.')
add('SaveStart; AtomicRun; SaveEnd fuse only when no edge enters their interior. A successful fused scan records one undo entry and commits the completed span; a failed scan leaves it unchanged. Native stores are used only when the program cannot have pending continuations. Shared leading literals factor only across contiguous alternatives; captures and guards are barriers, and alternative order is preserved.')
heading('Checked arithmetic and remaining obligations')
add('AffineLengths.lean verifies uniqueness, quotient recovery, divisibility, and sound rejection by remainder or upper bound in Lean 4.19.0. Its five theorems use only propext among reported axioms. This checks arithmetic consequences of an extracted equation, not Rust extraction, Cranelift, or complete regex semantics. Twenty Rust tests check the implementation and retain the earlier exhaustive comparisons.')
add('In the behavioral-presheaf semantics, these rewrites are intended to stay in the same fiber of ordered successful capture observations after forgetting finite resource annotations. Slot conversion is a representation correspondence; fusion and pruning refine traces. Costs remain directed. The proof and testing boundaries are explicit in formal/Revision3.md.','Small')
story.append(PageBreak())
add('API costs and compilation reuse','T')
add('Boolean and capture-producing calls were timed separately. The latter calls run_with, checks status and exposes the full capture vector to the benchmark. The gain therefore includes the cost of converting native slots into public Option spans.','Deck')
table(['Suite','Boolean gain','Capture-output gain'],[[label,f"{a['suites'][key]['speedup_geomean']:.2f}x",f"{a['suites'][key]['output_speedup_geomean']:.2f}x"] for key,label in [('base','Original 11'),('development','Development 17'),('fresh','Fresh 16')]],[240,144,144])
heading('Bounded process-local LRU cache')
add('EngineCache keys immutable engines by exact pattern source, the complete static parameter map and mode. Rc ownership keeps a program and native module alive through cache eviction or clearing. The configured capacity bounds retained entries, while callers can retain evicted engines. The cache is local to one thread; lookup is linear in resident entries.')
hits=[r['cache_hit_ns'] for r in rows];comp=[r['v03_compile_us'] for r in rows];pcomp=[r['pcre_compile_us'] for r in rows]
table(['Operation','Median range across 44 workloads'],[['Fresh v0.3 JIT construction',f'{min(comp):,.1f}-{max(comp):,.1f} microseconds'],['Fresh PCRE2 JIT construction',f'{min(pcomp):.1f}-{max(pcomp):.1f} microseconds'],['v0.3 cache-hit lookup',f'{min(hits):.1f}-{max(hits):.1f} nanoseconds']],[240,288])
add(f'Fresh JIT construction is {(1-overall["compile_ratio_geomean"])*100:.1f}% faster than v0.2 geometrically. A hit avoids construction; it does not reduce miss cost. Hit measurements use capacity 16 with one resident key, include Rc return/drop, and exclude matching. They do not establish realistic multi-key cache throughput.')
heading('Allocation measurements')
add('After a complete corpus warmup, 43/44 Boolean workloads allocate no Rust heap objects during matching. The generic compound guard makes 2,032 allocations across its 256 calls. All 44 measured cache-hit paths allocate zero Rust heap objects. These are separately instrumented counts, outside the timing campaign.')
story.append(PageBreak())
add('Reproduction and next targets','T')
add('The locked comparison binary links v0.2 and v0.3 as separate Cargo dependency aliases and calls the same PCRE2 10.49 shared library. All eight modes interleave per randomized sample: both versions in PE, Boolean JIT and capture-output JIT modes, plus ordinary and direct PCRE2 JIT calls.')
add('Three fresh processes per corpus; 9 timing samples and 7 construction samples per engine/workload/process. Samples target 20 ms after warmup. Each workload has 256 fixed inputs. The campaign contains 9,504 match rows, 7,392 construction rows and 1,188 cache-hit rows. Acceptance and shared-fragment capture spans are validated before timing; every timed count is checked.')
add('Host: AMD EPYC 9V74, Linux x86-64, logical CPU 0, Rust 1.99.1 release and Cranelift 0.125.3. PCRE2 10.49 uses the previously checked official source build with JIT enabled and -O3 -DNDEBUG. Host load/frequency are uncontrolled. Source, comparison binary, corpus and result hashes accompany the raw data.')
heading('Priority work after this revision')
for t in [
    'Compile the generic typed expression language to a residual IR that avoids temporary strings, argument vectors and environment maps while retaining strict errors and lambda semantics.',
    'Reduce continuation and undo costs for repeated captures and alternatives with capture barriers. Generalize affine reasoning to multiple unknown lengths with ordered feasible endpoints.',
    'Choose PE or native scanners using measured pattern shapes, then add tiering for actual reuse. Cold JIT misses still cost hundreds of microseconds despite cheaper construction and explicit caching.',
]:add(t)
add('Scope: full-subject byte semantics and ASCII transforms. PCRE2 host predicates are timed only on uniquely determined full captures. Synthetic cohorts do not establish application performance. New optimizations can change finite exhaustion across versions; Budget remains distinct from rejection, and scans are not charged per byte.','Small')
for label,url in [('PCRE2 10.49 release','https://github.com/PCRE2Project/pcre2/releases/tag/pcre2-10.49'),('PCRE2 JIT and direct API','https://www.pcre.org/current/doc/html/pcre2jit.html'),('PCRE2 performance','https://www.pcre.org/current/doc/html/pcre2perform.html'),('Lean theorem proving reference','https://lean-lang.org/theorem_proving_in_lean4/')]:add(f'<link href="{url}" color="#087F83">{label}</link>','Small')
def footer(c,doc):
    c.setStrokeColor(colors.HexColor('#CEDBE1'));c.line(42,35,570,35);c.setFont('RC3',8);c.setFillColor(gray)
    c.drawString(42,23,'CAPRICE / PERFORMANCE EXPERIMENT v0.3.0');c.drawRightString(570,23,str(doc.page))
SimpleDocTemplate(str(pdf),pagesize=(612,792),leftMargin=42,rightMargin=42,topMargin=38,bottomMargin=45,title='Caprice: performance revision 0.3',author='Anonymous author').build(story,onFirstPage=footer,onLaterPages=footer)
shutil.copy2(pdf,root/'caprice-performance-note.pdf')
print(pdf)

def mdtable(selected):
    return '| Workload | v0.2 ns | v0.3 ns | PCRE2 ns | Gain vs v0.2 |\n|---|---:|---:|---:|---:|\n'+''.join(f"| {names[r['workload']]} | {r['v02_ns']:.1f} | {r['v03_ns']:.1f} | {r['pcre_ns']:.1f} | {r['speedup']:.2f}x |\n" for r in selected)
readme=f'''# Caprice performance experiment v0.3.0

Read `caprice-performance-note.pdf` for measured results and implementation scope.
Across 44 synthetic workloads, Boolean matching improves **{overall['speedup_geomean']:.2f}x** and
capture-output calls **{overall['output_speedup_geomean']:.2f}x** geometrically versus v0.2. Native
Boolean medians beat PCRE2 10.49 on **{overall['faster_than_pcre']}/44** workloads.
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
use caprice::{{cache::EngineCache, expr::Params, Mode, Scratch}};
fn main() -> Result<(), String> {{
    let mut cache = EngineCache::new(16);
    let engine = cache.get_or_compile(r"(?<x>[ab]{{0,16}})\\k<x>", &Params::new(), Mode::Jit)?;
    let mut scratch = Scratch::default();
    assert!(engine.is_match_with(b"abab", &mut scratch)?);
    Ok(())
}}
```

`run_with(input, limits, scratch)` materializes the public capture vector on
success. Inspect `scratch.captures()` only after `MatchStatus::Match` from this
method. `is_match_with` uses native slots directly. Generic expressions refresh
a public view before strict evaluation. A reused match state may retain memory.
The cache bounds retained entries; caller-held `Rc` owners can keep evicted
engines alive. The cache is thread-local. Hits avoid construction; misses still
perform a complete compilation, and automatic tier selection is future work.

## Original and development corpus results

{mdtable([r for r in rows if r['suite']!='fresh'])}

## Fresh validation corpus

The engine was frozen before these timings were observed. They are synthetic,
and include large inputs, multiple capture lengths and generic guard fallback.

{mdtable([r for r in rows if r['suite']=='fresh'])}

Generic compound guards remain {f['v03_over_pcre']:.2f}x PCRE2 plus the timed host predicate;
this baseline uses a uniquely determined full capture. It is not a substitute
for general backtracking-aware guard semantics. Nested captures regress
{(1/n['speedup']-1)*100:.1f}% versus v0.2 on the fresh case. Tiny differences in other workloads are
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
'''
(root/'README.md').write_text(readme)
