#!/usr/bin/env python3
"""Secondary baseline: CPython bytes re; includes Python loop and host predicate."""
import csv,json,re,sys,time,random,platform
from pathlib import Path
work=json.loads(Path(sys.argv[1] if len(sys.argv)>1 else 'benchmarks/corpus.json').read_text())
out=sys.argv[2] if len(sys.argv)>2 else 'benchmarks/python'
rng=random.Random(int(sys.argv[3]) if len(sys.argv)>3 else 20261024)
def translate(s):
    s=re.sub(r'\(\?<([A-Za-z_][A-Za-z_0-9]*)>',r'(?P<\1>',s)
    return re.sub(r'\\k<([A-Za-z_][A-Za-z_0-9]*)>',r'(?P=\1)',s)
def check(rx,s,post):
    m=rx.fullmatch(s)
    if m is None:return False
    if post=='none':return True
    if post=='lower_admin':return m[1].lower()==b'admin'
    if post=='upper_pair':return m[1].upper()==m[2]
    if post=='len16':return len(m[1])<=16
    raise ValueError(post)
def run(job,n):
    w,rx,inputs=job;t=time.perf_counter_ns();total=0
    for _ in range(n):
        for s in inputs:total+=check(rx,s,w['post'])
    return time.perf_counter_ns()-t,total
jobs=[]
with open(out+'-compile.csv','w') as f:
    wr=csv.writer(f);wr.writerow(['workload','engine','sample','compile_ns','native_bytes','instructions'])
    for w in work:
        p=translate(w['baseline']).encode()
        for sample in range(7):
            re.purge();t=time.perf_counter_ns();rx=re.compile(p);ns=time.perf_counter_ns()-t;wr.writerow([w['id'],'python-re',sample,ns,0,0])
        inputs=[i['text'].encode() for i in w['inputs']]
        assert all(check(rx,s,w['post'])==i['expected'] for s,i in zip(inputs,w['inputs']))
        job=(w,rx,inputs);run(job,3);ns,_=run(job,4);rounds=max(1,min(100000,int(20_000_000*4/max(1,ns))));jobs.append((job,rounds))
with open(out+'-match.csv','w') as f:
    wr=csv.writer(f);wr.writerow(['workload','engine','sample','rounds','inputs','total_ns','ns_per_match','matches'])
    for sample in range(9):
        order=list(range(len(jobs)));rng.shuffle(order)
        for j in order:
            job,rounds=jobs[j];w,_,inputs=job;ns,total=run(job,rounds);n=rounds*len(inputs)
            assert total==rounds*sum(i['expected'] for i in w['inputs'])
            wr.writerow([w['id'],'python-re',sample,rounds,n,ns,ns/n,total])
Path(out+'-meta.json').write_text(json.dumps({'python':sys.version,'platform':platform.platform(),'samples':9,'compile_samples':7,'loop_overhead_included':True},indent=2))
