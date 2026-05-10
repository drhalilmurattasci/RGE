//! `brep-render` — CPU-side mesh-conversion substrate.
//!
//! Failure class: snapshot-recoverable
//!
//! Flat-shaded vertex-tripled buffer triple → [`RenderMesh`]. No GPU dep;
//! downstream consumers (gfx, future renderer) upload the vertex arrays
//! themselves.
//!
//! # Layering — buffer-typed input (LOAD-BEARING)
//!
//! `brep-render` is in the `RENDERER_CRATES` set under PLAN.md §1.3 rule 6
//! (forbidden-dep): renderer crates cannot depend on game-domain crates,
//! and `rge-cad-core` is game-domain (`rge-cad-` prefix). The substrate
//! therefore consumes raw `&[[f32; 3]]` positions, `&[u32]` indices and an
//! optional opaque `&[u64]` per-triangle face-label slice — NOT
//! `rge_cad_core::Tessellation` directly. A caller in `cad-projection` (or
//! similar Tier-2 bridge crate) that owns both `Tessellation` and a
//! `RenderMesh` consumer composes the conversion in two lines:
//!
//! ```ignore
//! let labels: Option<Vec<u64>> = tess
//!     .face_labels
//!     .as_ref()
//!     .map(|v| v.iter().map(|t| t.0).collect());
//! let mesh = RenderMesh::from_buffers(
//!     &tess.positions,
//!     &tess.indices,
//!     labels.as_deref(),
//! );
//! ```
//!
//! `TopologyFaceId` is a `pub u64` newtype wrapper so `face_id.0` is a
//! lossless u64 round-trip. Downstream bookkeeping that needs strong typing
//! continues to use `TopologyFaceId`; the renderer-tier substrate sees
//! opaque `u64` tags.
//!
//! # Shading model (LOAD-BEARING)
//!
//! CAD geometry is flat-shaded at face boundaries — normal smoothing
//! across edges would visually erase face structure and break
//! face-identity selection feedback later. Vertex-tripling is the
//! v0 implementation: each input triangle becomes 3 independent
//! output vertices carrying the triangle's face normal. Trade-off:
//! 3× vertex count vs. input, no shared-vertex optimization.
//!
//! # Index validity
//!
//! The caller is expected to pass index buffers that are already validated
//! (every index < `positions.len()`, `indices.len() % 3 == 0`). The
//! upstream `Tessellation::new` / `Tessellation::with_labels` constructors
//! enforce these invariants on cad-core's side. `from_buffers` panics on
//! out-of-bounds indices or a non-multiple-of-3 index count — both
//! indicate a caller bug.

#![forbid(unsafe_code)]

/// Tolerance below which a triangle's `(edge1 × edge2)` magnitude is
/// treated as zero-area (degenerate) and assigned a `[0, 0, 0]` normal
/// rather than NaN. Matches `cad-projection` picking's `EPSILON` for
/// consistency across the chapter.
const DEGENERATE_AREA_EPS: f32 = 1e-6;

/// Render-ready flat-shaded mesh.
///
/// Each input triangle becomes 3 independent output vertices with the
/// triangle's face normal. `face_labels` is **one-per-triangle** (NOT
/// per-vertex), mirroring the input shape exactly.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderMesh {
    /// 3 vertices per input triangle (no dedup, no shared vertices).
    pub positions: Vec<[f32; 3]>,
    /// One normal per vertex; the 3 vertices of each triangle share
    /// the triangle's face normal (flat shading).
    pub normals: Vec<[f32; 3]>,
    /// Triangle indices: dense `[0, 1, 2, 3, ..., 3n-1]` for `n`
    /// triangles (always `3*i, 3*i+1, 3*i+2` for triangle `i`).
    pub indices: Vec<u32>,
    /// One per **input triangle** (NOT per output vertex). Opaque
    /// `u64` per-triangle tag (semantically `TopologyFaceId.0` from
    /// cad-core; see module-level docs for the layering rationale).
    /// Matches the input slice's shape exactly when present; `None`
    /// when the input is unlabeled.
    pub face_labels: Option<Vec<u64>>,
}

// ---------------------------------------------------------------------------
// `[f32; 3]` math — raw helpers (no glam dep). Style mirrors
// `cad-projection::picking`'s `sub` / `cross` / `dot` precedent.
// ---------------------------------------------------------------------------

/// Subtract two `[f32; 3]` vectors component-wise.
#[inline]
fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// Cross-product of two `[f32; 3]` vectors.
#[inline]
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Dot-product of two `[f32; 3]` vectors.
#[inline]
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Length of a `[f32; 3]` vector.
#[inline]
fn length(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}

impl RenderMesh {
    /// Build a flat-shaded [`RenderMesh`] from raw triangle-soup buffers.
    ///
    /// - Triangle count = `indices.len() / 3`.
    /// - Vertex count = `3 * triangle_count` (vertex tripling).
    /// - Per-triangle normal: `(v1-v0) × (v2-v0)` then normalized;
    ///   degenerate triangles (`|cross| < DEGENERATE_AREA_EPS`)
    ///   produce `[0.0, 0.0, 0.0]` instead of NaN.
    /// - `face_labels = face_labels_in.map(<slice>::to_vec)` — preserved
    ///   1:1 in order.
    /// - Output `indices` are dense `[0, 1, 2, ..., 3n-1]`.
    ///
    /// # Panics
    ///
    /// Panics if `indices.len()` is not a multiple of 3, or if any index
    /// references an out-of-bounds position. Both indicate a caller
    /// contract violation (see module-level docs).
    #[must_use]
    pub fn from_buffers(
        positions: &[[f32; 3]],
        indices: &[u32],
        face_labels: Option<&[u64]>,
    ) -> Self {
        assert!(
            indices.len() % 3 == 0,
            "RenderMesh::from_buffers: indices.len() must be a multiple of 3 (got {})",
            indices.len()
        );

        let triangle_count = indices.len() / 3;
        let vertex_count = triangle_count * 3;

        let mut out_positions: Vec<[f32; 3]> = Vec::with_capacity(vertex_count);
        let mut out_normals: Vec<[f32; 3]> = Vec::with_capacity(vertex_count);
        let mut out_indices: Vec<u32> = Vec::with_capacity(vertex_count);

        for (tri_idx, tri) in indices.chunks_exact(3).enumerate() {
            let i0 = tri[0] as usize;
            let i1 = tri[1] as usize;
            let i2 = tri[2] as usize;
            // Trust the caller's validated invariants; OOB indicates a bug.
            let v0 = positions[i0];
            let v1 = positions[i1];
            let v2 = positions[i2];

            let edge1 = sub(v1, v0);
            let edge2 = sub(v2, v0);
            let cross_v = cross(edge1, edge2);
            let len = length(cross_v);
            let normal = if len < DEGENERATE_AREA_EPS {
                [0.0_f32, 0.0, 0.0]
            } else {
                [cross_v[0] / len, cross_v[1] / len, cross_v[2] / len]
            };

            out_positions.push(v0);
            out_positions.push(v1);
            out_positions.push(v2);
            out_normals.push(normal);
            out_normals.push(normal);
            out_normals.push(normal);
            let base = (tri_idx * 3) as u32;
            out_indices.push(base);
            out_indices.push(base + 1);
            out_indices.push(base + 2);
        }

        // 1:1 preservation of the caller's per-triangle tag slice — same
        // length, same values, same order. DO NOT reorder / filter / skip.
        let out_labels: Option<Vec<u64>> = face_labels.map(<[u64]>::to_vec);

        Self {
            positions: out_positions,
            normals: out_normals,
            indices: out_indices,
            face_labels: out_labels,
        }
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Empty input → empty output (no panic).
    #[test]
    fn empty_tessellation_yields_empty_render_mesh() {
        let positions: Vec<[f32; 3]> = Vec::new();
        let indices: Vec<u32> = Vec::new();
        let mesh = RenderMesh::from_buffers(&positions, &indices, None);
        assert!(mesh.positions.is_empty());
        assert!(mesh.normals.is_empty());
        assert!(mesh.indices.is_empty());
        assert_eq!(mesh.face_labels, None);
    }

    /// CCW XY-plane unit triangle → outward normal `+Z`. Vertex tripling
    /// produces 3 positions / 3 normals / dense `[0, 1, 2]` indices.
    #[test]
    fn single_triangle_yields_3_vertices_with_face_normal() {
        let positions = vec![[0.0_f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let indices = vec![0_u32, 1, 2];
        let labels = vec![42_u64];
        let mesh = RenderMesh::from_buffers(&positions, &indices, Some(&labels));
        assert_eq!(mesh.positions.len(), 3);
        assert_eq!(mesh.normals.len(), 3);
        assert_eq!(mesh.indices, vec![0_u32, 1, 2]);
        // Outward normal +Z (CCW from above).
        for n in &mesh.normals {
            assert!((n[0] - 0.0).abs() < 1e-5);
            assert!((n[1] - 0.0).abs() < 1e-5);
            assert!((n[2] - 1.0).abs() < 1e-5);
        }
        assert_eq!(mesh.face_labels, Some(vec![42_u64]));
    }

    /// Cuboid input (8 verts + 12 triangles per the canonical
    /// `CuboidOp::evaluate` shape; cf. `cad-core` mesh.rs::valid_box_constructs)
    /// → 36 positions / 36 normals / 36 indices / 12 face_labels.
    #[test]
    fn cuboid_yields_36_vertices_for_12_triangles() {
        let (positions, indices, labels) = synthetic_cuboid_buffers();
        let mesh = RenderMesh::from_buffers(&positions, &indices, Some(&labels));
        assert_eq!(mesh.positions.len(), 36);
        assert_eq!(mesh.normals.len(), 36);
        assert_eq!(mesh.indices.len(), 36);
        assert_eq!(mesh.face_labels.as_ref().expect("labeled").len(), 12);
    }

    /// Group RenderMesh triangles by `face_labels[i] == 1` (PosZ in
    /// CuboidOp's canonical NegZ→PosZ→NegY→PosY→NegX→PosX order). All 2
    /// triangles in that group must share normal `(0, 0, +1)` within
    /// tolerance.
    #[test]
    fn cuboid_pos_z_face_triangles_have_unit_zhat_normal() {
        let (positions, indices, labels) = synthetic_cuboid_buffers();
        let mesh = RenderMesh::from_buffers(&positions, &indices, Some(&labels));
        let labels_out = mesh.face_labels.as_ref().expect("labeled");
        let mut count = 0_usize;
        for (tri_idx, label) in labels_out.iter().enumerate() {
            if *label == 1 {
                let n0 = mesh.normals[tri_idx * 3];
                let n1 = mesh.normals[tri_idx * 3 + 1];
                let n2 = mesh.normals[tri_idx * 3 + 2];
                for n in [n0, n1, n2] {
                    assert!((n[0] - 0.0).abs() < 1e-5);
                    assert!((n[1] - 0.0).abs() < 1e-5);
                    assert!((n[2] - 1.0).abs() < 1e-5);
                }
                count += 1;
            }
        }
        assert_eq!(count, 2, "PosZ face must have exactly 2 triangles");
    }

    /// Every input face_label is preserved exactly — same length, same
    /// values, same order (NO reorder / filter / skip).
    #[test]
    fn face_labels_preserved_one_per_triangle_in_order() {
        let (positions, indices, _) = synthetic_cuboid_buffers();
        let labels: Vec<u64> = (0..12_u64).map(|i| i / 2).collect();
        let mesh = RenderMesh::from_buffers(&positions, &indices, Some(&labels));
        assert_eq!(mesh.face_labels.as_ref().expect("labeled"), &labels);
    }

    /// Unlabeled input → unlabeled output. Buffer-shape tests still pass.
    #[test]
    fn face_labels_none_when_input_unlabeled() {
        let (positions, indices, _) = synthetic_cuboid_buffers();
        let mesh = RenderMesh::from_buffers(&positions, &indices, None);
        assert_eq!(mesh.face_labels, None);
        assert_eq!(mesh.positions.len(), 36);
        assert_eq!(mesh.normals.len(), 36);
        assert_eq!(mesh.indices.len(), 36);
    }

    /// Coincident vertices → degenerate triangle → zero normal, NOT NaN.
    /// Every component must be finite.
    #[test]
    fn degenerate_triangle_produces_zero_normal_not_nan() {
        let positions = vec![[0.0_f32, 0.0, 0.0], [0.0, 0.0, 0.0], [1.0, 0.0, 0.0]];
        let indices = vec![0_u32, 1, 2];
        let mesh = RenderMesh::from_buffers(&positions, &indices, None);
        for n in &mesh.normals {
            assert!(
                n.iter().all(|c| c.is_finite()),
                "degenerate normal must not produce NaN; got {n:?}"
            );
            for c in n {
                assert!((c - 0.0).abs() < 1e-7, "expected zero normal, got {n:?}");
            }
        }
    }

    /// For a 4-triangle input, output indices are dense
    /// `[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]`.
    #[test]
    fn indices_are_dense_zero_through_3n_minus_one() {
        // 4 disjoint triangles. Six positions; faces share none for clarity
        // (each triangle uses 3 distinct positions).
        let positions = vec![
            [0.0_f32, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [0.0, 1.0, 1.0],
            [2.0, 0.0, 0.0],
            [3.0, 0.0, 0.0],
            [2.0, 1.0, 0.0],
            [2.0, 0.0, 2.0],
            [3.0, 0.0, 2.0],
            [2.0, 1.0, 2.0],
        ];
        let indices = vec![
            0, 1, 2, // tri 0
            3, 4, 5, // tri 1
            6, 7, 8, // tri 2
            9, 10, 11, // tri 3
        ];
        let mesh = RenderMesh::from_buffers(&positions, &indices, None);
        assert_eq!(mesh.indices, (0_u32..12).collect::<Vec<u32>>());
    }

    // -----------------------------------------------------------------------
    // Synthetic Cuboid fixture — mirrors `CuboidOp::evaluate`'s 8-vert /
    // 12-triangle / 6-face canonical layout exactly. Face order:
    // NegZ → PosZ → NegY → PosY → NegX → PosX (TopologyFaceId 0..5; 2 tris
    // per face). Used in three of the inline tests above + the integration
    // tests in `tests/render_mesh_smoke.rs`.
    //
    // The fixture is duplicated rather than imported from cad-core because
    // `forbidden-dep` rule 6 forbids `rge-brep-render` from depending on
    // `rge-cad-core` (renderer crate vs game-domain crate).
    // -----------------------------------------------------------------------

    /// Build the 8 positions / 36 indices / 12 face_labels of a 1×1×1 cuboid
    /// centered at origin, in the canonical `CuboidOp` face emission order.
    pub(super) fn synthetic_cuboid_buffers() -> (Vec<[f32; 3]>, Vec<u32>, Vec<u64>) {
        // 8 vertices of the [-0.5, +0.5]^3 cube — same shape as
        // `cad-core` mesh.rs::valid_box_constructs's positions.
        // Vertex layout:
        //   0: (-0.5, -0.5, -0.5)   4: (-0.5, -0.5, +0.5)
        //   1: (+0.5, -0.5, -0.5)   5: (+0.5, -0.5, +0.5)
        //   2: (+0.5, +0.5, -0.5)   6: (+0.5, +0.5, +0.5)
        //   3: (-0.5, +0.5, -0.5)   7: (-0.5, +0.5, +0.5)
        let positions = vec![
            [-0.5_f32, -0.5, -0.5],
            [0.5, -0.5, -0.5],
            [0.5, 0.5, -0.5],
            [-0.5, 0.5, -0.5],
            [-0.5, -0.5, 0.5],
            [0.5, -0.5, 0.5],
            [0.5, 0.5, 0.5],
            [-0.5, 0.5, 0.5],
        ];
        // Per-triangle indices in CCW order viewed from each face's outward
        // normal. Face order matches `CuboidOp::evaluate`: NegZ → PosZ →
        // NegY → PosY → NegX → PosX.
        let indices = vec![
            // NegZ (face 0): viewed from -Z, vertices wind CCW.
            0, 2, 1, 0, 3, 2, // PosZ (face 1): viewed from +Z, CCW.
            4, 5, 6, 4, 6, 7, // NegY (face 2).
            0, 1, 5, 0, 5, 4, // PosY (face 3).
            2, 3, 7, 2, 7, 6, // NegX (face 4).
            0, 4, 7, 0, 7, 3, // PosX (face 5).
            1, 2, 6, 1, 6, 5,
        ];
        // Two triangles per face → 12 face_labels in canonical order.
        let labels = vec![0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5_u64];
        (positions, indices, labels)
    }
}
