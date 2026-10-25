#!/usr/bin/env python3
"""Render the measured engineering note, and keep its tables reproducible."""
import json, statistics, csv, html
from pathlib import Path
from reportlab.platypus import SimpleDocTemplate, Paragraph, Spacer, Table, TableStyle, PageBreak, KeepTogether
from reportlab.lib.styles import getSampleStyleSheet, ParagraphStyle
from reportlab.lib import colors
from reportlab.lib.enums import TA_RIGHT
from reportlab.pdfbase import pdfmetrics
from reportlab.pdfbase.ttfonts import TTFont
pdfmetrics.registerFont(TTFont('CapriceSans','/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf'))
pdfmetrics.registerFont(TTFont('CapriceSans-Bold','/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf'))
pdfmetrics.registerFontFamily('CapriceSans',normal='CapriceSans',bold='CapriceSans-Bold',italic='CapriceSans',boldItalic='CapriceSans-Bold')
root=Path(__file__).resolve().parents[1]
data=json.loads((root/'benchmarks/performance-analysis.json').read_text())
main=data['comparisons']['current'];validation=data['comparisons']['current-validation']
out=root.parent/'deliverables';out.mkdir(exist_ok=True)
pdf=out/'caprice-performance-note.pdf'
navy=colors.HexColor('#132C42');teal=colors.HexColor('#087F83');muted=colors.HexColor('#526471');pale=colors.HexColor('#EFF5F7')
styles=getSampleStyleSheet()
styles.add(ParagraphStyle(name='Title2',fontName='Helvetica-Bold',fontSize=25,leading=29,textColor=navy,spaceAfter=12))
styles.add(ParagraphStyle(name='Deck',fontName='Helvetica',fontSize=12,leading=17,textColor=muted,spaceAfter=14))
styles.add(ParagraphStyle(name='H2x',fontName='Helvetica-Bold',fontSize=14,leading=18,textColor=navy,spaceBefore=12,spaceAfter=7))
styles.add(ParagraphStyle(name='Bodyx',fontName='Helvetica',fontSize=10,leading=14,textColor=navy,spaceAfter=9))
styles.add(ParagraphStyle(name='Smallx',fontName='Helvetica',fontSize=8.7,leading=12,textColor=muted,spaceAfter=7))
styles.add(ParagraphStyle(name='Cell',fontName='Helvetica',fontSize=8.7,leading=11,textColor=navy))
styles.add(ParagraphStyle(name='CellHead',fontName='Helvetica-Bold',fontSize=8.3,leading=10,textColor=colors.white))
for style in styles.byName.values():
    if getattr(style,'fontName',None)=='Helvetica': style.fontName='CapriceSans'
    elif getattr(style,'fontName',None)=='Helvetica-Bold': style.fontName='CapriceSans-Bold'
story=[]
def p(text,style='Bodyx'):return Paragraph(text,styles[style])
def add(text,style='Bodyx'):story.append(p(text,style))
def table(headers,rows,widths):
    content=[[p(html.escape(h),'CellHead') for h in headers]]+[[p(html.escape(str(x)),'Cell') for x in r] for r in rows]
    t=Table(content,colWidths=widths,repeatRows=1,hAlign='LEFT')
    t.setStyle(TableStyle([('BACKGROUND',(0,0),(-1,0),navy),('VALIGN',(0,0),(-1,-1),'TOP'),('LEFTPADDING',(0,0),(-1,-1),8),('RIGHTPADDING',(0,0),(-1,-1),8),('TOPPADDING',(0,0),(-1,-1),6),('BOTTOMPADDING',(0,0),(-1,-1),6),('ROWBACKGROUNDS',(0,1),(-1,-1),[colors.white,pale]),('LINEBELOW',(0,0),(-1,0),0.7,teal)]))
    story.append(t);story.append(Spacer(1,8))
names={'backref_short':'Short backreference','backref_long':'Long backreference','literal':'Literal','identifier':'Identifier','fields':'Key/value fields','alternation':'Alternation','static_parameter':'Static parameter','lambda_guard':'Lambda guard','dynamic_interpolation':'Dynamic interpolation','length_guard':'Length guard','constant_lambda':'Constant lambda','literal_16':'16-byte literal','literal_128':'128-byte literal','literal_512':'512-byte literal','overlapping_alternatives':'Overlapping alternatives','disjoint_alternatives':'Disjoint alternatives','eight_captures':'Eight captures','backref_no_delimiter':'Backreference, no delimiter','backref_long_mixed_failures':'Long backref, varied failures','lazy_delimiter':'Lazy repeat with delimiter','repeated_capture':'Repeated capture','guard_long':'Guard over long capture','emit_lengths':'Interpolation, varied lengths','wide_class':'Wide character class','negated_class':'Negated character class','class_scan_4096':'1 KiB class scan','length_guard_scale':'Length guard, longer captures','branch_captures':'Captures across alternatives'}
add('Caprice at PCRE2 speed','Title2')
add('Measured performance follow-up | Rust experiment v0.2.0 | Anonymous author','Deck')
add(f'<b>Warm matching is now competitive.</b> The new JIT has lower median latency than PCRE2 10.49 on {main["wins_by_median"]}/11 original workloads, with a {main["pcre_over_jit_geomean"]:.2f}x geometric-mean throughput advantage. Its latency is {main["speedup_vs_submitted_pe_geomean"]:.2f}x lower than the submitted PE engine rerun on this host.')
add('PCRE2 uses its direct JIT API. Both engines reuse match storage; baseline capture predicates use borrowed strings. Times include the matching call and any required host predicate. Lower nanoseconds and ratios are better.','Smallx')
table(['Workload','Submitted PE\nns/match','New JIT\nns/match','PCRE2 10.49\nns/match','JIT / PCRE2'],[[names[r['workload']],f"{r['submitted_pe_ns']:,.1f}",f"{r['jit_ns']:.1f}",f"{r['pcre_ns']:.1f}",f"{r['ratio']:.2f}x"] for r in main['rows']],[168,90,88,96,86])
add(f'The remaining original-suite gaps are fields ({next(r["ratio"] for r in main["rows"] if r["workload"]=="fields"):.2f}x), the length guard ({next(r["ratio"] for r in main["rows"] if r["workload"]=="length_guard"):.2f}x), and the lambda guard ({next(r["ratio"] for r in main["rows"] if r["workload"]=="lambda_guard"):.2f}x). These are median estimates, not significance claims.')
add('The result applies to repeated matching after compilation. JIT compilation and some ambiguous patterns remain substantial gaps. The submitted manuscript and its original measurements remain the historical baseline.','Smallx')
story.append(PageBreak())
add('Where parity holds, and breaks','Title2')
add(f'17 additional synthetic workloads: {validation["wins_by_median"]} lower medians; {validation["within_10_percent"]} within 10% of PCRE2. Geometric-mean latency is {(1/validation["pcre_over_jit_geomean"]-1)*100:.1f}% higher than PCRE2.','Deck')
table(['Additional workload','New JIT, ns','PCRE2, ns','JIT / PCRE2'],[[names[r['workload']],f"{r['jit_ns']:.1f}",f"{r['pcre_ns']:.1f}",f"{r['ratio']:.2f}x"] for r in validation['rows']],[244,94,94,96])
add('The added corpus was generated after the implementation was frozen; no further matcher changes were tuned against these results. It tests longer spans, earlier failures, overlapping alternatives, repeated captures and backreferences without separators. It is still synthetic.','Smallx')
add('The raw workload ID class_scan_4096 is historical: its generated subjects are actually 1,024-1,280 bytes, including the appended failure byte. The table labels the measured data accordingly.','Smallx')
story.append(PageBreak())
add('What changed in the engine','Title2')
for title,body in [
    ('Reusable match state','A caller-owned Scratch retains capture, continuation and undo buffers. All 28 tested workloads perform zero heap allocations in a warmed JIT call using this API; arbitrary expressions can still allocate in the fallback evaluator.'),
    ('Compact backtracking and capture restoration','A class repetition records one range of candidate endpoints. A capture undo log replaces full capture-vector copies. The range preserves greedy/lazy order, and every resumed candidate restores the same capture environment.'),
    ('Eliminate provably doomed choices','Disjoint following bytes or absolute end make certain class repetitions possessive. Disjoint first-byte alternatives dispatch directly. A necessary-literal filter rejects absent-literal failures before complex unbounded searches.'),
    ('Compile capture operations','Length guards, selected equality tests, and ASCII upper/lower interpolation operate on borrowed capture slices. Recognized applied lambdas retain strict evaluation and byte-limit checks. Unsupported expressions keep the original evaluator.'),
    ('Generate native operations','Cranelift now emits bounded literal loads, static class tests, scanner loops, anchors and branch dispatch. Capture writes, backreferences and lowered expressions use dedicated helpers without opcode dispatch. Other instructions keep the checked interpreter helper.'),
]:
    add(title,'H2x');add(body)
add('Correctness evidence','H2x')
add('14 Rust tests pass, including the original 36,391 bounded cases, 51,200 expression comparisons, 54,684 capture-trail/reuse comparisons, 11,056 native boundary cases and 15,120 finite-budget comparisons. Release Clippy passes with warnings denied. PCRE2 10.49 passes its four upstream test groups; capture differential tests also pass against it.')
add('The new Rust optimizations are tested and accompanied by explicit semantic obligations. The inherited Lean artifact verifies the earlier expression-specialization kernel; it is not a proof of this compiler, runtime, or native backend.','Smallx')
story.append(PageBreak())
add('The next engineering targets','Title2')
for title,body in [
    ('1. Propagate capture-length constraints','Highest-priority warm-match gap: the delimiter-free backreference is 2.95x slower. In a proven full-match shape x followed by its own backreference, the input length fixes 2|x| = |input|. Derive admissible lengths before enumerating endpoints. Likewise, len(x) <= k and equality to a constant can bound or reject captures early. Preserve missing/open capture semantics and strict errors.'),
    ('2. Inline capture state and factor shared prefixes','Repeated captures are 1.73x slower, eight captures 1.24x, and overlapping alternatives 1.28x. Use an explicit stable capture-slot layout for native writes, reduce unnecessary trail entries using liveness, and factor shared literal prefixes while preserving alternative priority and capture effects.'),
    ('3. Select scanner and guard code by shape','Long guards and negated classes remain 1.31-1.46x slower. Compare vectorized common-class scanners and fused capture predicates with the existing Rust scanner. The PE interpreter is faster than the JIT on seven of the additional workloads; the compiler should use that evidence when selecting a backend.'),
    ('4. Tier compilation and reuse compiled programs','Across all 28 workloads, Caprice JIT compilation costs 233-1,958 microseconds, versus 2.6-24.8 microseconds for PCRE2. Keep PE as the initial tier, cache by pattern and static parameters, then compile hot shapes. Where JIT wins, measured compile amortization against PE ranges from about 10,800 to 288,500 matches; seven validation workloads show no break-even.'),
]:
    add(title,'H2x');add(body)
add('A concrete next milestone','H2x')
add('Bring every measured warm workload within 10% of PCRE2, reduce the largest compile break-even points, and add representative application traffic before making a general performance claim. Required-literal rejection removes the tested missing-b explosion; surviving ambiguous searches can still exhaust their budget.')
story.append(PageBreak())
add('Measurement and reproduction','Title2')
add('Three fresh processes per corpus and PCRE2 version; nine randomized timing samples per workload/engine/process, seven compilation samples. Each corpus workload contains 256 fixed inputs. Timing samples target 20 ms after warmup and calibration. Tables pool 27 timing samples; raw samples, quartiles and process medians are included in the bundle.')
add('The current-version campaign contains 6,669 timing rows and 5,187 compilation rows across the original and additional corpora. The older PCRE2 10.42 campaign and three single-process diagnostic stages are retained separately. Diagnostic stages guided changes; they are not repeated controlled ablations.')
add('Host: AMD EPYC 9V74, Linux x86-64, one pinned logical CPU (0), Rust 1.99.1 release, Cranelift 0.125.3. PCRE2 10.49 was built from its official release tarball using GCC, -O3 -DNDEBUG, shared 8-bit library and JIT enabled. Host load and frequency are not controlled. Shared-container measurements and synthetic inputs limit generalization.')
add('The benchmark uses whole-subject byte semantics and ASCII transforms. PCRE2 postprocessors are timed only for workloads whose full captures are unique; they are not a general substitute for backtracking-aware guards. No Unicode, substring-search, callout, or production-traffic performance equivalence is established.')
add('Reproduce','H2x')
add('The bundle contains Cargo.lock, the submitted baseline sources, raw CSVs, corpus generators, source and binary hashes, allocation-counting code, a reviewable patch and the analysis scripts. README.md gives commands to build both versions, select PCRE2 10.49, run the tests and regenerate this note.')
add('Resource and formal scope','H2x')
add('Removing failed paths can change finite exhaustion relative to the submitted release. Budget remains distinct from rejection. The optimized VM and JIT are checked for equal residual budgets on the finite test suite. Scans and literal filters are not charged per byte; no wall-clock or global-memory bound is claimed. The native prototype targets 64-bit little-endian hosts and was measured only on x86-64.')
add('Primary references','H2x')
for title,url in [
    ('PCRE2 10.49 release','https://github.com/PCRE2Project/pcre2/releases/tag/pcre2-10.49'),
    ('PCRE2 JIT and direct fast-path API','https://www.pcre.org/current/doc/html/pcre2jit.html'),
    ('PCRE2 runtime storage and performance','https://www.pcre.org/current/doc/html/pcre2perform.html'),
    ('Cranelift 0.125.3 JIT documentation','https://docs.rs/cranelift-jit/0.125.3/cranelift_jit/'),
]:add(f'<link href="{url}" color="#087F83">{html.escape(title)}</link>','Smallx')
def footer(c,doc):
    c.setStrokeColor(colors.HexColor('#CEDBE1'));c.line(42,35,570,35)
    c.setFont('CapriceSans',8);c.setFillColor(muted)
    c.drawString(42,23,'CAPRICE / PERFORMANCE EXPERIMENT v0.2.0')
    c.drawRightString(570,23,str(doc.page))
doc=SimpleDocTemplate(str(pdf),pagesize=(612,792),leftMargin=42,rightMargin=42,topMargin=39,bottomMargin=46,title='Caprice: reaching PCRE2 performance',author='Anonymous author')
doc.build(story,onFirstPage=footer,onLaterPages=footer)
print(pdf)

def mdtable(rows):
    return '| Workload | New JIT ns | PCRE2 10.49 ns | JIT / PCRE2 |\n|---|---:|---:|---:|\n'+''.join(f"| {names[r['workload']]} | {r['jit_ns']:.1f} | {r['pcre_ns']:.1f} | {r['ratio']:.2f}x |\n" for r in rows)
readme=f'''# Caprice performance experiment v0.2.0

This is the implementation and measurement follow-up to the submitted anonymous
Caprice manuscript. Start with `caprice-performance-note.pdf` for the findings and
next engineering targets. The original submission is preserved in its separate
v0.1.0 release; this experimental bundle adds a new implementation and datasets.

## Measured outcome

Against PCRE2 **10.49**, using its direct JIT API, Caprice's reused-state JIT is
faster on **8/11** original workload medians and **9/17** additional workload
medians. Original-suite geometric-mean throughput is **{main['pcre_over_jit_geomean']:.2f}x** PCRE2.
Additional-suite geometric-mean latency is **{(1/validation['pcre_over_jit_geomean']-1)*100:.1f}% higher**.
The original-suite speedup over the submitted PE engine rerun on this host is
**{main['speedup_vs_submitted_pe_geomean']:.2f}x**, including the API change to reusable match state.

These are warm synthetic benchmarks, not a general speed guarantee. Compilation
is still much more expensive than PCRE2. Every warmed reusable JIT workload had
zero measured Rust heap allocations; generic fallback expressions may allocate.

{mdtable(main['rows'])}

## Additional validation

{mdtable(validation['rows'])}

The input corpus was fixed after implementation; the matcher was not subsequently
tuned to these results. The ID `class_scan_4096` actually contains 1,024-1,280 byte
subjects (including the appended failure byte). `performance-analysis.json`
records actual subject ranges, acceptance counts, quartiles and process medians.

## Implementation

- Reused `Scratch`, capture undo trail and single-frame range continuations.
- Proven forced endpoints and disjoint-first-byte branch dispatch.
- Necessary-literal rejection for complex unbounded repeats.
- Borrowed capture length/equality/ASCII-transform evaluation, with strict errors.
- Inlined native byte checks/scans plus dedicated capture and expression helpers.
- Generic checked fallback for expressions or residual instructions not lowered.

The shared semantics remain whole-subject byte matching; captures, backreferences,
typed strict applied lambdas and literal string interpolation are retained.
Limits and unsupported syntax follow the original release. Resource exhaustion
may differ across compiler versions because failed searches are removed. See
`formal/optimization-laws.md`; the inherited Lean proof does not verify the new
Rust transformations, parser, runtime or Cranelift backend.

## API

```rust
use caprice::{{Engine, Mode, Scratch, expr::Params}};
let engine = Engine::new(r"(?<x>[a-z]+)-\\k<x>", Params::new(), Mode::Jit)?;
let mut scratch = Scratch::default();
let matched = engine.is_match_with(b"abc-abc", &mut scratch)?;
assert!(matched);
```

`run_with(input, limits, &mut scratch)` returns `MatchStatus`; inspect
`scratch.captures()` only after `MatchStatus::Match`. Match storage belongs to the
caller. `Engine::run` and `is_match` retain the convenience API with fresh storage.
Native mode requires a 64-bit little-endian host; only x86-64 was tested.

## Reproduction

Requirements: Rust/Cargo compatible with the lockfile, Python 3, a C toolchain,
PCRE2 8-bit development library with JIT, and Linux `taskset`. PDF generation also
uses Python ReportLab. The optional original Lean proof uses Lean 4.19.0.

```sh
cargo build --release --locked
cargo test --release --locked -- --nocapture
cargo clippy --release --all-targets -- -D warnings
cargo build --release --locked --manifest-path baseline-v0.1.0/Cargo.toml --bin benchmark
cp baseline-v0.1.0/target/release/benchmark benchmarks/submitted-benchmark
python3 scripts/make_validation_corpus.py
python3 scripts/measure.py
```

The historical runner records Rust with `/root/.cargo/bin/rustc`, matching the
measurement host; adjust this path for another installation. Its absolute binary
path is metadata-only. `measure.py` uses the system PCRE2 (10.42 on the original
host); reproducing that comparison requires explicitly installing that version.

For the current baseline, build PCRE2 10.49 using `scripts/build_pcre2_10_49.sh`.
Then provide its `lib` directory:

```sh
python3 scripts/measure_current.py /absolute/path/to/pcre2-install/lib
target/release/allocations benchmarks/corpus.json > benchmarks/allocations.csv
target/release/allocations benchmarks/validation-corpus.json > benchmarks/validation-allocations.csv
python3 scripts/analyze_performance.py
python3 scripts/build_performance_note.py
```

The analysis script uses the included single-process diagnostic stage files.
Their source records are `benchmarks/stage1-source.json` through stage3.
Each main campaign has three fresh processes, 9 randomized timing samples and 7
compile samples per engine/workload. Every timed aggregate verifies acceptance
against corpus labels. All PCRE2 host predicates use uniquely determined full
captures and are included in timing; arbitrary guard semantics require Caprice's
backtracking integration. CPU affinity is pinned; host load/frequency are uncontrolled.

## Next priorities

1. Solve capture-length constraints before trying backreference endpoints.
2. Inline stable-layout capture slots and reduce trail work using liveness.
3. Factor overlapping prefixes and select vectorized scanners by pattern shape.
4. Cache compiled programs and tier from PE to JIT only when reuse amortizes it.

Targets and evidence are detailed in the PDF. No new journal submission is part
of this bundle. `performance-changes.patch` is a review aid; full sources are
authoritative. `MANIFEST.json` binds the packaged files; measurement manifests
bind exact source/binary versions and raw results used for the reported figures.
'''
(root/'README.md').write_text(readme)
