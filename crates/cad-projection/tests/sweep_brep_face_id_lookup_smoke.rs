//! cad-projection face-ID integration smoke for the variable-topology
//! `SweepOp` consumer.
//!
//! `SweepOp` already mints stable B-Rep face IDs (`SweepOp::brep_face_ids`)
//! and emits per-triangle topology labels from `SweepOp::evaluate`. This
//! suite proves the projection consumer surface
//! (`CadProjection::brep_face_id_for_triangle`) resolves a projected,
//! square-profile, monotonic-Z Sweep directly to those IDs — including the
//! canonical cap-first / side segment-major / profile-edge-major ordering
//! used by Sweep's labeled tessellation.
//!
//! Analogous to `extrude_brep_face_id_lookup_smoke.rs`,
//! `revolve_brep_face_id_lookup_smoke.rs`, and
//! `loft_brep_face_id_lookup_smoke.rs`, scoped to the Sweep consumer smoke
//! per ISSUE-21. Sweep face identity itself is covered by
//! `crates/cad-core/tests/sweep_face_identity_smoke.rs`.
//!
//! These tests prove:
//!
//! 1. A square profile (`n = 4`) swept along a 3-point monotonic-Z path
//!    (`m = 3`) projects to exactly `2 * 4 * 2 + 2 * (4 - 2) = 20`
//!    triangles, and the direct provider mints exactly
//!    `2 + 4 * (3 - 1) = 10` face IDs.
//! 2. Every projected triangle `0..20` resolves through
//!    `brep_face_id_for_triangle` to the corresponding direct
//!    `SweepOp::brep_face_ids` ID, in canonical order: triangles `0..2` →
//!    `FirstCap` (face index 0); `2..4` → `LastCap` (face index 1); side
//!    quad `(segment_index, edge_index)` → face index
//!    `2 + segment_index * 4 + edge_index`.
//! 3. An out-of-bounds triangle index and a `None` `brep_owner` both
//!    return `None`.

use rge_cad_core::{
    BRepFaceId, BRepOwnerId, BRepProvider, CadGraph, OperatorNode, Polygon2D, Polyline3D, SweepOp,
    Tolerance,
};
use rge_cad_projection::{BRepHandle, CadProjection};
use rge_kernel_ecs::{EntityId, World};

const TEST_OWNER: BRepOwnerId = BRepOwnerId::from_bytes([0x21; 16]);

fn tol() -> Tolerance {
    Tolerance::new(0.001).expect("tolerance")
}

/// Unit-square profile (`n = 4`, CCW).
fn unit_square() -> Polygon2D {
    Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]).expect("square")
}

/// Monotonic-Z path on the `+Z` axis through the supplied Z values.
fn z_path(zs: &[f32]) -> Polyline3D {
    Polyline3D::new(zs.iter().map(|z| [0.0, 0.0, *z]).collect()).expect("z-axis path")
}

/// Build a `(graph, projection, world, entity)` tuple with a single Sweep
/// committed and projected. The `BRepHandle.brep_owner` is set to
/// [`TEST_OWNER`] post-spawn so `brep_face_id_for_triangle` resolves
/// against a known owner-seed.
fn build_sweep_projection(
    profile: Polygon2D,
    path: Polyline3D,
) -> (CadGraph, CadProjection, World, EntityId) {
    let mut graph = CadGraph::new();
    graph.begin_operation().expect("begin");
    let sweep = SweepOp::new(profile, path);
    let sweep_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Sweep(sweep))
        .expect("add sweep");
    graph
        .graph_mut()
        .expect("mut2")
        .set_root(sweep_node)
        .expect("set root");
    graph.commit("test sweep").expect("commit");

    let mut projection = CadProjection::new();
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    let entity = projection
        .spawn_brep_entity(&mut world, sweep_node)
        .expect("spawn");
    if let Some(mut em) = world.entity_mut(entity) {
        if let Some(mut handle) = em.get_mut::<BRepHandle>() {
            handle.brep_owner = Some(TEST_OWNER);
        }
    }
    projection.tick(&mut world, &graph, tol()).expect("tick");

    (graph, projection, world, entity)
}

/// A square profile swept along a 3-point monotonic-Z path projects to
/// exactly 20 triangles, and the direct provider mints exactly 10 face IDs.
/// Every projected triangle `0..20` resolves to one of those 10 IDs.
#[test]
fn sweep_projection_query_returns_brep_face_id_for_each_triangle() {
    let (graph, projection, world, entity) =
        build_sweep_projection(unit_square(), z_path(&[0.0, 1.0, 2.0]));
    let mesh = projection.projected_mesh(entity).expect("mesh");
    // n=4, m=3 → 2 * n * (m - 1) + 2 * (n - 2) = 2*4*2 + 2*2 = 20 triangles.
    assert_eq!(mesh.triangle_count(), 20);

    // Direct provider face IDs: 2 + n * (m - 1) = 2 + 4 * 2 = 10.
    let direct_pairs =
        SweepOp::new(unit_square(), z_path(&[0.0, 1.0, 2.0])).brep_face_ids(TEST_OWNER);
    assert_eq!(direct_pairs.len(), 10);
    let direct_ids: Vec<BRepFaceId> = direct_pairs.iter().map(|(_, id)| *id).collect();

    // Each triangle resolves to one of the 10 face IDs.
    for tri in 0..20 {
        let id = projection
            .brep_face_id_for_triangle(entity, tri, &world, graph.graph())
            .unwrap_or_else(|| panic!("face id for triangle {tri}"));
        assert!(
            direct_ids.contains(&id),
            "triangle {tri} → unexpected face id"
        );
    }
}

/// The triangle → `BRepFaceId` mapping for a square Sweep over a 3-point
/// monotonic-Z path follows the canonical face-emission order documented
/// in `SweepOp::evaluate` and `impl BRepProvider for SweepOp`:
///
/// * Triangles 0-1 → `FirstCap` (face index 0; `n - 2 = 2` fan triangles)
/// * Triangles 2-3 → `LastCap` (face index 1; `n - 2 = 2` fan triangles)
/// * Side quad `(segment_index, edge_index)` → face index
///   `2 + segment_index * 4 + edge_index`, its 2 triangles at projected
///   positions `4 + 2 * (segment_index * 4 + edge_index)` and `+ 1`, in
///   segment-major, profile-edge-major order.
#[test]
fn sweep_projection_query_canonical_face_order_for_square() {
    let (graph, projection, world, entity) =
        build_sweep_projection(unit_square(), z_path(&[0.0, 1.0, 2.0]));
    let direct_ids: Vec<BRepFaceId> = SweepOp::new(unit_square(), z_path(&[0.0, 1.0, 2.0]))
        .brep_face_ids(TEST_OWNER)
        .into_iter()
        .map(|(_, id)| id)
        .collect();
    assert_eq!(direct_ids.len(), 10);

    // FirstCap triangles 0-1.
    for tri in 0..2 {
        let id = projection
            .brep_face_id_for_triangle(entity, tri, &world, graph.graph())
            .expect("face id");
        assert_eq!(id, direct_ids[0], "triangle {tri} should be FirstCap");
    }
    // LastCap triangles 2-3.
    for tri in 2..4 {
        let id = projection
            .brep_face_id_for_triangle(entity, tri, &world, graph.graph())
            .expect("face id");
        assert_eq!(id, direct_ids[1], "triangle {tri} should be LastCap");
    }
    // Side quads: segment-major, profile-edge-major. n=4, 2 path segments.
    for segment_index in 0..2 {
        for edge_index in 0..4 {
            let quad_ordinal = segment_index * 4 + edge_index;
            let face_idx = 2 + quad_ordinal;
            for offset in 0..2 {
                let tri = 4 + 2 * quad_ordinal + offset;
                let id = projection
                    .brep_face_id_for_triangle(entity, tri, &world, graph.graph())
                    .expect("face id");
                assert_eq!(
                    id, direct_ids[face_idx],
                    "triangle {tri} should be Side(segment {segment_index}, edge {edge_index})"
                );
            }
        }
    }
}

/// An out-of-bounds triangle index returns `None` rather than panicking.
#[test]
fn sweep_query_returns_none_for_out_of_bounds_triangle() {
    let (graph, projection, world, entity) =
        build_sweep_projection(unit_square(), z_path(&[0.0, 1.0, 2.0]));
    assert_eq!(
        projection.brep_face_id_for_triangle(entity, 99, &world, graph.graph()),
        None
    );
}

/// An entity whose `BRepHandle.brep_owner` is `None` (the legacy default)
/// returns `None` even when the projected mesh has `face_labels`.
#[test]
fn sweep_query_returns_none_when_brep_owner_is_none() {
    let (graph, projection, mut world, entity) =
        build_sweep_projection(unit_square(), z_path(&[0.0, 1.0, 2.0]));
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
