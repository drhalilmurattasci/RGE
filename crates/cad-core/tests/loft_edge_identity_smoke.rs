//! End-to-end smoke for the sub-7.2-ζ.δ B-Rep edge-identity substrate
//! (`LoftOp` — direct edge provider with two-input topology).
//!
//! Mirrors the cuboid + extrude precedent shape: rebuild stability
//! across `length` changes, topology-break propagation across
//! profile-count changes (both profiles change together, since
//! `LoftOp::evaluate` enforces equal counts), and owner disjointness.

use rge_cad_core::{BRepEdgeId, BRepEdgeProvider, BRepOwnerId, LoftOp, Polygon2D};

fn ccw_square() -> Polygon2D {
    Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]).expect("ccw unit square")
}

fn ccw_square_scaled() -> Polygon2D {
    Polygon2D::new(vec![[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [0.0, 2.0]]).expect("ccw 2-unit square")
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

fn ccw_pentagon_scaled() -> Polygon2D {
    Polygon2D::new(vec![
        [2.0, 0.0],
        [0.618, 1.902],
        [-1.618, 1.176],
        [-1.618, -1.176],
        [0.618, -1.902],
    ])
    .expect("scaled pentagon")
}

/// Same profile pair, three different `length` values — 12 edge IDs
/// byte-identical across all three rebuilds. Mirrors the Extrude
/// rebuild-stability precedent: `length` does not enter the BLAKE3
/// derivation for any `LoftFaceTag` variant, so it does not enter
/// edge derivation either.
#[test]
fn loft_edge_ids_stable_across_length_changes() {
    let owner = BRepOwnerId::from_bytes([0x12; 16]);
    let a = LoftOp::new(ccw_square(), ccw_square_scaled(), 1.0).expect("a");
    let b = LoftOp::new(ccw_square(), ccw_square_scaled(), 2.0).expect("b");
    let c = LoftOp::new(ccw_square(), ccw_square_scaled(), 0.5).expect("c");

    let edges_a: Vec<BRepEdgeId> = a.brep_edge_ids(owner);
    let edges_b: Vec<BRepEdgeId> = b.brep_edge_ids(owner);
    let edges_c: Vec<BRepEdgeId> = c.brep_edge_ids(owner);

    assert_eq!(edges_a, edges_b, "length change must preserve edge IDs");
    assert_eq!(edges_b, edges_c);
    // 3 * N where N=4.
    assert_eq!(edges_a.len(), 12);
}

/// Two squares (N=4) vs two pentagons (N=5) — both profiles change
/// together because `LoftOp::evaluate` enforces equal counts. Edge IDs
/// must be disjoint across the two topologies.
#[test]
fn loft_edge_ids_break_when_profile_count_changes() {
    let owner = BRepOwnerId::from_bytes([0x34; 16]);
    let sq_loft = LoftOp::new(ccw_square(), ccw_square_scaled(), 1.0).expect("sq×sq");
    let pen_loft = LoftOp::new(ccw_pentagon(), ccw_pentagon_scaled(), 1.0).expect("pen×pen");

    let sq_edges = sq_loft.brep_edge_ids(owner);
    let pen_edges = pen_loft.brep_edge_ids(owner);

    assert_eq!(sq_edges.len(), 12, "3*4");
    assert_eq!(pen_edges.len(), 15, "3*5");

    // Topology change → disjoint edge sets. Every Loft edge touches
    // at least one Side face whose face ID encodes BOTH
    // profile_a_count AND profile_b_count, so the break propagates
    // from face → edge.
    for sq_edge in &sq_edges {
        assert!(
            !pen_edges.contains(sq_edge),
            "edge IDs must not collide across topology change"
        );
    }
}

/// Different `BRepOwnerId`s produce disjoint edge identity spaces.
#[test]
fn loft_edge_ids_distinct_across_owners() {
    let owner_x = BRepOwnerId::from_bytes([0x11; 16]);
    let owner_y = BRepOwnerId::from_bytes([0x22; 16]);
    let op = LoftOp::new(ccw_square(), ccw_square_scaled(), 1.0).expect("op");

    let edges_x: Vec<BRepEdgeId> = op.brep_edge_ids(owner_x);
    let edges_y: Vec<BRepEdgeId> = op.brep_edge_ids(owner_y);

    for ex in &edges_x {
        assert!(
            !edges_y.contains(ex),
            "owner-x edge id leaked into owner-y space: {ex:?}"
        );
    }
}
