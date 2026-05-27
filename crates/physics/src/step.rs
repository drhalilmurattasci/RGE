//! Fixed-timestep physics step.
//!
//! Single entry point [`physics_step`]: advances the [`World`](crate::World)
//! by exactly one tick of [`FIXED_DT`] seconds and records inputs to the
//! physics-domain
//! [`PhysicsInputLedger`](crate::physics_input_ledger::PhysicsInputLedger)
//! so replay can reproduce the trajectory. The ledger here is
//! **`PhysicsInputLedger`**, not
//! [`rge_kernel_audit_ledger::AuditLedger`] — the two have structurally
//! different domains; see the
//! [`physics_input_ledger`](crate::physics_input_ledger) module docs for
//! the divergence rationale.
//!
//! ## Why fixed-step
//!
//! Variable-dt integration is non-deterministic over wall-clock noise:
//! identical inputs but a 16.6 ms vs 16.7 ms `dt` slip produce divergent
//! collision responses. PLAN.md §1.6.8 makes "fixed-timestep physics" a
//! pre-condition of the Replay-Stable mode. We pick **60 Hz** as the v1.0
//! cadence; multi-rate (e.g., 120 Hz physics under 60 Hz render) is a
//! post-Phase-5 concern.
//!
//! ## Input recording (`PhysicsInputLedger`)
//!
//! Every tick begins with a `ledger.begin_tick(world.tick)` which appends a
//! fresh [`TickRecord`](crate::physics_input_ledger::TickRecord). Forces,
//! impulses, and joint motor commands applied during the tick land in that
//! record as typed
//! [`PhysicsInput`](crate::physics_input_ledger::PhysicsInput) variants.
//! A replay run uses the same ledger to reapply those inputs in the same
//! order on a fresh world; same code + same per-tick ledger ⇒ same
//! trajectory (same-machine).

use rapier3d::math::Vector;

use crate::physics_input_ledger::{PhysicsInput, PhysicsInputLedger};
use crate::world::{PhysicsHandle, World};

/// Physics tick rate.
pub const PHYSICS_HZ: u32 = 60;

/// Physics fixed delta-time in seconds (`1 / PHYSICS_HZ`).
// `60 as f32` is exact so the `cast_precision_loss` warning is a false
// positive here — the resulting value (1/60) is the canonical 60 Hz dt.
#[allow(clippy::cast_precision_loss)]
pub const FIXED_DT: f32 = 1.0 / PHYSICS_HZ as f32;

/// Apply a one-tick external force to a body and record it for replay.
///
/// Per the §1.6.8 contract, **every** non-deterministic input that crosses the
/// boundary into the solver has to be recorded. For replay we re-apply via
/// [`apply_recorded_inputs`].
pub fn apply_force(
    world: &mut World,
    ledger: &mut PhysicsInputLedger,
    handle: PhysicsHandle,
    force: [f32; 3],
) {
    if let Some(b) = world.bodies.get_mut(handle.body) {
        b.add_force(Vector::new(force[0], force[1], force[2]), true);
    }
    record_current(
        ledger,
        world.tick,
        PhysicsInput::Force {
            body: handle.id,
            force,
        },
    );
}

/// Apply an instantaneous impulse and record it.
pub fn apply_impulse(
    world: &mut World,
    ledger: &mut PhysicsInputLedger,
    handle: PhysicsHandle,
    impulse: [f32; 3],
) {
    if let Some(b) = world.bodies.get_mut(handle.body) {
        b.apply_impulse(Vector::new(impulse[0], impulse[1], impulse[2]), true);
    }
    record_current(
        ledger,
        world.tick,
        PhysicsInput::Impulse {
            body: handle.id,
            impulse,
        },
    );
}

/// Append an input to the *current* tick's record.
///
/// If the current tick hasn't been begun yet (no `begin_tick`), we begin it
/// here so the input doesn't get dropped. This makes the order
/// `apply_*` → `physics_step` safe and the order
/// `physics_step` → `apply_*` (next tick) also safe.
fn record_current(ledger: &mut PhysicsInputLedger, tick: u64, input: PhysicsInput) {
    let needs_begin = ledger.records.last().map_or(true, |r| r.tick != tick);
    if needs_begin {
        ledger.begin_tick(tick);
    }
    ledger
        .records
        .last_mut()
        .expect("just ensured at least one record")
        .inputs
        .push(input);
}

/// Re-apply ledger inputs for the current tick onto the world before stepping.
///
/// Intended for replay: drives a fresh world from the recorded stream so the
/// trajectory matches the original run.
pub fn apply_recorded_inputs(
    world: &mut World,
    ledger: &PhysicsInputLedger,
    body_lookup: impl Fn(u64) -> Option<PhysicsHandle>,
) {
    let Some(record) = ledger.for_tick(world.tick) else {
        return;
    };
    for input in &record.inputs {
        match input {
            PhysicsInput::Force { body, force } => {
                if let Some(handle) = body_lookup(*body) {
                    if let Some(b) = world.bodies.get_mut(handle.body) {
                        b.add_force(Vector::new(force[0], force[1], force[2]), true);
                    }
                }
            }
            PhysicsInput::Impulse { body, impulse } => {
                if let Some(handle) = body_lookup(*body) {
                    if let Some(b) = world.bodies.get_mut(handle.body) {
                        b.apply_impulse(Vector::new(impulse[0], impulse[1], impulse[2]), true);
                    }
                }
            }
            PhysicsInput::JointMotor { .. } => {
                // v0.0.1: motor recording surface exists but joint authoring
                // hasn't reached motors yet. The replay path is wired so that
                // when it does, only this match arm grows.
            }
        }
    }
}

/// Advance the world by exactly one fixed timestep.
///
/// Pre-condition: any inputs for this tick (forces, impulses, joint commands)
/// have already been applied via [`apply_force`] / [`apply_impulse`] /
/// [`apply_recorded_inputs`].
///
/// Post-condition: `world.tick` is incremented; per-tick contact events are
/// available on the broadphase/narrowphase pair channels for [`crate::events`]
/// to drain.
pub fn physics_step(world: &mut World, ledger: &mut PhysicsInputLedger) {
    // Ensure the current tick has a ledger record even if no inputs landed,
    // so tick indices line up perfectly with replay walkers.
    let needs_begin = ledger.records.last().map_or(true, |r| r.tick != world.tick);
    if needs_begin {
        ledger.begin_tick(world.tick);
    }

    // We collect events out-of-band via the contact-pair queue (see
    // `crate::events`). The unit-impls of `PhysicsHooks` and `EventHandler`
    // for `()` are no-ops which is what we want — the pipeline wants `&dyn`
    // here, not real callback objects.
    let physics_hooks: () = ();
    let event_handler: () = ();

    // Scope the borrows on `world`'s fields so they're released before we
    // increment `world.tick`. `pipeline.step` needs simultaneous &mut access
    // to many disjoint fields; we open them inside the scope.
    //
    // NOTE: rapier 0.32 dropped the optional `query_pipeline` argument from
    // `step()`. The query-pipeline is now a transient view obtained from the
    // broadphase via `as_query_pipeline()` whenever a spatial query is needed
    // (see `crate::character`). The broadphase's BVH is rebuilt by
    // `pipeline.step` itself so consumers always see fresh data.
    {
        let bodies = &mut world.bodies;
        let colliders = &mut world.colliders;
        let islands = &mut world.islands;
        let broadphase = &mut world.broadphase;
        let narrowphase = &mut world.narrowphase;
        let impulse_joints = &mut world.impulse_joints;
        let multibody_joints = &mut world.multibody_joints;
        let ccd = &mut world.ccd;
        let params = &world.params;
        let pipeline = &mut world.pipeline;
        let gravity = world.gravity;

        pipeline.step(
            gravity,
            params,
            islands,
            broadphase,
            narrowphase,
            bodies,
            colliders,
            impulse_joints,
            multibody_joints,
            ccd,
            &physics_hooks,
            &event_handler,
        );
    }

    world.tick += 1;
}
