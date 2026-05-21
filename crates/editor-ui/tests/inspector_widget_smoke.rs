//! Phase 9 — editor-ui inspector widget smoke tests.
//!
//! Pins the formatter layer (`inspector_lines`) of the inspector widget
//! contract. Tests use only the public `&InspectorSnapshot` consumer
//! signature; no `EditorShell` instance is involved, no live wiring is
//! exercised. The egui-render `ui()` function is not exercised here
//! (testing rendered egui output requires a real `Context`; that's the
//! next-dispatch concern when the dock spawner wires up the widget).

use rge_editor_state::InspectorSnapshot;
use rge_editor_ui::widgets::inspector::{inspector_lines, INSPECTOR_LINE_COUNT};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build an `InspectorSnapshot` with all fields set to non-default values
/// so each row format can be asserted distinctly.
fn populated_snapshot() -> InspectorSnapshot {
    InspectorSnapshot {
        time_scale: 2.5,
        play_state_label: "Playing",
        tick_count: 42,
        has_snapshot: true,
        active_tool_label: "Translate",
        selection_len: 3,
        face_selection_len: 7,
        is_dirty: true,
        undo_stack_len: 5,
        undo_cursor: 3,
    }
}

/// Convenience: find the value paired with a given label in the line list.
/// Panics if the label is not present — caller is asserting on a row that
/// `inspector_lines` is supposed to always emit.
fn value_for(lines: &[(String, String)], label: &str) -> String {
    lines
        .iter()
        .find(|(l, _)| l == label)
        .map(|(_, v)| v.clone())
        .unwrap_or_else(|| panic!("label {label:?} not found in inspector_lines"))
}

// ---------------------------------------------------------------------------
// Line count + label set
// ---------------------------------------------------------------------------

#[test]
fn line_count_matches_constant_for_default_snapshot() {
    let lines = inspector_lines(&InspectorSnapshot::default());
    assert_eq!(
        lines.len(),
        INSPECTOR_LINE_COUNT,
        "inspector_lines must produce exactly INSPECTOR_LINE_COUNT rows; \
         if this fails the formatter dropped or added a field"
    );
    assert_eq!(
        INSPECTOR_LINE_COUNT, 9,
        "INSPECTOR_LINE_COUNT must equal the number of fields surfaced in v0 \
         (9 rows: time scale, play state, tick count, PIE snapshot, active tool, \
         selection, face selection, dirty, undo stack)"
    );
}

#[test]
fn line_count_is_stable_across_snapshot_shapes() {
    // Same row count for any InspectorSnapshot — the formatter never
    // conditionally drops rows. This pins the "fixed-shape" property
    // the UI layout depends on.
    let lines_default = inspector_lines(&InspectorSnapshot::default());
    let lines_populated = inspector_lines(&populated_snapshot());
    assert_eq!(lines_default.len(), lines_populated.len());
    assert_eq!(lines_default.len(), INSPECTOR_LINE_COUNT);
}

#[test]
fn label_set_is_exact_and_ordered() {
    // The labels themselves form a contract — UI dock layouts and future
    // localisation hooks key off them. This test pins the label set and
    // the order so accidental relabeling or row reordering trips fast.
    let lines = inspector_lines(&populated_snapshot());
    let labels: Vec<&str> = lines.iter().map(|(l, _)| l.as_str()).collect();
    assert_eq!(
        labels,
        vec![
            "Time Scale",
            "Play State",
            "Tick Count",
            "PIE Snapshot",
            "Active Tool",
            "Selection",
            "Face Selection",
            "Dirty",
            "Undo Stack",
        ]
    );
}

// ---------------------------------------------------------------------------
// Value formatting per field
// ---------------------------------------------------------------------------

#[test]
fn time_scale_renders_two_decimal_x_suffix() {
    let lines = inspector_lines(&InspectorSnapshot {
        time_scale: 0.5,
        ..InspectorSnapshot::default()
    });
    assert_eq!(value_for(&lines, "Time Scale"), "0.50x");

    let lines = inspector_lines(&InspectorSnapshot {
        time_scale: 4.0,
        ..InspectorSnapshot::default()
    });
    assert_eq!(value_for(&lines, "Time Scale"), "4.00x");

    let lines = inspector_lines(&InspectorSnapshot {
        time_scale: 1.234_567,
        ..InspectorSnapshot::default()
    });
    assert_eq!(
        value_for(&lines, "Time Scale"),
        "1.23x",
        "two-decimal format must round at the formatter; no trailing junk"
    );
}

#[test]
fn play_state_passes_through_label_string() {
    for label in ["Editing", "Playing", "Paused"] {
        let lines = inspector_lines(&InspectorSnapshot {
            play_state_label: label,
            ..InspectorSnapshot::default()
        });
        assert_eq!(value_for(&lines, "Play State"), label);
    }
}

#[test]
fn tick_count_is_plain_decimal() {
    let lines = inspector_lines(&InspectorSnapshot {
        tick_count: 0,
        ..InspectorSnapshot::default()
    });
    assert_eq!(value_for(&lines, "Tick Count"), "0");

    let lines = inspector_lines(&InspectorSnapshot {
        tick_count: 1_234_567,
        ..InspectorSnapshot::default()
    });
    assert_eq!(value_for(&lines, "Tick Count"), "1234567");
}

#[test]
fn pie_snapshot_renders_captured_or_dash() {
    let s_captured = InspectorSnapshot {
        has_snapshot: true,
        ..InspectorSnapshot::default()
    };
    assert_eq!(
        value_for(&inspector_lines(&s_captured), "PIE Snapshot"),
        "captured"
    );

    let s_no = InspectorSnapshot {
        has_snapshot: false,
        ..InspectorSnapshot::default()
    };
    assert_eq!(value_for(&inspector_lines(&s_no), "PIE Snapshot"), "—");
}

#[test]
fn active_tool_passes_through_label_string() {
    for label in ["Select", "Translate", "Rotate", "Scale", "Brush"] {
        let lines = inspector_lines(&InspectorSnapshot {
            active_tool_label: label,
            ..InspectorSnapshot::default()
        });
        assert_eq!(value_for(&lines, "Active Tool"), label);
    }
}

#[test]
fn selection_and_face_selection_render_count_plus_unit() {
    let lines = inspector_lines(&InspectorSnapshot {
        selection_len: 0,
        face_selection_len: 0,
        ..InspectorSnapshot::default()
    });
    assert_eq!(value_for(&lines, "Selection"), "0 entities");
    assert_eq!(value_for(&lines, "Face Selection"), "0 faces");

    let lines = inspector_lines(&InspectorSnapshot {
        selection_len: 1,
        face_selection_len: 42,
        ..InspectorSnapshot::default()
    });
    // Note: v0 does NOT pluralize ("1 entities" not "1 entity") — the
    // formatter is fixed-shape; pluralization is a separate
    // localisation concern.
    assert_eq!(value_for(&lines, "Selection"), "1 entities");
    assert_eq!(value_for(&lines, "Face Selection"), "42 faces");
}

#[test]
fn dirty_flag_renders_modified_or_saved() {
    let s_dirty = InspectorSnapshot {
        is_dirty: true,
        ..InspectorSnapshot::default()
    };
    assert_eq!(value_for(&inspector_lines(&s_dirty), "Dirty"), "modified");

    let s_clean = InspectorSnapshot {
        is_dirty: false,
        ..InspectorSnapshot::default()
    };
    assert_eq!(value_for(&inspector_lines(&s_clean), "Dirty"), "saved");
}

#[test]
fn undo_stack_renders_len_plus_cursor_in_parens() {
    let lines = inspector_lines(&InspectorSnapshot {
        undo_stack_len: 0,
        undo_cursor: 0,
        ..InspectorSnapshot::default()
    });
    assert_eq!(value_for(&lines, "Undo Stack"), "0 (cursor 0)");

    let lines = inspector_lines(&InspectorSnapshot {
        undo_stack_len: 5,
        undo_cursor: 3,
        ..InspectorSnapshot::default()
    });
    assert_eq!(value_for(&lines, "Undo Stack"), "5 (cursor 3)");

    // Edge case: cursor past every entry (redo tail exhausted).
    let lines = inspector_lines(&InspectorSnapshot {
        undo_stack_len: 3,
        undo_cursor: 3,
        ..InspectorSnapshot::default()
    });
    assert_eq!(value_for(&lines, "Undo Stack"), "3 (cursor 3)");
}

// ---------------------------------------------------------------------------
// Determinism + value integrity
// ---------------------------------------------------------------------------

#[test]
fn same_snapshot_produces_byte_identical_lines() {
    // Pure-function determinism: two calls on the same snapshot must
    // produce the exact same Vec.
    let s = populated_snapshot();
    let lines1 = inspector_lines(&s);
    let lines2 = inspector_lines(&s);
    assert_eq!(lines1, lines2);
}

#[test]
fn populated_snapshot_renders_all_non_default_values() {
    // Smoke check: every field of `populated_snapshot()` lands in the
    // corresponding row's value string. Guards against silent
    // field-skipping in the formatter.
    let lines = inspector_lines(&populated_snapshot());
    assert_eq!(value_for(&lines, "Time Scale"), "2.50x");
    assert_eq!(value_for(&lines, "Play State"), "Playing");
    assert_eq!(value_for(&lines, "Tick Count"), "42");
    assert_eq!(value_for(&lines, "PIE Snapshot"), "captured");
    assert_eq!(value_for(&lines, "Active Tool"), "Translate");
    assert_eq!(value_for(&lines, "Selection"), "3 entities");
    assert_eq!(value_for(&lines, "Face Selection"), "7 faces");
    assert_eq!(value_for(&lines, "Dirty"), "modified");
    assert_eq!(value_for(&lines, "Undo Stack"), "5 (cursor 3)");
}

#[test]
fn default_snapshot_renders_zero_state_strings() {
    // The `InspectorSnapshot::default()` produces `play_state_label = ""`
    // and `active_tool_label = ""` (empty `&'static str`), tick=0, etc.
    // The formatter must surface that empty state cleanly without panic
    // — useful for the dock-spawner default path that may hand a
    // zero-state snapshot before the host pumps real values.
    let lines = inspector_lines(&InspectorSnapshot::default());
    assert_eq!(value_for(&lines, "Time Scale"), "0.00x");
    assert_eq!(value_for(&lines, "Play State"), "");
    assert_eq!(value_for(&lines, "Tick Count"), "0");
    assert_eq!(value_for(&lines, "PIE Snapshot"), "—");
    assert_eq!(value_for(&lines, "Active Tool"), "");
    assert_eq!(value_for(&lines, "Selection"), "0 entities");
    assert_eq!(value_for(&lines, "Face Selection"), "0 faces");
    assert_eq!(value_for(&lines, "Dirty"), "saved");
    assert_eq!(value_for(&lines, "Undo Stack"), "0 (cursor 0)");
}
