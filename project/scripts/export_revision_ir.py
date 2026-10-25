#!/usr/bin/env python3
"""Export current compiler IR; normalize the legacy example's version banner."""
from pathlib import Path
import subprocess,tomllib,shutil
root=Path(__file__).resolve().parents[1]
version=tomllib.loads((root/'Cargo.toml').read_text())['package']['version']
target=root/'formal/residual.clif';archive=root/'history/v0.2-residual.clif'
if target.exists() and not archive.exists():shutil.copy2(target,archive)
text=subprocess.check_output(['cargo','run','--release','--locked','--example','dump_ir'],cwd=root,text=True)
lines=text.splitlines();lines[0]=f'; Caprice v{version} residual native byte-operation IR'
target.write_text('\n'.join(lines)+'\n')
print(target)
