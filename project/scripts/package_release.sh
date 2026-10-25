#!/usr/bin/env bash
# Pack release binaries with the bundled PCRE2 8-bit library.
# The archive is a GitHub release asset. Each executable gets an RPATH of
# $ORIGIN/../lib so it loads that library without LD_LIBRARY_PATH.
set -euo pipefail

if [[ $# -ne 1 || -z "${1:-}" ]]; then
    printf '%s\n' 'usage: package_release.sh OUTPUT_DIR' >&2
    exit 2
fi

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
project="$(cd -- "$script_dir/.." && pwd)"
root="$(cd -- "$project/.." && pwd)"
output="$(mkdir -p -- "$1" && cd -- "$1" && pwd)"
target_dir="${CARGO_TARGET_DIR:-$project/target}"
if [[ "$target_dir" != /* ]]; then
    target_dir="$project/$target_dir"
fi
pcre_lib="${CAPRICE_PCRE_LIB:-$root/.build/pcre2/install/lib}"
tarball="$root/third_party/pcre2/pcre2-10.49.tar.bz2"
work="${TMPDIR:-$root/.local/tmp}/caprice-package"
version="$(python3 - "$project/Cargo.toml" <<'PY'
import pathlib, re, sys
text = pathlib.Path(sys.argv[1]).read_text()
match = re.search(r'(?m)^version\s*=\s*"([^"]+)"', text)
if match is None:
    raise SystemExit("package version missing from Cargo.toml")
print(match.group(1))
PY
)"
if [[ -n "${CAPRICE_TARGET:-}" ]]; then
    triple="$CAPRICE_TARGET"
else
    triple="$(rustc -vV | awk '/^host: /{print $2}')"
fi
name="caprice-${version}-${triple}"

if ! command -v patchelf >/dev/null; then
    printf '%s\n' 'patchelf is required to set the package RPATH' >&2
    exit 1
fi
if [[ ! -f "$pcre_lib/libpcre2-8.so" ]]; then
    printf 'PCRE2 library missing: %s\n' "$pcre_lib/libpcre2-8.so" >&2
    exit 1
fi
if [[ ! -f "$tarball" ]]; then
    printf 'PCRE2 source archive missing: %s\n' "$tarball" >&2
    exit 1
fi
for bin in caprice benchmark allocations; do
    if [[ ! -x "$target_dir/release/$bin" ]]; then
        printf 'release binary missing: %s\n' "$target_dir/release/$bin" >&2
        exit 1
    fi
done
for example in compare dump_ir; do
    if [[ ! -x "$target_dir/release/examples/$example" ]]; then
        printf 'release example missing: %s\n' "$target_dir/release/examples/$example" >&2
        exit 1
    fi
done

rm -rf "$work"
mkdir -p "$work/$name/bin" "$work/$name/lib" "$work/$name/share/pcre2"
for bin in caprice benchmark allocations; do
    cp -a "$target_dir/release/$bin" "$work/$name/bin/$bin"
done
for example in compare dump_ir; do
    cp -a "$target_dir/release/examples/$example" "$work/$name/bin/$example"
done
cp -a "$pcre_lib"/libpcre2-8.so* "$work/$name/lib/"
rm -f "$work/$name/lib"/*.la
tar -xjf "$tarball" -O pcre2-10.49/LICENCE.md >"$work/$name/share/pcre2/LICENCE.md"
[[ -s "$work/$name/share/pcre2/LICENCE.md" ]]
cp -a "$root/LICENSE-MIT" "$root/LICENSE-APACHE" "$work/$name/"

for bin in "$work/$name/bin"/*; do
    patchelf --set-rpath '$ORIGIN/../lib' "$bin"
done

cat >"$work/$name/README.txt" <<EOF
Caprice ${version} for ${triple}.

bin/caprice MODE PATTERN INPUT [NAME=VALUE ...]
  MODE is raw, pe, or jit.

bin/benchmark, bin/allocations, and bin/compare read the corpus JSON paths
documented in project/README.md. bin/dump_ir prints sample native IR.

lib/ holds PCRE2 10.49, used by the comparison backends. The executables
load it through an RPATH of \$ORIGIN/../lib. share/pcre2/LICENCE.md is the
upstream PCRE2 license. Rust dependency notices stay in project/vendor of
the source tree.

Caprice itself is available under either LICENSE-MIT or LICENSE-APACHE.
EOF

(
    cd "$work/$name"
    find . -type f ! -name SHA256SUMS -print0 | sort -z | xargs -0 sha256sum
) >"$work/$name/SHA256SUMS"

env -u LD_LIBRARY_PATH "$work/$name/bin/caprice" jit '(?<x>[a-z]+)-\k<x>' 'abc-abc' \
    | python3 -c 'import json,sys; result=json.load(sys.stdin); assert result["status"]=="match", result'
linked=0
for bin in "$work/$name/bin"/*; do
    if ! readelf -d "$bin" | grep -q 'libpcre2-8'; then
        continue
    fi
    linked=1
    resolved="$(env -u LD_LIBRARY_PATH ldd "$bin" | sed -n 's/.*libpcre2-8\.so[^ ]* => \(.*\) (0x[0-9a-f]*)$/\1/p')"
    canonical="$(readlink -f "$resolved")"
    expected="$(readlink -f "$work/$name/lib")"
    case "$canonical" in
        "$expected"/*) ;;
        *)
            printf '%s resolved libpcre2-8 outside the package: %s\n' "$bin" "$resolved" >&2
            exit 1
            ;;
    esac
done
if [[ "$linked" -ne 1 ]]; then
    printf '%s\n' 'no packaged binary links libpcre2-8' >&2
    exit 1
fi

archive="$output/$name.tar.gz"
tar -C "$work" -czf "$archive" "$name"
(
    cd "$output"
    sha256sum "$name.tar.gz" >"$name.tar.gz.sha256"
)
rm -rf "$work"
printf '%s\n' "$archive"
