// Copyright (c) 2026 Robson Cassiano (https://eu.robsoncassiano.software)
// SPDX-License-Identifier: MIT

//! Doom platform layer for the RISC-V guest: the five `DG_*` hooks, the WAD file backend and
//! `main`. Everything crosses to the host through the `ecall` services in `setun_syscall.h`.

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "doomgeneric.h"
#include "doomtype.h"
#include "i_video.h"
#include "w_file.h"

#include "setun_syscall.h"

/* Palette state lives in i_video.c under CMAP256. */
extern struct color colors[256];
extern boolean palette_changed;

void DG_Init(void)
{
}

void DG_DrawFrame(void)
{
    if (palette_changed) {
        uint8_t rgb[768];
        for (int i = 0; i < 256; i++) {
            rgb[i * 3 + 0] = (uint8_t)colors[i].r;
            rgb[i * 3 + 1] = (uint8_t)colors[i].g;
            rgb[i * 3 + 2] = (uint8_t)colors[i].b;
        }
        setun_palette(rgb);
        palette_changed = 0;
    }
    setun_blit((const uint8_t *)DG_ScreenBuffer);
}

void DG_SleepMs(uint32_t ms)
{
    setun_sleep_ms((int32_t)ms);
}

uint32_t DG_GetTicksMs(void)
{
    return setun_ticks_ms();
}

int DG_GetKey(int *pressed, unsigned char *key)
{
    return setun_getkey(pressed, key);
}

void DG_SetWindowTitle(const char *title)
{
    (void)title;
}

/* ------------------------------------------------------------------------- */
/* WAD backend: the host owns the file, the guest reads by offset.           */
/* ------------------------------------------------------------------------- */

static wad_file_t host_wad_file;

/* Defined below; the engine dispatches W_Read/W_CloseFile through this class object. */
wad_file_class_t stdc_wad_file;

static wad_file_t *W_Host_OpenFile(char *path)
{
    (void)path;
#ifdef SETUN_RV32_DEMO
    /* The guest WAD backend always opens the mounted IWAD. Demo playback passes
     * `-playdemo demo1`, and `D_AddFile` first tries an external `demo1.lmp`.
     * Rejecting that path makes the engine fall back to the DEMO1 lump inside the
     * WAD, which is the only file the guest can read. */
    if (strstr(path, ".lmp") != NULL) {
        return NULL;
    }
#endif
    if (setun_wad_open() < 0) {
        return NULL;
    }
    host_wad_file.file_class = &stdc_wad_file;
    host_wad_file.mapped = NULL;
    host_wad_file.length = (unsigned int)setun_wad_size();
    return &host_wad_file;
}

static void W_Host_CloseFile(wad_file_t *wad)
{
    (void)wad;
}

static size_t W_Host_Read(wad_file_t *wad, unsigned int offset, void *buffer, size_t buffer_len)
{
    (void)wad;
    setun_wad_seek((int32_t)offset);
    return (size_t)setun_wad_read(buffer, (int32_t)buffer_len);
}

/* `w_file.c` dispatches through this class object; `w_file_stdc.c` is not built. */
wad_file_class_t stdc_wad_file = {
    W_Host_OpenFile,
    W_Host_CloseFile,
    W_Host_Read,
};

/* ------------------------------------------------------------------------- */
/* Entry point                                                               */
/* ------------------------------------------------------------------------- */

static char arg_doom[] = "doom";
static char arg_mb[] = "-mb";
static char arg_mb_value[] = "3";
static char arg_iwad[] = "-iwad";
static char arg_iwad_value[] = "doom1.wad";
static char arg_nosound[] = "-nosound";
static char arg_warp[] = "-warp";
static char arg_episode[] = "1";
static char arg_map[] = "1";
static char arg_skill[] = "-skill";
static char arg_skill_value[] = "3";
#ifdef SETUN_RV32_DEMO
static char arg_playdemo[] = "-playdemo";
static char arg_demo1[] = "demo1";
#endif

int main(void)
{
    char *argv[] = {
        arg_doom, arg_mb, arg_mb_value, arg_iwad, arg_iwad_value, arg_nosound,
        arg_warp, arg_episode, arg_map, arg_skill, arg_skill_value,
#ifdef SETUN_RV32_DEMO
        arg_playdemo, arg_demo1,
#endif
        NULL,
    };
    doomgeneric_Create((int)(sizeof(argv) / sizeof(argv[0])) - 1, argv);

    /* `D_DoomMain` returns after one tick; the platform owns the outer loop, as in the
     * native driver. */
    for (;;) {
        doomgeneric_Tick();
    }
}
