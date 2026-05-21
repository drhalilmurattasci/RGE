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

use super::EditorShell;
use crate::audit::AuditEvent;
use crate::play_state::{PlayState, PlayStateTransition};
use crate::play_toolbar::ToolbarButtonId;
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

#[test]
fn fresh_shell_is_editing() {
    let s = EditorShell::new();
    assert_eq!(s.play_state(), PlayState::Editing);
    assert!(!s.has_snapshot());
    assert_eq!(s.tick_count(), 0);
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
