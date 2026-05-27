//! Pin the `PhysicsInputLedger` ↔ generic kernel audit-ledger boundary.
//!
//! `crates/physics` deliberately owns a **physics-domain per-tick input
//! ledger** ([`rge_physics::physics_input_ledger::PhysicsInputLedger`]) that
//! is structurally distinct from
//! [`rge_kernel_audit_ledger::AuditLedger`]'s generic event log (typed
//! `PhysicsInput` variants vs. opaque `payload: Vec<u8>` + BLAKE3 `EventId`
//! — see `physics_input_ledger.rs` module-level docs for the divergence
//! rationale). These tests pin two structural boundary properties so the
//! distinction can't silently rot:
//!
//! 1. **Typed-resource routing**: the `PhysicsPlugin` tick contract is keyed
//!    by Rust type. A foreign resource staged in the [`PluginContext`] does
//!    NOT substitute for [`PhysicsInputLedger`]; the missing-ledger path
//!    still surfaces
//!    [`PluginError::ContractViolation { resource_type: "PhysicsInputLedger" }`]
//!    and the already-staged [`World`] is preserved (idempotent failure
//!    semantics — matching the existing `plugin_adapter_smoke.rs`
//!    `_when_input_ledger_missing` precedent, but with a sibling resource
//!    in the registry to prove no other type can stand in).
//!
//! 2. **Typed-record domain**: per-tick [`PhysicsInputLedger::records`]
//!    hold only [`PhysicsInput`] variants (`Force` / `Impulse` /
//!    `JointMotor`) — not opaque byte payloads. This is the structural
//!    asymmetry vs. the generic kernel audit ledger that justifies
//!    keeping the two ledgers separate; a future refactor that loses
//!    the typed-domain property must update or remove this test.

use rge_kernel_diagnostics::DiagnosticAggregator;
use rge_kernel_plugin_host::{Plugin, PluginContext, PluginError};
use rge_physics::physics_input_ledger::{PhysicsInput, PhysicsInputLedger};
use rge_physics::step::{apply_force, apply_impulse, physics_step};
use rge_physics::stubs::components_physics::{BodyKind, Collider, ColliderShape, RigidBody};
use rge_physics::{PhysicsPlugin, World};

/// Foreign typed resource used as a "wrong-type-but-present" decoy in the
/// PluginContext. Has a vaguely ledger-shaped surface (a Vec of byte
/// payloads, mirroring the generic kernel audit-ledger payload shape) so
/// the test sharpens the point: even a structurally-similar foreign
/// resource cannot substitute for the typed `PhysicsInputLedger`.
#[derive(Debug, Default)]
struct ForeignGenericPayloadLedger {
    _payloads: Vec<Vec<u8>>,
}

#[test]
fn plugin_tick_with_world_and_foreign_resource_still_violates_physics_input_ledger() {
    let mut plugin = PhysicsPlugin::new();
    let mut diags = DiagnosticAggregator::new();
    let mut ctx = PluginContext::new(&mut diags);

    // Stage World + a foreign (non-PhysicsInputLedger) typed resource. The
    // foreign resource is present but it does NOT type-match the plugin's
    // PhysicsInputLedger requirement — so the take() lookup must still miss.
    assert!(ctx.insert(World::new()).is_none());
    assert!(ctx.insert(ForeignGenericPayloadLedger::default()).is_none());
    assert!(ctx.contains::<World>());
    assert!(ctx.contains::<ForeignGenericPayloadLedger>());
    assert!(
        !ctx.contains::<PhysicsInputLedger>(),
        "PhysicsInputLedger MUST NOT be reported present when only a foreign \
         resource type was staged — the registry must be keyed by Rust type",
    );

    let err = plugin.tick(&mut ctx).expect_err("tick must fail");
    match err {
        PluginError::ContractViolation { resource_type } => {
            assert_eq!(
                resource_type, "PhysicsInputLedger",
                "no other staged resource may substitute for the typed \
                 physics-domain input ledger",
            );
        }
        other => panic!("expected ContractViolation for PhysicsInputLedger; got {other:?}"),
    }

    // Idempotent failure: World was supplied and must be put back so the
    // orchestrator can recover it; the foreign sibling resource was never
    // touched and must remain in place.
    assert!(
        ctx.contains::<World>(),
        "World must be preserved across the contract violation",
    );
    assert!(
        ctx.contains::<ForeignGenericPayloadLedger>(),
        "foreign sibling resource must be untouched by the failed tick",
    );
    assert_eq!(ctx.resource_count(), 2);
    assert_eq!(plugin.steps_run(), 0);
}

#[test]
fn physics_input_ledger_records_only_typed_physics_domain_inputs() {
    let mut world = World::new();
    let _ground = world.insert_body(
        RigidBody {
            kind: BodyKind::Fixed,
            ..RigidBody::default()
        },
        Some(Collider {
            shape: ColliderShape::Plane,
            ..Collider::default()
        }),
        [0.0, 0.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    );
    let cube = world.insert_body(
        RigidBody {
            kind: BodyKind::Dynamic,
            mass: 1.0,
            ..RigidBody::default()
        },
        Some(Collider {
            shape: ColliderShape::Cuboid {
                hx: 0.5,
                hy: 0.5,
                hz: 0.5,
            },
            ..Collider::default()
        }),
        [0.0, 5.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    );

    let mut ledger = PhysicsInputLedger::new();

    // Tick 0: a Force input.
    apply_force(&mut world, &mut ledger, cube, [0.5, 0.0, 0.0]);
    physics_step(&mut world, &mut ledger);

    // Tick 1: an Impulse input.
    apply_impulse(&mut world, &mut ledger, cube, [0.0, 0.0, 0.25]);
    physics_step(&mut world, &mut ledger);

    // Tick 2: no inputs — the ledger must still hold an (empty) tick record
    // so per-tick indices line up with the replay walker.
    physics_step(&mut world, &mut ledger);

    assert_eq!(
        ledger.len(),
        3,
        "expected one TickRecord per tick (including the empty third tick)",
    );
    for (i, record) in ledger.records.iter().enumerate() {
        let expected_tick = i as u64;
        assert_eq!(
            record.tick, expected_tick,
            "tick monotonicity broken at record {i}",
        );
    }

    // Structural-boundary assertion: every recorded input is a typed
    // PhysicsInput variant (Force / Impulse / JointMotor). The exhaustive
    // match is intentional — if a new variant is ever added it must
    // either be domain-typed (compiles, test still pins the property) or
    // we've leaked an opaque payload (won't compile, forcing review).
    for record in &ledger.records {
        for input in &record.inputs {
            match input {
                PhysicsInput::Force { .. }
                | PhysicsInput::Impulse { .. }
                | PhysicsInput::JointMotor { .. } => {}
            }
        }
    }

    let force_count = ledger
        .records
        .iter()
        .flat_map(|r| r.inputs.iter())
        .filter(|i| matches!(i, PhysicsInput::Force { .. }))
        .count();
    let impulse_count = ledger
        .records
        .iter()
        .flat_map(|r| r.inputs.iter())
        .filter(|i| matches!(i, PhysicsInput::Impulse { .. }))
        .count();
    assert_eq!(
        force_count, 1,
        "expected exactly one Force input across all ticks"
    );
    assert_eq!(
        impulse_count, 1,
        "expected exactly one Impulse input across all ticks",
    );
}
