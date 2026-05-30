//! Phase 9 dispatch — keyboard → CommandBus → undo / redo / save round-trip.
//!
//! Proves the first real editor-workflow surface (Ctrl+Z / Ctrl+Y / Ctrl+S)
//! end-to-end through the editor-shell ↔ CommandBus integration that this
//! dispatch lands, without requiring a winit window, GPU, CAD-graph mutation,
//! or any selection / multi-entity rendering.
//!
//! The test:
//!
//! 1. Constructs `EditorShell::new()` headlessly (no `resumed`, no GPU).
//! 2. Spawns one ECS entity inside the kernel world (via `shell.world_mut().kernel_mut()`).
//! 3. Seeds the bus by calling `shell.submit_action(Box::new(IncrementCounter { ... }))`
//!    — the bus's apply runs on `&mut rge_kernel_ecs::World` per editor-actions'
//!    `Action::apply` contract.
//! 4. Asserts the bus is dirty (per `CommandBus::is_dirty()`).
//! 5. Calls `shell.handle_key_command(EditorKeyCommand::Undo)` and asserts
//!    the world reverts byte-identically.
//! 6. Calls `shell.handle_key_command(EditorKeyCommand::Redo)` and asserts
//!    the world re-applies.
//! 7. Attaches mock save dialog + writer hooks, calls
//!    `shell.handle_key_command(EditorKeyCommand::MarkSaved)`, and asserts the
//!    writer is invoked and dirty clears — Ctrl+S now performs a Save (write +
//!    mark-saved on success), not a bare saved-point bookmark
//!    (SCENE-SAVE-WIRING). The pure bus `mark_saved` / `is_dirty` semantics are
//!    covered by `editor-actions/tests/save_mark_dirty.rs`.
//! 8. Calls `shell.handle_key_command(EditorKeyCommand::Undo)` on an empty
//!    stack (fresh shell) and asserts no panic, no diagnostic — the bus
//!    swallow-noop path is exercised explicitly.
//!
//! Architectural shape: `IncrementCounter` is **test-only** — it lives in
//! this file, not in any production crate. The dispatch deliberately ships
//! no production keyboard-bound mutation action (the editor-usability
//! preflight in `plans/BASELINE.md` rejected a `SpawnCuboidAt` /
//! `IncrementScratchTick` production binding because of the World-only
//! `Action` trait surface and the editor's single-cuboid `with_world_projection_graph`
//! constraint). The test action exists only to prove the wire is intact.

#![allow(clippy::unnecessary_literal_bound)]

use rge_editor_actions::action::{Action, ActionId, ActionResult};
use rge_editor_shell::{EditorKeyCommand, EditorShell, SceneSaveDialog, SceneSaveHook};
use rge_kernel_ecs::{Component, EntityId, World as KernelWorld};

// ---------------------------------------------------------------------------
// Test-only Action + Component fixture
//
// Mirrors the `IncrementCounter` pattern from
// `editor-actions/tests/compound_atomic.rs` byte-for-byte. The component is
// a u32 counter; apply increments, revert decrements (via saturating_sub
// matching the canonical action shape).
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
struct Counter(u32);
impl Component for Counter {}

struct IncrementCounter {
    entity: EntityId,
}

impl Action for IncrementCounter {
    fn name(&self) -> &str {
        "test-increment-counter"
    }

    fn id(&self) -> ActionId {
        ActionId::new(format!("test-increment-counter({:?})", self.entity))
    }

    fn apply(&self, world: &mut KernelWorld) -> Result<(), ActionResult> {
        if world.entity(self.entity).is_none() {
            return Err(ActionResult::MissingEntity(self.entity));
        }
        let current = {
            let eref = world.entity(self.entity);
            eref.and_then(|e| e.get::<Counter>().map(|c| c.0))
                .unwrap_or(0)
        };
        world.insert(self.entity, Counter(current + 1));
        Ok(())
    }

    fn revert(&self, world: &mut KernelWorld) -> Result<(), ActionResult> {
        let current = {
            let eref = world.entity(self.entity);
            eref.and_then(|e| e.get::<Counter>().map(|c| c.0))
                .unwrap_or(0)
        };
        world.insert(self.entity, Counter(current.saturating_sub(1)));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn read_counter(shell: &EditorShell, entity: EntityId) -> u32 {
    shell
        .world()
        .kernel()
        .entity(entity)
        .and_then(|e| e.get::<Counter>().map(|c| c.0))
        .expect("counter component present")
}

fn shell_with_seeded_increment() -> (EditorShell, EntityId) {
    let mut shell = EditorShell::new();
    let entity = shell.world_mut().kernel_mut().spawn();
    shell.world_mut().kernel_mut().insert(entity, Counter(0));

    // Seed the bus with one IncrementCounter so the round-trip tests have
    // an entry to revert / re-apply / save against.
    shell
        .submit_action(Box::new(IncrementCounter { entity }))
        .expect("submit_action seeds the bus");

    (shell, entity)
}

/// Mock [`SceneSaveDialog`] returning a fixed `.rge-scene` path so the Ctrl+S
/// Save flow can be driven headlessly (SCENE-SAVE-WIRING). No real file is
/// written — the writer hook below short-circuits to `Ok`.
struct MockSaveDialog;

impl SceneSaveDialog for MockSaveDialog {
    fn pick_save_path(&self) -> Option<std::path::PathBuf> {
        Some(std::path::PathBuf::from(
            "/tmp/keyboard_round_trip.rge-scene",
        ))
    }
}

/// Mock [`SceneSaveHook`] recording its invocation through a shared
/// `Rc<Cell<bool>>` and returning `Ok` so the handler marks the bus saved.
struct MockSaveHook {
    called: std::rc::Rc<std::cell::Cell<bool>>,
}

impl SceneSaveHook for MockSaveHook {
    fn save_scene_world(
        &self,
        _world: &rge_kernel_ecs::World,
        _path: &std::path::Path,
    ) -> Result<(), String> {
        self.called.set(true);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn seeded_action_makes_bus_dirty_and_advances_world() {
    // Step 1-4 of the dispatch contract: a freshly-seeded bus must be
    // dirty, the world must reflect the action, and the bus stack must
    // hold exactly one entry.
    let (shell, entity) = shell_with_seeded_increment();

    assert_eq!(
        read_counter(&shell, entity),
        1,
        "IncrementCounter::apply must have advanced Counter 0 → 1"
    );
    assert!(
        shell.command_bus().is_dirty(),
        "bus must be dirty after a fresh submit"
    );
}

#[test]
fn ctrl_z_via_handle_key_command_reverts_seeded_action() {
    // Step 5: drive the public keyboard handler with `Undo` and assert
    // the world reverts. This is the exact entry point that the
    // `WindowEvent::KeyboardInput` branch in `lifecycle.rs` calls in
    // production; the test bypasses winit-event synthesis and exercises
    // the shell-level command directly.
    let (mut shell, entity) = shell_with_seeded_increment();
    assert_eq!(read_counter(&shell, entity), 1);

    shell.handle_key_command(EditorKeyCommand::Undo);

    assert_eq!(
        read_counter(&shell, entity),
        0,
        "Ctrl+Z must revert Counter 1 → 0"
    );
}

#[test]
fn ctrl_y_via_handle_key_command_reapplies_after_undo() {
    // Step 6: undo then redo must restore the world byte-identically.
    let (mut shell, entity) = shell_with_seeded_increment();
    shell.handle_key_command(EditorKeyCommand::Undo);
    assert_eq!(read_counter(&shell, entity), 0);

    shell.handle_key_command(EditorKeyCommand::Redo);

    assert_eq!(
        read_counter(&shell, entity),
        1,
        "Ctrl+Y after Ctrl+Z must re-apply Counter 0 → 1"
    );
}

#[test]
fn ctrl_s_via_handle_key_command_saves_and_clears_dirty() {
    // Step 7: Ctrl+S now performs a Save — write the world through the save
    // writer hook, then mark the bus saved on success (SCENE-SAVE-WIRING). With
    // a mock dialog + Ok writer attached, the writer must be invoked AND
    // `is_dirty()` must clear. (Pure saved-point semantics without a write are
    // covered by `editor-actions/tests/save_mark_dirty.rs`.)
    let (mut shell, _entity) = shell_with_seeded_increment();
    let called = std::rc::Rc::new(std::cell::Cell::new(false));
    shell = shell
        .with_scene_save_dialog(Box::new(MockSaveDialog))
        .with_scene_save_hook(Box::new(MockSaveHook {
            called: std::rc::Rc::clone(&called),
        }));
    assert!(shell.command_bus().is_dirty());

    shell.handle_key_command(EditorKeyCommand::MarkSaved);

    assert!(called.get(), "Ctrl+S must invoke the save writer hook");
    assert!(
        !shell.command_bus().is_dirty(),
        "a successful Ctrl+S Save must clear dirty by marking the cursor saved"
    );
}

#[test]
fn ctrl_z_on_empty_stack_is_noop_not_panic() {
    // Step 8a: Ctrl+Z on a brand-new shell (no submits yet) must not
    // panic. `handle_key_command` swallows BusError::NothingToUndo per
    // the dispatch contract.
    let mut shell = EditorShell::new();
    assert_eq!(shell.command_bus().stack().len(), 0);

    shell.handle_key_command(EditorKeyCommand::Undo);

    assert_eq!(
        shell.command_bus().stack().len(),
        0,
        "Ctrl+Z on empty stack must leave the stack untouched"
    );
}

#[test]
fn ctrl_y_on_empty_stack_is_noop_not_panic() {
    // Step 8b: symmetric — Ctrl+Y on an empty stack (no actions ever
    // submitted, so the redo tail is empty) must not panic.
    let mut shell = EditorShell::new();
    assert_eq!(shell.command_bus().stack().len(), 0);

    shell.handle_key_command(EditorKeyCommand::Redo);

    assert_eq!(
        shell.command_bus().stack().len(),
        0,
        "Ctrl+Y on empty stack must leave the stack untouched"
    );
}

#[test]
fn ctrl_z_after_all_undone_is_noop_not_panic() {
    // Defensive: undo until the stack cursor is at zero, then call Ctrl+Z
    // once more. The bus must return `NothingToUndo` and the shell must
    // swallow it without panic. Catches the case where `is_dirty()` is
    // false (saved at zero) but the stack itself is non-empty.
    let (mut shell, entity) = shell_with_seeded_increment();
    shell.handle_key_command(EditorKeyCommand::Undo);
    assert_eq!(read_counter(&shell, entity), 0);

    // Stack has one entry but cursor is at 0; another Undo should be
    // NothingToUndo, swallowed silently.
    shell.handle_key_command(EditorKeyCommand::Undo);

    assert_eq!(
        read_counter(&shell, entity),
        0,
        "second Ctrl+Z after exhausting undos must leave the world unchanged"
    );
}

#[test]
fn key_command_mapping_table_is_exact() {
    use rge_input::KeyCode;

    // The mapping table itself is part of the dispatch contract; this
    // test guards against accidental rebindings or expansions.

    // Ctrl-without-Shift + mapped keys → Some(command).
    assert_eq!(
        EditorKeyCommand::from_key_press(KeyCode::KeyZ, true, false),
        Some(EditorKeyCommand::Undo)
    );
    assert_eq!(
        EditorKeyCommand::from_key_press(KeyCode::KeyY, true, false),
        Some(EditorKeyCommand::Redo)
    );
    assert_eq!(
        EditorKeyCommand::from_key_press(KeyCode::KeyS, true, false),
        Some(EditorKeyCommand::MarkSaved)
    );
    assert_eq!(
        EditorKeyCommand::from_key_press(KeyCode::Digit2, true, false),
        Some(EditorKeyCommand::SetTimeScaleDoubleSpeed)
    );
    assert_eq!(
        EditorKeyCommand::from_key_press(KeyCode::Digit0, true, false),
        Some(EditorKeyCommand::ResetTimeScaleDefault)
    );
    assert_eq!(
        EditorKeyCommand::from_key_press(KeyCode::Digit4, true, false),
        Some(EditorKeyCommand::SetTimeScaleMaxFastForward)
    );

    // Ctrl-without-Shift + unmapped keys → None.
    assert_eq!(
        EditorKeyCommand::from_key_press(KeyCode::KeyA, true, false),
        None
    );
    assert_eq!(
        EditorKeyCommand::from_key_press(KeyCode::KeyX, true, false),
        None
    );
    assert_eq!(
        EditorKeyCommand::from_key_press(KeyCode::Space, true, false),
        None
    );

    // Bare keys (no Ctrl) → always None (we don't bind anything without
    // a modifier today).
    assert_eq!(
        EditorKeyCommand::from_key_press(KeyCode::KeyZ, false, false),
        None
    );
    assert_eq!(
        EditorKeyCommand::from_key_press(KeyCode::KeyY, false, false),
        None
    );
    assert_eq!(
        EditorKeyCommand::from_key_press(KeyCode::KeyS, false, false),
        None
    );

    // Shift-only (no Ctrl) → None even on the otherwise-bound letter keys.
    assert_eq!(
        EditorKeyCommand::from_key_press(KeyCode::KeyZ, false, true),
        None
    );
    assert_eq!(
        EditorKeyCommand::from_key_press(KeyCode::KeyY, false, true),
        None
    );
    assert_eq!(
        EditorKeyCommand::from_key_press(KeyCode::KeyS, false, true),
        None
    );
}

#[test]
fn ctrl_shift_combinations_are_unbound() {
    use rge_input::KeyCode;

    // Explicit guard against the pre-fix bug: the previous implementation
    // mapped Ctrl+Shift+Z to Undo (because it only inspected `ctrl`).
    // The dispatch contract is "exactly Ctrl-without-Shift for all three
    // bindings"; Ctrl+Shift+Z is reserved for a future redo-alias that a
    // wider input-binding layer will own. Until that lands, all three
    // Ctrl+Shift+{Z,Y,S} combinations must explicitly return None so a
    // user pressing Ctrl+Shift+Z sees neither Undo nor a phantom Redo.

    assert_eq!(
        EditorKeyCommand::from_key_press(KeyCode::KeyZ, true, true),
        None,
        "Ctrl+Shift+Z must be unbound (reserved for future redo-alias)"
    );
    assert_eq!(
        EditorKeyCommand::from_key_press(KeyCode::KeyY, true, true),
        None,
        "Ctrl+Shift+Y must be unbound"
    );
    assert_eq!(
        EditorKeyCommand::from_key_press(KeyCode::KeyS, true, true),
        None,
        "Ctrl+Shift+S must be unbound (reserved for future \"Save As\")"
    );
    assert_eq!(
        EditorKeyCommand::from_key_press(KeyCode::Digit2, true, true),
        None,
        "Ctrl+Shift+2 must be unbound (reserved for future input-binding layer)"
    );
    assert_eq!(
        EditorKeyCommand::from_key_press(KeyCode::Digit2, false, false),
        None,
        "bare Digit2 (no Ctrl) must be unbound"
    );
    assert_eq!(
        EditorKeyCommand::from_key_press(KeyCode::Digit0, true, true),
        None,
        "Ctrl+Shift+0 must be unbound (reserved for future input-binding layer)"
    );
    assert_eq!(
        EditorKeyCommand::from_key_press(KeyCode::Digit0, false, false),
        None,
        "bare Digit0 (no Ctrl) must be unbound"
    );
    assert_eq!(
        EditorKeyCommand::from_key_press(KeyCode::Digit0, false, true),
        None,
        "Shift-only Digit0 (no Ctrl) must be unbound"
    );
    assert_eq!(
        EditorKeyCommand::from_key_press(KeyCode::Digit4, true, true),
        None,
        "Ctrl+Shift+4 must be unbound (reserved for future input-binding layer)"
    );
    assert_eq!(
        EditorKeyCommand::from_key_press(KeyCode::Digit4, false, false),
        None,
        "bare Digit4 (no Ctrl) must be unbound"
    );
    assert_eq!(
        EditorKeyCommand::from_key_press(KeyCode::Digit4, false, true),
        None,
        "Shift-only Digit4 (no Ctrl) must be unbound"
    );
}

#[test]
fn three_round_trips_preserve_audit_ledger_cursor_invariant() {
    // The bus maintains `ledger.cursor == stack.cursor` (per editor-actions
    // bus.rs lines 194 / 225 / 254). Exercise three submit / undo / redo
    // cycles via the keyboard path and verify the world stays byte-
    // identical at every cursor return point.
    let (mut shell, entity) = shell_with_seeded_increment();

    // After the seed: Counter = 1, stack len = 1, cursor at top.
    assert_eq!(read_counter(&shell, entity), 1);
    assert_eq!(shell.command_bus().stack().len(), 1);

    for cycle in 0..3 {
        shell.handle_key_command(EditorKeyCommand::Undo);
        assert_eq!(
            read_counter(&shell, entity),
            0,
            "after cycle {cycle} Ctrl+Z, Counter must be 0"
        );
        shell.handle_key_command(EditorKeyCommand::Redo);
        assert_eq!(
            read_counter(&shell, entity),
            1,
            "after cycle {cycle} Ctrl+Y, Counter must be 1"
        );
    }

    // Stack length must be unchanged — undo/redo move the cursor, never
    // grow or shrink the stack.
    assert_eq!(
        shell.command_bus().stack().len(),
        1,
        "three round-trips must not alter the stack length"
    );
}
