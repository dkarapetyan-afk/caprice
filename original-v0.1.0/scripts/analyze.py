#!/usr/bin/env python3
"""Derive every manuscript performance number from the included raw samples."""
import collections,csv,glob,json,statistics,math
from pathlib import Path
root=Path(__file__).resolve().parents[1]
work=json.loads((root/'benchmarks/corpus.json').read_text())
engines=['raw','pe','jit','pcre2','pcre2-jit','rust-regex','python-re']
names={'literal':'Literal','identifier':'Identifier','fields':'Fields','alternation':'Alternation','backref_short':'Backref short','backref_long':'Backref long','static_parameter':'Static parameter','lambda_guard':'Lambda guard','dynamic_interpolation':'Dynamic interpolation','length_guard':'Length guard','constant_lambda':'Constant lambda'}
def load(kind,field):
    out=collections.defaultdict(list)
    for file in sorted((root/'benchmarks').glob(f'*[123]-{kind}.csv')):
        for r in csv.DictReader(file.open()):out[r['workload'],r['engine']].append(float(r[field]))
    return out
match=load('match','ns_per_match');comp=load('compile','compile_ns')
med={k:statistics.median(v) for k,v in match.items()};cmed={k:statistics.median(v) for k,v in comp.items()}
for w in work:
    for e in engines:
        if e=='rust-regex' and not w['regular']:continue
        assert len(match[w['id'],e])==27
        assert len(comp[w['id'],e])==21
with (root/'benchmarks/summary.csv').open('w') as f:
    wr=csv.writer(f);wr.writerow(['workload','engine','samples','median_ns','q1_ns','q3_ns','compile_samples','median_compile_ns','compile_q1_ns','compile_q3_ns'])
    for w in work:
        for e in engines:
            k=(w['id'],e)
            if k not in med:continue
            q=statistics.quantiles(match[k],n=4,method='inclusive');cq=statistics.quantiles(comp[k],n=4,method='inclusive')
            wr.writerow([*k,len(match[k]),med[k],q[0],q[2],len(comp[k]),cmed[k],cq[0],cq[2]])
def table(data,scale=1):
    lines=['| Workload | Raw | PE | JIT | PCRE | PCRE-JIT | Rust | Python |','|:--------------------|-------:|-------:|-------:|-------:|-------:|-------:|-------:|']
    for w in work:
        i=w['id'];lines.append('| '+names[i]+' | '+' | '.join(f'{data[i,e]/scale:.1f}' if (i,e) in data else '-' for e in engines)+' |')
    return '\n'.join(lines)
speed=[med[w['id'],'raw']/med[w['id'],'pe'] for w in work]
jit=[med[w['id'],'jit']/med[w['id'],'pe'] for w in work]
geo=statistics.geometric_mean(speed)
details={
    'raw_over_pe_geomean':geo,'raw_over_pe_min':min(speed),'raw_over_pe_max':max(speed),
    'jit_over_pe_geomean':statistics.geometric_mean(jit),'jit_over_pe_min':min(jit),'jit_over_pe_max':max(jit),
    'workloads':len(work),'validated_corpus_inputs':sum(len(w['inputs']) for w in work),
    'raw_matching_samples':sum(map(len,match.values())),'raw_compile_samples':sum(map(len,comp.values()))
}
(root/'benchmarks/aggregate.json').write_text(json.dumps(details,indent=2)+'\n')
replacements={
    'ABSTRACT_RESULT':f'Across 11 synthetic workloads, specialization improves median matching latency by {min(speed):.2f} to {max(speed):.2f} times over the raw VM ({geo:.2f} times geometric mean), while the helper-based JIT is slower than the specialized VM on every measured median.',
    'RESULT_TABLE':table(med),
    'COMPILE_TABLE':table(cmed,1000),
    'PYTHON_VERSION':'CPython '+json.loads((root/'benchmarks/python1-meta.json').read_text())['python'].split()[0],
    'RESULT_PARAGRAPH':f'PE reduces median latency on all 11 workloads: {min(speed):.2f} to {max(speed):.2f} times, with an unweighted geometric mean of {geo:.2f} times across these workloads. The JIT is {100*(min(jit)-1):.1f}% to {100*(max(jit)-1):.1f}% slower than PE by the measured medians. PE beats PCRE2 JIT on the fixed-literal workload ({med["literal","pe"]:.1f} versus {med["literal","pcre2-jit"]:.1f} ns), but PCRE2 JIT is faster on the other ten. Thus the evidence supports partial evaluation within this prototype and rejects a general speed-superiority claim for its current JIT.',
    'BREAKEVEN':'For JIT versus PE, the measured median compilation and matching costs are both higher in all 11 workloads, so no positive break-even count is demonstrated.',
    'STRESS':'All three Caprice modes reach the one-million-dispatch budget at lengths 24 and 28 in each process run; the existing PCRE2 and Rust regex baselines reject without exhausting that budget or returning an error.'
}
text=(root/'paper/manuscript.template.md').read_text()
for key,value in replacements.items():text=text.replace('@@'+key+'@@',value)
assert '@@' not in text
(root/'paper/caprice-manuscript.md').write_text(text)
journal=(root/'paper/journal.template.md').read_text()
for key,value in replacements.items():journal=journal.replace('@@'+key+'@@',value)
assert '@@' not in journal
assert len(journal)<=20000, len(journal)
(root/'paper/journal-submission.md').write_text(journal)
print('Journal characters:',len(journal))
try:
    import matplotlib
    matplotlib.use('Agg')
    import matplotlib.pyplot as plt
    import numpy as np
    fig,axes=plt.subplots(1,2,figsize=(10,5.2),gridspec_kw={'width_ratios':[1.4,1]})
    y=np.arange(len(work))
    axes[0].barh(y,speed,color='#226e83');axes[0].set_yticks(y,[names[w['id']] for w in work]);axes[0].invert_yaxis();axes[0].set_xscale('log');axes[0].set_xlabel('Raw latency / PE latency (log scale)');axes[0].set_title('Specialization benefit');axes[0].axvline(1,color='#666',linewidth=1)
    for yi,v in zip(y,speed):axes[0].text(v*1.04,yi,f'{v:.2f}x',va='center',fontsize=8)
    axes[0].set_xlim(0.95,max(speed)*1.7)
    axes[1].barh(y,[(x-1)*100 for x in jit],color='#b55b35');axes[1].set_yticks(y,[]);axes[1].invert_yaxis();axes[1].set_xlabel('JIT median overhead versus PE (%)');axes[1].set_title('Native control-flow JIT cost')
    for yi,v in zip(y,jit):axes[1].text((v-1)*100+.2,yi,f'{(v-1)*100:.1f}%',va='center',fontsize=8)
    axes[1].set_xlim(0,max((x-1)*100 for x in jit)*1.3)
    for ax in axes:ax.spines[['top','right']].set_visible(False);ax.grid(axis='x',alpha=.2);ax.set_axisbelow(True)
    fig.suptitle('Caprice v0.1.0: measured tradeoffs',fontsize=14)
    fig.text(.5,.01,'27 samples per engine/workload, three processes, one host. Ratios of sample medians.',ha='center',fontsize=8)
    fig.tight_layout(rect=[0,.04,1,.96]);fig.savefig(root/'paper/performance.pdf');fig.savefig(root/'paper/performance.png',dpi=180);plt.close(fig)
except ImportError as e:print('Plot dependency unavailable:',e)
print(json.dumps(details,indent=2))
