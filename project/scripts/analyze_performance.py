#!/usr/bin/env python3
import csv,json,statistics,collections,math
from pathlib import Path
root=Path(__file__).resolve().parents[1];b=root/'benchmarks'
def summarize(prefix,runs):
    timings=collections.defaultdict(list);comp=collections.defaultdict(list);perrun=collections.defaultdict(list)
    for run in runs:
        local=collections.defaultdict(list)
        for r in csv.DictReader((b/f'{prefix}{run}-match.csv').open()):
            k=(r['workload'],r['engine']);t=float(r['ns_per_match']);timings[k].append(t);local[k].append(t)
        for k,vs in local.items():perrun[k].append(statistics.median(vs))
        for r in csv.DictReader((b/f'{prefix}{run}-compile.csv').open()):comp[r['workload'],r['engine']].append(float(r['compile_ns']))
    rows=[]
    for (w,e),v in sorted(timings.items()):
        q=statistics.quantiles(v,n=4,method='inclusive');c=comp[w,e]
        rows.append(dict(workload=w,engine=e,samples=len(v),median_ns=statistics.median(v),q1_ns=q[0],q3_ns=q[2],process_medians_ns=perrun[w,e],compile_median_ns=statistics.median(c)))
    with (b/f'{prefix}-summary.csv').open('w') as f:
        wr=csv.DictWriter(f,fieldnames=rows[0].keys());wr.writeheader();wr.writerows(rows)
    return {r['workload']+'|'+r['engine']:r for r in rows}
sets={p:summarize(p,[1,2,3]) for p in ['submitted','final','validation','current','current-validation']}
sets.update({p:summarize(p,['']) for p in ['stage1','stage2','stage3']})
out={'sets':sets,'comparisons':{},'corpora':{}}
for name,filename in [('current','corpus.json'),('current-validation','validation-corpus.json')]:
    corpus=json.loads((b/filename).read_text());rows=[]
    for w in corpus:
        d=sets[name];i=w['id'];j=d[i+'|jit-reuse'];p=d[i+'|pcre2-fast'];pe=d[i+'|pe-reuse']
        r=dict(workload=i,jit_ns=j['median_ns'],pe_ns=pe['median_ns'],allocated_jit_ns=d[i+'|jit']['median_ns'],pcre_ns=p['median_ns'],ratio=j['median_ns']/p['median_ns'],jit_compile_us=j['compile_median_ns']/1000,pcre_compile_us=p['compile_median_ns']/1000)
        r['process_ratios']=[a/z for a,z in zip(j['process_medians_ns'],p['process_medians_ns'])]
        saving=pe['median_ns']-j['median_ns'];extra=j['compile_median_ns']-pe['compile_median_ns']
        r['jit_break_even_vs_pe_matches']=math.ceil(extra/saving) if saving>0 and extra>0 else None
        if i+'|pe' in sets['submitted']:
            r['submitted_pe_ns']=sets['submitted'][i+'|pe']['median_ns']
            r['submitted_jit_ns']=sets['submitted'][i+'|jit']['median_ns']
            r['speedup_vs_submitted_pe']=r['submitted_pe_ns']/r['jit_ns']
        rows.append(r)
    out['comparisons'][name]={'rows':rows,'wins_by_median':sum(r['ratio']<1 for r in rows),'within_10_percent':sum(r['ratio']<=1.1 for r in rows),'pcre_over_jit_geomean':statistics.geometric_mean(1/r['ratio'] for r in rows),'worst_jit_over_pcre':max(r['ratio'] for r in rows),'best_jit_over_pcre':min(r['ratio'] for r in rows)}
    if name=='current':out['comparisons'][name]['speedup_vs_submitted_pe_geomean']=statistics.geometric_mean(r['speedup_vs_submitted_pe'] for r in rows)
    out['corpora'][name]=[{"workload":w['id'],"inputs":len(w['inputs']),"positive":sum(x['expected'] for x in w['inputs']),"min_bytes":min(len(x['text'].encode()) for x in w['inputs']),"max_bytes":max(len(x['text'].encode()) for x in w['inputs'])} for w in corpus]
(b/'performance-analysis.json').write_text(json.dumps(out,indent=2)+'\n')
for name,c in out['comparisons'].items():
    print(name,{k:v for k,v in c.items() if k!='rows'})
    for r in c['rows']:print(r['workload'],round(r['jit_ns'],1),round(r['pcre_ns'],1),round(r['ratio'],2))
