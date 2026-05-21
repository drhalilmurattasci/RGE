//! Phase 9 — headless inspector snapshot.
//!
//! Read-only `Copy` view of editor-session state surfaced to a future
//! inspector widget (which has no implementation today; see
//! `plans/BASELINE.md` editor-usability + reflection preflights). The
//! dispatch shipping this module ships the **model only** — there is
//! no rendered widget, no dock-spawner wiring, no `editor-ui` dep.
//!
//! # Why plain `Copy` fields and no reflection
//!
//! Per the §1.1 reflection-scale preflight (`plans/BASELINE.md`):
//! **0 production reflected types** in the workspace today. Forcing
//! reflection adoption to enable the inspector would (a) ship
//! adoption without a real consumer-driven shape, (b) conflate the
//! inspector dispatch with the reflection-adoption decision, and
//! (c) inflate the per-type LLVM cost projection that's currently
//! extrapolated linearly in `kernel/types/BUDGET.md` from 5 → 100
//! types. Hand-rolled fields here render via
//! `format!("{key}: {value}")`-style labels in the future widget
//! dispatch — no proc-macro overhead, no global registry, no need
//! to touch the kernel reflection substrate.
//!
//! The model is intentionally a flat bag of leaves — no `Vec`, no
//! nested `Option`, no aggregates that depend on missing
//! infrastructure (e.g. audit-ledger tail accessors). Future state
//! additions extend the struct additively.
//!
//! # Architectural invariants
//!
//! - **Single source per field.** Every field is sourced from exactly
//!   one `EditorShell` accessor and reflects the current observable
//!   state at the moment `inspector_snapshot()` is called. No staleness;
//!   no caching.
//! - **No interior mutability.** The struct is `Copy`; the producer
//!   reads each value once and lays out a fresh struct each call. There
//!   is no `Arc<RwLock<…>>` or shared handle.
//! - **No side effects on construction.** Building the snapshot is a
//!   pure read; no audit-ledger events, no bus submits, no resource
//!   inserts.

/// Plain-data view of editor-session state for the headless inspector
/// model. All ten fields are `Copy` leaves derived from already-public
/// `EditorShell` accessors; building a snapshot is a pure read with
/// no side effects.
///
/// # Field stability
///
/// - `time_scale`: read from the `TimeScale` ECS resource via
///   [`crate::EditorShell::time_scale`]. Always within
///   `[TimeScale::MIN, TimeScale::MAX]` because the bus-routed
///   `set_time_scale` clamps on submit; a snapshot from a fresh
///   `EditorShell::new()` reads `TimeScale::DEFAULT` (1.0).
/// - `play_state_label`: const `&'static str` from
///   [`crate::PlayState::label`]; one of `"Editing"` / `"Playing"` /
///   `"Paused"`.
/// - `tick_count`: monotonic `u64`; advances only when
///   `PlayState::game_systems_run()` returns `true`.
/// - `has_snapshot`: `true` while a PIE `WorldSnapshot` is captured
///   (between Play and Stop); `false` in pure Editing.
/// - `active_tool_label`: const `&'static str` from
///   [`rge_editor_state::ActiveTool::label`].
/// - `selection_len` / `face_selection_len`: `BTreeSet` cardinalities;
///   always `>= 0`. Empty on fresh construction.
/// - `is_dirty`: mirror of [`rge_editor_actions::CommandBus::is_dirty`];
///   flips on any non-no-op bus submit.
/// - `undo_stack_len`: total number of stack entries
///   ([`rge_editor_actions::UndoStack::len`]). Includes entries past the
///   cursor (the "redo tail").
/// - `undo_cursor`: cursor position
///   ([`rge_editor_actions::UndoStack::cursor`]); `<= undo_stack_len`.
///
/// # Trait bounds
///
/// Marked `Copy + Clone + Debug + PartialEq + Default` so consumers can
/// store snapshots, diff successive snapshots, and round-trip through
/// `#[derive(Debug)]` formatting. `Send + Sync` are auto-derived; the
/// `inspector_snapshot_is_copy_send_sync` smoke test pins those
/// statically.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct InspectorSnapshot {
    /// Current value of the `TimeScale` ECS resource (slider value).
    pub time_scale: f32,
    /// `PlayState::label()` — `"Editing"` / `"Playing"` / `"Paused"`.
    pub play_state_label: &'static str,
    /// Game-system tick counter (advances only while `PlayState::game_systems_run()`).
    pub tick_count: u64,
    /// `true` while a PIE `WorldSnapshot` is captured.
    pub has_snapshot: bool,
    /// `ActiveTool::label()` — `"Select"` / `"Translate"` / `"Rotate"` /
    /// `"Scale"` / `"Brush"`.
    pub active_tool_label: &'static str,
    /// Number of entities currently in `EditorCoord::selection`.
    pub selection_len: usize,
    /// Number of faces currently in `EditorCoord::face_selection`.
    pub face_selection_len: usize,
    /// `CommandBus::is_dirty()` — `true` when the bus cursor is past the
    /// last `mark_saved`.
    pub is_dirty: bool,
    /// `CommandBus::stack().len()` — total stack entries (cursor may sit
    /// anywhere in `[0, undo_stack_len]`).
    pub undo_stack_len: usize,
    /// `CommandBus::stack().cursor()` — current cursor position.
    pub undo_cursor: u64,
}
