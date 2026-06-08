//! Shell-owned execution seam for captured extension menu commands.
//!
//! This module deliberately stops at an injectable handler. It does not own
//! plugin discovery, loading, runtime execution, capabilities, async dispatch,
//! sandboxing, or registry execution.

use std::error::Error;
use std::fmt;

use rge_editor_ui::menus::Command;

use super::EditorShell;

/// Result returned by an injected extension-command handler.
pub type ExtensionCommandResult = Result<ExtensionCommandOutcome, ExtensionCommandError>;

/// Outcome for a captured extension command delivered to the injected handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtensionCommandOutcome {
    /// The handler accepted and completed the command.
    Handled,
    /// The handler was present but explicitly left this command unhandled.
    Unhandled,
}

/// Non-fatal handler failure surfaced by the extension-command seam.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtensionCommandError {
    message: String,
}

impl ExtensionCommandError {
    /// Build an error with a human-readable diagnostic message.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    /// Human-readable diagnostic message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for ExtensionCommandError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for ExtensionCommandError {}

/// Injectable shell-side handler for already-captured extension commands.
///
/// Implementations receive only `Command::Custom` / `Command::Plugin` values
/// captured by `EditorShell::route_menu_command`; core commands are routed by
/// the normal editor-shell handlers instead.
pub trait ExtensionCommandHandler {
    /// Handle one captured extension command.
    fn handle_extension_command(&mut self, command: &Command) -> ExtensionCommandResult;
}

/// Observable event recorded by the extension-command seam.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtensionCommandEvent {
    /// An extension activation was captured while no handler was configured.
    ///
    /// The command remains in `EditorShell::drain_extension_menu_commands()` so
    /// the no-handler path preserves the existing observable FIFO behavior.
    MissingHandler {
        /// Captured extension command.
        command: Command,
    },
    /// A configured handler accepted the command.
    Handled {
        /// Captured extension command.
        command: Command,
    },
    /// A configured handler explicitly declined the command.
    Unhandled {
        /// Captured extension command.
        command: Command,
    },
    /// A configured handler returned a non-fatal failure.
    Failed {
        /// Captured extension command.
        command: Command,
        /// Handler-provided failure diagnostic.
        error: String,
    },
}

impl EditorShell {
    /// Attach a synthetic or production extension-command handler.
    ///
    /// Any extension commands already captured in the shell-local FIFO are
    /// drained to the handler immediately in FIFO order. No plugin runtime,
    /// discovery, loading, or registry execution is implied by this seam.
    #[must_use]
    pub fn with_extension_command_handler(
        mut self,
        handler: Box<dyn ExtensionCommandHandler>,
    ) -> Self {
        self.extension_command_handler = Some(handler);
        self.process_captured_extension_menu_commands();
        self
    }

    /// Drain extension-command seam events in FIFO order.
    pub fn drain_extension_command_events(&mut self) -> Vec<ExtensionCommandEvent> {
        std::mem::take(&mut self.extension_command_events)
    }

    /// Drain captured extension commands to the configured handler, if present.
    ///
    /// Missing-handler behavior is intentionally non-fatal and preserves the
    /// existing captured-command FIFO for observation through
    /// [`Self::drain_extension_menu_commands`].
    pub fn process_captured_extension_menu_commands(&mut self) {
        let Some(handler) = self.extension_command_handler.as_mut() else {
            return;
        };

        for command in std::mem::take(&mut self.extension_menu_commands) {
            let diagnostic_id = command.diagnostic_id();
            let event = match handler.handle_extension_command(&command) {
                Ok(ExtensionCommandOutcome::Handled) => {
                    tracing::debug!(
                        target: "rge::editor-shell::extension-command",
                        command = %diagnostic_id,
                        "extension command handled by injected shell seam"
                    );
                    ExtensionCommandEvent::Handled { command }
                }
                Ok(ExtensionCommandOutcome::Unhandled) => {
                    tracing::warn!(
                        target: "rge::editor-shell::extension-command",
                        command = %diagnostic_id,
                        "extension command handler left command unhandled"
                    );
                    ExtensionCommandEvent::Unhandled { command }
                }
                Err(error) => {
                    let error = error.to_string();
                    tracing::warn!(
                        target: "rge::editor-shell::extension-command",
                        command = %diagnostic_id,
                        error = %error,
                        "extension command handler failed non-fatally"
                    );
                    ExtensionCommandEvent::Failed { command, error }
                }
            };
            self.extension_command_events.push(event);
        }
    }

    pub(crate) fn capture_extension_menu_command(&mut self, command: Command) {
        debug_assert!(
            matches!(command, Command::Custom(_) | Command::Plugin { .. }),
            "extension-command capture accepts only extension commands"
        );
        let diagnostic_id = command.diagnostic_id();
        let captured = command.clone();
        self.extension_menu_commands.push(command);

        if self.extension_command_handler.is_none() {
            tracing::debug!(
                target: "rge::editor-shell::extension-command",
                command = %diagnostic_id,
                "extension menu command captured with no configured handler"
            );
            self.extension_command_events
                .push(ExtensionCommandEvent::MissingHandler { command: captured });
            return;
        }

        tracing::debug!(
            target: "rge::editor-shell::extension-command",
            command = %diagnostic_id,
            "extension menu command captured for injected handler"
        );
        self.process_captured_extension_menu_commands();
    }
}
