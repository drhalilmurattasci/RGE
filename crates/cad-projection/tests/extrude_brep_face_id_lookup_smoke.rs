//! D-projection-β end-to-end smoke for cad-projection face-ID integration —
//! variable-N topology consumer (ExtrudeOp).
//!
//! Sub-α (D-projection-α) shipped face-ID propagation through projection for
//! Cuboid (fixed topology). Sub-β extends to Extrude — the smallest
//! variable-topology follow-up — and proves the projection consumer pattern
//! isn't a Cuboid special case. The API surface
//! (`ProjectedMesh.face_labels`, `BRepHandle.brep_owner`,
//! `CadProjection::brep_face_id_for_triangle`) is byte-identical to sub-α;
//! this test suite just validates that lazy resolution generalizes to
//! variable-N topology.
//!
//! These tests prove:
//!
//! 1. Each projected triangle of an Extrude root resolves to one of the
//!    `N + 2` stable [`BRepFaceId`]s minted by the upstream's
//!    [`BRepProvider`] impl.
//! 2. The mapping follows the canonical face-emission order: triangles 0..n-2
//!    → Bottom (face 0); n-2..2(n-2) → Top (face 1); 2(n-2)+2i, 2(n-2)+2i+1
//!    → Side(i) (face 2 + i).
//! 3. The same canonical order holds for both square (`n = 4`) and pentagon
//!    (`n = 5`) profiles.
//! 4. **LOAD-BEARING — rebuild stability across length changes.** Length is
//!    topology-preserving per D-7.2-β. Same edge IDs, same face IDs across
//!    rebuilds with same profile.
//! 5. **LOAD-BEARING — rebuild stability across profile coordinate changes.**
//!    Same profile vertex count + same vertex order = same topology.
//!    Coordinates change but BRepFaceIds are stable per D-7.2-β.
//! 6. **LOAD-BEARING — profile-count change invalidates Side face IDs.**
//!    Pentagon's `Side(i)` IDs are disjoint from square's, but Bottom and
//!    Top IDs match (categorical caps per D-7.2-β contract).
//! 7. Out-of-bounds triangle indices and `None` brep_owner return `None`.
//! 8. Distinct owners produce disjoint face IDs.

use rge_cad_core::{
    BRepFaceId, BRepOwnerId, BRepProvider, CadGraph, ExtrudeOp, OperatorNode, Polygon2D, Tolerance,
};
use rge_cad_projection::{BRepHandle, CadProjection};
use rge_kernel_ecs::{EntityId, World};

const TEST_OWNER: BRepOwnerId = BRepOwnerId::from_bytes([0xed; 16]);

fn tol() -> Tolerance {
    Tolerance::new(0.001).expect("tolerance")
}

fn unit_square() -> Polygon2D {
    Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]).expect("square")
}

fn larger_square() -> Polygon2D {
    Polygon2D::new(vec![[0.0, 0.0], [3.0, 0.0], [3.0, 3.0], [0.0, 3.0]]).expect("larger square")
}

fn pentagon() -> Polygon2D {
    Polygon2D::new(vec![
        [1.0, 0.0],
        [0.309, 0.951],
        [-0.809, 0.588],
        [-0.809, -0.588],
        [0.309, -0.951],
    ])
    .expect("pentagon")
}

/// Build a `(graph, projection, world, entity)` tuple with a single Extrude
/// committed and projected. The `BRepHandle.brep_owner` is set to
/// [`TEST_OWNER`] post-spawn so `brep_face_id_for_triangle` resolves
/// against a known owner-seed.
fn build_extrude_projection(
    profile: Polygon2D,
    length: f32,
) -> (CadGraph, CadProjection, World, EntityId) {
    let mut graph = CadGraph::new();
    graph.begin_operation().expect("begin");
    let extrude = ExtrudeOp::new(profile, length).expect("extrude");
    let extrude_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Extrude(extrude))
        .expect("add extrude");
    graph
        .graph_mut()
        .expect("mut2")
        .set_root(extrude_node)
        .expect("set root");
    graph.commit("test extrude").expect("commit");

    let mut projection = CadProjection::new();
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    let entity = projection
        .spawn_brep_entity(&mut world, extrude_node)
        .expect("spawn");
    if let Some(mut em) = world.entity_mut(entity) {
        if let Some(mut handle) = em.get_mut::<BRepHandle>() {
            handle.brep_owner = Some(TEST_OWNER);
        }
    }
    projection.tick(&mut world, &graph, tol()).expect("tick");

    (graph, projection, world, entity)
}

/// Each of the 12 triangles in a square Extrude's projected mesh resolves to
/// one of the 6 stable `BRepFaceId`s minted by the upstream's
/// `BRepProvider::brep_face_ids` impl.
#[test]
fn extrude_projection_query_returns_brep_face_id_for_each_triangle() {
    let (graph, projection, world, entity) = build_extrude_projection(unit_square(), 1.0);
    let mesh = projection.projected_mesh(entity).expect("mesh");
    // For a square (n=4) extrude: 4n-4 = 12 triangles.
    assert_eq!(mesh.triangle_count(), 12);

    // Direct provider face IDs.
    let extrude_for_compare = ExtrudeOp::new(unit_square(), 1.0).unwrap();
    let direct_pairs = extrude_for_compare.brep_face_ids(TEST_OWNER);
    assert_eq!(direct_pairs.len(), 6); // n+2 faces
    let direct_ids: Vec<BRepFaceId> = direct_pairs.iter().map(|(_, id)| *id).collect();

    // Each triangle returns one of the 6 face IDs.
    for tri in 0..12 {
        let id = projection
            .brep_face_id_for_triangle(entity, tri, &world, graph.graph())
            .unwrap_or_else(|| panic!("face id for triangle {tri}"));
        assert!(
            direct_ids.contains(&id),
            "triangle {tri} → unexpected face id"
        );
    }
}

/// The triangle → `BRepFaceId` mapping for an `n=4` square Extrude follows
/// the canonical face emission order documented in `ExtrudeOp::evaluate` and
/// `impl BRepProvider for ExtrudeOp`:
///
/// * Triangles 0-1 → Bottom (face 0; `n - 2 = 2` fan triangles)
/// * Triangles 2-3 → Top (face 1; `n - 2 = 2` fan triangles)
/// * Triangles 4-5 → Side(0) (face 2)
/// * Triangles 6-7 → Side(1) (face 3)
/// * Triangles 8-9 → Side(2) (face 4)
/// * Triangles 10-11 → Side(3) (face 5)
#[test]
fn extrude_projection_query_canonical_face_order_for_square() {
    let (graph, projection, world, entity) = build_extrude_projection(unit_square(), 1.0);
    let extrude_for_compare = ExtrudeOp::new(unit_square(), 1.0).unwrap();
    let direct_ids: Vec<BRepFaceId> = extrude_for_compare
        .brep_face_ids(TEST_OWNER)
        .into_iter()
        .map(|(_, id)| id)
        .collect();

    // Bottom triangles 0-1.
    for tri in 0..2 {
        let id = projection
            .brep_face_id_for_triangle(entity, tri, &world, graph.graph())
            .expect("face id");
        assert_eq!(id, direct_ids[0], "triangle {tri} should be Bottom");
    }
    // Top triangles 2-3.
    for tri in 2..4 {
        let id = projection
            .brep_face_id_for_triangle(entity, tri, &world, graph.graph())
            .expect("face id");
        assert_eq!(id, direct_ids[1], "triangle {tri} should be Top");
    }
    // Side(i) triangles at positions 4+2i, 5+2i.
    for i in 0..4 {
        let face_idx = 2 + i;
        for offset in 0..2 {
            let tri = 4 + 2 * i + offset;
            let id = projection
                .brep_face_id_for_triangle(entity, tri, &world, graph.graph())
                .expect("face id");
            assert_eq!(
                id, direct_ids[face_idx],
                "triangle {tri} should be Side({i})"
            );
        }
    }
}

/// Same canonical-order assertion as the square test but for `n = 5`
/// (pentagon) — variable-N coverage.
///
/// For `n=5` pentagon Extrude: `4*5 - 4 = 16` triangles.
///
/// * tri 0-2: Bottom (3 fan triangles)
/// * tri 3-5: Top (3 fan triangles)
/// * tri 6-7: Side(0)
/// * tri 8-9: Side(1)
/// * tri 10-11: Side(2)
/// * tri 12-13: Side(3)
/// * tri 14-15: Side(4)
#[test]
fn extrude_projection_query_canonical_face_order_for_pentagon() {
    let (graph, projection, world, entity) = build_extrude_projection(pentagon(), 1.0);
    let mesh = projection.projected_mesh(entity).expect("mesh");
    assert_eq!(mesh.triangle_count(), 16);

    let extrude_for_compare = ExtrudeOp::new(pentagon(), 1.0).unwrap();
    let direct_ids: Vec<BRepFaceId> = extrude_for_compare
        .brep_face_ids(TEST_OWNER)
        .into_iter()
        .map(|(_, id)| id)
        .collect();
    assert_eq!(direct_ids.len(), 7); // n+2 = 7

    // Bottom: triangles 0-2 (n-2=3 fan tris).
    for tri in 0..3 {
        let id = projection
            .brep_face_id_for_triangle(entity, tri, &world, graph.graph())
            .expect("face id");
        assert_eq!(id, direct_ids[0], "Bottom");
    }
    // Top: triangles 3-5.
    for tri in 3..6 {
        let id = projection
            .brep_face_id_for_triangle(entity, tri, &world, graph.graph())
            .expect("face id");
        assert_eq!(id, direct_ids[1], "Top");
    }
    // Sides: 2 triangles per side, 5 sides.
    for i in 0..5 {
        let face_idx = 2 + i;
        for offset in 0..2 {
            let tri = 6 + 2 * i + offset;
            let id = projection
                .brep_face_id_for_triangle(entity, tri, &world, graph.graph())
                .expect("face id");
            assert_eq!(id, direct_ids[face_idx], "Side({i})");
        }
    }
}

/// **LOAD-BEARING — rebuild stability across length changes.**
///
/// Length is topology-preserving per the D-7.2-β contract (the per-face tag
/// for `Bottom`/`Top`/`Side` does NOT include length). Same profile + same
/// vertex order ⇒ same edge IDs and same face IDs across rebuilds with
/// different lengths. This is the cad-projection consumer-pressure test for
/// the topology-preserving rebuild axis of the variable-N substrate.
#[test]
fn extrude_face_ids_stable_across_length_changes() {
    let (mut graph, mut projection, mut world, entity) =
        build_extrude_projection(unit_square(), 1.0);
    let initial_ids: Vec<BRepFaceId> = (0..12)
        .filter_map(|tri| projection.brep_face_id_for_triangle(entity, tri, &world, graph.graph()))
        .collect();
    assert_eq!(initial_ids.len(), 12);

    // Rebuild with length=2.0.
    graph.begin_operation().expect("begin");
    let new_extrude = ExtrudeOp::new(unit_square(), 2.0).unwrap();
    let new_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Extrude(new_extrude))
        .expect("rebuild");
    graph
        .graph_mut()
        .expect("mut2")
        .set_root(new_node)
        .expect("root");
    graph.commit("rebuild len=2").expect("commit");
    projection.remap_entity(entity, new_node).expect("remap");
    projection.tick(&mut world, &graph, tol()).expect("tick");

    let rebuilt_ids: Vec<BRepFaceId> = (0..12)
        .filter_map(|tri| projection.brep_face_id_for_triangle(entity, tri, &world, graph.graph()))
        .collect();
    assert_eq!(
        initial_ids, rebuilt_ids,
        "face IDs must be stable across length rebuilds"
    );
}

/// **LOAD-BEARING — rebuild stability across profile coordinate changes.**
///
/// Same profile vertex count + same vertex order = same topology.
/// Coordinates change but `BRepFaceId`s are stable per D-7.2-β (the `Side`
/// tag includes only `edge_index` and `profile_count`, neither of which
/// vary with vertex coordinates).
#[test]
fn extrude_face_ids_stable_across_profile_coordinate_changes() {
    let (mut graph, mut projection, mut world, entity) =
        build_extrude_projection(unit_square(), 1.0);
    let initial_ids: Vec<BRepFaceId> = (0..12)
        .filter_map(|tri| projection.brep_face_id_for_triangle(entity, tri, &world, graph.graph()))
        .collect();

    graph.begin_operation().expect("begin");
    let new_extrude = ExtrudeOp::new(larger_square(), 1.0).unwrap();
    let new_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Extrude(new_extrude))
        .expect("rebuild");
    graph
        .graph_mut()
        .expect("mut2")
        .set_root(new_node)
        .expect("root");
    graph.commit("rebuild larger profile").expect("commit");
    projection.remap_entity(entity, new_node).expect("remap");
    projection.tick(&mut world, &graph, tol()).expect("tick");

    let rebuilt_ids: Vec<BRepFaceId> = (0..12)
        .filter_map(|tri| projection.brep_face_id_for_triangle(entity, tri, &world, graph.graph()))
        .collect();
    assert_eq!(
        initial_ids, rebuilt_ids,
        "face IDs must be stable across profile coordinate changes"
    );
}

/// **LOAD-BEARING — profile-count change invalidates Side face IDs.**
///
/// `n=4 → n=5` changes topology. Per D-7.2-β:
///
/// * Bottom and Top IDs are **categorical** — their tag does NOT include
///   `profile_count`, so caps' IDs match across the topology change.
/// * Side IDs include `profile_count` in the tag, so EVERY square Side ID
///   must be disjoint from every pentagon Side ID.
///
/// This is the substrate-honesty test for sub-β: the projection consumer
/// surface preserves the D-7.2-β substrate's distinction between
/// categorical caps and topology-broken Sides.
#[test]
fn extrude_face_ids_change_when_profile_count_changes() {
    let (graph_sq, projection_sq, world_sq, entity_sq) =
        build_extrude_projection(unit_square(), 1.0);
    let sq_ids: Vec<BRepFaceId> = (0..12)
        .filter_map(|tri| {
            projection_sq.brep_face_id_for_triangle(entity_sq, tri, &world_sq, graph_sq.graph())
        })
        .collect();

    let (graph_pen, projection_pen, world_pen, entity_pen) =
        build_extrude_projection(pentagon(), 1.0);
    let pen_mesh = projection_pen.projected_mesh(entity_pen).expect("mesh");
    let pen_tri_count = pen_mesh.triangle_count();
    let pen_ids: Vec<BRepFaceId> = (0..pen_tri_count)
        .filter_map(|tri| {
            projection_pen.brep_face_id_for_triangle(entity_pen, tri, &world_pen, graph_pen.graph())
        })
        .collect();

    assert_eq!(sq_ids.len(), 12);
    assert_eq!(pen_ids.len(), 16);

    // Bottom and Top IDs are categorical (no profile_count in tag) — they
    // SHOULD match across profile shape changes per D-7.2-β. Triangles 0-1
    // are Bottom in the square's projected mesh; 0-2 are Bottom in the
    // pentagon's. Triangles 2-3 are Top in square; 3-5 are Top in pentagon.
    assert_eq!(
        sq_ids[0], pen_ids[0],
        "Bottom is categorical, should match across n=4 → n=5"
    );
    assert_eq!(
        sq_ids[2], pen_ids[3],
        "Top is categorical, should match across n=4 → n=5"
    );
    // Side IDs (square at tri 4+, pentagon at tri 6+) must be disjoint —
    // `profile_count` (4 vs 5) is in the Side tag.
    let sq_sides: Vec<&BRepFaceId> = sq_ids[4..].iter().collect();
    let pen_sides: Vec<&BRepFaceId> = pen_ids[6..].iter().collect();
    for sq_side in &sq_sides {
        for pen_side in &pen_sides {
            assert_ne!(
                sq_side, pen_side,
                "side IDs must not collide across profile-count change"
            );
        }
    }
}

/// An out-of-bounds triangle index returns `None` rather than panicking.
#[test]
fn extrude_query_returns_none_for_out_of_bounds_triangle() {
    let (graph, projection, world, entity) = build_extrude_projection(unit_square(), 1.0);
    assert_eq!(
        projection.brep_face_id_for_triangle(entity, 99, &world, graph.graph()),
        None
    );
}

/// An entity whose `BRepHandle.brep_owner` is `None` (the legacy default)
/// returns `None` even when the projected mesh has `face_labels`.
#[test]
fn extrude_query_returns_none_when_brep_owner_is_none() {
    let (graph, projection, mut world, entity) = build_extrude_projection(unit_square(), 1.0);
    if let Some(mut em) = world.entity_mut(entity) {
        if let Some(mut handle) = em.get_mut::<BRepHandle>() {
            handle.brep_owner = None;
        }
    }
    assert_eq!(
        projection.brep_face_id_for_triangle(entity, 0, &world, graph.graph()),
        None
    );
}

/// Distinct `BRepOwnerId` seeds produce disjoint `BRepFaceId` spaces even
/// when the geometry is byte-identical. Owner-seeded identity is preserved
/// through the projection consumer surface for variable-N topology.
#[test]
fn distinct_owners_produce_disjoint_face_ids_through_extrude_projection() {
    let owner_y = BRepOwnerId::from_bytes([0xab; 16]);
    assert_ne!(TEST_OWNER, owner_y, "owners must be distinct for this test");

    let (graph, projection, mut world, entity) = build_extrude_projection(unit_square(), 1.0);
    let ids_x: Vec<BRepFaceId> = (0..12)
        .filter_map(|tri| projection.brep_face_id_for_triangle(entity, tri, &world, graph.graph()))
        .collect();
    if let Some(mut em) = world.entity_mut(entity) {
        if let Some(mut handle) = em.get_mut::<BRepHandle>() {
            handle.brep_owner = Some(owner_y);
        }
    }
    let ids_y: Vec<BRepFaceId> = (0..12)
        .filter_map(|tri| projection.brep_face_id_for_triangle(entity, tri, &world, graph.graph()))
        .collect();
    assert_eq!(ids_x.len(), 12);
    assert_eq!(ids_y.len(), 12);

    for id_x in &ids_x {
        assert!(
            !ids_y.contains(id_x),
            "owner-x face id leaked into owner-y space"
        );
    }
}
