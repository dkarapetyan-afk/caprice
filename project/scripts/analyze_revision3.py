#!/usr/bin/env python3
"""Summaries from the frozen, interleaved revision-3 campaign only."""
import csv,json,statistics,collections
from pathlib import Path
root=Path(__file__).resolve().parents[1];b=root/'benchmarks'
out={'suites':{},'rows':[],'method':'median of 27 timing samples; 21 compile samples; 27 one-resident-key cache-hit samples per workload'}
for group,corpus in [('base','corpus.json'),('development','validation-corpus.json'),('fresh','revision3-corpus.json')]:
    timing=collections.defaultdict(list);comp=collections.defaultdict(list);cache=collections.defaultdict(list);prun=collections.defaultdict(list)
    for run in range(1,4):
        local=collections.defaultdict(list)
        for r in csv.DictReader((b/f'compare-{group}{run}-match.csv').open()):
            key=r['workload'],r['engine'];v=float(r['ns_per_match']);timing[key].append(v);local[key].append(v)
        for k,v in local.items():prun[k].append(statistics.median(v))
        for r in csv.DictReader((b/f'compare-{group}{run}-compile.csv').open()):comp[r['workload'],r['engine']].append(float(r['compile_ns']))
        for r in csv.DictReader((b/f'compare-{group}{run}-cache.csv').open()):cache[r['workload']].append(float(r['ns_per_lookup']))
    rows=[]
    for w in json.loads((b/corpus).read_text()):
        i=w['id'];m={e:statistics.median(v) for (wi,e),v in timing.items() if wi==i}
        q={e:statistics.quantiles(v,n=4,method='inclusive') for (wi,e),v in timing.items() if wi==i}
        c={e:statistics.median(v) for (wi,e),v in comp.items() if wi==i}
        row={'suite':group,'workload':i,'v02_ns':m['v02-jit'],'v03_ns':m['v03-jit'],'pcre_ns':m['pcre2-fast'],
             'speedup':m['v02-jit']/m['v03-jit'],'v03_over_pcre':m['v03-jit']/m['pcre2-fast'],
             'v02_output_ns':m['v02-output'],'v03_output_ns':m['v03-output'],'output_speedup':m['v02-output']/m['v03-output'],
             'v03_pe_ns':m['v03-pe'],'v02_compile_us':c['v02-jit']/1000,'v03_compile_us':c['v03-jit']/1000,'pcre_compile_us':c['pcre2-fast']/1000,
             'cache_hit_ns':statistics.median(cache[i]),'samples_per_engine':27,'process_medians':{e:prun[i,e] for e in m},
             'v03_iqr_ns':[q['v03-jit'][0],q['v03-jit'][2]],'pcre_iqr_ns':[q['pcre2-fast'][0],q['pcre2-fast'][2]],
             'input_bytes_range':[min(len(x['text'].encode()) for x in w['inputs']),max(len(x['text'].encode()) for x in w['inputs'])],
             'positive_inputs':sum(x['expected'] for x in w['inputs'])}
        rows.append(row);out['rows'].append(row)
    out['suites'][group]={'workloads':len(rows),'v03_faster_than_v02':sum(r['speedup']>1 for r in rows),
        'v03_faster_than_pcre':sum(r['v03_over_pcre']<1 for r in rows),'within_10_percent_of_pcre':sum(r['v03_over_pcre']<=1.1 for r in rows),
        'speedup_geomean':statistics.geometric_mean(r['speedup'] for r in rows),'v03_over_pcre_geomean':statistics.geometric_mean(r['v03_over_pcre'] for r in rows),
        'output_speedup_geomean':statistics.geometric_mean(r['output_speedup'] for r in rows)}
out['overall']={'speedup_geomean':statistics.geometric_mean(r['speedup'] for r in out['rows']),
    'output_speedup_geomean':statistics.geometric_mean(r['output_speedup'] for r in out['rows']),
    'faster_than_v02':sum(r['speedup']>1 for r in out['rows']),'faster_than_pcre':sum(r['v03_over_pcre']<1 for r in out['rows']),
    'compile_ratio_geomean':statistics.geometric_mean(r['v03_compile_us']/r['v02_compile_us'] for r in out['rows'])}
(b/'revision3-analysis.json').write_text(json.dumps(out,indent=2)+'\n')
with (b/'revision3-summary.csv').open('w') as f:
    fields=[k for k,v in out['rows'][0].items() if not isinstance(v,(list,dict))];w=csv.DictWriter(f,fieldnames=fields,extrasaction='ignore');w.writeheader();w.writerows(out['rows'])
print('overall',out['overall'])
for k,v in out['suites'].items():print(k,v)
for r in out['rows']:print(r['workload'],round(r['v02_ns'],1),round(r['v03_ns'],1),round(r['pcre_ns'],1),round(r['speedup'],2),round(r['v03_over_pcre'],2))
