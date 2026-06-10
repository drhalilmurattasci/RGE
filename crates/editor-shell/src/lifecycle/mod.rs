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
//!
//! # 2026-05-21 inline-test extraction
//!
//! The inline `#[cfg(test)] mod tests { ... }` block that previously
//! lived at the foot of this file (~140 LoC, 10 tests covering PIE
//! state machine + Play/Stop snapshot round-trip + game-system
//! gating + time-scale + audit-ledger) is now in
//! [`tests`](self::tests) (the sibling `tests.rs` file in this module
//! directory). All tests touched only the public `EditorShell` API,
//! so no `pub(crate)` promotion was needed for the move. The
//! extraction drops this file under the 1000-LoC `// SPLIT-EXEMPTION`
//! threshold, so the previous exemption annotation has been removed.
//!
//! # 2026-05-21 Phase 9 egui host integration (dispatch B)
//!
//! Adds the `egui_host: Option<EguiHost>` field on [`EditorShell`],
//! constructs it after wgpu+winit init in [`crate::render_path::EditorShell::init_render_state`],
//! routes winit events through `EguiHost::on_window_event` BEFORE the
//! existing editor branches, gates the `KeyboardInput` + `MouseInput`
//! branches on `!egui_consumed` (so egui owns events when it has
//! focus), and forwards resize updates to the host. The egui render
//! pass itself lives in [`crate::render_path::EditorShell::render_frame`]
//! (between the cuboid+highlight pass and `queue.submit()`, same
//! encoder, same surface view, `LoadOp::Load`).
//!
//! # 2026-05-21 Phase 9 live inspector dock tab (dispatch C)
//!
//! Adds the `inspector_handoff: Option<Arc<InspectorHandoff>>` field
//! cloned from the host's own handoff in
//! [`crate::render_path::EditorShell::init_render_state`]. Each
//! `render_frame` (BEFORE the egui pass) calls
//! [`Self::inspector_snapshot`] and publishes the result through the
//! held handoff. The host's `InspectorTabBody` reads the same handoff
//! on its tab render, so the dock area's `"Inspector"` tab reflects
//! the live editor state. No new public methods land here — the wire
//! is mediated entirely through existing `inspector_snapshot()` and
//! the host's `inspector_handoff()` accessor.
//!
//! # 2026-05-21 Phase 9 keyboard playback shortcuts (dispatch E)
//!
//! Adds the [`playback::EditorPlaybackCommand`] enum
//! (`TogglePlay` / `Stop`) plus
//! [`EditorShell::handle_playback_command`]. The
//! `WindowEvent::KeyboardInput` branch in [`Self::window_event`]
//! falls through from the Ctrl-bound [`EditorKeyCommand`] lookup to
//! the plain-key [`EditorPlaybackCommand`] lookup so the user can
//! press `Space` (toggle Editing/Playing/Paused) and `Escape` (stop
//! PIE) without touching the toolbar. Both lookups share the
//! `egui_consumed` gate from dispatch B; the playback commands route
//! through the existing [`Self::handle_button`] state-machine driver
//! — no new toolbar UI, no new ECS state, no CommandBus involvement.
//!
//! # 2026-05-21 Phase 9 face-pick over viewport (dispatch F)
//!
//! Updates the `WindowEvent::MouseInput` branch in
//! [`Self::window_event`] so a click that egui marked as `consumed`
//! still falls through to [`Self::handle_left_click`] when the cursor
//! is over the transparent [`rge_editor_egui_host::TabBody::Viewport`]
//! tab body. Inspector clicks and tab-chrome clicks remain consumed
//! (no accidental face-picking). The gate factors a tiny pure helper
//! [`should_fire_face_pick`] so the decision logic is unit-testable
//! without a real `EguiHost`.
//!
//! # 2026-05-21 Phase 9 render-only glTF mesh (dispatch G)
//!
//! Adds the `prebuilt_render_mesh: Option<RenderMesh>` field plus a
//! new [`EditorShell::with_render_mesh`] constructor for the
//! `rge-editor --glb <path>` flag. The constructor stashes a
//! pre-built [`rge_brep_render::RenderMesh`] (typically loaded from a
//! glTF/GLB file via `rge_io_gltf::import_glb`) without invoking the
//! CAD pipeline at all: `cad_world` / `projection` / `cad_graph` /
//! `cad_entity` all remain `None`.
//!
//! Doctrinal note (matches `rge_authority_fragmentation_risk.md`):
//! glTF meshes are NOT CAD bodies. v0 deliberately does NOT add an
//! `OperatorNode::ImportedMesh` variant — kittycad governs the
//! canonical operator IR, and the imported-mesh concept is editor-
//! local. The render path [`crate::render_path::EditorShell::init_render_state`]
//! branches on whether a CAD scene or a prebuilt mesh is present
//! (mutually exclusive at construction); face-pick / save / undo
//! naturally no-op in render-only mode because the existing
//! defensive guards in [`crate::pick_path::EditorShell::handle_left_click`]
//! already return early when `projection` is `None`.
//
// SPLIT-EXEMPTION: After landing the Phase 9 egui host wire-up
// (dispatch B) + the dispatch C handoff field, this file is ~1030 LoC
// — just over the threshold. The egui-host integration is naturally
// cohesive with the existing lifecycle (window_event input routing,
// resumed init, render_frame composition all live here); extracting
// it would scatter cohesive material across two files. A follow-up
// cohesion-debt dispatch can extract `window_event`'s match arms into
// a dedicated `events.rs` sibling once the arm count grows further;
// pre-emptive extraction would be cosmetic.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use rge_brep_render::RenderMesh;
use rge_cad_core::CadGraph;
use rge_cad_projection::{BRepHandle, CadProjection};
use rge_editor_actions::CommandBus;
use rge_editor_egui_host::{
    EguiHost, InspectorHandoff, MenuCommandHandoff, PredicateContextHandoff, SaveStatusHandoff,
};
use rge_editor_ui::menus::{default_editor_menu, Command};
use rge_gfx::{
    Camera as GfxCamera, DirectionalLight, GfxContext, IndexBuffer, LitMesh, LitMeshPipeline,
    Material, SurfaceContext,
};
use rge_input::{translate_keyboard, InputEvent, KeyCode};
use rge_kernel_ecs::{EntityId as KernelEntityId, World as KernelWorld};
use winit::application::ApplicationHandler;
use winit::event::{MouseScrollDelta, WindowEvent};
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
use crate::world::{ComponentBlob, ComponentTypeId, World};

/// Default progress-line interval (frames). Mirrors rustforge's
/// `PROGRESS_FRAME_INTERVAL` — once per ~second at 60Hz.
const PROGRESS_FRAME_INTERVAL: u64 = 60;

type EntityClipboardItem = Vec<(ComponentTypeId, ComponentBlob)>;

pub mod accelerator;
pub mod asset_reload;
pub mod commands;
pub mod extension_command;
pub mod open_request;
pub mod playback;
pub mod save_request;
pub mod save_source;
pub mod unsaved_changes;
pub mod window_title;

pub use accelerator::keycode_to_shortcut;
pub use asset_reload::AssetReloadHook;
pub use commands::{EditorKeyCommand, SetTimeScale};
pub use extension_command::{
    ExtensionCommandError, ExtensionCommandEvent, ExtensionCommandHandler, ExtensionCommandOutcome,
    ExtensionCommandResult,
};
pub use open_request::{GlbOpenDialog, SceneOpenHook};
pub use playback::EditorPlaybackCommand;
pub use save_request::{
    NewProjectSaveDialog, NewProjectSaveHook, ProjectSaveHook, SceneSaveDialog, SceneSaveHook,
};
pub use save_source::SaveSource;
pub use unsaved_changes::{
    UnsavedChangesContext, UnsavedChangesDecision, UnsavedChangesDialog, UnsavedChangesRequest,
    UnsavedChangesSourceKind,
};

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
    /// Set by command sources that request application exit without holding an
    /// `ActiveEventLoop`; consumed at the next winit event boundary.
    quit_requested: bool,
    /// Set by command sources that request the future command-palette surface to
    /// toggle; consumed by a future UI boundary.
    command_palette_toggle_requested: bool,

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

    /// Dispatch G + I — pre-built render-only meshes, populated by
    /// [`Self::with_render_meshes`] (or the single-mesh wrapper
    /// [`Self::with_render_mesh`]). **Mutually exclusive with the
    /// CAD fields above**: a shell is either CAD-driven (cuboid demo
    /// path) OR render-only (glTF-import path), never both. Render
    /// path `init_render_state_post_surface` sources the
    /// [`RenderMesh`] sequence for GPU upload from this vec when
    /// non-empty; face-pick already short-circuits when `projection`
    /// is `None`.
    ///
    /// Dispatch I extends the single-mesh dispatch-G storage
    /// (`Option<RenderMesh>`) to a `Vec<RenderMesh>` so a glTF/GLB
    /// file with multiple primitives renders all of them, not just
    /// the first. Each entry maps 1:1 to a `LitMesh` in
    /// [`Self::meshes`] after the GPU upload step.
    ///
    /// Empty when:
    /// - CAD-driven path is in use (`with_world_projection_graph`).
    /// - `with_render_meshes(vec![])` was called (defensive — the
    ///   binary rejects zero-mesh glTF files before reaching here).
    pub(crate) prebuilt_render_meshes: Vec<RenderMesh>,

    /// Dispatch K — per-mesh `base_color` parallel to
    /// [`Self::prebuilt_render_meshes`]. Populated by
    /// [`Self::with_render_meshes_and_base_colors`] (or by
    /// [`Self::with_render_meshes`] which fills every slot with
    /// opaque white `[1.0, 1.0, 1.0, 1.0]`). The render path
    /// consumes this Vec in `init_render_state_post_surface` to
    /// build one [`Material`] per mesh, each carrying the matching
    /// `base_color` in its UBO. Length invariant:
    /// `prebuilt_render_base_colors.len() == prebuilt_render_meshes.len()`
    /// — enforced at construction time by the
    /// `with_render_meshes_and_base_colors` constructor.
    pub(crate) prebuilt_render_base_colors: Vec<[f32; 4]>,

    /// Dispatch M2 — per-mesh `base_color_texture` parallel to
    /// [`Self::prebuilt_render_meshes`]. Each entry is
    /// `Some((width, height, pixels))` when the source glTF
    /// material carried an embedded `base_color_texture` whose
    /// image decoded to `Rgba8`; `None` otherwise (no material, no
    /// texture, or non-RGBA8 pixel format).
    ///
    /// The render path consumes this Vec alongside
    /// `prebuilt_render_base_colors` to construct one
    /// [`Material`] per mesh: `Material::new` is called with the
    /// owned RGBA8 bytes when present, falling back to the
    /// `WHITE_1X1_RGBA` placeholder when `None`. Either way the
    /// dispatch-K `update_color` follows, so a `base_color` tint
    /// modulates whatever texture is bound.
    ///
    /// Length invariant: `prebuilt_render_base_textures.len() ==
    /// prebuilt_render_meshes.len()` — enforced by
    /// `with_render_meshes_and_base_colors_and_textures`.
    pub(crate) prebuilt_render_base_textures: Vec<Option<(u32, u32, Vec<u8>)>>,

    /// Asset hot-reload — source path of the `--glb` file the editor
    /// was launched against, preserved so the R-key handler can
    /// re-import it. `None` for the default cuboid-demo path (R-key
    /// silently no-ops there); `Some(path)` for the `--glb` path
    /// when the editor binary calls
    /// [`Self::with_glb_reload_source`] after construction.
    pub(crate) glb_source_path: Option<PathBuf>,

    /// Asset hot-reload — caller-supplied loader callback that knows
    /// how to re-import the glTF/GLB at [`Self::glb_source_path`].
    /// Boxed-dyn so the editor binary's hook impl (which owns the
    /// `rge-io-gltf` dep) can be handed to editor-shell without
    /// threading the io-gltf type through. `None` for the default
    /// cuboid-demo path. See [`AssetReloadHook`] for the trait.
    pub(crate) reload_hook: Option<Box<dyn AssetReloadHook>>,

    /// In-app "Open" (Ctrl+O) — caller-supplied dialog callback that
    /// prompts the user for an Open candidate (a `.glb`, `.rge-scene`, or
    /// `.rge-project`; [`Self::handle_open_request`] dispatches on the
    /// returned path's kind). Boxed-dyn so the editor binary's impl
    /// (which owns the `rfd` dep) can be handed to editor-shell without
    /// threading `rfd` through — editor-shell never gains an `rfd`
    /// dependency. `None` when no dialog was attached (e.g. headless
    /// tests, or any path the binary did not wire); `Ctrl+O` warn-logs
    /// and no-ops there. Set via [`Self::with_glb_open_dialog`]. See
    /// [`GlbOpenDialog`].
    pub(crate) open_dialog: Option<Box<dyn GlbOpenDialog>>,

    /// In-app "Open scene" (Ctrl+O on a `.rge-scene` / `.rge-project`) —
    /// caller-supplied loader callback that reads a scene path into a
    /// fresh kernel `World`. Boxed-dyn so the editor binary's impl (which
    /// owns the `rge-scene-loader` edge) can be handed to editor-shell
    /// without threading `rge-scene-loader` / `rge-data` through —
    /// editor-shell never gains either dependency. `None` when no scene
    /// hook was attached (headless tests, or a launch mode the binary did
    /// not wire); a scene `Ctrl+O` warn-logs and no-ops there. Set via
    /// [`Self::with_scene_open_hook`]. See [`SceneOpenHook`].
    pub(crate) scene_open_hook: Option<Box<dyn SceneOpenHook>>,

    /// In-app "Save" (Ctrl+S) — caller-supplied dialog callback that prompts
    /// for a `*.rge-scene` save destination. Boxed-dyn so the editor binary's
    /// `rfd` impl can be handed to editor-shell without threading `rfd`
    /// through — editor-shell never gains an `rfd` dependency. `None` when no
    /// dialog was attached (headless tests, or a launch mode the binary did not
    /// wire); `Ctrl+S` warn-logs and no-ops there. Set via
    /// [`Self::with_scene_save_dialog`]. See [`SceneSaveDialog`].
    pub(crate) save_dialog: Option<Box<dyn SceneSaveDialog>>,

    /// In-app "Save" (Ctrl+S) — caller-supplied writer callback that writes the
    /// live `World` to a `.rge-scene` path. Boxed-dyn so the editor binary's
    /// `rge-scene-loader` impl can be handed to editor-shell without threading
    /// `rge-scene-loader` / `rge-data` through — editor-shell never gains either
    /// dependency. `None` when no hook was attached; `Ctrl+S` warn-logs and
    /// no-ops there. Set via [`Self::with_scene_save_hook`]. See
    /// [`SceneSaveHook`].
    pub(crate) scene_save_hook: Option<Box<dyn SceneSaveHook>>,

    /// In-app "Save" (Ctrl+S) — caller-supplied writer callback that writes the
    /// live `World` back to a `.rge-project` (overwrite its first scene +
    /// re-write the manifest). The project-axis companion to
    /// [`Self::scene_save_hook`]. Boxed-dyn so the editor binary's
    /// `rge-scene-loader` impl can be handed to editor-shell without threading
    /// `rge-scene-loader` / `rge-data` through — editor-shell never gains either
    /// dependency. `None` when no hook was attached; a `Project` `Ctrl+S`
    /// warn-logs and no-ops there. Set via [`Self::with_project_save_hook`]. See
    /// [`ProjectSaveHook`].
    pub(crate) project_save_hook: Option<Box<dyn ProjectSaveHook>>,

    /// Save-As to a NEW `.rge-project` tree (Ctrl+Shift+S) — caller-supplied
    /// directory picker. Boxed-dyn so the editor binary's `rfd` impl is handed
    /// to editor-shell without threading `rfd` through. `None` when not attached;
    /// `Ctrl+Shift+S` warn-logs and no-ops there. Set via
    /// [`Self::with_new_project_save_dialog`]. See [`NewProjectSaveDialog`].
    pub(crate) new_project_dialog: Option<Box<dyn NewProjectSaveDialog>>,

    /// Save-As to a NEW `.rge-project` tree (Ctrl+Shift+S) — caller-supplied
    /// writer that creates a fresh project tree from the live `World` (over
    /// `rge_scene_loader::save_world_as_new_project`). Boxed-dyn so the binary's
    /// `rge-scene-loader` impl is handed to editor-shell without threading it
    /// through. `None` when not attached; `Ctrl+Shift+S` warn-logs and no-ops
    /// there. Set via [`Self::with_new_project_save_hook`]. See
    /// [`NewProjectSaveHook`].
    pub(crate) new_project_hook: Option<Box<dyn NewProjectSaveHook>>,

    /// Unsaved-changes confirmation for dirty File -> Close, File -> Quit, and
    /// window close requests. Boxed-dyn so the editor binary's native `rfd`
    /// implementation can be injected without adding a native-dialog dependency
    /// to editor-shell. `None` is treated as Cancel on dirty requests, keeping
    /// headless/test construction conservative.
    pub(crate) unsaved_changes_dialog: Option<Box<dyn UnsavedChangesDialog>>,

    /// In-app "Save" (Ctrl+S) — the document a `Ctrl+S` writes back to: a
    /// `.rge-scene` (silent overwrite) or a literal `.rge-project` (overwrite
    /// first scene + manifest). `Some(_)` after opening / launching a
    /// `.rge-scene` / `.rge-project` (or a successful Save-As — `Ctrl+S` with no
    /// source commits a [`SaveSource::Scene`], `Ctrl+Shift+S` commits a new
    /// [`SaveSource::Project`]); `Ctrl+S` then routes by variant with
    /// no dialog. `None` for a blank / demo / `.glb` context — `Ctrl+S` falls
    /// back to Save-As. Cleared by [`Self::replace_world`] and re-committed by
    /// [`Self::handle_open_request`]. Set at construction via
    /// [`Self::with_save_source`]; read via [`Self::save_source`] /
    /// [`Self::save_source_path`].
    pub(crate) save_source: Option<SaveSource>,

    /// EDITOR-WINDOW-TITLE — the last title handed to the winit window's
    /// `set_title`, so [`Self::sync_window_title`] only re-titles on a change
    /// (not every frame). `None` until the first sync.
    pub(crate) last_window_title: Option<String>,

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

    /// Dispatch K — per-mesh material bind groups, populated 1:1 with
    /// [`Self::meshes`] in `init_render_state_post_surface`. Each
    /// entry owns its own UBO (`base_color` + `phong`) + 1×1 white
    /// placeholder texture + sampler + bind group, but the bind-group
    /// LAYOUT is identical across entries so the `LitMeshPipeline`
    /// can rebind any entry mid-pass without re-validation. Empty
    /// until `resumed`; the render path's `encode_main_pass` guards
    /// on `materials.is_empty()` for the same reason it guards on
    /// `meshes.is_empty()`. The pre-dispatch-K shared
    /// `material: Option<Material>` slot is gone — what was one
    /// global material is now the first (and only) entry in
    /// `materials` for the CAD-cuboid path.
    pub(crate) materials: Vec<Material>,

    /// Directional light UBO. `None` until `resumed`.
    pub(crate) light: Option<DirectionalLight>,

    /// GPU-uploaded meshes drawn by [`crate::render_path::EditorShell::encode_main_pass`].
    ///
    /// - **CAD cuboid path** (`with_world_projection_graph`) pushes
    ///   exactly ONE [`LitMesh`] (the projected cuboid) here during
    ///   `init_render_state_post_surface`.
    /// - **Render-only glTF path** (`with_render_meshes`) pushes N
    ///   meshes — one per [`rge_io_gltf::MeshAsset`] in the loaded
    ///   scene.
    /// - **Pre-`resumed` / headless paths** leave the Vec empty;
    ///   `encode_main_pass` skips drawing when the Vec is empty.
    ///
    /// The sub-ε highlight overlay (used only by the CAD path) reuses
    /// `meshes[0]`'s vertex buffer with the
    /// [`Self::highlight_index_buffer`]; glTF mode never sets the
    /// highlight buffer so the overlay path is never reached for
    /// imported meshes. See [`crate::render_path::EditorShell::encode_main_pass`]
    /// for the per-mesh draw loop.
    pub(crate) meshes: Vec<LitMesh>,

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

    // ---- Phase 9 egui host integration (dispatch B) -----------------------
    //
    // The egui+egui_dock host that paints editor UI on top of the wgpu
    // cuboid pass. Constructed lazily in [`Self::init_render_state`]
    // alongside the wgpu surface + winit window; `None` until that
    // callback runs. Existing tests that build `EditorShell::new()` /
    // `EditorShell::with_world(world)` and never enter the winit event
    // loop see `egui_host == None` and observe byte-identical pre-host
    // lifecycle behavior — the render path falls back to "cuboid-only"
    // when `egui_host.is_none()`.
    //
    // Per the egui host integration preflight (recorded in
    // `plans/BASELINE.md`): editor-shell depends on
    // `rge-editor-egui-host`, never the reverse. This field is the
    // single point of host ownership; the dispatch C
    // `DockState<TabBody>` + inspector tab body live INSIDE the host
    // crate, not on additional fields here.
    pub(crate) egui_host: Option<EguiHost>,

    // ---- Phase 9 live inspector dock tab (dispatch C) ----------------------
    //
    // Cloned `Arc` to the same [`InspectorHandoff`] the host stores
    // inside its [`rge_editor_egui_host::InspectorTabBody`]. Set in
    // [`crate::render_path::EditorShell::init_render_state`] right
    // after the host is constructed; remains `None` for shells that
    // never trigger render init (existing PIE / snapshot / time-scale
    // tests). Used by [`crate::render_path::EditorShell::render_frame`]
    // to publish a fresh [`crate::InspectorSnapshot`] through the
    // handoff once per frame, BEFORE the egui pass — so the dock
    // area's "Inspector" tab renders this frame's editor-session
    // state (tick count / time scale / play state / selection / undo).
    //
    // Owning the `Arc` here (instead of reaching through
    // `self.egui_host.as_ref().unwrap().inspector_handoff()` each
    // frame) keeps the publish path independent of the host's borrow
    // — the publish loop in `render_frame` takes `&self`-only borrows
    // and does NOT contend with the `&mut self.egui_host` borrow the
    // host's `render()` call needs immediately after.
    pub(crate) inspector_handoff: Option<Arc<InspectorHandoff>>,

    // ---- Save-status bar (EDITOR-SAVE-STATUS-INDICATOR) ---------------------
    //
    // Cloned `Arc` to the same [`SaveStatusHandoff`] the host stores. Set in
    // [`crate::render_path::EditorShell::init_render_state`] alongside
    // `inspector_handoff`; remains `None` for shells that never trigger render
    // init. Used by the same per-frame publish path
    // ([`crate::render_path::EditorShell::render_frame`]) to publish a fresh
    // [`rge_editor_state::SaveStatusSnapshot`] (open save source file name +
    // dirty flag) BEFORE the egui pass, so the host's bottom status bar renders
    // this frame's save state. Sibling to `inspector_handoff` — same `&self`-only
    // publish borrow, independent of the `&mut self.egui_host` render borrow.
    pub(crate) save_status_handoff: Option<Arc<SaveStatusHandoff>>,

    // ---- Live predicate context (MENU-DYNAMIC-RESOLVE) ----------------------
    //
    // Cloned `Arc` to the host's [`PredicateContextHandoff`] — a latest-only slot
    // sibling to `save_status_handoff`. Set in `init_render_state`; `None` for
    // shells that never trigger render init. The same per-frame publish path
    // publishes a fresh [`Self::predicate_context`] (live PlayState / can_* /
    // is_editing / selection) BEFORE the egui pass, so the host re-resolves the
    // menu and greys disabled items this frame.
    pub(crate) predicate_context_handoff: Option<Arc<PredicateContextHandoff>>,

    // ---- Menu command FIFO (MENUBAR-FILE-WIRING) ----------------------------
    //
    // Cloned `Arc` to the host's [`MenuCommandHandoff`] — a host→shell FIFO, NOT
    // a latest-only snapshot like the three handoffs above. Set in
    // [`crate::render_path::EditorShell::init_render_state`] alongside them;
    // `None` for shells that never trigger render init. Drained at the TOP of
    // each `render_frame` (before this frame's render borrows) by
    // [`crate::render_path::EditorShell::drain_and_route_menu_commands`], which
    // routes each `Command` one-way through the shared menu-command sink.
    pub(crate) menu_command_handoff: Option<Arc<MenuCommandHandoff>>,

    /// Shell-owned FIFO of extension commands captured by
    /// [`crate::render_path::EditorShell::route_menu_command`].
    ///
    /// Holds `Command::Custom` / `Command::Plugin` for a future plugin/action
    /// executor. Capturing keeps extension menu activations observable without
    /// pretending the editor-shell can execute them before a runtime is wired.
    pub(crate) extension_menu_commands: Vec<Command>,

    /// Optional shell-owned handler for already-captured extension commands.
    ///
    /// This is an injected seam only. It receives commands from
    /// [`crate::render_path::EditorShell::route_menu_command`]'s extension
    /// capture path and does not imply plugin runtime/discovery/loading.
    pub(crate) extension_command_handler: Option<Box<dyn ExtensionCommandHandler>>,

    /// Observable outcomes from the extension-command seam.
    pub(crate) extension_command_events: Vec<ExtensionCommandEvent>,

    /// Shell-local entity clipboard for the bounded Edit Copy/Paste path.
    ///
    /// Stores cloned legacy component blobs only. It is not the OS clipboard, is
    /// not serialized, and intentionally does not clone typed kernel components
    /// or CAD graph/projection/render identity.
    entity_clipboard: Vec<EntityClipboardItem>,
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
        // `rge_kernel_ecs::World` resource — but ONLY if the caller has
        // not already pre-populated one. `insert_resource` REPLACES any
        // existing instance, so an unconditional insert would silently
        // overwrite a caller-provided `TimeScale::with_value(...)`. The
        // resource-presence check preserves caller intent (e.g. a scene
        // loader that wants the editor to start at a non-default scale).
        if world.kernel().resource::<TimeScale>().is_none() {
            world.kernel_mut().insert_resource(TimeScale::default());
        }
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
            quit_requested: false,
            command_palette_toggle_requested: false,
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
            materials: Vec::new(),
            light: None,
            meshes: Vec::new(),
            cursor_pos: None,
            highlight_material: None,
            highlight_index_buffer: None,
            texture_pool: None,
            buffer_pool: None,
            compiled_frame_graph: None,
            command_bus: CommandBus::new(),
            modifiers: ModifiersState::empty(),
            egui_host: None,
            inspector_handoff: None,
            save_status_handoff: None,
            predicate_context_handoff: None,
            menu_command_handoff: None,
            extension_menu_commands: Vec::new(),
            extension_command_handler: None,
            extension_command_events: Vec::new(),
            entity_clipboard: Vec::new(),
            prebuilt_render_meshes: Vec::new(),
            prebuilt_render_base_colors: Vec::new(),
            prebuilt_render_base_textures: Vec::new(),
            glb_source_path: None,
            reload_hook: None,
            open_dialog: None,
            scene_open_hook: None,
            save_dialog: None,
            scene_save_hook: None,
            project_save_hook: None,
            new_project_dialog: None,
            new_project_hook: None,
            unsaved_changes_dialog: None,
            save_source: None,
            last_window_title: None,
        }
    }

    /// Replace the live editor [`World`] at runtime with a caller-provided
    /// kernel world, resetting the shell to the same load-only baseline
    /// [`Self::with_world`] produces. **Editing-only** (mirrors the PIE gate
    /// on [`Self::reload_render_assets`]). The runtime substrate for in-app
    /// scene Open (EDITOR-WORLD-SWAP); the `Ctrl+O` scene-open wiring that
    /// drives it lives in [`Self::handle_open_request`] (SCENE-OPEN-WIRING).
    ///
    /// Resets to the `with_world` data baseline:
    /// - clears the CAD-mode fields (`cad_world` / `projection` / `cad_graph`
    ///   / `cad_entity`);
    /// - clears all render content (`prebuilt_render_*` + the GPU `meshes` /
    ///   `materials` + `highlight_index_buffer`) so the viewport renders blank
    ///   — `encode_main_pass` skips an empty mesh set, matching the `--scene`
    ///   load-only semantics (rendering scene entities is future work);
    /// - drops the PIE `snapshot` and resets the selection (`coord`);
    /// - installs a fresh [`CommandBus`] so an old-world undo/redo can never
    ///   replay against the new kernel world;
    /// - clears `glb_source_path` AND `save_source` (the swapped-in world
    ///   has no GLB hot-reload source; the Open handler re-commits the
    ///   save source on success for a `.rge-scene` / `.rge-project`).
    ///
    /// Preserves the GPU device/context, the editor camera, the attached
    /// loader/dialog/scene/save/unsaved hooks (`reload_hook` / `open_dialog` /
    /// `scene_open_hook` / `save_dialog` / `scene_save_hook` /
    /// `project_save_hook` / `unsaved_changes_dialog` — `Ctrl+O`, `Ctrl+S`,
    /// close guards, and the R-key reload stay wired),
    /// `PlayState`, the audit ledger, and the tick counter. `notify`-watcher
    /// teardown is the binary's concern (it reacts to the now-`None`
    /// [`Self::glb_source_path`] and drops the watcher); this method does no
    /// watcher work.
    ///
    /// # Errors
    ///
    /// Returns `Err` with NO mutation if called outside [`PlayState::Editing`]
    /// — a mid-PIE world swap would corrupt the snapshot/restore contract.
    pub fn replace_world(&mut self, world: KernelWorld) -> Result<(), String> {
        if self.play_state() != PlayState::Editing {
            return Err(format!(
                "replace_world: PIE state is {}; world swap only allowed in Editing",
                self.play_state().label()
            ));
        }

        // Install the new kernel world into a fresh wrapper (this also clears
        // the wrapper's legacy blob storage), defaulting a `TimeScale` in only
        // if the caller did not provide one — mirrors `with_world`.
        let mut wrapper = World::new();
        *wrapper.kernel_mut() = world;
        if wrapper.kernel().resource::<TimeScale>().is_none() {
            wrapper.kernel_mut().insert_resource(TimeScale::default());
        }
        self.world = wrapper;

        // Reset to the `with_world` data baseline.
        self.cad_world = None;
        self.projection = None;
        self.cad_graph = None;
        self.cad_entity = None;
        self.prebuilt_render_meshes.clear();
        self.prebuilt_render_base_colors.clear();
        self.prebuilt_render_base_textures.clear();
        self.meshes.clear();
        self.materials.clear();
        self.highlight_index_buffer = None;
        self.snapshot = None;
        self.coord = EditorCoord::new();
        self.command_bus = CommandBus::new();
        self.glb_source_path = None;
        self.save_source = None;
        self.entity_clipboard.clear();

        Ok(())
    }

    /// Reset the editor to a fresh, unsourced empty world for File -> New.
    ///
    /// This reuses [`Self::replace_world`], so it is Editing-only and inherits
    /// that method's full reset semantics: clear source, selection, render
    /// content, PIE snapshot, and command bus. It does not prompt for unsaved
    /// changes or create a project/file on disk.
    pub fn handle_new_file_request(&mut self) {
        if let Err(error) = self.replace_world(KernelWorld::new()) {
            tracing::debug!(
                target: "rge::editor-shell::menu",
                %error,
                "New file request ignored"
            );
        }
    }

    /// Close the current document into a fresh, unsourced empty world.
    ///
    /// This intentionally does not exit the application. Clean documents reuse
    /// [`Self::replace_world`] directly. Dirty documents first ask the injected
    /// [`UnsavedChangesDialog`]; Cancel or no hook leaves editor state
    /// unchanged, while Discard proceeds through the same reset path.
    pub fn handle_close_file_request(&mut self) {
        if !self.confirm_close_family_request(UnsavedChangesRequest::CloseFile) {
            return;
        }
        if let Err(error) = self.replace_world(KernelWorld::new()) {
            tracing::debug!(
                target: "rge::editor-shell::menu",
                %error,
                "Close file request ignored"
            );
        }
    }

    /// Request application exit from a command source that does not own the
    /// winit event loop.
    ///
    /// The request is consumed at the next event-loop boundary and routed
    /// through the same `ActiveEventLoop::exit` path as `CloseRequested`.
    /// This is application quit, not document close. Clean documents set the
    /// pending request directly. Dirty documents first ask the injected
    /// [`UnsavedChangesDialog`]; Cancel or no hook leaves editor state and the
    /// pending request unchanged, while Discard sets the same pending request.
    pub fn handle_quit_request(&mut self) {
        if !self.confirm_close_family_request(UnsavedChangesRequest::QuitApplication) {
            return;
        }
        self.quit_requested = true;
    }

    /// Decide whether a `WindowEvent::CloseRequested` should call
    /// `ActiveEventLoop::exit`. Exposed inside the module for deterministic
    /// tests without constructing a native dialog or event loop.
    #[must_use]
    fn should_exit_on_window_close_request(&self) -> bool {
        self.confirm_close_family_request(UnsavedChangesRequest::WindowClose)
    }

    /// Shared guard for dirty close-family requests.
    ///
    /// Clean requests bypass the hook entirely. Dirty requests with no hook are
    /// conservative and behave as Cancel.
    #[must_use]
    fn confirm_close_family_request(&self, request: UnsavedChangesRequest) -> bool {
        if !self.command_bus().is_dirty() {
            return true;
        }

        let context = UnsavedChangesContext::from_save_source(request, self.save_source.as_ref());
        let Some(dialog) = self.unsaved_changes_dialog.as_ref() else {
            tracing::warn!(
                target: "rge::editor-shell::unsaved_changes",
                ?request,
                "dirty close-family request cancelled: no unsaved_changes_dialog attached"
            );
            return false;
        };

        match dialog.confirm_discard_unsaved_changes(&context) {
            UnsavedChangesDecision::Discard => true,
            UnsavedChangesDecision::Cancel => {
                tracing::info!(
                    target: "rge::editor-shell::unsaved_changes",
                    ?request,
                    "dirty close-family request cancelled by user"
                );
                false
            }
        }
    }

    fn take_quit_request(&mut self) -> bool {
        let requested = self.quit_requested;
        self.quit_requested = false;
        requested
    }

    /// Request that the future command-palette UI toggle at the next consumer
    /// boundary.
    ///
    /// This deliberately does not create or render a palette. It only makes the
    /// existing core `Command::ToggleCommandPalette` observable instead of
    /// letting it disappear at the menu-command router.
    pub fn handle_command_palette_toggle_request(&mut self) {
        self.command_palette_toggle_requested = true;
    }

    pub(crate) fn take_command_palette_toggle_request(&mut self) -> bool {
        let requested = self.command_palette_toggle_requested;
        self.command_palette_toggle_requested = false;
        requested
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
            quit_requested: false,
            command_palette_toggle_requested: false,
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
            materials: Vec::new(),
            light: None,
            meshes: Vec::new(),
            cursor_pos: None,
            highlight_material: None,
            highlight_index_buffer: None,
            texture_pool: None,
            buffer_pool: None,
            compiled_frame_graph: None,
            command_bus: CommandBus::new(),
            modifiers: ModifiersState::empty(),
            egui_host: None,
            inspector_handoff: None,
            save_status_handoff: None,
            predicate_context_handoff: None,
            menu_command_handoff: None,
            extension_menu_commands: Vec::new(),
            extension_command_handler: None,
            extension_command_events: Vec::new(),
            entity_clipboard: Vec::new(),
            prebuilt_render_meshes: Vec::new(),
            prebuilt_render_base_colors: Vec::new(),
            prebuilt_render_base_textures: Vec::new(),
            glb_source_path: None,
            reload_hook: None,
            open_dialog: None,
            scene_open_hook: None,
            save_dialog: None,
            scene_save_hook: None,
            project_save_hook: None,
            new_project_dialog: None,
            new_project_hook: None,
            unsaved_changes_dialog: None,
            save_source: None,
            last_window_title: None,
        }
    }

    /// Construct an [`EditorShell`] with a single render-only mesh
    /// (no CAD). **Dispatch G entry point**, kept as a backward-compat
    /// wrapper around the dispatch-I multi-mesh
    /// [`Self::with_render_meshes`].
    ///
    /// All caveats and doctrinal notes documented on
    /// [`Self::with_render_meshes`] apply — this method just routes a
    /// single-mesh input through the same construction path. Useful
    /// for callers (tests, future single-mesh ingestion paths) that
    /// don't want to spell out `vec![mesh]` at the call site.
    #[must_use]
    pub fn with_render_mesh(mesh: RenderMesh) -> Self {
        Self::with_render_meshes(vec![mesh])
    }

    /// Construct an [`EditorShell`] with N render-only meshes (no CAD).
    /// **Dispatch I entry point** for `rge-editor --glb <path>` —
    /// renders every mesh primitive of the loaded glTF/GLB file, not
    /// just the first one.
    ///
    /// The supplied [`RenderMesh`] sequence is typically built by the
    /// editor binary from a glTF/GLB file via `rge_io_gltf::import_glb`
    /// + per-primitive `RenderMesh::from_buffers(positions, indices,
    /// None)` (in scene-entity order). Each mesh becomes one
    /// [`rge_gfx::LitMesh`] during render init; the render pass
    /// draws them in order through a single pipeline + bind-group
    /// state.
    ///
    /// # Render-only semantics
    ///
    /// All caveats apply (same as the dispatch-G single-mesh
    /// constructor):
    ///
    /// - No CAD operator graph; `cad_graph` is `None`.
    /// - No CAD projection; `projection` is `None`.
    /// - No CAD ECS world; `cad_world` is `None`.
    /// - No B-Rep face labels — face-pick silently no-ops via the
    ///   existing `handle_left_click` projection-None guard.
    /// - No save / undo for any loaded mesh.
    /// - No materials / textures — all meshes render against the
    ///   hardcoded white-1×1 Lambert+Phong material.
    /// - No glTF node transforms — every mesh renders at the local
    ///   origin regardless of its glTF placement.
    ///
    /// The wrapper [`crate::world::World`] is still constructed with a
    /// `TimeScale::default()` resource so the inspector snapshot's
    /// time-scale field reads as `1.00x` and the playback shortcuts
    /// (`Space` / `Escape`) still work.
    ///
    /// # Camera framing
    ///
    /// The camera frames the **union AABB** over every supplied
    /// mesh's positions via [`compute_aabb_union`] +
    /// [`isometric_camera_for_bounds`]. If the union is empty or
    /// non-finite (e.g. every mesh was malformed; in practice
    /// `RenderMesh::from_buffers` would have panicked), the camera
    /// falls back to [`EditorCameraState::default()`].
    ///
    /// # Zero-mesh policy (defensive)
    ///
    /// An empty `meshes` Vec is accepted defensively:
    /// `init_render_state` will no-op when both CAD fields AND the
    /// prebuilt-mesh Vec are empty (matching the W03 "no scene
    /// attached" path). The `rge-editor` binary REJECTS zero-mesh
    /// glTF files before reaching this constructor, so in production
    /// this branch should never fire.
    ///
    /// # Doctrinal note
    ///
    /// Imported meshes are NOT CAD bodies. Per
    /// `rge_authority_fragmentation_risk.md` ("kittycad governs the
    /// spec; resist parallel enums / mirror types / ML-specific IRs /
    /// shadow models"), this dispatch deliberately does NOT add an
    /// `rge_cad_core::OperatorNode::ImportedMesh` variant — the
    /// canonical operator IR stays as kittycad defines it. Imported
    /// meshes live entirely in the editor's render path, never
    /// crossing into the CAD authority surface.
    #[must_use]
    pub fn with_render_meshes(meshes: Vec<RenderMesh>) -> Self {
        // Backward-compat wrapper. Dispatch K added base_color; M2
        // added base_color_texture. Both get filled with the
        // documented defaults (white tint, no texture) so callers
        // who only have a `Vec<RenderMesh>` see identical pre-K /
        // pre-M2 behaviour.
        let n = meshes.len();
        Self::with_render_meshes_and_base_colors_and_textures(
            meshes,
            vec![[1.0, 1.0, 1.0, 1.0]; n],
            vec![None; n],
        )
    }

    /// Construct an [`EditorShell`] with N render-only meshes plus
    /// matching per-mesh `base_color` factors (no CAD).
    /// **Dispatch K entry point** for `rge-editor --glb <path>` —
    /// renders every mesh primitive with the colour resolved from
    /// the glTF `MaterialAsset`, not a hardcoded white.
    ///
    /// Each `base_colors[i]` is a linear-space `[r, g, b, a]` that
    /// will be uploaded into the matching mesh's
    /// [`rge_gfx::Material`] UBO during
    /// `init_render_state_post_surface`. The render path's per-mesh
    /// `set_bind_group(2, materials[i].bind_group(), ..)` then
    /// produces correct tinting in the Lambert+Phong fragment
    /// shader.
    ///
    /// # Length invariant
    ///
    /// `meshes.len() == base_colors.len()` is REQUIRED. Mismatched
    /// lengths indicate a caller contract violation (the editor
    /// binary's `load_all_glb_meshes` guarantees alignment); we
    /// panic with a clear message rather than silently truncating
    /// or padding.
    ///
    /// # Render-only semantics
    ///
    /// All caveats documented on [`Self::with_render_meshes`]
    /// apply unchanged — this constructor only adds the per-mesh
    /// colour axis. Textures / PBR / animation / face-pick / save /
    /// undo remain explicitly out of scope.
    ///
    /// # Panics
    ///
    /// Panics if `meshes.len() != base_colors.len()`.
    #[must_use]
    pub fn with_render_meshes_and_base_colors(
        meshes: Vec<RenderMesh>,
        base_colors: Vec<[f32; 4]>,
    ) -> Self {
        let n = meshes.len();
        Self::with_render_meshes_and_base_colors_and_textures(meshes, base_colors, vec![None; n])
    }

    /// Construct an [`EditorShell`] with N render-only meshes, N
    /// per-mesh `base_color` factors, and N per-mesh optional
    /// embedded `base_color_texture` payloads (no CAD).
    /// **Dispatch M2 entry point** for `rge-editor --glb <path>` —
    /// renders glTF base-colour textures sampled per-fragment when
    /// present, otherwise tints the per-mesh material by `base_color`
    /// against the existing 1×1 white placeholder texture.
    ///
    /// `textures[i]` semantics:
    /// - `Some((width, height, pixels))`: `pixels.len() == width *
    ///   height * 4` (RGBA8). The render path's `Material::new` is
    ///   called with these bytes; the resulting bind group samples
    ///   the texture in the fragment shader.
    /// - `None`: the editor shell uses the existing
    ///   `WHITE_1X1_RGBA` placeholder texture for that mesh's
    ///   `Material`. The dispatch-K `base_color` tint still
    ///   applies, so the mesh renders as a uniform-tinted Lambert+
    ///   Phong surface.
    ///
    /// # Length invariants
    ///
    /// `meshes.len() == base_colors.len() == textures.len()`.
    /// Mismatched lengths indicate a caller contract violation
    /// (the editor binary's `load_all_glb_meshes` returns aligned
    /// Vecs by construction); we panic with a clear message.
    ///
    /// # Render-only semantics
    ///
    /// All caveats documented on [`Self::with_render_meshes`]
    /// apply unchanged — this constructor only adds the per-mesh
    /// texture-pixel axis. PBR / normal / metallic-roughness
    /// textures, samplers, animation, face-pick, save / undo all
    /// remain explicitly out of scope.
    ///
    /// # Panics
    ///
    /// Panics if `meshes.len() != base_colors.len()` or `meshes.
    /// len() != textures.len()`.
    #[must_use]
    pub fn with_render_meshes_and_base_colors_and_textures(
        meshes: Vec<RenderMesh>,
        base_colors: Vec<[f32; 4]>,
        textures: Vec<Option<(u32, u32, Vec<u8>)>>,
    ) -> Self {
        assert_eq!(
            meshes.len(),
            base_colors.len(),
            "with_render_meshes_and_base_colors_and_textures: meshes ({}) and base_colors ({}) must have matching length",
            meshes.len(),
            base_colors.len(),
        );
        assert_eq!(
            meshes.len(),
            textures.len(),
            "with_render_meshes_and_base_colors_and_textures: meshes ({}) and textures ({}) must have matching length",
            meshes.len(),
            textures.len(),
        );

        // Install `TimeScale` as a resource on the editor wrapper world
        // so the inspector + playback shortcuts work identically to
        // the CAD-driven path. Same `world` construction shape as
        // `with_world_projection_graph` minus the CAD plumbing.
        let mut world = World::new();
        world.kernel_mut().insert_resource(TimeScale::default());
        // Dispatch I — auto-frame the camera against the UNION of all
        // supplied meshes' AABBs. Falls back to the default editor
        // camera when the union is empty / non-finite.
        let editor_camera = match compute_aabb_union(&meshes) {
            Some((min, max)) => isometric_camera_for_bounds(min, max),
            None => EditorCameraState::default(),
        };
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
            quit_requested: false,
            command_palette_toggle_requested: false,
            editor_camera,
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
            materials: Vec::new(),
            light: None,
            meshes: Vec::new(),
            cursor_pos: None,
            highlight_material: None,
            highlight_index_buffer: None,
            texture_pool: None,
            buffer_pool: None,
            compiled_frame_graph: None,
            command_bus: CommandBus::new(),
            modifiers: ModifiersState::empty(),
            egui_host: None,
            inspector_handoff: None,
            save_status_handoff: None,
            predicate_context_handoff: None,
            menu_command_handoff: None,
            extension_menu_commands: Vec::new(),
            extension_command_handler: None,
            extension_command_events: Vec::new(),
            entity_clipboard: Vec::new(),
            prebuilt_render_meshes: meshes,
            prebuilt_render_base_colors: base_colors,
            prebuilt_render_base_textures: textures,
            glb_source_path: None,
            reload_hook: None,
            open_dialog: None,
            scene_open_hook: None,
            save_dialog: None,
            scene_save_hook: None,
            project_save_hook: None,
            new_project_dialog: None,
            new_project_hook: None,
            unsaved_changes_dialog: None,
            save_source: None,
            last_window_title: None,
        }
    }

    /// Attach a glb source path + reload hook for the R-key handler.
    ///
    /// Called by the editor binary (`rge-editor::main`) after building
    /// the shell via [`Self::with_render_meshes_and_base_colors_and_textures`]
    /// on the `--glb <path>` flag. The default cuboid-demo path
    /// never calls this — R-key silently no-ops there.
    ///
    /// `path` is the file the user passed to `--glb`. The handler
    /// passes it to `hook.reload_glb(path)` on every R press.
    ///
    /// `hook` is the binary-owned loader callback that knows how to
    /// re-import the glTF/GLB at `path`. See [`AssetReloadHook`]
    /// for the contract — the binary's hook impl typically wraps its
    /// own `load_all_glb_meshes` helper.
    ///
    /// Idempotent: calling twice replaces the previous source + hook.
    /// Tests use this to swap loaders mid-shell-lifetime.
    pub fn attach_glb_reload_source<H: AssetReloadHook + 'static>(
        &mut self,
        path: PathBuf,
        hook: H,
    ) {
        self.glb_source_path = Some(path);
        self.reload_hook = Some(Box::new(hook));
    }

    /// Attach a GLB loader hook WITHOUT a source path.
    ///
    /// Companion to [`Self::attach_glb_reload_source`] for launch modes
    /// that have no `--glb` file yet (the default cuboid demo and the
    /// `--scene` path): the `.glb` branch of the in-app Open handler
    /// ([`Self::handle_open_request`]) needs a loader hook to import a
    /// user-picked GLB, but there is no initial source path to reload
    /// with R until the user actually opens one. This sets only
    /// [`Self::reload_hook`]; [`Self::glb_source_path`] stays `None`, so
    /// R-key reload correctly no-ops until a successful GLB Open commits a
    /// path. After that first GLB Open, R-key follows the opened file.
    /// (Scene Opens route through [`Self::with_scene_open_hook`] instead
    /// and set no GLB source.)
    ///
    /// Safe to call in any mode because the v0 [`AssetReloadHook`] impl
    /// (`rge-editor::GlbLoaderHook`) is stateless — it re-imports from
    /// whatever path it is handed on each call. Idempotent: calling
    /// twice replaces the previous hook.
    pub fn attach_glb_loader_hook<H: AssetReloadHook + 'static>(&mut self, hook: H) {
        self.reload_hook = Some(Box::new(hook));
    }

    /// Attach a native "Open" file dialog for the `Ctrl+O` handler.
    ///
    /// Called by the editor binary (`rge-editor::main`) in every launch
    /// mode (default cuboid demo, `--glb`, `--scene`) so `Ctrl+O` works
    /// from any starting state. The `dialog` is the binary-owned `rfd`
    /// impl of [`GlbOpenDialog`] (kept GLB-prefixed in name though it now
    /// offers `.glb` / `.rge-scene` / `.rge-project`); editor-shell holds
    /// only the boxed trait object and never gains an `rfd` dependency.
    ///
    /// Consuming builder (`mut self -> Self`) so it composes in the
    /// binary's construction chain alongside the other `with_*`
    /// constructors. The `.glb` branch of [`Self::handle_open_request`]
    /// also requires a loader hook via [`Self::attach_glb_reload_source`]
    /// / [`Self::attach_glb_loader_hook`], and the scene branch requires
    /// [`Self::with_scene_open_hook`]; with a dialog but the relevant hook
    /// missing, `Ctrl+O` warn-logs and no-ops (the binary attaches all
    /// three).
    #[must_use]
    pub fn with_glb_open_dialog(mut self, dialog: Box<dyn GlbOpenDialog>) -> Self {
        self.open_dialog = Some(dialog);
        self
    }

    /// Attach a scene-open loader hook for the `Ctrl+O` scene path.
    ///
    /// Called by the editor binary (`rge-editor::main`) in every launch
    /// mode so opening a `.rge-scene` / `.rge-project` works from any
    /// starting state. The `hook` is the binary-owned `rge-scene-loader`
    /// impl of [`SceneOpenHook`]; editor-shell holds only the boxed trait
    /// object and never gains an `rge-scene-loader` / `rge-data`
    /// dependency.
    ///
    /// Consuming builder (`mut self -> Self`) so it composes in the
    /// binary's construction chain alongside [`Self::with_glb_open_dialog`]
    /// and the other `with_*` constructors. The scene branch of
    /// [`Self::handle_open_request`] requires this hook — with a dialog
    /// but no scene hook, a picked scene path warn-logs and no-ops.
    #[must_use]
    pub fn with_scene_open_hook(mut self, hook: Box<dyn SceneOpenHook>) -> Self {
        self.scene_open_hook = Some(hook);
        self
    }

    /// Attach a native "Save" file dialog for the `Ctrl+S` (Save-As) handler.
    ///
    /// Called by the editor binary (`rge-editor::main`) in every launch mode so
    /// `Ctrl+S` works from any starting state. The `dialog` is the binary-owned
    /// `rfd` impl of [`SceneSaveDialog`]; editor-shell holds only the boxed
    /// trait object and never gains an `rfd` dependency.
    ///
    /// Consuming builder (`mut self -> Self`) so it composes in the binary's
    /// construction chain alongside [`Self::with_glb_open_dialog`] and the other
    /// `with_*` constructors. [`Self::handle_save_request`] also requires a
    /// writer via [`Self::with_scene_save_hook`]; with a dialog but no writer,
    /// `Ctrl+S` warn-logs and no-ops.
    #[must_use]
    pub fn with_scene_save_dialog(mut self, dialog: Box<dyn SceneSaveDialog>) -> Self {
        self.save_dialog = Some(dialog);
        self
    }

    /// Attach a scene-save writer hook for the `Ctrl+S` (Save-As) handler.
    ///
    /// Called by the editor binary (`rge-editor::main`) in every launch mode so
    /// Save-As writes a `.rge-scene` from any starting state. The `hook` is the
    /// binary-owned `rge-scene-loader` impl of [`SceneSaveHook`]; editor-shell
    /// holds only the boxed trait object and never gains an `rge-scene-loader` /
    /// `rge-data` dependency.
    ///
    /// Consuming builder (`mut self -> Self`) so it composes in the binary's
    /// construction chain. [`Self::handle_save_request`] requires this hook —
    /// with a dialog but no writer, `Ctrl+S` warn-logs and no-ops.
    #[must_use]
    pub fn with_scene_save_hook(mut self, hook: Box<dyn SceneSaveHook>) -> Self {
        self.scene_save_hook = Some(hook);
        self
    }

    /// Attach a project-save writer hook for the `Ctrl+S` (`.rge-project`)
    /// handler.
    ///
    /// Called by the editor binary (`rge-editor::main`) in every launch mode so
    /// `Ctrl+S` on an open `.rge-project` writes the world back to it. The
    /// `hook` is the binary-owned `rge-scene-loader` impl of [`ProjectSaveHook`]
    /// (over `save_project_world_to_path`); editor-shell holds only the boxed
    /// trait object and never gains an `rge-scene-loader` / `rge-data`
    /// dependency. The project-axis companion to [`Self::with_scene_save_hook`].
    ///
    /// Consuming builder (`mut self -> Self`) so it composes in the binary's
    /// construction chain. The `Project` arm of [`Self::handle_save_request`]
    /// requires this hook — with a `Project` source but no writer, `Ctrl+S`
    /// warn-logs and no-ops.
    #[must_use]
    pub fn with_project_save_hook(mut self, hook: Box<dyn ProjectSaveHook>) -> Self {
        self.project_save_hook = Some(hook);
        self
    }

    /// Attach a native folder picker for the `Ctrl+Shift+S` (Save-As to a NEW
    /// `.rge-project` tree) handler.
    ///
    /// Called by the editor binary (`rge-editor::main`) in every launch mode.
    /// The `dialog` is the binary-owned `rfd` impl of [`NewProjectSaveDialog`];
    /// editor-shell holds only the boxed trait object and never gains an `rfd`
    /// dependency. The new-project companion to [`Self::with_scene_save_dialog`].
    ///
    /// Consuming builder (`mut self -> Self`).
    /// [`Self::handle_save_as_new_project_request`] also requires a writer via
    /// [`Self::with_new_project_save_hook`]; with a dialog but no writer,
    /// `Ctrl+Shift+S` warn-logs and no-ops.
    #[must_use]
    pub fn with_new_project_save_dialog(mut self, dialog: Box<dyn NewProjectSaveDialog>) -> Self {
        self.new_project_dialog = Some(dialog);
        self
    }

    /// Attach a new-project writer hook for the `Ctrl+Shift+S` handler.
    ///
    /// Called by the editor binary (`rge-editor::main`) in every launch mode.
    /// The `hook` is the binary-owned `rge-scene-loader` impl of
    /// [`NewProjectSaveHook`] (over `save_world_as_new_project`); editor-shell
    /// holds only the boxed trait object and never gains an `rge-scene-loader` /
    /// `rge-data` dependency. The new-project companion to
    /// [`Self::with_project_save_hook`].
    ///
    /// Consuming builder (`mut self -> Self`).
    /// [`Self::handle_save_as_new_project_request`] requires this hook — with a
    /// dialog but no writer, `Ctrl+Shift+S` warn-logs and no-ops.
    #[must_use]
    pub fn with_new_project_save_hook(mut self, hook: Box<dyn NewProjectSaveHook>) -> Self {
        self.new_project_hook = Some(hook);
        self
    }

    /// Attach an unsaved-changes confirmation hook for dirty close-family
    /// requests.
    ///
    /// Called by the editor binary (`rge-editor::main`) in every launch mode so
    /// File -> Close, File -> Quit, and window close can ask whether to discard
    /// unsaved changes. The hook is binary-owned and may use a native dialog;
    /// editor-shell holds only the boxed trait object. Dirty requests with no
    /// hook are treated as Cancel.
    #[must_use]
    pub fn with_unsaved_changes_dialog(mut self, dialog: Box<dyn UnsavedChangesDialog>) -> Self {
        self.unsaved_changes_dialog = Some(dialog);
        self
    }

    /// Seed the [`SaveSource`] a `Ctrl+S` writes back to.
    ///
    /// Called by the editor binary (`rge-editor::main`) on a `--scene <file>`
    /// launch so the first `Ctrl+S` writes straight back to the launched file
    /// (no Save-As prompt): [`SaveSource::Scene`] for a `.rge-scene`,
    /// [`SaveSource::Project`] for a literal `.rge-project`. Equivalent to a
    /// successful Open committing the source via [`Self::handle_open_request`].
    ///
    /// Consuming builder (`mut self -> Self`) so it composes in the binary's
    /// construction chain.
    #[must_use]
    pub fn with_save_source(mut self, source: SaveSource) -> Self {
        self.save_source = Some(source);
        self
    }

    // ---- accessors (read-only) ---------------------------------------------

    /// The current glb hot-reload source path, if any.
    ///
    /// `Some(path)` after a `--glb <path>` launch (via
    /// [`Self::attach_glb_reload_source`]) OR after a successful in-app
    /// Open (via [`Self::handle_open_request`], which commits the
    /// picked path only once the load + swap succeed). `None` for the
    /// default cuboid demo / `--scene` path until the user opens a GLB.
    ///
    /// Exposed so tests can assert the commit-after-success ordering of
    /// [`Self::handle_open_request`] directly (a failed Open must leave
    /// this unchanged; a successful Open must commit the new path).
    #[must_use]
    pub fn glb_source_path(&self) -> Option<&std::path::Path> {
        self.glb_source_path.as_deref()
    }

    /// The current [`SaveSource`] a `Ctrl+S` writes back to, if any.
    ///
    /// `Some(_)` after opening / launching a `.rge-scene` / `.rge-project` or a
    /// successful Save-As; `Ctrl+S` routes by variant. `None` for a blank /
    /// demo / `.glb` context (where `Ctrl+S` is Save-As). Exposed so tests can
    /// assert the commit-on-Open / commit-on-Save-As ordering.
    #[must_use]
    pub fn save_source(&self) -> Option<&SaveSource> {
        self.save_source.as_ref()
    }

    /// The on-disk path of the current [`SaveSource`], if any — a display
    /// convenience for the window title / status bar (which take an
    /// `Option<&Path>`). `Some(path)` is the `.rge-scene` file or the literal
    /// `.rge-project`; `None` matches [`Self::save_source`] being `None`.
    #[must_use]
    pub fn save_source_path(&self) -> Option<&std::path::Path> {
        self.save_source.as_ref().map(SaveSource::path)
    }

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

    /// Replace the entity selection with every live entity in the editor world.
    ///
    /// This is coordination-state only: it does not mutate CAD geometry,
    /// components, face selection, or the undo stack. Used by
    /// `Command::SelectAll` from the canonical Edit menu.
    pub fn select_all_entities(&mut self) {
        let entities: Vec<_> = self.world.entities().collect();
        self.coord.selection.replace_with(entities);
    }

    /// Delete currently selected entities from the editor wrapper world.
    ///
    /// This removes entity IDs and legacy component blobs, then clears entity
    /// selection and prunes face selections whose entity was deleted. It does
    /// not mutate CAD graphs, projection caches, render meshes, or the undo
    /// stack; those authoritative deletion paths need a separate command-bus
    /// action.
    pub fn delete_selected_entities(&mut self) -> usize {
        let selected: Vec<_> = self.coord.selection.iter().collect();
        if selected.is_empty() {
            return 0;
        }

        let selected_set: std::collections::BTreeSet<_> = selected.iter().copied().collect();
        let mut deleted = 0;
        for entity in selected {
            if self.world.despawn(entity) {
                deleted += 1;
            }
        }

        self.coord.selection.clear();
        let (survivors, _) = self
            .coord
            .face_selection
            .partition(|selection| !selected_set.contains(&selection.entity));
        self.coord.face_selection = survivors;
        deleted
    }

    /// Duplicate currently selected legacy-blob entities in the editor world.
    ///
    /// The new duplicates receive cloned legacy component blobs and become the
    /// entity selection. Face selection is cleared because no authoritative
    /// face-ID remapping exists for this bounded wrapper-world operation.
    pub fn duplicate_selected_entities(&mut self) -> Vec<crate::world::EntityId> {
        let selected: Vec<_> = self.coord.selection.iter().collect();
        if selected.is_empty() {
            return Vec::new();
        }

        let duplicates: Vec<_> = selected
            .into_iter()
            .filter_map(|entity| self.world.duplicate_entity_blobs(entity))
            .collect();
        self.coord
            .selection
            .replace_with(duplicates.iter().copied());
        self.coord.face_selection.clear();
        duplicates
    }

    /// Copy currently selected legacy-blob entities into the shell clipboard.
    ///
    /// This stores cloned legacy component blobs only. It does not touch the OS
    /// clipboard, typed kernel components, CAD graph/projection data, render
    /// meshes, the command bus, or dirty/undo state.
    pub fn copy_selected_entities(&mut self) -> usize {
        let selected: Vec<_> = self.coord.selection.iter().collect();
        if selected.is_empty() {
            return 0;
        }

        self.entity_clipboard = selected
            .into_iter()
            .filter_map(|entity| self.world.clone_entity_blobs(entity))
            .collect();
        self.entity_clipboard.len()
    }

    /// Paste shell-local copied legacy-blob entities into the editor world.
    ///
    /// Pasted entities receive fresh entity IDs, cloned legacy component blobs,
    /// and become the entity selection. Face selection is cleared because no
    /// authoritative face-ID remapping exists for this bounded wrapper-world
    /// operation.
    pub fn paste_copied_entities(&mut self) -> Vec<crate::world::EntityId> {
        if self.entity_clipboard.is_empty() {
            return Vec::new();
        }

        let snapshots = self.entity_clipboard.clone();
        let pasted: Vec<_> = snapshots
            .into_iter()
            .map(|components| self.world.spawn_with_component_blobs(components))
            .collect();
        self.coord.selection.replace_with(pasted.iter().copied());
        self.coord.face_selection.clear();
        pasted
    }

    /// Cut currently selected legacy-blob entities from the editor world.
    ///
    /// This is Copy followed by the existing bounded wrapper-world Delete path:
    /// the shell-local clipboard receives cloned legacy component blobs before
    /// selected entities are despawned. It does not touch the OS clipboard,
    /// typed kernel components, CAD graph/projection data, render meshes, the
    /// command bus, or dirty/undo state.
    pub fn cut_selected_entities(&mut self) -> usize {
        let selected: Vec<_> = self.coord.selection.iter().collect();
        if selected.is_empty() {
            return 0;
        }

        self.entity_clipboard = selected
            .iter()
            .filter_map(|entity| self.world.clone_entity_blobs(*entity))
            .collect();
        self.delete_selected_entities()
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

    /// Read-only snapshot of editor-session state for the headless
    /// inspector model. Builds a fresh [`crate::InspectorSnapshot`] from
    /// already-public accessors; pure read, zero side effects, zero
    /// allocations. See [`crate::inspector`] for the field-by-field
    /// stability contract.
    ///
    /// The snapshot reflects the editor's observable state at the moment
    /// of the call — there is no caching. A test or future inspector
    /// widget can call this once per frame (or once per redraw) without
    /// inducing audit-ledger noise, bus submits, or resource churn.
    #[must_use]
    pub fn inspector_snapshot(&self) -> crate::InspectorSnapshot {
        let bus = self.command_bus();
        crate::InspectorSnapshot {
            time_scale: self.time_scale().value(),
            play_state_label: self.state.label(),
            tick_count: self.tick_count,
            has_snapshot: self.snapshot.is_some(),
            active_tool_label: self.coord.active_tool.label(),
            selection_len: self.coord.selection.len(),
            face_selection_len: self.coord.face_selection.len(),
            is_dirty: bus.is_dirty(),
            undo_stack_len: bus.stack().len(),
            undo_cursor: bus.stack().cursor(),
        }
    }

    /// Build a fresh [`rge_editor_state::SaveStatusSnapshot`] for the bottom
    /// status bar — the open save source's display name (the `.rge-scene` file
    /// name, or a `.rge-project`'s manifest name — its folder name when unnamed —
    /// via [`SaveSource::display_name`]) + the Command-Bus dirty flag. Pure read,
    /// zero side effects; mirrors [`Self::inspector_snapshot`]. Produced fresh
    /// per frame and published through `save_status_handoff` BEFORE the egui pass.
    #[must_use]
    pub fn save_status_snapshot(&self) -> rge_editor_state::SaveStatusSnapshot {
        rge_editor_state::SaveStatusSnapshot {
            source_name: self
                .save_source()
                .and_then(SaveSource::display_name)
                .map(std::string::ToString::to_string),
            is_dirty: self.command_bus().is_dirty(),
        }
    }

    /// The live [`PredicateContext`](rge_editor_ui::menus::PredicateContext) for
    /// this frame — drives the canonical menu registry's enablement (and
    /// visibility) predicates when the host re-resolves the menu. Pure read, zero
    /// side effects; mirrors [`Self::save_status_snapshot`]. `can_*` come straight
    /// from the canonical `PlayState::can_*` queries (the host/menu re-encodes no
    /// validity rule); `is_editing` is the non-PIE (Editing) state — gates the File
    /// Save/Open/Save-As items; `has_selection` reflects the entity selection;
    /// `has_frameable_scene` reflects the same live bounds source that
    /// [`Self::reset_camera`] consumes; `focused_tab` is unset (the host owns tab
    /// focus, not the shell). Produced fresh per frame and published through
    /// `predicate_context_handoff` BEFORE the egui pass.
    #[must_use]
    #[allow(clippy::field_reassign_with_default)]
    pub fn predicate_context(&self) -> rge_editor_ui::menus::PredicateContext {
        // `PredicateContext` is `#[non_exhaustive]` (owned by editor-ui), so it
        // cannot be built with a struct literal from this crate — start from
        // `default()` and set the pub fields.
        let mut ctx = rge_editor_ui::menus::PredicateContext::default();
        ctx.play_state = self.state.label().to_ascii_lowercase();
        ctx.has_selection = self.coord.selection.len() > 0;
        ctx.has_selectable_entities = self.world.entity_count() > 0;
        ctx.has_clipboard_entities = !self.entity_clipboard.is_empty();
        ctx.can_play = self.state.can_play();
        ctx.can_pause = self.state.can_pause();
        ctx.can_stop = self.state.can_stop();
        ctx.can_step = self.state.can_step();
        ctx.is_editing = !self.state.is_pie_active();
        ctx.has_frameable_scene = self.current_scene_bounds().is_some();
        ctx
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

    /// The live scene's renderable AABB union, sourced the same way as
    /// the render path's mesh step (`render_path.rs` Step 6): the
    /// prebuilt mesh Vec when populated (the `--glb` render-only path),
    /// else the single CAD projection mesh. `None` when nothing is
    /// frameable — e.g. a fresh [`EditorShell::new`] with neither side
    /// populated.
    fn current_scene_bounds(&self) -> Option<Aabb> {
        if !self.prebuilt_render_meshes.is_empty() {
            compute_aabb_union(&self.prebuilt_render_meshes)
        } else if let (Some(entity), Some(projection), Some(cad_world)) = (
            self.cad_entity,
            self.projection.as_ref(),
            self.cad_world.as_ref(),
        ) {
            let mesh = projection.render_mesh_for(entity, cad_world)?;
            compute_aabb_union(std::slice::from_ref(&mesh))
        } else {
            None
        }
    }

    /// View → Reset Camera. Reframe [`Self::editor_camera`] to the
    /// isometric view of the live scene's AABB union (via
    /// [`compute_aabb_union`] + [`isometric_camera_for_bounds`]),
    /// falling back to [`EditorCameraState::default`] when the scene is
    /// empty / non-finite. This is the runtime equivalent of the
    /// auto-frame the `with_render_meshes…` constructor performs, but
    /// against the LIVE shell instead of constructor args.
    ///
    /// Infallible: the menu router calls it directly with no error to
    /// swallow (contrast the A3 Play items' `route_play_button`, whose
    /// `handle_button` can return a benign invalid-state error).
    pub fn reset_camera(&mut self) {
        self.editor_camera = match self.current_scene_bounds() {
            Some((min, max)) => isometric_camera_for_bounds(min, max),
            None => EditorCameraState::default(),
        };
    }

    /// View -> Zoom In. Move [`Self::editor_camera`] closer to its target while
    /// preserving the current view direction, target, up vector, FOV, and clip
    /// planes. Infallible; a degenerate eye-target vector falls back to the
    /// default camera direction.
    pub fn zoom_camera_in(&mut self) {
        self.zoom_camera_by(0.8);
    }

    /// View -> Zoom Out. Move [`Self::editor_camera`] away from its target while
    /// preserving the current view direction, target, up vector, FOV, and clip
    /// planes. Uses the inverse factor of [`Self::zoom_camera_in`], so one zoom in
    /// followed by one zoom out returns to the prior distance within float error.
    pub fn zoom_camera_out(&mut self) {
        self.zoom_camera_by(1.25);
    }

    fn zoom_camera_for_viewport_mouse_wheel(
        &mut self,
        delta: &MouseScrollDelta,
        over_viewport_tab: bool,
    ) {
        match viewport_mouse_wheel_zoom_direction(delta, over_viewport_tab) {
            Some(ViewportMouseWheelZoom::In) => self.zoom_camera_in(),
            Some(ViewportMouseWheelZoom::Out) => self.zoom_camera_out(),
            None => {}
        }
    }

    fn zoom_camera_by(&mut self, factor: f32) {
        if !factor.is_finite() || factor <= 0.0 {
            return;
        }
        let offset = self.editor_camera.eye - self.editor_camera.target;
        let default_offset = EditorCameraState::default().eye - EditorCameraState::default().target;
        let offset_len = offset.length();
        let (direction, distance) =
            if offset.is_finite() && offset_len.is_finite() && offset_len > 1e-6 {
                (offset / offset_len, offset_len)
            } else {
                (default_offset.normalize(), default_offset.length())
            };
        let new_distance = (distance * factor).max(1e-3);
        self.editor_camera.eye = self.editor_camera.target + direction * new_distance;
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

    /// True if the most-recent cursor position lies over the
    /// host's [`rge_editor_egui_host::TabBody::Viewport`] tab body.
    ///
    /// Dispatch F helper. Returns `false` when:
    ///
    /// - the egui host is not constructed yet (pre-`resumed` shell),
    /// - no `CursorMoved` event has been observed yet
    ///   (`cursor_pos.is_none()`),
    /// - the host's viewport-rect sink is empty (no render frame yet)
    ///   or its mutex is poisoned,
    /// - the cursor lies outside the captured rect.
    ///
    /// Called by the `WindowEvent::MouseInput` branch in
    /// [`Self::window_event`] to decide whether a click that egui
    /// marked as `consumed` should still fall through to face-pick.
    /// `pub(crate)` so the [`should_fire_face_pick`] decision can be
    /// reasoned about without exposing the substrate publicly.
    pub(crate) fn is_pointer_over_viewport_tab(&self) -> bool {
        let Some(host) = self.egui_host.as_ref() else {
            return false;
        };
        let Some(cursor) = self.cursor_pos else {
            return false;
        };
        host.is_pointer_over_viewport(cursor)
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

// ---------------------------------------------------------------------------
// Dispatch H — render-only mesh auto-framing
// ---------------------------------------------------------------------------

/// Axis-aligned bounding box. `min[i] <= max[i]` for every axis on a
/// non-empty / non-degenerate AABB; both equal on a single-point cloud.
pub(crate) type Aabb = (glam::Vec3, glam::Vec3);

/// Compute an axis-aligned bounding box from a triangle-soup of
/// positions.
///
/// Returns `None` when:
/// - `positions` is empty (no AABB to compute), OR
/// - any coordinate is non-finite (NaN / ±Infinity), which would
///   poison the camera math downstream.
///
/// The check is defensive — production `RenderMesh::from_buffers`
/// already requires positions to be sane (an out-of-bounds index
/// would panic before reaching here). The non-finite guard exists so
/// the editor's `--glb` path treats a corrupt file as "fall back to
/// the default camera" rather than as a crash.
#[must_use]
pub(crate) fn compute_aabb(positions: &[[f32; 3]]) -> Option<Aabb> {
    if positions.is_empty() {
        return None;
    }
    let mut min = glam::Vec3::splat(f32::INFINITY);
    let mut max = glam::Vec3::splat(f32::NEG_INFINITY);
    for p in positions {
        for c in p {
            if !c.is_finite() {
                return None;
            }
        }
        let v = glam::Vec3::from(*p);
        min = min.min(v);
        max = max.max(v);
    }
    Some((min, max))
}

/// Compute the **union** axis-aligned bounding box across multiple
/// triangle-soup meshes — used by dispatch-I's
/// [`EditorShell::with_render_meshes`] to frame a camera that
/// captures EVERY primitive of a multi-mesh glTF, not just one.
///
/// Returns `None` when:
/// - the input slice is empty, OR
/// - **every** mesh yields a `None` from [`compute_aabb`] (i.e. all
///   meshes had empty positions or non-finite coordinates).
///
/// A mix of valid + invalid meshes is treated as "use the valid
/// ones": malformed entries are skipped, and the returned bounds
/// span only the meshes that compute_aabb accepted. This matches the
/// dispatch-G defensive posture: an editor that loaded a partly-
/// corrupt glTF should still frame whatever IS renderable rather
/// than collapse to the default camera or panic.
#[must_use]
pub(crate) fn compute_aabb_union(meshes: &[rge_brep_render::RenderMesh]) -> Option<Aabb> {
    let mut min = glam::Vec3::splat(f32::INFINITY);
    let mut max = glam::Vec3::splat(f32::NEG_INFINITY);
    let mut any_valid = false;
    for mesh in meshes {
        if let Some((mn, mx)) = compute_aabb(&mesh.positions) {
            min = min.min(mn);
            max = max.max(mx);
            any_valid = true;
        }
    }
    if any_valid {
        Some((min, max))
    } else {
        None
    }
}

/// Build an [`EditorCameraState`] that frames the given AABB from the
/// editor's canonical isometric direction.
///
/// # Framing math
///
/// - **Target** = AABB center (`(min + max) / 2`).
/// - **Eye direction** = canonical isometric `(1, 1, 1) / √3`. Matches
///   the default `EditorCameraState`'s eye-to-origin direction; for a
///   1×1×1 cube centered at the origin this produces eye `≈ (3, 3, 3)`
///   — the same vantage point the default-cuboid demo uses.
/// - **Distance** = `3.0 × bbox_diagonal`. The factor matches the
///   existing default's `eye→target` distance (≈ 5.196) divided by
///   the unit-cube diagonal (≈ 1.732). With `fov_y = π/4`, the AABB
///   occupies ≈ 40% of the vertical FOV — comfortably visible with
///   margin.
/// - **Near / far** scale with distance: `near = max(0.001, 0.01 ×
///   distance)`, `far = max(100.0, 10.0 × distance)`. The lower
///   bounds preserve the default-cuboid framing for small bboxes;
///   the multipliers cover any scale from sub-millimeter to
///   kilometer.
///
/// # Degenerate-bbox handling
///
/// A zero-extent AABB (`min == max`, e.g. a single-point cloud) has
/// `diagonal = 0`, which would collapse the eye onto the target. In
/// that case the function uses an **effective diagonal of 1.0** so
/// the camera sits at a sane non-zero distance from the point. The
/// rendered output is still a single point at the target — there's
/// nothing else to show — but the camera math doesn't NaN.
///
/// # Pure function
///
/// No `EditorShell` access; no I/O. The caller (currently
/// [`EditorShell::with_render_mesh`]) decides whether to apply the
/// result.
#[must_use]
pub(crate) fn isometric_camera_for_bounds(min: glam::Vec3, max: glam::Vec3) -> EditorCameraState {
    let center = (min + max) * 0.5;
    let diag = (max - min).length();
    // Degenerate handling — zero-extent AABB gets a unit-distance
    // fallback so the camera math doesn't divide by zero or place
    // the eye AT the target.
    let effective_diag = if diag < 1e-6 { 1.0 } else { diag };
    // Match the default-cuboid camera's `eye/diag ≈ 3.0` ratio.
    let distance = effective_diag * 3.0;
    // Canonical isometric direction: (1, 1, 1) / √3 (matches the
    // default `eye = (3, 3, 3)` direction from origin).
    let dir = glam::Vec3::new(1.0, 1.0, 1.0).normalize();
    let eye = center + dir * distance;
    EditorCameraState {
        eye,
        target: center,
        up: glam::Vec3::Y,
        fov_y_radians: std::f32::consts::FRAC_PI_4,
        // Floor at the default-cuboid's near/far so a 1×1×1 mesh sees
        // identical clip planes to the existing demo. Scale upward
        // for larger meshes so they're not clipped at the back.
        near: (distance * 0.01).max(0.001),
        far: (distance * 10.0).max(100.0),
    }
}

/// Dispatch F — pure decision function for the
/// `WindowEvent::MouseInput { left_pressed }` branch.
///
/// Inputs:
/// - `egui_consumed`: whether `egui_winit::State::on_window_event`
///   reported the click was consumed by an egui widget (true when the
///   pointer is over any egui-reserved rect, which today is the
///   entire window because the dock area fills it).
/// - `over_viewport_tab`: whether the cursor was over the transparent
///   [`rge_editor_egui_host::TabBody::Viewport`] body rect at the
///   moment of the click (queried via
///   [`EditorShell::is_pointer_over_viewport_tab`]).
///
/// Returns `true` iff the click should reach
/// [`EditorShell::handle_left_click`] (the face-pick path).
///
/// Truth table:
///
/// | `egui_consumed` | `over_viewport_tab` | result | rationale |
/// |---|---|---|---|
/// | `false` | `false` | `true` | Pre-dock world; pre-dispatch-D behavior. |
/// | `false` | `true`  | `true` | Pre-dock + over viewport (no conflict). |
/// | `true`  | `false` | `false` | Click on Inspector / tab chrome — egui owns it. |
/// | `true`  | `true`  | `true` | **The dispatch-F fix**: click on transparent viewport falls through. |
///
/// Equivalently: `!egui_consumed || over_viewport_tab`. Spelled as a
/// helper rather than inline so the test pinning the truth table
/// reads as plainly as the spec.
#[must_use]
pub(crate) fn should_fire_face_pick(egui_consumed: bool, over_viewport_tab: bool) -> bool {
    !egui_consumed || over_viewport_tab
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ViewportMouseWheelZoom {
    In,
    Out,
}

fn viewport_mouse_wheel_zoom_direction(
    delta: &MouseScrollDelta,
    over_viewport_tab: bool,
) -> Option<ViewportMouseWheelZoom> {
    if !over_viewport_tab {
        return None;
    }

    let vertical_cmp = match delta {
        MouseScrollDelta::LineDelta(_, y) => y.partial_cmp(&0.0),
        MouseScrollDelta::PixelDelta(pos) => pos.y.partial_cmp(&0.0),
    };

    match vertical_cmp {
        Some(std::cmp::Ordering::Greater) => Some(ViewportMouseWheelZoom::In),
        Some(std::cmp::Ordering::Less) => Some(ViewportMouseWheelZoom::Out),
        _ => None,
    }
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
        //
        // Phase 9 dispatch B: route winit events through `egui_host`
        // BEFORE the editor branches. The host's
        // `egui_winit::State::on_window_event` adapter updates egui's
        // internal input state (cursor, focus, IME, modifier tracking)
        // and returns `EventResponse { consumed, repaint }`:
        //   - `consumed == true` means an egui widget claimed the event
        //     (text field keystroke, button click). The editor's
        //     application-level handler (face-pick, Ctrl+Z) should
        //     skip this event. KEY + MOUSE branches gate on
        //     `!egui_consumed`.
        //   - `consumed == false` means no egui widget claimed it; the
        //     editor handles normally.
        //   - `repaint == true` means egui's visual state changed and
        //     wants a redraw; we forward `request_redraw()`.
        // ModifiersChanged + CursorMoved are observed by BOTH egui and
        // the editor (both subsystems track this state independently);
        // they are not gated. Resized + RedrawRequested + CloseRequested
        // are editor-only.
        let egui_consumed =
            if let (Some(host), Some(window)) = (self.egui_host.as_mut(), self.window.as_ref()) {
                let response = host.on_window_event(window, &event);
                if response.repaint {
                    window.request_redraw();
                }
                response.consumed
            } else {
                false
            };
        match event {
            WindowEvent::CloseRequested => {
                tracing::info!(
                    target: "rge::editor-shell::lifecycle",
                    ticks = self.tick_count,
                    "close requested"
                );
                if self.should_exit_on_window_close_request() {
                    event_loop.exit();
                }
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
                //
                // Phase 9 dispatch B: forward the new surface size +
                // scale factor to the egui host so its
                // `ScreenDescriptor` for the next frame reflects the
                // resize. `host.resize` is a pure-data update — no
                // wgpu surface reconfigure (the editor's surface_ctx
                // already did that via `resize_render_path`).
                if let (Some(host), Some(window)) = (self.egui_host.as_mut(), self.window.as_ref())
                {
                    host.resize(
                        new_size.width,
                        new_size.height,
                        window.scale_factor() as f32,
                    );
                }
            }
            WindowEvent::RedrawRequested => {
                self.tick_redraw();
                // EDITOR-WINDOW-TITLE — reflect the open document + dirty state
                // in the title bar (set_title only fires on change).
                self.sync_window_title();
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
            WindowEvent::MouseWheel { delta, .. } => {
                let over_viewport = self.is_pointer_over_viewport_tab();
                self.zoom_camera_for_viewport_mouse_wheel(&delta, over_viewport);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                // Sub-δ.2 single-select left-click. Other buttons /
                // Released events are no-ops in v0; right / middle /
                // scroll / drag / hover are non-goals (later dispatches).
                //
                // Phase 9 dispatch B: gate on `!egui_consumed` so clicks
                // on an egui panel/widget don't also fall through to
                // viewport face-pick. The host's hit-test consumes the
                // click when it lands on a widget; otherwise it falls
                // through to here.
                //
                // Phase 9 dispatch F: extend the gate so clicks on the
                // transparent Viewport tab body still reach face-pick
                // even when egui marks them consumed. The dock area
                // covers the whole window (so egui consumes every
                // click), but the Viewport tab is the "scene's
                // surface" — clicks on it should pick faces on the
                // cuboid. Inspector + tab-chrome clicks remain
                // consumed (no accidental picking).
                use winit::event::{ElementState, MouseButton};
                if state == ElementState::Pressed && button == MouseButton::Left {
                    let over_viewport = self.is_pointer_over_viewport_tab();
                    if should_fire_face_pick(egui_consumed, over_viewport) {
                        self.handle_left_click();
                    }
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
                //
                // Phase 9 dispatch B: gate on `!egui_consumed` so
                // keystrokes typed into an egui text field don't ALSO
                // trigger Command Bus shortcuts (e.g. Ctrl+Z in a text
                // field undoes text edits, not the global undo stack).
                //
                // Playback shortcuts: after the menu-accelerator lookup
                // (W08.3) and the `EditorKeyCommand` time-scale lookup, fall
                // through to the plain-key [`EditorPlaybackCommand`] lookup so
                // `Space` / `Escape` reach the PIE state machine. All lookups
                // are bounded by the same `egui_consumed` gate — typing into an
                // egui text field cannot accidentally toggle Play.
                if !egui_consumed {
                    if let Some(InputEvent::KeyDown(key)) = translate_keyboard(&event) {
                        let ctrl = self.modifiers.control_key();
                        let shift = self.modifiers.shift_key();
                        // W08.3 cutover — the canonical menu (`default_editor_menu`)
                        // is the single source of truth for accelerator → command.
                        // Resolve the keystroke to a `Shortcut` and, if the menu
                        // binds it, route the SAME `Command` the menu bar routes
                        // through the shared `route_menu_command` sink. This collapses
                        // the former `EditorKeyCommand` Save / Save-As / Undo / Redo
                        // arm and the inline `Ctrl+O` arm into one menu-sourced path;
                        // the parity guard (`lifecycle::accelerator`) pins that the
                        // menu binds these canonical accelerators while `from_key_press`
                        // returns None for them (no shadow remains). Resolve is
                        // on-demand against the LIVE `predicate_context()` (keydowns
                        // are human-frequency, so caching buys nothing), via
                        // `enabled_command_for_shortcut` so a DISABLED accelerator
                        // (e.g. Ctrl+S while a play session is active and Save is
                        // greyed) does NOT fire — matching the menu's greying.
                        let ctx = self.predicate_context();
                        let menu_command =
                            keycode_to_shortcut(key, ctrl, shift).and_then(|shortcut| {
                                default_editor_menu()
                                    .resolve(&ctx)
                                    .enabled_command_for_shortcut(&shortcut)
                                    .cloned()
                            });
                        if let Some(command) = menu_command {
                            // Canonical menu accelerators, including File/Edit/View
                            // bindings. `Ctrl+O` is now precise
                            // (the old inline arm fired on Ctrl+Shift+O too; the menu
                            // binds CTRL-only, so Ctrl+Shift+O is a no-op).
                            self.route_menu_command(command);
                        } else if let Some(cmd) = EditorKeyCommand::from_key_press(key, ctrl, shift)
                        {
                            // Execution-only time-scale binds (Ctrl+2 / Ctrl+0 /
                            // Ctrl+4) — no menu home, so they route through
                            // `EditorKeyCommand`. W08.4 retired its File/Edit
                            // Save / Save-As / Undo / Redo variants (now resolved
                            // only through the canonical menu above), so
                            // `from_key_press` returns Some only for these three
                            // digit binds.
                            self.handle_key_command(cmd);
                        } else if let Some(cmd) =
                            EditorPlaybackCommand::from_key_press(key, self.modifiers)
                        {
                            // Plain Space / Escape → PIE state machine (unchanged).
                            self.handle_playback_command(cmd);
                        } else if key == KeyCode::KeyR && self.modifiers.is_empty() {
                            // Plain `R` (no modifiers) — asset hot-reload (unchanged).
                            // Fires only in `PlayState::Editing` with a source path +
                            // hook attached via [`Self::attach_glb_reload_source`];
                            // every other condition warn-logs and no-ops.
                            self.handle_asset_reload();
                        }
                    }
                }
            }
            _ => {}
        }
        if self.take_quit_request() {
            tracing::info!(
                target: "rge::editor-shell::lifecycle",
                ticks = self.tick_count,
                "quit requested"
            );
            event_loop.exit();
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
mod tests;
