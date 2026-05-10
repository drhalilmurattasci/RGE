//! Render-backed face-selection sub-β — `CameraView` ↔ `pick_face` ↔
//! `FaceSelection` end-to-end smoke.
//!
//! Sub-α (`brep-render`) shipped the CPU-side flat-shaded mesh-conversion
//! substrate. Sub-β (this crate's new `camera` module) ships the
//! unproject primitive that turns (screen pixel + view_proj + viewport
//! size) into the existing picker's
//! [`rge_cad_projection::picking::Ray`] shape.
//!
//! This file is the **headless integration smoke** that demonstrates
//! sub-β's place in the chapter — it composes all three substrates:
//!
//! ```text
//! caller-built view_proj                                (Mat4)
//!         │
//!         ▼
//! CameraView::screen_to_world_ray(screen_pos)            (sub-β)
//!         │
//!         ▼
//! cad_projection::picking::Ray
//!         │
//!         ▼
//! CadProjection::pick_face(&ray, &world, &graph)        (Headless face-picking sub-α)
//!         │
//!         ▼
//! FacePick { entity, owner, face_id, t, triangle_index }
//!         │
//!         ▼
//! editor_state::FaceSelection { entity, owner, face_id }
//!         │
//!         ▼
//! EditorCoord::face_selection (FaceSelectionSet)        (Editor selection persistence sub-β)
//! ```
//!
//! No GPU, no winit, no surface, no event handler. Caller-composed
//! `Mat4::look_at_rh` + `Mat4::perspective_rh` view_proj — sub-β does
//! NOT prescribe view math.
//!
//! Hard non-goals (NOT exercised here):
//!
//! - No mouse-event wiring (sub-δ).
//! - No camera state tracking over time (sub-γ).
//! - No view/projection matrix construction by the substrate (caller composes).
//! - No GPU readback picking, no edge picking, no Fillet output identity.

use glam::{Mat4, Vec3};
use rge_cad_core::{
    BRepFaceId, BRepOwnerId, CadGraph, CuboidFaceTag, CuboidOp, OperatorNode, Tolerance,
};
use rge_cad_projection::{BRepHandle, CadProjection};
use rge_editor_shell::camera::CameraView;
use rge_editor_shell::coord::{EditorCoord, FaceSelection};
use rge_kernel_ecs::World;

const ENTITY_OWNER: BRepOwnerId = BRepOwnerId::from_bytes([0x42; 16]);

fn tol() -> Tolerance {
    Tolerance::new(0.001).expect("tolerance")
}

/// Build a `(graph, projection, world, entity)` tuple with a single 1×1×1
/// Cuboid committed and projected at origin under [`ENTITY_OWNER`].
///
/// Mirrors the pattern in `crates/cad-projection/tests/face_picking_smoke.rs`
/// (the Headless face-picking sub-α integration tests).
fn build_unit_cuboid() -> (CadGraph, CadProjection, World, rge_kernel_ecs::EntityId) {
    let mut graph = CadGraph::new();
    graph.begin_operation().expect("begin");
    let cuboid_node = graph
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Cuboid(CuboidOp {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
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

    (graph, projection, world, entity)
}

/// Construct a wgpu-convention view * projection for an editor camera at
/// `(0, 0, 5)` looking at the origin, 800×600 viewport, 45° FOV.
fn editor_camera_view_proj(viewport: [f32; 2]) -> Mat4 {
    let view = Mat4::look_at_rh(
        Vec3::new(0.0, 0.0, 5.0),
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(0.0, 1.0, 0.0),
    );
    let aspect = viewport[0] / viewport[1];
    let proj = Mat4::perspective_rh(std::f32::consts::FRAC_PI_4, aspect, 0.1, 100.0);
    proj * view
}

/// **Test 1** — center pixel of the viewport, camera looking down at
/// origin → ray hits the cuboid's +Z (top) face. Demonstrates the full
/// `CameraView → pick_face` chain.
#[test]
fn camera_view_screen_center_picks_cuboid_top_face() {
    let (graph, projection, world, entity) = build_unit_cuboid();
    let viewport = [800.0_f32, 600.0_f32];
    let cam = CameraView {
        view_proj: editor_camera_view_proj(viewport),
        viewport_size: viewport,
    };

    let ray = cam
        .screen_to_world_ray([viewport[0] / 2.0, viewport[1] / 2.0])
        .expect("non-degenerate camera must yield a ray");

    let pick = projection
        .pick_face(&ray, &world, graph.graph())
        .expect("center-screen ray under a camera at z=+5 must hit the cuboid +Z face");

    assert_eq!(pick.entity, entity, "the unit cuboid entity must be picked");
    assert_eq!(pick.owner, ENTITY_OWNER);
    assert_eq!(
        pick.face_id,
        BRepFaceId::for_cuboid_face(ENTITY_OWNER, CuboidFaceTag::PosZ),
        "the picked face_id must be the +Z (top) face"
    );
}

/// **Test 2** — far-off-axis screen pixel (top-left corner-ish) under
/// the same camera → ray points off into the corner of the view frustum
/// and misses the small centered cuboid.
#[test]
fn camera_view_off_axis_screen_pos_misses_centered_cuboid() {
    let (graph, projection, world, _entity) = build_unit_cuboid();
    let viewport = [800.0_f32, 600.0_f32];
    let cam = CameraView {
        view_proj: editor_camera_view_proj(viewport),
        viewport_size: viewport,
    };

    // Top-left corner-ish pixel — toward NDC (-1, +1) which under a 45°
    // FOV camera at z=+5 points well outside the [-0.5, 0.5]^3 cuboid's
    // silhouette.
    let ray = cam.screen_to_world_ray([50.0, 50.0]).expect("invertible");

    let pick = projection.pick_face(&ray, &world, graph.graph());
    assert!(
        pick.is_none(),
        "off-axis ray must miss the centered cuboid; got {pick:?}"
    );
}

/// **Test 3** — full chain: `CameraView::screen_to_world_ray` →
/// `pick_face` → `FaceSelection` → `EditorCoord.face_selection`. Proves
/// sub-β's API composes through to the editor coordination state with
/// no impedance mismatch.
#[test]
fn camera_view_composes_into_face_selection_set() {
    let (graph, projection, world, entity) = build_unit_cuboid();
    let viewport = [800.0_f32, 600.0_f32];
    let cam = CameraView {
        view_proj: editor_camera_view_proj(viewport),
        viewport_size: viewport,
    };

    let ray = cam
        .screen_to_world_ray([viewport[0] / 2.0, viewport[1] / 2.0])
        .expect("invertible");
    let pick = projection
        .pick_face(&ray, &world, graph.graph())
        .expect("hit");

    // Sub-β returns a Ray; sub-α's picker returns a FacePick; the editor
    // composes those three identifying fields into a FaceSelection.
    let selection = FaceSelection {
        entity: pick.entity,
        owner: pick.owner,
        face_id: pick.face_id,
    };

    let mut coord = EditorCoord::default();
    let newly_added = coord.face_selection.add(selection);
    assert!(newly_added, "fresh add must report newly-added");
    assert!(
        coord.face_selection.contains(&selection),
        "EditorCoord.face_selection must contain the just-added FaceSelection"
    );
    assert_eq!(coord.face_selection.len(), 1);
    assert_eq!(selection.entity, entity);
    assert_eq!(selection.owner, ENTITY_OWNER);
    assert_eq!(
        selection.face_id,
        BRepFaceId::for_cuboid_face(ENTITY_OWNER, CuboidFaceTag::PosZ),
    );
}
