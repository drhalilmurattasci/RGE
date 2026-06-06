// SPLIT-EXEMPTION: cohesive render-path lifecycle. Holds
// `init_render_state` / `init_render_state_post_surface` /
// `init_render_state_headless` / `reload_render_assets` /
// `acquire_depth_view` / `render_frame` / `render_frame_to_target`
// / `encode_main_pass` / `resize_render_path`. Every method shares
// the same set of `pub(crate)` GPU fields on `EditorShell`
// (`gfx_ctx`, `pipeline`, `gfx_camera`, `light`, `materials`,
// `meshes`, frame-graph pools); splitting would either expose those
// across module boundaries (visibility churn) or fragment the
// single coherent "render-state lifetime" surface. A future trim
// could extract the const block + `build_lit_mesh_compiled_frame_graph`
// helpers into a sibling once a second consumer appears; today
// there is only one.

//! Sub-δ.1.B + sub-ε render path for [`crate::EditorShell`].
//!
//! Split out from `lifecycle.rs` as a pure structural refactor on
//! 2026-05-11 (post Render-backed face-selection chapter close-out).
//! All methods live in `impl EditorShell { … }` blocks here; no new
//! types, no new public API, no visibility changes — Rust resolves
//! the methods across files at compile time.
//!
//! Contents:
//!
//! - The `DEFAULT_CLEAR` / `WHITE_1X1_RGBA` / `HIGHLIGHT_COLOR` /
//!   `HIGHLIGHT_PHONG` constants + `default_light_direction()` helper.
//! - [`EditorShell::init_render_state`] — wgpu/Surface/Pipeline/Material/
//!   LitMesh/Light/Camera GPU init triggered from `resumed`.
//! - [`EditorShell::render_frame`] — the per-frame encode → set_pipeline
//!   → set_bind_groups → draw_indexed → submit → present sequence
//!   (including the sub-ε overlay's second `draw_indexed`).
//! - [`EditorShell::resize_render_path`] — surface reconfigure on
//!   `WindowEvent::Resized`.

use std::sync::Arc;

use rge_editor_actions::BusError;
use rge_editor_ui::menus::Command;
use rge_gfx::{
    build_resource_map, BufferPool, Camera as GfxCamera, CompiledFrameGraph, DepthStateKey,
    DirectionalLight, FrameGraph, GfxContext, LitMesh, LitMeshPipeline, Material,
    ResourceClassDescriptor, ResourceId, SurfaceContext, TextureDescriptor, TexturePool,
};
use winit::dpi::LogicalSize;
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowAttributes;

use crate::lifecycle::EditorShell;
use crate::play_state::PlayState;
use crate::play_toolbar::ToolbarButtonId;
use crate::render_input::RenderInput;

/// Default render-path background color (R, G, B, A) used as the
/// `LoadOp::Clear` value on the surface texture's color attachment.
/// Dark neutral gray — high enough contrast that a Lambert+Phong-shaded
/// cuboid is visible without overpowering its brightness range.
const DEFAULT_CLEAR: wgpu::Color = wgpu::Color {
    r: 0.12,
    g: 0.12,
    b: 0.14,
    a: 1.0,
};

/// Default directional light direction (sub-δ.1.B). Light travels
/// toward `(-1, -1, -1)` (normalised); illuminates the +X / +Y / +Z
/// faces of a cuboid at the origin with distinct shading variations
/// from the camera at `(3, 3, 3)`.
fn default_light_direction() -> glam::Vec3 {
    glam::Vec3::new(-1.0, -1.0, -1.0).normalize()
}

/// Default 1×1 white texture (4 bytes RGBA8Unorm, single texel) used as
/// the placeholder texture for the [`Material`]. The Lambert+Phong
/// shader samples this texture but the default base color is white,
/// so the shading variation comes entirely from the light/normal
/// dot product (no texturing in sub-δ.1.B).
const WHITE_1X1_RGBA: [u8; 4] = [255, 255, 255, 255];

/// Selection-highlight tint for sub-ε visual feedback. Orange. Applied to
/// the second `Material` via [`Material::update_color`] so the overlay
/// `draw_indexed` over the main cuboid uses this color through the existing
/// `LitMeshPipeline`'s Lambert+Phong shader (no shader, no pipeline, and
/// no `Material` struct changes).
///
/// Hard-coded for the first visual-feedback pass; theme / config integration
/// is out of scope for sub-ε.
pub(crate) const HIGHLIGHT_COLOR: glam::Vec4 = glam::Vec4::new(1.0, 0.6, 0.0, 1.0);

/// Phong factors for the highlight material — same shape as `Material::new`'s
/// default `(ambient, diffuse, specular, shininess)` so the shading
/// continuity with the main cuboid is preserved.
const HIGHLIGHT_PHONG: glam::Vec4 = glam::Vec4::new(0.1, 1.0, 0.5, 32.0);

/// Dispatch K — default Phong factors applied to every per-mesh
/// `Material` produced from a glTF `base_color`. Same `(ambient,
/// diffuse, specular, shininess)` shape `Material::new` initialises
/// to, so the only thing that varies between meshes is `base_color`.
/// glTF's `metallic` / `roughness` factors are intentionally NOT
/// translated into Phong shininess for v0 — the Lambert+Phong
/// shader has no PBR slot and a guessed mapping would be more
/// misleading than a uniform default.
const DEFAULT_PHONG: glam::Vec4 = glam::Vec4::new(0.1, 1.0, 0.5, 32.0);

// ---------------------------------------------------------------------------
// Phase 6 sub-β — transient depth wire constants + helper
// ---------------------------------------------------------------------------

/// Phase 6 sub-β depth attachment format. `Depth24Plus` is the wgpu
/// portable depth format (24-bit unsigned normalised) — sufficient for
/// the single-pass `lit_mesh` flow's depth-test purposes. The format is
/// pinned by this constant so the [`TextureDescriptor`] in the
/// `FrameGraph` and the [`DepthStateKey`] on the [`LitMeshPipeline`]
/// stay in lockstep.
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth24Plus;

/// Phase 6 sub-β stable identifier for the per-frame transient depth
/// texture. `const` so `init_render_state` and `render_frame` reference
/// the exact same bytes without re-deriving. The ASCII prefix
/// `rge.edsh.depth.` is informational only — opaque-ness is preserved
/// per `ResourceId`'s contract.
const DEPTH_RESOURCE_ID: ResourceId = ResourceId::from_bytes([
    b'r', b'g', b'e', b'.', b'e', b'd', b's', b'h', b'.', b'd', b'e', b'p', b't', b'h', 0, 1,
]);

/// Compile a single-pass `FrameGraph` for the `lit_mesh` flow against
/// the current surface dimensions. Helper called from
/// [`EditorShell::init_render_state`] and
/// [`EditorShell::resize_render_path`] (the latter on every surface
/// resize because [`TextureDescriptor`] is keyed on `width`/`height`
/// and the descriptor flows verbatim into pool free-list identity).
///
/// One pass `"lit_mesh"` declares one write of [`DEPTH_RESOURCE_ID`]
/// at [`DEPTH_FORMAT`] with `RENDER_ATTACHMENT` usage. `compile()`
/// always succeeds for a single-pass graph (no cycles possible, every
/// declared resource is written by definition).
fn build_lit_mesh_compiled_frame_graph(
    surface_width: u32,
    surface_height: u32,
) -> CompiledFrameGraph {
    let depth_descriptor = TextureDescriptor {
        width: surface_width.max(1),
        height: surface_height.max(1),
        depth_or_array_layers: 1,
        mip_level_count: 1,
        sample_count: 1,
        format: DEPTH_FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        dimension: wgpu::TextureDimension::D2,
        view_dimension: wgpu::TextureViewDimension::D2,
    };
    let mut fg = FrameGraph::new();
    fg.add_pass(
        "lit_mesh",
        vec![],
        vec![(
            DEPTH_RESOURCE_ID,
            ResourceClassDescriptor::Texture(depth_descriptor),
        )],
    )
    .expect("single-pass FrameGraph add_pass: only failure mode is descriptor mismatch, impossible here");
    fg.compile().expect(
        "single-pass FrameGraph compile: no cycles, every read matched by a write (no reads)",
    )
}

/// Phase 6 sub-β [`DepthStateKey`] for the shared [`LitMeshPipeline`].
/// `LessEqual` + `depth_write_enabled: false` per the future-note at
/// the render_frame highlight-overlay site below — both the main
/// cuboid draw and the highlight-overlay draw share this single
/// pipeline, and `depth_write_enabled: false` on the shared pipeline
/// inhibits the Z-fight that identical-position cuboid + overlay
/// geometry would otherwise produce against a populated depth buffer.
/// The depth buffer stays at `Clear(1.0)` for the entire frame; every
/// fragment passes `LessEqual` against 1.0; render order determines
/// visibility (overlay drawn second wins where it draws). This
/// preserves the pre-sub-β no-depth visual behavior exactly while
/// consuming the transient depth substrate end-to-end.
fn lit_mesh_depth_state() -> DepthStateKey {
    DepthStateKey::new(DEPTH_FORMAT, false, wgpu::CompareFunction::LessEqual)
}

/// 3-way outcome of [`EditorShell::acquire_depth_view`]. Distinguishes
/// "render state not yet initialised" (caller returns `false`) from
/// "transient depth allocation skipped this frame" (caller requests
/// another redraw and returns `true`) from the success path. Shared
/// between production [`EditorShell::render_frame`] and the crate-local
/// `render_frame_e2e_perf` harness so production behaviour is preserved
/// exactly when either code path takes the skip branch.
pub(crate) enum DepthViewOutcome {
    /// Render state is not initialised (any of `gfx_ctx` /
    /// `compiled_frame_graph` / `texture_pool` / `buffer_pool` is
    /// `None`). Caller returns `false`.
    Uninitialized,
    /// Transient depth allocation failed (`build_resource_map` Err).
    /// Caller logs (already done by
    /// [`EditorShell::acquire_depth_view`]), requests another redraw
    /// if a window is present, and returns `true` so the event loop
    /// continues.
    RecoverableSkip,
    /// Depth view acquired successfully.
    Acquired(wgpu::TextureView),
}

impl EditorShell {
    /// Build the GPU-side render state on first `resumed`.
    ///
    /// **Phase 1 (unconditional)** — every production `resumed` call:
    /// 1. `winit::Window` → `Arc<Window>`
    /// 2. `GfxContext::new_headless()` (instance / adapter / device / queue)
    /// 3. `SurfaceContext::new(&ctx, Arc<Window>)` (configure + surface)
    /// 4. `EguiHost::new(...)` + `InspectorHandoff` + `SaveStatusHandoff` + `PredicateContextHandoff` clones
    ///
    /// **Phase 2 (gated on `has_cad_scene || has_prebuilt_mesh`)** —
    /// delegates to [`Self::init_render_state_post_surface`]:
    /// 5. `Material::new` (per mesh) / `DirectionalLight::new` / `GfxCamera::new`
    /// 6. `LitMeshPipeline::new(...)` against the surface's color format
    /// 7. `RenderMesh` upload (from `projection.render_mesh_for(entity)`
    ///    or `self.prebuilt_render_meshes`) → `LitMesh::from_render_mesh`
    /// 8. Camera UBO populated with the editor camera's first view*proj
    ///
    /// When neither a CAD scene nor a prebuilt render mesh is attached
    /// (empty-world `--scene` launch) Phase 2 is skipped entirely; the
    /// cuboid pipeline + frame-graph + mesh fields remain `None` /
    /// empty and [`Self::render_frame`] takes its egui-only branch so
    /// the dock + Inspector tab + bottom save-status bar chrome are still
    /// visible.
    ///
    /// Returns `Err(...)` only if the GPU-side initialisation fails (no
    /// adapter, no compatible surface format, surface create_surface,
    /// pipeline compile, buffer allocation). The error is propagated up
    /// to `resumed` which logs and continues with a placeholder banner.
    pub(crate) fn init_render_state(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        // ISSUE-249 — two-phase production init.
        //
        // Phase 1 runs UNCONDITIONALLY when production winit `resumed`
        // invokes this method. It constructs the winit window,
        // [`GfxContext`], [`SurfaceContext`], [`EguiHost`], and the
        // [`InspectorHandoff`] + [`SaveStatusHandoff`] + [`PredicateContextHandoff`]
        // clones so a world-only `--scene` launch (no CAD scene, no prebuilt render
        // mesh) still produces a visible editor window with the dock +
        // Inspector tab + bottom save-status bar chrome painted via the
        // egui-only branch in [`Self::render_frame`].
        //
        // Phase 2 remains gated on `has_cad_scene || has_prebuilt_mesh`
        // and consumes [`Self::init_render_state_post_surface`] to build
        // the cuboid pipeline / per-mesh materials / lit-mesh upload /
        // frame-graph substrate plus the existing mesh-side
        // assertions. When neither side is attached, Phase 2 is
        // skipped entirely; the cuboid pipeline / frame-graph / mesh
        // fields remain `None` / empty and `render_frame` takes the
        // egui-only path.

        // Phase 1 — Step 1: winit window.
        let attrs = WindowAttributes::default()
            .with_title("RGE Editor")
            .with_inner_size(LogicalSize::new(1024_u32, 768_u32));
        let window = event_loop
            .create_window(attrs)
            .map_err(|e| format!("create_window: {e}"))?;
        let window = Arc::new(window);

        // Phase 1 — Step 2: GfxContext.
        let gfx_ctx = GfxContext::new_headless().map_err(|e| format!("gfx ctx: {e}"))?;

        // Phase 1 — Step 3: SurfaceContext.
        let surface_ctx = SurfaceContext::new(&gfx_ctx, Arc::clone(&window))
            .map_err(|e| format!("surface: {e}"))?;
        let format = surface_ctx.config().format;
        let width = surface_ctx.config().width;
        let height = surface_ctx.config().height;

        // Phase 2 — cuboid pipeline + render-mesh upload, gated.
        //
        // The post-surface helper consumes `gfx_ctx` by value and
        // stashes it on `self` alongside every other Phase 2 field
        // (`pipeline` / `gfx_camera` / `materials` / `meshes` /
        // `texture_pool` / `buffer_pool` / `compiled_frame_graph`).
        // The mesh-side assertions inside the helper run only inside
        // this branch, preserving their contract intact.
        //
        // In the empty-world branch we stash `gfx_ctx` ourselves so
        // the EguiHost construction below (and the egui-only
        // `render_frame` path) can read `self.gfx_ctx`.
        let has_cad_scene = self.cad_world.is_some() && self.cad_entity.is_some();
        let has_prebuilt_mesh = !self.prebuilt_render_meshes.is_empty();
        if has_cad_scene || has_prebuilt_mesh {
            self.init_render_state_post_surface(gfx_ctx, format, width, height)?;
        } else {
            self.gfx_ctx = Some(gfx_ctx);
        }

        // Stash the winit-bound bits not covered by the shared helper.
        self.window = Some(window);
        // EDITOR-WINDOW-TITLE — a freshly created window starts at the static
        // creation title ("RGE Editor"); invalidate the title cache so the next
        // `sync_window_title` re-applies the live title. Without this, a
        // suspend/resume that installs a NEW window would skip `set_title`
        // (cached title == desired title) and leave the new window mistitled.
        self.last_window_title = None;
        self.surface_ctx = Some(surface_ctx);

        // Phase 9 egui host integration (dispatch B) — construct the
        // headless `EguiHost` now that wgpu + winit primitives are all
        // in `self`. The host's render pass runs LATER inside
        // [`Self::render_frame`] between the cuboid+highlight pass and
        // `queue.submit()`. Construction itself is infallible; failure
        // to construct would be a wgpu pipeline-validation panic that
        // would have already torn down the render init above.
        //
        // `depth_format = None` because egui is a 2D overlay drawn on
        // top of the cuboid; it needs no depth tests. `msaa_samples = 1`
        // matches the editor's single-sample frame-graph
        // (`render_path::build_lit_mesh_compiled_frame_graph` uses
        // `sample_count = 1`).
        //
        // After the host is constructed, clone its three latest-only handoffs
        // into `self.inspector_handoff` + `self.save_status_handoff` +
        // `self.predicate_context_handoff` (Dispatch C for the inspector;
        // EDITOR-SAVE-STATUS-INDICATOR for the save-status bar;
        // MENU-DYNAMIC-RESOLVE for the live predicate context) so the per-frame
        // publish path (in [`Self::render_frame`] below) can call
        // `handoff.publish(Arc::new(self.inspector_snapshot()))`,
        // `handoff.publish(Arc::new(self.save_status_snapshot()))`, and
        // `handoff.publish(Arc::new(self.predicate_context()))` without
        // re-borrowing `self.egui_host`. Each `self.*_handoff` points at the same
        // slot as the host's consumer (the Inspector tab body / the bottom status
        // bar / the re-resolved menu) — the publish/acquire pairs are the live wires.
        if let (Some(gfx_ctx), Some(surface_ctx), Some(window)) = (
            self.gfx_ctx.as_ref(),
            self.surface_ctx.as_ref(),
            self.window.as_ref(),
        ) {
            let surface_format = surface_ctx.config().format;
            let host = rge_editor_egui_host::EguiHost::new(
                gfx_ctx.device(),
                surface_format,
                None,
                1,
                Arc::clone(window),
                rge_editor_egui_host::ViewportId::ROOT,
            );
            self.inspector_handoff = Some(Arc::clone(host.inspector_handoff()));
            self.save_status_handoff = Some(Arc::clone(host.save_status_handoff()));
            self.predicate_context_handoff = Some(Arc::clone(host.predicate_context_handoff()));
            self.menu_command_handoff = Some(Arc::clone(host.menu_command_handoff()));
            self.egui_host = Some(host);
        }

        // Kick off the first redraw so the cuboid appears.
        if let Some(w) = self.window.as_ref() {
            w.request_redraw();
        }

        Ok(())
    }

    /// Drain the host→shell menu-command FIFO ([`rge_editor_egui_host::MenuCommandHandoff`])
    /// and route each [`Command`] **one-way** through [`Self::route_menu_command`].
    /// Called at the TOP of [`Self::render_frame`] (the sole redraw entry),
    /// processing the commands the File bar enqueued during the PREVIOUS frame's
    /// egui pass — before this frame's surface / window / encoder borrows are
    /// taken, so the `&mut self` handler calls don't contend with a live `&self`
    /// borrow. The host's continuous `request_redraw` ensures a command enqueued
    /// one frame is processed at the top of the next. No-op when render init has
    /// not populated the handoff (`menu_command_handoff == None`).
    pub(crate) fn drain_and_route_menu_commands(&mut self) {
        let Some(handoff) = self.menu_command_handoff.clone() else {
            return;
        };
        for cmd in handoff.drain() {
            self.route_menu_command(cmd);
        }
    }

    /// Route a single resolved [`Command`] **one-way** to its existing handler —
    /// the command sink shared by BOTH command sources, which is what makes the
    /// canonical menu (`rge_editor_ui::menus::default_menu`) the single source of
    /// truth for what each accelerator does:
    ///
    /// - the host→shell menu FIFO ([`Self::drain_and_route_menu_commands`]), and
    /// - the editor-shell keyboard accelerator path (W08.3): `window_event`
    ///   resolves a keystroke to its enabled `Command` via `keycode_to_shortcut` +
    ///   [`ResolveResult::enabled_command_for_shortcut`](rge_editor_ui::menus::ResolveResult::enabled_command_for_shortcut),
    ///   then routes it here. A keystroke and its menu item therefore dispatch the
    ///   identical `Command` when the entry is enabled (both resolve through the
    ///   same canonical menu); the parity guard (`lifecycle::accelerator`) pins
    ///   that `from_key_press` does not shadow the canonical menu-routed binds.
    ///
    /// MENUBAR-FILE-WIRING (Dispatch B) routes the File authoring-loop commands
    /// plus app Quit;
    /// A2 (MENUREGISTRY-EDITMENU) adds the Edit `Undo` / `Redo` commands — the bus
    /// undo/redo that the `Ctrl+Z` / `Ctrl+Y` keystrokes resolve to; A3
    /// (MENUREGISTRY-PLAYMENU) adds the Play `PlayStart` / `PlayPause` /
    /// `PlayStop` / `PlayStep` commands, routed to [`EditorShell::handle_button`]
    /// — the same PIE driver as the Space / Escape playback keys; A4
    /// (VIEWMENU-RESETCAMERA) adds the View camera commands, routed to
    /// [`EditorShell::reset_camera`], [`EditorShell::zoom_camera_in`], and
    /// [`EditorShell::zoom_camera_out`]. Any other `Command` is logged + ignored.
    ///
    /// `pub` so headless tests can drive a `Command` without synthesizing a winit
    /// `KeyEvent` or a menu click (mirrors [`EditorShell::handle_key_command`]).
    pub fn route_menu_command(&mut self, command: Command) {
        match command {
            Command::NewFile => self.handle_new_file_request(),
            Command::OpenFile => self.handle_open_request(),
            Command::Save => self.handle_save_request(),
            Command::SaveAs => self.handle_save_as_new_project_request(),
            Command::Close => self.handle_close_file_request(),
            Command::Quit => self.handle_quit_request(),
            // Edit menu (A2) — route to the bus undo/redo (the same path the
            // Ctrl+Z / Ctrl+Y keystrokes resolve to) and swallow the empty-stack
            // errors, so an Undo on a fresh editor is a no-op rather than
            // diagnostic spam.
            Command::Undo => match self.undo_command() {
                Ok(()) | Err(BusError::NothingToUndo) => {}
                Err(e) => tracing::warn!(
                    target: "rge::editor-shell::menu",
                    error = ?e,
                    "menu Undo dispatched but bus returned non-NothingToUndo error"
                ),
            },
            Command::Redo => match self.redo_command() {
                Ok(()) | Err(BusError::NothingToRedo) => {}
                Err(e) => tracing::warn!(
                    target: "rge::editor-shell::menu",
                    error = ?e,
                    "menu Redo dispatched but bus returned non-NothingToRedo error"
                ),
            },
            Command::SelectAll => self.select_all_entities(),
            Command::Cut => {
                self.cut_selected_entities();
            }
            Command::Copy => {
                self.copy_selected_entities();
            }
            Command::Paste => {
                self.paste_copied_entities();
            }
            Command::Delete => {
                self.delete_selected_entities();
            }
            Command::Duplicate => {
                self.duplicate_selected_entities();
            }
            // Play menu (A3) — route to the already-runtime-wired PIE driver
            // `handle_button`, the same one Space / Escape use. The menu items
            // are static, so one can be clicked in a state where its transition
            // is a no-op (e.g. Stop while Editing); that is benign and swallowed
            // (see `route_play_button`).
            Command::PlayStart => self.route_play_button(ToolbarButtonId::Play),
            Command::PlayPause => self.route_play_button(ToolbarButtonId::Pause),
            Command::PlayStop => self.route_play_button(ToolbarButtonId::Stop),
            Command::PlayStep => self.route_play_button(ToolbarButtonId::Step),
            // View menu (A4) — camera commands are infallible: Reset Camera
            // reframes to scene bounds (or the default pose), and zoom commands
            // move the eye along the current view vector without touching target.
            Command::ResetCamera => self.reset_camera(),
            Command::ZoomIn => self.zoom_camera_in(),
            Command::ZoomOut => self.zoom_camera_out(),
            other => {
                tracing::debug!(
                    target: "rge::editor-shell::menu",
                    command = %other.diagnostic_id(),
                    "menu command not routed (File New/Open/Save/Save-As/Close/Quit + Edit Undo/Redo/Select-All/Cut/Copy/Paste/Delete/Duplicate + Play Play/Pause/Stop/Step + View camera commands only)"
                );
            }
        }
    }

    /// Route a Play-menu click to the PIE driver [`EditorShell::handle_button`],
    /// swallowing the benign invalid-state transition errors. The Play menu items
    /// are STATIC (always present + clickable), so an item can be activated in a
    /// [`PlayState`] where its transition is a no-op — e.g. Stop / Pause while
    /// `Editing`, where `handle_button` returns `PlayStateError` BEFORE mutating
    /// anything. That is normal user behaviour (not an error), so it is logged at
    /// debug, not warn. Mirrors the swallow in `handle_playback_command` (Space /
    /// Escape).
    fn route_play_button(&mut self, button: ToolbarButtonId) {
        if let Err(e) = self.handle_button(button) {
            tracing::debug!(
                target: "rge::editor-shell::menu",
                error = ?e,
                ?button,
                "Play menu item clicked where the PIE transition is a no-op"
            );
        }
    }

    /// Render one frame on `WindowEvent::RedrawRequested` (sub-δ.1.B).
    ///
    /// Acquires the next surface texture, records a single render pass
    /// that clears to [`DEFAULT_CLEAR`] and draws the cuboid mesh with
    /// the [`LitMeshPipeline`] + camera/light/material bind groups,
    /// presents, and schedules the next redraw.
    ///
    /// ISSUE-249 — when Phase 2 of [`Self::init_render_state`] did NOT
    /// construct the cuboid pipeline / frame graph (empty-world
    /// `--scene` launch), this method dispatches to
    /// [`Self::render_frame_egui_only`] instead. That branch
    /// acquires the surface texture, clears the target, publishes the
    /// inspector + save-status handoffs, runs `EguiHost::render`,
    /// submits, presents, and returns `true`. It never touches the
    /// cuboid pipeline,
    /// frame-graph, or depth-view rendering.
    ///
    /// Returns `false` when the render path is not initialised at all
    /// (no winit window / surface, e.g. pre-`resumed` shell); caller
    /// should fall through to existing W03 behaviour.
    pub(crate) fn render_frame(&mut self) -> bool {
        // MENUBAR-FILE-WIRING — drain + route any menu commands the File bar
        // enqueued during the PREVIOUS frame's egui pass, BEFORE this frame's
        // borrows (surface frame / window / encoder) are taken, so the
        // `&mut self` handlers don't contend with a live `&self` borrow.
        // `render_frame` is the sole redraw entry (it early-returns
        // `render_frame_egui_only` for the Phase-1 path), so this single drain
        // covers both render paths; the host's continuous `request_redraw`
        // guarantees a command enqueued one frame is processed at the top of the
        // next. No-op until render init populates the handoff.
        self.drain_and_route_menu_commands();

        // ISSUE-249 — Phase 1-only egui presenter. `init_render_state`
        // populates `pipeline` and `compiled_frame_graph` together as
        // part of Phase 2; either being absent means Phase 2 did not
        // run (empty-world launch). Take the egui-only path so the
        // dock + Inspector tab + bottom save-status bar chrome paint on a
        // cleared target.
        if self.pipeline.is_none() || self.compiled_frame_graph.is_none() {
            return self.render_frame_egui_only();
        }

        // Phase A — depth-view prep via the frame-graph substrate. Done
        // BEFORE surface acquire so a `build_resource_map` failure
        // skips without wasting a surface frame, matching the
        // pre-extraction control flow.
        let depth_view = match self.acquire_depth_view() {
            DepthViewOutcome::Uninitialized => return false,
            DepthViewOutcome::RecoverableSkip => {
                if let Some(w) = self.window.as_ref() {
                    w.request_redraw();
                }
                return true;
            }
            DepthViewOutcome::Acquired(view) => view,
        };

        // Phase B — winit-bound surface + window fields.
        let Some(surface_ctx) = self.surface_ctx.as_ref() else {
            return false;
        };
        let Some(window) = self.window.as_ref() else {
            return false;
        };

        // Phase C — acquire the next surface texture. Skip on
        // Timeout/Occluded/Outdated/Lost/Validation; request another
        // redraw so the resize handler / wgpu reconfigure can recover.
        // wgpu 29's `get_current_texture` returns the enum
        // `CurrentSurfaceTexture` (NOT `Result<…>`); see
        // wgpu-29.0.3/src/api/surface_texture.rs:55.
        let frame = match surface_ctx.surface().get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            other => {
                tracing::warn!(
                    target: "rge::editor-shell::lifecycle",
                    "skip frame: {other:?}"
                );
                window.request_redraw();
                return true;
            }
        };
        let target_view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // Phase D — create the per-frame encoder. Production drives
        // BOTH the cuboid pass AND the optional Phase 9 egui pass into
        // a single encoder + single submit; the harness in
        // `render_frame_e2e_perf.rs` still uses
        // [`Self::render_frame_to_target`] which internally creates
        // its own encoder + submits (egui-free measurement path).
        let Some(gfx_ctx_for_encode) = self.gfx_ctx.as_ref() else {
            return false;
        };
        let mut encoder =
            gfx_ctx_for_encode
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("rge-editor.frame.encoder"),
                });

        // Phase E — encode the cuboid + sub-ε highlight pass.
        if !self.encode_main_pass(&mut encoder, &target_view, &depth_view) {
            return false;
        }

        // Phase F.1 — dispatch C inspector snapshot publish. Compute
        // the snapshot via the existing `&self`-only accessor BEFORE
        // the disjoint `&mut self.egui_host` borrow below, so the
        // `inspector_snapshot()` reads don't contend with the host's
        // `render()` &mut. Publish through the held
        // `Arc<InspectorHandoff>` (set in [`Self::init_render_state`]
        // alongside `egui_host`); the host's [`InspectorTabBody`]
        // reads the same handoff during its dock-tab render in
        // Phase F.2 below.
        //
        // No-op when `inspector_handoff == None` (no render init has
        // run yet — pre-resumed shells, headless tests).
        if let Some(handoff) = self.inspector_handoff.as_ref() {
            let snapshot = self.inspector_snapshot();
            handoff.publish(Arc::new(snapshot));
        }

        // EDITOR-SAVE-STATUS-INDICATOR — sibling publish of the save-status
        // snapshot (open save source file name + dirty flag) through the held
        // `Arc<SaveStatusHandoff>`, so the host's bottom status bar sees this
        // frame's state. Same `&self`-only borrow, disjoint from the host's
        // `&mut self.egui_host` render borrow below. No-op when render init
        // has not run (pre-resumed shells, headless tests).
        if let Some(handoff) = self.save_status_handoff.as_ref() {
            let snapshot = self.save_status_snapshot();
            handoff.publish(Arc::new(snapshot));
        }
        // MENU-DYNAMIC-RESOLVE — sibling publish of the live `PredicateContext`
        // through the held `Arc<PredicateContextHandoff>`, so the host re-resolves
        // the menu and greys disabled items this frame. Same `&self`-only borrow,
        // disjoint from the host's `&mut self.egui_host` render borrow below.
        if let Some(handoff) = self.predicate_context_handoff.as_ref() {
            let ctx = self.predicate_context();
            handoff.publish(Arc::new(ctx));
        }

        // Phase F.2 — optional egui pass into the same encoder.
        // `LoadOp::Load` preserves the cuboid pixels; egui paints on
        // top. The host owns its [`egui_dock::DockState`] + tab
        // viewer dispatch internally (dispatch C); there is no
        // caller-supplied UI closure. When `egui_host` is `None`
        // (existing tests + pre-resumed shell state) the egui pass is
        // skipped entirely — byte-identical pre-host behaviour.
        //
        // Borrow-split: `self.egui_host.as_mut()`, `self.window.as_ref()`,
        // and `self.gfx_ctx.as_ref()` all touch disjoint fields. The
        // Rust borrow checker (NLL) ends the `gfx_ctx_for_encode`
        // immutable borrow at the end of the preceding statement and
        // the publish path's `&self` borrows above; taking a fresh
        // `&mut self.egui_host` here is therefore safe.
        if let (Some(host), Some(window_ref), Some(gfx_ctx)) = (
            self.egui_host.as_mut(),
            self.window.as_ref(),
            self.gfx_ctx.as_ref(),
        ) {
            host.render(
                window_ref,
                gfx_ctx.device(),
                gfx_ctx.queue(),
                &mut encoder,
                &target_view,
            );
        }

        // Phase G — submit + present + schedule next redraw.
        let Some(gfx_ctx_for_submit) = self.gfx_ctx.as_ref() else {
            return false;
        };
        gfx_ctx_for_submit
            .queue()
            .submit(std::iter::once(encoder.finish()));
        frame.present();
        window.request_redraw();
        true
    }

    /// ISSUE-249 — egui-only frame presenter for Phase 1-only state.
    ///
    /// Activated by [`Self::render_frame`] when the cuboid pipeline /
    /// frame graph were not constructed (empty-world `--scene`
    /// launch). Acquires the surface texture, issues a clear-only
    /// pass to [`DEFAULT_CLEAR`], publishes the inspector + save-status
    /// handoffs, runs [`rge_editor_egui_host::EguiHost::render`] against the
    /// same encoder, submits, presents, and returns `true`.
    ///
    /// Does NOT touch the cuboid pipeline, the frame-graph substrate,
    /// or the depth-view path — all four fields are `None` / empty in
    /// Phase 1-only state and any reference would `Option::unwrap` on
    /// `None`.
    ///
    /// Returns `false` when Phase 1 itself didn't complete (no
    /// `surface_ctx` / `window` / `gfx_ctx` / `egui_host`), so the
    /// `EguiHost`-exists postcondition documented on
    /// [`Self::render_frame`] holds: an egui-only presented frame
    /// implies `egui_host.is_some()`.
    fn render_frame_egui_only(&mut self) -> bool {
        let Some(surface_ctx) = self.surface_ctx.as_ref() else {
            return false;
        };
        let Some(window) = self.window.as_ref() else {
            return false;
        };
        let Some(gfx_ctx) = self.gfx_ctx.as_ref() else {
            return false;
        };
        if self.egui_host.is_none() {
            return false;
        }

        // Acquire the next surface texture. Same skip-on-transient
        // policy as the cuboid path: request another redraw so the
        // surface reconfigure / resize handler can recover.
        let frame = match surface_ctx.surface().get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t)
            | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            other => {
                tracing::warn!(
                    target: "rge::editor-shell::lifecycle",
                    "skip egui-only frame: {other:?}"
                );
                window.request_redraw();
                return true;
            }
        };
        let target_view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder =
            gfx_ctx
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("rge-editor.frame.encoder.egui-only"),
                });

        // Clear pass — the cuboid path's `encode_main_pass` would have
        // issued `LoadOp::Clear(DEFAULT_CLEAR)` as part of the main
        // pass; here we issue a clear-only pass because there is no
        // main geometry to draw. The subsequent egui pass uses
        // `LoadOp::Load`, so the clear color shows through wherever
        // the dock area is transparent.
        {
            let _clear_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rge-editor.frame.egui-only.clear"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(DEFAULT_CLEAR),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
        }

        // Dispatch C inspector snapshot publish — same shape as the
        // cuboid path so the Inspector tab body sees a live snapshot
        // on every Phase 1-only frame too. `&self` borrow is
        // released before the disjoint `&mut self.egui_host` borrow
        // below.
        if let Some(handoff) = self.inspector_handoff.as_ref() {
            let snapshot = self.inspector_snapshot();
            handoff.publish(Arc::new(snapshot));
        }

        // EDITOR-SAVE-STATUS-INDICATOR — sibling publish of the save-status
        // snapshot (open save source file name + dirty flag) through the held
        // `Arc<SaveStatusHandoff>`, so the host's bottom status bar sees this
        // frame's state. Same `&self`-only borrow, disjoint from the host's
        // `&mut self.egui_host` render borrow below. No-op when render init
        // has not run (pre-resumed shells, headless tests).
        if let Some(handoff) = self.save_status_handoff.as_ref() {
            let snapshot = self.save_status_snapshot();
            handoff.publish(Arc::new(snapshot));
        }
        // MENU-DYNAMIC-RESOLVE — sibling publish of the live `PredicateContext`
        // through the held `Arc<PredicateContextHandoff>`, so the host re-resolves
        // the menu and greys disabled items this frame. Same `&self`-only borrow,
        // disjoint from the host's `&mut self.egui_host` render borrow below.
        if let Some(handoff) = self.predicate_context_handoff.as_ref() {
            let ctx = self.predicate_context();
            handoff.publish(Arc::new(ctx));
        }

        // Egui pass into the same encoder. `egui_host` is `Some` per
        // the early guard above; the `if let` here re-borrows
        // disjointly from the immutable reads above.
        if let (Some(host), Some(window_ref), Some(gfx_ctx_ref)) = (
            self.egui_host.as_mut(),
            self.window.as_ref(),
            self.gfx_ctx.as_ref(),
        ) {
            host.render(
                window_ref,
                gfx_ctx_ref.device(),
                gfx_ctx_ref.queue(),
                &mut encoder,
                &target_view,
            );
        }

        let Some(gfx_ctx_for_submit) = self.gfx_ctx.as_ref() else {
            return false;
        };
        gfx_ctx_for_submit
            .queue()
            .submit(std::iter::once(encoder.finish()));
        frame.present();
        window.request_redraw();
        true
    }

    /// Shared post-surface render-state setup. Consumes a constructed
    /// [`GfxContext`] plus the chosen color target format + size and
    /// builds Steps 4–6 of `init_render_state` (camera / material /
    /// highlight material / light / pipeline / pool / frame-graph /
    /// mesh) before stashing every produced field on `self`. Called
    /// from production [`Self::init_render_state`] (after Steps 1–3
    /// build the winit window + `SurfaceContext`) and from the
    /// crate-local [`Self::init_render_state_headless`] (which skips
    /// winit Steps 1 + 3 entirely). Centralising this avoids parity
    /// drift between the two init paths.
    fn init_render_state_post_surface(
        &mut self,
        gfx_ctx: GfxContext,
        format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        let aspect = (width.max(1) as f32) / (height.max(1) as f32);

        // Step 4 — bind groups (camera UBO + per-mesh materials + light).
        let gfx_camera = GfxCamera::new(&gfx_ctx).map_err(|e| format!("gfx camera: {e:?}"))?;
        gfx_camera.update(
            &gfx_ctx,
            self.editor_camera.view_proj(aspect),
            glam::Mat4::IDENTITY,
        );

        // Dispatch K — resolve the per-mesh `base_color` sequence
        // BEFORE building the materials Vec. CAD path: single white
        // default (matches pre-dispatch-K hardcoded behaviour). glTF
        // path: clone the prebuilt sequence (one entry per
        // mesh-bearing entity, populated by the editor binary's
        // `load_all_glb_meshes`). The two sides are mutually
        // exclusive — at most one is non-empty.
        let base_colors_for_meshes: Vec<[f32; 4]> = if !self.prebuilt_render_base_colors.is_empty()
        {
            self.prebuilt_render_base_colors.clone()
        } else {
            // CAD path or untextured default — single white material.
            vec![[1.0, 1.0, 1.0, 1.0]]
        };

        // Dispatch M2 — per-mesh texture payloads parallel to the
        // base_colors vec. CAD path: no textures, length-1 `None`
        // vec. glTF path: the editor binary's
        // `load_all_glb_meshes_with_textures` returns
        // `Vec<Option<(u32, u32, Vec<u8>)>>` aligned to the meshes,
        // and the constructor stores it on
        // `prebuilt_render_base_textures`. `materials.len()` is
        // derived from `base_colors_for_meshes.len()`, which equals
        // the meshes vec length on the glTF path and is 1 on the
        // CAD path — the texture sequence below mirrors that
        // exactly so the per-index lookup stays safe.
        let textures_for_meshes: Vec<Option<(u32, u32, Vec<u8>)>> =
            if !self.prebuilt_render_base_textures.is_empty() {
                self.prebuilt_render_base_textures.clone()
            } else {
                // CAD path (or empty glTF — defensive): single `None`,
                // so the cuboid demo material remains the 1×1 white
                // placeholder.
                vec![None]
            };

        // Build one `Material` per upcoming mesh. Each owns its own
        // texture + sampler + bind group; the UBO is then refreshed
        // with the per-mesh base_color via `update_color`. The bind-
        // group LAYOUT is identical across entries (all materials
        // use the same `Material::new` constructor; texture
        // dimensions don't affect the layout shape) so the
        // `LitMeshPipeline` built below can rebind any entry without
        // re-validation.
        //
        // When `textures_for_meshes[i]` is `Some((w, h, pixels))`,
        // `Material::new` is called with the owned RGBA8 bytes —
        // the resulting texture binding samples the real image.
        // When `None`, the existing `WHITE_1X1_RGBA` placeholder is
        // used, preserving the pre-M2 single-colour Lambert+Phong
        // result modulated by `base_color`.
        let mut materials: Vec<Material> = Vec::with_capacity(base_colors_for_meshes.len());
        for (i, base_color) in base_colors_for_meshes.iter().enumerate() {
            let m = match textures_for_meshes.get(i).and_then(Option::as_ref) {
                Some((width, height, pixels)) => {
                    Material::new(&gfx_ctx, pixels, *width, *height)
                        .map_err(|e| format!("material[{i}] (textured {width}x{height}): {e:?}"))?
                }
                None => Material::new(&gfx_ctx, &WHITE_1X1_RGBA, 1, 1)
                    .map_err(|e| format!("material[{i}] (placeholder 1x1): {e:?}"))?,
            };
            m.update_color(&gfx_ctx, glam::Vec4::from_array(*base_color), DEFAULT_PHONG);
            materials.push(m);
        }
        // The materials Vec is non-empty here: the CAD branch above
        // always pushes one entry, and the glTF branch is only taken
        // when `prebuilt_render_base_colors` is non-empty (which the
        // length-invariant assert in
        // `with_render_meshes_and_base_colors` ties to a non-empty
        // mesh Vec). materials[0] is always a valid layout source
        // for pipeline construction below + for any future
        // bind-group-layout consumer in the highlight path.
        debug_assert!(!materials.is_empty(), "materials populated above");

        // sub-ε: a second `Material` for the highlight overlay. Same
        // bind-group layout as the main materials (so the existing
        // `LitMeshPipeline` accepts it at @group(2)); the UBO is then
        // refreshed with `HIGHLIGHT_COLOR` via `update_color`.
        let highlight_material = Material::new(&gfx_ctx, &WHITE_1X1_RGBA, 1, 1)
            .map_err(|e| format!("highlight material: {e:?}"))?;
        highlight_material.update_color(&gfx_ctx, HIGHLIGHT_COLOR, HIGHLIGHT_PHONG);
        let light = DirectionalLight::new(&gfx_ctx).map_err(|e| format!("light: {e:?}"))?;
        light.update(&gfx_ctx, default_light_direction(), glam::Vec3::ONE);

        // Step 5 — pipeline against the chosen color format, depth-ready
        // per Phase 6 sub-α + sub-β. The `lit_mesh_depth_state()` choice
        // (`LessEqual` + `depth_write_enabled: false`) is documented at
        // the helper's site above. Sources the material bind-group
        // layout from `materials[0]` — every entry shares the same
        // layout (see comment above).
        let pipeline = LitMeshPipeline::new_with_depth(
            &gfx_ctx,
            gfx_camera.bind_group_layout(),
            light.bind_group_layout(),
            materials[0].bind_group_layout(),
            format,
            Some(lit_mesh_depth_state()),
        )
        .map_err(|e| format!("pipeline: {e:?}"))?;

        // Step 5b — frame-graph substrate plumbing (sub-β). Construct
        // the per-frame transient-texture pool, the (unused-but-
        // required-by-API) transient-buffer pool, and the compiled
        // single-pass `lit_mesh` graph. Per ADR-118 / dispatch 122 the
        // substrate-discipline rule "pass-record sites must NOT call
        // pool.acquire directly" is preserved: every code path goes
        // through `build_resource_map` at frame start.
        let texture_pool = TexturePool::new();
        let buffer_pool = BufferPool::new();
        let compiled_frame_graph = build_lit_mesh_compiled_frame_graph(width, height);

        // Step 6 — source the [`RenderMesh`] sequence from either the
        // CAD projection (cuboid demo path: one mesh) OR the
        // prebuilt-mesh Vec (`--glb` render-only path: N meshes).
        //
        // Dispatch G / I: branches based on which side was populated
        // at construction. The two sides are mutually exclusive per
        // the `with_render_meshes` / `with_world_projection_graph`
        // constructors — neither stores both. The early-return guard
        // in [`Self::init_render_state`] ensures at least one side
        // contributes a mesh by the time we reach here.
        let render_meshes: Vec<rge_brep_render::RenderMesh> = if !self
            .prebuilt_render_meshes
            .is_empty()
        {
            // `--glb` mode: meshes were built by the editor
            // binary from glTF MeshAsset data. Clone each into a
            // local Vec; the upload step below converts each to
            // a `LitMesh`. Cloning is cheap relative to GPU
            // upload cost; the prebuilt field stays populated
            // for diagnostics / future re-upload (e.g. on
            // device-lost reconstruction).
            self.prebuilt_render_meshes.clone()
        } else {
            let entity = self.cad_entity.expect(
                    "init_render_state_post_surface: cad_entity must be Some when prebuilt meshes is empty — caller bails on neither",
                );
            let projection = self.projection.as_ref().expect(
                    "init_render_state_post_surface: projection must be Some when prebuilt meshes is empty — caller bails on neither",
                );
            let cad_world = self.cad_world.as_ref().expect(
                    "init_render_state_post_surface: cad_world must be Some when prebuilt meshes is empty — caller bails on neither",
                );
            vec![projection
                .render_mesh_for(entity, cad_world)
                .ok_or_else(|| "render_mesh_for returned None for the cuboid entity".to_string())?]
        };
        let mut uploaded_meshes: Vec<LitMesh> = Vec::with_capacity(render_meshes.len());
        for (idx, mesh) in render_meshes.iter().enumerate() {
            let lit = LitMesh::from_render_mesh(&gfx_ctx, mesh)
                .map_err(|e| format!("LitMesh::from_render_mesh[{idx}]: {e:?}"))?;
            uploaded_meshes.push(lit);
        }

        // Dispatch K invariant: per-mesh materials must align 1:1 with
        // uploaded meshes. The CAD path produces (1 mesh, 1 material);
        // the glTF path's `with_render_meshes_and_base_colors`
        // constructor enforces the input length pair, and both vecs
        // are sized off the same `base_colors_for_meshes` / meshes
        // source above. A mismatch here is a substrate bug.
        debug_assert_eq!(
            uploaded_meshes.len(),
            materials.len(),
            "uploaded_meshes ({}) and materials ({}) must align 1:1",
            uploaded_meshes.len(),
            materials.len(),
        );

        // Stash all post-surface fields. Winit-bound bits (window,
        // surface_ctx) are owned by the caller and stashed there.
        self.gfx_ctx = Some(gfx_ctx);
        self.pipeline = Some(pipeline);
        self.gfx_camera = Some(gfx_camera);
        self.materials = materials;
        self.highlight_material = Some(highlight_material);
        self.light = Some(light);
        self.meshes = uploaded_meshes;
        self.texture_pool = Some(texture_pool);
        self.buffer_pool = Some(buffer_pool);
        self.compiled_frame_graph = Some(compiled_frame_graph);

        Ok(())
    }

    /// Asset hot-reload — swap [`Self::meshes`] + [`Self::materials`] in
    /// place using the supplied buffer inputs, reusing the existing
    /// `gfx_ctx` + pipeline + camera + light + frame-graph state.
    ///
    /// Called by [`Self::handle_asset_reload`] after the R-key handler
    /// has invoked the [`crate::AssetReloadHook`] and received fresh
    /// vecs. Direct tests can call this method without going through
    /// the hook, simulating a successful reload deterministically.
    ///
    /// # Atomic-swap semantics
    ///
    /// Builds the new materials + new lit-meshes FIRST; only after
    /// both Vecs are fully constructed does the swap onto
    /// `self.materials` / `self.meshes` happen. A GPU-upload failure
    /// at material[k] or mesh[k] returns `Err(...)` with the prior
    /// state intact — the caller retains the previous frame.
    ///
    /// # Preserved state (NOT touched)
    ///
    /// - `pipeline`, `gfx_camera`, `light` — same Lambert+Phong
    ///   pipeline + per-frame UBO refresh keeps working unchanged.
    /// - `surface_ctx`, `window` — winit window + wgpu surface stay
    ///   alive across reload.
    /// - `texture_pool`, `buffer_pool`, `compiled_frame_graph` —
    ///   frame-graph substrate is reused as-is.
    /// - `editor_camera` — user-orbit state preserved (no reframe
    ///   per dispatch contract).
    ///
    /// # Cleared state
    ///
    /// `highlight_index_buffer` is cleared: face indices from the
    /// previous mesh are no longer valid against the new geometry.
    /// CAD-cuboid path never reaches this method (the R-key handler
    /// gates on `glb_source_path.is_some()`), so highlight clearing
    /// is purely defensive in render-only mode.
    ///
    /// # Errors
    ///
    /// - `"PIE state X; reload only allowed in Editing"` — pressed
    ///   R while in Playing or Paused. Dispatch contract: no
    ///   mid-PIE asset swaps (would conflict with snapshot/restore).
    /// - `"render state not initialized"` — called before
    ///   `init_render_state` / `init_render_state_headless`
    ///   succeeded. Should never fire from the production R-key path
    ///   because the editor's first frame initialises render state
    ///   before any winit input can arrive.
    /// - `"meshes ({}) and base_colors ({}) length mismatch"` /
    ///   `"meshes ({}) and textures ({}) length mismatch"` — caller
    ///   contract violation; the hook impl must return aligned
    ///   vecs.
    /// - `"empty mesh set"` — defensive; matches the
    ///   `encode_main_pass` `meshes.is_empty()` skip-guard.
    /// - `"reload material[k]: …"` / `"reload LitMesh[k]: …"` —
    ///   `Material::new` or `LitMesh::from_render_mesh` failed
    ///   (out-of-VRAM, dimension violation, etc.).
    pub(crate) fn reload_render_assets(
        &mut self,
        meshes: Vec<rge_brep_render::RenderMesh>,
        base_colors: Vec<[f32; 4]>,
        textures: Vec<Option<(u32, u32, Vec<u8>)>>,
    ) -> Result<(), String> {
        if self.play_state() != PlayState::Editing {
            return Err(format!(
                "reload_render_assets: PIE state is {}; reload only allowed in Editing",
                self.play_state().label()
            ));
        }
        if meshes.len() != base_colors.len() {
            return Err(format!(
                "reload_render_assets: meshes ({}) and base_colors ({}) length mismatch",
                meshes.len(),
                base_colors.len(),
            ));
        }
        if meshes.len() != textures.len() {
            return Err(format!(
                "reload_render_assets: meshes ({}) and textures ({}) length mismatch",
                meshes.len(),
                textures.len(),
            ));
        }
        if meshes.is_empty() {
            return Err("reload_render_assets: empty mesh set; reload requires >=1 mesh".into());
        }

        let Some(gfx_ctx) = self.gfx_ctx.as_ref() else {
            return Err(
                "reload_render_assets: render state not initialized; reload only fires after the first frame"
                    .into(),
            );
        };

        let mut new_materials: Vec<Material> = Vec::with_capacity(meshes.len());
        for (i, base_color) in base_colors.iter().enumerate() {
            let m = match textures[i].as_ref() {
                Some((w, h, pixels)) => Material::new(gfx_ctx, pixels, *w, *h)
                    .map_err(|e| format!("reload material[{i}] textured: {e:?}"))?,
                None => Material::new(gfx_ctx, &WHITE_1X1_RGBA, 1, 1)
                    .map_err(|e| format!("reload material[{i}] placeholder: {e:?}"))?,
            };
            m.update_color(gfx_ctx, glam::Vec4::from_array(*base_color), DEFAULT_PHONG);
            new_materials.push(m);
        }

        let mut new_meshes: Vec<LitMesh> = Vec::with_capacity(meshes.len());
        for (i, mesh) in meshes.iter().enumerate() {
            let lit = LitMesh::from_render_mesh(gfx_ctx, mesh)
                .map_err(|e| format!("reload LitMesh[{i}]: {e:?}"))?;
            new_meshes.push(lit);
        }

        // Atomic swap — only after both vec builds succeeded.
        self.materials = new_materials;
        self.meshes = new_meshes;
        self.prebuilt_render_meshes = meshes;
        self.prebuilt_render_base_colors = base_colors;
        self.prebuilt_render_base_textures = textures;
        self.highlight_index_buffer = None;

        Ok(())
    }

    /// Acquire the per-frame transient depth attachment via the
    /// frame-graph substrate. Returns a 3-way [`DepthViewOutcome`] so
    /// production [`Self::render_frame`] can distinguish "render state
    /// uninitialised" from "`build_resource_map` skip" from success
    /// without losing the pre-extraction control-flow shape.
    ///
    /// Shared with the crate-local `render_frame_e2e_perf` harness so
    /// the perf path exercises the same `tex_pool.begin_frame()` +
    /// `buf_pool.begin_frame()` + `build_resource_map` substrate that
    /// production performs every frame.
    pub(crate) fn acquire_depth_view(&mut self) -> DepthViewOutcome {
        let Some(gfx_ctx) = self.gfx_ctx.as_ref() else {
            return DepthViewOutcome::Uninitialized;
        };
        let Some(compiled) = self.compiled_frame_graph.as_ref() else {
            return DepthViewOutcome::Uninitialized;
        };
        let Some(tex_pool) = self.texture_pool.as_mut() else {
            return DepthViewOutcome::Uninitialized;
        };
        let Some(buf_pool) = self.buffer_pool.as_mut() else {
            return DepthViewOutcome::Uninitialized;
        };
        tex_pool.begin_frame();
        buf_pool.begin_frame();
        let map = match build_resource_map(compiled, gfx_ctx.device(), tex_pool, buf_pool) {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(
                    target: "rge::editor-shell::lifecycle",
                    "skip frame: build_resource_map: {e:?}"
                );
                return DepthViewOutcome::RecoverableSkip;
            }
        };
        let depth_arc = Arc::clone(
            map.texture_map
                .get(&DEPTH_RESOURCE_ID)
                .expect("well-formed single-pass FrameGraph guarantees DEPTH_RESOURCE_ID present in texture_map"),
        );
        DepthViewOutcome::Acquired(depth_arc.create_view(&wgpu::TextureViewDescriptor::default()))
    }

    /// Encode the lit-mesh frame against the provided color + depth
    /// target views and submit the command buffer. Production
    /// [`Self::render_frame`] wraps this with the winit-bound surface
    /// acquire + present + `request_redraw`. The crate-local
    /// `render_frame_e2e_perf` harness drives this directly against
    /// an offscreen color target so it measures encode/submit cost
    /// minus surface acquire/present.
    ///
    /// Returns `false` only when the encode-body field state is
    /// uninitialised (any of `gfx_ctx` / `pipeline` / `gfx_camera` /
    /// `light` / `material` / `cuboid_mesh` is `None`). Returns `true`
    /// on successful encode + submit.
    ///
    /// Caller is responsible for surface acquire / present /
    /// `request_redraw` (production) or the offscreen target's
    /// lifetime + readback policy (harness).
    ///
    /// The body is the same render-pass record that the pre-extraction
    /// `render_frame` performed: clear-color, depth-stencil with
    /// `Clear(1.0)`, three bind-group binds, one `draw_indexed` for the
    /// main cuboid plus the optional sub-ε highlight overlay.
    /// Cuboid-only render wrapper used by the
    /// `render_frame_e2e_perf` test harness. Creates its own encoder,
    /// calls [`Self::encode_main_pass`], submits, and returns. Does
    /// NOT include the Phase 9 egui pass — the harness measures
    /// cuboid-render perf in isolation.
    ///
    /// `#[cfg_attr(not(test), allow(dead_code))]` because the lib
    /// build no longer reaches this function in production (Phase 9
    /// production goes through [`Self::render_frame`] which calls
    /// `encode_main_pass` + optional egui pass on the same encoder).
    /// Test builds reach it via `render_frame_e2e_perf.rs`.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn render_frame_to_target(
        &self,
        target_view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
    ) -> bool {
        let Some(gfx_ctx) = self.gfx_ctx.as_ref() else {
            return false;
        };
        let mut encoder =
            gfx_ctx
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("rge-editor.frame.encoder"),
                });
        if !self.encode_main_pass(&mut encoder, target_view, depth_view) {
            return false;
        }
        gfx_ctx.queue().submit(std::iter::once(encoder.finish()));
        true
    }

    /// Encode the cuboid (or N glTF meshes) + sub-ε highlight overlay
    /// render pass into the caller-provided `encoder`. Pure encode —
    /// does NOT create the encoder, does NOT submit, does NOT
    /// present. Used by both [`Self::render_frame_to_target`] (the
    /// harness wrapper) and [`Self::render_frame`] (the production
    /// path, which appends the Phase 9 egui pass into the same
    /// encoder before submitting).
    ///
    /// Returns `false` when any of the required render-state fields
    /// (`pipeline` / `gfx_camera` / `light` / `material`) is `None`,
    /// or when [`Self::meshes`] is empty — matching the pre-dispatch-I
    /// skip-frame contract (CAD path: one mesh; glTF path: ≥1 mesh).
    ///
    /// # Dispatch-I multi-mesh draw loop
    ///
    /// The pipeline + camera + light + material bind groups bind ONCE
    /// per frame; the loop over [`Self::meshes`] performs one
    /// `set_vertex_buffer` + `set_index_buffer` + `draw_indexed` per
    /// mesh. This is the standard "N draws, same pipeline state"
    /// shape — minimal per-mesh overhead, no extra render passes.
    ///
    /// Highlight overlay (sub-ε) stays CAD-only by construction:
    /// `highlight_index_buffer` is set only by `handle_left_click`
    /// in CAD mode (face-pick is a no-op for glTF assets). The
    /// overlay binds `meshes[0]`'s vertex buffer with a tinted
    /// material; in glTF mode the overlay is never wired up so the
    /// `if let Some(...)` skips it entirely.
    pub(crate) fn encode_main_pass(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        target_view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
    ) -> bool {
        let Some(pipeline) = self.pipeline.as_ref() else {
            return false;
        };
        let Some(gfx_camera) = self.gfx_camera.as_ref() else {
            return false;
        };
        let Some(light) = self.light.as_ref() else {
            return false;
        };
        if self.meshes.is_empty() {
            return false;
        }
        // Dispatch K — the per-mesh materials Vec must be populated and
        // aligned 1:1 with `self.meshes`. `init_render_state_post_surface`
        // enforces this via the `debug_assert_eq!` at field-stash time;
        // re-checking at draw time covers any future construction path
        // that might bypass init.
        if self.materials.len() != self.meshes.len() {
            return false;
        }

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rge-editor.frame"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(DEFAULT_CLEAR),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                // Phase 6 sub-β — transient depth attachment passed in
                // from [`Self::acquire_depth_view`]. Matches the
                // pipeline's [`lit_mesh_depth_state`] (`LessEqual` +
                // `depth_write_enabled: false`); the depth buffer stays
                // at the `Clear(1.0)` value for the entire frame and
                // every fragment passes `LessEqual` against 1.0 — depth
                // is functionally a no-op for the cuboid + overlay,
                // matching the pre-sub-β no-depth visual behavior
                // exactly while consuming the transient substrate
                // end-to-end. Lifts the cuboid+overlay Z-fight that
                // a non-`false`-write depth state would introduce
                // (regression prevention); does NOT prove a
                // user-visible Z-fight fix — that claim requires
                // sub-γ measurement or a visual harness.
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            pass.set_pipeline(pipeline.pipeline());
            pass.set_bind_group(0, gfx_camera.bind_group(), &[]);
            pass.set_bind_group(1, light.bind_group(), &[]);

            // Dispatch I + K — loop over every mesh. Pipeline +
            // camera + light bind groups stay bound for the whole
            // sequence; the material bind group (@group(2)) and the
            // vertex / index buffers swap per mesh.
            //
            // The CAD-cuboid path puts exactly ONE mesh here
            // (`meshes = [cuboid_lit_mesh]`, `materials = [white]`)
            // so this loop runs once and the behaviour matches the
            // pre-dispatch-K single-draw shape with one
            // set_bind_group(2) per draw instead of one before the
            // loop. The `--glb` path puts N meshes + N materials
            // here, one per glTF primitive entity; the loop draws
            // them in scene-entity order, each tinted by its
            // `base_color`.
            for (i, mesh) in self.meshes.iter().enumerate() {
                pass.set_bind_group(2, self.materials[i].bind_group(), &[]);
                pass.set_vertex_buffer(0, mesh.vertex_buffer().buffer().slice(..));
                if let Some(ib) = mesh.index_buffer() {
                    pass.set_index_buffer(ib.buffer().slice(..), ib.index_format());
                    pass.draw_indexed(0..ib.index_count(), 0, 0..1);
                } else {
                    pass.draw(0..mesh.vertex_buffer().vertex_count(), 0..1);
                }
            }

            // Sub-ε — selection highlight overlay. Reuses the same
            // `LitMeshPipeline` + camera/light bind groups + vertex
            // buffer; only swaps the @group(2) material bind group and
            // the index buffer. Purely additive: when either field is
            // `None`, the if-let skips the overlay and the main mesh
            // (CAD cuboid) renders unchanged.
            //
            // The overlay's vertex buffer comes from `meshes[0]`
            // (the only mesh in CAD mode); in glTF mode the highlight
            // fields are never set (face-pick is a no-op there) so
            // this branch is silently skipped. The `[0]` index is safe
            // because the `self.meshes.is_empty()` guard above
            // ensured at least one mesh exists.
            //
            // Post sub-β: the depth attachment is now populated (see
            // the `depth_stencil_attachment` block above) and the
            // shared `LitMeshPipeline` carries
            // `DepthStateKey { LessEqual, depth_write_enabled: false }`
            // — the overlay's same-position-as-cuboid geometry passes
            // depth-test against the Clear(1.0) buffer (every fragment
            // <= 1.0) without writing, so render order (overlay second)
            // determines visibility on shared pixels. The Z-fight that
            // a `depth_write_enabled: true` pipeline would produce here
            // is structurally prevented.
            if let (Some(highlight_mat), Some(highlight_ib)) = (
                self.highlight_material.as_ref(),
                self.highlight_index_buffer.as_ref(),
            ) {
                let primary_mesh = &self.meshes[0];
                pass.set_vertex_buffer(0, primary_mesh.vertex_buffer().buffer().slice(..));
                pass.set_bind_group(2, highlight_mat.bind_group(), &[]);
                pass.set_index_buffer(highlight_ib.buffer().slice(..), highlight_ib.index_format());
                pass.draw_indexed(0..highlight_ib.index_count(), 0, 0..1);
            }
        }

        true
    }

    /// Crate-local headless render-state initializer for the
    /// `render_frame_e2e_perf` harness. Skips winit Steps 1 + 3 from
    /// [`Self::init_render_state`] (no `winit::Window`, no winit-bound
    /// `SurfaceContext`); delegates Steps 4–6 to the shared
    /// [`Self::init_render_state_post_surface`] helper so production
    /// and headless code paths cannot drift apart.
    ///
    /// The caller supplies the offscreen color target's format / width
    /// / height so the pipeline + frame-graph + camera aspect match
    /// what the harness will hand to [`Self::render_frame_to_target`].
    /// Dispatch N2 — gate extended from `#[cfg(test)]` to
    /// `#[cfg(any(test, feature = "test-harness"))]` so the
    /// `visual_test_harness` module's pub fn can call this from
    /// external consumers when the feature is enabled. The body and
    /// `pub(crate)` visibility are unchanged; this method stays a
    /// crate-internal API, just compiled in two more configurations.
    #[cfg(any(test, feature = "test-harness"))]
    pub(crate) fn init_render_state_headless(
        &mut self,
        target_format: wgpu::TextureFormat,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        // Dispatch G: mirror the production guard — accept either a
        // CAD scene OR a prebuilt render-only mesh. Headless tests
        // can exercise either path.
        let has_cad_scene = self.cad_world.is_some() && self.cad_entity.is_some();
        let has_prebuilt_mesh = !self.prebuilt_render_meshes.is_empty();
        if !has_cad_scene && !has_prebuilt_mesh {
            return Ok(());
        }
        // Step 2 — GfxContext (winit-independent).
        let gfx_ctx = GfxContext::new_headless().map_err(|e| format!("gfx ctx: {e}"))?;
        // Steps 4–6 — shared helper.
        self.init_render_state_post_surface(gfx_ctx, target_format, width, height)
    }

    /// Reconfigure the render-path surface on `WindowEvent::Resized`
    /// (sub-δ.1.B). Updates the camera UBO with a new view*proj matrix
    /// for the new aspect ratio. No-op when render path is not
    /// initialised.
    ///
    /// `render_input` carries the sim/editor-side inputs the render
    /// path consumes on resize — today exactly [`EditorCameraState`].
    /// GPU-backed state (surface, gfx_ctx, gfx_camera UBO) is read /
    /// mutated via `&mut self` as before. See
    /// [`crate::render_input::RenderInput`] for the snapshot-handoff
    /// boundary rationale.
    pub(crate) fn resize_render_path(
        &mut self,
        render_input: &RenderInput<'_>,
        new_w: u32,
        new_h: u32,
    ) {
        if new_w == 0 || new_h == 0 {
            return;
        }
        let Some(gfx_ctx) = self.gfx_ctx.as_ref() else {
            return;
        };
        if let Some(surface_ctx) = self.surface_ctx.as_mut() {
            surface_ctx.resize(gfx_ctx, new_w, new_h);
        }
        let aspect = (new_w as f32) / (new_h as f32);
        let view_proj = render_input.editor_camera.view_proj(aspect);
        if let Some(camera) = self.gfx_camera.as_ref() {
            camera.update(gfx_ctx, view_proj, glam::Mat4::IDENTITY);
        }
        // Phase 6 sub-β — rebuild the compiled `lit_mesh` frame-graph
        // against the new surface dimensions. [`TextureDescriptor`] is
        // keyed on `width`/`height`, and the descriptor flows verbatim
        // into [`TexturePool`]'s free-list identity; new descriptor =>
        // new pool slot. Old slots for the previous descriptor drain
        // through the ring rotation and accumulate in `free_lists` as
        // stale entries (bounded by `FRAMES_IN_FLIGHT=2` allocations per
        // resize). Acceptable bounded leak for v0; pool-level
        // free-list pruning is out of scope.
        if self.compiled_frame_graph.is_some() {
            self.compiled_frame_graph = Some(build_lit_mesh_compiled_frame_graph(new_w, new_h));
        }
    }
}
