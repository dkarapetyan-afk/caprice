#!/usr/bin/env bash
# Build the exact upstream source used for the current-version comparison.
set -euo pipefail
caprice_build_root="${1:-$PWD/pcre2-build}"
mkdir -p "$caprice_build_root"
caprice_build_root="$(cd "$caprice_build_root" && pwd)"
python3 - "$caprice_build_root" <<'PY'
import sys,urllib.request,hashlib
from pathlib import Path
p=Path(sys.argv[1])/'pcre2-10.49.tar.bz2'
url='https://github.com/PCRE2Project/pcre2/releases/download/pcre2-10.49/pcre2-10.49.tar.bz2'
if not p.exists():urllib.request.urlretrieve(url,p)
assert hashlib.sha256(p.read_bytes()).hexdigest()=='53c156e1ba416a20da8e65395daa132da0d80e76910424caca3fcdae7831d384','source checksum mismatch'
PY
tar --no-same-owner -xjf "$caprice_build_root/pcre2-10.49.tar.bz2" -C "$caprice_build_root"
cd "$caprice_build_root/pcre2-10.49"
./configure --enable-jit --enable-shared --disable-static --disable-pcre2grep \
    --disable-pcre2-16 --disable-pcre2-32 --prefix="$caprice_build_root/install" CFLAGS='-O3 -DNDEBUG'
make -j "${CAPRICE_BUILD_JOBS:-2}"
make check
make install
printf 'PCRE2 library directory: %s\n' "$caprice_build_root/install/lib"
