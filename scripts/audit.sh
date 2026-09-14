#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
# SQLx's optional MySQL dependency is present in Cargo.lock but is not enabled.
# Fail closed if a feature change ever makes RSA part of any configured build.
# Review rationale and upstream warnings in docs/DEPENDENCIES.md.
rsa_tree="$(cargo tree --workspace --target all --all-features --locked -i rsa)"
if [[ -n "$rsa_tree" ]]; then
  echo 'RSA is now in the active dependency graph. Review RUSTSEC-2023-0071 before release.' >&2
  exit 1
fi
AUDIT_BIN="${AUDIT_BIN:-cargo-audit}"
"$AUDIT_BIN" audit --ignore RUSTSEC-2023-0071
