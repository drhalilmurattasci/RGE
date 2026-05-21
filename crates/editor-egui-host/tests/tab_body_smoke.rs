//! Phase 9 dispatch C — public-API smoke tests for
//! [`rge_editor_egui_host::TabBody`] / [`InspectorTabBody`] /
//! [`EditorTabViewer`].
//!
//! These tests assert the **public-API shape** of the dock-tab dispatch
//! and the publish/acquire wire between an external handoff and the
//! `InspectorTabBody` that the host stores inside its `DockState`.
//! They do NOT spin up an egui `Ui` or a wgpu device — `TabViewer::ui`
//! is exercised by the host's render path in real frames (and proved
//! end-to-end by the egui+wgpu test infrastructure, which a dispatch-A
//! comment in `host_scaffolding_smoke.rs` explicitly declared
//! out-of-scope at this layer).

use std::sync::Arc;

use egui_dock::TabViewer;
use rge_editor_egui_host::{EditorTabViewer, InspectorHandoff, InspectorTabBody, TabBody};
use rge_editor_state::InspectorSnapshot;

// ---------------------------------------------------------------------------
// Trait bounds
// ---------------------------------------------------------------------------

/// Compile-time assertion: `TabBody` is `Send + 'static` so an
/// `egui_dock::DockState<TabBody>` field on an `EguiHost` inherits the
/// host's `Send + 'static` bound.
#[test]
fn tab_body_is_send_static() {
    fn assert_send_static<T: Send + 'static>() {}
    assert_send_static::<TabBody>();
    assert_send_static::<InspectorTabBody>();
}

// ---------------------------------------------------------------------------
// Title dispatch
// ---------------------------------------------------------------------------

#[test]
fn inspector_variant_title_is_inspector() {
    let handoff = Arc::new(InspectorHandoff::new());
    let body = TabBody::Inspector(InspectorTabBody::new(handoff));
    assert_eq!(body.title(), "Inspector");
}

#[test]
fn placeholder_variant_carries_provided_title() {
    let body = TabBody::Placeholder {
        title: "Scratch Tab".to_string(),
    };
    assert_eq!(body.title(), "Scratch Tab");
}

#[test]
fn editor_tab_viewer_title_matches_tab_body_title() {
    // The TabViewer::title impl delegates to TabBody::title; this
    // pins that contract end-to-end.
    let handoff = Arc::new(InspectorHandoff::new());
    let mut inspector = TabBody::Inspector(InspectorTabBody::new(handoff));
    let mut placeholder = TabBody::Placeholder {
        title: "Hello".to_string(),
    };

    let mut viewer = EditorTabViewer;
    assert_eq!(viewer.title(&mut inspector).text(), "Inspector");
    assert_eq!(viewer.title(&mut placeholder).text(), "Hello");
}

// ---------------------------------------------------------------------------
// Closeable behavior
// ---------------------------------------------------------------------------

#[test]
fn tabs_are_not_closeable_in_v0() {
    // Dispatch C invariant: no menu-driven tab respawn yet, so all
    // tabs must remain non-closeable so the user cannot end up in a
    // "no inspector visible" state.
    let handoff = Arc::new(InspectorHandoff::new());
    let inspector = TabBody::Inspector(InspectorTabBody::new(handoff));
    let placeholder = TabBody::Placeholder {
        title: "Foo".to_string(),
    };

    let viewer = EditorTabViewer;
    assert!(!viewer.is_closeable(&inspector));
    assert!(!viewer.is_closeable(&placeholder));
}

// ---------------------------------------------------------------------------
// Handoff wire through the tab body
// ---------------------------------------------------------------------------

#[test]
fn inspector_tab_body_handoff_is_same_arc_as_constructor_input() {
    // Pointer equality on the Arc: the body must NOT clone-deep into
    // a new InspectorHandoff at construction (that would silently
    // fork the publish/acquire wire and the host's tab would never
    // observe editor-shell's publishes).
    let handoff = Arc::new(InspectorHandoff::new());
    let body = InspectorTabBody::new(Arc::clone(&handoff));
    assert!(
        Arc::ptr_eq(body.handoff(), &handoff),
        "InspectorTabBody::handoff() must return the same Arc passed to new()"
    );
}

#[test]
fn external_publish_visible_through_tab_body_handoff() {
    // End-to-end: simulate the editor-shell publisher publishing
    // through an outer Arc clone, then verify the tab body's inner
    // Arc clone observes the same data.
    let handoff = Arc::new(InspectorHandoff::new());
    let body = InspectorTabBody::new(Arc::clone(&handoff));

    let mut snap = InspectorSnapshot::default();
    snap.tick_count = 4242;
    snap.active_tool_label = "Brush";
    snap.is_dirty = true;
    handoff.publish(Arc::new(snap));

    let observed = body
        .handoff()
        .acquire()
        .expect("body's handoff must observe the external publish");
    assert_eq!(observed.tick_count, 4242);
    assert_eq!(observed.active_tool_label, "Brush");
    assert!(observed.is_dirty);
}

#[test]
fn tab_body_handoff_observes_generation_advance() {
    // The shared-Arc semantics mean tab body and external publisher
    // see the same generation counter.
    let handoff = Arc::new(InspectorHandoff::new());
    let body = InspectorTabBody::new(Arc::clone(&handoff));

    assert_eq!(body.handoff().generation(), 0);
    handoff.publish(Arc::new(InspectorSnapshot::default()));
    assert_eq!(body.handoff().generation(), 1);
    handoff.publish(Arc::new(InspectorSnapshot::default()));
    assert_eq!(body.handoff().generation(), 2);
}

// ---------------------------------------------------------------------------
// Empty-state semantics
// ---------------------------------------------------------------------------

#[test]
fn tab_body_acquire_returns_none_before_first_publish() {
    // Without this guarantee, the host's `EditorTabViewer::ui` would
    // panic on a fresh handoff. The empty-state path (rendering
    // `InspectorSnapshot::default()`) is exactly what protects the
    // first frame from looking broken.
    let handoff = Arc::new(InspectorHandoff::new());
    let body = InspectorTabBody::new(handoff);
    assert!(body.handoff().acquire().is_none());
}

// ---------------------------------------------------------------------------
// Variant construction shape (compile-time + simple value checks)
// ---------------------------------------------------------------------------

#[test]
fn placeholder_can_be_constructed_with_owned_string() {
    // Pins the field name + the public surface contract: a
    // String-typed `title` field accessible via pattern matching.
    let body = TabBody::Placeholder {
        title: String::from("X"),
    };
    match &body {
        TabBody::Placeholder { title } => assert_eq!(title, "X"),
        TabBody::Inspector(_) | TabBody::Viewport => panic!("expected Placeholder variant"),
    }
}

#[test]
fn inspector_can_be_constructed_via_inspector_tab_body() {
    let handoff = Arc::new(InspectorHandoff::new());
    let body = TabBody::Inspector(InspectorTabBody::new(handoff));
    match &body {
        TabBody::Inspector(_inspector) => {}
        TabBody::Placeholder { .. } | TabBody::Viewport => panic!("expected Inspector variant"),
    }
}

#[test]
fn viewport_can_be_constructed_as_unit_variant() {
    // Pins TabBody::Viewport as a unit variant — no associated data,
    // constructible without any handoff or title argument.
    let body = TabBody::Viewport;
    match &body {
        TabBody::Viewport => {}
        TabBody::Inspector(_) | TabBody::Placeholder { .. } => {
            panic!("expected Viewport variant")
        }
    }
}
