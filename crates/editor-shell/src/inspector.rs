//! Phase 9 — headless inspector snapshot (re-export from `editor-state`).
//!
//! The `InspectorSnapshot` struct definition moved to
//! `rge_editor_state::inspector_snapshot` so both `editor-shell` (the
//! producer, via `EditorShell::inspector_snapshot()`) and `editor-ui`
//! (the future consumer, via the `widgets::inspector` render fn) can
//! reference the same type without forcing either crate to depend on
//! the other. Both crates already depend on `editor-state`; the move
//! adds zero new Cargo edges and keeps the editor-shell ↔ editor-ui
//! hosting direction open (a future dispatch can add either direction
//! without a cycle).
//!
//! The re-export below preserves the historical
//! `editor-shell::InspectorSnapshot` public-API path so existing
//! consumers — including the 11 tests in
//! `crates/editor-shell/tests/inspector_snapshot_smoke.rs` — see
//! byte-identical API surface. The producer-side accessor
//! [`crate::EditorShell::inspector_snapshot`] is unchanged.
//!
//! See [`rge_editor_state::inspector_snapshot`] for the full doctrine
//! note (why this is not a 6th coordination category and why no
//! reflection adoption was needed).

pub use rge_editor_state::InspectorSnapshot;
