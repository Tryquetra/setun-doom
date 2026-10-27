#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
KERNEL_BIN="$REPO_ROOT/target/riscv64gc-unknown-none-elf/release/setun-kernel"

cd "$REPO_ROOT"

echo "Compilando Setun-DOS RISC-V Bare-Metal Unikernel..."
cargo build --target riscv64gc-unknown-none-elf --release -p setun-kernel

if ! command -v qemu-system-riscv64 &> /dev/null; then
    echo "============================================================"
    echo "  qemu-system-riscv64 não encontrado no PATH."
    echo "  Para instalar os pacotes oficiais no Arch Linux:"
    echo "    sudo pacman -S qemu-system-riscv qemu-system-riscv-firmware"
    echo "============================================================"
    exit 1
fi

DISPLAY_OPT="${DISPLAY_OPT:--display sdl,show-cursor=on}"
if [ -z "${DISPLAY:-}" ] && [ -z "${WAYLAND_DISPLAY:-}" ]; then
    DISPLAY_OPT="-nographic"
fi

echo "============================================================"
echo " Executando Setun-DOS RISC-V Unikernel no QEMU (virt)..."
echo " Opção de exibição: $DISPLAY_OPT"
echo " Soltar ponteiro do mouse: Pressione Ctrl + Alt + G"
echo "============================================================"
qemu-system-riscv64 \
    -machine virt \
    -cpu rv64 \
    -m 512M \
    -device VGA,vgamem_mb=16 \
    -device virtio-tablet-pci \
    -serial stdio \
    $DISPLAY_OPT \
    -bios default \
    -kernel "$KERNEL_BIN"
