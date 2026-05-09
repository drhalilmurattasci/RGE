//! End-to-end smoke for the sub-7.2-δ B-Rep face-identity substrate
//! (LoftOp — two-input local-provider, fourth operator).
//!
//! These tests are the gate for the dispatch — they prove (1) angle
//! / length-stability across rebuilds, (2) shape-size stability at the
//! same point count (substrate does not inspect coordinates), (3) Side
//! IDs break across profile-count changes (both profiles must change
//! together because `LoftOp::evaluate` enforces equal counts at runtime),
//! and (4) distinct owners produce disjoint identity spaces (mirroring
//! the cuboid / extrude / revolve precedent).
//!
//! `LoftOp` introduces a topology axis no prior dispatch has touched: a
//! **two-input local-provider** (two profile inputs producing one solid).
//! The substrate handles this by encoding BOTH profile counts
//! independently in the `Side` tag — substrate honesty even though
//! `LoftOp::evaluate` enforces equality at runtime.

use rge_cad_core::{BRepFaceId, BRepOwnerId, BRepProvider, LoftOp, Polygon2D};

fn square() -> Polygon2D {
    Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]).expect("square")
}

fn larger_square() -> Polygon2D {
    Polygon2D::new(vec![[0.0, 0.0], [3.0, 0.0], [3.0, 3.0], [0.0, 3.0]]).expect("larger square")
}

/// Strictly convex 5-point CCW pentagon.
///
/// Built from the regular-pentagon idiom that the per-operator unit tests
/// already validate (1.0 / 0.309 / 0.951 / 0.809 / 0.588 / etc.). 5 finite
/// points; passes `Polygon2D::new` (>= 3 points / no NaN / no coincident
/// adjacent) AND `LoftOp::evaluate`'s convexity gate (all cross-products
/// share sign).
fn pentagon() -> Polygon2D {
    Polygon2D::new(vec![
        [1.0, 0.0],
        [0.309, 0.951],
        [-0.809, 0.588],
        [-0.809, -0.588],
        [0.309, -0.951],
    ])
    .expect("regular pentagon")
}

/// Same profiles, three different `length` values; six `BRepFaceId`s
/// byte-identical across all three rebuilds.
///
/// This is the rebuild-stability assertion of sub-7.2-δ: changing only
/// `length` (the loft's height) MUST NOT alter the derived face identity,
/// because the BLAKE3 derivation feeds only `(domain, owner, kind, tag)` —
/// none of which vary with `length`. (For the `Side` variants, `tag`
/// includes `edge_index`, `profile_a_count`, `profile_b_count`; all
/// unchanged here.)
#[test]
fn loft_face_ids_stable_across_length_changes() {
    let owner = BRepOwnerId::from_bytes([0x9a; 16]);
    let a = LoftOp::new(square(), square(), 1.0).expect("len=1");
    let b = LoftOp::new(square(), square(), 2.0).expect("len=2");
    let c = LoftOp::new(square(), square(), 0.5).expect("len=0.5");

    let ids_a: Vec<BRepFaceId> = a
        .brep_face_ids(owner)
        .into_iter()
        .map(|(_, id)| id)
        .collect();
    let ids_b: Vec<BRepFaceId> = b
        .brep_face_ids(owner)
        .into_iter()
        .map(|(_, id)| id)
        .collect();
    let ids_c: Vec<BRepFaceId> = c
        .brep_face_ids(owner)
        .into_iter()
        .map(|(_, id)| id)
        .collect();

    assert_eq!(ids_a, ids_b);
    assert_eq!(ids_b, ids_c);
    // 4 sides + Bottom cap + Top cap = 6 face IDs.
    assert_eq!(ids_a.len(), 6);
}

/// Same point counts, different profile coordinates — IDs preserved
/// because the substrate does NOT inspect coordinate values.
///
/// This pins clauses 1 + 3 of the [`LoftFaceTag`] stability contract:
/// caps are categorical (no inner data) and Sides depend only on
/// `(edge_index, profile_a_count, profile_b_count)`; numeric coordinates
/// are never inspected. A unit square and a 3-unit square (same point
/// count, different coordinates) MUST produce byte-identical face IDs
/// for every face.
#[test]
fn loft_face_ids_stable_across_shape_size_changes() {
    let owner = BRepOwnerId::from_bytes([0xbc; 16]);
    let small = LoftOp::new(square(), square(), 1.0).expect("small");
    let large = LoftOp::new(larger_square(), larger_square(), 1.0).expect("large");

    let ids_small: Vec<BRepFaceId> = small
        .brep_face_ids(owner)
        .into_iter()
        .map(|(_, id)| id)
        .collect();
    let ids_large: Vec<BRepFaceId> = large
        .brep_face_ids(owner)
        .into_iter()
        .map(|(_, id)| id)
        .collect();

    assert_eq!(
        ids_small, ids_large,
        "shape-size change at same point count must preserve all IDs"
    );
}

/// Square (N=4) vs pentagon (N=5) — different topology, identity must NOT
/// be silently preserved on `Side` faces.
///
/// `LoftOp::evaluate` enforces `profile_a.len() == profile_b.len()` at
/// runtime, so the count-change scenario must change BOTH profiles
/// simultaneously. Bottom and Top IDs MAY (and per spec DO) match between
/// the two because their BLAKE3 input ends after the discriminant byte —
/// caps are categorical (clauses 1-2 of the stability contract). But
/// every `Side` ID MUST differ between square-loft and pentagon-loft
/// because BOTH profile counts (4 vs 5) are hashed into the `Side` tag.
#[test]
fn loft_face_ids_break_when_both_profile_counts_change_together() {
    let owner = BRepOwnerId::from_bytes([0xde; 16]);
    let sq_loft = LoftOp::new(square(), square(), 1.0).expect("square");
    let pen_loft = LoftOp::new(pentagon(), pentagon(), 1.0).expect("pentagon");

    let ids_sq: Vec<BRepFaceId> = sq_loft
        .brep_face_ids(owner)
        .into_iter()
        .map(|(_, id)| id)
        .collect();
    let ids_pen: Vec<BRepFaceId> = pen_loft
        .brep_face_ids(owner)
        .into_iter()
        .map(|(_, id)| id)
        .collect();

    assert_eq!(ids_sq.len(), 6); // 4 sides + Bottom + Top
    assert_eq!(ids_pen.len(), 7); // 5 sides + Bottom + Top

    // Bottom (index 0) and Top (index 1) IDs are EXPECTED to match across
    // the two — caps' BLAKE3 input ends after the discriminant byte and
    // does NOT hash either profile count in. This is part of the v0
    // substrate contract pinned in `LoftFaceTag`'s docstring (categorical
    // caps, clauses 1-2).
    assert_eq!(ids_sq[0], ids_pen[0], "Bottom is categorical");
    assert_eq!(ids_sq[1], ids_pen[1], "Top is categorical");

    // The break-point: every Side ID differs between square and pentagon.
    // The square has 4 sides at indices 2..6 and the pentagon has 5 sides
    // at indices 2..7. NO pair of side IDs across the two should collide,
    // because BOTH `profile_a_count` and `profile_b_count` (4 vs 5) are
    // hashed into each side's BLAKE3 input.
    for sq_side in &ids_sq[2..] {
        for pen_side in &ids_pen[2..] {
            assert_ne!(
                sq_side, pen_side,
                "Side IDs must NOT collide across profile-count change"
            );
        }
    }
}

/// Different `BRepOwnerId`s produce disjoint identity spaces — no
/// `BRepFaceId` minted under one owner collides with any minted under
/// another. Mirrors the cuboid / extrude / revolve `*_face_ids_distinct_
/// across_owners` precedent.
#[test]
fn loft_face_ids_distinct_across_owners() {
    let owner_x = BRepOwnerId::from_bytes([0x11; 16]);
    let owner_y = BRepOwnerId::from_bytes([0x22; 16]);
    let op = LoftOp::new(square(), square(), 1.0).expect("op");

    let ids_x: Vec<BRepFaceId> = op
        .brep_face_ids(owner_x)
        .into_iter()
        .map(|(_, id)| id)
        .collect();
    let ids_y: Vec<BRepFaceId> = op
        .brep_face_ids(owner_y)
        .into_iter()
        .map(|(_, id)| id)
        .collect();

    // Disjoint sets — different owners produce different identity spaces
    // for the same operator (caps included; `owner.as_bytes()` is hashed
    // into every BLAKE3 input regardless of the tag variant).
    for id_x in &ids_x {
        assert!(
            !ids_y.contains(id_x),
            "owner-disambiguation failed: id from owner_x found in owner_y's set"
        );
    }
}
