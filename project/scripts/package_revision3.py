#!/usr/bin/env python3
"""Verify frozen evidence and package the reviewable performance artifact."""
import json,hashlib,difflib,os,zipfile
from pathlib import Path
root=Path(__file__).resolve().parents[1];b=root/'benchmarks'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
manifest=json.loads((b/'revision3-measurement-manifest.json').read_text())
for f,d in manifest['source_sha256'].items():assert sha(root/f)==d,f
for f,d in manifest['result_sha256'].items():assert sha(b/f)==d,f
for f,d in manifest['corpus_sha256'].items():assert sha(b/f)==d,f
assert sha(root/'target/release/examples/compare')==manifest['binary_sha256']
# Retained historical campaign source hashes are checked against v0.2 when the
# live compiler or its tests have subsequently changed.
for name in ['current-measurement-manifest.json','final-measurement-manifest.json']:
    m=json.loads((b/name).read_text())
    for f,d in m['source_sha256'].items():
        candidates=[root/f,root/'baseline-v0.2.0'/f]
        assert any(p.is_file() and sha(p)==d for p in candidates),(name,f)
    for f,d in m['result_sha256'].items():assert sha(b/f)==d,(name,f)
before=root/'baseline-v0.2.0';parts=[]
files=[root/'Cargo.toml',root/'Cargo.lock',*sorted((root/'src').glob('*.rs')),*sorted((root/'tests').glob('*.rs'))]
for f in files:
    relative=str(f.relative_to(root));old=before/relative
    parts.extend(difflib.unified_diff(old.read_text().splitlines(True) if old.exists() else [],f.read_text().splitlines(True),fromfile='a/'+relative,tofile='b/'+relative))
(root/'performance-changes.patch').write_text(''.join(parts))
items=[]
for directory,dirs,names in os.walk(root):
    dirs[:]=sorted(d for d in dirs if d not in {'target','__pycache__','pcre2-build'} and not d.startswith('.'))
    for name in sorted(names):
        if name in {'MANIFEST.json','submitted-benchmark','v0.2-benchmark'} or name.endswith('.tmp'):continue
        items.append(Path(directory)/name)
package={'artifact':'Caprice performance experiment','version':'0.3.0','files':{str(p.relative_to(root)):sha(p) for p in items}}
(root/'MANIFEST.json').write_text(json.dumps(package,indent=2)+'\n');items.append(root/'MANIFEST.json')
target=root.parent/'deliverables/caprice-performance-experiment.zip'
with zipfile.ZipFile(target,'w',zipfile.ZIP_DEFLATED,compresslevel=9) as z:
    for p in items:z.write(p,'caprice-performance/'+str(p.relative_to(root)))
with zipfile.ZipFile(target) as z:
    assert z.testzip() is None
    for f,d in package['files'].items():assert hashlib.sha256(z.read('caprice-performance/'+f)).hexdigest()==d,f
print(json.dumps({'path':str(target),'files':len(items),'bytes':target.stat().st_size,'sha256':sha(target)},indent=2))
