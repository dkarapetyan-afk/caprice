#!/usr/bin/env bash
set -euo pipefail
caprice_project_root="$(cd -- "$(dirname -- "$0")/.." && pwd)"
caprice_build_root="${1:-$caprice_project_root/../.build/pcre2}"
mkdir -p "$caprice_build_root"
caprice_build_root="$(cd -- "$caprice_build_root" && pwd)"
python3 - "$caprice_project_root/../third_party/pcre2/pcre2-10.49.tar.bz2" "$caprice_build_root" <<'PY'
import sys,pathlib,hashlib,shutil
source=pathlib.Path(sys.argv[1]);destination=pathlib.Path(sys.argv[2])/source.name
assert source.is_file(),'bundled PCRE2 source archive is missing'
assert hashlib.sha256(source.read_bytes()).hexdigest()=='53c156e1ba416a20da8e65395daa132da0d80e76910424caca3fcdae7831d384','source checksum mismatch'
if source.resolve()!=destination.resolve():shutil.copyfile(source,destination)
PY
# The original checked upstream builder finds the pre-seeded source and needs
# no download. Its configure flags and upstream tests are unchanged.
bash "$caprice_project_root/scripts/build_pcre2_10_49.sh" "$caprice_build_root"
