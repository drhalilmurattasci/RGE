// adapted from rustforge::apps::editor-app::app_lifecycle on 2026-05-05 — PlayState transitions added
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
//!
//! # 2026-05-11 post-chapter cohesion-debt split
//!
//! After the Render-backed face-selection chapter closed at ~1268
//! lines (under an inline `SPLIT-EXEMPTION` that scheduled the split
//! for after the chapter), the file was split into three cohesive
//! files within `crates/editor-shell/src/`:
//!
//! - this file: `EditorShell` struct + `ApplicationHandler` trait impl
//!   + constructors + PIE state machine + `WorldSnapshot` round-trip
//!   + toolbar entry points + diagnostics helpers.
//! - [`crate::render_path`]: render-state init, per-frame
//!   `render_frame`, resize hook, highlight constants.
//! - [`crate::pick_path`]: `handle_left_click`, `rebuild_highlight_overlay`.
//!
//! The split is pure structural — every existing test passes
//! byte-identically and the public API is unchanged. Methods reachable
//! from the cross-file `impl EditorShell { … }` blocks are marked
//! `pub(crate)` (a private-to-crate boundary, no public-API delta).
//!
//! # 2026-05-21 Phase 9 keyboard → CommandBus wire-up + time-scale migration
//!
//! Phase 9 added [`commands::EditorKeyCommand`] + the `command_bus` /
//! `modifiers` fields + the five narrow shell command methods
//! (`submit_action` / `undo_command` / `redo_command` /
//! `mark_saved_command` / `command_bus`) + `handle_key_command` + two new
//! arms in `window_event` (`ModifiersChanged` / `KeyboardInput`). The
//! follow-up time-scale-via-bus dispatch migrated `TimeScale` from an
//! EditorShell field into a `rge_kernel_ecs::World` resource and routed
//! the `set_time_scale` mutation through a new
//! [`commands::SetTimeScale`] action.
//!
//! Per dispatch "second command source" policy that material lives in
//! the nested [`commands`] module rather than continuing to grow this
//! file:
//!
//! - [`commands::EditorKeyCommand`] + key-binding mapping table.
//! - `EditorShell::{submit_action, undo_command, redo_command,
//!   mark_saved_command, command_bus, handle_key_command, set_time_scale}`.
//! - [`commands::SetTimeScale`] — payload-based merge so slider drags
//!   coalesce; `Send + Sync` (no interior-mutability) because the
//!   `Action` trait requires it.
//
// SPLIT-EXEMPTION: After moving the Phase 9 command material into
// `crate::lifecycle::commands` (this dispatch), this file is still ~1040
// LoC because of: the EditorShell struct itself (~140 LoC with field
// docs), both constructors (~140 LoC each), the PIE state machine
// (handle_button + advance_game_tick + tick_redraw ~140 LoC), the
// ApplicationHandler impl (window_event ~120 LoC), and the inline
// `#[cfg(test)] mod tests { … }` block at the file foot (~250 LoC).
// The natural next split is the inline tests cluster — extracting to
// a sibling `tests.rs` is a clean structural move with no behavioural
// delta, but is itself a separate cohesion-debt followup (and the test
// cluster references private-to-`lifecycle` symbols that would have
// to be `pub(crate)`-promoted in lockstep). Not in scope for this
// dispatch; flagged for the next editor-shell file-cohesion sweep.

use std::sync::Arc;
use std::time::Instant;

use rge_cad_core::CadGraph;
use rge_cad_projection::{BRepHandle, CadProjection};
use rge_editor_actions::CommandBus;
use rge_gfx::{
    Camera as GfxCamera, DirectionalLight, GfxContext, IndexBuffer, LitMesh, LitMeshPipeline,
    Material, SurfaceContext,
};
use rge_input::{translate_keyboard, InputEvent};
use rge_kernel_ecs::{EntityId as KernelEntityId, World as KernelWorld};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::keyboard::ModifiersState;
use winit::window::{Window, WindowId};

use crate::audit::{AuditEvent, AuditLedger};
use crate::camera::EditorCameraState;
use crate::coord::EditorCoord;
use crate::play_state::{PlayState, PlayStateError, PlayStateTransition};
use crate::play_toolbar::{PlayToolbar, ToolbarButtonId};
use crate::render_input::{RenderHandoff, RenderInputOwned};
use crate::snapshot::{capture_and_audit, restore_and_audit, WorldSnapshot};
use crate::time_scale::{TimeScale, TimeScaleClass};
use crate::viewport::Viewport;
use crate::world::World;

/// Default progress-line interval (frames). Mirrors rustforge's
/// `PROGRESS_FRAME_INTERVAL` — once per ~second at 60Hz.
const PROGRESS_FRAME_INTERVAL: u64 = 60;

pub mod commands;

pub use commands::{EditorKeyCommand, SetTimeScale};

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
    pub(crate) coord: EditorCoord,
    state: PlayState,
    snapshot: Option<WorldSnapshot>,
    toolbar: PlayToolbar,
    // Phase 9 time-scale-via-bus migration: `TimeScale` is now a
    // `rge_kernel_ecs::World` resource (stored on `self.world.kernel()`)
    // rather than an EditorShell field. The bus-routed `set_time_scale`
    // in `commands::SetTimeScale` is the sole writer; the public
    // `time_scale(&self)` accessor reads from the resource and returns
    // a `Copy` value, preserving the prior API shape. Resources are NOT
    // included in `WorldSnapshot::capture` (snapshot.rs only takes
    // `serialize_snapshot` + `capture_blob_state`), so the slider value
    // persists across Play/Stop by construction.
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
    pub(crate) editor_camera: EditorCameraState,

    /// Per-ADR-117 latest-only render-input handoff slot. Sim/editor
    /// path publishes a fresh `Arc<RenderInputOwned>` snapshot every
    /// `tick_redraw`; resize/redraw paths `acquire()` the most-recent
    /// snapshot instead of constructing `RenderInput` ad-hoc from
    /// `self.editor_camera`. Always present (`RenderHandoff::new()`
    /// at construction). Single-threaded today; the substrate is
    /// `Send + Sync` and ready for a future dedicated render thread
    /// without API changes. See `crate::render_input` for semantics.
    render_handoff: RenderHandoff,

    /// Optional CAD-domain ECS world holding the renderable entity. The
    /// `world` field above is the editor-shell wrapper used by the W03
    /// PIE plumbing; this kernel-side world is the projection's source
    /// of truth (`world.entity::<BRepHandle>()` etc.). Sub-δ.1.B does
    /// NOT integrate this with the PIE wrapper — the two worlds coexist
    /// in parallel; the wrapper's snapshot tests are unaffected.
    pub(crate) cad_world: Option<KernelWorld>,

    /// Optional projection layer that owns the cached `ProjectedMesh`
    /// per entity. Non-`None` iff `cad_world` is non-`None`.
    pub(crate) projection: Option<CadProjection>,

    /// Optional CAD graph (committed operator history). Non-`None` iff
    /// `cad_world` is non-`None`. Sub-δ.2's mouse-pick flow consumes
    /// `cad_graph.graph()` as the second argument to
    /// [`CadProjection::pick_face`] (via [`crate::camera::pick_face_at`]).
    pub(crate) cad_graph: Option<CadGraph>,

    /// Optional pre-resolved entity inside `cad_world` to render. Sub-δ.1.B
    /// renders one cuboid; the entity is captured at construction so the
    /// render path doesn't re-query.
    pub(crate) cad_entity: Option<KernelEntityId>,

    /// winit window the surface is bound to (kept alive for the surface's
    /// `'static` lifetime). `None` until `resumed`.
    pub(crate) window: Option<Arc<Window>>,

    /// wgpu instance / adapter / device / queue. `None` until `resumed`.
    pub(crate) gfx_ctx: Option<GfxContext>,

    /// Surface + configuration. `None` until `resumed`.
    pub(crate) surface_ctx: Option<SurfaceContext>,

    /// Compiled lit-mesh render pipeline. `None` until `resumed`.
    pub(crate) pipeline: Option<LitMeshPipeline>,

    /// Camera UBO (GPU side). `None` until `resumed`.
    pub(crate) gfx_camera: Option<GfxCamera>,

    /// Material bind group + UBO + texture. `None` until `resumed`.
    pub(crate) material: Option<Material>,

    /// Directional light UBO. `None` until `resumed`.
    pub(crate) light: Option<DirectionalLight>,

    /// GPU-uploaded mesh for the cuboid entity. `None` until `resumed`.
    pub(crate) cuboid_mesh: Option<LitMesh>,

    /// Most recent cursor position from `WindowEvent::CursorMoved`, in
    /// **physical pixels** (winit 0.30 `CursorMoved.position` convention,
    /// matching `SurfaceConfiguration.width / height`). `None` until the
    /// first `CursorMoved` event arrives. Read by
    /// [`Self::handle_left_click`] to compute the click ray.
    pub(crate) cursor_pos: Option<[f32; 2]>,

    // ---- sub-ε selection highlight overlay -------------------------------
    //
    // The picked face's triangles are drawn as a second `draw_indexed`
    // after the main cuboid, reusing the existing `LitMeshPipeline` +
    // camera/light bind groups but binding a separate tinted `Material`
    // and a freshly-built `IndexBuffer` containing only the matching
    // triangles' dense indices.
    //
    // Both fields are `Option<…>`: `highlight_material` is built once on
    // `resumed` alongside the main material; `highlight_index_buffer` is
    // rebuilt by [`Self::handle_left_click`] on every click that resolves
    // a face (cleared to `None` on no-hit).
    /// Tinted [`Material`] for the highlight overlay. Same bind-group
    /// layout as the main material — only the `base_color` UBO differs
    /// (see [`crate::render_path::HIGHLIGHT_COLOR`]). `None` until `resumed`.
    pub(crate) highlight_material: Option<Material>,

    /// Dense triangle-vertex index buffer for the currently-highlighted
    /// face. `None` when no face is selected (or when the picker resolved
    /// to a face on an unlabeled mesh / `face_labels = None` source). Built
    /// in [`Self::handle_left_click`] from
    /// [`CadProjection::face_triangle_indices`].
    pub(crate) highlight_index_buffer: Option<IndexBuffer>,

    /// Phase 6 sub-β — frame-graph substrate transient-texture pool.
    /// `None` until [`Self::init_render_state`]; rotated via
    /// `begin_frame()` once per [`Self::render_frame`] per ADR-118 D4.
    pub(crate) texture_pool: Option<rge_gfx::TexturePool>,

    /// Phase 6 sub-β — frame-graph substrate transient-buffer pool.
    /// Required by [`rge_gfx::build_resource_map`]'s signature even
    /// though sub-β consumes only a transient texture (depth); a future
    /// transient-buffer consumer would populate `map.buffer_map` from
    /// the same builder pass without further plumbing churn.
    pub(crate) buffer_pool: Option<rge_gfx::BufferPool>,

    /// Phase 6 sub-β — compiled per-frame resource flow for the
    /// single-pass `"lit_mesh"` graph (one transient depth-texture
    /// write, no reads). Rebuilt on surface resize in
    /// [`Self::resize_render_path`] because [`rge_gfx::TextureDescriptor`]
    /// is keyed on `width`/`height` and the descriptor flows verbatim
    /// into pool free-list identity.
    pub(crate) compiled_frame_graph: Option<rge_gfx::CompiledFrameGraph>,

    // ---- Phase 9 CommandBus integration -----------------------------------
    //
    // The bus mediates all undoable editor mutations into the kernel
    // [`rge_kernel_ecs::World`] held inside the wrapper [`crate::world::World`]
    // (`shell.world.kernel_mut()`). Per PLAN §6.16 the bus is the **single
    // mediation layer** for editor mutations; per editor-actions §1 the
    // `Action` trait is `(&mut rge_kernel_ecs::World)`-only. CAD-graph and
    // projection mutations are intentionally NOT on the bus today — they
    // wait for a future "CAD-state into ECS" design dispatch with its own
    // preflight (see `plans/BASELINE.md` editor-usability preflight, §F →
    // SpawnCuboidAt rejection note).
    /// Bus owned by the shell so a single editor session has one undo
    /// history, one audit-ledger cursor, and one save-mark across all
    /// keyboard / future-menu / future-toolbar command sources. Constructed
    /// fresh in both `with_world` and `with_world_projection_graph`.
    command_bus: CommandBus,

    /// Latest [`ModifiersState`] from `WindowEvent::ModifiersChanged`. winit
    /// 0.30 delivers `KeyEvent` without modifier flags (only `physical_key`
    /// + `logical_key` + `state`); the modifier state must be tracked
    /// separately on the receiving side. Used by [`Self::window_event`] to
    /// detect Ctrl+Z / Ctrl+Y / Ctrl+S without a broad input refactor.
    modifiers: ModifiersState,
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
    pub fn with_world(mut world: World) -> Self {
        // Phase 9 time-scale-via-bus migration: install `TimeScale` as a
        // `rge_kernel_ecs::World` resource. `insert_resource` REPLACES
        // any existing instance, so this is also idempotent if the caller
        // pre-populated the world with a non-default `TimeScale`.
        world.kernel_mut().insert_resource(TimeScale::default());
        Self {
            world,
            coord: EditorCoord::new(),
            state: PlayState::default(),
            snapshot: None,
            toolbar: PlayToolbar::standard(),
            viewport: Viewport::default(),
            audit: AuditLedger::default(),
            tick_count: 0,
            last_frame_instant: None,
            initialized: false,
            editor_camera: EditorCameraState::default(),
            render_handoff: RenderHandoff::new(),
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
            highlight_material: None,
            highlight_index_buffer: None,
            texture_pool: None,
            buffer_pool: None,
            compiled_frame_graph: None,
            command_bus: CommandBus::new(),
            modifiers: ModifiersState::empty(),
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

        // Phase 9 time-scale-via-bus migration: install `TimeScale` as a
        // resource on the editor wrapper world (NOT on `cad_world` — the
        // kernel `World` field that holds editor state is `self.world.kernel`).
        let mut world = World::new();
        world.kernel_mut().insert_resource(TimeScale::default());
        Self {
            world,
            coord: EditorCoord::new(),
            state: PlayState::default(),
            snapshot: None,
            toolbar: PlayToolbar::standard(),
            viewport: Viewport::default(),
            audit: AuditLedger::default(),
            tick_count: 0,
            last_frame_instant: None,
            initialized: false,
            editor_camera: EditorCameraState::default(),
            render_handoff: RenderHandoff::new(),
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
            highlight_material: None,
            highlight_index_buffer: None,
            texture_pool: None,
            buffer_pool: None,
            compiled_frame_graph: None,
            command_bus: CommandBus::new(),
            modifiers: ModifiersState::empty(),
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

    // CommandBus-integration methods (`submit_action`, `undo_command`,
    // `redo_command`, `mark_saved_command`, `command_bus`,
    // `handle_key_command`, `set_time_scale`) live in [`commands`].

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
    ///
    /// Reads from the `TimeScale` ECS resource on `self.world.kernel()`.
    /// Returns `TimeScale::default()` defensively if (somehow) the
    /// resource was removed — both constructors install it, and there is
    /// no production path that removes it, so the fallback should never
    /// fire in practice. Returning a `Copy` value preserves the prior
    /// API shape so call sites and tests need no rewrite.
    #[must_use]
    pub fn time_scale(&self) -> TimeScale {
        self.world
            .kernel()
            .resource::<TimeScale>()
            .map(|r| *r)
            .unwrap_or_default()
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

    /// Borrow the per-ADR-117 latest-only render-input handoff slot.
    ///
    /// Sim-side `tick_redraw` calls `publish()` on this slot once
    /// per frame; resize/redraw event-loop arms call `acquire()` to
    /// read the most-recently-published `RenderInputOwned` snapshot.
    /// Exposed for the Phase 6.2 runtime integration end-to-end test
    /// (`tests/render_input_boundary.rs`) and for any future
    /// out-of-crate caller that needs to observe the handoff
    /// generation counter without taking the slot mutex.
    ///
    /// The accessor returns a shared reference; mutation is internal
    /// to the handoff via its own `&self` `publish` / `acquire`
    /// methods (the substrate is interior-mutable by design — see
    /// `crate::render_input` for the `Mutex<Option<Arc<_>>>` +
    /// `AtomicU64` composition).
    #[must_use]
    pub fn render_handoff(&self) -> &RenderHandoff {
        &self.render_handoff
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

    /// Advance one game-system tick, applying the configured time-scale.
    /// Editor systems are not invoked here (they run unconditionally on
    /// every redraw, regardless of `PlayState` — PLAN.md constitutional
    /// principle #8).
    fn advance_game_tick(&mut self, dt_seconds: f32) {
        let scaled = self.time_scale().apply(dt_seconds, TimeScaleClass::Game);
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
        self.viewport.update_overlay(self.state, self.time_scale());

        // 4) Phase 6.2 runtime integration — publish a fresh
        //    `RenderInputOwned` snapshot to the per-ADR-117 handoff
        //    slot. The render-side `Resized` / `RedrawRequested`
        //    arms call `acquire()` on the same slot below instead
        //    of constructing `RenderInput` ad-hoc from
        //    `self.editor_camera`. Camera-only payload preserved
        //    (matches the borrowed `RenderInput` field set).
        //
        //    Anchor fields (per PLAN §1.5.2 / ADR-117 sub-decision 3):
        //    - `ecs_tick` = `self.tick_count` (the editor-shell's
        //      authoritative game-system tick counter; advances only
        //      when `PlayState::game_systems_run()` fires, which is
        //      the closest analogue to "kernel-ecs tick" available
        //      pre-kernel-ecs integration).
        //    - `checkpoint_id` = `0` (no `cad-projection` checkpoint
        //      counter exists on `EditorShell` today; per dispatch
        //      spec, `0` is acceptable for v0 — the values matter
        //      for the empirical-invariant test, not for runtime
        //      correctness today).
        let snapshot = std::sync::Arc::new(RenderInputOwned {
            ecs_tick: self.tick_count,
            checkpoint_id: 0,
            editor_camera: self.editor_camera,
        });
        self.render_handoff.publish(snapshot);

        // 5) Diagnostic progress line at the rustforge interval.
        if self.tick_count > 0 && self.tick_count % PROGRESS_FRAME_INTERVAL == 0 {
            tracing::trace!(
                target: "rge::editor-shell::lifecycle",
                tick = self.tick_count,
                state = self.state.label(),
                scale = self.time_scale().value(),
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
            self.time_scale().value(),
        )
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
                // Phase 6.2 runtime integration — acquire the most
                // recently published snapshot from the per-ADR-117
                // `RenderHandoff` slot instead of constructing a
                // `RenderInput` view ad-hoc from `self.editor_camera`.
                //
                // Resize can fire before any `tick_redraw` (e.g. the
                // first `WindowEvent::Resized` from winit's initial
                // size negotiation may arrive before the first
                // `RedrawRequested`). In that case the slot is empty
                // — publish a fresh snapshot inline so the resize
                // proceeds with the current camera. This keeps the
                // resize path coupled to the SAME handoff substrate
                // the render path consumes, instead of bypassing it
                // with an ad-hoc local view.
                if self.render_handoff.acquire().is_none() {
                    let snapshot = std::sync::Arc::new(RenderInputOwned {
                        ecs_tick: self.tick_count,
                        checkpoint_id: 0,
                        editor_camera: self.editor_camera,
                    });
                    self.render_handoff.publish(snapshot);
                }
                let owned = self
                    .render_handoff
                    .acquire()
                    .expect("inline publish above guarantees a snapshot");
                let render_input = owned.as_render_input();
                self.resize_render_path(&render_input, new_size.width, new_size.height);
                // `owned` (Arc) drops here; the handoff slot retains
                // its own reference for the next acquire.
            }
            WindowEvent::RedrawRequested => {
                self.tick_redraw();
                // Phase 6.2 runtime integration — acquire the most
                // recently published snapshot from the per-ADR-117
                // `RenderHandoff` slot. `render_frame` currently
                // reads zero sim-side state per frame (all per-frame
                // sim reads belong on the snapshot side of the
                // boundary — see `render_input.rs::RenderInput` and
                // the `render_frame_body_does_not_read_self_editor_camera`
                // discipline test); the acquire here anchors the
                // §6.2 contract "render reads frozen
                // WorldSnapshot{N}" by routing the render path
                // through the same handoff substrate even when the
                // current `render_frame` consumer is a no-op for
                // per-frame sim state. When `render_frame` grows
                // per-frame sim reads in a later dispatch, they pull
                // off the held snapshot — the wiring is already in
                // place.
                let _snapshot = self.render_handoff.acquire();
                let _rendered = self.render_frame();
                // `_snapshot` Arc drops at end of scope; sim is free
                // to publish a newer snapshot for the next frame.
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
            WindowEvent::ModifiersChanged(new_modifiers) => {
                // Phase 9 CommandBus integration: track modifier state so
                // the `KeyboardInput` branch below can detect Ctrl+Z /
                // Ctrl+Y / Ctrl+S without scanning `KeyEvent`s for the
                // physical Ctrl key itself. winit 0.30 delivers the full
                // `ModifiersState` here on every modifier transition.
                self.modifiers = new_modifiers.state();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                // Phase 9 CommandBus integration. Press-only (KeyDown):
                // we don't act on KeyUp for the undo / redo / save
                // bindings — these are discrete commands. Future
                // tool-modifier keybinds (e.g. hold Shift to constrain)
                // would consume KeyUp separately.
                //
                // Use `rge_input::translate_keyboard` to map winit's
                // physical-key surface to our v0 `KeyCode` set. This is
                // a pure helper — no `Input<T>` resource, no broader
                // input integration is started by this dispatch.
                if let Some(InputEvent::KeyDown(key)) = translate_keyboard(&event) {
                    let ctrl = self.modifiers.control_key();
                    let shift = self.modifiers.shift_key();
                    if let Some(cmd) = EditorKeyCommand::from_key_press(key, ctrl, shift) {
                        self.handle_key_command(cmd);
                    }
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
