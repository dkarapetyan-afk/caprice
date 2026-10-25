#!/usr/bin/env python3
import hashlib,json,sys
from pathlib import Path,PurePosixPath
root=Path(__file__).resolve().parents[1]
m=json.loads((root/'HANDOFF_MANIFEST.json').read_text());fail=[]
for name,digest in m['files'].items():
    relative=PurePosixPath(name)
    if relative.is_absolute() or '..' in relative.parts:raise ValueError('invalid manifest path')
    p=root/name
    if not p.is_file() or hashlib.sha256(p.read_bytes()).hexdigest()!=digest:fail.append(name)
if fail:
    print('Changed or missing files:',*fail,sep='\n');sys.exit(1)
print(f"Verified {len(m['files'])} files for Caprice {m['engine_version']}.")
