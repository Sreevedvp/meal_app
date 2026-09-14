#!/usr/bin/env bash
set -euo pipefail
case "$(uname -s)-$(uname -m)" in
  Linux-x86_64) archive=dx-x86_64-unknown-linux-gnu; digest=4363e4ed2a3f1eb7f4d38d2d59aed59ce43271c44c16b425e92c89a64761fbe7 ;;
  Linux-aarch64) archive=dx-aarch64-unknown-linux-gnu; digest=8f1a17d3218700ffbe15e6540d936a178b2556fc801121a31082e3ba4ab9ef55 ;;
  Darwin-arm64) archive=dx-aarch64-apple-darwin; digest=342c92bc33dca60dadd725ed5e249923f049ae2e1e50b7477f4968d56695b425 ;;
  *) echo 'Unsupported platform for the pinned CLI installer.' >&2; exit 1 ;;
esac
install_dir="${1:-work/tools}"
mkdir -p "$install_dir"
cli_tmp="$(mktemp -d)"
trap 'rm -rf "$cli_tmp"' EXIT
curl --fail --location --retry 3 "https://github.com/DioxusLabs/dioxus/releases/download/v0.7.10/$archive.tar.gz" -o "$cli_tmp/dx.tar.gz"
(cd "$cli_tmp" && echo "$digest  dx.tar.gz" | shasum -a 256 -c -)
tar -xzf "$cli_tmp/dx.tar.gz" -C "$cli_tmp"
install -m 755 "$cli_tmp/dx" "$install_dir/dx"
