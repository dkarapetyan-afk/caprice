#!/usr/bin/env python3
"""Run measured processes sequentially, pinned to one available logical CPU."""
import os, subprocess, sys, csv, json, hashlib
from pathlib import Path
root=Path(__file__).resolve().parents[1]
os.chdir(root)
cpu=min(os.sched_getaffinity(0))
sources=[root/'Cargo.toml',root/'Cargo.lock',*sorted((root/'src').glob('*.rs'))]
manifest={'source_sha256':{str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest() for p in sources},
          'benchmark_binary_sha256':hashlib.sha256((root/'target/release/benchmark').read_bytes()).hexdigest(),
          'logical_cpu':cpu,'processes':3,'seeds':[20261024+i for i in range(3)]}
start_run=int(sys.argv[1]) if len(sys.argv)>1 else 0
for run in range(start_run,3):
    seed=str(20261024+run)
    for command in [
        ['target/release/benchmark','benchmarks/corpus.json',f'benchmarks/run{run+1}',seed],
        [sys.executable,'scripts/bench_python.py','benchmarks/corpus.json',f'benchmarks/python{run+1}',seed]
    ]:
        subprocess.run(['taskset','-c',str(cpu)]+command,check=True)
        prefix=command[-2]
        jobs=11 if any(x.endswith('bench_python.py') for x in command) else 64
        for kind,count in [('match',9),('compile',7)]:
            rows=list(csv.DictReader(open(prefix+'-'+kind+'.csv')))
            assert len(rows)==jobs*count,(prefix,kind,len(rows))
        json.loads(Path(prefix+'-meta.json').read_text())
for run in range(1,4):
    for prefix,jobs in [(f'benchmarks/run{run}',64),(f'benchmarks/python{run}',11)]:
        for kind,count in [('match',9),('compile',7)]:
            assert len(list(csv.DictReader(open(prefix+'-'+kind+'.csv'))))==jobs*count
manifest['result_sha256']={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted((root/'benchmarks').glob('*[123]-*.csv'))}
(root/'benchmarks/measurement-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
print(f'completed three process runs; logical CPU {cpu}',flush=True)
