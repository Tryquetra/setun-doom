// ============================================================================
// Copyright (c) 2026 Robson Cassiano (https://eu.robsoncassiano.software)
//
// Driver de Plataforma `doomgeneric_setun.c`
// Conecta o motor C do Doom (1993) à arquitetura ternária balanceada Setun-Doom
// (`libsetun_doom.a` em Rust) e exibe em janela SDL2 interativa (ou headless).
// Distribuído sob a licença MIT / GPL-2.0-or-later para compatibilidade com doomgeneric.
// ============================================================================

#include "doomkeys.h"
#include "doomgeneric.h"
#include "i_video.h"

#include <ctype.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include <SDL.h>

typedef struct {
    int64_t  acc_val;
    int64_t  r_val;
    int64_t  pc;
    uint64_t cycles;
    uint64_t adder_carry_propagations;
    uint64_t ternary_trit_flips;
    uint64_t ternary_half_swings;
    uint64_t ternary_full_swings;
    uint64_t binary32_bit_flips;
    float    digit_reduction_pct;
    float    energy_reduction_pct;
    int8_t   acc_trits[21];
} SetunLiveTelemetrySnapshot;

// Funções exportadas pela nossa biblioteca Rust `libsetun_doom.a` (src/mmo.rs)
extern void setun_BlitVideoBuffer(const uint8_t* video_buffer_320x200, uint8_t* out_rgba_320x200);
extern void setun_PrintTelemetry(uint32_t frame_num);
extern void setun_PushKeyEvent(int32_t pressed, uint8_t doom_key);
extern void setun_GetLiveTelemetrySnapshot(SetunLiveTelemetrySnapshot* out_snap);

static uint32_t     s_FrameCount = 0;
static uint32_t     s_MaxFrames  = 0; // 0 = ilimitado
static int          s_Headless   = 0;
static uint8_t      s_SetunRgba320x200[320 * 200 * 4];

static SDL_Window*   s_Window   = NULL;
static SDL_Renderer* s_Renderer = NULL;
static SDL_Texture*  s_Texture  = NULL;

// Salva o quadro atual renderizado pelo Framebuffer Ternário Setun (Tryte6 -> RGBA) em arquivo PPM
static void save_ternary_frame_ppm(const char* path, const uint8_t* rgba)
{
    FILE* f = fopen(path, "wb");
    if (!f) return;
    fprintf(f, "P6\n320 200\n255\n");
    for (int i = 0; i < 320 * 200; ++i) {
        fputc(rgba[i * 4 + 0], f);
        fputc(rgba[i * 4 + 1], f);
        fputc(rgba[i * 4 + 2], f);
    }
    fclose(f);
}

static uint8_t convert_sdl_key_to_doom(SDL_Keycode key)
{
    switch (key) {
        case SDLK_RETURN:
        case SDLK_KP_ENTER: return KEY_ENTER;
        case SDLK_ESCAPE:   return KEY_ESCAPE;
        case SDLK_LEFT:
        case SDLK_a:        return KEY_LEFTARROW;
        case SDLK_RIGHT:
        case SDLK_d:        return KEY_RIGHTARROW;
        case SDLK_UP:
        case SDLK_w:        return KEY_UPARROW;
        case SDLK_DOWN:
        case SDLK_s:        return KEY_DOWNARROW;
        case SDLK_LCTRL:
        case SDLK_RCTRL:    return KEY_FIRE;
        case SDLK_SPACE:
        case SDLK_e:        return KEY_USE;
        case SDLK_LSHIFT:
        case SDLK_RSHIFT:   return KEY_RSHIFT;
        case SDLK_LALT:
        case SDLK_RALT:     return KEY_LALT;
        case SDLK_TAB:      return KEY_TAB;
        case SDLK_F1:       return KEY_F1;
        case SDLK_F2:       return KEY_F2;
        case SDLK_F3:       return KEY_F3;
        case SDLK_F4:       return KEY_F4;
        case SDLK_F5:       return KEY_F5;
        case SDLK_F6:       return KEY_F6;
        case SDLK_F7:       return KEY_F7;
        case SDLK_F8:       return KEY_F8;
        case SDLK_F9:       return KEY_F9;
        case SDLK_F10:      return KEY_F10;
        case SDLK_F11:      return KEY_F11;
        case SDLK_EQUALS:
        case SDLK_PLUS:     return KEY_EQUALS;
        case SDLK_MINUS:    return KEY_MINUS;
        default:
            if (key >= 0 && key < 128) {
                return (uint8_t)tolower((int)key);
            }
            return 0;
    }
}

// Fonte bitmap 3x5 compacta para renderização de rótulos de telemetria sem dependência de bibliotecas externas
static const uint16_t s_Font3x5[] = {
    [' '] = 0x0000,
    ['-'] = 0x01C0,
    ['+'] = 0x0BA0,
    ['%'] = 0x5555,
    ['.'] = 0x0002,
    [':'] = 0x0208,
    ['/'] = 0x1248,
    ['<'] = 0x1242,
    ['='] = 0x038E,
    ['>'] = 0x4212,
    ['0'] = 0x7B6F, ['1'] = 0x2492, ['2'] = 0x73E7, ['3'] = 0x73CF, ['4'] = 0x5BC9,
    ['5'] = 0x79CF, ['6'] = 0x79EF, ['7'] = 0x7249, ['8'] = 0x7BEF, ['9'] = 0x7BCF,
    ['A'] = 0x7BE9, ['B'] = 0x7B6E, ['C'] = 0x7927, ['D'] = 0x736E, ['E'] = 0x79E7,
    ['F'] = 0x79E1, ['G'] = 0x796F, ['H'] = 0x5BE9, ['I'] = 0x7497, ['J'] = 0x126E,
    ['K'] = 0x5AE9, ['L'] = 0x4927, ['M'] = 0x7FED, ['N'] = 0x7B6D, ['O'] = 0x7B6F,
    ['P'] = 0x7BE1, ['Q'] = 0x7B6B, ['R'] = 0x7BE5, ['S'] = 0x79CE, ['T'] = 0x7492,
    ['U'] = 0x5B6F, ['V'] = 0x5B6A, ['W'] = 0x5BF7, ['X'] = 0x5AAB, ['Y'] = 0x5AE2,
    ['Z'] = 0x7267,
};

static void draw_tiny_char(SDL_Renderer* renderer, int x, int y, char c)
{
    unsigned char uc = (unsigned char)c;
    if (uc >= 'a' && uc <= 'z') uc -= 32;
    if (uc >= sizeof(s_Font3x5)/sizeof(s_Font3x5[0])) return;
    uint16_t glyph = s_Font3x5[uc];
    if (glyph == 0 && c != ' ') return;

    for (int row = 0; row < 5; row++) {
        for (int col = 0; col < 3; col++) {
            int bit = 14 - (row * 3 + col);
            if ((glyph >> bit) & 1) {
                SDL_RenderDrawPoint(renderer, x + col, y + row);
            }
        }
    }
}

static void draw_tiny_text(SDL_Renderer* renderer, int x, int y, const char* str)
{
    while (*str) {
        draw_tiny_char(renderer, x, y, *str);
        x += 4;
        str++;
    }
}

static int s_ShowHud = 1;

// Desenha o painel HUD de telemetria ternária Setun sobre a janela gráfica SDL2
static void render_setun_hud(SDL_Renderer* renderer)
{
    SetunLiveTelemetrySnapshot snap;
    setun_GetLiveTelemetrySnapshot(&snap);

    SDL_SetRenderDrawBlendMode(renderer, SDL_BLENDMODE_BLEND);

    // 1. Fundo do painel do osciloscópio (estilo terminal industrial âmbar/cinza escuro)
    SDL_SetRenderDrawColor(renderer, 8, 12, 14, 230);
    SDL_Rect panel_rect = { 4, 3, 312, 42 };
    SDL_RenderFillRect(renderer, &panel_rect);

    // Borda do painel
    SDL_SetRenderDrawColor(renderer, 35, 75, 70, 255);
    SDL_RenderDrawRect(renderer, &panel_rect);

    // Rótulos de Seção em Inglês
    SDL_SetRenderDrawColor(renderer, 130, 160, 150, 255);
    draw_tiny_text(renderer, 8, 5, "SETUN ACCUMULATOR (21 TRITS)");
    draw_tiny_text(renderer, 145, 5, "ENERGY SAVE");
    draw_tiny_text(renderer, 245, 5, "BSP SIDE");
    draw_tiny_text(renderer, 8, 24, "ALU CARRY GATES PROPAGATION");

    // 2. Lâmpadas de Néon dos 21 Trits da Word21 (Acumulador Setun)
    int led_start_x = 8;
    int led_y = 12;
    int led_w = 4;
    int led_h = 7;
    int led_gap = 2;

    for (int i = 0; i < 21; i++) {
        int8_t trit = snap.acc_trits[20 - i];
        if (trit > 0) {
            SDL_SetRenderDrawColor(renderer, 30, 255, 120, 255); // Positive (+1): Bright Neon Green
        } else if (trit < 0) {
            SDL_SetRenderDrawColor(renderer, 255, 80, 40, 255);  // Negative (-1): Soviet Amber/Orange
        } else {
            SDL_SetRenderDrawColor(renderer, 45, 50, 55, 255);   // Zero (0): Dark Neutral
        }
        SDL_Rect led_rect = { led_start_x + i * (led_w + led_gap), led_y, led_w, led_h };
        SDL_RenderFillRect(renderer, &led_rect);
    }

    // 3. Barra de Economia de Energia Física (Delta V^2 Reduction %)
    float reduction = snap.energy_reduction_pct;
    if (reduction < 0.0f) reduction = 0.0f;
    if (reduction > 100.0f) reduction = 100.0f;

    int bar_x = 145;
    int bar_y = 12;
    int bar_max_w = 60;
    int bar_h = 7;
    int bar_w = (int)((reduction / 100.0f) * bar_max_w);

    SDL_SetRenderDrawColor(renderer, 25, 30, 35, 255);
    SDL_Rect bar_bg = { bar_x, bar_y, bar_max_w, bar_h };
    SDL_RenderFillRect(renderer, &bar_bg);

    SDL_SetRenderDrawColor(renderer, 0, 220, 255, 255);
    SDL_Rect bar_fill = { bar_x, bar_y, bar_w, bar_h };
    SDL_RenderFillRect(renderer, &bar_fill);

    SDL_SetRenderDrawColor(renderer, 60, 80, 90, 255);
    SDL_RenderDrawRect(renderer, &bar_bg);

    // Valor percentual textual (ex: "60%")
    char pct_buf[16];
    snprintf(pct_buf, sizeof(pct_buf), "%d%%", (int)reduction);
    SDL_SetRenderDrawColor(renderer, 0, 220, 255, 255);
    draw_tiny_text(renderer, bar_x + bar_max_w + 4, 13, pct_buf);

    // 4. Indicador de Carry Gates e Pulsos da ALU
    int pulse_x = 8;
    int pulse_y = 32;
    int pulse_count = (int)((snap.adder_carry_propagations / 1000) % 45);
    SDL_SetRenderDrawColor(renderer, 240, 190, 40, 255);
    for (int p = 0; p < pulse_count; p++) {
        SDL_Rect p_rect = { pulse_x + p * 4, pulse_y, 2, 6 };
        SDL_RenderFillRect(renderer, &p_rect);
    }

    // 5. Indicador Tricótomo BSP (< = >) com Rótulo
    int bsp_x = 285;
    int bsp_y = 11;
    if (snap.acc_val > 0) {
        SDL_SetRenderDrawColor(renderer, 40, 255, 120, 255);
        draw_tiny_text(renderer, bsp_x, bsp_y, "FRONT");
    } else if (snap.acc_val < 0) {
        SDL_SetRenderDrawColor(renderer, 255, 60, 40, 255);
        draw_tiny_text(renderer, bsp_x, bsp_y, "BACK");
    } else {
        SDL_SetRenderDrawColor(renderer, 255, 255, 255, 255);
        draw_tiny_text(renderer, bsp_x, bsp_y, "PLANE");
    }

    SDL_SetRenderDrawBlendMode(renderer, SDL_BLENDMODE_NONE);
}

static int poll_sdl_events(void)
{
    SDL_Event e;
    while (SDL_PollEvent(&e)) {
        if (e.type == SDL_QUIT) {
            return 0;
        }
        if (e.type == SDL_KEYDOWN) {
            if (e.key.keysym.sym == SDLK_F12) {
                s_ShowHud = !s_ShowHud;
                continue;
            }
        }
        if (e.type == SDL_KEYDOWN || e.type == SDL_KEYUP) {
            int pressed = (e.type == SDL_KEYDOWN) ? 1 : 0;
            uint8_t dkey = convert_sdl_key_to_doom(e.key.keysym.sym);
            if (dkey != 0) {
                setun_PushKeyEvent(pressed, dkey);
            }
        }
    }
    return 1;
}

int main(int argc, char **argv)
{
    const char* max_frames_env = getenv("SETUN_DOOM_MAX_FRAMES");
    if (max_frames_env) {
        s_MaxFrames = (uint32_t)atoi(max_frames_env);
    }
    const char* headless_env = getenv("SETUN_HEADLESS");
    if (headless_env && atoi(headless_env) != 0) {
        s_Headless = 1;
    }

    printf("================================================================\n");
    printf(" INICIANDO DOOM (1993) SOBRE EMULADOR TERNARIO SETUN-DOOM (21-TRIT)\n");
    printf("================================================================\n");

    if (!s_Headless) {
        if (SDL_Init(SDL_INIT_VIDEO) == 0) {
            s_Window = SDL_CreateWindow(
                "Setun-Doom (21-Trit Balanced Ternary ALU + Tryte6 Framebuffer)",
                SDL_WINDOWPOS_CENTERED,
                SDL_WINDOWPOS_CENTERED,
                960,
                600,
                SDL_WINDOW_SHOWN | SDL_WINDOW_RESIZABLE
            );
            if (s_Window) {
                s_Renderer = SDL_CreateRenderer(s_Window, -1, SDL_RENDERER_ACCELERATED | SDL_RENDERER_PRESENTVSYNC);
                if (!s_Renderer) {
                    s_Renderer = SDL_CreateRenderer(s_Window, -1, SDL_RENDERER_SOFTWARE);
                }
                if (s_Renderer) {
                    SDL_RenderSetLogicalSize(s_Renderer, 320, 200);
                    s_Texture = SDL_CreateTexture(
                        s_Renderer,
                        SDL_PIXELFORMAT_ABGR8888,
                        SDL_TEXTUREACCESS_STREAMING,
                        320,
                        200
                    );
                }
            }
        } else {
            fprintf(stderr, "[Aviso] SDL_Init falhou (%s); executando em modo headless.\n", SDL_GetError());
            s_Headless = 1;
        }
    }

    doomgeneric_Create(argc, argv);

    while (1)
    {
        if (!s_Headless) {
            if (!poll_sdl_events()) {
                break;
            }
        }

        doomgeneric_Tick();
        s_FrameCount++;

        if (I_VideoBuffer != NULL) {
            setun_BlitVideoBuffer((const uint8_t*)I_VideoBuffer, s_SetunRgba320x200);
            if (!s_Headless && s_Renderer && s_Texture) {
                SDL_UpdateTexture(s_Texture, NULL, s_SetunRgba320x200, 320 * 4);
                SDL_RenderClear(s_Renderer);
                SDL_RenderCopy(s_Renderer, s_Texture, NULL, NULL);
                if (s_ShowHud) {
                    render_setun_hud(s_Renderer);
                }
                SDL_RenderPresent(s_Renderer);
            }
        }

        if (s_FrameCount == 1 || (s_FrameCount % 35 == 0)) {
            setun_PrintTelemetry(s_FrameCount);
        }

        if (s_MaxFrames > 0 && s_FrameCount >= s_MaxFrames) {
            save_ternary_frame_ppm("/home/robson/coding/setun-doom/setun_doom_frame.ppm", s_SetunRgba320x200);
            printf("================================================================\n");
            printf(" Frame ternario 320x200 salvo em: setun_doom_frame.ppm\n");
            printf("================================================================\n");
            break;
        }
    }

    if (!s_Headless) {
        if (s_Texture)  SDL_DestroyTexture(s_Texture);
        if (s_Renderer) SDL_DestroyRenderer(s_Renderer);
        if (s_Window)   SDL_DestroyWindow(s_Window);
        SDL_Quit();
    }

    return 0;
}
