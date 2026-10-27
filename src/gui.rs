// Copyright (c) 2026 Robson Cassiano (https://eu.robsoncassiano.software)
// SPDX-License-Identifier: MIT

//! Native SDL2 graphical interface for interactive visualization of the Setun-Doom testbench.
//!
//! Displays an SDL2 window (640x400) showing the rendered testbench frame
//! and enables interactive keyboard camera control:
//! - W / S: move forward and backward in BSP scene
//! - A / D: translate camera sideways
//! - Space / Enter: reset scene or inject ALU cycles
//! - ESC / Q: close window

use std::ffi::{c_char, c_void, CString};
use std::ptr;
use crate::memory::{FB_HEIGHT, FB_WIDTH, FB_SIZE};

const SDL_INIT_VIDEO: u32 = 0x00000020;
const SDL_WINDOW_SHOWN: u32 = 0x00000004;
const SDL_RENDERER_ACCELERATED: u32 = 0x00000002;
const SDL_RENDERER_PRESENTVSYNC: u32 = 0x00000004;
const SDL_TEXTUREACCESS_STREAMING: i32 = 1;
const SDL_PIXELFORMAT_ABGR8888: u32 = 376840196; // SDL_PIXELFORMAT_RGBA32 / ABGR8888 compatible with memory.rs

const SDL_QUIT: u32 = 0x100;
const SDL_KEYDOWN: u32 = 0x300;

const SDLK_ESCAPE: i32 = 27;
const SDLK_Q: i32 = 'q' as i32;
const SDLK_W: i32 = 'w' as i32;
const SDLK_S: i32 = 's' as i32;
const SDLK_A: i32 = 'a' as i32;
const SDLK_D: i32 = 'd' as i32;
const SDLK_SPACE: i32 = ' ' as i32;

#[derive(Copy, Clone)]
#[repr(C)]
struct SDL_Keysym {
    scancode: i32,
    sym: i32,
    mod_state: u16,
    unused: u32,
}

#[derive(Copy, Clone)]
#[repr(C)]
struct SDL_KeyboardEvent {
    event_type: u32,
    timestamp: u32,
    window_id: u32,
    state: u8,
    repeat: u8,
    padding2: u8,
    padding3: u8,
    keysym: SDL_Keysym,
}

#[derive(Copy, Clone)]
#[repr(C)]
union SDL_Event {
    event_type: u32,
    key: SDL_KeyboardEvent,
    padding: [u8; 56],
}

#[link(name = "SDL2")]
extern "C" {
    fn SDL_Init(flags: u32) -> i32;
    fn SDL_Quit();
    fn SDL_CreateWindow(
        title: *const c_char,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        flags: u32,
    ) -> *mut c_void;
    fn SDL_DestroyWindow(window: *mut c_void);
    fn SDL_CreateRenderer(window: *mut c_void, index: i32, flags: u32) -> *mut c_void;
    fn SDL_DestroyRenderer(renderer: *mut c_void);
    fn SDL_CreateTexture(
        renderer: *mut c_void,
        format: u32,
        access: i32,
        w: i32,
        h: i32,
    ) -> *mut c_void;
    fn SDL_DestroyTexture(texture: *mut c_void);
    fn SDL_UpdateTexture(
        texture: *mut c_void,
        rect: *const c_void,
        pixels: *const c_void,
        pitch: i32,
    ) -> i32;
    fn SDL_RenderClear(renderer: *mut c_void) -> i32;
    fn SDL_RenderCopy(
        renderer: *mut c_void,
        texture: *mut c_void,
        srcrect: *const c_void,
        dstrect: *const c_void,
    ) -> i32;
    fn SDL_RenderPresent(renderer: *mut c_void);
    fn SDL_PollEvent(event: *mut SDL_Event) -> i32;
    fn SDL_Delay(ms: u32);
}

/// Interactive testbench graphical window.
pub struct BenchGui {
    window: *mut c_void,
    renderer: *mut c_void,
    texture: *mut c_void,
    pub cam_x: f64,
    pub cam_y: f64,
}

impl BenchGui {
    /// Initializes the SDL2 graphical window (640x400 virtual resolution for Setun's 320x200).
    pub fn new(title: &str) -> Option<Self> {
        unsafe {
            if SDL_Init(SDL_INIT_VIDEO) != 0 {
                eprintln!("[GUI] Failed to initialize SDL2 video subsystem.");
                return None;
            }
            let c_title = CString::new(title).unwrap_or_default();
            let window = SDL_CreateWindow(
                c_title.as_ptr(),
                0x1FFF0000, // SDL_WINDOWPOS_CENTERED
                0x1FFF0000,
                (FB_WIDTH * 2) as i32,
                (FB_HEIGHT * 2) as i32,
                SDL_WINDOW_SHOWN,
            );
            if window.is_null() {
                eprintln!("[GUI] Failed to create SDL2 window.");
                SDL_Quit();
                return None;
            }

            let renderer = SDL_CreateRenderer(
                window,
                -1,
                SDL_RENDERER_ACCELERATED | SDL_RENDERER_PRESENTVSYNC,
            );
            if renderer.is_null() {
                eprintln!("[GUI] Failed to create accelerated SDL2 renderer.");
                SDL_DestroyWindow(window);
                SDL_Quit();
                return None;
            }

            let texture = SDL_CreateTexture(
                renderer,
                SDL_PIXELFORMAT_ABGR8888,
                SDL_TEXTUREACCESS_STREAMING,
                FB_WIDTH as i32,
                FB_HEIGHT as i32,
            );
            if texture.is_null() {
                eprintln!("[GUI] Failed to create 320x200 streaming texture.");
                SDL_DestroyRenderer(renderer);
                SDL_DestroyWindow(window);
                SDL_Quit();
                return None;
            }

            Some(Self {
                window,
                renderer,
                texture,
                cam_x: 3.0,
                cam_y: 0.0,
            })
        }
    }

    /// Updates the pixel buffer on screen.
    pub fn present_frame(&mut self, rgba_buffer: &[u8]) {
        assert_eq!(rgba_buffer.len(), FB_SIZE * 4);
        unsafe {
            SDL_UpdateTexture(
                self.texture,
                ptr::null(),
                rgba_buffer.as_ptr() as *const c_void,
                (FB_WIDTH * 4) as i32,
            );
            SDL_RenderClear(self.renderer);
            SDL_RenderCopy(self.renderer, self.texture, ptr::null(), ptr::null());
            SDL_RenderPresent(self.renderer);
        }
    }

    /// Handles window events. Returns false if user requested exit (ESC, Q, or close window).
    pub fn poll_events(&mut self) -> bool {
        let mut event = SDL_Event { event_type: 0 };
        unsafe {
            while SDL_PollEvent(&mut event) != 0 {
                let ev_type = event.event_type;
                if ev_type == SDL_QUIT {
                    return false;
                }
                if ev_type == SDL_KEYDOWN {
                    let sym = event.key.keysym.sym;
                    match sym {
                        SDLK_ESCAPE | SDLK_Q => return false,
                        SDLK_W => self.cam_x -= 0.5,
                        SDLK_S => self.cam_x += 0.5,
                        SDLK_A => self.cam_y -= 0.5,
                        SDLK_D => self.cam_y += 0.5,
                        SDLK_SPACE => {
                            self.cam_x = 3.0;
                            self.cam_y = 0.0;
                        }
                        _ => {}
                    }
                }
            }
        }
        true
    }

    /// Polls events and dispatches key down events to a callback. Returns false on exit request.
    pub fn poll_events_with_keys<F: FnMut(i32)>(&mut self, mut on_key: F) -> bool {
        let mut event = SDL_Event { event_type: 0 };
        unsafe {
            while SDL_PollEvent(&mut event) != 0 {
                let ev_type = event.event_type;
                if ev_type == SDL_QUIT {
                    return false;
                }
                if ev_type == SDL_KEYDOWN {
                    let sym = event.key.keysym.sym;
                    if sym == SDLK_ESCAPE || sym == SDLK_Q {
                        return false;
                    }
                    on_key(sym);
                }
            }
        }
        true
    }

    /// Short delay for frame rate pacing (60 FPS).
    pub fn delay(&self, ms: u32) {
        unsafe {
            SDL_Delay(ms);
        }
    }
}

impl Drop for BenchGui {
    fn drop(&mut self) {
        unsafe {
            if !self.texture.is_null() {
                SDL_DestroyTexture(self.texture);
            }
            if !self.renderer.is_null() {
                SDL_DestroyRenderer(self.renderer);
            }
            if !self.window.is_null() {
                SDL_DestroyWindow(self.window);
            }
            SDL_Quit();
        }
    }
}
