//! `rge-editor-shell` — editor host: winit lifecycle + Play-in-Editor (PIE) state machine.
//!
//! Failure class: snapshot-recoverable
//!
//! Per PLAN §1.13: editor-shell owns the [`PlayState`] state machine and the
//! [`WorldSnapshot`] that backs Play/Stop round-trip — the canonical PIE
//! snapshot mechanism (PLAN §6.13). PIE-snapshot failures (state-machine
//! invariant violation, snapshot decode error, restore mid-tick) are
//! recoverable by restoring from the most recent snapshot rather than
//! restarting the session. Mirrors physics + cad-projection + editor-actions
//! (stateful subsystems with PIE participation).
//!
//! Phase 5 deliverable per [`IMPLEMENTATION.md`](../../plans/IMPLEMENTATION.md).
//! Implements W03 dispatch (PLAN.md §6.13 PIE; §1.15 editor-state coordination).
//!
//! # Architecture
//!
//! `EditorShell` owns:
//! - the winit `ApplicationHandler` impl (in [`lifecycle`])
//! - the [`PlayState`] state machine
//! - the [`WorldSnapshot`] that backs Play/Stop round-trip
//! - the play-mode toolbar registration ([`play_toolbar`])
//! - the [`TimeScale`] slider (game systems scale; editor systems don't)
//! - a placeholder [`Viewport`] widget (renders "Editing"/"Playing" text)
//!
//! Authority boundaries (per PLAN.md §1.15):
//! - **runtime entity state** lives in `kernel/ecs::World` (stubbed locally
//!   here as [`world::World`]).
//! - **editor coordination state** (selection, active tool) lives in
//!   `crates/editor-state` and is re-exported via [`coord`].
//! - editor-state **does not** participate in `WorldSnapshot` — selection
//!   and active tool persist across Play/Stop cycles by virtue of living
//!   on the editor side of the boundary.
//!
//! # Phase 5 abort condition
//!
//! Per `IMPLEMENTATION.md` Phase 5: if PIE snapshot/restore exceeds 500ms on
//! a 10k-entity scene, ECS storage layout needs redesign. Timing harness
//! lives in [`snapshot::measure_round_trip`]; results are documented in
//! `RGE/plans/BASELINE.md`.

#![allow(clippy::module_name_repetitions)]

pub mod audit;
pub mod camera;
pub mod coord;
pub mod lifecycle;
mod pick_path;
pub mod play_state;
pub mod play_toolbar;
#[cfg(test)]
mod render_frame_e2e_perf;
mod render_input;
mod render_path;
pub mod snapshot;
pub mod time_scale;
pub mod viewport;
pub mod world;

pub use camera::{pick_face_at, CameraView, EditorCameraState};
pub use lifecycle::EditorShell;
pub use play_state::{PlayState, PlayStateError, PlayStateTransition};
pub use play_toolbar::{PlayToolbar, ToolbarButton, ToolbarButtonId};
pub use render_input::{RenderHandoff, RenderInput, RenderInputOwned};
pub use snapshot::{SnapshotMetrics, WorldSnapshot};
pub use time_scale::{TimeScale, TimeScaleClass};
pub use viewport::Viewport;
