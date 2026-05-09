//! End-to-end smoke for the sub-7.2-α B-Rep face-identity substrate.
//!
//! These tests are the gate for the dispatch — they prove that
//! `BRepFaceId`s seeded by a caller-supplied `BRepOwnerId` are byte-identical
//! across parameter rebuilds of `CuboidOp` (the rebuild-stability property),
//! and that distinct owners produce disjoint identity spaces (the
//! owner-disambiguation property).

use rge_cad_core::{BRepFaceId, BRepOwnerId, BRepProvider, CuboidOp};

/// Same `BRepOwnerId`, three `CuboidOp` rebuilds with different parameters,
/// 6 face IDs byte-identical across all three rebuilds.
///
/// This is the load-bearing assertion of sub-7.2-α: parameter changes
/// (width/height/depth) must NOT alter the derived face identity, because
/// the derivation feeds only `(domain, owner, kind, tag)` into BLAKE3 — none
/// of those vary with the parameters.
#[test]
fn cuboid_face_ids_stable_across_parameter_rebuilds() {
    let owner = BRepOwnerId::from_bytes([0xab; 16]);
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

    let ids_a: Vec<BRepFaceId> = cuboid_a
        .brep_face_ids(owner)
        .into_iter()
        .map(|(_, id)| id)
        .collect();
    let ids_b: Vec<BRepFaceId> = cuboid_b
        .brep_face_ids(owner)
        .into_iter()
        .map(|(_, id)| id)
        .collect();
    let ids_c: Vec<BRepFaceId> = cuboid_c
        .brep_face_ids(owner)
        .into_iter()
        .map(|(_, id)| id)
        .collect();

    assert_eq!(ids_a, ids_b);
    assert_eq!(ids_b, ids_c);
    assert_eq!(ids_a.len(), 6);
}

/// Different `BRepOwnerId`s produce disjoint identity spaces — no
/// `BRepFaceId` minted under one owner collides with any minted under
/// another. This is the owner-disambiguation property: each independent
/// CAD model the caller wants to give a stable identity space gets its
/// own owner seed, and the spaces don't overlap.
#[test]
fn cuboid_face_ids_distinct_across_owners() {
    let owner_x = BRepOwnerId::from_bytes([0x11; 16]);
    let owner_y = BRepOwnerId::from_bytes([0x22; 16]);
    let cuboid = CuboidOp {
        width: 1.0,
        height: 1.0,
        depth: 1.0,
    };

    let ids_x: Vec<BRepFaceId> = cuboid
        .brep_face_ids(owner_x)
        .into_iter()
        .map(|(_, id)| id)
        .collect();
    let ids_y: Vec<BRepFaceId> = cuboid
        .brep_face_ids(owner_y)
        .into_iter()
        .map(|(_, id)| id)
        .collect();

    // Disjoint sets — different owners produce different identity spaces.
    for id_x in &ids_x {
        assert!(!ids_y.contains(id_x));
    }
}

/// Round-trip cross-check: re-running `brep_face_ids` on the same operator
/// instance produces byte-identical output every call (no time / no state
/// dependence). The substrate is purely deterministic over (owner, kind,
/// tag) — there is no internal state that could drift between invocations.
#[test]
fn cuboid_face_ids_are_byte_stable_within_a_single_process() {
    let owner = BRepOwnerId::from_bytes([0xcd; 16]);
    let cuboid = CuboidOp::default();

    let first = cuboid.brep_face_ids(owner);
    let second = cuboid.brep_face_ids(owner);
    let third = cuboid.brep_face_ids(owner);

    assert_eq!(first, second);
    assert_eq!(second, third);
    assert_eq!(first.len(), 6);
}
