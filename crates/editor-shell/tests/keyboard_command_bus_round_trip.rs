//! Keyboard → CommandBus → undo / redo / save round-trip (Phase 9, carried
//! through the W08 accelerator-execution thread).
//!
//! Proves the editor-workflow surface (Ctrl+Z / Ctrl+Y / Ctrl+S) end-to-end
//! through the editor-shell ↔ CommandBus integration, without requiring a winit
//! window, GPU, CAD-graph mutation, or any selection / multi-entity rendering.
//!
//! Post-W08.3/W08.4 the File/Edit accelerators are resolved through the canonical
//! menu and dispatched by `EditorShell::route_menu_command` (the shared sink for
//! the host→shell menu FIFO + the keyboard accelerator path); `EditorKeyCommand` /
//! `handle_key_command` are retained only for the execution-only time-scale binds.
//! So the undo/redo/save round-trips here drive `route_menu_command(Command::…)`,
//! and the `from_key_press` guards assert the retired File/Edit binds now return
//! `None`.
//!
//! The tests:
//!
//! 1. Construct `EditorShell::new()` headlessly (no `resumed`, no GPU).
//! 2. Spawn one ECS entity inside the kernel world (via `shell.world_mut().kernel_mut()`).
//! 3. Seed the bus via `shell.submit_action(Box::new(IncrementCounter { ... }))`
//!    — the bus's apply runs on `&mut rge_kernel_ecs::World` per editor-actions'
//!    `Action::apply` contract.
//! 4. Drive `route_menu_command(Command::Undo / Redo)` and assert the world
//!    reverts / re-applies byte-identically; `route_menu_command(Command::Save)`
//!    with mock dialog + writer hooks invokes the writer and clears dirty
//!    (SCENE-SAVE-WIRING); empty-stack Undo/Redo are silent no-ops.
//! 5. Guard `EditorKeyCommand::from_key_press`: only the Ctrl+digit time-scale
//!    binds map; the retired Ctrl+Z / Ctrl+Y / Ctrl+S and Ctrl+Shift+S return
//!    `None`.
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
use rge_editor_ui::menus::Command;
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
fn route_menu_command_redo_on_empty_stack_is_noop_not_panic() {
    // Ctrl+Y on an empty stack (no actions ever submitted, so the redo tail is
    // empty) routes Command::Redo through route_menu_command and must not panic —
    // the NothingToRedo swallow holds on the shared sink.
    let mut shell = EditorShell::new();
    assert_eq!(shell.command_bus().stack().len(), 0);

    shell.route_menu_command(Command::Redo);

    assert_eq!(
        shell.command_bus().stack().len(),
        0,
        "Command::Redo on empty stack must leave the stack untouched"
    );
}

#[test]
fn route_menu_command_undo_after_all_undone_is_noop_not_panic() {
    // Defensive: undo until the stack cursor is at zero, then route Command::Undo
    // once more. The bus must return `NothingToUndo` and route_menu_command must
    // swallow it without panic. Catches the case where `is_dirty()` is false
    // (saved at zero) but the stack itself is non-empty.
    let (mut shell, entity) = shell_with_seeded_increment();
    shell.route_menu_command(Command::Undo);
    assert_eq!(read_counter(&shell, entity), 0);

    // Stack has one entry but cursor is at 0; another Undo should be
    // NothingToUndo, swallowed silently.
    shell.route_menu_command(Command::Undo);

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
    // test guards against accidental rebindings or expansions. Post-W08.4
    // from_key_press maps ONLY the three Ctrl+digit time-scale binds — the
    // File/Edit accelerators (Ctrl+Z / Ctrl+Y / Ctrl+S, Ctrl+Shift+S) were
    // retired to the canonical menu.

    // Ctrl-without-Shift + the time-scale digits → Some(time-scale command).
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

    // The retired File/Edit accelerators → None now (menu-routed, not here).
    assert_eq!(
        EditorKeyCommand::from_key_press(KeyCode::KeyZ, true, false),
        None,
        "Ctrl+Z retired to the menu (Undo)"
    );
    assert_eq!(
        EditorKeyCommand::from_key_press(KeyCode::KeyY, true, false),
        None,
        "Ctrl+Y retired to the menu (Redo)"
    );
    assert_eq!(
        EditorKeyCommand::from_key_press(KeyCode::KeyS, true, false),
        None,
        "Ctrl+S retired to the menu (Save)"
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
fn ctrl_shift_bindings_today() {
    use rge_input::KeyCode;

    // Post-W08.4 NO Ctrl+Shift binding exists in from_key_press — Save-As
    // (formerly Ctrl+Shift+S) was retired to the canonical menu along with the
    // other File/Edit accelerators. Every Ctrl+Shift+key must return None, so a
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
        "Ctrl+Shift+S retired to the menu (Save-As)"
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
    // cycles via the keyboard command sink (route_menu_command — the post-W08.3
    // home of Ctrl+Z / Ctrl+Y) and verify the world stays byte-identical at every
    // cursor return point.
    let (mut shell, entity) = shell_with_seeded_increment();

    // After the seed: Counter = 1, stack len = 1, cursor at top.
    assert_eq!(read_counter(&shell, entity), 1);
    assert_eq!(shell.command_bus().stack().len(), 1);

    for cycle in 0..3 {
        shell.route_menu_command(Command::Undo);
        assert_eq!(
            read_counter(&shell, entity),
            0,
            "after cycle {cycle} Ctrl+Z, Counter must be 0"
        );
        shell.route_menu_command(Command::Redo);
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

// ---------------------------------------------------------------------------
// route_menu_command is the shared command sink for BOTH the host→shell menu FIFO
// and the keyboard accelerator path (W08.3 cutover). A keystroke resolves to its
// menu `Command` (keycode_to_shortcut -> command_for_shortcut) and dispatches
// here; post-W08.4 this IS the home of Ctrl+O / Ctrl+S / Ctrl+Shift+S / Ctrl+Z /
// Ctrl+Y (their EditorKeyCommand mirror is retired). These prove the sink
// reverts / re-applies / saves correctly on a real seeded bus.
// ---------------------------------------------------------------------------

#[test]
fn route_menu_command_undo_reverts_like_ctrl_z() {
    let (mut shell, entity) = shell_with_seeded_increment();
    assert_eq!(read_counter(&shell, entity), 1);

    shell.route_menu_command(Command::Undo);

    assert_eq!(
        read_counter(&shell, entity),
        0,
        "Command::Undo via route_menu_command must revert Counter 1 -> 0 (same as Ctrl+Z)"
    );
}

#[test]
fn route_menu_command_redo_reapplies_like_ctrl_y() {
    let (mut shell, entity) = shell_with_seeded_increment();
    shell.route_menu_command(Command::Undo);
    assert_eq!(read_counter(&shell, entity), 0);

    shell.route_menu_command(Command::Redo);

    assert_eq!(
        read_counter(&shell, entity),
        1,
        "Command::Redo via route_menu_command must re-apply Counter 0 -> 1 (same as Ctrl+Y)"
    );
}

#[test]
fn route_menu_command_undo_on_empty_stack_is_noop_not_panic() {
    // The empty-stack swallow (`BusError::NothingToUndo`) must hold on the shared
    // sink too — a keyboard or menu Undo on a fresh editor is a silent no-op.
    let mut shell = EditorShell::new();
    assert_eq!(shell.command_bus().stack().len(), 0);

    shell.route_menu_command(Command::Undo);

    assert_eq!(
        shell.command_bus().stack().len(),
        0,
        "Command::Undo on an empty stack must leave the stack untouched"
    );
}

#[test]
fn route_menu_command_save_saves_and_clears_dirty_like_ctrl_s() {
    // Ctrl+S resolves to Command::Save; route_menu_command must drive the same
    // Save (write through the hook, then mark the bus saved) as
    // handle_key_command(Save) — the writer fires and dirty clears.
    let (mut shell, _entity) = shell_with_seeded_increment();
    let called = std::rc::Rc::new(std::cell::Cell::new(false));
    shell = shell
        .with_scene_save_dialog(Box::new(MockSaveDialog))
        .with_scene_save_hook(Box::new(MockSaveHook {
            called: std::rc::Rc::clone(&called),
        }));
    assert!(shell.command_bus().is_dirty());

    shell.route_menu_command(Command::Save);

    assert!(
        called.get(),
        "Command::Save via route_menu_command must invoke the save writer hook"
    );
    assert!(
        !shell.command_bus().is_dirty(),
        "a successful Save must clear dirty by marking the cursor saved"
    );
}
