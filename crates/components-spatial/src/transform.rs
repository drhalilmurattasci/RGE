// adapted from rustforge::runtime-bvh::aabb on 2026-05-05 — replaced glam Vec3/Quat
//                                                            with `[f32; N]` so the
//                                                            v0 components crate has
//                                                            no math-crate dep.
//
//! [`Transform`] — local-space rigid+uniform-scale frame.
//!
//! Authoritative for "where does this entity sit in its parent's frame". Copied
//! through the scene tree by `kernel/ecs::TreeRelationStorage::propagate` into
//! [`crate::GlobalTransform`]. See PLAN.md §1.5.1 (canonical entity roles —
//! every renderable carries a `Transform`).

use serde::{Deserialize, Serialize};

/// Translation + rotation (quaternion XYZW) + non-uniform scale, all stored as
/// plain arrays so this crate stays free of any math-crate dependency.
///
/// Identity = zero translation, identity quaternion, unit scale. Construct via
/// [`Transform::IDENTITY`] or [`Transform::from_translation`]. Composition
/// lives downstream (transform-propagation system in W11+); this type is
/// state-only per W01 exit criteria.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    /// Translation (x, y, z) in parent frame.
    pub translation: [f32; 3],
    /// Rotation quaternion (x, y, z, w) — unit length expected; not enforced
    /// at the type level so test fixtures and serde round-trips are simple.
    pub rotation: [f32; 4],
    /// Per-axis scale. `[1.0, 1.0, 1.0]` = unit; nonuniform allowed.
    pub scale: [f32; 3],
}

impl Transform {
    /// Identity transform — no translation, identity rotation, unit scale.
    pub const IDENTITY: Transform = Transform {
        translation: [0.0, 0.0, 0.0],
        rotation: [0.0, 0.0, 0.0, 1.0],
        scale: [1.0, 1.0, 1.0],
    };

    /// Translation-only transform (identity rotation, unit scale).
    #[inline]
    #[must_use]
    pub const fn from_translation(t: [f32; 3]) -> Self {
        Self {
            translation: t,
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        }
    }
}

impl Default for Transform {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl rge_kernel_ecs::Component for Transform {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_round_trip_ron() {
        let t = Transform::IDENTITY;
        let s = ron::to_string(&t).expect("serialize");
        let back: Transform = ron::from_str(&s).expect("deserialize");
        assert_eq!(t, back);
    }

    #[test]
    fn nontrivial_round_trip_ron() {
        // 90 deg about Y, written as the literal sin(45)/cos(45) value.
        let half_sqrt2 = std::f32::consts::FRAC_1_SQRT_2;
        let t = Transform {
            translation: [1.0, -2.5, 3.0],
            rotation: [0.0, half_sqrt2, 0.0, half_sqrt2],
            scale: [2.0, 1.0, 0.5],
        };
        let s = ron::to_string(&t).expect("serialize");
        let back: Transform = ron::from_str(&s).expect("deserialize");
        assert_eq!(t, back);
    }

    #[test]
    fn from_translation_keeps_unit_scale() {
        let t = Transform::from_translation([4.0, 5.0, 6.0]);
        for (got, want) in t.translation.iter().zip([4.0_f32, 5.0, 6.0].iter()) {
            assert!((got - want).abs() < f32::EPSILON);
        }
        for (got, want) in t.rotation.iter().zip([0.0_f32, 0.0, 0.0, 1.0].iter()) {
            assert!((got - want).abs() < f32::EPSILON);
        }
        for (got, want) in t.scale.iter().zip([1.0_f32, 1.0, 1.0].iter()) {
            assert!((got - want).abs() < f32::EPSILON);
        }
    }
}
