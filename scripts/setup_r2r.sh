#!/usr/bin/env bash
# Prepares the vendored R2R reference tool for the REBEL-6 Doom pipeline.
#
# vendor/RV32IToREBEL is gitignored, so a fresh worktree has no translator. This script
# clones the Tryquetra fork and checks out setun-doom-compat, which carries upstream
# Soppe/RV32IToREBEL plus the four compatibility changes the Doom build needs (symbol+addend
# operands, unquoted .string parameters, descriptive symbol errors, REBEL-6 only backend).
# Override the remote with R2R_REPO and the branch with R2R_REF.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENDOR="$ROOT/vendor/RV32IToREBEL"
# The fork is the only R2R source. SSH keeps the checkout push-capable; set R2R_REPO
# to the HTTPS URL for an anonymous clone.
REPO="${R2R_REPO:-git@github.com:Tryquetra/RV32IToREBEL.git}"
REF="${R2R_REF:-setun-doom-compat}"

if [[ ! -d "$VENDOR/.git" ]]; then
    if [[ -e "$VENDOR" ]]; then
        echo "$VENDOR exists and is not a git checkout; remove it and rerun" >&2
        exit 1
    fi
    git clone --quiet --no-checkout --origin setun "$REPO" "$VENDOR"
fi

if ! git -C "$VENDOR" remote get-url setun >/dev/null 2>&1; then
    git -C "$VENDOR" remote add setun "$REPO"
fi
git -C "$VENDOR" fetch --quiet setun "+refs/heads/$REF:refs/remotes/setun/$REF"
if git -C "$VENDOR" rev-parse --verify --quiet "refs/heads/$REF" >/dev/null; then
    git -C "$VENDOR" checkout --quiet "$REF"
    git -C "$VENDOR" merge --ff-only --quiet "setun/$REF"
else
    git -C "$VENDOR" checkout --quiet --track -b "$REF" "setun/$REF"
fi

cmake -S "$VENDOR/code" -B "$VENDOR/build" \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_CXX_FLAGS="-include cstdint -include algorithm"
cmake --build "$VENDOR/build" -j"$(nproc)"
