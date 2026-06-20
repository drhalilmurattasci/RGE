//! Commands that menu entries dispatch when activated.
//!
//! adapted from rustforge::apps::editor-app::egui_overlay (menu bar) on 2026-05-05
//! — rebuilt as data-driven `MenuRegistry`.
//!
//! The rustforge prior art used a `MenuAction` enum on the host
//! (`MenuAction::OpenFile`, `MenuAction::Save`, etc.) and rendered
//! menus directly. Here the registry stores [`Command`] inside each
//! [`crate::menus::MenuEntry`] so plugins can register entries that
//! dispatch through their own command without the host needing to add
//! a variant — extension is via [`Command::Plugin`] / [`Command::Custom`].
//!
//! The variant set covers the v0.8 plan §6.3 floor (`open file, save,
//! undo, redo`) plus the standard editor surface inferred from the
//! rustforge `MenuAction` enum. `Custom(String)` is the catch-all the
//! editor-app uses to forward stable action ids; `Plugin` is the
//! tier-3 extension hatch that pairs a plugin id with the plugin's
//! own action id.

/// What clicking / activating a menu entry dispatches.
///
/// Open enum: every variant beyond [`Self::Custom`] / [`Self::Plugin`]
/// is a "core" command the editor-shell knows how to dispatch
/// directly. New core commands require a deliberate addition here;
/// plugin commands stay in [`Self::Plugin`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Command {
    /// File → Open.
    OpenFile,
    /// File → New.
    NewFile,
    /// File → Save.
    Save,
    /// File → Save As.
    SaveAs,
    /// File → Close.
    Close,
    /// File → Exit / Quit.
    Quit,
    /// Edit → Undo.
    Undo,
    /// Edit → Redo.
    Redo,
    /// Edit → Cut.
    Cut,
    /// Edit → Copy.
    Copy,
    /// Edit → Paste.
    Paste,
    /// Edit → Delete.
    Delete,
    /// Edit → Duplicate.
    Duplicate,
    /// Edit -> Delete Current CAD Cuboid.
    DeleteCurrentCadCuboid,
    /// Edit → Select All.
    SelectAll,
    /// View → Reset Camera.
    ResetCamera,
    /// View → Zoom In.
    ZoomIn,
    /// View → Zoom Out.
    ZoomOut,
    /// Toggle the command palette.
    ToggleCommandPalette,
    /// Play in Editor — start.
    PlayStart,
    /// Play in Editor — stop.
    PlayStop,
    /// Play in Editor — pause.
    PlayPause,
    /// Play in Editor — single-frame step.
    PlayStep,
    /// A core action keyed by stable id (`"editor.action.foo"`). Used
    /// when adding a full enum variant would be premature; the host
    /// dispatches by string lookup. Mirrors the rustforge
    /// `MenuAction::from_action_id` path.
    Custom(String),
    /// A plugin-supplied action. The pair is `(plugin_id, action_id)`;
    /// the script-host routes the call to the named plugin which
    /// resolves the action id internally.
    Plugin {
        /// Stable plugin id (`"com.example.my-plugin"`).
        plugin_id: String,
        /// Plugin-defined action id.
        action_id: String,
    },
}

impl Command {
    /// `true` when this command is one of the built-in core variants.
    /// `Custom` and `Plugin` return `false`. Useful for diagnostics
    /// and dispatch routing in the editor-shell.
    #[must_use]
    pub fn is_core(&self) -> bool {
        !matches!(self, Self::Custom(_) | Self::Plugin { .. })
    }

    /// Stable diagnostic id. Core variants return their snake_case
    /// name; `Custom(s)` returns the inner string; `Plugin{p, a}`
    /// returns `"plugin:<p>::<a>"`.
    #[must_use]
    pub fn diagnostic_id(&self) -> String {
        match self {
            Self::OpenFile => "open_file".into(),
            Self::NewFile => "new_file".into(),
            Self::Save => "save".into(),
            Self::SaveAs => "save_as".into(),
            Self::Close => "close".into(),
            Self::Quit => "quit".into(),
            Self::Undo => "undo".into(),
            Self::Redo => "redo".into(),
            Self::Cut => "cut".into(),
            Self::Copy => "copy".into(),
            Self::Paste => "paste".into(),
            Self::Delete => "delete".into(),
            Self::Duplicate => "duplicate".into(),
            Self::DeleteCurrentCadCuboid => "delete_current_cad_cuboid".into(),
            Self::SelectAll => "select_all".into(),
            Self::ResetCamera => "reset_camera".into(),
            Self::ZoomIn => "zoom_in".into(),
            Self::ZoomOut => "zoom_out".into(),
            Self::ToggleCommandPalette => "toggle_command_palette".into(),
            Self::PlayStart => "play_start".into(),
            Self::PlayStop => "play_stop".into(),
            Self::PlayPause => "play_pause".into(),
            Self::PlayStep => "play_step".into(),
            Self::Custom(s) => s.clone(),
            Self::Plugin {
                plugin_id,
                action_id,
            } => {
                format!("plugin:{plugin_id}::{action_id}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_core_separates_extensions() {
        assert!(Command::Save.is_core());
        assert!(Command::Undo.is_core());
        assert!(!Command::Custom("x".into()).is_core());
        assert!(!Command::Plugin {
            plugin_id: "p".into(),
            action_id: "a".into(),
        }
        .is_core());
    }

    #[test]
    fn diagnostic_ids_are_stable() {
        assert_eq!(Command::OpenFile.diagnostic_id(), "open_file");
        assert_eq!(Command::Save.diagnostic_id(), "save");
        assert_eq!(
            Command::DeleteCurrentCadCuboid.diagnostic_id(),
            "delete_current_cad_cuboid"
        );
        assert_eq!(
            Command::Custom("editor.action.foo".into()).diagnostic_id(),
            "editor.action.foo"
        );
        assert_eq!(
            Command::Plugin {
                plugin_id: "com.example".into(),
                action_id: "go".into(),
            }
            .diagnostic_id(),
            "plugin:com.example::go",
        );
    }
}
