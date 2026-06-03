//! Editor-egui-host handoffs — [`InspectorHandoff`] and [`SaveStatusHandoff`]
//! (the two latest-only snapshot handoffs the host READS from the editor-shell
//! publisher) plus [`MenuCommandHandoff`] (a host→shell FIFO queue the host
//! WRITES menu-dispatched [`rge_editor_ui::menus::Command`]s into, for the
//! editor-shell consumer to drain — the reverse direction).
//!
//! Both are **type aliases** over the workspace's shared
//! [`rge_editor_state::Handoff`]:
//!
//! - [`InspectorHandoff`] = `Handoff<InspectorSnapshot>` — carries an
//!   [`rge_editor_state::InspectorSnapshot`] to the host's
//!   [`crate::InspectorTabBody`] consumer.
//! - [`SaveStatusHandoff`] = `Handoff<SaveStatusSnapshot>` — carries a
//!   [`rge_editor_state::SaveStatusSnapshot`] (open save source file name +
//!   dirty flag) to the host's bottom status bar.
//!
//! # Why aliases over a shared generic (not three hand-written copies)
//!
//! These two slots, plus editor-shell's `RenderHandoff`, were three
//! byte-identical hand-written copies of the same `Mutex<Option<Arc<_>>>` +
//! `AtomicU64` latest-only slot. With the third copy landed (Rule of Three),
//! the mechanism was unified into [`rge_editor_state::Handoff`]`<T>`
//! (GENERIC-LATEST-HANDOFF); the names persist as aliases so every call site is
//! unchanged. The earlier doctrine kept the copies verbatim "so audits grep the
//! same `Mutex<Option<Arc<` shape" — that intent is now served better by the
//! single generic definition (one place to audit). The mechanism's unit tests
//! live with the generic in `rge-editor-state`; the host-integration tests
//! (publish/acquire through the tab body, dock layout) live in this crate's
//! `tests/`.
//!
//! # Why the generic lives in editor-state (dep direction)
//!
//! The editor-shell publisher and the host consumer are in different crates,
//! and the host crate must NOT depend on editor-shell (would create a cycle and
//! foreclose the planned `editor-shell → editor-egui-host` direction). Both
//! crates already depend on `rge-editor-state`, so the shared `Handoff<T>`
//! lives there: editor-shell holds an `Arc<InspectorHandoff>` clone and
//! publishes through it; the host's tab body holds another clone and acquires
//! from it; neither crate depends on the other. No `unsafe`, std-only
//! (`unsafe_code = "forbid"` honored by the generic).

use std::collections::VecDeque;
use std::sync::Mutex;

use rge_editor_state::{Handoff, InspectorSnapshot, SaveStatusSnapshot};
use rge_editor_ui::menus::Command;

/// Latest-only handoff carrying an [`InspectorSnapshot`] from the editor-shell
/// publisher to the host's [`crate::InspectorTabBody`]. A type alias over the
/// shared [`Handoff`]; see [`rge_editor_state::Handoff`] for the full
/// latest-only contract.
pub type InspectorHandoff = Handoff<InspectorSnapshot>;

/// Latest-only handoff carrying a [`SaveStatusSnapshot`] (open save source file
/// name + dirty flag) from the editor-shell publisher to the host's bottom
/// status bar. A type alias over the shared [`Handoff`]; see
/// [`rge_editor_state::Handoff`] for the full latest-only contract.
pub type SaveStatusHandoff = Handoff<SaveStatusSnapshot>;

/// Maximum number of pending menu [`Command`]s [`MenuCommandHandoff`] buffers
/// before the shell drains them. A generous should-never-reach guard: the shell
/// drains the queue every frame (Dispatch B), so a backlog this deep means the
/// shell has stalled. On overflow the newest push is dropped with a warning
/// rather than growing unboundedly.
const MENU_COMMAND_QUEUE_CAP: usize = 64;

/// Host→shell FIFO channel for menu-dispatched [`Command`]s.
///
/// Unlike [`InspectorHandoff`] / [`SaveStatusHandoff`] (latest-only
/// [`Handoff`]`<T>` slots the host READS), menu clicks are **events**: each is a
/// distinct user intent that must NOT overwrite an earlier one. So this is a
/// deliberately different shape — a bounded FIFO `VecDeque<Command>` behind a
/// single [`Mutex`] (both [`Self::push`] and [`Self::drain`] take the same
/// lock), NOT a latest-only alias.
///
/// The host's [`crate::EguiHost::render`] pushes a [`Command`] when a File,
/// Edit, Play, or View menu item is activated; the editor-shell consumer clones the `Arc`
/// (via [`crate::EguiHost::menu_command_handoff`]) and drains it at the top of
/// each frame (`EditorShell::drain_and_route_menu_commands`), routing each
/// [`Command`] one-way to its existing handler.
#[derive(Debug, Default)]
pub struct MenuCommandHandoff {
    queue: Mutex<VecDeque<Command>>,
}

impl MenuCommandHandoff {
    /// Construct an empty queue.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Enqueue a menu-dispatched [`Command`] (FIFO `push_back`). When the queue
    /// is already at [`MENU_COMMAND_QUEUE_CAP`] the push is dropped with a
    /// `tracing::warn!` — a stall guard that should never fire in practice (the
    /// shell drains every frame).
    pub fn push(&self, cmd: Command) {
        let mut q = self.queue.lock().expect("MenuCommandHandoff lock poisoned");
        if q.len() >= MENU_COMMAND_QUEUE_CAP {
            tracing::warn!(
                target: "rge::editor-egui-host::menu",
                cap = MENU_COMMAND_QUEUE_CAP,
                dropped = %cmd.diagnostic_id(),
                "menu command queue full; dropping newest command (shell not draining?)"
            );
            return;
        }
        q.push_back(cmd);
    }

    /// Drain all pending commands in FIFO order, leaving the queue empty.
    #[must_use]
    pub fn drain(&self) -> Vec<Command> {
        let mut q = self.queue.lock().expect("MenuCommandHandoff lock poisoned");
        q.drain(..).collect()
    }

    /// Number of pending (un-drained) commands. Primarily for tests/diagnostics.
    #[must_use]
    pub fn len(&self) -> usize {
        self.queue
            .lock()
            .expect("MenuCommandHandoff lock poisoned")
            .len()
    }

    /// `true` when no commands are pending.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod menu_command_handoff_tests {
    use super::{Command, MenuCommandHandoff, MENU_COMMAND_QUEUE_CAP};

    #[test]
    fn drains_fifo_then_empties() {
        let h = MenuCommandHandoff::new();
        assert!(h.is_empty());
        h.push(Command::OpenFile);
        h.push(Command::Save);
        h.push(Command::SaveAs);
        assert_eq!(h.len(), 3);
        assert_eq!(
            h.drain(),
            vec![Command::OpenFile, Command::Save, Command::SaveAs],
            "commands drain in FIFO (push) order"
        );
        assert!(h.is_empty(), "drain empties the queue");
        assert!(h.drain().is_empty(), "a second drain returns nothing");
    }

    #[test]
    fn is_bounded_dropping_newest() {
        let h = MenuCommandHandoff::new();
        for _ in 0..MENU_COMMAND_QUEUE_CAP {
            h.push(Command::OpenFile);
        }
        assert_eq!(h.len(), MENU_COMMAND_QUEUE_CAP);
        // Overflow pushes are dropped (drop-newest); len stays capped and the
        // distinct overflow commands never leak into the retained queue.
        h.push(Command::Save);
        h.push(Command::SaveAs);
        assert_eq!(
            h.len(),
            MENU_COMMAND_QUEUE_CAP,
            "overflow pushes are dropped"
        );
        let drained = h.drain();
        assert_eq!(drained.len(), MENU_COMMAND_QUEUE_CAP);
        assert!(
            drained.iter().all(|c| *c == Command::OpenFile),
            "drop-newest keeps the first CAP pushes; the overflow Save/SaveAs are gone"
        );
    }
}
