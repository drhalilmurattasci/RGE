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
//! # Dispatch arc
//!
//! - **Dispatch A** (`cc1f1e8`) — scaffold: [`EguiHost`] struct +
//!   constructor + input adapter + resize hook. No `render()`, no
//!   DockState.
//! - **Dispatch B** (`f3c7fd7`) — render pass: [`EguiHost::render`] takes
//!   a UI closure and paints into the editor's encoder. No DockState,
//!   no inspector yet.
//! - **Dispatch C** (`28ecae1`) — live inspector dock tab:
//!   - [`handoff::InspectorHandoff`] — latest-only snapshot handoff carrying
//!     an `InspectorSnapshot` (since GENERIC-LATEST-HANDOFF, a type alias over
//!     the shared `rge_editor_state::Handoff`).
//!   - [`tabs::TabBody`] / [`tabs::InspectorTabBody`] /
//!     [`tabs::EditorTabViewer`] — host-owned dock tab bodies + the
//!     [`egui_dock::TabViewer`] dispatch.
//!   - [`EguiHost`] now owns an [`egui_dock::DockState`]`<TabBody>` and an
//!     `Arc<InspectorHandoff>`; [`EguiHost::render`] paints a full
//!     [`egui_dock::DockArea`] inside the egui frame (no caller-side
//!     UI closure — the host's dock layout is the UI).
//!   - [`EguiHost::inspector_handoff`] exposes the handoff clone so
//!     editor-shell can `publish` a fresh inspector snapshot each
//!     frame.
//! - **Dispatch D** (`eb40817`) — split dock layout so the cuboid
//!   is visible alongside the inspector:
//!   - Adds [`tabs::TabBody::Viewport`] (unit variant, no state).
//!   - [`tabs::EditorTabViewer::clear_background`] returns `false` for
//!     `Viewport` so the dock library doesn't paint over the cuboid
//!     pixels written by `editor-shell::render_path::encode_main_pass`
//!     before the egui pass. `Inspector` + `Placeholder` keep
//!     background-clearing for text legibility.
//!   - [`EguiHost::new`] builds the initial `DockState` as a 2-pane
//!     layout: `Viewport` on the left/main area (~75%), `Inspector`
//!     docked right (~25%).
//! - **Dispatch F** — face-pick over viewport. The
//!   egui dock area consumes ALL pointer input by default (because it
//!   covers the whole window), making `handle_left_click` unreachable.
//!   This dispatch adds the smallest substrate that lets editor-shell
//!   route clicks **on the transparent Viewport tab body** through to
//!   face-pick while keeping Inspector + tab-chrome clicks consumed:
//!   - [`tabs::ViewportRectSink`] type alias
//!     (`Mutex<Option<egui::Rect>>`).
//!   - [`tabs::EditorTabViewer::with_viewport_rect_sink`] constructor
//!     that wires a shared `Arc<ViewportRectSink>` clone; when the
//!     `Viewport` tab renders, the viewer captures `ui.max_rect()`
//!     into the sink.
//!   - [`EguiHost`] owns an `Arc<ViewportRectSink>`; the host's
//!     `render` clones it into the per-frame viewer.
//!   - [`EguiHost::viewport_tab_rect`] and
//!     [`EguiHost::is_pointer_over_viewport`] accessors expose the
//!     captured rect (with the physical→logical DPI conversion
//!     handled internally).
//! - **EDITOR-SAVE-STATUS-INDICATOR** — in-app bottom status bar showing the
//!   open save source file name + dirty marker, alongside the inspector:
//!   - [`handoff::SaveStatusHandoff`] — a second latest-only handoff carrying
//!     a [`rge_editor_state::SaveStatusSnapshot`] (like
//!     [`handoff::InspectorHandoff`], a type alias over the shared
//!     `rge_editor_state::Handoff` since GENERIC-LATEST-HANDOFF).
//!   - [`EguiHost`] now owns BOTH the `Arc<InspectorHandoff>` and the
//!     `Arc<SaveStatusHandoff>`; [`EguiHost::save_status_handoff`] exposes
//!     the clone so editor-shell publishes a fresh save-status snapshot each
//!     frame, the same way it publishes the inspector snapshot.
//!   - [`EguiHost::render`] draws a bottom [`egui::TopBottomPanel`] (via
//!     [`rge_editor_ui::widgets::save_status::ui`]) BEFORE the
//!     [`egui_dock::DockArea`], so the status bar sits below the dock; the
//!     `render` signature is unchanged.
//!
//! # Headless by design
//!
//! This crate does NOT depend on `rge-editor-shell`. The wiring
//! direction is `editor-shell → editor-egui-host` (shell hosts the egui
//! pass in its render loop). Adding the reverse dep would create a
//! cycle and foreclose the planned host architecture.
//!
//! Deps:
//!
//! - `egui` / `egui-winit` / `egui-wgpu` / `egui_dock` — workspace pins.
//! - `wgpu` / `winit` — workspace pins (the wgpu device + winit window
//!   the constructor consumes are produced by editor-shell's `resumed`
//!   callback but passed in as borrowed primitives).
//! - `rge-editor-state` — for [`rge_editor_state::InspectorSnapshot`]
//!   inside [`handoff::InspectorHandoff`] and the tab body, and
//!   [`rge_editor_state::SaveStatusSnapshot`] inside
//!   [`handoff::SaveStatusHandoff`].
//! - `rge-editor-ui` — for [`rge_editor_ui::widgets::inspector::ui`]
//!   which the [`tabs::EditorTabViewer::ui`] dispatch calls when an
//!   Inspector tab renders, and [`rge_editor_ui::widgets::save_status::ui`]
//!   which [`EguiHost::render`] calls for the bottom status bar.

#![allow(clippy::module_name_repetitions)]

use std::sync::Arc;

// Re-export selected egui types so editor-shell (and other consumers
// of this host crate) don't need to declare a direct `egui` dep just
// to reference these constants. Limit the surface to types editor-shell
// actually needs: `ViewportId` for the constructor and
// `egui_winit::EventResponse` for the input adapter return type.
pub use egui::ViewportId;
pub use egui_winit::EventResponse;
use rge_editor_ui::menus::{Command, ExtensionPoint, MenuEntry, MenuRegistry, PredicateContext};
use winit::event::WindowEvent;
use winit::window::Window;

pub mod handoff;
pub mod tabs;

pub use handoff::{InspectorHandoff, MenuCommandHandoff, SaveStatusHandoff};
pub use tabs::{EditorTabViewer, InspectorTabBody, TabBody, ViewportRectSink};

// ---------------------------------------------------------------------------
// Dock layout constants
// ---------------------------------------------------------------------------

/// Fraction of the parent (root) node's width that the OLD node
/// retains after the dispatch-D `split_right` call in [`EguiHost::new`].
/// `0.75` leaves ~25% of the width for the newly-inserted right pane
/// (the Inspector tab), matching the dispatch-D scope (`docked on the
/// right at about 25% width`).
///
/// Public for `tests/dock_layout_smoke.rs` so the integration test can
/// assert the geometric intent without re-reading egui_dock's
/// `fraction` semantics. A future polish dispatch can tune this without
/// touching the test's intent.
///
/// Per egui_dock 0.19 docs: "fraction specifies how much of the parent
/// node's area the OLD node will attempt to occupy after the split"
/// (`egui_dock-0.19.1/src/dock_state/tree/mod.rs` line 419) — the new
/// right pane therefore gets `1.0 - INSPECTOR_PANE_OLD_FRACTION`.
pub const INSPECTOR_PANE_OLD_FRACTION: f32 = 0.75;

// ---------------------------------------------------------------------------
// File menu
// ---------------------------------------------------------------------------

/// Extension-point id for the editor's main-menu **File** surface. Plugins (a
/// future dispatch) register additional File entries against this same id.
const FILE_MENU_EXTENSION_POINT: &str = "editor.main_menu.file";

/// Extension-point id for the editor's main-menu **Edit** surface (A2).
const EDIT_MENU_EXTENSION_POINT: &str = "editor.main_menu.edit";

/// Build the production [`MenuRegistry`] with BOTH main-menu extension points
/// (File + Edit), register each point's entries, resolve ONCE against an empty
/// [`PredicateContext`], and project each point's resolved entries to the
/// `(label, `[`Command`]`)` pairs the menu bar paints. Returns `(file, edit)`.
///
/// The registry is the single source of truth for both menus' content + order.
/// Every entry carries the default order hint (`OrderHint::AtEnd`) in the
/// default section, so `resolve` returns each point's entries in registration
/// order:
/// - **File** = Open / Save / Save As New Project (byte-identical labels to A1,
///   behaviour-identical). "Save As New Project…" enqueues [`Command::SaveAs`];
///   the editor-shell consumer routes it to
///   `EditorShell::handle_save_as_new_project_request`.
/// - **Edit** = Undo / Redo, enqueuing [`Command::Undo`] / [`Command::Redo`],
///   which the editor-shell drain routes to `EditorShell::undo_command` /
///   `redo_command` — behaviour-identical to the existing `Ctrl+Z` / `Ctrl+Y`
///   keystroke path.
///
/// Both menus are static (no predicates / dynamic visibility), so resolving
/// once at construction is sufficient and the results are cached on the host;
/// per-frame re-resolve is deferred to a future dispatch. Construction errors
/// are unreachable here (fresh registry, distinct ids), hence the `expect`s.
fn build_main_menu_entries() -> (Vec<(String, Command)>, Vec<(String, Command)>) {
    let mut registry = MenuRegistry::new();
    let file_point = ExtensionPoint::new(FILE_MENU_EXTENSION_POINT);
    let edit_point = ExtensionPoint::new(EDIT_MENU_EXTENSION_POINT);
    registry
        .declare_extension_point(file_point.clone())
        .expect("static File extension point declares cleanly");
    registry
        .declare_extension_point(edit_point.clone())
        .expect("static Edit extension point declares cleanly");
    for (id, label, command) in [
        ("file.open", "Open…", Command::OpenFile),
        ("file.save", "Save", Command::Save),
        ("file.save_as", "Save As New Project…", Command::SaveAs),
    ] {
        registry
            .register_entry(&file_point, MenuEntry::new(id, label, command))
            .expect("static File menu entries register cleanly");
    }
    for (id, label, command) in [
        ("edit.undo", "Undo", Command::Undo),
        ("edit.redo", "Redo", Command::Redo),
    ] {
        registry
            .register_entry(&edit_point, MenuEntry::new(id, label, command))
            .expect("static Edit menu entries register cleanly");
    }
    let resolved = registry.resolve(&PredicateContext::default());
    let project = |point: &ExtensionPoint| -> Vec<(String, Command)> {
        resolved
            .entries_for(point)
            .iter()
            .map(|r| (r.entry.label.clone(), r.entry.command.clone()))
            .collect()
    };
    (project(&file_point), project(&edit_point))
}

// ---------------------------------------------------------------------------
// EguiHost
// ---------------------------------------------------------------------------

/// egui + egui_dock host. Owns the three core egui subsystems, the
/// most-recently-observed surface dimensions, the editor's dock state,
/// two latest-only snapshot handoffs that connect the editor-shell
/// publisher to the host (the inspector handoff, consumed by the in-host
/// [`InspectorTabBody`], and the save-status handoff, consumed by the
/// bottom status bar in [`Self::render`]), and a [`MenuCommandHandoff`] —
/// a host→shell FIFO queue the File + Edit menu bars enqueue [`Command`]s onto.
///
/// # Trait bounds
///
/// `EguiHost` is `Send + 'static` (all inner types are `Send + 'static`)
/// but is **not** `Sync` — `egui_wgpu::Renderer` holds wgpu resources
/// that are not safely shareable across threads without external
/// synchronization. The compile-time assertion lives in
/// `tests/host_scaffolding_smoke.rs::host_is_send_and_static`.
///
/// # Construction
///
/// [`EguiHost::new`] takes the wgpu device, surface format, depth format,
/// sample count, and an `Arc<Window>`. All are produced by editor-shell's
/// `resumed` callback; this crate has no opinion about *where* those
/// primitives originate. The initial [`DockState`] is built with a
/// single [`TabBody::Inspector`] tab so the inspector is visible from
/// frame 1 with zero further setup.
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
    /// `update_buffers` + `render` call sites are driven by
    /// [`EguiHost::render`].
    renderer: egui_wgpu::Renderer,

    /// Most-recent physical-pixel surface dimensions, in
    /// `[width, height]`. Updated by [`Self::resize`]; consumed by the
    /// per-frame `egui_wgpu::ScreenDescriptor` constructed inside
    /// [`Self::render`].
    surface_size: [u32; 2],

    /// Most-recent device scale factor. `egui_winit::State` tracks its
    /// own copy via `WindowEvent::ScaleFactorChanged`; this field is a
    /// cache for the `ScreenDescriptor::pixels_per_point` field.
    pixels_per_point: f32,

    /// Host-owned dock layout. Stores the live set of tabs, including
    /// the always-present [`TabBody::Inspector`] tab installed at
    /// construction. Mutable per render (egui_dock may move/resize
    /// tabs in response to user input).
    dock_state: egui_dock::DockState<TabBody>,

    /// `Arc<InspectorHandoff>` retained by the host so editor-shell can
    /// reach the same handoff the [`InspectorTabBody`] reads from via
    /// [`Self::inspector_handoff`]. Cloned into the inspector tab body
    /// at construction; the two clones (host field + tab body field)
    /// point at the same underlying slot.
    inspector_handoff: Arc<InspectorHandoff>,

    /// `Arc<SaveStatusHandoff>` retained by the host so editor-shell can
    /// publish a fresh save-status snapshot (open save source file name + dirty
    /// flag) each frame; the host's `render` acquires it to draw the bottom
    /// status bar. Sibling to `inspector_handoff` — same latest-only shape.
    save_status_handoff: Arc<SaveStatusHandoff>,

    /// `Arc<MenuCommandHandoff>` retained by the host so the editor-shell
    /// consumer can drain the menu-dispatched [`Command`]s the File + Edit menu
    /// bars enqueue (via [`Self::menu_command_handoff`]). Unlike the two handoffs
    /// above this is a host→shell **FIFO command queue**, not a latest-only
    /// snapshot slot. The editor-shell drains + routes it
    /// (`EditorShell::drain_and_route_menu_commands`) at the top of each frame.
    menu_command_handoff: Arc<MenuCommandHandoff>,

    /// Dispatch F — shared sink that captures the
    /// [`TabBody::Viewport`] body rect (egui logical points) on each
    /// render frame. The host clones this `Arc` into the per-frame
    /// [`EditorTabViewer`]; the viewer writes `Some(ui.max_rect())`
    /// during the Viewport `ui()` arm. [`Self::is_pointer_over_viewport`]
    /// reads the latest captured rect to answer editor-shell's
    /// "should this click fall through to face-pick?" question.
    ///
    /// `None` between construction and the first render frame — the
    /// host's `render` resets the sink to `None` at the start of each
    /// frame, then the Viewport ui() arm fills it. After the first
    /// successful frame the slot has a value.
    viewport_tab_rect_sink: Arc<ViewportRectSink>,

    /// The File menu's resolved `(label, `[`Command`]`)` entries, produced once
    /// at construction by [`build_main_menu_entries`] — the [`MenuRegistry`]
    /// resolve output projected for painting. [`Self::render`]'s File menu bar
    /// iterates this each frame; the menu is static so it never changes after
    /// construction (per-frame re-resolve is deferred to a future dispatch).
    file_menu_entries: Vec<(String, Command)>,

    /// The Edit menu's resolved `(label, `[`Command`]`)` entries (Undo / Redo),
    /// produced once at construction by [`build_main_menu_entries`] alongside
    /// the File entries. [`Self::render`]'s Edit menu bar iterates this; static,
    /// so it never changes after construction.
    edit_menu_entries: Vec<(String, Command)>,
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
        // format, and a [`RendererOptions`] config struct.
        let renderer_options = egui_wgpu::RendererOptions {
            msaa_samples,
            depth_stencil_format: depth_format,
            dithering: false,
            predictable_texture_filtering: false,
        };
        let renderer = egui_wgpu::Renderer::new(device, surface_format, renderer_options);

        // Dispatch C / D — build the initial dock state with a
        // viewport tab on the left/main area and the inspector tab
        // docked on the right at ~25% width. The viewport tab is
        // intentionally non-obscuring: [`tabs::EditorTabViewer::clear_background`]
        // returns `false` for `TabBody::Viewport`, so the cuboid
        // pixels written by the editor's `encode_main_pass` (before
        // the egui pass) remain visible through this tab. Single
        // dock-state construction; egui_dock manages tab rearrange /
        // drag-undock interactively.
        //
        // The handoff `Arc` is cloned into the inspector tab body so
        // the editor-shell publisher path (via
        // [`Self::inspector_handoff`]) and the consumer path
        // (`InspectorTabBody::handoff`) share the same slot.
        //
        // `split_right(NodeIndex::root(), 0.75, ...)` keeps the OLD
        // (viewport) node at ~75% of the parent and places the NEW
        // (inspector) node at ~25% on the right — per egui_dock's
        // documented contract: "fraction specifies how much of the
        // parent node's area the old node will attempt to occupy after
        // the split". The fraction value lives in
        // `INSPECTOR_PANE_OLD_FRACTION` so a future polish dispatch can
        // tune it without re-reading the egui_dock semantics.
        let inspector_handoff = Arc::new(InspectorHandoff::new());
        let save_status_handoff = Arc::new(SaveStatusHandoff::new());
        let menu_command_handoff = Arc::new(MenuCommandHandoff::new());
        let viewport_tab = TabBody::Viewport;
        let inspector_tab =
            TabBody::Inspector(InspectorTabBody::new(Arc::clone(&inspector_handoff)));
        let mut dock_state = egui_dock::DockState::new(vec![viewport_tab]);
        dock_state.main_surface_mut().split_right(
            egui_dock::NodeIndex::root(),
            INSPECTOR_PANE_OLD_FRACTION,
            vec![inspector_tab],
        );

        // Dispatch F — viewport rect sink. Construct empty; the first
        // `EguiHost::render` will populate it inside the EditorTabViewer's
        // Viewport ui() arm. Shared via `Arc` so editor-shell can query
        // through [`Self::is_pointer_over_viewport`] without taking a
        // borrow that conflicts with the per-frame viewer's writes.
        let viewport_tab_rect_sink: Arc<ViewportRectSink> = Arc::new(std::sync::Mutex::new(None));

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

        // A1/A2 — produce the File + Edit menu entries once from the data-driven
        // `MenuRegistry` (one registry, both extension points, resolved once).
        let (file_menu_entries, edit_menu_entries) = build_main_menu_entries();

        Self {
            context,
            state,
            renderer,
            surface_size: [inner_size.width, inner_size.height],
            pixels_per_point,
            dock_state,
            inspector_handoff,
            save_status_handoff,
            menu_command_handoff,
            viewport_tab_rect_sink,
            file_menu_entries,
            edit_menu_entries,
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
    pub fn on_window_event(
        &mut self,
        window: &Window,
        event: &WindowEvent,
    ) -> egui_winit::EventResponse {
        self.state.on_window_event(window, event)
    }

    /// Update the host's view of the surface dimensions + scale factor.
    /// Called by the embedding render loop after `surface_ctx.resize(...)`.
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

    /// Borrow the egui context (read-only). Used for
    /// `Context::request_repaint`, scope inspection, etc.
    #[must_use]
    pub fn context(&self) -> &egui::Context {
        &self.context
    }

    /// Most-recent surface dimensions in physical pixels `[w, h]`.
    #[must_use]
    pub fn surface_size(&self) -> [u32; 2] {
        self.surface_size
    }

    /// Most-recent device scale factor (pixels-per-egui-point).
    #[must_use]
    pub fn pixels_per_point(&self) -> f32 {
        self.pixels_per_point
    }

    /// Borrow the shared inspector-snapshot handoff.
    ///
    /// Editor-shell publishes a fresh snapshot through this handoff
    /// once per frame BEFORE calling [`Self::render`]; the host's
    /// [`InspectorTabBody`] (which holds a clone of this same `Arc`)
    /// acquires the most-recently-published snapshot when its dock
    /// tab renders.
    ///
    /// The returned reference borrows from the host; clone the `Arc`
    /// (`Arc::clone(host.inspector_handoff())`) if the caller needs to
    /// hold an owned handle across borrows of the host.
    #[must_use]
    pub fn inspector_handoff(&self) -> &Arc<InspectorHandoff> {
        &self.inspector_handoff
    }

    /// Borrow the shared save-status handoff.
    ///
    /// Editor-shell publishes a fresh [`rge_editor_state::SaveStatusSnapshot`]
    /// through this handoff once per frame BEFORE calling [`Self::render`];
    /// the host's `render` acquires the most-recently-published snapshot to
    /// draw the bottom status bar. Sibling to [`Self::inspector_handoff`].
    ///
    /// Clone the `Arc` (`Arc::clone(host.save_status_handoff())`) if the
    /// caller needs to hold an owned handle across borrows of the host.
    #[must_use]
    pub fn save_status_handoff(&self) -> &Arc<SaveStatusHandoff> {
        &self.save_status_handoff
    }

    /// Borrow the shared menu-command handoff (host→shell FIFO).
    ///
    /// The File + Edit menu bars drawn by [`Self::render`] enqueue a
    /// [`rge_editor_ui::menus::Command`] when an item is activated; the
    /// editor-shell consumer clones this `Arc` and drains the queue at the top
    /// of each frame (`EditorShell::drain_and_route_menu_commands`), routing
    /// each command one-way to its existing handler.
    ///
    /// Clone the `Arc` (`Arc::clone(host.menu_command_handoff())`) to hold an
    /// owned handle across borrows of the host.
    #[must_use]
    pub fn menu_command_handoff(&self) -> &Arc<MenuCommandHandoff> {
        &self.menu_command_handoff
    }

    /// Borrow the host's dock state. Exposed primarily for tests that
    /// assert the layout shape (tab count, tab titles) without
    /// spinning up a real wgpu device.
    #[must_use]
    pub fn dock_state(&self) -> &egui_dock::DockState<TabBody> {
        &self.dock_state
    }

    /// The most-recently-rendered [`TabBody::Viewport`] body rect in
    /// **egui logical points** (DPI-independent). `None` before the
    /// first render frame, or if the Viewport tab was not rendered on
    /// the most recent frame (e.g. drag-detached into a window surface
    /// — currently impossible since tabs are non-closeable).
    ///
    /// Dispatch F substrate — editor-shell uses
    /// [`Self::is_pointer_over_viewport`] (the physical-pixel wrapper)
    /// to decide whether a click that egui marked as `consumed` should
    /// fall through to face-pick. This raw accessor is exposed so
    /// future dispatches can build other pointer-vs-tab queries (e.g.
    /// hover-tooltips that fire only when the cursor is over the
    /// viewport).
    ///
    /// Returns `None` if the sink mutex is poisoned. Poisoning is
    /// rare; the host treats it as a no-op (no face-pick fallback,
    /// editor falls back to the dispatch-D consumed-everywhere
    /// behavior) rather than a hard error.
    #[must_use]
    pub fn viewport_tab_rect(&self) -> Option<egui::Rect> {
        self.viewport_tab_rect_sink.lock().ok().and_then(|g| *g)
    }

    /// True when `physical_pos` (in **physical pixels**, matching
    /// winit's `WindowEvent::CursorMoved.position` convention) is
    /// inside the most-recently-captured Viewport tab body rect.
    ///
    /// Performs the physical→logical conversion internally using
    /// [`Self::pixels_per_point`] so editor-shell can pass its raw
    /// `cursor_pos: [f32; 2]` field without thinking about DPI.
    ///
    /// Returns `false` when:
    ///
    /// - No frame has been rendered yet (sink empty).
    /// - The sink mutex is poisoned (treated as "no viewport
    ///   visible" — editor falls back to dispatch-D's
    ///   consumed-everywhere behavior, which is safe but suppresses
    ///   face-pick).
    /// - `pixels_per_point` is zero or non-finite (defensive — would
    ///   indicate a deeper init bug; not expected in practice).
    /// - `physical_pos` lies outside the captured rect (the usual
    ///   case for clicks on Inspector / tab chrome / outside the
    ///   window).
    ///
    /// # Coordinate spaces (load-bearing)
    ///
    /// winit reports `CursorMoved.position` in **physical pixels** at
    /// the surface's native resolution. egui's `Ui::max_rect()`
    /// returns **logical points** (DPI-independent; multiplied by
    /// `pixels_per_point` for physical rendering). The conversion is
    /// `logical = physical / pixels_per_point`. With `pixels_per_point
    /// = 1.5` on a 150% scaled display, a physical click at
    /// `(900, 600)` lands at logical `(600, 400)`.
    #[must_use]
    pub fn is_pointer_over_viewport(&self, physical_pos: [f32; 2]) -> bool {
        let Some(rect) = self.viewport_tab_rect() else {
            return false;
        };
        let ppp = self.pixels_per_point;
        if ppp <= 0.0 || !ppp.is_finite() {
            return false;
        }
        let logical = egui::pos2(physical_pos[0] / ppp, physical_pos[1] / ppp);
        rect.contains(logical)
    }

    /// Render one egui frame.
    ///
    /// Records an egui render pass on the provided `encoder` with
    /// `LoadOp::Load` against `color_view`, preserving whatever the
    /// caller drew before. The pass has no depth attachment (egui is a
    /// 2D overlay; depth tests don't apply).
    ///
    /// The frame's UI is a bottom save-status [`egui::TopBottomPanel`]
    /// (open save source name + dirty marker) plus the host's
    /// [`egui_dock::DockArea`] filling the remaining area above it —
    /// there is no caller-supplied UI closure (the dispatch-B `run_ui`
    /// parameter was dropped in dispatch C, since the host now owns its
    /// layout via [`DockState`]). A top File menu bar (Open / Save / Save As
    /// New Project) is rendered above the dock; future dispatches that add
    /// further menus or floating windows layer those inside the same render
    /// path.
    ///
    /// # Flow (per the egui 0.34 + egui-wgpu 0.34 lifecycle)
    ///
    /// 1. Take winit-translated input from
    ///    [`egui_winit::State::take_egui_input`].
    /// 2. Run the UI via [`egui::Context::run_ui`] — the top File menu bar,
    ///    the bottom save-status panel, then the dock area — producing a
    ///    [`egui::FullOutput`].
    /// 3. Apply platform output ([`egui_winit::State::handle_platform_output`]).
    /// 4. Free textures egui marked for deletion this frame.
    /// 5. Upload new texture deltas to the renderer.
    /// 6. Tessellate `FullOutput::shapes` into clipped primitives.
    /// 7. Build a fresh [`egui_wgpu::ScreenDescriptor`].
    /// 8. Update GPU buffers via [`egui_wgpu::Renderer::update_buffers`]
    ///    on the caller's encoder.
    /// 9. Begin a render pass with `LoadOp::Load`, render egui's
    ///    primitives, end the pass.
    pub fn render(
        &mut self,
        window: &Window,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        color_view: &wgpu::TextureView,
    ) {
        // Step 1: drain winit-translated input from `egui_winit::State`.
        let raw_input = self.state.take_egui_input(window);

        // Step 2: run the dock-area UI closure. We split-borrow
        // `&mut self.dock_state` BEFORE the `run_ui` call so the
        // closure can mutate dock state without conflicting with the
        // `&self.context` borrow held by `run_ui`. Field borrows are
        // disjoint by NLL — `self.context` and `self.dock_state` are
        // distinct paths.
        //
        // The closure body uses `DockArea::show_inside(root_ui, ...)`
        // — `root_ui` is the background-layer Ui created by
        // [`egui::Context::run_ui_dyn`] with `max_rect = ctx.available_rect()`;
        // `show_inside` allocates the dock area within that rect.
        //
        // Dispatch F — clone the viewport-rect sink `Arc` into the
        // per-frame [`EditorTabViewer`]. The viewer writes the current
        // frame's Viewport body rect into the sink during its `ui()`
        // dispatch. The sink is reset to `None` at the start of each
        // frame so a stale value from a previous frame can't influence
        // pointer routing if the Viewport tab is gone for some reason
        // (e.g. drag-detach into a window surface in a future
        // dispatch).
        if let Ok(mut guard) = self.viewport_tab_rect_sink.lock() {
            *guard = None;
        }
        let viewport_sink = Arc::clone(&self.viewport_tab_rect_sink);
        // Acquire the latest save-status snapshot BEFORE the `run_ui` borrow
        // (mirrors the dock_state / viewport_sink split-borrow above, since
        // `self.context` is borrowed by `run_ui`). Empty slot → default
        // (`"No file"`) so the status bar is visible from frame 1.
        let save_status = self
            .save_status_handoff
            .acquire()
            .map(|arc| (*arc).clone())
            .unwrap_or_default();
        // Clone the menu-command FIFO `Arc` BEFORE the `run_ui` borrow (mirrors
        // the `save_status` / `viewport_sink` split-borrows) so the closure owns
        // its handle. The File + Edit menu bars push onto it; the editor-shell
        // drains + routes it at the top of render_frame.
        let menu_commands = Arc::clone(&self.menu_command_handoff);
        // Borrow the registry-resolved File + Edit entries (disjoint fields)
        // before the `run_ui` closure so the closure captures THESE field-borrows,
        // not all of `self` (which `&mut self.dock_state` already borrows mutably).
        let file_entries = &self.file_menu_entries;
        let edit_entries = &self.edit_menu_entries;
        let dock_state = &mut self.dock_state;
        let full_output = self.context.run_ui(raw_input, |root_ui| {
            // Top menu bar — File ▸ Open / Save / Save As New Project, and Edit ▸
            // Undo / Redo. Added BEFORE the bottom status bar + DockArea so egui
            // reserves the top strip and the dock fills the remaining central
            // rect. Activating an item ENQUEUES a `Command` onto the host→shell
            // FIFO; the editor-shell drain routes it (File wiring + A2 Edit).
            egui::Panel::top("rge_menu_bar").show_inside(root_ui, |ui| {
                egui::MenuBar::new().ui(ui, |ui| {
                    ui.menu_button("File", |ui| {
                        for (label, cmd) in file_entries {
                            if ui.button(label.as_str()).clicked() {
                                menu_commands.push(cmd.clone());
                                ui.close();
                            }
                        }
                    });
                    ui.menu_button("Edit", |ui| {
                        for (label, cmd) in edit_entries {
                            if ui.button(label.as_str()).clicked() {
                                menu_commands.push(cmd.clone());
                                ui.close();
                            }
                        }
                    });
                });
            });
            // Bottom status bar — open save source file name + dirty marker. Added
            // BEFORE the DockArea so egui reserves the bottom strip and the
            // dock fills the remaining central rect.
            egui::TopBottomPanel::bottom("rge_save_status_bar").show_inside(root_ui, |ui| {
                rge_editor_ui::widgets::save_status::ui(&save_status, ui);
            });
            let mut viewer = EditorTabViewer::with_viewport_rect_sink(Arc::clone(&viewport_sink));
            egui_dock::DockArea::new(dock_state)
                .style(egui_dock::Style::from_egui(root_ui.style().as_ref()))
                .show_inside(root_ui, &mut viewer);
        });

        // Step 3: apply platform-side output (cursor icon, IME, etc.).
        self.state
            .handle_platform_output(window, full_output.platform_output);

        // Step 4: free textures egui marked for deletion in this frame.
        for id in &full_output.textures_delta.free {
            self.renderer.free_texture(id);
        }

        // Step 5: upload texture deltas (new + updated textures).
        for (id, image_delta) in &full_output.textures_delta.set {
            self.renderer
                .update_texture(device, queue, *id, image_delta);
        }

        // Step 6: tessellate shapes into clipped primitives.
        let pixels_per_point = full_output.pixels_per_point;
        let primitives = self
            .context
            .tessellate(full_output.shapes, pixels_per_point);

        // Step 7: screen descriptor for this frame.
        let screen_descriptor = egui_wgpu::ScreenDescriptor {
            size_in_pixels: self.surface_size,
            pixels_per_point,
        };

        // Step 8: update GPU buffers on the caller's encoder.
        let _user_cmd_bufs =
            self.renderer
                .update_buffers(device, queue, encoder, &primitives, &screen_descriptor);

        // Step 9: begin render pass + render + drop (ends the pass).
        {
            let pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("rge-editor-egui-host.egui-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: color_view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let mut pass = pass.forget_lifetime();
            self.renderer
                .render(&mut pass, &primitives, &screen_descriptor);
        }
    }
}

#[cfg(test)]
mod menu_tests {
    use rge_editor_ui::menus::Command;

    use super::{build_main_menu_entries, MenuCommandHandoff};

    #[test]
    fn file_menu_registry_resolves_the_authoring_loop_commands() {
        let (file, _edit) = build_main_menu_entries();
        assert_eq!(
            file,
            vec![
                ("Open…".to_owned(), Command::OpenFile),
                ("Save".to_owned(), Command::Save),
                ("Save As New Project…".to_owned(), Command::SaveAs),
            ],
            "the MenuRegistry resolves the File menu to exactly \
             Open / Save / Save-As-new-project, in order"
        );
    }

    #[test]
    fn edit_menu_registry_resolves_undo_redo_in_order() {
        let (_file, edit) = build_main_menu_entries();
        assert_eq!(
            edit,
            vec![
                ("Undo".to_owned(), Command::Undo),
                ("Redo".to_owned(), Command::Redo),
            ],
            "the MenuRegistry resolves the Edit menu to exactly Undo / Redo, in order"
        );
    }

    #[test]
    fn file_menu_entries_round_trip_through_the_handoff_in_order() {
        let (file, _edit) = build_main_menu_entries();
        let handoff = MenuCommandHandoff::new();
        for (_, cmd) in file {
            handoff.push(cmd);
        }
        assert_eq!(
            handoff.drain(),
            vec![Command::OpenFile, Command::Save, Command::SaveAs],
            "each resolved File item enqueues its Command; they drain FIFO"
        );
    }

    #[test]
    fn edit_menu_entries_round_trip_through_the_handoff_in_order() {
        let (_file, edit) = build_main_menu_entries();
        let handoff = MenuCommandHandoff::new();
        for (_, cmd) in edit {
            handoff.push(cmd);
        }
        assert_eq!(
            handoff.drain(),
            vec![Command::Undo, Command::Redo],
            "each resolved Edit item enqueues its Command; they drain FIFO"
        );
    }
}
