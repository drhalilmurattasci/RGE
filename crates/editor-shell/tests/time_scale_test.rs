//! W03 exit-criterion test: time-scale slider affects game systems but
//! NOT editor systems. Per PLAN.md constitutional principle #8: the
//! editor never freezes — game-time dilation must not slow gizmos,
//! panel animations, or the hot-reload watcher.

use rge_editor_shell::world::ComponentTypeId;
use rge_editor_shell::{EditorShell, TimeScale, TimeScaleClass, ToolbarButtonId};

const POSITION: ComponentTypeId = ComponentTypeId(2);

fn position_x(shell: &EditorShell, e: rge_editor_shell::world::EntityId) -> f32 {
    let blob = shell.world().component(e, POSITION).expect("position");
    let mut bytes = [0u8; 4];
    bytes.copy_from_slice(&blob[0..4]);
    f32::from_le_bytes(bytes)
}

#[test]
fn time_scale_within_range() {
    let t = TimeScale::with_value(0.5);
    assert!(t.value() >= TimeScale::MIN);
    assert!(t.value() <= TimeScale::MAX);

    let clamped_low = TimeScale::with_value(-10.0);
    assert!((clamped_low.value() - TimeScale::MIN).abs() < f32::EPSILON);

    let clamped_high = TimeScale::with_value(100.0);
    assert!((clamped_high.value() - TimeScale::MAX).abs() < f32::EPSILON);
}

#[test]
fn slow_motion_halves_game_progress() {
    let mut shell = EditorShell::new();
    let e = shell.world_mut().spawn();
    shell
        .world_mut()
        .insert_component(e, POSITION, vec![0u8; 12]);

    // 0.5x means 60 game ticks should advance position by ~half compared
    // to 1.0x.
    shell.set_time_scale(0.5);
    shell.handle_button(ToolbarButtonId::Play).unwrap();
    shell.run_for_redraws(60);
    let x_half = position_x(&shell, e);

    shell.handle_button(ToolbarButtonId::Stop).unwrap();
    shell.set_time_scale(1.0);
    shell.handle_button(ToolbarButtonId::Play).unwrap();
    shell.run_for_redraws(60);
    let x_full = position_x(&shell, e);

    // x_full should be ~2x x_half (within FP noise).
    assert!(x_half > 0.0);
    assert!(x_full > 0.0);
    let ratio = x_full / x_half;
    assert!(
        (ratio - 2.0).abs() < 0.01,
        "expected ~2.0x ratio, got {ratio}"
    );
}

#[test]
fn fast_forward_doubles_game_progress() {
    let mut shell = EditorShell::new();
    let e = shell.world_mut().spawn();
    shell
        .world_mut()
        .insert_component(e, POSITION, vec![0u8; 12]);

    shell.set_time_scale(2.0);
    shell.handle_button(ToolbarButtonId::Play).unwrap();
    shell.run_for_redraws(30);
    let x_2x_30 = position_x(&shell, e);

    shell.handle_button(ToolbarButtonId::Stop).unwrap();
    shell.set_time_scale(1.0);
    shell.handle_button(ToolbarButtonId::Play).unwrap();
    shell.run_for_redraws(30);
    let x_1x_30 = position_x(&shell, e);

    assert!(x_2x_30 > x_1x_30);
    let ratio = x_2x_30 / x_1x_30;
    assert!(
        (ratio - 2.0).abs() < 0.01,
        "expected ~2.0x ratio, got {ratio}"
    );
}

#[test]
fn editor_systems_ignore_time_scale() {
    let scale = TimeScale::with_value(0.01); // extreme slow-motion
    let dt = 0.016_f32;
    let editor_dt = scale.apply(dt, TimeScaleClass::Editor);
    assert!(
        (editor_dt - dt).abs() < 1e-6,
        "editor systems must see raw dt regardless of slider"
    );
}

#[test]
fn time_scale_audit_event_records_change() {
    let mut shell = EditorShell::new();
    shell.set_time_scale(0.25);
    shell.set_time_scale(2.5);

    let mut tsc_count = 0;
    for e in shell.audit().iter() {
        if e.tag() == "TimeScaleChanged" {
            tsc_count += 1;
        }
    }
    assert_eq!(tsc_count, 2);
}

#[test]
fn extreme_min_scale_no_underflow() {
    let mut shell = EditorShell::new();
    let e = shell.world_mut().spawn();
    shell
        .world_mut()
        .insert_component(e, POSITION, vec![0u8; 12]);

    shell.set_time_scale(TimeScale::MIN);
    shell.handle_button(ToolbarButtonId::Play).unwrap();
    shell.run_for_redraws(120);
    let x = position_x(&shell, e);

    // x = 120 ticks * (1/60 dt) * 0.01 scale = 0.02
    assert!(x > 0.0);
    assert!(x < 0.05);
}

// ===========================================================================
// Phase 9 — TimeScale via CommandBus (new dispatch)
//
// `set_time_scale` now routes through the bus. The four tests below assert:
// (1) full undo/redo round-trip via Ctrl+Z/Y bindings; (2) rapid drags
// coalesce into one stack entry per the 500 ms coalesce window;
// (3) TimeScale ECS resource persists across Play/Stop (resources are not
// in WorldSnapshot); (4) the dual-ledger arrangement still records
// `TimeScaleChanged` on the editor-shell audit ledger per locked decision #3.
// ===========================================================================

#[test]
fn set_time_scale_routes_through_bus_and_undo_redo_round_trips() {
    use rge_editor_shell::EditorKeyCommand;

    let mut shell = EditorShell::new();
    assert!(
        (shell.time_scale().value() - TimeScale::DEFAULT).abs() < f32::EPSILON,
        "fresh shell must start at TimeScale::DEFAULT (1.0)"
    );
    assert_eq!(
        shell.command_bus().stack().len(),
        0,
        "fresh shell must have an empty undo stack"
    );

    // Submit a single SetTimeScale via the public slider API.
    shell.set_time_scale(2.5);
    assert!(
        (shell.time_scale().value() - 2.5).abs() < f32::EPSILON,
        "after set_time_scale(2.5), accessor must read 2.5 from the resource"
    );
    assert_eq!(
        shell.command_bus().stack().len(),
        1,
        "submit must have pushed exactly one SetTimeScale entry"
    );
    assert!(
        shell.command_bus().is_dirty(),
        "bus must be dirty after submit"
    );

    // Ctrl+Z reverts back to the pre-submit value (1.0 = DEFAULT).
    shell.handle_key_command(EditorKeyCommand::Undo);
    assert!(
        (shell.time_scale().value() - TimeScale::DEFAULT).abs() < f32::EPSILON,
        "Ctrl+Z must restore TimeScale to its pre-submit value"
    );

    // Ctrl+Y reapplies.
    shell.handle_key_command(EditorKeyCommand::Redo);
    assert!(
        (shell.time_scale().value() - 2.5).abs() < f32::EPSILON,
        "Ctrl+Y must re-apply SetTimeScale(2.5)"
    );

    // Stack length is unchanged through undo/redo (cursor moves, not the
    // stack itself).
    assert_eq!(
        shell.command_bus().stack().len(),
        1,
        "undo/redo must not alter stack length"
    );
}

#[test]
fn rapid_time_scale_changes_coalesce_to_one_stack_entry() {
    let mut shell = EditorShell::new();

    // Five rapid drags within the bus's 500 ms coalesce window — all
    // carry the same ActionId (SET_TIME_SCALE_ID = "set-time-scale"), so
    // editor-actions §6.16.7 / coalesce_window.rs collapses them into one
    // stack entry. The merge implementation keeps the original `from`
    // (1.0 = pre-drag) while adopting the newer `to` on every drag event.
    shell.set_time_scale(1.5);
    shell.set_time_scale(2.0);
    shell.set_time_scale(2.5);
    shell.set_time_scale(3.0);
    shell.set_time_scale(3.5);

    assert!(
        (shell.time_scale().value() - 3.5).abs() < f32::EPSILON,
        "final value after the drag is the most recent submit"
    );
    assert_eq!(
        shell.command_bus().stack().len(),
        1,
        "rapid drags within 500 ms must coalesce to ONE stack entry; \
         got {} entries (coalesce window broken or merge() rejected)",
        shell.command_bus().stack().len()
    );

    // One Ctrl+Z reverts the entire drag to the pre-drag value (1.0).
    // The merged entry's `revert` uses the original `from`, not the
    // most-recent intermediate value.
    use rge_editor_shell::EditorKeyCommand;
    shell.handle_key_command(EditorKeyCommand::Undo);
    assert!(
        (shell.time_scale().value() - TimeScale::DEFAULT).abs() < f32::EPSILON,
        "single Ctrl+Z must restore the pre-drag value (1.0), not an \
         intermediate drag value"
    );
}

#[test]
fn time_scale_resource_persists_across_play_stop() {
    // Phase 9 locked decision #2: TimeScale lives in `rge_kernel_ecs::World`
    // as a resource, and `WorldSnapshot::capture` only serializes typed
    // snapshot components + the legacy blob storage. Resources are NOT
    // captured, so the slider value MUST persist across Play/Stop —
    // matching the pre-migration behaviour (where TimeScale was an
    // EditorShell field and therefore obviously not in the snapshot).
    let mut shell = EditorShell::new();
    shell.set_time_scale(0.5);
    assert!((shell.time_scale().value() - 0.5).abs() < f32::EPSILON);

    // Press Play (captures a snapshot of the world's snapshot components),
    // run a few ticks, then Stop (restores from snapshot).
    shell.handle_button(ToolbarButtonId::Play).unwrap();
    shell.run_for_redraws(5);
    shell.handle_button(ToolbarButtonId::Stop).unwrap();

    // After Stop the slider must still read 0.5 — the snapshot path did
    // not touch the TimeScale resource.
    assert!(
        (shell.time_scale().value() - 0.5).abs() < f32::EPSILON,
        "TimeScale resource must persist across Play/Stop; got {}",
        shell.time_scale().value()
    );
}

#[test]
fn time_scale_changed_audit_event_still_recorded_after_migration() {
    // Locked decision #3: dual-ledger by design. The bus's internal
    // AuditLedger records `EventKind::Action` per submit; the
    // editor-shell's own AuditLedger (`shell.audit()`) continues to
    // record `AuditEvent::TimeScaleChanged { from, to }` per call to
    // `set_time_scale`. This test pins that the audit event is still
    // emitted post-migration so the existing
    // `time_scale_audit_event_records_change` test (above) keeps the
    // counting invariant and so any consumer of the ring-buffer audit
    // ledger continues to observe the change.
    let mut shell = EditorShell::new();
    shell.set_time_scale(0.25);
    shell.set_time_scale(2.5);

    let mut tsc = 0;
    let mut last_from = None;
    let mut last_to = None;
    for e in shell.audit().iter() {
        if let rge_editor_shell::audit::AuditEvent::TimeScaleChanged { from, to } = e {
            tsc += 1;
            last_from = Some(*from);
            last_to = Some(*to);
        }
    }
    assert_eq!(
        tsc, 2,
        "two `set_time_scale` calls must emit two TimeScaleChanged events"
    );
    assert!(
        (last_from.unwrap() - 0.25).abs() < f32::EPSILON,
        "last TimeScaleChanged.from must be 0.25 (pre-second-submit slider value)"
    );
    assert!(
        (last_to.unwrap() - 2.5).abs() < f32::EPSILON,
        "last TimeScaleChanged.to must be 2.5"
    );
}
