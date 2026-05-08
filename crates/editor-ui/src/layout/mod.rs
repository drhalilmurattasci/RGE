//! `editor_ui::layout` — RON workspace loader, layout-tree types, hot-reload watcher.
//!
//! Per PLAN.md §6.5 (page layout) + ADR-018 (RON over JSON/XML).
//!
//! Wave W09 deliverable. Adapted from
//! `rustforge::apps::editor-app::ir_bridge` on 2026-05-05 — generalized for
//! `Workspace`. The rustforge precursor was a single-IR bake-or-load bridge for
//! graphics state; this module reuses the same RON serde-typed pattern but the
//! IR is a recursive `LayoutNode` tree describing a panelled editor layout
//! (UE Slate `FLayoutSaveRestore` analogue).
//!
//! ## Module map
//!
//! * `node` — `LayoutNode` enum (`HSplit`, `VSplit`, `Stack`, `Toolbar`) + `TabId` stub.
//! * `workspace` — `Workspace` document struct (name, version, theme, layout,
//!   menu/toolbar/overlay metadata) + `MainMenuEntry`/`WorkspaceToolbar`/`ShortcutsOverlay`.
//! * `io` — RON read/write with `canonical_pretty_config` for byte-stable round-trip.
//! * `version` — workspace versioning + `0.1.0 → 0.2.0` migration ladder.
//! * `reconcile` — diff-based hot-reload (preserves scroll/selection/focus on
//!   panes whose `LayoutNodeId` is unchanged across reload).
//! * `hot_reload` — `notify`-backed file watcher.
//!
//! ## Vendored defaults
//!
//! Four workspaces ship under `assets/defaults/`:
//!
//! * `default-workspace.ron` — 3-pane scene+viewport+inspector.
//! * `animation-workspace.ron` — anim graph + timeline.
//! * `sculpt-workspace.ron` — large viewport + brush panel.
//! * `code-workspace.ron` — script editor + inspector.
//!
//! These are also `include_str!`'d as constants on this module (see
//! `DEFAULT_WORKSPACE_RON` etc.) so a release binary launched from any working
//! directory has the canonical defaults available without depending on the
//! assets dir being reachable on disk (rustforge Phase 3 P2 contract).

pub mod hot_reload;
pub mod io;
pub mod node;
pub mod reconcile;
pub mod version;
pub mod workspace;

pub use hot_reload::{ChangeEvent, WatchError, WorkspaceWatcher};
pub use io::{
    canonical_pretty_config, deserialize_workspace, read_workspace, serialize_workspace,
    workspace_content_hash, write_workspace, WorkspaceIoError,
};
pub use node::{LayoutNode, LayoutNodeId, LayoutValidateError, TabId, ToolbarPosition};
pub use reconcile::{diff, Op};
pub use version::{migrate, CURRENT_WORKSPACE_VERSION, MIN_SUPPORTED_WORKSPACE_VERSION};
pub use workspace::{MainMenuEntry, ShortcutsOverlay, Workspace, WorkspaceToolbar};

/// Compile-time-baked content of `assets/defaults/default-workspace.ron`.
///
/// Phase 3 P2 packaging contract (per rustforge `ir_bridge`): release binary
/// launched from any cwd still has every default workspace available without
/// the assets dir being reachable on disk.
pub const DEFAULT_WORKSPACE_RON: &str = include_str!("../../assets/defaults/default-workspace.ron");

/// Compile-time-baked content of `assets/defaults/animation-workspace.ron`.
pub const ANIMATION_WORKSPACE_RON: &str =
    include_str!("../../assets/defaults/animation-workspace.ron");

/// Compile-time-baked content of `assets/defaults/sculpt-workspace.ron`.
pub const SCULPT_WORKSPACE_RON: &str = include_str!("../../assets/defaults/sculpt-workspace.ron");

/// Compile-time-baked content of `assets/defaults/code-workspace.ron`.
pub const CODE_WORKSPACE_RON: &str = include_str!("../../assets/defaults/code-workspace.ron");

/// One of the four built-in workspace presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultWorkspace {
    /// 3-pane scene+viewport+inspector.
    Default,
    /// Anim graph + timeline.
    Animation,
    /// Large viewport + brush panel.
    Sculpt,
    /// Script editor + inspector.
    Code,
}

impl DefaultWorkspace {
    /// Borrow the embedded RON text.
    #[must_use]
    pub fn ron(self) -> &'static str {
        match self {
            DefaultWorkspace::Default => DEFAULT_WORKSPACE_RON,
            DefaultWorkspace::Animation => ANIMATION_WORKSPACE_RON,
            DefaultWorkspace::Sculpt => SCULPT_WORKSPACE_RON,
            DefaultWorkspace::Code => CODE_WORKSPACE_RON,
        }
    }

    /// Filename under `assets/defaults/`.
    #[must_use]
    pub fn filename(self) -> &'static str {
        match self {
            DefaultWorkspace::Default => "default-workspace.ron",
            DefaultWorkspace::Animation => "animation-workspace.ron",
            DefaultWorkspace::Sculpt => "sculpt-workspace.ron",
            DefaultWorkspace::Code => "code-workspace.ron",
        }
    }

    /// Load the embedded workspace, running migrations + validation.
    ///
    /// # Errors
    ///
    /// Returns `WorkspaceIoError::RonParse` if the embedded RON does not parse,
    /// `WorkspaceIoError::UnsupportedVersion` if the embedded version is past
    /// `CURRENT_WORKSPACE_VERSION`, or `WorkspaceIoError::Validate` if the
    /// loaded tree fails `LayoutNode::validate`.
    pub fn load(self) -> Result<Workspace, WorkspaceIoError> {
        deserialize_workspace(self.ron(), self.filename())
    }

    /// Iterator over all four built-in presets.
    #[must_use]
    pub fn all() -> [DefaultWorkspace; 4] {
        [
            DefaultWorkspace::Default,
            DefaultWorkspace::Animation,
            DefaultWorkspace::Sculpt,
            DefaultWorkspace::Code,
        ]
    }
}

#[cfg(test)]
mod smoke {
    use super::*;

    #[test]
    fn all_four_defaults_load() {
        for which in DefaultWorkspace::all() {
            let ws = which.load().unwrap_or_else(|e| {
                panic!("default workspace {} failed to load: {e}", which.filename())
            });
            assert_eq!(ws.version, CURRENT_WORKSPACE_VERSION);
        }
    }
}
