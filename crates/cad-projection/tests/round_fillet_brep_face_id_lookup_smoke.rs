//! cad-projection face-ID integration smoke for the topology-preserving
//! `RoundFilletOp` consumer (GitHub issue #24).
//!
//! Per ADR-119 D4, `RoundFilletOp` is identity-preserving at the face
//! resolver: `RoundFilletOp::evaluate` clones the upstream Cuboid's
//! positions verbatim and substitutes filleted-edge endpoint indices with
//! new inset vertices *within the two adjacent faces' triangles*. Every
//! upstream face's surface still exists in the output mesh — possibly with
//! different vertex indices, but with the same semantic identity — so the
//! `cad-core` `brep_face_ids_for_node` resolver recurses through
//! `OperatorNode::RoundFillet` to the unique upstream and returns the
//! Cuboid's face IDs unchanged (`crates/cad-core/src/topology/resolve.rs`).
//! Cylinder-cap and corner-patch triangles added by RoundFillet are
//! nameless in v0 — labeled `TopologyFaceId::DEGENERATE` per ADR-119 D3.
//!
//! This single smoke proves the projection consumer surface
//! (`CadProjection::brep_face_id_for_triangle`) resolves a Cuboid projected
//! *through* a `RoundFilletOp` root: inherited non-degenerate face labels
//! resolve directly to the upstream Cuboid's stable `BRepFaceId`s, while
//! every `TopologyFaceId::DEGENERATE` cap/corner triangle resolves to
//! `None`. It exercises the existing lazy face lookup only — no BRep
//! identity derivation rule, RoundFillet tessellation, or label-emission
//! semantics are changed.
//!
//! Resolver-level coverage that `CuboidOp -> RoundFilletOp` inherits Cuboid
//! face IDs already lives in `crates/cad-core/src/topology/resolve.rs`;
//! Cuboid face identity itself is covered by `brep_face_id_lookup_smoke.rs`.
//! This file adds the missing cad-projection consumer smoke.

use rge_cad_core::{
    BRepEdgeProvider, BRepFaceId, BRepOwnerId, BRepProvider, CadGraph, CuboidOp, OperatorNode,
    RoundFilletOp, Tolerance, TopologyFaceId,
};
use rge_cad_projection::{BRepHandle, CadProjection};
use rge_kernel_ecs::World;

const TEST_OWNER: BRepOwnerId = BRepOwnerId::from_bytes([0x24; 16]);

fn tol() -> Tolerance {
    Tolerance::new(0.001).expect("tolerance")
}

/// Build a `CadGraph` with `CuboidOp -> RoundFilletOp`, project the
/// RoundFillet root, and prove face lookup resolves through the
/// RoundFillet root: every inherited non-degenerate Cuboid face label
/// resolves to the upstream Cuboid's stable `BRepFaceId`, and every
/// `TopologyFaceId::DEGENERATE` cap/corner triangle resolves to `None`.
///
/// The `RoundFilletOp` rounds a single Cuboid edge (radius 0.1). The edge
/// ID is selected through `CuboidOp::brep_edge_ids(owner)` and the operator
/// is constructed via `RoundFilletOp::new` — no edge IDs are synthesized by
/// hand and no private RoundFillet internals are touched.
#[test]
fn cuboid_through_round_fillet_root_resolves_upstream_and_degenerate_caps() {
    // --- Build CuboidOp -> RoundFilletOp, RoundFillet as root. -----------
    let cuboid = CuboidOp {
        width: 1.0,
        height: 1.0,
        depth: 1.0,
    };
    // Select a real upstream Cuboid edge ID via the BRepEdgeProvider
    // surface, then construct the RoundFilletOp against the upstream.
    let edges = cuboid.brep_edge_ids(TEST_OWNER);
    let round = RoundFilletOp::new(&cuboid, TEST_OWNER, vec![edges[0]], 0.1)
        .expect("round fillet construction");

    let mut graph = CadGraph::new();
    graph.begin_operation().expect("begin");
    let cuboid_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Cuboid(cuboid.clone()))
        .expect("add cuboid");
    let round_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::RoundFillet(round))
        .expect("add round fillet");
    graph
        .graph_mut()
        .expect("mut")
        .connect(cuboid_node, round_node, 0)
        .expect("connect cuboid -> round fillet");
    graph
        .graph_mut()
        .expect("mut")
        .set_root(round_node)
        .expect("set round fillet root");
    graph.commit("cuboid -> round fillet").expect("commit");

    // --- Spawn + project the RoundFillet root. ---------------------------
    let mut projection = CadProjection::new();
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    // Entity is bound to the RoundFillet root (NOT the upstream Cuboid
    // node), so the smoke specifically exercises graph-recursive face
    // lookup through `OperatorNode::RoundFillet`.
    let entity = projection
        .spawn_brep_entity(&mut world, round_node)
        .expect("spawn");
    if let Some(mut em) = world.entity_mut(entity) {
        if let Some(mut handle) = em.get_mut::<BRepHandle>() {
            handle.brep_owner = Some(TEST_OWNER);
        }
    }
    projection.tick(&mut world, &graph, tol()).expect("tick");

    // --- The projected RoundFillet mesh is labeled. ----------------------
    let mesh = projection.projected_mesh(entity).expect("mesh");
    let face_labels = mesh
        .face_labels
        .as_ref()
        .expect("RoundFillet inherits Cuboid's labeled tessellation");
    assert_eq!(
        face_labels.len(),
        mesh.triangle_count(),
        "one face label per projected triangle"
    );

    // The mesh must carry BOTH inherited non-degenerate Cuboid labels and
    // at least one DEGENERATE label from RoundFillet-added cap geometry.
    let non_degenerate: Vec<TopologyFaceId> = face_labels
        .iter()
        .copied()
        .filter(|label| *label != TopologyFaceId::DEGENERATE)
        .collect();
    assert!(
        !non_degenerate.is_empty(),
        "projected mesh must contain inherited non-degenerate Cuboid labels"
    );
    assert!(
        face_labels
            .iter()
            .any(|label| *label == TopologyFaceId::DEGENERATE),
        "projected mesh must contain at least one DEGENERATE cap/corner label"
    );
    // Every non-degenerate label inherited from the Cuboid is one of the
    // six canonical face indices TopologyFaceId(0..=5).
    for label in &non_degenerate {
        assert!(
            label.0 <= 5,
            "inherited non-degenerate label {label:?} must be a Cuboid face index 0..=5"
        );
    }

    // --- Direct upstream-Cuboid face IDs, minted under the SAME owner. ---
    let direct_pairs: Vec<(TopologyFaceId, BRepFaceId)> = cuboid.brep_face_ids(TEST_OWNER);
    assert_eq!(direct_pairs.len(), 6, "Cuboid emits exactly 6 face IDs");

    // All six Cuboid faces survive face-strip-removal substitution and are
    // still present as inherited labels in the projected RoundFillet mesh.
    for (topo, _) in &direct_pairs {
        assert!(
            non_degenerate.contains(topo),
            "Cuboid face {topo:?} must survive as an inherited RoundFillet label"
        );
    }

    // --- Per-triangle resolution through the RoundFillet root. -----------
    let mut saw_inherited = false;
    let mut saw_degenerate = false;
    for tri in 0..mesh.triangle_count() {
        let label = face_labels[tri];
        let resolved = projection.brep_face_id_for_triangle(entity, tri, &world, graph.graph());

        if label == TopologyFaceId::DEGENERATE {
            // RoundFillet-added cap/corner geometry is nameless in v0 —
            // no upstream face owns it, so the resolver finds no match.
            saw_degenerate = true;
            assert_eq!(
                resolved, None,
                "triangle {tri}: DEGENERATE cap/corner geometry must resolve to None"
            );
        } else {
            // Inherited Cuboid face: must resolve to the upstream Cuboid
            // BRepFaceId minted under the same owner for that exact
            // TopologyFaceId — not merely "some face id".
            saw_inherited = true;
            let expected = direct_pairs
                .iter()
                .find(|(topo, _)| *topo == label)
                .map(|(_, id)| *id)
                .unwrap_or_else(|| {
                    panic!("triangle {tri}: label {label:?} has no upstream Cuboid face id")
                });
            assert_eq!(
                resolved,
                Some(expected),
                "triangle {tri} (label {label:?}) must resolve to the upstream Cuboid face id"
            );
        }
    }
    assert!(
        saw_inherited,
        "at least one inherited Cuboid triangle must be observed"
    );
    assert!(
        saw_degenerate,
        "at least one DEGENERATE cap/corner triangle must be observed"
    );

    // --- Same-fixture None checks. ---------------------------------------
    // Out-of-bounds triangle index returns None rather than panicking.
    assert_eq!(
        projection.brep_face_id_for_triangle(
            entity,
            mesh.triangle_count() + 999,
            &world,
            graph.graph()
        ),
        None,
        "out-of-bounds triangle index must resolve to None"
    );

    // An entity whose `BRepHandle.brep_owner` is cleared resolves to None
    // even with a valid, labeled projected mesh.
    if let Some(mut em) = world.entity_mut(entity) {
        if let Some(mut handle) = em.get_mut::<BRepHandle>() {
            handle.brep_owner = None;
        }
    }
    assert_eq!(
        projection.brep_face_id_for_triangle(entity, 0, &world, graph.graph()),
        None,
        "missing BRepHandle.brep_owner must resolve to None"
    );
}
