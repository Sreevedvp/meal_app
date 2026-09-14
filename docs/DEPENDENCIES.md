# Dependency review — 14 September 2026

`cargo-audit` 0.22.2 checked 682 lockfile dependencies against RustSec commit `455fd4bac659b5f1fca3810661c2d8b3c25dad05`. The unfiltered audit reports one vulnerability plus five warnings. `npm` reported zero known vulnerabilities in the locked browser-test dependencies. This is a point-in-time advisory check, not a security certification.

## Conditional exception

**RUSTSEC-2023-0071 (`rsa` 0.9.10):** this unpatched RSA timing advisory is in the lockfile through SQLx's optional MySQL dependency. This application enables only PostgreSQL. `cargo tree --workspace --target all --all-features --locked -i rsa` produces no active dependency tree. No application code performs RSA operations through this crate.

`scripts/audit.sh` checks that fact before allowing this single advisory ID during the audit. If a future feature change activates RSA on any configured target, the script fails and the exception must be reviewed. No general severity threshold or blanket advisory suppression is configured. Run the raw `cargo audit` when reviewing lockfile changes so the exception remains visible.

## Upstream warnings

- `glib` 0.18.5: **RUSTSEC-2024-0429**, unsound string iteration in the Linux desktop GTK dependency. The deployed server and WebAssembly client do not depend on GTK. A Linux desktop build is not a supported release target and needs an upstream upgrade before distribution.
- `rand` 0.7.3: **RUSTSEC-2026-0097**, unsoundness under specific custom-logger/thread-RNG conditions. It is pulled through `phf_generator` → `phf_codegen` → the native WebView parser as a build dependency. It is absent from the web client graph; no application custom RNG logger is configured. Review again before a native release.
- `fxhash`, `paste`, `proc-macro-error`: **RUSTSEC-2025-0057**, **RUSTSEC-2024-0436**, **RUSTSEC-2024-0370** mark upstream crates unmaintained. These are dependency-maintenance risks; do not mistake a successful vulnerability gate for an absence of warnings. Prefer supported upstream releases over local forks merely to silence the report.

The workflow runs this gate on pushes, pull requests and weekly. Dependabot proposes Cargo, npm, Actions and container updates; review and retest them. Container image vulnerability scanning and real OIDC provider verification remain deployment responsibilities.
