# Bundled third-party source

project/vendor contains the exact dependency source chosen by the lockfiles,
produced with Cargo vendor. Upstream license notices are retained inside each
crate directory; rust-dependencies.json indexes package versions and declared
license identifiers. Keep these notices when using or redistributing that source.

pcre2/pcre2-10.49.tar.bz2 is the upstream PCRE2 release source, including its
license and notices. Source URL:
https://github.com/PCRE2Project/pcre2/releases/download/pcre2-10.49/pcre2-10.49.tar.bz2

SHA-256: 53c156e1ba416a20da8e65395daa132da0d80e76910424caca3fcdae7831d384

Rust and Lean toolchain binaries, C/system tools and optional Python document
packages are installed separately. Their source dependencies are not required
for the included Rust code once a compiler/toolchain is installed.
