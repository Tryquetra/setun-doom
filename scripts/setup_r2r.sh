#!/usr/bin/env bash
# Prepares the vendored R2R reference tool for the REBEL-6 Doom pipeline.
#
# The vendor directory is gitignored, so a fresh worktree does not contain it. This script
# copies the pristine upstream checkout from the main worktree (or $R2R_SOURCE), applies the
# small compatibility patch in scripts/r2r.patch, and builds the translator.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENDOR="$ROOT/vendor/RV32IToREBEL"
SOURCE="${R2R_SOURCE:-$ROOT/../setun-doom/vendor/RV32IToREBEL}"

if [[ ! -d "$VENDOR" ]]; then
    if [[ ! -d "$SOURCE" ]]; then
        echo "R2R source not found at $SOURCE; set R2R_SOURCE to a pristine checkout" >&2
        exit 1
    fi
    cp -r "$SOURCE" "$VENDOR"
fi

if ! grep -q "find_last_of" "$VENDOR/code/Assemblers/REBEL6/executableprogram.cpp"; then
    patch -d "$ROOT" -p1 < "$ROOT/scripts/r2r.patch"
fi

cmake -S "$VENDOR/code" -B "$VENDOR/build" \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_CXX_FLAGS="-include cstdint -include algorithm"
cmake --build "$VENDOR/build" -j"$(nproc)"
