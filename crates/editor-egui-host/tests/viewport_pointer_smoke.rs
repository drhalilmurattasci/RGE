//! Phase 9 dispatch F — viewport pointer-detection smoke.
//!
//! These tests pin the host's
//! [`rge_editor_egui_host::EguiHost::is_pointer_over_viewport`]
//! contract WITHOUT spinning up a wgpu device. They do so by going
//! through the [`rge_editor_egui_host::EditorTabViewer`] +
//! [`rge_editor_egui_host::ViewportRectSink`] substrate directly: the
//! viewer's `ui()` method writes a rect into the sink (in production
//! this happens inside `EguiHost::render` while drawing the Viewport
//! tab body); the host's accessor reads from the same sink with the
//! physical → logical DPI conversion applied.
//!
//! Constructing a real `EguiHost` requires a `wgpu::Device` +
//! `winit::Window` (dispatch-A scaffolding doc), so the rect-sink
//! seam is what's reachable in headless tests. The sink-level tests
//! here cover the **same** decision logic the host wraps in
//! `is_pointer_over_viewport`; an end-to-end host construction test
//! is left for a future dispatch that builds a headless wgpu device
//! fixture.

use std::sync::{Arc, Mutex};

use rge_editor_egui_host::{ViewportRectSink, INSPECTOR_PANE_OLD_FRACTION};

// ---------------------------------------------------------------------------
// ViewportRectSink shape
// ---------------------------------------------------------------------------

#[test]
fn viewport_rect_sink_is_mutex_of_option_rect() {
    // Construct an empty sink and verify the (Mutex<Option<Rect>>)
    // shape behaves the way the host expects: empty → None;
    // write → Some(rect); read → cloned rect; reset → None.
    let sink: Arc<ViewportRectSink> = Arc::new(Mutex::new(None));

    assert!(sink.lock().unwrap().is_none(), "fresh sink must be empty");

    // Write
    let rect = egui::Rect::from_min_max(egui::pos2(10.0, 20.0), egui::pos2(110.0, 80.0));
    *sink.lock().unwrap() = Some(rect);

    // Read
    let observed = sink.lock().unwrap().expect("sink populated");
    assert_eq!(observed.min, egui::pos2(10.0, 20.0));
    assert_eq!(observed.max, egui::pos2(110.0, 80.0));

    // Reset
    *sink.lock().unwrap() = None;
    assert!(sink.lock().unwrap().is_none(), "sink must reset cleanly");
}

#[test]
fn viewport_rect_sink_is_send_sync() {
    fn assert_send_sync<T: Send + Sync + 'static>() {}
    assert_send_sync::<ViewportRectSink>();
    assert_send_sync::<Arc<ViewportRectSink>>();
}

// ---------------------------------------------------------------------------
// Pure decision: pointer-inside / pointer-outside semantics
//
// These mirror what `EguiHost::is_pointer_over_viewport` does
// internally with the physical → logical conversion.
// ---------------------------------------------------------------------------

/// Sink-driven version of the host's `is_pointer_over_viewport`.
/// Replicates the host's logic for headless testing.
fn sink_is_pointer_over_viewport(
    sink: &ViewportRectSink,
    physical_pos: [f32; 2],
    pixels_per_point: f32,
) -> bool {
    let Ok(guard) = sink.lock() else { return false };
    let Some(rect) = *guard else { return false };
    if pixels_per_point <= 0.0 || !pixels_per_point.is_finite() {
        return false;
    }
    let logical = egui::pos2(
        physical_pos[0] / pixels_per_point,
        physical_pos[1] / pixels_per_point,
    );
    rect.contains(logical)
}

#[test]
fn empty_sink_reports_pointer_not_over_viewport() {
    let sink: ViewportRectSink = Mutex::new(None);
    assert!(!sink_is_pointer_over_viewport(&sink, [100.0, 100.0], 1.0));
}

#[test]
fn pointer_inside_logical_rect_with_ppp_1_is_inside() {
    let sink: ViewportRectSink = Mutex::new(Some(egui::Rect::from_min_max(
        egui::pos2(0.0, 0.0),
        egui::pos2(800.0, 600.0),
    )));
    // ppp = 1.0: physical == logical.
    assert!(sink_is_pointer_over_viewport(&sink, [400.0, 300.0], 1.0));
    assert!(sink_is_pointer_over_viewport(&sink, [0.0, 0.0], 1.0));
    assert!(sink_is_pointer_over_viewport(&sink, [800.0, 600.0], 1.0));
}

#[test]
fn pointer_outside_logical_rect_with_ppp_1_is_outside() {
    let sink: ViewportRectSink = Mutex::new(Some(egui::Rect::from_min_max(
        egui::pos2(0.0, 0.0),
        egui::pos2(800.0, 600.0),
    )));
    assert!(!sink_is_pointer_over_viewport(&sink, [801.0, 300.0], 1.0));
    assert!(!sink_is_pointer_over_viewport(&sink, [400.0, 601.0], 1.0));
    assert!(!sink_is_pointer_over_viewport(&sink, [-1.0, -1.0], 1.0));
}

#[test]
fn pointer_inside_at_dpi_scale_15_converts_physical_to_logical() {
    // The Viewport tab rect is in egui logical points: 0..600 wide,
    // 0..400 tall. At pixels_per_point=1.5 (150% scaling), physical
    // coordinates run 0..900 wide, 0..600 tall.
    let sink: ViewportRectSink = Mutex::new(Some(egui::Rect::from_min_max(
        egui::pos2(0.0, 0.0),
        egui::pos2(600.0, 400.0),
    )));

    // Physical [450, 300] / 1.5 = logical [300, 200] — inside.
    assert!(sink_is_pointer_over_viewport(&sink, [450.0, 300.0], 1.5));

    // Physical [900, 600] / 1.5 = logical [600, 400] — on the edge
    // (Rect::contains is inclusive on min/max bounds).
    assert!(sink_is_pointer_over_viewport(&sink, [900.0, 600.0], 1.5));

    // Physical [901, 0] / 1.5 ≈ logical [600.67, 0] — outside max_x.
    assert!(!sink_is_pointer_over_viewport(&sink, [901.0, 0.0], 1.5));
}

#[test]
fn pointer_inside_split_layout_rect_simulates_dispatch_d_geometry() {
    // Approximate the dispatch-D split layout: viewport on the left
    // INSPECTOR_PANE_OLD_FRACTION (~75%), inspector on the right ~25%.
    // For an 800x600 logical surface that means the viewport rect is
    // roughly [0,0]..[600,600] minus tab-bar height (~25px). Use a
    // representative value that captures the geometric intent.
    let total_w = 800.0_f32;
    let viewport_w = total_w * INSPECTOR_PANE_OLD_FRACTION;
    let tab_bar_h = 25.0;
    let total_h = 600.0_f32;
    let viewport_rect =
        egui::Rect::from_min_max(egui::pos2(0.0, tab_bar_h), egui::pos2(viewport_w, total_h));
    let sink: ViewportRectSink = Mutex::new(Some(viewport_rect));

    // ppp = 1.0 for simplicity.
    // Click on the cuboid center area (well inside the viewport).
    assert!(sink_is_pointer_over_viewport(
        &sink,
        [viewport_w / 2.0, total_h / 2.0],
        1.0
    ));
    // Click on the inspector area (well outside the viewport).
    assert!(!sink_is_pointer_over_viewport(
        &sink,
        [viewport_w + 50.0, total_h / 2.0],
        1.0
    ));
    // Click on the viewport tab bar (above the body rect).
    assert!(!sink_is_pointer_over_viewport(
        &sink,
        [viewport_w / 2.0, tab_bar_h / 2.0],
        1.0
    ));
}

// ---------------------------------------------------------------------------
// DPI defensiveness
// ---------------------------------------------------------------------------

#[test]
fn zero_or_negative_pixels_per_point_falls_safe() {
    let sink: ViewportRectSink = Mutex::new(Some(egui::Rect::from_min_max(
        egui::pos2(0.0, 0.0),
        egui::pos2(100.0, 100.0),
    )));
    assert!(!sink_is_pointer_over_viewport(&sink, [50.0, 50.0], 0.0));
    assert!(!sink_is_pointer_over_viewport(&sink, [50.0, 50.0], -1.0));
}

#[test]
fn non_finite_pixels_per_point_falls_safe() {
    let sink: ViewportRectSink = Mutex::new(Some(egui::Rect::from_min_max(
        egui::pos2(0.0, 0.0),
        egui::pos2(100.0, 100.0),
    )));
    assert!(!sink_is_pointer_over_viewport(
        &sink,
        [50.0, 50.0],
        f32::NAN
    ));
    assert!(!sink_is_pointer_over_viewport(
        &sink,
        [50.0, 50.0],
        f32::INFINITY
    ));
}

// ---------------------------------------------------------------------------
// EditorTabViewer + sink end-to-end
//
// Exercise the path the host uses inside `render`: construct a viewer
// over a sink, then verify the sink can be written / read through the
// same Arc.
// ---------------------------------------------------------------------------

#[test]
fn editor_tab_viewer_with_sink_holds_same_arc() {
    use rge_editor_egui_host::EditorTabViewer;
    let sink: Arc<ViewportRectSink> = Arc::new(Mutex::new(None));
    let viewer = EditorTabViewer::with_viewport_rect_sink(Arc::clone(&sink));
    let viewer_sink = viewer
        .viewport_rect_sink()
        .expect("viewer should report Some(sink) after with_viewport_rect_sink");
    assert!(
        Arc::ptr_eq(viewer_sink, &sink),
        "viewer must hold the same Arc the host clones into it — no silent forking"
    );
}

#[test]
fn editor_tab_viewer_default_has_no_sink() {
    use rge_editor_egui_host::EditorTabViewer;
    let viewer = EditorTabViewer::default();
    assert!(
        viewer.viewport_rect_sink().is_none(),
        "default viewer must not capture viewport rect (tests + non-host construction)"
    );
}

// ---------------------------------------------------------------------------
// Function-pointer surface — pin the public API shape
// ---------------------------------------------------------------------------

#[test]
fn public_api_surface_includes_dispatch_f_accessors() {
    // Compile-time pin of the new public surface; if these signatures
    // drift, this file fails to compile loudly.
    use rge_editor_egui_host::EguiHost;
    let _ = EguiHost::viewport_tab_rect as fn(&EguiHost) -> Option<egui::Rect>;
    let _ = EguiHost::is_pointer_over_viewport as fn(&EguiHost, [f32; 2]) -> bool;
}
