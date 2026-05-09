//! [`BRepProvider`] — sibling trait to [`crate::operators::Operator`] that
//! exposes stable B-Rep face ids for an operator instance.
//!
//! # Why this is a sibling trait, not an `Operator` extension
//!
//! `BRepProvider` is intentionally a separate trait — NOT a default-method
//! extension on [`crate::operators::Operator`], NOT an `Operator: BRepProvider`
//! supertrait bound, and NOT a new method on the `Operator` trait. This
//! preserves three invariants:
//!
//! 1. v0 implements [`BRepProvider`] for `CuboidOp` only. Forcing every
//!    operator to provide a `BRepProvider` impl up front would require
//!    per-operator face-tag enums (`ExtrudeFaceTag`, `RevolveFaceTag`, …)
//!    that are out of scope for sub-7.2-α.
//! 2. The substrate is opt-in. Callers that don't need stable B-Rep ids
//!    pay zero overhead and ignore the trait.
//! 3. Future `BRepProvider` impls for other operators are pure additions
//!    — they don't churn the `Operator` trait or `OperatorNode` enum.

use super::face_id::{BRepFaceId, BRepOwnerId};
use crate::tessellation::TopologyFaceId;

/// Pair the existing per-tessellation [`TopologyFaceId`] (sequential within a
/// single labeled `Tessellation`, not stable across rebuilds) with the new
/// rebuild-stable [`BRepFaceId`] (owner-seeded, parameter-rebuild-invariant).
///
/// The returned pairs are in canonical face-emission order — for `CuboidOp`,
/// `(TopologyFaceId(0), BRepFaceId::for_cuboid_face(owner, NegZ))`,
/// `(TopologyFaceId(1), BRepFaceId::for_cuboid_face(owner, PosZ))`, …,
/// `(TopologyFaceId(5), BRepFaceId::for_cuboid_face(owner, PosX))`.
///
/// # Implementor contract
///
/// Implementors MUST return the pairs in the same order as the operator's
/// `evaluate` method emits triangles into face groups. Mismatch between the
/// `TopologyFaceId` -> tag mapping and the actual `evaluate` emission order
/// would silently mis-name faces in any downstream consumer (cad-projection,
/// gfx, plug-in editors).
///
/// # v0 implementor list
///
/// * `CuboidOp` — see `crate::operators::cuboid::CuboidOp` for the impl.
///
/// All other operators (`ExtrudeOp` / `RevolveOp` / `BooleanOp` / `LoftOp` /
/// `SweepOp` / `TransformOp`) deliberately do NOT implement this trait in
/// sub-7.2-α. Each lands its `BRepProvider` impl in a future sub-dispatch.
pub trait BRepProvider {
    /// Return the operator's faces paired as `(sequential_id, stable_id)`,
    /// in canonical face-emission order.
    fn brep_face_ids(&self, owner: BRepOwnerId) -> Vec<(TopologyFaceId, BRepFaceId)>;
}
