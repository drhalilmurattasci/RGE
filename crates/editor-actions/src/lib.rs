//! `rge-editor-actions` — Command Bus + `UndoStack`.
//!
//! Failure class: snapshot-recoverable
//!
//! Command Bus + `UndoStack` per PLAN §6.16. The single mediation layer for
//! editor mutations into runtime state.
//!
//! # Overview
//!
//! Every editor mutation must flow through [`CommandBus::submit`]. The bus
//! applies the action to the [`rge_kernel_ecs::World`], projects it to the
//! audit ledger, and pushes it onto the [`UndoStack`]. Same-target actions
//! submitted within a 500 ms window are coalesced by default (§6.16.7).
//!
//! # Failure class
//!
//! `snapshot-recoverable`: bus-state corruption is recoverable via snapshot
//! restore + audit-ledger replay. Plain `recoverable` would be too lax —
//! losing the undo stack mid-session matters.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod action;
pub mod bus;
pub mod coalesce;
pub mod compound;
pub mod undo_stack;

pub use action::{
    Action, ActionContext, ActionContextFamily, ActionId, ActionResult, ActionView, ActionViewRef,
    MergeOutcome, WorldActionContext,
};
pub use bus::{BusEntry, BusError, CommandBus};
pub use coalesce::CoalesceWindow;
pub use compound::CompoundAction;
pub use undo_stack::{SaveMark, UndoStack};
