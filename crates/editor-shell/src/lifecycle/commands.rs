//! Phase 9 CommandBus integration — keyboard bindings + bus-routed mutations.
//!
//! Holds the editor-side material that drives [`rge_editor_actions`] from
//! the editor-shell side of the Tier-2 boundary:
//!
//! - [`EditorKeyCommand`] — the closed enum of bus-bound keyboard commands
//!   (`Ctrl+Z` / `Ctrl+Y` / `Ctrl+S`) plus its physical-key → command
//!   mapping table.
//! - [`SetTimeScale`] — the first non-test [`rge_editor_actions::Action`]
//!   impl in the workspace. Routes the time-scale slider mutation through
//!   the bus so undo/redo work on it.
//! - The narrow `impl EditorShell` block exposing
//!   [`EditorShell::submit_action`] / [`EditorShell::undo_command`] /
//!   [`EditorShell::redo_command`] / [`EditorShell::mark_saved_command`] /
//!   [`EditorShell::command_bus`] / [`EditorShell::handle_key_command`] /
//!   [`EditorShell::set_time_scale`].
//!
//! # Architectural invariants
//!
//! - Every bus call delegates to `self.world.kernel_mut()` so the bus sees
//!   only the inner `rge_kernel_ecs::World`. The wrapper [`crate::world::World`]
//!   is never handed to the bus directly — that preserves the
//!   [`rge_editor_actions::Action::apply`] `(&self, world: &mut
//!   rge_kernel_ecs::World)` contract.
//! - `Action: Send + Sync + 'static` is a hard trait bound; [`SetTimeScale`]
//!   carries pre-captured `{ from, to }` plain `f32`s (no `Cell` / `RefCell` /
//!   interior mutability) so it is trivially `Send + Sync`.
//! - Coalescing across rapid slider drags is achieved via a constant
//!   [`SetTimeScale::id()`] plus a payload-based [`SetTimeScale::merge`]
//!   that parses `next.payload()` and keeps the original `from` while
//!   adopting the newer `to`. This way a 20-event drag from 1.0 → 3.5
//!   collapses to one stack entry whose revert restores 1.0.
//! - Dual-ledger by design (locked decision for this dispatch):
//!   - The bus's internal [`rge_editor_actions::AuditLedger`] continues
//!     to record `EventKind::Action` on each submit.
//!   - [`EditorShell::audit`] (the editor's own [`crate::audit::AuditLedger`])
//!     continues to record [`crate::audit::AuditEvent::TimeScaleChanged`]
//!     on each `set_time_scale` call so existing tests
//!     (`time_scale_audit_event_records_change`) keep passing without
//!     editing.
//!   - Ledger consolidation is a separate future dispatch.

use rge_editor_actions::action::{Action, ActionId, ActionResult, MergeOutcome};
use rge_editor_actions::BusError;
use rge_input::KeyCode;
use rge_kernel_ecs::World as KernelWorld;

use crate::audit::AuditEvent;
use crate::lifecycle::EditorShell;
use crate::time_scale::TimeScale;

// ---------------------------------------------------------------------------
// EditorKeyCommand
// ---------------------------------------------------------------------------

/// Editor-side keyboard command bound to the Command Bus.
///
/// The set is intentionally minimal — only the three undo/redo/save bindings
/// the Phase 9 keyboard → CommandBus integration dispatch ships. Future
/// editor keybinds (Play/Stop, selection clear, tool switch) extend this enum
/// rather than growing parallel command channels.
///
/// Mapping from physical keys is performed by [`Self::from_key_press`]; the
/// dispatch into the bus by [`EditorShell::handle_key_command`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorKeyCommand {
    /// `Ctrl+Z` — revert the most recent action on the Command Bus.
    Undo,
    /// `Ctrl+Y` — re-apply the next action on the Command Bus.
    Redo,
    /// `Ctrl+S` — mark the current bus cursor as the saved point.
    MarkSaved,
    /// `Ctrl+2` — set the [`TimeScale`] resource to 2.0 (double speed) via
    /// the existing [`EditorShell::set_time_scale`] → [`SetTimeScale`] →
    /// [`rge_editor_actions::CommandBus::submit`] path. Reuses the
    /// existing slider action so undo/redo and the no-op short-circuit at
    /// 2.0 require no additional surface.
    SetTimeScaleDoubleSpeed,
}

impl EditorKeyCommand {
    /// Map a translated [`KeyCode`] plus the relevant modifier flags
    /// (`ctrl` and `shift`) to an [`EditorKeyCommand`]. Returns `None` for
    /// any other key combination so the keyboard branch in
    /// `EditorShell::window_event` can ignore unbound keys cheaply.
    ///
    /// Bind-set today (mirrors common editor conventions; **exactly**
    /// Ctrl-without-Shift for all three bindings):
    ///
    /// | Combination | Command |
    /// |---|---|
    /// | `Ctrl+Z` (no Shift) | [`EditorKeyCommand::Undo`] |
    /// | `Ctrl+Y` (no Shift) | [`EditorKeyCommand::Redo`] |
    /// | `Ctrl+S` (no Shift) | [`EditorKeyCommand::MarkSaved`] |
    ///
    /// `Ctrl+Shift+Z` is **not** mapped today (the standard "redo" alias is
    /// part of a wider input-binding configurability layer that is out of
    /// scope for this dispatch). The Shift guard is explicit so that
    /// future bindings (e.g. `Ctrl+Shift+S` for "Save As", `Ctrl+Shift+Z`
    /// for "Redo") slot in additively without behavioural collision with
    /// the no-Shift bind set above.
    ///
    /// The `alt` modifier is intentionally ignored — Alt may be combined
    /// with Ctrl for tool-specific actions (e.g. drag-modifier) that don't
    /// route through the Command Bus. If future bus-bound commands need
    /// Alt-disambiguation, extend this signature additively.
    #[must_use]
    pub fn from_key_press(key: KeyCode, ctrl: bool, shift: bool) -> Option<Self> {
        if !ctrl || shift {
            return None;
        }
        Some(match key {
            KeyCode::KeyZ => Self::Undo,
            KeyCode::KeyY => Self::Redo,
            KeyCode::KeyS => Self::MarkSaved,
            KeyCode::Digit2 => Self::SetTimeScaleDoubleSpeed,
            _ => return None,
        })
    }
}

// ---------------------------------------------------------------------------
// SetTimeScale — the first production Action impl in the workspace.
// ---------------------------------------------------------------------------

/// Stable action id for [`SetTimeScale`].
///
/// Constant (not parameterised by `from` / `to`) so that the bus's
/// [`rge_editor_actions::CommandBus`] 500 ms coalesce window collapses rapid
/// slider drags into one stack entry. A drag from 1.0 → 1.5 → 2.0 → 3.5
/// within 500 ms produces one merged entry whose revert restores 1.0.
const SET_TIME_SCALE_ID: &str = "set-time-scale";

/// [`rge_editor_actions::Action`] that sets the [`TimeScale`] resource in
/// `rge_kernel_ecs::World`. `apply` switches to `to`, `revert` switches back
/// to `from`; coalescing keeps the original `from` while adopting the newer
/// `to`.
///
/// # `Send + Sync` discipline
///
/// `Action: Send + Sync + 'static` is a hard trait bound. This struct
/// therefore carries plain `f32` values captured at submit time and uses no
/// interior mutability (`Cell` / `RefCell` would break `Sync`). The values
/// are clamped at submit time by [`TimeScale::with_value`] before being
/// stored, so apply/revert never need to re-clamp.
///
/// # Coalesce-and-merge semantics
///
/// When a follow-up `SetTimeScale` action arrives within the bus's 500 ms
/// window with the same [`Action::id`], the bus calls
/// [`Action::merge`]; this impl parses the new `to` from `next.payload()`
/// and updates `self.to` while keeping `self.from` untouched. The bus then
/// applies the new action (advancing the world to the new `to`), but the
/// stack entry that absorbed the merge is `self` — so undo restores the
/// pre-drag `from`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SetTimeScale {
    /// The `TimeScale` value the world had at submit time. Restored on revert.
    pub from: f32,
    /// The `TimeScale` value to apply. Already clamped to
    /// `[TimeScale::MIN, TimeScale::MAX]` by [`TimeScale::with_value`].
    pub to: f32,
}

impl SetTimeScale {
    /// Construct a [`SetTimeScale`] action with pre-clamped `from` and `to`.
    ///
    /// The caller is responsible for capturing `from` from the current
    /// [`TimeScale`] resource value before submit; [`EditorShell::set_time_scale`]
    /// does that handoff.
    #[must_use]
    pub fn new(from: f32, to: f32) -> Self {
        Self { from, to }
    }
}

impl Action for SetTimeScale {
    fn name(&self) -> &str {
        "set-time-scale"
    }

    fn id(&self) -> ActionId {
        // Constant id (not parameterised by the f32 values) so rapid slider
        // drags within the 500 ms coalesce window collapse into one stack
        // entry. See `merge` for how the merged entry preserves the
        // original `from`.
        ActionId::new(SET_TIME_SCALE_ID)
    }

    fn payload(&self) -> Vec<u8> {
        // Encode `to` as 4 little-endian bytes so `merge` can parse a
        // follow-up action's payload without needing serde or a downcast.
        //
        // Bus-ledger interaction (subtle):
        //
        // - On the INITIAL (non-coalesced) submit the bus records one
        //   `EventKind::Action` ledger entry carrying these bytes — the
        //   ledger therefore captures the FIRST `to` value of a drag.
        // - On subsequent COALESCED submits (same `id`, within the bus's
        //   500 ms window) the bus calls `merge`, mutates the existing
        //   stack entry's `to` in-place, and applies the new action to
        //   the world — but DOES NOT record a new ledger event and DOES
        //   NOT refresh the original entry's payload bytes on the ledger.
        //   See `rge_editor_actions::CommandBus::submit` §6.16.7
        //   ("the world advances via a direct `action.apply` — no new
        //   stack entry, no new ledger event").
        //
        // Net: the bus's audit ledger records only one event per drag
        // burst, carrying the FIRST `to`. The editor-shell's own audit
        // ledger (dual-ledger per locked decision #3) is the source of
        // truth for per-change history — `set_time_scale` records a
        // `TimeScaleChanged { from, to }` event on it for every non-no-op
        // call, regardless of whether the bus coalesced the submit.
        self.to.to_le_bytes().to_vec()
    }

    fn apply(&self, world: &mut KernelWorld) -> Result<(), ActionResult> {
        // `insert_resource` REPLACES any existing TimeScale per kernel/ecs
        // `World::insert_resource`. Clamp via `with_value` defensively even
        // though `EditorShell::set_time_scale` already clamps before submit.
        world.insert_resource(TimeScale::with_value(self.to));
        Ok(())
    }

    fn revert(&self, world: &mut KernelWorld) -> Result<(), ActionResult> {
        world.insert_resource(TimeScale::with_value(self.from));
        Ok(())
    }

    fn merge(&mut self, next: &dyn Action) -> MergeOutcome {
        // Parse `next.payload()` as 4-byte little-endian f32 (matches
        // `Self::payload`'s encoding). Reject any payload of unexpected
        // shape — that would indicate an unrelated action sharing our id
        // (which shouldn't happen given `SET_TIME_SCALE_ID` is private),
        // but the cheap shape check is defensive against future
        // misconfiguration.
        let payload = next.payload();
        let Ok(bytes): Result<[u8; 4], _> = payload.try_into() else {
            return MergeOutcome::Distinct;
        };
        let new_to = f32::from_le_bytes(bytes);
        // Re-clamp into the valid range so a corrupted payload can't shove
        // an out-of-range value into the merged entry.
        let new_to = new_to.clamp(TimeScale::MIN, TimeScale::MAX);
        self.to = new_to;
        MergeOutcome::Merged
    }
}

// ---------------------------------------------------------------------------
// Shell-side CommandBus surface
// ---------------------------------------------------------------------------

impl EditorShell {
    /// Submit an [`Action`] through the Command Bus. Returns the bus's
    /// error verbatim (so callers can distinguish coalesce / ledger /
    /// apply failures); the keyboard handler wraps this and swallows
    /// `NothingToUndo`/`NothingToRedo` on `Self::handle_key_command`.
    ///
    /// # Errors
    ///
    /// Propagates [`BusError`] from [`rge_editor_actions::CommandBus::submit`].
    pub fn submit_action(&mut self, action: Box<dyn Action>) -> Result<(), BusError> {
        self.command_bus.submit(action, self.world.kernel_mut())
    }

    /// Undo the most recent action via the Command Bus.
    ///
    /// # Errors
    ///
    /// Propagates [`BusError`] from [`rge_editor_actions::CommandBus::undo`].
    /// `NothingToUndo` is returned (not panicked); the keyboard handler
    /// ignores it.
    pub fn undo_command(&mut self) -> Result<(), BusError> {
        self.command_bus.undo(self.world.kernel_mut())
    }

    /// Redo the next action via the Command Bus.
    ///
    /// # Errors
    ///
    /// Propagates [`BusError`] from [`rge_editor_actions::CommandBus::redo`].
    /// `NothingToRedo` is returned (not panicked); the keyboard handler
    /// ignores it.
    pub fn redo_command(&mut self) -> Result<(), BusError> {
        self.command_bus.redo(self.world.kernel_mut())
    }

    /// Mark the current bus cursor as the saved point. Drives
    /// `CommandBus::is_dirty()` to `false` until the next submit / undo /
    /// redo moves the cursor away from this position.
    pub fn mark_saved_command(&mut self) {
        self.command_bus.mark_saved();
    }

    /// Borrow the Command Bus for read-only introspection (tests, future
    /// status-bar dirty indicator). Mutations route through
    /// [`Self::submit_action`] / [`Self::undo_command`] /
    /// [`Self::redo_command`] / [`Self::mark_saved_command`] — never
    /// through this accessor.
    #[must_use]
    pub fn command_bus(&self) -> &rge_editor_actions::CommandBus {
        &self.command_bus
    }

    /// Dispatch a single editor key command. Public so headless tests can
    /// drive the bus without synthesizing winit `KeyEvent`s; production
    /// usage routes through the `WindowEvent::KeyboardInput` branch in
    /// `Self::window_event`.
    ///
    /// Swallows `BusError::NothingToUndo` / `NothingToRedo` on empty
    /// stack (per the user-facing contract: Ctrl+Z on a fresh editor must
    /// be a no-op, not a diagnostic spam). Other errors are traced.
    pub fn handle_key_command(&mut self, command: EditorKeyCommand) {
        match command {
            EditorKeyCommand::Undo => match self.undo_command() {
                Ok(()) | Err(BusError::NothingToUndo) => {}
                Err(e) => tracing::warn!(
                    target: "rge::editor-shell::lifecycle",
                    error = ?e,
                    "Ctrl+Z dispatched but bus returned non-NothingToUndo error"
                ),
            },
            EditorKeyCommand::Redo => match self.redo_command() {
                Ok(()) | Err(BusError::NothingToRedo) => {}
                Err(e) => tracing::warn!(
                    target: "rge::editor-shell::lifecycle",
                    error = ?e,
                    "Ctrl+Y dispatched but bus returned non-NothingToRedo error"
                ),
            },
            EditorKeyCommand::MarkSaved => self.mark_saved_command(),
            EditorKeyCommand::SetTimeScaleDoubleSpeed => self.set_time_scale(2.0),
        }
    }

    /// Adjust the time-scale slider. Captures `from` from the current
    /// [`TimeScale`] resource value, builds a [`SetTimeScale`] action, and
    /// routes it through [`Self::submit_action`]. Also records an
    /// [`AuditEvent::TimeScaleChanged`] on the editor-shell's own
    /// audit ledger so the existing
    /// `time_scale_audit_event_records_change` test keeps passing
    /// byte-identically (dual-ledger per the locked dispatch decision).
    ///
    /// Submit errors from the bus are surfaced via `tracing::warn!` but
    /// not propagated — the slider has no user-visible failure mode today,
    /// and `SetTimeScale::apply` cannot fail (it only calls
    /// `insert_resource` which is infallible).
    ///
    /// # Coalesce behaviour
    ///
    /// Rapid drags within the bus's 500 ms coalesce window collapse into a
    /// single undo-stack entry, because [`SetTimeScale::id`] returns a
    /// constant and [`SetTimeScale::merge`] preserves the pre-drag `from`.
    /// One Ctrl+Z then restores the slider to its pre-drag value.
    pub fn set_time_scale(&mut self, value: f32) {
        let from = self.time_scale().value();
        let to = TimeScale::with_value(value).value();
        // No-op short-circuit: when the post-clamp `to` already equals the
        // current resource value (the same f32 bits), skip the bus submit
        // AND the editor-shell audit event so:
        // (a) the bus does NOT flip `is_dirty()` for a no-change input
        //     (otherwise a slider repaint or a programmatic `set_time_scale(current)`
        //     would mark the project dirty for nothing);
        // (b) the bus undo-stack does NOT grow with no-op entries (a Ctrl+Z
        //     against a no-op entry would silently do nothing — confusing UX);
        // (c) the editor-shell audit ledger does NOT accumulate phantom
        //     `TimeScaleChanged { from: X, to: X }` events (consumers count
        //     events by tag and would treat the no-op as a real change).
        // Equality test uses `f32::EPSILON` to absorb any 1-ulp drift across
        // the clamp-then-compare round-trip (in practice the clamp is a
        // straight pass-through when the input is already in-range so the
        // bits match exactly, but the tolerance is harmless).
        if (to - from).abs() < f32::EPSILON {
            return;
        }
        let action = Box::new(SetTimeScale::new(from, to));
        if let Err(e) = self.submit_action(action) {
            tracing::warn!(
                target: "rge::editor-shell::lifecycle",
                error = ?e,
                "SetTimeScale submit failed (non-fatal; slider state unchanged)"
            );
            return;
        }
        // Dual-ledger: also record on the editor-shell's own ring-buffer
        // audit ledger so existing tests (`time_scale_audit_event_records_change`)
        // continue to count the right number of `TimeScaleChanged` events.
        self.audit.record(AuditEvent::TimeScaleChanged { from, to });
    }
}
