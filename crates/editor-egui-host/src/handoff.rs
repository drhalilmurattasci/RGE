//! Editor-egui-host handoff aliases — [`InspectorHandoff`] and
//! [`SaveStatusHandoff`], the two latest-only snapshot handoffs the host reads
//! from the editor-shell publisher.
//!
//! Both are **type aliases** over the workspace's shared
//! [`rge_editor_state::Handoff`]:
//!
//! - [`InspectorHandoff`] = `Handoff<InspectorSnapshot>` — carries an
//!   [`rge_editor_state::InspectorSnapshot`] to the host's
//!   [`crate::InspectorTabBody`] consumer.
//! - [`SaveStatusHandoff`] = `Handoff<SaveStatusSnapshot>` — carries a
//!   [`rge_editor_state::SaveStatusSnapshot`] (open scene file name + dirty
//!   flag) to the host's bottom status bar.
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

use rge_editor_state::{Handoff, InspectorSnapshot, SaveStatusSnapshot};

/// Latest-only handoff carrying an [`InspectorSnapshot`] from the editor-shell
/// publisher to the host's [`crate::InspectorTabBody`]. A type alias over the
/// shared [`Handoff`]; see [`rge_editor_state::Handoff`] for the full
/// latest-only contract.
pub type InspectorHandoff = Handoff<InspectorSnapshot>;

/// Latest-only handoff carrying a [`SaveStatusSnapshot`] (open scene file name
/// + dirty flag) from the editor-shell publisher to the host's bottom status
/// bar. A type alias over the shared [`Handoff`]; see
/// [`rge_editor_state::Handoff`] for the full latest-only contract.
pub type SaveStatusHandoff = Handoff<SaveStatusSnapshot>;
