//! `LayoutNode` — recursive tree of pane splits, stacks, and toolbars.
//!
//! Adapted from rustforge::apps::editor-app::ir_bridge on 2026-05-05 — generalized
//! for Workspace. The rustforge precursor stored a single graphics-IR struct per RON
//! file; here we keep the same serde-typed RON pattern but the IR is a recursive
//! `LayoutNode` tree describing a panelled editor layout (UE Slate `FLayoutSaveRestore`
//! analogue). Per PLAN.md §6.5 / ADR-018 — RON only.
//!
//! Each `Stack` holds an ordered list of `TabId`s. `TabId` is a stable string id
//! resolved at runtime by the dock's `SpawnerRegistry` (W10). Toolbars carry an
//! `extension_point` id resolved against the menu registry (W08).
//!
//! Stable ids (the optional `id` field on every variant) are the load-bearing piece
//! for diff-based hot-reload (`reconcile.rs`): two trees with identical ids are
//! reconciled in place, preserving scroll/selection/focus state attached to those
//! panes; mismatched ids cause the corresponding subtree to be rebuilt.

use serde::{Deserialize, Serialize};

/// Stable identifier for a dock tab.
///
/// Local to the W09 layout module — the real `TabId` lives in `editor-ui/dock` (W10).
/// This stub preserves wire format (`"tab/scene"` etc.) so workspace RON files written
/// against W09 stay valid once W10 lands.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TabId(pub String);

impl TabId {
    /// Construct a `TabId` from any string-like value.
    #[must_use]
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Borrow the underlying string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for TabId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

impl From<String> for TabId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

/// Toolbar pinning edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolbarPosition {
    /// Pinned to the top of the parent area.
    Top,
    /// Pinned to the bottom of the parent area.
    Bottom,
    /// Pinned to the left of the parent area.
    Left,
    /// Pinned to the right of the parent area.
    Right,
}

/// Stable, optional id used by the diff-based hot-reload reconciler.
///
/// Two layout trees with matching `LayoutNodeId` at the same path are reconciled in place
/// (preserving scroll/selection/focus on that subtree). Trees without ids fall back
/// to structural matching by variant + path.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LayoutNodeId(pub String);

impl LayoutNodeId {
    /// Construct a `LayoutNodeId`.
    #[must_use]
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Borrow the underlying string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Recursive layout tree.
///
/// A `Workspace`'s root pane is one `LayoutNode`; splits nest other nodes; stacks
/// hold tab ids; toolbars are leaves keyed by an extension point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LayoutNode {
    /// Horizontal split — `left` and `right` children separated by a vertical bar.
    /// `ratio` is the left child's fraction of total width, in `0.0..=1.0`.
    HSplit {
        /// Left child's fraction of total width (clamped to `0.05..=0.95` at validate-time).
        ratio: f32,
        /// Optional stable id (preserves split-bar drag state across hot-reload).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<LayoutNodeId>,
        /// Left child subtree.
        left: Box<LayoutNode>,
        /// Right child subtree.
        right: Box<LayoutNode>,
    },
    /// Vertical split — `top` and `bottom` children separated by a horizontal bar.
    /// `ratio` is the top child's fraction of total height, in `0.0..=1.0`.
    VSplit {
        /// Top child's fraction of total height (clamped to `0.05..=0.95` at validate-time).
        ratio: f32,
        /// Optional stable id.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<LayoutNodeId>,
        /// Top child subtree.
        top: Box<LayoutNode>,
        /// Bottom child subtree.
        bottom: Box<LayoutNode>,
    },
    /// Tabbed stack — an ordered list of tab ids resolved by the dock spawner registry.
    Stack {
        /// Optional stable id (preserves which tab is active across hot-reload).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<LayoutNodeId>,
        /// Tab ids in display order.
        tabs: Vec<TabId>,
    },
    /// Toolbar pinned to one edge of the parent area.
    Toolbar {
        /// Edge to pin against.
        position: ToolbarPosition,
        /// Extension-point id (resolved against the menu registry — W08).
        extension_point: String,
        /// Optional bound state field name; `None` ⇒ always visible.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        visible: Option<String>,
        /// Optional stable id.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<LayoutNodeId>,
    },
}

impl LayoutNode {
    /// Borrow the node's optional stable id.
    #[must_use]
    pub fn id(&self) -> Option<&LayoutNodeId> {
        match self {
            LayoutNode::HSplit { id, .. }
            | LayoutNode::VSplit { id, .. }
            | LayoutNode::Stack { id, .. }
            | LayoutNode::Toolbar { id, .. } => id.as_ref(),
        }
    }

    /// Validate structural invariants:
    ///
    /// * split ratios in `0.05..=0.95` (no zero-width / zero-height children),
    /// * stacks non-empty,
    /// * toolbar `extension_point` non-empty.
    ///
    /// Recurses into children. Returns the first error found.
    ///
    /// # Errors
    ///
    /// Returns `LayoutValidateError::SplitRatioOutOfRange` for split ratios
    /// outside `0.05..=0.95`, `LayoutValidateError::EmptyStack` for `Stack`
    /// nodes with zero tabs, or `LayoutValidateError::EmptyExtensionPoint` for
    /// `Toolbar` nodes whose extension-point id is empty.
    pub fn validate(&self) -> Result<(), LayoutValidateError> {
        match self {
            LayoutNode::HSplit {
                ratio, left, right, ..
            }
            | LayoutNode::VSplit {
                ratio,
                top: left,
                bottom: right,
                ..
            } => {
                if !(0.05..=0.95).contains(ratio) {
                    return Err(LayoutValidateError::SplitRatioOutOfRange(*ratio));
                }
                left.validate()?;
                right.validate()?;
                Ok(())
            }
            LayoutNode::Stack { tabs, .. } => {
                if tabs.is_empty() {
                    return Err(LayoutValidateError::EmptyStack);
                }
                Ok(())
            }
            LayoutNode::Toolbar {
                extension_point, ..
            } => {
                if extension_point.is_empty() {
                    return Err(LayoutValidateError::EmptyExtensionPoint);
                }
                Ok(())
            }
        }
    }
}

/// Errors returned by `LayoutNode::validate`.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum LayoutValidateError {
    /// Split ratio outside the legal `0.05..=0.95` range.
    #[error("split ratio {0} outside 0.05..=0.95")]
    SplitRatioOutOfRange(f32),
    /// `Stack` with zero tabs.
    #[error("stack has zero tabs")]
    EmptyStack,
    /// `Toolbar` with empty extension-point id.
    #[error("toolbar extension_point is empty")]
    EmptyExtensionPoint,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_passes_for_canonical_3pane() {
        let node = LayoutNode::HSplit {
            ratio: 0.2,
            id: Some(LayoutNodeId::new("root")),
            left: Box::new(LayoutNode::Stack {
                id: Some(LayoutNodeId::new("scene")),
                tabs: vec![TabId::new("tab/scene")],
            }),
            right: Box::new(LayoutNode::HSplit {
                ratio: 0.75,
                id: None,
                left: Box::new(LayoutNode::Stack {
                    id: Some(LayoutNodeId::new("viewport")),
                    tabs: vec![TabId::new("tab/viewport")],
                }),
                right: Box::new(LayoutNode::Stack {
                    id: Some(LayoutNodeId::new("inspector")),
                    tabs: vec![TabId::new("tab/inspector")],
                }),
            }),
        };
        node.validate().expect("3-pane layout validates");
    }

    #[test]
    fn validate_rejects_extreme_ratio() {
        let node = LayoutNode::HSplit {
            ratio: 0.0,
            id: None,
            left: Box::new(LayoutNode::Stack {
                id: None,
                tabs: vec![TabId::new("a")],
            }),
            right: Box::new(LayoutNode::Stack {
                id: None,
                tabs: vec![TabId::new("b")],
            }),
        };
        assert!(matches!(
            node.validate(),
            Err(LayoutValidateError::SplitRatioOutOfRange(_))
        ));
    }

    #[test]
    fn validate_rejects_empty_stack() {
        let node = LayoutNode::Stack {
            id: None,
            tabs: vec![],
        };
        assert_eq!(node.validate(), Err(LayoutValidateError::EmptyStack));
    }

    #[test]
    fn tab_id_round_trip_str() {
        let t = TabId::from("tab/scene");
        assert_eq!(t.as_str(), "tab/scene");
    }
}
