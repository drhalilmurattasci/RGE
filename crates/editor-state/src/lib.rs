//! `rge-editor-state` — editor coordination state.
//!
//! Failure class: recoverable
//!
//! Editor coordination state per PLAN §1.15. Five categories fixed at v0.8;
//! this revision implements 3 (Selection / Hover / `ActiveTool`); modal-state
//! and drag-drop remain stubs until demonstrated demand (per IMPLEMENTATION.md
//! Phase 5.2 + §0.6 freeze policy).
//!
//! Coordination state, NOT authoritative content. Stores `EntityId` references
//! and primitive coordination context — never component bodies, cad-core nodes,
//! or asset payloads. The `editor-state-ownership` architecture lint enforces.
//!
//! Per [PLAN.md §1.15](../../plans/PLAN.md). **Coordination state, not authoritative content.**
//! Authority lives in `kernel/ecs`, `cad-core`, Command Bus + audit-ledger.
//! editor-state only coordinates interaction context across editor panels.
//!
//! Five categories — fixed at v0.8 per architecture freeze (§0.6).
//! Adding a 6th requires ADR + freeze-policy gate.
//!
//! The crate also hosts a few **dep-neutral shared types** that are explicitly
//! NOT coordination categories — the read-only observation aggregators
//! ([`InspectorSnapshot`] / [`SaveStatusSnapshot`]) and the shared latest-only
//! [`Handoff`] primitive. They live here because both `editor-shell` and
//! `editor-egui-host` depend on `editor-state` and neither may depend on the
//! other; housing the shared type here avoids a dependency cycle. They do not
//! count against the §0.6 five-category freeze.

pub mod active_tool;
pub mod drag_drop;
pub mod face_selection;
pub mod handoff;
pub mod hover;
pub mod inspector_snapshot;
pub mod modal_state;
pub mod save_status_snapshot;
pub mod selection;

pub use active_tool::ActiveTool;
pub use face_selection::{FaceSelection, FaceSelectionSet};
// Shared latest-only snapshot handoff primitive (dep-neutral infrastructure,
// not a 6th coordination category; see the module-level doc comment). The
// editor-shell `RenderHandoff` and editor-egui-host `InspectorHandoff` /
// `SaveStatusHandoff` are type aliases over this.
pub use handoff::Handoff;
pub use hover::{Hover, PanelId};
// Phase 9 — read-only observation aggregator (not a 6th coordination
// category; see the module-level doc comment for the doctrine note).
pub use inspector_snapshot::InspectorSnapshot;
// Read-only save-state observation aggregator for the editor's bottom status
// bar (not a 6th coordination category; see the module-level doctrine note).
pub use save_status_snapshot::SaveStatusSnapshot;
pub use selection::Selection;
