#!/usr/bin/env python3
import hashlib,json,os,sys,zipfile,re
from pathlib import Path
root=Path(__file__).resolve().parents[1]
target=Path(sys.argv[1]).expanduser().resolve()
if root==target.parent or root in target.parents:raise ValueError('write the output ZIP outside the handoff directory')
files=[]
for base,dirs,names in os.walk(root):
    dirs[:]=sorted(d for d in dirs if d not in {'target','.build','__pycache__','.git','pcre2-build'} and not d.startswith('.venv'))
    for name in sorted(names):
        if name in {'HANDOFF_MANIFEST.json'} or name.endswith(('.tmp','.swp')):continue
        p=Path(base)/name
        if p.is_symlink():raise ValueError(f'symlinks are not packaged: {p}')
        files.append(p)
version=re.search(r'(?m)^version\s*=\s*"([^"]+)"',(root/'project/Cargo.toml').read_text())
if version is None:raise ValueError('read the project package version before packaging')
manifest={'artifact':'Caprice complete local handoff','engine_version':version.group(1),
    'files':{str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest() for p in files}}
(root/'HANDOFF_MANIFEST.json').write_text(json.dumps(manifest,indent=2)+'\n')
files.append(root/'HANDOFF_MANIFEST.json');target.parent.mkdir(parents=True,exist_ok=True)
with zipfile.ZipFile(target,'w',zipfile.ZIP_DEFLATED,compresslevel=6,strict_timestamps=False) as z:
    for p in files:z.write(p,'caprice-local-handoff/'+str(p.relative_to(root)))
with zipfile.ZipFile(target) as z:
    assert z.testzip() is None
    for name,digest in manifest['files'].items():assert hashlib.sha256(z.read('caprice-local-handoff/'+name)).hexdigest()==digest,name
print(json.dumps({'path':str(target),'files':len(files),'bytes':target.stat().st_size,'sha256':hashlib.sha256(target.read_bytes()).hexdigest()},indent=2))
