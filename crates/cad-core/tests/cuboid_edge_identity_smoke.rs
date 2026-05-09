//! End-to-end smoke for the sub-7.2-ζ.α B-Rep edge-identity substrate.
//!
//! These tests are the gate for the dispatch — they prove that
//! `BRepEdgeId`s seeded by a caller-supplied `BRepOwnerId` are byte-
//! identical across parameter rebuilds of `CuboidOp` (the rebuild-
//! stability property), and that distinct owners produce disjoint
//! identity spaces (the owner-disambiguation property), mirroring the
//! sub-7.2-α face-identity smoke.

use rge_cad_core::{BRepEdgeId, BRepEdgeProvider, BRepOwnerId, CuboidOp};

/// Same owner, three Cuboid rebuilds with different dimensions —
/// 12 edge IDs byte-identical across all three rebuilds. Mirrors the
/// `cuboid_face_ids_stable_across_parameter_rebuilds` precedent from
/// sub-7.2-α.
#[test]
fn cuboid_edge_ids_stable_across_parameter_rebuilds() {
    let owner = BRepOwnerId::from_bytes([0xed; 16]);
    let cuboid_a = CuboidOp {
        width: 1.0,
        height: 1.0,
        depth: 1.0,
    };
    let cuboid_b = CuboidOp {
        width: 2.0,
        height: 1.0,
        depth: 1.0,
    };
    let cuboid_c = CuboidOp {
        width: 0.5,
        height: 2.0,
        depth: 0.5,
    };

    let edges_a: Vec<BRepEdgeId> = cuboid_a.brep_edge_ids(owner);
    let edges_b: Vec<BRepEdgeId> = cuboid_b.brep_edge_ids(owner);
    let edges_c: Vec<BRepEdgeId> = cuboid_c.brep_edge_ids(owner);

    assert_eq!(edges_a, edges_b);
    assert_eq!(edges_b, edges_c);
    assert_eq!(edges_a.len(), 12);
}

/// Different owners produce disjoint edge identity spaces — no
/// `BRepEdgeId` minted under one owner collides with any minted under
/// another. Mirrors the sub-7.2-α face-identity precedent.
#[test]
fn cuboid_edge_ids_distinct_across_owners() {
    let owner_x = BRepOwnerId::from_bytes([0x11; 16]);
    let owner_y = BRepOwnerId::from_bytes([0x22; 16]);
    let cuboid = CuboidOp {
        width: 1.0,
        height: 1.0,
        depth: 1.0,
    };

    let edges_x: Vec<BRepEdgeId> = cuboid.brep_edge_ids(owner_x);
    let edges_y: Vec<BRepEdgeId> = cuboid.brep_edge_ids(owner_y);

    // Disjoint sets — different owners produce different identity spaces.
    for ex in &edges_x {
        assert!(
            !edges_y.contains(ex),
            "owner-x edge id leaked into owner-y space: {ex:?}"
        );
    }
}
