//! Phase 9 egui host integration — dispatch A scaffold smoke tests.
//!
//! Pure headless tests: trait-bound assertions + public-API surface
//! presence. Construction tests are deferred to dispatch B (where
//! editor-shell provides a real wgpu device + winit window), and
//! end-to-end render tests are deferred to dispatch C (where DockState
//! + TabBody + Inspector wiring lands).
//!
//! Why no GPU construction test here: `egui_wgpu::Renderer::new`
//! requires a real `wgpu::Device`, which in turn requires
//! `Instance::new() -> request_adapter().await -> request_device().await`.
//! Spinning that up in a dispatch-A test adds substantial fragility
//! (recorder-host-only, async runtime, potential CI hangs) for a
//! payoff that dispatch B exercises naturally through editor-shell.

use rge_editor_egui_host::EguiHost;
use rge_editor_ui::menus::{ExtensionPoint, MenuEntry, RegistryError};

// ---------------------------------------------------------------------------
// Trait bounds
// ---------------------------------------------------------------------------

/// Compile-time assertion: `EguiHost` is `Send + 'static`. This is the
/// minimum bound for the future editor-shell wire-up to store an
/// `EguiHost` in an `Option<EguiHost>` field and pass it across
/// `&mut self` boundaries without lifetime gymnastics.
///
/// `EguiHost` is intentionally NOT required to be `Sync` —
/// `egui_wgpu::Renderer` holds wgpu resources that need external
/// synchronization for cross-thread use. The single-threaded editor
/// render loop owns the host exclusively, so `Send + 'static` is
/// sufficient.
#[test]
fn host_is_send_and_static() {
    fn assert_send_and_static<T: Send + 'static>() {}
    assert_send_and_static::<EguiHost>();
}

// ---------------------------------------------------------------------------
// Public-API surface presence
// ---------------------------------------------------------------------------

/// Compile-time assertion that the public surface compiles as expected.
/// If a future dispatch accidentally changes a method signature or
/// removes a public method this test fails to compile loudly.
#[test]
fn public_api_surface_is_present() {
    // Reference each public method as a function pointer (zero runtime
    // cost; pure compile-time check). If `EguiHost` drops or renames any
    // of these, this file no longer compiles.
    let _ = EguiHost::new
        as fn(
            &wgpu::Device,
            wgpu::TextureFormat,
            Option<wgpu::TextureFormat>,
            u32,
            std::sync::Arc<winit::window::Window>,
            egui::ViewportId,
        ) -> EguiHost;
    let _ = EguiHost::on_window_event
        as fn(
            &mut EguiHost,
            &winit::window::Window,
            &winit::event::WindowEvent,
        ) -> egui_winit::EventResponse;
    let _ = EguiHost::resize as fn(&mut EguiHost, u32, u32, f32);
    let _ = EguiHost::context as fn(&EguiHost) -> &egui::Context;
    let _ = EguiHost::surface_size as fn(&EguiHost) -> [u32; 2];
    let _ = EguiHost::pixels_per_point as fn(&EguiHost) -> f32;
    let _ = EguiHost::register_menu_entry
        as fn(&mut EguiHost, &ExtensionPoint, MenuEntry) -> Result<(), RegistryError>;
    let _ = EguiHost::register_plugin_menu_entry
        as fn(&mut EguiHost, MenuEntry) -> Result<(), RegistryError>;
    let _ = EguiHost::toggle_command_palette as fn(&mut EguiHost);
    let _ = EguiHost::is_command_palette_open as fn(&EguiHost) -> bool;

    // Dispatch C: `render` no longer takes a caller-supplied UI
    // closure — the host owns the [`egui_dock::DockState`] layout
    // internally, so the signature is just the 5 wgpu/winit borrows.
    // Use function-pointer coercion: if `render` is renamed or its
    // arg list drifts this sentinel fails to compile.
    let _ = EguiHost::render
        as fn(
            &mut EguiHost,
            &winit::window::Window,
            &wgpu::Device,
            &wgpu::Queue,
            &mut wgpu::CommandEncoder,
            &wgpu::TextureView,
        );

    // Dispatch C: new public surface for the inspector handoff +
    // dock-state accessors.
    let _ = EguiHost::inspector_handoff
        as fn(&EguiHost) -> &std::sync::Arc<rge_editor_egui_host::InspectorHandoff>;
    let _ = EguiHost::dock_state
        as fn(&EguiHost) -> &egui_dock::DockState<rge_editor_egui_host::TabBody>;
}

// ---------------------------------------------------------------------------
// Module-level smoke
// ---------------------------------------------------------------------------

/// Confirm the crate root re-exports `EguiHost` (the only public type
/// in dispatch A). Catches accidental visibility regressions.
#[test]
fn crate_re_exports_egui_host() {
    fn _check<T>() {}
    _check::<EguiHost>();
}
