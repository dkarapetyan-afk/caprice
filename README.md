# Caprice

Caprice is a whole-subject byte matcher. Patterns can name captures, use
backreferences, interpolate with `${...}`, and guard matches with `(?{...})`.
Version 0.3.0 compiles those patterns to a native backend with Cranelift
0.125.3. Partial evaluation and an interpreter remain available.

The current engine is in `project/`. Measured results and the language scope
are in `project/README.md` and `project/caprice-performance-note.pdf`.

## Layout

- `project/` is the v0.3.0 engine, tests, benchmarks, and vendored crates.
- `project/baseline-v0.1.0/` and `project/baseline-v0.2.0/` are retained
  comparison sources. v0.2 is built with the current crate as a dev-dependency.
- `original-v0.1.0/` is the original source tree.
- `papers/` holds the manuscript and performance notes.
- `third_party/` holds the PCRE2 10.49 source tarball and the dependency index.

Builds are offline. `project/vendor/` is the Cargo vendor tree, and
`.cargo/config.toml` points crates.io at it.

## Build and test

The checked `rust-toolchain.toml` selects Rust 1.99.1. The host is Linux
x86-64. Install a C toolchain, pkg-config, Python 3, Perl, tar, and bzip2,
then from `project/`:

```sh
bash scripts/build_pcre2_local.sh
bash scripts/cargo_local.sh test --release --frozen
bash scripts/cargo_local.sh clippy --release --frozen --all-targets -- -D warnings
bash scripts/cargo_local.sh run --release --frozen --bin caprice -- jit '(?<x>[a-z]+)-\k<x>' 'abc-abc'
```

`build_pcre2_local.sh` checks the bundled PCRE2 10.49 archive and installs it
under `.build/pcre2/`. `cargo_local.sh` links and loads that library.
`--frozen` keeps the build on `Cargo.lock` and the vendor tree.

The `caprice` binary takes `raw`, `pe`, or `jit`, then a pattern and an input:

```text
caprice jit '(?<x>[a-z]+)-\k<x>' abc-abc
```

## GitHub

Pushes to `main` and pull requests run the Linux test and Clippy workflow.
A tag such as `v0.3.0` builds the release binaries and publishes
`caprice-<version>-<target>.tar.gz`. The version in that name comes from
`project/Cargo.toml`. The archive contains `caprice`, `benchmark`,
`allocations`, `compare`, `dump_ir`, and the PCRE2 8-bit library those
comparison programs load.

Dependabot opens grouped updates for GitHub Actions and for
`project/Cargo.toml`. It does not update the baseline crates. After a Cargo
update, refresh the vendor tree and commit it:

```sh
bash project/scripts/refresh_vendor.sh
```

## License

Caprice is dual-licensed under either of

* the Apache License, Version 2.0, in [LICENSE-APACHE](LICENSE-APACHE)
* the MIT license in [LICENSE-MIT](LICENSE-MIT)

at your option.

`project/vendor/` and `third_party/pcre2/` keep the upstream notices shipped
with those sources. This dual license does not replace those notices.
