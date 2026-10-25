#!/usr/bin/env python3
"""Repeat all final workloads against an explicitly selected shared PCRE2."""
import csv,hashlib,json,os,subprocess,sys
from pathlib import Path
root=Path(__file__).resolve().parents[1];os.chdir(root)
lib=Path(sys.argv[1]).resolve();cpu=min(os.sched_getaffinity(0))
env=dict(os.environ,LD_LIBRARY_PATH=str(lib))
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
manifest={'library':str(lib),'library_sha256':sha(lib/'libpcre2-8.so'),'cpu':cpu,
    'binary_sha256':sha(root/'target/release/benchmark'),
    'source_sha256':{str(p.relative_to(root)):sha(p) for p in [root/'Cargo.toml',root/'Cargo.lock',*sorted((root/'src').glob('*.rs'))]},
    'corpus_sha256':{name:sha(root/'benchmarks'/name) for name in ['corpus.json','validation-corpus.json']},
    'processes':3,'timing_samples_per_job':9,'compile_samples_per_job':7}
for run in range(1,4):
    for group,corpus,jobs in [('current','corpus.json',97),('current-validation','validation-corpus.json',150)]:
        prefix=f'benchmarks/{group}{run}'
        print(f'PCRE2 current: {group} process {run}/3',flush=True)
        subprocess.run(['taskset','-c',str(cpu),'target/release/benchmark','benchmarks/'+corpus,prefix,str(839102+run)],env=env,check=True)
        for kind,n in [('match',9),('compile',7)]:
            assert len(list(csv.DictReader(open(f'{prefix}-{kind}.csv'))))==jobs*n
        meta=json.loads(Path(prefix+'-meta.json').read_text())
        assert meta['pcre2'].startswith('10.49'),meta['pcre2']
for name,digest in manifest['source_sha256'].items():assert sha(root/name)==digest,name
manifest['result_sha256']={p.name:sha(p) for p in sorted((root/'benchmarks').glob('current*[123]-*.csv'))}
(root/'benchmarks/current-measurement-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
print('Current-version data and source hashes verified.',flush=True)
