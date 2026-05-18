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
//! 1. `BRepProvider` is implemented per-operator, landing as each operator's
//!    face topology is inspected and given a canonical face-tag enum
//!    (`CuboidFaceTag`, `ExtrudeFaceTag`, …). Forcing every operator to
//!    provide a `BRepProvider` impl up front would require those enums for
//!    operators whose faces have not yet been inspected.
//! 2. The substrate is opt-in. Callers that don't need stable B-Rep ids
//!    pay zero overhead and ignore the trait.
//! 3. Future `BRepProvider` impls for other operators are pure additions
//!    — they don't churn the `Operator` trait or `OperatorNode` enum.

use super::edge_id::BRepEdgeId;
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
/// # Direct implementor list
///
/// The following operators implement `BRepProvider` directly:
///
/// * `CuboidOp` — see `crate::operators::cuboid::CuboidOp` for the impl.
/// * `ExtrudeOp` — see `crate::operators::extrude::ExtrudeOp` for the impl.
/// * `RevolveOp` — see `crate::operators::revolve::RevolveOp` for the impl.
/// * `LoftOp` — see `crate::operators::loft::LoftOp` for the impl.
/// * `SweepOp` — see `crate::operators::sweep::SweepOp` for the impl.
///
/// The remaining operators (`BooleanOp` / `TransformOp` and the fillet-family
/// operators) do NOT implement this trait directly — fillet-family face
/// identity flows through the resolver instead. Each remaining operator lands
/// its `BRepProvider` impl (or resolver path) in a future sub-dispatch.
pub trait BRepProvider {
    /// Return the operator's faces paired as `(sequential_id, stable_id)`,
    /// in canonical face-emission order.
    fn brep_face_ids(&self, owner: BRepOwnerId) -> Vec<(TopologyFaceId, BRepFaceId)>;
}

/// Operators that can mint stable B-Rep edge identities.
///
/// Sibling trait to [`BRepProvider`] — implementing one does NOT imply
/// implementing the other. Keeping them independent lets each operator
/// opt in to edge identity when its edge topology has been explicitly
/// inspected, without pressure to return half-baked edge IDs ahead of
/// that inspection.
///
/// As of sub-7.2-ζ.α, only `CuboidOp` implements this trait; Extrude /
/// Revolve / Loft will be subsequent sub-dispatches.
pub trait BRepEdgeProvider {
    /// Return the operator's stable edge identities for a given owner.
    ///
    /// The order of the returned `Vec` is canonical for the operator —
    /// callers may rely on positional access if the operator's
    /// docstring documents an explicit edge-emission order.
    fn brep_edge_ids(&self, owner: BRepOwnerId) -> Vec<BRepEdgeId>;
}
