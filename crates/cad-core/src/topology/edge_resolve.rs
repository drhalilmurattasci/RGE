//! Graph-level B-Rep edge-identity resolver.
//!
//! Mirror of [`crate::topology::resolve`] for edges. Direct providers
//! (Cuboid/Extrude/Revolve/Loft/Sweep) yield their own edge IDs via
//! [`BRepEdgeProvider`]; identity-preserving operators (Transform,
//! Fillet) recurse to their single upstream input — Transform returns
//! upstream edges unchanged, Fillet returns upstream edges minus the
//! filleted selection; Boolean / any future [`OperatorNode`]
//! variant return [`BRepResolveError::TopologyChangingOperator`].
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
//! five current direct face providers also implement edge provider),
//! and the catch-all `_ => Err(TopologyChangingOperator)` covers
//! Boolean and any future variant. If/when that hypothetical case
//! arises, the error vocabulary will be revisited then.
//!
//! Sub-7.2-ζ.ε ships graph-level Transform inheritance for edges
//! ONLY. The Phase 7.2 stress-test gate-closure (sub-7.2-ζ.ζ:
//! 100 chains × 10 random rebuilds with face+edge IDs preserved per
//! [`crate::TopologyEvolution`]) is the next and final dispatch
//! before the Phase 7.2 IMPLEMENTATION.md exit criterion closes.
//!
//! # Architectural posture (D-Fillet sub-ε.β)
//!
//! Extends the identity-preserving arm to [`OperatorNode::Fillet`]
//! with filtered inheritance: the resolver recurses to the upstream,
//! retrieves the upstream's edges, then filters out the FilletOp's
//! selected edges (`op.edges()`) before returning. The construction-
//! time invariants on [`crate::operators::FilletError`]
//! (`EmptyEdgeSelection`, `EdgeNotInUpstream`,
//! `UnsupportedEdgeGeometry`) make the filter total: every member of
//! `op.edges()` was originally a member of
//! `upstream.brep_edge_ids(op.owner())`.
//!
//! Adjacent edges (sharing a corner vertex with a filleted edge)
//! retain bit-identical 2-endpoint geometry — the chamfer caps add
//! new vertices/triangles incident at the shared corner but do NOT
//! modify the adjacent edge's own geometry. Since
//! [`BRepEdgeId::for_face_pair`] derives identity from face IDs only
//! and faces inherit unchanged through Fillet (sub-ε.α), adjacent
//! edges' byte-identity is preserved and they belong to the
//! "inherited" set.
//!
//! **Owner-discipline invariant**: the resolver propagates the
//! caller's `owner` argument unchanged to the upstream recursion. If
//! `caller_owner ≠ op.owner()`, the filter trivially passes every
//! edge (edge bytes don't match across owner spaces). Callers should
//! resolve with the same owner the FilletOp was constructed against
//! — the substrate does not enforce this, matching the existing
//! resolver discipline (`owner` is informational beyond the caller's
//! identity space).
//!
//! **Filter complexity**: `Vec::contains` is O(n·m) for n filleted
//! and m upstream edges. n is small in practice (1-4) and m is
//! bounded per upstream operator. Fine for v0; future
//! `HashSet<BRepEdgeId>` upgrade is a one-line optimization if scale
//! pressure surfaces.
//!
//! [`BRepProvider`]: crate::topology::BRepProvider
//! [`BRepFaceId`]: crate::topology::BRepFaceId
//! [`BRepEdgeId::for_face_pair`]: crate::topology::BRepEdgeId::for_face_pair

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
/// * Direct providers (Cuboid / Extrude / Revolve / Loft / Sweep):
///   call [`BRepEdgeProvider::brep_edge_ids`] on the operator-variant
///   payload.
/// * Identity-preserving (Transform): recurse to the unique input
///   node and return its edges unchanged. The owner is propagated
///   verbatim — same discipline as
///   [`crate::topology::brep_face_ids_for_node`].
/// * Filtered-inheriting (Fillet): recurse to the unique input node
///   and return its edges minus the FilletOp's filleted selection.
///   Filleted edges are excluded because they lose 2-endpoint
///   geometry under chamfering (D-Fillet sub-ε.β); non-filleted
///   upstream edges retain bit-identical byte-identity.
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
/// its unique input unchanged, [`OperatorNode::Fillet`] recurses then
/// filters out the filleted selection, every other arm — including
/// the catch-all that absorbs future variants — returns
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
        OperatorNode::Sweep(op) => Ok(op.brep_edge_ids(owner)),

        OperatorNode::Transform(_) => {
            // TransformOp is arity 1 — exactly one EdgeKind::Input(port=0)
            // edge. Walk incoming edges to find the single input, then
            // recurse. Owner propagates unchanged.
            let upstream = single_input_node(graph, node)?;
            resolve_recursive(graph, upstream, owner, in_flight)
        }

        OperatorNode::Fillet(op) => {
            // FilletOp is arity 1. Recurse to the unique input to get
            // the upstream's edge set, then filter out the FilletOp's
            // filleted selection: filleted edges lose 2-endpoint
            // geometry under chamfering (the original edge is bit-
            // identical in the output mesh, but chamfer caps are
            // attached adjacent to it — strict topology partition
            // marks them as "modified"). Non-filleted upstream edges
            // retain bit-identical byte-identity (FilletOp.evaluate
            // appends; never modifies upstream positions/indices).
            // Filter complexity is O(n*m); see module-doc.
            let upstream = single_input_node(graph, node)?;
            let upstream_edges = resolve_recursive(graph, upstream, owner, in_flight)?;
            let filleted = op.edges();
            let filtered: Vec<BRepEdgeId> = upstream_edges
                .into_iter()
                .filter(|edge| !filleted.contains(edge))
                .collect();
            Ok(filtered)
        }

        OperatorNode::RoundFillet(_) => {
            // RoundFilletOp is arity 1. Per ADR-119 D2 (curved-edge
            // inheritance), filleted edges KEEP their `BRepEdgeId`
            // because `BRepEdgeId::for_face_pair` derives identity from
            // the two adjacent faces' IDs, NOT from edge shape. The
            // edge's geometry changes from a sharp line to a smooth
            // arc, but the two faces it bounds (and their identities)
            // are unchanged — the edge IS the topological intersection
            // of those two faces, regardless of cross-section. This is
            // the substantive divergence from chamfer's edge-resolver
            // arm: chamfer FilletOp strips selected edges from the
            // surviving set (sub-ε.β), but RoundFilletOp preserves
            // ALL upstream edges including the filleted ones —
            // matching ADR D2's curved-edge-inheritance shape.
            //
            // Recurse to the unique input and return its edges
            // unchanged (same pattern as Transform).
            let upstream = single_input_node(graph, node)?;
            resolve_recursive(graph, upstream, owner, in_flight)
        }

        // Catch-all for the topology-changing Boolean operator AND for
        // any future OperatorNode variant added without explicit
        // handling. The kind is read via the Operator trait so the
        // error message is accurate. Mirrors
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
        BooleanOp, CuboidOp, ExtrudeOp, FilletOp, OpKind, Polygon2D, Polyline3D, RevolveOp,
        RoundFilletOp, SweepOp, TransformOp,
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
    /// carries the correct [`OpKind`] for Boolean — the remaining v0
    /// topology-changing operator for edge resolution (Sweep now
    /// resolves directly). Mirror of the corresponding face-resolver
    /// unit test.
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
    }

    /// Sweep through the resolver returns `Ok` and exactly equals a
    /// direct [`BRepEdgeProvider::brep_edge_ids`] call — for both a
    /// 2-point path and a multi-segment path. Proves the resolver routes
    /// `OperatorNode::Sweep` to the direct provider without perturbing
    /// its edge IDs.
    #[test]
    fn resolver_direct_sweep_matches_direct_provider() {
        let owner = BRepOwnerId::from_bytes([0xa5; 16]);

        // 2-point path: n=4, s=1 → 12 edges.
        let path_2 = Polyline3D::new(vec![[0.0, 0.0, 0.0], [0.0, 0.0, 1.0]]).expect("2-point path");
        let sweep_2 = SweepOp::new(unit_square(), path_2);
        let direct_2: Vec<BRepEdgeId> = sweep_2.brep_edge_ids(owner);
        let mut cad = CadGraph::new();
        cad.begin_operation().expect("begin");
        let sweep_node = cad
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::Sweep(sweep_2))
            .expect("sweep");
        cad.commit("sweep 2-point").expect("commit");
        let through_2: Vec<BRepEdgeId> =
            brep_edge_ids_for_node(cad.graph(), sweep_node, owner).expect("resolve 2-point sweep");
        assert_eq!(through_2, direct_2);
        assert_eq!(through_2.len(), 12);

        // Multi-segment path: n=3, s=2 → 15 edges.
        let path_3 = Polyline3D::new(vec![[0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, 2.0]])
            .expect("3-point path");
        let triangle =
            Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]]).expect("triangle profile");
        let sweep_3 = SweepOp::new(triangle, path_3);
        let direct_3: Vec<BRepEdgeId> = sweep_3.brep_edge_ids(owner);
        let mut cad2 = CadGraph::new();
        cad2.begin_operation().expect("begin");
        let sweep_node_3 = cad2
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::Sweep(sweep_3))
            .expect("sweep");
        cad2.commit("sweep 3-point").expect("commit");
        let through_3: Vec<BRepEdgeId> = brep_edge_ids_for_node(cad2.graph(), sweep_node_3, owner)
            .expect("resolve multi-segment sweep");
        assert_eq!(through_3, direct_3);
        assert_eq!(through_3.len(), 15);
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

    /// D-Fillet sub-ε.β load-bearing assertion (single-edge case):
    /// Cuboid → Fillet(edges=[e0]) returns 11 edges = upstream's 12
    /// minus the filleted one. The surviving edges are byte-equal to
    /// the upstream's non-selected slice — they retain their full
    /// `BRepEdgeId` byte identity through Fillet because
    /// [`crate::topology::BRepEdgeId::for_face_pair`] derives identity
    /// from face IDs only and face IDs inherit unchanged (sub-ε.α).
    #[test]
    fn resolver_cuboid_then_fillet_filters_selected_edges_inherits_rest() {
        let owner = BRepOwnerId::from_bytes([0x55; 16]);
        let cube = CuboidOp {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
        };
        let upstream_edges = cube.brep_edge_ids(owner);
        assert_eq!(upstream_edges.len(), 12, "cuboid has 12 edges");

        let filleted = vec![upstream_edges[0]];
        let fillet = FilletOp::new(&cube, owner, filleted.clone(), 0.1).expect("fillet");

        let mut cad = CadGraph::new();
        cad.begin_operation().expect("begin");
        let cube_node = cad
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::Cuboid(cube))
            .expect("add cuboid");
        let fillet_node = cad
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::Fillet(fillet))
            .expect("add fillet");
        cad.graph_mut()
            .expect("mut")
            .connect(cube_node, fillet_node, 0)
            .expect("connect cuboid->fillet");
        cad.commit("cuboid -> fillet").expect("commit");

        let chain_edges: Vec<BRepEdgeId> = brep_edge_ids_for_node(cad.graph(), fillet_node, owner)
            .expect("resolve cuboid->fillet chain");

        let expected: Vec<BRepEdgeId> = upstream_edges
            .iter()
            .filter(|e| !filleted.contains(e))
            .copied()
            .collect();
        assert_eq!(chain_edges.len(), 11);
        assert_eq!(
            chain_edges, expected,
            "Fillet edge resolver must filter selected and inherit rest byte-equal"
        );
    }

    /// D-Fillet sub-ε.β multi-edge selection: 3 filleted edges → 9
    /// inherited. Order-preserving filter: the result preserves
    /// upstream emission order minus the selected set.
    #[test]
    fn resolver_cuboid_then_fillet_multi_edge_selection_filters_all_selected() {
        let owner = BRepOwnerId::from_bytes([0x66; 16]);
        let cube = CuboidOp {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
        };
        let upstream_edges = cube.brep_edge_ids(owner);
        let filleted = vec![upstream_edges[0], upstream_edges[3], upstream_edges[7]];
        let fillet = FilletOp::new(&cube, owner, filleted.clone(), 0.05).expect("fillet");

        let mut cad = CadGraph::new();
        cad.begin_operation().expect("begin");
        let cube_node = cad
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::Cuboid(cube))
            .expect("add cuboid");
        let fillet_node = cad
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::Fillet(fillet))
            .expect("add fillet");
        cad.graph_mut()
            .expect("mut")
            .connect(cube_node, fillet_node, 0)
            .expect("connect");
        cad.commit("cuboid -> fillet (3 edges)").expect("commit");

        let chain_edges: Vec<BRepEdgeId> =
            brep_edge_ids_for_node(cad.graph(), fillet_node, owner).expect("resolve");
        let expected: Vec<BRepEdgeId> = upstream_edges
            .iter()
            .filter(|e| !filleted.contains(e))
            .copied()
            .collect();
        assert_eq!(chain_edges.len(), 9);
        assert_eq!(chain_edges, expected);
    }

    /// D-Fillet sub-ε.β boundary case: filleting ALL 12 cuboid edges
    /// returns an empty Vec. Empty result is a legal substrate
    /// outcome (distinct from `TopologyChangingOperator` — the
    /// resolver succeeds with zero surviving edges).
    #[test]
    fn resolver_cuboid_then_fillet_all_edges_selected_returns_empty() {
        let owner = BRepOwnerId::from_bytes([0x77; 16]);
        let cube = CuboidOp {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
        };
        let all_edges = cube.brep_edge_ids(owner);
        let fillet = FilletOp::new(&cube, owner, all_edges.clone(), 0.1).expect("fillet");

        let mut cad = CadGraph::new();
        cad.begin_operation().expect("begin");
        let cube_node = cad
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::Cuboid(cube))
            .expect("add cuboid");
        let fillet_node = cad
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::Fillet(fillet))
            .expect("add fillet");
        cad.graph_mut()
            .expect("mut")
            .connect(cube_node, fillet_node, 0)
            .expect("connect");
        cad.commit("cuboid -> fillet (all edges)").expect("commit");

        let chain_edges: Vec<BRepEdgeId> =
            brep_edge_ids_for_node(cad.graph(), fillet_node, owner).expect("resolve");
        assert!(
            chain_edges.is_empty(),
            "filleting all upstream edges must yield empty edge set"
        );
    }

    /// D-Fillet sub-ε.β rebuild-stability: three different radii
    /// against the same edge selection produce byte-identical
    /// surviving edges. Radius enters chamfer geometry only, never
    /// edge-identity filtering.
    #[test]
    fn resolver_fillet_edge_set_stable_under_radius_parameter_change() {
        let owner = BRepOwnerId::from_bytes([0x88; 16]);
        let cube_template = CuboidOp {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
        };
        let upstream_edges = cube_template.brep_edge_ids(owner);
        let filleted = vec![upstream_edges[2]];

        let build = |radius: f32| -> Vec<BRepEdgeId> {
            let cube = cube_template.clone();
            let fillet = FilletOp::new(&cube, owner, filleted.clone(), radius).expect("fillet");
            let mut cad = CadGraph::new();
            cad.begin_operation().expect("begin");
            let cube_node = cad
                .graph_mut()
                .expect("mut")
                .add_operator(OperatorNode::Cuboid(cube))
                .expect("add cuboid");
            let fillet_node = cad
                .graph_mut()
                .expect("mut")
                .add_operator(OperatorNode::Fillet(fillet))
                .expect("add fillet");
            cad.graph_mut()
                .expect("mut")
                .connect(cube_node, fillet_node, 0)
                .expect("connect");
            cad.commit("cuboid -> fillet").expect("commit");
            brep_edge_ids_for_node(cad.graph(), fillet_node, owner).expect("resolve")
        };

        let edges_at_01 = build(0.1);
        let edges_at_02 = build(0.2);
        let edges_at_05 = build(0.5);

        assert_eq!(
            edges_at_01, edges_at_02,
            "fillet radius 0.1 -> 0.2 must not change surviving edge set"
        );
        assert_eq!(
            edges_at_02, edges_at_05,
            "fillet radius 0.2 -> 0.5 must not change surviving edge set"
        );
        assert_eq!(edges_at_01.len(), 11);
    }

    /// ADR-119 sub-α load-bearing assertion (single-edge case):
    /// Cuboid → RoundFillet returns ALL 12 upstream edges
    /// byte-identical, INCLUDING the filleted one. Per ADR D2,
    /// curved-edge inheritance preserves `BRepEdgeId` because
    /// `for_face_pair` derives identity from face IDs (which inherit
    /// unchanged per sub-α D4) — not from edge shape.
    #[test]
    fn resolver_cuboid_then_round_fillet_inherits_all_edges_including_filleted() {
        let owner = BRepOwnerId::from_bytes([0xb1; 16]);
        let cube = CuboidOp {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
        };
        let upstream_edges = cube.brep_edge_ids(owner);
        assert_eq!(upstream_edges.len(), 12, "cuboid has 12 edges");

        let filleted = vec![upstream_edges[0]];
        let round = RoundFilletOp::new(&cube, owner, filleted.clone(), 0.1).expect("round");

        let mut cad = CadGraph::new();
        cad.begin_operation().expect("begin");
        let cube_node = cad
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::Cuboid(cube))
            .expect("add cuboid");
        let round_node = cad
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::RoundFillet(round))
            .expect("add round");
        cad.graph_mut()
            .expect("mut")
            .connect(cube_node, round_node, 0)
            .expect("connect cuboid->round");
        cad.commit("cuboid -> round").expect("commit");

        let chain_edges: Vec<BRepEdgeId> = brep_edge_ids_for_node(cad.graph(), round_node, owner)
            .expect("resolve cuboid->round chain");

        assert_eq!(
            chain_edges.len(),
            12,
            "RoundFillet preserves ALL upstream edges (ADR-119 D2)"
        );
        assert_eq!(
            chain_edges, upstream_edges,
            "RoundFillet edges must be byte-equal to upstream's — including the filleted one"
        );
        // The filleted edge is in the surviving set — directly opposite
        // to chamfer FilletOp's filter behavior.
        assert!(
            chain_edges.contains(&filleted[0]),
            "the filleted edge must survive RoundFillet's edge resolver \
             (curved-edge inheritance per ADR-119 D2)"
        );
    }

    /// ADR-119 sub-α multi-edge inheritance: RoundFillet preserves the
    /// full upstream edge set regardless of selection size or which
    /// edges are filleted. Distinguishes RoundFillet's edge-resolver
    /// behavior from chamfer FilletOp's "filter-the-selection" arm.
    #[test]
    fn resolver_round_fillet_preserves_all_edges_regardless_of_selection() {
        let owner = BRepOwnerId::from_bytes([0xb2; 16]);
        let cube = CuboidOp {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
        };
        let upstream_edges = cube.brep_edge_ids(owner);

        // Try several selection sizes; all should produce 12 surviving
        // edges (ALL upstream edges inherited).
        let selections = vec![
            vec![upstream_edges[0]],
            vec![upstream_edges[0], upstream_edges[3], upstream_edges[7]],
            upstream_edges.clone(), // all 12
        ];

        for filleted in selections {
            let round = RoundFilletOp::new(&cube, owner, filleted.clone(), 0.05).expect("round");
            let mut cad = CadGraph::new();
            cad.begin_operation().expect("begin");
            let cube_node = cad
                .graph_mut()
                .expect("mut")
                .add_operator(OperatorNode::Cuboid(cube.clone()))
                .expect("add cuboid");
            let round_node = cad
                .graph_mut()
                .expect("mut")
                .add_operator(OperatorNode::RoundFillet(round))
                .expect("add round");
            cad.graph_mut()
                .expect("mut")
                .connect(cube_node, round_node, 0)
                .expect("connect");
            cad.commit("cuboid -> round").expect("commit");

            let chain_edges: Vec<BRepEdgeId> =
                brep_edge_ids_for_node(cad.graph(), round_node, owner).expect("resolve");
            assert_eq!(
                chain_edges,
                upstream_edges,
                "RoundFillet with {} filleted edges must preserve full upstream set",
                filleted.len()
            );
        }
    }

    /// D-Fillet sub-ε.β composition with Transform arm
    /// (sub-7.2-ζ.ε): Cuboid → Transform → Fillet inherits Cuboid
    /// edges through Transform, then filters at Fillet. Result is
    /// byte-equal to direct Cuboid edges minus the filleted set.
    #[test]
    fn resolver_cuboid_transform_fillet_chains_edges_through_both() {
        let owner = BRepOwnerId::from_bytes([0x99; 16]);
        let cube = CuboidOp {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
        };
        let upstream_edges = cube.brep_edge_ids(owner);
        let filleted = vec![upstream_edges[5]];
        let fillet = FilletOp::new(&cube, owner, filleted.clone(), 0.1).expect("fillet");

        let mut cad = CadGraph::new();
        cad.begin_operation().expect("begin");
        let cube_node = cad
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::Cuboid(cube))
            .expect("add cuboid");
        let xform_node = cad
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::Transform(TransformOp {
                translation: [2.0, 0.0, 0.0],
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
            .connect(cube_node, xform_node, 0)
            .expect("connect cuboid->xform");
        cad.graph_mut()
            .expect("mut")
            .connect(xform_node, fillet_node, 0)
            .expect("connect xform->fillet");
        cad.commit("cuboid -> transform -> fillet").expect("commit");

        let chain_edges: Vec<BRepEdgeId> = brep_edge_ids_for_node(cad.graph(), fillet_node, owner)
            .expect("resolve through both arms");
        let expected: Vec<BRepEdgeId> = upstream_edges
            .iter()
            .filter(|e| !filleted.contains(e))
            .copied()
            .collect();
        assert_eq!(chain_edges.len(), 11);
        assert_eq!(
            chain_edges, expected,
            "Cuboid -> Transform -> Fillet must inherit Cuboid edges minus filleted"
        );
    }
}
