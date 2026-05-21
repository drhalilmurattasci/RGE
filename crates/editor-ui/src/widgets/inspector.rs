//! Phase 9 — editor inspector widget over the shared
//! [`rge_editor_state::InspectorSnapshot`] observation aggregator.
//!
//! The widget ships in two layers:
//!
//! - [`inspector_lines`] — a pure formatter that turns a snapshot into a
//!   `Vec<(label, value)>` of display strings. Testable without an egui
//!   `Context`; pinned by the unit tests in
//!   `crates/editor-ui/tests/inspector_widget_smoke.rs`.
//! - [`ui`] — the egui render function. Walks `inspector_lines` and
//!   renders each pair as a labeled row. Pure function, no widget state.
//!
//! # No live-wiring in this module
//!
//! This dispatch deliberately does NOT wire the widget into the dock
//! spawner registry; `"tab/inspector"` continues to spawn
//! `PlaceholderTabBody`. The live-snapshot-delivery mechanism (per-frame
//! snapshot_fn closure vs. `RwLock` shared slot vs. handoff substrate)
//! is the next dispatch's decision. Until that decision lands, this
//! widget is reachable only via direct construction from tests or
//! future host code, taking a `&InspectorSnapshot` that the caller
//! produces however it pleases.
//!
//! # No reflection
//!
//! Per the §1.1 reflection-scale preflight (`plans/BASELINE.md`): zero
//! production reflected types today. The widget renders 10 plain `Copy`
//! fields directly. When/if a real reflected component model lands and
//! a future generic property editor is needed, *that* widget can use
//! reflection; this fixed-shape inspector does not.

use rge_editor_state::InspectorSnapshot;

/// Number of rows produced by [`inspector_lines`]. Pinned as a constant so
/// the count-stability test can fail loudly if a field is accidentally
/// dropped from the formatter.
pub const INSPECTOR_LINE_COUNT: usize = 9;

/// Build a deterministic `Vec<(label, value)>` of display strings from the
/// snapshot. Pure function; the same snapshot always produces the same
/// rows in the same order.
///
/// Row order is fixed at v0:
///
/// 1. Time Scale
/// 2. Play State
/// 3. Tick Count
/// 4. PIE Snapshot
/// 5. Active Tool
/// 6. Selection
/// 7. Face Selection
/// 8. Dirty
/// 9. Undo Stack
///
/// Each row's `value` is a short string (no leading/trailing whitespace,
/// no embedded newlines) suitable for a single egui label.
#[must_use]
pub fn inspector_lines(snapshot: &InspectorSnapshot) -> Vec<(String, String)> {
    vec![
        (
            "Time Scale".to_string(),
            format!("{:.2}x", snapshot.time_scale),
        ),
        (
            "Play State".to_string(),
            snapshot.play_state_label.to_string(),
        ),
        ("Tick Count".to_string(), snapshot.tick_count.to_string()),
        (
            "PIE Snapshot".to_string(),
            if snapshot.has_snapshot {
                "captured".to_string()
            } else {
                "—".to_string()
            },
        ),
        (
            "Active Tool".to_string(),
            snapshot.active_tool_label.to_string(),
        ),
        (
            "Selection".to_string(),
            format!("{} entities", snapshot.selection_len),
        ),
        (
            "Face Selection".to_string(),
            format!("{} faces", snapshot.face_selection_len),
        ),
        (
            "Dirty".to_string(),
            if snapshot.is_dirty {
                "modified".to_string()
            } else {
                "saved".to_string()
            },
        ),
        (
            "Undo Stack".to_string(),
            format!(
                "{} (cursor {})",
                snapshot.undo_stack_len, snapshot.undo_cursor
            ),
        ),
    ]
}

/// Render the inspector into an egui scope. Walks [`inspector_lines`] and
/// renders each (label, value) pair as a `ui.label(label); ui.label(value);`
/// row inside a `ui.horizontal(…)` group. Pure function, no widget state.
///
/// Calling this multiple times per frame is safe but wasteful — the
/// `inspector_lines` allocation runs each call. Consumers that care
/// about per-frame allocation can cache the lines vector themselves.
pub fn ui(snapshot: &InspectorSnapshot, ui: &mut egui::Ui) {
    for (label, value) in inspector_lines(snapshot) {
        ui.horizontal(|ui| {
            ui.label(label);
            ui.label(value);
        });
    }
}
