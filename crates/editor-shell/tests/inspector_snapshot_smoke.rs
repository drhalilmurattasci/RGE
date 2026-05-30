//! Phase 9 dispatch — headless `InspectorSnapshot` smoke tests.
//!
//! Pins the model contract for a future inspector widget: every field
//! reflects the matching public `EditorShell` accessor at call time,
//! the struct is `Copy + Send + Sync`, and the snapshot is a pure read
//! with no side effects on the editor state.
//!
//! No `editor-ui` widget is involved here; this dispatch ships the
//! headless model only. The tests use only existing public state — no
//! synthetic data, no fake reflection adoption.

use rge_editor_shell::{EditorShell, InspectorSnapshot, TimeScale, ToolbarButtonId};
use rge_editor_state::ActiveTool;

// ---------------------------------------------------------------------------
// Defaults
// ---------------------------------------------------------------------------

#[test]
fn fresh_shell_snapshot_reads_defaults() {
    // A brand-new `EditorShell::new()` must produce a snapshot whose
    // every field reads the corresponding default:
    // - TimeScale resource installed at `TimeScale::DEFAULT` (= 1.0).
    // - PlayState `Editing`.
    // - tick_count 0, no PIE snapshot held.
    // - ActiveTool default `Select`.
    // - empty Selection / FaceSelection.
    // - bus is clean (not dirty), empty undo stack.
    let shell = EditorShell::new();
    let s = shell.inspector_snapshot();

    assert!(
        (s.time_scale - TimeScale::DEFAULT).abs() < f32::EPSILON,
        "fresh shell time_scale must be DEFAULT (1.0); got {}",
        s.time_scale
    );
    assert_eq!(s.play_state_label, "Editing");
    assert_eq!(s.tick_count, 0);
    assert!(!s.has_snapshot);
    assert_eq!(s.active_tool_label, ActiveTool::default().label());
    assert_eq!(s.selection_len, 0);
    assert_eq!(s.face_selection_len, 0);
    assert!(!s.is_dirty);
    assert_eq!(s.undo_stack_len, 0);
    assert_eq!(s.undo_cursor, 0);
}

// ---------------------------------------------------------------------------
// TimeScale + dirty-flag interaction
// ---------------------------------------------------------------------------

#[test]
fn snapshot_reflects_time_scale_change_and_dirty_flag() {
    // `set_time_scale` is now a real production bus submit source. After
    // one non-no-op submit, the snapshot must reflect (a) the new
    // TimeScale, (b) dirty=true (cursor advanced past mark_saved), and
    // (c) undo_stack_len=1 / undo_cursor=1.
    let mut shell = EditorShell::new();
    shell.set_time_scale(2.5);
    let s = shell.inspector_snapshot();

    assert!(
        (s.time_scale - 2.5).abs() < f32::EPSILON,
        "after set_time_scale(2.5), snapshot.time_scale must be 2.5; got {}",
        s.time_scale
    );
    assert!(s.is_dirty, "non-no-op submit must flip is_dirty");
    assert_eq!(s.undo_stack_len, 1);
    assert_eq!(s.undo_cursor, 1);
}

#[test]
fn snapshot_reflects_mark_saved_clearing_dirty() {
    // `mark_saved_command()` snaps the saved cursor to the current cursor,
    // returning the bus to a clean state, and the snapshot must reflect the
    // is_dirty transition. (Ctrl+S now routes through Save and only marks saved
    // after a successful write (SCENE-SAVE-WIRING); this test pins the direct
    // mark-saved path that the inspector dirty flag mirrors.)
    let mut shell = EditorShell::new();
    shell.set_time_scale(0.5);
    assert!(shell.inspector_snapshot().is_dirty);

    shell.mark_saved_command();
    let s = shell.inspector_snapshot();

    assert!(
        !s.is_dirty,
        "mark_saved must clear is_dirty in the snapshot"
    );
    assert_eq!(s.undo_stack_len, 1, "stack length unchanged by mark_saved");
    assert_eq!(s.undo_cursor, 1, "cursor unchanged by mark_saved");
}

// ---------------------------------------------------------------------------
// PlayState transitions
// ---------------------------------------------------------------------------

#[test]
fn snapshot_reflects_play_state_transitions() {
    let mut shell = EditorShell::new();
    assert_eq!(shell.inspector_snapshot().play_state_label, "Editing");
    assert!(!shell.inspector_snapshot().has_snapshot);

    shell.handle_button(ToolbarButtonId::Play).unwrap();
    let after_play = shell.inspector_snapshot();
    assert_eq!(after_play.play_state_label, "Playing");
    assert!(
        after_play.has_snapshot,
        "Play must capture a WorldSnapshot reflected by has_snapshot"
    );

    shell.handle_button(ToolbarButtonId::Pause).unwrap();
    assert_eq!(shell.inspector_snapshot().play_state_label, "Paused");

    shell.handle_button(ToolbarButtonId::Stop).unwrap();
    let after_stop = shell.inspector_snapshot();
    assert_eq!(after_stop.play_state_label, "Editing");
    assert!(
        !after_stop.has_snapshot,
        "Stop must release the WorldSnapshot reflected by has_snapshot"
    );
}

#[test]
fn snapshot_tick_count_advances_only_in_playing() {
    let mut shell = EditorShell::new();
    assert_eq!(shell.inspector_snapshot().tick_count, 0);

    // Editing: ticks are not advanced by run_for_redraws.
    shell.run_for_redraws(5);
    assert_eq!(
        shell.inspector_snapshot().tick_count,
        0,
        "Editing must not advance tick_count"
    );

    // Playing: ticks advance per redraw.
    shell.handle_button(ToolbarButtonId::Play).unwrap();
    shell.run_for_redraws(7);
    let after_play = shell.inspector_snapshot();
    assert_eq!(after_play.tick_count, 7);
    assert_eq!(after_play.play_state_label, "Playing");
}

// ---------------------------------------------------------------------------
// Selection counts
// ---------------------------------------------------------------------------

#[test]
fn snapshot_reflects_selection_changes() {
    use rge_kernel_ecs::EntityId;
    let mut shell = EditorShell::new();
    assert_eq!(shell.inspector_snapshot().selection_len, 0);

    // EditorCoord lives outside the bus per PLAN §1.15
    // (coordination-not-authority); mutations are direct, not bus-routed.
    // The snapshot still reads them correctly because it consults the
    // live `self.coord` at call time.
    let e1 = EntityId::new();
    let e2 = EntityId::new();
    shell.coord_mut().selection.add(e1);
    shell.coord_mut().selection.add(e2);
    assert_eq!(shell.inspector_snapshot().selection_len, 2);

    shell.coord_mut().selection.clear();
    assert_eq!(shell.inspector_snapshot().selection_len, 0);
}

#[test]
fn snapshot_reflects_active_tool_changes() {
    let mut shell = EditorShell::new();
    assert_eq!(shell.inspector_snapshot().active_tool_label, "Select");

    shell.coord_mut().active_tool = ActiveTool::Translate;
    assert_eq!(
        shell.inspector_snapshot().active_tool_label,
        "Translate",
        "snapshot must follow the active tool through direct mutation"
    );

    shell.coord_mut().active_tool = ActiveTool::Brush;
    assert_eq!(shell.inspector_snapshot().active_tool_label, "Brush");
}

// ---------------------------------------------------------------------------
// Undo-stack progression via the real bus submit source
// ---------------------------------------------------------------------------

#[test]
fn snapshot_reflects_undo_stack_progression_via_time_scale() {
    use rge_editor_shell::EditorKeyCommand;

    // Use `set_time_scale(...)` as the bus submit source — the only
    // existing production-grade Action in the workspace. Avoids fake
    // test-only Actions inside this dispatch's test surface.
    let mut shell = EditorShell::new();
    let s0 = shell.inspector_snapshot();
    assert_eq!(s0.undo_stack_len, 0);
    assert_eq!(s0.undo_cursor, 0);

    // Submit three distinct non-no-op SetTimeScale actions. They share
    // a constant ActionId so the bus's 500ms coalesce window collapses
    // them into one stack entry whose `from` is the pre-burst value
    // (1.0) and whose `to` is the latest (3.5). The snapshot therefore
    // reads stack_len=1 / cursor=1 after the burst.
    shell.set_time_scale(1.5);
    shell.set_time_scale(2.5);
    shell.set_time_scale(3.5);
    let after_burst = shell.inspector_snapshot();
    assert_eq!(after_burst.undo_stack_len, 1);
    assert_eq!(after_burst.undo_cursor, 1);
    assert!(after_burst.is_dirty);
    assert!(
        (after_burst.time_scale - 3.5).abs() < f32::EPSILON,
        "coalesced burst lands on the latest `to`"
    );

    // Ctrl+Z reverts the merged entry → cursor → 0, but stack still has
    // the entry (it sits past the cursor, in the redo tail).
    shell.handle_key_command(EditorKeyCommand::Undo);
    let after_undo = shell.inspector_snapshot();
    assert_eq!(after_undo.undo_stack_len, 1);
    assert_eq!(after_undo.undo_cursor, 0);
    assert!(
        (after_undo.time_scale - 1.0).abs() < f32::EPSILON,
        "Ctrl+Z must restore the pre-burst TimeScale (1.0)"
    );

    // Ctrl+Y re-applies → cursor → 1 again.
    shell.handle_key_command(EditorKeyCommand::Redo);
    let after_redo = shell.inspector_snapshot();
    assert_eq!(after_redo.undo_stack_len, 1);
    assert_eq!(after_redo.undo_cursor, 1);
    assert!((after_redo.time_scale - 3.5).abs() < f32::EPSILON);
}

// ---------------------------------------------------------------------------
// Trait bounds
// ---------------------------------------------------------------------------

#[test]
fn inspector_snapshot_is_copy_send_sync() {
    // Compile-time trait-bound smoke test. If a future change adds a
    // non-Copy / non-Send / non-Sync field to InspectorSnapshot the
    // assertion below fails to compile, alerting before the trait
    // contract drifts.
    fn assert_copy_send_sync<T: Copy + Send + Sync + 'static>() {}
    assert_copy_send_sync::<InspectorSnapshot>();
}

#[test]
fn inspector_snapshot_default_is_zeroed() {
    // `#[derive(Default)]` must produce a sensible all-zero default —
    // useful for consumers that want to construct a snapshot ad-hoc
    // (test fixtures, future widget unit tests).
    let s = InspectorSnapshot::default();
    assert_eq!(s.time_scale, 0.0);
    assert_eq!(s.play_state_label, "");
    assert_eq!(s.tick_count, 0);
    assert!(!s.has_snapshot);
    assert_eq!(s.active_tool_label, "");
    assert_eq!(s.selection_len, 0);
    assert_eq!(s.face_selection_len, 0);
    assert!(!s.is_dirty);
    assert_eq!(s.undo_stack_len, 0);
    assert_eq!(s.undo_cursor, 0);
}

#[test]
fn inspector_snapshot_round_trip_is_pure_read() {
    // Building the snapshot must not mutate observable editor state.
    // Take two back-to-back snapshots with no intervening mutation and
    // assert byte-equality.
    let shell = EditorShell::new();
    let s1 = shell.inspector_snapshot();
    let s2 = shell.inspector_snapshot();
    assert_eq!(s1, s2, "back-to-back snapshots must be byte-identical");
}
