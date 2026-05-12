//! `cad_core::operators` — operator type system + concrete operator
//! implementations.
//!
//! Failure class: snapshot-recoverable
//!
//! # Design
//!
//! * [`Operator`] trait — uniform contract every operator implements.
//! * [`OperatorNode`] enum — tagged union the operator graph stores; preserves
//!   serde round-trip via `#[serde(tag = "kind")]`.
//! * [`OpKind`] — discriminant enum, lightweight metadata.
//! * [`EdgeKind`] — typed edge payload identifying the input port at which
//!   the upstream node's tessellation feeds the downstream operator.
//!
//! Phase 7.1 D-prime shipped [`CuboidOp`] and [`TransformOp`]; Phase 7
//! D-Extrude added [`ExtrudeOp`] (with [`Polygon2D`] profile); Phase 7
//! D-Revolve added [`RevolveOp`] (sweep around Y-axis); Phase 7 D-Boolean
//! added [`BooleanOp`] (union/intersection/difference of two upstream
//! tessellations via the csgrs CSG library — first cad-core operator with
//! a Tier-3 dependency, see ADR-112); Phase 7 D-Loft adds [`LoftOp`]
//! (bridge two convex 2D profiles at different `+Z` heights via
//! identity-pairing v0 vertex correspondence).

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::tessellation::Tessellation;

pub mod boolean;
pub mod cuboid;
pub mod extrude;
pub mod fillet;
pub mod loft;
pub mod revolve;
pub mod round_fillet;
pub mod sweep;
pub mod transform;

pub use boolean::{BooleanMode, BooleanOp};
pub use cuboid::CuboidOp;
pub use extrude::{ExtrudeOp, Polygon2D, Polygon2DError};
pub use fillet::{FilletError, FilletOp};
pub use loft::LoftOp;
pub use revolve::RevolveOp;
pub use round_fillet::{RoundFilletError, RoundFilletOp};
pub use sweep::{Polyline3D, Polyline3DError, SweepOp};
pub use transform::TransformOp;

// ---------------------------------------------------------------------------
// OpError
// ---------------------------------------------------------------------------

/// Errors produced during operator evaluation.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum OpError {
    /// The number of inputs supplied did not match the operator's declared
    /// arity.
    #[error("wrong arity: expected {expected}, got {got}")]
    WrongArity {
        /// Number of inputs the operator declares.
        expected: usize,
        /// Number of inputs actually supplied.
        got: usize,
    },
    /// Evaluation produced no geometry where some was expected.
    #[error("operator produced empty result")]
    EmptyResult,
    /// An operator parameter is out of its valid domain.
    #[error("invalid parameter: {0}")]
    InvalidParameter(String),
}

// ---------------------------------------------------------------------------
// OpKind
// ---------------------------------------------------------------------------

/// Discriminant tag for operator kinds.
///
/// Wired alongside [`OperatorNode`] for cheap dispatch in inspectors without
/// matching on the full payload-bearing enum.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum OpKind {
    /// `BooleanOp` — union/intersection/difference of two upstream
    /// tessellations.
    Boolean,
    /// `CuboidOp` — origin-centered axis-aligned box primitive.
    Cuboid,
    /// `ExtrudeOp` — sweep a 2D convex polygon profile along `+Z`.
    Extrude,
    /// `FilletOp` — Cuboid-only chamfer-approximation fillet operator
    /// (D-Fillet sub-α, first BRepEdgeId consumer).
    Fillet,
    /// `LoftOp` — bridge two convex 2D profiles at different `+Z` heights.
    Loft,
    /// `RevolveOp` — rotate a 2D profile around the Y-axis through 2π.
    Revolve,
    /// `RoundFilletOp` — real round fillet substrate (ADR-119,
    /// Cuboid-only in sub-α).
    RoundFillet,
    /// `SweepOp` — sweep a 2D convex profile along a 3D polyline path.
    Sweep,
    /// `TransformOp` — affine TRS applied to one upstream tessellation.
    Transform,
}

// ---------------------------------------------------------------------------
// EdgeKind
// ---------------------------------------------------------------------------

/// Edge payload stored on every operator-graph edge.
///
/// `Input(port)` says: this edge feeds the downstream operator's `port`-th
/// declared input. Future operators with multiple ordered inputs (e.g. a
/// Boolean union with `lhs=0` and `rhs=1`) reuse the same variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EdgeKind {
    /// The edge feeds the destination operator's `port`-th input.
    Input(u8),
}

// ---------------------------------------------------------------------------
// Operator trait
// ---------------------------------------------------------------------------

/// Uniform contract every CAD operator implements.
///
/// `evaluate` produces an output [`Tessellation`] given the upstream inputs;
/// `structural_hash` is the local hash (NOT the recursive-into-inputs hash —
/// the graph evaluator combines these). `arity` declares how many inputs
/// `evaluate` expects.
pub trait Operator: std::fmt::Debug + Send + Sync {
    /// Discriminant tag — see [`OpKind`].
    fn op_kind(&self) -> OpKind;

    /// Number of upstream tessellations `evaluate` expects.
    fn arity(&self) -> usize;

    /// 32-byte BLAKE3 over `(op_kind discriminant, parameters)`.
    ///
    /// Must be deterministic across processes. Does NOT include input hashes
    /// — the [`crate::OperatorGraph::evaluate`] combines this with upstream
    /// hashes to produce the cache key.
    fn structural_hash(&self) -> [u8; 32];

    /// Run the operator. `inputs[i]` is the upstream tessellation feeding
    /// port `i`. The order matches the declared arity.
    ///
    /// # Errors
    ///
    /// * [`OpError::WrongArity`] if `inputs.len() != self.arity()`.
    /// * [`OpError::InvalidParameter`] for out-of-domain parameter values.
    /// * [`OpError::EmptyResult`] if evaluation succeeded but produced no
    ///   geometry (operator-specific — Cuboid/Transform never raise this).
    fn evaluate(&self, inputs: &[&Tessellation]) -> Result<Tessellation, OpError>;

    /// Predict whether this operator's output [`Tessellation`] will carry
    /// `face_labels` given the labeled-state of each input.
    ///
    /// `inputs_labeled[i]` corresponds to port `i`. The slice length equals
    /// [`Self::arity`].
    ///
    /// Used by [`crate::OperatorGraph::evaluate`] to fold upstream-labeled
    /// state into the cache key, so that two evaluations with identical
    /// operator config but distinct input-label states cache separately.
    /// Defense in depth against operator implementations that forget to
    /// reflect label-emitting parameters in [`Self::structural_hash`]
    /// (audit-2 finding A1.4 / A5.2 / Pairing N2 — "latent-but-explosive"
    /// cache-collision bug).
    ///
    /// **Default**: `inputs_labeled.iter().any(|b| *b)` — labels propagate
    /// through any operator that takes labeled input. Operators that
    /// strip labels OR that emit labels regardless of input must override.
    ///
    /// **Contract**: this method's return value MUST match the actual
    /// `evaluate(...)` output's [`Tessellation::is_labeled`] for the same
    /// inputs. If the prediction diverges from reality, the cache key
    /// becomes inconsistent and stale entries may surface.
    fn output_is_labeled(&self, inputs_labeled: &[bool]) -> bool {
        inputs_labeled.iter().any(|b| *b)
    }
}

// ---------------------------------------------------------------------------
// OperatorNode (tagged-union wrapper for graph storage)
// ---------------------------------------------------------------------------

/// Tagged-union enum the operator graph stores as its `N` payload.
///
/// `#[serde(tag = "kind")]` produces a stable wire representation
/// (`{ "kind": "Cuboid", "width": 1.0, ... }`) that is forward-compatible
/// when new variants are added.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum OperatorNode {
    /// Boolean combinator — see [`BooleanOp`].
    Boolean(BooleanOp),
    /// Cuboid primitive — see [`CuboidOp`].
    Cuboid(CuboidOp),
    /// Extrude — see [`ExtrudeOp`].
    Extrude(ExtrudeOp),
    /// Fillet — see [`FilletOp`].
    Fillet(FilletOp),
    /// Loft — see [`LoftOp`].
    Loft(LoftOp),
    /// Revolve — see [`RevolveOp`].
    Revolve(RevolveOp),
    /// RoundFillet — see [`RoundFilletOp`].
    RoundFillet(RoundFilletOp),
    /// Sweep — see [`SweepOp`].
    Sweep(SweepOp),
    /// Transform — see [`TransformOp`].
    Transform(TransformOp),
}

impl OperatorNode {
    /// Reborrow as a `&dyn Operator` for uniform dispatch.
    #[must_use]
    pub fn as_operator(&self) -> &dyn Operator {
        match self {
            OperatorNode::Boolean(op) => op,
            OperatorNode::Cuboid(op) => op,
            OperatorNode::Extrude(op) => op,
            OperatorNode::Fillet(op) => op,
            OperatorNode::Loft(op) => op,
            OperatorNode::Revolve(op) => op,
            OperatorNode::RoundFillet(op) => op,
            OperatorNode::Sweep(op) => op,
            OperatorNode::Transform(op) => op,
        }
    }
}

impl Operator for OperatorNode {
    fn op_kind(&self) -> OpKind {
        self.as_operator().op_kind()
    }

    fn arity(&self) -> usize {
        self.as_operator().arity()
    }

    fn structural_hash(&self) -> [u8; 32] {
        self.as_operator().structural_hash()
    }

    fn evaluate(&self, inputs: &[&Tessellation]) -> Result<Tessellation, OpError> {
        self.as_operator().evaluate(inputs)
    }

    fn output_is_labeled(&self, inputs_labeled: &[bool]) -> bool {
        self.as_operator().output_is_labeled(inputs_labeled)
    }
}

// ---------------------------------------------------------------------------
// Unit tests for the wrapper enum (operator-specific tests live in their
// own modules).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operator_node_dispatches_cuboid() {
        let node = OperatorNode::Cuboid(CuboidOp::default());
        assert_eq!(node.op_kind(), OpKind::Cuboid);
        assert_eq!(node.arity(), 0);
        let mesh = node.evaluate(&[]).expect("eval");
        assert_eq!(mesh.vertex_count(), 8);
    }

    #[test]
    fn operator_node_dispatches_transform() {
        let node = OperatorNode::Transform(TransformOp::default());
        assert_eq!(node.op_kind(), OpKind::Transform);
        assert_eq!(node.arity(), 1);
    }

    #[test]
    fn operator_node_dispatches_extrude() {
        let profile = Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]])
            .expect("square profile");
        let node = OperatorNode::Extrude(ExtrudeOp::new(profile, 1.0).expect("extrude op"));
        assert_eq!(node.op_kind(), OpKind::Extrude);
        assert_eq!(node.arity(), 0);
        let mesh = node.evaluate(&[]).expect("evaluate");
        // n=4 ⇒ 8 vertices, 12 triangles, 36 indices.
        assert_eq!(mesh.vertex_count(), 8);
        assert_eq!(mesh.triangle_count(), 12);
        assert_eq!(mesh.indices.len(), 36);
    }

    #[test]
    fn operator_node_dispatches_revolve() {
        let profile = Polygon2D::new(vec![[1.0, 0.0], [2.0, 0.0], [2.0, 1.0], [1.0, 1.0]])
            .expect("revolve square profile");
        let node = OperatorNode::Revolve(RevolveOp::new(profile, 6).expect("revolve op"));
        assert_eq!(node.op_kind(), OpKind::Revolve);
        assert_eq!(node.arity(), 0);
        let mesh = node.evaluate(&[]).expect("evaluate");
        // n=4 × 6 segments ⇒ 24 vertices, 48 triangles, 144 indices.
        assert_eq!(mesh.vertex_count(), 24);
        assert_eq!(mesh.triangle_count(), 48);
        assert_eq!(mesh.indices.len(), 144);
    }

    #[test]
    fn operator_node_dispatches_loft() {
        let profile_a = Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]])
            .expect("loft profile_a square");
        let profile_b = Polygon2D::new(vec![[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]])
            .expect("loft profile_b square");
        let node = OperatorNode::Loft(LoftOp::new(profile_a, profile_b, 1.5).expect("loft op"));
        assert_eq!(node.op_kind(), OpKind::Loft);
        assert_eq!(node.arity(), 0);
        let mesh = node.evaluate(&[]).expect("evaluate");
        // n=4 ⇒ 8 vertices, 12 triangles, 36 indices.
        assert_eq!(mesh.vertex_count(), 8);
        assert_eq!(mesh.triangle_count(), 12);
        assert_eq!(mesh.indices.len(), 36);
    }

    #[test]
    fn operator_node_dispatches_boolean() {
        let node = OperatorNode::Boolean(BooleanOp::union());
        assert_eq!(node.op_kind(), OpKind::Boolean);
        assert_eq!(node.arity(), 2);
        // Wrong arity (no inputs) yields WrongArity.
        let err = node.evaluate(&[]).unwrap_err();
        assert!(matches!(
            err,
            OpError::WrongArity {
                expected: 2,
                got: 0
            }
        ));
    }

    #[test]
    fn operator_node_dispatches_sweep() {
        let profile = Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]])
            .expect("sweep square profile");
        let path =
            sweep::Polyline3D::new(vec![[0.0, 0.0, 0.0], [0.0, 0.0, 1.0]]).expect("z-axis path");
        let node = OperatorNode::Sweep(SweepOp::new(profile, path));
        assert_eq!(node.op_kind(), OpKind::Sweep);
        assert_eq!(node.arity(), 0);
        let mesh = node.evaluate(&[]).expect("evaluate");
        // n=4, m=2 ⇒ 8 vertices, 4n-4 = 12 triangles, 36 indices.
        assert_eq!(mesh.vertex_count(), 8);
        assert_eq!(mesh.triangle_count(), 12);
        assert_eq!(mesh.indices.len(), 36);
    }

    /// Default trait-level [`Operator::output_is_labeled`] returns `false` on
    /// an empty `inputs_labeled` slice — `Iterator::any` over empty is `false`.
    ///
    /// Post-D-projection-α (2026-05-09): `CuboidOp` and `TransformOp` both
    /// override the default. To exercise the trait-default behaviour over
    /// an empty `inputs_labeled` slice, `BooleanOp` is used here — Boolean
    /// uses the default `iter().any` impl, and an empty slice yields `false`.
    #[test]
    fn output_is_labeled_default_returns_false_on_empty_inputs() {
        // BooleanOp uses the default impl; empty inputs_labeled → false.
        let op = BooleanOp::union();
        assert!(!op.output_is_labeled(&[]));
    }

    /// Default trait-level [`Operator::output_is_labeled`] propagates `true`
    /// when ANY input slot is labeled. `BooleanOp`'s actual evaluate semantics
    /// (`lhs.is_labeled() || rhs.is_labeled()`) match this default exactly.
    #[test]
    fn output_is_labeled_default_propagates_any_labeled_input() {
        // BooleanOp is arity 2 and uses the default impl (which matches the
        // actual evaluate dispatch).
        let op = BooleanOp::union();
        assert!(!op.output_is_labeled(&[false, false]));
        assert!(op.output_is_labeled(&[true, false]));
        assert!(op.output_is_labeled(&[false, true]));
        assert!(op.output_is_labeled(&[true, true]));
    }

    /// SemVer hardening fixture: [`OpKind`] is `#[non_exhaustive]`, so
    /// cross-crate consumers MUST include a wildcard arm when pattern-matching.
    /// This test simulates that consumer pattern: when future variants
    /// (Fillet per PLAN §1.5.4 + ADR-098) are added, the wildcard arm
    /// absorbs them and this test still compiles — proving the
    /// `#[non_exhaustive]` annotation is correctly applied.
    #[test]
    #[allow(
        unreachable_patterns,
        reason = "intentional: simulates cross-crate consumer pattern; \
                  same-crate compilation sees the enum as exhaustive so the \
                  wildcard arm is unreachable from inside the crate, but the \
                  `#[non_exhaustive]` SemVer barrier requires it for external \
                  consumers"
    )]
    fn op_kind_non_exhaustive_pattern_match_compiles() {
        let kind = OpKind::Cuboid;
        let _label = match kind {
            OpKind::Boolean => "boolean",
            OpKind::Cuboid => "cuboid",
            OpKind::Extrude => "extrude",
            OpKind::Fillet => "fillet",
            OpKind::Loft => "loft",
            OpKind::Revolve => "revolve",
            OpKind::RoundFillet => "round_fillet",
            OpKind::Sweep => "sweep",
            OpKind::Transform => "transform",
            _ => "future-variant", // required by #[non_exhaustive]
        };
    }

    #[test]
    fn operator_node_dispatches_fillet() {
        use crate::topology::{BRepEdgeProvider, BRepOwnerId};
        // Build a Cuboid, derive a real edge ID, build a FilletOp,
        // wrap it in OperatorNode, dispatch op_kind / arity. Mirrors
        // the existing operator_node_dispatches_* tests.
        let cube = CuboidOp::default();
        let owner = BRepOwnerId::from_bytes([0x55; 16]);
        let edge = cube.brep_edge_ids(owner)[0];
        let fillet = FilletOp::new(&cube, owner, vec![edge], 0.1).expect("fillet");
        let node = OperatorNode::Fillet(fillet);
        assert_eq!(node.op_kind(), OpKind::Fillet);
        assert_eq!(node.arity(), 1);
        // Wrong arity (no inputs) yields WrongArity.
        let err = node.evaluate(&[]).unwrap_err();
        assert!(matches!(
            err,
            OpError::WrongArity {
                expected: 1,
                got: 0
            }
        ));
    }

    #[test]
    fn operator_node_dispatches_round_fillet() {
        use crate::topology::{BRepEdgeProvider, BRepOwnerId};
        // Build a Cuboid, derive a real edge ID, build a RoundFilletOp,
        // wrap it in OperatorNode, dispatch op_kind / arity. Mirrors
        // operator_node_dispatches_fillet exactly — the two operators
        // share API shape but are byte-distinct per ADR-119 D6.
        let cube = CuboidOp::default();
        let owner = BRepOwnerId::from_bytes([0x88; 16]);
        let edge = cube.brep_edge_ids(owner)[0];
        let round = RoundFilletOp::new(&cube, owner, vec![edge], 0.1).expect("round fillet");
        let node = OperatorNode::RoundFillet(round);
        assert_eq!(node.op_kind(), OpKind::RoundFillet);
        assert_eq!(node.arity(), 1);
        let err = node.evaluate(&[]).unwrap_err();
        assert!(matches!(
            err,
            OpError::WrongArity {
                expected: 1,
                got: 0
            }
        ));
    }
}
