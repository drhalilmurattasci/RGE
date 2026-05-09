//! Graph-level B-Rep edge-identity resolver.
//!
//! Mirror of [`crate::topology::resolve`] for edges. Direct providers
//! (Cuboid/Extrude/Revolve/Loft) yield their own edge IDs via
//! [`BRepEdgeProvider`]; Transform recurses to its single upstream
//! input and returns those edges unchanged; Boolean / Sweep / any
//! future [`OperatorNode`] variant return
//! [`BRepResolveError::TopologyChangingOperator`].
//!
//! Failure class inherited: snapshot-recoverable.
//!
//! # Architectural posture (sub-7.2-ζ.ε)
//!
//! Edge identity composes with face identity by construction:
//! [`BRepEdgeId`] is derived from the operator's own [`BRepFaceId`]s
//! (sub-7.2-ζ.α), so any topology preservation/break that holds for
//! face IDs also holds for edge IDs at the same graph node. Transform
//! inheritance preserves both. The compositional-honesty check for
//! mode transitions (e.g. Revolve full↔partial) was tested at the
//! direct-provider layer (sub-7.2-ζ.γ) and propagates through this
//! resolver automatically.
//!
//! [`BRepResolveError`] is reused from [`crate::topology::resolve`]
//! unchanged. The hypothetical "operator implements [`BRepProvider`]
//! but not [`BRepEdgeProvider`]" case has no current consumer (all
//! four current direct face providers also implement edge provider),
//! and the catch-all `_ => Err(TopologyChangingOperator)` covers
//! Boolean / Sweep / any future variant identically to the face
//! resolver. If/when that hypothetical case arises, the error
//! vocabulary will be revisited then.
//!
//! Sub-7.2-ζ.ε ships graph-level Transform inheritance for edges
//! ONLY. The Phase 7.2 stress-test gate-closure (sub-7.2-ζ.ζ:
//! 100 chains × 10 random rebuilds with face+edge IDs preserved per
//! [`crate::TopologyEvolution`]) is the next and final dispatch
//! before the Phase 7.2 IMPLEMENTATION.md exit criterion closes.
//!
//! [`BRepProvider`]: crate::topology::BRepProvider
//! [`BRepFaceId`]: crate::topology::BRepFaceId

use std::collections::HashSet;

use rge_kernel_graph_foundation::NodeId;

use crate::graph::OperatorGraph;
use crate::operators::OperatorNode;
use crate::topology::{BRepEdgeId, BRepEdgeProvider, BRepOwnerId, BRepResolveError};

// ---------------------------------------------------------------------------
// Public resolver entry point
// ---------------------------------------------------------------------------

/// Resolve B-Rep edge identity for a node in an [`OperatorGraph`],
/// dispatching by operator kind.
///
/// * Direct providers (Cuboid / Extrude / Revolve / Loft): call
///   [`BRepEdgeProvider::brep_edge_ids`] on the operator-variant
///   payload.
/// * Identity-preserving (Transform): recurse to the unique input
///   node and return its edges unchanged. The owner is propagated
///   verbatim — same discipline as
///   [`crate::topology::brep_face_ids_for_node`].
/// * Topology-changing (Boolean, Sweep, any future variant): return
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
/// See [`BRepResolveError`] variants (re-used unchanged from the face
/// resolver — `NodeNotInGraph`, `TopologyChangingOperator`,
/// `UnexpectedArity`, `CycleDetected`). All errors are structural
/// graph-shape problems or explicit "operator unsupported" signals;
/// no internal panic path exists.
pub fn brep_edge_ids_for_node(
    graph: &OperatorGraph,
    node: NodeId,
    owner: BRepOwnerId,
) -> Result<Vec<BRepEdgeId>, BRepResolveError> {
    let mut in_flight = HashSet::new();
    resolve_recursive(graph, node, owner, &mut in_flight)
}

// ---------------------------------------------------------------------------
// Internal recursion
// ---------------------------------------------------------------------------

/// Recursive helper. `in_flight` mirrors
/// [`OperatorGraph::evaluate`]'s in-flight `HashSet<NodeId>` pattern:
/// every recursion entry inserts the node id, returns
/// [`BRepResolveError::CycleDetected`] on double-insertion, and
/// unconditionally removes the id on exit so sibling subtrees with
/// overlapping ancestors still resolve.
fn resolve_recursive(
    graph: &OperatorGraph,
    node: NodeId,
    owner: BRepOwnerId,
    in_flight: &mut HashSet<NodeId>,
) -> Result<Vec<BRepEdgeId>, BRepResolveError> {
    if !in_flight.insert(node) {
        return Err(BRepResolveError::CycleDetected { node });
    }

    let result = resolve_step(graph, node, owner, in_flight);

    in_flight.remove(&node);
    result
}

/// Single dispatch step. Looking up `node`, then matching on the
/// [`OperatorNode`] variant: direct providers call into their
/// [`BRepEdgeProvider`] impl, [`OperatorNode::Transform`] recurses to
/// its unique input, every other arm — including the catch-all that
/// absorbs future variants — returns
/// [`BRepResolveError::TopologyChangingOperator`].
fn resolve_step(
    graph: &OperatorGraph,
    node: NodeId,
    owner: BRepOwnerId,
    in_flight: &mut HashSet<NodeId>,
) -> Result<Vec<BRepEdgeId>, BRepResolveError> {
    let operator_node = graph
        .node(node)
        .ok_or(BRepResolveError::NodeNotInGraph { node })?;

    match operator_node {
        OperatorNode::Cuboid(op) => Ok(op.brep_edge_ids(owner)),
        OperatorNode::Extrude(op) => Ok(op.brep_edge_ids(owner)),
        OperatorNode::Revolve(op) => Ok(op.brep_edge_ids(owner)),
        OperatorNode::Loft(op) => Ok(op.brep_edge_ids(owner)),

        OperatorNode::Transform(_) => {
            // TransformOp is arity 1 — exactly one EdgeKind::Input(port=0)
            // edge. Walk incoming edges to find the single input, then
            // recurse. Owner propagates unchanged.
            let upstream = single_input_node(graph, node)?;
            resolve_recursive(graph, upstream, owner, in_flight)
        }

        // Catch-all for topology-changing operators (Boolean, Sweep)
        // AND for any future OperatorNode variant added without
        // explicit handling. The kind is read via the Operator trait
        // so the error message is accurate. Mirrors
        // `topology::resolve::resolve_step`.
        other => Err(BRepResolveError::TopologyChangingOperator {
            kind: other.as_operator().op_kind(),
        }),
    }
}

/// Walk a node's incoming edges and return the single upstream
/// [`NodeId`]. Used for arity-1 identity-preserving operators
/// (Transform today). Returns
/// [`BRepResolveError::UnexpectedArity`] if the node has anything
/// other than exactly one incoming edge.
///
/// Duplicate of [`crate::topology::resolve`]'s private helper — kept
/// independent here so the face resolver stays byte-identical
/// regardless of edge-resolver changes.
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
        BooleanOp, CuboidOp, ExtrudeOp, OpKind, Polygon2D, Polyline3D, RevolveOp, SweepOp,
        TransformOp,
    };
    use crate::topology::BRepEdgeProvider;
    use crate::CadGraph;

    fn unit_square() -> Polygon2D {
        Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]).expect("square")
    }

    /// Fresh / synthetic [`NodeId`] — not in any graph — surfaces
    /// [`BRepResolveError::NodeNotInGraph`] from the edge resolver.
    /// Mirror of `topology::resolve::tests::error_node_not_in_graph_returns_correct_variant`.
    #[test]
    fn error_node_not_in_graph_returns_correct_variant() {
        let graph = OperatorGraph::new();
        let fresh = NodeId::from_raw(0xdead_beef_cafe_babe_u128);
        let owner = BRepOwnerId::from_bytes([0x00; 16]);

        let err = brep_edge_ids_for_node(&graph, fresh, owner)
            .expect_err("fresh NodeId must produce error");
        assert_eq!(err, BRepResolveError::NodeNotInGraph { node: fresh });
    }

    /// Verify the [`BRepResolveError::TopologyChangingOperator`] error
    /// carries the correct [`OpKind`] for both Boolean and Sweep — the
    /// two known v0 unsupported operators. Mirror of the corresponding
    /// face-resolver unit test.
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
        let err = brep_edge_ids_for_node(cad.graph(), bool_node, owner)
            .expect_err("Boolean must produce error");
        assert_eq!(
            err,
            BRepResolveError::TopologyChangingOperator {
                kind: OpKind::Boolean
            }
        );

        // Sweep
        let mut cad2 = CadGraph::new();
        cad2.begin_operation().expect("begin");
        let path = Polyline3D::new(vec![[0.0, 0.0, 0.0], [0.0, 0.0, 1.0]]).expect("path");
        let sweep_node = cad2
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::Sweep(SweepOp::new(unit_square(), path)))
            .expect("sweep");
        cad2.commit("sweep").expect("commit");

        let err2 = brep_edge_ids_for_node(cad2.graph(), sweep_node, owner)
            .expect_err("Sweep must produce error");
        assert_eq!(
            err2,
            BRepResolveError::TopologyChangingOperator {
                kind: OpKind::Sweep
            }
        );
    }

    /// Building a Transform node with zero incoming edges is
    /// structurally possible because [`OperatorGraph::connect`] does
    /// not validate arity at attach time. Resolving against it surfaces
    /// [`BRepResolveError::UnexpectedArity`] rather than panicking.
    /// Mirror of the corresponding face-resolver unit test.
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
        let err = brep_edge_ids_for_node(cad.graph(), xform_node, owner)
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

    /// Direct provider through the resolver matches a direct
    /// [`BRepEdgeProvider::brep_edge_ids`] call. Proves the resolver
    /// doesn't perturb a leaf-direct-provider's edge IDs.
    #[test]
    fn resolver_direct_cuboid_matches_direct_provider_call() {
        let cuboid = CuboidOp {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
        };
        let owner = BRepOwnerId::from_bytes([0x33; 16]);

        let direct: Vec<BRepEdgeId> = cuboid.brep_edge_ids(owner);

        let mut cad = CadGraph::new();
        cad.begin_operation().expect("begin");
        let cuboid_node = cad
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::Cuboid(cuboid))
            .expect("cuboid");
        cad.commit("cuboid").expect("commit");

        let through_resolver: Vec<BRepEdgeId> =
            brep_edge_ids_for_node(cad.graph(), cuboid_node, owner).expect("resolve");

        assert_eq!(through_resolver, direct);
    }

    /// Direct provider through the resolver works for Extrude /
    /// Revolve / Loft variants as well — covers all four direct-edge-
    /// provider arms of the resolver match.
    #[test]
    fn resolver_direct_extrude_revolve_loft_match_direct_provider() {
        use crate::operators::LoftOp;

        let owner = BRepOwnerId::from_bytes([0x44; 16]);

        // Extrude
        let extrude = ExtrudeOp::new(unit_square(), 1.0).expect("extrude");
        let extrude_direct: Vec<BRepEdgeId> = extrude.brep_edge_ids(owner);
        let mut cad = CadGraph::new();
        cad.begin_operation().expect("begin");
        let extrude_node = cad
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::Extrude(extrude))
            .expect("extrude");
        cad.commit("extrude").expect("commit");
        let extrude_chain: Vec<BRepEdgeId> =
            brep_edge_ids_for_node(cad.graph(), extrude_node, owner).expect("resolve extrude");
        assert_eq!(extrude_chain, extrude_direct);

        // Revolve
        let revolve_profile = Polygon2D::new(vec![[1.0, 0.0], [2.0, 0.0], [2.0, 1.0], [1.0, 1.0]])
            .expect("revolve profile");
        let revolve = RevolveOp::new(revolve_profile, 6).expect("revolve");
        let revolve_direct: Vec<BRepEdgeId> = revolve.brep_edge_ids(owner);
        let mut cad2 = CadGraph::new();
        cad2.begin_operation().expect("begin");
        let revolve_node = cad2
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::Revolve(revolve))
            .expect("revolve");
        cad2.commit("revolve").expect("commit");
        let revolve_chain: Vec<BRepEdgeId> =
            brep_edge_ids_for_node(cad2.graph(), revolve_node, owner).expect("resolve revolve");
        assert_eq!(revolve_chain, revolve_direct);

        // Loft
        let loft_a = unit_square();
        let loft_b =
            Polygon2D::new(vec![[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]]).expect("loft b");
        let loft = LoftOp::new(loft_a, loft_b, 1.5).expect("loft");
        let loft_direct: Vec<BRepEdgeId> = loft.brep_edge_ids(owner);
        let mut cad3 = CadGraph::new();
        cad3.begin_operation().expect("begin");
        let loft_node = cad3
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::Loft(loft))
            .expect("loft");
        cad3.commit("loft").expect("commit");
        let loft_chain: Vec<BRepEdgeId> =
            brep_edge_ids_for_node(cad3.graph(), loft_node, owner).expect("resolve loft");
        assert_eq!(loft_chain, loft_direct);
    }
}
