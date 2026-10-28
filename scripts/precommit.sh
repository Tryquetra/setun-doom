#!/usr/bin/env bash
# Formatting, lint and test gate. Run before committing.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

echo "== cargo fmt --check =="
cargo fmt --all --check

echo "== cargo clippy --all-targets =="
cargo clippy --all-targets -- -D warnings

echo "== cargo test --release =="
cargo test --release
