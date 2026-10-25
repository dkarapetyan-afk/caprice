#!/usr/bin/env python3
"""Sequential fresh-process benchmarks. Do not edit sources during a run."""
from pathlib import Path
import os, sys, subprocess, hashlib, json, csv, platform
root=Path(__file__).resolve().parents[1]
os.chdir(root)
cpu=min(os.sched_getaffinity(0))
def digest(p):return hashlib.sha256(p.read_bytes()).hexdigest()
sources=[root/'Cargo.toml',root/'Cargo.lock',*sorted((root/'src').glob('*.rs')),*sorted((root/'tests').glob('*.rs')),*sorted((root/'scripts').glob('*.py'))]
manifest={'source_sha256':{str(p.relative_to(root)):digest(p) for p in sources},
          'binary_sha256':{p:digest(root/p) for p in ['target/release/benchmark','benchmarks/submitted-benchmark']},
          'corpus_sha256':{p:digest(root/'benchmarks'/p) for p in ['corpus.json','validation-corpus.json']},
          'cpu':cpu,'processes_per_corpus':3,'samples_per_job':9,'compile_samples_per_job':7,
          'platform':platform.platform(),'rustc':subprocess.check_output(['/root/.cargo/bin/rustc','--version'],text=True).strip()}
for run in range(1,4):
    for name,binary,corpus,jobs in [
        ('submitted','benchmarks/submitted-benchmark','corpus.json',64),
        ('final','target/release/benchmark','corpus.json',97),
        ('validation','target/release/benchmark','validation-corpus.json',150),
    ]:
        prefix=f'benchmarks/{name}{run}'
        print(f'{name} process {run}/3',flush=True)
        subprocess.run(['taskset','-c',str(cpu),binary,'benchmarks/'+corpus,prefix,str(739102+run)],check=True)
        for kind,n in [('match',9),('compile',7)]:
            rows=list(csv.DictReader(open(f'{prefix}-{kind}.csv')))
            assert len(rows)==jobs*n,(prefix,kind,len(rows),jobs*n)
for p in sources:assert digest(p)==manifest['source_sha256'][str(p.relative_to(root))],p
manifest['result_sha256']={p.name:digest(p) for group in ['submitted','final','validation'] for p in sorted((root/'benchmarks').glob(group+'[123]-*.csv'))}
(root/'benchmarks/final-measurement-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
print('All counts and source hashes verified.',flush=True)
