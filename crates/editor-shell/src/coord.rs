//! Editor coordination state — re-exported from `rge_editor_state`.
//!
//! Per PLAN.md §1.15: `Selection` and `ActiveTool` are **coordination** state,
//! not authoritative content. The authoritative definitions now live in
//! `crates/editor-state`; this module re-exports them and keeps only the
//! [`EditorCoord`] wrapper that the lifecycle and snapshot layers use.

pub use rge_editor_state::{ActiveTool, FaceSelection, FaceSelectionSet, Selection};

/// Editor-side coordination state container. Holds the things that *do not*
/// participate in `WorldSnapshot` and therefore persist across Play/Stop.
///
/// Per PLAN.md §1.15 fixed-five coordination categories, W03 only stages
/// the two the PIE round-trip test exercises (selection + active tool).
/// `hover`, `modal_state`, and `drag_drop` join when their owning waves
/// land (W08+ editor-ui).
#[derive(Debug, Default, Clone)]
pub struct EditorCoord {
    /// Currently selected entities (per-panel selection collapses to a
    /// single shared set in W03; W08+ may shard per panel).
    pub selection: Selection,
    /// Currently active editor tool (gizmo / brush / select).
    pub active_tool: ActiveTool,
    /// Face-level selections (entity + owner-seeded `BRepFaceId`).
    ///
    /// Per editor selection persistence sub-α/β: face-level selection
    /// substrate that callers validate through
    /// `cad-projection::CadProjection::face_resolves_in_projection` via
    /// [`FaceSelectionSet::partition`]. Closure-driven validation; nothing
    /// auto-pruned. See sub-α docstring on
    /// [`rge_editor_state::FaceSelectionSet`] for the partition contract.
    pub face_selection: FaceSelectionSet,
}

impl EditorCoord {
    /// Construct an empty coordination state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}
