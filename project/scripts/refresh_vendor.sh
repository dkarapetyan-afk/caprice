#!/usr/bin/env bash
# Rewrite project/vendor from crates.io so it matches project/Cargo.lock.
# This needs network access. CI does not run it. Dependabot updates the
# lockfile only; commit the refreshed vendor tree with that lockfile.
# Historical baseline directories are not modified.
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
project="$(cd -- "$script_dir/.." && pwd)"
root="$(cd -- "$project/.." && pwd)"
work="$root/.local/tmp/vendor-refresh"
rm -rf "$work"
mkdir -p "$work"
export TMPDIR="$work"
export TMP="$work"
export TEMP="$work"

declare -a stashed=()
restore_configs() {
    local item cfg dest
    for item in "${stashed[@]}"; do
        cfg="${item%%|*}"
        dest="${item#*|}"
        mkdir -p "$(dirname -- "$cfg")"
        mv -- "$dest" "$cfg"
    done
    stashed=()
}
stash_one() {
    local cfg="$1" dest
    if [[ -f "$cfg" ]]; then
        dest="$work/stash${cfg#"$root"}"
        mkdir -p "$(dirname -- "$dest")"
        mv -- "$cfg" "$dest"
        stashed+=("$cfg|$dest")
    fi
}
trap restore_configs EXIT
stash_one "$project/.cargo/config.toml"
stash_one "$root/.cargo/config.toml"

cd "$project"
cargo vendor --locked "$work/vendor"
restore_configs
trap - EXIT

rm -rf "$project/vendor.new"
mv -- "$work/vendor" "$project/vendor.new"
rm -rf "$project/vendor"
mv -- "$project/vendor.new" "$project/vendor"
rm -rf "$work"
printf '%s\n' 'project/vendor matches project/Cargo.lock'
