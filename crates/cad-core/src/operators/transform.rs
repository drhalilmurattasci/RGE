//! `TransformOp` — affine TRS transform on a single upstream tessellation
//! (arity 1).
//!
//! Failure class: snapshot-recoverable
//!
//! Builds the standard scale → rotate → translate matrix via
//! [`glam::Mat4::from_scale_rotation_translation`] and applies it to every
//! position in the upstream mesh. Indices pass through unchanged. Transform is
//! topology-preserving, so any upstream `face_labels` carry through one-for-one
//! (positions only; normals are NOT carried).
//!
//! # Capability surface (per ADR-104)
//!
//! * `boolean_robust_under_tolerance`: true (no boolean op).
//! * `deterministic_triangulation`: true (mat4 × vec3 bit-deterministic given identical TRS inputs).
//! * `t_junction_handling`: true (preserves upstream topology unchanged).
//! * `concave_input_supported`: true (passes upstream positions through).
//! * `arity`: 1.
//! * `output_labeled_when_input_labeled`: **true** — Transform preserves
//!   tessellation topology, so labeled input yields labeled output and
//!   unlabeled input yields unlabeled output.

use serde::{Deserialize, Serialize};

use crate::operators::{OpError, OpKind, Operator};
use crate::tessellation::Tessellation;

/// Affine TRS transform applied to one upstream `Tessellation`.
///
/// `rotation_quat_xyzw` is `[x, y, z, w]` to match glam's
/// [`glam::Quat::from_xyzw`] convention. The identity rotation is
/// `[0.0, 0.0, 0.0, 1.0]`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TransformOp {
    /// Translation in object space `[x, y, z]`.
    pub translation: [f32; 3],
    /// Rotation as a quaternion `[x, y, z, w]` (glam ordering).
    pub rotation_quat_xyzw: [f32; 4],
    /// Per-axis scale `[sx, sy, sz]`.
    pub scale: [f32; 3],
}

impl Default for TransformOp {
    /// Identity transform: no translation, identity rotation, unit scale.
    fn default() -> Self {
        Self {
            translation: [0.0, 0.0, 0.0],
            rotation_quat_xyzw: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        }
    }
}

impl Operator for TransformOp {
    fn op_kind(&self) -> OpKind {
        OpKind::Transform
    }

    fn arity(&self) -> usize {
        1
    }

    fn structural_hash(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"transform");
        for v in self.translation {
            hasher.update(&v.to_le_bytes());
        }
        for v in self.rotation_quat_xyzw {
            hasher.update(&v.to_le_bytes());
        }
        for v in self.scale {
            hasher.update(&v.to_le_bytes());
        }
        *hasher.finalize().as_bytes()
    }

    fn evaluate(&self, inputs: &[&Tessellation]) -> Result<Tessellation, OpError> {
        if inputs.len() != self.arity() {
            return Err(OpError::WrongArity {
                expected: self.arity(),
                got: inputs.len(),
            });
        }
        let upstream = inputs[0];

        let mat = glam::Mat4::from_scale_rotation_translation(
            glam::Vec3::from(self.scale),
            glam::Quat::from_xyzw(
                self.rotation_quat_xyzw[0],
                self.rotation_quat_xyzw[1],
                self.rotation_quat_xyzw[2],
                self.rotation_quat_xyzw[3],
            ),
            glam::Vec3::from(self.translation),
        );

        let positions: Vec<[f32; 3]> = upstream
            .positions
            .iter()
            .map(|&p| {
                let v = mat.transform_point3(glam::Vec3::from(p));
                [v.x, v.y, v.z]
            })
            .collect();

        // Indices pass through unchanged.
        let indices = upstream.indices.clone();

        let result = match upstream.face_labels() {
            Some(labels) => Tessellation::with_labels(positions, indices, labels.to_vec()),
            None => Tessellation::new(positions, indices),
        };
        result.map_err(|e| {
            OpError::InvalidParameter(format!("TransformOp produced invalid tessellation: {e}"))
        })
    }

    /// `TransformOp::evaluate` is topology-preserving: it transforms vertex
    /// positions, clones the triangle indices unchanged, and passes any
    /// upstream `face_labels` through one-for-one. Labeled input therefore
    /// yields labeled output and unlabeled input yields unlabeled output.
    ///
    /// For this arity-one operator that coincides with the default
    /// `iter().any` rule, but the override is kept explicit so the cache-key
    /// prediction stays pinned to Transform's documented pass-through
    /// contract.
    fn output_is_labeled(&self, inputs_labeled: &[bool]) -> bool {
        inputs_labeled.iter().any(|b| *b)
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tessellation::TopologyFaceId;

    fn quad() -> Tessellation {
        Tessellation::new(
            vec![
                [0.0_f32, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
            ],
            vec![0, 1, 2, 0, 2, 3],
        )
        .expect("quad ok")
    }

    /// Build a 4-vertex / 2-triangle mesh from caller-supplied positions.
    /// Unlike [`quad`], positions may carry non-zero Z so all three axes
    /// are observable under rotation and non-uniform scale.
    fn mesh(positions: Vec<[f32; 3]>) -> Tessellation {
        assert_eq!(positions.len(), 4, "mesh() expects exactly 4 vertices");
        Tessellation::new(positions, vec![0, 1, 2, 0, 2, 3]).expect("mesh ok")
    }

    #[test]
    fn identity_transform_preserves_vertices_bit_identical() {
        let upstream = quad();
        let op = TransformOp::default();
        let out = op.evaluate(&[&upstream]).expect("evaluate");
        assert_eq!(out.positions, upstream.positions);
        assert_eq!(out.indices, upstream.indices);
    }

    #[test]
    fn translation_shifts_positions_on_x() {
        let upstream = quad();
        let op = TransformOp {
            translation: [1.0, 0.0, 0.0],
            ..TransformOp::default()
        };
        let out = op.evaluate(&[&upstream]).expect("evaluate");
        for (i, [x, y, z]) in out.positions.iter().enumerate() {
            let [ox, oy, oz] = upstream.positions[i];
            assert!(
                (*x - (ox + 1.0)).abs() < 1e-6,
                "x not shifted by 1.0 at idx {i}"
            );
            assert!((*y - oy).abs() < 1e-6);
            assert!((*z - oz).abs() < 1e-6);
        }
    }

    #[test]
    fn arity_violation_returns_wrong_arity() {
        let op = TransformOp::default();
        let err = op.evaluate(&[]).unwrap_err();
        assert!(matches!(
            err,
            OpError::WrongArity {
                expected: 1,
                got: 0
            }
        ));
    }

    #[test]
    fn rotation_y_90_deg_maps_xyz_to_z_y_neg_x() {
        // +90 deg positive Y-axis rotation. Quaternion for a half-angle of
        // 45 deg about +Y: [x, y, z, w] = [0, sin(45 deg), 0, cos(45 deg)]
        // = [0, sqrt(0.5), 0, sqrt(0.5)].
        let h = 0.5_f32.sqrt();
        let upstream = mesh(vec![
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [2.0, 3.0, -1.0],
            [-1.0, -2.0, 4.0],
        ]);
        let op = TransformOp {
            rotation_quat_xyzw: [0.0, h, 0.0, h],
            ..TransformOp::default()
        };
        let out = op.evaluate(&[&upstream]).expect("evaluate");

        // Expected positions are hard-coded from the standard right-handed
        // +90 deg Y rotation formula (x, y, z) -> (z, y, -x), NOT derived
        // from TransformOp::evaluate.
        let expected: [[f32; 3]; 4] = [
            [0.0, 0.0, -1.0],
            [1.0, 0.0, 0.0],
            [-1.0, 3.0, -2.0],
            [4.0, -2.0, 1.0],
        ];
        for (i, ([ex, ey, ez], [x, y, z])) in expected.iter().zip(out.positions.iter()).enumerate()
        {
            assert!((x - ex).abs() < 1e-5, "x mismatch at idx {i}: {x} vs {ex}");
            assert!((y - ey).abs() < 1e-5, "y mismatch at idx {i}: {y} vs {ey}");
            assert!((z - ez).abs() < 1e-5, "z mismatch at idx {i}: {z} vs {ez}");
        }
    }

    #[test]
    fn non_uniform_scale_multiplies_each_axis_independently() {
        // Distinct per-axis scale factors so X, Y, and Z are each observable.
        let (sx, sy, sz) = (2.0_f32, 3.0_f32, 4.0_f32);
        let upstream = mesh(vec![
            [1.0, 1.0, 1.0],
            [2.0, 0.0, 1.0],
            [0.0, 2.0, 3.0],
            [1.0, 1.0, 2.0],
        ]);
        let op = TransformOp {
            scale: [sx, sy, sz],
            ..TransformOp::default()
        };
        let out = op.evaluate(&[&upstream]).expect("evaluate");

        // Hard-coded expected positions = per-axis multiplication
        // (x * sx, y * sy, z * sz).
        let expected: [[f32; 3]; 4] = [
            [2.0, 3.0, 4.0],
            [4.0, 0.0, 4.0],
            [0.0, 6.0, 12.0],
            [2.0, 3.0, 8.0],
        ];
        for (i, ([ex, ey, ez], [x, y, z])) in expected.iter().zip(out.positions.iter()).enumerate()
        {
            assert!((x - ex).abs() < 1e-5, "x mismatch at idx {i}: {x} vs {ex}");
            assert!((y - ey).abs() < 1e-5, "y mismatch at idx {i}: {y} vs {ey}");
            assert!((z - ez).abs() < 1e-5, "z mismatch at idx {i}: {z} vs {ez}");
        }
    }

    #[test]
    fn structural_hash_is_deterministic() {
        let a = TransformOp {
            translation: [1.0, 2.0, 3.0],
            rotation_quat_xyzw: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        };
        let b = a.clone();
        let c = TransformOp {
            translation: [1.0, 2.0, 3.5], // changed
            ..a.clone()
        };
        assert_eq!(a.structural_hash(), b.structural_hash());
        assert_ne!(a.structural_hash(), c.structural_hash());
    }

    #[test]
    fn structural_hash_distinguishes_rotation_only_and_scale_only_changes() {
        let h = 0.5_f32.sqrt();
        let base = TransformOp {
            translation: [1.0, 2.0, 3.0],
            rotation_quat_xyzw: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        };
        // Rotation-only variant: only rotation_quat_xyzw differs from base.
        let rotation_only = TransformOp {
            rotation_quat_xyzw: [0.0, h, 0.0, h],
            ..base.clone()
        };
        // Scale-only variant: only scale differs from base.
        let scale_only = TransformOp {
            scale: [2.0, 3.0, 4.0],
            ..base.clone()
        };

        let base_hash = base.structural_hash();
        let rotation_hash = rotation_only.structural_hash();
        let scale_hash = scale_only.structural_hash();

        assert_ne!(
            base_hash, rotation_hash,
            "rotation-only change must not collapse to the base hash"
        );
        assert_ne!(
            base_hash, scale_hash,
            "scale-only change must not collapse to the base hash"
        );
        assert_ne!(
            rotation_hash, scale_hash,
            "rotation-only and scale-only variants must not share a hash"
        );
    }

    /// A 2-triangle quad carrying caller-supplied per-triangle face labels.
    /// Used to prove Transform passes `face_labels` through unchanged.
    fn labeled_quad(labels: Vec<TopologyFaceId>) -> Tessellation {
        Tessellation::with_labels(
            vec![
                [0.0_f32, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
            ],
            vec![0, 1, 2, 0, 2, 3],
            labels,
        )
        .expect("labeled quad ok")
    }

    #[test]
    fn labeled_input_yields_labeled_output_with_identical_face_ids() {
        let labels = vec![TopologyFaceId(7), TopologyFaceId(3)];
        let upstream = labeled_quad(labels.clone());
        let op = TransformOp {
            translation: [5.0, -2.0, 1.0],
            ..TransformOp::default()
        };
        let out = op.evaluate(&[&upstream]).expect("evaluate");

        assert!(out.is_labeled());
        assert!(out.face_labels().is_some());
        assert_eq!(out.face_labels().unwrap().len(), out.triangle_count());
        assert_eq!(out.face_labels(), upstream.face_labels());
        assert_eq!(out.face_labels().unwrap(), labels.as_slice());
        assert_eq!(out.indices, upstream.indices);
    }

    #[test]
    fn unlabeled_input_yields_unlabeled_output() {
        let upstream = quad();
        assert!(!upstream.is_labeled());
        let op = TransformOp {
            scale: [2.0, 2.0, 2.0],
            ..TransformOp::default()
        };
        let out = op.evaluate(&[&upstream]).expect("evaluate");
        assert!(!out.is_labeled());
        assert!(out.face_labels().is_none());
    }

    #[test]
    fn labels_are_pass_through_duplicates_preserved() {
        let h = 0.5_f32.sqrt();
        let labels = vec![TopologyFaceId(9), TopologyFaceId(9)];
        let upstream = labeled_quad(labels.clone());
        let op = TransformOp {
            translation: [1.0, 1.0, 1.0],
            rotation_quat_xyzw: [0.0, h, 0.0, h],
            scale: [3.0, 1.0, 2.0],
        };
        let out = op.evaluate(&[&upstream]).expect("evaluate");
        assert_eq!(out.face_labels().unwrap(), labels.as_slice());
        assert_eq!(out.indices, upstream.indices);
    }

    /// `TransformOp::evaluate` passes labels through one-for-one, so
    /// [`Operator::output_is_labeled`] mirrors the input's labeled-state.
    #[test]
    fn transform_output_is_labeled_matches_input_labeled_state() {
        let op = TransformOp::default();
        assert!(op.output_is_labeled(&[true]));
        assert!(!op.output_is_labeled(&[false]));
    }
}
