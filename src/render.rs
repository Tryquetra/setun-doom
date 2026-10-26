//! # Column Rasterizer (`R_DrawColumn`), Spans (`R_DrawSpan`), and 3D BSP Scene Renderer
//!
//! Implements internal loops of the Doom graphics engine utilizing:
//! - Texture coordinate interpolation in 11.10 balanced ternary fixed-point ([`TernaryFixed`]).
//! - Shading attenuation via trit shifts without binary `COLORMAP` tables.
//! - Front-to-back BSP tree traversal ([`BspTree`]) integrated with
//!   [`TernaryOcclusionClipper`] to render 3D sectors directly into the 320x200 VGA framebuffer.

use crate::bsp::{BspTree, TernaryOcclusionClipper, WallSeg};
use crate::fixed::TernaryFixed;
use crate::memory::{Memory, FB_HEIGHT, FB_WIDTH};
use crate::word::Tryte6;

/// Parameters for rendering a vertical textured wall slice (Doom `R_DrawColumn`).
#[derive(Copy, Clone, Debug)]
pub struct ColumnDrawParams<'a> {
    /// Screen column X (`0..320`).
    pub x: usize,
    /// Starting scanline Y at top of column (`0..200`).
    pub yl: usize,
    /// Ending scanline Y at base of column (`0..200`).
    pub yh: usize,
    /// Initial vertical texture coordinate in 11.10 balanced ternary fixed point.
    pub tex_frac_start: TernaryFixed,
    /// Texture coordinate step per vertical pixel (`fracstep` in 11.10).
    pub tex_step: TernaryFixed,
    /// Vertical texture column of 64 texels (`Tryte6`).
    pub texture_column: &'a [Tryte6; 64],
    /// Ternary lighting shift (`-128..+127`).
    pub light_shift: i16,
}

/// Ternary 3D renderer for walls, ceilings, and floors in Setun-Doom.
pub struct TernaryRenderer;

impl TernaryRenderer {
    /// Ternary equivalent of `R_DrawColumn`: renders vertical wall column `x`
    /// between `yl..=yh` interpolating coordinate `frac` in [`TernaryFixed`] (`11.10` trits).
    pub fn draw_column(mem: &mut Memory, params: &ColumnDrawParams<'_>) {
        if params.x >= FB_WIDTH || params.yl > params.yh || params.yl >= FB_HEIGHT {
            return;
        }
        let y_end = params.yh.min(FB_HEIGHT - 1);
        let mut frac = params.tex_frac_start;

        for y in params.yl..=y_end {
            // Extracts texel index using balanced integer part modulo 64
            let tex_idx = frac.to_int_balanced().rem_euclid(64) as usize;
            let base_color = params.texture_column[tex_idx];

            // Direct illumination by symmetric displacement on Tryte6
            let lit_val = (base_color.as_i16() + params.light_shift).clamp(-128, 127);
            let final_color = Tryte6::from_i64(lit_val as i64);

            let _ = mem.write_pixel_tryte(params.x, y, final_color);
            frac += params.tex_step;
        }
    }

    /// Ternary equivalent of `R_DrawSpan`: renders horizontal floor or ceiling span (`y`)
    /// between `x1..=x2` interpolating 2D coordinates `(u, v)` in [`TernaryFixed`].
    pub fn draw_span(
        mem: &mut Memory,
        y: usize,
        x1: usize,
        x2: usize,
        mut u: TernaryFixed,
        mut v: TernaryFixed,
        du: TernaryFixed,
        dv: TernaryFixed,
        flat_tex: &[Tryte6; 4096], // Standard 64x64 Doom flat
    ) {
        if y >= FB_HEIGHT || x1 > x2 || x1 >= FB_WIDTH {
            return;
        }
        let x_end = x2.min(FB_WIDTH - 1);
        for x in x1..=x_end {
            let tx = (u.to_int_balanced().rem_euclid(64)) as usize;
            let ty = (v.to_int_balanced().rem_euclid(64)) as usize;
            let color = flat_tex[ty * 64 + tx];
            let _ = mem.write_pixel_tryte(x, y, color);
            u += du;
            v += dv;
        }
    }

    /// Renders a full 3D scene traversing the [`BspTree`] Front-to-Back from
    /// `(cam_x, cam_y)`, culling occluded walls with [`TernaryOcclusionClipper`] and projecting
    /// ceiling, textured wall, and floor for each visible column.
    /// Returns total wall segments drawn and column pixels rendered.
    pub fn render_bsp_scene(
        mem: &mut Memory,
        tree: &BspTree,
        cam_x: TernaryFixed,
        cam_y: TernaryFixed,
    ) -> (usize, usize) {
        let mut clipper = TernaryOcclusionClipper::new();
        let mut segs_drawn = 0usize;
        let mut cols_drawn = 0usize;

        // Generates procedural brick/metal texture of 64 texels in Tryte6
        let mut brick_tex = [Tryte6::ZERO; 64];
        for (i, slot) in brick_tex.iter_mut().enumerate() {
            let mortar = (i % 16 == 0) || (i % 8 == 0 && (i / 8) % 2 == 1);
            let shade = if mortar { 40u8 } else { 160u8.wrapping_add((i as u8) * 2) };
            *slot = Tryte6::from_palette_index(shade);
        }

        let ceiling_color = Tryte6::from_palette_index(24);
        let floor_color = Tryte6::from_palette_index(72);

        tree.traverse_front_to_back(cam_x, cam_y, &mut clipper, |seg: &WallSeg, visible_cols: &[usize]| {
            segs_drawn += 1;
            cols_drawn += visible_cols.len();

            // Computes apparent half-height of wall on screen: half_h = 80 / distance
            let dist_int = seg.distance.to_int_balanced().max(1) as usize;
            let half_h = (80 / dist_int).clamp(8, 95);
            let center_y = FB_HEIGHT / 2;
            let yl = center_y.saturating_sub(half_h);
            let yh = (center_y + half_h).min(FB_HEIGHT - 1);

            let wall_height_px = (yh - yl + 1).max(1) as i64;
            let tex_step = TernaryFixed::from_int(64) / TernaryFixed::from_int(wall_height_px);
            let light_shift = -(dist_int as i16 * 4);

            for &x in visible_cols {
                // Fills ceiling above wall
                for y in 0..yl {
                    let _ = mem.write_pixel_tryte(x, y, ceiling_color);
                }
                // Renders vertical wall column with 11.10 fixed-point interpolator
                let col_params = ColumnDrawParams {
                    x,
                    yl,
                    yh,
                    tex_frac_start: TernaryFixed::from_int((x % 16) as i64),
                    tex_step,
                    texture_column: &brick_tex,
                    light_shift,
                };
                Self::draw_column(mem, &col_params);
                // Fills floor below wall
                for y in (yh + 1)..FB_HEIGHT {
                    let _ = mem.write_pixel_tryte(x, y, floor_color);
                }
            }
        });

        (segs_drawn, cols_drawn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bsp::{BspChild, BspNode, Subsector};

    #[test]
    fn test_draw_column_and_bsp_scene_rendering() {
        let mut mem = Memory::new();
        let near_sub = Subsector {
            id: 0,
            segs: vec![WallSeg {
                id: 1,
                screen_x0: 0,
                screen_x1: 319,
                solid: true,
                color: Tryte6::from_palette_index(180),
                distance: TernaryFixed::from_int(2),
            }],
        };
        let far_sub = Subsector {
            id: 1,
            segs: vec![WallSeg {
                id: 2,
                screen_x0: 0,
                screen_x1: 319,
                solid: true,
                color: Tryte6::from_palette_index(60),
                distance: TernaryFixed::from_int(8),
            }],
        };

        let tree = BspTree {
            nodes: vec![BspNode {
                x: TernaryFixed::ZERO,
                y: TernaryFixed::ZERO,
                dx: TernaryFixed::ZERO,
                dy: TernaryFixed::ONE,
                front_child: BspChild::Subsector(0),
                back_child: BspChild::Subsector(1),
            }],
            subsectors: vec![near_sub, far_sub],
            root: BspChild::Node(0),
        };

        let (segs, cols) = TernaryRenderer::render_bsp_scene(
            &mut mem,
            &tree,
            TernaryFixed::from_int(4),
            TernaryFixed::ZERO,
        );

        assert_eq!(segs, 1);
        assert_eq!(cols, 320);

        // Verifies that ceiling (y=5), wall (y=100), and floor (y=195) receive distinct Tryte6 values
        let px_ceil = mem.read_pixel_tryte(160, 5).unwrap();
        let px_wall = mem.read_pixel_tryte(160, 100).unwrap();
        let px_floor = mem.read_pixel_tryte(160, 195).unwrap();
        assert_ne!(px_ceil, px_wall);
        assert_ne!(px_wall, px_floor);
    }
}
