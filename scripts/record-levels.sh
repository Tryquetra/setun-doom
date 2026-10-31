#!/usr/bin/env bash
# Records the Setun-Doom level videos on Hyprland.
#
# Each scenario opens a terminal HUD and the emulator window side by side on the
# external monitor (DP-2, scale 1), records a 1920x1080 region with
# gpu-screen-recorder, then trims the result to 60 seconds with ffmpeg.
#
# Usage:
#   ./scripts/record-levels.sh [all|level0|level1-fast|level1-veracity|level2|level3]
#
# Environment:
#   OUT             output directory (default: <repo>/recordings)
#   RECORD_SECONDS  raw capture length per scenario (default: 62)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${OUT:-$ROOT/recordings}"
RAW="$OUT/raw"
FINAL="$OUT/final"
LOGS="$OUT/logs"
WORK="$OUT/work"
RECORD_SECONDS="${RECORD_SECONDS:-62}"

mkdir -p "$RAW" "$FINAL" "$LOGS" "$WORK"
EMPTY_COMPOSE="$WORK/empty.XCompose"
printf '# recording compose table\n' >"$EMPTY_COMPOSE"

REC_PID=""
ORIGINAL_WORKSPACE="$(hyprctl activeworkspace -j | jq -r '.name')"
RECORDING_WORKSPACE="special:setun-rec"

focus_workspace() {
    hyprctl dispatch "hl.dsp.focus({ workspace = \"$1\" })" >/dev/null
    sleep 0.5
}

cleanup_processes() {
    if [[ -n "$REC_PID" ]] && kill -0 "$REC_PID" 2>/dev/null; then
        kill -INT "$REC_PID" 2>/dev/null || true
        wait "$REC_PID" 2>/dev/null || true
    fi
    REC_PID=""
    pkill -TERM -f '^foot -a setun-hud' 2>/dev/null || true
    pkill -TERM -f 'doomgeneric_setun -iwad' 2>/dev/null || true
    pkill -TERM -f 'target/release/setun-doom-rv32' 2>/dev/null || true
    pkill -TERM -f 'target/release/setun_dos' 2>/dev/null || true
    pkill -TERM -f 'target/release/setun-run' 2>/dev/null || true
    pkill -TERM -f 'Setun Backdrop' 2>/dev/null || true
    sleep 1
    focus_workspace "$ORIGINAL_WORKSPACE"
}

trap cleanup_processes EXIT

window_address() {
    local class="$1"
    local tries="${2:-150}"
    local addr=""
    for _ in $(seq 1 "$tries"); do
        addr=$(hyprctl clients -j | jq -r --arg c "$class" \
            '.[] | select(.class == $c) | .address' | head -n1)
        if [[ -n "$addr" && "$addr" != "null" ]]; then
            printf '%s\n' "$addr"
            return 0
        fi
        sleep 0.1
    done
    return 1
}

place_window() {
    local class="$1" x="$2" y="$3" w="$4" h="$5" addr
    if ! addr=$(window_address "$class"); then
        echo "[record] window class '$class' not found" >&2
        return 1
    fi
    hyprctl dispatch "hl.dsp.window.float({ action = \"on\", window = \"address:$addr\" })" >/dev/null
    hyprctl dispatch "hl.dsp.window.resize({ x = $w, y = $h, relative = false, window = \"address:$addr\" })" >/dev/null
    hyprctl dispatch "hl.dsp.window.move({ x = $x, y = $y, relative = false, window = \"address:$addr\" })" >/dev/null
    sleep 0.3
}

launch_backdrop() {
    mpv --no-config --no-border --no-osc --no-input-default-bindings \
        --force-window=yes --idle=yes --background-color='#000000' \
        --title='Setun Backdrop' --really-quiet \
        >"$LOGS/backdrop.log" 2>&1 &
    sleep 0.5
    place_window mpv 0 -1440 1920 1080
}

launch_hud() {
    local name="$1" command="$2" width="$3" height="$4" fold_width="${5:-110}" font_size="${6:-16}"
    XCOMPOSEFILE="$EMPTY_COMPOSE" foot -a setun-hud -T "Setun-Doom $name" -w "${width}x${height}" \
        -o "font=monospace:size=$font_size" \
        -e bash -lc "cd '$ROOT'; clear; $command 2>&1 | sed -u -E 's/={10,}/====================/' | stdbuf -oL fold -s -w '$fold_width' | tee '$LOGS/$name.log'" \
        >"$LOGS/$name.foot.log" 2>&1 &
    place_window setun-hud 0 -1440 "$width" "$height"
}

start_recording() {
    local region="$1" output="$2" log="$3"
    gpu-screen-recorder -w "$region" -f 60 -k h264 -c mp4 -cursor no -q very_high \
        -o "$output" >"$log" 2>&1 &
    REC_PID=$!
    sleep 1
    if ! kill -0 "$REC_PID" 2>/dev/null; then
        echo "[record] gpu-screen-recorder failed, see $log" >&2
        return 1
    fi
}

stop_recording() {
    if [[ -z "$REC_PID" ]]; then
        return 0
    fi
    kill -INT "$REC_PID" 2>/dev/null || true
    wait "$REC_PID" 2>/dev/null || true
    REC_PID=""
}

trim_video() {
    local raw="$1" final="$2" name
    name="$(basename "${final%.mp4}")"
    ffmpeg -y -hide_banner -loglevel error -ss 2 -t 60 -i "$raw" -frames:v 3600 \
        -r 60 -c:v libx264 -preset slow -crf 18 -pix_fmt yuv420p \
        -movflags +faststart "$final"
    ffprobe -v error -show_entries stream=width,height,r_frame_rate \
        -show_entries format=duration -of default=nw=1 "$final" \
        | tee "$LOGS/$name.ffprobe.txt"
}

start_scene() {
    local name="$1" region="$2"
    start_recording "$region" "$RAW/$name.mp4" "$LOGS/$name.gsr.log"
}

finish_scene() {
    local name="$1" seconds="$2"
    sleep "$seconds"
    stop_recording
    trim_video "$RAW/$name.mp4" "$FINAL/$name.mp4"
}

build_level2() {
    ./target/release/setun-ld examples/level2_doom_core.c src/runtime/minilibc.c \
        vendor/doomgeneric/doomgeneric/m_fixed.c \
        vendor/doomgeneric/doomgeneric/m_bbox.c \
        vendor/doomgeneric/doomgeneric/m_random.c \
        -Isrc/runtime -Ivendor/doomgeneric/doomgeneric \
        -o "$WORK/level2.setun"
}

scenario_level0() {
    start_scene level0-fidelity 1920x1080+0+-1440
    launch_hud level0-fidelity \
        "SETUN_DOOM_MAX_FRAMES=2000 ./vendor/doomgeneric/doomgeneric/doomgeneric_setun \
         -iwad vendor/DOOM1.WAD -playdemo demo1" 960 1080 74
    place_window doomgeneric_setun 960 -1200 960 600
    finish_scene level0-fidelity "$RECORD_SECONDS"
}

scenario_level1_fast() {
    start_scene level1-fast 1920x1080+0+-1440
    launch_hud level1-fast \
        "SETUN_RV32_ELF=vendor/doomgeneric/riscv32/doom_rv32_demo.elf \
         SETUN_RV32_MAX_FRAMES=4500 ./target/release/setun-doom-rv32" 960 1080 74
    place_window setun-doom-rv32 960 -1200 960 600
    finish_scene level1-fast "$RECORD_SECONDS"
}

scenario_level1_veracity() {
    start_scene level1-veracity 1920x1080+0+-1440
    launch_hud level1-veracity \
        "SETUN_RV32_ELF=vendor/doomgeneric/riscv32/doom_rv32_demo.elf \
         SETUN_RV32_VERACITY=1 SETUN_RV32_MAX_FRAMES=55 \
         ./target/release/setun-doom-rv32" 960 1080 74
    place_window setun-doom-rv32 960 -1200 960 600
    finish_scene level1-veracity 65
}

scenario_level2() {
    build_level2 >"$LOGS/level2-fidelity.build.log" 2>&1
    start_scene level2-fidelity 1920x1080+0+-1440
    launch_hud level2-fidelity \
        "./target/release/setun-run '$WORK/level2.setun' --trace \
         --trace-steps 1200 --trace-delay-ms 55 --max-steps 2000000" 1920 1080 150 18
    finish_scene level2-fidelity "$RECORD_SECONDS"
}

scenario_level3() {
    start_scene level3-dos-doom 1920x1080+0+-1440
    launch_hud level3-dos-doom \
        "./target/release/setun_dos --exec doom --frames 2350" 640 1080 52 14
    place_window setun_dos 640 -1280 1280 800
    finish_scene level3-dos-doom "$RECORD_SECONDS"
}

run_scenario() {
    local name="$1"
    focus_workspace "$RECORDING_WORKSPACE"
    launch_backdrop
    case "$name" in
        level0|level0-fidelity) scenario_level0 ;;
        level1-fast) scenario_level1_fast ;;
        level1-veracity) scenario_level1_veracity ;;
        level2|level2-fidelity) scenario_level2 ;;
        level3|level3-dos-doom) scenario_level3 ;;
        *)
            echo "[record] unknown scenario '$name'" >&2
            exit 2
            ;;
    esac
    cleanup_processes
}

case "${1:-all}" in
    all)
        run_scenario level0
        run_scenario level1-fast
        run_scenario level1-veracity
        run_scenario level2
        run_scenario level3
        ;;
    list)
        printf '%s\n' level0-fidelity level1-fast level1-veracity level2-fidelity level3-dos-doom
        ;;
    *)
        run_scenario "$1"
        ;;
esac
