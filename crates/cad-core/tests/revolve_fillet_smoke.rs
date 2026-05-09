//! End-to-end smoke for D-Fillet sub-γ — Revolve variant of the
//! BRepEdgeId consumer pattern.
//!
//! These tests are the gate for the dispatch — they prove:
//!
//! 1. `FilletOp::new_for_revolve` accepts cap-side edge IDs from a
//!    Partial Revolve's `BRepEdgeProvider`.
//! 2. Side-side edge IDs (in either Full or Partial mode) are
//!    rejected with `FilletError::UnsupportedEdgeGeometry` at
//!    construction time — substrate honesty: validation works, but
//!    chamfer geometry doesn't (circular paths).
//! 3. **Load-bearing rebuild-stability test (angle-within-mode axis)**:
//!    `fillet_revolve_cap_side_edge_remains_valid_across_partial_angle_changes`
//!    captures a cap-side edge ID at one angle within Partial mode,
//!    proves it is still valid for `FilletOp::new_for_revolve` against
//!    rebuilds at other angles within Partial mode (angle is
//!    topology-preserving within mode, per D-7.2-ζ.γ).
//! 4. **Load-bearing topology-change test (segment-count axis)**:
//!    `fillet_revolve_cap_side_edge_invalidated_by_segment_count_change`
//!    proves that segment-count changes break edge IDs — the
//!    `segment_count` field is hashed into the Side face tag.
//! 5. **Load-bearing topology-change test (mode axis)**:
//!    `fillet_revolve_cap_side_edge_invalidated_by_mode_change`
//!    proves Full ↔ Partial mode change breaks edge IDs — the `mode`
//!    field is hashed into the Side face tag.

use std::f32::consts::{FRAC_PI_2, PI};

use rge_cad_core::{
    BRepEdgeId, BRepEdgeProvider, BRepOwnerId, CadGraph, FilletError, FilletOp, Operator,
    OperatorNode, Polygon2D, RevolveOp, TessellationCache, Tolerance,
};

/// Square profile in +X half-plane (Revolve requires `x >= 0`).
fn ring_profile() -> Polygon2D {
    Polygon2D::new(vec![[1.0, 0.0], [2.0, 0.0], [2.0, 1.0], [1.0, 1.0]]).expect("ring")
}

/// Cap-side edge in Partial mode is accepted by `FilletOp::new_for_revolve`.
#[test]
fn fillet_validates_revolve_partial_cap_side_edge() {
    let owner = BRepOwnerId::from_bytes([0xed; 16]);
    let revolve = RevolveOp::partial(ring_profile(), 8, FRAC_PI_2).expect("partial");
    let edges = revolve.brep_edge_ids(owner);
    let n: usize = 4; // square profile
    assert_eq!(edges.len(), 3 * n); // 3*n for partial = 12

    // edges[n..2n] are start-cap-side (supported); pick edges[n].
    let cap_edge = edges[n];
    let fillet = FilletOp::new_for_revolve(&revolve, owner, vec![cap_edge], 0.05)
        .expect("cap-side edge supported");
    assert_eq!(fillet.edges().len(), 1);
}

/// Full-revolution side-side edges are circular paths and reject with
/// `UnsupportedEdgeGeometry` — substrate honesty.
#[test]
fn fillet_rejects_revolve_full_mode_side_side_edge() {
    let owner = BRepOwnerId::from_bytes([0xab; 16]);
    let revolve = RevolveOp::new(ring_profile(), 8).expect("full");
    let edges = revolve.brep_edge_ids(owner);
    assert_eq!(edges.len(), 4); // n=4 for full mode

    let result = FilletOp::new_for_revolve(&revolve, owner, vec![edges[0]], 0.05);
    assert!(matches!(
        result,
        Err(FilletError::UnsupportedEdgeGeometry { .. })
    ));
}

/// Partial-revolution side-side edges (canonical `0..n`) are also
/// circular paths and reject with `UnsupportedEdgeGeometry`.
#[test]
fn fillet_rejects_revolve_partial_side_side_edge() {
    let owner = BRepOwnerId::from_bytes([0xcd; 16]);
    let revolve = RevolveOp::partial(ring_profile(), 8, PI).expect("partial");
    let edges = revolve.brep_edge_ids(owner);
    let n: usize = 4;

    // edges[0..n] are side-side (unsupported in partial mode too).
    let side_side_edge = edges[0];
    let result = FilletOp::new_for_revolve(&revolve, owner, vec![side_side_edge], 0.05);
    assert!(matches!(
        result,
        Err(FilletError::UnsupportedEdgeGeometry { .. })
    ));
}

/// **Load-bearing rebuild-stability test (angle-within-mode axis).**
///
/// Angle changes within Partial mode preserve cap-side edge IDs (per
/// D-7.2-ζ.γ: angle is topology-preserving within mode; only segments
/// and mode are topology-changing).
#[test]
fn fillet_revolve_cap_side_edge_remains_valid_across_partial_angle_changes() {
    let owner = BRepOwnerId::from_bytes([0x12; 16]);
    let profile = ring_profile();
    let n: usize = 4;

    let rev_a = RevolveOp::partial(profile.clone(), 8, FRAC_PI_2).expect("a");
    let cap_edge = rev_a.brep_edge_ids(owner)[n]; // first start-cap-side

    // Same edge ID is valid against rebuilds at different angles within
    // Partial mode.
    let rev_b = RevolveOp::partial(profile.clone(), 8, PI).expect("b");
    let rev_c = RevolveOp::partial(profile, 8, PI * 1.5).expect("c");

    assert!(
        rev_b.brep_edge_ids(owner).contains(&cap_edge),
        "edge id captured at angle=π/2 must remain in rebuilt angle=π edge list"
    );
    assert!(
        rev_c.brep_edge_ids(owner).contains(&cap_edge),
        "edge id captured at angle=π/2 must remain in rebuilt angle=3π/2 edge list"
    );

    let fillet_a = FilletOp::new_for_revolve(&rev_a, owner, vec![cap_edge], 0.05).expect("a");
    let fillet_b = FilletOp::new_for_revolve(&rev_b, owner, vec![cap_edge], 0.05).expect("b");
    let fillet_c = FilletOp::new_for_revolve(&rev_c, owner, vec![cap_edge], 0.05).expect("c");
    assert_eq!(fillet_a.edges(), fillet_b.edges());
    assert_eq!(fillet_b.edges(), fillet_c.edges());
}

/// **Load-bearing topology-change test (segment-count axis).**
///
/// Segment count change breaks edge IDs — `segment_count` is hashed
/// into the Side face tag's BLAKE3 input.
#[test]
fn fillet_revolve_cap_side_edge_invalidated_by_segment_count_change() {
    let owner = BRepOwnerId::from_bytes([0x34; 16]);
    let profile = ring_profile();
    let n: usize = 4;

    let rev_8 = RevolveOp::partial(profile.clone(), 8, PI).expect("8");
    let cap_edge_8 = rev_8.brep_edge_ids(owner)[n];

    let rev_16 = RevolveOp::partial(profile, 16, PI).expect("16");
    let edges_16 = rev_16.brep_edge_ids(owner);

    // Edge ID from segments=8 must NOT appear in segments=16's edge list.
    assert!(
        !edges_16.contains(&cap_edge_8),
        "cap-side edge id from segments=8 must NOT be valid against segments=16"
    );

    // FilletOp::new_for_revolve with the segments=8 edge ID against
    // segments=16 must error with EdgeNotInUpstream.
    let result = FilletOp::new_for_revolve(&rev_16, owner, vec![cap_edge_8], 0.05);
    assert!(matches!(result, Err(FilletError::EdgeNotInUpstream { .. })));
}

/// **Load-bearing topology-change test (mode axis).**
///
/// Full ↔ Partial mode change breaks edge IDs — `mode` is hashed
/// into the Side face tag's BLAKE3 input.
#[test]
fn fillet_revolve_cap_side_edge_invalidated_by_mode_change() {
    let owner = BRepOwnerId::from_bytes([0x56; 16]);
    let profile = ring_profile();
    let n: usize = 4;

    let partial = RevolveOp::partial(profile.clone(), 8, PI).expect("partial");
    let cap_edge_partial = partial.brep_edge_ids(owner)[n]; // cap-side, only in Partial

    let full = RevolveOp::new(profile, 8).expect("full");
    let edges_full = full.brep_edge_ids(owner);

    // The cap-side edge ID from Partial must NOT appear in Full's
    // edge list (Full has no caps; only n side-side edges, all with
    // mode=Full in the tag, distinct from Partial's mode=Partial Side
    // IDs).
    assert!(
        !edges_full.contains(&cap_edge_partial),
        "Partial cap-side edge id must NOT appear in Full mode's edge list"
    );

    let result = FilletOp::new_for_revolve(&full, owner, vec![cap_edge_partial], 0.05);
    assert!(matches!(result, Err(FilletError::EdgeNotInUpstream { .. })));
}

/// End-to-end Revolve(Partial) → Fillet through `CadGraph`/`OperatorGraph`
/// evaluates and produces a well-formed tessellation.
#[test]
fn fillet_revolve_through_operator_graph_evaluates_correctly() {
    let owner = BRepOwnerId::from_bytes([0x42; 16]);
    let revolve = RevolveOp::partial(ring_profile(), 8, PI).expect("rev");
    let n: usize = 4;
    let cap_edge = revolve.brep_edge_ids(owner)[n]; // first start-cap-side
    let fillet = FilletOp::new_for_revolve(&revolve, owner, vec![cap_edge], 0.05).expect("fillet");

    // Compute the upstream tessellation size for the assertion.
    let upstream = revolve.evaluate(&[]).expect("rev tess");
    let upstream_positions = upstream.positions.len();
    let upstream_indices = upstream.indices.len();

    let mut cad = CadGraph::new();
    cad.begin_operation().expect("begin");
    let revolve_node = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Revolve(revolve))
        .expect("revolve");
    let fillet_node = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Fillet(fillet))
        .expect("fillet");
    cad.graph_mut()
        .expect("mut")
        .connect(revolve_node, fillet_node, 0)
        .expect("connect");
    cad.graph_mut()
        .expect("mut")
        .set_root(fillet_node)
        .expect("set root");
    cad.commit("revolve -> fillet").expect("commit");

    // Evaluate end-to-end. After 1 fillet: +2 verts + 6 indices.
    let mut cache = TessellationCache::new();
    let tess = cad
        .graph()
        .evaluate(fillet_node, &mut cache, Tolerance::new(0.001).expect("tol"))
        .expect("evaluate");
    assert_eq!(tess.positions.len(), upstream_positions + 2);
    assert_eq!(tess.indices.len(), upstream_indices + 6);
}

/// Radius validation works the same for Revolve as for other
/// upstreams.
#[test]
fn fillet_revolve_radius_validation_unchanged() {
    let owner = BRepOwnerId::from_bytes([0x78; 16]);
    let revolve = RevolveOp::partial(ring_profile(), 8, PI).expect("rev");
    let cap_edge = revolve.brep_edge_ids(owner)[4];

    let zero = FilletOp::new_for_revolve(&revolve, owner, vec![cap_edge], 0.0);
    assert!(matches!(zero, Err(FilletError::InvalidRadius { .. })));
    let neg = FilletOp::new_for_revolve(&revolve, owner, vec![cap_edge], -0.1);
    assert!(matches!(neg, Err(FilletError::InvalidRadius { .. })));
}

/// Phantom edge ID with bytes that don't correspond to any
/// canonical Revolve edge under the owner is rejected with
/// `EdgeNotInUpstream`.
#[test]
fn fillet_revolve_rejects_unknown_edge_id() {
    let owner = BRepOwnerId::from_bytes([0x9a; 16]);
    let revolve = RevolveOp::partial(ring_profile(), 8, PI).expect("rev");
    let phantom = BRepEdgeId::from_bytes([0u8; 16]);
    let result = FilletOp::new_for_revolve(&revolve, owner, vec![phantom], 0.1);
    assert!(matches!(result, Err(FilletError::EdgeNotInUpstream { .. })));
}
