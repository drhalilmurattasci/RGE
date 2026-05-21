//! `rge-editor-egui-host` — egui + egui_dock host for the editor render loop.
//!
//! Failure class: recoverable
//!
//! Per PLAN §1.13: egui-host failures (renderer init error, paint
//! failure, input-event-adapter mismatch) are transient and recoverable
//! in-place — drop the bad frame, log, continue. No PIE state is owned
//! by this crate; the editor's authoritative state lives in
//! `kernel/ecs`, `cad-core`, and the Command Bus + audit-ledger.
//!
//! # Phase 9 dispatch A scaffold
//!
//! This crate ships the **scaffold** of the egui host integration
//! recommended in the "egui host integration preflight" (recorded in
//! `plans/BASELINE.md`):
//!
//! - [`EguiHost`] — owns `egui::Context` + `egui_winit::State` +
//!   `egui_wgpu::Renderer`.
//! - [`EguiHost::new`] — constructor over wgpu device + surface format
//!   + depth format + sample count + winit window.
//! - [`EguiHost::on_window_event`] — input adapter that delegates to
//!   `egui_winit::State::on_window_event` and returns its
//!   [`egui_winit::EventResponse`] so the embedding render loop (a
//!   future editor-shell dispatch) can branch on `response.consumed`.
//! - [`EguiHost::resize`] — accepts new physical-pixel surface
//!   dimensions + scale factor; stores them for the future
//!   `egui_wgpu::ScreenDescriptor` rendered by dispatch B.
//!
//! # Headless by design
//!
//! This crate does NOT depend on `rge-editor-shell`. The planned wiring
//! direction is the reverse: a future dispatch lets `editor-shell` add
//! `rge-editor-egui-host` as a production dep so its render loop can
//! host the egui pass. Reversing that direction (egui-host → shell)
//! would create a cycle and foreclose the planned host architecture.
//!
//! Deps already accessible without `editor-shell`:
//!
//! - `egui` / `egui-winit` / `egui-wgpu` / `egui_dock` — workspace pins.
//! - `wgpu` / `winit` — workspace pins (the wgpu device + winit window
//!   the constructor consumes are produced by editor-shell's `resumed`
//!   callback in dispatch B but passed in as borrowed primitives).
//! - `rge-editor-state` — for `InspectorSnapshot` consumption in
//!   dispatch C; declared today so dispatch C can grow this crate
//!   without an additional Cargo edit.
//! - `rge-editor-ui` — for `widgets::inspector::ui` and other widgets
//!   the host will eventually call from its DockState render path
//!   (dispatch C).
//!
//! # What this dispatch does NOT do
//!
//! - NO `editor-shell` modification (zero source change outside this crate).
//! - NO `DockState<TabBody>` construction (dispatch C).
//! - NO `TabBody` enum (dispatch C).
//! - NO `render()` method that actually paints (dispatch B).
//! - NO inspector wiring (dispatch C).
//! - NO snapshot-delivery substrate (`InspectorHandoff`) (dispatch C).
//! - NO menu Command additions.
//! - NO PlaceholderTabBody replacement.
//! - NO reflection adoption.

#![allow(clippy::module_name_repetitions)]

use std::sync::Arc;

use winit::event::WindowEvent;
use winit::window::Window;

// ---------------------------------------------------------------------------
// EguiHost
// ---------------------------------------------------------------------------

/// egui + egui_dock host. Owns the three core egui subsystems and the
/// most-recently-observed surface dimensions.
///
/// # Trait bounds
///
/// `EguiHost` is `Send` (all three inner types are `Send`) but is **not**
/// `Sync` in general — `egui_wgpu::Renderer` holds wgpu resources that
/// are not safely shareable across threads without external
/// synchronization. The compile-time assertion lives in
/// `tests/host_scaffolding_smoke.rs::host_is_send_and_static`.
///
/// # Construction
///
/// [`EguiHost::new`] takes the wgpu device, surface format, depth format,
/// sample count, and an `Arc<Window>`. All are produced by editor-shell's
/// `resumed` callback (in dispatch B, where the wire-up lands); this
/// crate has no opinion about *where* those primitives originate.
pub struct EguiHost {
    /// The egui immediate-mode context. Cheaply cloneable
    /// (`Arc`-backed); the cloned handle stored in
    /// [`egui_winit::State`] shares state with this one.
    context: egui::Context,

    /// Adapter from winit `WindowEvent` to egui's `RawInput`. Tracks
    /// modifier state, focus, cursor position, IME, etc. internally.
    state: egui_winit::State,

    /// GPU renderer for egui draw lists. Allocates wgpu buffers,
    /// textures, and a pipeline at construction; the per-frame
    /// `update_buffers` + `render` call sites land in dispatch B.
    ///
    /// `#[allow(dead_code)]` is intentional for dispatch A: the field
    /// is constructed (its pipeline + bind groups are eagerly built by
    /// `Renderer::new`, which is what we want to validate the
    /// integration shape) but is not read again until dispatch B's
    /// render-pass invocation. Dispatch B removes this allow.
    #[allow(dead_code)]
    renderer: egui_wgpu::Renderer,

    /// Most-recent physical-pixel surface dimensions, in
    /// `[width, height]`. Updated by [`Self::resize`]; consumed by the
    /// future `egui_wgpu::ScreenDescriptor` constructor in dispatch B's
    /// `render()` method.
    surface_size: [u32; 2],

    /// Most-recent device scale factor. `egui_winit::State` tracks its
    /// own copy via `WindowEvent::ScaleFactorChanged`; this field is a
    /// cache for the `ScreenDescriptor::pixels_per_point` field in
    /// dispatch B.
    pixels_per_point: f32,
}

impl EguiHost {
    /// Construct an [`EguiHost`] from primitives produced by the
    /// embedding render loop's `resumed` callback.
    ///
    /// # Parameters
    ///
    /// - `device` — wgpu device, used to create the renderer's
    ///   bind-group layouts, sampler, and shader module.
    /// - `surface_format` — color attachment format of the editor's
    ///   surface. The renderer's pipeline must match this format.
    /// - `depth_format` — `Some(format)` if the host shares the
    ///   editor's depth attachment (matches editor-shell's
    ///   `DEPTH_FORMAT = Depth24Plus`); `None` for a depth-less egui
    ///   pass that always overlays without z-tests.
    /// - `msaa_samples` — sample count of the editor's color
    ///   attachment. Single-sample today (`1`), matching
    ///   `editor-shell::render_path::build_lit_mesh_compiled_frame_graph`.
    /// - `window` — `Arc<Window>`, retained internally so
    ///   [`egui_winit::State`] can read the scale factor.
    /// - `viewport_id` — typically [`egui::ViewportId::ROOT`] for the
    ///   single-window editor.
    ///
    /// # Errors
    ///
    /// Construction is infallible; egui's subsystems do not return
    /// `Result` from their constructors. A subsequent dispatch may
    /// promote this to `Result<Self, EguiHostError>` if config-validation
    /// surfaces real failure modes.
    #[must_use]
    pub fn new(
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
        depth_format: Option<wgpu::TextureFormat>,
        msaa_samples: u32,
        window: Arc<Window>,
        viewport_id: egui::ViewportId,
    ) -> Self {
        let context = egui::Context::default();
        let pixels_per_point = window.scale_factor() as f32;
        let inner_size = window.inner_size();

        // egui-winit 0.34 with winit 0.30: `State::new` takes the cloned
        // egui context, the viewport id, a `&dyn HasDisplayHandle`
        // (winit Window implements this), an optional initial scale
        // factor, optional theme, and an optional max-texture-side hint.
        let state = egui_winit::State::new(
            context.clone(),
            viewport_id,
            window.as_ref(),
            Some(pixels_per_point),
            None,
            None,
        );

        // egui-wgpu 0.34 `Renderer::new` with wgpu 29: device, color
        // format, and a [`RendererOptions`] config struct. The 0.34
        // bump consolidated the depth-format / msaa / dithering /
        // predictable-filtering knobs from positional args into a
        // dedicated config so future options can extend additively
        // without breaking the call site.
        let renderer_options = egui_wgpu::RendererOptions {
            msaa_samples,
            depth_stencil_format: depth_format,
            // Disabled — editor renders sRGB; dithering is unnecessary
            // for editor UI legibility and adds noise to color
            // assertions that snapshot tests might want to make.
            dithering: false,
            // Disabled — we want the GPU's native texture filtering
            // for everything except egui-internal snapshot tests
            // (which aren't part of this dispatch).
            predictable_texture_filtering: false,
        };
        let renderer = egui_wgpu::Renderer::new(device, surface_format, renderer_options);

        tracing::debug!(
            target: "rge::editor-egui-host",
            surface_w = inner_size.width,
            surface_h = inner_size.height,
            scale = pixels_per_point,
            ?surface_format,
            ?depth_format,
            msaa_samples,
            "EguiHost constructed"
        );

        Self {
            context,
            state,
            renderer,
            surface_size: [inner_size.width, inner_size.height],
            pixels_per_point,
        }
    }

    /// Adapt a winit `WindowEvent` into egui's input stream. Returns
    /// [`egui_winit::EventResponse`] so the embedding render loop can
    /// branch on `response.consumed`:
    ///
    /// - When `consumed == true`, an egui widget claimed the event
    ///   (text-field keystroke, button click, drag). The editor's
    ///   application-level handler (e.g. Phase 9's
    ///   `EditorKeyCommand::from_key_press`) should **skip** this
    ///   event.
    /// - When `consumed == false`, no egui widget claimed it. The
    ///   editor handles it normally (face-pick on viewport click,
    ///   Ctrl+Z to the Command Bus, etc.).
    ///
    /// `response.repaint == true` signals that egui's visual state
    /// changed (cursor moved over a hover, focus shifted, animation
    /// frame); the embedding loop should request a window redraw.
    ///
    /// Modifier and cursor events are passed unconditionally (both
    /// egui and the editor track them as state) — the dispatch-B
    /// editor-shell wire-up will route Keyboard / Mouse events
    /// "egui-first then editor-fallback" while routing
    /// ModifiersChanged / CursorMoved through both subsystems
    /// unconditionally.
    pub fn on_window_event(
        &mut self,
        window: &Window,
        event: &WindowEvent,
    ) -> egui_winit::EventResponse {
        self.state.on_window_event(window, event)
    }

    /// Update the host's view of the surface dimensions + scale factor.
    /// Called by the embedding render loop (dispatch B) after
    /// `surface_ctx.resize(...)`.
    ///
    /// This does NOT invoke `egui_wgpu::Renderer::resize` directly —
    /// the renderer's `update_buffers` + `render` calls in dispatch B
    /// will read [`Self::surface_size`] + [`Self::pixels_per_point`]
    /// into a fresh `ScreenDescriptor` per frame.
    pub fn resize(&mut self, new_w: u32, new_h: u32, pixels_per_point: f32) {
        self.surface_size = [new_w, new_h];
        self.pixels_per_point = pixels_per_point;
        tracing::trace!(
            target: "rge::editor-egui-host",
            new_w,
            new_h,
            pixels_per_point,
            "EguiHost::resize"
        );
    }

    /// Borrow the egui context (read-only). Dispatch B / C use this for
    /// `Context::request_repaint`, scope inspection, and similar.
    #[must_use]
    pub fn context(&self) -> &egui::Context {
        &self.context
    }

    /// Most-recent surface dimensions in physical pixels `[w, h]`.
    /// Read by dispatch B when building the per-frame
    /// `egui_wgpu::ScreenDescriptor`.
    #[must_use]
    pub fn surface_size(&self) -> [u32; 2] {
        self.surface_size
    }

    /// Most-recent device scale factor (pixels-per-egui-point).
    #[must_use]
    pub fn pixels_per_point(&self) -> f32 {
        self.pixels_per_point
    }
}
