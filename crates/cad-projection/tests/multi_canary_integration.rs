//! Multi-canary cross-substrate composition smoke test (audit-2026-05-09
//! round 6, finding C3).
//!
//! Closes a real coverage gap: each Tier-2 canary plugin
//! (`CadProjectionPlugin` / `GfxPlugin` / `PhysicsPlugin` / `AudioPlugin`) is
//! exercised in isolation by its own `tests/plugin_adapter_smoke.rs`, but no
//! existing test stages all four canary substrates simultaneously in a single
//! [`PluginContext`] and drives the unified `PluginHost` lifecycle across the
//! whole quartet. This file fills that gap with one composition smoke test.
//!
//! # Why this lives in `cad-projection`
//!
//! Cargo only auto-discovers integration tests under each crate's own
//! `tests/` directory; a workspace-root `tests/` folder would not be picked
//! up by `cargo test --workspace`. The ADR-114 four-canary suite settled on
//! cad-projection as the natural composition root because it is the only
//! canary that already has a Tier-2 dependency relationship (cad-core), and
//! its `tests/cross_substrate_determinism.rs` already establishes the
//! cross-substrate-determinism precedent. The other three canary crates
//! (gfx / physics / audio) are pulled in as `[dev-dependencies]` so the
//! production cad-projection dep graph is unchanged — `rge-cad-projection`'s
//! release artefact still has the same direct deps as before.
//!
//! # What's verified
//!
//! 1. The four canary plugins register cleanly under their canonical IDs
//!    (`rge-cad-projection.brep-handles-plugin`,
//!    `rge-gfx.headless-triangle-plugin`, `rge-physics.fixed-step-plugin`,
//!    `rge-audio.scheduling-plugin`).
//! 2. `PluginHost::init_all` advances every plugin from `Pending` to
//!    `Initialized` — no init-time interaction between substrates.
//! 3. With every required resource staged in the same `PluginContext`,
//!    `PluginHost::tick_all` ticks all four plugins in BTreeMap-key order
//!    without any plugin failing on a missing resource (i.e. no resource
//!    type collision between substrates that would cause one canary to
//!    accidentally consume another's resource).
//! 4. After the tick, every staged resource is back in the context (the
//!    put-back invariant generalises to a 4-substrate scene).
//! 5. `PluginHost::shutdown_all` LIFO-shuts every plugin without error.
//!
//! # GPU-free graceful skip
//!
//! `GfxPlugin` requires a wgpu adapter for its `HeadlessTarget` resource. On
//! a CI runner without a GPU, the existing canary pattern is to print a
//! `SKIP` message and return cleanly (`gfx::tests::ctx_or_skip`). This file
//! follows the same convention so the integration test never spuriously
//! fails on headless workers — a missing GPU is reported as a deliberate
//! skip, not a failure.

use kira::backend::mock::{MockBackend, MockBackendSettings};
use kira::AudioManagerSettings;
use rge_audio::components::Entity as AudioEntity;
use rge_audio::{
    AudioFrame, AudioManager, AudioPlugin, AudioSource, OwnedAudioSchedule, PlaybackState,
    AUDIO_PLUGIN_ID,
};
use rge_cad_core::{CadGraph, CuboidOp, OperatorNode, Tolerance};
use rge_cad_projection::{
    BRepHandle, CadProjection, CadProjectionPlugin, CAD_PROJECTION_PLUGIN_ID,
};
use rge_gfx::{GfxContext, GfxContextError, GfxPlugin, HeadlessTarget, GFX_PLUGIN_ID};
use rge_kernel_diagnostics::DiagnosticAggregator;
use rge_kernel_ecs::World as EcsWorld;
use rge_kernel_plugin_host::{PluginContext, PluginHost, PluginId, PluginState};
use rge_physics::physics_input_ledger::PhysicsInputLedger;
use rge_physics::stubs::components_physics::{BodyKind, Collider, ColliderShape, RigidBody};
use rge_physics::{PhysicsPlugin, World as PhysicsWorld, PHYSICS_PLUGIN_ID};

// ---------------------------------------------------------------------------
// Helpers — one factory per substrate. Each factory mirrors the smallest
// "non-trivial scene" the corresponding canary's solo smoke test uses, so
// that any composition bug surfaces in this test the way it would surface in
// the per-canary suites.
// ---------------------------------------------------------------------------

fn tol() -> Tolerance {
    Tolerance::new(0.001).expect("tolerance")
}

/// CAD substrate: a fresh [`CadGraph`] with one committed [`CuboidOp`] root
/// node — minimal but non-trivial so the projection tick has work to do.
fn make_cad_graph() -> (CadGraph, rge_kernel_graph_foundation::NodeId) {
    let mut cad = CadGraph::new();
    cad.begin_operation().expect("begin");
    let node = cad
        .graph_mut()
        .expect("mut")
        .add_operator(OperatorNode::Cuboid(CuboidOp {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
        }))
        .expect("add cuboid");
    cad.graph_mut()
        .expect("mut")
        .set_root(node)
        .expect("set root");
    cad.commit("multi-canary integration cuboid")
        .expect("commit");
    (cad, node)
}

/// Physics substrate: one fixed ground plane + one dynamic cube above it +
/// a fresh [`PhysicsInputLedger`]. Mirrors `physics::tests::make_scene_world`.
fn make_physics_scene() -> (PhysicsWorld, PhysicsInputLedger) {
    let mut world = PhysicsWorld::new();
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
    let _cube = world.insert_body(
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
    (world, PhysicsInputLedger::new())
}

/// Audio substrate: a headless mock-backend [`AudioManager`] at 48 kHz with
/// one listener + one playing source, plus a matching [`AudioFrame`].
/// Mirrors `audio::tests::make_scene_manager` + `make_scene_frame`.
fn make_audio_scene() -> (AudioManager<MockBackend>, AudioFrame) {
    let mut manager = AudioManager::<MockBackend>::with_settings(AudioManagerSettings {
        backend_settings: MockBackendSettings {
            sample_rate: 48_000,
        },
        ..Default::default()
    })
    .expect("mock backend always succeeds");

    let listener_entity = AudioEntity(1);
    let source_entity = AudioEntity(2);
    let listener_xform = rge_audio::Transform::default();
    let source_xform = rge_audio::Transform::from_position([0.0, 0.0, -2.0]);

    manager
        .register_listener(listener_entity, &listener_xform)
        .expect("register listener");

    let samples = rge_audio::waveform::sine_wave(440.0, 48_000, 0.05);
    manager.register_clip_from_samples("ping", 48_000, &samples);

    let source = AudioSource {
        clip: "ping".into(),
        desired_state: PlaybackState::Playing,
        distances: (1.0, 100.0),
        ..AudioSource::default()
    };
    manager
        .register_source(source_entity, &source_xform, &source)
        .expect("register source");

    let frame = AudioFrame {
        sources: vec![OwnedAudioSchedule {
            entity: source_entity,
            transform: source_xform,
            source: AudioSource {
                clip: "ping".into(),
                desired_state: PlaybackState::Playing,
                distances: (1.0, 100.0),
                ..AudioSource::default()
            },
        }],
        listener: Some((listener_entity, rge_audio::Transform::default())),
        records: Vec::new(),
    };

    (manager, frame)
}

/// Gfx substrate: an [`Option`] so the test can `SKIP` cleanly on a runner
/// without a GPU adapter. Mirrors `gfx::tests::ctx_or_skip` + the canary's
/// 32x32 [`HeadlessTarget`].
fn try_make_gfx_scene() -> Option<(GfxContext, HeadlessTarget)> {
    match GfxContext::new_headless() {
        Ok(gfx_ctx) => {
            let target = HeadlessTarget::new(&gfx_ctx, 32, 32).expect("target");
            Some((gfx_ctx, target))
        }
        Err(GfxContextError::NoAdapter) => {
            eprintln!(
                "SKIP (no GPU adapter): multi_canary_integration_smoke skipped — \
                 GfxContext init returned NoAdapter"
            );
            None
        }
        Err(e) => panic!("unexpected GfxContext init error: {e}"),
    }
}

// ---------------------------------------------------------------------------
// The smoke test — register all four canary plugins, stage all nine
// resources, run init/tick/shutdown, assert nothing collided.
//
// "Nine resources" comes from:
//   • cad-projection: World, CadGraph, Tolerance        (3)
//   • gfx:            GfxContext, HeadlessTarget        (2)
//   • physics:        PhysicsWorld, PhysicsInputLedger  (2)
//   • audio:          AudioManager<MockBackend>, AudioFrame  (2)
//
// Note that "World" (kernel/ecs) and "PhysicsWorld" (physics::World) are
// distinct types — `PluginContext` keys by `TypeId`, so each gets its own
// slot in the resource registry. This is precisely the kind of multi-
// substrate composition that this test is designed to surface bugs in: if
// either substrate accidentally reused the wrong World type in its
// `tick`-side `take`, the dispatch from one canary's tick would consume the
// other canary's resource and the second canary would fail with a
// `ContractViolation` for the shared type.
// ---------------------------------------------------------------------------

/// Multi-canary composition smoke: register cad-projection + gfx + physics +
/// audio plugins together; stage all nine resources in one
/// [`PluginContext`]; init/tick/shutdown the whole quartet through a single
/// [`PluginHost`] without any inter-canary interference.
#[test]
fn multi_canary_integration_smoke() {
    // GPU-presence gate up-front so the rest of the test reads linearly.
    let Some((gfx_ctx, gfx_target)) = try_make_gfx_scene() else {
        return;
    };

    // ---- Build all four substrates in parallel (logical, not threaded). ----
    let mut ecs_world = EcsWorld::new();
    ecs_world.register_snapshot_component::<BRepHandle>();

    let (cad, cad_node) = make_cad_graph();
    let mut projection = CadProjection::new();
    let cad_entity = projection
        .spawn_brep_entity(&mut ecs_world, cad_node)
        .expect("spawn brep entity");

    let (physics_world, physics_ledger) = make_physics_scene();
    let (audio_manager, audio_frame) = make_audio_scene();

    // ---- Register all four canary plugins under their canonical ids. ----
    let cad_id = PluginId::new(CAD_PROJECTION_PLUGIN_ID);
    let gfx_id = PluginId::new(GFX_PLUGIN_ID);
    let physics_id = PluginId::new(PHYSICS_PLUGIN_ID);
    let audio_id = PluginId::new(AUDIO_PLUGIN_ID);

    let mut host = PluginHost::new();
    host.register(
        cad_id.clone(),
        Box::new(CadProjectionPlugin::from_projection(projection)),
    )
    .expect("register cad-projection plugin");
    host.register(gfx_id.clone(), Box::new(GfxPlugin::new()))
        .expect("register gfx plugin");
    host.register(physics_id.clone(), Box::new(PhysicsPlugin::new()))
        .expect("register physics plugin");
    host.register(audio_id.clone(), Box::new(AudioPlugin::new()))
        .expect("register audio plugin");

    // host.iter_ids() is BTreeMap-ordered (plugin_host::host.rs:594) — the
    // four canonical ids must all be present.
    let ids: Vec<&PluginId> = host.iter_ids().collect();
    assert_eq!(ids.len(), 4, "four plugins registered, got {}", ids.len());
    let id_strs: Vec<&str> = ids.iter().map(|id| id.as_str()).collect();
    assert!(id_strs.contains(&CAD_PROJECTION_PLUGIN_ID));
    assert!(id_strs.contains(&GFX_PLUGIN_ID));
    assert!(id_strs.contains(&PHYSICS_PLUGIN_ID));
    assert!(id_strs.contains(&AUDIO_PLUGIN_ID));

    for id in [&cad_id, &gfx_id, &physics_id, &audio_id] {
        assert_eq!(
            host.state(id),
            Some(PluginState::Pending),
            "all plugins start Pending; got {:?} for {}",
            host.state(id),
            id.as_str(),
        );
    }

    // ---- init_all: every canary must transition Pending → Initialized. ----
    let mut diags = DiagnosticAggregator::new();
    let mut ctx = PluginContext::new(&mut diags);

    let init_report = host.init_all(&mut ctx);
    assert_eq!(
        init_report.initialized.len(),
        4,
        "all four plugins must init OK; got initialized={:?}, failed={:?}",
        init_report.initialized,
        init_report.failed,
    );
    assert!(
        init_report.failed.is_empty(),
        "init must not fail; got {:?}",
        init_report.failed,
    );
    for id in [&cad_id, &gfx_id, &physics_id, &audio_id] {
        assert_eq!(
            host.state(id),
            Some(PluginState::Initialized),
            "expected Initialized for {}, got {:?}",
            id.as_str(),
            host.state(id),
        );
    }

    // ---- Stage every resource the four canaries collectively need. ----
    // Order intentionally interleaved across substrates so a missing-type
    // bug would not be masked by happen-to-be-correct insertion ordering.
    assert!(
        ctx.insert(ecs_world).is_none(),
        "no prior World in ctx — first insert must return None",
    );
    assert!(
        ctx.insert(physics_world).is_none(),
        "no prior PhysicsWorld in ctx — types differ from EcsWorld",
    );
    assert!(ctx.insert(cad).is_none(), "no prior CadGraph in ctx");
    assert!(ctx.insert(gfx_ctx).is_none(), "no prior GfxContext in ctx");
    assert!(
        ctx.insert(physics_ledger).is_none(),
        "no prior PhysicsInputLedger in ctx",
    );
    assert!(ctx.insert(tol()).is_none(), "no prior Tolerance in ctx");
    assert!(
        ctx.insert(gfx_target).is_none(),
        "no prior HeadlessTarget in ctx",
    );
    assert!(
        ctx.insert(audio_manager).is_none(),
        "no prior AudioManager in ctx",
    );
    assert!(
        ctx.insert(audio_frame).is_none(),
        "no prior AudioFrame in ctx"
    );
    assert_eq!(
        ctx.resource_count(),
        9,
        "exactly nine staged resources before tick_all",
    );

    // ---- tick_all: every canary must extract its own resources, do its
    // work, and put them back without disturbing any sibling's slot. ----
    let tick_report = host.tick_all(&mut ctx);
    assert_eq!(
        tick_report.ticked, 4,
        "all four canaries must tick OK in one batch; failed={:?}",
        tick_report.failed,
    );
    assert!(
        tick_report.failed.is_empty(),
        "no canary may fail when every required resource is staged; \
         this assertion fires if a real composition bug exists \
         (e.g. resource type collision) — got {:?}",
        tick_report.failed,
    );
    for id in [&cad_id, &gfx_id, &physics_id, &audio_id] {
        assert_eq!(
            host.state(id),
            Some(PluginState::Initialized),
            "post-tick state must remain Initialized for {}, got {:?}",
            id.as_str(),
            host.state(id),
        );
    }

    // ---- Post-tick invariants: every staged resource is back in ctx. ----
    // The put-back invariant is per-canary in their solo smoke tests; here
    // we prove it composes — a missing resource on this list is a real
    // composition bug surfaced by this test (probably a panic during one
    // canary's tick that drops a resource, which is a CRITICAL finding
    // because catch_unwind is supposed to repackage it as Failed not as a
    // dropped resource).
    assert!(
        ctx.contains::<EcsWorld>(),
        "EcsWorld (cad-projection) must be put back",
    );
    assert!(
        ctx.contains::<CadGraph>(),
        "CadGraph (cad-projection) must be put back",
    );
    assert!(
        ctx.contains::<Tolerance>(),
        "Tolerance (cad-projection) must be put back",
    );
    assert!(
        ctx.contains::<GfxContext>(),
        "GfxContext (gfx) must be put back",
    );
    assert!(
        ctx.contains::<HeadlessTarget>(),
        "HeadlessTarget (gfx) must be put back",
    );
    assert!(
        ctx.contains::<PhysicsWorld>(),
        "PhysicsWorld (physics) must be put back",
    );
    assert!(
        ctx.contains::<PhysicsInputLedger>(),
        "PhysicsInputLedger (physics) must be put back",
    );
    assert!(
        ctx.contains::<AudioManager<MockBackend>>(),
        "AudioManager<MockBackend> (audio) must be put back",
    );
    assert!(
        ctx.contains::<AudioFrame>(),
        "AudioFrame (audio) must be put back",
    );
    assert_eq!(
        ctx.resource_count(),
        9,
        "exactly nine resources still in ctx after tick_all",
    );

    // ---- Cad-side post-tick proof of work: BRepHandle.mesh_id is now Some.
    // (This mirrors the per-canary smoke; here it doubles as proof that the
    // sibling canaries did not mangle the cad-projection's World.) ----
    {
        let world_ref = ctx
            .get_mut::<EcsWorld>()
            .expect("EcsWorld present after tick");
        let er = world_ref.entity(cad_entity).expect("cad entity preserved");
        let handle = er.get::<BRepHandle>().expect("brep handle present");
        assert!(
            handle.mesh_id.is_some(),
            "BRepHandle.mesh_id must be Some after tick — proves \
             cad-projection ran without sibling-canary interference",
        );
    }

    // ---- Physics-side post-tick proof of work. ----
    {
        let physics_ref = ctx
            .get_mut::<PhysicsWorld>()
            .expect("PhysicsWorld present after tick");
        assert_eq!(
            physics_ref.tick, 1,
            "physics world tick counter must advance by exactly 1",
        );
    }

    // ---- Audio-side post-tick proof of work. ----
    {
        let frame_ref = ctx
            .get_mut::<AudioFrame>()
            .expect("AudioFrame present after tick");
        assert_eq!(
            frame_ref.records.len(),
            1,
            "audio frame must have exactly one record after one tick",
        );
    }

    // ---- shutdown_all: LIFO sequence; no canary may report a failure. ----
    let shutdown_report = host.shutdown_all(&mut ctx);
    assert_eq!(
        shutdown_report.shutdown.len(),
        4,
        "all four plugins must shut down; got shutdown={:?}, failed={:?}",
        shutdown_report.shutdown,
        shutdown_report.failed,
    );
    assert!(
        shutdown_report.failed.is_empty(),
        "no canary may fail to shut down; got {:?}",
        shutdown_report.failed,
    );
    assert_eq!(host.count(), 0, "host empty after shutdown_all");
}
