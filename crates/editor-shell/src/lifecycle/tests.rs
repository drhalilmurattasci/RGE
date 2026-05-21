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
