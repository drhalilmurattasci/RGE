//! End-to-end smoke for the sub-7.2-ζ.γ B-Rep edge-identity substrate
//! (`RevolveOp` — direct edge provider with mode-driven topology).
//!
//! These tests prove (1) angle-stability within Partial mode preserves
//! edge IDs, (2) shape-size changes at fixed `(n, segments)` preserve
//! edge IDs in Full mode, (3) Full vs Partial mode produces disjoint
//! edge ID spaces (the load-bearing compositional-honesty check —
//! sub-7.2-γ's mode break must propagate to edges by construction),
//! (4) segment-count changes break edge IDs (Side face IDs encode
//! segment_count, propagated transitively), and (5) different owners
//! produce disjoint identity spaces.

use std::f32::consts::PI;

use rge_cad_core::{BRepEdgeId, BRepEdgeProvider, BRepOwnerId, Polygon2D, RevolveOp};

/// Square on the +X side of the Y-axis — `(1,0)..(2,0)..(2,1)..(1,1)`. CCW.
fn ccw_square_off_axis() -> Polygon2D {
    Polygon2D::new(vec![[1.0, 0.0], [2.0, 0.0], [2.0, 1.0], [1.0, 1.0]]).expect("ccw +x square")
}

/// Wider square — same CCW winding, same vertex count, larger radius.
fn ccw_wider_square() -> Polygon2D {
    Polygon2D::new(vec![[3.0, 0.0], [5.0, 0.0], [5.0, 2.0], [3.0, 2.0]])
        .expect("ccw +x wider square")
}

/// Within Partial mode at fixed `(n, segments)`, three angles below
/// 2π preserve every edge ID. Edge count = 3n = 12 stable; IDs
/// byte-identical. Mirrors the face-substrate precedent: angle does
/// not enter the BLAKE3 derivation for Side / StartCap / EndCap face
/// tags within a single mode, so it doesn't enter the edge derivation
/// either.
#[test]
fn revolve_partial_edge_ids_stable_across_angle_changes_within_partial_mode() {
    let owner = BRepOwnerId::from_bytes([0x12; 16]);
    let square = ccw_square_off_axis();
    let a = RevolveOp::partial(square.clone(), 8, PI / 4.0).expect("a");
    let b = RevolveOp::partial(square.clone(), 8, PI / 2.0).expect("b");
    let c = RevolveOp::partial(square, 8, PI * 0.75).expect("c");

    let edges_a: Vec<BRepEdgeId> = a.brep_edge_ids(owner);
    let edges_b: Vec<BRepEdgeId> = b.brep_edge_ids(owner);
    let edges_c: Vec<BRepEdgeId> = c.brep_edge_ids(owner);

    assert_eq!(
        edges_a, edges_b,
        "angle change within Partial mode must preserve edge IDs"
    );
    assert_eq!(edges_b, edges_c);
    // 3n with n=4 in partial mode.
    assert_eq!(edges_a.len(), 12);
}

/// Full mode, fixed `(n, segments)` — shape-size change preserves edge
/// IDs. The substrate does NOT inspect profile coordinates for face
/// identity, so edge IDs (which derive from face IDs) are equally
/// coordinate-invariant at fixed `(n, segments, mode)`.
#[test]
fn revolve_full_edge_ids_stable_across_shape_size_changes() {
    let owner = BRepOwnerId::from_bytes([0x34; 16]);
    let small = RevolveOp::new(ccw_square_off_axis(), 8).expect("small");
    let large = RevolveOp::new(ccw_wider_square(), 8).expect("large");

    let edges_small = small.brep_edge_ids(owner);
    let edges_large = large.brep_edge_ids(owner);

    assert_eq!(
        edges_small, edges_large,
        "shape-size change at same (n, segments) preserves edge IDs"
    );
    // n=4 in full mode.
    assert_eq!(edges_small.len(), 4);
}

/// THE LOAD-BEARING COMPOSITIONAL-HONESTY TEST.
///
/// Same profile, same segments, different mode: Full vs Partial.
/// Edge counts differ (n vs 3n) AND edge IDs are disjoint sets
/// because face IDs differ (mode is hashed into the Side face tag's
/// BLAKE3 input, per sub-7.2-γ). The face substrate's mode break
/// must propagate through edge derivation automatically — this test
/// is the gate that proves it does.
#[test]
fn revolve_full_and_partial_edge_ids_are_disjoint() {
    let owner = BRepOwnerId::from_bytes([0xab; 16]);
    let square = ccw_square_off_axis();
    let full = RevolveOp::new(square.clone(), 8).expect("full");
    let partial = RevolveOp::partial(square, 8, PI * 1.99).expect("partial");

    let full_edges = full.brep_edge_ids(owner);
    let partial_edges = partial.brep_edge_ids(owner);

    assert_eq!(full_edges.len(), 4, "Full mode = n edges");
    assert_eq!(partial_edges.len(), 12, "Partial mode = 3n edges");

    // Edge IDs differ because face IDs differ (mode is in the Side
    // face tag's BLAKE3 input). This is the compositional-honesty
    // check: sub-7.2-γ's mode break propagates through edge
    // derivation automatically.
    for full_edge in &full_edges {
        assert!(
            !partial_edges.contains(full_edge),
            "full-mode edge ID leaked into partial-mode set; mode break must propagate to edges"
        );
    }
}

/// Segment count is in the Side face tag → propagates to edge IDs.
/// 8 segments vs 16 segments at same profile + same mode produces
/// disjoint edge ID sets in Full mode (where every edge is a
/// Side-Side adjacency).
#[test]
fn revolve_edge_ids_break_when_segment_count_changes() {
    let owner = BRepOwnerId::from_bytes([0x56; 16]);
    let square = ccw_square_off_axis();
    let r8 = RevolveOp::new(square.clone(), 8).expect("8 segments");
    let r16 = RevolveOp::new(square, 16).expect("16 segments");

    let edges_8 = r8.brep_edge_ids(owner);
    let edges_16 = r16.brep_edge_ids(owner);

    assert_eq!(edges_8.len(), 4);
    assert_eq!(edges_16.len(), 4);

    // No edge ID minted under 8 segments may appear in the 16-segment set —
    // segment_count is in each Side face's BLAKE3 input, and every Full-
    // mode edge involves two Side faces, so the break is total.
    for e8 in &edges_8 {
        assert!(
            !edges_16.contains(e8),
            "edge IDs must NOT be preserved across segment-count changes"
        );
    }
}

/// Different `BRepOwnerId`s produce disjoint edge identity spaces.
#[test]
fn revolve_edge_ids_distinct_across_owners() {
    let owner_x = BRepOwnerId::from_bytes([0x11; 16]);
    let owner_y = BRepOwnerId::from_bytes([0x22; 16]);
    let op = RevolveOp::partial(ccw_square_off_axis(), 8, PI / 2.0).expect("op");

    let edges_x: Vec<BRepEdgeId> = op.brep_edge_ids(owner_x);
    let edges_y: Vec<BRepEdgeId> = op.brep_edge_ids(owner_y);

    for ex in &edges_x {
        assert!(
            !edges_y.contains(ex),
            "owner-x edge id leaked into owner-y space: {ex:?}"
        );
    }
}
