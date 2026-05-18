//! Integration smoke for GitHub issue #25: a committed
//! `CuboidOp -> RoundFilletOp` graph chain preserves the upstream
//! Cuboid's B-Rep face and edge identity vectors through the public
//! graph resolvers.
//!
//! This is the cargo-discovered integration counterpart to the
//! in-crate unit tests in `topology/resolve.rs` and
//! `topology/edge_resolve.rs`. It keeps the proof at the public API
//! surface and pins two independent variation axes:
//!
//! 1. **Radius variation** — three valid positive radii on the same
//!    selected edge set. RoundFillet's radius parameter never enters
//!    B-Rep ID derivation (ADR-119 D2/D4), so the resolved Cuboid face
//!    and edge IDs are byte-identical across radii.
//! 2. **Selected-edge variation** — a single-edge selection, a
//!    multi-edge selection, and all 12 Cuboid edges. RoundFillet
//!    inherits the *full* upstream face/edge set regardless of which
//!    edges are filleted; the filleted edges themselves survive
//!    unchanged.
//!
//! For every variation, the resolved RoundFillet-node vectors are
//! compared with `assert_eq!` against the direct upstream Cuboid
//! vectors — no sorting, deduplication, filtering, or normalization.

use rge_cad_core::{
    brep_edge_ids_for_node, brep_face_ids_for_node, BRepEdgeId, BRepEdgeProvider, BRepFaceId,
    BRepOwnerId, BRepProvider, CadGraph, CuboidOp, OperatorNode, RoundFilletOp,
};

/// Fixed owner used for every chain in this smoke. A constant owner
/// keeps the upstream reference vectors and the resolved vectors
/// directly comparable.
fn owner() -> BRepOwnerId {
    BRepOwnerId::from_bytes([0x25; 16])
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

/// Build, commit, and resolve one `CuboidOp -> RoundFilletOp` chain.
///
/// Returns the face and edge ID vectors resolved at the RoundFillet
/// node via `brep_face_ids_for_node` / `brep_edge_ids_for_node`.
fn resolve_round_fillet_chain(
    cuboid: CuboidOp,
    owner: BRepOwnerId,
    selected_edges: Vec<BRepEdgeId>,
    radius: f32,
) -> (Vec<BRepFaceId>, Vec<BRepEdgeId>) {
    let round = RoundFilletOp::new(&cuboid, owner, selected_edges, radius)
        .expect("RoundFilletOp::new must accept a valid Cuboid edge selection");

    let mut cad = CadGraph::new();
    cad.begin_operation().expect("begin");
    let cuboid_node = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Cuboid(cuboid))
        .expect("add cuboid");
    let round_node = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::RoundFillet(round))
        .expect("add round fillet");
    cad.graph_mut()
        .expect("mut")
        .connect(cuboid_node, round_node, 0)
        .expect("connect cuboid -> round fillet");
    cad.commit("cuboid -> round fillet").expect("commit");

    let faces: Vec<BRepFaceId> = brep_face_ids_for_node(cad.graph(), round_node, owner)
        .expect("resolve faces at RoundFillet node")
        .into_iter()
        .map(|(_, id)| id)
        .collect();
    let edges: Vec<BRepEdgeId> = brep_edge_ids_for_node(cad.graph(), round_node, owner)
        .expect("resolve edges at RoundFillet node");
    (faces, edges)
}

/// Radius variation: three valid positive radii on the *same* selected
/// edge set all resolve to the upstream Cuboid's face and edge IDs.
///
/// RoundFillet's radius is pure geometry — it never enters B-Rep ID
/// derivation — so the resolved vectors must be byte-identical to the
/// direct upstream Cuboid vectors for every radius.
#[test]
fn radius_variation_preserves_cuboid_face_and_edge_ids() {
    let owner = owner();
    let cuboid = fixture_cuboid();
    let (ref_faces, ref_edges) = upstream_refs(&cuboid, owner);

    // Same single-edge selection across all radii.
    let selection = vec![ref_edges[0]];

    for &radius in &[0.05_f32, 0.1, 0.25] {
        let (faces, edges) =
            resolve_round_fillet_chain(cuboid.clone(), owner, selection.clone(), radius);

        assert_eq!(
            faces, ref_faces,
            "radius {radius}: resolved RoundFillet face IDs must equal the upstream Cuboid face IDs"
        );
        assert_eq!(
            edges, ref_edges,
            "radius {radius}: resolved RoundFillet edge IDs must equal the upstream Cuboid edge IDs"
        );
    }

    assert_eq!(ref_faces.len(), 6, "cuboid has 6 faces");
    assert_eq!(ref_edges.len(), 12, "cuboid has 12 edges");
}

/// Selected-edge variation: a single-edge selection, a multi-edge
/// selection, and all 12 Cuboid edges all resolve to the upstream
/// Cuboid's face and edge IDs.
///
/// RoundFillet inherits the full upstream face/edge set regardless of
/// which edges are filleted, and the filleted edges themselves survive
/// unchanged in the resolved edge vector.
#[test]
fn selected_edge_variation_preserves_cuboid_face_and_edge_ids() {
    let owner = owner();
    let cuboid = fixture_cuboid();
    let (ref_faces, ref_edges) = upstream_refs(&cuboid, owner);

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
        let (faces, edges) =
            resolve_round_fillet_chain(cuboid.clone(), owner, selection.clone(), radius);

        assert_eq!(
            faces, ref_faces,
            "{selected_count}-edge selection: resolved RoundFillet face IDs must equal \
             the upstream Cuboid face IDs"
        );
        assert_eq!(
            edges, ref_edges,
            "{selected_count}-edge selection: resolved RoundFillet edge IDs must equal \
             all upstream Cuboid edge IDs, including the filleted edges"
        );

        // The filleted edges are inside the resolved set — RoundFillet
        // preserves curved edges (ADR-119 D2), unlike chamfer FilletOp.
        for filleted in &selection {
            assert!(
                edges.contains(filleted),
                "{selected_count}-edge selection: each filleted edge must survive \
                 the RoundFillet edge resolver"
            );
        }
    }
}
