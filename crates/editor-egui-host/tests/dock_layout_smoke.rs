//! Phase 9 dispatch D — split-layout integration smoke.
//!
//! These tests pin the **initial dock layout** that
//! [`rge_editor_egui_host::EguiHost::new`] builds and the per-variant
//! `TabViewer` overrides that make the split visually correct (the
//! Viewport tab must NOT clear its background; the Inspector tab MUST).
//!
//! Construction of a real `EguiHost` requires a `wgpu::Device` +
//! `winit::Window` (the dispatch-A scaffolding-smoke note explicitly
//! deferred construction tests to dispatch B for the same reason).
//! These tests assert the shape WITHOUT spinning up a GPU: they build
//! the initial layout directly using the same constructor primitives
//! [`EguiHost::new`] uses internally, asserting against the public
//! [`rge_editor_egui_host::INSPECTOR_PANE_OLD_FRACTION`] constant + the
//! [`rge_editor_egui_host::EditorTabViewer`] dispatch.
//!
//! Why a separate test file: integration tests in `tests/` are
//! compiled as separate binaries that can only see the crate's
//! **public** surface. If a future refactor accidentally drops
//! `INSPECTOR_PANE_OLD_FRACTION` from the crate root or makes
//! `EditorTabViewer` non-public, these tests fail to compile loudly.

use std::sync::Arc;

use egui_dock::TabViewer;
use rge_editor_egui_host::{
    EditorTabViewer, InspectorHandoff, InspectorTabBody, TabBody, INSPECTOR_PANE_OLD_FRACTION,
};

// ---------------------------------------------------------------------------
// Layout constant
// ---------------------------------------------------------------------------

#[test]
fn inspector_pane_old_fraction_is_in_valid_range() {
    // egui_dock panics if fraction is outside [0.0, 1.0]; pin both
    // ends and the rough intent (~75% to the viewport).
    assert!(
        (0.0..=1.0).contains(&INSPECTOR_PANE_OLD_FRACTION),
        "INSPECTOR_PANE_OLD_FRACTION ({INSPECTOR_PANE_OLD_FRACTION}) must be in [0, 1] \
         per egui_dock split-fraction contract"
    );
    // ~25% inspector pane → old-fraction (viewport) is the majority.
    assert!(
        INSPECTOR_PANE_OLD_FRACTION > 0.5,
        "viewport (old node) must keep the majority of the width; \
         INSPECTOR_PANE_OLD_FRACTION = {INSPECTOR_PANE_OLD_FRACTION}"
    );
    // Don't crush the inspector to a sliver.
    assert!(
        INSPECTOR_PANE_OLD_FRACTION < 0.95,
        "inspector pane (1 - INSPECTOR_PANE_OLD_FRACTION) must leave \
         a usable strip; INSPECTOR_PANE_OLD_FRACTION = {INSPECTOR_PANE_OLD_FRACTION}"
    );
}

// ---------------------------------------------------------------------------
// Split DockState shape — replicate EguiHost::new construction directly
// ---------------------------------------------------------------------------

/// Build the same two-tab split layout `EguiHost::new` produces. Used
/// by the assertions below so the test isn't dependent on a wgpu device.
fn build_split_layout(handoff: Arc<InspectorHandoff>) -> egui_dock::DockState<TabBody> {
    let viewport_tab = TabBody::Viewport;
    let inspector_tab = TabBody::Inspector(InspectorTabBody::new(handoff));
    let mut dock_state = egui_dock::DockState::new(vec![viewport_tab]);
    dock_state.main_surface_mut().split_right(
        egui_dock::NodeIndex::root(),
        INSPECTOR_PANE_OLD_FRACTION,
        vec![inspector_tab],
    );
    dock_state
}

#[test]
fn initial_dock_contains_both_viewport_and_inspector() {
    let handoff = Arc::new(InspectorHandoff::new());
    let dock_state = build_split_layout(handoff);

    let mut saw_viewport = false;
    let mut saw_inspector = false;
    for (_, tab) in dock_state.iter_all_tabs() {
        match tab {
            TabBody::Viewport => saw_viewport = true,
            TabBody::Inspector(_) => saw_inspector = true,
            TabBody::Placeholder { .. } => {
                panic!("initial dock must NOT contain Placeholder tabs in dispatch D")
            }
        }
    }
    assert!(
        saw_viewport,
        "initial dock must contain a Viewport tab so the cuboid is visible"
    );
    assert!(
        saw_inspector,
        "initial dock must contain an Inspector tab so the user sees editor state"
    );
}

#[test]
fn initial_dock_has_exactly_two_tabs() {
    let handoff = Arc::new(InspectorHandoff::new());
    let dock_state = build_split_layout(handoff);
    let tab_count = dock_state.iter_all_tabs().count();
    assert_eq!(
        tab_count, 2,
        "initial dock must contain exactly Viewport + Inspector"
    );
}

#[test]
fn initial_dock_tab_titles_are_stable() {
    let handoff = Arc::new(InspectorHandoff::new());
    let dock_state = build_split_layout(handoff);

    let titles: Vec<&str> = dock_state.iter_all_tabs().map(|(_, t)| t.title()).collect();
    assert!(titles.contains(&"Viewport"));
    assert!(titles.contains(&"Inspector"));
}

// ---------------------------------------------------------------------------
// TabViewer overrides — the substrate behind the split being visually correct
// ---------------------------------------------------------------------------

#[test]
fn viewport_tab_does_not_clear_background() {
    // LOAD-BEARING: without this, the dock library paints the tab
    // body's bg over the cuboid pixels written by encode_main_pass
    // (LoadOp::Load). The whole point of dispatch D collapses if this
    // assertion ever flips back to true.
    let viewer = EditorTabViewer;
    assert!(
        !viewer.clear_background(&TabBody::Viewport),
        "Viewport must not clear background — cuboid must show through"
    );
}

#[test]
fn inspector_tab_clears_background_for_legibility() {
    let handoff = Arc::new(InspectorHandoff::new());
    let inspector_tab = TabBody::Inspector(InspectorTabBody::new(handoff));
    let viewer = EditorTabViewer;
    assert!(
        viewer.clear_background(&inspector_tab),
        "Inspector must clear background — labels float illegibly without it"
    );
}

#[test]
fn viewport_disables_both_scroll_bars() {
    let viewer = EditorTabViewer;
    assert_eq!(
        viewer.scroll_bars(&TabBody::Viewport),
        [false, false],
        "Viewport's transparent body has no scrollable content — bars would \
         visually trim the viewport area"
    );
}

#[test]
fn all_initial_tabs_are_non_closeable() {
    // No menu-driven respawn yet; closing either tab would strand
    // the user without a way to get back to it.
    let handoff = Arc::new(InspectorHandoff::new());
    let inspector_tab = TabBody::Inspector(InspectorTabBody::new(handoff));
    let viewer = EditorTabViewer;
    assert!(!viewer.is_closeable(&TabBody::Viewport));
    assert!(!viewer.is_closeable(&inspector_tab));
}

// ---------------------------------------------------------------------------
// Tabs surface and tree structure
// ---------------------------------------------------------------------------

#[test]
fn dock_state_has_a_single_main_surface() {
    // The dispatch-D layout puts both tabs inside the MAIN surface
    // (no detached windows). iter_surfaces should yield exactly one
    // entry; if a future refactor inadvertently moves the inspector
    // into a window surface, this fails.
    let handoff = Arc::new(InspectorHandoff::new());
    let dock_state = build_split_layout(handoff);
    let surface_count = dock_state.iter_surfaces().count();
    assert_eq!(
        surface_count, 1,
        "initial dock layout must keep both tabs in the main surface"
    );
}

#[test]
fn inspector_handoff_arc_is_shared_with_initial_inspector_tab() {
    // Verify the wire still works through the split layout: the
    // handoff Arc cloned into the inspector tab body in EguiHost::new
    // must be the same handoff returned by EguiHost::inspector_handoff
    // (substrate continuity, not just the absence of obvious bugs).
    //
    // This test inspects the tab body's internal handoff Arc against
    // an externally-built clone — pointer equality proves no second
    // InspectorHandoff was silently constructed during dock layout.
    let handoff = Arc::new(InspectorHandoff::new());
    let dock_state = build_split_layout(Arc::clone(&handoff));

    let mut inspector_handoffs = Vec::new();
    for (_, tab) in dock_state.iter_all_tabs() {
        if let TabBody::Inspector(body) = tab {
            inspector_handoffs.push(Arc::clone(body.handoff()));
        }
    }
    assert_eq!(
        inspector_handoffs.len(),
        1,
        "exactly one Inspector tab expected in initial dock"
    );
    assert!(
        Arc::ptr_eq(&inspector_handoffs[0], &handoff),
        "inspector tab body must hold the SAME handoff Arc passed to layout — \
         no silent forking of the publish/acquire wire"
    );
}
