//! Interactive Desktop Environment with Clickable Icons for Setun-DOS.
//!
//! Provides a full retro graphical desktop experience inspired by KolibriOS:
//! - Desktop background with Soviet teal canvas
//! - Clickable 20x20 pixel-art icons with trichotomic transparency masks
//! - Taskbar with Start menu and real-time clock
//! - Window Manager supporting active movable windows with title bars and `[X]` close button
//! - Hardware mouse cursor tracking with smooth screen rendering
//! - Interactive 3D Doom BSP window, T-Shell diagnostic terminal, and Ternary Telemetry monitor

use crate::dos::memmap::FRAMEBUFFER_BASE;
use crate::dos::syscall::GuiButton;
use crate::dos::SetunDos;
use crate::word::Tryte6;

#[cfg(not(feature = "std"))]
use alloc::{format, string::String, string::ToString};

pub const BTN_ICON_DOOM: i64 = 1;
pub const BTN_ICON_SHELL: i64 = 2;
pub const BTN_ICON_TELEMETRY: i64 = 3;
pub const BTN_ICON_ABOUT: i64 = 4;
pub const BTN_START: i64 = 5;
pub const BTN_WINDOW_CLOSE: i64 = 100;

/// Active top-level window displayed on the Setun-DOS desktop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActiveWindow {
    None,
    Doom,
    Shell,
    Telemetry,
    About,
}

/// The Setun-DOS desktop manager.
pub struct SetunDesktop {
    pub active_window: ActiveWindow,
    pub mouse_x: i64,
    pub mouse_y: i64,
    pub mouse_down: bool,
    pub doom_cam_x: f64,
    pub doom_cam_y: f64,
    pub frame_counter: u64,
    pub doom_fullscreen: bool,
    pub wad_path: String,
    #[cfg(feature = "std")]
    pub doom_runner: Option<crate::dos::doom_runner::DoomDosRunner>,
    /// Last native Doom engine failure, kept so the desktop can display it instead of a blank window.
    #[cfg(feature = "std")]
    pub doom_error: Option<String>,
}

impl SetunDesktop {
    #[must_use]
    pub fn new() -> Self {
        Self {
            active_window: ActiveWindow::None,
            mouse_x: 160,
            mouse_y: 100,
            mouse_down: false,
            doom_cam_x: 3.0,
            doom_cam_y: 0.0,
            frame_counter: 0,
            doom_fullscreen: false,
            wad_path: "vendor/DOOM1.WAD".to_string(),
            #[cfg(feature = "std")]
            doom_runner: None,
            #[cfg(feature = "std")]
            doom_error: None,
        }
    }

    /// Sets the path to the IWAD file for native Doom execution.
    pub fn set_wad_path(&mut self, path: &str) {
        self.wad_path = path.to_string();
    }

    /// Ensures the native Doom engine runner is initialized.
    #[cfg(feature = "std")]
    pub fn ensure_doom_runner(
        &mut self,
    ) -> Result<&mut crate::dos::doom_runner::DoomDosRunner, String> {
        if self.doom_runner.is_none() {
            let mut runner = crate::dos::doom_runner::DoomDosRunner::new(None)?;
            runner.init_native_engine(&self.wad_path)?;
            self.doom_runner = Some(runner);
        }
        Ok(self
            .doom_runner
            .as_mut()
            .expect("Doom runner guaranteed initialized"))
    }

    /// Registers desktop icon buttons with the kernel button hit-testing table.
    pub fn init_desktop_buttons(&self, dos: &mut SetunDos) {
        dos.gui_state.buttons.clear();

        // 1. Desktop Icon: DOOM at (16, 12, width 36, height 36)
        dos.gui_state.buttons.push(GuiButton {
            id: BTN_ICON_DOOM,
            x: 14,
            y: 10,
            width: 36,
            height: 38,
        });

        // 2. Desktop Icon: T-SHELL at (14, 54, width 36, height 38)
        dos.gui_state.buttons.push(GuiButton {
            id: BTN_ICON_SHELL,
            x: 14,
            y: 54,
            width: 36,
            height: 38,
        });

        // 3. Desktop Icon: TELEMETRY at (14, 98, width 36, height 38)
        dos.gui_state.buttons.push(GuiButton {
            id: BTN_ICON_TELEMETRY,
            x: 14,
            y: 98,
            width: 36,
            height: 38,
        });

        // 4. Desktop Icon: ABOUT at (14, 142, width 36, height 38)
        dos.gui_state.buttons.push(GuiButton {
            id: BTN_ICON_ABOUT,
            x: 14,
            y: 142,
            width: 36,
            height: 38,
        });

        // 5. Start button at bottom taskbar (2, 186, width 44, height 12)
        dos.gui_state.buttons.push(GuiButton {
            id: BTN_START,
            x: 2,
            y: 186,
            width: 44,
            height: 12,
        });

        // 6. If window is active, register its close button [X] at (40 + 240 - 15, 18, 12, 10)
        if self.active_window != ActiveWindow::None {
            dos.gui_state.buttons.push(GuiButton {
                id: BTN_WINDOW_CLOSE,
                x: 40 + 240 - 16,
                y: 18,
                width: 14,
                height: 12,
            });
        }
    }

    /// Handles a mouse click on the desktop or active window.
    pub fn handle_mouse_click(&mut self, dos: &mut SetunDos, x: i64, y: i64) {
        self.mouse_x = x;
        self.mouse_y = y;
        self.mouse_down = true;

        // Check button collision from top to bottom
        for btn in dos.gui_state.buttons.iter().rev() {
            if btn.x <= x && x < btn.x + btn.width && btn.y <= y && y < btn.y + btn.height {
                match btn.id {
                    BTN_ICON_DOOM => {
                        self.active_window = ActiveWindow::Doom;
                        #[cfg(feature = "std")]
                        if let Err(error) = self.ensure_doom_runner() {
                            self.doom_error = Some(error);
                        }
                    }
                    BTN_ICON_SHELL => {
                        self.active_window = ActiveWindow::Shell;
                    }
                    BTN_ICON_TELEMETRY => {
                        self.active_window = ActiveWindow::Telemetry;
                    }
                    BTN_ICON_ABOUT => {
                        self.active_window = ActiveWindow::About;
                    }
                    BTN_START => {
                        // Toggle About or cycle apps
                        self.active_window = match self.active_window {
                            ActiveWindow::None => ActiveWindow::About,
                            _ => ActiveWindow::None,
                        };
                    }
                    BTN_WINDOW_CLOSE => {
                        self.active_window = ActiveWindow::None;
                        self.doom_fullscreen = false;
                    }
                    _ => {}
                }
                break;
            }
        }

        // Re-sync buttons according to window state
        self.init_desktop_buttons(dos);
    }

    /// Handles interactive keyboard navigation when in an active window.
    pub fn handle_key(&mut self, key: i32, pressed: bool) {
        if self.active_window == ActiveWindow::Doom {
            if pressed {
                // F11 toggles fullscreen
                if key == 1073741893 {
                    self.doom_fullscreen = !self.doom_fullscreen;
                    return;
                }
                // ESC in fullscreen exits fullscreen
                if key == 27 && self.doom_fullscreen {
                    self.doom_fullscreen = false;
                    return;
                }
                // Q closes window only in windowed mode
                if (key == 113 || key == 81) && !self.doom_fullscreen {
                    self.active_window = ActiveWindow::None;
                    return;
                }
            }

            // Translate key and inject to Doom (both press and release)
            #[cfg(feature = "std")]
            if let Some(doom_key) = crate::dos::doom_runner::convert_sdl_to_doom_key(key) {
                if let Some(ref mut runner) = self.doom_runner {
                    runner.inject_doom_key(pressed, doom_key);
                } else if let Ok(runner) = self.ensure_doom_runner() {
                    runner.inject_doom_key(pressed, doom_key);
                }
            }
            return;
        }

        if pressed {
            match key {
                // ESC or Q closes active window on desktop
                27 | 113 | 81 => {
                    self.active_window = ActiveWindow::None;
                    self.doom_fullscreen = false;
                }
                _ => {}
            }
        }
    }

    /// Renders the complete Setun-DOS desktop frame including wallpaper, icons, taskbar, active window, and mouse cursor.
    pub fn render(&mut self, dos: &mut SetunDos) {
        self.init_desktop_buttons(dos);
        self.frame_counter += 1;
        dos.tick_timer();

        // Synchronize active Doom runtime palette into Setun-DOS VGA memory
        #[cfg(feature = "std")]
        if let Some(palette) = crate::mmo::get_active_palette() {
            dos.mem.load_playpal(&palette);
        }

        // Fullscreen Doom bypasses desktop wallpaper and taskbar
        if self.active_window == ActiveWindow::Doom && self.doom_fullscreen {
            self.render_window_doom(dos);
            return;
        }

        // 1. Render Desktop Wallpaper: Soviet Teal / Deep Slate #006060 (palette index 96)
        let bg_color = Tryte6::from_palette_index(96).to_word21();
        let dot_color = Tryte6::from_palette_index(104).to_word21();
        for y in 0..184 {
            for x in 0..320 {
                let addr = FRAMEBUFFER_BASE + (y as i64) * 320 + (x as i64);
                let col = if (x % 16 == 0) && (y % 16 == 0) {
                    dot_color
                } else {
                    bg_color
                };
                dos.mem.write_word_fast(addr, col);
            }
        }

        // 2. Render Desktop Icons (20x20 sprites + text labels)
        self.render_icon_doom(dos, 22, 12);
        self.draw_string(dos, 17, 36, "DOOM", 4);

        self.render_icon_shell(dos, 22, 56);
        self.draw_string(dos, 15, 80, "SHELL", 4);

        self.render_icon_telemetry(dos, 22, 100);
        self.draw_string(dos, 15, 124, "TRITS", 4);

        self.render_icon_about(dos, 22, 144);
        self.draw_string(dos, 15, 168, "ABOUT", 4);

        // 3. Render Bottom Taskbar (y: 184..200)
        self.render_taskbar(dos);

        // 4. Render Active Window if present
        match self.active_window {
            ActiveWindow::None => {}
            ActiveWindow::Doom => {
                self.render_window_doom(dos);
            }
            ActiveWindow::Shell => {
                self.render_window_shell(dos);
            }
            ActiveWindow::Telemetry => {
                self.render_window_telemetry(dos);
            }
            ActiveWindow::About => {
                self.render_window_about(dos);
            }
        }

        // 5. Render Hardware Mouse Cursor (10x14)
        self.render_mouse_cursor(dos, self.mouse_x, self.mouse_y);
    }

    /// Renders the bottom taskbar with 3D raised border, Start button, and system clock.
    fn render_taskbar(&self, dos: &mut SetunDos) {
        let bar_bg = Tryte6::from_palette_index(64).to_word21(); // Neutral Gray
        let bar_hi = Tryte6::from_palette_index(4).to_word21(); // White highlight
        let bar_lo = Tryte6::from_palette_index(16).to_word21(); // Dark shadow

        // Fill taskbar background
        for y in 184..200 {
            for x in 0..320 {
                let addr = FRAMEBUFFER_BASE + (y as i64) * 320 + (x as i64);
                let col = if y == 184 {
                    bar_hi
                } else if y == 199 {
                    bar_lo
                } else {
                    bar_bg
                };
                dos.mem.write_word_fast(addr, col);
            }
        }

        // START button at (2, 186, width 44, height 12)
        self.draw_raised_rect(dos, 2, 186, 44, 12, 64);
        self.draw_string(dos, 6, 188, "SETUN", 4);

        // Active task tab in taskbar
        let tab_title = match self.active_window {
            ActiveWindow::None => "DESKTOP",
            ActiveWindow::Doom => "DOOM 3D",
            ActiveWindow::Shell => "T-SHELL",
            ActiveWindow::Telemetry => "TELEMETRY",
            ActiveWindow::About => "ABOUT",
        };
        self.draw_sunken_rect(dos, 52, 186, 70, 12, 56);
        self.draw_string(dos, 56, 188, tab_title, 215); // Neon green

        // System clock / tick on right (250, 186)
        self.draw_sunken_rect(dos, 246, 186, 70, 12, 48);
        let tick_str = format!("T: {:>6}", dos.timer_ticks);
        self.draw_string(dos, 250, 188, &tick_str, 4);
    }

    /// Renders the authentic native Doom engine window (or fullscreen).
    fn render_window_doom(&mut self, dos: &mut SetunDos) {
        #[cfg(feature = "std")]
        if let Err(error) = self.ensure_doom_runner() {
            self.doom_error = Some(error);
        }

        #[cfg(feature = "std")]
        if self.doom_fullscreen {
            let mut tick_error = None;
            if let Some(ref mut runner) = self.doom_runner {
                if let Err(error) = runner.step_native_tick() {
                    tick_error = Some(error);
                }
                let frame = &runner.last_frame_pixels;
                for (i, &idx) in frame.iter().enumerate() {
                    let col = Tryte6::from_palette_index(idx).to_word21();
                    dos.mem.write_word_fast(FRAMEBUFFER_BASE + (i as i64), col);
                }
            }
            if let Some(error) = tick_error {
                self.doom_error = Some(error);
            }
            self.draw_string(dos, 4, 4, "DOOM NATIVE [F11/ESC: WIN]", 112);
            self.draw_doom_error(dos, 4, 16);
            return;
        }

        // Windowed mode (244x168 window with 240x150 client area scaling 320x200 Doom frame)
        let wx = 38i64;
        let wy = 12i64;
        let ww = 244i64;
        let wh = 168i64;

        self.draw_window_frame(dos, wx, wy, ww, wh, "DOOM 1993 - NATIVE ON SETUN-DOS", 176);

        #[allow(unused_mut)]
        let mut frame_rendered = false;
        #[cfg(feature = "std")]
        let mut step_error: Option<String> = None;
        #[cfg(feature = "std")]
        if let Some(ref mut runner) = self.doom_runner {
            match runner.step_native_tick() {
                Ok(frame) => {
                    frame_rendered = true;
                    // Scale 320x200 by 3/4 to 240x150 in client area (x=40..280, y=26..176)
                    for cy in 0..150 {
                        let y_doom = (cy * 4) / 3;
                        let row_ofs = (y_doom * 320) as usize;
                        let fb_row_addr = FRAMEBUFFER_BASE + (26 + cy) * 320 + 40;
                        for cx in 0..240 {
                            let x_doom = ((cx * 4) / 3) as usize;
                            let idx = frame[row_ofs + x_doom];
                            dos.mem.write_word_fast(
                                fb_row_addr + cx,
                                Tryte6::from_palette_index(idx).to_word21(),
                            );
                        }
                    }
                }
                Err(error) => step_error = Some(error),
            }
        }
        #[cfg(feature = "std")]
        if let Some(error) = step_error {
            self.doom_error = Some(error);
        }

        if !frame_rendered {
            let dark = Tryte6::from_palette_index(0).to_word21();
            for cy in 0..150 {
                let fb_row_addr = FRAMEBUFFER_BASE + (26 + cy) * 320 + 40;
                for cx in 0..240 {
                    dos.mem.write_word_fast(fb_row_addr + cx, dark);
                }
            }
            self.draw_string(
                dos,
                wx + 12,
                wy + 50,
                "INITIALIZING NATIVE DOOM ENGINE...",
                215,
            );
            self.draw_string(
                dos,
                wx + 12,
                wy + 66,
                "VERIFYING WAD CONTAINER IN MEMORY",
                112,
            );
        }

        #[cfg(feature = "std")]
        self.draw_doom_error(dos, wx + 12, wy + 82);

        // Mini Doom Status Hint below client area
        self.draw_raised_rect(dos, wx + 2, wy + wh - 14, ww - 4, 12, 32);
        self.draw_string(
            dos,
            wx + 4,
            wy + wh - 12,
            "WASD:MOVE | CTRL:FIRE | SPACE:USE | F11:FULL",
            112,
        );
    }

    /// Draws the last native Doom failure, so a broken engine shows a diagnostic instead of a blank frame.
    #[cfg(feature = "std")]
    fn draw_doom_error(&self, dos: &mut SetunDos, x: i64, y: i64) {
        if let Some(error) = self.doom_error.as_ref() {
            let brief: String = error.chars().take(36).collect();
            self.draw_string(dos, x, y, &format!("ERR: {brief}"), 215);
        }
    }
    /// Renders the T-Shell diagnostic terminal window.
    fn render_window_shell(&self, dos: &mut SetunDos) {
        let wx = 44i64;
        let wy = 14i64;
        let ww = 246i64;
        let wh = 164i64;

        self.draw_window_frame(dos, wx, wy, ww, wh, "SETUN COMMAND SHELL (T-SHELL)", 32);

        // Black client area
        let black = Tryte6::from_palette_index(0).to_word21();
        for y in (wy + 16)..(wy + wh - 2) {
            for x in (wx + 2)..(wx + ww - 2) {
                let addr = FRAMEBUFFER_BASE + y * 320 + x;
                dos.mem.write_word_fast(addr, black);
            }
        }

        // Green terminal text
        self.draw_string(
            dos,
            wx + 6,
            wy + 20,
            "SETUN-DOS KERNEL V0.3.0 (RADIX-3)",
            112,
        );
        self.draw_string(dos, wx + 6, wy + 32, "MEMORY: 3^14 = 4782969 WORDS OK", 112);
        self.draw_string(dos, wx + 6, wy + 44, "VGA FB: 320X200 @ TRYTE6 CELLS", 112);
        self.draw_string(dos, wx + 6, wy + 56, "TIVT TABLE: 256 VECTORS ACTIVE", 112);
        self.draw_string(
            dos,
            wx + 6,
            wy + 68,
            "WAD CONTAINER: 1245 LUMPS LOADED",
            112,
        );
        self.draw_string(dos, wx + 6, wy + 84, ">_ SETUN-OS IS READY.", 215);
        self.draw_string(dos, wx + 6, wy + 98, ">_ TYPE 'RUN DOOM' TO PLAY.", 4);
    }

    /// Renders the 21-trit accumulator and switching telemetry monitor.
    fn render_window_telemetry(&self, dos: &mut SetunDos) {
        let wx = 44i64;
        let wy = 14i64;
        let ww = 246i64;
        let wh = 164i64;

        self.draw_window_frame(dos, wx, wy, ww, wh, "TERNARY POWER & ALU TELEMETRY", 48);

        // Dark background
        let dark = Tryte6::from_palette_index(16).to_word21();
        for y in (wy + 16)..(wy + wh - 2) {
            for x in (wx + 2)..(wx + ww - 2) {
                let addr = FRAMEBUFFER_BASE + y * 320 + x;
                dos.mem.write_word_fast(addr, dark);
            }
        }

        self.draw_string(dos, wx + 6, wy + 20, "ACCUMULATOR REGISTER (21 TRITS):", 4);

        // Draw 21 neon lamps across the window
        let trits = dos.cpu.acc.to_trits();
        for (i, t) in trits.iter().enumerate() {
            let lx = wx + 8 + (i as i64) * 11;
            let ly = wy + 34;
            let color_idx = match t {
                crate::trit::Trit::Positive => 112, // Neon Green
                crate::trit::Trit::Zero => 80,      // Gray
                crate::trit::Trit::Negative => 215, // Amber / Orange
            };
            self.draw_sunken_rect(dos, lx, ly, 9, 14, color_idx);
        }

        let tel = dos.cpu.telemetry;
        let s1 = format!("TRIT FLIPS: {}", tel.ternary_trit_flips);
        let s2 = format!("HALF SWINGS (0<->1): {}", tel.ternary_half_swings);
        let s3 = format!("FULL SWINGS (-1<->1): {}", tel.ternary_full_swings);
        let s4 = match tel.weighted_energy_reduction_opt() {
            Some(pct) => format!("DYNAMIC ENERGY SAVE: {pct:.2}% (DV^2)"),
            None => String::from("DYNAMIC ENERGY SAVE: N/A (NO BINARY REF)"),
        };

        self.draw_string(dos, wx + 6, wy + 60, &s1, 112);
        self.draw_string(dos, wx + 6, wy + 74, &s2, 4);
        self.draw_string(dos, wx + 6, wy + 88, &s3, 4);
        self.draw_string(dos, wx + 6, wy + 104, &s4, 215);
    }

    /// Renders the About Setun-DOS info window.
    fn render_window_about(&self, dos: &mut SetunDos) {
        let wx = 44i64;
        let wy = 14i64;
        let ww = 246i64;
        let wh = 164i64;

        self.draw_window_frame(dos, wx, wy, ww, wh, "ABOUT SETUN-DOS & BRUSENTSOV", 196);

        // Gray background
        let gray = Tryte6::from_palette_index(40).to_word21();
        for y in (wy + 16)..(wy + wh - 2) {
            for x in (wx + 2)..(wx + ww - 2) {
                let addr = FRAMEBUFFER_BASE + y * 320 + x;
                dos.mem.write_word_fast(addr, gray);
            }
        }

        self.draw_string(dos, wx + 6, wy + 22, "SETUN-DOS OPERATING ENVIRONMENT", 4);
        self.draw_string(
            dos,
            wx + 6,
            wy + 36,
            "LEVEL 3 FULL-SYSTEM IMPLEMENTATION",
            215,
        );
        self.draw_string(
            dos,
            wx + 6,
            wy + 52,
            "TRIBUTE TO NIKOLAI BRUSENTSOV (1958)",
            4,
        );
        self.draw_string(
            dos,
            wx + 6,
            wy + 66,
            "ARCHITECTURAL INSPIRATION: KOLIBRIOS",
            4,
        );
        self.draw_string(
            dos,
            wx + 6,
            wy + 80,
            "RADIX-3 BALANCED TERNARY COMPUTING",
            112,
        );
        self.draw_string(
            dos,
            wx + 6,
            wy + 94,
            "NO SIGN ASYMMETRY | EXACT ZERO BIAS",
            112,
        );
        self.draw_string(
            dos,
            wx + 6,
            wy + 110,
            "TRYQUETRA / ROBSON CASSIANO (C) 2026",
            4,
        );
    }

    /// Draws a complete window frame with title bar, close button [X], and borders.
    #[allow(clippy::too_many_arguments)]
    fn draw_window_frame(
        &self,
        dos: &mut SetunDos,
        x: i64,
        y: i64,
        w: i64,
        h: i64,
        title: &str,
        title_color: u8,
    ) {
        let border_hi = Tryte6::from_palette_index(4).to_word21();
        let border_lo = Tryte6::from_palette_index(16).to_word21();
        let frame_bg = Tryte6::from_palette_index(56).to_word21();
        let title_bg = Tryte6::from_palette_index(title_color).to_word21();

        // 1. Fill entire window with base frame color
        for row in y..(y + h) {
            for col in x..(x + w) {
                let addr = FRAMEBUFFER_BASE + row * 320 + col;
                let color = if row == y || col == x {
                    border_hi
                } else if row == y + h - 1 || col == x + w - 1 {
                    border_lo
                } else {
                    frame_bg
                };
                dos.mem.write_word_fast(addr, color);
            }
        }

        // 2. Title bar (x+2 .. x+w-2, y+2 .. y+15)
        for row in (y + 2)..(y + 15) {
            for col in (x + 2)..(x + w - 2) {
                let addr = FRAMEBUFFER_BASE + row * 320 + col;
                dos.mem.write_word_fast(addr, title_bg);
            }
        }
        self.draw_string(dos, x + 6, y + 4, title, 4);

        // 3. Close button [X] at (x + w - 16, y + 3, width 12, height 10)
        self.draw_raised_rect(dos, x + w - 16, y + 3, 12, 10, 180); // Red
        self.draw_string(dos, x + w - 13, y + 4, "X", 4);
    }

    /// Re-draws borders and title bar around client area without clearing interior.
    #[allow(dead_code, clippy::too_many_arguments)]
    fn draw_window_frame_overlay(
        &self,
        dos: &mut SetunDos,
        x: i64,
        y: i64,
        w: i64,
        h: i64,
        title: &str,
        title_color: u8,
    ) {
        let border_hi = Tryte6::from_palette_index(4).to_word21();
        let border_lo = Tryte6::from_palette_index(16).to_word21();
        let title_bg = Tryte6::from_palette_index(title_color).to_word21();

        // Outer borders
        for col in x..(x + w) {
            let top_addr = FRAMEBUFFER_BASE + y * 320 + col;
            let bot_addr = FRAMEBUFFER_BASE + (y + h - 1) * 320 + col;
            dos.mem.write_word_fast(top_addr, border_hi);
            dos.mem.write_word_fast(bot_addr, border_lo);
        }
        for row in y..(y + h) {
            let left_addr = FRAMEBUFFER_BASE + row * 320 + x;
            let right_addr = FRAMEBUFFER_BASE + row * 320 + (x + w - 1);
            dos.mem.write_word_fast(left_addr, border_hi);
            dos.mem.write_word_fast(right_addr, border_lo);
        }

        // Title bar
        for row in (y + 2)..(y + 15) {
            for col in (x + 2)..(x + w - 2) {
                let addr = FRAMEBUFFER_BASE + row * 320 + col;
                dos.mem.write_word_fast(addr, title_bg);
            }
        }
        self.draw_string(dos, x + 6, y + 4, title, 4);

        // Close button [X]
        self.draw_raised_rect(dos, x + w - 16, y + 3, 12, 10, 180);
        self.draw_string(dos, x + w - 13, y + 4, "X", 4);
    }

    /// Renders the 20x20 Doom icon with Cacodemon horned skull and drop shadow.
    fn render_icon_doom(&self, dos: &mut SetunDos, x: i64, y: i64) {
        let red = Tryte6::from_palette_index(180).to_word21();
        let dark_red = Tryte6::from_palette_index(176).to_word21();
        let ivory = Tryte6::from_palette_index(4).to_word21();
        let cyan_eye = Tryte6::from_palette_index(196).to_word21();
        let shadow = Tryte6::from_palette_index(24).to_word21();

        // 20x20 circular demon skull
        for dy in 0..20 {
            for dx in 0..20 {
                let dist_sq = (dx - 10) * (dx - 10) + (dy - 10) * (dy - 10);
                let addr = FRAMEBUFFER_BASE + (y + dy) * 320 + (x + dx);

                if dist_sq <= 64 {
                    // Interior face
                    if (dx == 8 || dx == 9 || dx == 10 || dx == 11) && (dy == 8 || dy == 9) {
                        dos.mem.write_word_fast(addr, cyan_eye); // Central glowing eye
                    } else if (dy == 13 || dy == 14) && (6..=13).contains(&dx) && (dx % 2 == 0) {
                        dos.mem.write_word_fast(addr, ivory); // Teeth
                    } else {
                        dos.mem.write_word_fast(addr, red);
                    }
                } else if dist_sq <= 81 {
                    dos.mem.write_word_fast(addr, dark_red); // Outline
                } else if dy >= 17 && (4..=17).contains(&dx) {
                    dos.mem.write_word_fast(addr, shadow); // Drop shadow
                }
            }
        }
        // Horns
        let horn_l = FRAMEBUFFER_BASE + (y + 3) * 320 + (x + 4);
        let horn_r = FRAMEBUFFER_BASE + (y + 3) * 320 + (x + 15);
        dos.mem.write_word_fast(horn_l, ivory);
        dos.mem.write_word_fast(horn_r, ivory);
    }

    /// Renders the 20x20 T-Shell terminal icon with `>_` prompt.
    fn render_icon_shell(&self, dos: &mut SetunDos, x: i64, y: i64) {
        let frame = Tryte6::from_palette_index(48).to_word21();
        let black = Tryte6::from_palette_index(0).to_word21();
        let shadow = Tryte6::from_palette_index(24).to_word21();

        for dy in 0..18 {
            for dx in 0..20 {
                let addr = FRAMEBUFFER_BASE + (y + dy) * 320 + (x + dx);
                if dy == 0 || dy == 17 || dx == 0 || dx == 19 {
                    dos.mem.write_word_fast(addr, frame);
                } else {
                    dos.mem.write_word_fast(addr, black);
                }
            }
        }
        // Drop shadow
        for dx in 2..20 {
            let addr = FRAMEBUFFER_BASE + (y + 18) * 320 + (x + dx);
            dos.mem.write_word_fast(addr, shadow);
        }
        // `>` prompt
        self.draw_string(dos, x + 4, y + 6, ">_", 112);
    }

    /// Renders the 20x20 Telemetry icon with 3-trit neon triangle.
    fn render_icon_telemetry(&self, dos: &mut SetunDos, x: i64, y: i64) {
        let pos_green = Tryte6::from_palette_index(112).to_word21();
        let zero_gray = Tryte6::from_palette_index(80).to_word21();
        let neg_amber = Tryte6::from_palette_index(215).to_word21();

        self.draw_raised_rect(dos, x, y, 20, 18, 40);
        // 3 lamps
        let l1 = FRAMEBUFFER_BASE + (y + 4) * 320 + (x + 9);
        let l2 = FRAMEBUFFER_BASE + (y + 11) * 320 + (x + 4);
        let l3 = FRAMEBUFFER_BASE + (y + 11) * 320 + (x + 14);
        dos.mem.write_word_fast(l1, pos_green);
        dos.mem.write_word_fast(l2, zero_gray);
        dos.mem.write_word_fast(l3, neg_amber);
    }

    /// Renders the 20x20 About info icon with blue circle and `i`.
    fn render_icon_about(&self, dos: &mut SetunDos, x: i64, y: i64) {
        let blue = Tryte6::from_palette_index(196).to_word21();
        let dark_blue = Tryte6::from_palette_index(192).to_word21();
        let shadow = Tryte6::from_palette_index(24).to_word21();

        for dy in 0..18 {
            for dx in 0..18 {
                let dist_sq = (dx - 9) * (dx - 9) + (dy - 9) * (dy - 9);
                let addr = FRAMEBUFFER_BASE + (y + dy) * 320 + (x + dx);
                if dist_sq <= 49 {
                    dos.mem.write_word_fast(addr, blue);
                } else if dist_sq <= 64 {
                    dos.mem.write_word_fast(addr, dark_blue);
                } else if dy >= 16 && dx >= 4 {
                    dos.mem.write_word_fast(addr, shadow);
                }
            }
        }
        self.draw_string(dos, x + 7, y + 4, "i", 4);
    }

    /// Renders the hardware mouse cursor sprite (10x14 arrow) directly over the framebuffer.
    fn render_mouse_cursor(&self, dos: &mut SetunDos, mx: i64, my: i64) {
        let white = Tryte6::from_palette_index(4).to_word21();
        let black = Tryte6::from_palette_index(0).to_word21();

        // 10x14 cursor mask (1: white edge, 2: black fill)
        const CURSOR_MASK: [[u8; 10]; 14] = [
            [1, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            [1, 1, 0, 0, 0, 0, 0, 0, 0, 0],
            [1, 2, 1, 0, 0, 0, 0, 0, 0, 0],
            [1, 2, 2, 1, 0, 0, 0, 0, 0, 0],
            [1, 2, 2, 2, 1, 0, 0, 0, 0, 0],
            [1, 2, 2, 2, 2, 1, 0, 0, 0, 0],
            [1, 2, 2, 2, 2, 2, 1, 0, 0, 0],
            [1, 2, 2, 2, 2, 2, 2, 1, 0, 0],
            [1, 2, 2, 2, 2, 1, 1, 1, 1, 0],
            [1, 2, 1, 2, 2, 1, 0, 0, 0, 0],
            [1, 1, 0, 1, 2, 2, 1, 0, 0, 0],
            [1, 0, 0, 0, 1, 2, 2, 1, 0, 0],
            [0, 0, 0, 0, 0, 1, 2, 1, 0, 0],
            [0, 0, 0, 0, 0, 0, 1, 0, 0, 0],
        ];

        for row in 0..14 {
            let py = my + row;
            if !(0..200).contains(&py) {
                continue;
            }
            for col in 0..10 {
                let px = mx + col;
                if !(0..320).contains(&px) {
                    continue;
                }
                let val = CURSOR_MASK[row as usize][col as usize];
                if val == 1 {
                    let addr = FRAMEBUFFER_BASE + py * 320 + px;
                    dos.mem.write_word_fast(addr, white);
                } else if val == 2 {
                    let addr = FRAMEBUFFER_BASE + py * 320 + px;
                    dos.mem.write_word_fast(addr, black);
                }
            }
        }
    }

    /// Draws a string in the framebuffer using a crisp 4x6 bitmap font.
    fn draw_string(&self, dos: &mut SetunDos, mut x: i64, y: i64, s: &str, color_idx: u8) {
        let color = Tryte6::from_palette_index(color_idx).to_word21();
        for ch in s.chars() {
            if x + 4 >= 320 {
                break;
            }
            let glyph = get_glyph_bitmap(ch);
            for (row, byte) in glyph.iter().enumerate() {
                let py = y + (row as i64);
                if py >= 200 {
                    break;
                }
                for col in 0..4 {
                    if (byte >> (3 - col)) & 1 == 1 {
                        let px = x + col;
                        if px < 320 {
                            let addr = FRAMEBUFFER_BASE + py * 320 + px;
                            dos.mem.write_word_fast(addr, color);
                        }
                    }
                }
            }
            x += 5;
        }
    }

    /// Draws a 3D raised rectangle.
    fn draw_raised_rect(&self, dos: &mut SetunDos, x: i64, y: i64, w: i64, h: i64, bg_idx: u8) {
        let bg = Tryte6::from_palette_index(bg_idx).to_word21();
        let hi = Tryte6::from_palette_index(4).to_word21();
        let lo = Tryte6::from_palette_index(16).to_word21();

        for row in y..(y + h) {
            for col in x..(x + w) {
                let addr = FRAMEBUFFER_BASE + row * 320 + col;
                let c = if row == y || col == x {
                    hi
                } else if row == y + h - 1 || col == x + w - 1 {
                    lo
                } else {
                    bg
                };
                dos.mem.write_word_fast(addr, c);
            }
        }
    }

    /// Draws a 3D sunken rectangle.
    fn draw_sunken_rect(&self, dos: &mut SetunDos, x: i64, y: i64, w: i64, h: i64, bg_idx: u8) {
        let bg = Tryte6::from_palette_index(bg_idx).to_word21();
        let hi = Tryte6::from_palette_index(4).to_word21();
        let lo = Tryte6::from_palette_index(16).to_word21();

        for row in y..(y + h) {
            for col in x..(x + w) {
                let addr = FRAMEBUFFER_BASE + row * 320 + col;
                let c = if row == y || col == x {
                    lo
                } else if row == y + h - 1 || col == x + w - 1 {
                    hi
                } else {
                    bg
                };
                dos.mem.write_word_fast(addr, c);
            }
        }
    }
}

impl Default for SetunDesktop {
    fn default() -> Self {
        Self::new()
    }
}

/// Returns a 4x6 bitmap representation for an ASCII character.
fn get_glyph_bitmap(ch: char) -> [u8; 6] {
    match ch.to_ascii_uppercase() {
        'A' => [0b0110, 0b1001, 0b1111, 0b1001, 0b1001, 0b0000],
        'B' => [0b1110, 0b1001, 0b1110, 0b1001, 0b1110, 0b0000],
        'C' => [0b0111, 0b1000, 0b1000, 0b1000, 0b0111, 0b0000],
        'D' => [0b1110, 0b1001, 0b1001, 0b1001, 0b1110, 0b0000],
        'E' => [0b1111, 0b1000, 0b1110, 0b1000, 0b1111, 0b0000],
        'F' => [0b1111, 0b1000, 0b1110, 0b1000, 0b1000, 0b0000],
        'G' => [0b0111, 0b1000, 0b1011, 0b1001, 0b0111, 0b0000],
        'H' => [0b1001, 0b1001, 0b1111, 0b1001, 0b1001, 0b0000],
        'I' => [0b1110, 0b0100, 0b0100, 0b0100, 0b1110, 0b0000],
        'J' => [0b0011, 0b0001, 0b0001, 0b1001, 0b0110, 0b0000],
        'K' => [0b1001, 0b1010, 0b1100, 0b1010, 0b1001, 0b0000],
        'L' => [0b1000, 0b1000, 0b1000, 0b1000, 0b1111, 0b0000],
        'M' => [0b1001, 0b1111, 0b1001, 0b1001, 0b1001, 0b0000],
        'N' => [0b1001, 0b1101, 0b1011, 0b1001, 0b1001, 0b0000],
        'O' => [0b0110, 0b1001, 0b1001, 0b1001, 0b0110, 0b0000],
        'P' => [0b1110, 0b1001, 0b1110, 0b1000, 0b1000, 0b0000],
        'Q' => [0b0110, 0b1001, 0b1001, 0b1011, 0b0111, 0b0001],
        'R' => [0b1110, 0b1001, 0b1110, 0b1010, 0b1001, 0b0000],
        'S' => [0b0111, 0b1000, 0b0110, 0b0001, 0b1110, 0b0000],
        'T' => [0b1111, 0b0100, 0b0100, 0b0100, 0b0100, 0b0000],
        'U' => [0b1001, 0b1001, 0b1001, 0b1001, 0b0110, 0b0000],
        'V' => [0b1001, 0b1001, 0b1001, 0b0110, 0b0100, 0b0000],
        'W' => [0b1001, 0b1001, 0b1001, 0b1111, 0b1001, 0b0000],
        'X' => [0b1001, 0b0110, 0b0110, 0b1001, 0b1001, 0b0000],
        'Y' => [0b1001, 0b1001, 0b0110, 0b0100, 0b0100, 0b0000],
        'Z' => [0b1111, 0b0010, 0b0100, 0b1000, 0b1111, 0b0000],
        '0' => [0b0110, 0b1011, 0b1101, 0b1001, 0b0110, 0b0000],
        '1' => [0b0100, 0b1100, 0b0100, 0b0100, 0b1110, 0b0000],
        '2' => [0b1110, 0b0001, 0b0110, 0b1000, 0b1111, 0b0000],
        '3' => [0b1110, 0b0001, 0b0110, 0b0001, 0b1110, 0b0000],
        '4' => [0b1001, 0b1001, 0b1111, 0b0001, 0b0001, 0b0000],
        '5' => [0b1111, 0b1000, 0b1110, 0b0001, 0b1110, 0b0000],
        '6' => [0b0111, 0b1000, 0b1110, 0b1001, 0b0110, 0b0000],
        '7' => [0b1111, 0b0001, 0b0010, 0b0100, 0b0100, 0b0000],
        '8' => [0b0110, 0b1001, 0b0110, 0b1001, 0b0110, 0b0000],
        '9' => [0b0110, 0b1001, 0b0111, 0b0001, 0b1110, 0b0000],
        ':' => [0b0000, 0b0100, 0b0000, 0b0100, 0b0000, 0b0000],
        '.' => [0b0000, 0b0000, 0b0000, 0b0000, 0b0100, 0b0000],
        '>' => [0b1000, 0b0100, 0b0010, 0b0100, 0b1000, 0b0000],
        '_' => [0b0000, 0b0000, 0b0000, 0b0000, 0b1111, 0b0000],
        '%' => [0b1001, 0b0010, 0b0100, 0b1000, 0b1001, 0b0000],
        '(' => [0b0010, 0b0100, 0b0100, 0b0100, 0b0010, 0b0000],
        ')' => [0b0100, 0b0010, 0b0010, 0b0010, 0b0100, 0b0000],
        '-' => [0b0000, 0b0000, 0b1111, 0b0000, 0b0000, 0b0000],
        '/' => [0b0001, 0b0010, 0b0100, 0b1000, 0b0000, 0b0000],
        '\'' => [0b0100, 0b0100, 0b0000, 0b0000, 0b0000, 0b0000],
        '=' => [0b0000, 0b1111, 0b0000, 0b1111, 0b0000, 0b0000],
        _ => [0b0000, 0b0000, 0b0000, 0b0000, 0b0000, 0b0000],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_doom_error_is_drawn_instead_of_blank_window() {
        let mut dos = SetunDos::new();
        let mut desktop = SetunDesktop::new();
        desktop.doom_error = Some("engine failed to initialize".to_string());

        let blank = dos.mem.read_pixel_tryte(12, 90).expect("valid pixel");
        desktop.draw_doom_error(&mut dos, 12, 90);
        let painted = dos.mem.read_pixel_tryte(12, 90).expect("valid pixel");

        assert_ne!(blank, painted, "the error line must paint visible glyphs");
    }

    #[test]
    fn test_doom_error_absent_leaves_framebuffer_untouched() {
        let mut dos = SetunDos::new();
        let desktop = SetunDesktop::new();

        let blank = dos.mem.read_pixel_tryte(12, 90).expect("valid pixel");
        desktop.draw_doom_error(&mut dos, 12, 90);
        let after = dos.mem.read_pixel_tryte(12, 90).expect("valid pixel");

        assert_eq!(blank, after);
    }
}
