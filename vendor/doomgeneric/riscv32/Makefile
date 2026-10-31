# Copyright (c) 2026 Robson Cassiano (https://eu.robsoncassiano.software)
# SPDX-License-Identifier: MIT
#
# Cross-compiles doomgeneric to bare-metal RV32IM for the Setun-Doom DBT.
# The guest talks to the host through `ecall` services (see src/setun_syscall.h).

ENGINE  := ../doomgeneric
BUILD   := build
OUTPUT  := doom_rv32.elf

CC      := clang
TARGET  := --target=riscv32-unknown-elf
EXTRA_CFLAGS ?=
CFLAGS  := $(TARGET) -march=rv32im -mabi=ilp32 -O2 -ffreestanding -fno-common -fno-stack-protector \
           -Wall -Wextra -Wno-unused-parameter -Wno-sign-compare \
           -I$(ENGINE) -isystem include \
           -DSETUN_RV32_GUEST -DCMAP256 -DDOOMGENERIC_RESX=320 -DDOOMGENERIC_RESY=200 -DNORMALUNIX -D_DEFAULT_SOURCE \
           $(EXTRA_CFLAGS)
LDFLAGS := -flavor gnu -T link.ld --gc-sections -static -e _start

LD      := $(shell ls $(HOME)/.rustup/toolchains/*/lib/rustlib/x86_64-unknown-linux-gnu/bin/rust-lld 2>/dev/null | head -1)

ENGINE_SRC := $(filter-out doomgeneric_%.c w_file_stdc.c i_cdmus.c i_joystick.c \
               i_sdlmusic.c i_sdlsound.c i_allegrosound.c i_allegromusic.c \
               i_sound.c,$(notdir $(wildcard $(ENGINE)/*.c)))
PORT_SRC   := src/libc.c src/platform.c src/stubs.c src/softfloat.c
ASM_SRC    := src/start.S

OBJS := $(addprefix $(BUILD)/,$(ENGINE_SRC:.c=.o)) $(addprefix $(BUILD)/,$(notdir $(PORT_SRC:.c=.o))) $(BUILD)/start.o

all: $(OUTPUT)

# Demo playback build: same engine, extra `-playdemo demo1` argument and a
# separate object directory so the normal playable ELF stays untouched.
demo:
	$(MAKE) BUILD=build_demo OUTPUT=doom_rv32_demo.elf EXTRA_CFLAGS=-DSETUN_RV32_DEMO

clean-demo:
	rm -rf build_demo
	rm -f doom_rv32_demo.elf

$(BUILD):
	mkdir -p $(BUILD)

$(BUILD)/%.o: $(ENGINE)/%.c | $(BUILD)
	$(CC) $(CFLAGS) -c $< -o $@

$(BUILD)/%.o: src/%.c | $(BUILD)
	$(CC) $(CFLAGS) -c $< -o $@

$(BUILD)/start.o: src/start.S | $(BUILD)
	$(CC) $(CFLAGS) -c $< -o $@

$(OUTPUT): $(OBJS) link.ld
	$(LD) $(LDFLAGS) -o $@ $(OBJS)

clean:
	rm -rf $(BUILD) $(OUTPUT)

.PHONY: all clean
