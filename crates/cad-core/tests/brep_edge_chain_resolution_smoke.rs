//! End-to-end smoke for the sub-7.2-ζ.ε graph-level B-Rep edge-
//! identity resolver (Transform inheritance, no `BRepEdgeProvider`
//! for `TransformOp`).
//!
//! Mirror of [`brep_chain_resolution_smoke`] for edges. These tests
//! are the gate for the dispatch — they prove:
//!
//! 1. Direct providers through the resolver match a direct
//!    [`BRepEdgeProvider::brep_edge_ids`] call (no double-processing).
//! 2. A Cuboid → Transform chain inherits the Cuboid's edge IDs
//!    verbatim.
//! 3. Three different Transform parameter sets (translate / rotate /
//!    scale) all preserve identity — placement is not topology.
//! 4. Multi-hop Transform chains inherit upstream edge IDs unchanged.
//! 5. Different owners stay disjoint through Transform chains.
//! 6. Boolean returns [`BRepResolveError::TopologyChangingOperator`].
//! 7. Sweep returns [`BRepResolveError::TopologyChangingOperator`].
//! 8. An unknown / fresh `NodeId` returns
//!    [`BRepResolveError::NodeNotInGraph`].
//! 9. Sub-7.2-ε face resolver and ζ.ε edge resolver agree on Transform
//!    inheritance — running both against the same Cuboid → Transform
//!    chain produces face IDs and edge IDs that match what the direct
//!    Cuboid produces locally.
//!
//! Tests #2 and #4 are the load-bearing inheritance assertions: they
//! pin the architectural decision that Transform is identity-
//! preserving at the resolver layer (NOT a `BRepEdgeProvider` impl on
//! `TransformOp`). Test #9 cross-validates that the two resolvers use
//! the same chain semantics.

use rge_cad_core::{
    brep_edge_ids_for_node, brep_face_ids_for_node, BRepEdgeId, BRepEdgeProvider, BRepFaceId,
    BRepOwnerId, BRepProvider, BRepResolveError, BooleanOp, CadGraph, CuboidOp, OpKind,
    OperatorNode, Polygon2D, Polyline3D, SweepOp, TransformOp,
};
use rge_kernel_graph_foundation::NodeId;

fn unit_cuboid() -> CuboidOp {
    CuboidOp {
        width: 1.0,
        height: 1.0,
        depth: 1.0,
    }
}

fn unit_square() -> Polygon2D {
    Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]).expect("square")
}

/// Direct Cuboid through the resolver returns the same edge IDs as a
/// direct [`BRepEdgeProvider::brep_edge_ids`] call.
///
/// Proves the resolver doesn't double-process or re-derive edge IDs
/// for a leaf direct provider — it just delegates to the operator's
/// own `brep_edge_ids` impl. Mirror of
/// `resolver_against_direct_cuboid_matches_direct_provider` from the
/// face-resolver smoke.
#[test]
fn resolver_against_direct_cuboid_matches_direct_provider() {
    let owner = BRepOwnerId::from_bytes([0x42; 16]);
    let cuboid = unit_cuboid();

    let direct: Vec<BRepEdgeId> = cuboid.brep_edge_ids(owner);

    let mut cad = CadGraph::new();
    cad.begin_operation().expect("begin");
    let cuboid_node = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Cuboid(cuboid))
        .expect("add cuboid");
    cad.commit("cuboid").expect("commit");

    let chain: Vec<BRepEdgeId> =
        brep_edge_ids_for_node(cad.graph(), cuboid_node, owner).expect("resolve direct cuboid");

    assert_eq!(chain, direct);
    assert_eq!(chain.len(), 12);
}

/// Cuboid → Transform: every edge ID exactly equals the direct Cuboid
/// edge ID. This is the load-bearing inheritance assertion of
/// sub-7.2-ζ.ε. Transform changes placement, not topology, so an edge
/// selected before a Transform must remain the same edge after.
#[test]
fn cuboid_through_transform_inherits_cuboid_edges() {
    let owner = BRepOwnerId::from_bytes([0x42; 16]);
    let cuboid = unit_cuboid();

    let direct_edges: Vec<BRepEdgeId> = cuboid.brep_edge_ids(owner);

    let mut cad = CadGraph::new();
    cad.begin_operation().expect("begin");
    let cuboid_node = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Cuboid(cuboid))
        .expect("add cuboid");
    let transform_node = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Transform(TransformOp {
            translation: [5.0, 0.0, 0.0],
            rotation_quat_xyzw: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        }))
        .expect("add transform");
    cad.graph_mut()
        .expect("mut")
        .connect(cuboid_node, transform_node, 0)
        .expect("connect");
    cad.graph_mut()
        .expect("mut")
        .set_root(transform_node)
        .expect("set root");
    cad.commit("cuboid -> transform").expect("commit");

    let chain_edges: Vec<BRepEdgeId> =
        brep_edge_ids_for_node(cad.graph(), transform_node, owner).expect("resolve chain");

    assert_eq!(
        chain_edges, direct_edges,
        "Transform must inherit Cuboid edges unchanged"
    );
    assert_eq!(chain_edges.len(), 12);
}

/// Three Transforms differing in placement parameters (pure translate
/// / pure rotate / pure scale) all produce byte-identical edge IDs
/// because the resolver inherits from the Cuboid upstream and
/// Transform parameters never enter the BLAKE3 derivation.
#[test]
fn transform_parameter_changes_do_not_change_edges() {
    let owner = BRepOwnerId::from_bytes([0x99; 16]);

    // Three Transforms, each with a different placement parameter set.
    let xforms = vec![
        // Pure translate.
        TransformOp {
            translation: [5.0, 0.0, 0.0],
            rotation_quat_xyzw: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        },
        // Pure rotate (~40° around Y; quaternion = [0, sin(20°), 0, cos(20°)]).
        TransformOp {
            translation: [0.0, 0.0, 0.0],
            rotation_quat_xyzw: [0.0, 0.342_020_14, 0.0, 0.939_692_62],
            scale: [1.0, 1.0, 1.0],
        },
        // Pure scale.
        TransformOp {
            translation: [0.0, 0.0, 0.0],
            rotation_quat_xyzw: [0.0, 0.0, 0.0, 1.0],
            scale: [2.5, 2.5, 2.5],
        },
    ];

    let mut all_edges: Vec<Vec<BRepEdgeId>> = Vec::new();
    for xform in xforms {
        let mut cad = CadGraph::new();
        cad.begin_operation().expect("begin");
        let cuboid_node = cad
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::Cuboid(unit_cuboid()))
            .expect("add cuboid");
        let xform_node = cad
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::Transform(xform))
            .expect("add transform");
        cad.graph_mut()
            .expect("mut")
            .connect(cuboid_node, xform_node, 0)
            .expect("connect");
        cad.commit("cuboid -> xform").expect("commit");

        let edges: Vec<BRepEdgeId> =
            brep_edge_ids_for_node(cad.graph(), xform_node, owner).expect("resolve");
        all_edges.push(edges);
    }

    // All three placements must produce the same edges.
    for window in all_edges.windows(2) {
        assert_eq!(
            window[0], window[1],
            "transform parameter change must not change edges"
        );
    }
}

/// Cuboid → Transform → Transform: resolving at the second Transform
/// returns edges equal to the direct Cuboid edges. Multi-hop
/// inheritance.
#[test]
fn multi_hop_transforms_preserve_edges() {
    let owner = BRepOwnerId::from_bytes([0x77; 16]);
    let cuboid = unit_cuboid();

    let direct_edges: Vec<BRepEdgeId> = cuboid.brep_edge_ids(owner);

    let mut cad = CadGraph::new();
    cad.begin_operation().expect("begin");
    let cuboid_node = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Cuboid(cuboid))
        .expect("add cuboid");
    let xform1 = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Transform(TransformOp {
            translation: [3.0, 0.0, 0.0],
            rotation_quat_xyzw: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        }))
        .expect("add xform1");
    let xform2 = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Transform(TransformOp {
            translation: [0.0, 7.0, 0.0],
            rotation_quat_xyzw: [0.0, 0.0, 0.0, 1.0],
            scale: [2.0, 2.0, 2.0],
        }))
        .expect("add xform2");
    cad.graph_mut()
        .expect("mut")
        .connect(cuboid_node, xform1, 0)
        .expect("cuboid->xform1");
    cad.graph_mut()
        .expect("mut")
        .connect(xform1, xform2, 0)
        .expect("xform1->xform2");
    cad.commit("multi-hop").expect("commit");

    let chain_edges: Vec<BRepEdgeId> =
        brep_edge_ids_for_node(cad.graph(), xform2, owner).expect("resolve multi-hop");

    assert_eq!(
        chain_edges, direct_edges,
        "multi-hop Transform chain must inherit Cuboid edges unchanged"
    );
}

/// Different `BRepOwnerId`s flow through Transform chains
/// independently — the chain inherits whatever owner the consumer
/// supplies. No edge ID minted under one owner collides with any
/// minted under the other, matching the direct-provider precedent.
#[test]
fn different_owners_stay_disjoint_through_transform_chain() {
    let owner_x = BRepOwnerId::from_bytes([0x11; 16]);
    let owner_y = BRepOwnerId::from_bytes([0x22; 16]);

    let mut cad = CadGraph::new();
    cad.begin_operation().expect("begin");
    let cuboid_node = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Cuboid(unit_cuboid()))
        .expect("add cuboid");
    let xform_node = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Transform(TransformOp::default()))
        .expect("add transform");
    cad.graph_mut()
        .expect("mut")
        .connect(cuboid_node, xform_node, 0)
        .expect("connect");
    cad.commit("for owner-disambig").expect("commit");

    let edges_x: Vec<BRepEdgeId> =
        brep_edge_ids_for_node(cad.graph(), xform_node, owner_x).expect("owner_x");
    let edges_y: Vec<BRepEdgeId> =
        brep_edge_ids_for_node(cad.graph(), xform_node, owner_y).expect("owner_y");

    for ex in &edges_x {
        assert!(
            !edges_y.contains(ex),
            "owner-disambiguation failed through Transform chain: \
             edge from owner_x found in owner_y's set"
        );
    }
}

/// A Boolean node resolves to
/// [`BRepResolveError::TopologyChangingOperator`] — neither empty
/// edges nor a panic. The error carries [`OpKind::Boolean`].
#[test]
fn boolean_returns_topology_changing_error() {
    let mut cad = CadGraph::new();
    cad.begin_operation().expect("begin");
    let a = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Cuboid(CuboidOp {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
        }))
        .expect("a");
    let b = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Cuboid(CuboidOp {
            width: 1.0001, // distinct hash from `a`
            height: 1.0,
            depth: 1.0,
        }))
        .expect("b");
    let bool_node = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Boolean(BooleanOp::union()))
        .expect("bool");
    cad.graph_mut()
        .expect("mut")
        .connect(a, bool_node, 0)
        .expect("a->bool");
    cad.graph_mut()
        .expect("mut")
        .connect(b, bool_node, 1)
        .expect("b->bool");
    cad.commit("boolean").expect("commit");

    let owner = BRepOwnerId::from_bytes([0xaa; 16]);
    let result = brep_edge_ids_for_node(cad.graph(), bool_node, owner);

    assert!(
        matches!(
            result,
            Err(BRepResolveError::TopologyChangingOperator {
                kind: OpKind::Boolean
            })
        ),
        "Boolean must surface TopologyChangingOperator; got {result:?}"
    );
}

/// A Sweep node resolves to
/// [`BRepResolveError::TopologyChangingOperator`]. The error carries
/// [`OpKind::Sweep`].
#[test]
fn sweep_returns_topology_changing_error() {
    let mut cad = CadGraph::new();
    cad.begin_operation().expect("begin");
    let path = Polyline3D::new(vec![[0.0, 0.0, 0.0], [0.0, 0.0, 1.0]]).expect("path");
    let sweep_node = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Sweep(SweepOp::new(unit_square(), path)))
        .expect("sweep");
    cad.commit("sweep").expect("commit");

    let owner = BRepOwnerId::from_bytes([0xbb; 16]);
    let result = brep_edge_ids_for_node(cad.graph(), sweep_node, owner);

    assert!(
        matches!(
            result,
            Err(BRepResolveError::TopologyChangingOperator {
                kind: OpKind::Sweep
            })
        ),
        "Sweep must surface TopologyChangingOperator; got {result:?}"
    );
}

/// An unknown / fresh [`NodeId`] (one not present in the graph) returns
/// [`BRepResolveError::NodeNotInGraph`].
#[test]
fn unknown_node_returns_not_in_graph_error() {
    // Build a small graph with at least one node so the graph isn't trivially empty.
    let mut cad = CadGraph::new();
    cad.begin_operation().expect("begin");
    let _cuboid_node = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Cuboid(unit_cuboid()))
        .expect("cuboid");
    cad.commit("seed").expect("commit");

    // Synthesize a NodeId that cannot collide with the content-derived
    // cuboid node: u128::MAX is reserved by the test as a synthetic value.
    let fresh = NodeId::from_raw(u128::MAX);
    let owner = BRepOwnerId::from_bytes([0xcc; 16]);

    let err = brep_edge_ids_for_node(cad.graph(), fresh, owner)
        .expect_err("fresh NodeId must produce error");
    assert_eq!(err, BRepResolveError::NodeNotInGraph { node: fresh });
}

/// Cross-substrate compositional check: sub-7.2-ε face resolver and
/// sub-7.2-ζ.ε edge resolver agree on Transform inheritance —
/// running both against the same Cuboid → Transform chain produces
/// face IDs and edge IDs that match what the direct Cuboid produces
/// locally. Cross-validates that the two resolvers use the same
/// chain semantics.
#[test]
fn face_and_edge_resolvers_agree_on_transform_inheritance() {
    let owner = BRepOwnerId::from_bytes([0xcd; 16]);
    let cuboid = unit_cuboid();

    let direct_faces: Vec<BRepFaceId> = cuboid
        .brep_face_ids(owner)
        .into_iter()
        .map(|(_, id)| id)
        .collect();
    let direct_edges: Vec<BRepEdgeId> = cuboid.brep_edge_ids(owner);

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
            translation: [2.0, 3.0, 4.0],
            rotation_quat_xyzw: [0.0, 0.0, 0.0, 1.0],
            scale: [1.5, 1.5, 1.5],
        }))
        .expect("add transform");
    cad.graph_mut()
        .expect("mut")
        .connect(cuboid_node, xform_node, 0)
        .expect("connect");
    cad.commit("cuboid -> transform").expect("commit");

    let chain_faces: Vec<BRepFaceId> = brep_face_ids_for_node(cad.graph(), xform_node, owner)
        .expect("face resolve")
        .into_iter()
        .map(|(_, id)| id)
        .collect();
    let chain_edges: Vec<BRepEdgeId> =
        brep_edge_ids_for_node(cad.graph(), xform_node, owner).expect("edge resolve");

    assert_eq!(chain_faces, direct_faces);
    assert_eq!(chain_edges, direct_edges);
}
