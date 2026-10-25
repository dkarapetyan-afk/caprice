# Caprice local handoff - v0.3.0

Extract the ZIP into a Linux directory, such as ~/src/, then start development in **project/**. This is the latest engine, with all source,
tests, proofs, benchmark inputs/results, older baselines and vendored Rust crates.
The commands below target Linux x86-64, including Ubuntu inside WSL2 on Windows.

## 1. Install system tools and Rust once

On Ubuntu/WSL, obtain a C compiler, make, Python, tar and bzip2:

```sh
sudo apt update
sudo apt install -y build-essential pkg-config python3 perl tar bzip2 unzip
```

Install Rust through [rustup](https://rust-lang.github.io/rustup/installation/index.html).
The measured toolchain was **rustc/cargo 1.99.1**. For the same version:

```sh
rustup toolchain install 1.99.1 --profile minimal --component clippy --component rustfmt
cd caprice-local-handoff/project
rustup override set 1.99.1
```

If you already have a compatible compiler, you can use it directly. Compiler/C
toolchains, Python packages and Lean binaries are not embedded in this archive.
Rust dependency sources and the exact PCRE2 source tarball are included, so the
following C/Rust build steps do not require network access after tool installation.

## 2. Build the bundled PCRE2 library, then build/test the engine

From project/:

```sh
bash scripts/build_pcre2_local.sh
bash scripts/cargo_local.sh test --release --frozen
bash scripts/cargo_local.sh build --release --frozen --example compare
bash scripts/cargo_local.sh clippy --release --frozen --all-targets -- -D warnings
```

`build_pcre2_local.sh` checks the bundled PCRE2 10.49 source checksum and builds
it into ../.build/pcre2/install. `cargo_local.sh` selects that library for linking
and execution. Cargo's configuration uses project/vendor/ instead of downloading
crates. `--frozen` enforces the lockfile and disables Cargo network access.
The source-vendoring behavior is documented in the
[Cargo book](https://doc.rust-lang.org/cargo/commands/cargo-vendor.html).

Try a match:

```sh
bash scripts/cargo_local.sh run --release --frozen --bin caprice -- jit '(?<x>[a-z]+)-\k<x>' 'abc-abc'
```

Run shell commands inside WSL/Linux. Native Windows builds and benchmark scripts
have not been validated. The existing native backend requires a 64-bit
little-endian host; the implementation was tested on Linux x86-64.

## 3. Continue performance work

Read **CONTINUE_HERE.md**, then project/README.md and
papers/caprice-performance-note-v0.3.pdf. The current largest gap is generic
compound-expression evaluation, followed by repeated captures and first JIT
compilation. The next task should compile residual typed expressions while
preserving strict evaluation, capture rollback and expression limits.

To repeat the interleaved v0.2/v0.3/PCRE2 campaign from project/:

```sh
bash scripts/cargo_local.sh build --release --frozen --example compare
python3 scripts/measure_revision3.py ../.build/pcre2/install/lib
python3 scripts/analyze_revision3.py
```

The 44 workloads, comparison baseline sources and original results are included.
Re-running a campaign replaces its local result files; keep a copy or use version
control if you want to retain the recorded run. Results depend on the local CPU.

## 4. Check the formal proofs (optional)

Install Lean through [elan](https://github.com/leanprover/elan), then use the
included formal/lean-toolchain pin for **Lean 4.19.0**. From the top-level handoff directory:

```sh
cd project/formal
lean Specialization.lean
lean AffineLengths.lean
```

These use Lean's bundled Std library and need no Mathlib checkout. The first
elan invocation may download the pinned Lean toolchain. These proofs cover the
specialization kernel and affine arithmetic; they do not verify the Rust parser,
AST length extraction, full VM or Cranelift backend.

## 5. Papers and optional document builds

- papers/caprice-manuscript.pdf: exact submitted manuscript.
- papers/original-source/: editable Markdown, LaTeX, HTML, CSS and figures.
- papers/caprice-performance-note-v0.3.pdf: latest performance follow-up.
- papers/caprice-performance-note-v0.2.pdf: earlier performance follow-up.
- publication/: submission receipt, original release hashes and provenance.

To regenerate the latest performance note, install Python ReportLab and DejaVu
fonts, then run `python3 scripts/build_revision3_note.py` from project/. It writes
../deliverables/caprice-performance-note.pdf and refreshes project/README.md.
To rebuild the original manuscript, use original-v0.1.0/scripts/build_paper.sh;
that optional workflow needs pandoc, pdflatex, matplotlib and numpy.

## Verify or repackage the handoff

From the top-level caprice-local-handoff/ directory:

```sh
python3 tools/verify_handoff.py
python3 tools/pack_handoff.py /absolute/path/caprice-local-handoff-updated.zip
```

The verifier checks every file against HANDOFF_MANIFEST.json. Once you edit
files, that manifest describes the original handoff and will report differences.
The packer writes a new archive with an updated manifest, omitting build caches.

The archive includes upstream dependency licenses, complete old submissions,
and historical measurements. Temporary compiler output, account credentials,
toolchain binaries and this session's helper machinery are not required to
continue the project and are not part of the package.

## 6. GitHub

Use this directory as the git repository root. GitHub reads
`.github/workflows/` and `.github/dependabot.yml` from that root.
`rust-toolchain.toml` pins Rust 1.99.1.

Pushes to `main` or `master`, and pull requests, run Linux CI: offline
`--frozen` release tests and Clippy, using the vendored crates and bundled
PCRE2 10.49. A tag `v*` runs the same checks, then builds the binaries and
publishes `caprice-<version>-<target>.tar.gz` on the GitHub release. The
version in that name comes from `project/Cargo.toml`. The archive contains
`caprice`, `benchmark`, `allocations`, `compare`, `dump_ir`, and the PCRE2
8-bit library those comparison tools load. A manual run of the
Release workflow uploads that archive as an Actions artifact without creating
a release.

Dependabot opens weekly grouped updates for GitHub Actions and for
`project/Cargo.toml`. It does not update `baseline-v0.1.0`, `baseline-v0.2.0`,
or `original-v0.1.0`. After a Cargo update, refresh and commit the vendor
tree before merging:

```sh
bash project/scripts/refresh_vendor.sh
```
