#!/usr/bin/env bash
set -euo pipefail
caprice_project_root="$(cd -- "$(dirname -- "$0")/.." && pwd)"
caprice_library_dir="$caprice_project_root/../.build/pcre2/install/lib"
if [[ ! -f "$caprice_library_dir/libpcre2-8.so" ]]; then
    printf '%s\n' 'Build PCRE2 first: bash scripts/build_pcre2_local.sh' >&2
    exit 1
fi
caprice_cargo_config="$(python3 - "$caprice_library_dir" <<'PY'
import json,sys
print('build.rustflags = '+json.dumps(['-L','native='+sys.argv[1]]))
PY
)"
cd "$caprice_project_root"
export LD_LIBRARY_PATH="$caprice_library_dir${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
exec cargo --config "$caprice_cargo_config" "$@"
