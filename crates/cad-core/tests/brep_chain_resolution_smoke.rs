//! End-to-end smoke for the sub-7.2-ε graph-level B-Rep face-identity
//! resolver (Transform inheritance, no `BRepProvider` for `TransformOp`).
//!
//! These tests are the gate for the dispatch — they prove:
//!
//! 1. Direct providers through the resolver match a direct
//!    [`BRepProvider::brep_face_ids`] call (no double-processing).
//! 2. A Cuboid → Transform chain inherits the Cuboid's IDs verbatim.
//! 3. Three different Transform parameter sets (translate / rotate /
//!    scale) all preserve identity — placement is not topology.
//! 4. Multi-hop Transform chains inherit upstream IDs unchanged.
//! 5. Different owners stay disjoint through Transform chains.
//! 6. Boolean returns [`BRepResolveError::TopologyChangingOperator`].
//! 7. Sweep resolves directly to its own [`BRepProvider::brep_face_ids`]
//!    output — Sweep is a direct face provider, not topology-changing.
//! 8. An unknown / fresh `NodeId` returns
//!    [`BRepResolveError::NodeNotInGraph`].
//!
//! Tests #2 and #4 are the load-bearing inheritance assertions: they
//! pin the architectural decision that Transform is identity-preserving
//! at the resolver layer (NOT a `BRepProvider` impl on `TransformOp`).

use rge_cad_core::{
    brep_face_ids_for_node, BRepFaceId, BRepOwnerId, BRepProvider, BRepResolveError, BooleanOp,
    CadGraph, CuboidOp, OpKind, OperatorNode, Polygon2D, Polyline3D, SweepOp, TransformOp,
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

/// Direct Cuboid through the resolver returns the same IDs as a direct
/// [`BRepProvider::brep_face_ids`] call.
///
/// Proves the resolver doesn't double-process or re-derive IDs for a
/// leaf direct provider — it just delegates to the operator's own
/// `brep_face_ids` impl.
#[test]
fn resolver_against_direct_cuboid_matches_direct_provider() {
    let owner = BRepOwnerId::from_bytes([0x42; 16]);
    let cuboid = unit_cuboid();

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
        .expect("add cuboid");
    cad.commit("cuboid").expect("commit");

    let chain: Vec<BRepFaceId> = brep_face_ids_for_node(cad.graph(), cuboid_node, owner)
        .expect("resolve direct cuboid")
        .into_iter()
        .map(|(_, id)| id)
        .collect();

    assert_eq!(chain, direct);
    assert_eq!(chain.len(), 6);
}

/// Cuboid → Transform: every face ID exactly equals the direct Cuboid
/// face ID. This is the load-bearing inheritance assertion of
/// sub-7.2-ε. Transform changes placement, not topology, so a face
/// selected before a Transform must remain the same face after.
#[test]
fn cuboid_through_transform_inherits_cuboid_ids() {
    let owner = BRepOwnerId::from_bytes([0x42; 16]);
    let cuboid = unit_cuboid();

    let direct_ids: Vec<BRepFaceId> = cuboid
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

    let chain_ids: Vec<BRepFaceId> = brep_face_ids_for_node(cad.graph(), transform_node, owner)
        .expect("resolve chain")
        .into_iter()
        .map(|(_, id)| id)
        .collect();

    assert_eq!(
        chain_ids, direct_ids,
        "Transform must inherit Cuboid IDs unchanged"
    );
}

/// Three Transforms differing in placement parameters (pure translate
/// / pure rotate / pure scale) all produce byte-identical face IDs
/// because the resolver inherits from the Cuboid upstream and
/// Transform parameters never enter the BLAKE3 derivation.
#[test]
fn transform_parameter_changes_do_not_change_ids() {
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

    let mut all_ids: Vec<Vec<BRepFaceId>> = Vec::new();
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

        let ids: Vec<BRepFaceId> = brep_face_ids_for_node(cad.graph(), xform_node, owner)
            .expect("resolve")
            .into_iter()
            .map(|(_, id)| id)
            .collect();
        all_ids.push(ids);
    }

    // All three placements must produce the same IDs.
    for window in all_ids.windows(2) {
        assert_eq!(
            window[0], window[1],
            "transform parameter change must not change IDs"
        );
    }
}

/// Cuboid → Transform → Transform: resolving at the second Transform
/// returns IDs equal to the direct Cuboid IDs. Multi-hop inheritance.
#[test]
fn multi_hop_transforms_preserve_ids() {
    let owner = BRepOwnerId::from_bytes([0x77; 16]);
    let cuboid = unit_cuboid();

    let direct_ids: Vec<BRepFaceId> = cuboid
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

    let chain_ids: Vec<BRepFaceId> = brep_face_ids_for_node(cad.graph(), xform2, owner)
        .expect("resolve multi-hop")
        .into_iter()
        .map(|(_, id)| id)
        .collect();

    assert_eq!(
        chain_ids, direct_ids,
        "multi-hop Transform chain must inherit Cuboid IDs unchanged"
    );
}

/// Different `BRepOwnerId`s flow through Transform chains independently
/// — the chain inherits whatever owner the consumer supplies. No ID
/// minted under one owner collides with any minted under the other,
/// matching the direct-provider precedent.
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

    let ids_x: Vec<BRepFaceId> = brep_face_ids_for_node(cad.graph(), xform_node, owner_x)
        .expect("owner_x")
        .into_iter()
        .map(|(_, id)| id)
        .collect();
    let ids_y: Vec<BRepFaceId> = brep_face_ids_for_node(cad.graph(), xform_node, owner_y)
        .expect("owner_y")
        .into_iter()
        .map(|(_, id)| id)
        .collect();

    for id_x in &ids_x {
        assert!(
            !ids_y.contains(id_x),
            "owner-disambiguation failed through Transform chain: \
             id from owner_x found in owner_y's set"
        );
    }
}

/// A Boolean node resolves to
/// [`BRepResolveError::TopologyChangingOperator`] — neither empty IDs
/// nor a panic. The error carries [`OpKind::Boolean`].
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
    let result = brep_face_ids_for_node(cad.graph(), bool_node, owner);

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

/// A Sweep node resolves directly to its own
/// [`BRepProvider::brep_face_ids`] output. Sweep is a direct face
/// provider — not topology-changing — at the face resolver as of the
/// Sweep face-identity slice, so the resolver delegates to the
/// operator's `brep_face_ids` impl rather than erroring.
#[test]
fn sweep_resolves_to_direct_provider_ids() {
    let path =
        Polyline3D::new(vec![[0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, 2.0]]).expect("path");
    let sweep = SweepOp::new(unit_square(), path);
    let owner = BRepOwnerId::from_bytes([0xbb; 16]);

    let direct = sweep.brep_face_ids(owner);

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
    // Square profile (n = 4) over a 3-point path (m = 3) → 2 + 4 * 2 = 10.
    assert_eq!(through_resolver.len(), 10);
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

    let err = brep_face_ids_for_node(cad.graph(), fresh, owner)
        .expect_err("fresh NodeId must produce error");
    assert_eq!(err, BRepResolveError::NodeNotInGraph { node: fresh });
}
