// adapted from rustforge::apps::editor-app::app_lifecycle on 2026-05-05 — PlayState transitions added
//
// SPLIT-EXEMPTION: cohesive `EditorShell` lifecycle file aggregating the
// W03 PIE state machine + sub-δ.1.B render path init/frame/resize +
// sub-δ.2 click handler + the `winit::ApplicationHandler` impl that
// dispatches between them. Splitting now would interleave with the
// pending sub-ε (selection highlight) + sub-ζ (smoke integration)
// dispatches and obscure the `EditorShell` mutable-self boundary
// (every method touches `&mut self` over a different field set; a
// helper-module split would require either pub-ifying internals or
// duplicating the `Option<…>`-guard scaffolding). Module extraction
// is queued for a dedicated post-chapter refactor dispatch when the
// shape stabilises (after sub-ζ). Per PLAN.md §1.3 Rule 3 (1085 lines
// vs 1000-line cap).
//
//! `EditorShell` — the editor host that owns winit's `ApplicationHandler`,
//! the PIE state machine, and the world/snapshot/audit-ledger triad.
//!
//! Per W03 dispatch and PLAN.md §6.13. Adapted from
//! `rustforge/apps/editor-app/src/app_lifecycle.rs`. The original drives a
//! single editor app with no PIE concept — its `RedrawRequested` always
//! ticks game systems. RGE's `EditorShell` adds:
//!
//! - [`PlayState`] gating: `RedrawRequested` only ticks game systems when
//!   `state.game_systems_run()` returns `true`.
//! - [`WorldSnapshot`] capture on `[Play]`, restore on `[Stop]`.
//! - [`TimeScale`] applied to the per-tick `dt` for game systems.
//! - [`PlayToolbar`] wired through [`Self::handle_button`].
//!
//! The original rustforge file pulls in wgpu device/queue/pipeline state
//! and an egui overlay; W03 strips those out (gfx wave W21+ owns wgpu)
//! and keeps only the lifecycle skeleton + PIE plumbing. Window creation
//! is also stubbed — `resumed` allocates the [`Viewport`] but does not
//! create a winit window (the real `editor/rge-editor` binary will own
//! that and forward events to `EditorShell`).
//!
//! # Sub-δ.1.B render path
//!
//! Sub-δ.1.B layers the **first triangle on screen** path on top of the
//! W03 PIE skeleton without modifying any of the existing PIE/snapshot
//! plumbing. The render path runs in parallel to the existing
//! `tick_redraw` (game-systems gating) — `RedrawRequested` first ticks
//! the editor systems (existing path), then renders one frame from the
//! pre-built scene held in `cad_world` / `projection` / `cad_graph`
//! when those are present.
//!
//! All render-path GPU state is `Option<…>`: it is empty during
//! construction (so the existing tests that build `EditorShell::new()`
//! and never enter the winit loop continue to work — `resumed` is what
//! populates the GPU side) and `Some(_)` once the editor's `resumed`
//! callback has constructed the wgpu instance + surface + pipeline +
//! lit-mesh. `rge-editor` is the only call site that triggers the
//! render path; all existing call sites keep `cad_world == None` and
//! see byte-identical lifecycle behaviour to W03.

use std::sync::Arc;
use std::time::Instant;

use rge_cad_core::CadGraph;
use rge_cad_projection::{BRepHandle, CadProjection};
use rge_gfx::{
    Camera as GfxCamera, DirectionalLight, GfxContext, LitMesh, LitMeshPipeline, Material,
    SurfaceContext,
};
use rge_kernel_ecs::{EntityId as KernelEntityId, World as KernelWorld};
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowAttributes, WindowId};

use crate::audit::{AuditEvent, AuditLedger};
use crate::camera::EditorCameraState;
use crate::coord::EditorCoord;
use crate::play_state::{PlayState, PlayStateError, PlayStateTransition};
use crate::play_toolbar::{PlayToolbar, ToolbarButtonId};
use crate::snapshot::{capture_and_audit, restore_and_audit, WorldSnapshot};
use crate::time_scale::{TimeScale, TimeScaleClass};
use crate::viewport::Viewport;
use crate::world::World;

/// Default progress-line interval (frames). Mirrors rustforge's
/// `PROGRESS_FRAME_INTERVAL` — once per ~second at 60Hz.
const PROGRESS_FRAME_INTERVAL: u64 = 60;

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

/// The editor host. Owns:
///
/// - the live `World` (authoritative runtime state during Editing; mutable
///   during Playing; restored on Stop)
/// - the editor coordination state (`EditorCoord`) — *never* in the
///   snapshot, so it persists across Play/Stop (PLAN.md §1.15)
/// - the `PlayState` machine
/// - the optional captured snapshot (`Some` while in PIE, `None` in Editing)
/// - the play-mode toolbar registration
/// - the time-scale setting
/// - the placeholder viewport widget
/// - the audit ledger for PIE events
///
/// Lifecycle (winit 0.30 `ApplicationHandler`):
///
/// ```text
/// resumed       — first call: allocate Viewport, log "ready" banner.
///                 Idempotent on re-resume (mobile suspend/resume).
/// window_event  — `RedrawRequested` drives one tick (game systems gated
///                 by PlayState); `CloseRequested` exits the loop.
/// suspended     — drop transient widget state; preserve PIE snapshot
///                 (so resume-from-suspend in Playing keeps the round-trip
///                 viable).
/// ```
pub struct EditorShell {
    world: World,
    coord: EditorCoord,
    state: PlayState,
    snapshot: Option<WorldSnapshot>,
    toolbar: PlayToolbar,
    time_scale: TimeScale,
    viewport: Viewport,
    audit: AuditLedger,
    /// Total ticks executed (game-system ticks; pumped by Redraw +
    /// `PlayState`). Used for diagnostics + the audit-log capture-tick field.
    tick_count: u64,
    /// Last frame's wall-clock instant. Real schedule-driver maintains a
    /// running accumulator (W04+); W03 stages the field.
    last_frame_instant: Option<Instant>,
    /// Whether `resumed()` has run at least once. winit allows multiple
    /// resume callbacks (mobile); we treat the second as a no-op for the
    /// fields that have already been initialized.
    initialized: bool,

    // ---- sub-δ.1.B render path -------------------------------------------
    //
    // All `Option<…>` so existing tests / call sites that don't need the
    // render path see byte-identical behaviour. `cad_world` is `Some` when
    // a render scene is attached (see `with_world_projection_graph`). Every
    // GPU field below is populated lazily inside `resumed`.
    /// Editor-runtime camera intent. Always present (`Default::default()`
    /// at construction). The view*projection matrix is recomputed each
    /// frame from the current surface aspect ratio.
    editor_camera: EditorCameraState,

    /// Optional CAD-domain ECS world holding the renderable entity. The
    /// `world` field above is the editor-shell wrapper used by the W03
    /// PIE plumbing; this kernel-side world is the projection's source
    /// of truth (`world.entity::<BRepHandle>()` etc.). Sub-δ.1.B does
    /// NOT integrate this with the PIE wrapper — the two worlds coexist
    /// in parallel; the wrapper's snapshot tests are unaffected.
    cad_world: Option<KernelWorld>,

    /// Optional projection layer that owns the cached `ProjectedMesh`
    /// per entity. Non-`None` iff `cad_world` is non-`None`.
    projection: Option<CadProjection>,

    /// Optional CAD graph (committed operator history). Non-`None` iff
    /// `cad_world` is non-`None`. Sub-δ.2's mouse-pick flow consumes
    /// `cad_graph.graph()` as the second argument to
    /// [`CadProjection::pick_face`] (via [`crate::camera::pick_face_at`]).
    cad_graph: Option<CadGraph>,

    /// Optional pre-resolved entity inside `cad_world` to render. Sub-δ.1.B
    /// renders one cuboid; the entity is captured at construction so the
    /// render path doesn't re-query.
    cad_entity: Option<KernelEntityId>,

    /// winit window the surface is bound to (kept alive for the surface's
    /// `'static` lifetime). `None` until `resumed`.
    window: Option<Arc<Window>>,

    /// wgpu instance / adapter / device / queue. `None` until `resumed`.
    gfx_ctx: Option<GfxContext>,

    /// Surface + configuration. `None` until `resumed`.
    surface_ctx: Option<SurfaceContext>,

    /// Compiled lit-mesh render pipeline. `None` until `resumed`.
    pipeline: Option<LitMeshPipeline>,

    /// Camera UBO (GPU side). `None` until `resumed`.
    gfx_camera: Option<GfxCamera>,

    /// Material bind group + UBO + texture. `None` until `resumed`.
    material: Option<Material>,

    /// Directional light UBO. `None` until `resumed`.
    light: Option<DirectionalLight>,

    /// GPU-uploaded mesh for the cuboid entity. `None` until `resumed`.
    cuboid_mesh: Option<LitMesh>,

    /// Most recent cursor position from `WindowEvent::CursorMoved`, in
    /// **physical pixels** (winit 0.30 `CursorMoved.position` convention,
    /// matching `SurfaceConfiguration.width / height`). `None` until the
    /// first `CursorMoved` event arrives. Read by
    /// [`Self::handle_left_click`] to compute the click ray.
    cursor_pos: Option<[f32; 2]>,
}

impl EditorShell {
    /// Construct a fresh shell with an empty world.
    #[must_use]
    pub fn new() -> Self {
        Self::with_world(World::new())
    }

    /// Construct with a pre-populated world (used by tests and by the
    /// `editor/rge-editor` binary's scene-load path).
    #[must_use]
    pub fn with_world(world: World) -> Self {
        Self {
            world,
            coord: EditorCoord::new(),
            state: PlayState::default(),
            snapshot: None,
            toolbar: PlayToolbar::standard(),
            time_scale: TimeScale::default(),
            viewport: Viewport::default(),
            audit: AuditLedger::default(),
            tick_count: 0,
            last_frame_instant: None,
            initialized: false,
            editor_camera: EditorCameraState::default(),
            cad_world: None,
            projection: None,
            cad_graph: None,
            cad_entity: None,
            window: None,
            gfx_ctx: None,
            surface_ctx: None,
            pipeline: None,
            gfx_camera: None,
            material: None,
            light: None,
            cuboid_mesh: None,
            cursor_pos: None,
        }
    }

    /// Construct an [`EditorShell`] with a pre-built CAD scene attached
    /// to the render path. **Sub-δ.1.B entry point** for `rge-editor`.
    ///
    /// `cad_world` must contain exactly one entity carrying a
    /// [`BRepHandle`] (with `brep_owner` set), and `projection` must
    /// already have been ticked (`projection.tick(&mut cad_world,
    /// &cad_graph, tolerance)`) so the cuboid's `ProjectedMesh` lives
    /// in the cache. The render path will look up that entity and
    /// upload its `RenderMesh` once on `resumed`.
    ///
    /// # Panics
    ///
    /// Panics if `cad_world` does not contain exactly one
    /// [`BRepHandle`]-carrying entity. Sub-δ.1.B is single-cuboid only;
    /// this is the substrate-honest contract — multi-entity scenes are
    /// a separate dispatch.
    #[must_use]
    pub fn with_world_projection_graph(
        cad_world: KernelWorld,
        projection: CadProjection,
        cad_graph: CadGraph,
    ) -> Self {
        let entity = {
            let mut iter = cad_world.query::<BRepHandle>();
            let first = iter.next().map(|(e, _)| e).expect(
                "with_world_projection_graph: cad_world must contain one BRepHandle entity",
            );
            assert!(
                iter.next().is_none(),
                "with_world_projection_graph: sub-δ.1.B is single-cuboid only \
                 (multi-entity rendering is a later dispatch); cad_world has more \
                 than one BRepHandle entity"
            );
            first
        };

        Self {
            world: World::new(),
            coord: EditorCoord::new(),
            state: PlayState::default(),
            snapshot: None,
            toolbar: PlayToolbar::standard(),
            time_scale: TimeScale::default(),
            viewport: Viewport::default(),
            audit: AuditLedger::default(),
            tick_count: 0,
            last_frame_instant: None,
            initialized: false,
            editor_camera: EditorCameraState::default(),
            cad_world: Some(cad_world),
            projection: Some(projection),
            cad_graph: Some(cad_graph),
            cad_entity: Some(entity),
            window: None,
            gfx_ctx: None,
            surface_ctx: None,
            pipeline: None,
            gfx_camera: None,
            material: None,
            light: None,
            cuboid_mesh: None,
            cursor_pos: None,
        }
    }

    // ---- accessors (read-only) ---------------------------------------------

    /// Borrow the live world (mutable access exposed for tests / scene-load).
    #[must_use]
    pub fn world(&self) -> &World {
        &self.world
    }

    /// Mutable world access. Real editors funnel mutations through the
    /// Command Bus (PLAN.md §6.16); W03 leaves direct access for the
    /// integration test that builds the 100-entity scene.
    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    /// Current `PlayState`.
    #[must_use]
    pub fn play_state(&self) -> PlayState {
        self.state
    }

    /// Borrow the editor coordination state.
    #[must_use]
    pub fn coord(&self) -> &EditorCoord {
        &self.coord
    }

    /// Mutable editor-coord access (selection updates land here).
    pub fn coord_mut(&mut self) -> &mut EditorCoord {
        &mut self.coord
    }

    /// Borrow the play-mode toolbar.
    #[must_use]
    pub fn toolbar(&self) -> &PlayToolbar {
        &self.toolbar
    }

    /// Current time-scale.
    #[must_use]
    pub fn time_scale(&self) -> TimeScale {
        self.time_scale
    }

    /// Borrow the audit ledger (read-only; tests assert event sequence).
    #[must_use]
    pub fn audit(&self) -> &AuditLedger {
        &self.audit
    }

    /// Borrow the placeholder viewport.
    #[must_use]
    pub fn viewport(&self) -> &Viewport {
        &self.viewport
    }

    /// Total game-system ticks executed since shell construction.
    #[must_use]
    pub fn tick_count(&self) -> u64 {
        self.tick_count
    }

    /// Whether a snapshot is currently held (i.e. in PIE).
    #[must_use]
    pub fn has_snapshot(&self) -> bool {
        self.snapshot.is_some()
    }

    // ---- toolbar entry points ----------------------------------------------

    /// Dispatch a toolbar-button press. Returns the resulting transition,
    /// or `Err` if the press was rejected by the state machine. The
    /// integration test asserts the exact transition sequence; the real
    /// UI swallows errors silently (disabled buttons should never have
    /// fired, but the state machine is the authoritative gate).
    ///
    /// # Errors
    ///
    /// Returns [`PlayStateError`] when the button press is invalid for the
    /// current [`PlayState`] (e.g. pressing Stop while in Editing).
    ///
    /// # Panics
    ///
    /// Panics if the internal snapshot invariant is violated (i.e.
    /// `StoppedAndRestored` is returned without a snapshot being held).
    pub fn handle_button(
        &mut self,
        id: ToolbarButtonId,
    ) -> Result<PlayStateTransition, PlayStateError> {
        match id {
            ToolbarButtonId::Play => {
                let before = self.state;
                let t = self.state.play()?;
                if t == PlayStateTransition::StartedPlay {
                    // Capture the snapshot at the moment of Play.
                    let snap = capture_and_audit(&self.world, self.tick_count, &mut self.audit);
                    self.snapshot = Some(snap);
                }
                self.audit.record(AuditEvent::PlayPressed {
                    before_state: before.label(),
                });
                Ok(t)
            }
            ToolbarButtonId::Pause => {
                let t = self.state.pause()?;
                self.audit.record(AuditEvent::PausePressed);
                Ok(t)
            }
            ToolbarButtonId::Stop => {
                let t = self.state.stop()?;
                if t == PlayStateTransition::StoppedAndRestored {
                    let snap = self
                        .snapshot
                        .take()
                        .expect("StoppedAndRestored implies snapshot was held");
                    restore_and_audit(&snap, &mut self.world, &mut self.audit);
                }
                self.audit.record(AuditEvent::StopPressed);
                Ok(t)
            }
            ToolbarButtonId::Step => {
                let t = self.state.step()?;
                self.audit.record(AuditEvent::StepPressed);
                // Step advances one game tick at the configured scale,
                // *bypassing* the PlayState gate (Step is the explicit
                // "tick once even though Paused" affordance).
                self.advance_game_tick(default_dt());
                Ok(t)
            }
            ToolbarButtonId::FrameStep => {
                let t = self.state.frame_step()?;
                self.audit.record(AuditEvent::FrameStepPressed);
                // FrameStep is "advance one render frame". W03 stages it as
                // a tick advance equal to one frame at 60Hz; W04 will
                // diverge tick from frame via the schedule accumulator.
                self.advance_game_tick(default_dt());
                Ok(t)
            }
        }
    }

    /// Adjust the time-scale slider. Records a [`AuditEvent::TimeScaleChanged`]
    /// audit event with the from/to values.
    pub fn set_time_scale(&mut self, value: f32) {
        let from = self.time_scale.value();
        let prev = self.time_scale.set(value);
        debug_assert!(
            (prev - from).abs() < 1e-9,
            "TimeScale::set returned previous != self.value()"
        );
        self.audit.record(AuditEvent::TimeScaleChanged {
            from,
            to: self.time_scale.value(),
        });
    }

    /// Advance one game-system tick, applying the configured time-scale.
    /// Editor systems are not invoked here (they run unconditionally on
    /// every redraw, regardless of `PlayState` — PLAN.md constitutional
    /// principle #8).
    fn advance_game_tick(&mut self, dt_seconds: f32) {
        let scaled = self.time_scale.apply(dt_seconds, TimeScaleClass::Game);
        self.world.tick_game_systems(scaled);
        self.tick_count += 1;
    }

    /// Tick the schedule for one redraw. Internal — invoked from
    /// `window_event::RedrawRequested`, but exposed `pub(crate)` so the
    /// integration test can drive ticks without spinning a real winit
    /// event loop.
    pub fn tick_redraw(&mut self) {
        // 1) Update wall-clock dt (real schedule-accumulator wave will
        //    refine this; W03 fixes 1/60 = 16.67ms).
        let dt = default_dt();
        self.last_frame_instant = Some(Instant::now());

        // 2) Game systems run only when PlayState says so.
        if self.state.game_systems_run() {
            self.advance_game_tick(dt);
        }

        // 3) Editor systems always run. W03 has no editor systems yet; the
        //    only "editor side-effect" is updating the viewport overlay.
        self.viewport.update_overlay(self.state, self.time_scale);

        // 4) Diagnostic progress line at the rustforge interval.
        if self.tick_count > 0 && self.tick_count % PROGRESS_FRAME_INTERVAL == 0 {
            tracing::trace!(
                target: "rge::editor-shell::lifecycle",
                tick = self.tick_count,
                state = self.state.label(),
                scale = self.time_scale.value(),
                "tick"
            );
        }
    }

    /// Drive `n` redraws in a tight loop. Used by the round-trip
    /// integration test (60-tick run between Play and Stop).
    pub fn run_for_redraws(&mut self, n: u64) {
        for _ in 0..n {
            self.tick_redraw();
        }
    }

    // ---- diagnostics --------------------------------------------------------

    /// Compose a one-line readiness banner (rustforge pattern).
    fn ready_banner(&self) -> String {
        format!(
            "rge-editor-shell: ready — viewport {}x{} state={} scale=×{:.2}",
            self.viewport.width(),
            self.viewport.height(),
            self.state.label(),
            self.time_scale.value(),
        )
    }

    // ---- sub-δ.1.B render path ------------------------------------------

    /// Build the GPU-side render state on first `resumed`.
    ///
    /// Composes (in order):
    /// 1. `winit::Window` → `Arc<Window>`
    /// 2. `GfxContext::new_headless()` (instance / adapter / device / queue)
    /// 3. `SurfaceContext::new(&ctx, Arc<Window>)` (configure + surface)
    /// 4. `Material::new(white 1×1)` / `DirectionalLight::new` / `GfxCamera::new`
    /// 5. `LitMeshPipeline::new(...)` against the surface's color format
    /// 6. `RenderMesh` from `projection.render_mesh_for(entity)` →
    ///    `LitMesh::from_render_mesh`
    /// 7. Update camera UBO with the editor camera's first view*proj
    ///
    /// Returns `Err(...)` only if the GPU-side initialisation fails (no
    /// adapter, no compatible surface format, surface create_surface,
    /// pipeline compile, buffer allocation). The error is propagated up
    /// to `resumed` which logs and continues with a placeholder banner —
    /// existing W03 behaviour is preserved when `cad_world == None`.
    fn init_render_state(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        // Sub-δ.1.B is single-cuboid: bail with a no-op when no CAD scene
        // was attached. This keeps the existing W03 tests' behaviour
        // (resumed is a no-op apart from the ready banner).
        if self.cad_world.is_none() || self.cad_entity.is_none() {
            return Ok(());
        }

        // Step 1 — winit window.
        let attrs = WindowAttributes::default()
            .with_title("RGE Editor")
            .with_inner_size(LogicalSize::new(1024_u32, 768_u32));
        let window = event_loop
            .create_window(attrs)
            .map_err(|e| format!("create_window: {e}"))?;
        let window = Arc::new(window);

        // Step 2 — GfxContext.
        let gfx_ctx = GfxContext::new_headless().map_err(|e| format!("gfx ctx: {e}"))?;

        // Step 3 — SurfaceContext.
        let surface_ctx = SurfaceContext::new(&gfx_ctx, Arc::clone(&window))
            .map_err(|e| format!("surface: {e}"))?;
        let format = surface_ctx.config().format;
        let width = surface_ctx.config().width;
        let height = surface_ctx.config().height;
        let aspect = (width.max(1) as f32) / (height.max(1) as f32);

        // Step 4 — bind groups (camera UBO + material + light).
        let gfx_camera = GfxCamera::new(&gfx_ctx).map_err(|e| format!("gfx camera: {e:?}"))?;
        gfx_camera.update(
            &gfx_ctx,
            self.editor_camera.view_proj(aspect),
            glam::Mat4::IDENTITY,
        );
        let material = Material::new(&gfx_ctx, &WHITE_1X1_RGBA, 1, 1)
            .map_err(|e| format!("material: {e:?}"))?;
        let light = DirectionalLight::new(&gfx_ctx).map_err(|e| format!("light: {e:?}"))?;
        light.update(&gfx_ctx, default_light_direction(), glam::Vec3::ONE);

        // Step 5 — pipeline against the surface's color format.
        let pipeline = LitMeshPipeline::new(
            &gfx_ctx,
            gfx_camera.bind_group_layout(),
            light.bind_group_layout(),
            material.bind_group_layout(),
            format,
        )
        .map_err(|e| format!("pipeline: {e:?}"))?;

        // Step 6 — RenderMesh → LitMesh for the cuboid entity.
        let entity = self.cad_entity.expect("checked above");
        let projection = self.projection.as_ref().expect("checked above");
        let cad_world = self.cad_world.as_ref().expect("checked above");
        let render_mesh = projection
            .render_mesh_for(entity, cad_world)
            .ok_or_else(|| "render_mesh_for returned None for the cuboid entity".to_string())?;
        let cuboid_mesh = LitMesh::from_render_mesh(&gfx_ctx, &render_mesh)
            .map_err(|e| format!("LitMesh::from_render_mesh: {e:?}"))?;

        // Step 7 — stash everything.
        self.window = Some(window);
        self.gfx_ctx = Some(gfx_ctx);
        self.surface_ctx = Some(surface_ctx);
        self.pipeline = Some(pipeline);
        self.gfx_camera = Some(gfx_camera);
        self.material = Some(material);
        self.light = Some(light);
        self.cuboid_mesh = Some(cuboid_mesh);

        // Kick off the first redraw so the cuboid appears.
        if let Some(w) = self.window.as_ref() {
            w.request_redraw();
        }

        Ok(())
    }

    /// Render one frame on `WindowEvent::RedrawRequested` (sub-δ.1.B).
    ///
    /// Acquires the next surface texture, records a single render pass
    /// that clears to [`DEFAULT_CLEAR`] and draws the cuboid mesh with
    /// the [`LitMeshPipeline`] + camera/light/material bind groups,
    /// presents, and schedules the next redraw.
    ///
    /// Returns `false` when the render path is not initialised (e.g.
    /// `cad_world == None`); caller should fall through to existing
    /// W03 behaviour.
    fn render_frame(&mut self) -> bool {
        let Some(gfx_ctx) = self.gfx_ctx.as_ref() else {
            return false;
        };
        let Some(surface_ctx) = self.surface_ctx.as_ref() else {
            return false;
        };
        let Some(pipeline) = self.pipeline.as_ref() else {
            return false;
        };
        let Some(gfx_camera) = self.gfx_camera.as_ref() else {
            return false;
        };
        let Some(light) = self.light.as_ref() else {
            return false;
        };
        let Some(material) = self.material.as_ref() else {
            return false;
        };
        let Some(mesh) = self.cuboid_mesh.as_ref() else {
            return false;
        };
        let Some(window) = self.window.as_ref() else {
            return false;
        };

        // Acquire the next surface texture. Skip the frame on
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
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder =
            gfx_ctx
                .device()
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("rge-editor.frame.encoder"),
                });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rge-editor.frame"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
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

            pass.set_pipeline(pipeline.pipeline());
            pass.set_bind_group(0, gfx_camera.bind_group(), &[]);
            pass.set_bind_group(1, light.bind_group(), &[]);
            pass.set_bind_group(2, material.bind_group(), &[]);
            pass.set_vertex_buffer(0, mesh.vertex_buffer().buffer().slice(..));

            if let Some(ib) = mesh.index_buffer() {
                pass.set_index_buffer(ib.buffer().slice(..), ib.index_format());
                pass.draw_indexed(0..ib.index_count(), 0, 0..1);
            } else {
                pass.draw(0..mesh.vertex_buffer().vertex_count(), 0..1);
            }
        }

        gfx_ctx.queue().submit(std::iter::once(encoder.finish()));
        frame.present();
        window.request_redraw();
        true
    }

    /// Handle a left-click event (sub-δ.2). Composes the most recent
    /// cursor position + current viewport size + the editor camera into
    /// a click ray, picks a face, and routes the resulting
    /// [`crate::coord::FaceSelection`] into [`crate::coord::EditorCoord`].
    ///
    /// **v0 single-select clear-on-miss semantics**: a click clears the
    /// existing face_selection set unconditionally and adds the new
    /// selection iff the picker resolves a hit. This matches the
    /// standard CAD convention (Fusion 360, Onshape, FreeCAD) where a
    /// bare click selects exactly one face and a click in empty space
    /// clears the selection. Multi-select via shift / ctrl is a future
    /// dispatch (chapter sub-ε is visual feedback; sub-ζ is integration
    /// smoke; multi-select lands later).
    ///
    /// No-op when:
    ///
    /// * `cursor_pos` is `None` (no `CursorMoved` event observed yet),
    /// * `surface_ctx` is `None` (render path not yet initialised — e.g.
    ///   the W03 PIE-only test paths that never enter `resumed`'s
    ///   render-path branch), OR
    /// * `cad_world` / `projection` / `cad_graph` is `None` (no CAD
    ///   scene attached — same condition guarding `init_render_state`).
    ///
    /// Tracing target: `rge::editor-shell::pick`.
    fn handle_left_click(&mut self) {
        // Defensive guards — if any required state is absent, no-op.
        let Some(cursor) = self.cursor_pos else {
            return;
        };
        let Some(surface_ctx) = self.surface_ctx.as_ref() else {
            return;
        };
        let Some(projection) = self.projection.as_ref() else {
            return;
        };
        let Some(cad_world) = self.cad_world.as_ref() else {
            return;
        };
        let Some(cad_graph) = self.cad_graph.as_ref() else {
            return;
        };

        let viewport = [
            surface_ctx.config().width as f32,
            surface_ctx.config().height as f32,
        ];
        let camera_view = self.editor_camera.to_camera_view(viewport);

        // Compute the selection (immutable borrows of self.* end after
        // this binding; the mutable `self.coord` borrow that follows
        // is then unconflicted).
        let selection = crate::camera::pick_face_at(
            &camera_view,
            cursor,
            projection,
            cad_world,
            cad_graph.graph(),
        );

        // v0 single-select clear-on-miss.
        self.coord.face_selection.clear();
        match selection {
            Some(sel) => {
                self.coord.face_selection.add(sel);
                tracing::info!(
                    target: "rge::editor-shell::pick",
                    "click at ({:.1}, {:.1}): picked entity={:?} face_id={:?}",
                    cursor[0],
                    cursor[1],
                    sel.entity,
                    sel.face_id,
                );
            }
            None => {
                tracing::info!(
                    target: "rge::editor-shell::pick",
                    "click at ({:.1}, {:.1}): no hit; selection cleared",
                    cursor[0],
                    cursor[1],
                );
            }
        }
    }

    /// Reconfigure the render-path surface on `WindowEvent::Resized`
    /// (sub-δ.1.B). Updates the camera UBO with a new view*proj matrix
    /// for the new aspect ratio. No-op when render path is not
    /// initialised.
    fn resize_render_path(&mut self, new_w: u32, new_h: u32) {
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
        let view_proj = self.editor_camera.view_proj(aspect);
        if let Some(camera) = self.gfx_camera.as_ref() {
            camera.update(gfx_ctx, view_proj, glam::Mat4::IDENTITY);
        }
    }
}

impl Default for EditorShell {
    fn default() -> Self {
        Self::new()
    }
}

/// Default frame-time for ticks (60Hz). Real schedule-accumulator (W04+)
/// will compute this from wall clock; W03 fixes the value so the
/// round-trip test is deterministic across machines.
///
/// Not `const` because Rust 1.78 does not allow FP arithmetic in const
/// functions (see rust-lang issue #57241); the literal value is
/// trivially inlinable by LLVM regardless.
fn default_dt() -> f32 {
    1.0 / 60.0
}

// -------------------------------------------------------------------------
// winit ApplicationHandler — the event-loop entry surface
// -------------------------------------------------------------------------

impl ApplicationHandler<()> for EditorShell {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // adapted from rustforge::apps::editor-app::app_lifecycle on 2026-05-05
        //   — wgpu/window-construction stripped (W21+ owns those); we keep
        //     the idempotent re-resume guard.
        if self.initialized {
            return;
        }
        // Sub-δ.1.B render path (skipped when no CAD scene is attached).
        // Existing W03 behaviour preserved when `cad_world == None` — the
        // helper bails fast and we just log the ready banner.
        if let Err(e) = self.init_render_state(event_loop) {
            tracing::error!(
                target: "rge::editor-shell::lifecycle",
                "init_render_state: {e}"
            );
        }
        tracing::info!(
            target: "rge::editor-shell::lifecycle",
            "{}",
            self.ready_banner()
        );
        self.initialized = true;
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        // adapted from rustforge::apps::editor-app::app_lifecycle on 2026-05-05
        //   — egui-overlay routing + IR-rebuild + close-persist stripped.
        //     PIE-aware tick driver replaces the rustforge unconditional
        //     `app.run_for_ticks(1)` call.
        match event {
            WindowEvent::CloseRequested => {
                tracing::info!(
                    target: "rge::editor-shell::lifecycle",
                    ticks = self.tick_count,
                    "close requested"
                );
                event_loop.exit();
            }
            WindowEvent::Resized(new_size) => {
                self.viewport.resize(new_size.width, new_size.height);
                self.resize_render_path(new_size.width, new_size.height);
            }
            WindowEvent::RedrawRequested => {
                self.tick_redraw();
                let _rendered = self.render_frame();
            }
            WindowEvent::CursorMoved { position, .. } => {
                // Track the latest cursor position for the next left-click
                // (sub-δ.2). winit 0.30 reports `CursorMoved.position` in
                // physical pixels, matching `SurfaceConfiguration.width /
                // height`; no DPI conversion needed.
                self.cursor_pos = Some([position.x as f32, position.y as f32]);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                // Sub-δ.2 single-select left-click. Other buttons /
                // Released events are no-ops in v0; right / middle /
                // scroll / drag / hover are non-goals (later dispatches).
                use winit::event::{ElementState, MouseButton};
                if state == ElementState::Pressed && button == MouseButton::Left {
                    self.handle_left_click();
                }
            }
            _ => {}
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        // Mobile-style suspend: drop transient widget state but PRESERVE
        // any in-flight PIE snapshot — resuming from suspend should leave
        // PIE viable. The `initialized` flag is reset so `resumed` rebuilds
        // the viewport.
        tracing::info!(
            target: "rge::editor-shell::lifecycle",
            "suspended (PIE snapshot preserved={})",
            self.snapshot.is_some()
        );
        self.initialized = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::ComponentTypeId;

    fn build_scene(shell: &mut EditorShell, n: usize) {
        for i in 0..n {
            let e = shell.world_mut().spawn();
            shell.world_mut().insert_component(
                e,
                ComponentTypeId(1),
                (i as u64).to_le_bytes().to_vec(),
            );
            shell
                .world_mut()
                .insert_component(e, ComponentTypeId(2), vec![0u8; 12]);
        }
    }

    #[test]
    fn fresh_shell_is_editing() {
        let s = EditorShell::new();
        assert_eq!(s.play_state(), PlayState::Editing);
        assert!(!s.has_snapshot());
        assert_eq!(s.tick_count(), 0);
    }

    #[test]
    fn play_button_captures_snapshot() {
        let mut s = EditorShell::new();
        build_scene(&mut s, 5);
        let t = s.handle_button(ToolbarButtonId::Play).unwrap();
        assert_eq!(t, PlayStateTransition::StartedPlay);
        assert!(s.has_snapshot());
        assert_eq!(s.play_state(), PlayState::Playing);
    }

    #[test]
    fn editing_does_not_tick_game_systems() {
        let mut s = EditorShell::new();
        build_scene(&mut s, 5);
        let pre = s.world().serialize();
        s.run_for_redraws(10);
        let post = s.world().serialize();
        assert_eq!(pre, post, "Editing must not advance game state");
        assert_eq!(s.tick_count(), 0);
    }

    #[test]
    fn playing_advances_game_systems() {
        let mut s = EditorShell::new();
        build_scene(&mut s, 5);
        s.handle_button(ToolbarButtonId::Play).unwrap();
        let pre = s.world().serialize();
        s.run_for_redraws(10);
        let post = s.world().serialize();
        assert_ne!(pre, post, "Playing must advance game state");
        assert_eq!(s.tick_count(), 10);
    }

    #[test]
    fn stop_restores_snapshot() {
        let mut s = EditorShell::new();
        build_scene(&mut s, 10);
        let pre_play = s.world().serialize();
        s.handle_button(ToolbarButtonId::Play).unwrap();
        s.run_for_redraws(60);
        let mid = s.world().serialize();
        assert_ne!(pre_play, mid);
        s.handle_button(ToolbarButtonId::Stop).unwrap();
        let post_stop = s.world().serialize();
        assert_eq!(pre_play, post_stop, "byte-identical restore");
        assert!(!s.has_snapshot());
        assert_eq!(s.play_state(), PlayState::Editing);
    }

    #[test]
    fn pause_freezes_game_systems() {
        let mut s = EditorShell::new();
        build_scene(&mut s, 5);
        s.handle_button(ToolbarButtonId::Play).unwrap();
        s.run_for_redraws(5);
        let mid = s.world().serialize();
        s.handle_button(ToolbarButtonId::Pause).unwrap();
        s.run_for_redraws(20);
        let after_pause = s.world().serialize();
        assert_eq!(mid, after_pause, "Paused must freeze game state");
    }

    #[test]
    fn step_advances_one_tick_in_paused() {
        let mut s = EditorShell::new();
        build_scene(&mut s, 5);
        s.handle_button(ToolbarButtonId::Play).unwrap();
        s.handle_button(ToolbarButtonId::Pause).unwrap();
        let pre = s.world().serialize();
        let pre_count = s.tick_count();
        s.handle_button(ToolbarButtonId::Step).unwrap();
        let post = s.world().serialize();
        assert_ne!(pre, post, "Step must advance one tick");
        assert_eq!(s.tick_count(), pre_count + 1);
    }

    #[test]
    fn step_invalid_in_editing() {
        let mut s = EditorShell::new();
        let result = s.handle_button(ToolbarButtonId::Step);
        assert!(result.is_err());
    }

    #[test]
    fn time_scale_affects_game_only() {
        let mut s = EditorShell::new();
        let e = s.world_mut().spawn();
        s.world_mut()
            .insert_component(e, ComponentTypeId(2), vec![0u8; 12]);
        s.set_time_scale(2.0);
        s.handle_button(ToolbarButtonId::Play).unwrap();
        s.run_for_redraws(60);
        let p = s.world().component(e, ComponentTypeId(2)).unwrap().clone();
        let mut x_bytes = [0u8; 4];
        x_bytes.copy_from_slice(&p[0..4]);
        let x = f32::from_le_bytes(x_bytes);
        // Position increments by `dt_scaled` per tick; with scale=2 and
        // dt=1/60 across 60 ticks, x = 60 * (1/60) * 2 = 2.0
        assert!((x - 2.0).abs() < 1e-3, "expected ~2.0, got {x}");
    }

    #[test]
    fn audit_records_play_stop() {
        let mut s = EditorShell::new();
        build_scene(&mut s, 5);
        s.handle_button(ToolbarButtonId::Play).unwrap();
        s.handle_button(ToolbarButtonId::Stop).unwrap();
        let tags: Vec<_> = s.audit().iter().map(AuditEvent::tag).collect();
        assert!(tags.contains(&"SnapshotCaptured"));
        assert!(tags.contains(&"PlayPressed"));
        assert!(tags.contains(&"SnapshotRestored"));
        assert!(tags.contains(&"StopPressed"));
    }
}
