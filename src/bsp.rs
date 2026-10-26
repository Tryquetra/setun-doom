//! # Trichotomous BSP Tree (`Ternary Binary Space Partitioning`) and Trit Occlusion Clipper
//!
//! In classic Doom (1993), `R_RenderBSPNode` and `R_PointOnSide` partition the 2D map via partition
//! lines (`node_t`). On binary hardware, the side test returns `0` (front) or `1` (back),
//! forcing collinear points onto an arbitrary side.
//!
//! In the **Setun-Doom** architecture, the 2D cross product determinant in [`TernaryFixed`]
//! yields a natural [`Trit`] in the accumulator (`ACC`):
//! - [`Trit::Negative`] (`-1`): Camera is in the **Back** subspace.
//! - [`Trit::Zero`] (`0`): Camera is **Collinear** with the partition line (`On-Plane`).
//! - [`Trit::Positive`] (`+1`): Camera is in the **Front** subspace.
//!
//! Additionally, the **Horizontal Occlusion Clipper (`solidsegs`)** stores the 320 screen columns
//! in **16 [`Word21`] words** ($16 \times 21 = 336$ trits), where each column $x \in [0, 319]$
//! has 1 trit indicating:
//! - `-1` (`Negative`): Column fully sealed by a solid wall (occluded).
//! - `0` (`Zero`): Column partially open (portal with upper or lower texture).
//! - `+1` (`Positive`): Column fully open (sky or unrendered background).

use crate::fixed::TernaryFixed;
use crate::memory::FB_WIDTH;
use crate::trit::Trit;
use crate::word::{Word21, Tryte6};

/// Identifier of a child in the BSP tree: points to either an internal splitter node or a leaf (Subsector).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BspChild {
    /// Index of an internal node in the BSP node table.
    Node(usize),
    /// Index of a convex leaf (`Subsector`) containing wall segments (`Segs`).
    Subsector(usize),
}

/// Partition line and metadata of an internal BSP tree node (Doom `node_t` adapted for `TernaryFixed`).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct BspNode {
    /// Initial X coordinate of partition line (`11.10` trits).
    pub x: TernaryFixed,
    /// Initial Y coordinate of partition line (`11.10` trits).
    pub y: TernaryFixed,
    /// Direction vector $\Delta X$ of partition line (`11.10` trits).
    pub dx: TernaryFixed,
    /// Direction vector $\Delta Y$ of partition line (`11.10` trits).
    pub dy: TernaryFixed,
    /// Front subspace (`Trit::Positive`).
    pub front_child: BspChild,
    /// Back subspace (`Trit::Negative`).
    pub back_child: BspChild,
}

impl BspNode {
    /// Determines which of the 3 spatial states the point `(px, py)` occupies relative to partition line
    /// (trichotomous `R_PointOnSide`).
    ///
    /// Computes the sign of the 2D cross product:
    /// $$C = (P_x - X) \cdot \Delta Y - (P_y - Y) \cdot \Delta X$$
    /// - Returns [`Trit::Positive`] (`+1`) if in Front.
    /// - Returns [`Trit::Zero`] (`0`) if exactly collinear on the partition line.
    /// - Returns [`Trit::Negative`] (`-1`) if in Back.
    #[inline(always)]
    pub fn point_on_side(&self, px: TernaryFixed, py: TernaryFixed) -> Trit {
        // Fast paths for axis-aligned lines (Carmack's classic optimization)
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

/// Convex wall segment belonging to a `Subsector` (`seg_t`).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct WallSeg {
    pub id: usize,
    /// Starting screen column projected on display (`0..320`).
    pub screen_x0: usize,
    /// Ending screen column (inclusive) projected on display (`0..320`).
    pub screen_x1: usize,
    /// Indicates whether wall is solid (`true`, fully seals column) or portal/window (`false`).
    pub solid: bool,
    /// Base color (`Tryte6`) of wall for rasterization.
    pub color: Tryte6,
    /// Mean perpendicular distance for height projection and depth shading.
    pub distance: TernaryFixed,
}

/// Convex leaf of the BSP tree (`subsector_t`), containing a span of wall segments.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Subsector {
    pub id: usize,
    pub segs: Vec<WallSeg>,
}

/// Horizontal Occlusion Clipper over Trit Mesh (`TernaryOcclusionClipper`).
///
/// Stores state of all 320 vertical VGA framebuffer columns across 16 [`Word21`] words
/// ($16 \times 21 = 336$ trits):
/// - [`Trit::Positive`] (`+1`): Open column.
/// - [`Trit::Zero`] (`0`): Partially open column (portal).
/// - [`Trit::Negative`] (`-1`): Fully occluded column (solid wall already rendered in front).
pub struct TernaryOcclusionClipper {
    /// 16 words of 21 trits covering 336 columns.
    words: [Word21; 16],
    /// Count of columns that are not fully occluded (`!= Trit::Negative`).
    open_columns: usize,
}

impl TernaryOcclusionClipper {
    /// Initializes all 320 screen columns as fully open (`Trit::Positive`).
    pub fn new() -> Self {
        let all_open_trits = [Trit::Positive; 21];
        let all_open_word = Word21::from_trits(&all_open_trits);
        Self {
            words: [all_open_word; 16],
            open_columns: FB_WIDTH,
        }
    }

    /// Resets the clipper for the start of a new frame (`DG_DrawFrame`).
    pub fn clear(&mut self) {
        *self = Self::new();
    }

    /// Returns `true` if all 320 screen columns are sealed by solid walls
    /// (allowing BSP traversal to terminate early).
    #[inline(always)]
    pub const fn is_screen_fully_occluded(&self) -> bool {
        self.open_columns == 0
    }

    /// Returns the ternary visibility state of column `x` (`0..320`).
    #[inline]
    pub fn column_state(&self, x: usize) -> Trit {
        assert!(x < FB_WIDTH);
        let word_idx = x / 21;
        let trit_idx = x % 21;
        let trits = self.words[word_idx].to_trits();
        trits[trit_idx]
    }

    /// Verifies if an entire chunk of 21 columns is fully occluded (`Word21::MIN_VAL`),
    /// culling 21 pixels simultaneously with a single `i64` comparison.
    #[inline(always)]
    pub const fn is_chunk21_occluded(&self, chunk_idx: usize) -> bool {
        self.words[chunk_idx].as_i64() == Word21::MIN_VAL
    }

    /// Tests and clips a horizontal wall segment `[x0..=x1]`.
    /// Returns the list of visible column indices `x` to be rasterized,
    /// updating trits to `Negative` (if `solid == true`) or `Zero` (if `solid == false`).
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

/// Complete BSP tree containing splitter nodes and convex subsectors.
pub struct BspTree {
    pub nodes: Vec<BspNode>,
    pub subsectors: Vec<Subsector>,
    pub root: BspChild,
}

impl BspTree {
    /// Traverses the BSP tree in Front-to-Back order starting from camera position
    /// `(cam_x, cam_y)`, updating the ternary occlusion clipper and invoking `on_visible_seg`
    /// only for visible wall fragments.
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

                // Trichotomous dispatch:
                // - Positive (+1) or Zero (0): visit Front first, then Back.
                // - Negative (-1): visit Back first, then Front.
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
        // Vertical partition line at X = 10.0 pointing toward +Y (dx = 0, dy = +1)
        let node = BspNode {
            x: TernaryFixed::from_int(10),
            y: TernaryFixed::from_int(0),
            dx: TernaryFixed::ZERO,
            dy: TernaryFixed::ONE,
            front_child: BspChild::Subsector(0),
            back_child: BspChild::Subsector(1),
        };

        // Point at X = 25 (Front -> Positive)
        assert_eq!(
            node.point_on_side(TernaryFixed::from_int(25), TernaryFixed::from_int(5)),
            Trit::Positive
        );
        // Point collinear with line X = 10 (Collinear -> Zero)
        assert_eq!(
            node.point_on_side(TernaryFixed::from_int(10), TernaryFixed::from_int(99)),
            Trit::Zero
        );
        // Point at X = -5 (Back -> Negative)
        assert_eq!(
            node.point_on_side(TernaryFixed::from_int(-5), TernaryFixed::from_int(5)),
            Trit::Negative
        );
    }

    #[test]
    fn test_front_to_back_bsp_occlusion_culling() {
        // Subsector 0 (Front, near): solid wall covering all columns 0..=319
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

        // Subsector 1 (Back, far): wall at 50..=150 that must be 100% culled by occlusion
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

        // Camera at X = +5 (Front side -> visits Subsector 0 first, seals 320 columns and culls Subsector 1)
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
