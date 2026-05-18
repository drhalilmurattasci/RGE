//! End-to-end smoke for the Sweep face-identity slice — the
//! `BRepProvider` impl for `SweepOp` plus the direct `OperatorNode::Sweep`
//! arm in the graph-level face resolver.
//!
//! These tests are the gate for the dispatch — they prove:
//!
//! 1. `SweepOp::brep_face_ids` returns exactly `2 + n * (m - 1)` pairs.
//! 2. Pair order follows `SweepOp::evaluate`'s canonical face-label
//!    contract: first cap, last cap, then sides in segment-major,
//!    profile-edge-major order.
//! 3. Every minted ID is unique within one owner.
//! 4. IDs minted under different owners are disjoint.
//! 5. Numeric rebuilds that preserve `profile_count` and
//!    `path_segment_count` keep byte-identical IDs — for caps AND sides.
//! 6. A profile-count change breaks side IDs (caps stay categorical).
//! 7. A path-segment-count change breaks side IDs (caps stay
//!    categorical).
//! 8. Repeated `brep_face_ids` calls are byte-identical.
//! 9. Direct `brep_face_ids_for_node` resolution matches the direct
//!    provider call.
//! 10. Sweep IDs are disjoint from the existing Extrude face-id
//!     namespace via the operator-kind separator.

use std::collections::HashSet;

use rge_cad_core::{
    brep_face_ids_for_node, BRepFaceId, BRepOwnerId, BRepProvider, CadGraph, ExtrudeFaceTag,
    OperatorNode, Polygon2D, Polyline3D, SweepFaceTag, SweepOp, TopologyFaceId,
};

fn unit_square() -> Polygon2D {
    Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]).expect("square")
}

fn pentagon() -> Polygon2D {
    Polygon2D::new(vec![
        [0.0, 0.0],
        [2.0, 0.0],
        [2.5, 1.5],
        [1.0, 2.5],
        [-0.5, 1.5],
    ])
    .expect("pentagon")
}

fn z_path(zs: &[f32]) -> Polyline3D {
    Polyline3D::new(zs.iter().map(|z| [0.0, 0.0, *z]).collect()).expect("z-axis path")
}

/// `brep_face_ids` returns exactly `2 + n * (m - 1)` pairs across several
/// `(profile, path)` shapes.
#[test]
fn sweep_face_id_count_is_2_plus_n_times_segments() {
    let owner = BRepOwnerId::from_bytes([0x01; 16]);
    // n = 4, m = 2 → 2 + 4 * 1 = 6.
    assert_eq!(
        SweepOp::new(unit_square(), z_path(&[0.0, 1.0]))
            .brep_face_ids(owner)
            .len(),
        6
    );
    // n = 5, m = 4 → 2 + 5 * 3 = 17.
    assert_eq!(
        SweepOp::new(pentagon(), z_path(&[0.0, 1.0, 2.0, 3.0]))
            .brep_face_ids(owner)
            .len(),
        17
    );
    // n = 4, m = 5 → 2 + 4 * 4 = 18.
    assert_eq!(
        SweepOp::new(unit_square(), z_path(&[0.0, 1.0, 2.0, 3.0, 4.0]))
            .brep_face_ids(owner)
            .len(),
        18
    );
}

/// Pair order exactly follows `SweepOp::evaluate`'s canonical face-label
/// contract: `TopologyFaceId(0)` first cap, `TopologyFaceId(1)` last cap,
/// then sides `TopologyFaceId(2 + segment_index * n + edge_index)` in
/// segment-major, profile-edge-major order, each carrying the matching
/// [`SweepFaceTag`].
#[test]
fn sweep_face_ids_follow_canonical_order() {
    let owner = BRepOwnerId::from_bytes([0x02; 16]);
    // Pentagon (n = 5) over a 3-point path (m = 3 → 2 segments).
    let n = 5u32;
    let path_segment_count = 2u32;
    let op = SweepOp::new(pentagon(), z_path(&[0.0, 1.0, 2.0]));
    let pairs = op.brep_face_ids(owner);
    assert_eq!(pairs.len(), 2 + (n * path_segment_count) as usize);

    // Caps.
    assert_eq!(pairs[0].0, TopologyFaceId(0));
    assert_eq!(
        pairs[0].1,
        BRepFaceId::for_sweep_face(owner, SweepFaceTag::FirstCap)
    );
    assert_eq!(pairs[1].0, TopologyFaceId(1));
    assert_eq!(
        pairs[1].1,
        BRepFaceId::for_sweep_face(owner, SweepFaceTag::LastCap)
    );

    // Sides advance by segment, then by profile edge.
    for segment_index in 0..path_segment_count {
        for edge_index in 0..n {
            let ordinal = u64::from(segment_index) * u64::from(n) + u64::from(edge_index);
            let pair = pairs[2 + ordinal as usize];
            assert_eq!(
                pair.0,
                TopologyFaceId(2 + ordinal),
                "sequential id mismatch at segment {segment_index} edge {edge_index}"
            );
            assert_eq!(
                pair.1,
                BRepFaceId::for_sweep_face(
                    owner,
                    SweepFaceTag::Side {
                        segment_index,
                        edge_index,
                        profile_count: n,
                        path_segment_count,
                    },
                ),
                "stable id mismatch at segment {segment_index} edge {edge_index}"
            );
        }
    }
}

/// Every minted `BRepFaceId` is unique within one owner — no two faces
/// of a single Sweep collide.
#[test]
fn sweep_face_ids_unique_within_owner() {
    let owner = BRepOwnerId::from_bytes([0x03; 16]);
    let op = SweepOp::new(pentagon(), z_path(&[0.0, 1.0, 2.0, 3.0]));
    let pairs = op.brep_face_ids(owner);

    let ids: Vec<BRepFaceId> = pairs.iter().map(|(_, id)| *id).collect();
    let unique: HashSet<BRepFaceId> = ids.iter().copied().collect();
    assert_eq!(
        unique.len(),
        ids.len(),
        "all Sweep face IDs must be unique within one owner"
    );

    // The sequential TopologyFaceId stream is likewise collision-free.
    let seq: HashSet<TopologyFaceId> = pairs.iter().map(|(seq, _)| *seq).collect();
    assert_eq!(seq.len(), pairs.len(), "TopologyFaceId stream has no dupes");
}

/// IDs minted under different owners are fully disjoint — owner
/// disambiguation holds for every face of the Sweep.
#[test]
fn sweep_face_ids_disjoint_across_owners() {
    let owner_a = BRepOwnerId::from_bytes([0x11; 16]);
    let owner_b = BRepOwnerId::from_bytes([0x22; 16]);
    let op = SweepOp::new(unit_square(), z_path(&[0.0, 1.0, 2.0]));

    let ids_a: HashSet<BRepFaceId> = op
        .brep_face_ids(owner_a)
        .into_iter()
        .map(|(_, id)| id)
        .collect();
    let ids_b: HashSet<BRepFaceId> = op
        .brep_face_ids(owner_b)
        .into_iter()
        .map(|(_, id)| id)
        .collect();

    assert!(
        ids_a.is_disjoint(&ids_b),
        "Sweep face IDs must be disjoint across distinct owners"
    );
}

/// Numeric rebuilds that preserve `profile_count` and
/// `path_segment_count` keep byte-identical IDs — proven for BOTH caps
/// and sides. The substrate never inspects profile or path coordinates,
/// so scaling the profile and moving the path vertices (without changing
/// either count) leaves every face ID untouched.
#[test]
fn sweep_face_ids_stable_under_numeric_rebuild() {
    let owner = BRepOwnerId::from_bytes([0x44; 16]);

    // Baseline: unit square over a 3-point z-path.
    let baseline = SweepOp::new(unit_square(), z_path(&[0.0, 1.0, 2.0]));

    // Rebuild: same vertex counts (n = 4, m = 3) but different numeric
    // coordinates — a larger square and a path with X drift and different
    // Z spacing.
    let scaled_square =
        Polygon2D::new(vec![[0.0, 0.0], [3.0, 0.0], [3.0, 3.0], [0.0, 3.0]]).expect("scaled");
    let drifted_path = Polyline3D::new(vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.7], [2.0, 0.0, 4.2]])
        .expect("drifted path");
    let rebuilt = SweepOp::new(scaled_square, drifted_path);

    let baseline_ids = baseline.brep_face_ids(owner);
    let rebuilt_ids = rebuilt.brep_face_ids(owner);

    // Whole ordered ID stream is byte-identical — caps and sides alike.
    assert_eq!(
        baseline_ids, rebuilt_ids,
        "numeric rebuild preserving counts must keep all Sweep face IDs"
    );

    // Spell out the caps explicitly so the contract is unmistakable.
    assert_eq!(baseline_ids[0].1, rebuilt_ids[0].1, "first cap stable");
    assert_eq!(baseline_ids[1].1, rebuilt_ids[1].1, "last cap stable");
    // And at least one side.
    assert_eq!(baseline_ids[2].1, rebuilt_ids[2].1, "first side stable");
}

/// A profile-count change (square → pentagon) breaks side IDs by
/// construction, while the categorical caps stay stable.
#[test]
fn sweep_profile_count_change_breaks_sides_keeps_caps() {
    let owner = BRepOwnerId::from_bytes([0x55; 16]);
    let square = SweepOp::new(unit_square(), z_path(&[0.0, 1.0]));
    let penta = SweepOp::new(pentagon(), z_path(&[0.0, 1.0]));

    let square_ids = square.brep_face_ids(owner);
    let penta_ids = penta.brep_face_ids(owner);

    // Caps are categorical → stable across the topology change.
    assert_eq!(square_ids[0].1, penta_ids[0].1, "first cap categorical");
    assert_eq!(square_ids[1].1, penta_ids[1].1, "last cap categorical");

    // The first side (segment 0, edge 0) differs because profile_count is
    // hashed into the BLAKE3 input.
    assert_ne!(
        square_ids[2].1, penta_ids[2].1,
        "side IDs must break across a profile-count change"
    );
}

/// A path-segment-count change (2-point path → 3-point path) breaks side
/// IDs by construction, while the categorical caps stay stable.
#[test]
fn sweep_path_segment_count_change_breaks_sides_keeps_caps() {
    let owner = BRepOwnerId::from_bytes([0x66; 16]);
    let short = SweepOp::new(unit_square(), z_path(&[0.0, 1.0]));
    let long = SweepOp::new(unit_square(), z_path(&[0.0, 1.0, 2.0]));

    let short_ids = short.brep_face_ids(owner);
    let long_ids = long.brep_face_ids(owner);

    // Caps are categorical → stable across the topology change.
    assert_eq!(short_ids[0].1, long_ids[0].1, "first cap categorical");
    assert_eq!(short_ids[1].1, long_ids[1].1, "last cap categorical");

    // The first side (segment 0, edge 0) differs because
    // path_segment_count is hashed into the BLAKE3 input.
    assert_ne!(
        short_ids[2].1, long_ids[2].1,
        "side IDs must break across a path-segment-count change"
    );
}

/// Repeated `brep_face_ids` calls for the same Sweep and owner return
/// byte-identical IDs in byte-identical order.
#[test]
fn sweep_face_ids_repeated_calls_byte_identical() {
    let owner = BRepOwnerId::from_bytes([0x77; 16]);
    let op = SweepOp::new(pentagon(), z_path(&[0.0, 1.0, 2.0, 3.0]));
    let first = op.brep_face_ids(owner);
    let second = op.brep_face_ids(owner);
    assert_eq!(first, second);
    for ((_, a), (_, b)) in first.iter().zip(second.iter()) {
        assert_eq!(a.as_bytes(), b.as_bytes(), "ID bytes must be identical");
    }
}

/// Direct `brep_face_ids_for_node` resolution for a Sweep node returns
/// the same ordered IDs as a direct `BRepProvider::brep_face_ids` call.
#[test]
fn sweep_resolver_matches_direct_provider() {
    let owner = BRepOwnerId::from_bytes([0x88; 16]);
    let op = SweepOp::new(pentagon(), z_path(&[0.0, 1.0, 2.0]));
    let direct = op.brep_face_ids(owner);

    let mut cad = CadGraph::new();
    cad.begin_operation().expect("begin");
    let sweep_node = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Sweep(op))
        .expect("sweep");
    cad.commit("sweep").expect("commit");

    let resolved =
        brep_face_ids_for_node(cad.graph(), sweep_node, owner).expect("resolve sweep node");
    assert_eq!(resolved, direct);
}

/// Sweep face IDs are disjoint from the existing Extrude face-id
/// namespace — the `b"sweep:"` vs `b"extrude:"` operator-kind separator
/// keeps the two identity spaces apart even when discriminant bytes and
/// owner coincide.
#[test]
fn sweep_face_ids_disjoint_from_extrude_namespace() {
    let owner = BRepOwnerId::from_bytes([0x99; 16]);

    // Both caps carry no inner data and share discriminant byte 0
    // (FirstCap = 0, Bottom = 0); the kind separator alone keeps them
    // disjoint.
    let sweep_first = BRepFaceId::for_sweep_face(owner, SweepFaceTag::FirstCap);
    let extrude_bottom = BRepFaceId::for_extrude_face(owner, ExtrudeFaceTag::Bottom);
    assert_ne!(
        sweep_first, extrude_bottom,
        "Sweep and Extrude cap IDs must not collide"
    );

    // The whole Sweep ID set is disjoint from a same-owner Extrude ID set.
    let sweep_ids: HashSet<BRepFaceId> = SweepOp::new(unit_square(), z_path(&[0.0, 1.0]))
        .brep_face_ids(owner)
        .into_iter()
        .map(|(_, id)| id)
        .collect();
    let extrude_ids: HashSet<BRepFaceId> = [
        BRepFaceId::for_extrude_face(owner, ExtrudeFaceTag::Bottom),
        BRepFaceId::for_extrude_face(owner, ExtrudeFaceTag::Top),
        BRepFaceId::for_extrude_face(
            owner,
            ExtrudeFaceTag::Side {
                edge_index: 0,
                profile_count: 4,
            },
        ),
    ]
    .into_iter()
    .collect();
    assert!(
        sweep_ids.is_disjoint(&extrude_ids),
        "Sweep and Extrude face-id namespaces must be disjoint"
    );
}
