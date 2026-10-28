//! # Árvore BSP Tricótoma (`Ternary Binary Space Partitioning`) e Clipper de Oclusão em Trits
//!
//! No Doom clássico (1993), `R_RenderBSPNode` e `R_PointOnSide` dividem o mapa 2D por linhas
//! de partição (`node_t`). Em máquinas binárias, o teste de lado devolve `0` (frente) ou `1` (trás),
//! forçando pontos colineares a cair arbitrariamente em um dos lados.
//!
//! Na arquitetura **Setun-Doom**, o determinante do produto vetorial 2D em [`TernaryFixed`]
//! produz um [`Trit`] natural no acumulador (`ACC`):
//! - [`Trit::Negative`] (`-1`): Câmera está no subespaço **Traseiro** (`Back`).
//! - [`Trit::Zero`] (`0`): Câmera está **Exatamente Colinear** à linha divisora (`On-Plane`).
//! - [`Trit::Positive`] (`+1`): Câmera está no subespaço **Frontal** (`Front`).
//!
//! Além disso, o **Clipper Horizontal de Oclusão (`solidsegs`)** armazena as 320 colunas da tela
//! em **16 palavras [`Word21`]** ($16 \times 21 = 336$ trits), onde cada coluna $x \in [0, 319]$
//! possui 1 trit indicando:
//! - `-1` (`Negative`): Coluna totalmente fechada por parede sólida (oclusa).
//! - `0` (`Zero`): Coluna parcialmente aberta (portal com *upper/lower texture*).
//! - `+1` (`Positive`): Coluna totalmente aberta (céu/vazio).

use crate::fixed::TernaryFixed;
use crate::memory::FB_WIDTH;
use crate::trit::Trit;
use crate::word::{Tryte6, Word21};

/// Identificador de um filho na árvore BSP: pode apontar para outro nó divisor ou para uma folha (Subsector).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BspChild {
    /// Índice de um nó interno na tabela de nós BSP.
    Node(usize),
    /// Índice de uma folha convexa (`Subsector`) contendo segmentos de parede (`Segs`).
    Subsector(usize),
}

/// Linha divisora e metadados de um nó interno da Árvore BSP (`node_t` do Doom adaptado para `TernaryFixed`).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct BspNode {
    /// Coordenada X inicial da linha partidora (`11.10` trits).
    pub x: TernaryFixed,
    /// Coordenada Y inicial da linha partidora (`11.10` trits).
    pub y: TernaryFixed,
    /// Vetor diretor $\Delta X$ da linha partidora (`11.10` trits).
    pub dx: TernaryFixed,
    /// Vetor diretor $\Delta Y$ da linha partidora (`11.10` trits).
    pub dy: TernaryFixed,
    /// Subespaço frontal (`Trit::Positive`).
    pub front_child: BspChild,
    /// Subespaço traseiro (`Trit::Negative`).
    pub back_child: BspChild,
}

impl BspNode {
    /// Determina em qual dos 3 estados espaciais o ponto `(px, py)` está em relação à linha divisora
    /// (`R_PointOnSide` tricótomo).
    ///
    /// Calcula o sinal do produto vetorial 2D:
    /// $$C = (P_x - X) \cdot \Delta Y - (P_y - Y) \cdot \Delta X$$
    /// - Retorna [`Trit::Positive`] (`+1`) se estiver na Frente.
    /// - Retorna [`Trit::Zero`] (`0`) se estiver exatamente sobre a reta divisora.
    /// - Retorna [`Trit::Negative`] (`-1`) se estiver Atrás.
    #[inline(always)]
    pub fn point_on_side(&self, px: TernaryFixed, py: TernaryFixed) -> Trit {
        // Caminhos rápidos para linhas alinhadas aos eixos (otimização clássica de Carmack)
        if self.dx.raw_i64() == 0 {
            let diff_x = px - self.x;
            return if self.dy.raw_i64() > 0 {
                diff_x.sign_trit()
            } else {
                -diff_x.sign_trit()
            };
        }
        if self.dy.raw_i64() == 0 {
            let diff_y = py - self.y;
            return if self.dx.raw_i64() < 0 {
                diff_y.sign_trit()
            } else {
                -diff_y.sign_trit()
            };
        }

        let rel_x = px - self.x;
        let rel_y = py - self.y;
        let left = rel_x * self.dy;
        let right = rel_y * self.dx;
        (left - right).sign_trit()
    }
}

/// Segmento de parede convexo pertencente a um `Subsector` (`seg_t`).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct WallSeg {
    pub id: usize,
    /// Coluna inicial projetada na tela (`0..320`).
    pub screen_x0: usize,
    /// Coluna final (inclusiva) projetada na tela (`0..320`).
    pub screen_x1: usize,
    /// Indica se a parede é sólida (`true`, fecha totalmente a coluna) ou portal/janela (`false`).
    pub solid: bool,
    /// Cor base (`Tryte6`) da parede para rasterização.
    pub color: Tryte6,
    /// Distância perpendicular média para cálculo de altura e iluminação.
    pub distance: TernaryFixed,
}

/// Folha convexa da árvore BSP (`subsector_t`), contendo um intervalo de segmentos de parede.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Subsector {
    pub id: usize,
    pub segs: Vec<WallSeg>,
}

/// Clipper Horizontal de Oclusão em Malha de Trits (`TernaryOcclusionClipper`).
///
/// Armazena o estado das 320 colunas verticais do Framebuffer VGA em 16 palavras [`Word21`]
/// ($16 \times 21 = 336$ trits):
/// - [`Trit::Positive`] (`+1`): Coluna aberta.
/// - [`Trit::Zero`] (`0`): Coluna parcialmente aberta (portal).
/// - [`Trit::Negative`] (`-1`): Coluna totalmente oclusa (parede sólida já desenhada na frente).
pub struct TernaryOcclusionClipper {
    /// 16 palavras de 21 trits cobrindo 336 colunas.
    words: [Word21; 16],
    /// Contagem de colunas que ainda não estão totalmente oclusas (`!= Trit::Negative`).
    open_columns: usize,
}

impl TernaryOcclusionClipper {
    /// Inicializa todas as 320 colunas da tela como totalmente abertas (`Trit::Positive`).
    pub fn new() -> Self {
        let all_open_trits = [Trit::Positive; 21];
        let all_open_word = Word21::from_trits(&all_open_trits);
        Self {
            words: [all_open_word; 16],
            open_columns: FB_WIDTH,
        }
    }

    /// Reinicia o clipper para o início de um novo quadro (`DG_DrawFrame`).
    pub fn clear(&mut self) {
        *self = Self::new();
    }

    /// Retorna `true` se todas as 320 colunas da tela já foram fechadas por paredes sólidas
    /// (permitindo interromper a travessia da árvore BSP imediatamente).
    #[inline(always)]
    pub const fn is_screen_fully_occluded(&self) -> bool {
        self.open_columns == 0
    }

    /// Retorna o estado ternário de visibilidade da coluna `x` (`0..320`).
    #[inline]
    pub fn column_state(&self, x: usize) -> Trit {
        assert!(x < FB_WIDTH);
        let word_idx = x / 21;
        let trit_idx = x % 21;
        let trits = self.words[word_idx].to_trits();
        trits[trit_idx]
    }

    /// Verifica se uma palavra inteira de 21 colunas está totalmente oclusa (`Word21::MIN_VAL`),
    /// descartando 21 pixels de uma só vez em 1 comparação de `i64`.
    #[inline(always)]
    pub const fn is_chunk21_occluded(&self, chunk_idx: usize) -> bool {
        self.words[chunk_idx].as_i64() == Word21::MIN_VAL
    }

    /// Testa e recorta um segmento de parede horizontal `[x0..=x1]`.
    /// Retorna a lista de colunas `x` visíveis que devem ser efetivamente desenhadas,
    /// e atualiza os trits para `Negative` (se `solid == true`) ou `Zero` (se `solid == false`).
    pub fn clip_and_mark_seg(&mut self, x0: usize, x1: usize, solid: bool) -> Vec<usize> {
        let start = x0.min(FB_WIDTH.saturating_sub(1));
        let end = x1.min(FB_WIDTH.saturating_sub(1));
        if start > end || self.is_screen_fully_occluded() {
            return Vec::new();
        }

        let mut visible_cols = Vec::with_capacity(end - start + 1);
        for x in start..=end {
            let word_idx = x / 21;
            let trit_idx = x % 21;
            let mut trits = self.words[word_idx].to_trits();
            if trits[trit_idx] != Trit::Negative {
                visible_cols.push(x);
                if solid {
                    trits[trit_idx] = Trit::Negative;
                    self.open_columns = self.open_columns.saturating_sub(1);
                } else {
                    trits[trit_idx] = Trit::Zero;
                }
                self.words[word_idx] = Word21::from_trits(&trits);
            }
        }
        visible_cols
    }
}

impl Default for TernaryOcclusionClipper {
    fn default() -> Self {
        Self::new()
    }
}

/// Árvore BSP completa contendo nós divisores e subsectores convexos.
pub struct BspTree {
    pub nodes: Vec<BspNode>,
    pub subsectors: Vec<Subsector>,
    pub root: BspChild,
}

impl BspTree {
    /// Percorre a árvore BSP da frente para trás (`Front-to-Back`) a partir da posição da câmera
    /// `(cam_x, cam_y)`, alimentando o clipper ternário de oclusão e invocando `on_visible_seg`
    /// apenas para os fragmentos de parede efetivamente visíveis.
    pub fn traverse_front_to_back<F>(
        &self,
        cam_x: TernaryFixed,
        cam_y: TernaryFixed,
        clipper: &mut TernaryOcclusionClipper,
        mut on_visible_seg: F,
    ) where
        F: FnMut(&WallSeg, &[usize]),
    {
        self.visit_child(self.root, cam_x, cam_y, clipper, &mut on_visible_seg);
    }

    fn visit_child<F>(
        &self,
        child: BspChild,
        cam_x: TernaryFixed,
        cam_y: TernaryFixed,
        clipper: &mut TernaryOcclusionClipper,
        on_visible_seg: &mut F,
    ) where
        F: FnMut(&WallSeg, &[usize]),
    {
        if clipper.is_screen_fully_occluded() {
            return;
        }

        match child {
            BspChild::Subsector(sub_idx) => {
                let sub = &self.subsectors[sub_idx];
                for seg in &sub.segs {
                    if clipper.is_screen_fully_occluded() {
                        break;
                    }
                    let cols = clipper.clip_and_mark_seg(seg.screen_x0, seg.screen_x1, seg.solid);
                    if !cols.is_empty() {
                        on_visible_seg(seg, &cols);
                    }
                }
            }
            BspChild::Node(node_idx) => {
                let node = &self.nodes[node_idx];
                let side = node.point_on_side(cam_x, cam_y);

                // Seleção tricótoma:
                // - Positive (+1) ou Zero (0): visita Front primeiro, depois Back.
                // - Negative (-1): visita Back primeiro, depois Front.
                let (near, far) = match side {
                    Trit::Positive | Trit::Zero => (node.front_child, node.back_child),
                    Trit::Negative => (node.back_child, node.front_child),
                };

                self.visit_child(near, cam_x, cam_y, clipper, on_visible_seg);
                if !clipper.is_screen_fully_occluded() {
                    self.visit_child(far, cam_x, cam_y, clipper, on_visible_seg);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bsp_point_on_side_trichotomy() {
        // Linha divisora vertical em X = 10.0 apontando para +Y (dx = 0, dy = +1)
        let node = BspNode {
            x: TernaryFixed::from_int(10),
            y: TernaryFixed::from_int(0),
            dx: TernaryFixed::ZERO,
            dy: TernaryFixed::ONE,
            front_child: BspChild::Subsector(0),
            back_child: BspChild::Subsector(1),
        };

        // Ponto em X = 25 (Frente -> Positive)
        assert_eq!(
            node.point_on_side(TernaryFixed::from_int(25), TernaryFixed::from_int(5)),
            Trit::Positive
        );
        // Ponto exatamente sobre a linha X = 10 (Colinear -> Zero)
        assert_eq!(
            node.point_on_side(TernaryFixed::from_int(10), TernaryFixed::from_int(99)),
            Trit::Zero
        );
        // Ponto em X = -5 (Trás -> Negative)
        assert_eq!(
            node.point_on_side(TernaryFixed::from_int(-5), TernaryFixed::from_int(5)),
            Trit::Negative
        );
    }

    #[test]
    fn test_front_to_back_bsp_occlusion_culling() {
        // Subsector 0 (Frontal, próximo): parede sólida cobrindo todas as colunas 0..=319
        let near_sub = Subsector {
            id: 0,
            segs: vec![WallSeg {
                id: 10,
                screen_x0: 0,
                screen_x1: 319,
                solid: true,
                color: Tryte6::from_palette_index(120),
                distance: TernaryFixed::from_int(2),
            }],
        };

        // Subsector 1 (Traseiro, distante): parede em 50..=150 que deve ser 100% descartada por oclusão!
        let far_sub = Subsector {
            id: 1,
            segs: vec![WallSeg {
                id: 20,
                screen_x0: 50,
                screen_x1: 150,
                solid: true,
                color: Tryte6::from_palette_index(40),
                distance: TernaryFixed::from_int(12),
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

        let mut clipper = TernaryOcclusionClipper::new();
        let mut drawn_seg_ids = Vec::new();

        // Câmera em X = +5 (lado Frontal -> visita Subsector 0 primeiro, fecha as 320 colunas e poda Subsector 1)
        tree.traverse_front_to_back(
            TernaryFixed::from_int(5),
            TernaryFixed::ZERO,
            &mut clipper,
            |seg, cols| {
                drawn_seg_ids.push((seg.id, cols.len()));
            },
        );

        assert!(clipper.is_screen_fully_occluded());
        assert_eq!(drawn_seg_ids, vec![(10, 320)]);
    }
}
