//! `Workspace` — top-level RON-serialized editor layout document.
//!
//! Adapted from rustforge::apps::editor-app::ir_bridge on 2026-05-05 — generalized
//! for Workspace. UE Slate analogue: `FLayoutSaveRestore` snapshot. Per PLAN.md §6.5
//! / ADR-018 — RON only, single source-format family.
//!
//! Workspaces are versioned (`0.1.0`, `0.2.0`, ...). The version field gates the
//! migration logic in `version.rs`. Round-trip byte-identical RON is a CI gate
//! (per the wave's exit criteria).

use serde::{Deserialize, Serialize};

use super::node::{LayoutNode, LayoutNodeId, ToolbarPosition};

/// Current workspace schema version. Bumped when adding/removing fields with
/// breaking semantics; the on-disk migration ladder lives in `version.rs`.
pub const CURRENT_WORKSPACE_VERSION: &str = "0.2.0";

/// Earliest schema version the loader can migrate from.
pub const MIN_SUPPORTED_WORKSPACE_VERSION: &str = "0.1.0";

/// One row of a top-level menu (resolved by W08 menu registry by `id`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MainMenuEntry {
    /// Stable id (resolved against the menu registry — W08).
    pub id: String,
    /// Display label (English; localization is W05+).
    pub label: String,
}

/// Toolbar bound to one of the workspace's edges (independent of `LayoutNode::Toolbar`,
/// which is for in-pane toolbars; this list is for the workspace-level chrome row).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceToolbar {
    /// Stable id (also used by the reconciler to preserve toggle state).
    pub id: String,
    /// Edge to pin against.
    pub position: ToolbarPosition,
    /// Extension-point id (resolved against the menu registry — W08).
    pub extension_point: String,
    /// Whether visible at startup.
    #[serde(default = "default_true")]
    pub visible: bool,
}

fn default_true() -> bool {
    true
}

/// Shortcuts overlay configuration. Surfaced via the `?` key (UE pattern).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShortcutsOverlay {
    /// Whether the overlay is enabled in this workspace.
    pub enabled: bool,
    /// Optional extension-point id to source overlay rows (e.g. `"shortcuts.default"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extension_point: Option<String>,
}

impl Default for ShortcutsOverlay {
    fn default() -> Self {
        Self {
            enabled: true,
            extension_point: None,
        }
    }
}

/// Top-level workspace document — one of these per RON file.
///
/// Versioned via `version`. Migrations live in `version.rs`; on load, the version
/// field is checked first and a stale workspace is run through the migration
/// ladder before being returned to the caller.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Workspace {
    /// Display name (e.g. `"Default"`, `"Animation"`, `"Sculpt"`, `"Code"`).
    pub name: String,
    /// Schema version of this document on disk. See `CURRENT_WORKSPACE_VERSION`.
    pub version: String,
    /// Theme id (resolved against `ui-theme` — W05). `None` ⇒ system default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<String>,
    /// Root layout tree.
    pub layout: LayoutNode,
    /// Top-level menu rows (File, Edit, Window, ...).
    pub main_menu: Vec<MainMenuEntry>,
    /// Workspace-level toolbars (separate from in-pane `LayoutNode::Toolbar`s).
    pub toolbars: Vec<WorkspaceToolbar>,
    /// Shortcuts-overlay configuration.
    #[serde(default)]
    pub shortcuts_overlay: ShortcutsOverlay,
}

impl Workspace {
    /// Construct a minimal workspace pinned to the current schema version.
    #[must_use]
    pub fn new(name: impl Into<String>, layout: LayoutNode) -> Self {
        Self {
            name: name.into(),
            version: CURRENT_WORKSPACE_VERSION.to_owned(),
            theme: None,
            layout,
            main_menu: Vec::new(),
            toolbars: Vec::new(),
            shortcuts_overlay: ShortcutsOverlay::default(),
        }
    }

    /// Validate structural invariants on the layout tree and string fields.
    ///
    /// Returns the first error found. `version` and `name` non-empty; layout
    /// tree validates per `LayoutNode::validate`.
    ///
    /// # Errors
    ///
    /// Forwards any error from `LayoutNode::validate`.
    pub fn validate(&self) -> Result<(), super::node::LayoutValidateError> {
        // Tree-level invariants — version/name are checked by `io.rs` at load time.
        self.layout.validate()
    }

    /// Return a stable iterator of every `(LayoutNodeId, &LayoutNode)` pair in the tree.
    ///
    /// Order is pre-order (root first, then children left-to-right). Used by the
    /// reconciler to build the "id → node" map for diffing.
    #[must_use]
    pub fn id_index(&self) -> Vec<(&LayoutNodeId, &LayoutNode)> {
        let mut out = Vec::new();
        collect_ids(&self.layout, &mut out);
        out
    }
}

fn collect_ids<'a>(node: &'a LayoutNode, out: &mut Vec<(&'a LayoutNodeId, &'a LayoutNode)>) {
    if let Some(id) = node.id() {
        out.push((id, node));
    }
    match node {
        LayoutNode::HSplit { left, right, .. } => {
            collect_ids(left, out);
            collect_ids(right, out);
        }
        LayoutNode::VSplit { top, bottom, .. } => {
            collect_ids(top, out);
            collect_ids(bottom, out);
        }
        LayoutNode::Stack { .. } | LayoutNode::Toolbar { .. } => {}
    }
}

#[cfg(test)]
mod tests {
    use super::super::node::TabId;
    use super::*;

    fn fixture() -> Workspace {
        Workspace {
            name: "Default".into(),
            version: CURRENT_WORKSPACE_VERSION.into(),
            theme: Some("dark".into()),
            layout: LayoutNode::HSplit {
                ratio: 0.2,
                id: Some(LayoutNodeId::new("root")),
                left: Box::new(LayoutNode::Stack {
                    id: Some(LayoutNodeId::new("scene")),
                    tabs: vec![TabId::new("tab/scene")],
                }),
                right: Box::new(LayoutNode::Stack {
                    id: Some(LayoutNodeId::new("viewport")),
                    tabs: vec![TabId::new("tab/viewport")],
                }),
            },
            main_menu: vec![MainMenuEntry {
                id: "menu.file".into(),
                label: "File".into(),
            }],
            toolbars: vec![],
            shortcuts_overlay: ShortcutsOverlay::default(),
        }
    }

    #[test]
    fn id_index_collects_every_id() {
        let ws = fixture();
        let ids: Vec<_> = ws.id_index().iter().map(|(id, _)| id.0.clone()).collect();
        assert_eq!(ids, vec!["root", "scene", "viewport"]);
    }

    #[test]
    fn validate_passes_on_fixture() {
        fixture().validate().expect("fixture validates");
    }
}
