//! Render-backed face-selection sub-γ end-to-end smoke for
//! [`CadProjection::render_mesh_for`].
//!
//! These integration tests exercise the **adapter chain** that turns a
//! `cad-projection`-owned `ProjectedMesh` (cad-core-derived data with
//! `TopologyFaceId` face labels) into a `brep-render::RenderMesh`
//! (renderer-domain flat-shaded mesh with opaque `u64` face labels), and
//! verifies:
//!
//! * The triangle-count contract per D-projection-α (Cuboid) and
//!   D-projection-β (Extrude).
//! * **The chain-consistency invariant**: the renderer-side opaque
//!   `u64` label resolves to the SAME identity as the picker-side
//!   `BRepFaceId`. Without this, sub-α's opaque-buffer deviation could
//!   silently desync the two paths.
//! * **RoundFillet renderer-label alignment (GitHub issue #31)**: a
//!   `CuboidOp -> RoundFilletOp` root preserves inherited non-degenerate
//!   labels — and RoundFillet-added degenerate cap/corner labels — into
//!   `RenderMesh.face_labels` in the same triangle order as projection
//!   lookup.
//! * **FilletOp renderer-label alignment (GitHub issue #32)**: a
//!   `CuboidOp -> FilletOp` root preserves inherited non-degenerate Cuboid
//!   labels into `RenderMesh.face_labels` in projection-lookup triangle
//!   order, while `TopologyFaceId::DEGENERATE` chamfer-cap labels survive
//!   the adapter but are not resolved as upstream Cuboid faces.
//!
//! Test inventory:
//! * `render_mesh_face_labels_resolve_consistently_with_picker` — Cuboid
//!   renderer/picker chain consistency.
//! * `cuboid_render_mesh_triangle_count_matches_d_projection_alpha_contract`
//!   — Cuboid triangle-count contract.
//! * `extrude_square_render_mesh_triangle_count_matches_d_projection_beta_contract`
//!   — Extrude triangle-count contract.
//! * `round_fillet_render_mesh_face_labels_align_with_projection_lookup`
//!   — RoundFillet renderer-label alignment (GitHub issue #31).
//! * `fillet_render_mesh_face_labels_align_with_projection_lookup`
//!   — FilletOp renderer-label alignment (GitHub issue #32).

use rge_cad_core::{
    brep_face_ids_for_node, BRepEdgeProvider, BRepFaceId, BRepOwnerId, BRepProvider, CadGraph,
    CuboidOp, ExtrudeOp, FilletOp, OperatorNode, Polygon2D, RoundFilletOp, Tolerance,
    TopologyFaceId,
};
use rge_cad_projection::{BRepHandle, CadProjection};
use rge_kernel_ecs::World;

const ENTITY_OWNER: BRepOwnerId = BRepOwnerId::from_bytes([0x42; 16]);

fn tol() -> Tolerance {
    Tolerance::new(0.001).expect("tolerance")
}

/// Build a `(graph, projection, world, entity)` tuple with a single Cuboid
/// committed and projected. The `BRepHandle.brep_owner` is set to
/// `ENTITY_OWNER` post-spawn.
fn build_cuboid(
    width: f32,
    height: f32,
    depth: f32,
) -> (
    CadGraph,
    CadProjection,
    World,
    rge_kernel_ecs::EntityId,
    rge_kernel_graph_foundation::NodeId,
) {
    let mut graph = CadGraph::new();
    graph.begin_operation().expect("begin");
    let cuboid_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Cuboid(CuboidOp {
            width,
            height,
            depth,
        }))
        .expect("add cuboid");
    graph
        .graph_mut()
        .expect("mut2")
        .set_root(cuboid_node)
        .expect("set root");
    graph.commit("cuboid").expect("commit");

    let mut projection = CadProjection::new();
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    let entity = projection
        .spawn_brep_entity(&mut world, cuboid_node)
        .expect("spawn");
    if let Some(mut em) = world.entity_mut(entity) {
        if let Some(mut handle) = em.get_mut::<BRepHandle>() {
            handle.brep_owner = Some(ENTITY_OWNER);
        }
    }
    projection.tick(&mut world, &graph, tol()).expect("tick");

    (graph, projection, world, entity, cuboid_node)
}

/// **Test 1 — LOAD-BEARING: chain consistency between renderer-side opaque
/// `u64` labels and picker-side `BRepFaceId` resolution.**
///
/// Build a 1×1×1 Cuboid entity. For each of the 12 triangles, demonstrate
/// two parallel resolution paths converge to the same `BRepFaceId`:
///
/// 1. **Renderer path** — `render_mesh_for` returns `RenderMesh` with
///    opaque `u64` face labels; wrap each `u64` back into
///    `TopologyFaceId(label)` and resolve via the same
///    `brep_face_ids_for_node` machinery the picker uses internally.
/// 2. **Picker path** — `brep_face_id_for_triangle` returns the same
///    `BRepFaceId` directly.
///
/// Without this invariant, sub-α's opaque-buffer deviation
/// (`u64` instead of `TopologyFaceId` for renderer-tier consumption)
/// could silently desync the two paths. This test is the wire-format
/// proof that the round-trip works.
#[test]
fn render_mesh_face_labels_resolve_consistently_with_picker() {
    let (graph, projection, world, entity, source_node) = build_cuboid(1.0, 1.0, 1.0);

    let render = projection
        .render_mesh_for(entity, &world)
        .expect("must render for valid Cuboid entity");
    let render_labels = render
        .face_labels
        .as_ref()
        .expect("Cuboid is labeled per D-projection-α");
    assert_eq!(
        render_labels.len(),
        12,
        "Cuboid → 12 triangles → 12 face_labels"
    );

    // Build the resolver pair-list ONCE (matches the picker's internal
    // `brep_face_id_for_triangle` lookup pattern, but exposed inline here
    // so the test demonstrates the chain explicitly).
    let pairs: Vec<(TopologyFaceId, BRepFaceId)> =
        brep_face_ids_for_node(graph.graph(), source_node, ENTITY_OWNER)
            .expect("Cuboid resolver must succeed");

    for tri_idx in 0..12 {
        // Renderer-side path: opaque u64 → TopologyFaceId → BRepFaceId.
        let render_label_u64 = render_labels[tri_idx];
        let topology_id = TopologyFaceId(render_label_u64);
        let render_resolved: BRepFaceId = pairs
            .iter()
            .find(|(t, _)| *t == topology_id)
            .map(|(_, brep_id)| *brep_id)
            .expect("renderer-side label must resolve to a BRepFaceId");

        // Picker-side path: existing CadProjection::brep_face_id_for_triangle.
        let picker_resolved: BRepFaceId = projection
            .brep_face_id_for_triangle(entity, tri_idx, &world, graph.graph())
            .expect("picker-side resolution must succeed for Cuboid triangle");

        assert_eq!(
            render_resolved, picker_resolved,
            "renderer-side BRepFaceId resolution (via u64 → TopologyFaceId → resolver) MUST \
             match picker-side BRepFaceId resolution at triangle {tri_idx}; otherwise the \
             opaque-u64 wire format silently desyncs the two paths"
        );
    }
}

/// **Test 2** — 1×1×1 Cuboid → RenderMesh with `positions.len() == 36`,
/// `normals.len() == 36`, `indices.len() == 36`, `face_labels.len() == 12`
/// (per D-projection-α contract: 12 triangles × 3 vertices = 36 due to
/// vertex tripling).
#[test]
fn cuboid_render_mesh_triangle_count_matches_d_projection_alpha_contract() {
    let (_graph, projection, world, entity, _node) = build_cuboid(1.0, 1.0, 1.0);

    let mesh = projection
        .render_mesh_for(entity, &world)
        .expect("must render");
    assert_eq!(
        mesh.positions.len(),
        36,
        "Cuboid: 12 triangles × 3 vertex-tripling = 36 positions"
    );
    assert_eq!(mesh.normals.len(), 36);
    assert_eq!(mesh.indices.len(), 36);
    let labels = mesh.face_labels.as_ref().expect("Cuboid is labeled");
    assert_eq!(
        labels.len(),
        12,
        "Cuboid: 12 input triangles → 12 face_labels"
    );
    // Sanity check: indices are dense [0..36].
    for (i, idx) in mesh.indices.iter().enumerate() {
        assert_eq!(*idx as usize, i, "indices must be dense [0, 1, ..., 35]");
    }
}

/// **Test 3** — square ExtrudeOp (n=4) → 4n-4 = 12 triangles → 36
/// positions, 36 normals, 36 indices, 12 face_labels (per D-projection-β
/// contract).
#[test]
fn extrude_square_render_mesh_triangle_count_matches_d_projection_beta_contract() {
    let mut graph = CadGraph::new();
    graph.begin_operation().expect("begin");
    // 4-vertex square profile (CCW in the XY plane).
    let profile =
        Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]).expect("square");
    let extrude = ExtrudeOp::new(profile, 1.0).expect("extrude construction");
    let extrude_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Extrude(extrude))
        .expect("add extrude");
    graph
        .graph_mut()
        .expect("mut")
        .set_root(extrude_node)
        .expect("set root");
    graph.commit("extrude square").expect("commit");

    let mut projection = CadProjection::new();
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    let entity = projection
        .spawn_brep_entity(&mut world, extrude_node)
        .expect("spawn");
    if let Some(mut em) = world.entity_mut(entity) {
        if let Some(mut handle) = em.get_mut::<BRepHandle>() {
            handle.brep_owner = Some(ENTITY_OWNER);
        }
    }
    projection.tick(&mut world, &graph, tol()).expect("tick");

    let mesh = projection
        .render_mesh_for(entity, &world)
        .expect("must render Extrude entity");
    // 4n-4 = 12 triangles for n=4: bottom (n-2=2) + top (n-2=2) + sides
    // (2*n=8) = 12 → 36 positions / normals / indices.
    assert_eq!(
        mesh.positions.len(),
        36,
        "Extrude n=4: 12 triangles × 3 = 36 positions per D-projection-β"
    );
    assert_eq!(mesh.normals.len(), 36);
    assert_eq!(mesh.indices.len(), 36);
    let labels = mesh.face_labels.as_ref().expect("Extrude is labeled");
    assert_eq!(
        labels.len(),
        12,
        "Extrude n=4: 12 input triangles → 12 face_labels per D-projection-β"
    );
}

/// **Test 4 — RoundFillet renderer-label alignment for GitHub issue #31.**
///
/// Build a `CuboidOp -> RoundFilletOp` graph with the RoundFillet node as
/// root, project the RoundFillet root entity, and prove
/// `CadProjection::render_mesh_for` preserves the projected
/// `TopologyFaceId` labels into the renderer-side opaque `u64`
/// `RenderMesh.face_labels` buffer in the SAME triangle order used by
/// projection lookup:
///
/// 1. Every renderer-side `u64` equals the projected mesh's
///    `TopologyFaceId.0` carried at the same triangle index — the adapter
///    must not drop, reorder, or mis-convert labels.
/// 2. Every inherited non-degenerate renderer label resolves — through the
///    upstream Cuboid face mapping for `ENTITY_OWNER` — to the exact
///    `BRepFaceId` returned by `brep_face_id_for_triangle` for that
///    triangle.
/// 3. RoundFillet-added degenerate cap/corner labels survive the adapter:
///    at least one `TopologyFaceId::DEGENERATE.0` entry is present in the
///    renderer label buffer rather than being silently filtered out.
///
/// This smoke is distinct from the Cuboid and Extrude render-adapter
/// smokes above — it specifically exercises `OperatorNode::RoundFillet`
/// and binds the projected entity to the RoundFillet root, not the
/// upstream Cuboid node.
#[test]
fn round_fillet_render_mesh_face_labels_align_with_projection_lookup() {
    // --- Build CuboidOp -> RoundFilletOp, RoundFillet as root. -----------
    let cuboid = CuboidOp {
        width: 1.0,
        height: 1.0,
        depth: 1.0,
    };
    // Real upstream Cuboid edge IDs via the BRepEdgeProvider surface — no
    // `BRepEdgeId` is synthesized by hand.
    let edges = cuboid.brep_edge_ids(ENTITY_OWNER);
    let round = RoundFilletOp::new(&cuboid, ENTITY_OWNER, vec![edges[0]], 0.1)
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

    // --- Spawn + project the RoundFillet root (NOT the Cuboid node). -----
    let mut projection = CadProjection::new();
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    let entity = projection
        .spawn_brep_entity(&mut world, round_node)
        .expect("spawn");
    if let Some(mut em) = world.entity_mut(entity) {
        if let Some(mut handle) = em.get_mut::<BRepHandle>() {
            handle.brep_owner = Some(ENTITY_OWNER);
        }
    }
    projection.tick(&mut world, &graph, tol()).expect("tick");

    // --- Projected vs. renderer-side label buffers. ----------------------
    let projected = projection
        .projected_mesh(entity)
        .expect("RoundFillet root must have a projected mesh after tick");
    let projected_labels = projected
        .face_labels
        .as_ref()
        .expect("RoundFillet inherits the Cuboid's labeled tessellation");

    let render = projection
        .render_mesh_for(entity, &world)
        .expect("must render for the projected RoundFillet entity");
    let render_labels = render
        .face_labels
        .as_ref()
        .expect("labeled projected mesh must yield Some(face_labels) through the adapter");

    assert_eq!(
        render_labels.len(),
        projected.triangle_count(),
        "one opaque u64 renderer label per projected RoundFillet triangle"
    );
    assert_eq!(
        render_labels.len(),
        projected_labels.len(),
        "renderer-side and projected label buffers must have equal length"
    );
    // Renderer label N is exactly the projected `TopologyFaceId.0` at N —
    // proves the adapter preserves order and value, not just `Some(_)`.
    for (tri, (&render_label, &topo_label)) in render_labels
        .iter()
        .zip(projected_labels.iter())
        .enumerate()
    {
        assert_eq!(
            render_label, topo_label.0,
            "triangle {tri}: renderer-side u64 label must equal the projected \
             TopologyFaceId.0 carried at the same triangle index"
        );
    }

    // --- Upstream Cuboid face mapping, minted under the SAME owner. ------
    let direct_pairs: Vec<(TopologyFaceId, BRepFaceId)> = cuboid.brep_face_ids(ENTITY_OWNER);
    assert_eq!(direct_pairs.len(), 6, "Cuboid emits exactly 6 face IDs");

    let mut saw_inherited = false;
    let mut saw_degenerate = false;
    for (tri, &render_label) in render_labels.iter().enumerate() {
        let topo = TopologyFaceId(render_label);
        if topo == TopologyFaceId::DEGENERATE {
            // RoundFillet-added cap/corner geometry is nameless in v0; the
            // adapter must still carry the opaque DEGENERATE label through.
            saw_degenerate = true;
            continue;
        }
        // Inherited non-degenerate Cuboid label: resolving it through the
        // upstream Cuboid face mapping must equal the picker's answer for
        // the exact same triangle — not merely "some BRepFaceId".
        saw_inherited = true;
        let expected = direct_pairs
            .iter()
            .find(|(t, _)| *t == topo)
            .map(|(_, id)| *id)
            .unwrap_or_else(|| {
                panic!("triangle {tri}: renderer label {topo:?} has no upstream Cuboid face id")
            });
        let picker_resolved = projection
            .brep_face_id_for_triangle(entity, tri, &world, graph.graph())
            .expect("picker-side resolution must succeed for an inherited triangle");
        assert_eq!(
            picker_resolved, expected,
            "triangle {tri}: renderer label {topo:?} resolved through the upstream Cuboid \
             face mapping MUST match brep_face_id_for_triangle"
        );
    }

    assert!(
        saw_inherited,
        "at least one inherited non-degenerate renderer label must be observed"
    );
    assert!(
        render_labels
            .iter()
            .any(|label| *label == TopologyFaceId::DEGENERATE.0),
        "RoundFillet-added cap/corner geometry must leave at least one \
         TopologyFaceId::DEGENERATE.0 entry in the renderer label buffer"
    );
    assert!(
        saw_degenerate,
        "at least one DEGENERATE cap/corner triangle must be observed"
    );
}

/// **Test 5 — FilletOp renderer-label alignment for GitHub issue #32.**
///
/// Build a `CuboidOp -> FilletOp` graph with the Fillet node as root,
/// project the Fillet root entity, and prove
/// `CadProjection::render_mesh_for` preserves the projected
/// `TopologyFaceId` labels into the renderer-side opaque `u64`
/// `RenderMesh.face_labels` buffer in the SAME triangle order used by
/// projection lookup:
///
/// 1. Every renderer-side `u64` equals the projected mesh's
///    `TopologyFaceId.0` carried at the same triangle index — the adapter
///    must not drop, reorder, or mis-convert labels.
/// 2. Every inherited non-degenerate renderer label resolves — through the
///    Fillet-root graph resolver (`brep_face_ids_for_node` for
///    `fillet_node`) — to the exact `BRepFaceId` returned by
///    `brep_face_id_for_triangle` for that triangle.
/// 3. `TopologyFaceId::DEGENERATE` chamfer-cap labels survive the adapter
///    but are nameless v0 geometry: they must NOT be resolved as upstream
///    Cuboid faces, and picker-side lookup returns `None` for them.
///
/// This smoke is distinct from the Cuboid, Extrude, and RoundFillet
/// render-adapter smokes above — it specifically exercises
/// `OperatorNode::Fillet` and binds the projected entity to the Fillet
/// root, not the upstream Cuboid node.
#[test]
fn fillet_render_mesh_face_labels_align_with_projection_lookup() {
    // --- Build CuboidOp -> FilletOp, Fillet as root. ---------------------
    let cuboid = CuboidOp {
        width: 1.0,
        height: 1.0,
        depth: 1.0,
    };
    // Real upstream Cuboid edge IDs via the BRepEdgeProvider surface — no
    // `BRepEdgeId` is synthesized by hand.
    let edges = cuboid.brep_edge_ids(ENTITY_OWNER);
    let fillet =
        FilletOp::new(&cuboid, ENTITY_OWNER, vec![edges[0]], 0.1).expect("fillet construction");

    let mut graph = CadGraph::new();
    graph.begin_operation().expect("begin");
    let cuboid_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Cuboid(cuboid.clone()))
        .expect("add cuboid");
    let fillet_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Fillet(fillet))
        .expect("add fillet");
    graph
        .graph_mut()
        .expect("mut")
        .connect(cuboid_node, fillet_node, 0)
        .expect("connect cuboid -> fillet");
    graph
        .graph_mut()
        .expect("mut")
        .set_root(fillet_node)
        .expect("set fillet root");
    graph.commit("cuboid -> fillet").expect("commit");

    // --- Spawn + project the Fillet root (NOT the Cuboid node). ----------
    let mut projection = CadProjection::new();
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    let entity = projection
        .spawn_brep_entity(&mut world, fillet_node)
        .expect("spawn");
    if let Some(mut em) = world.entity_mut(entity) {
        if let Some(mut handle) = em.get_mut::<BRepHandle>() {
            handle.brep_owner = Some(ENTITY_OWNER);
        }
    }
    projection.tick(&mut world, &graph, tol()).expect("tick");

    // --- Projected vs. renderer-side label buffers. ----------------------
    let projected = projection
        .projected_mesh(entity)
        .expect("Fillet root must have a projected mesh after tick");
    let projected_labels = projected
        .face_labels
        .as_ref()
        .expect("FilletOp inherits the Cuboid's labeled tessellation");

    let render = projection
        .render_mesh_for(entity, &world)
        .expect("must render for the projected Fillet entity");
    let render_labels = render
        .face_labels
        .as_ref()
        .expect("labeled projected mesh must yield Some(face_labels) through the adapter");

    assert_eq!(
        render_labels.len(),
        projected.triangle_count(),
        "one opaque u64 renderer label per projected Fillet triangle"
    );
    assert_eq!(
        render_labels.len(),
        projected_labels.len(),
        "renderer-side and projected label buffers must have equal length"
    );
    // Renderer label N is exactly the projected `TopologyFaceId.0` at N —
    // proves the adapter preserves order and value, not just `Some(_)`.
    for (tri, (&render_label, &topo_label)) in render_labels
        .iter()
        .zip(projected_labels.iter())
        .enumerate()
    {
        assert_eq!(
            render_label, topo_label.0,
            "triangle {tri}: renderer-side u64 label must equal the projected \
             TopologyFaceId.0 carried at the same triangle index"
        );
    }

    // --- Fillet-root resolver pairs — the same identity path the picker
    // uses internally for `brep_face_id_for_triangle`. ------------------
    let pairs: Vec<(TopologyFaceId, BRepFaceId)> =
        brep_face_ids_for_node(graph.graph(), fillet_node, ENTITY_OWNER)
            .expect("Fillet-root resolver must succeed");

    let mut saw_inherited = false;
    let mut saw_degenerate = false;
    for (tri, &render_label) in render_labels.iter().enumerate() {
        let topo = TopologyFaceId(render_label);
        if topo == TopologyFaceId::DEGENERATE {
            // FilletOp-added chamfer-cap geometry is nameless in v0; it must
            // NOT be resolved as an upstream Cuboid face. Picker-side lookup
            // returns `None` for these caps rather than a stable BRepFaceId.
            saw_degenerate = true;
            assert_eq!(
                projection.brep_face_id_for_triangle(entity, tri, &world, graph.graph()),
                None,
                "triangle {tri}: DEGENERATE chamfer-cap geometry has no stable BRepFaceId"
            );
            continue;
        }
        // Inherited non-degenerate Cuboid label: resolving it through the
        // Fillet-root graph resolver must equal the picker's answer for the
        // exact same triangle — not merely "some BRepFaceId".
        saw_inherited = true;
        let expected = pairs
            .iter()
            .find(|(t, _)| *t == topo)
            .map(|(_, id)| *id)
            .unwrap_or_else(|| {
                panic!("triangle {tri}: renderer label {topo:?} has no Fillet-root face id")
            });
        let picker_resolved = projection
            .brep_face_id_for_triangle(entity, tri, &world, graph.graph())
            .expect("picker-side resolution must succeed for an inherited triangle");
        assert_eq!(
            picker_resolved, expected,
            "triangle {tri}: renderer label {topo:?} resolved through the Fillet-root \
             resolver MUST match brep_face_id_for_triangle"
        );
    }

    assert!(
        saw_inherited,
        "at least one inherited non-degenerate renderer label must be observed"
    );
    assert!(
        saw_degenerate,
        "at least one DEGENERATE chamfer-cap label must be observed as a fixture sanity check"
    );
}
