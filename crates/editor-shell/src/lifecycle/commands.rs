//! Phase 9 CommandBus integration — keyboard bindings + bus-routed mutations.
//!
//! Holds the editor-side material that drives [`rge_editor_actions`] from
//! the editor-shell side of the Tier-2 boundary:
//!
//! - [`EditorKeyCommand`] — the closed enum of bus-bound keyboard commands
//!   (the `Ctrl+2` / `Ctrl+0` / `Ctrl+4` time-scale binds; the File/Edit
//!   accelerators are resolved through the canonical menu since the W08 thread)
//!   plus its physical-key → command mapping table.
//! - [`SetTimeScale`] — the first non-test [`rge_editor_actions::Action`]
//!   impl in the workspace. Routes the time-scale slider mutation through
//!   the bus so undo/redo work on it.
//! - The narrow `impl EditorShell` block exposing
//!   [`EditorShell::submit_action`] / [`EditorShell::undo_command`] /
//!   [`EditorShell::redo_command`] / [`EditorShell::mark_saved_command`] /
//!   [`EditorShell::command_bus`] / [`EditorShell::handle_key_command`] /
//!   [`EditorShell::set_time_scale`].
//!
//! # Architectural invariants
//!
//! - Every bus call delegates to `self.world.kernel_mut()` so the bus sees
//!   only the inner `rge_kernel_ecs::World`. The wrapper [`crate::world::World`]
//!   is never handed to the bus directly — that preserves the
//!   [`rge_editor_actions::Action::apply`] `(&self, world: &mut
//!   rge_kernel_ecs::World)` contract.
//! - `Action: Send + Sync + 'static` is a hard trait bound; [`SetTimeScale`]
//!   carries pre-captured `{ from, to }` plain `f32`s (no `Cell` / `RefCell` /
//!   interior mutability) so it is trivially `Send + Sync`.
//! - Coalescing across rapid slider drags is achieved via a constant
//!   [`SetTimeScale::id()`] plus a payload-based [`SetTimeScale::merge`]
//!   that parses `next.payload()` and keeps the original `from` while
//!   adopting the newer `to`. This way a 20-event drag from 1.0 → 3.5
//!   collapses to one stack entry whose revert restores 1.0.
//! - Dual-ledger by design (locked decision for this dispatch):
//!   - The bus's internal [`rge_editor_actions::AuditLedger`] continues
//!     to record `EventKind::Action` on each submit.
//!   - [`EditorShell::audit`] (the editor's own [`crate::audit::AuditLedger`])
//!     continues to record [`crate::audit::AuditEvent::TimeScaleChanged`]
//!     on each `set_time_scale` call so existing tests
//!     (`time_scale_audit_event_records_change`) keep passing without
//!     editing.
//!   - Ledger consolidation is a separate future dispatch.

use std::error::Error;
use std::fmt;

use rge_cad_core::{
    CadGraph, CheckpointError, CheckpointId, CuboidOp, GraphBuildError, OperatorNode, Tolerance,
};
use rge_cad_projection::{BRepHandle, CadProjection, ProjectionError};
use rge_editor_actions::action::{Action, ActionId, ActionResult, MergeOutcome};
use rge_editor_actions::BusError;
use rge_input::KeyCode;
use rge_kernel_ecs::{EntityId as KernelEntityId, World as KernelWorld};

use crate::audit::AuditEvent;
use crate::lifecycle::EditorShell;
use crate::time_scale::TimeScale;

// ---------------------------------------------------------------------------
// EditorKeyCommand
// ---------------------------------------------------------------------------

/// Editor-side keyboard command with no canonical-menu home.
///
/// Post-W08.3 the menu accelerators are resolved through the canonical menu
/// (`default_editor_menu` → `ResolveResult::command_for_shortcut` →
/// [`EditorShell::route_menu_command`]); W08.4 retired their `EditorKeyCommand`
/// mirror so those keystroke→command literals live ONLY in the menu. What remains
/// here is the execution-only time-scale set (`Ctrl+2` / `Ctrl+0` / `Ctrl+4`) —
/// keybinds the menu intentionally does NOT carry, so they are not duplicated.
/// Future menu-less editor keybinds extend this enum rather than growing parallel
/// command channels.
///
/// Mapping from physical keys is performed by [`Self::from_key_press`]; the
/// dispatch into the slider path by [`EditorShell::handle_key_command`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorKeyCommand {
    /// `Ctrl+2` — set the [`TimeScale`] resource to 2.0 (double speed) via
    /// the existing [`EditorShell::set_time_scale`] → [`SetTimeScale`] →
    /// [`rge_editor_actions::CommandBus::submit`] path. Reuses the
    /// existing slider action so undo/redo and the no-op short-circuit at
    /// 2.0 require no additional surface.
    SetTimeScaleDoubleSpeed,
    /// `Ctrl+0` — reset the [`TimeScale`] resource to [`TimeScale::DEFAULT`]
    /// (1.0) through the same existing [`EditorShell::set_time_scale`] →
    /// [`SetTimeScale`] → [`rge_editor_actions::CommandBus::submit`] path.
    /// Reuses the existing slider action and its no-op short-circuit, so a
    /// Ctrl+0 fired while the slider already reads 1.0 grows neither the
    /// bus stack, the dirty flag, nor the editor-shell audit ledger.
    ResetTimeScaleDefault,
    /// `Ctrl+4` — set the [`TimeScale`] resource to [`TimeScale::MAX`] (max
    /// fast-forward) through the same existing [`EditorShell::set_time_scale`]
    /// → [`SetTimeScale`] → [`rge_editor_actions::CommandBus::submit`] path.
    /// Reuses the existing slider action and its no-op short-circuit, so a
    /// repeated Ctrl+4 while already at [`TimeScale::MAX`] grows neither the
    /// bus stack, the dirty flag, nor the editor-shell audit ledger.
    SetTimeScaleMaxFastForward,
}

impl EditorKeyCommand {
    /// Map a translated [`KeyCode`] plus the relevant modifier flags
    /// (`ctrl` and `shift`) to an [`EditorKeyCommand`]. Returns `None` for
    /// any other key combination so the keyboard branch in
    /// `EditorShell::window_event` can ignore unbound keys cheaply.
    ///
    /// Bind-set today — the execution-only time-scale binds. The File/Edit
    /// accelerators (`Ctrl+O` / `Ctrl+S` / `Ctrl+Shift+S` / `Ctrl+Z` / `Ctrl+Y`)
    /// are resolved through the canonical menu, NOT here, since the W08.3 cutover
    /// + the W08.4 retirement:
    ///
    /// | Combination | Command |
    /// |---|---|
    /// | `Ctrl+2` (no Shift) | [`EditorKeyCommand::SetTimeScaleDoubleSpeed`] |
    /// | `Ctrl+0` (no Shift) | [`EditorKeyCommand::ResetTimeScaleDefault`] |
    /// | `Ctrl+4` (no Shift) | [`EditorKeyCommand::SetTimeScaleMaxFastForward`] |
    ///
    /// No `Ctrl+Shift` binding exists today (Save-As retired to the menu). The
    /// `alt` modifier is intentionally ignored — Alt may be combined with Ctrl
    /// for tool-specific actions (e.g. drag-modifier) that don't route through
    /// the Command Bus. If future bus-bound commands need Alt-disambiguation,
    /// extend this signature additively.
    #[must_use]
    pub fn from_key_press(key: KeyCode, ctrl: bool, shift: bool) -> Option<Self> {
        // Ctrl-without-Shift digit binds only; the File/Edit accelerators
        // (Ctrl+O/S/Shift+S/Z/Y) route through the canonical menu (W08.3) and
        // W08.4 retired their EditorKeyCommand mirror.
        if !ctrl || shift {
            return None;
        }
        Some(match key {
            KeyCode::Digit2 => Self::SetTimeScaleDoubleSpeed,
            KeyCode::Digit0 => Self::ResetTimeScaleDefault,
            KeyCode::Digit4 => Self::SetTimeScaleMaxFastForward,
            _ => return None,
        })
    }
}

// ---------------------------------------------------------------------------
// SetTimeScale — the first production Action impl in the workspace.
// ---------------------------------------------------------------------------

/// Stable action id for [`SetTimeScale`].
///
/// Constant (not parameterised by `from` / `to`) so that the bus's
/// [`rge_editor_actions::CommandBus`] 500 ms coalesce window collapses rapid
/// slider drags into one stack entry. A drag from 1.0 → 1.5 → 2.0 → 3.5
/// within 500 ms produces one merged entry whose revert restores 1.0.
const SET_TIME_SCALE_ID: &str = "set-time-scale";

// ---------------------------------------------------------------------------
// CAD cuboid add entry point
// ---------------------------------------------------------------------------

/// Result of [`EditorShell::add_cad_cuboid_to_empty_scene`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CadCuboidAddResult {
    /// CAD checkpoint that was HEAD before the cuboid operation began.
    pub pre_add_head: CheckpointId,
    /// CAD checkpoint produced by committing the cuboid operation.
    pub committed_head: CheckpointId,
    /// ECS entity spawned by [`CadProjection::spawn_brep_entity`] for the cuboid.
    pub entity: KernelEntityId,
}

/// Error returned when the bounded CAD cuboid add entry point refuses or fails.
#[derive(Debug)]
pub enum CadCuboidAddError {
    /// The shell is not an empty/new scene for this bounded first mutation.
    SceneNotEmpty(&'static str),
    /// Error from the CAD checkpoint transaction.
    Checkpoint(CheckpointError),
    /// Error while adding or rooting the cuboid operator.
    Graph(GraphBuildError),
    /// Error while spawning or projecting the B-Rep entity.
    Projection(ProjectionError),
}

/// Read-only summary of the shell's current CAD scene/projection/render state.
///
/// This is a value snapshot for headless callers and tests. It exposes counts
/// and presence flags only; it does not expose graph, projection, world, render,
/// or GPU handles.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CadSceneInspection {
    /// Whether a CAD graph is installed on the shell.
    pub cad_graph_present: bool,
    /// Whether a CAD world is installed on the shell.
    pub cad_world_present: bool,
    /// Whether a CAD projection cache is installed on the shell.
    pub cad_projection_present: bool,
    /// Whether the shell tracks a CAD entity for the single-CAD render path.
    pub tracked_cad_entity_present: bool,
    /// Whether the tracked CAD entity is still live in the CAD world.
    pub tracked_cad_entity_live: bool,
    /// Whether the tracked CAD entity currently resolves through
    /// [`CadProjection::render_mesh_for`].
    pub tracked_cad_entity_render_mesh_present: bool,
    /// Current CAD graph checkpoint head, when a graph is installed.
    pub cad_graph_head: Option<CheckpointId>,
    /// Whether the installed CAD graph has a root node.
    pub cad_graph_root_present: bool,
    /// Number of nodes in the installed CAD graph.
    pub cad_graph_node_count: usize,
    /// Number of live CAD-world entities carrying [`BRepHandle`].
    pub live_brep_entity_count: usize,
    /// Number of live B-Rep entities that currently resolve through
    /// [`CadProjection::render_mesh_for`].
    pub projection_render_mesh_count: usize,
    /// Number of prebuilt render-only meshes attached to the shell.
    pub prebuilt_render_mesh_count: usize,
    /// Number of GPU-uploaded meshes already present on the shell.
    pub uploaded_mesh_count: usize,
    /// Whether the current scene has finite bounds suitable for framing.
    pub current_scene_frameable: bool,
    /// Whether the current CAD selection has finite bounds suitable for framing.
    pub selected_cad_scene_frameable: bool,
}

impl fmt::Display for CadCuboidAddError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SceneNotEmpty(reason) => write!(f, "scene is not empty: {reason}"),
            Self::Checkpoint(error) => write!(f, "CAD checkpoint operation failed: {error}"),
            Self::Graph(error) => write!(f, "CAD operator graph operation failed: {error}"),
            Self::Projection(error) => write!(f, "CAD projection operation failed: {error}"),
        }
    }
}

impl Error for CadCuboidAddError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::SceneNotEmpty(_) => None,
            Self::Checkpoint(error) => Some(error),
            Self::Graph(error) => Some(error),
            Self::Projection(error) => Some(error),
        }
    }
}

impl From<CheckpointError> for CadCuboidAddError {
    fn from(error: CheckpointError) -> Self {
        Self::Checkpoint(error)
    }
}

impl From<GraphBuildError> for CadCuboidAddError {
    fn from(error: GraphBuildError) -> Self {
        Self::Graph(error)
    }
}

impl From<ProjectionError> for CadCuboidAddError {
    fn from(error: ProjectionError) -> Self {
        Self::Projection(error)
    }
}

fn cad_entity_has_live_brep_handle(cad_world: &KernelWorld, cad_entity: KernelEntityId) -> bool {
    cad_world
        .entity(cad_entity)
        .map(|entity| entity.get::<BRepHandle>().is_some())
        .unwrap_or(false)
}

impl EditorShell {
    /// Clear the stored single-CAD entity id only when it no longer resolves
    /// to a live CAD-world entity carrying [`BRepHandle`].
    ///
    /// This is intentionally narrower than projection cleanup: render-mesh
    /// availability is inspection data, not the stale-id predicate. The helper
    /// mutates only `self.cad_entity`.
    pub fn clear_stale_tracked_cad_entity(&mut self) -> bool {
        let Some(cad_entity) = self.cad_entity else {
            return false;
        };
        let tracked_entity_live = self
            .cad_world
            .as_ref()
            .map(|cad_world| cad_entity_has_live_brep_handle(cad_world, cad_entity))
            .unwrap_or(false);
        if tracked_entity_live {
            return false;
        }

        self.cad_entity = None;
        true
    }

    /// Inspect the shell's current CAD scene shape without mutating editor state.
    ///
    /// The uploaded mesh count is a pure read of `self.meshes.len()`. This method
    /// does not tick projection, initialize render state, enqueue actions, upload
    /// meshes, touch GPU resources, resume a window, or mutate selection/camera.
    #[must_use]
    pub fn cad_scene_inspection(&self) -> CadSceneInspection {
        let (cad_graph_head, cad_graph_root_present, cad_graph_node_count) =
            if let Some(cad_graph) = self.cad_graph.as_ref() {
                let graph = cad_graph.graph();
                (
                    Some(cad_graph.head()),
                    graph.root().and_then(|root| graph.node(root)).is_some(),
                    graph.node_count(),
                )
            } else {
                (None, false, 0)
            };

        let live_brep_entity_count = self
            .cad_world
            .as_ref()
            .map(|cad_world| cad_world.query::<BRepHandle>().count())
            .unwrap_or(0);

        let projection_render_mesh_count = if let (Some(projection), Some(cad_world)) =
            (self.projection.as_ref(), self.cad_world.as_ref())
        {
            cad_world
                .query::<BRepHandle>()
                .filter_map(|(entity, _)| projection.render_mesh_for(entity, cad_world))
                .count()
        } else {
            0
        };

        let tracked_cad_entity_live = if let (Some(cad_world), Some(cad_entity)) =
            (self.cad_world.as_ref(), self.cad_entity)
        {
            cad_entity_has_live_brep_handle(cad_world, cad_entity)
        } else {
            false
        };

        let tracked_cad_entity_render_mesh_present =
            if let (Some(projection), Some(cad_world), Some(cad_entity)) = (
                self.projection.as_ref(),
                self.cad_world.as_ref(),
                self.cad_entity,
            ) {
                projection.render_mesh_for(cad_entity, cad_world).is_some()
            } else {
                false
            };

        CadSceneInspection {
            cad_graph_present: self.cad_graph.is_some(),
            cad_world_present: self.cad_world.is_some(),
            cad_projection_present: self.projection.is_some(),
            tracked_cad_entity_present: self.cad_entity.is_some(),
            tracked_cad_entity_live,
            tracked_cad_entity_render_mesh_present,
            cad_graph_head,
            cad_graph_root_present,
            cad_graph_node_count,
            live_brep_entity_count,
            projection_render_mesh_count,
            prebuilt_render_mesh_count: self.prebuilt_render_meshes.len(),
            uploaded_mesh_count: self.meshes.len(),
            current_scene_frameable: self.current_scene_bounds().is_some(),
            selected_cad_scene_frameable: self.selected_cad_scene_bounds().is_some(),
        }
    }

    /// Add exactly one default CAD cuboid to an empty/new CAD scene.
    ///
    /// This is intentionally a headless shell entry point, not a CommandBus
    /// action, menu route, shortcut, save/dirty signal, or global undo path.
    /// The only accepted starting state is a shell with no CAD graph,
    /// projection, CAD world, live CAD entity, prebuilt render meshes, or
    /// uploaded render meshes. A stale tracked CAD entity id is normalized
    /// before the guard; any populated or partially populated scene returns a
    /// clear error rather than replacing an existing root or inventing
    /// composition behavior.
    pub fn add_cad_cuboid_to_empty_scene(
        &mut self,
    ) -> Result<CadCuboidAddResult, CadCuboidAddError> {
        self.clear_stale_tracked_cad_entity();
        if self.cad_world.is_some()
            || self.projection.is_some()
            || self.cad_graph.is_some()
            || self.cad_entity.is_some()
        {
            return Err(CadCuboidAddError::SceneNotEmpty(
                "CAD graph/projection/world/entity state is already initialized",
            ));
        }
        if !self.prebuilt_render_meshes.is_empty() || !self.meshes.is_empty() {
            return Err(CadCuboidAddError::SceneNotEmpty(
                "render content is already initialized",
            ));
        }

        let mut graph = CadGraph::new();
        let pre_add_head = graph.head();
        graph.begin_operation()?;
        let cuboid_node = graph
            .graph_mut()?
            .add_operator(OperatorNode::Cuboid(CuboidOp::default()))?;
        graph.graph_mut()?.set_root(cuboid_node)?;
        let committed_head = graph.commit("editor-shell-add-default-cuboid")?;

        let mut cad_world = KernelWorld::new();
        cad_world.register_snapshot_component::<BRepHandle>();
        let mut projection = CadProjection::new();
        let entity = projection.spawn_brep_entity(&mut cad_world, cuboid_node)?;
        let tolerance =
            Tolerance::new(0.001).expect("literal 0.001 tolerance is positive and finite");
        projection.tick(&mut cad_world, &graph, tolerance)?;

        self.cad_world = Some(cad_world);
        self.projection = Some(projection);
        self.cad_graph = Some(graph);
        self.cad_entity = Some(entity);

        Ok(CadCuboidAddResult {
            pre_add_head,
            committed_head,
            entity,
        })
    }
}

/// [`rge_editor_actions::Action`] that sets the [`TimeScale`] resource in
/// `rge_kernel_ecs::World`. `apply` switches to `to`, `revert` switches back
/// to `from`; coalescing keeps the original `from` while adopting the newer
/// `to`.
///
/// # `Send + Sync` discipline
///
/// `Action: Send + Sync + 'static` is a hard trait bound. This struct
/// therefore carries plain `f32` values captured at submit time and uses no
/// interior mutability (`Cell` / `RefCell` would break `Sync`). The values
/// are clamped at submit time by [`TimeScale::with_value`] before being
/// stored, so apply/revert never need to re-clamp.
///
/// # Coalesce-and-merge semantics
///
/// When a follow-up `SetTimeScale` action arrives within the bus's 500 ms
/// window with the same [`Action::id`], the bus calls
/// [`Action::merge`]; this impl parses the new `to` from `next.payload()`
/// and updates `self.to` while keeping `self.from` untouched. The bus then
/// applies the new action (advancing the world to the new `to`), but the
/// stack entry that absorbed the merge is `self` — so undo restores the
/// pre-drag `from`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SetTimeScale {
    /// The `TimeScale` value the world had at submit time. Restored on revert.
    pub from: f32,
    /// The `TimeScale` value to apply. Already clamped to
    /// `[TimeScale::MIN, TimeScale::MAX]` by [`TimeScale::with_value`].
    pub to: f32,
}

impl SetTimeScale {
    /// Construct a [`SetTimeScale`] action with pre-clamped `from` and `to`.
    ///
    /// The caller is responsible for capturing `from` from the current
    /// [`TimeScale`] resource value before submit; [`EditorShell::set_time_scale`]
    /// does that handoff.
    #[must_use]
    pub fn new(from: f32, to: f32) -> Self {
        Self { from, to }
    }
}

impl Action for SetTimeScale {
    fn name(&self) -> &str {
        "set-time-scale"
    }

    fn id(&self) -> ActionId {
        // Constant id (not parameterised by the f32 values) so rapid slider
        // drags within the 500 ms coalesce window collapse into one stack
        // entry. See `merge` for how the merged entry preserves the
        // original `from`.
        ActionId::new(SET_TIME_SCALE_ID)
    }

    fn payload(&self) -> Vec<u8> {
        // Encode `to` as 4 little-endian bytes so `merge` can parse a
        // follow-up action's payload without needing serde or a downcast.
        //
        // Bus-ledger interaction (subtle):
        //
        // - On the INITIAL (non-coalesced) submit the bus records one
        //   `EventKind::Action` ledger entry carrying these bytes — the
        //   ledger therefore captures the FIRST `to` value of a drag.
        // - On subsequent COALESCED submits (same `id`, within the bus's
        //   500 ms window) the bus calls `merge`, mutates the existing
        //   stack entry's `to` in-place, and applies the new action to
        //   the world — but DOES NOT record a new ledger event and DOES
        //   NOT refresh the original entry's payload bytes on the ledger.
        //   See `rge_editor_actions::CommandBus::submit` §6.16.7
        //   ("the world advances via a direct `action.apply` — no new
        //   stack entry, no new ledger event").
        //
        // Net: the bus's audit ledger records only one event per drag
        // burst, carrying the FIRST `to`. The editor-shell's own audit
        // ledger (dual-ledger per locked decision #3) is the source of
        // truth for per-change history — `set_time_scale` records a
        // `TimeScaleChanged { from, to }` event on it for every non-no-op
        // call, regardless of whether the bus coalesced the submit.
        self.to.to_le_bytes().to_vec()
    }

    fn apply(&self, world: &mut KernelWorld) -> Result<(), ActionResult> {
        // `insert_resource` REPLACES any existing TimeScale per kernel/ecs
        // `World::insert_resource`. Clamp via `with_value` defensively even
        // though `EditorShell::set_time_scale` already clamps before submit.
        world.insert_resource(TimeScale::with_value(self.to));
        Ok(())
    }

    fn revert(&self, world: &mut KernelWorld) -> Result<(), ActionResult> {
        world.insert_resource(TimeScale::with_value(self.from));
        Ok(())
    }

    fn merge(&mut self, next: &dyn Action) -> MergeOutcome {
        // Parse `next.payload()` as 4-byte little-endian f32 (matches
        // `Self::payload`'s encoding). Reject any payload of unexpected
        // shape — that would indicate an unrelated action sharing our id
        // (which shouldn't happen given `SET_TIME_SCALE_ID` is private),
        // but the cheap shape check is defensive against future
        // misconfiguration.
        let payload = next.payload();
        let Ok(bytes): Result<[u8; 4], _> = payload.try_into() else {
            return MergeOutcome::Distinct;
        };
        let new_to = f32::from_le_bytes(bytes);
        // Re-clamp into the valid range so a corrupted payload can't shove
        // an out-of-range value into the merged entry.
        let new_to = new_to.clamp(TimeScale::MIN, TimeScale::MAX);
        self.to = new_to;
        MergeOutcome::Merged
    }
}

// ---------------------------------------------------------------------------
// Shell-side CommandBus surface
// ---------------------------------------------------------------------------

impl EditorShell {
    /// Submit an [`Action`] through the Command Bus. Returns the bus's
    /// error verbatim (so callers can distinguish coalesce / ledger /
    /// apply failures); [`EditorShell::route_menu_command`] wraps the undo/redo
    /// calls and swallows `NothingToUndo` / `NothingToRedo`.
    ///
    /// # Errors
    ///
    /// Propagates [`BusError`] from [`rge_editor_actions::CommandBus::submit`].
    pub fn submit_action(&mut self, action: Box<dyn Action>) -> Result<(), BusError> {
        self.command_bus.submit(action, self.world.kernel_mut())
    }

    /// Undo the most recent action via the Command Bus.
    ///
    /// # Errors
    ///
    /// Propagates [`BusError`] from [`rge_editor_actions::CommandBus::undo`].
    /// `NothingToUndo` is returned (not panicked); the `Command::Undo` arm of
    /// [`EditorShell::route_menu_command`] ignores it.
    pub fn undo_command(&mut self) -> Result<(), BusError> {
        self.command_bus.undo(self.world.kernel_mut())
    }

    /// Redo the next action via the Command Bus.
    ///
    /// # Errors
    ///
    /// Propagates [`BusError`] from [`rge_editor_actions::CommandBus::redo`].
    /// `NothingToRedo` is returned (not panicked); the `Command::Redo` arm of
    /// [`EditorShell::route_menu_command`] ignores it.
    pub fn redo_command(&mut self) -> Result<(), BusError> {
        self.command_bus.redo(self.world.kernel_mut())
    }

    /// Mark the current bus cursor as the saved point. Drives
    /// `CommandBus::is_dirty()` to `false` until the next submit / undo /
    /// redo moves the cursor away from this position.
    pub fn mark_saved_command(&mut self) {
        self.command_bus.mark_saved();
    }

    /// Borrow the Command Bus for read-only introspection (tests, future
    /// status-bar dirty indicator). Mutations route through
    /// [`Self::submit_action`] / [`Self::undo_command`] /
    /// [`Self::redo_command`] / [`Self::mark_saved_command`] — never
    /// through this accessor.
    #[must_use]
    pub fn command_bus(&self) -> &rge_editor_actions::CommandBus {
        &self.command_bus
    }

    /// Dispatch a single editor key command — the execution-only time-scale binds
    /// (`Ctrl+2` / `Ctrl+0` / `Ctrl+4`). Public so headless tests can drive the
    /// slider path without synthesizing winit `KeyEvent`s; production usage routes
    /// through the `WindowEvent::KeyboardInput` branch in `Self::window_event`.
    /// Each arm delegates to [`Self::set_time_scale`], whose no-op short-circuit
    /// means a bind fired at the already-current value grows neither the bus stack
    /// nor the dirty flag.
    ///
    /// The File/Edit accelerators (Open / Save / Save-As / Undo / Redo /
    /// Select All) are
    /// dispatched by [`Self::route_menu_command`], not here, since the W08.3
    /// cutover + the W08.4 retirement of their `EditorKeyCommand` mirror.
    pub fn handle_key_command(&mut self, command: EditorKeyCommand) {
        match command {
            EditorKeyCommand::SetTimeScaleDoubleSpeed => self.set_time_scale(2.0),
            EditorKeyCommand::ResetTimeScaleDefault => self.set_time_scale(TimeScale::DEFAULT),
            EditorKeyCommand::SetTimeScaleMaxFastForward => self.set_time_scale(TimeScale::MAX),
        }
    }

    /// Adjust the time-scale slider. Captures `from` from the current
    /// [`TimeScale`] resource value, builds a [`SetTimeScale`] action, and
    /// routes it through [`Self::submit_action`]. Also records an
    /// [`AuditEvent::TimeScaleChanged`] on the editor-shell's own
    /// audit ledger so the existing
    /// `time_scale_audit_event_records_change` test keeps passing
    /// byte-identically (dual-ledger per the locked dispatch decision).
    ///
    /// Submit errors from the bus are surfaced via `tracing::warn!` but
    /// not propagated — the slider has no user-visible failure mode today,
    /// and `SetTimeScale::apply` cannot fail (it only calls
    /// `insert_resource` which is infallible).
    ///
    /// # Coalesce behaviour
    ///
    /// Rapid drags within the bus's 500 ms coalesce window collapse into a
    /// single undo-stack entry, because [`SetTimeScale::id`] returns a
    /// constant and [`SetTimeScale::merge`] preserves the pre-drag `from`.
    /// One Ctrl+Z then restores the slider to its pre-drag value.
    pub fn set_time_scale(&mut self, value: f32) {
        let from = self.time_scale().value();
        let to = TimeScale::with_value(value).value();
        // No-op short-circuit: when the post-clamp `to` already equals the
        // current resource value (the same f32 bits), skip the bus submit
        // AND the editor-shell audit event so:
        // (a) the bus does NOT flip `is_dirty()` for a no-change input
        //     (otherwise a slider repaint or a programmatic `set_time_scale(current)`
        //     would mark the project dirty for nothing);
        // (b) the bus undo-stack does NOT grow with no-op entries (a Ctrl+Z
        //     against a no-op entry would silently do nothing — confusing UX);
        // (c) the editor-shell audit ledger does NOT accumulate phantom
        //     `TimeScaleChanged { from: X, to: X }` events (consumers count
        //     events by tag and would treat the no-op as a real change).
        // Equality test uses `f32::EPSILON` to absorb any 1-ulp drift across
        // the clamp-then-compare round-trip (in practice the clamp is a
        // straight pass-through when the input is already in-range so the
        // bits match exactly, but the tolerance is harmless).
        if (to - from).abs() < f32::EPSILON {
            return;
        }
        let action = Box::new(SetTimeScale::new(from, to));
        if let Err(e) = self.submit_action(action) {
            tracing::warn!(
                target: "rge::editor-shell::lifecycle",
                error = ?e,
                "SetTimeScale submit failed (non-fatal; slider state unchanged)"
            );
            return;
        }
        // Dual-ledger: also record on the editor-shell's own ring-buffer
        // audit ledger so existing tests (`time_scale_audit_event_records_change`)
        // continue to count the right number of `TimeScaleChanged` events.
        self.audit.record(AuditEvent::TimeScaleChanged { from, to });
    }
}
