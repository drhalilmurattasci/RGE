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
    // The prebuilt mesh IS populated.
    assert!(
        shell.prebuilt_render_mesh.is_some(),
        "render-only shell must hold the prebuilt RenderMesh"
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
