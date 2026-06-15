// SPLIT-EXEMPTION: cohesive lifecycle test module. Holds the inline
// PIE state-machine / snapshot / time-scale / audit-ledger / glTF
// render-construction tests + the asset-hot-reload (R-key) substrate
// tests added 2026-05-22. Every test exercises EditorShell-level
// invariants (state-machine + cross-field consistency) so they
// belong together in one cohesive file; splitting would scatter the
// "shell-level test posture" across siblings for no cognitive gain.
// A future trim could split out the AABB / camera-framing pure
// math tests into a `lifecycle/geom_tests.rs` sibling if the file
// keeps growing — pre-emptive extraction is cosmetic today.

//! Inline `lifecycle` tests — extracted from `lifecycle/mod.rs` foot.
//!
//! Pure mechanical extraction; test bodies are byte-identical to the
//! pre-extraction `#[cfg(test)] mod tests { ... }` block, with imports
//! rewritten to use explicit module paths rather than `use super::*;`
//! (the file-level move makes `super` resolve to the `lifecycle` module
//! itself, so the same set of names remains in scope).
//!
//! All tests here exercise only the public `EditorShell` API:
//! - PIE state-machine transitions (Play / Pause / Stop / Step).
//! - Snapshot capture / restore round-trip.
//! - Game-system tick advancement gating by `PlayState`.
//! - `TimeScale` interaction with the per-tick `dt`.
//! - `AuditLedger` event recording.
//!
//! No `pub(crate)` promotions were required for the extraction; the
//! tests were already touching only public API surface.

use std::time::{Duration, Instant};

use rge_cad_core::{BRepFaceId, BRepOwnerId, CuboidFaceTag};
use winit::dpi::PhysicalPosition;
use winit::event::MouseScrollDelta;

use super::window_title::editor_window_title;
use super::{
    EditorShell, SaveSource, UnsavedChangesContext, UnsavedChangesDecision, UnsavedChangesDialog,
    UnsavedChangesRequest, UnsavedChangesSourceKind, ViewportCursorGrabTestEvent,
};
use crate::audit::AuditEvent;
use crate::coord::FaceSelection;
use crate::play_state::{PlayState, PlayStateTransition};
use crate::play_toolbar::ToolbarButtonId;
use crate::time_scale::TimeScale;
use crate::world::ComponentTypeId;

fn build_scene(shell: &mut EditorShell, n: usize) {
    for i in 0..n {
        let e = shell.world_mut().spawn();
        shell.world_mut().insert_component(
            e,
            ComponentTypeId(1),
            (i as u64).to_le_bytes().to_vec(),
        );
        shell
            .world_mut()
            .insert_component(e, ComponentTypeId(2), vec![0u8; 12]);
    }
}

#[derive(Debug, PartialEq)]
struct UnsavedEditorSnapshot {
    wrapper_entity_count: usize,
    kernel_entity_count: usize,
    save_source: Option<SaveSource>,
    selection: Vec<crate::world::EntityId>,
    face_selection: Vec<FaceSelection>,
    has_clipboard_entities: bool,
    is_dirty: bool,
    quit_requested: bool,
    time_scale: f32,
}

struct MockUnsavedChangesDialog {
    decision: UnsavedChangesDecision,
    calls: std::rc::Rc<std::cell::RefCell<Vec<UnsavedChangesContext>>>,
}

impl UnsavedChangesDialog for MockUnsavedChangesDialog {
    fn confirm_discard_unsaved_changes(
        &self,
        context: &UnsavedChangesContext,
    ) -> UnsavedChangesDecision {
        self.calls.borrow_mut().push(context.clone());
        self.decision
    }
}

fn mock_unsaved_changes_dialog(
    decision: UnsavedChangesDecision,
) -> (
    Box<dyn UnsavedChangesDialog>,
    std::rc::Rc<std::cell::RefCell<Vec<UnsavedChangesContext>>>,
) {
    let calls = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    (
        Box::new(MockUnsavedChangesDialog {
            decision,
            calls: std::rc::Rc::clone(&calls),
        }),
        calls,
    )
}

fn unsaved_seed_clean_shell() -> EditorShell {
    let mut shell = EditorShell::new().with_save_source(SaveSource::Scene(
        std::path::PathBuf::from("/tmp/unsaved-clean.rge-scene"),
    ));
    build_scene(&mut shell, 2);
    let selected = shell
        .world()
        .entities()
        .next()
        .expect("seeded world has entity");
    shell.coord_mut().selection.add(selected);
    assert_eq!(
        shell.copy_selected_entities(),
        1,
        "precondition: seeded clean shell has clipboard state"
    );
    assert!(
        !shell.command_bus().is_dirty(),
        "precondition: direct scene setup does not dirty the bus"
    );
    shell
}

fn unsaved_seed_dirty_shell() -> EditorShell {
    let mut shell = EditorShell::new().with_save_source(SaveSource::Scene(
        std::path::PathBuf::from("/tmp/unsaved-dirty.rge-scene"),
    ));
    build_scene(&mut shell, 3);
    let ids: Vec<_> = shell.world().entities().collect();
    let owner = BRepOwnerId::from_bytes([0x36; 16]);
    let selected_face = FaceSelection {
        entity: ids[0],
        owner,
        face_id: BRepFaceId::for_cuboid_face(owner, CuboidFaceTag::PosX),
    };
    shell.coord_mut().selection.add(ids[0]);
    shell.coord_mut().face_selection.add(selected_face);
    assert_eq!(
        shell.copy_selected_entities(),
        1,
        "precondition: seeded dirty shell has clipboard state"
    );
    shell.set_time_scale(2.0);
    assert!(
        shell.command_bus().is_dirty(),
        "precondition: seeded shell is dirty"
    );
    assert!(
        !shell.quit_requested,
        "precondition: no pending quit request"
    );
    shell
}

fn unsaved_snapshot(shell: &EditorShell) -> UnsavedEditorSnapshot {
    UnsavedEditorSnapshot {
        wrapper_entity_count: shell.world().entity_count(),
        kernel_entity_count: shell.world().kernel().entity_count(),
        save_source: shell.save_source().cloned(),
        selection: shell.coord().selection.iter().collect(),
        face_selection: shell.coord().face_selection.iter().copied().collect(),
        has_clipboard_entities: shell.predicate_context().has_clipboard_entities,
        is_dirty: shell.command_bus().is_dirty(),
        quit_requested: shell.quit_requested,
        time_scale: shell.time_scale().value(),
    }
}

fn assert_unsaved_context(
    context: &UnsavedChangesContext,
    request: UnsavedChangesRequest,
    source_kind: UnsavedChangesSourceKind,
    display_name: &str,
) {
    assert_eq!(context.request(), request);
    assert_eq!(context.source_kind(), source_kind);
    assert_eq!(context.source_display_name(), Some(display_name));
    assert!(context.source_path().is_some());
}

#[test]
fn fresh_shell_is_editing() {
    let s = EditorShell::new();
    assert_eq!(s.play_state(), PlayState::Editing);
    assert!(!s.has_snapshot());
    assert_eq!(s.tick_count(), 0);
}

#[test]
fn predicate_context_tracks_play_state() {
    // The live PredicateContext mirrors the canonical PlayState::can_* for the
    // shell's current state (the host re-resolves the menu + greys items from it).
    // Each state has a distinct enablement pattern, pinning the 1:1 mapping; the
    // Document-mutating File items gate on `is_editing`.
    let mut s = EditorShell::new();

    // Editing: only Play (start) is valid; document File items enabled.
    let ctx = s.predicate_context();
    assert!(ctx.can_play);
    assert!(!ctx.can_pause);
    assert!(!ctx.can_stop);
    assert!(!ctx.can_step);
    assert!(ctx.is_editing);
    assert!(
        !ctx.has_frameable_scene,
        "fresh shell has no scene bounds for View camera framing"
    );
    assert_eq!(ctx.play_state, "editing");

    // Playing: Pause + Stop valid; Play + Step invalid; document File items disabled.
    s.handle_button(ToolbarButtonId::Play).unwrap();
    let ctx = s.predicate_context();
    assert!(!ctx.can_play);
    assert!(ctx.can_pause);
    assert!(ctx.can_stop);
    assert!(!ctx.can_step);
    assert!(!ctx.is_editing);
    assert_eq!(ctx.play_state, "playing");

    // Paused: all four play transitions valid; still not editing (PIE active).
    s.handle_button(ToolbarButtonId::Pause).unwrap();
    let ctx = s.predicate_context();
    assert!(ctx.can_play);
    assert!(ctx.can_pause);
    assert!(ctx.can_stop);
    assert!(ctx.can_step);
    assert!(!ctx.is_editing);
    assert_eq!(ctx.play_state, "paused");
}

#[test]
fn predicate_context_reports_frameable_prebuilt_scene() {
    let shell = EditorShell::with_render_mesh(build_test_render_mesh());
    let ctx = shell.predicate_context();
    assert!(
        ctx.has_frameable_scene,
        "prebuilt render meshes make View camera framing scene-aware"
    );
}

#[test]
fn predicate_context_reports_selectable_entities() {
    let mut shell = EditorShell::new();
    assert!(
        !shell.predicate_context().has_selectable_entities,
        "fresh shell has no selectable entities"
    );

    build_scene(&mut shell, 2);

    assert!(
        shell.predicate_context().has_selectable_entities,
        "a non-empty world enables Edit / Select All"
    );
}

#[test]
fn play_button_captures_snapshot() {
    let mut s = EditorShell::new();
    build_scene(&mut s, 5);
    let t = s.handle_button(ToolbarButtonId::Play).unwrap();
    assert_eq!(t, PlayStateTransition::StartedPlay);
    assert!(s.has_snapshot());
    assert_eq!(s.play_state(), PlayState::Playing);
}

#[test]
fn editing_does_not_tick_game_systems() {
    let mut s = EditorShell::new();
    build_scene(&mut s, 5);
    let pre = s.world().serialize();
    s.run_for_redraws(10);
    let post = s.world().serialize();
    assert_eq!(pre, post, "Editing must not advance game state");
    assert_eq!(s.tick_count(), 0);
}

#[test]
fn playing_advances_game_systems() {
    let mut s = EditorShell::new();
    build_scene(&mut s, 5);
    s.handle_button(ToolbarButtonId::Play).unwrap();
    let pre = s.world().serialize();
    s.run_for_redraws(10);
    let post = s.world().serialize();
    assert_ne!(pre, post, "Playing must advance game state");
    assert_eq!(s.tick_count(), 10);
}

#[test]
fn stop_restores_snapshot() {
    let mut s = EditorShell::new();
    build_scene(&mut s, 10);
    let pre_play = s.world().serialize();
    s.handle_button(ToolbarButtonId::Play).unwrap();
    s.run_for_redraws(60);
    let mid = s.world().serialize();
    assert_ne!(pre_play, mid);
    s.handle_button(ToolbarButtonId::Stop).unwrap();
    let post_stop = s.world().serialize();
    assert_eq!(pre_play, post_stop, "byte-identical restore");
    assert!(!s.has_snapshot());
    assert_eq!(s.play_state(), PlayState::Editing);
}

#[test]
fn pause_freezes_game_systems() {
    let mut s = EditorShell::new();
    build_scene(&mut s, 5);
    s.handle_button(ToolbarButtonId::Play).unwrap();
    s.run_for_redraws(5);
    let mid = s.world().serialize();
    s.handle_button(ToolbarButtonId::Pause).unwrap();
    s.run_for_redraws(20);
    let after_pause = s.world().serialize();
    assert_eq!(mid, after_pause, "Paused must freeze game state");
}

#[test]
fn step_advances_one_tick_in_paused() {
    let mut s = EditorShell::new();
    build_scene(&mut s, 5);
    s.handle_button(ToolbarButtonId::Play).unwrap();
    s.handle_button(ToolbarButtonId::Pause).unwrap();
    let pre = s.world().serialize();
    let pre_count = s.tick_count();
    s.handle_button(ToolbarButtonId::Step).unwrap();
    let post = s.world().serialize();
    assert_ne!(pre, post, "Step must advance one tick");
    assert_eq!(s.tick_count(), pre_count + 1);
}

#[test]
fn step_invalid_in_editing() {
    let mut s = EditorShell::new();
    let result = s.handle_button(ToolbarButtonId::Step);
    assert!(result.is_err());
}

#[test]
fn time_scale_affects_game_only() {
    let mut s = EditorShell::new();
    let e = s.world_mut().spawn();
    s.world_mut()
        .insert_component(e, ComponentTypeId(2), vec![0u8; 12]);
    s.set_time_scale(2.0);
    s.handle_button(ToolbarButtonId::Play).unwrap();
    s.run_for_redraws(60);
    let p = s.world().component(e, ComponentTypeId(2)).unwrap().clone();
    let mut x_bytes = [0u8; 4];
    x_bytes.copy_from_slice(&p[0..4]);
    let x = f32::from_le_bytes(x_bytes);
    // Position increments by `dt_scaled` per tick; with scale=2 and
    // dt=1/60 across 60 ticks, x = 60 * (1/60) * 2 = 2.0
    assert!((x - 2.0).abs() < 1e-3, "expected ~2.0, got {x}");
}

#[test]
fn audit_records_play_stop() {
    let mut s = EditorShell::new();
    build_scene(&mut s, 5);
    s.handle_button(ToolbarButtonId::Play).unwrap();
    s.handle_button(ToolbarButtonId::Stop).unwrap();
    let tags: Vec<_> = s.audit().iter().map(AuditEvent::tag).collect();
    assert!(tags.contains(&"SnapshotCaptured"));
    assert!(tags.contains(&"PlayPressed"));
    assert!(tags.contains(&"SnapshotRestored"));
    assert!(tags.contains(&"StopPressed"));
}

// ---------------------------------------------------------------------------
// Dispatch F — face-pick decision helper
// ---------------------------------------------------------------------------
//
// The `should_fire_face_pick(consumed, over_viewport)` pure function is
// the single decision the MouseInput arm in `lifecycle/mod.rs` makes for
// each left-click after dispatch F. Tests pin the 4-row truth table so a
// future refactor that flips a bit silently fails this file.

#[test]
fn face_pick_fires_when_egui_not_consumed_and_not_over_viewport() {
    // Pre-dock world / pre-dispatch-D behavior. Today the dock area
    // covers the whole window so this row is rare in practice, but
    // the predicate must keep firing.
    assert!(super::should_fire_face_pick(false, false));
}

#[test]
fn face_pick_fires_when_egui_not_consumed_and_over_viewport() {
    // egui_consumed=false implies no widget claimed the click; whether
    // the cursor is over the Viewport tab is irrelevant — fire.
    assert!(super::should_fire_face_pick(false, true));
}

#[test]
fn face_pick_blocked_when_egui_consumed_and_not_over_viewport() {
    // The Inspector tab / tab chrome path: click went to egui, no
    // viewport fallthrough — DO NOT fire face-pick (prevents
    // accidental picking through Inspector labels / tab titles).
    assert!(!super::should_fire_face_pick(true, false));
}

#[test]
fn face_pick_fires_when_egui_consumed_but_over_viewport() {
    // THE dispatch-F fix. egui consumes the click (it always does
    // today since the dock fills the window), but the cursor is over
    // the transparent Viewport tab → fall through to face-pick.
    assert!(super::should_fire_face_pick(true, true));
}

// ---------------------------------------------------------------------------
// Dispatch G — `EditorShell::with_render_mesh` (render-only glTF mode)
// ---------------------------------------------------------------------------

/// Build a minimal triangle [`RenderMesh`] for the constructor tests.
/// Uses [`rge_brep_render::RenderMesh::from_buffers`] directly so the
/// test doesn't depend on `rge-io-gltf` to exercise the editor-shell
/// surface.
fn build_test_render_mesh() -> rge_brep_render::RenderMesh {
    // One triangle in the XY plane at z=0; vertex tripling makes it
    // 3 vertices / 3 indices (consistent with the production
    // RenderMesh shape).
    let positions: Vec<[f32; 3]> = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    let indices: Vec<u32> = vec![0, 1, 2];
    rge_brep_render::RenderMesh::from_buffers(&positions, &indices, None)
}

#[test]
fn with_render_mesh_constructs_shell_without_cad() {
    let mesh = build_test_render_mesh();
    let shell = EditorShell::with_render_mesh(mesh);
    // CAD-side fields are all None — the imported mesh path does not
    // synthesize a fake operator graph / projection.
    assert!(
        shell.cad_world.is_none(),
        "render-only shell must NOT have a CAD ECS world"
    );
    assert!(
        shell.projection.is_none(),
        "render-only shell must NOT have a CAD projection"
    );
    assert!(
        shell.cad_graph.is_none(),
        "render-only shell must NOT have an operator graph"
    );
    assert!(
        shell.cad_entity.is_none(),
        "render-only shell must NOT pre-resolve a CAD entity"
    );
    // The prebuilt mesh IS populated. Dispatch I: storage is now
    // `Vec<RenderMesh>` — a single-mesh constructor must produce a
    // Vec of length 1.
    assert_eq!(
        shell.prebuilt_render_meshes.len(),
        1,
        "single-mesh constructor must hold exactly 1 prebuilt RenderMesh"
    );
}

#[test]
fn with_render_mesh_preserves_default_inspector_state() {
    let mesh = build_test_render_mesh();
    let shell = EditorShell::with_render_mesh(mesh);
    let snap = shell.inspector_snapshot();
    assert_eq!(snap.time_scale, 1.0);
    assert_eq!(snap.play_state_label, "Editing");
    assert_eq!(snap.tick_count, 0);
    assert!(!snap.has_snapshot);
    assert_eq!(snap.active_tool_label, "Select");
    assert_eq!(snap.selection_len, 0);
    assert_eq!(snap.face_selection_len, 0);
    assert!(!snap.is_dirty);
    assert_eq!(snap.undo_stack_len, 0);
    assert_eq!(snap.undo_cursor, 0);
}

#[test]
fn with_render_mesh_default_play_state_is_editing() {
    let mesh = build_test_render_mesh();
    let shell = EditorShell::with_render_mesh(mesh);
    assert_eq!(shell.play_state(), PlayState::Editing);
    assert_eq!(shell.tick_count(), 0);
    assert!(!shell.has_snapshot());
}

#[test]
fn with_render_mesh_play_pause_stop_round_trip_works() {
    // The render-only shell must still support the PIE state machine
    // — playback shortcuts (`Space`/`Escape`) drive PIE which is
    // orthogonal to the rendered geometry. Verify the round trip
    // mechanically.
    let mesh = build_test_render_mesh();
    let mut shell = EditorShell::with_render_mesh(mesh);

    shell.handle_button(ToolbarButtonId::Play).expect("play");
    assert_eq!(shell.play_state(), PlayState::Playing);
    assert!(shell.has_snapshot());

    shell.handle_button(ToolbarButtonId::Pause).expect("pause");
    assert_eq!(shell.play_state(), PlayState::Paused);
    assert!(shell.has_snapshot());

    shell.handle_button(ToolbarButtonId::Stop).expect("stop");
    assert_eq!(shell.play_state(), PlayState::Editing);
    assert!(!shell.has_snapshot());
}

// ---------------------------------------------------------------------------
// Dispatch H — auto-framing helpers + integration
// ---------------------------------------------------------------------------

#[test]
fn compute_aabb_empty_positions_returns_none() {
    // Empty input has no bbox to derive — caller falls back to the
    // default-cuboid camera.
    assert!(super::compute_aabb(&[]).is_none());
}

#[test]
fn compute_aabb_nan_position_returns_none() {
    // Defensive: a NaN coordinate in any axis poisons the camera
    // math; surface as None and let the caller fall back.
    assert!(super::compute_aabb(&[[f32::NAN, 0.0, 0.0]]).is_none());
    assert!(super::compute_aabb(&[[0.0, f32::NAN, 0.0]]).is_none());
    assert!(super::compute_aabb(&[[0.0, 0.0, f32::NAN]]).is_none());
}

#[test]
fn compute_aabb_infinity_position_returns_none() {
    assert!(super::compute_aabb(&[[f32::INFINITY, 0.0, 0.0]]).is_none());
    assert!(super::compute_aabb(&[[0.0, f32::NEG_INFINITY, 0.0]]).is_none());
}

#[test]
fn compute_aabb_single_point_yields_zero_extent_aabb() {
    let (min, max) =
        super::compute_aabb(&[[1.0, 2.0, 3.0]]).expect("single finite point should produce AABB");
    assert_eq!(min, glam::Vec3::new(1.0, 2.0, 3.0));
    assert_eq!(max, glam::Vec3::new(1.0, 2.0, 3.0));
}

#[test]
fn compute_aabb_unit_cube_positions_match_canonical_extents() {
    // 1×1×1 cube centered at origin (matches cube.glb fixture shape).
    let positions = [
        [-0.5, -0.5, -0.5],
        [0.5, -0.5, -0.5],
        [0.5, 0.5, -0.5],
        [-0.5, 0.5, -0.5],
        [-0.5, -0.5, 0.5],
        [0.5, -0.5, 0.5],
        [0.5, 0.5, 0.5],
        [-0.5, 0.5, 0.5],
    ];
    let (min, max) = super::compute_aabb(&positions).expect("valid positions");
    assert_eq!(min, glam::Vec3::new(-0.5, -0.5, -0.5));
    assert_eq!(max, glam::Vec3::new(0.5, 0.5, 0.5));
}

#[test]
fn compute_aabb_translated_bounds_offset_min_max_together() {
    // Mimics cube.glb's translated cube (if the transform tree were
    // applied — which v0 does NOT do; the test here just exercises the
    // AABB helper against a translated point cloud).
    let positions = [[10.5, 20.5, 30.5], [11.5, 21.5, 31.5]];
    let (min, max) = super::compute_aabb(&positions).expect("valid positions");
    assert_eq!(min, glam::Vec3::new(10.5, 20.5, 30.5));
    assert_eq!(max, glam::Vec3::new(11.5, 21.5, 31.5));
}

#[test]
fn isometric_camera_for_unit_cube_matches_existing_default() {
    // The framing math is calibrated against the
    // `EditorCameraState::default()` placement: a centered 1×1×1 cube
    // produces the SAME camera as the hardcoded default-cuboid demo.
    // This pins the dispatch-G visual continuity: glTF cube ≈ CAD
    // cube on screen (modulo materials).
    let cam = super::isometric_camera_for_bounds(
        glam::Vec3::new(-0.5, -0.5, -0.5),
        glam::Vec3::new(0.5, 0.5, 0.5),
    );
    let default_cam = crate::camera::EditorCameraState::default();
    assert!(
        (cam.eye - default_cam.eye).length() < 1e-4,
        "auto-framed eye {:?} should match default {:?}",
        cam.eye,
        default_cam.eye
    );
    assert_eq!(cam.target, default_cam.target);
    assert_eq!(cam.up, default_cam.up);
    assert_eq!(cam.fov_y_radians, default_cam.fov_y_radians);
}

#[test]
fn isometric_camera_target_equals_bounds_center() {
    // Arbitrary translated bbox — the camera should ALWAYS point at
    // the AABB center, never at the world origin.
    let (min, max) = (
        glam::Vec3::new(100.0, 200.0, 300.0),
        glam::Vec3::new(110.0, 220.0, 330.0),
    );
    let cam = super::isometric_camera_for_bounds(min, max);
    let expected_center = (min + max) * 0.5; // (105, 210, 315)
    assert_eq!(cam.target, expected_center);
}

#[test]
fn isometric_camera_distance_scales_with_diagonal() {
    // A bbox 10× larger should put the camera ~10× further from the
    // target — otherwise it'd clip into the geometry or be invisible.
    let small_cam = super::isometric_camera_for_bounds(
        glam::Vec3::new(-0.5, -0.5, -0.5),
        glam::Vec3::new(0.5, 0.5, 0.5),
    );
    let large_cam = super::isometric_camera_for_bounds(
        glam::Vec3::new(-5.0, -5.0, -5.0),
        glam::Vec3::new(5.0, 5.0, 5.0),
    );
    let small_dist = (small_cam.eye - small_cam.target).length();
    let large_dist = (large_cam.eye - large_cam.target).length();
    let ratio = large_dist / small_dist;
    // 10× geometry → 10× distance (within FP rounding).
    assert!(
        (ratio - 10.0).abs() < 1e-3,
        "distance ratio {ratio} should be ~10.0 for a 10× larger bbox"
    );
}

#[test]
fn isometric_camera_for_tiny_bounds_has_nonzero_distance() {
    // A 0.001-extent bbox shouldn't collapse the eye onto the target
    // (would divide-by-zero the view matrix). The distance must be a
    // sane positive number.
    let cam = super::isometric_camera_for_bounds(
        glam::Vec3::new(-0.0005, -0.0005, -0.0005),
        glam::Vec3::new(0.0005, 0.0005, 0.0005),
    );
    let distance = (cam.eye - cam.target).length();
    assert!(
        distance > 0.0 && distance.is_finite(),
        "tiny bbox produced distance {distance} — must be positive + finite"
    );
}

#[test]
fn isometric_camera_for_degenerate_zero_extent_uses_fallback() {
    // A single-point cloud (min == max) gets `effective_diag = 1.0`
    // so the camera sits at a sane fallback distance, pointing at
    // the single point.
    let point = glam::Vec3::new(1.0, 2.0, 3.0);
    let cam = super::isometric_camera_for_bounds(point, point);
    assert_eq!(cam.target, point);
    let distance = (cam.eye - cam.target).length();
    // effective_diag = 1.0, distance = 3.0 × effective_diag = 3.0.
    assert!(
        (distance - 3.0).abs() < 1e-4,
        "degenerate-bbox distance {distance} should be 3.0 (fallback)"
    );
}

#[test]
fn isometric_camera_near_far_scale_with_distance() {
    // The near / far planes must scale with the bbox so a 100-unit
    // mesh isn't clipped at far = 100. Default-cuboid scale uses the
    // floor (near = 0.1, far = 100); large scales lift both.
    let large_cam = super::isometric_camera_for_bounds(
        glam::Vec3::new(0.0, 0.0, 0.0),
        glam::Vec3::new(100.0, 100.0, 100.0),
    );
    let distance = (large_cam.eye - large_cam.target).length();
    assert!(
        large_cam.far >= distance,
        "far plane {} should be >= eye-target distance {}",
        large_cam.far,
        distance
    );
    assert!(
        large_cam.near > 0.0 && large_cam.near < distance,
        "near plane {} should be in (0, distance={}) range",
        large_cam.near,
        distance
    );
    assert!(large_cam.near.is_finite());
    assert!(large_cam.far.is_finite());
}

#[test]
fn with_render_mesh_translated_bounds_yields_framed_camera() {
    // End-to-end: a RenderMesh whose positions are NOT centered at
    // the origin should produce a shell whose camera targets the
    // mesh's bounds center.
    let positions: Vec<[f32; 3]> = vec![[10.0, 20.0, 30.0], [12.0, 20.0, 30.0], [10.0, 22.0, 30.0]];
    let indices: Vec<u32> = vec![0, 1, 2];
    let mesh = rge_brep_render::RenderMesh::from_buffers(&positions, &indices, None);
    let shell = EditorShell::with_render_mesh(mesh);
    let expected_center = glam::Vec3::new(11.0, 21.0, 30.0);
    assert_eq!(
        shell.editor_camera.target, expected_center,
        "shell's editor_camera.target must be the mesh's bbox center"
    );
    // Eye must be offset from center by the isometric direction.
    let dir = (shell.editor_camera.eye - shell.editor_camera.target).normalize();
    let canonical = glam::Vec3::new(1.0, 1.0, 1.0).normalize();
    assert!(
        (dir - canonical).length() < 1e-4,
        "camera direction should match canonical isometric (1,1,1)/√3"
    );
}

#[test]
fn reset_camera_frames_prebuilt_meshes() {
    // EditorShell::reset_camera reframes editor_camera to the LIVE scene's
    // bounds. Build a shell from an OFF-ORIGIN mesh (auto-framed at
    // construction), clobber the camera away, then reset and assert it
    // reframes to the same bbox center + canonical isometric direction.
    let positions: Vec<[f32; 3]> = vec![[10.0, 20.0, 30.0], [12.0, 20.0, 30.0], [10.0, 22.0, 30.0]];
    let indices: Vec<u32> = vec![0, 1, 2];
    let mesh = rge_brep_render::RenderMesh::from_buffers(&positions, &indices, None);
    let mut shell = EditorShell::with_render_mesh(mesh);
    // Move the camera somewhere unrelated to prove reset_camera reframes.
    shell.editor_camera.target = glam::Vec3::ZERO;
    shell.editor_camera.eye = glam::Vec3::splat(999.0);

    shell.reset_camera();

    let expected_center = glam::Vec3::new(11.0, 21.0, 30.0);
    assert_eq!(
        shell.editor_camera.target, expected_center,
        "reset_camera reframes editor_camera.target to the live mesh's bbox center"
    );
    let dir = (shell.editor_camera.eye - shell.editor_camera.target).normalize();
    let canonical = glam::Vec3::new(1.0, 1.0, 1.0).normalize();
    assert!(
        (dir - canonical).length() < 1e-4,
        "reset_camera restores the canonical isometric (1,1,1)/√3 direction"
    );
}

#[test]
fn reset_camera_with_no_scene_falls_back_to_default() {
    // A fresh EditorShell::new() has neither prebuilt meshes nor a CAD scene,
    // so current_scene_bounds() is None and reset_camera falls back to the
    // default pose (eye at (3,3,3), per editor_camera_state_default_eye_at_3_3_3).
    let mut shell = EditorShell::new();
    shell.editor_camera.eye = glam::Vec3::splat(999.0);

    shell.reset_camera();

    assert_eq!(
        shell.editor_camera.eye,
        glam::Vec3::new(3.0, 3.0, 3.0),
        "with nothing frameable, reset_camera falls back to the default camera pose"
    );
}

fn viewport_left_double_click_seed_shell() -> EditorShell {
    let positions: Vec<[f32; 3]> = vec![[10.0, 20.0, 30.0], [12.0, 20.0, 30.0], [10.0, 22.0, 30.0]];
    let indices: Vec<u32> = vec![0, 1, 2];
    let mesh = rge_brep_render::RenderMesh::from_buffers(&positions, &indices, None);
    EditorShell::with_render_mesh(mesh)
}

fn move_viewport_left_double_click_camera_off_scene(shell: &mut EditorShell) {
    shell.editor_camera.target = glam::Vec3::splat(-100.0);
    shell.editor_camera.eye = glam::Vec3::splat(999.0);
}

fn viewport_left_double_click_cad_shell() -> (
    EditorShell,
    rge_kernel_ecs::EntityId,
    rge_kernel_ecs::EntityId,
) {
    use rge_cad_core::{CadGraph, CuboidOp, OperatorNode, Tolerance, TransformOp};
    use rge_cad_projection::{BRepHandle, CadProjection};
    use rge_kernel_ecs::World;

    let mut graph = CadGraph::new();
    graph
        .begin_operation()
        .expect("CadGraph::begin_operation: no in-progress op pre-seed");
    let (origin_node, offset_node) = {
        let graph_mut = graph
            .graph_mut()
            .expect("CadGraph::graph_mut: in-progress op was just begun");
        let origin_node = graph_mut
            .add_operator(OperatorNode::Cuboid(CuboidOp {
                width: 2.0,
                height: 2.0,
                depth: 2.0,
            }))
            .expect("OperatorGraph::add_operator: origin cuboid is unique");
        let offset_raw = graph_mut
            .add_operator(OperatorNode::Cuboid(CuboidOp {
                width: 1.0,
                height: 1.5,
                depth: 1.0,
            }))
            .expect("OperatorGraph::add_operator: offset cuboid is unique");
        let offset_node = graph_mut
            .add_operator(OperatorNode::Transform(TransformOp {
                translation: [10.0, 0.0, 0.0],
                ..TransformOp::default()
            }))
            .expect("OperatorGraph::add_operator: offset transform is unique");
        graph_mut
            .connect(offset_raw, offset_node, 0)
            .expect("OperatorGraph::connect: offset cuboid feeds transform");
        graph_mut
            .set_root(origin_node)
            .expect("OperatorGraph::set_root: origin cuboid is a valid root");
        (origin_node, offset_node)
    };
    graph
        .commit("issue-393-selected-cad-double-click")
        .expect("CadGraph::commit: in-progress op has a root");

    let tolerance = Tolerance::new(0.001).expect("Tolerance::new(0.001): finite positive");
    let mut projection = CadProjection::new();
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    let origin_entity = projection
        .spawn_brep_entity(&mut world, origin_node)
        .expect("CadProjection::spawn_brep_entity: origin node exists");
    projection
        .tick(&mut world, &graph, tolerance)
        .expect("CadProjection::tick: origin entity projects");
    let mut shell = EditorShell::with_world_projection_graph(world, projection, graph);

    let offset_entity = {
        let projection = shell
            .projection
            .as_mut()
            .expect("CAD shell carries projection");
        let cad_world = shell.cad_world.as_mut().expect("CAD shell carries world");
        projection
            .spawn_brep_entity(cad_world, offset_node)
            .expect("CadProjection::spawn_brep_entity: offset node exists")
    };
    {
        let projection = shell
            .projection
            .as_mut()
            .expect("CAD shell carries projection");
        let cad_world = shell.cad_world.as_mut().expect("CAD shell carries world");
        let cad_graph = shell.cad_graph.as_ref().expect("CAD shell carries graph");
        projection
            .tick(cad_world, cad_graph, tolerance)
            .expect("CadProjection::tick: offset entity projects");
    }

    (shell, origin_entity, offset_entity)
}

fn camera_for_cad_entities(
    shell: &EditorShell,
    entities: &[rge_kernel_ecs::EntityId],
) -> crate::camera::EditorCameraState {
    let projection = shell
        .projection
        .as_ref()
        .expect("CAD shell carries projection");
    let cad_world = shell.cad_world.as_ref().expect("CAD shell carries world");
    let meshes = entities
        .iter()
        .map(|entity| {
            projection
                .render_mesh_for(*entity, cad_world)
                .expect("selected CAD entity has projected render mesh")
        })
        .collect::<Vec<_>>();
    let (min, max) =
        super::compute_aabb_union(&meshes).expect("selected CAD render meshes have finite bounds");
    super::isometric_camera_for_bounds(min, max)
}

fn perform_viewport_left_double_click(shell: &mut EditorShell) {
    let first = Instant::now();

    shell.cursor_pos = Some([100.0, 200.0]);
    shell.handle_viewport_left_press(true, true, first);
    shell.cursor_pos = Some([103.0, 204.0]);
    shell.handle_viewport_left_press(true, true, first + Duration::from_millis(250));
}

#[test]
fn viewport_left_double_click_second_viewport_press_frames_prebuilt_scene() {
    let expected = viewport_left_double_click_seed_shell().editor_camera;
    let mut shell = viewport_left_double_click_seed_shell();
    move_viewport_left_double_click_camera_off_scene(&mut shell);
    let moved = shell.editor_camera;
    let first = Instant::now();

    shell.cursor_pos = Some([100.0, 200.0]);
    shell.handle_viewport_left_press(true, true, first);

    assert_camera_unchanged(moved, shell.editor_camera);

    shell.cursor_pos = Some([103.0, 204.0]);
    shell.handle_viewport_left_press(true, true, first + Duration::from_millis(250));

    assert_camera_unchanged(expected, shell.editor_camera);
}

#[test]
fn viewport_left_double_click_selected_cad_entity_frames_selected_bounds() {
    let (mut shell, origin_entity, offset_entity) = viewport_left_double_click_cad_shell();
    let scene_expected = camera_for_cad_entities(&shell, &[origin_entity]);
    let selected_expected = camera_for_cad_entities(&shell, &[offset_entity]);
    assert_ne!(
        scene_expected.target, selected_expected.target,
        "test setup must distinguish selected CAD framing from scene-wide fallback"
    );

    shell.coord_mut().selection.add(offset_entity);
    move_viewport_left_double_click_camera_off_scene(&mut shell);

    perform_viewport_left_double_click(&mut shell);

    assert_camera_unchanged(selected_expected, shell.editor_camera);
}

#[test]
fn viewport_left_double_click_multiple_selected_cad_entities_frames_union() {
    let (mut shell, origin_entity, offset_entity) = viewport_left_double_click_cad_shell();
    let origin_expected = camera_for_cad_entities(&shell, &[origin_entity]);
    let offset_expected = camera_for_cad_entities(&shell, &[offset_entity]);
    let union_expected = camera_for_cad_entities(&shell, &[origin_entity, offset_entity]);
    assert_ne!(
        union_expected.target, origin_expected.target,
        "test setup must distinguish selected union from the first CAD entity"
    );
    assert_ne!(
        union_expected.target, offset_expected.target,
        "test setup must distinguish selected union from the second CAD entity"
    );

    shell.coord_mut().selection.add(origin_entity);
    shell.coord_mut().selection.add(offset_entity);
    move_viewport_left_double_click_camera_off_scene(&mut shell);

    perform_viewport_left_double_click(&mut shell);

    assert_camera_unchanged(union_expected, shell.editor_camera);
}

#[test]
fn viewport_left_double_click_cad_no_selection_falls_back_to_scene_bounds() {
    let (mut shell, origin_entity, offset_entity) = viewport_left_double_click_cad_shell();
    let scene_expected = camera_for_cad_entities(&shell, &[origin_entity]);
    let offset_expected = camera_for_cad_entities(&shell, &[offset_entity]);
    assert_ne!(
        scene_expected.target, offset_expected.target,
        "test setup must distinguish scene fallback from the unselected CAD entity"
    );

    move_viewport_left_double_click_camera_off_scene(&mut shell);

    perform_viewport_left_double_click(&mut shell);

    assert_camera_unchanged(scene_expected, shell.editor_camera);
}

#[test]
fn viewport_left_double_click_unresolved_selected_cad_entity_falls_back_to_scene_bounds() {
    let (mut shell, origin_entity, offset_entity) = viewport_left_double_click_cad_shell();
    let scene_expected = camera_for_cad_entities(&shell, &[origin_entity]);
    let offset_expected = camera_for_cad_entities(&shell, &[offset_entity]);
    assert_ne!(
        scene_expected.target, offset_expected.target,
        "test setup must distinguish scene fallback from a valid selected CAD entity"
    );

    shell
        .coord_mut()
        .selection
        .add(rge_kernel_ecs::EntityId::new());
    move_viewport_left_double_click_camera_off_scene(&mut shell);

    perform_viewport_left_double_click(&mut shell);

    assert_camera_unchanged(scene_expected, shell.editor_camera);
}

#[test]
fn viewport_left_double_click_empty_scene_falls_back_to_default_camera() {
    let mut shell = EditorShell::new();
    move_viewport_left_double_click_camera_off_scene(&mut shell);
    let first = Instant::now();

    shell.cursor_pos = Some([40.0, 60.0]);
    shell.handle_viewport_left_press(false, true, first);
    shell.cursor_pos = Some([43.0, 64.0]);
    shell.handle_viewport_left_press(false, true, first + Duration::from_millis(250));

    assert_camera_unchanged(
        crate::camera::EditorCameraState::default(),
        shell.editor_camera,
    );
}

#[test]
fn viewport_left_double_click_outside_or_invalid_cursor_is_no_op_and_resets() {
    let mut shell = viewport_left_double_click_seed_shell();
    move_viewport_left_double_click_camera_off_scene(&mut shell);
    let moved = shell.editor_camera;
    let first = Instant::now();

    shell.cursor_pos = Some([40.0, 60.0]);
    shell.handle_viewport_left_press(true, false, first);
    shell.cursor_pos = Some([43.0, 64.0]);
    shell.handle_viewport_left_press(true, true, first + Duration::from_millis(250));
    assert_camera_unchanged(moved, shell.editor_camera);

    shell.cursor_pos = None;
    shell.handle_viewport_left_press(true, true, first + Duration::from_millis(500));
    shell.cursor_pos = Some([43.0, 64.0]);
    shell.handle_viewport_left_press(true, true, first + Duration::from_millis(600));
    assert_camera_unchanged(moved, shell.editor_camera);

    shell.cursor_pos = Some([f32::NAN, 64.0]);
    shell.handle_viewport_left_press(true, true, first + Duration::from_millis(700));
    shell.cursor_pos = Some([43.0, 64.0]);
    shell.handle_viewport_left_press(true, true, first + Duration::from_millis(800));
    assert_camera_unchanged(moved, shell.editor_camera);
}

#[test]
fn viewport_left_double_click_time_or_movement_threshold_miss_does_not_frame() {
    let first = Instant::now();

    let mut too_late = viewport_left_double_click_seed_shell();
    move_viewport_left_double_click_camera_off_scene(&mut too_late);
    let moved = too_late.editor_camera;
    too_late.cursor_pos = Some([40.0, 60.0]);
    too_late.handle_viewport_left_press(true, true, first);
    too_late.cursor_pos = Some([40.0, 60.0]);
    too_late.handle_viewport_left_press(true, true, first + Duration::from_secs(2));
    assert_camera_unchanged(moved, too_late.editor_camera);

    let mut too_far = viewport_left_double_click_seed_shell();
    move_viewport_left_double_click_camera_off_scene(&mut too_far);
    let moved = too_far.editor_camera;
    too_far.cursor_pos = Some([40.0, 60.0]);
    too_far.handle_viewport_left_press(true, true, first);
    too_far.cursor_pos = Some([80.0, 60.0]);
    too_far.handle_viewport_left_press(true, true, first + Duration::from_millis(250));
    assert_camera_unchanged(moved, too_far.editor_camera);
}

#[test]
fn viewport_left_double_click_single_press_preserves_face_pick_gate_and_does_not_frame() {
    assert!(super::should_fire_face_pick(false, false));
    assert!(super::should_fire_face_pick(false, true));
    assert!(!super::should_fire_face_pick(true, false));
    assert!(super::should_fire_face_pick(true, true));

    let mut shell = viewport_left_double_click_seed_shell();
    move_viewport_left_double_click_camera_off_scene(&mut shell);
    let moved = shell.editor_camera;

    shell.cursor_pos = Some([40.0, 60.0]);
    shell.handle_viewport_left_press(false, true, Instant::now());

    assert_camera_unchanged(moved, shell.editor_camera);
}

#[test]
fn viewport_left_double_click_selected_cad_single_press_preserves_face_pick_gate() {
    assert!(super::should_fire_face_pick(false, false));
    assert!(super::should_fire_face_pick(false, true));
    assert!(!super::should_fire_face_pick(true, false));
    assert!(super::should_fire_face_pick(true, true));

    let (mut shell, _origin_entity, offset_entity) = viewport_left_double_click_cad_shell();
    let selected_expected = camera_for_cad_entities(&shell, &[offset_entity]);
    shell.coord_mut().selection.add(offset_entity);
    move_viewport_left_double_click_camera_off_scene(&mut shell);
    let moved = shell.editor_camera;
    assert_ne!(
        moved.eye, selected_expected.eye,
        "test setup must distinguish first-press no-op from selected framing"
    );

    shell.cursor_pos = Some([40.0, 60.0]);
    shell.handle_viewport_left_press(false, true, Instant::now());

    assert_camera_unchanged(moved, shell.editor_camera);
    assert!(
        shell.coord().face_selection.is_empty(),
        "headless lifecycle face-pick path remains a no-op without surface context"
    );
}

#[test]
fn reset_camera_resets_stale_viewport_left_double_click_before_scene_frame() {
    let expected_reset = viewport_left_double_click_seed_shell().editor_camera;
    let mut shell = viewport_left_double_click_seed_shell();
    move_viewport_left_double_click_camera_off_scene(&mut shell);
    let first = Instant::now();

    shell.cursor_pos = Some([40.0, 60.0]);
    shell.handle_viewport_left_press(true, true, first);
    shell.reset_camera();
    assert_camera_unchanged(expected_reset, shell.editor_camera);

    move_viewport_left_double_click_camera_off_scene(&mut shell);
    let after_reset_move = shell.editor_camera;
    assert_ne!(
        after_reset_move.eye, expected_reset.eye,
        "test setup must distinguish a stale scene frame from no-op after reset_camera"
    );

    shell.cursor_pos = Some([43.0, 64.0]);
    shell.handle_viewport_left_press(true, true, first + Duration::from_millis(250));

    assert_camera_unchanged(after_reset_move, shell.editor_camera);
}

#[test]
fn reset_camera_resets_stale_viewport_left_double_click_before_selected_cad_frame() {
    let (mut shell, origin_entity, offset_entity) = viewport_left_double_click_cad_shell();
    let scene_expected = camera_for_cad_entities(&shell, &[origin_entity]);
    let selected_expected = camera_for_cad_entities(&shell, &[offset_entity]);
    assert_ne!(
        scene_expected.target, selected_expected.target,
        "test setup must distinguish reset-camera scene framing from selected CAD framing"
    );

    shell.coord_mut().selection.add(offset_entity);
    move_viewport_left_double_click_camera_off_scene(&mut shell);
    let first = Instant::now();

    shell.cursor_pos = Some([40.0, 60.0]);
    shell.handle_viewport_left_press(true, true, first);
    shell.reset_camera();
    assert_camera_unchanged(scene_expected, shell.editor_camera);

    move_viewport_left_double_click_camera_off_scene(&mut shell);
    let after_reset_move = shell.editor_camera;
    assert_ne!(
        after_reset_move.target, selected_expected.target,
        "test setup must distinguish stale selected CAD framing from no-op after reset_camera"
    );

    shell.cursor_pos = Some([43.0, 64.0]);
    shell.handle_viewport_left_press(true, true, first + Duration::from_millis(250));

    assert_camera_unchanged(after_reset_move, shell.editor_camera);
}

#[test]
fn viewport_left_double_click_zoom_in_resets_stale_before_scene_frame() {
    let expected_framed = viewport_left_double_click_seed_shell().editor_camera;
    let mut shell = viewport_left_double_click_seed_shell();
    move_viewport_left_double_click_camera_off_scene(&mut shell);
    let moved = shell.editor_camera;
    assert_ne!(
        moved.eye, expected_framed.eye,
        "test setup must distinguish stale scene framing from no-op"
    );
    let first = Instant::now();

    shell.cursor_pos = Some([40.0, 60.0]);
    shell.handle_viewport_left_press(true, true, first);
    shell.zoom_camera_in();
    let after_zoom = shell.editor_camera;
    assert_ne!(
        after_zoom.eye, expected_framed.eye,
        "test setup must distinguish stale scene framing from zoom in"
    );
    assert_ne!(
        after_zoom.eye, moved.eye,
        "test setup must exercise actual zoom in before the second press"
    );

    shell.cursor_pos = Some([43.0, 64.0]);
    shell.handle_viewport_left_press(true, true, first + Duration::from_millis(250));

    assert_camera_unchanged(after_zoom, shell.editor_camera);
}

#[test]
fn viewport_left_double_click_zoom_out_resets_stale_before_selected_cad_frame() {
    let (mut shell, origin_entity, offset_entity) = viewport_left_double_click_cad_shell();
    let scene_expected = camera_for_cad_entities(&shell, &[origin_entity]);
    let selected_expected = camera_for_cad_entities(&shell, &[offset_entity]);
    assert_ne!(
        scene_expected.target, selected_expected.target,
        "test setup must distinguish scene fallback from selected CAD framing"
    );

    shell.coord_mut().selection.add(offset_entity);
    move_viewport_left_double_click_camera_off_scene(&mut shell);
    let moved = shell.editor_camera;
    assert_ne!(
        moved.target, selected_expected.target,
        "test setup must distinguish stale selected CAD framing from no-op"
    );
    let first = Instant::now();

    shell.cursor_pos = Some([40.0, 60.0]);
    shell.handle_viewport_left_press(true, true, first);
    shell.zoom_camera_out();
    let after_zoom = shell.editor_camera;
    assert_ne!(
        after_zoom.target, selected_expected.target,
        "test setup must distinguish stale selected CAD framing from zoom out"
    );
    assert_ne!(
        after_zoom.eye, moved.eye,
        "test setup must exercise actual zoom out before the second press"
    );

    shell.cursor_pos = Some([43.0, 64.0]);
    shell.handle_viewport_left_press(true, true, first + Duration::from_millis(250));

    assert_camera_unchanged(after_zoom, shell.editor_camera);
}

#[test]
fn zoom_camera_in_and_out_preserve_target_and_direction() {
    let mut shell = EditorShell::new();
    shell.editor_camera.target = glam::Vec3::new(1.0, 2.0, 3.0);
    shell.editor_camera.eye = glam::Vec3::new(1.0, 2.0, 13.0);
    let target = shell.editor_camera.target;
    let up = shell.editor_camera.up;
    let fov = shell.editor_camera.fov_y_radians;
    let near = shell.editor_camera.near;
    let far = shell.editor_camera.far;
    let direction = (shell.editor_camera.eye - shell.editor_camera.target).normalize();

    shell.zoom_camera_in();
    let zoomed_in_offset = shell.editor_camera.eye - target;
    assert_eq!(shell.editor_camera.target, target);
    assert_eq!(shell.editor_camera.up, up);
    assert_eq!(shell.editor_camera.fov_y_radians, fov);
    assert_eq!(shell.editor_camera.near, near);
    assert_eq!(shell.editor_camera.far, far);
    assert!(
        (zoomed_in_offset.length() - 8.0).abs() < 1e-5,
        "Zoom In should scale 10.0 distance by 0.8; got {}",
        zoomed_in_offset.length()
    );
    assert!(
        (zoomed_in_offset.normalize() - direction).length() < 1e-5,
        "Zoom In should preserve view direction"
    );

    shell.zoom_camera_out();
    let round_trip_offset = shell.editor_camera.eye - target;
    assert!(
        (round_trip_offset.length() - 10.0).abs() < 1e-5,
        "Zoom In followed by Zoom Out should return to the prior distance; got {}",
        round_trip_offset.length()
    );
    assert!(
        (round_trip_offset.normalize() - direction).length() < 1e-5,
        "Zoom Out should preserve view direction"
    );
}

fn wheel_zoom_seed_shell() -> EditorShell {
    let mut shell = EditorShell::new();
    shell.editor_camera.target = glam::Vec3::new(1.0, 2.0, 3.0);
    shell.editor_camera.eye = glam::Vec3::new(1.0, 2.0, 13.0);
    shell
}

fn assert_camera_static_invariants_preserved(
    before: crate::camera::EditorCameraState,
    after: crate::camera::EditorCameraState,
    expected_distance: f32,
) {
    let offset = after.eye - before.target;
    let direction = (before.eye - before.target).normalize();
    assert_eq!(after.target, before.target);
    assert_eq!(after.up, before.up);
    assert_eq!(after.fov_y_radians, before.fov_y_radians);
    assert_eq!(after.near, before.near);
    assert_eq!(after.far, before.far);
    assert!(
        (offset.length() - expected_distance).abs() < 1e-5,
        "wheel zoom distance should be {expected_distance}; got {}",
        offset.length()
    );
    assert!(
        (offset.normalize() - direction).length() < 1e-5,
        "wheel zoom should preserve view direction"
    );
}

fn assert_camera_unchanged(
    before: crate::camera::EditorCameraState,
    after: crate::camera::EditorCameraState,
) {
    assert_eq!(after.eye, before.eye);
    assert_eq!(after.target, before.target);
    assert_eq!(after.up, before.up);
    assert_eq!(after.fov_y_radians, before.fov_y_radians);
    assert_eq!(after.near, before.near);
    assert_eq!(after.far, before.far);
}

fn assert_camera_orbit_invariants_preserved(
    before: crate::camera::EditorCameraState,
    after: crate::camera::EditorCameraState,
) {
    assert_eq!(after.target, before.target);
    assert_eq!(after.up, before.up);
    assert_eq!(after.fov_y_radians, before.fov_y_radians);
    assert_eq!(after.near, before.near);
    assert_eq!(after.far, before.far);
    assert!(after.eye.is_finite(), "orbit eye must stay finite");
    assert!(after.target.is_finite(), "orbit target must stay finite");

    let before_distance = (before.eye - before.target).length();
    let after_distance = (after.eye - after.target).length();
    assert!(
        (after_distance - before_distance).abs() < 1e-5,
        "orbit should preserve eye-target distance; before={before_distance}, after={after_distance}"
    );
}

fn assert_vec3_approx_eq(actual: glam::Vec3, expected: glam::Vec3, context: &str) {
    assert!(
        (actual - expected).length() < 1e-5,
        "{context}: actual={actual:?}, expected={expected:?}"
    );
}

fn assert_camera_pan_invariants_preserved(
    before: crate::camera::EditorCameraState,
    after: crate::camera::EditorCameraState,
    expected_translation: glam::Vec3,
) {
    assert_vec3_approx_eq(
        after.eye - before.eye,
        expected_translation,
        "pan eye delta",
    );
    assert_vec3_approx_eq(
        after.target - before.target,
        expected_translation,
        "pan target delta",
    );
    assert_vec3_approx_eq(
        after.eye - after.target,
        before.eye - before.target,
        "pan eye-target offset",
    );
    assert_eq!(after.up, before.up);
    assert_eq!(after.fov_y_radians, before.fov_y_radians);
    assert_eq!(after.near, before.near);
    assert_eq!(after.far, before.far);
    assert!(after.eye.is_finite(), "pan eye must stay finite");
    assert!(after.target.is_finite(), "pan target must stay finite");
}

#[test]
fn viewport_mouse_wheel_signs_map_line_and_pixel_vertical_delta() {
    assert_eq!(
        super::viewport_mouse_wheel_zoom_direction(&MouseScrollDelta::LineDelta(8.0, 1.0), true),
        Some(super::ViewportMouseWheelZoom::In)
    );
    assert_eq!(
        super::viewport_mouse_wheel_zoom_direction(
            &MouseScrollDelta::PixelDelta(PhysicalPosition::new(8.0, 1.0)),
            true,
        ),
        Some(super::ViewportMouseWheelZoom::In)
    );
    assert_eq!(
        super::viewport_mouse_wheel_zoom_direction(&MouseScrollDelta::LineDelta(8.0, -1.0), true),
        Some(super::ViewportMouseWheelZoom::Out)
    );
    assert_eq!(
        super::viewport_mouse_wheel_zoom_direction(
            &MouseScrollDelta::PixelDelta(PhysicalPosition::new(8.0, -1.0)),
            true,
        ),
        Some(super::ViewportMouseWheelZoom::Out)
    );
    assert_eq!(
        super::viewport_mouse_wheel_zoom_direction(&MouseScrollDelta::LineDelta(8.0, 0.0), true),
        None
    );
    assert_eq!(
        super::viewport_mouse_wheel_zoom_direction(
            &MouseScrollDelta::PixelDelta(PhysicalPosition::new(8.0, 0.0)),
            true,
        ),
        None
    );
    assert_eq!(
        super::viewport_mouse_wheel_zoom_direction(&MouseScrollDelta::LineDelta(0.0, 1.0), false),
        None
    );
}

#[test]
fn viewport_mouse_wheel_positive_vertical_delta_zooms_in_preserving_invariants() {
    let mut shell = wheel_zoom_seed_shell();
    let before = shell.editor_camera;

    shell.zoom_camera_for_viewport_mouse_wheel(&MouseScrollDelta::LineDelta(12.0, 1.0), true);

    assert_camera_static_invariants_preserved(before, shell.editor_camera, 8.0);
}

#[test]
fn viewport_mouse_wheel_negative_vertical_delta_zooms_out_preserving_invariants() {
    let mut shell = wheel_zoom_seed_shell();
    let before = shell.editor_camera;

    shell.zoom_camera_for_viewport_mouse_wheel(
        &MouseScrollDelta::PixelDelta(PhysicalPosition::new(12.0, -1.0)),
        true,
    );

    assert_camera_static_invariants_preserved(before, shell.editor_camera, 12.5);
}

#[test]
fn viewport_mouse_wheel_horizontal_only_delta_is_no_op() {
    let mut shell = wheel_zoom_seed_shell();
    let before = shell.editor_camera;

    shell.zoom_camera_for_viewport_mouse_wheel(&MouseScrollDelta::LineDelta(12.0, 0.0), true);
    shell.zoom_camera_for_viewport_mouse_wheel(
        &MouseScrollDelta::PixelDelta(PhysicalPosition::new(-12.0, 0.0)),
        true,
    );

    assert_camera_unchanged(before, shell.editor_camera);
}

#[test]
fn viewport_mouse_wheel_false_viewport_hit_test_is_no_op() {
    let mut shell = wheel_zoom_seed_shell();
    let before = shell.editor_camera;

    shell.zoom_camera_for_viewport_mouse_wheel(&MouseScrollDelta::LineDelta(0.0, 1.0), false);
    shell.zoom_camera_for_viewport_mouse_wheel(
        &MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, -1.0)),
        false,
    );

    assert_camera_unchanged(before, shell.editor_camera);
}

#[test]
fn viewport_mouse_wheel_no_cursor_or_no_host_is_no_op() {
    let mut no_cursor = wheel_zoom_seed_shell();
    assert!(no_cursor.cursor_pos.is_none());
    let before = no_cursor.editor_camera;
    let over_viewport = no_cursor.is_pointer_over_viewport_tab();
    assert!(!over_viewport);

    no_cursor.zoom_camera_for_viewport_mouse_wheel(
        &MouseScrollDelta::LineDelta(0.0, 1.0),
        over_viewport,
    );

    assert_camera_unchanged(before, no_cursor.editor_camera);

    let mut no_host = wheel_zoom_seed_shell();
    no_host.cursor_pos = Some([42.0, 24.0]);
    let before = no_host.editor_camera;
    let over_viewport = no_host.is_pointer_over_viewport_tab();
    assert!(!over_viewport);

    no_host.zoom_camera_for_viewport_mouse_wheel(
        &MouseScrollDelta::LineDelta(0.0, 1.0),
        over_viewport,
    );

    assert_camera_unchanged(before, no_host.editor_camera);
}

#[test]
fn viewport_mouse_wheel_resets_stale_left_double_click_before_scene_frame() {
    let expected_framed = viewport_left_double_click_seed_shell().editor_camera;
    let mut shell = viewport_left_double_click_seed_shell();
    move_viewport_left_double_click_camera_off_scene(&mut shell);
    let moved = shell.editor_camera;
    assert_ne!(
        moved.eye, expected_framed.eye,
        "test setup must distinguish stale scene framing from no-op"
    );
    let first = Instant::now();

    shell.cursor_pos = Some([40.0, 60.0]);
    shell.handle_viewport_left_press(true, true, first);
    shell.zoom_camera_for_viewport_mouse_wheel(&MouseScrollDelta::LineDelta(0.0, 1.0), true);
    let after_wheel = shell.editor_camera;
    assert_ne!(
        after_wheel.eye, expected_framed.eye,
        "test setup must distinguish stale scene framing from wheel zoom"
    );
    assert_ne!(
        after_wheel.eye, moved.eye,
        "test setup must exercise actual wheel zoom before the second press"
    );
    shell.cursor_pos = Some([43.0, 64.0]);
    shell.handle_viewport_left_press(true, true, first + Duration::from_millis(250));

    assert_camera_unchanged(after_wheel, shell.editor_camera);
}

#[test]
fn viewport_mouse_wheel_resets_stale_left_double_click_before_selected_cad_frame() {
    let (mut shell, origin_entity, offset_entity) = viewport_left_double_click_cad_shell();
    let scene_expected = camera_for_cad_entities(&shell, &[origin_entity]);
    let selected_expected = camera_for_cad_entities(&shell, &[offset_entity]);
    assert_ne!(
        scene_expected.target, selected_expected.target,
        "test setup must distinguish scene fallback from selected CAD framing"
    );

    shell.coord_mut().selection.add(offset_entity);
    move_viewport_left_double_click_camera_off_scene(&mut shell);
    let moved = shell.editor_camera;
    assert_ne!(
        moved.target, scene_expected.target,
        "test setup must distinguish stale scene framing from no-op"
    );
    assert_ne!(
        moved.target, selected_expected.target,
        "test setup must distinguish stale selected CAD framing from no-op"
    );
    let first = Instant::now();

    shell.cursor_pos = Some([40.0, 60.0]);
    shell.handle_viewport_left_press(true, true, first);
    shell.zoom_camera_for_viewport_mouse_wheel(&MouseScrollDelta::LineDelta(12.0, 0.0), true);
    shell.cursor_pos = Some([43.0, 64.0]);
    shell.handle_viewport_left_press(true, true, first + Duration::from_millis(250));

    assert_camera_unchanged(moved, shell.editor_camera);
}

#[test]
fn viewport_right_button_orbit_starts_only_with_cursor_and_viewport_hit() {
    let mut shell = wheel_zoom_seed_shell();
    shell.set_viewport_cursor_grab_test_window_available(true);

    shell.start_viewport_orbit_drag(true);
    assert!(!shell.is_viewport_orbit_drag_active());
    assert!(shell.viewport_cursor_grab_test_events().is_empty());

    shell.cursor_pos = Some([40.0, 60.0]);
    shell.start_viewport_orbit_drag(false);
    assert!(!shell.is_viewport_orbit_drag_active());
    assert!(shell.viewport_cursor_grab_test_events().is_empty());

    shell.cursor_pos = Some([f32::NAN, 60.0]);
    shell.start_viewport_orbit_drag(true);
    assert!(!shell.is_viewport_orbit_drag_active());
    assert!(shell.viewport_cursor_grab_test_events().is_empty());

    shell.cursor_pos = Some([40.0, 60.0]);
    shell.start_viewport_orbit_drag(true);
    assert!(shell.is_viewport_orbit_drag_active());
    assert_eq!(
        shell.viewport_cursor_grab_test_events(),
        &[ViewportCursorGrabTestEvent::Grab]
    );
}

#[test]
fn viewport_right_button_orbit_cursor_delta_rotates_eye_preserving_camera_invariants() {
    let mut shell = wheel_zoom_seed_shell();
    shell.cursor_pos = Some([40.0, 60.0]);
    shell.start_viewport_orbit_drag(true);
    let before = shell.editor_camera;

    shell.update_viewport_orbit_drag([72.0, 84.0]);

    assert!(shell.is_viewport_orbit_drag_active());
    assert_camera_orbit_invariants_preserved(before, shell.editor_camera);
    assert!(
        (shell.editor_camera.eye - before.eye).length() > 1e-5,
        "non-zero orbit drag should move the eye around the target"
    );
}

#[test]
fn viewport_right_button_orbit_release_stops_future_cursor_rotation() {
    let mut shell = wheel_zoom_seed_shell();
    shell.set_viewport_cursor_grab_test_window_available(true);
    shell.cursor_pos = Some([40.0, 60.0]);
    shell.start_viewport_orbit_drag(true);
    shell.update_viewport_orbit_drag([72.0, 84.0]);
    shell.stop_viewport_orbit_drag();
    assert!(!shell.is_viewport_orbit_drag_active());
    assert_eq!(
        shell.viewport_cursor_grab_test_events(),
        &[
            ViewportCursorGrabTestEvent::Grab,
            ViewportCursorGrabTestEvent::Release,
        ]
    );
    let after_release = shell.editor_camera;

    shell.update_viewport_orbit_drag([120.0, 160.0]);

    assert_camera_unchanged(after_release, shell.editor_camera);
}

#[test]
fn viewport_right_button_orbit_no_active_or_non_finite_cursor_is_no_op() {
    let mut shell = wheel_zoom_seed_shell();
    let before = shell.editor_camera;

    shell.update_viewport_orbit_drag([72.0, 84.0]);

    assert_camera_unchanged(before, shell.editor_camera);

    shell.cursor_pos = Some([40.0, 60.0]);
    shell.start_viewport_orbit_drag(true);
    let before_non_finite_drag = shell.editor_camera;

    shell.update_viewport_orbit_drag([f32::NAN, 84.0]);

    assert!(shell.is_viewport_orbit_drag_active());
    assert_camera_unchanged(before_non_finite_drag, shell.editor_camera);
}

#[test]
fn viewport_middle_button_pan_starts_only_with_finite_cursor_and_viewport_hit() {
    let mut shell = wheel_zoom_seed_shell();
    shell.set_viewport_cursor_grab_test_window_available(true);

    shell.start_viewport_pan_drag(true);
    assert!(!shell.is_viewport_pan_drag_active());
    assert!(shell.viewport_cursor_grab_test_events().is_empty());

    shell.cursor_pos = Some([40.0, 60.0]);
    let over_viewport = shell.is_pointer_over_viewport_tab();
    assert!(!over_viewport, "fresh test shell has no egui host");
    shell.start_viewport_pan_drag(over_viewport);
    assert!(!shell.is_viewport_pan_drag_active());
    assert!(shell.viewport_cursor_grab_test_events().is_empty());

    shell.start_viewport_pan_drag(false);
    assert!(!shell.is_viewport_pan_drag_active());
    assert!(shell.viewport_cursor_grab_test_events().is_empty());

    shell.cursor_pos = Some([f32::NAN, 60.0]);
    shell.start_viewport_pan_drag(true);
    assert!(!shell.is_viewport_pan_drag_active());
    assert!(shell.viewport_cursor_grab_test_events().is_empty());

    shell.cursor_pos = Some([40.0, 60.0]);
    shell.start_viewport_pan_drag(true);
    assert!(shell.is_viewport_pan_drag_active());
    assert_eq!(
        shell.viewport_cursor_grab_test_events(),
        &[ViewportCursorGrabTestEvent::Grab]
    );
}

#[test]
fn viewport_middle_button_pan_cursor_delta_translates_eye_and_target_in_view_plane() {
    let mut shell = wheel_zoom_seed_shell();
    shell.cursor_pos = Some([400.0, 300.0]);
    shell.start_viewport_pan_drag(true);
    let before = shell.editor_camera;

    shell.update_viewport_pan_drag([460.0, 330.0]);

    assert!(shell.is_viewport_pan_drag_active());
    let world_units_per_pixel =
        2.0 * 10.0 * (before.fov_y_radians * 0.5).tan() / shell.viewport.height() as f32;
    let expected_translation = glam::Vec3::new(
        -60.0 * world_units_per_pixel,
        30.0 * world_units_per_pixel,
        0.0,
    );
    assert_camera_pan_invariants_preserved(before, shell.editor_camera, expected_translation);
}

#[test]
fn viewport_middle_button_pan_release_stops_future_cursor_translation() {
    let mut shell = wheel_zoom_seed_shell();
    shell.set_viewport_cursor_grab_test_window_available(true);
    shell.cursor_pos = Some([40.0, 60.0]);
    shell.start_viewport_pan_drag(true);
    shell.update_viewport_pan_drag([72.0, 84.0]);
    shell.stop_viewport_pan_drag();
    assert!(!shell.is_viewport_pan_drag_active());
    assert_eq!(
        shell.viewport_cursor_grab_test_events(),
        &[
            ViewportCursorGrabTestEvent::Grab,
            ViewportCursorGrabTestEvent::Release,
        ]
    );
    let after_release = shell.editor_camera;

    shell.update_viewport_pan_drag([120.0, 160.0]);

    assert_camera_unchanged(after_release, shell.editor_camera);
}

#[test]
fn viewport_middle_button_pan_no_active_zero_delta_and_non_finite_cursor_are_no_ops() {
    let mut shell = wheel_zoom_seed_shell();
    let before = shell.editor_camera;

    shell.update_viewport_pan_drag([72.0, 84.0]);

    assert_camera_unchanged(before, shell.editor_camera);

    shell.cursor_pos = Some([40.0, 60.0]);
    shell.start_viewport_pan_drag(true);
    let after_start = shell.editor_camera;

    shell.update_viewport_pan_drag([40.0, 60.0]);
    assert!(shell.is_viewport_pan_drag_active());
    assert_camera_unchanged(after_start, shell.editor_camera);

    shell.update_viewport_pan_drag([f32::NAN, 84.0]);
    assert!(shell.is_viewport_pan_drag_active());
    assert_camera_unchanged(after_start, shell.editor_camera);
}

#[test]
fn viewport_middle_button_pan_degenerate_or_non_finite_camera_vectors_are_no_ops() {
    let mut degenerate_eye_target = wheel_zoom_seed_shell();
    degenerate_eye_target.cursor_pos = Some([40.0, 60.0]);
    degenerate_eye_target.start_viewport_pan_drag(true);
    degenerate_eye_target.editor_camera.eye = degenerate_eye_target.editor_camera.target;
    let before = degenerate_eye_target.editor_camera;

    degenerate_eye_target.update_viewport_pan_drag([72.0, 84.0]);

    assert_camera_unchanged(before, degenerate_eye_target.editor_camera);

    let mut degenerate_up = wheel_zoom_seed_shell();
    degenerate_up.cursor_pos = Some([40.0, 60.0]);
    degenerate_up.start_viewport_pan_drag(true);
    degenerate_up.editor_camera.up = glam::Vec3::ZERO;
    let before = degenerate_up.editor_camera;

    degenerate_up.update_viewport_pan_drag([72.0, 84.0]);

    assert_camera_unchanged(before, degenerate_up.editor_camera);

    let mut non_finite_eye = wheel_zoom_seed_shell();
    non_finite_eye.cursor_pos = Some([40.0, 60.0]);
    non_finite_eye.start_viewport_pan_drag(true);
    non_finite_eye.editor_camera.eye.x = f32::INFINITY;
    let before = non_finite_eye.editor_camera;

    non_finite_eye.update_viewport_pan_drag([72.0, 84.0]);

    assert_camera_unchanged(before, non_finite_eye.editor_camera);
}

#[test]
fn viewport_middle_button_pan_left_face_pick_gate_is_unchanged() {
    assert!(super::should_fire_face_pick(false, false));
    assert!(super::should_fire_face_pick(false, true));
    assert!(!super::should_fire_face_pick(true, false));
    assert!(super::should_fire_face_pick(true, true));
}

#[test]
fn viewport_middle_button_pan_wheel_zoom_still_requires_viewport_hit() {
    let mut shell = wheel_zoom_seed_shell();
    let before = shell.editor_camera;

    shell.zoom_camera_for_viewport_mouse_wheel(&MouseScrollDelta::LineDelta(0.0, 1.0), false);

    assert_camera_unchanged(before, shell.editor_camera);

    shell.zoom_camera_for_viewport_mouse_wheel(&MouseScrollDelta::LineDelta(0.0, 1.0), true);

    assert_camera_static_invariants_preserved(before, shell.editor_camera, 8.0);
}

#[test]
fn viewport_middle_button_pan_right_orbit_still_rotates_eye_only() {
    let mut shell = wheel_zoom_seed_shell();
    shell.cursor_pos = Some([40.0, 60.0]);
    shell.start_viewport_orbit_drag(true);
    let before = shell.editor_camera;

    shell.update_viewport_orbit_drag([72.0, 84.0]);

    assert!(shell.is_viewport_orbit_drag_active());
    assert_camera_orbit_invariants_preserved(before, shell.editor_camera);
    assert!((shell.editor_camera.eye - before.eye).length() > 1e-5);
}

#[test]
fn viewport_drag_cursor_grab_no_window_and_failed_grab_preserve_drag_activation() {
    let mut no_window = wheel_zoom_seed_shell();
    no_window.cursor_pos = Some([40.0, 60.0]);

    no_window.start_viewport_orbit_drag(true);

    assert!(no_window.is_viewport_orbit_drag_active());
    assert!(no_window.viewport_cursor_grab_test_events().is_empty());

    let mut failed_grab = wheel_zoom_seed_shell();
    failed_grab.set_viewport_cursor_grab_test_window_available(true);
    failed_grab.set_viewport_cursor_grab_test_fail_grab(true);
    failed_grab.cursor_pos = Some([40.0, 60.0]);

    failed_grab.start_viewport_pan_drag(true);

    assert!(failed_grab.is_viewport_pan_drag_active());
    assert_eq!(
        failed_grab.viewport_cursor_grab_test_events(),
        &[ViewportCursorGrabTestEvent::Grab]
    );
}

#[test]
fn viewport_drag_cursor_grab_release_waits_until_all_viewport_drags_stop() {
    let mut shell = wheel_zoom_seed_shell();
    shell.set_viewport_cursor_grab_test_window_available(true);
    shell.cursor_pos = Some([40.0, 60.0]);

    shell.start_viewport_orbit_drag(true);
    shell.start_viewport_pan_drag(true);

    assert!(shell.is_viewport_orbit_drag_active());
    assert!(shell.is_viewport_pan_drag_active());
    assert_eq!(
        shell.viewport_cursor_grab_test_events(),
        &[
            ViewportCursorGrabTestEvent::Grab,
            ViewportCursorGrabTestEvent::Grab,
        ]
    );

    shell.stop_viewport_orbit_drag();

    assert!(!shell.is_viewport_orbit_drag_active());
    assert!(shell.is_viewport_pan_drag_active());
    assert_eq!(
        shell.viewport_cursor_grab_test_events(),
        &[
            ViewportCursorGrabTestEvent::Grab,
            ViewportCursorGrabTestEvent::Grab,
        ]
    );

    shell.stop_viewport_pan_drag();

    assert!(!shell.is_viewport_pan_drag_active());
    assert_eq!(
        shell.viewport_cursor_grab_test_events(),
        &[
            ViewportCursorGrabTestEvent::Grab,
            ViewportCursorGrabTestEvent::Grab,
            ViewportCursorGrabTestEvent::Release,
        ]
    );

    let mut reverse = wheel_zoom_seed_shell();
    reverse.set_viewport_cursor_grab_test_window_available(true);
    reverse.cursor_pos = Some([40.0, 60.0]);

    reverse.start_viewport_orbit_drag(true);
    reverse.start_viewport_pan_drag(true);
    reverse.stop_viewport_pan_drag();

    assert!(reverse.is_viewport_orbit_drag_active());
    assert!(!reverse.is_viewport_pan_drag_active());
    assert_eq!(
        reverse.viewport_cursor_grab_test_events(),
        &[
            ViewportCursorGrabTestEvent::Grab,
            ViewportCursorGrabTestEvent::Grab,
        ]
    );

    reverse.stop_viewport_orbit_drag();

    assert_eq!(
        reverse.viewport_cursor_grab_test_events(),
        &[
            ViewportCursorGrabTestEvent::Grab,
            ViewportCursorGrabTestEvent::Grab,
            ViewportCursorGrabTestEvent::Release,
        ]
    );
}

#[test]
fn viewport_focus_loss_active_orbit_cancels_and_releases_cursor_grab() {
    let mut shell = wheel_zoom_seed_shell();
    shell.set_viewport_cursor_grab_test_window_available(true);
    shell.cursor_pos = Some([40.0, 60.0]);
    shell.start_viewport_orbit_drag(true);
    shell.update_viewport_orbit_drag([72.0, 84.0]);
    let after_drag = shell.editor_camera;

    shell.handle_window_focus_change(false);

    assert!(!shell.is_viewport_orbit_drag_active());
    assert!(!shell.is_viewport_pan_drag_active());
    assert_eq!(
        shell.viewport_cursor_grab_test_events(),
        &[
            ViewportCursorGrabTestEvent::Grab,
            ViewportCursorGrabTestEvent::Release,
        ]
    );

    shell.update_viewport_orbit_drag([120.0, 160.0]);
    assert_camera_unchanged(after_drag, shell.editor_camera);
}

#[test]
fn viewport_focus_loss_active_pan_cancels_and_releases_cursor_grab() {
    let mut shell = wheel_zoom_seed_shell();
    shell.set_viewport_cursor_grab_test_window_available(true);
    shell.cursor_pos = Some([40.0, 60.0]);
    shell.start_viewport_pan_drag(true);
    shell.update_viewport_pan_drag([72.0, 84.0]);
    let after_drag = shell.editor_camera;

    shell.handle_window_focus_change(false);

    assert!(!shell.is_viewport_orbit_drag_active());
    assert!(!shell.is_viewport_pan_drag_active());
    assert_eq!(
        shell.viewport_cursor_grab_test_events(),
        &[
            ViewportCursorGrabTestEvent::Grab,
            ViewportCursorGrabTestEvent::Release,
        ]
    );

    shell.update_viewport_pan_drag([120.0, 160.0]);
    assert_camera_unchanged(after_drag, shell.editor_camera);
}

#[test]
fn viewport_focus_loss_combined_drag_releases_once_after_both_cancel() {
    let mut shell = wheel_zoom_seed_shell();
    shell.set_viewport_cursor_grab_test_window_available(true);
    shell.cursor_pos = Some([40.0, 60.0]);
    shell.start_viewport_orbit_drag(true);
    shell.start_viewport_pan_drag(true);

    shell.handle_window_focus_change(false);

    assert!(!shell.is_viewport_orbit_drag_active());
    assert!(!shell.is_viewport_pan_drag_active());
    assert_eq!(
        shell.viewport_cursor_grab_test_events(),
        &[
            ViewportCursorGrabTestEvent::Grab,
            ViewportCursorGrabTestEvent::Grab,
            ViewportCursorGrabTestEvent::Release,
        ]
    );
}

#[test]
fn viewport_focus_loss_without_active_drag_does_not_release_cursor_grab() {
    let mut shell = wheel_zoom_seed_shell();
    shell.set_viewport_cursor_grab_test_window_available(true);

    shell.handle_window_focus_change(false);

    assert!(!shell.is_viewport_orbit_drag_active());
    assert!(!shell.is_viewport_pan_drag_active());
    assert!(shell.viewport_cursor_grab_test_events().is_empty());
}

#[test]
fn viewport_focus_loss_focused_true_preserves_active_drag_state() {
    let mut shell = wheel_zoom_seed_shell();
    shell.set_viewport_cursor_grab_test_window_available(true);
    shell.cursor_pos = Some([40.0, 60.0]);
    shell.start_viewport_orbit_drag(true);

    shell.handle_window_focus_change(true);

    assert!(shell.is_viewport_orbit_drag_active());
    assert!(!shell.is_viewport_pan_drag_active());
    assert_eq!(
        shell.viewport_cursor_grab_test_events(),
        &[ViewportCursorGrabTestEvent::Grab]
    );
}

#[test]
fn viewport_focus_loss_resets_stale_left_double_click_before_scene_frame() {
    let expected_framed = viewport_left_double_click_seed_shell().editor_camera;
    let mut shell = viewport_left_double_click_seed_shell();
    move_viewport_left_double_click_camera_off_scene(&mut shell);
    let moved = shell.editor_camera;
    assert_ne!(
        moved.eye, expected_framed.eye,
        "test setup must distinguish stale scene framing from no-op"
    );
    let first = Instant::now();

    shell.cursor_pos = Some([40.0, 60.0]);
    shell.handle_viewport_left_press(true, true, first);
    shell.handle_window_focus_change(false);
    shell.cursor_pos = Some([43.0, 64.0]);
    shell.handle_viewport_left_press(true, true, first + Duration::from_millis(250));

    assert_camera_unchanged(moved, shell.editor_camera);
}

#[test]
fn viewport_focus_loss_resets_stale_left_double_click_before_selected_cad_frame() {
    let (mut shell, origin_entity, offset_entity) = viewport_left_double_click_cad_shell();
    let scene_expected = camera_for_cad_entities(&shell, &[origin_entity]);
    let selected_expected = camera_for_cad_entities(&shell, &[offset_entity]);
    assert_ne!(
        scene_expected.target, selected_expected.target,
        "test setup must distinguish scene fallback from selected CAD framing"
    );

    shell.coord_mut().selection.add(offset_entity);
    move_viewport_left_double_click_camera_off_scene(&mut shell);
    let moved = shell.editor_camera;
    assert_ne!(
        moved.target, scene_expected.target,
        "test setup must distinguish stale scene framing from no-op"
    );
    assert_ne!(
        moved.target, selected_expected.target,
        "test setup must distinguish stale selected CAD framing from no-op"
    );
    let first = Instant::now();

    shell.cursor_pos = Some([40.0, 60.0]);
    shell.handle_viewport_left_press(true, true, first);
    shell.handle_window_focus_change(false);
    shell.cursor_pos = Some([43.0, 64.0]);
    shell.handle_viewport_left_press(true, true, first + Duration::from_millis(250));

    assert_camera_unchanged(moved, shell.editor_camera);
}

#[test]
fn viewport_cursor_left_active_orbit_cancels_and_releases_cursor_grab() {
    let mut shell = wheel_zoom_seed_shell();
    shell.set_viewport_cursor_grab_test_window_available(true);
    shell.cursor_pos = Some([40.0, 60.0]);
    shell.start_viewport_orbit_drag(true);
    shell.update_viewport_orbit_drag([72.0, 84.0]);
    let after_drag = shell.editor_camera;

    shell.handle_cursor_left();

    assert!(shell.cursor_pos.is_none());
    assert!(!shell.is_viewport_orbit_drag_active());
    assert!(!shell.is_viewport_pan_drag_active());
    assert_eq!(
        shell.viewport_cursor_grab_test_events(),
        &[
            ViewportCursorGrabTestEvent::Grab,
            ViewportCursorGrabTestEvent::Release,
        ]
    );

    shell.update_viewport_orbit_drag([120.0, 160.0]);
    assert_camera_unchanged(after_drag, shell.editor_camera);
}

#[test]
fn viewport_cursor_left_active_pan_cancels_and_releases_cursor_grab() {
    let mut shell = wheel_zoom_seed_shell();
    shell.set_viewport_cursor_grab_test_window_available(true);
    shell.cursor_pos = Some([40.0, 60.0]);
    shell.start_viewport_pan_drag(true);
    shell.update_viewport_pan_drag([72.0, 84.0]);
    let after_drag = shell.editor_camera;

    shell.handle_cursor_left();

    assert!(shell.cursor_pos.is_none());
    assert!(!shell.is_viewport_orbit_drag_active());
    assert!(!shell.is_viewport_pan_drag_active());
    assert_eq!(
        shell.viewport_cursor_grab_test_events(),
        &[
            ViewportCursorGrabTestEvent::Grab,
            ViewportCursorGrabTestEvent::Release,
        ]
    );

    shell.update_viewport_pan_drag([120.0, 160.0]);
    assert_camera_unchanged(after_drag, shell.editor_camera);
}

#[test]
fn viewport_cursor_left_combined_drag_releases_once_after_both_cancel() {
    let mut shell = wheel_zoom_seed_shell();
    shell.set_viewport_cursor_grab_test_window_available(true);
    shell.cursor_pos = Some([40.0, 60.0]);
    shell.start_viewport_orbit_drag(true);
    shell.start_viewport_pan_drag(true);

    shell.handle_cursor_left();

    assert!(shell.cursor_pos.is_none());
    assert!(!shell.is_viewport_orbit_drag_active());
    assert!(!shell.is_viewport_pan_drag_active());
    assert_eq!(
        shell.viewport_cursor_grab_test_events(),
        &[
            ViewportCursorGrabTestEvent::Grab,
            ViewportCursorGrabTestEvent::Grab,
            ViewportCursorGrabTestEvent::Release,
        ]
    );
}

#[test]
fn viewport_cursor_left_without_active_drag_does_not_release_cursor_grab() {
    let mut shell = wheel_zoom_seed_shell();
    shell.set_viewport_cursor_grab_test_window_available(true);
    shell.cursor_pos = Some([40.0, 60.0]);

    shell.handle_cursor_left();

    assert!(shell.cursor_pos.is_none());
    assert!(!shell.is_viewport_orbit_drag_active());
    assert!(!shell.is_viewport_pan_drag_active());
    assert!(shell.viewport_cursor_grab_test_events().is_empty());
}

#[test]
fn viewport_cursor_left_clears_cursor_position_without_reinstalling_one() {
    let mut shell = wheel_zoom_seed_shell();
    shell.cursor_pos = Some([40.0, 60.0]);

    shell.handle_cursor_left();
    shell.start_viewport_orbit_drag(true);

    assert!(shell.cursor_pos.is_none());
    assert!(!shell.is_viewport_orbit_drag_active());
}

#[test]
fn viewport_cursor_left_resets_stale_left_double_click() {
    let expected_framed = viewport_left_double_click_seed_shell().editor_camera;
    let mut shell = viewport_left_double_click_seed_shell();
    move_viewport_left_double_click_camera_off_scene(&mut shell);
    let moved = shell.editor_camera;
    assert_ne!(
        moved.eye, expected_framed.eye,
        "test setup must distinguish stale double-click framing from no-op"
    );
    let first = Instant::now();

    shell.cursor_pos = Some([40.0, 60.0]);
    shell.handle_viewport_left_press(true, true, first);
    shell.handle_cursor_left();
    shell.cursor_pos = Some([43.0, 64.0]);
    shell.handle_viewport_left_press(true, true, first + Duration::from_millis(250));

    assert_camera_unchanged(moved, shell.editor_camera);
}

#[test]
fn zoom_camera_degenerate_eye_target_uses_default_direction() {
    let mut shell = EditorShell::new();
    shell.editor_camera.target = glam::Vec3::new(5.0, 6.0, 7.0);
    shell.editor_camera.eye = shell.editor_camera.target;

    shell.zoom_camera_in();

    let offset = shell.editor_camera.eye - shell.editor_camera.target;
    let default_distance = (crate::camera::EditorCameraState::default().eye
        - crate::camera::EditorCameraState::default().target)
        .length();
    assert!(
        offset.is_finite() && offset.length() > 0.0,
        "degenerate zoom should produce a finite non-zero camera offset"
    );
    assert!(
        (offset.length() - (default_distance * 0.8)).abs() < 1e-5,
        "degenerate zoom should use the default camera distance before applying the zoom factor"
    );
}

#[test]
fn reset_camera_frames_cad_projection_scene() {
    // The CAD-projection arm of `current_scene_bounds` (mod.rs ~1672-1678): empty
    // prebuilt meshes + Some(cad_entity / projection / cad_world) ->
    // `projection.render_mesh_for(...)` -> `compute_aabb_union`. The prebuilt arm
    // (reset_camera_frames_prebuilt_meshes) and the `None` arm
    // (reset_camera_with_no_scene_falls_back_to_default) are covered; this arm was
    // NOT, yet it backs the user-reachable View -> Reset Camera on the CAD path.
    //
    // Use a NON-unit cuboid: a 1x1x1 origin cube frames to the default (3,3,3)
    // pose (see with_render_mesh_unit_cube_camera_matches_default_cuboid), which
    // could not separate the CAD arm from the None-fallback. A 4x2x6 cuboid frames
    // away from default, so a regression that returns `None` (silent default
    // fallback) makes the equality assertions below fail.
    use rge_cad_core::{CadGraph, CuboidOp, OperatorNode, Tolerance};
    use rge_cad_projection::{BRepHandle, CadProjection};
    use rge_kernel_ecs::World;

    // Build a single-cuboid CAD scene (mirrors render_frame_e2e_perf's
    // build_unit_cuboid_world, but 4x2x6 instead of 1x1x1).
    let mut graph = CadGraph::new();
    graph
        .begin_operation()
        .expect("CadGraph::begin_operation: no in-progress op pre-seed");
    let cuboid_node = graph
        .graph_mut()
        .expect("CadGraph::graph_mut: in-progress op was just begun")
        .add_operator(OperatorNode::Cuboid(CuboidOp {
            width: 4.0,
            height: 2.0,
            depth: 6.0,
        }))
        .expect("OperatorGraph::add_operator: 4x2x6 cuboid is content-derived NodeId-unique");
    graph
        .graph_mut()
        .expect("CadGraph::graph_mut: in-progress op still active")
        .set_root(cuboid_node)
        .expect("OperatorGraph::set_root: cuboid_node is the only root candidate");
    graph
        .commit("postaudit-reset-camera-cuboid")
        .expect("CadGraph::commit: in-progress op has a root and a valid snapshot");

    let mut projection = CadProjection::new();
    let mut world = World::new();
    world.register_snapshot_component::<BRepHandle>();
    projection
        .spawn_brep_entity(&mut world, cuboid_node)
        .expect("CadProjection::spawn_brep_entity: cuboid_node exists at the committed head");
    let tolerance = Tolerance::new(0.001).expect("Tolerance::new(0.001): finite positive");
    projection
        .tick(&mut world, &graph, tolerance)
        .expect("CadProjection::tick: graph head valid and entity registered");

    // Expected framing, computed from the SAME projection call the branch makes.
    let entity = world
        .query::<BRepHandle>()
        .next()
        .map(|(e, _)| e)
        .expect("cuboid world has exactly one BRepHandle entity");
    let mesh = projection
        .render_mesh_for(entity, &world)
        .expect("render_mesh_for: the committed cuboid projects to a mesh");
    let (min, max) = super::compute_aabb_union(std::slice::from_ref(&mesh))
        .expect("the cuboid mesh has finite, non-degenerate bounds");
    let expected = super::isometric_camera_for_bounds(min, max);
    assert_ne!(
        expected.eye,
        glam::Vec3::new(3.0, 3.0, 3.0),
        "the 4x2x6 cuboid must frame away from the default (3,3,3) pose, else this \
         test could not separate the CAD-projection arm from the None-fallback"
    );

    // CAD-projection shell: prebuilt meshes empty + CAD fields set -> the arm runs.
    let mut shell = EditorShell::with_world_projection_graph(world, projection, graph);
    // Clobber BOTH eye and target away from any framed value so each assertion is
    // load-bearing (the bbox center may be the origin, which a ZERO clobber could
    // not distinguish).
    shell.editor_camera.eye = glam::Vec3::splat(999.0);
    shell.editor_camera.target = glam::Vec3::splat(999.0);

    shell.reset_camera();

    assert_eq!(
        shell.editor_camera.target, expected.target,
        "reset_camera frames editor_camera.target to the CAD projection mesh's bbox center"
    );
    assert_eq!(
        shell.editor_camera.eye, expected.eye,
        "reset_camera frames editor_camera.eye via the CAD-projection arm (not the \
         None-fallback default)"
    );
}

#[test]
fn with_render_mesh_unit_cube_camera_matches_default_cuboid() {
    // The dispatch-G visual continuity invariant: a 1×1×1 origin-
    // centered glTF mesh should look like the default-cuboid demo on
    // screen (same camera, same shading, same materials — only the
    // mesh source differs).
    let positions: Vec<[f32; 3]> = vec![
        [-0.5, -0.5, -0.5],
        [0.5, -0.5, -0.5],
        [0.5, 0.5, -0.5],
        [-0.5, 0.5, -0.5],
        [-0.5, -0.5, 0.5],
        [0.5, -0.5, 0.5],
        [0.5, 0.5, 0.5],
        [-0.5, 0.5, 0.5],
    ];
    let indices: Vec<u32> = vec![
        0, 1, 2, 0, 2, 3, // -Z
        4, 6, 5, 4, 7, 6, // +Z
        0, 3, 7, 0, 7, 4, // -X
        1, 5, 6, 1, 6, 2, // +X
        3, 2, 6, 3, 6, 7, // +Y
        0, 4, 5, 0, 5, 1, // -Y
    ];
    let mesh = rge_brep_render::RenderMesh::from_buffers(&positions, &indices, None);
    let shell = EditorShell::with_render_mesh(mesh);
    let default_cam = crate::camera::EditorCameraState::default();
    assert!(
        (shell.editor_camera.eye - default_cam.eye).length() < 1e-4,
        "unit-cube --glb camera eye {:?} must match default-cuboid eye {:?}",
        shell.editor_camera.eye,
        default_cam.eye
    );
    assert_eq!(shell.editor_camera.target, default_cam.target);
}

#[test]
fn with_render_mesh_face_pick_no_op_when_no_projection() {
    // Render-only shells have `projection: None`. The face-pick code
    // path is guarded by exactly this; calling `handle_left_click`
    // (via the public surface that delegates to it) must be a no-op
    // — no panic, no selection change.
    let mesh = build_test_render_mesh();
    let shell = EditorShell::with_render_mesh(mesh);

    // Simulate a cursor position (the click handler reads
    // `self.cursor_pos`, normally set by `WindowEvent::CursorMoved`).
    // We can't drive that through a public setter, but we CAN verify
    // the face-pick path is unreachable by checking the projection
    // guard structurally: it returns early on `projection.is_none()`.
    // Empirical: face_selection_len stays at 0 even if the shell
    // somehow received a click (since the underlying handler bails).
    assert_eq!(shell.coord().face_selection.len(), 0);
    assert!(
        shell.projection.is_none(),
        "the projection-None guard in handle_left_click is what makes face-pick a no-op"
    );
}

// ---------------------------------------------------------------------------
// Dispatch I — `compute_aabb_union` + `with_render_meshes` (multi-mesh)
// ---------------------------------------------------------------------------

#[test]
fn compute_aabb_union_empty_slice_returns_none() {
    // No meshes → no union; caller falls back to default camera.
    let meshes: Vec<rge_brep_render::RenderMesh> = Vec::new();
    assert!(super::compute_aabb_union(&meshes).is_none());
}

#[test]
fn compute_aabb_union_single_mesh_matches_compute_aabb() {
    // Backward-compat: a Vec of one mesh must yield bounds identical
    // to what `compute_aabb` would return on that mesh's positions
    // alone. Pins the dispatch-H invariant that the single-mesh
    // wrapper produces the same camera as before.
    let positions: Vec<[f32; 3]> = vec![[0.0, 0.0, 0.0], [1.0, 1.0, 1.0]];
    let mesh = rge_brep_render::RenderMesh::from_buffers(&positions, &[0, 1, 0], None);
    let union = super::compute_aabb_union(std::slice::from_ref(&mesh)).expect("valid");
    let single = super::compute_aabb(&mesh.positions).expect("valid");
    assert_eq!(union.0, single.0);
    assert_eq!(union.1, single.1);
}

#[test]
fn compute_aabb_union_two_disjoint_meshes_spans_both() {
    // Two meshes at different positions — the union covers both.
    let p1: Vec<[f32; 3]> = vec![[0.0, 0.0, 0.0], [1.0, 1.0, 1.0]];
    let p2: Vec<[f32; 3]> = vec![[5.0, 5.0, 5.0], [10.0, 10.0, 10.0]];
    let m1 = rge_brep_render::RenderMesh::from_buffers(&p1, &[0, 1, 0], None);
    let m2 = rge_brep_render::RenderMesh::from_buffers(&p2, &[0, 1, 0], None);
    let (min, max) = super::compute_aabb_union(&[m1, m2]).expect("valid");
    assert_eq!(min, glam::Vec3::new(0.0, 0.0, 0.0));
    assert_eq!(max, glam::Vec3::new(10.0, 10.0, 10.0));
}

#[test]
fn compute_aabb_union_skips_empty_meshes() {
    // A mix of valid + empty meshes: union spans only the valid one.
    let empty_positions: Vec<[f32; 3]> = vec![];
    let valid_positions: Vec<[f32; 3]> = vec![[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]];
    let empty = rge_brep_render::RenderMesh::from_buffers(&empty_positions, &[], None);
    let valid = rge_brep_render::RenderMesh::from_buffers(&valid_positions, &[0, 1, 0], None);
    let (min, max) = super::compute_aabb_union(&[empty, valid]).expect("valid");
    assert_eq!(min, glam::Vec3::new(1.0, 2.0, 3.0));
    assert_eq!(max, glam::Vec3::new(4.0, 5.0, 6.0));
}

#[test]
fn compute_aabb_union_all_empty_returns_none() {
    // All meshes empty → no valid bounds → None (caller falls back).
    let e1: Vec<[f32; 3]> = vec![];
    let e2: Vec<[f32; 3]> = vec![];
    let m1 = rge_brep_render::RenderMesh::from_buffers(&e1, &[], None);
    let m2 = rge_brep_render::RenderMesh::from_buffers(&e2, &[], None);
    assert!(super::compute_aabb_union(&[m1, m2]).is_none());
}

#[test]
fn with_render_meshes_stores_all_meshes() {
    let m1 = rge_brep_render::RenderMesh::from_buffers(
        &[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
        &[0, 1, 2],
        None,
    );
    let m2 = rge_brep_render::RenderMesh::from_buffers(
        &[[10.0, 10.0, 10.0], [11.0, 10.0, 10.0], [10.0, 11.0, 10.0]],
        &[0, 1, 2],
        None,
    );
    let shell = EditorShell::with_render_meshes(vec![m1, m2]);
    assert_eq!(
        shell.prebuilt_render_meshes.len(),
        2,
        "multi-mesh constructor must hold all supplied RenderMeshes"
    );
}

#[test]
fn with_render_meshes_camera_targets_union_center() {
    // Two meshes at disjoint positions. The camera must target their
    // UNION center, not just the first mesh's center — otherwise the
    // second mesh sits outside the view.
    let p1: Vec<[f32; 3]> = vec![[0.0, 0.0, 0.0], [1.0, 1.0, 1.0]];
    let p2: Vec<[f32; 3]> = vec![[9.0, 9.0, 9.0], [10.0, 10.0, 10.0]];
    let m1 = rge_brep_render::RenderMesh::from_buffers(&p1, &[0, 1, 0], None);
    let m2 = rge_brep_render::RenderMesh::from_buffers(&p2, &[0, 1, 0], None);
    let shell = EditorShell::with_render_meshes(vec![m1, m2]);
    // Union: min = (0,0,0), max = (10,10,10), center = (5,5,5).
    assert_eq!(shell.editor_camera.target, glam::Vec3::new(5.0, 5.0, 5.0));
}

#[test]
fn with_render_meshes_empty_falls_back_to_default_camera() {
    // Defensive: empty Vec → no union → fall back to default camera
    // (matching dispatch-H's None-AABB fallback).
    let shell = EditorShell::with_render_meshes(vec![]);
    let default_cam = crate::camera::EditorCameraState::default();
    assert_eq!(shell.editor_camera.eye, default_cam.eye);
    assert_eq!(shell.editor_camera.target, default_cam.target);
    assert!(shell.prebuilt_render_meshes.is_empty());
}

#[test]
fn with_render_mesh_backward_compat_routes_through_multi_mesh() {
    // The dispatch-G single-mesh wrapper must produce the same shell
    // as the equivalent dispatch-I call. Pins the wrapper contract.
    let positions: Vec<[f32; 3]> = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    let indices: Vec<u32> = vec![0, 1, 2];
    let mesh = rge_brep_render::RenderMesh::from_buffers(&positions, &indices, None);
    let mesh_clone = rge_brep_render::RenderMesh::from_buffers(&positions, &indices, None);

    let single = EditorShell::with_render_mesh(mesh);
    let multi = EditorShell::with_render_meshes(vec![mesh_clone]);

    assert_eq!(single.prebuilt_render_meshes.len(), 1);
    assert_eq!(multi.prebuilt_render_meshes.len(), 1);
    assert_eq!(single.editor_camera.target, multi.editor_camera.target);
    assert_eq!(single.editor_camera.eye, multi.editor_camera.eye);
}

#[test]
fn with_render_meshes_face_pick_still_no_op_in_multi_mesh_mode() {
    // Multi-mesh mode is still render-only: face-pick has no
    // projection to query, so it silently no-ops the same way the
    // single-mesh dispatch-G path did.
    let positions: Vec<[f32; 3]> = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    let m1 = rge_brep_render::RenderMesh::from_buffers(&positions, &[0, 1, 2], None);
    let m2 = rge_brep_render::RenderMesh::from_buffers(&positions, &[0, 1, 2], None);
    let shell = EditorShell::with_render_meshes(vec![m1, m2]);
    assert!(shell.projection.is_none());
    assert_eq!(shell.coord().face_selection.len(), 0);
}

// ---------------------------------------------------------------------------
// Dispatch F — fresh shell defensive tests (pre-existing, kept after the
// dispatch-I section so they run last in the file alongside other
// defensive checks).
// ---------------------------------------------------------------------------

#[test]
fn fresh_shell_reports_pointer_not_over_viewport() {
    // Defensive — a shell that hasn't had `resumed` called has no
    // egui_host yet, so the predicate must return false (no
    // accidental face-pick from uninitialized state).
    let shell = EditorShell::new();
    assert!(!shell.is_pointer_over_viewport_tab());
}

#[test]
fn shell_with_no_cursor_pos_reports_pointer_not_over_viewport() {
    // Even after `resumed`, if no CursorMoved has fired,
    // `cursor_pos == None` → predicate returns false.
    let shell = EditorShell::new();
    assert!(shell.cursor_pos.is_none());
    assert!(!shell.is_pointer_over_viewport_tab());
}

// ---------------------------------------------------------------------------
// Dispatch K — per-mesh `base_color` parallel-Vec construction
// ---------------------------------------------------------------------------

#[test]
fn with_render_meshes_populates_white_base_colors_for_every_mesh() {
    // Backward-compat wrapper: every mesh gets the opaque white
    // default. Verifies the Vec lengths stay aligned and every entry
    // is [1, 1, 1, 1].
    let positions: Vec<[f32; 3]> = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    let m1 = rge_brep_render::RenderMesh::from_buffers(&positions, &[0, 1, 2], None);
    let m2 = rge_brep_render::RenderMesh::from_buffers(&positions, &[0, 1, 2], None);
    let shell = EditorShell::with_render_meshes(vec![m1, m2]);
    assert_eq!(shell.prebuilt_render_meshes.len(), 2);
    assert_eq!(shell.prebuilt_render_base_colors.len(), 2);
    for bc in &shell.prebuilt_render_base_colors {
        assert_eq!(*bc, [1.0, 1.0, 1.0, 1.0]);
    }
}

#[test]
fn with_render_meshes_and_base_colors_stores_supplied_colors() {
    // The dispatch-K constructor stores both vecs verbatim. Each
    // mesh-color pair lines up index-for-index so the render path
    // can bind them in lockstep.
    let positions: Vec<[f32; 3]> = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    let m1 = rge_brep_render::RenderMesh::from_buffers(&positions, &[0, 1, 2], None);
    let m2 = rge_brep_render::RenderMesh::from_buffers(&positions, &[0, 1, 2], None);
    let shell = EditorShell::with_render_meshes_and_base_colors(
        vec![m1, m2],
        vec![[0.9, 0.1, 0.1, 1.0], [0.1, 0.2, 0.9, 1.0]],
    );
    assert_eq!(shell.prebuilt_render_meshes.len(), 2);
    assert_eq!(shell.prebuilt_render_base_colors.len(), 2);
    assert_eq!(shell.prebuilt_render_base_colors[0], [0.9, 0.1, 0.1, 1.0]);
    assert_eq!(shell.prebuilt_render_base_colors[1], [0.1, 0.2, 0.9, 1.0]);
}

#[test]
fn with_render_meshes_and_base_colors_empty_pair_constructs() {
    // Both vecs empty is the boundary defensive case — the
    // length-match invariant is satisfied (0 == 0), the shell
    // constructs, and `init_render_state` will no-op on the empty
    // mesh Vec (matching the W03 "no scene attached" path).
    let shell = EditorShell::with_render_meshes_and_base_colors(vec![], vec![]);
    assert!(shell.prebuilt_render_meshes.is_empty());
    assert!(shell.prebuilt_render_base_colors.is_empty());
}

#[test]
#[should_panic(expected = "meshes (2) and base_colors (1) must have matching length")]
fn with_render_meshes_and_base_colors_panics_on_length_mismatch() {
    // Caller contract: every mesh must have exactly one base_color.
    // A mismatch is a substrate bug (the editor binary's
    // `load_all_glb_meshes` returns aligned vecs by construction),
    // so we panic with a clear message rather than silently
    // truncating or padding.
    let positions: Vec<[f32; 3]> = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    let m1 = rge_brep_render::RenderMesh::from_buffers(&positions, &[0, 1, 2], None);
    let m2 = rge_brep_render::RenderMesh::from_buffers(&positions, &[0, 1, 2], None);
    drop(EditorShell::with_render_meshes_and_base_colors(
        vec![m1, m2],
        vec![[0.9, 0.1, 0.1, 1.0]],
    ));
}

#[test]
fn with_render_mesh_single_wraps_white_via_multi_path() {
    // Sanity: the dispatch-G single-mesh wrapper now routes through
    // `with_render_meshes` (which fills white) -> through
    // `with_render_meshes_and_base_colors`. Verify the white default
    // shows up on the single-mesh path too.
    let positions: Vec<[f32; 3]> = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    let mesh = rge_brep_render::RenderMesh::from_buffers(&positions, &[0, 1, 2], None);
    let shell = EditorShell::with_render_mesh(mesh);
    assert_eq!(shell.prebuilt_render_meshes.len(), 1);
    assert_eq!(shell.prebuilt_render_base_colors.len(), 1);
    assert_eq!(shell.prebuilt_render_base_colors[0], [1.0, 1.0, 1.0, 1.0]);
}

#[test]
fn cad_path_constructors_leave_prebuilt_base_colors_empty() {
    // Sanity: the CAD `new` / `with_world` paths must NOT pollute
    // `prebuilt_render_base_colors`. Empty Vec on the CAD side
    // ensures `init_render_state_post_surface`'s CAD branch fills
    // the materials Vec with one white default rather than reading
    // a stale prebuilt sequence.
    let shell = EditorShell::new();
    assert!(shell.prebuilt_render_base_colors.is_empty());
    assert!(shell.prebuilt_render_meshes.is_empty());
}

// ---------------------------------------------------------------------------
// Dispatch M2 — per-mesh `base_color_texture` parallel-Vec construction
// ---------------------------------------------------------------------------

#[test]
fn with_render_meshes_fills_none_textures_for_every_mesh() {
    // Backward-compat: the dispatch-pre-M2 `with_render_meshes`
    // wrapper must fill `prebuilt_render_base_textures` with `None`
    // entries matching the mesh count, so the render path's
    // dispatch-M2 branch uses the `WHITE_1X1_RGBA` placeholder for
    // every mesh.
    let positions: Vec<[f32; 3]> = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    let m1 = rge_brep_render::RenderMesh::from_buffers(&positions, &[0, 1, 2], None);
    let m2 = rge_brep_render::RenderMesh::from_buffers(&positions, &[0, 1, 2], None);
    let shell = EditorShell::with_render_meshes(vec![m1, m2]);
    assert_eq!(shell.prebuilt_render_base_textures.len(), 2);
    for t in &shell.prebuilt_render_base_textures {
        assert!(t.is_none());
    }
}

#[test]
fn with_render_meshes_and_base_colors_fills_none_textures() {
    // Same wrapper-fill behaviour for the K-era constructor: it
    // routes through M2's new constructor with `None` textures.
    let positions: Vec<[f32; 3]> = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    let mesh = rge_brep_render::RenderMesh::from_buffers(&positions, &[0, 1, 2], None);
    let shell =
        EditorShell::with_render_meshes_and_base_colors(vec![mesh], vec![[0.5, 0.5, 0.5, 1.0]]);
    assert_eq!(shell.prebuilt_render_base_textures.len(), 1);
    assert!(shell.prebuilt_render_base_textures[0].is_none());
}

#[test]
fn with_render_meshes_and_base_colors_and_textures_stores_provided_pixels() {
    // The dispatch-M2 constructor stores all three parallel vecs
    // verbatim. Texture payload is a 1×1 red RGBA8.
    let positions: Vec<[f32; 3]> = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    let mesh = rge_brep_render::RenderMesh::from_buffers(&positions, &[0, 1, 2], None);
    let shell = EditorShell::with_render_meshes_and_base_colors_and_textures(
        vec![mesh],
        vec![[1.0, 1.0, 1.0, 1.0]],
        vec![Some((1, 1, vec![255, 0, 0, 255]))],
    );
    assert_eq!(shell.prebuilt_render_base_textures.len(), 1);
    let tex = shell.prebuilt_render_base_textures[0]
        .as_ref()
        .expect("stored Some");
    assert_eq!(tex.0, 1);
    assert_eq!(tex.1, 1);
    assert_eq!(tex.2, vec![255, 0, 0, 255]);
}

#[test]
#[should_panic(expected = "meshes (2) and textures (1) must have matching length")]
fn with_render_meshes_and_base_colors_and_textures_panics_on_texture_length_mismatch() {
    let positions: Vec<[f32; 3]> = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    let m1 = rge_brep_render::RenderMesh::from_buffers(&positions, &[0, 1, 2], None);
    let m2 = rge_brep_render::RenderMesh::from_buffers(&positions, &[0, 1, 2], None);
    drop(
        EditorShell::with_render_meshes_and_base_colors_and_textures(
            vec![m1, m2],
            vec![[1.0, 1.0, 1.0, 1.0]; 2],
            vec![None], // 1 texture vs 2 meshes
        ),
    );
}

#[test]
#[should_panic(expected = "meshes (2) and base_colors (1) must have matching length")]
fn with_render_meshes_and_base_colors_and_textures_panics_on_color_length_mismatch() {
    let positions: Vec<[f32; 3]> = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    let m1 = rge_brep_render::RenderMesh::from_buffers(&positions, &[0, 1, 2], None);
    let m2 = rge_brep_render::RenderMesh::from_buffers(&positions, &[0, 1, 2], None);
    drop(
        EditorShell::with_render_meshes_and_base_colors_and_textures(
            vec![m1, m2],
            vec![[1.0, 1.0, 1.0, 1.0]], // 1 color vs 2 meshes
            vec![None, None],
        ),
    );
}

#[test]
fn cad_path_constructors_leave_prebuilt_base_textures_empty() {
    let shell = EditorShell::new();
    assert!(shell.prebuilt_render_base_textures.is_empty());
}

// ---------------------------------------------------------------------------
// Asset hot-reload — substrate tests (gates only; the GPU swap-success
// path lives in `visual_smoke.rs` because it needs a real `gfx_ctx`).
// ---------------------------------------------------------------------------

/// Build a single trivial [`rge_brep_render::RenderMesh`] for the gate-only
/// tests below. Pure CPU construction — no `gfx_ctx` needed because the
/// shell's `reload_render_assets` only reaches the GPU build phase AFTER
/// the PIE and length-invariant gates fire.
fn dummy_render_mesh() -> rge_brep_render::RenderMesh {
    let positions: Vec<[f32; 3]> = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
    rge_brep_render::RenderMesh::from_buffers(&positions, &[0, 1, 2], None)
}

#[test]
fn reload_render_assets_rejects_color_length_mismatch() {
    let mut s = EditorShell::new();
    let result = s.reload_render_assets(
        vec![dummy_render_mesh()],
        vec![[1.0, 1.0, 1.0, 1.0], [0.5, 0.5, 0.5, 1.0]], // 2 vs 1
        vec![None],
    );
    let msg = result.expect_err("mismatched lengths must return Err");
    assert!(
        msg.contains("base_colors") && msg.contains("length mismatch"),
        "err message should mention base_colors length mismatch; got: {msg}"
    );
}

#[test]
fn reload_render_assets_rejects_texture_length_mismatch() {
    let mut s = EditorShell::new();
    let result = s.reload_render_assets(
        vec![dummy_render_mesh()],
        vec![[1.0, 1.0, 1.0, 1.0]],
        vec![None, None], // 2 vs 1
    );
    let msg = result.expect_err("mismatched lengths must return Err");
    assert!(
        msg.contains("textures") && msg.contains("length mismatch"),
        "err message should mention textures length mismatch; got: {msg}"
    );
}

#[test]
fn reload_render_assets_rejects_empty_mesh_set() {
    let mut s = EditorShell::new();
    let result = s.reload_render_assets(vec![], vec![], vec![]);
    let msg = result.expect_err("empty inputs must return Err");
    assert!(
        msg.contains("empty mesh set"),
        "err message should mention empty mesh set; got: {msg}"
    );
}

#[test]
fn reload_render_assets_rejected_in_playing_state() {
    let mut s = EditorShell::new();
    // Need at least one entity for `Play` to capture a snapshot.
    let e = s.world_mut().spawn();
    s.world_mut()
        .insert_component(e, ComponentTypeId(2), vec![0u8; 12]);
    s.handle_button(ToolbarButtonId::Play)
        .expect("Play transition from Editing");
    assert_eq!(s.play_state(), PlayState::Playing);

    let result = s.reload_render_assets(
        vec![dummy_render_mesh()],
        vec![[1.0, 1.0, 1.0, 1.0]],
        vec![None],
    );
    let msg = result.expect_err("PIE active: reload must return Err");
    assert!(
        msg.contains("Playing") && msg.contains("Editing"),
        "err message should mention Playing vs Editing; got: {msg}"
    );
}

#[test]
fn reload_render_assets_rejected_in_paused_state() {
    let mut s = EditorShell::new();
    let e = s.world_mut().spawn();
    s.world_mut()
        .insert_component(e, ComponentTypeId(2), vec![0u8; 12]);
    s.handle_button(ToolbarButtonId::Play).expect("Play");
    s.handle_button(ToolbarButtonId::Pause).expect("Pause");
    assert_eq!(s.play_state(), PlayState::Paused);

    let result = s.reload_render_assets(
        vec![dummy_render_mesh()],
        vec![[1.0, 1.0, 1.0, 1.0]],
        vec![None],
    );
    let msg = result.expect_err("Paused: reload must return Err");
    assert!(
        msg.contains("Paused") && msg.contains("Editing"),
        "err message should mention Paused vs Editing; got: {msg}"
    );
}

#[test]
fn reload_render_assets_rejected_before_render_init() {
    // Editing + aligned lengths but no `gfx_ctx` (no
    // `init_render_state_headless` ran) → "render state not
    // initialized" error path. Verifies the order of checks: PIE +
    // length pass, GPU check fails.
    let mut s = EditorShell::new();
    let result = s.reload_render_assets(
        vec![dummy_render_mesh()],
        vec![[1.0, 1.0, 1.0, 1.0]],
        vec![None],
    );
    let msg = result.expect_err("no gfx_ctx: reload must return Err");
    assert!(
        msg.contains("render state not initialized"),
        "err message should mention render state not initialized; got: {msg}"
    );
}

#[test]
fn handle_asset_reload_with_no_source_path_is_noop() {
    // R-key from a default cuboid-demo shell (no glb_source_path, no
    // reload_hook) must silently warn-log + no-op. Field state is
    // unchanged after the call.
    let mut s = EditorShell::new();
    assert!(s.glb_source_path.is_none());
    assert!(s.reload_hook.is_none());
    s.handle_asset_reload();
    // Still no path / hook, still in Editing, no panic.
    assert!(s.glb_source_path.is_none());
    assert!(s.reload_hook.is_none());
    assert_eq!(s.play_state(), PlayState::Editing);
}

#[test]
fn attach_glb_reload_source_stashes_path_and_hook() {
    struct StubHook;
    impl crate::AssetReloadHook for StubHook {
        fn reload_glb(
            &self,
            _path: &std::path::Path,
        ) -> Result<
            (
                Vec<rge_brep_render::RenderMesh>,
                Vec<[f32; 4]>,
                Vec<Option<(u32, u32, Vec<u8>)>>,
            ),
            String,
        > {
            Err("stub: never called by this test".into())
        }
    }
    let mut s = EditorShell::new();
    assert!(s.glb_source_path.is_none());
    assert!(s.reload_hook.is_none());
    s.attach_glb_reload_source(std::path::PathBuf::from("/tmp/test.glb"), StubHook);
    assert_eq!(
        s.glb_source_path.as_ref().map(|p| p.as_path()),
        Some(std::path::Path::new("/tmp/test.glb"))
    );
    assert!(s.reload_hook.is_some());
}

#[test]
fn handle_asset_reload_surfaces_hook_error_as_warn_and_retains_state() {
    // Hook returns Err → handler warn-logs and no-ops. The shell's
    // pre-reload state (prebuilt_render_meshes empty here, since
    // we constructed via `new()`) is unchanged.
    struct FailingHook;
    impl crate::AssetReloadHook for FailingHook {
        fn reload_glb(
            &self,
            _path: &std::path::Path,
        ) -> Result<
            (
                Vec<rge_brep_render::RenderMesh>,
                Vec<[f32; 4]>,
                Vec<Option<(u32, u32, Vec<u8>)>>,
            ),
            String,
        > {
            Err("simulated parse failure".into())
        }
    }
    let mut s = EditorShell::new();
    s.attach_glb_reload_source(std::path::PathBuf::from("/tmp/fake.glb"), FailingHook);
    let before = s.prebuilt_render_meshes.len();
    s.handle_asset_reload();
    // No panic; field state preserved.
    assert_eq!(s.prebuilt_render_meshes.len(), before);
    assert!(s.meshes.is_empty(), "no GPU upload happened");
}

// ---------------------------------------------------------------------------
// In-app GLB Open (Ctrl+O, the `.glb` branch) — commit-after-success
// ordering (gate tests).
//
// These cover the NON-success paths (cancel, failing candidate, missing
// dialog) headlessly — no `gfx_ctx` is reached because the handler returns
// before `reload_render_assets`. The GPU swap-SUCCESS path (open commits
// the path only after the swap succeeds) lives in
// `rge-editor/src/main.rs`'s GPU-guarded end-to-end suite, alongside the
// analogous R-key success test, because it needs a real `gfx_ctx` + the
// real glTF loader + a fixture.
// ---------------------------------------------------------------------------

/// Mock [`GlbOpenDialog`] returning a fixed, pre-configured result so
/// the open handler can be driven without a native dialog.
struct MockOpenDialog {
    result: Option<std::path::PathBuf>,
}

impl crate::GlbOpenDialog for MockOpenDialog {
    fn pick_glb_path(&self) -> Option<std::path::PathBuf> {
        self.result.clone()
    }
}

/// Mock [`AssetReloadHook`] that always fails — used to drive the
/// failing-candidate path of [`EditorShell::handle_open_request`]
/// without a malformed file on disk.
struct AlwaysFailHook;

impl crate::AssetReloadHook for AlwaysFailHook {
    fn reload_glb(
        &self,
        _path: &std::path::Path,
    ) -> Result<
        (
            Vec<rge_brep_render::RenderMesh>,
            Vec<[f32; 4]>,
            Vec<Option<(u32, u32, Vec<u8>)>>,
        ),
        String,
    > {
        Err("simulated open: parse failure".into())
    }
}

#[test]
fn open_request_cancel_mutates_nothing() {
    // Test C — dialog returns `None` (user cancelled). The handler
    // info-logs and returns BEFORE touching the loader hook or any
    // render state: glb_source_path stays as it was, no GPU upload.
    let mut s = EditorShell::new().with_glb_open_dialog(Box::new(MockOpenDialog { result: None }));
    // A loader hook is present so we prove it is the cancel — not a
    // missing hook — that no-ops (cancel is checked before the hook).
    s.attach_glb_loader_hook(AlwaysFailHook);
    // attach_glb_loader_hook leaves glb_source_path untouched.
    assert!(s.glb_source_path().is_none());

    s.handle_open_request();

    assert!(
        s.glb_source_path().is_none(),
        "cancelled Open must not commit any path"
    );
    assert!(s.meshes.is_empty(), "cancelled Open must not upload meshes");
    assert_eq!(s.play_state(), PlayState::Editing);
}

#[test]
fn open_request_failing_candidate_leaves_source_path_unchanged() {
    // Test A — dialog returns a candidate, but the loader rejects it
    // (malformed). glb_source_path must remain its PRIOR value (the
    // last good file), NOT the rejected candidate — the previous frame
    // is retained. This is the commit-after-success safety property.
    let prior = std::path::PathBuf::from("/tmp/prior_good.glb");
    let mut s = EditorShell::new().with_glb_open_dialog(Box::new(MockOpenDialog {
        result: Some(std::path::PathBuf::from("/tmp/freshly_picked_but_bad.glb")),
    }));
    // Seed a prior good source path + a failing hook. (The hook fails
    // for BOTH R-key and Open here; we only exercise Open.)
    s.attach_glb_reload_source(prior.clone(), AlwaysFailHook);
    assert_eq!(s.glb_source_path(), Some(prior.as_path()));
    let meshes_before = s.prebuilt_render_meshes.len();

    s.handle_open_request();

    // The rejected candidate must NOT have been committed.
    assert_eq!(
        s.glb_source_path(),
        Some(prior.as_path()),
        "a failing Open must leave glb_source_path at the prior good path, not the rejected candidate"
    );
    assert_eq!(
        s.prebuilt_render_meshes.len(),
        meshes_before,
        "failing Open must not mutate prebuilt meshes"
    );
    assert!(s.meshes.is_empty(), "failing Open must not upload meshes");
}

#[test]
fn open_request_with_no_dialog_is_noop() {
    // Defensive — no dialog attached (the binary always attaches one,
    // but headless construction does not). Ctrl+O warn-logs and
    // no-ops; no path committed even though a loader hook is present.
    let mut s = EditorShell::new();
    s.attach_glb_loader_hook(AlwaysFailHook);
    assert!(s.open_dialog.is_none());
    assert!(s.glb_source_path().is_none());

    s.handle_open_request();

    assert!(s.glb_source_path().is_none());
    assert!(s.meshes.is_empty());
}

#[test]
fn open_request_outside_editing_is_noop() {
    // PIE gate — Open only fires in Editing (mirrors the R-key gate).
    // Drive the shell into Playing, then assert Ctrl+O no-ops without
    // committing the candidate.
    let mut s = EditorShell::new().with_glb_open_dialog(Box::new(MockOpenDialog {
        result: Some(std::path::PathBuf::from("/tmp/should_not_commit.glb")),
    }));
    s.attach_glb_loader_hook(AlwaysFailHook);
    // Need an entity for Play to capture a snapshot.
    let e = s.world_mut().spawn();
    s.world_mut()
        .insert_component(e, ComponentTypeId(2), vec![0u8; 12]);
    s.handle_button(ToolbarButtonId::Play)
        .expect("Play transition from Editing");
    assert_eq!(s.play_state(), PlayState::Playing);

    s.handle_open_request();

    assert!(
        s.glb_source_path().is_none(),
        "Open during PIE must not commit a path"
    );
    assert!(s.meshes.is_empty());
}

// ---------------------------------------------------------------------------
// EDITOR-WORLD-SWAP — runtime `replace_world` substrate
// ---------------------------------------------------------------------------

#[test]
fn replace_world_installs_new_world_and_stays_non_cad() {
    // Swap from a default (`with_world`) shell: the live world reflects the
    // swapped-in kernel world, and the shell stays in non-CAD / blank-render
    // mode.
    let mut s = EditorShell::new();
    let mut next = rge_kernel_ecs::World::new();
    next.spawn();
    next.spawn();
    next.spawn();
    s.replace_world(next)
        .expect("world swap allowed in Editing");
    assert_eq!(
        s.world().kernel().entity_count(),
        3,
        "the live world must reflect the swapped-in kernel world"
    );
    assert!(s.cad_world.is_none());
    assert!(s.projection.is_none());
    assert!(s.prebuilt_render_meshes.is_empty());
    assert!(s.meshes.is_empty());
}

#[test]
fn replace_world_from_cad_mode_clears_cad_fields() {
    // White-box: force the CAD-mode fields `Some` (constructing a full
    // `with_world_projection_graph` cuboid is disproportionate for a unit
    // test, and `replace_world` clears all four CAD fields with one
    // unconditional reset). Assert every CAD field is `None` afterward.
    let mut s = EditorShell::new();
    s.cad_world = Some(rge_kernel_ecs::World::new());
    let dummy_entity = s.cad_world.as_mut().unwrap().spawn();
    s.cad_entity = Some(dummy_entity);
    let next = rge_kernel_ecs::World::new();
    s.replace_world(next)
        .expect("world swap allowed in Editing");
    assert!(s.cad_world.is_none(), "cad_world must clear");
    assert!(s.cad_entity.is_none(), "cad_entity must clear");
    assert!(s.projection.is_none(), "projection must stay clear");
    assert!(s.cad_graph.is_none(), "cad_graph must stay clear");
}

#[test]
fn replace_world_from_render_mesh_mode_blanks_viewport() {
    // Swap from a glTF render-only shell: the prebuilt render content is
    // cleared so the viewport renders blank (the v0 `--scene` semantics).
    let mesh = build_test_render_mesh();
    let mut s = EditorShell::with_render_mesh(mesh);
    assert_eq!(
        s.prebuilt_render_meshes.len(),
        1,
        "precondition: 1 prebuilt mesh"
    );
    let next = rge_kernel_ecs::World::new();
    s.replace_world(next)
        .expect("world swap allowed in Editing");
    assert!(
        s.prebuilt_render_meshes.is_empty(),
        "prebuilt meshes cleared"
    );
    assert!(
        s.prebuilt_render_base_colors.is_empty(),
        "base colors cleared"
    );
    assert!(
        s.prebuilt_render_base_textures.is_empty(),
        "textures cleared"
    );
    assert!(s.meshes.is_empty(), "GPU meshes cleared");
    assert!(s.materials.is_empty(), "GPU materials cleared");
}

#[test]
fn replace_world_is_rejected_outside_editing() {
    // Editing-only gate: a swap during Play must be a no-op error.
    let mut s = EditorShell::new();
    build_scene(&mut s, 4);
    s.handle_button(ToolbarButtonId::Play).expect("enter Play");
    assert_eq!(s.play_state(), PlayState::Playing);
    let before = s.world().kernel().entity_count();
    let mut next = rge_kernel_ecs::World::new();
    next.spawn();
    let result = s.replace_world(next);
    assert!(result.is_err(), "world swap outside Editing must error");
    assert_eq!(
        s.play_state(),
        PlayState::Playing,
        "state unchanged on the error path"
    );
    assert_eq!(
        s.world().kernel().entity_count(),
        before,
        "the live world must be untouched on the error path"
    );
}

#[test]
fn replace_world_resets_command_bus() {
    // A dirty undo/redo stack + dirty flag from the old world must NOT
    // survive a swap, or an old-world undo could replay against the new
    // kernel world.
    let mut s = EditorShell::new();
    s.set_time_scale(2.0);
    assert!(
        s.inspector_snapshot().is_dirty,
        "precondition: a command dirtied the bus"
    );
    let next = rge_kernel_ecs::World::new();
    s.replace_world(next)
        .expect("world swap allowed in Editing");
    let snap = s.inspector_snapshot();
    assert!(
        !snap.is_dirty,
        "replace_world must install a fresh, clean CommandBus"
    );
    assert_eq!(
        snap.undo_stack_len, 0,
        "undo stack must be empty after swap"
    );
    assert_eq!(snap.undo_cursor, 0, "undo cursor must reset after swap");
}

#[test]
fn replace_world_clears_glb_source_but_keeps_hooks() {
    // The swapped-in world has no GLB hot-reload source, so the source
    // pointer clears; the loader hook is preserved so Ctrl+O / R-key stay
    // wired (the binary tears the watcher down off the now-`None` source).
    let mut s = EditorShell::new();
    s.attach_glb_reload_source(
        std::path::PathBuf::from("/tmp/world-swap.glb"),
        AlwaysFailHook,
    );
    assert!(
        s.glb_source_path().is_some(),
        "precondition: a GLB source is attached"
    );
    let next = rge_kernel_ecs::World::new();
    s.replace_world(next)
        .expect("world swap allowed in Editing");
    assert!(
        s.glb_source_path().is_none(),
        "replace_world must clear the GLB source pointer"
    );
    assert!(
        s.reload_hook.is_some(),
        "loader hook must be preserved across the swap"
    );
}

// ---------------------------------------------------------------------------
// SCENE-OPEN-WIRING — Ctrl+O scene Open (`.rge-scene` / `.rge-project`)
// ---------------------------------------------------------------------------

/// Mock [`crate::SceneOpenHook`] returning a pre-configured result so the
/// scene-open branch of [`EditorShell::handle_open_request`] can be
/// driven without a real `.rge-scene` file on disk. `entity_count` sets
/// how many entities the returned world has (so a test can observe the
/// swap); `fail` makes the hook return `Err`, exercising the no-op
/// failure path.
struct MockSceneOpenHook {
    entity_count: usize,
    fail: bool,
}

impl crate::SceneOpenHook for MockSceneOpenHook {
    fn load_scene_world(&self, _path: &std::path::Path) -> Result<rge_kernel_ecs::World, String> {
        if self.fail {
            return Err("simulated scene load failure".into());
        }
        let mut world = rge_kernel_ecs::World::new();
        for _ in 0..self.entity_count {
            world.spawn();
        }
        Ok(world)
    }
}

/// Mock [`crate::SceneOpenHook`] that ALSO supplies a project display name, so
/// the `.rge-project` open path (`handle_open_request` → `project_display_name`
/// → `SaveSource::Project { name }`) can be exercised end-to-end. `entity_count`
/// sizes the swapped-in world; `display_name` is returned verbatim from
/// `project_display_name` — in the production binary that override delegates to
/// `rge_scene_loader::read_project_name`.
struct NamingSceneOpenHook {
    entity_count: usize,
    display_name: Option<String>,
}

impl crate::SceneOpenHook for NamingSceneOpenHook {
    fn load_scene_world(&self, _path: &std::path::Path) -> Result<rge_kernel_ecs::World, String> {
        let mut world = rge_kernel_ecs::World::new();
        for _ in 0..self.entity_count {
            world.spawn();
        }
        Ok(world)
    }

    fn project_display_name(&self, _path: &std::path::Path) -> Option<String> {
        self.display_name.clone()
    }
}

#[test]
fn scene_open_swaps_world_and_clears_glb_source() {
    // Dialog returns a `.rge-scene`; the scene hook yields a 2-entity
    // world. handle_open_request must load-then-swap: the live world
    // reflects the 2 entities and `glb_source_path` is cleared by
    // `replace_world` (proving the swap ran, not just the load).
    let mut s = EditorShell::new()
        .with_glb_open_dialog(Box::new(MockOpenDialog {
            result: Some(std::path::PathBuf::from("/tmp/level.rge-scene")),
        }))
        .with_scene_open_hook(Box::new(MockSceneOpenHook {
            entity_count: 2,
            fail: false,
        }));
    // Seed a prior GLB source so the test can prove the scene Open clears it.
    s.attach_glb_reload_source(std::path::PathBuf::from("/tmp/prior.glb"), AlwaysFailHook);
    assert!(
        s.glb_source_path().is_some(),
        "precondition: a GLB source is attached"
    );

    s.handle_open_request();

    assert_eq!(
        s.world().kernel().entity_count(),
        2,
        "scene Open must swap in the hook's world"
    );
    assert!(
        s.glb_source_path().is_none(),
        "scene Open must clear glb_source_path via replace_world"
    );
    assert!(s.cad_world.is_none(), "scene Open stays in non-CAD mode");
}

#[test]
fn scene_open_failure_leaves_world_and_source_unchanged() {
    // The scene hook returns Err (malformed scene). handle_open_request
    // must warn + no-op: the live world is untouched and the prior GLB
    // source survives (replace_world is never reached). This is the scene
    // analogue of the GLB commit-after-success property.
    let mut s = EditorShell::new()
        .with_glb_open_dialog(Box::new(MockOpenDialog {
            result: Some(std::path::PathBuf::from("/tmp/broken.rge-scene")),
        }))
        .with_scene_open_hook(Box::new(MockSceneOpenHook {
            entity_count: 3,
            fail: true,
        }));
    build_scene(&mut s, 5);
    let prior = std::path::PathBuf::from("/tmp/prior_good.glb");
    s.attach_glb_reload_source(prior.clone(), AlwaysFailHook);
    let before = s.world().kernel().entity_count();

    s.handle_open_request();

    assert_eq!(
        s.world().kernel().entity_count(),
        before,
        "a failing scene load must not swap the live world"
    );
    assert_eq!(
        s.glb_source_path(),
        Some(prior.as_path()),
        "a failing scene load must leave glb_source_path unchanged"
    );
}

#[test]
fn open_unsupported_extension_is_noop() {
    // A picked path that is neither `.glb` nor a scene → warn + no-op:
    // no world swap (the scene hook would have produced 9 entities), no
    // source commit, no mesh upload.
    let mut s = EditorShell::new()
        .with_glb_open_dialog(Box::new(MockOpenDialog {
            result: Some(std::path::PathBuf::from("/tmp/notes.txt")),
        }))
        .with_scene_open_hook(Box::new(MockSceneOpenHook {
            entity_count: 9,
            fail: false,
        }));
    s.attach_glb_loader_hook(AlwaysFailHook);
    let before = s.world().kernel().entity_count();

    s.handle_open_request();

    assert_eq!(
        s.world().kernel().entity_count(),
        before,
        "unsupported extension must not swap the world"
    );
    assert!(
        s.glb_source_path().is_none(),
        "unsupported extension commits no source"
    );
    assert!(
        s.meshes.is_empty(),
        "unsupported extension uploads no meshes"
    );
}

#[test]
fn scene_open_without_hook_is_noop() {
    // `.rge-scene` picked but no scene_open_hook attached (e.g. headless
    // construction): warn + no-op, the live world untouched.
    let mut s = EditorShell::new().with_glb_open_dialog(Box::new(MockOpenDialog {
        result: Some(std::path::PathBuf::from("/tmp/level.rge-scene")),
    }));
    build_scene(&mut s, 3);
    let before = s.world().kernel().entity_count();
    assert!(s.scene_open_hook.is_none(), "precondition: no scene hook");

    s.handle_open_request();

    assert_eq!(
        s.world().kernel().entity_count(),
        before,
        "scene Open with no hook must not swap the world"
    );
}

#[test]
fn scene_open_outside_editing_is_noop() {
    // PIE gate — a scene Open during Play must no-op (mirrors the GLB
    // gate). handle_open_request returns at the PIE check before the
    // dialog or hook is consulted.
    let mut s = EditorShell::new()
        .with_glb_open_dialog(Box::new(MockOpenDialog {
            result: Some(std::path::PathBuf::from("/tmp/level.rge-scene")),
        }))
        .with_scene_open_hook(Box::new(MockSceneOpenHook {
            entity_count: 7,
            fail: false,
        }));
    build_scene(&mut s, 2);
    s.handle_button(ToolbarButtonId::Play)
        .expect("Play transition from Editing");
    assert_eq!(s.play_state(), PlayState::Playing);
    let before = s.world().kernel().entity_count();

    s.handle_open_request();

    assert_eq!(
        s.play_state(),
        PlayState::Playing,
        "state unchanged on the gated path"
    );
    assert_eq!(
        s.world().kernel().entity_count(),
        before,
        "scene Open during PIE must not swap the world"
    );
}

#[test]
fn scene_open_preserves_scene_hook_for_a_second_open() {
    // The scene hook must survive a successful scene Open (replace_world
    // preserves it), so a second scene Open still works. Assert the hook
    // is still present after the first swap AND drive a second Open.
    let mut s = EditorShell::new()
        .with_glb_open_dialog(Box::new(MockOpenDialog {
            result: Some(std::path::PathBuf::from("/tmp/level.rge-scene")),
        }))
        .with_scene_open_hook(Box::new(MockSceneOpenHook {
            entity_count: 2,
            fail: false,
        }));

    s.handle_open_request();
    assert_eq!(
        s.world().kernel().entity_count(),
        2,
        "first scene Open swaps"
    );
    assert!(
        s.scene_open_hook.is_some(),
        "replace_world must preserve the scene hook"
    );

    // A second scene Open must still route through the preserved hook.
    s.handle_open_request();
    assert_eq!(
        s.world().kernel().entity_count(),
        2,
        "second scene Open still swaps (the hook survived the first swap)"
    );
}

#[test]
fn scene_open_accepts_literal_rge_project() {
    // The literal extensionless `.rge-project` path (no `Path::extension()`)
    // must route to the scene branch — this is the file-name case the
    // dialog's All-Files filter exists to make pickable (OQ2). Dialog
    // returns a `.rge-project`; the scene hook yields a 2-entity world;
    // the swap runs and clears the seeded `glb_source_path`.
    let mut s = EditorShell::new()
        .with_glb_open_dialog(Box::new(MockOpenDialog {
            result: Some(std::path::PathBuf::from("/tmp/.rge-project")),
        }))
        .with_scene_open_hook(Box::new(MockSceneOpenHook {
            entity_count: 2,
            fail: false,
        }));
    s.attach_glb_reload_source(std::path::PathBuf::from("/tmp/prior.glb"), AlwaysFailHook);

    s.handle_open_request();

    assert_eq!(
        s.world().kernel().entity_count(),
        2,
        "literal .rge-project must route to the scene branch and swap the world"
    );
    assert!(
        s.glb_source_path().is_none(),
        "literal .rge-project scene Open must clear glb_source_path"
    );
}

// ---------------------------------------------------------------------------
// SCENE-SAVE-WIRING — Ctrl+S in-app Save (Save-As, `.rge-scene`)
// ---------------------------------------------------------------------------

/// Mock [`crate::SceneSaveDialog`] returning a fixed result so the save handler
/// can be driven without a native dialog (`None` simulates cancel).
struct MockSaveDialog {
    result: Option<std::path::PathBuf>,
}

impl crate::SceneSaveDialog for MockSaveDialog {
    fn pick_save_path(&self) -> Option<std::path::PathBuf> {
        self.result.clone()
    }
}

/// Mock [`crate::SceneSaveHook`] recording its invocation count through a shared
/// `Rc<Cell<usize>>` the test retains, returning `Ok`/`Err` per `fail`. (The
/// real `.rge-scene` disk write is covered by `rge-scene-loader`'s own
/// round-trip tests and the binary `SceneSaveWriterHook` test; here we only
/// prove the handler's wiring + the mark-saved-on-success contract.)
struct MockSaveHook {
    fail: bool,
    calls: std::rc::Rc<std::cell::Cell<usize>>,
}

impl crate::SceneSaveHook for MockSaveHook {
    fn save_scene_world(
        &self,
        _world: &rge_kernel_ecs::World,
        _path: &std::path::Path,
    ) -> Result<(), String> {
        self.calls.set(self.calls.get() + 1);
        if self.fail {
            Err("simulated scene save failure".into())
        } else {
            Ok(())
        }
    }
}

/// Build a [`MockSaveHook`] plus the shared call-counter handle the test keeps.
fn save_hook(fail: bool) -> (MockSaveHook, std::rc::Rc<std::cell::Cell<usize>>) {
    let calls = std::rc::Rc::new(std::cell::Cell::new(0usize));
    (
        MockSaveHook {
            fail,
            calls: std::rc::Rc::clone(&calls),
        },
        calls,
    )
}

#[test]
fn save_writes_via_hook_and_marks_saved() {
    // Dirty shell + dialog returns a `.rge-scene` + writer returns Ok:
    // handle_save_request must invoke the writer once AND mark the bus saved
    // (is_dirty cleared) — the new Ctrl+S = Save behavior.
    let (hook, calls) = save_hook(false);
    let mut s = EditorShell::new()
        .with_scene_save_dialog(Box::new(MockSaveDialog {
            result: Some(std::path::PathBuf::from("/tmp/level.rge-scene")),
        }))
        .with_scene_save_hook(Box::new(hook));
    s.set_time_scale(2.0);
    assert!(
        s.command_bus().is_dirty(),
        "precondition: the bus is dirty before Save"
    );

    s.handle_save_request();

    assert_eq!(
        calls.get(),
        1,
        "Save must invoke the writer hook exactly once"
    );
    assert!(
        !s.command_bus().is_dirty(),
        "a successful Save must mark the bus saved (is_dirty cleared)"
    );
}

#[test]
fn save_failure_does_not_mark_saved() {
    // Writer returns Err: the writer IS invoked, but the bus must NOT be marked
    // saved (is_dirty stays true).
    let (hook, calls) = save_hook(true);
    let mut s = EditorShell::new()
        .with_scene_save_dialog(Box::new(MockSaveDialog {
            result: Some(std::path::PathBuf::from("/tmp/level.rge-scene")),
        }))
        .with_scene_save_hook(Box::new(hook));
    s.set_time_scale(2.0);
    assert!(s.command_bus().is_dirty());

    s.handle_save_request();

    assert_eq!(calls.get(), 1, "the writer hook must have been invoked");
    assert!(
        s.command_bus().is_dirty(),
        "a failed Save must NOT mark the bus saved"
    );
}

#[test]
fn save_cancelled_dialog_is_noop() {
    // Dialog returns None (cancelled): the writer is never reached and the bus
    // saved-point is untouched (is_dirty stays true).
    let (hook, calls) = save_hook(false);
    let mut s = EditorShell::new()
        .with_scene_save_dialog(Box::new(MockSaveDialog { result: None }))
        .with_scene_save_hook(Box::new(hook));
    s.set_time_scale(2.0);
    assert!(s.command_bus().is_dirty());

    s.handle_save_request();

    assert_eq!(calls.get(), 0, "a cancelled Save must not reach the writer");
    assert!(
        s.command_bus().is_dirty(),
        "a cancelled Save must leave the bus dirty"
    );
}

#[test]
fn save_without_dialog_is_noop() {
    // No dialog attached (headless construction): Ctrl+S warn-logs and no-ops;
    // the writer is never reached and the bus stays dirty.
    let (hook, calls) = save_hook(false);
    let mut s = EditorShell::new().with_scene_save_hook(Box::new(hook));
    s.set_time_scale(2.0);
    assert!(s.save_dialog.is_none(), "precondition: no save dialog");
    assert!(s.command_bus().is_dirty());

    s.handle_save_request();

    assert_eq!(calls.get(), 0, "no dialog -> the writer is never reached");
    assert!(
        s.command_bus().is_dirty(),
        "a no-dialog Save must leave the bus dirty"
    );
}

#[test]
fn save_without_hook_is_noop() {
    // Dialog returns a path but no writer attached: warn + no-op; the bus stays
    // dirty (no mark_saved).
    let mut s = EditorShell::new().with_scene_save_dialog(Box::new(MockSaveDialog {
        result: Some(std::path::PathBuf::from("/tmp/level.rge-scene")),
    }));
    s.set_time_scale(2.0);
    assert!(s.scene_save_hook.is_none(), "precondition: no save hook");
    assert!(s.command_bus().is_dirty());

    s.handle_save_request();

    assert!(
        s.command_bus().is_dirty(),
        "a no-writer Save must leave the bus dirty"
    );
}

#[test]
fn save_outside_editing_is_noop() {
    // PIE gate — Save only fires in Editing (mirrors the Ctrl+O gate). During
    // Play the handler returns at the PIE check before the dialog or writer is
    // consulted.
    let (hook, calls) = save_hook(false);
    let mut s = EditorShell::new()
        .with_scene_save_dialog(Box::new(MockSaveDialog {
            result: Some(std::path::PathBuf::from("/tmp/level.rge-scene")),
        }))
        .with_scene_save_hook(Box::new(hook));
    build_scene(&mut s, 2);
    s.handle_button(ToolbarButtonId::Play)
        .expect("Play transition from Editing");
    assert_eq!(s.play_state(), PlayState::Playing);

    s.handle_save_request();

    assert_eq!(calls.get(), 0, "Save during PIE must not reach the writer");
    assert_eq!(
        s.play_state(),
        PlayState::Playing,
        "state unchanged on the gated path"
    );
}

#[test]
fn ctrl_s_routes_to_save() {
    // Ctrl+S resolves through the canonical menu to Command::Save ->
    // route_menu_command, which must drive the full Save flow (writer invoked +
    // bus marked saved), not a bare mark_saved.
    let (hook, calls) = save_hook(false);
    let mut s = EditorShell::new()
        .with_scene_save_dialog(Box::new(MockSaveDialog {
            result: Some(std::path::PathBuf::from("/tmp/level.rge-scene")),
        }))
        .with_scene_save_hook(Box::new(hook));
    s.set_time_scale(2.0);
    assert!(s.command_bus().is_dirty());

    s.route_menu_command(rge_editor_ui::menus::Command::Save);

    assert_eq!(
        calls.get(),
        1,
        "Ctrl+S (Save) must route through handle_save_request to the writer"
    );
    assert!(
        !s.command_bus().is_dirty(),
        "Ctrl+S Save must mark the bus saved on success"
    );
}

// ---------------------------------------------------------------------------
// SCENE-SAVE-SOURCE-PATH — true Save (silent overwrite of the opened scene)
// ---------------------------------------------------------------------------

/// Mock [`crate::SceneSaveHook`] recording its call count + the last path it
/// received (via shared handles the test retains), returning `Ok`. Used to
/// prove the silent path writes to the tracked source.
struct RecordingSaveHook {
    calls: std::rc::Rc<std::cell::Cell<usize>>,
    last_path: std::rc::Rc<std::cell::RefCell<Option<std::path::PathBuf>>>,
}

impl crate::SceneSaveHook for RecordingSaveHook {
    fn save_scene_world(
        &self,
        _world: &rge_kernel_ecs::World,
        path: &std::path::Path,
    ) -> Result<(), String> {
        self.calls.set(self.calls.get() + 1);
        *self.last_path.borrow_mut() = Some(path.to_path_buf());
        Ok(())
    }
}

#[test]
fn scene_open_commits_scene_save_source() {
    // A successful `.rge-scene` Open commits a `SaveSource::Scene`.
    let mut s = EditorShell::new()
        .with_glb_open_dialog(Box::new(MockOpenDialog {
            result: Some(std::path::PathBuf::from("/tmp/level.rge-scene")),
        }))
        .with_scene_open_hook(Box::new(MockSceneOpenHook {
            entity_count: 2,
            fail: false,
        }));

    s.handle_open_request();

    assert_eq!(
        s.save_source(),
        Some(&SaveSource::Scene(std::path::PathBuf::from(
            "/tmp/level.rge-scene"
        ))),
        "a successful .rge-scene Open must commit a SaveSource::Scene"
    );
}

#[test]
fn scene_open_of_rge_project_commits_project_save_source() {
    // PROJECT-SAVE-WIRING: a literal `.rge-project` Open swaps the world AND
    // commits a `SaveSource::Project` (so `Ctrl+S` writes back to it via the
    // project hook). Previously a project stayed untracked / Save-As.
    let mut s = EditorShell::new()
        .with_glb_open_dialog(Box::new(MockOpenDialog {
            result: Some(std::path::PathBuf::from("/tmp/.rge-project")),
        }))
        .with_scene_open_hook(Box::new(MockSceneOpenHook {
            entity_count: 2,
            fail: false,
        }));

    s.handle_open_request();

    assert_eq!(
        s.world().kernel().entity_count(),
        2,
        ".rge-project Open still swaps the world"
    );
    assert_eq!(
        s.save_source(),
        Some(&SaveSource::Project {
            path: std::path::PathBuf::from("/tmp/.rge-project"),
            name: None,
        }),
        "a literal .rge-project Open must commit a SaveSource::Project (the mock \
         open hook supplies no manifest name → None)"
    );
}

#[test]
fn save_with_source_path_overwrites_without_dialog() {
    // With a known source, Save writes straight to it; the dialog is never
    // consulted (it is attached as a CANCEL dialog whose `None` would abort a
    // Save-As), and the writer receives the source path.
    let calls = std::rc::Rc::new(std::cell::Cell::new(0usize));
    let last_path = std::rc::Rc::new(std::cell::RefCell::new(None));
    let src = std::path::PathBuf::from("/tmp/tracked.rge-scene");
    let mut s = EditorShell::new()
        .with_save_source(SaveSource::Scene(src.clone()))
        .with_scene_save_dialog(Box::new(MockSaveDialog { result: None }))
        .with_scene_save_hook(Box::new(RecordingSaveHook {
            calls: std::rc::Rc::clone(&calls),
            last_path: std::rc::Rc::clone(&last_path),
        }));
    s.set_time_scale(2.0);
    assert!(s.command_bus().is_dirty());

    s.handle_save_request();

    assert_eq!(calls.get(), 1, "silent Save must invoke the writer once");
    assert_eq!(
        last_path.borrow().as_deref(),
        Some(src.as_path()),
        "silent Save must write to the tracked source path (dialog bypassed)"
    );
    assert!(
        !s.command_bus().is_dirty(),
        "silent Save must mark the bus saved"
    );
    assert_eq!(
        s.save_source(),
        Some(&SaveSource::Scene(src.clone())),
        "the save source is unchanged after a silent Save"
    );
}

#[test]
fn save_without_source_path_prompts_and_commits() {
    // No source -> Save-As: the dialog's pick becomes the new tracked source on
    // a successful write (so the next Ctrl+S overwrites it silently).
    let (hook, calls) = save_hook(false);
    let picked = std::path::PathBuf::from("/tmp/picked.rge-scene");
    let mut s = EditorShell::new()
        .with_scene_save_dialog(Box::new(MockSaveDialog {
            result: Some(picked.clone()),
        }))
        .with_scene_save_hook(Box::new(hook));
    s.set_time_scale(2.0);
    assert!(s.save_source().is_none(), "precondition: no source");

    s.handle_save_request();

    assert_eq!(calls.get(), 1, "Save-As invokes the writer");
    assert_eq!(
        s.save_source(),
        Some(&SaveSource::Scene(picked.clone())),
        "a successful Save-As commits the picked path as a new SaveSource::Scene"
    );
    assert!(!s.command_bus().is_dirty(), "Save-As marks the bus saved");
}

#[test]
fn save_as_failure_does_not_commit_source_path() {
    // Save-As whose write fails commits no source and does not mark saved.
    let (hook, calls) = save_hook(true);
    let mut s = EditorShell::new()
        .with_scene_save_dialog(Box::new(MockSaveDialog {
            result: Some(std::path::PathBuf::from("/tmp/picked.rge-scene")),
        }))
        .with_scene_save_hook(Box::new(hook));
    s.set_time_scale(2.0);

    s.handle_save_request();

    assert_eq!(calls.get(), 1, "the writer was invoked");
    assert!(
        s.save_source().is_none(),
        "a failed Save-As must not commit a source"
    );
    assert!(
        s.command_bus().is_dirty(),
        "a failed Save-As must not mark the bus saved"
    );
}

#[test]
fn replace_world_clears_save_source() {
    // A world swap resets the save source.
    let mut s = EditorShell::new().with_save_source(SaveSource::Scene(std::path::PathBuf::from(
        "/tmp/tracked.rge-scene",
    )));
    assert!(s.save_source().is_some(), "precondition: source set");

    s.replace_world(rge_kernel_ecs::World::new())
        .expect("world swap allowed in Editing");

    assert!(
        s.save_source().is_none(),
        "replace_world must clear save_source"
    );
}

#[test]
fn save_outside_editing_with_source_is_noop() {
    // Even with a tracked source, Save is PIE-gated: during Play it no-ops (the
    // writer is never reached, the source is untouched).
    let (hook, calls) = save_hook(false);
    let src = std::path::PathBuf::from("/tmp/tracked.rge-scene");
    let mut s = EditorShell::new()
        .with_save_source(SaveSource::Scene(src.clone()))
        .with_scene_save_hook(Box::new(hook));
    build_scene(&mut s, 2);
    s.handle_button(ToolbarButtonId::Play)
        .expect("Play transition from Editing");
    assert_eq!(s.play_state(), PlayState::Playing);

    s.handle_save_request();

    assert_eq!(calls.get(), 0, "Save during PIE must not reach the writer");
    assert_eq!(
        s.save_source(),
        Some(&SaveSource::Scene(src.clone())),
        "PIE-gated Save leaves the source untouched"
    );
}

// ---------------------------------------------------------------------------
// PROJECT-SAVE-WIRING — Ctrl+S routes a `SaveSource::Project` to the project
// hook (overwrite first scene + manifest); the scene hook is never consulted.
// ---------------------------------------------------------------------------

/// Mock [`crate::ProjectSaveHook`] recording its call count + the last project
/// path it received, returning `Ok`/`Err` per `fail`. (The real `.rge-project`
/// disk write is covered by `rge-scene-loader`'s round-trip tests + the binary
/// `ProjectSaveWriterHook` test; here we only prove the handler's routing + the
/// mark-saved-on-success contract.)
struct RecordingProjectSaveHook {
    fail: bool,
    calls: std::rc::Rc<std::cell::Cell<usize>>,
    last_path: std::rc::Rc<std::cell::RefCell<Option<std::path::PathBuf>>>,
}

impl crate::ProjectSaveHook for RecordingProjectSaveHook {
    fn save_project_world(
        &self,
        _world: &rge_kernel_ecs::World,
        project_path: &std::path::Path,
    ) -> Result<(), String> {
        self.calls.set(self.calls.get() + 1);
        *self.last_path.borrow_mut() = Some(project_path.to_path_buf());
        if self.fail {
            Err("simulated project save failure".into())
        } else {
            Ok(())
        }
    }
}

#[test]
fn save_with_project_source_routes_to_project_hook() {
    // A `SaveSource::Project` Ctrl+S writes through the project hook (not the
    // scene hook), receives the project path, and marks the bus saved on Ok.
    let calls = std::rc::Rc::new(std::cell::Cell::new(0usize));
    let last_path = std::rc::Rc::new(std::cell::RefCell::new(None));
    let (scene_hook, scene_calls) = save_hook(false);
    let project = std::path::PathBuf::from("/tmp/proj/.rge-project");
    let mut s = EditorShell::new()
        .with_save_source(SaveSource::Project {
            path: project.clone(),
            name: None,
        })
        .with_scene_save_hook(Box::new(scene_hook))
        .with_project_save_hook(Box::new(RecordingProjectSaveHook {
            fail: false,
            calls: std::rc::Rc::clone(&calls),
            last_path: std::rc::Rc::clone(&last_path),
        }));
    s.set_time_scale(2.0);
    assert!(s.command_bus().is_dirty());

    s.handle_save_request();

    assert_eq!(
        calls.get(),
        1,
        "Project Save must invoke the project hook once"
    );
    assert_eq!(
        scene_calls.get(),
        0,
        "Project Save must NOT consult the scene hook"
    );
    assert_eq!(
        last_path.borrow().as_deref(),
        Some(project.as_path()),
        "the project hook must receive the tracked .rge-project path"
    );
    assert!(
        !s.command_bus().is_dirty(),
        "a successful Project Save must mark the bus saved"
    );
    assert_eq!(
        s.save_source(),
        Some(&SaveSource::Project {
            path: project.clone(),
            name: None,
        }),
        "the Project source is unchanged after a silent Save"
    );
}

#[test]
fn project_save_failure_does_not_mark_saved() {
    // The project hook returns Err: invoked once, but the bus stays dirty.
    let calls = std::rc::Rc::new(std::cell::Cell::new(0usize));
    let last_path = std::rc::Rc::new(std::cell::RefCell::new(None));
    let project = std::path::PathBuf::from("/tmp/proj/.rge-project");
    let mut s = EditorShell::new()
        .with_save_source(SaveSource::Project {
            path: project.clone(),
            name: None,
        })
        .with_project_save_hook(Box::new(RecordingProjectSaveHook {
            fail: true,
            calls: std::rc::Rc::clone(&calls),
            last_path: std::rc::Rc::clone(&last_path),
        }));
    s.set_time_scale(2.0);
    assert!(s.command_bus().is_dirty());

    s.handle_save_request();

    assert_eq!(calls.get(), 1, "the project hook was invoked");
    assert!(
        s.command_bus().is_dirty(),
        "a failed Project Save must NOT mark the bus saved"
    );
}

#[test]
fn project_save_without_hook_is_noop() {
    // A Project source but no project_save_hook attached: warn + no-op; the bus
    // stays dirty (defensive — the binary attaches the hook in every mode).
    let mut s = EditorShell::new().with_save_source(SaveSource::Project {
        path: std::path::PathBuf::from("/tmp/proj/.rge-project"),
        name: None,
    });
    s.set_time_scale(2.0);
    assert!(
        s.project_save_hook.is_none(),
        "precondition: no project hook"
    );
    assert!(s.command_bus().is_dirty());

    s.handle_save_request();

    assert!(
        s.command_bus().is_dirty(),
        "a no-hook Project Save must leave the bus dirty"
    );
}

// ---------------------------------------------------------------------------
// EDITOR-WINDOW-TITLE — window title reflects save source + dirty state
// ---------------------------------------------------------------------------

#[test]
fn window_title_no_source_clean() {
    assert_eq!(editor_window_title(None, false), "RGE Editor");
}

#[test]
fn window_title_no_source_dirty() {
    assert_eq!(editor_window_title(None, true), "RGE Editor *");
}

#[test]
fn window_title_with_source_clean() {
    assert_eq!(
        editor_window_title(Some("level.rge-scene"), false),
        "level.rge-scene — RGE Editor"
    );
}

#[test]
fn window_title_with_source_dirty() {
    assert_eq!(
        editor_window_title(Some("level.rge-scene"), true),
        "level.rge-scene * — RGE Editor"
    );
}

#[test]
fn window_title_uses_display_name_verbatim() {
    // The pure formatter no longer extracts a file name — it formats the
    // already-resolved display name as-is (extraction lives in
    // SaveSource::display_name). A project folder name passes through unchanged.
    assert_eq!(
        editor_window_title(Some("my-game"), false),
        "my-game — RGE Editor"
    );
}

// ---------------------------------------------------------------------------
// SAVE-SOURCE-DISPLAY-NAME — SaveSource::display_name (title/status display)
// ---------------------------------------------------------------------------

#[test]
fn display_name_scene_is_file_name() {
    let s = SaveSource::Scene(std::path::PathBuf::from("/a/b/level.rge-scene"));
    assert_eq!(s.display_name(), Some("level.rge-scene"));
}

#[test]
fn display_name_project_prefers_manifest_name() {
    // With a manifest name in hand, a project reads as its declared name —
    // not the containing folder.
    let s = SaveSource::Project {
        path: std::path::PathBuf::from("/projects/my-game/.rge-project"),
        name: Some("My Cool Game".to_string()),
    };
    assert_eq!(s.display_name(), Some("My Cool Game"));
}

#[test]
fn display_name_project_without_manifest_name_falls_back_to_folder() {
    // No manifest name (`None`) → the containing folder name, not the literal
    // `.rge-project`.
    let s = SaveSource::Project {
        path: std::path::PathBuf::from("/projects/my-game/.rge-project"),
        name: None,
    };
    assert_eq!(s.display_name(), Some("my-game"));
}

#[test]
fn display_name_project_empty_manifest_name_falls_back_to_folder() {
    // An empty manifest name must not blank the title — fall back to the folder.
    let s = SaveSource::Project {
        path: std::path::PathBuf::from("/projects/my-game/.rge-project"),
        name: Some(String::new()),
    };
    assert_eq!(s.display_name(), Some("my-game"));
}

#[test]
fn display_name_project_whitespace_manifest_name_falls_back_to_folder() {
    // A whitespace-only manifest name is treated as absent (it must not render a
    // blank title) — fall back to the project folder name. Regression guard for
    // the `trim()` check in `display_name`.
    let s = SaveSource::Project {
        path: std::path::PathBuf::from("/projects/my-game/.rge-project"),
        name: Some("   ".to_string()),
    };
    assert_eq!(s.display_name(), Some("my-game"));
}

#[test]
fn display_name_project_without_parent_falls_back_to_file_name() {
    // A bare `.rge-project` (no parent dir, no manifest name) falls back to the
    // file name.
    let s = SaveSource::Project {
        path: std::path::PathBuf::from(".rge-project"),
        name: None,
    };
    assert_eq!(s.display_name(), Some(".rge-project"));
}

#[test]
fn project_save_source_surfaces_folder_name_in_status_snapshot() {
    // End-to-end: an unnamed Project save source surfaces its folder name (not
    // `.rge-project`) in the status snapshot the bottom bar renders.
    let s = EditorShell::new().with_save_source(SaveSource::Project {
        path: std::path::PathBuf::from("/projects/my-game/.rge-project"),
        name: None,
    });
    assert_eq!(
        s.save_status_snapshot().source_name.as_deref(),
        Some("my-game"),
        "an unnamed Project save source must surface its folder name in the status snapshot"
    );
}

#[test]
fn named_project_save_source_surfaces_manifest_name_in_status_snapshot() {
    // End-to-end: a Project save source carrying a manifest name surfaces that
    // name (not the folder) in the status snapshot the bottom bar renders.
    let s = EditorShell::new().with_save_source(SaveSource::Project {
        path: std::path::PathBuf::from("/projects/my-game/.rge-project"),
        name: Some("My Cool Game".to_string()),
    });
    assert_eq!(
        s.save_status_snapshot().source_name.as_deref(),
        Some("My Cool Game"),
        "a named Project save source must surface its manifest name in the status snapshot"
    );
}

#[test]
fn scene_save_source_surfaces_file_name_in_status_snapshot() {
    // A Scene source is unchanged: its file name surfaces (regression guard).
    let s = EditorShell::new().with_save_source(SaveSource::Scene(std::path::PathBuf::from(
        "/projects/demo/level.rge-scene",
    )));
    assert_eq!(
        s.save_status_snapshot().source_name.as_deref(),
        Some("level.rge-scene")
    );
}

#[test]
fn project_open_threads_hook_display_name_into_save_source() {
    // End-to-end Open wiring (audit Finding 4): opening a `.rge-project` must ask
    // the binary-owned SceneOpenHook for the manifest display name and thread it
    // into `SaveSource::Project { name }`, so the title / bottom bar show the
    // manifest name — not the folder. The direct `display_name` / snapshot tests
    // construct the variant by hand and bypass this `open_request.rs` plumbing.
    let mut s = EditorShell::new()
        .with_glb_open_dialog(Box::new(MockOpenDialog {
            result: Some(std::path::PathBuf::from("/projects/my-game/.rge-project")),
        }))
        .with_scene_open_hook(Box::new(NamingSceneOpenHook {
            entity_count: 2,
            display_name: Some("My Cool Game".to_string()),
        }));

    s.handle_open_request();

    let source = s
        .save_source()
        .expect("a .rge-project Open must commit a save source");
    assert!(
        source.is_project(),
        "opening a `.rge-project` must commit a Project save source"
    );
    assert_eq!(
        source.path(),
        std::path::Path::new("/projects/my-game/.rge-project"),
        "the committed Project path must be the opened candidate"
    );
    // `display_name() == "My Cool Game"` (≠ folder `my-game`) proves the hook's
    // name was threaded into the variant rather than the folder fallback.
    assert_eq!(
        source.display_name(),
        Some("My Cool Game"),
        "the hook's project_display_name must drive the source display name"
    );
    assert_eq!(
        s.save_status_snapshot().source_name.as_deref(),
        Some("My Cool Game"),
        "the manifest name must reach the status snapshot the bottom bar renders"
    );
}

#[test]
fn project_open_without_hook_name_falls_back_to_folder() {
    // Companion to the positive case: when the open hook supplies no name (the
    // default `MockSceneOpenHook` returns `None`), a `.rge-project` Open falls
    // back to the project folder name through the real open path.
    let mut s = EditorShell::new()
        .with_glb_open_dialog(Box::new(MockOpenDialog {
            result: Some(std::path::PathBuf::from("/projects/my-game/.rge-project")),
        }))
        .with_scene_open_hook(Box::new(MockSceneOpenHook {
            entity_count: 2,
            fail: false,
        }));

    s.handle_open_request();

    let source = s
        .save_source()
        .expect("a .rge-project Open must commit a save source");
    assert!(source.is_project(), "must commit a Project save source");
    assert_eq!(
        source.display_name(),
        Some("my-game"),
        "with no hook name, the Project source must fall back to the folder name"
    );
}

#[test]
fn sync_window_title_without_window_is_noop() {
    // A headless shell has no winit window; the per-frame title sync must not
    // panic and must leave the windowless shell untouched.
    let mut s = EditorShell::new();
    assert!(
        s.window.is_none(),
        "precondition: headless shell has no window"
    );

    s.sync_window_title();

    assert!(
        s.window.is_none(),
        "sync_window_title must not create a window"
    );
    assert!(
        s.last_window_title.is_none(),
        "a windowless sync commits no title"
    );
}

// ---------------------------------------------------------------------------
// NEWPROJECT-SAVE-WIRING — Ctrl+Shift+S Save-As to a NEW `.rge-project` tree
// (create the tree via the new-project hook, adopt it as the save source).
// ---------------------------------------------------------------------------

/// Mock [`crate::NewProjectSaveDialog`] returning a fixed directory (`None`
/// simulates cancel).
struct MockNewProjectDialog {
    dir: Option<std::path::PathBuf>,
}

impl crate::NewProjectSaveDialog for MockNewProjectDialog {
    fn pick_new_project_dir(&self) -> Option<std::path::PathBuf> {
        self.dir.clone()
    }
}

/// Mock [`crate::NewProjectSaveHook`] recording its call count + the dir it
/// received, returning a fixed created-`.rge-project` path on success or an
/// `Err` per `fail`. (The real tree creation is covered by `rge-scene-loader`'s
/// round-trip tests + the binary hook; here we only prove the handler's wiring +
/// the adopt-source / mark-saved contract.)
struct RecordingNewProjectHook {
    fail: bool,
    created: std::path::PathBuf,
    calls: std::rc::Rc<std::cell::Cell<usize>>,
    last_dir: std::rc::Rc<std::cell::RefCell<Option<std::path::PathBuf>>>,
}

impl crate::NewProjectSaveHook for RecordingNewProjectHook {
    fn save_world_as_new_project(
        &self,
        _world: &rge_kernel_ecs::World,
        project_dir: &std::path::Path,
    ) -> Result<std::path::PathBuf, String> {
        self.calls.set(self.calls.get() + 1);
        *self.last_dir.borrow_mut() = Some(project_dir.to_path_buf());
        if self.fail {
            Err("simulated new-project create failure".into())
        } else {
            Ok(self.created.clone())
        }
    }
}

#[test]
fn save_as_new_project_creates_and_adopts_project_source() {
    // Ctrl+Shift+S: the dialog picks a dir, the hook creates the tree there and
    // returns the `.rge-project` path, and the shell adopts it as
    // `SaveSource::Project { path, name: <folder> }` and marks the bus saved.
    let calls = std::rc::Rc::new(std::cell::Cell::new(0usize));
    let last_dir = std::rc::Rc::new(std::cell::RefCell::new(None));
    let created = std::path::PathBuf::from("/projects/my-game/.rge-project");
    let mut s = EditorShell::new()
        .with_new_project_save_dialog(Box::new(MockNewProjectDialog {
            dir: Some(std::path::PathBuf::from("/projects/my-game")),
        }))
        .with_new_project_save_hook(Box::new(RecordingNewProjectHook {
            fail: false,
            created: created.clone(),
            calls: std::rc::Rc::clone(&calls),
            last_dir: std::rc::Rc::clone(&last_dir),
        }));
    s.set_time_scale(2.0);
    assert!(s.command_bus().is_dirty());

    s.handle_save_as_new_project_request();

    assert_eq!(calls.get(), 1, "the new-project hook is invoked once");
    assert_eq!(
        last_dir.borrow().as_deref(),
        Some(std::path::Path::new("/projects/my-game")),
        "the hook receives the picked directory"
    );
    assert_eq!(
        s.save_source(),
        Some(&SaveSource::Project {
            path: created.clone(),
            name: Some("my-game".to_string()),
        }),
        "success adopts the created .rge-project with the folder-derived name"
    );
    assert_eq!(
        s.save_source().and_then(|src| src.display_name()),
        Some("my-game"),
        "the adopted source's display name is the folder-derived project name"
    );
    assert!(
        !s.command_bus().is_dirty(),
        "a successful Save-As (new project) marks the bus saved"
    );
}

#[test]
fn save_as_new_project_cancel_is_noop() {
    // Dialog returns None (cancel): no source adopted, bus untouched, hook never
    // called.
    let calls = std::rc::Rc::new(std::cell::Cell::new(0usize));
    let last_dir = std::rc::Rc::new(std::cell::RefCell::new(None));
    let mut s = EditorShell::new()
        .with_new_project_save_dialog(Box::new(MockNewProjectDialog { dir: None }))
        .with_new_project_save_hook(Box::new(RecordingNewProjectHook {
            fail: false,
            created: std::path::PathBuf::from("/unused/.rge-project"),
            calls: std::rc::Rc::clone(&calls),
            last_dir: std::rc::Rc::clone(&last_dir),
        }));
    s.set_time_scale(2.0);
    assert!(s.command_bus().is_dirty());

    s.handle_save_as_new_project_request();

    assert_eq!(calls.get(), 0, "a cancelled dialog never calls the hook");
    assert_eq!(s.save_source(), None, "cancel adopts no save source");
    assert!(
        s.command_bus().is_dirty(),
        "cancel leaves the bus dirty (no mutation)"
    );
}

#[test]
fn save_as_new_project_without_dialog_is_noop() {
    let mut s = EditorShell::new();
    assert!(s.new_project_dialog.is_none(), "precondition: no dialog");
    s.set_time_scale(2.0);

    s.handle_save_as_new_project_request();

    assert_eq!(s.save_source(), None);
    assert!(s.command_bus().is_dirty(), "no dialog -> no-op, bus dirty");
}

#[test]
fn save_as_new_project_without_hook_is_noop() {
    // A dialog picks a dir but no new_project_hook is attached: warn + no-op.
    let mut s = EditorShell::new().with_new_project_save_dialog(Box::new(MockNewProjectDialog {
        dir: Some(std::path::PathBuf::from("/projects/my-game")),
    }));
    assert!(s.new_project_hook.is_none(), "precondition: no hook");
    s.set_time_scale(2.0);

    s.handle_save_as_new_project_request();

    assert_eq!(s.save_source(), None, "missing hook adopts no source");
    assert!(
        s.command_bus().is_dirty(),
        "missing hook -> no-op, bus dirty"
    );
}

#[test]
fn save_as_new_project_hook_error_does_not_adopt_or_mark_saved() {
    let calls = std::rc::Rc::new(std::cell::Cell::new(0usize));
    let last_dir = std::rc::Rc::new(std::cell::RefCell::new(None));
    let mut s = EditorShell::new()
        .with_new_project_save_dialog(Box::new(MockNewProjectDialog {
            dir: Some(std::path::PathBuf::from("/projects/my-game")),
        }))
        .with_new_project_save_hook(Box::new(RecordingNewProjectHook {
            fail: true,
            created: std::path::PathBuf::from("/projects/my-game/.rge-project"),
            calls: std::rc::Rc::clone(&calls),
            last_dir: std::rc::Rc::clone(&last_dir),
        }));
    s.set_time_scale(2.0);
    assert!(s.command_bus().is_dirty());

    s.handle_save_as_new_project_request();

    assert_eq!(calls.get(), 1, "the hook was invoked");
    assert_eq!(s.save_source(), None, "a failed create adopts no source");
    assert!(
        s.command_bus().is_dirty(),
        "a failed create must NOT mark the bus saved"
    );
}

#[test]
fn save_as_new_project_outside_editing_is_noop() {
    // PIE gate — Save-As (new project) only fires in Editing, mirroring
    // `save_outside_editing_is_noop` for the Ctrl+S path. During Play the
    // handler returns at the PIE check BEFORE the dialog or the new-project hook
    // is consulted, so a mid-Play Ctrl+Shift+S can never persist the transient
    // play-state world as a brand-new on-disk `.rge-project`.
    let calls = std::rc::Rc::new(std::cell::Cell::new(0usize));
    let last_dir = std::rc::Rc::new(std::cell::RefCell::new(None));
    let mut s = EditorShell::new()
        .with_new_project_save_dialog(Box::new(MockNewProjectDialog {
            dir: Some(std::path::PathBuf::from("/projects/my-game")),
        }))
        .with_new_project_save_hook(Box::new(RecordingNewProjectHook {
            fail: false,
            created: std::path::PathBuf::from("/projects/my-game/.rge-project"),
            calls: std::rc::Rc::clone(&calls),
            last_dir: std::rc::Rc::clone(&last_dir),
        }));
    build_scene(&mut s, 2);
    s.handle_button(ToolbarButtonId::Play)
        .expect("Play transition from Editing");
    assert_eq!(s.play_state(), PlayState::Playing);

    s.handle_save_as_new_project_request();

    assert_eq!(
        calls.get(),
        0,
        "Save-As during PIE must not reach the new-project hook"
    );
    assert_eq!(
        s.save_source(),
        None,
        "a gated Save-As adopts no save source"
    );
    assert_eq!(
        s.play_state(),
        PlayState::Playing,
        "state unchanged on the gated path"
    );
}

#[test]
fn from_key_press_does_not_decode_retired_file_edit_binds() {
    use rge_input::KeyCode;

    use crate::EditorKeyCommand;
    // W08.4 retired Ctrl+Shift+S (Save-As) and Ctrl+S (Save) from from_key_press —
    // they resolve through the canonical menu now. Both, and the other File/Edit
    // combos, must return None (only the Ctrl+digit time-scale binds map here).
    assert_eq!(
        EditorKeyCommand::from_key_press(KeyCode::KeyS, true, true),
        None,
        "Ctrl+Shift+S retired to the menu (Save-As)"
    );
    assert_eq!(
        EditorKeyCommand::from_key_press(KeyCode::KeyS, true, false),
        None,
        "Ctrl+S retired to the menu (Save)"
    );
    assert_eq!(
        EditorKeyCommand::from_key_press(KeyCode::KeyO, true, true),
        None,
        "Ctrl+Shift+O is unmapped"
    );
    assert_eq!(
        EditorKeyCommand::from_key_press(KeyCode::KeyS, false, true),
        None,
        "Shift+S without Ctrl is unmapped"
    );
}

#[test]
fn save_as_then_ctrl_s_routes_through_project_hook() {
    // After a successful Save-As (new project), a plain Ctrl+S routes the adopted
    // Project source through the existing ProjectSaveHook (silent overwrite),
    // proving the source was adopted end-to-end.
    let created = std::path::PathBuf::from("/projects/my-game/.rge-project");
    let project_calls = std::rc::Rc::new(std::cell::Cell::new(0usize));
    let project_last = std::rc::Rc::new(std::cell::RefCell::new(None));
    let mut s = EditorShell::new()
        .with_new_project_save_dialog(Box::new(MockNewProjectDialog {
            dir: Some(std::path::PathBuf::from("/projects/my-game")),
        }))
        .with_new_project_save_hook(Box::new(RecordingNewProjectHook {
            fail: false,
            created: created.clone(),
            calls: std::rc::Rc::new(std::cell::Cell::new(0usize)),
            last_dir: std::rc::Rc::new(std::cell::RefCell::new(None)),
        }))
        .with_project_save_hook(Box::new(RecordingProjectSaveHook {
            fail: false,
            calls: std::rc::Rc::clone(&project_calls),
            last_path: std::rc::Rc::clone(&project_last),
        }));

    s.handle_save_as_new_project_request();
    assert!(
        s.save_source().is_some_and(|src| src.is_project()),
        "Save-As adopts a Project source"
    );

    // Dirty the bus, then a plain Ctrl+S routes the adopted Project source.
    s.set_time_scale(2.0);
    assert!(s.command_bus().is_dirty());
    s.handle_save_request();

    assert_eq!(
        project_calls.get(),
        1,
        "plain Ctrl+S after Save-As routes through the existing project hook"
    );
    assert_eq!(
        project_last.borrow().as_deref(),
        Some(created.as_path()),
        "the project hook receives the adopted .rge-project path"
    );
    assert!(
        !s.command_bus().is_dirty(),
        "the silent re-save through the adopted Project source marks saved"
    );
}

// ---------------------------------------------------------------------------
// MENUBAR-FILE-WIRING (Dispatch B) — menu Command -> handler routing
// ---------------------------------------------------------------------------

#[test]
fn unsaved_clean_close_skips_confirmation_and_resets_document() {
    let (dialog, calls) = mock_unsaved_changes_dialog(UnsavedChangesDecision::Cancel);
    let mut shell = unsaved_seed_clean_shell().with_unsaved_changes_dialog(dialog);

    shell.handle_close_file_request();

    assert!(calls.borrow().is_empty(), "clean Close must not prompt");
    assert_eq!(shell.world().entity_count(), 0);
    assert_eq!(shell.world().kernel().entity_count(), 0);
    assert_eq!(shell.save_source(), None);
    assert!(shell.coord().selection.is_empty());
    assert!(!shell.predicate_context().has_clipboard_entities);
    assert!(!shell.command_bus().is_dirty());
}

#[test]
fn unsaved_dirty_close_cancel_preserves_document_state() {
    let (dialog, calls) = mock_unsaved_changes_dialog(UnsavedChangesDecision::Cancel);
    let mut shell = unsaved_seed_dirty_shell().with_unsaved_changes_dialog(dialog);
    let before = unsaved_snapshot(&shell);

    shell.handle_close_file_request();

    assert_eq!(unsaved_snapshot(&shell), before);
    let calls = calls.borrow();
    assert_eq!(calls.len(), 1);
    assert_unsaved_context(
        &calls[0],
        UnsavedChangesRequest::CloseFile,
        UnsavedChangesSourceKind::Scene,
        "unsaved-dirty.rge-scene",
    );
}

#[test]
fn unsaved_dirty_close_no_hook_preserves_document_state() {
    let mut shell = unsaved_seed_dirty_shell();
    let before = unsaved_snapshot(&shell);

    shell.handle_close_file_request();

    assert_eq!(unsaved_snapshot(&shell), before);
}

#[test]
fn unsaved_dirty_close_discard_resets_document() {
    let (dialog, calls) = mock_unsaved_changes_dialog(UnsavedChangesDecision::Discard);
    let mut shell = unsaved_seed_dirty_shell().with_unsaved_changes_dialog(dialog);

    shell.handle_close_file_request();

    assert_eq!(shell.world().entity_count(), 0);
    assert_eq!(shell.world().kernel().entity_count(), 0);
    assert_eq!(shell.save_source(), None);
    assert!(shell.coord().selection.is_empty());
    assert!(shell.coord().face_selection.iter().next().is_none());
    assert!(!shell.predicate_context().has_clipboard_entities);
    assert!(!shell.command_bus().is_dirty());
    let calls = calls.borrow();
    assert_eq!(calls.len(), 1);
    assert_unsaved_context(
        &calls[0],
        UnsavedChangesRequest::CloseFile,
        UnsavedChangesSourceKind::Scene,
        "unsaved-dirty.rge-scene",
    );
}

#[test]
fn unsaved_clean_quit_skips_confirmation_and_requests_exit() {
    let (dialog, calls) = mock_unsaved_changes_dialog(UnsavedChangesDecision::Cancel);
    let mut shell = unsaved_seed_clean_shell().with_unsaved_changes_dialog(dialog);

    shell.handle_quit_request();

    assert!(calls.borrow().is_empty(), "clean Quit must not prompt");
    assert!(shell.quit_requested);
    assert_eq!(shell.world().entity_count(), 2);
    assert!(shell.save_source().is_some());
}

#[test]
fn unsaved_dirty_quit_cancel_preserves_document_and_pending_quit() {
    let (dialog, calls) = mock_unsaved_changes_dialog(UnsavedChangesDecision::Cancel);
    let mut shell = unsaved_seed_dirty_shell().with_unsaved_changes_dialog(dialog);
    let before = unsaved_snapshot(&shell);

    shell.handle_quit_request();

    assert_eq!(unsaved_snapshot(&shell), before);
    let calls = calls.borrow();
    assert_eq!(calls.len(), 1);
    assert_unsaved_context(
        &calls[0],
        UnsavedChangesRequest::QuitApplication,
        UnsavedChangesSourceKind::Scene,
        "unsaved-dirty.rge-scene",
    );
}

#[test]
fn unsaved_dirty_quit_no_hook_preserves_document_and_pending_quit() {
    let mut shell = unsaved_seed_dirty_shell();
    let before = unsaved_snapshot(&shell);

    shell.handle_quit_request();

    assert_eq!(unsaved_snapshot(&shell), before);
}

#[test]
fn unsaved_dirty_quit_discard_requests_exit_without_resetting_document() {
    let (dialog, calls) = mock_unsaved_changes_dialog(UnsavedChangesDecision::Discard);
    let mut shell = unsaved_seed_dirty_shell().with_unsaved_changes_dialog(dialog);
    let before = unsaved_snapshot(&shell);

    shell.handle_quit_request();

    let mut after = unsaved_snapshot(&shell);
    assert!(after.quit_requested);
    after.quit_requested = false;
    assert_eq!(after, before);
    let calls = calls.borrow();
    assert_eq!(calls.len(), 1);
    assert_unsaved_context(
        &calls[0],
        UnsavedChangesRequest::QuitApplication,
        UnsavedChangesSourceKind::Scene,
        "unsaved-dirty.rge-scene",
    );
}

#[test]
fn unsaved_clean_window_close_skips_confirmation_and_allows_exit() {
    let (dialog, calls) = mock_unsaved_changes_dialog(UnsavedChangesDecision::Cancel);
    let shell = unsaved_seed_clean_shell().with_unsaved_changes_dialog(dialog);

    assert!(shell.should_exit_on_window_close_request());

    assert!(
        calls.borrow().is_empty(),
        "clean window close must not prompt"
    );
}

#[test]
fn unsaved_dirty_window_close_cancel_blocks_exit_and_preserves_state() {
    let (dialog, calls) = mock_unsaved_changes_dialog(UnsavedChangesDecision::Cancel);
    let shell = unsaved_seed_dirty_shell().with_unsaved_changes_dialog(dialog);
    let before = unsaved_snapshot(&shell);

    assert!(!shell.should_exit_on_window_close_request());

    assert_eq!(unsaved_snapshot(&shell), before);
    let calls = calls.borrow();
    assert_eq!(calls.len(), 1);
    assert_unsaved_context(
        &calls[0],
        UnsavedChangesRequest::WindowClose,
        UnsavedChangesSourceKind::Scene,
        "unsaved-dirty.rge-scene",
    );
}

#[test]
fn unsaved_dirty_window_close_no_hook_blocks_exit_and_preserves_state() {
    let shell = unsaved_seed_dirty_shell();
    let before = unsaved_snapshot(&shell);

    assert!(!shell.should_exit_on_window_close_request());

    assert_eq!(unsaved_snapshot(&shell), before);
}

#[test]
fn unsaved_dirty_window_close_discard_allows_exit() {
    let (dialog, calls) = mock_unsaved_changes_dialog(UnsavedChangesDecision::Discard);
    let shell = unsaved_seed_dirty_shell().with_unsaved_changes_dialog(dialog);
    let before = unsaved_snapshot(&shell);

    assert!(shell.should_exit_on_window_close_request());

    assert_eq!(unsaved_snapshot(&shell), before);
    let calls = calls.borrow();
    assert_eq!(calls.len(), 1);
    assert_unsaved_context(
        &calls[0],
        UnsavedChangesRequest::WindowClose,
        UnsavedChangesSourceKind::Scene,
        "unsaved-dirty.rge-scene",
    );
}

mod menu_routing {
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::rc::Rc;
    use std::sync::Arc;

    use rge_editor_egui_host::MenuCommandHandoff;
    use rge_editor_ui::menus::Command;

    use super::*;
    use crate::lifecycle::{
        ExtensionCommandError, ExtensionCommandEvent, ExtensionCommandHandler,
        ExtensionCommandOutcome, ExtensionCommandResult,
    };

    /// A `MenuCommandHandoff` pre-loaded with `cmds` (FIFO), wrapped in an `Arc`
    /// ready to attach to a shell's `menu_command_handoff` field.
    fn handoff_with(cmds: &[Command]) -> Arc<MenuCommandHandoff> {
        let h = Arc::new(MenuCommandHandoff::new());
        for c in cmds {
            h.push(c.clone());
        }
        h
    }

    struct RecordingExtensionHandler {
        seen: Rc<RefCell<Vec<Command>>>,
        outcomes: VecDeque<ExtensionCommandResult>,
    }

    impl ExtensionCommandHandler for RecordingExtensionHandler {
        fn handle_extension_command(&mut self, command: &Command) -> ExtensionCommandResult {
            self.seen.borrow_mut().push(command.clone());
            self.outcomes
                .pop_front()
                .unwrap_or(Ok(ExtensionCommandOutcome::Handled))
        }
    }

    fn extension_handler(
        outcomes: Vec<ExtensionCommandResult>,
    ) -> (RecordingExtensionHandler, Rc<RefCell<Vec<Command>>>) {
        let seen = Rc::new(RefCell::new(Vec::new()));
        (
            RecordingExtensionHandler {
                seen: Rc::clone(&seen),
                outcomes: outcomes.into(),
            },
            seen,
        )
    }

    #[test]
    fn menu_new_file_command_resets_to_empty_unsourced_world() {
        // Command::NewFile drained from the menu handoff must reach
        // handle_new_file_request -> replace_world(KernelWorld::new()). It is
        // a reset-to-empty operation, not a disk/project creation path.
        let mut s = EditorShell::new().with_save_source(SaveSource::Scene(
            std::path::PathBuf::from("/tmp/old.rge-scene"),
        ));
        build_scene(&mut s, 2);
        let selected = s
            .world()
            .entities()
            .next()
            .expect("seeded world has entity");
        s.coord_mut().selection.add(selected);
        assert_eq!(
            s.copy_selected_entities(),
            1,
            "precondition: New clears a non-empty shell clipboard"
        );
        assert!(
            s.predicate_context().has_clipboard_entities,
            "precondition: clipboard state is published"
        );
        s.set_time_scale(2.0);
        assert!(
            s.command_bus().is_dirty(),
            "precondition: seeded dirty world"
        );
        assert!(s.save_source().is_some(), "precondition: source is set");
        s.menu_command_handoff = Some(handoff_with(&[Command::NewFile]));

        s.drain_and_route_menu_commands();

        assert_eq!(
            s.world().entity_count(),
            0,
            "New resets the wrapper world to empty"
        );
        assert_eq!(
            s.world().kernel().entity_count(),
            0,
            "New resets the kernel world to empty"
        );
        assert_eq!(s.save_source(), None, "New clears the adopted save source");
        assert!(
            s.coord().selection.is_empty(),
            "New clears editor coordination selection"
        );
        assert!(
            !s.predicate_context().has_clipboard_entities,
            "New clears the shell-local entity clipboard"
        );
        assert!(
            !s.command_bus().is_dirty(),
            "New installs a fresh clean CommandBus"
        );
        assert_eq!(
            s.time_scale(),
            TimeScale::default(),
            "New installs default time scale on the fresh world"
        );
    }

    #[test]
    fn menu_close_file_command_resets_to_empty_unsourced_world() {
        // Command::Close is document-close only: it reuses the same
        // replace_world(KernelWorld::new()) reset substrate as New and does not
        // exit the application.
        let mut s = EditorShell::new().with_save_source(SaveSource::Scene(
            std::path::PathBuf::from("/tmp/open.rge-scene"),
        ));
        build_scene(&mut s, 2);
        let selected = s
            .world()
            .entities()
            .next()
            .expect("seeded world has entity");
        s.coord_mut().selection.add(selected);
        assert_eq!(
            s.copy_selected_entities(),
            1,
            "precondition: Close clears a non-empty shell clipboard"
        );
        assert!(
            !s.command_bus().is_dirty(),
            "precondition: clean Close should not need an unsaved prompt"
        );
        s.menu_command_handoff = Some(handoff_with(&[Command::Close]));

        s.drain_and_route_menu_commands();

        assert_eq!(
            s.play_state(),
            PlayState::Editing,
            "Close does not quit or enter PIE"
        );
        assert_eq!(s.world().entity_count(), 0, "Close clears wrapper world");
        assert_eq!(
            s.world().kernel().entity_count(),
            0,
            "Close clears kernel world"
        );
        assert_eq!(
            s.save_source(),
            None,
            "Close clears the adopted save source"
        );
        assert!(s.coord().selection.is_empty(), "Close clears selection");
        assert!(
            !s.predicate_context().has_clipboard_entities,
            "Close clears the shell-local clipboard"
        );
        assert!(
            !s.command_bus().is_dirty(),
            "Close installs a fresh clean CommandBus"
        );
    }

    #[test]
    fn menu_quit_command_requests_application_exit_without_closing_document() {
        // Command::Quit is app-exit intent, not document-close. route_menu_command
        // cannot own the winit ActiveEventLoop, so it sets a pending request that
        // window_event consumes via the same event_loop.exit() path as
        // CloseRequested.
        let mut s = EditorShell::new().with_save_source(SaveSource::Scene(
            std::path::PathBuf::from("/tmp/open.rge-scene"),
        ));
        build_scene(&mut s, 2);
        s.menu_command_handoff = Some(handoff_with(&[Command::Quit]));

        s.drain_and_route_menu_commands();

        assert_eq!(
            s.world().entity_count(),
            2,
            "Quit does not reset or close the current document"
        );
        assert!(
            s.save_source().is_some(),
            "Quit does not clear the adopted save source"
        );
        assert!(
            s.take_quit_request(),
            "Quit sets a pending application-exit request"
        );
        assert!(
            !s.take_quit_request(),
            "the pending quit request is single-consume"
        );
    }

    #[test]
    fn menu_open_file_command_routes_to_open() {
        // Command::OpenFile drained from the menu handoff must reach
        // handle_open_request — observed by the scene hook's world swapping in.
        let mut s = EditorShell::new()
            .with_glb_open_dialog(Box::new(MockOpenDialog {
                result: Some(std::path::PathBuf::from("/tmp/level.rge-scene")),
            }))
            .with_scene_open_hook(Box::new(MockSceneOpenHook {
                entity_count: 2,
                fail: false,
            }));
        s.menu_command_handoff = Some(handoff_with(&[Command::OpenFile]));

        s.drain_and_route_menu_commands();

        assert_eq!(
            s.world().kernel().entity_count(),
            2,
            "Command::OpenFile routes to handle_open_request (world swapped in)"
        );
    }

    #[test]
    fn menu_save_command_routes_to_save() {
        // Command::Save reaches handle_save_request — with no tracked source it
        // takes the Save-As-scene arm, writing via the scene hook + marking saved.
        let (hook, calls) = save_hook(false);
        let mut s = EditorShell::new()
            .with_scene_save_dialog(Box::new(MockSaveDialog {
                result: Some(std::path::PathBuf::from("/tmp/level.rge-scene")),
            }))
            .with_scene_save_hook(Box::new(hook));
        s.set_time_scale(2.0);
        assert!(s.command_bus().is_dirty());
        s.menu_command_handoff = Some(handoff_with(&[Command::Save]));

        s.drain_and_route_menu_commands();

        assert_eq!(
            calls.get(),
            1,
            "Command::Save routes to handle_save_request (writer invoked)"
        );
        assert!(
            !s.command_bus().is_dirty(),
            "a successful menu Save marks the bus saved"
        );
    }

    #[test]
    fn menu_save_as_command_routes_to_save_as_new_project() {
        // Command::SaveAs reaches handle_save_as_new_project_request (the menu
        // item is labelled "Save As New Project") — observed by the new-project
        // hook firing + a Project source being adopted.
        let calls = std::rc::Rc::new(std::cell::Cell::new(0usize));
        let last_dir = std::rc::Rc::new(std::cell::RefCell::new(None));
        let mut s = EditorShell::new()
            .with_new_project_save_dialog(Box::new(MockNewProjectDialog {
                dir: Some(std::path::PathBuf::from("/projects/my-game")),
            }))
            .with_new_project_save_hook(Box::new(RecordingNewProjectHook {
                fail: false,
                created: std::path::PathBuf::from("/projects/my-game/.rge-project"),
                calls: std::rc::Rc::clone(&calls),
                last_dir: std::rc::Rc::clone(&last_dir),
            }));
        s.set_time_scale(2.0);
        s.menu_command_handoff = Some(handoff_with(&[Command::SaveAs]));

        s.drain_and_route_menu_commands();

        assert_eq!(
            calls.get(),
            1,
            "Command::SaveAs routes to handle_save_as_new_project_request"
        );
        assert!(
            s.save_source().is_some_and(|src| src.is_project()),
            "menu Save-As adopts a new .rge-project save source"
        );
    }

    #[test]
    fn menu_undo_command_reverts_via_bus() {
        // Command::Undo drained from the menu handoff must reach undo_command —
        // observed by the bus reverting the last action: a submitted SetTimeScale
        // leaves the bus dirty; menu Undo returns it to the saved point.
        // Behaviour-identical to Ctrl+Z.
        let mut s = EditorShell::new();
        s.set_time_scale(2.0);
        assert!(
            s.command_bus().is_dirty(),
            "precondition: a submitted action leaves the bus dirty"
        );
        s.menu_command_handoff = Some(handoff_with(&[Command::Undo]));

        s.drain_and_route_menu_commands();

        assert!(
            !s.command_bus().is_dirty(),
            "Command::Undo routes to undo_command (last action reverted to the saved point)"
        );
    }

    #[test]
    fn menu_redo_command_reapplies_via_bus() {
        // Command::Redo drained from the menu handoff must reach redo_command —
        // observed by the bus re-applying a previously-undone action.
        // Behaviour-identical to Ctrl+Y.
        let mut s = EditorShell::new();
        s.set_time_scale(2.0);
        s.undo_command().expect("undo the submitted action");
        assert!(
            !s.command_bus().is_dirty(),
            "precondition: after undo the bus is back at the saved point"
        );
        s.menu_command_handoff = Some(handoff_with(&[Command::Redo]));

        s.drain_and_route_menu_commands();

        assert!(
            s.command_bus().is_dirty(),
            "Command::Redo routes to redo_command (the undone action is re-applied)"
        );
    }

    #[test]
    fn menu_select_all_command_replaces_entity_selection() {
        // Command::SelectAll drained from the menu handoff must reach
        // EditorShell::select_all_entities. It is coordination state only: the
        // world is unchanged, and face selection is not touched.
        let mut s = EditorShell::new();
        build_scene(&mut s, 3);
        let ids: Vec<_> = s.world().entities().collect();
        s.coord_mut().selection.add(ids[0]);
        s.menu_command_handoff = Some(handoff_with(&[Command::SelectAll]));

        s.drain_and_route_menu_commands();

        assert_eq!(
            s.coord().selection.iter().collect::<Vec<_>>(),
            ids,
            "Command::SelectAll routes to select_all_entities"
        );
        assert_eq!(
            s.world().entity_count(),
            3,
            "Select All changes coordination state, not world contents"
        );
        assert_eq!(
            s.coord().face_selection.len(),
            0,
            "Select All leaves face selection untouched"
        );
    }

    #[test]
    fn menu_delete_command_deletes_selected_entities_and_prunes_selection() {
        // Command::Delete drained from the menu handoff reaches
        // EditorShell::delete_selected_entities. This is the bounded
        // wrapper-world path only: entity/component blobs are removed and
        // coordination selections are pruned.
        let mut s = EditorShell::new();
        build_scene(&mut s, 3);
        let ids: Vec<_> = s.world().entities().collect();
        let owner = BRepOwnerId::from_bytes([0x5d; 16]);
        let selected_face = FaceSelection {
            entity: ids[0],
            owner,
            face_id: BRepFaceId::for_cuboid_face(owner, CuboidFaceTag::PosX),
        };
        s.coord_mut().selection.add(ids[0]);
        s.coord_mut().face_selection.add(selected_face);
        s.menu_command_handoff = Some(handoff_with(&[Command::Delete]));

        s.drain_and_route_menu_commands();

        assert_eq!(s.world().entity_count(), 2);
        assert!(
            !s.world().entities().any(|id| id == ids[0]),
            "deleted entity is no longer live"
        );
        assert_eq!(
            s.world().component(ids[0], ComponentTypeId(1)),
            None,
            "deleted entity's legacy blob components are removed"
        );
        assert!(
            s.world().component(ids[1], ComponentTypeId(1)).is_some(),
            "unselected entities keep their legacy blob components"
        );
        assert!(
            s.coord().selection.is_empty(),
            "entity selection is cleared after deleting selected entities"
        );
        assert!(
            s.coord().face_selection.is_empty(),
            "face selections belonging to deleted entities are pruned"
        );
    }

    #[test]
    fn menu_duplicate_command_clones_selected_legacy_blobs() {
        // Command::Duplicate drained from the menu handoff reaches
        // EditorShell::duplicate_selected_entities. This is the bounded
        // wrapper-world path only: legacy component blobs are cloned onto new
        // entities, which become the new entity selection.
        let mut s = EditorShell::new();
        build_scene(&mut s, 2);
        let ids: Vec<_> = s.world().entities().collect();
        let owner = BRepOwnerId::from_bytes([0x6e; 16]);
        let selected_face = FaceSelection {
            entity: ids[0],
            owner,
            face_id: BRepFaceId::for_cuboid_face(owner, CuboidFaceTag::PosY),
        };
        let original_tick = s.world().component(ids[0], ComponentTypeId(1)).cloned();
        let original_position = s.world().component(ids[0], ComponentTypeId(2)).cloned();
        s.coord_mut().selection.add(ids[0]);
        s.coord_mut().face_selection.add(selected_face);
        s.menu_command_handoff = Some(handoff_with(&[Command::Duplicate]));

        s.drain_and_route_menu_commands();

        assert_eq!(s.world().entity_count(), 3);
        assert!(
            s.world().entities().any(|id| id == ids[0]),
            "the original entity remains live"
        );
        let selected_after: Vec<_> = s.coord().selection.iter().collect();
        assert_eq!(
            selected_after.len(),
            1,
            "Duplicate selects the newly created duplicate"
        );
        let duplicate = selected_after[0];
        assert_ne!(
            duplicate, ids[0],
            "selected entity after Duplicate is a fresh entity"
        );
        assert_eq!(
            s.world().component(duplicate, ComponentTypeId(1)).cloned(),
            original_tick,
            "duplicate receives cloned TickCounter blob"
        );
        assert_eq!(
            s.world().component(duplicate, ComponentTypeId(2)).cloned(),
            original_position,
            "duplicate receives cloned Position blob"
        );
        assert!(
            s.coord().face_selection.is_empty(),
            "face selection is cleared because face IDs are not remapped"
        );
    }

    #[test]
    fn menu_copy_paste_commands_clone_selected_legacy_blobs() {
        // Command::Copy stores selected wrapper-world legacy blobs in a
        // shell-local clipboard. Command::Paste spawns fresh entities from that
        // clipboard and selects the pasted entities. This is intentionally not
        // an OS clipboard or authoritative CAD/projection clone.
        let mut s = EditorShell::new();
        build_scene(&mut s, 2);
        let ids: Vec<_> = s.world().entities().collect();
        let owner = BRepOwnerId::from_bytes([0x71; 16]);
        let selected_face = FaceSelection {
            entity: ids[1],
            owner,
            face_id: BRepFaceId::for_cuboid_face(owner, CuboidFaceTag::NegZ),
        };
        let original_tick = s.world().component(ids[0], ComponentTypeId(1)).cloned();
        let original_position = s.world().component(ids[0], ComponentTypeId(2)).cloned();
        s.coord_mut().selection.add(ids[0]);
        s.menu_command_handoff = Some(handoff_with(&[Command::Copy]));

        s.drain_and_route_menu_commands();

        assert_eq!(
            s.world().entity_count(),
            2,
            "Copy does not mutate world contents"
        );
        assert!(
            s.predicate_context().has_clipboard_entities,
            "Copy publishes a non-empty Paste predicate"
        );
        assert_eq!(
            s.coord().selection.iter().collect::<Vec<_>>(),
            vec![ids[0]],
            "Copy leaves entity selection unchanged"
        );

        s.coord_mut().selection.clear();
        s.coord_mut().selection.add(ids[1]);
        s.coord_mut().face_selection.add(selected_face);
        s.menu_command_handoff = Some(handoff_with(&[Command::Paste]));

        s.drain_and_route_menu_commands();

        assert_eq!(s.world().entity_count(), 3);
        assert!(
            s.world().entities().any(|id| id == ids[0]),
            "the copied source remains live"
        );
        let selected_after: Vec<_> = s.coord().selection.iter().collect();
        assert_eq!(selected_after.len(), 1, "Paste selects the pasted entity");
        let pasted = selected_after[0];
        assert!(
            !ids.contains(&pasted),
            "Paste creates a fresh entity rather than re-selecting an original"
        );
        assert_eq!(
            s.world().component(pasted, ComponentTypeId(1)).cloned(),
            original_tick,
            "pasted entity receives cloned TickCounter blob"
        );
        assert_eq!(
            s.world().component(pasted, ComponentTypeId(2)).cloned(),
            original_position,
            "pasted entity receives cloned Position blob"
        );
        assert!(
            s.coord().face_selection.is_empty(),
            "Paste clears face selection because face IDs are not remapped"
        );
    }

    #[test]
    fn menu_cut_command_copies_then_deletes_selected_legacy_blobs() {
        // Command::Cut is intentionally bounded to the wrapper-world substrate:
        // it copies selected legacy blobs into the shell-local clipboard, then
        // delegates deletion to the same selected-entity despawn path as Delete.
        let mut s = EditorShell::new();
        build_scene(&mut s, 2);
        let ids: Vec<_> = s.world().entities().collect();
        let owner = BRepOwnerId::from_bytes([0x72; 16]);
        let selected_face = FaceSelection {
            entity: ids[0],
            owner,
            face_id: BRepFaceId::for_cuboid_face(owner, CuboidFaceTag::NegY),
        };
        let original_tick = s.world().component(ids[0], ComponentTypeId(1)).cloned();
        let original_position = s.world().component(ids[0], ComponentTypeId(2)).cloned();
        s.coord_mut().selection.add(ids[0]);
        s.coord_mut().face_selection.add(selected_face);
        s.menu_command_handoff = Some(handoff_with(&[Command::Cut]));

        s.drain_and_route_menu_commands();

        assert_eq!(s.world().entity_count(), 1);
        assert!(
            !s.world().entities().any(|id| id == ids[0]),
            "Cut removes the selected source entity"
        );
        assert!(
            s.world().entities().any(|id| id == ids[1]),
            "Cut leaves unselected entities live"
        );
        assert!(
            s.predicate_context().has_clipboard_entities,
            "Cut leaves copied legacy blobs available for Paste"
        );
        assert!(
            s.coord().selection.is_empty(),
            "Cut clears entity selection via the Delete path"
        );
        assert!(
            s.coord().face_selection.is_empty(),
            "Cut prunes face selection via the Delete path"
        );

        s.menu_command_handoff = Some(handoff_with(&[Command::Paste]));
        s.drain_and_route_menu_commands();

        assert_eq!(s.world().entity_count(), 2);
        let selected_after: Vec<_> = s.coord().selection.iter().collect();
        assert_eq!(
            selected_after.len(),
            1,
            "Paste selects the cut-pasted entity"
        );
        let pasted = selected_after[0];
        assert!(
            !ids.contains(&pasted),
            "Paste after Cut creates a fresh entity"
        );
        assert_eq!(
            s.world().component(pasted, ComponentTypeId(1)).cloned(),
            original_tick,
            "cut-pasted entity receives cloned TickCounter blob"
        );
        assert_eq!(
            s.world().component(pasted, ComponentTypeId(2)).cloned(),
            original_position,
            "cut-pasted entity receives cloned Position blob"
        );
    }

    #[test]
    fn menu_play_start_command_starts_pie() {
        // Command::PlayStart drained from the menu handoff must reach
        // handle_button(Play) — observed by the PlayState transitioning
        // Editing -> Playing (the same PIE driver as the Space key).
        let mut s = EditorShell::new();
        assert_eq!(
            s.play_state(),
            PlayState::Editing,
            "precondition: a fresh shell is in Editing"
        );
        s.menu_command_handoff = Some(handoff_with(&[Command::PlayStart]));

        s.drain_and_route_menu_commands();

        assert_eq!(
            s.play_state(),
            PlayState::Playing,
            "Command::PlayStart routes to handle_button(Play) (PIE started)"
        );
    }

    #[test]
    fn menu_play_pause_step_stop_round_trip_via_pie() {
        // The remaining three Play commands route to handle_button(Pause/Step/Stop):
        // from Playing, PlayPause -> Paused, PlayStep stays Paused (ticks once under
        // the pause gate), PlayStop -> Editing (snapshot restored).
        let mut s = EditorShell::new();
        s.menu_command_handoff = Some(handoff_with(&[Command::PlayStart]));
        s.drain_and_route_menu_commands();
        assert_eq!(s.play_state(), PlayState::Playing, "PlayStart -> Playing");

        s.menu_command_handoff = Some(handoff_with(&[Command::PlayPause]));
        s.drain_and_route_menu_commands();
        assert_eq!(
            s.play_state(),
            PlayState::Paused,
            "Command::PlayPause routes to handle_button(Pause)"
        );

        s.menu_command_handoff = Some(handoff_with(&[Command::PlayStep]));
        s.drain_and_route_menu_commands();
        assert_eq!(
            s.play_state(),
            PlayState::Paused,
            "Command::PlayStep routes to handle_button(Step) (stays Paused)"
        );

        s.menu_command_handoff = Some(handoff_with(&[Command::PlayStop]));
        s.drain_and_route_menu_commands();
        assert_eq!(
            s.play_state(),
            PlayState::Editing,
            "Command::PlayStop routes to handle_button(Stop) (restored to Editing)"
        );
    }

    #[test]
    fn menu_play_stop_while_editing_is_a_swallowed_noop() {
        // The Play menu items are STATIC, so Stop can be clicked while Editing —
        // handle_button(Stop) returns PlayStateError::NoSnapshot BEFORE mutating;
        // route_play_button swallows it and the state stays Editing.
        let mut s = EditorShell::new();
        assert_eq!(s.play_state(), PlayState::Editing);
        s.menu_command_handoff = Some(handoff_with(&[Command::PlayStop]));

        s.drain_and_route_menu_commands();

        assert_eq!(
            s.play_state(),
            PlayState::Editing,
            "an invalid-state Play menu click is a swallowed no-op"
        );
    }

    #[test]
    fn menu_reset_camera_command_reframes_via_view() {
        // Command::ResetCamera drained from the menu handoff must reach
        // EditorShell::reset_camera — observed by editor_camera reframing to the
        // live scene's bounds center after the camera was moved away.
        let positions: Vec<[f32; 3]> =
            vec![[10.0, 20.0, 30.0], [12.0, 20.0, 30.0], [10.0, 22.0, 30.0]];
        let indices: Vec<u32> = vec![0, 1, 2];
        let mesh = rge_brep_render::RenderMesh::from_buffers(&positions, &indices, None);
        let mut s = EditorShell::with_render_mesh(mesh);
        s.editor_camera.target = glam::Vec3::ZERO;
        s.editor_camera.eye = glam::Vec3::splat(999.0);
        s.menu_command_handoff = Some(handoff_with(&[Command::ResetCamera]));

        s.drain_and_route_menu_commands();

        assert_eq!(
            s.editor_camera.target,
            glam::Vec3::new(11.0, 21.0, 30.0),
            "Command::ResetCamera routes to reset_camera (camera reframed to scene bounds)"
        );
    }

    #[test]
    fn menu_zoom_commands_route_via_view() {
        // Command::ZoomIn / ZoomOut drained from the menu handoff must reach the
        // View camera zoom helpers. Observed by eye-target distance changing
        // while target stays fixed.
        let mut s = EditorShell::new();
        s.editor_camera.target = glam::Vec3::ZERO;
        s.editor_camera.eye = glam::Vec3::new(0.0, 0.0, 10.0);
        s.menu_command_handoff = Some(handoff_with(&[Command::ZoomIn]));

        s.drain_and_route_menu_commands();

        assert_eq!(s.editor_camera.target, glam::Vec3::ZERO);
        assert!(
            ((s.editor_camera.eye - s.editor_camera.target).length() - 8.0).abs() < 1e-5,
            "Command::ZoomIn routes to zoom_camera_in"
        );

        s.menu_command_handoff = Some(handoff_with(&[Command::ZoomOut]));
        s.drain_and_route_menu_commands();

        assert!(
            ((s.editor_camera.eye - s.editor_camera.target).length() - 10.0).abs() < 1e-5,
            "Command::ZoomOut routes to zoom_camera_out"
        );
    }

    #[test]
    fn menu_toggle_command_palette_sets_one_shot_request() {
        // View -> Command Palette and Ctrl+Shift+P both route the same core
        // command here. The shell records activation as a one-shot request so
        // the egui host can toggle the palette window during render.
        let mut s = EditorShell::new();
        s.menu_command_handoff = Some(handoff_with(&[Command::ToggleCommandPalette]));

        s.drain_and_route_menu_commands();

        assert!(
            s.take_command_palette_toggle_request(),
            "ToggleCommandPalette sets a pending palette-toggle request"
        );
        assert!(
            !s.take_command_palette_toggle_request(),
            "the pending palette-toggle request is single-consume"
        );
        assert_eq!(
            s.save_source(),
            None,
            "toggling the future palette does not run document handlers"
        );
        assert!(
            s.drain_extension_menu_commands().is_empty(),
            "the core palette toggle is not captured as an extension command"
        );
    }

    #[test]
    fn menu_extension_commands_without_handler_remain_observable_fifo() {
        // Command::Custom / Command::Plugin are extension activations. The shell
        // retains them FIFO when no handler is configured instead of dropping
        // them at the routing boundary.
        let plugin = Command::Plugin {
            plugin_id: "com.example.mesh-audit".to_owned(),
            action_id: "open-panel".to_owned(),
        };
        let custom = Command::Custom("plugin.export.scene".to_owned());
        let mut s = EditorShell::new();
        s.menu_command_handoff = Some(handoff_with(&[plugin.clone(), custom.clone()]));

        s.drain_and_route_menu_commands();

        assert_eq!(
            s.drain_extension_menu_commands(),
            vec![plugin, custom],
            "extension commands are retained FIFO when no handler is configured"
        );
        assert!(
            s.drain_extension_menu_commands().is_empty(),
            "extension command drain is one-shot"
        );
        assert_eq!(
            s.drain_extension_command_events(),
            vec![
                ExtensionCommandEvent::MissingHandler {
                    command: Command::Plugin {
                        plugin_id: "com.example.mesh-audit".to_owned(),
                        action_id: "open-panel".to_owned(),
                    },
                },
                ExtensionCommandEvent::MissingHandler {
                    command: Command::Custom("plugin.export.scene".to_owned()),
                },
            ],
            "missing-handler activations are also observable as seam events"
        );
        assert_eq!(
            s.save_source(),
            None,
            "capturing extension commands does not run document handlers"
        );
    }

    #[test]
    fn menu_extension_commands_are_delivered_to_configured_handler_fifo() {
        // The injected seam receives only commands that first passed through the
        // shell's extension capture path, preserving host FIFO order.
        let plugin = Command::Plugin {
            plugin_id: "com.example.mesh-audit".to_owned(),
            action_id: "open-panel".to_owned(),
        };
        let custom = Command::Custom("plugin.export.scene".to_owned());
        let (handler, seen) = extension_handler(vec![
            Ok(ExtensionCommandOutcome::Handled),
            Ok(ExtensionCommandOutcome::Handled),
        ]);
        let mut s = EditorShell::new().with_extension_command_handler(Box::new(handler));
        s.menu_command_handoff = Some(handoff_with(&[plugin.clone(), custom.clone()]));

        s.drain_and_route_menu_commands();

        assert_eq!(
            seen.borrow().clone(),
            vec![plugin.clone(), custom.clone()],
            "configured extension handler receives Plugin then Custom FIFO"
        );
        assert!(
            s.drain_extension_menu_commands().is_empty(),
            "configured handler drains captured extension commands"
        );
        assert_eq!(
            s.drain_extension_command_events(),
            vec![
                ExtensionCommandEvent::Handled { command: plugin },
                ExtensionCommandEvent::Handled { command: custom },
            ],
            "handled outcomes are observable"
        );
    }

    #[test]
    fn menu_core_commands_are_not_delivered_to_extension_handler() {
        // Representative core commands keep their existing menu behavior while
        // the extension seam receives only the captured extension command.
        let plugin = Command::Plugin {
            plugin_id: "com.example.mesh-audit".to_owned(),
            action_id: "open-panel".to_owned(),
        };
        let (handler, seen) = extension_handler(vec![Ok(ExtensionCommandOutcome::Handled)]);
        let (hook, calls) = save_hook(false);
        let mut s = EditorShell::new()
            .with_scene_save_dialog(Box::new(MockSaveDialog {
                result: Some(std::path::PathBuf::from("/tmp/level.rge-scene")),
            }))
            .with_scene_save_hook(Box::new(hook))
            .with_extension_command_handler(Box::new(handler));
        s.set_time_scale(2.0);
        assert!(s.command_bus().is_dirty());
        s.menu_command_handoff = Some(handoff_with(&[
            Command::Save,
            plugin.clone(),
            Command::ToggleCommandPalette,
        ]));

        s.drain_and_route_menu_commands();

        assert_eq!(calls.get(), 1, "Save still routes to the save handler");
        assert!(
            !s.command_bus().is_dirty(),
            "successful Save still marks the bus saved"
        );
        assert!(
            s.take_command_palette_toggle_request(),
            "ToggleCommandPalette still sets the shell one-shot request"
        );
        assert_eq!(
            seen.borrow().clone(),
            vec![plugin.clone()],
            "extension handler does not receive Save or ToggleCommandPalette"
        );
        assert_eq!(
            s.drain_extension_command_events(),
            vec![ExtensionCommandEvent::Handled { command: plugin }],
            "only the extension command records a handler outcome"
        );
    }

    #[test]
    fn menu_extension_handler_failure_and_unhandled_are_nonfatal() {
        // Failure and unhandled results are recorded, do not run document
        // handlers, and do not stop later extension commands from being
        // delivered to the same injected handler.
        let first = Command::Plugin {
            plugin_id: "com.example.mesh-audit".to_owned(),
            action_id: "fail".to_owned(),
        };
        let second = Command::Custom("plugin.unhandled".to_owned());
        let third = Command::Plugin {
            plugin_id: "com.example.mesh-audit".to_owned(),
            action_id: "handled-after-failure".to_owned(),
        };
        let (handler, seen) = extension_handler(vec![
            Err(ExtensionCommandError::new("synthetic handler failure")),
            Ok(ExtensionCommandOutcome::Unhandled),
            Ok(ExtensionCommandOutcome::Handled),
        ]);
        let mut s = EditorShell::new().with_extension_command_handler(Box::new(handler));
        s.menu_command_handoff = Some(handoff_with(&[
            first.clone(),
            second.clone(),
            third.clone(),
        ]));

        s.drain_and_route_menu_commands();

        assert_eq!(
            seen.borrow().clone(),
            vec![first.clone(), second.clone(), third.clone()],
            "handler failure/unhandled results do not stop later commands"
        );
        assert!(
            s.drain_extension_menu_commands().is_empty(),
            "all captured extension commands were drained to the handler"
        );
        assert_eq!(
            s.save_source(),
            None,
            "extension handler failure/unhandled paths do not run document handlers"
        );
        assert_eq!(
            s.drain_extension_command_events(),
            vec![
                ExtensionCommandEvent::Failed {
                    command: first,
                    error: "synthetic handler failure".to_owned(),
                },
                ExtensionCommandEvent::Unhandled { command: second },
                ExtensionCommandEvent::Handled { command: third },
            ],
            "failure, unhandled, and later handled outcomes are observable"
        );
    }

    #[test]
    fn render_frame_drains_menu_commands_at_its_top() {
        // The route tests above call `drain_and_route_menu_commands`
        // DIRECTLY; this pins that `render_frame` (the sole redraw entry) actually
        // invokes the drain at its TOP — before this frame's surface/window
        // borrows — by routing an enqueued `Command::Save` through to the save
        // hook even on a headless shell. With no render init, `render_frame`
        // early-returns via `render_frame_egui_only` (which guards on the absent
        // surface), but the top-of-frame drain runs first. A refactor that moved
        // the drain below the borrows would fail this test (and reintroduce the
        // borrow hazard the placement avoids).
        let (hook, calls) = save_hook(false);
        let mut s = EditorShell::new()
            .with_scene_save_dialog(Box::new(MockSaveDialog {
                result: Some(std::path::PathBuf::from("/tmp/level.rge-scene")),
            }))
            .with_scene_save_hook(Box::new(hook));
        s.set_time_scale(2.0);
        assert!(s.command_bus().is_dirty());
        s.menu_command_handoff = Some(handoff_with(&[Command::Save]));

        let _ = s.render_frame();

        assert_eq!(
            calls.get(),
            1,
            "render_frame must drain + route the enqueued menu Command at its top"
        );
        assert!(
            !s.command_bus().is_dirty(),
            "the routed menu Save marked the bus saved"
        );
    }
}
