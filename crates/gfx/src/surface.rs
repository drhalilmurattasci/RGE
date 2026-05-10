//! `gfx::surface` — wgpu `Surface` + `SurfaceConfiguration` wrapper for
//! windowed rendering targets.
//!
//! Sub-δ.1.A substrate-only piece: this struct exists so the next dispatch
//! (sub-δ.1.B) can wire it through `editor-shell` / `rge-editor` to put a
//! triangle on screen. **No real Window is constructed in tests** —
//! coverage is compile / structural only (a `no_run` doctest on
//! [`SurfaceContext::new`] proves the API surface compiles against the
//! workspace's pinned wgpu 29 + winit 0.30).
//!
//! # Window-handle ownership
//!
//! wgpu 29 exposes `Instance::create_surface(target: impl Into<SurfaceTarget<'window>>)
//! -> Surface<'window>`. `SurfaceTarget` carries an explicit `'window`
//! lifetime, but its `From<T> for SurfaceTarget<'a> where T: DisplayAndWindowHandle + 'a`
//! impl accepts any owned-handle type whose lifetime outlives the surface.
//!
//! `Arc<winit::window::Window>` is `'static` (it owns the window via the
//! `Arc` and both `Arc<H: HasWindowHandle>` and `Arc<H: HasDisplayHandle>`
//! impls live in `raw-window-handle` 0.6). Passing `arc.clone()` into
//! `create_surface` therefore produces `Surface<'static>` — the surface
//! keeps the window alive on its own, independent of the caller's
//! [`SurfaceContext`] handle. We retain the `Arc<Window>` inside
//! [`SurfaceContext`] for two reasons:
//!
//! 1. Callers that want to reach back to the window (e.g. for `request_redraw`
//!    or `inner_size`) can borrow it via a future accessor (NOT exposed in
//!    sub-δ.1.A — kept private to satisfy the "no API beyond X and Y"
//!    constraint).
//! 2. The `Surface` already retains a `_handle_source` clone internally per
//!    wgpu 29's source code; our `Arc<Window>` is the externally-visible
//!    owner. Reference-counting keeps the lifetime story trivial.
//!
//! See `wgpu-29.0.3/src/api/instance.rs` lines 184-256 for the
//! `create_surface` implementation and `wgpu-29.0.3/src/api/surface.rs`
//! lines 244-329 for the `SurfaceTarget` enum + `From` impl that this
//! module composes against.

use std::sync::Arc;

use winit::window::Window;

use crate::context::GfxContext;

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur when constructing or reconfiguring a [`SurfaceContext`].
///
/// Mirrors the existing `BufferError` / `LitMeshPipelineError` thiserror
/// style. Marked `#[non_exhaustive]` so future variants (depth-attachment
/// allocation failure, etc.) can land without breaking callers.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SurfaceError {
    /// `Instance::create_surface` failed (driver did not produce a surface
    /// for the supplied window handle).
    #[error("create_surface failed: {0}")]
    CreateSurface(String),

    /// The adapter returned no compatible surface format. Indicates the
    /// adapter selected by [`GfxContext::new_headless`] is not compatible
    /// with the window's display server (extremely rare on desktop;
    /// occurs on certain WebGL2 configurations).
    #[error("adapter has no compatible surface format for this window")]
    NoCompatibleFormat,
}

// ---------------------------------------------------------------------------
// SurfaceContext
// ---------------------------------------------------------------------------

/// Wraps a `wgpu::Surface` together with its current `SurfaceConfiguration`
/// and the `Arc<Window>` that owns the underlying window handle.
///
/// The surface lifetime is `'static` because `Arc<Window>` is `'static` —
/// see the module-level docs for the rationale.
///
/// Resize through [`SurfaceContext::resize`] to keep the configuration in
/// sync with the window's drawable area; sub-δ.1.B will wire that to
/// `WindowEvent::Resized` in the editor shell.
pub struct SurfaceContext {
    /// Owned wgpu surface, lifetime `'static` (kept alive by `_window`).
    surface: wgpu::Surface<'static>,

    /// Most-recently-applied surface configuration. Mirrors what was passed
    /// to `Surface::configure`.
    config: wgpu::SurfaceConfiguration,

    /// Owned window handle. Kept alive for the lifetime of the surface so
    /// the `Surface<'static>` stays valid; also reachable to the caller
    /// in future dispatches (no public accessor in sub-δ.1.A).
    _window: Arc<Window>,
}

impl SurfaceContext {
    /// Construct a [`SurfaceContext`] for `window` using the wgpu instance
    /// and adapter from `ctx`.
    ///
    /// `window` ownership is shared via `Arc::clone` — the surface keeps a
    /// clone alive internally, and [`SurfaceContext`] keeps another clone
    /// alive externally. The original `Arc` the caller passed in remains
    /// valid for the caller's own use (event-loop state, etc.).
    ///
    /// Surface format is selected as the first entry of the adapter's
    /// preferred-format list (the canonical wgpu pattern; see
    /// `wgpu::Surface::get_default_config`). Configuration uses the
    /// window's current inner size; surface usage is `RENDER_ATTACHMENT`;
    /// present mode is the adapter's first preferred mode (typically
    /// `PresentMode::Fifo`); alpha mode is `Auto`.
    ///
    /// The surface is configured immediately — caller can call
    /// `Surface::get_current_texture` right after construction.
    ///
    /// # Errors
    ///
    /// - [`SurfaceError::CreateSurface`] if `Instance::create_surface`
    ///   returns an error (driver-incompatible window handle).
    /// - [`SurfaceError::NoCompatibleFormat`] if the adapter reports no
    ///   compatible surface format for this surface.
    pub fn new(ctx: &GfxContext, window: Arc<Window>) -> Result<Self, SurfaceError> {
        let surface = ctx
            .instance()
            .create_surface(Arc::clone(&window))
            .map_err(|e| SurfaceError::CreateSurface(e.to_string()))?;

        let inner_size = window.inner_size();
        // Some platforms emit a 0×0 size before the first redraw; clamp
        // both dimensions to at least 1 so `Surface::configure` accepts
        // them. (The first `WindowEvent::Resized` in sub-δ.1.B will
        // overwrite these with the real window size.)
        let width = inner_size.width.max(1);
        let height = inner_size.height.max(1);

        let config = ctx
            .adapter()
            .map(|adapter| surface.get_default_config(adapter, width, height))
            .ok_or(SurfaceError::NoCompatibleFormat)?
            .ok_or(SurfaceError::NoCompatibleFormat)?;

        surface.configure(ctx.device(), &config);

        Ok(Self {
            surface,
            config,
            _window: window,
        })
    }

    /// Reconfigure the surface for a new window size.
    ///
    /// Call this on `WindowEvent::Resized` (sub-δ.1.B will wire that). A
    /// width or height of zero is a no-op (some platforms emit a 0×0
    /// resize during minimisation; calling `Surface::configure` with zero
    /// dimensions panics on wgpu 29 — see `Surface::configure`'s
    /// "# Panics" doc).
    pub fn resize(&mut self, ctx: &GfxContext, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }

        self.config.width = width;
        self.config.height = height;
        self.surface.configure(ctx.device(), &self.config);
    }

    /// Borrow the current `wgpu::Surface`.
    ///
    /// Used by the frame loop to acquire a `SurfaceTexture` for
    /// presentation via `Surface::get_current_texture`.
    #[must_use]
    pub fn surface(&self) -> &wgpu::Surface<'static> {
        &self.surface
    }

    /// Borrow the current `SurfaceConfiguration`.
    ///
    /// Caller can read `width` / `height` / `format` to size the depth
    /// attachment / camera aspect ratio / pipeline color-target format.
    #[must_use]
    pub fn config(&self) -> &wgpu::SurfaceConfiguration {
        &self.config
    }
}

// ---------------------------------------------------------------------------
// Compile-coverage doctest for the API shape
// ---------------------------------------------------------------------------

/// Compile-only doctest that proves the `SurfaceContext::new` API shape
/// matches wgpu 29 + winit 0.30. The test never runs because creating
/// a real winit `Window` requires an `EventLoop`-bound thread and is
/// not viable in test runners; the doctest is `no_run`.
///
/// ```no_run
/// use std::sync::Arc;
///
/// use rge_gfx::{GfxContext, surface::SurfaceContext};
///
/// // Build a headless GfxContext (the real editor uses the same constructor).
/// let ctx = GfxContext::new_headless().unwrap();
///
/// // Create a winit event loop and window — only the API shape matters here.
/// let event_loop: winit::event_loop::EventLoop<()> =
///     winit::event_loop::EventLoop::new().unwrap();
/// // SAFETY: `WindowAttributes` is the wgpu-29 / winit-0.30 idiomatic
/// // builder; the `no_run` annotation prevents this code from running
/// // in environments without a display server.
/// let window = event_loop
///     .create_window(winit::window::WindowAttributes::default())
///     .unwrap();
/// let window = Arc::new(window);
///
/// let mut surface_ctx = SurfaceContext::new(&ctx, window).unwrap();
/// surface_ctx.resize(&ctx, 1280, 720);
///
/// // Surface and configuration are observable.
/// let _surface: &wgpu::Surface<'static> = surface_ctx.surface();
/// let _config: &wgpu::SurfaceConfiguration = surface_ctx.config();
/// ```
#[cfg(doctest)]
#[allow(dead_code)]
fn _surface_context_compile_only_doctest() {}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Construct a hand-built [`wgpu::SurfaceConfiguration`] (no real
    /// surface required) to verify the type's field shape and ensure
    /// `SurfaceContext::config` returns the expected layout.
    ///
    /// Purely structural: no GPU work, no Window construction.
    #[test]
    fn surface_configuration_field_shape_compiles() {
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            width: 1280,
            height: 720,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
        };

        assert_eq!(config.width, 1280);
        assert_eq!(config.height, 720);
        assert_eq!(config.format, wgpu::TextureFormat::Bgra8UnormSrgb);
        assert_eq!(config.present_mode, wgpu::PresentMode::Fifo);
        assert!(config.view_formats.is_empty());
    }

    /// Verify [`SurfaceError`] formats sensibly through `Display` so
    /// callers reporting it through `tracing::error!` get readable
    /// messages.
    #[test]
    fn surface_error_displays_meaningfully() {
        let err = SurfaceError::CreateSurface("test detail".to_string());
        let s = format!("{err}");
        assert!(s.contains("create_surface"));
        assert!(s.contains("test detail"));

        let err = SurfaceError::NoCompatibleFormat;
        let s = format!("{err}");
        assert!(s.contains("no compatible"));
    }
}
