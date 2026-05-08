//! Diff-based hot-reload reconciler.
//!
//! Adapted from rustforge::apps::editor-app::ir_bridge on 2026-05-05 — generalized
//! for Workspace. The rustforge precursor reloaded a single IR type wholesale on
//! every file change; here we minimize repaint cost by computing a structural diff
//! between the previous and incoming `Workspace`, emitting an `Op` list that the
//! dock (W10) can apply with minimal egui-state mutation.
//!
//! Stable `LayoutNodeId`s on the layout tree are the load-bearing piece: panes whose ids
//! match across reload preserve their scroll/selection/focus state. New ids ⇒ pane
//! created. Missing ids ⇒ pane removed. Same id, different tabs ⇒ tab list updated
//! in place. Per PLAN.md §6.7: `<50ms` from file-save to repaint.
//!
//! ## Operation set
//!
//! * `Op::Preserve { id }` — pane state retained as-is.
//! * `Op::UpdateStack { id, tabs }` — same pane, new tab list.
//! * `Op::ReplaceSubtree { id, .. }` — pane's structural variant changed; subtree rebuilt.
//! * `Op::Insert { id, .. }` — pane added in this reload.
//! * `Op::Remove { id }` — pane removed in this reload.
//! * `Op::ReplaceRoot` — the root layout has no stable id (or the id changed)
//!   and a wholesale repaint is required.
//!
//! See [`Op`] for the variant set and [`diff`] for the algorithm.

use std::collections::HashMap;

use super::node::{LayoutNode, LayoutNodeId, TabId};
use super::workspace::Workspace;

/// One reconciliation operation, emitted by `diff` and consumed by the dock.
#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    /// Pane with `id` is unchanged structurally — preserve all egui state.
    Preserve {
        /// Stable id of the preserved pane.
        id: LayoutNodeId,
    },
    /// `Stack` with `id` has a new tab list — preserve scroll/focus on shared tabs.
    UpdateStack {
        /// Stable id of the pane.
        id: LayoutNodeId,
        /// New tab list.
        tabs: Vec<TabId>,
    },
    /// Pane with `id` exists in both trees but variant or split-ratio changed
    /// enough to require a subtree rebuild. The dock should rebuild from `node`.
    ReplaceSubtree {
        /// Stable id of the pane.
        id: LayoutNodeId,
        /// Replacement subtree.
        node: LayoutNode,
    },
    /// New pane introduced in this reload.
    Insert {
        /// Stable id of the new pane.
        id: LayoutNodeId,
        /// Subtree to insert.
        node: LayoutNode,
    },
    /// Pane that existed in the prior workspace is gone in the new one.
    Remove {
        /// Stable id of the removed pane.
        id: LayoutNodeId,
    },
    /// Root has no stable id (or id changed) — wholesale rebuild.
    ReplaceRoot {
        /// New root layout.
        layout: LayoutNode,
    },
}

/// Compute the diff between two workspace layout trees.
///
/// Algorithm:
///
/// 1. Build `id → &LayoutNode` maps for both trees (`Workspace::id_index`).
/// 2. For each id in the new tree:
///    * if also in old, compare the variants:
///      * same variant + same `tabs` / `extension_point` ⇒ `Preserve`,
///      * `Stack` with different tabs ⇒ `UpdateStack`,
///      * different variant or different toolbar fields ⇒ `ReplaceSubtree`,
///      * splits with different ratios ⇒ `ReplaceSubtree` (cheap; the dock can
///        special-case "ratio only" if it wants to).
///    * else ⇒ `Insert`.
/// 3. For each id in old but not new: `Remove`.
/// 4. If root ids don't match ⇒ a single `ReplaceRoot` (covers the unstable-id
///    fallback path).
#[must_use]
pub fn diff(old: &Workspace, new: &Workspace) -> Vec<Op> {
    let old_root_id = old.layout.id();
    let new_root_id = new.layout.id();

    // Unstable-root fallback: if either side has no root id, or the ids differ,
    // we wholesale-rebuild. This is the safe path when the user reorganizes the
    // top-level layout — it can't preserve any pane state because there's no
    // stable anchor.
    match (old_root_id, new_root_id) {
        (Some(a), Some(b)) if a == b => {}
        _ => {
            return vec![Op::ReplaceRoot {
                layout: new.layout.clone(),
            }]
        }
    }

    let old_index: HashMap<&LayoutNodeId, &LayoutNode> = old.id_index().into_iter().collect();
    let new_index: HashMap<&LayoutNodeId, &LayoutNode> = new.id_index().into_iter().collect();

    let mut ops = Vec::new();

    for (id, new_node) in &new_index {
        if let Some(old_node) = old_index.get(id) {
            ops.push(diff_pane(id, old_node, new_node));
        } else {
            ops.push(Op::Insert {
                id: (*id).clone(),
                node: (*new_node).clone(),
            });
        }
    }

    for id in old_index.keys() {
        if !new_index.contains_key(id) {
            ops.push(Op::Remove { id: (*id).clone() });
        }
    }

    ops
}

fn diff_pane(id: &LayoutNodeId, old: &LayoutNode, new: &LayoutNode) -> Op {
    match (old, new) {
        (LayoutNode::Stack { tabs: a, .. }, LayoutNode::Stack { tabs: b, .. }) => {
            if a == b {
                Op::Preserve { id: id.clone() }
            } else {
                Op::UpdateStack {
                    id: id.clone(),
                    tabs: b.clone(),
                }
            }
        }
        (LayoutNode::HSplit { ratio: ra, .. }, LayoutNode::HSplit { ratio: rb, .. })
        | (LayoutNode::VSplit { ratio: ra, .. }, LayoutNode::VSplit { ratio: rb, .. }) => {
            if (ra - rb).abs() < f32::EPSILON {
                // Children's diffs surface separately via id_index — preserve self.
                Op::Preserve { id: id.clone() }
            } else {
                Op::ReplaceSubtree {
                    id: id.clone(),
                    node: new.clone(),
                }
            }
        }
        (
            LayoutNode::Toolbar {
                position: pa,
                extension_point: ea,
                visible: va,
                ..
            },
            LayoutNode::Toolbar {
                position: pb,
                extension_point: eb,
                visible: vb,
                ..
            },
        ) => {
            if pa == pb && ea == eb && va == vb {
                Op::Preserve { id: id.clone() }
            } else {
                Op::ReplaceSubtree {
                    id: id.clone(),
                    node: new.clone(),
                }
            }
        }
        // Variant changed (e.g. Stack → HSplit) — wholesale subtree rebuild.
        _ => Op::ReplaceSubtree {
            id: id.clone(),
            node: new.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::super::node::{LayoutNodeId, TabId, ToolbarPosition};
    use super::super::workspace::{ShortcutsOverlay, Workspace, CURRENT_WORKSPACE_VERSION};
    use super::*;

    fn ws_with_layout(layout: LayoutNode) -> Workspace {
        Workspace {
            name: "T".into(),
            version: CURRENT_WORKSPACE_VERSION.into(),
            theme: None,
            layout,
            main_menu: vec![],
            toolbars: vec![],
            shortcuts_overlay: ShortcutsOverlay::default(),
        }
    }

    fn root_split(left: LayoutNode, right: LayoutNode) -> LayoutNode {
        LayoutNode::HSplit {
            ratio: 0.3,
            id: Some(LayoutNodeId::new("root")),
            left: Box::new(left),
            right: Box::new(right),
        }
    }

    #[test]
    fn identical_workspaces_emit_only_preserves() {
        let layout = root_split(
            LayoutNode::Stack {
                id: Some(LayoutNodeId::new("a")),
                tabs: vec![TabId::new("tab/scene")],
            },
            LayoutNode::Stack {
                id: Some(LayoutNodeId::new("b")),
                tabs: vec![TabId::new("tab/viewport")],
            },
        );
        let old = ws_with_layout(layout.clone());
        let new = ws_with_layout(layout);
        let ops = diff(&old, &new);
        assert!(ops.iter().all(|op| matches!(op, Op::Preserve { .. })));
        assert_eq!(ops.len(), 3); // root + a + b
    }

    #[test]
    fn changed_tab_list_emits_update_stack() {
        let old = ws_with_layout(root_split(
            LayoutNode::Stack {
                id: Some(LayoutNodeId::new("a")),
                tabs: vec![TabId::new("tab/scene")],
            },
            LayoutNode::Stack {
                id: Some(LayoutNodeId::new("b")),
                tabs: vec![TabId::new("tab/viewport")],
            },
        ));
        let new = ws_with_layout(root_split(
            LayoutNode::Stack {
                id: Some(LayoutNodeId::new("a")),
                tabs: vec![TabId::new("tab/scene"), TabId::new("tab/console")],
            },
            LayoutNode::Stack {
                id: Some(LayoutNodeId::new("b")),
                tabs: vec![TabId::new("tab/viewport")],
            },
        ));
        let ops = diff(&old, &new);
        let updates: Vec<_> = ops
            .iter()
            .filter(|op| matches!(op, Op::UpdateStack { .. }))
            .collect();
        assert_eq!(updates.len(), 1);
        if let Op::UpdateStack { id, tabs } = updates[0] {
            assert_eq!(id.0, "a");
            assert_eq!(tabs.len(), 2);
        }
    }

    #[test]
    fn removed_pane_emits_remove() {
        let old = ws_with_layout(root_split(
            LayoutNode::Stack {
                id: Some(LayoutNodeId::new("a")),
                tabs: vec![TabId::new("tab/scene")],
            },
            LayoutNode::Stack {
                id: Some(LayoutNodeId::new("b")),
                tabs: vec![TabId::new("tab/viewport")],
            },
        ));
        // New layout drops 'b' — the diff should emit Remove for it. We construct
        // an unbalanced new layout where the right side lost its id (so the diff
        // flags the prior id as removed).
        let new = ws_with_layout(LayoutNode::HSplit {
            ratio: 0.3,
            id: Some(LayoutNodeId::new("root")),
            left: Box::new(LayoutNode::Stack {
                id: Some(LayoutNodeId::new("a")),
                tabs: vec![TabId::new("tab/scene")],
            }),
            right: Box::new(LayoutNode::Stack {
                id: Some(LayoutNodeId::new("c")),
                tabs: vec![TabId::new("tab/console")],
            }),
        });
        let ops = diff(&old, &new);
        let removes: Vec<_> = ops
            .iter()
            .filter_map(|op| match op {
                Op::Remove { id } => Some(id.0.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(removes, vec!["b".to_string()]);
        let inserts: Vec<_> = ops
            .iter()
            .filter_map(|op| match op {
                Op::Insert { id, .. } => Some(id.0.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(inserts, vec!["c".to_string()]);
    }

    #[test]
    fn changed_root_id_emits_replace_root() {
        let old = ws_with_layout(LayoutNode::Stack {
            id: Some(LayoutNodeId::new("root_v1")),
            tabs: vec![TabId::new("tab/scene")],
        });
        let new = ws_with_layout(LayoutNode::Stack {
            id: Some(LayoutNodeId::new("root_v2")),
            tabs: vec![TabId::new("tab/scene")],
        });
        let ops = diff(&old, &new);
        assert!(matches!(ops.as_slice(), [Op::ReplaceRoot { .. }]));
    }

    #[test]
    fn toolbar_field_change_emits_replace_subtree() {
        let old = ws_with_layout(LayoutNode::HSplit {
            ratio: 0.3,
            id: Some(LayoutNodeId::new("root")),
            left: Box::new(LayoutNode::Toolbar {
                id: Some(LayoutNodeId::new("toolbar")),
                position: ToolbarPosition::Top,
                extension_point: "tools.transform".into(),
                visible: None,
            }),
            right: Box::new(LayoutNode::Stack {
                id: Some(LayoutNodeId::new("b")),
                tabs: vec![TabId::new("tab/viewport")],
            }),
        });
        let new = ws_with_layout(LayoutNode::HSplit {
            ratio: 0.3,
            id: Some(LayoutNodeId::new("root")),
            left: Box::new(LayoutNode::Toolbar {
                id: Some(LayoutNodeId::new("toolbar")),
                position: ToolbarPosition::Bottom, // CHANGED
                extension_point: "tools.transform".into(),
                visible: None,
            }),
            right: Box::new(LayoutNode::Stack {
                id: Some(LayoutNodeId::new("b")),
                tabs: vec![TabId::new("tab/viewport")],
            }),
        });
        let ops = diff(&old, &new);
        assert!(ops
            .iter()
            .any(|op| matches!(op, Op::ReplaceSubtree { id, .. } if id.0 == "toolbar")));
    }
}
