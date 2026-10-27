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

DISPLAY_OPT="${DISPLAY_OPT:--display gtk,grab-on-hover=off,show-cursor=on,window-close=on}"
if [ -z "${DISPLAY:-}" ] && [ -z "${WAYLAND_DISPLAY:-}" ]; then
    DISPLAY_OPT="-nographic"
fi

echo "============================================================"
echo " Executando Setun-DOS RISC-V Unikernel no QEMU (virt)..."
echo " Opção de exibição: $DISPLAY_OPT"
echo " Janela [X]: Fechamento habilitado (window-close=on ou Ctrl+Alt+Q)"
echo " Mouse/Tablet: Cursor livre sem aprisionamento (grab-on-hover=off)"
echo " Navegação: Mouse/Tablet, ou teclas W/A/S/D + Espaço"
echo " Atalhos: '1'=DOOM, '2'=SHELL, '3'=TELEMETRY, '4'=ABOUT, 'q'=Sair"
echo "============================================================"
qemu-system-riscv64 \
    -machine virt \
    -cpu rv64 \
    -m 512M \
    -device VGA,vgamem_mb=16 \
    -device virtio-tablet-device \
    -serial stdio \
    $DISPLAY_OPT \
    -bios default \
    -kernel "$KERNEL_BIN"
