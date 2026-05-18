//! Integration smoke for GitHub issue #28: a committed
//! `CuboidOp -> FilletOp` graph chain resolves B-Rep identity at the
//! Fillet node through the public graph resolvers.
//!
//! This is the cargo-discovered integration counterpart to the
//! in-crate unit tests in `topology/resolve.rs` and
//! `topology/edge_resolve.rs`, and the chamfer-Fillet sibling of
//! `round_fillet_brep_chain_resolution_smoke.rs`. It pins the chamfer
//! `FilletOp` contract, which is deliberately *opposite* to
//! `RoundFilletOp` on the edge axis:
//!
//! 1. **Face inheritance** — `FilletOp.evaluate` clones the upstream
//!    positions/indices verbatim and only appends chamfer-cap geometry,
//!    so every upstream Cuboid face survives bit-identically. The
//!    resolved Fillet face vector equals the full upstream Cuboid face
//!    vector across every radius and edge selection.
//! 2. **Edge filtering** — a chamfered edge loses its 2-endpoint
//!    geometry, so the Fillet edge resolver inherits the upstream
//!    Cuboid edges *minus* the selected filleted edges. RoundFillet
//!    preserves the curved edges; chamfer FilletOp drops them.
//!
//! Two independent variation axes are covered:
//!
//! * **Radius variation** — three valid positive radii on the same
//!   selected edge set. Fillet's radius never enters B-Rep ID
//!   derivation, so the resolved face/edge vectors are byte-identical
//!   across radii.
//! * **Selected-edge variation** — a single-edge selection, a
//!   multi-edge selection, and all 12 Cuboid edges. The resolved face
//!   vector is always the full upstream set; the resolved edge vector
//!   is the upstream set with that exact selection filtered out, down
//!   to an empty vector when all 12 edges are filleted.
//!
//! Every comparison uses `assert_eq!` against vectors produced
//! directly from the upstream Cuboid — no sorting, deduplication, or
//! normalization.

use rge_cad_core::{
    brep_edge_ids_for_node, brep_face_ids_for_node, BRepEdgeId, BRepEdgeProvider, BRepFaceId,
    BRepOwnerId, BRepProvider, CadGraph, CuboidOp, FilletOp, OperatorNode,
};

/// Fixed owner used for every chain in this smoke. A constant owner
/// keeps the upstream reference vectors and the resolved vectors
/// directly comparable.
fn owner() -> BRepOwnerId {
    BRepOwnerId::from_bytes([0x28; 16])
}

/// Deterministic, non-cubic `CuboidOp` fixture. Distinct extents make
/// the test independent of any accidental width == height == depth
/// symmetry while staying a plain 6-face / 12-edge cuboid.
fn fixture_cuboid() -> CuboidOp {
    CuboidOp {
        width: 2.0,
        height: 1.5,
        depth: 1.0,
    }
}

/// Upstream reference vectors taken directly from the Cuboid:
/// `brep_face_ids` mapped to `Vec<BRepFaceId>` in emission order, and
/// `brep_edge_ids` as `Vec<BRepEdgeId>` in emission order.
fn upstream_refs(cuboid: &CuboidOp, owner: BRepOwnerId) -> (Vec<BRepFaceId>, Vec<BRepEdgeId>) {
    let faces: Vec<BRepFaceId> = cuboid
        .brep_face_ids(owner)
        .into_iter()
        .map(|(_, id)| id)
        .collect();
    let edges: Vec<BRepEdgeId> = cuboid.brep_edge_ids(owner);
    (faces, edges)
}

/// Upstream Cuboid edge vector with the selected filleted edges
/// removed, preserving original upstream emission order and byte
/// identity for the surviving edges.
fn upstream_edges_minus_selection(
    upstream_edges: &[BRepEdgeId],
    selection: &[BRepEdgeId],
) -> Vec<BRepEdgeId> {
    upstream_edges
        .iter()
        .filter(|e| !selection.contains(e))
        .copied()
        .collect()
}

/// Build, commit, and resolve one `CuboidOp -> FilletOp` chain.
///
/// Returns the face and edge ID vectors resolved at the Fillet node
/// via `brep_face_ids_for_node` / `brep_edge_ids_for_node`.
fn resolve_fillet_chain(
    cuboid: CuboidOp,
    owner: BRepOwnerId,
    selected_edges: Vec<BRepEdgeId>,
    radius: f32,
) -> (Vec<BRepFaceId>, Vec<BRepEdgeId>) {
    let fillet = FilletOp::new(&cuboid, owner, selected_edges, radius)
        .expect("FilletOp::new must accept a valid Cuboid edge selection");

    let mut cad = CadGraph::new();
    cad.begin_operation().expect("begin");
    let cuboid_node = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Cuboid(cuboid))
        .expect("add cuboid");
    let fillet_node = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Fillet(fillet))
        .expect("add fillet");
    cad.graph_mut()
        .expect("mut")
        .connect(cuboid_node, fillet_node, 0)
        .expect("connect cuboid -> fillet");
    cad.commit("cuboid -> fillet").expect("commit");

    let faces: Vec<BRepFaceId> = brep_face_ids_for_node(cad.graph(), fillet_node, owner)
        .expect("resolve faces at Fillet node")
        .into_iter()
        .map(|(_, id)| id)
        .collect();
    let edges: Vec<BRepEdgeId> = brep_edge_ids_for_node(cad.graph(), fillet_node, owner)
        .expect("resolve edges at Fillet node");
    (faces, edges)
}

/// Radius variation: three valid positive radii on the *same* selected
/// edge set all resolve to the upstream Cuboid's full face vector and
/// to the upstream edge vector minus the filleted selection.
///
/// Fillet's radius is pure geometry — it never enters B-Rep ID
/// derivation — so the resolved vectors must be byte-identical across
/// every radius.
#[test]
fn radius_variation_inherits_faces_and_filters_selected_edges() {
    let owner = owner();
    let cuboid = fixture_cuboid();
    let (ref_faces, ref_edges) = upstream_refs(&cuboid, owner);
    assert_eq!(ref_faces.len(), 6, "cuboid has 6 faces");
    assert_eq!(ref_edges.len(), 12, "cuboid has 12 edges");

    // Same single-edge selection across all radii.
    let selection = vec![ref_edges[0]];
    let expected_edges = upstream_edges_minus_selection(&ref_edges, &selection);
    assert_eq!(
        expected_edges.len(),
        11,
        "one filleted edge removed from 12 leaves 11"
    );

    for &radius in &[0.05_f32, 0.1, 0.25] {
        let (faces, edges) = resolve_fillet_chain(cuboid.clone(), owner, selection.clone(), radius);

        assert_eq!(
            faces, ref_faces,
            "radius {radius}: resolved Fillet face IDs must equal the full upstream Cuboid face IDs"
        );
        assert_eq!(
            edges, expected_edges,
            "radius {radius}: resolved Fillet edge IDs must equal the upstream Cuboid \
             edges minus the filleted selection"
        );
    }
}

/// Selected-edge variation: a single-edge selection, a multi-edge
/// selection, and all 12 Cuboid edges.
///
/// For every selection the resolved face vector is the full upstream
/// Cuboid face vector, and the resolved edge vector is the upstream
/// Cuboid edge vector with that exact selection filtered out — in
/// original upstream order, byte-identical for the surviving edges.
#[test]
fn selected_edge_variation_inherits_faces_and_filters_selected_edges() {
    let owner = owner();
    let cuboid = fixture_cuboid();
    let (ref_faces, ref_edges) = upstream_refs(&cuboid, owner);
    assert_eq!(ref_faces.len(), 6, "cuboid has 6 faces");
    assert_eq!(ref_edges.len(), 12, "cuboid has 12 edges");

    let selections: Vec<Vec<BRepEdgeId>> = vec![
        // Single-edge selection.
        vec![ref_edges[0]],
        // Multi-edge selection (three non-contiguous edges).
        vec![ref_edges[0], ref_edges[3], ref_edges[7]],
        // All 12 Cuboid edges.
        ref_edges.clone(),
    ];

    // A fixed valid radius isolates this test to the edge-selection axis.
    let radius = 0.1_f32;

    for selection in selections {
        let selected_count = selection.len();
        let expected_edges = upstream_edges_minus_selection(&ref_edges, &selection);

        let (faces, edges) = resolve_fillet_chain(cuboid.clone(), owner, selection.clone(), radius);

        assert_eq!(
            faces, ref_faces,
            "{selected_count}-edge selection: resolved Fillet face IDs must equal \
             the full upstream Cuboid face IDs"
        );
        assert_eq!(
            edges, expected_edges,
            "{selected_count}-edge selection: resolved Fillet edge IDs must equal \
             the upstream Cuboid edges minus that exact selection"
        );

        // The filleted edges are dropped — chamfer FilletOp filters
        // selected edges, unlike RoundFillet which preserves them.
        for filleted in &selection {
            assert!(
                !edges.contains(filleted),
                "{selected_count}-edge selection: each filleted edge must be filtered \
                 out of the resolved Fillet edge vector"
            );
        }
    }
}

/// All-12-edge selection: filleting every Cuboid edge leaves the
/// resolved Fillet edge vector empty, while the resolved Fillet face
/// vector still carries all 6 upstream Cuboid faces.
#[test]
fn all_edges_filleted_yields_empty_edges_and_full_faces() {
    let owner = owner();
    let cuboid = fixture_cuboid();
    let (ref_faces, ref_edges) = upstream_refs(&cuboid, owner);
    assert_eq!(ref_edges.len(), 12, "cuboid has 12 edges");

    let (faces, edges) = resolve_fillet_chain(cuboid.clone(), owner, ref_edges.clone(), 0.1);

    assert_eq!(
        faces, ref_faces,
        "all-12-edge selection: resolved Fillet face vector must still hold the 6 \
         upstream Cuboid face IDs"
    );
    assert_eq!(faces.len(), 6, "all 6 Cuboid faces survive chamfering");
    assert!(
        edges.is_empty(),
        "all-12-edge selection: every Cuboid edge is filleted, so the resolved Fillet \
         edge vector must be empty"
    );
}
