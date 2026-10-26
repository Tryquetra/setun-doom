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

// Funções exportadas pela nossa biblioteca Rust `libsetun_doom.a` (src/mmo.rs)
extern void setun_BlitVideoBuffer(const uint8_t* video_buffer_320x200, uint8_t* out_rgba_320x200);
extern void setun_PrintTelemetry(uint32_t frame_num);
extern void setun_PushKeyEvent(int32_t pressed, uint8_t doom_key);

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

static int poll_sdl_events(void)
{
    SDL_Event e;
    while (SDL_PollEvent(&e)) {
        if (e.type == SDL_QUIT) {
            return 0;
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
