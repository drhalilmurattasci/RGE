//! Phase 9 — keyboard playback shortcuts (`Space` / `Escape`).
//!
//! Routes plain-key (no Ctrl, no Shift, no Alt) `Space` / `Escape` events
//! to the existing [`EditorShell::handle_button`] state-machine driver.
//! No CommandBus involvement; no new ECS state; no toolbar UI.
//!
//! # Why a separate module from `commands.rs`
//!
//! `commands.rs` ships [`crate::lifecycle::commands::EditorKeyCommand`] —
//! `Ctrl+Z` / `Ctrl+Y` / `Ctrl+S` — and the [`SetTimeScale`] [`Action`]
//! impl. Every command there routes through
//! [`rge_editor_actions::CommandBus`] so its mutations appear on the
//! undo stack + audit ledger.
//!
//! Playback transitions are different in kind:
//!
//! - They drive the [`crate::play_state::PlayState`] state machine, not
//!   the kernel-ecs `World`. PIE snapshot capture/restore is an editor
//!   lifecycle event, not an undoable user action — pressing `Space` to
//!   start play and then `Ctrl+Z` should NOT roll back to the
//!   pre-`Space` state (that would be incoherent — the user might be
//!   mid-tick).
//! - They are gated by the state machine itself (`Pause` from `Editing`
//!   is rejected with `PlayStateError::NotInPie`; the bus has no such
//!   semantic-domain rejections).
//! - The audit trail already lives on [`crate::audit::AuditLedger`]
//!   via [`crate::audit::AuditEvent::PlayPressed`] / `StopPressed` etc.
//!   recorded inside `handle_button`; routing through the bus would
//!   produce a dual record without adding observability.
//!
//! Keeping playback in its own module makes the dispatch lane explicit:
//! Ctrl-bound bindings → `commands::EditorKeyCommand` → bus;
//! plain-key bindings → `playback::EditorPlaybackCommand` → state machine.
//!
//! # Toggle semantics (`Space`)
//!
//! `Space` is the universal "go / pause" key. Mapping:
//!
//! | State before | Space outcome |
//! |---|---|
//! | [`PlayState::Editing`] | [`ToolbarButtonId::Play`] → `Playing` (captures snapshot) |
//! | [`PlayState::Playing`] | [`ToolbarButtonId::Pause`] → `Paused` |
//! | [`PlayState::Paused`] | [`ToolbarButtonId::Play`] → `Playing` (Resume) |
//!
//! Re-uses [`EditorShell::handle_button`] verbatim — the audit ledger
//! and snapshot capture/restore happen there. No duplicate code path.
//!
//! # Stop semantics (`Escape`)
//!
//! `Escape` is the "exit play mode" key. Mapping:
//!
//! | State before | Escape outcome |
//! |---|---|
//! | [`PlayState::Editing`] | NO-OP (no PIE in flight) |
//! | [`PlayState::Playing`] | [`ToolbarButtonId::Stop`] → `Editing` (restores snapshot) |
//! | [`PlayState::Paused`] | [`ToolbarButtonId::Stop`] → `Editing` (restores snapshot) |
//!
//! In `Editing` the press is silently dropped — the toolbar's `Stop`
//! handler would have errored with [`PlayStateError::NoSnapshot`], but
//! that is exactly the "nothing to stop" case we want to render as a
//! no-op rather than a diagnostic line. (Symmetric to `Ctrl+Z` on an
//! empty undo stack — see `commands::EditorShell::handle_key_command`'s
//! `NothingToUndo` swallow.)
//!
//! # No modifiers
//!
//! `Space` and `Escape` fire only when **no** modifiers are held —
//! Ctrl, Shift, or Alt being down skips the binding. This keeps future
//! Ctrl+Space / Shift+Escape bindings unblocked for additional commands
//! (e.g. Ctrl+Space for "step one tick", Shift+Escape for "force stop
//! without restore") that future dispatches may add.

use rge_input::KeyCode;
use winit::keyboard::ModifiersState;

use crate::lifecycle::EditorShell;
use crate::play_state::PlayState;
use crate::play_toolbar::ToolbarButtonId;

// ---------------------------------------------------------------------------
// EditorPlaybackCommand
// ---------------------------------------------------------------------------

/// Editor-side keyboard command that drives the PIE state machine
/// (without touching the [`rge_editor_actions::CommandBus`] — see the
/// module-level docstring for why).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorPlaybackCommand {
    /// `Space` — toggle Editing / Playing / Paused per the universal
    /// "go-or-pause" mapping. See module docs for the per-state
    /// outcome table.
    TogglePlay,

    /// `Escape` — stop the current PIE session (if any). Silent no-op
    /// while in `Editing` so the binding is safe to bind even when no
    /// PIE is in flight.
    Stop,
}

impl EditorPlaybackCommand {
    /// Map a translated [`KeyCode`] plus the current
    /// [`ModifiersState`] to an [`EditorPlaybackCommand`]. Returns
    /// `None` for any modifier-held press or any unbound key.
    ///
    /// Plain-key bindings ONLY: any of Ctrl / Shift / Alt down makes
    /// the binding inert. The `Logo` (Super/Win/Cmd) modifier is also
    /// rejected so future Cmd+Space bindings (macOS spotlight-style)
    /// don't collide.
    #[must_use]
    pub fn from_key_press(key: KeyCode, modifiers: ModifiersState) -> Option<Self> {
        // Any modifier disables the binding. Matching `is_empty()` is
        // the cleanest spelling against winit 0.30's `ModifiersState`
        // bitflag set; `control_key()` / `shift_key()` / `alt_key()` /
        // `super_key()` would need to be `&&`-checked individually
        // with the same effect but more verbosely.
        if !modifiers.is_empty() {
            return None;
        }
        Some(match key {
            KeyCode::Space => Self::TogglePlay,
            KeyCode::Escape => Self::Stop,
            _ => return None,
        })
    }
}

// ---------------------------------------------------------------------------
// EditorShell::handle_playback_command
// ---------------------------------------------------------------------------

impl EditorShell {
    /// Dispatch a single [`EditorPlaybackCommand`] against the PIE
    /// state machine. Public so headless tests can drive playback
    /// without synthesizing winit `KeyEvent`s; production usage
    /// routes through the `WindowEvent::KeyboardInput` branch in
    /// [`EditorShell::window_event`].
    ///
    /// All toolbar-button errors are swallowed silently so a stray
    /// press from an "impossible" state doesn't spam diagnostics. The
    /// only expected-error paths are:
    ///
    /// - [`EditorPlaybackCommand::TogglePlay`] from `Playing` mapping
    ///   to `Pause` — `Pause` cannot error from `Playing`.
    /// - [`EditorPlaybackCommand::Stop`] from `Editing` — `Stop`
    ///   would return `PlayStateError::NoSnapshot`; we route AROUND
    ///   that by checking the state first and not calling
    ///   `handle_button(Stop)` in `Editing`.
    ///
    /// # Why this method instead of inline match in `window_event`
    ///
    /// (a) keeps the keyboard branch in `window_event` short + readable;
    /// (b) lets headless tests drive playback via `&mut EditorShell`
    /// without a winit `KeyEvent`; (c) symmetric to
    /// [`EditorShell::handle_key_command`] for `EditorKeyCommand` —
    /// every editor key-bound concept gets a single dispatch method.
    pub fn handle_playback_command(&mut self, command: EditorPlaybackCommand) {
        match command {
            EditorPlaybackCommand::TogglePlay => {
                // Decide button based on current state. Read once;
                // handle_button takes &mut self and will mutate
                // self.state, but the post-read value is what drives
                // the mapping (no race; this is single-threaded).
                let button = match self.play_state() {
                    PlayState::Editing | PlayState::Paused => ToolbarButtonId::Play,
                    PlayState::Playing => ToolbarButtonId::Pause,
                };
                if let Err(e) = self.handle_button(button) {
                    // Expected to never fire given the state-switched
                    // mapping above, but the swallow keeps the binding
                    // diagnostic-free if a future state addition
                    // breaks the invariant before this method is
                    // updated.
                    tracing::warn!(
                        target: "rge::editor-shell::lifecycle",
                        error = ?e,
                        ?button,
                        "Space binding hit unexpected toolbar-button error"
                    );
                }
            }
            EditorPlaybackCommand::Stop => {
                // Escape in `Editing` is intentionally a silent no-op
                // — there's no PIE in flight. Avoiding the
                // `handle_button(Stop)` call sidesteps the
                // `PlayStateError::NoSnapshot` error path that we'd
                // otherwise have to swallow.
                if self.play_state().is_pie_active() {
                    if let Err(e) = self.handle_button(ToolbarButtonId::Stop) {
                        tracing::warn!(
                            target: "rge::editor-shell::lifecycle",
                            error = ?e,
                            "Escape binding hit unexpected toolbar-button error"
                        );
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! # On the egui-consumed gate
    //!
    //! These tests drive [`EditorShell::handle_playback_command`]
    //! directly, bypassing the `!egui_consumed` gate in
    //! [`crate::lifecycle::EditorShell::window_event`]'s
    //! `KeyboardInput` arm. That gate is not exercised here because
    //! its substrate (a real
    //! [`rge_editor_egui_host::EguiHost`]) requires a `wgpu::Device`
    //! + `winit::Window`, neither of which a headless shell has.
    //!
    //! The same gap exists for [`commands::EditorKeyCommand`]'s test
    //! coverage (see `tests/keyboard_command_bus_round_trip.rs` which
    //! also calls `handle_key_command` directly). When the host
    //! eventually gains a headless test seam — e.g. an
    //! `EguiHost::with_mock_input(...)` constructor that doesn't
    //! touch wgpu — both keyboard surfaces can be tested through the
    //! same gate in one shot.

    use super::*;

    // -----------------------------------------------------------------------
    // from_key_press — key/modifier mapping
    // -----------------------------------------------------------------------

    #[test]
    fn space_maps_to_toggle_play_with_no_modifiers() {
        assert_eq!(
            EditorPlaybackCommand::from_key_press(KeyCode::Space, ModifiersState::empty()),
            Some(EditorPlaybackCommand::TogglePlay)
        );
    }

    #[test]
    fn escape_maps_to_stop_with_no_modifiers() {
        assert_eq!(
            EditorPlaybackCommand::from_key_press(KeyCode::Escape, ModifiersState::empty()),
            Some(EditorPlaybackCommand::Stop)
        );
    }

    #[test]
    fn space_with_ctrl_is_unbound() {
        assert_eq!(
            EditorPlaybackCommand::from_key_press(KeyCode::Space, ModifiersState::CONTROL),
            None
        );
    }

    #[test]
    fn space_with_shift_is_unbound() {
        assert_eq!(
            EditorPlaybackCommand::from_key_press(KeyCode::Space, ModifiersState::SHIFT),
            None
        );
    }

    #[test]
    fn space_with_alt_is_unbound() {
        assert_eq!(
            EditorPlaybackCommand::from_key_press(KeyCode::Space, ModifiersState::ALT),
            None
        );
    }

    #[test]
    fn space_with_super_is_unbound() {
        assert_eq!(
            EditorPlaybackCommand::from_key_press(KeyCode::Space, ModifiersState::SUPER),
            None
        );
    }

    #[test]
    fn escape_with_any_modifier_is_unbound() {
        for m in [
            ModifiersState::CONTROL,
            ModifiersState::SHIFT,
            ModifiersState::ALT,
            ModifiersState::SUPER,
        ] {
            assert_eq!(
                EditorPlaybackCommand::from_key_press(KeyCode::Escape, m),
                None
            );
        }
    }

    #[test]
    fn unbound_keys_return_none() {
        let no_mod = ModifiersState::empty();
        for key in [
            KeyCode::KeyA,
            KeyCode::KeyZ,
            KeyCode::Enter,
            KeyCode::Tab,
            KeyCode::ArrowLeft,
        ] {
            assert_eq!(
                EditorPlaybackCommand::from_key_press(key, no_mod),
                None,
                "{key:?} should not map to a playback command"
            );
        }
    }

    // -----------------------------------------------------------------------
    // handle_playback_command — state transitions
    // -----------------------------------------------------------------------

    #[test]
    fn space_from_editing_enters_playing() {
        let mut shell = EditorShell::new();
        assert_eq!(shell.play_state(), PlayState::Editing);
        shell.handle_playback_command(EditorPlaybackCommand::TogglePlay);
        assert_eq!(shell.play_state(), PlayState::Playing);
        assert!(
            shell.has_snapshot(),
            "Editing -> Playing must capture a snapshot via handle_button(Play)"
        );
    }

    #[test]
    fn space_from_playing_enters_paused() {
        let mut shell = EditorShell::new();
        shell.handle_playback_command(EditorPlaybackCommand::TogglePlay); // -> Playing
        shell.handle_playback_command(EditorPlaybackCommand::TogglePlay); // -> Paused
        assert_eq!(shell.play_state(), PlayState::Paused);
        assert!(
            shell.has_snapshot(),
            "Pause must preserve the snapshot captured at Play"
        );
    }

    #[test]
    fn space_from_paused_resumes_playing() {
        let mut shell = EditorShell::new();
        shell.handle_playback_command(EditorPlaybackCommand::TogglePlay); // Editing -> Playing
        shell.handle_playback_command(EditorPlaybackCommand::TogglePlay); // Playing -> Paused
        shell.handle_playback_command(EditorPlaybackCommand::TogglePlay); // Paused -> Playing
        assert_eq!(shell.play_state(), PlayState::Playing);
        assert!(shell.has_snapshot());
    }

    #[test]
    fn space_cycle_returns_to_playing_then_stop_restores_editing() {
        // End-to-end exercise of all three Space states + Escape stop.
        let mut shell = EditorShell::new();
        for cycle in 0..3 {
            shell.handle_playback_command(EditorPlaybackCommand::TogglePlay); // Editing -> Playing
            assert_eq!(shell.play_state(), PlayState::Playing, "cycle {cycle}");
            shell.handle_playback_command(EditorPlaybackCommand::TogglePlay); // Playing -> Paused
            assert_eq!(shell.play_state(), PlayState::Paused, "cycle {cycle}");
            shell.handle_playback_command(EditorPlaybackCommand::Stop); // Paused -> Editing
            assert_eq!(shell.play_state(), PlayState::Editing, "cycle {cycle}");
            assert!(
                !shell.has_snapshot(),
                "Stop must restore + drop the snapshot (cycle {cycle})"
            );
        }
    }

    #[test]
    fn escape_in_editing_is_silent_noop() {
        let mut shell = EditorShell::new();
        assert_eq!(shell.play_state(), PlayState::Editing);
        let before_audit_len = shell.audit().len();
        shell.handle_playback_command(EditorPlaybackCommand::Stop);
        assert_eq!(
            shell.play_state(),
            PlayState::Editing,
            "Escape in Editing must not transition"
        );
        assert!(
            !shell.has_snapshot(),
            "Escape in Editing must not produce a snapshot"
        );
        assert_eq!(
            shell.audit().len(),
            before_audit_len,
            "Escape in Editing must not record an audit event"
        );
    }

    #[test]
    fn escape_from_playing_stops_and_restores() {
        let mut shell = EditorShell::new();
        shell.handle_playback_command(EditorPlaybackCommand::TogglePlay); // -> Playing
        assert!(shell.has_snapshot());
        shell.handle_playback_command(EditorPlaybackCommand::Stop); // -> Editing
        assert_eq!(shell.play_state(), PlayState::Editing);
        assert!(!shell.has_snapshot());
    }

    #[test]
    fn escape_from_paused_stops_and_restores() {
        let mut shell = EditorShell::new();
        shell.handle_playback_command(EditorPlaybackCommand::TogglePlay); // -> Playing
        shell.handle_playback_command(EditorPlaybackCommand::TogglePlay); // -> Paused
        shell.handle_playback_command(EditorPlaybackCommand::Stop); // -> Editing
        assert_eq!(shell.play_state(), PlayState::Editing);
        assert!(!shell.has_snapshot());
    }

    // -----------------------------------------------------------------------
    // Inspector snapshot reflects playback transitions
    // -----------------------------------------------------------------------

    #[test]
    fn inspector_snapshot_reflects_play_state_transitions_via_shortcuts() {
        let mut shell = EditorShell::new();

        let snap0 = shell.inspector_snapshot();
        assert_eq!(snap0.play_state_label, "Editing");
        assert!(!snap0.has_snapshot);

        shell.handle_playback_command(EditorPlaybackCommand::TogglePlay);
        let snap1 = shell.inspector_snapshot();
        assert_eq!(snap1.play_state_label, "Playing");
        assert!(snap1.has_snapshot);

        shell.handle_playback_command(EditorPlaybackCommand::TogglePlay);
        let snap2 = shell.inspector_snapshot();
        assert_eq!(snap2.play_state_label, "Paused");
        assert!(snap2.has_snapshot);

        shell.handle_playback_command(EditorPlaybackCommand::Stop);
        let snap3 = shell.inspector_snapshot();
        assert_eq!(snap3.play_state_label, "Editing");
        assert!(!snap3.has_snapshot);
    }

    #[test]
    fn inspector_snapshot_tick_count_advances_while_playing_via_shortcuts() {
        let mut shell = EditorShell::new();
        assert_eq!(shell.inspector_snapshot().tick_count, 0);

        shell.handle_playback_command(EditorPlaybackCommand::TogglePlay); // -> Playing
        shell.run_for_redraws(5);

        let snap = shell.inspector_snapshot();
        assert_eq!(
            snap.tick_count, 5,
            "5 redraws while Playing must advance tick_count by 5"
        );
        assert_eq!(snap.play_state_label, "Playing");
    }

    #[test]
    fn inspector_snapshot_tick_count_frozen_after_escape_back_to_editing() {
        let mut shell = EditorShell::new();
        shell.handle_playback_command(EditorPlaybackCommand::TogglePlay); // -> Playing
        shell.run_for_redraws(3);
        assert_eq!(shell.inspector_snapshot().tick_count, 3);

        shell.handle_playback_command(EditorPlaybackCommand::Stop); // -> Editing
                                                                    // run_for_redraws after Stop must NOT advance tick_count
                                                                    // (PlayState::Editing::game_systems_run() == false).
        shell.run_for_redraws(10);
        let snap = shell.inspector_snapshot();
        assert_eq!(snap.tick_count, 3, "ticks must freeze when Stopped");
        assert_eq!(snap.play_state_label, "Editing");
    }
}
