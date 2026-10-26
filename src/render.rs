//! # Rasterizador de Colunas (`R_DrawColumn`), Pisos (`R_DrawSpan`) e Cena BSP 3D
//!
//! Implementa os loops internos do motor gráfico do Doom utilizando:
//! - Interpolação de coordenadas de textura em ponto fixo ternário `11.10` ([`TernaryFixed`]).
//! - Atenuação de iluminação por deslocamento de trits sem tabelas `COLORMAP`.
//! - Travessia da árvore BSP da frente para trás ([`BspTree`]) combinada com o
//!   [`TernaryOcclusionClipper`] para renderizar setores 3D diretamente no Framebuffer VGA (`320x200`).

use crate::bsp::{BspTree, TernaryOcclusionClipper, WallSeg};
use crate::fixed::TernaryFixed;
use crate::memory::{Memory, FB_HEIGHT, FB_WIDTH};
use crate::word::Tryte6;

/// Parâmetros para desenhar uma fatia vertical de parede texturizada (`R_DrawColumn` do Doom).
#[derive(Copy, Clone, Debug)]
pub struct ColumnDrawParams<'a> {
    /// Coluna X na tela (`0..320`).
    pub x: usize,
    /// Linha inicial Y no topo da coluna (`0..200`).
    pub yl: usize,
    /// Linha final Y na base da coluna (`0..200`).
    pub yh: usize,
    /// Coordenada vertical inicial na textura em ponto fixo ternário `11.10`.
    pub tex_frac_start: TernaryFixed,
    /// Passo de incremento de textura por pixel vertical (`fracstep` em `11.10`).
    pub tex_step: TernaryFixed,
    /// Textura vertical de 64 texels (`Tryte6`).
    pub texture_column: &'a [Tryte6; 64],
    /// Atenuação de luz ternária (`-128..+127`).
    pub light_shift: i16,
}

/// Renderizador 3D ternário para paredes, tetos e pisos do Setun-Doom.
pub struct TernaryRenderer;

impl TernaryRenderer {
    /// Equivalente ternário de `R_DrawColumn`: desenha uma fatia vertical de parede na coluna `x`
    /// entre `yl..=yh` interpolando a coordenada `frac` em [`TernaryFixed`] (`11.10` trits).
    pub fn draw_column(mem: &mut Memory, params: &ColumnDrawParams<'_>) {
        if params.x >= FB_WIDTH || params.yl > params.yh || params.yl >= FB_HEIGHT {
            return;
        }
        let y_end = params.yh.min(FB_HEIGHT - 1);
        let mut frac = params.tex_frac_start;

        for y in params.yl..=y_end {
            // Extrai o índice do texel usando a parte inteira balanceada módulo 64
            let tex_idx = frac.to_int_balanced().rem_euclid(64) as usize;
            let base_color = params.texture_column[tex_idx];

            // Iluminação direta por deslocamento simétrico no Tryte6
            let lit_val = (base_color.as_i16() + params.light_shift).clamp(-128, 127);
            let final_color = Tryte6::from_i64(lit_val as i64);

            let _ = mem.write_pixel_tryte(params.x, y, final_color);
            frac += params.tex_step;
        }
    }

    /// Equivalente ternário de `R_DrawSpan`: desenha uma linha horizontal de chão ou teto (`y`)
    /// entre `x1..=x2` interpolando coordenadas 2D `(u, v)` em [`TernaryFixed`].
    pub fn draw_span(
        mem: &mut Memory,
        y: usize,
        x1: usize,
        x2: usize,
        mut u: TernaryFixed,
        mut v: TernaryFixed,
        du: TernaryFixed,
        dv: TernaryFixed,
        flat_tex: &[Tryte6; 4096], // Flat 64x64 padrão do Doom
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

    /// Renderiza uma cena 3D completa percorrendo a [`BspTree`] da frente para trás a partir de
    /// `(cam_x, cam_y)`, descartando paredes ocultas com [`TernaryOcclusionClipper`] e projetando
    /// teto, parede texturizada e piso para cada coluna visível.
    /// Retorna a quantidade total de segmentos de parede desenhados e colunas preenchidas.
    pub fn render_bsp_scene(
        mem: &mut Memory,
        tree: &BspTree,
        cam_x: TernaryFixed,
        cam_y: TernaryFixed,
    ) -> (usize, usize) {
        let mut clipper = TernaryOcclusionClipper::new();
        let mut segs_drawn = 0usize;
        let mut cols_drawn = 0usize;

        // Gera uma textura procedural de tijolos/metal de 64 texels em Tryte6
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

            // Calcula meia-altura aparente da parede na tela: half_h = 80 / distance
            let dist_int = seg.distance.to_int_balanced().max(1) as usize;
            let half_h = (80 / dist_int).clamp(8, 95);
            let center_y = FB_HEIGHT / 2;
            let yl = center_y.saturating_sub(half_h);
            let yh = (center_y + half_h).min(FB_HEIGHT - 1);

            let wall_height_px = (yh - yl + 1).max(1) as i64;
            let tex_step = TernaryFixed::from_int(64) / TernaryFixed::from_int(wall_height_px);
            let light_shift = -(dist_int as i16 * 4);

            for &x in visible_cols {
                // Preenche teto acima da parede
                for y in 0..yl {
                    let _ = mem.write_pixel_tryte(x, y, ceiling_color);
                }
                // Desenha a fatia vertical da parede com interpolador de ponto fixo 11.10
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
                // Preenche piso abaixo da parede
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

        // Verifica que o teto (y=5), a parede central (y=100) e o chão (y=195) receberam Tryte6 distintos
        let px_ceil = mem.read_pixel_tryte(160, 5).unwrap();
        let px_wall = mem.read_pixel_tryte(160, 100).unwrap();
        let px_floor = mem.read_pixel_tryte(160, 195).unwrap();
        assert_ne!(px_ceil, px_wall);
        assert_ne!(px_wall, px_floor);
    }
}
