//! [`UndoStack`] and [`SaveMark`].

use std::marker::PhantomData;

use crate::action::{ActionContextFamily, WorldActionContext};
use crate::bus::BusEntry;

// ---------------------------------------------------------------------------
// SaveMark
// ---------------------------------------------------------------------------

/// Mark indicating the cursor position at the last explicit save.
///
/// `cursor == save_mark.0` means there are no unsaved changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SaveMark(pub u64);

// ---------------------------------------------------------------------------
// UndoStack
// ---------------------------------------------------------------------------

/// The ordered history of [`BusEntry`]s that have been applied.
///
/// The `cursor` tracks how many entries are in the "applied" portion of the
/// stack. Entries at indices `[cursor, len)` are available for redo.
///
/// The [`CommandBus`](crate::CommandBus) owns the stack; `UndoStack` alone
/// does not call `apply`/`revert` — that is the bus's responsibility.
pub struct UndoStack<F: ActionContextFamily = WorldActionContext> {
    /// All entries, including redo tail above the cursor.
    pub(crate) entries: Vec<BusEntry<F>>,
    /// Number of applied entries. In `[0, entries.len()]`.
    pub(crate) cursor: u64,
    /// Cursor position at the last explicit save, if any.
    save_mark: Option<SaveMark>,
    context: PhantomData<fn(F)>,
}

impl<F: ActionContextFamily> UndoStack<F> {
    /// Create an empty [`UndoStack`].
    #[must_use]
    pub fn new_for_context() -> Self {
        Self {
            entries: Vec::new(),
            cursor: 0,
            save_mark: None,
            context: PhantomData,
        }
    }

    /// Total number of entries (applied + redo tail).
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` when the stack has no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Current cursor (number of applied entries).
    #[must_use]
    pub fn cursor(&self) -> u64 {
        self.cursor
    }

    /// The save mark, if one has been set via [`mark_saved`](Self::mark_saved).
    #[must_use]
    pub fn save_mark(&self) -> Option<SaveMark> {
        self.save_mark
    }

    /// Mark the current cursor as the saved state.
    pub fn mark_saved(&mut self) {
        self.save_mark = Some(SaveMark(self.cursor));
    }

    /// Returns `true` when the current cursor differs from the save mark.
    ///
    /// A new (never-saved) stack with no edits is considered clean
    /// (`is_dirty() == false`).
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        match self.save_mark {
            Some(SaveMark(mark)) => self.cursor != mark,
            // No save mark: dirty only if edits have been made (cursor > 0).
            None => self.cursor > 0,
        }
    }
}

impl UndoStack<WorldActionContext> {
    /// Create an empty World-only [`UndoStack`].
    #[must_use]
    pub fn new() -> Self {
        Self::new_for_context()
    }
}

impl Default for UndoStack<WorldActionContext> {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_stack_is_clean() {
        let s = UndoStack::new();
        assert!(!s.is_dirty());
        assert_eq!(s.cursor(), 0);
        assert!(s.is_empty());
    }

    #[test]
    fn mark_saved_at_zero() {
        let mut s = UndoStack::new();
        s.mark_saved();
        assert!(!s.is_dirty());
    }

    #[test]
    fn save_mark_tracks_cursor() {
        let mut s = UndoStack::new();
        // Simulate cursor movement done by the bus.
        s.cursor = 3;
        s.mark_saved();
        assert_eq!(s.save_mark(), Some(SaveMark(3)));
        assert!(!s.is_dirty());

        s.cursor = 4;
        assert!(s.is_dirty());
    }
}
