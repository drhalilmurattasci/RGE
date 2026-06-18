//! [`CommandBus`], [`BusEntry`], and [`BusError`].

use std::time::{SystemTime, UNIX_EPOCH};

use rge_kernel_audit_ledger::{AuditLedger, EventKind, LedgerError};
use rge_kernel_diagnostics::{Diagnostic, DiagnosticAggregator, DiagnosticSink, FailureClass};

use crate::action::{
    Action, ActionContextFamily, ActionResult, ActionViewRef, MergeOutcome, WorldActionContext,
};
use crate::coalesce::CoalesceWindow;
use crate::undo_stack::UndoStack;

// ---------------------------------------------------------------------------
// BusEntry
// ---------------------------------------------------------------------------

/// One entry in the undo stack.
///
/// `#[non_exhaustive]` — future variants (e.g. `CadCheckpoint` in
/// Phase 4-Geometry) will be added additively.
#[non_exhaustive]
pub enum BusEntry<F: ActionContextFamily = WorldActionContext> {
    /// A single reversible [`Action`].
    Action(Box<dyn Action<F>>),
}

impl<F: ActionContextFamily + 'static> BusEntry<F> {
    /// Call `apply` on the inner action (dispatching over variants).
    ///
    /// # Errors
    ///
    /// Propagates the error from the inner action's `apply`.
    pub(crate) fn apply(&self, context: &mut F::Context<'_>) -> Result<(), ActionResult> {
        match self {
            Self::Action(a) => a.apply(context),
        }
    }

    /// Call `revert` on the inner action.
    ///
    /// # Errors
    ///
    /// Propagates the error from the inner action's `revert`.
    pub(crate) fn revert(&self, context: &mut F::Context<'_>) -> Result<(), ActionResult> {
        match self {
            Self::Action(a) => a.revert(context),
        }
    }
}

// ---------------------------------------------------------------------------
// BusError
// ---------------------------------------------------------------------------

/// Errors returned by [`CommandBus`] operations.
#[derive(Debug, thiserror::Error)]
pub enum BusError {
    /// The undo stack is already at its beginning.
    #[error("nothing to undo (cursor at 0)")]
    NothingToUndo,
    /// The undo stack cursor is already at the end of history.
    #[error("nothing to redo (cursor at history end)")]
    NothingToRedo,
    /// The action's `apply` or `revert` returned an error.
    #[error("action failed: {0}")]
    ActionFailed(#[from] ActionResult),
    /// The audit-ledger rejected the cursor movement.
    #[error("audit-ledger error: {0}")]
    LedgerError(#[from] LedgerError),
}

// ---------------------------------------------------------------------------
// CommandBus
// ---------------------------------------------------------------------------

/// The Command Bus — the single mediation layer for editor mutations.
///
/// All editor mutations must flow through [`submit`](Self::submit). The bus:
/// 1. Optionally coalesces the new action with the most recent one (§6.16.7).
/// 2. Applies the action to the [`World`].
/// 3. Projects the action to the audit ledger.
/// 4. Pushes the action onto the undo stack.
///
/// Undo and redo walk the stack, calling `revert`/`apply` and adjusting both
/// the stack cursor and the audit-ledger cursor in lockstep.
pub struct CommandBus<F: ActionContextFamily = WorldActionContext> {
    stack: UndoStack<F>,
    coalesce: CoalesceWindow,
    ledger: AuditLedger,
    /// Diagnostic sink for non-fatal errors (e.g. action apply failures).
    diagnostics: DiagnosticAggregator,
}

impl<F: ActionContextFamily + 'static> CommandBus<F> {
    /// Create a new context-family [`CommandBus`] with the default 500 ms
    /// coalesce window.
    #[must_use]
    pub fn new_for_context() -> Self {
        Self::with_coalesce_window_for_context(500)
    }

    /// Create a new context-family [`CommandBus`] with a custom coalesce window.
    #[must_use]
    pub fn with_coalesce_window_for_context(window_ms: u64) -> Self {
        Self {
            stack: UndoStack::new_for_context(),
            coalesce: CoalesceWindow::new(window_ms),
            ledger: AuditLedger::new(),
            diagnostics: DiagnosticAggregator::new(),
        }
    }

    // ── Submission ────────────────────────────────────────────────────────────

    /// Submit an action to the bus.
    ///
    /// # Flow
    ///
    /// 1. Capture wall-clock milliseconds.
    /// 2. If the action's id matches the most recent entry's id within the
    ///    coalesce window, attempt [`Action::merge`]:
    ///    - [`MergeOutcome::Merged`]: the existing entry absorbs `action` (no
    ///      new stack entry, no new ledger event — the world advances via a
    ///      direct `action.apply`).
    ///    - [`MergeOutcome::Distinct`]: keep both — fall through to normal
    ///      submission.
    /// 3. Apply via [`Action::apply`]. On error, emit a diagnostic and return.
    /// 4. Truncate the redo tail (entries above the cursor are discarded).
    /// 5. Project to the audit ledger.
    /// 6. Push the entry and advance the cursor.
    /// 7. Update the coalesce window.
    ///
    /// # Panics
    ///
    /// Panics if the internal cursor value exceeds `usize::MAX`, which is an
    /// unreachable invariant on any supported platform (the undo stack cannot
    /// hold more entries than available memory).
    ///
    /// # Errors
    ///
    /// Returns [`BusError::ActionFailed`] if `action.apply` fails.
    /// Returns [`BusError::LedgerError`] if the ledger cursor is inconsistent
    /// (should be unreachable in normal operation).
    pub fn submit(
        &mut self,
        action: Box<dyn Action<F>>,
        context: &mut F::Context<'_>,
    ) -> Result<(), BusError> {
        let now_ms = wall_clock_ms();
        let action_id = action.id();

        // ── Coalesce attempt ─────────────────────────────────────────────────
        if self.coalesce.should_coalesce(&action_id, now_ms) && !self.stack.entries.is_empty() {
            let cursor =
                usize::try_from(self.stack.cursor).expect("cursor fits in usize — invariant");
            if cursor > 0 {
                let last_idx = cursor - 1;
                let BusEntry::Action(ref mut existing) = self.stack.entries[last_idx];
                match existing.merge(&ActionViewRef::new(action.as_ref())) {
                    MergeOutcome::Merged => {
                        // The merged entry already represents the intent; apply
                        // `action` to advance the world to the new merged state.
                        if let Err(e) = action.apply(context) {
                            self.emit_apply_error(action.name(), &e);
                            return Err(BusError::ActionFailed(e));
                        }
                        self.coalesce.note_recorded(action_id, now_ms);
                        return Ok(());
                    }
                    MergeOutcome::Distinct => {
                        // Fall through to normal submission.
                    }
                }
            }
        }

        // ── Apply ────────────────────────────────────────────────────────────
        if let Err(e) = action.apply(context) {
            self.emit_apply_error(action.name(), &e);
            return Err(BusError::ActionFailed(e));
        }

        // ── Truncate redo tail ───────────────────────────────────────────────
        let cursor = usize::try_from(self.stack.cursor).expect("cursor fits in usize — invariant");
        self.stack.entries.truncate(cursor);
        let ledger_len = self.ledger.len() as u64;
        if ledger_len > self.stack.cursor {
            self.ledger.truncate(self.stack.cursor)?;
        }

        // ── Ledger projection ────────────────────────────────────────────────
        self.ledger.record(EventKind::Action, action.payload());

        // ── Push + advance cursor ────────────────────────────────────────────
        self.stack.entries.push(BusEntry::Action(action));
        self.stack.cursor += 1;

        // Advance ledger cursor to match stack cursor.
        self.ledger.set_cursor(self.stack.cursor)?;

        // ── Coalesce window update ───────────────────────────────────────────
        self.coalesce.note_recorded(action_id, now_ms);

        Ok(())
    }

    // ── Undo / Redo ───────────────────────────────────────────────────────────

    /// Undo the most recently applied action.
    ///
    /// # Panics
    ///
    /// Panics if the internal cursor value exceeds `usize::MAX` (unreachable
    /// invariant — see [`submit`](Self::submit)).
    ///
    /// # Errors
    ///
    /// - [`BusError::NothingToUndo`] when cursor is already at `0`.
    /// - [`BusError::ActionFailed`] when `revert` returns an error.
    /// - [`BusError::LedgerError`] when the ledger cursor adjustment fails.
    pub fn undo(&mut self, context: &mut F::Context<'_>) -> Result<(), BusError> {
        if self.stack.cursor == 0 {
            return Err(BusError::NothingToUndo);
        }

        let idx = usize::try_from(self.stack.cursor - 1).expect("cursor fits in usize — invariant");
        self.stack.entries[idx].revert(context)?;

        self.stack.cursor -= 1;
        self.ledger.set_cursor(self.stack.cursor)?;

        // Reset coalesce window across the undo boundary.
        self.coalesce.reset();

        Ok(())
    }

    /// Redo the next available action.
    ///
    /// # Panics
    ///
    /// Panics if the internal cursor value exceeds `usize::MAX` (unreachable
    /// invariant — see [`submit`](Self::submit)).
    ///
    /// # Errors
    ///
    /// - [`BusError::NothingToRedo`] when cursor is already at the end.
    /// - [`BusError::ActionFailed`] when `apply` returns an error.
    /// - [`BusError::LedgerError`] when the ledger cursor adjustment fails.
    pub fn redo(&mut self, context: &mut F::Context<'_>) -> Result<(), BusError> {
        let cursor = usize::try_from(self.stack.cursor).expect("cursor fits in usize — invariant");
        if cursor >= self.stack.entries.len() {
            return Err(BusError::NothingToRedo);
        }

        self.stack.entries[cursor].apply(context)?;

        self.stack.cursor += 1;
        self.ledger.set_cursor(self.stack.cursor)?;

        // Reset coalesce window after redo.
        self.coalesce.reset();

        Ok(())
    }

    // ── Accessors ─────────────────────────────────────────────────────────────

    /// Borrow the undo stack.
    #[must_use]
    pub fn stack(&self) -> &UndoStack<F> {
        &self.stack
    }

    /// Borrow the audit ledger.
    #[must_use]
    pub fn ledger(&self) -> &AuditLedger {
        &self.ledger
    }

    /// Mark the current state as saved.
    pub fn mark_saved(&mut self) {
        self.stack.mark_saved();
        self.coalesce.reset();
    }

    /// Returns `true` when there are unsaved changes.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.stack.is_dirty()
    }

    /// Borrow the accumulated diagnostics.
    #[must_use]
    pub fn diagnostics(&self) -> &DiagnosticAggregator {
        &self.diagnostics
    }

    // ── Private helpers ───────────────────────────────────────────────────────

    fn emit_apply_error(&mut self, action_name: &str, err: &ActionResult) {
        let msg = format!("action `{action_name}` failed: {err}");
        tracing::error!(%msg, "CommandBus: action apply failed");
        self.diagnostics
            .emit(Diagnostic::error(msg).with_failure_class(FailureClass::SnapshotRecoverable));
    }
}

impl CommandBus<WorldActionContext> {
    /// Create a new World-only [`CommandBus`] with the default 500 ms coalesce
    /// window.
    #[must_use]
    pub fn new() -> Self {
        Self::new_for_context()
    }

    /// Create a new World-only [`CommandBus`] with a custom coalesce window.
    #[must_use]
    pub fn with_coalesce_window(window_ms: u64) -> Self {
        Self::with_coalesce_window_for_context(window_ms)
    }
}

impl Default for CommandBus<WorldActionContext> {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Wall-clock helper
// ---------------------------------------------------------------------------

/// Return the current wall-clock time in milliseconds since UNIX epoch.
///
/// Saturates to `u64::MAX` if the system clock is set absurdly far in the
/// future (year ~584 million). In practice the cast is safe.
fn wall_clock_ms() -> u64 {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    u64::try_from(ms).unwrap_or(u64::MAX)
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unnecessary_literal_bound)]
mod tests {
    use rge_kernel_ecs::{Component, EntityId, World};

    use super::*;
    use crate::action::ActionId;

    #[derive(Debug, Clone, PartialEq)]
    struct Val(i32);
    impl Component for Val {}

    /// Insert `Val(value)` on apply; remove it on revert.
    struct InsertVal {
        entity: EntityId,
        value: i32,
    }

    impl Action for InsertVal {
        fn name(&self) -> &str {
            "insert-val"
        }

        fn id(&self) -> ActionId {
            ActionId::new(format!("insert-val({:?})", self.entity))
        }

        fn apply(&self, world: &mut World) -> Result<(), ActionResult> {
            if world.entity(self.entity).is_none() {
                return Err(ActionResult::MissingEntity(self.entity));
            }
            world.insert(self.entity, Val(self.value));
            Ok(())
        }

        fn revert(&self, world: &mut World) -> Result<(), ActionResult> {
            world.remove::<Val>(self.entity);
            Ok(())
        }
    }

    #[test]
    fn submit_advances_cursor() {
        let mut bus = CommandBus::new();
        let mut world = World::new();
        let e = world.spawn();
        bus.submit(
            Box::new(InsertVal {
                entity: e,
                value: 1,
            }),
            &mut world,
        )
        .unwrap();
        assert_eq!(bus.stack().cursor(), 1);
    }

    #[test]
    fn undo_nothing_errors() {
        let mut bus = CommandBus::new();
        let mut world = World::new();
        assert!(matches!(bus.undo(&mut world), Err(BusError::NothingToUndo)));
    }

    #[test]
    fn redo_nothing_errors() {
        let mut bus = CommandBus::new();
        let mut world = World::new();
        assert!(matches!(bus.redo(&mut world), Err(BusError::NothingToRedo)));
    }

    #[test]
    fn undo_reverts_world() {
        let mut bus = CommandBus::new();
        let mut world = World::new();
        let e = world.spawn();
        bus.submit(
            Box::new(InsertVal {
                entity: e,
                value: 42,
            }),
            &mut world,
        )
        .unwrap();
        assert_eq!(world.entity(e).unwrap().get::<Val>(), Some(&Val(42)));
        bus.undo(&mut world).unwrap();
        assert_eq!(world.entity(e).unwrap().get::<Val>(), None);
        assert_eq!(bus.stack().cursor(), 0);
    }

    #[test]
    fn redo_reapplies_world() {
        let mut bus = CommandBus::new();
        let mut world = World::new();
        let e = world.spawn();
        bus.submit(
            Box::new(InsertVal {
                entity: e,
                value: 7,
            }),
            &mut world,
        )
        .unwrap();
        bus.undo(&mut world).unwrap();
        bus.redo(&mut world).unwrap();
        assert_eq!(world.entity(e).unwrap().get::<Val>(), Some(&Val(7)));
        assert_eq!(bus.stack().cursor(), 1);
    }

    #[test]
    fn submit_truncates_redo_tail() {
        // Use a single entity for all actions so component columns stay
        // contiguous. Each action overwrites the same Val slot.
        // ReplaceVal: stores old and new; revert restores old.
        struct ReplaceVal {
            entity: EntityId,
            new_value: i32,
            old_value: i32,
        }

        impl Action for ReplaceVal {
            fn name(&self) -> &str {
                "replace-val"
            }

            fn id(&self) -> ActionId {
                // Different new_value → different id so actions don't coalesce.
                ActionId::new(format!("replace-val({:?},{})", self.entity, self.new_value))
            }

            fn apply(&self, world: &mut World) -> Result<(), ActionResult> {
                world.insert(self.entity, Val(self.new_value));
                Ok(())
            }

            fn revert(&self, world: &mut World) -> Result<(), ActionResult> {
                world.insert(self.entity, Val(self.old_value));
                Ok(())
            }
        }

        let mut bus = CommandBus::new();
        let mut world = World::new();
        let e = world.spawn_with(Val(0));

        bus.submit(
            Box::new(ReplaceVal {
                entity: e,
                new_value: 1,
                old_value: 0,
            }),
            &mut world,
        )
        .unwrap();
        bus.submit(
            Box::new(ReplaceVal {
                entity: e,
                new_value: 2,
                old_value: 1,
            }),
            &mut world,
        )
        .unwrap();
        bus.undo(&mut world).unwrap(); // val back to 1

        // Submit a third action — truncates the redo tail (original entry 2).
        bus.submit(
            Box::new(ReplaceVal {
                entity: e,
                new_value: 3,
                old_value: 1,
            }),
            &mut world,
        )
        .unwrap();

        assert_eq!(bus.stack().len(), 2);
        assert_eq!(bus.stack().cursor(), 2);
        assert_eq!(world.entity(e).unwrap().get::<Val>(), Some(&Val(3)));
    }
}
