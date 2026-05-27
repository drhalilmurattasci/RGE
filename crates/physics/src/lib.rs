//! `rge-physics` — `Rapier3D` wrap, ECS schedule integration, deterministic replay.
//!
//! Failure class: snapshot-recoverable
//!
//! Wave **W11** deliverable per [`tasks/W11/PLAN.md`](../../tasks/W11/PLAN.md);
//! architecture per [PLAN.md §6.10](../../plans/PLAN.md) and determinism mode
//! [§1.6.8](../../plans/PLAN.md) ("Replay-Stable v1.0", same-machine
//! gameplay-only).
//!
//! ## Surface
//!
//! - [`World`] — single Rapier-backed world resource (one per ECS world).
//! - [`SCHEDULE_STAGES`] — fixed four-stage ordering: `pre_physics` →
//!   `physics_step` → `post_physics` → `contact_events`.
//! - [`sync`] — bidirectional `Transform ↔ RigidBody` sync, change-detection
//!   driven (writes from ECS go in pre-step; writes from solver come out
//!   post-step).
//! - [`step`] — fixed 60 Hz `physics_step`; records per-tick inputs (forces,
//!   impulses, joint motor torques) into the physics-domain
//!   [`physics_input_ledger::PhysicsInputLedger`] (typed per-tick records,
//!   structurally distinct from the generic
//!   [`rge_kernel_audit_ledger::AuditLedger`] event log — see
//!   [`physics_input_ledger`] module docs for the divergence rationale).
//! - [`events`] — Rapier contact pairs → typed `CollisionStarted`,
//!   `CollisionEnded`, `TriggerEntered`, `TriggerExited` channels.
//! - [`character`] — kinematic capsule [`CharacterController`]
//!   (`slope_limit`, `step_offset`).
//! - [`joint`] — `Revolute`/`Prismatic`/`Spherical`/`Fixed` mappings to Rapier.
//!
//! ## Local-twin substrate ([`stubs`] module)
//!
//! The [`stubs`] module carries minimal local twins of `components-physics`
//! (W01) and `kernel/events` so this crate compiles & tests in isolation.
//! When the upstream `pub use` swap-over lands, those twins go away and the
//! public types land in their proper crates without touching this code's
//! surface.
//!
//! The W11 dispatch originally listed `kernel/audit-ledger` alongside the
//! above as a local-twin substrate, but that crate has since shipped as
//! [`rge_kernel_audit_ledger::AuditLedger`] (Phase 2.3) and physics
//! intentionally does **not** migrate onto it. Per
//! [`physics_input_ledger`]'s module-level docs, the two have structurally
//! different domains (per-tick typed `PhysicsInput` records vs. generic
//! `Event` payload+BLAKE3 stream), so physics keeps its own domain ledger
//! ([`physics_input_ledger::PhysicsInputLedger`]) — that type is the
//! intentional physics-domain ledger surface, not a "stub" of the kernel
//! substrate.
//!
//! ## Determinism contract
//!
//! Same-machine, same-binary: 1000-tick replays produce byte-identical
//! [`World::serialize_state`] output. We rely on:
//!
//! 1. **Fixed timestep** — 1/60 s, never `dt`-driven.
//! 2. **`enhanced-determinism` Cargo feature** on `rapier3d` — selects the
//!    deterministic broadphase + solver order.
//! 3. **Pinned versions** in workspace `Cargo.toml` (`rapier3d = "0.32"`).
//! 4. **No floating-point env reads** — no time-of-day, no entropy. RNG, if
//!    needed downstream, must be seeded from the per-tick
//!    [`physics_input_ledger::PhysicsInputLedger`] (or another deterministic
//!    upstream source — not from wall-clock or env entropy).
//!
//! Cross-platform `Lockstep-Stable` is **explicitly out of scope** at v1.0
//! per §1.6.8.

pub mod character;
pub mod events;
pub mod joint;
pub mod participate;
pub mod physics_input_ledger;
pub mod plugin_adapter;
pub mod step;
pub mod stubs;
pub mod sync;
pub mod world;

pub use character::{CharacterController, CharacterMove};
pub use events::{
    CollisionEnded, CollisionStarted, ContactEventChannel, TriggerEntered, TriggerExited,
};
pub use joint::{Joint, JointHandle, JointKind};
pub use participate::PHYSICS_WORLD_PARTICIPANT_ID;
pub use plugin_adapter::{PhysicsPlugin, PHYSICS_PLUGIN_ID};
pub use step::{physics_step, FIXED_DT, PHYSICS_HZ};
pub use stubs::components_physics::{BodyKind, Collider, ColliderShape, RigidBody, Velocity};
pub use sync::{post_physics, pre_physics, Transform};
pub use world::{PhysicsHandle, World};

/// The four ordered schedule stages this crate contributes.
///
/// Equivalent to a `kernel/schedule` `Stage` enum once W11.5 lands. Keeping
/// this as a string slice array means the consumer can wire it into whatever
/// scheduler back-end ships first.
pub const SCHEDULE_STAGES: [&str; 4] = [
    "pre_physics",
    "physics_step",
    "post_physics",
    "contact_events",
];

/// One full ordered tick: sync ECS → world, step, sync world → ECS, drain
/// events. Driver function used by tests and by the bench harness; production
/// systems wire each stage to the kernel scheduler instead.
pub fn run_tick(
    world: &mut World,
    transforms: &mut [(PhysicsHandle, Transform)],
    velocities: &mut [(PhysicsHandle, Velocity)],
    events: &ContactEventChannel,
    ledger: &mut physics_input_ledger::PhysicsInputLedger,
) {
    pre_physics(world, transforms, velocities);
    physics_step(world, ledger);
    post_physics(world, transforms, velocities);
    events::drain(world, events);
}
