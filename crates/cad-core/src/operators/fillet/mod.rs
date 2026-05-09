//! `FilletOp` — first real consumer of the [`BRepEdgeId`] substrate.
//!
//! D-Fillet sub-α/β/γ: chamfer-approximation fillet operator that takes
//! a list of [`BRepEdgeId`]s plus a radius, validates each edge against
//! the upstream's [`BRepEdgeProvider`], and produces a bounded geometric
//! change per selected edge.
//!
//! Failure class: snapshot-recoverable.
//!
//! # Scope
//!
//! * Upstream operators: sub-α [`CuboidOp`] (fixed 12-edge topology),
//!   sub-β [`ExtrudeOp`] (variable 3N-edge topology), sub-γ
//!   [`RevolveOp`] (mode-driven topology — Full vs Partial). Loft fillet
//!   variant is the next sub-dispatch.
//! * Geometry: **chamfer approximation**, NOT round-fillet kernel.
//!   For each filleted edge, the 2 endpoint corners gain an inward-
//!   offset replica vertex and 2 chamfer-cap triangles connect them.
//!   Per filleted edge: +2 vertices, +2 triangles. Linear in
//!   selection count.
//! * Real round-fillet geometry (quarter-cylinder tessellation,
//!   face-strip removal, multi-edge corner blending, curvature
//!   continuity) is OUT OF SCOPE.
//! * **Revolve geometry support matrix**: cap-side edges in Partial
//!   mode are supported; all side-side adjacencies (Full and Partial
//!   modes) are circular paths and return
//!   [`FilletError::UnsupportedEdgeGeometry`] at construction time.
//!   See [`revolve`] for the full per-edge support matrix.
//!
//! # NON-GOALS
//!
//! * No `impl BRepProvider for FilletOp` (output-side face identity).
//! * No `impl BRepEdgeProvider for FilletOp` (output-side edge identity).
//! * No general fillet kernel.
//! * No Boolean / Sweep input.
//! * No multi-edge corner-sharing geometry. The chamfer is per-edge
//!   independent; if two filleted edges share a corner, the geometry
//!   may be visually weird, but the substrate-validation test does
//!   not exercise that case.
//! * **No public [`FilletUpstream`] trait.** The trait is `pub(crate)`
//!   only — abstraction earned by 3+ implementations existing today
//!   (Cuboid + Extrude + Revolve). External consumer plug-in is a
//!   separate ADR-level decision.
//! * **No support for circular-path Revolve edges in v0**. Side-side
//!   adjacencies (Full and Partial) return
//!   [`FilletError::UnsupportedEdgeGeometry`] at construction time
//!   rather than fabricating geometry.
//!
//! # Pattern: BRepEdgeId-as-constructor-parameter
//!
//! This is the first operator to consume [`BRepEdgeId`] in its
//! constructor. The validation pattern (resolve each ID against
//! the upstream's [`BRepEdgeProvider`], reject unknown IDs) is the
//! precedent for future similar operators (Chamfer, Shell, EdgeBlend).
//!
//! Sub-γ internal refactor: per-edge data is stored as a unified
//! [`ChamferSpec`] carrier `(vertex_a, vertex_b, inward_direction)`
//! computed at construction time. Each upstream type implements the
//! [`FilletUpstream`] trait, providing per-edge resolution; the public
//! constructors ([`FilletOp::new`] for Cuboid in [`cuboid`],
//! [`FilletOp::new_for_extrude`] for Extrude in [`extrude`],
//! [`FilletOp::new_for_revolve`] for Revolve in [`revolve`]) are thin
//! delegates to a shared [`FilletOp::from_upstream`] helper. Evaluation
//! is upstream-agnostic.
//!
//! Today FilletOp falls into the catch-all in
//! [`crate::topology::resolve::brep_face_ids_for_node`] /
//! [`crate::topology::edge_resolve::brep_edge_ids_for_node`] and
//! returns
//! [`crate::topology::BRepResolveError::TopologyChangingOperator`] —
//! correct, since it changes topology (adds vertices/triangles) and
//! does not provide its own face/edge identity in sub-α/β/γ.

use serde::{Deserialize, Serialize};

use crate::operators::{OpError, OpKind, Operator};
use crate::tessellation::Tessellation;
use crate::topology::{BRepEdgeId, BRepEdgeProvider, BRepOwnerId};

mod cuboid;
mod extrude;
mod revolve;

// ---------------------------------------------------------------------------
// FilletError
// ---------------------------------------------------------------------------

/// Construction-time errors for [`FilletOp::new`] /
/// [`FilletOp::new_for_extrude`] / [`FilletOp::new_for_revolve`].
///
/// Marked `#[non_exhaustive]` so future variant additions are
/// non-breaking. Existing pattern matches via `matches!(... Err(FilletError::Variant { .. }))`
/// continue to compile unchanged (`matches!` is non-exhaustive by default).
#[derive(Clone, Copy, Debug, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum FilletError {
    /// `radius` must be finite and strictly positive.
    #[error("fillet radius must be finite and > 0; got {radius}")]
    InvalidRadius {
        /// The offending radius value.
        radius: f32,
    },

    /// Caller passed an empty edge selection — degenerate operator.
    #[error("fillet edge list is empty; degenerate operator")]
    EmptyEdgeSelection,

    /// One of the supplied [`BRepEdgeId`]s does not match any edge
    /// emitted by the upstream's [`BRepEdgeProvider`].
    #[error("edge id {edge:?} does not appear in upstream's BRepEdgeProvider output")]
    EdgeNotInUpstream {
        /// The unknown edge id.
        edge: BRepEdgeId,
    },

    /// The supplied edge ID is valid against the upstream's
    /// [`BRepEdgeProvider`], but its geometry is not supported by
    /// FilletOp's chamfer-approximation pattern in v0.
    ///
    /// Currently surfaced only by [`RevolveOp`] upstreams: side-side
    /// adjacencies in either Full or Partial mode are circular paths
    /// (sweep through `segments` vertices, not 2-endpoint edges) and
    /// reject with this variant. See [`revolve`] for the full support
    /// matrix.
    ///
    /// The construction-time rejection means a constructed FilletOp
    /// can never be in a state where evaluation will fail on
    /// unsupported geometry.
    #[error("edge id {edge:?} has unsupported geometry: {reason}")]
    UnsupportedEdgeGeometry {
        /// The offending edge id.
        edge: BRepEdgeId,
        /// Static description of why the geometry is not supported.
        reason: &'static str,
    },
}

// ---------------------------------------------------------------------------
// ChamferSpec — unified per-filleted-edge carrier
// ---------------------------------------------------------------------------

/// Per-filleted-edge data used at evaluation. Stored in the order the
/// caller supplied edges. Computed at construction time so evaluation
/// is upstream-agnostic.
///
/// The `inward_direction` magnitude is upstream-specific (sub-α uses
/// the raw face-normal-bisector half-magnitude ~0.707; sub-β computes
/// per-edge from profile geometry; sub-γ Revolve uses a centroid-based
/// approach normalized to the same ~0.707 magnitude convention so the
/// structural delta — vertex/index counts — is consistent across
/// operator types). At evaluation time it is multiplied by `radius` to
/// produce the actual chamfer offset applied to each endpoint corner.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct ChamferSpec {
    /// Index of the first endpoint corner in the upstream Tessellation's
    /// position array.
    pub(crate) vertex_a: u32,
    /// Index of the second endpoint corner in the upstream Tessellation's
    /// position array.
    pub(crate) vertex_b: u32,
    /// Inward chamfer-offset direction. Magnitude is whatever the
    /// upstream-specific computation produces; multiplied by `radius`
    /// at evaluation time.
    pub(crate) inward_direction: [f32; 3],
}

// ---------------------------------------------------------------------------
// FilletUpstream — internal trait abstracting per-upstream resolution
// ---------------------------------------------------------------------------

/// Internal trait that abstracts the per-upstream-operator pieces of
/// `FilletOp` construction. Implementations live alongside their
/// operator's fillet adapter ([`cuboid`], [`extrude`], [`revolve`]).
///
/// The trait is intentionally `pub(crate)` — there is no public API
/// surface, and the substrate-doctrine principle says the abstraction
/// is earned by 3+ implementations existing today (Cuboid + Extrude +
/// Revolve). If/when a future external consumer needs to plug in their
/// own upstream type, that's a separate ADR-level decision.
///
/// The supertrait bound on [`BRepEdgeProvider`] gives the generic
/// constructor [`FilletOp::from_upstream`] uniform access to the
/// upstream's edge list for canonical-index resolution.
pub(crate) trait FilletUpstream: BRepEdgeProvider {
    /// Resolve a canonical edge index (the position in `brep_edge_ids`
    /// output) to the data needed for chamfer evaluation.
    ///
    /// # Errors
    ///
    /// Returns `Err(reason)` when the edge's geometry is not supported
    /// by FilletOp's chamfer-approximation pattern. The caller wraps
    /// this with the edge ID into [`FilletError::UnsupportedEdgeGeometry`].
    ///
    /// Cuboid + Extrude implementations always return `Ok(spec)` for
    /// any valid canonical index — those upstreams have no
    /// circular-path edges. Revolve returns `Err(...)` for side-side
    /// adjacencies (circular paths) in either Full or Partial mode.
    fn resolve_chamfer_spec(&self, canonical_index: usize) -> Result<ChamferSpec, &'static str>;
}

// ---------------------------------------------------------------------------
// FilletOp
// ---------------------------------------------------------------------------

/// FilletOp — bounded chamfer along selected upstream edges.
///
/// Constructed via [`FilletOp::new`] (Cuboid upstream),
/// [`FilletOp::new_for_extrude`] (Extrude upstream), or
/// [`FilletOp::new_for_revolve`] (Revolve upstream); each constructor
/// validates each edge against the upstream's [`crate::topology::BRepEdgeProvider`]
/// and resolves each [`BRepEdgeId`] back to a [`ChamferSpec`] so
/// evaluation can locate the geometry without holding a graph
/// reference.
///
/// Arity 1 — takes the upstream's tessellation as input.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FilletOp {
    /// Selected edges by stable identity. Mirrors the user-facing
    /// API surface.
    pub(super) edges: Vec<BRepEdgeId>,
    /// Resolved per-edge chamfer spec — one per selected edge, in
    /// the same order. Used at evaluation time to locate vertices
    /// and apply the chamfer offset without re-resolving via graph
    /// context. Computed at construction time.
    pub(super) chamfer_specs: Vec<ChamferSpec>,
    /// Chamfer offset distance, in world units.
    pub(super) radius: f32,
    /// Owner the substrate-resolved IDs were derived against.
    /// Stored so future-arity sanity (e.g. snapshot round-trip
    /// re-validation) can use it.
    pub(super) owner: BRepOwnerId,
}

impl FilletOp {
    /// Borrow the validated edge selection.
    #[must_use]
    pub fn edges(&self) -> &[BRepEdgeId] {
        &self.edges
    }

    /// Returns the chamfer radius.
    #[must_use]
    pub fn radius(&self) -> f32 {
        self.radius
    }

    /// Returns the owner the edge IDs were validated against.
    #[must_use]
    pub fn owner(&self) -> BRepOwnerId {
        self.owner
    }

    /// Generic constructor over any [`FilletUpstream`].
    ///
    /// Performs the shared validation (radius finiteness, non-empty
    /// edge selection, per-edge upstream lookup) and per-upstream
    /// chamfer-spec resolution.
    ///
    /// # Errors
    ///
    /// * [`FilletError::InvalidRadius`] if `radius` is non-finite or
    ///   `<= 0`.
    /// * [`FilletError::EmptyEdgeSelection`] if `edges` is empty.
    /// * [`FilletError::EdgeNotInUpstream`] if any edge ID does not
    ///   appear in `upstream.brep_edge_ids(owner)`.
    /// * [`FilletError::UnsupportedEdgeGeometry`] if a known edge ID
    ///   has geometry FilletOp cannot chamfer in v0 (e.g. Revolve
    ///   side-side circular paths).
    pub(super) fn from_upstream<U: FilletUpstream>(
        upstream: &U,
        owner: BRepOwnerId,
        edges: Vec<BRepEdgeId>,
        radius: f32,
    ) -> Result<Self, FilletError> {
        if !radius.is_finite() || radius <= 0.0 {
            return Err(FilletError::InvalidRadius { radius });
        }
        if edges.is_empty() {
            return Err(FilletError::EmptyEdgeSelection);
        }

        let upstream_edges = upstream.brep_edge_ids(owner);
        let mut chamfer_specs = Vec::with_capacity(edges.len());
        for &edge_id in &edges {
            let canonical_index = upstream_edges
                .iter()
                .position(|id| *id == edge_id)
                .ok_or(FilletError::EdgeNotInUpstream { edge: edge_id })?;
            let spec = upstream
                .resolve_chamfer_spec(canonical_index)
                .map_err(|reason| FilletError::UnsupportedEdgeGeometry {
                    edge: edge_id,
                    reason,
                })?;
            chamfer_specs.push(spec);
        }

        Ok(Self {
            edges,
            chamfer_specs,
            radius,
            owner,
        })
    }
}

impl Operator for FilletOp {
    fn op_kind(&self) -> OpKind {
        OpKind::Fillet
    }

    fn arity(&self) -> usize {
        1
    }

    fn structural_hash(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"fillet:");
        hasher.update(&self.radius.to_le_bytes());
        hasher.update(self.owner.as_bytes());
        hasher.update(
            &u32::try_from(self.edges.len())
                .unwrap_or(u32::MAX)
                .to_le_bytes(),
        );
        for edge in &self.edges {
            hasher.update(edge.as_bytes());
        }
        let hash = hasher.finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(hash.as_bytes());
        out
    }

    fn evaluate(&self, inputs: &[&Tessellation]) -> Result<Tessellation, OpError> {
        if inputs.len() != self.arity() {
            return Err(OpError::WrongArity {
                expected: self.arity(),
                got: inputs.len(),
            });
        }
        let upstream = inputs[0];
        let mut positions = upstream.positions.clone();
        let mut indices = upstream.indices.clone();

        // For each filleted edge, locate its 2 endpoint corners in
        // the upstream's vertex array and add 2 chamfer-cap triangles.
        // Per-edge ChamferSpec is upstream-agnostic — vertex_a / vertex_b
        // index into `positions`, inward_direction is multiplied by
        // radius to produce the offset applied to each endpoint corner.
        for spec in &self.chamfer_specs {
            // Defensive bounds check — surface a structured error
            // rather than panicking if the upstream tessellation is
            // smaller than the indices the spec captured (e.g. the
            // operator was reused against a non-matching upstream).
            let vertex_a_usize = spec.vertex_a as usize;
            let vertex_b_usize = spec.vertex_b as usize;
            if vertex_a_usize >= positions.len() || vertex_b_usize >= positions.len() {
                return Err(OpError::InvalidParameter(format!(
                    "fillet vertex index out of bounds: a={}, b={}, positions.len={}",
                    spec.vertex_a,
                    spec.vertex_b,
                    positions.len()
                )));
            }

            let corner_a = positions[vertex_a_usize];
            let corner_b = positions[vertex_b_usize];

            // Add 2 new vertices: each endpoint corner offset along
            // the spec's inward direction by `radius`.
            let offset_a = [
                corner_a[0] + spec.inward_direction[0] * self.radius,
                corner_a[1] + spec.inward_direction[1] * self.radius,
                corner_a[2] + spec.inward_direction[2] * self.radius,
            ];
            let offset_b = [
                corner_b[0] + spec.inward_direction[0] * self.radius,
                corner_b[1] + spec.inward_direction[1] * self.radius,
                corner_b[2] + spec.inward_direction[2] * self.radius,
            ];

            let offset_a_idx = u32::try_from(positions.len()).unwrap_or(u32::MAX);
            positions.push(offset_a);
            let offset_b_idx = u32::try_from(positions.len()).unwrap_or(u32::MAX);
            positions.push(offset_b);

            // Add 2 chamfer-cap triangles connecting the original
            // edge endpoints with the offset replicas. Winding is
            // chosen to match sub-α's bit-identical formula (so the
            // Cuboid behavior is preserved). Exact winding-correctness
            // for multi-edge configurations is explicitly out of scope.
            indices.push(spec.vertex_a);
            indices.push(spec.vertex_b);
            indices.push(offset_a_idx);

            indices.push(spec.vertex_b);
            indices.push(offset_b_idx);
            indices.push(offset_a_idx);
        }

        Tessellation::new(positions, indices)
            .map_err(|e| OpError::InvalidParameter(format!("fillet output invalid: {e}")))
    }

    /// `FilletOp::evaluate` calls [`Tessellation::new`] on the
    /// extended positions, which produces an unlabeled output
    /// regardless of whether the upstream input carried
    /// `face_labels`. Mirrors [`crate::operators::TransformOp`]'s
    /// label-stripping override so the cache-key prediction matches
    /// reality.
    fn output_is_labeled(&self, _inputs_labeled: &[bool]) -> bool {
        false
    }
}

// ---------------------------------------------------------------------------
// Operator-trait + accessor unit tests (upstream-agnostic).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operators::CuboidOp;

    fn unit_cube() -> CuboidOp {
        CuboidOp {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
        }
    }

    fn owner() -> BRepOwnerId {
        BRepOwnerId::from_bytes([0xed; 16])
    }

    #[test]
    fn op_kind_is_fillet() {
        let cube = unit_cube();
        let edge = cube.brep_edge_ids(owner())[0];
        let op = FilletOp::new(&cube, owner(), vec![edge], 0.1).expect("ok");
        assert_eq!(op.op_kind(), OpKind::Fillet);
    }

    #[test]
    fn arity_is_one() {
        let cube = unit_cube();
        let edge = cube.brep_edge_ids(owner())[0];
        let op = FilletOp::new(&cube, owner(), vec![edge], 0.1).expect("ok");
        assert_eq!(op.arity(), 1);
    }

    #[test]
    fn structural_hash_changes_with_radius() {
        let cube = unit_cube();
        let edge = cube.brep_edge_ids(owner())[0];
        let a = FilletOp::new(&cube, owner(), vec![edge], 0.1).expect("a");
        let b = FilletOp::new(&cube, owner(), vec![edge], 0.2).expect("b");
        assert_ne!(a.structural_hash(), b.structural_hash());
    }

    #[test]
    fn structural_hash_changes_with_edge_selection() {
        let cube = unit_cube();
        let edges = cube.brep_edge_ids(owner());
        let a = FilletOp::new(&cube, owner(), vec![edges[0]], 0.1).expect("a");
        let b = FilletOp::new(&cube, owner(), vec![edges[0], edges[1]], 0.1).expect("b");
        assert_ne!(a.structural_hash(), b.structural_hash());
    }

    #[test]
    fn structural_hash_includes_owner() {
        let owner_a = BRepOwnerId::from_bytes([0x11; 16]);
        let owner_b = BRepOwnerId::from_bytes([0x22; 16]);
        let cube = unit_cube();
        // Use the FIRST edge from each owner — same canonical
        // position (NegZ ∩ NegY), but different owner means different
        // BRepEdgeId bytes (face IDs include owner in their derivation).
        let edge_a = cube.brep_edge_ids(owner_a)[0];
        let edge_b = cube.brep_edge_ids(owner_b)[0];
        let a = FilletOp::new(&cube, owner_a, vec![edge_a], 0.1).expect("a");
        let b = FilletOp::new(&cube, owner_b, vec![edge_b], 0.1).expect("b");
        assert_ne!(
            a.structural_hash(),
            b.structural_hash(),
            "different owners should produce different structural hashes"
        );
    }

    #[test]
    fn structural_hash_is_deterministic() {
        let cube = unit_cube();
        let edge = cube.brep_edge_ids(owner())[0];
        let a = FilletOp::new(&cube, owner(), vec![edge], 0.1).expect("a");
        let b = FilletOp::new(&cube, owner(), vec![edge], 0.1).expect("b");
        assert_eq!(a.structural_hash(), b.structural_hash());
    }

    #[test]
    fn evaluate_rejects_wrong_arity_zero_inputs() {
        let cube = unit_cube();
        let edge = cube.brep_edge_ids(owner())[0];
        let op = FilletOp::new(&cube, owner(), vec![edge], 0.1).expect("ok");
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
    fn evaluate_rejects_wrong_arity_two_inputs() {
        let cube = unit_cube();
        let edge = cube.brep_edge_ids(owner())[0];
        let op = FilletOp::new(&cube, owner(), vec![edge], 0.1).expect("ok");
        let upstream = cube.evaluate(&[]).expect("cube tess");
        let err = op.evaluate(&[&upstream, &upstream]).unwrap_err();
        assert!(matches!(
            err,
            OpError::WrongArity {
                expected: 1,
                got: 2
            }
        ));
    }

    /// `FilletOp::evaluate` strips labels (calls `Tessellation::new`
    /// which always produces an unlabeled mesh) — so
    /// `output_is_labeled` must return `false` regardless of input
    /// label state. Mirrors `TransformOp::transform_output_is_labeled_strips`.
    #[test]
    fn output_is_labeled_strips() {
        let cube = unit_cube();
        let edge = cube.brep_edge_ids(owner())[0];
        let op = FilletOp::new(&cube, owner(), vec![edge], 0.1).expect("ok");
        assert!(!op.output_is_labeled(&[false]));
        assert!(!op.output_is_labeled(&[true]));
    }
}
