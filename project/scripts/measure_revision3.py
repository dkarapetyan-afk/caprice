#!/usr/bin/env python3
"""Frozen three-process comparisons; engines interleave within each sample."""
import csv,json,hashlib,os,subprocess,sys,shutil,platform
from pathlib import Path
root=Path(__file__).resolve().parents[1];os.chdir(root)
lib=Path(sys.argv[1]).resolve();cpu=min(os.sched_getaffinity(0));env=dict(os.environ,LD_LIBRARY_PATH=str(lib))
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
files=[root/'Cargo.toml',root/'Cargo.lock',*sorted((root/'src').glob('*.rs')),*sorted((root/'examples').glob('*.rs')),*sorted((root/'tests').glob('*.rs')),*sorted((root/'formal').glob('*.lean')),
       root/'baseline-v0.2.0/Cargo.toml',root/'baseline-v0.2.0/Cargo.lock',*sorted((root/'baseline-v0.2.0/src').glob('*.rs')),
       Path(__file__).resolve(),root/'scripts/make_revision3_corpus.py']
corpora=[('base','corpus.json',11),('development','validation-corpus.json',17),('fresh','revision3-corpus.json',16)]
manifest={'source_sha256':{str(p.relative_to(root)):sha(p) for p in files},'corpus_sha256':{n:sha(root/'benchmarks'/n) for _,n,_ in corpora},
          'binary_sha256':sha(root/'target/release/examples/compare'),'pcre2_library_sha256':sha(lib/'libpcre2-8.so'),
          'cpu':cpu,'processes_per_corpus':3,'timing_samples_per_job':9,'compile_samples_per_job':7,'cache_samples_per_workload':9,
          'platform':platform.platform(),'rustc':subprocess.check_output([shutil.which('rustc') or '/root/.cargo/bin/rustc','--version'],text=True).strip(),
          'note':'v0.2 and v0.3 are dependency aliases in the same release binary; hot modes interleave per timing sample; outputs and Boolean queries are distinct jobs.'}
for run in range(1,4):
    order=corpora[run-1:]+corpora[:run-1]
    for group,corpus,count in order:
        prefix=f'benchmarks/compare-{group}{run}';print(f'Process {run}/3: {group}',flush=True)
        subprocess.run(['taskset','-c',str(cpu),'target/release/examples/compare','benchmarks/'+corpus,prefix,str(1039102+run)],env=env,check=True)
        for kind,n,jobs in [('match',9,count*8),('compile',7,count*8),('cache',9,count)]:
            assert len(list(csv.DictReader(open(f'{prefix}-{kind}.csv'))))==jobs*n,(prefix,kind)
        meta=json.loads(Path(prefix+'-meta.json').read_text());assert meta['pcre2'].startswith('10.49'),meta
for p in files:assert sha(p)==manifest['source_sha256'][str(p.relative_to(root))],p
manifest['result_sha256']={p.name:sha(p) for p in sorted((root/'benchmarks').glob('compare-*[123]-*.csv'))}
(root/'benchmarks/revision3-measurement-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
print('All frozen source, row counts and result hashes verified.',flush=True)
