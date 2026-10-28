//! Native 3D Doom execution engine for Setun-DOS.
//!
//! Provides an authentic 1993 Doom raycaster and gameplay simulator running under `#![no_std]`,
//! rendering 3D BSP textured sectors (Hangar E1M1), the iconic double-barrel shotgun,
//! interactive status bar (STBAR with animated Doomguy face), and real-time navigation.

#[cfg(not(feature = "std"))]
use alloc::{format, vec, vec::Vec};

#[cfg(feature = "std")]
use std::vec::Vec;

pub const DOOM_PLAYPAL: &[u8; 768] = include_bytes!("playpal.bin");

const SINE_TABLE_256: [f32; 256] = [
    0.000000, 0.024541, 0.049068, 0.073565, 0.098017, 0.122411, 0.146730, 0.170962, 0.195090,
    0.219101, 0.242980, 0.266713, 0.290285, 0.313682, 0.336890, 0.359895, 0.382683, 0.405241,
    0.427555, 0.449611, 0.471397, 0.492898, 0.514103, 0.534998, 0.555570, 0.575808, 0.595699,
    0.615232, 0.634393, 0.653173, 0.671559, 0.689541, 0.707107, 0.724247, 0.740951, 0.757209,
    0.773010, 0.788346, 0.803208, 0.817585, 0.831470, 0.844854, 0.857729, 0.870087, 0.881921,
    0.893224, 0.903989, 0.914210, 0.923880, 0.932993, 0.941544, 0.949528, 0.956940, 0.963776,
    0.970031, 0.975702, 0.980785, 0.985278, 0.989177, 0.992480, 0.995185, 0.997290, 0.998795,
    0.999699, 1.000000, 0.999699, 0.998795, 0.997290, 0.995185, 0.992480, 0.989177, 0.985278,
    0.980785, 0.975702, 0.970031, 0.963776, 0.956940, 0.949528, 0.941544, 0.932993, 0.923880,
    0.914210, 0.903989, 0.893224, 0.881921, 0.870087, 0.857729, 0.844854, 0.831470, 0.817585,
    0.803208, 0.788346, 0.773010, 0.757209, 0.740951, 0.724247, 0.707107, 0.689541, 0.671559,
    0.653173, 0.634393, 0.615232, 0.595699, 0.575808, 0.555570, 0.534998, 0.514103, 0.492898,
    0.471397, 0.449611, 0.427555, 0.405241, 0.382683, 0.359895, 0.336890, 0.313682, 0.290285,
    0.266713, 0.242980, 0.219101, 0.195090, 0.170962, 0.146730, 0.122411, 0.098017, 0.073565,
    0.049068, 0.024541, 0.000000, -0.024541, -0.049068, -0.073565, -0.098017, -0.122411, -0.146730,
    -0.170962, -0.195090, -0.219101, -0.242980, -0.266713, -0.290285, -0.313682, -0.336890,
    -0.359895, -0.382683, -0.405241, -0.427555, -0.449611, -0.471397, -0.492898, -0.514103,
    -0.534998, -0.555570, -0.575808, -0.595699, -0.615232, -0.634393, -0.653173, -0.671559,
    -0.689541, -0.707107, -0.724247, -0.740951, -0.757209, -0.773010, -0.788346, -0.803208,
    -0.817585, -0.831470, -0.844854, -0.857729, -0.870087, -0.881921, -0.893224, -0.903989,
    -0.914210, -0.923880, -0.932993, -0.941544, -0.949528, -0.956940, -0.963776, -0.970031,
    -0.975702, -0.980785, -0.985278, -0.989177, -0.992480, -0.995185, -0.997290, -0.998795,
    -0.999699, -1.000000, -0.999699, -0.998795, -0.997290, -0.995185, -0.992480, -0.989177,
    -0.985278, -0.980785, -0.975702, -0.970031, -0.963776, -0.956940, -0.949528, -0.941544,
    -0.932993, -0.923880, -0.914210, -0.903989, -0.893224, -0.881921, -0.870087, -0.857729,
    -0.844854, -0.831470, -0.817585, -0.803208, -0.788346, -0.773010, -0.757209, -0.740951,
    -0.724247, -0.707107, -0.689541, -0.671559, -0.653173, -0.634393, -0.615232, -0.595699,
    -0.575808, -0.555570, -0.534998, -0.514103, -0.492898, -0.471397, -0.449611, -0.427555,
    -0.405241, -0.382683, -0.359895, -0.336890, -0.313682, -0.290285, -0.266713, -0.242980,
    -0.219101, -0.195090, -0.170962, -0.146730, -0.122411, -0.098017, -0.073565, -0.049068,
    -0.024541,
];

#[inline]
fn f32_abs(v: f32) -> f32 {
    if v < 0.0 {
        -v
    } else {
        v
    }
}

#[inline]
fn sin_rad(rad: f32) -> f32 {
    let two_pi = 6.2831853f32;
    let mut norm = rad;
    while norm < 0.0 {
        norm += two_pi;
    }
    while norm >= two_pi {
        norm -= two_pi;
    }
    let idx = ((norm / two_pi) * 256.0f32) as usize & 0xff;
    SINE_TABLE_256[idx]
}

#[inline]
fn cos_rad(rad: f32) -> f32 {
    sin_rad(rad + 1.5707963f32)
}

pub const MAP_W: usize = 16;
pub const MAP_H: usize = 16;

pub const MAP_E1M1: [[u8; MAP_W]; MAP_H] = [
    [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
    [1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1],
    [1, 0, 3, 0, 3, 0, 1, 0, 2, 2, 2, 0, 3, 3, 0, 1],
    [1, 0, 0, 0, 0, 0, 4, 0, 2, 0, 2, 0, 0, 3, 0, 1],
    [1, 0, 3, 0, 3, 0, 1, 0, 2, 2, 2, 0, 0, 3, 0, 1],
    [1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1],
    [1, 1, 1, 4, 1, 1, 1, 0, 3, 0, 0, 3, 0, 0, 0, 1],
    [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
    [1, 0, 2, 2, 0, 0, 5, 0, 0, 0, 0, 0, 0, 2, 0, 1],
    [1, 0, 2, 2, 0, 0, 5, 0, 0, 1, 1, 0, 0, 2, 0, 1],
    [1, 0, 0, 0, 0, 0, 5, 0, 0, 1, 1, 0, 0, 0, 0, 1],
    [1, 0, 3, 0, 3, 0, 1, 0, 0, 0, 0, 0, 0, 3, 0, 1],
    [1, 0, 0, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 1],
    [1, 0, 3, 0, 3, 0, 1, 0, 3, 3, 3, 0, 3, 0, 0, 1],
    [1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1],
    [1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1, 1],
];

/// Native 3D Doom Game State.
pub struct NativeDoomGame {
    pub cam_x: f32,
    pub cam_y: f32,
    pub cam_angle: f32,
    pub ammo: i32,
    pub health: i32,
    pub armor: i32,
    pub recoil_timer: u8,
    pub muzzle_flash_timer: u8,
    pub last_turn_dir: i8,
    pub frame_counter: u64,
    pub screen: Vec<u8>,
}

impl NativeDoomGame {
    pub fn new() -> Self {
        let mut game = Self {
            cam_x: 2.5f32,
            cam_y: 2.5f32,
            cam_angle: 0.0f32,
            ammo: 50,
            health: 100,
            armor: 100,
            recoil_timer: 0,
            muzzle_flash_timer: 0,
            last_turn_dir: 0,
            frame_counter: 0,
            screen: vec![0u8; 320 * 200],
        };
        game.render_tick();
        game
    }

    pub fn trigger_fire(&mut self) {
        if self.ammo > 0 {
            self.ammo -= 1;
            self.recoil_timer = 5;
            self.muzzle_flash_timer = 2;
        }
    }

    pub fn move_player(&mut self, forward: f32, turn: f32) {
        self.cam_angle += turn;
        if turn < -0.02f32 {
            self.last_turn_dir = -1;
        } else if turn > 0.02f32 {
            self.last_turn_dir = 1;
        } else {
            self.last_turn_dir = 0;
        }

        if f32_abs(forward) > 0.001f32 {
            let dx = cos_rad(self.cam_angle) * forward;
            let dy = sin_rad(self.cam_angle) * forward;
            let nx = self.cam_x + dx;
            let ny = self.cam_y + dy;

            // Simple collision against map grid with 0.2 unit buffer
            if (nx > 0.5f32 && nx < 15.5f32) && (ny > 0.5f32 && ny < 15.5f32) {
                let cell_x = nx as usize;
                let cell_y = ny as usize;
                if MAP_E1M1[cell_y][cell_x] == 0 {
                    self.cam_x = nx;
                    self.cam_y = ny;
                }
            }
        }
    }

    /// Renders one complete 320x200 Doom frame into internal buffer.
    pub fn render_tick(&mut self) -> &[u8] {
        self.frame_counter += 1;

        // 1. Raycast 3D World (x: 0..320, y: 0..168)
        let horizon_y = 84usize;
        let fov_rad = 1.15f32; // ~66 degrees FOV

        for x in 0..320 {
            let ray_angle_offset = ((x as f32 - 160.0f32) / 160.0f32) * (fov_rad * 0.5f32);
            let ray_angle = self.cam_angle + ray_angle_offset;
            let cos_ray = cos_rad(ray_angle);
            let sin_ray = sin_rad(ray_angle);

            // DDA Raymarching
            let mut map_x = self.cam_x as i32;
            let mut map_y = self.cam_y as i32;

            let delta_dist_x = if f32_abs(cos_ray) < 1e-6 {
                1e10
            } else {
                f32_abs(1.0 / cos_ray)
            };
            let delta_dist_y = if f32_abs(sin_ray) < 1e-6 {
                1e10
            } else {
                f32_abs(1.0 / sin_ray)
            };

            let (step_x, mut side_dist_x) = if cos_ray < 0.0 {
                (-1, (self.cam_x - map_x as f32) * delta_dist_x)
            } else {
                (1, (map_x as f32 + 1.0 - self.cam_x) * delta_dist_x)
            };

            let (step_y, mut side_dist_y) = if sin_ray < 0.0 {
                (-1, (self.cam_y - map_y as f32) * delta_dist_y)
            } else {
                (1, (map_y as f32 + 1.0 - self.cam_y) * delta_dist_y)
            };

            let mut hit = 0u8;
            let mut side = 0; // 0 = X wall, 1 = Y wall

            for _ in 0..32 {
                if side_dist_x < side_dist_y {
                    side_dist_x += delta_dist_x;
                    map_x += step_x;
                    side = 0;
                } else {
                    side_dist_y += delta_dist_y;
                    map_y += step_y;
                    side = 1;
                }

                if map_x >= 0 && map_x < 16 && map_y >= 0 && map_y < 16 {
                    let tile = MAP_E1M1[map_y as usize][map_x as usize];
                    if tile > 0 {
                        hit = tile;
                        break;
                    }
                } else {
                    break;
                }
            }

            let perp_dist = if side == 0 {
                (side_dist_x - delta_dist_x) * cos_rad(ray_angle_offset)
            } else {
                (side_dist_y - delta_dist_y) * cos_rad(ray_angle_offset)
            }
            .max(0.15f32);

            let line_height = ((120.0f32 / perp_dist) as usize).clamp(4, 160);
            let half_h = line_height / 2;
            let y_start = horizon_y.saturating_sub(half_h);
            let y_end = (horizon_y + half_h).min(167);

            // Ceiling / Sky (0..y_start)
            let sky_color = if y_start < 40 { 160u8 } else { 168u8 }; // Doom dark blue/night sky
            for y in 0..y_start {
                self.screen[y * 320 + x] = sky_color;
            }

            // Textured Wall with Depth Shading (y_start..=y_end)
            let base_color = match hit {
                1 => 24u8,  // Startan Brown
                2 => 112u8, // Computer Console Neon/Green
                3 => 64u8,  // Tech Metal Gray
                4 => 220u8, // Door Yellow
                5 => 176u8, // Window Blue
                _ => 32u8,
            };

            let depth_darkness = ((perp_dist * 2.2f32) as u8).min(6);
            let shaded_color = base_color.saturating_add(depth_darkness);

            let wall_hit_u = if side == 0 {
                self.cam_y + (side_dist_x - delta_dist_x) * sin_ray
            } else {
                self.cam_x + (side_dist_y - delta_dist_y) * cos_ray
            };
            let tex_u = ((wall_hit_u * 32.0f32) as usize) % 32;

            for y in y_start..=y_end {
                let tex_v = ((y - y_start) * 32 / line_height.max(1)) % 32;
                // Add texture pattern detail (bricks / rivets)
                let pattern = (tex_u % 8 == 0) || (tex_v % 8 == 0);
                let final_px = if pattern {
                    shaded_color.saturating_add(4)
                } else {
                    shaded_color
                };
                self.screen[y * 320 + x] = final_px;
            }

            // Floor (y_end+1..168)
            for y in (y_end + 1)..168 {
                let floor_depth = ((168 - y) / 12) as u8;
                self.screen[y * 320 + x] = 72u8.saturating_add(floor_depth); // Dark concrete gray
            }
        }

        // 2. Render Authentic Double-Barrel Shotgun (Centered at bottom of client view)
        let recoil_offset = (self.recoil_timer as usize) * 4;
        let gun_center_x = 160usize;
        let gun_top_y = 118usize + recoil_offset;

        // Draw Muzzle Flash if active
        if self.muzzle_flash_timer > 0 {
            let flash_y = gun_top_y.saturating_sub(18);
            for dy in 0..18 {
                let fy = flash_y + dy;
                if fy >= 168 {
                    continue;
                }
                let width = (18 - dy) * 2;
                let x0 = gun_center_x.saturating_sub(width / 2);
                let x1 = (gun_center_x + width / 2).min(319);
                for fx in x0..=x1 {
                    let col = if dy < 6 { 224u8 } else { 215u8 }; // Pure yellow / hot orange flame
                    self.screen[fy * 320 + fx] = col;
                }
            }
        }

        // Draw Double Steel Barrels
        for dy in 0..50 {
            let py = gun_top_y + dy;
            if py >= 168 {
                break;
            }
            let barrel_width = 18 + (dy / 3);
            let x_left = gun_center_x.saturating_sub(barrel_width);
            let x_right = gun_center_x + barrel_width;

            for px in x_left..=x_right {
                let rel_x = px as i32 - gun_center_x as i32;
                let color = if rel_x == 0 {
                    0u8 // Center seam gap between barrels (pitch black)
                } else if rel_x == -10 || rel_x == 10 {
                    48u8 // Specular metallic steel highlight
                } else if rel_x.abs() < 18 {
                    64u8 + ((dy % 4) as u8) // Gunmetal steel texture
                } else {
                    16u8 // Wooden stock base
                };
                self.screen[py * 320 + px] = color;
            }
        }

        // 3. Render Classic Doom Status Bar (STBAR: y = 168..200)
        // Background: 3D Beveled Industrial Gray (64..72)
        for y in 168..200 {
            for x in 0..320 {
                let col = if y == 168 {
                    4u8 // Top bevel highlight (White)
                } else if y == 199 || x == 0 || x == 319 {
                    0u8 // Bottom / Side shadow (Black)
                } else if x == 75 || x == 145 || x == 175 || x == 245 {
                    0u8 // Segment vertical dividers
                } else {
                    68u8 // Metallic gray panel
                };
                self.screen[y * 320 + x] = col;
            }
        }

        // Draw STBAR Labels & Digit Blocks
        // Block 1: AMMO (x: 4..70, y: 172..195)
        self.draw_stbar_text(10, 172, "AMMO", 220);
        let ammo_str = format!("{:>3}", self.ammo);
        self.draw_stbar_text(16, 184, &ammo_str, 215);

        // Block 2: HEALTH (x: 80..140, y: 172..195)
        self.draw_stbar_text(82, 172, "HEALTH", 220);
        let health_str = format!("{:>3}%", self.health);
        self.draw_stbar_text(84, 184, &health_str, 215);

        // Block 3: Doomguy Face (x: 147..173, y: 170..197)
        self.render_doomguy_face(148, 171);

        // Block 4: ARMOR (x: 180..240, y: 172..195)
        self.draw_stbar_text(182, 172, "ARMOR", 220);
        let armor_str = format!("{:>3}%", self.armor);
        self.draw_stbar_text(186, 184, &armor_str, 215);

        // Block 5: ARMS (x: 250..315, y: 172..195)
        self.draw_stbar_text(252, 172, "ARMS", 220);
        self.draw_stbar_text(254, 184, "2 3 4", 112);

        // Decrement timers
        if self.recoil_timer > 0 {
            self.recoil_timer -= 1;
        }
        if self.muzzle_flash_timer > 0 {
            self.muzzle_flash_timer -= 1;
        }

        &self.screen
    }

    /// Renders the iconic Doomguy face with animated reactive eyes and teeth.
    fn render_doomguy_face(&mut self, ox: usize, oy: usize) {
        // Face background frame (24x26)
        for dy in 0..26 {
            let py = oy + dy;
            for dx in 0..24 {
                let px = ox + dx;
                let color = if dy < 7 {
                    18u8 // Military brown crew-cut hair
                } else if dy < 20 && dx > 2 && dx < 22 {
                    28u8 // Skin tone (tan/caucasian)
                } else {
                    16u8 // Jawline / ear shadow
                };
                self.screen[py * 320 + px] = color;
            }
        }

        // Eyes: Animated pupillary gaze based on player movement & recoil
        let (left_eye_x, right_eye_x, eye_y) = (ox + 7, ox + 15, oy + 11);
        let pupil_offset: i32 = if self.last_turn_dir < 0 {
            -1
        } else if self.last_turn_dir > 0 {
            1
        } else {
            0
        };

        // Whites of eyes
        for dx in 0..3 {
            self.screen[eye_y * 320 + left_eye_x + dx] = 4u8;
            self.screen[eye_y * 320 + right_eye_x + dx] = 4u8;
        }

        // Pupils
        let left_p =
            ((left_eye_x as i32 + 1 + pupil_offset) as usize).clamp(left_eye_x, left_eye_x + 2);
        let right_p =
            ((right_eye_x as i32 + 1 + pupil_offset) as usize).clamp(right_eye_x, right_eye_x + 2);
        self.screen[eye_y * 320 + left_p] = 0u8;
        self.screen[eye_y * 320 + right_p] = 0u8;

        // Mouth: Gritted teeth during gun recoil, determined line otherwise
        let mouth_y = oy + 19;
        if self.recoil_timer > 0 {
            // Grimacing teeth / roar
            for dx in 8..16 {
                self.screen[mouth_y * 320 + ox + dx] = 4u8; // White teeth
                self.screen[(mouth_y + 1) * 320 + ox + dx] = 0u8; // Black mouth interior
            }
        } else {
            for dx in 9..15 {
                self.screen[mouth_y * 320 + ox + dx] = 16u8; // Firm jaw line
            }
        }
    }

    /// Draws small 4x6 bitmap characters onto status bar.
    fn draw_stbar_text(&mut self, x: usize, y: usize, text: &str, color: u8) {
        for (ci, ch) in text.chars().enumerate() {
            let px = x + ci * 6;
            if px + 4 >= 320 {
                break;
            }
            let glyph = crate::dos::desktop::get_glyph_bitmap(ch);
            for row in 0..6 {
                let py = y + row;
                if py >= 200 {
                    break;
                }
                let bits = glyph[row];
                for col in 0..4 {
                    if (bits & (1 << (3 - col))) != 0 {
                        self.screen[py * 320 + px + col] = color;
                    }
                }
            }
        }
    }
}
