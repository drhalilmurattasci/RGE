//! End-to-end smoke for the sub-7.2-ζ.β B-Rep edge-identity substrate
//! (`ExtrudeOp` — direct edge provider).
//!
//! Mirrors the `cuboid_edge_identity_smoke.rs` precedent shape: rebuild
//! stability, topology-break propagation, and owner disjointness for the
//! 3N edge IDs minted by `ExtrudeOp::brep_edge_ids`.

use rge_cad_core::{BRepEdgeId, BRepEdgeProvider, BRepOwnerId, ExtrudeOp, Polygon2D};

fn ccw_square() -> Polygon2D {
    Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]).expect("ccw unit square")
}

fn ccw_pentagon() -> Polygon2D {
    Polygon2D::new(vec![
        [1.0, 0.0],
        [0.309, 0.951],
        [-0.809, 0.588],
        [-0.809, -0.588],
        [0.309, -0.951],
    ])
    .expect("regular pentagon")
}

/// Same square profile, three different `length` values — 12 edge IDs
/// byte-identical across all three rebuilds. This is the rebuild-
/// stability assertion of sub-7.2-ζ.β: changing only the prism's height
/// must NOT alter edge identity, because edge IDs derive from face IDs
/// and `length` does not enter the face-ID derivation for any tag in
/// `ExtrudeFaceTag`.
#[test]
fn extrude_edge_ids_stable_across_length_changes() {
    let owner = BRepOwnerId::from_bytes([0x12; 16]);
    let square = ccw_square();
    let a = ExtrudeOp::new(square.clone(), 1.0).expect("a");
    let b = ExtrudeOp::new(square.clone(), 2.0).expect("b");
    let c = ExtrudeOp::new(square, 0.5).expect("c");

    let edges_a: Vec<BRepEdgeId> = a.brep_edge_ids(owner);
    let edges_b: Vec<BRepEdgeId> = b.brep_edge_ids(owner);
    let edges_c: Vec<BRepEdgeId> = c.brep_edge_ids(owner);

    assert_eq!(edges_a, edges_b, "length change must preserve edge IDs");
    assert_eq!(edges_b, edges_c);
    // 3 * N where N=4.
    assert_eq!(edges_a.len(), 12);
}

/// Square (N=4) vs pentagon (N=5) — different topology, edge IDs must
/// be disjoint between the two. The break propagates transitively
/// because each `Side(i)` face ID has `profile_count` hashed in, and
/// every Extrude edge involves at least one Side face.
#[test]
fn extrude_edge_ids_break_when_profile_count_changes() {
    let owner = BRepOwnerId::from_bytes([0x34; 16]);
    let sq_extrude = ExtrudeOp::new(ccw_square(), 1.0).expect("sq");
    let pen_extrude = ExtrudeOp::new(ccw_pentagon(), 1.0).expect("pen");

    let sq_edges = sq_extrude.brep_edge_ids(owner);
    let pen_edges = pen_extrude.brep_edge_ids(owner);

    assert_eq!(sq_edges.len(), 12, "3*4");
    assert_eq!(pen_edges.len(), 15, "3*5");

    // Topology change → disjoint edge sets. Every Extrude edge
    // touches at least one Side face whose face ID encodes
    // profile_count, so the break propagates from face → edge.
    for sq_edge in &sq_edges {
        assert!(
            !pen_edges.contains(sq_edge),
            "edge IDs must not collide across topology change"
        );
    }
}

/// Different `BRepOwnerId`s produce disjoint edge identity spaces — no
/// `BRepEdgeId` minted under one owner collides with any minted under
/// another. Mirrors the cuboid-edge precedent.
#[test]
fn extrude_edge_ids_distinct_across_owners() {
    let owner_x = BRepOwnerId::from_bytes([0x11; 16]);
    let owner_y = BRepOwnerId::from_bytes([0x22; 16]);
    let op = ExtrudeOp::new(ccw_square(), 1.0).expect("op");

    let edges_x: Vec<BRepEdgeId> = op.brep_edge_ids(owner_x);
    let edges_y: Vec<BRepEdgeId> = op.brep_edge_ids(owner_y);

    for ex in &edges_x {
        assert!(
            !edges_y.contains(ex),
            "owner-x edge id leaked into owner-y space: {ex:?}"
        );
    }
}
