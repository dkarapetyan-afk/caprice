#!/usr/bin/env python3
"""Build a source/evidence archive; exclude binaries and private receipts."""
import hashlib,json,shutil,zipfile
from pathlib import Path
root=Path(__file__).resolve().parents[1]
out=root.parent/'deliverables';out.mkdir(exist_ok=True)
measurement=json.loads((root/'benchmarks/measurement-manifest.json').read_text())
for name,digest in measurement['source_sha256'].items():
    assert hashlib.sha256((root/name).read_bytes()).hexdigest()==digest,name
for name,digest in measurement['result_sha256'].items():
    assert hashlib.sha256((root/'benchmarks'/name).read_bytes()).hexdigest()==digest,name
assert len((root/'paper/journal-submission.md').read_text())<=20000
assert 'sorryAx' not in (root/'benchmarks/validation-lean.txt').read_text()
assert '9 passed; 0 failed' in (root/'benchmarks/validation-rust.txt').read_text()
allowed_dirs={'src','tests','formal','scripts','benchmarks','paper','examples'}
allowed_ext={'.rs','.lean','.clif','.ebnf','.py','.sh','.c','.json','.csv','.txt','.md','.tex','.html','.css','.pdf','.png'}
files=[root/'Cargo.toml',root/'Cargo.lock',root/'README.md']
for directory in sorted(allowed_dirs):
    files.extend(p for p in (root/directory).rglob('*') if p.is_file() and p.suffix in allowed_ext and '__pycache__' not in p.parts)
files=sorted(set(files))
manifest={'artifact':'caprice','version':'0.1.0','author':'Anonymous author',
          'validation':{'rust_tests':9,'shared_cases':20767,'extended_cases':15624,'lean':'4.19.0; no sorry or additional axioms','pdf_pages':12},
          'files':[{'path':str(p.relative_to(root)),'size_bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in files]}
mp=root/'MANIFEST.json';mp.write_text(json.dumps(manifest,indent=2)+'\n');files.append(mp)
archive=out/'caprice-v0.1.0.zip'
with zipfile.ZipFile(archive,'w',zipfile.ZIP_DEFLATED,compresslevel=9) as z:
    for p in files:z.write(p,'caprice/'+str(p.relative_to(root)))
with zipfile.ZipFile(archive) as z:
    assert z.testzip() is None
    for f in manifest['files']:assert hashlib.sha256(z.read('caprice/'+f['path'])).hexdigest()==f['sha256']
pdf=out/'caprice-manuscript.pdf';shutil.copy2(root/'paper/caprice-manuscript.pdf',pdf)
release={'files':[{'filename':p.name,'size_bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in [pdf,archive]],'bundle_entries':len(files)}
(out/'caprice-release-manifest.json').write_text(json.dumps(release,indent=2)+'\n')
print(json.dumps(release,indent=2))
