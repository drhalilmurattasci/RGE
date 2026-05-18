//! cad-projection face-ID integration smoke for the topology-preserving
//! `TransformOp` consumer (GitHub issue #23).
//!
//! Follow-up to the `TransformOp` face-label preservation work: `TransformOp`
//! transforms vertex positions, clones triangle indices unchanged, and passes
//! upstream `face_labels` through one-for-one. The `cad-core`
//! `brep_face_ids_for_node` resolver mirrors that — it treats `Transform` as an
//! identity-preserving operator and recurses to the unique upstream input
//! (`crates/cad-core/src/topology/resolve.rs`).
//!
//! This single smoke proves the projection consumer surface
//! (`CadProjection::brep_face_id_for_triangle`) still resolves a Cuboid
//! projected *through* a non-identity `TransformOp` root directly to the
//! upstream Cuboid's stable `BRepFaceId`s. It exercises the existing lazy face
//! lookup only — no BRep identity derivation rule is changed.
//!
//! The test fails if `TransformOp` projection loses face labels (the projected
//! mesh's `face_labels` would be `None`, so every triangle resolves to `None`)
//! or if cad-projection face lookup no longer resolves through the Transform
//! root to the upstream Cuboid face IDs.
//!
//! Cuboid face identity itself is covered by `brep_face_id_lookup_smoke.rs`;
//! `TransformOp` label pass-through is covered by `TransformOp`'s own unit
//! tests in `crates/cad-core/src/operators/transform.rs`.

use rge_cad_core::{
    BRepFaceId, BRepOwnerId, BRepProvider, CadGraph, CuboidOp, OperatorNode, Tolerance, TransformOp,
};
use rge_cad_projection::{BRepHandle, CadProjection};
use rge_kernel_ecs::World;

const TEST_OWNER: BRepOwnerId = BRepOwnerId::from_bytes([0x23; 16]);

fn tol() -> Tolerance {
    Tolerance::new(0.001).expect("tolerance")
}

/// Build a `CadGraph` with `CuboidOp -> TransformOp`, project the Transform
/// root, and prove every projected triangle resolves through the Transform
/// root to the upstream Cuboid's stable `BRepFaceId`s.
///
/// The `TransformOp` carries a non-identity translation and default
/// rotation/scale (per the issue #23 plan): Transform is topology-preserving,
/// so the canonical Cuboid face-emission order (triangles 0-1 → NegZ, 2-3 →
/// PosZ, 4-5 → NegY, 6-7 → PosY, 8-9 → NegX, 10-11 → PosX) carries through
/// unchanged, and each triangle must resolve to the upstream Cuboid face ID
/// minted under the same `BRepOwnerId`.
#[test]
fn cuboid_through_transform_root_resolves_upstream_cuboid_face_ids() {
    // --- Build CuboidOp -> TransformOp, Transform as root. ---------------
    let cuboid = CuboidOp {
        width: 1.0,
        height: 1.0,
        depth: 1.0,
    };
    let mut graph = CadGraph::new();
    graph.begin_operation().expect("begin");
    let cuboid_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Cuboid(cuboid.clone()))
        .expect("add cuboid");
    // Non-identity translation, default (identity) rotation + unit scale.
    let transform_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Transform(TransformOp {
            translation: [3.0, -2.0, 1.0],
            ..TransformOp::default()
        }))
        .expect("add transform");
    graph
        .graph_mut()
        .expect("mut")
        .connect(cuboid_node, transform_node, 0)
        .expect("connect cuboid -> transform");
    graph
        .graph_mut()
        .expect("mut")
        .set_root(transform_node)
        .expect("set transform root");
    graph.commit("cuboid -> transform").expect("commit");

    // --- Spawn + project the Transform root. -----------------------------
    let mut projection = CadProjection::new();
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    // Entity is bound to the Transform root (NOT the upstream Cuboid node),
    // so the smoke specifically covers lookup through the projected
    // Transform output.
    let entity = projection
        .spawn_brep_entity(&mut world, transform_node)
        .expect("spawn");
    if let Some(mut em) = world.entity_mut(entity) {
        if let Some(mut handle) = em.get_mut::<BRepHandle>() {
            handle.brep_owner = Some(TEST_OWNER);
        }
    }
    projection.tick(&mut world, &graph, tol()).expect("tick");

    // --- Prove face lookup resolves through the Transform root. ----------
    let mesh = projection.projected_mesh(entity).expect("mesh");
    assert_eq!(
        mesh.triangle_count(),
        12,
        "Transform is topology-preserving: a Cuboid still projects to 12 triangles"
    );
    assert!(
        mesh.face_labels.is_some(),
        "TransformOp must preserve upstream face labels through projection"
    );

    // Direct upstream-Cuboid face IDs, minted under the SAME owner-seed.
    let direct_ids: Vec<BRepFaceId> = cuboid
        .brep_face_ids(TEST_OWNER)
        .into_iter()
        .map(|(_, id)| id)
        .collect();
    assert_eq!(direct_ids.len(), 6, "Cuboid emits exactly 6 face IDs");

    // Every projected triangle of the Transform root resolves to the
    // upstream Cuboid face ID in canonical 2-triangles-per-face order.
    for tri in 0..12 {
        let resolved = projection
            .brep_face_id_for_triangle(entity, tri, &world, graph.graph())
            .unwrap_or_else(|| {
                panic!("triangle {tri}: face lookup must resolve through the Transform root")
            });
        assert_eq!(
            resolved,
            direct_ids[tri / 2],
            "triangle {tri} must resolve to the upstream Cuboid face id (face {})",
            tri / 2
        );
    }
}
