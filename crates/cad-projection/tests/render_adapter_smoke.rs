// SPLIT-EXEMPTION: cohesive render-adapter smoke aggregation — every test in
// this file exercises the one adapter chain `CadProjection::render_mesh_for`
// (ProjectedMesh `TopologyFaceId` -> `RenderMesh` opaque `u64` labels) and
// shares the same fixtures, imports, and chain-consistency invariant across
// the Cuboid, Extrude, RoundFillet, Fillet, Transform, Sweep, Loft, and
// Revolve operator cases. The GitHub issues driving this coverage (incl. #36)
// explicitly require each per-operator smoke to live in this single file and
// forbid adding another integration test file, so splitting is not an option.

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
//! * **TransformOp renderer-label alignment (GitHub issue #33)**: a
//!   `CuboidOp -> TransformOp` root preserves inherited non-degenerate
//!   Cuboid labels into `RenderMesh.face_labels` in projection-lookup
//!   triangle order. `TransformOp` is topology-preserving, so no
//!   `TopologyFaceId::DEGENERATE` label is expected.
//! * **SweepOp renderer-label alignment (GitHub issue #34)**: a
//!   `SweepOp`-root entity (square profile, 3-point monotonic-Z path)
//!   preserves the projected `TopologyFaceId` labels into
//!   `RenderMesh.face_labels` in projection-lookup triangle order.
//!   `SweepOp` mints stable cap/side face labels, so every renderer
//!   label is non-degenerate and resolves to the same `BRepFaceId` as
//!   picker-side lookup.
//! * **LoftOp renderer-label alignment (GitHub issue #35)**: a
//!   `LoftOp`-root entity (square profile → larger-square profile)
//!   preserves the projected `TopologyFaceId` labels into
//!   `RenderMesh.face_labels` in projection-lookup triangle order.
//!   `LoftOp` mints stable bottom-cap, top-cap, and side face labels,
//!   so every renderer label is non-degenerate and resolves through the
//!   Loft-root graph resolver to the same `BRepFaceId` as picker-side
//!   lookup. Distinct from `loft_brep_face_id_lookup_smoke.rs`, which
//!   covers picker-side lookup only.
//! * **RevolveOp renderer-label alignment (GitHub issue #36)**: a
//!   `RevolveOp`-root entity (Partial-mode square ring profile, 8
//!   segments, angle = π) preserves the projected `TopologyFaceId`
//!   labels into `RenderMesh.face_labels` in projection-lookup triangle
//!   order. Partial-mode `RevolveOp` mints stable side-face and
//!   start/end cap face labels, so every renderer label is non-degenerate
//!   and resolves through the Revolve-root graph resolver to the same
//!   `BRepFaceId` as picker-side lookup. Distinct from
//!   `revolve_brep_face_id_lookup_smoke.rs`, which covers picker-side
//!   lookup only.
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
//! * `transform_render_mesh_face_labels_align_with_projection_lookup`
//!   — TransformOp renderer-label alignment (GitHub issue #33).
//! * `sweep_render_mesh_face_labels_align_with_projection_lookup`
//!   — SweepOp renderer-label alignment (GitHub issue #34).
//! * `loft_render_mesh_face_labels_align_with_projection_lookup`
//!   — LoftOp renderer-label alignment (GitHub issue #35).
//! * `revolve_render_mesh_face_labels_align_with_projection_lookup`
//!   — RevolveOp renderer-label alignment (GitHub issue #36).

use std::f32::consts::PI;

use rge_cad_core::{
    brep_face_ids_for_node, BRepEdgeProvider, BRepFaceId, BRepOwnerId, BRepProvider, CadGraph,
    CuboidOp, ExtrudeOp, FilletOp, LoftOp, OperatorNode, Polygon2D, Polyline3D, RevolveOp,
    RoundFilletOp, SweepOp, Tolerance, TopologyFaceId, TransformOp,
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

/// **Test 6 — TransformOp renderer-label alignment for GitHub issue #33.**
///
/// Build a `CuboidOp -> TransformOp` graph with a non-identity Transform
/// node as root, project the Transform root entity, and prove
/// `CadProjection::render_mesh_for` preserves the projected
/// `TopologyFaceId` labels into the renderer-side opaque `u64`
/// `RenderMesh.face_labels` buffer in the SAME triangle order used by
/// projection lookup:
///
/// 1. Every renderer-side `u64` equals the projected mesh's
///    `TopologyFaceId.0` carried at the same triangle index — the adapter
///    must not drop, reorder, or mis-convert labels.
/// 2. `TransformOp` is topology-preserving — it transforms vertex
///    positions, clones triangle indices unchanged, and passes upstream
///    `face_labels` through one-for-one — so every renderer label is an
///    inherited non-degenerate Cuboid label. No `TopologyFaceId::DEGENERATE`
///    entry is expected for this Cuboid-through-Transform fixture.
/// 3. Every renderer label resolves — through the Transform-root graph
///    resolver (`brep_face_ids_for_node` for `transform_node`) — to the
///    upstream Cuboid `BRepFaceId` minted under `ENTITY_OWNER`, and that
///    identity equals the `BRepFaceId` returned by
///    `brep_face_id_for_triangle` for the exact same triangle.
///
/// This smoke is distinct from the Cuboid, Extrude, RoundFillet, and Fillet
/// render-adapter smokes above — it specifically exercises
/// `OperatorNode::Transform` and binds the projected entity to the
/// Transform root, not the upstream Cuboid node. It is also distinct from
/// `transform_brep_face_lookup_smoke.rs`, which covers picker-side lookup
/// only; this smoke covers the renderer adapter's opaque
/// `RenderMesh.face_labels` buffer.
#[test]
fn transform_render_mesh_face_labels_align_with_projection_lookup() {
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

    // --- Spawn + project the Transform root (NOT the Cuboid node). -------
    let mut projection = CadProjection::new();
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    let entity = projection
        .spawn_brep_entity(&mut world, transform_node)
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
        .expect("Transform root must have a projected mesh after tick");
    let projected_labels = projected
        .face_labels
        .as_ref()
        .expect("TransformOp inherits the Cuboid's labeled tessellation");

    let render = projection
        .render_mesh_for(entity, &world)
        .expect("must render for the projected Transform entity");
    let render_labels = render
        .face_labels
        .as_ref()
        .expect("labeled projected mesh must yield Some(face_labels) through the adapter");

    assert_eq!(
        render_labels.len(),
        projected.triangle_count(),
        "one opaque u64 renderer label per projected Transform triangle"
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

    // --- Transform-root resolver pairs — the same identity path the picker
    // uses internally for `brep_face_id_for_triangle`. -------------------
    let pairs: Vec<(TopologyFaceId, BRepFaceId)> =
        brep_face_ids_for_node(graph.graph(), transform_node, ENTITY_OWNER)
            .expect("Transform-root resolver must succeed");

    // Upstream Cuboid face mapping, minted under the SAME owner — the
    // identities the topology-preserving Transform root must inherit
    // unchanged.
    let direct_pairs: Vec<(TopologyFaceId, BRepFaceId)> = cuboid.brep_face_ids(ENTITY_OWNER);
    assert_eq!(direct_pairs.len(), 6, "Cuboid emits exactly 6 face IDs");

    let mut saw_inherited = false;
    for (tri, &render_label) in render_labels.iter().enumerate() {
        let topo = TopologyFaceId(render_label);
        // TransformOp adds no nameless cap or corner geometry, so every
        // renderer label is a non-degenerate inherited Cuboid label.
        assert_ne!(
            topo,
            TopologyFaceId::DEGENERATE,
            "triangle {tri}: TransformOp preserves topology — no DEGENERATE \
             renderer label is expected for the Cuboid-through-Transform fixture"
        );
        saw_inherited = true;
        // The label must be one of the upstream Cuboid's 6 face identities,
        // minted under `ENTITY_OWNER`.
        let upstream = direct_pairs
            .iter()
            .find(|(t, _)| *t == topo)
            .map(|(_, id)| *id)
            .unwrap_or_else(|| {
                panic!("triangle {tri}: renderer label {topo:?} has no upstream Cuboid face id")
            });
        // Resolving the label through the Transform-root graph resolver must
        // yield the SAME upstream Cuboid identity — Transform inherits, it
        // does not re-mint.
        let resolved = pairs
            .iter()
            .find(|(t, _)| *t == topo)
            .map(|(_, id)| *id)
            .unwrap_or_else(|| {
                panic!("triangle {tri}: renderer label {topo:?} has no Transform-root face id")
            });
        assert_eq!(
            resolved, upstream,
            "triangle {tri}: Transform-root resolver MUST inherit the upstream \
             Cuboid BRepFaceId for renderer label {topo:?}"
        );
        // ... and it must match the picker's answer for the same triangle.
        let picker_resolved = projection
            .brep_face_id_for_triangle(entity, tri, &world, graph.graph())
            .expect("picker-side resolution must succeed for an inherited triangle");
        assert_eq!(
            picker_resolved, resolved,
            "triangle {tri}: renderer label {topo:?} resolved through the \
             Transform-root resolver MUST match brep_face_id_for_triangle"
        );
    }

    assert!(
        saw_inherited,
        "every renderer label must be an inherited non-degenerate Cuboid label"
    );
}

/// **Test 7 — SweepOp renderer-label alignment for GitHub issue #34.**
///
/// Build a graph whose root is a `SweepOp` node — the canonical
/// square-profile, 3-point monotonic-Z fixture from
/// `sweep_brep_face_id_lookup_smoke.rs` — project the Sweep root entity,
/// and prove `CadProjection::render_mesh_for` preserves the projected
/// `TopologyFaceId` labels into the renderer-side opaque `u64`
/// `RenderMesh.face_labels` buffer in the SAME triangle order used by
/// projection lookup:
///
/// 1. `RenderMesh.face_labels` is `Some`, with exactly one opaque `u64`
///    per projected Sweep triangle (20 for this fixture), and every
///    renderer-side `u64` equals the projected mesh's `TopologyFaceId.0`
///    carried at the same triangle index — the adapter must not drop,
///    reorder, or mis-convert labels.
/// 2. `SweepOp` mints stable face labels for both caps and side faces,
///    so every renderer label is non-degenerate. Each label resolves —
///    through the Sweep-root graph resolver (`brep_face_ids_for_node`
///    for `sweep_node`) — to the exact `BRepFaceId` returned by
///    `brep_face_id_for_triangle` for that same triangle.
///
/// This smoke is distinct from the Cuboid, Extrude, RoundFillet, Fillet,
/// and Transform render-adapter smokes above — it specifically exercises
/// `OperatorNode::Sweep` and binds the projected entity to the Sweep
/// root. It is also distinct from `sweep_brep_face_id_lookup_smoke.rs`,
/// which covers picker-side lookup only; this smoke covers the renderer
/// adapter's opaque `RenderMesh.face_labels` buffer.
#[test]
fn sweep_render_mesh_face_labels_align_with_projection_lookup() {
    // --- Build a SweepOp root: square profile, 3-point monotonic-Z path. --
    // The canonical fixture mirrored from sweep_brep_face_id_lookup_smoke.rs:
    // a unit-square profile (n = 4, CCW) swept along a +Z path (m = 3).
    let profile =
        Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]).expect("square");
    let path = Polyline3D::new(vec![[0.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, 2.0]])
        .expect("z-axis path");
    let sweep = SweepOp::new(profile, path);

    let mut graph = CadGraph::new();
    graph.begin_operation().expect("begin");
    let sweep_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Sweep(sweep))
        .expect("add sweep");
    graph
        .graph_mut()
        .expect("mut")
        .set_root(sweep_node)
        .expect("set sweep root");
    graph.commit("sweep").expect("commit");

    // --- Spawn + project the Sweep root. ---------------------------------
    let mut projection = CadProjection::new();
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    let entity = projection
        .spawn_brep_entity(&mut world, sweep_node)
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
        .expect("Sweep root must have a projected mesh after tick");
    // n=4, m=3 → 2 * n * (m - 1) + 2 * (n - 2) = 2*4*2 + 2*2 = 20 triangles.
    assert_eq!(
        projected.triangle_count(),
        20,
        "square-profile, 3-point monotonic-Z Sweep projects to exactly 20 triangles"
    );
    let projected_labels = projected
        .face_labels
        .as_ref()
        .expect("SweepOp emits a labeled tessellation for caps and side faces");

    let render = projection
        .render_mesh_for(entity, &world)
        .expect("must render for the projected Sweep entity");
    let render_labels = render
        .face_labels
        .as_ref()
        .expect("labeled projected mesh must yield Some(face_labels) through the adapter");

    assert_eq!(
        render_labels.len(),
        20,
        "the square-profile, monotonic-Z Sweep fixture yields exactly 20 renderer labels"
    );
    assert_eq!(
        render_labels.len(),
        projected.triangle_count(),
        "one opaque u64 renderer label per projected Sweep triangle"
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

    // --- Sweep-root resolver pairs — the same identity path the picker
    // uses internally for `brep_face_id_for_triangle`. -------------------
    let pairs: Vec<(TopologyFaceId, BRepFaceId)> =
        brep_face_ids_for_node(graph.graph(), sweep_node, ENTITY_OWNER)
            .expect("Sweep-root resolver must succeed");

    let mut saw_label = false;
    for (tri, &render_label) in render_labels.iter().enumerate() {
        let topo = TopologyFaceId(render_label);
        // SweepOp mints stable face labels for caps and side faces, so no
        // DEGENERATE renderer label is expected for this valid fixture.
        assert_ne!(
            topo,
            TopologyFaceId::DEGENERATE,
            "triangle {tri}: SweepOp emits stable cap/side face labels — no \
             DEGENERATE renderer label is expected for the square-profile, \
             monotonic-Z Sweep fixture"
        );
        saw_label = true;
        // Resolve the renderer label through the Sweep-root graph resolver.
        let resolved = pairs
            .iter()
            .find(|(t, _)| *t == topo)
            .map(|(_, id)| *id)
            .unwrap_or_else(|| {
                panic!("triangle {tri}: renderer label {topo:?} has no Sweep-root face id")
            });
        // ... and it must match the picker's answer for the exact same
        // triangle — not merely "some BRepFaceId".
        let picker_resolved = projection
            .brep_face_id_for_triangle(entity, tri, &world, graph.graph())
            .expect("picker-side resolution must succeed for a Sweep triangle");
        assert_eq!(
            picker_resolved, resolved,
            "triangle {tri}: renderer label {topo:?} resolved through the \
             Sweep-root resolver MUST match brep_face_id_for_triangle"
        );
    }

    assert!(
        saw_label,
        "every renderer label must be a non-degenerate Sweep face label"
    );
}

/// **Test 8 — LoftOp renderer-label alignment for GitHub issue #35.**
///
/// Build a graph whose root is a `LoftOp` node — the canonical
/// square-profile → larger-square-profile fixture mirrored from
/// `loft_brep_face_id_lookup_smoke.rs` — project the Loft root entity,
/// and prove `CadProjection::render_mesh_for` preserves the projected
/// `TopologyFaceId` labels into the renderer-side opaque `u64`
/// `RenderMesh.face_labels` buffer in the SAME triangle order used by
/// projection lookup:
///
/// 1. `RenderMesh.face_labels` is `Some`, with exactly one opaque `u64`
///    per projected Loft triangle (12 for this n=4 fixture), and every
///    renderer-side `u64` equals the projected mesh's `TopologyFaceId.0`
///    carried at the same triangle index — the adapter must not drop,
///    reorder, or mis-convert labels.
/// 2. `LoftOp` mints stable face labels for the bottom cap, top cap, and
///    side faces, so every renderer label is non-degenerate. Each label
///    resolves — through the Loft-root graph resolver
///    (`brep_face_ids_for_node` for `loft_node`) — to the exact
///    `BRepFaceId` returned by `brep_face_id_for_triangle` for that same
///    triangle.
///
/// This smoke is distinct from the Cuboid, Extrude, RoundFillet, Fillet,
/// Transform, and Sweep render-adapter smokes above — it specifically
/// exercises `OperatorNode::Loft` and binds the projected entity to the
/// Loft root. It is also distinct from
/// `loft_brep_face_id_lookup_smoke.rs`, which covers picker-side lookup
/// only; this smoke covers the renderer adapter's opaque
/// `RenderMesh.face_labels` buffer.
#[test]
fn loft_render_mesh_face_labels_align_with_projection_lookup() {
    // --- Build a LoftOp root: square profile → larger-square profile. -----
    // The canonical fixture mirrored from loft_brep_face_id_lookup_smoke.rs:
    // a unit-square profile (n = 4, CCW) lofted to a larger square profile.
    let profile_a =
        Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]).expect("square");
    let profile_b = Polygon2D::new(vec![[0.0, 0.0], [3.0, 0.0], [3.0, 3.0], [0.0, 3.0]])
        .expect("larger square");
    let loft = LoftOp::new(profile_a, profile_b, 1.0).expect("loft construction");

    let mut graph = CadGraph::new();
    graph.begin_operation().expect("begin");
    let loft_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Loft(loft))
        .expect("add loft");
    graph
        .graph_mut()
        .expect("mut")
        .set_root(loft_node)
        .expect("set loft root");
    graph.commit("loft").expect("commit");

    // --- Spawn + project the Loft root. ----------------------------------
    let mut projection = CadProjection::new();
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    let entity = projection
        .spawn_brep_entity(&mut world, loft_node)
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
        .expect("Loft root must have a projected mesh after tick");
    // n=4 ⇒ 4n - 4 = 12 triangles (bottom 2 + top 2 + sides 8).
    assert_eq!(
        projected.triangle_count(),
        12,
        "square-profile → larger-square-profile Loft projects to exactly 12 triangles"
    );
    let projected_labels = projected
        .face_labels
        .as_ref()
        .expect("LoftOp emits a labeled tessellation for caps and side faces");

    let render = projection
        .render_mesh_for(entity, &world)
        .expect("must render for the projected Loft entity");
    let render_labels = render
        .face_labels
        .as_ref()
        .expect("labeled projected mesh must yield Some(face_labels) through the adapter");

    assert_eq!(
        render_labels.len(),
        12,
        "the square-profile → larger-square-profile Loft fixture yields exactly 12 renderer labels"
    );
    assert_eq!(
        render_labels.len(),
        projected.triangle_count(),
        "one opaque u64 renderer label per projected Loft triangle"
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

    // --- Loft-root resolver pairs — the same identity path the picker
    // uses internally for `brep_face_id_for_triangle`. -------------------
    let pairs: Vec<(TopologyFaceId, BRepFaceId)> =
        brep_face_ids_for_node(graph.graph(), loft_node, ENTITY_OWNER)
            .expect("Loft-root resolver must succeed");

    let mut saw_label = false;
    for (tri, &render_label) in render_labels.iter().enumerate() {
        let topo = TopologyFaceId(render_label);
        // LoftOp mints stable face labels for the bottom cap, top cap, and
        // side faces, so no DEGENERATE renderer label is expected for this
        // valid square-profile Loft fixture.
        assert_ne!(
            topo,
            TopologyFaceId::DEGENERATE,
            "triangle {tri}: LoftOp emits stable bottom-cap, top-cap, and \
             side face labels — no DEGENERATE renderer label is expected for \
             the square-profile → larger-square-profile Loft fixture"
        );
        saw_label = true;
        // Resolve the renderer label through the Loft-root graph resolver.
        let resolved = pairs
            .iter()
            .find(|(t, _)| *t == topo)
            .map(|(_, id)| *id)
            .unwrap_or_else(|| {
                panic!("triangle {tri}: renderer label {topo:?} has no Loft-root face id")
            });
        // ... and it must match the picker's answer for the exact same
        // triangle — not merely "some BRepFaceId".
        let picker_resolved = projection
            .brep_face_id_for_triangle(entity, tri, &world, graph.graph())
            .expect("picker-side resolution must succeed for a Loft triangle");
        assert_eq!(
            picker_resolved, resolved,
            "triangle {tri}: renderer label {topo:?} resolved through the \
             Loft-root resolver MUST match brep_face_id_for_triangle"
        );
    }

    assert!(
        saw_label,
        "every renderer label must be a non-degenerate Loft face label"
    );
}

/// **Test 9 — RevolveOp renderer-label alignment for GitHub issue #36.**
///
/// Build a graph whose root is a `RevolveOp` node — the canonical
/// Partial-mode square ring profile from
/// `revolve_brep_face_id_lookup_smoke.rs` (n = 4, segments = 8,
/// angle = π) — project the Revolve root entity, and prove
/// `CadProjection::render_mesh_for` preserves the projected
/// `TopologyFaceId` labels into the renderer-side opaque `u64`
/// `RenderMesh.face_labels` buffer in the SAME triangle order used by
/// projection lookup:
///
/// 1. `RenderMesh.face_labels` is `Some`, with exactly one opaque `u64`
///    per projected Revolve triangle (68 for this Partial-mode fixture:
///    `2 * n * segments + 2 * (n - 2) = 64 + 4`), and every renderer-side
///    `u64` equals the projected mesh's `TopologyFaceId.0` carried at the
///    same triangle index — the adapter must not drop, reorder, or
///    mis-convert labels.
/// 2. Partial-mode `RevolveOp` mints stable face labels for the `n` side
///    faces and the start/end caps, so every renderer label is
///    non-degenerate. Each label resolves — through the Revolve-root
///    graph resolver (`brep_face_ids_for_node` for `revolve_node`) — to
///    the exact `BRepFaceId` returned by `brep_face_id_for_triangle` for
///    that same triangle.
///
/// This smoke is distinct from the Cuboid, Extrude, RoundFillet, Fillet,
/// Transform, Sweep, and Loft render-adapter smokes above — it
/// specifically exercises `OperatorNode::Revolve` and binds the projected
/// entity to the Revolve root. It is also distinct from
/// `revolve_brep_face_id_lookup_smoke.rs`, which covers picker-side
/// lookup only; this smoke covers the renderer adapter's opaque
/// `RenderMesh.face_labels` buffer.
#[test]
fn revolve_render_mesh_face_labels_align_with_projection_lookup() {
    // --- Build a RevolveOp root: Partial-mode square ring profile. --------
    // The canonical fixture mirrored from revolve_brep_face_id_lookup_smoke.rs:
    // a square ring profile (n = 4) revolved Partial-mode (8 segments, π).
    let profile =
        Polygon2D::new(vec![[1.0, 0.0], [2.0, 0.0], [2.0, 1.0], [1.0, 1.0]]).expect("ring");
    let revolve = RevolveOp::partial(profile, 8, PI).expect("revolve partial");

    let mut graph = CadGraph::new();
    graph.begin_operation().expect("begin");
    let revolve_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Revolve(revolve))
        .expect("add revolve");
    graph
        .graph_mut()
        .expect("mut")
        .set_root(revolve_node)
        .expect("set revolve root");
    graph.commit("revolve").expect("commit");

    // --- Spawn + project the Revolve root. -------------------------------
    let mut projection = CadProjection::new();
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    let entity = projection
        .spawn_brep_entity(&mut world, revolve_node)
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
        .expect("Revolve root must have a projected mesh after tick");
    // Partial: 2*n*segments + 2*(n-2) = 2*4*8 + 2*2 = 64 + 4 = 68 triangles.
    assert_eq!(
        projected.triangle_count(),
        68,
        "Partial-mode square-ring Revolve (8 segments, π) projects to exactly 68 triangles"
    );
    let projected_labels = projected
        .face_labels
        .as_ref()
        .expect("RevolveOp emits a labeled tessellation for side faces and caps");

    let render = projection
        .render_mesh_for(entity, &world)
        .expect("must render for the projected Revolve entity");
    let render_labels = render
        .face_labels
        .as_ref()
        .expect("labeled projected mesh must yield Some(face_labels) through the adapter");

    assert_eq!(
        render_labels.len(),
        68,
        "the Partial-mode square-ring Revolve fixture yields exactly 68 renderer labels"
    );
    assert_eq!(
        render_labels.len(),
        projected.triangle_count(),
        "one opaque u64 renderer label per projected Revolve triangle"
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

    // --- Revolve-root resolver pairs — the same identity path the picker
    // uses internally for `brep_face_id_for_triangle`. -------------------
    let pairs: Vec<(TopologyFaceId, BRepFaceId)> =
        brep_face_ids_for_node(graph.graph(), revolve_node, ENTITY_OWNER)
            .expect("Revolve-root resolver must succeed");

    let mut saw_label = false;
    for (tri, &render_label) in render_labels.iter().enumerate() {
        let topo = TopologyFaceId(render_label);
        // Partial-mode RevolveOp mints stable side-face and start/end cap
        // labels, so no DEGENERATE renderer label is expected for this
        // valid square-ring Revolve fixture.
        assert_ne!(
            topo,
            TopologyFaceId::DEGENERATE,
            "triangle {tri}: Partial-mode RevolveOp emits stable side-face and \
             start/end cap labels — no DEGENERATE renderer label is expected \
             for the square-ring Revolve fixture"
        );
        saw_label = true;
        // Resolve the renderer label through the Revolve-root graph resolver.
        let resolved = pairs
            .iter()
            .find(|(t, _)| *t == topo)
            .map(|(_, id)| *id)
            .unwrap_or_else(|| {
                panic!("triangle {tri}: renderer label {topo:?} has no Revolve-root face id")
            });
        // ... and it must match the picker's answer for the exact same
        // triangle — not merely "some BRepFaceId".
        let picker_resolved = projection
            .brep_face_id_for_triangle(entity, tri, &world, graph.graph())
            .expect("picker-side resolution must succeed for a Revolve triangle");
        assert_eq!(
            picker_resolved, resolved,
            "triangle {tri}: renderer label {topo:?} resolved through the \
             Revolve-root resolver MUST match brep_face_id_for_triangle"
        );
    }

    assert!(
        saw_label,
        "every renderer label must be a non-degenerate Revolve face label"
    );
}
