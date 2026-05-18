//! Graph-level B-Rep face-identity resolver.
//!
//! Failure class: snapshot-recoverable
//!
//! Composes per-operator [`BRepProvider`] impls into chain-aware face
//! identity. Direct providers (Cuboid, Extrude, Revolve, Loft, Sweep)
//! yield their own face IDs. Identity-preserving operators (Transform,
//! Fillet) recurse to their upstream and return its IDs unchanged.
//!
//! Topology-changing operators (Boolean, plus any future
//! [`OperatorNode`] variant the resolver doesn't explicitly handle)
//! return [`BRepResolveError::TopologyChangingOperator`] — the
//! resolver does not silently fabricate IDs for them.
//!
//! # Architectural posture (sub-7.2-ε)
//!
//! `BRepProvider` stays "operator can mint its own local face IDs."
//! Transform stays semantically truthful (it changes placement, not
//! topology) and remains a non-`BRepProvider`. The chain-aware
//! resolution lives here at the graph layer rather than inside
//! `TransformOp`, so:
//!
//! * Direct providers each implement `BRepProvider` (Cuboid / Extrude /
//!   Revolve / Loft / Sweep today).
//! * Identity-preserving operators (Transform today) inherit upstream
//!   IDs verbatim through this resolver — they do NOT implement
//!   `BRepProvider`.
//! * Topology-changing operators (Boolean, future ones) return an
//!   explicit error — neither identity-preserving nor providing local
//!   IDs.
//!
//! Sub-7.2-ε shipped graph-level Transform inheritance; the Sweep
//! face-identity slice adds the direct `OperatorNode::Sweep` provider
//! arm (Sweep is a direct face provider, not topology-changing, at the
//! face resolver). Boolean propagation and projection plumbing remain
//! open. Phase 7.2 IMPLEMENTATION.md exit criterion is NOT closed by
//! this module.
//!
//! # Architectural posture (D-Fillet sub-ε.α)
//!
//! Extends the identity-preserving arm to [`OperatorNode::Fillet`].
//! [`FilletOp::evaluate`] clones the upstream's `positions` and
//! `indices` verbatim and APPENDS chamfer-cap geometry — every
//! upstream face exists bit-identical in the output mesh, so the
//! upstream's face IDs describe surfaces that still exist after a
//! Fillet. Chamfer-cap triangles are unnamed in v0 (no face ID
//! assigned); a future direct `impl BRepProvider for FilletOp` may
//! mint cap-face IDs (sub-ε.γ).
//!
//! Edge inheritance through Fillet is intentionally NOT extended in
//! sub-ε.α. Filleted edges lose their 2-endpoint geometry under
//! chamfering (the original edge is replaced by chamfer-cap segments),
//! so edge pass-through would silently misrepresent topology. The
//! edge resolver ([`crate::topology::edge_resolve`]) keeps Fillet in
//! its catch-all and returns
//! [`BRepResolveError::TopologyChangingOperator`] for now; sub-ε.β
//! will revisit edge inheritance with filtered-out filleted edges as
//! the bounded next slice.

use std::collections::HashSet;

use rge_kernel_graph_foundation::NodeId;
use thiserror::Error;

use crate::graph::OperatorGraph;
use crate::operators::{OpKind, OperatorNode};
use crate::tessellation::TopologyFaceId;
use crate::topology::{BRepFaceId, BRepOwnerId, BRepProvider};

// ---------------------------------------------------------------------------
// BRepResolveError
// ---------------------------------------------------------------------------

/// Errors produced by graph-level B-Rep face-identity resolution.
///
/// Inherits the snapshot-recoverable failure class of
/// [`crate::graph::OperatorGraph`]: every variant is a structural mis-build
/// of the graph or an explicit "operator does not preserve topology" signal,
/// not a state-corruption.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BRepResolveError {
    /// The named node does not exist in the operator graph.
    #[error("node not found in graph: {node}")]
    NodeNotInGraph {
        /// The missing node id.
        node: NodeId,
    },

    /// The operator at this node does not preserve topology and the
    /// resolver does not synthesize IDs for it. [`OpKind::Boolean`] is
    /// the v0 unsupported operator; any future [`OperatorNode`] variant
    /// not explicitly handled also produces this error via the catch-all
    /// arm in [`brep_face_ids_for_node`].
    #[error(
        "operator kind {kind:?} does not preserve topology; no graph-level B-Rep face identity"
    )]
    TopologyChangingOperator {
        /// The unsupported operator kind, surfaced via the
        /// [`Operator::op_kind`] trait method so the message is
        /// accurate even for future-added variants.
        kind: OpKind,
    },

    /// An identity-preserving operator (e.g. [`OperatorNode::Transform`],
    /// arity 1) did not have exactly its declared input count of incoming
    /// edges. This is structurally a graph-construction bug; the resolver
    /// surfaces it rather than panicking.
    ///
    /// Reachable through public API because
    /// [`OperatorGraph::connect`] does NOT validate arity at attach time;
    /// only [`OperatorGraph::evaluate`] does. A Transform node with zero
    /// incoming edges is therefore buildable and would surface this
    /// error here.
    #[error("operator at node {node} expected exactly {expected} input(s), got {got}")]
    UnexpectedArity {
        /// The arity-violating node id.
        node: NodeId,
        /// Operator's declared input count.
        expected: usize,
        /// Actual number of incoming edges.
        got: usize,
    },

    /// A cycle was detected during recursive resolution at this node.
    ///
    /// [`OperatorGraph`] itself rejects cycles at construction (per
    /// `kernel/graph-foundation::Graph::insert_edge`'s endpoint-presence
    /// check is not enough on its own; the operator-graph's own
    /// cycle-detection lives in `effective_hash_and_label` during
    /// evaluate). This error is therefore primarily defensive — it
    /// should be unreachable through the public API but is checked
    /// anyway to mirror [`crate::graph::OperatorGraph::evaluate`]'s
    /// in-flight `HashSet` pattern.
    #[error("cycle detected in operator graph during B-Rep resolution at node {node}")]
    CycleDetected {
        /// The node revisited during recursive resolution.
        node: NodeId,
    },
}

// ---------------------------------------------------------------------------
// Public resolver entry point
// ---------------------------------------------------------------------------

/// Resolve B-Rep face identity for a node in an [`OperatorGraph`],
/// dispatching by operator kind.
///
/// * Direct providers (Cuboid / Extrude / Revolve / Loft / Sweep):
///   call [`BRepProvider::brep_face_ids`] on the operator-variant
///   payload.
/// * Identity-preserving (Transform, Fillet): recurse to the unique
///   input node and return its IDs unchanged.
/// * Topology-changing (Boolean, any future variant): return
///   [`BRepResolveError::TopologyChangingOperator`].
///
/// `owner` is propagated unchanged through Transform chains — the
/// chain inherits whatever owner the consumer supplies at the call
/// site. Per [`BRepOwnerId`] docs the owner-seed must NOT be derived
/// from [`NodeId`] or `effective_hash` (parameter-rebuild stability
/// would break); this resolver upholds that contract by never
/// inspecting either when constructing the recursion.
///
/// # Errors
///
/// See [`BRepResolveError`] variants. All errors are structural
/// graph-shape problems or explicit "operator unsupported" signals;
/// no internal panic path exists.
pub fn brep_face_ids_for_node(
    graph: &OperatorGraph,
    node: NodeId,
    owner: BRepOwnerId,
) -> Result<Vec<(TopologyFaceId, BRepFaceId)>, BRepResolveError> {
    let mut in_flight = HashSet::new();
    resolve_recursive(graph, node, owner, &mut in_flight)
}

// ---------------------------------------------------------------------------
// Internal recursion
// ---------------------------------------------------------------------------

/// Recursive helper. `in_flight` mirrors [`OperatorGraph::evaluate`]'s
/// in-flight `HashSet<NodeId>` pattern: every recursion entry inserts
/// the node id, returns [`BRepResolveError::CycleDetected`] on
/// double-insertion, and unconditionally removes the id on exit so
/// sibling subtrees with overlapping ancestors still resolve.
fn resolve_recursive(
    graph: &OperatorGraph,
    node: NodeId,
    owner: BRepOwnerId,
    in_flight: &mut HashSet<NodeId>,
) -> Result<Vec<(TopologyFaceId, BRepFaceId)>, BRepResolveError> {
    if !in_flight.insert(node) {
        return Err(BRepResolveError::CycleDetected { node });
    }

    let result = resolve_step(graph, node, owner, in_flight);

    in_flight.remove(&node);
    result
}

/// Single dispatch step. Looking up `node`, then matching on the
/// [`OperatorNode`] variant: direct providers call into their
/// [`BRepProvider`] impl, identity-preserving operators
/// ([`OperatorNode::Transform`] / [`OperatorNode::Fillet`]) recurse to
/// their unique input, every other arm — including the catch-all that
/// absorbs future variants — returns
/// [`BRepResolveError::TopologyChangingOperator`].
fn resolve_step(
    graph: &OperatorGraph,
    node: NodeId,
    owner: BRepOwnerId,
    in_flight: &mut HashSet<NodeId>,
) -> Result<Vec<(TopologyFaceId, BRepFaceId)>, BRepResolveError> {
    let operator_node = graph
        .node(node)
        .ok_or(BRepResolveError::NodeNotInGraph { node })?;

    match operator_node {
        OperatorNode::Cuboid(op) => Ok(op.brep_face_ids(owner)),
        OperatorNode::Extrude(op) => Ok(op.brep_face_ids(owner)),
        OperatorNode::Revolve(op) => Ok(op.brep_face_ids(owner)),
        OperatorNode::Loft(op) => Ok(op.brep_face_ids(owner)),
        OperatorNode::Sweep(op) => Ok(op.brep_face_ids(owner)),

        OperatorNode::Transform(_) => {
            // TransformOp is arity 1 — exactly one EdgeKind::Input(port=0)
            // edge. Walk incoming edges to find the single input, then
            // recurse. Owner propagates unchanged.
            let upstream = single_input_node(graph, node)?;
            resolve_recursive(graph, upstream, owner, in_flight)
        }

        OperatorNode::Fillet(_) => {
            // FilletOp is arity 1. Its `evaluate` clones upstream
            // positions/indices verbatim and APPENDS chamfer-cap
            // geometry (see `operators::fillet::mod`), so every upstream
            // face exists bit-identical in the output mesh and inherits
            // its BRepFaceId via the same recursion pattern as Transform.
            // Chamfer-cap triangles are unnamed in v0 — no face ID
            // assigned. Edge inheritance is intentionally deferred to
            // sub-ε.β (filleted edges lose 2-endpoint geometry under
            // chamfering and remain in `edge_resolve`'s catch-all).
            let upstream = single_input_node(graph, node)?;
            resolve_recursive(graph, upstream, owner, in_flight)
        }

        OperatorNode::RoundFillet(_) => {
            // RoundFilletOp is arity 1. Per ADR-119 D4, faces retain
            // identity under face-strip removal because identity is the
            // semantic surface, not the mesh shape. RoundFilletOp's
            // `evaluate` clones upstream positions verbatim (no upstream
            // vertex is moved or removed) and substitutes the filleted-
            // edge endpoint indices with new inset vertices within the
            // two adjacent faces' triangles; every upstream face's
            // surface still exists in the output mesh — possibly with
            // different vertex indices but with the same semantic
            // identity. Face IDs therefore inherit unchanged through
            // RoundFillet via the same recursion pattern as Transform
            // and chamfer Fillet. Cylinder-cap surface triangles are
            // unnamed in v0 (TopologyFaceId::DEGENERATE per ADR-119 D3).
            let upstream = single_input_node(graph, node)?;
            resolve_recursive(graph, upstream, owner, in_flight)
        }

        // Catch-all for topology-changing operators (Boolean) AND for
        // any future OperatorNode variant added without explicit
        // handling. The kind is read via the Operator trait so the
        // error message is accurate.
        other => Err(BRepResolveError::TopologyChangingOperator {
            kind: other.as_operator().op_kind(),
        }),
    }
}

/// Walk a node's incoming edges and return the single upstream
/// [`NodeId`]. Used for arity-1 identity-preserving operators
/// (Transform today). Returns [`BRepResolveError::UnexpectedArity`] if
/// the node has anything other than exactly one incoming edge.
fn single_input_node(graph: &OperatorGraph, node: NodeId) -> Result<NodeId, BRepResolveError> {
    let inner = graph.inner();
    let incoming: Vec<_> = inner.incoming(node).collect();
    if incoming.len() != 1 {
        return Err(BRepResolveError::UnexpectedArity {
            node,
            expected: 1,
            got: incoming.len(),
        });
    }
    let edge_id = incoming[0];
    let rec = inner
        .edge(edge_id)
        .ok_or(BRepResolveError::NodeNotInGraph { node })?;
    Ok(rec.src)
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operators::{
        BooleanOp, CuboidOp, ExtrudeOp, FilletOp, Polygon2D, Polyline3D, RevolveOp, RoundFilletOp,
        SweepOp, TransformOp,
    };
    use crate::topology::BRepEdgeProvider;
    use crate::CadGraph;

    fn unit_square() -> Polygon2D {
        Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]).expect("square")
    }

    #[test]
    fn error_node_not_in_graph_returns_correct_variant() {
        let graph = OperatorGraph::new();
        let fresh = NodeId::from_raw(0xdead_beef_cafe_babe_u128);
        let owner = BRepOwnerId::from_bytes([0x00; 16]);

        let err = brep_face_ids_for_node(&graph, fresh, owner)
            .expect_err("fresh NodeId must produce error");
        assert_eq!(err, BRepResolveError::NodeNotInGraph { node: fresh });
    }

    /// Verify the [`BRepResolveError::TopologyChangingOperator`] error
    /// carries the correct [`OpKind`] for Boolean — the one known v0
    /// unsupported operator (Sweep became a direct face provider in the
    /// Sweep face-identity slice). Future-added variants would also flow
    /// through the catch-all and surface their kind, but we don't
    /// enumerate hypothetical variants here.
    #[test]
    fn error_topology_changing_operator_carries_correct_kind() {
        // Boolean
        let mut cad = CadGraph::new();
        cad.begin_operation().expect("begin");
        let cube_a = cad
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::Cuboid(CuboidOp {
                width: 1.0,
                height: 1.0,
                depth: 1.0,
            }))
            .expect("cube_a");
        let cube_b = cad
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::Cuboid(CuboidOp {
                width: 1.0001,
                height: 1.0,
                depth: 1.0,
            }))
            .expect("cube_b");
        let bool_node = cad
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::Boolean(BooleanOp::union()))
            .expect("bool");
        cad.graph_mut()
            .expect("mut")
            .connect(cube_a, bool_node, 0)
            .expect("a->bool");
        cad.graph_mut()
            .expect("mut")
            .connect(cube_b, bool_node, 1)
            .expect("b->bool");
        cad.commit("boolean").expect("commit");

        let owner = BRepOwnerId::from_bytes([0x01; 16]);
        let err = brep_face_ids_for_node(cad.graph(), bool_node, owner)
            .expect_err("Boolean must produce error");
        assert_eq!(
            err,
            BRepResolveError::TopologyChangingOperator {
                kind: OpKind::Boolean
            }
        );
    }

    /// Direct `OperatorNode::Sweep` resolution through the resolver
    /// returns the same ordered IDs as a direct
    /// [`BRepProvider::brep_face_ids`] call. Sweep is a direct face
    /// provider — not topology-changing — at the face resolver as of the
    /// Sweep face-identity slice.
    #[test]
    fn resolver_direct_sweep_matches_direct_provider() {
        let owner = BRepOwnerId::from_bytes([0x5e; 16]);
        let path =
            Polyline3D::new(vec![[0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, 2.0]]).expect("path");
        let sweep = SweepOp::new(unit_square(), path);

        let direct: Vec<(TopologyFaceId, BRepFaceId)> = sweep.brep_face_ids(owner);

        let mut cad = CadGraph::new();
        cad.begin_operation().expect("begin");
        let sweep_node = cad
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::Sweep(sweep))
            .expect("sweep");
        cad.commit("sweep").expect("commit");

        let through_resolver =
            brep_face_ids_for_node(cad.graph(), sweep_node, owner).expect("resolve direct sweep");

        assert_eq!(through_resolver, direct);
    }

    /// Building a Transform node with zero incoming edges is structurally
    /// possible because [`OperatorGraph::connect`] does not validate
    /// arity at attach time. Resolving against it surfaces
    /// [`BRepResolveError::UnexpectedArity`] rather than panicking.
    #[test]
    fn error_unexpected_arity_when_transform_has_no_input() {
        let mut cad = CadGraph::new();
        cad.begin_operation().expect("begin");
        let xform_node = cad
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::Transform(TransformOp::default()))
            .expect("xform");
        cad.commit("dangling transform").expect("commit");

        let owner = BRepOwnerId::from_bytes([0x02; 16]);
        let err = brep_face_ids_for_node(cad.graph(), xform_node, owner)
            .expect_err("dangling Transform must produce error");
        assert_eq!(
            err,
            BRepResolveError::UnexpectedArity {
                node: xform_node,
                expected: 1,
                got: 0,
            }
        );
    }

    /// [`BRepResolveError::CycleDetected`] is defensive — the public
    /// `OperatorGraph` API rejects cycle-creating edges at evaluate
    /// time, and the resolver itself only ever follows
    /// `EdgeKind::Input` upstream from a leaf, so a true cycle here
    /// would require a graph-foundation bypass. We document the
    /// invariant and exercise the error variant's `Display` /
    /// `PartialEq` shape via a synthetic value rather than constructing
    /// an actually-cyclic graph.
    #[test]
    fn error_cycle_detected_unreachable_through_public_api_documented() {
        let synthetic = BRepResolveError::CycleDetected {
            node: NodeId::from_raw(7),
        };
        // Render goes through the `thiserror`-derived Display impl.
        let rendered = format!("{synthetic}");
        assert!(rendered.contains("cycle detected"));
        assert!(rendered.contains("node:"));
        // Round-trip via Clone + PartialEq.
        let cloned = synthetic.clone();
        assert_eq!(synthetic, cloned);
    }

    /// Sanity test: every error variant's `Display` impl renders
    /// without panicking and includes a recognisable substring. Pure
    /// `thiserror` ergonomics smoke; not a substantive substrate test.
    #[test]
    fn error_display_renders_for_all_variants() {
        let n = NodeId::from_raw(42);
        let cases: Vec<BRepResolveError> = vec![
            BRepResolveError::NodeNotInGraph { node: n },
            BRepResolveError::TopologyChangingOperator {
                kind: OpKind::Boolean,
            },
            BRepResolveError::UnexpectedArity {
                node: n,
                expected: 1,
                got: 0,
            },
            BRepResolveError::CycleDetected { node: n },
        ];
        for c in &cases {
            let s = format!("{c}");
            assert!(!s.is_empty(), "error variant Display produced empty string");
        }
    }

    /// Sanity for [`BRepResolveError`] derive bundle: `Clone` +
    /// `PartialEq` + `Eq` round-trip cleanly on a representative
    /// variant.
    #[test]
    fn error_clone_partialeq_eq_round_trip() {
        let err = BRepResolveError::TopologyChangingOperator {
            kind: OpKind::Sweep,
        };
        let cloned = err.clone();
        assert_eq!(err, cloned);
        assert_eq!(err, err);
    }

    /// Direct provider through the resolver matches a direct
    /// [`BRepProvider::brep_face_ids`] call. Proves the resolver
    /// doesn't perturb a leaf-direct-provider's IDs.
    #[test]
    fn resolver_direct_cuboid_matches_direct_provider_call() {
        let cuboid = CuboidOp {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
        };
        let owner = BRepOwnerId::from_bytes([0x33; 16]);

        let direct: Vec<BRepFaceId> = cuboid
            .brep_face_ids(owner)
            .into_iter()
            .map(|(_, id)| id)
            .collect();

        let mut cad = CadGraph::new();
        cad.begin_operation().expect("begin");
        let cuboid_node = cad
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::Cuboid(cuboid))
            .expect("cuboid");
        cad.commit("cuboid").expect("commit");

        let through_resolver: Vec<BRepFaceId> =
            brep_face_ids_for_node(cad.graph(), cuboid_node, owner)
                .expect("resolve")
                .into_iter()
                .map(|(_, id)| id)
                .collect();

        assert_eq!(through_resolver, direct);
    }

    /// Direct provider through the resolver works for Extrude and
    /// Revolve variants as well — covers all four direct-provider arms
    /// of the resolver match.
    #[test]
    fn resolver_direct_extrude_and_revolve_match_direct_provider() {
        let owner = BRepOwnerId::from_bytes([0x44; 16]);

        // Extrude
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("extrude");
        let extrude_direct: Vec<BRepFaceId> = extrude
            .brep_face_ids(owner)
            .into_iter()
            .map(|(_, id)| id)
            .collect();
        let mut cad = CadGraph::new();
        cad.begin_operation().expect("begin");
        let extrude_node = cad
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::Extrude(extrude))
            .expect("extrude");
        cad.commit("extrude").expect("commit");
        let extrude_chain: Vec<BRepFaceId> =
            brep_face_ids_for_node(cad.graph(), extrude_node, owner)
                .expect("resolve extrude")
                .into_iter()
                .map(|(_, id)| id)
                .collect();
        assert_eq!(extrude_chain, extrude_direct);

        // Revolve
        let revolve_profile = Polygon2D::new(vec![[1.0, 0.0], [2.0, 0.0], [2.0, 1.0], [1.0, 1.0]])
            .expect("revolve profile");
        let revolve = RevolveOp::new(revolve_profile, 6).expect("revolve");
        let revolve_direct: Vec<BRepFaceId> = revolve
            .brep_face_ids(owner)
            .into_iter()
            .map(|(_, id)| id)
            .collect();
        let mut cad2 = CadGraph::new();
        cad2.begin_operation().expect("begin");
        let revolve_node = cad2
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::Revolve(revolve))
            .expect("revolve");
        cad2.commit("revolve").expect("commit");
        let revolve_chain: Vec<BRepFaceId> =
            brep_face_ids_for_node(cad2.graph(), revolve_node, owner)
                .expect("resolve revolve")
                .into_iter()
                .map(|(_, id)| id)
                .collect();
        assert_eq!(revolve_chain, revolve_direct);
    }

    /// D-Fillet sub-ε.α load-bearing assertion: Cuboid → Fillet
    /// inherits the Cuboid's face IDs unchanged. Mirrors the sub-7.2-ε
    /// Transform-inheritance precedent — FilletOp.evaluate clones
    /// upstream positions/indices verbatim and appends chamfer-cap
    /// geometry, so every upstream face exists bit-identical in the
    /// output mesh and the resolver delegates face identity to the
    /// upstream.
    #[test]
    fn resolver_cuboid_then_fillet_inherits_cuboid_face_ids() {
        let owner = BRepOwnerId::from_bytes([0x55; 16]);
        let cuboid = CuboidOp {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
        };

        let direct_ids: Vec<BRepFaceId> = cuboid
            .brep_face_ids(owner)
            .into_iter()
            .map(|(_, id)| id)
            .collect();

        let edges = cuboid.brep_edge_ids(owner);
        let fillet = FilletOp::new(&cuboid, owner, vec![edges[0]], 0.1).expect("fillet");

        let mut cad = CadGraph::new();
        cad.begin_operation().expect("begin");
        let cuboid_node = cad
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::Cuboid(cuboid))
            .expect("add cuboid");
        let fillet_node = cad
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::Fillet(fillet))
            .expect("add fillet");
        cad.graph_mut()
            .expect("mut")
            .connect(cuboid_node, fillet_node, 0)
            .expect("connect cuboid->fillet");
        cad.commit("cuboid -> fillet").expect("commit");

        let chain_ids: Vec<BRepFaceId> = brep_face_ids_for_node(cad.graph(), fillet_node, owner)
            .expect("resolve cuboid->fillet chain")
            .into_iter()
            .map(|(_, id)| id)
            .collect();

        assert_eq!(
            chain_ids, direct_ids,
            "Fillet must inherit Cuboid face IDs unchanged"
        );
        assert_eq!(chain_ids.len(), 6, "cuboid has 6 faces");
    }

    /// D-Fillet sub-ε.α rebuild-stability assertion: FilletOp with
    /// three different radii on the same edge selection produces
    /// byte-identical face IDs. The resolver inherits from the
    /// upstream and Fillet parameters (radius / edge selection) never
    /// enter face-identity derivation. Pins the contract that
    /// parameter-driven Fillet edits do not invalidate cached face
    /// selections downstream.
    #[test]
    fn resolver_fillet_face_ids_stable_under_radius_parameter_change() {
        let owner = BRepOwnerId::from_bytes([0x66; 16]);
        let cuboid_template = CuboidOp {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
        };
        let edges = cuboid_template.brep_edge_ids(owner);

        let build_chain = |radius: f32| -> Vec<BRepFaceId> {
            let cuboid = cuboid_template.clone();
            let fillet = FilletOp::new(&cuboid, owner, vec![edges[0]], radius).expect("fillet");
            let mut cad = CadGraph::new();
            cad.begin_operation().expect("begin");
            let cuboid_node = cad
                .graph_mut()
                .expect("mut")
                .add_operator(OperatorNode::Cuboid(cuboid))
                .expect("add cuboid");
            let fillet_node = cad
                .graph_mut()
                .expect("mut")
                .add_operator(OperatorNode::Fillet(fillet))
                .expect("add fillet");
            cad.graph_mut()
                .expect("mut")
                .connect(cuboid_node, fillet_node, 0)
                .expect("connect");
            cad.commit("cuboid -> fillet").expect("commit");
            brep_face_ids_for_node(cad.graph(), fillet_node, owner)
                .expect("resolve")
                .into_iter()
                .map(|(_, id)| id)
                .collect()
        };

        let ids_at_01 = build_chain(0.1);
        let ids_at_02 = build_chain(0.2);
        let ids_at_05 = build_chain(0.5);

        assert_eq!(
            ids_at_01, ids_at_02,
            "fillet radius 0.1 -> 0.2 must not change face IDs"
        );
        assert_eq!(
            ids_at_02, ids_at_05,
            "fillet radius 0.2 -> 0.5 must not change face IDs"
        );
    }

    /// ADR-119 sub-α load-bearing assertion: Cuboid → RoundFillet
    /// inherits the Cuboid's face IDs unchanged. Per ADR D4 the face-
    /// strip-removal substitution in `RoundFilletOp::evaluate` changes
    /// vertex indices within filleted-edge-adjacent face triangles but
    /// every original face's surface still exists in the output —
    /// semantic identity preserved, mesh shape changed.
    #[test]
    fn resolver_cuboid_then_round_fillet_inherits_cuboid_face_ids() {
        let owner = BRepOwnerId::from_bytes([0xa1; 16]);
        let cuboid = CuboidOp {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
        };

        let direct_ids: Vec<BRepFaceId> = cuboid
            .brep_face_ids(owner)
            .into_iter()
            .map(|(_, id)| id)
            .collect();

        let edges = cuboid.brep_edge_ids(owner);
        let round = RoundFilletOp::new(&cuboid, owner, vec![edges[0]], 0.1).expect("round");

        let mut cad = CadGraph::new();
        cad.begin_operation().expect("begin");
        let cuboid_node = cad
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::Cuboid(cuboid))
            .expect("add cuboid");
        let round_node = cad
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::RoundFillet(round))
            .expect("add round");
        cad.graph_mut()
            .expect("mut")
            .connect(cuboid_node, round_node, 0)
            .expect("connect cuboid->round");
        cad.commit("cuboid -> round").expect("commit");

        let chain_ids: Vec<BRepFaceId> = brep_face_ids_for_node(cad.graph(), round_node, owner)
            .expect("resolve cuboid->round chain")
            .into_iter()
            .map(|(_, id)| id)
            .collect();

        assert_eq!(
            chain_ids, direct_ids,
            "RoundFillet must inherit Cuboid face IDs unchanged"
        );
        assert_eq!(chain_ids.len(), 6, "cuboid has 6 faces");
    }

    /// ADR-119 sub-α rebuild-stability: RoundFilletOp with three
    /// different radii on the same edge selection produces byte-
    /// identical face IDs. Pins the contract that radius parameter
    /// changes don't invalidate downstream face-selection.
    #[test]
    fn resolver_round_fillet_face_ids_stable_under_radius_change() {
        let owner = BRepOwnerId::from_bytes([0xa2; 16]);
        let cuboid_template = CuboidOp {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
        };
        let edges = cuboid_template.brep_edge_ids(owner);

        let build_chain = |radius: f32| -> Vec<BRepFaceId> {
            let cuboid = cuboid_template.clone();
            let round = RoundFilletOp::new(&cuboid, owner, vec![edges[0]], radius).expect("round");
            let mut cad = CadGraph::new();
            cad.begin_operation().expect("begin");
            let cuboid_node = cad
                .graph_mut()
                .expect("mut")
                .add_operator(OperatorNode::Cuboid(cuboid))
                .expect("add cuboid");
            let round_node = cad
                .graph_mut()
                .expect("mut")
                .add_operator(OperatorNode::RoundFillet(round))
                .expect("add round");
            cad.graph_mut()
                .expect("mut")
                .connect(cuboid_node, round_node, 0)
                .expect("connect");
            cad.commit("cuboid -> round").expect("commit");
            brep_face_ids_for_node(cad.graph(), round_node, owner)
                .expect("resolve")
                .into_iter()
                .map(|(_, id)| id)
                .collect()
        };

        let ids_at_01 = build_chain(0.1);
        let ids_at_02 = build_chain(0.2);
        let ids_at_05 = build_chain(0.5);

        assert_eq!(ids_at_01, ids_at_02);
        assert_eq!(ids_at_02, ids_at_05);
    }

    /// D-Fillet sub-ε.α composition test: Cuboid → Transform → Fillet
    /// inherits Cuboid IDs through both identity-preserving arms.
    /// Validates the new Fillet arm composes cleanly with the
    /// pre-existing Transform arm — the two recursions are
    /// indistinguishable to the resolver and chain freely.
    #[test]
    fn resolver_cuboid_transform_fillet_chains_through_both() {
        let owner = BRepOwnerId::from_bytes([0x77; 16]);
        let cuboid = CuboidOp {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
        };

        let direct_ids: Vec<BRepFaceId> = cuboid
            .brep_face_ids(owner)
            .into_iter()
            .map(|(_, id)| id)
            .collect();

        let edges = cuboid.brep_edge_ids(owner);
        let fillet = FilletOp::new(&cuboid, owner, vec![edges[0]], 0.1).expect("fillet");

        let mut cad = CadGraph::new();
        cad.begin_operation().expect("begin");
        let cuboid_node = cad
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::Cuboid(cuboid))
            .expect("add cuboid");
        let xform_node = cad
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::Transform(TransformOp {
                translation: [3.0, 0.0, 0.0],
                rotation_quat_xyzw: [0.0, 0.0, 0.0, 1.0],
                scale: [1.0, 1.0, 1.0],
            }))
            .expect("add transform");
        let fillet_node = cad
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::Fillet(fillet))
            .expect("add fillet");
        cad.graph_mut()
            .expect("mut")
            .connect(cuboid_node, xform_node, 0)
            .expect("connect cuboid->xform");
        cad.graph_mut()
            .expect("mut")
            .connect(xform_node, fillet_node, 0)
            .expect("connect xform->fillet");
        cad.commit("cuboid -> transform -> fillet").expect("commit");

        let chain_ids: Vec<BRepFaceId> = brep_face_ids_for_node(cad.graph(), fillet_node, owner)
            .expect("resolve through both arms")
            .into_iter()
            .map(|(_, id)| id)
            .collect();

        assert_eq!(
            chain_ids, direct_ids,
            "Cuboid -> Transform -> Fillet must inherit Cuboid IDs unchanged"
        );
    }
}
