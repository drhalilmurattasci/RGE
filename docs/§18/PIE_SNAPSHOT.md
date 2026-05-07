# PIE_SNAPSHOT

| Companion to | PLAN.md §6.13 (`SnapshotParticipate` substrate) + PLAN.md §13.2 (cross-architecture coherence quality gate) |
|---|---|
| Status | Stable v0 — substrate shipped pre-2026-05-07; load-bearing as of 2026-05-08 with three real participants live (`cad-core.cad-graph` + `cad-projection.brep-handles` + `physics.rapier-rigid-bodies`) |
| Audience | Subsystem authors needing capture / restore for Play-in-Editor; orchestrator authors composing PIE snapshots from many subsystems; future audio / particles / gfx / editor-actions / editor-state participants per PLAN §13.2 |
| Sibling doc | `CAD_CORE_MODEL.md` — `cad-core.cad-graph` participant implementer; `CAD_PROJECTION.md` — `cad-projection.brep-handles` participant implementer; both lean on this substrate |
| Reference impls | `kernel/ecs/src/participate.rs` (`SnapshotParticipate` trait + `PieSnapshot` aggregator + `RGEP` envelope) · `crates/cad-core/src/checkpoints/participate.rs` (`cad-core.cad-graph` impl) · `crates/cad-projection/src/lib.rs` (`cad-projection.brep-handles` impl) · `crates/physics/src/participate.rs` (`physics.rapier-rigid-bodies` impl) |

> Convention defined by `PLUGIN_HOST_PATTERNS.md` §header. Per PLAN §13.2 the substrate's quality gate is "all stateful Tier-2 has `SnapshotParticipate`" — this doc documents how to satisfy that gate.

## 1. What is PIE

Play-in-Editor (PIE) is the workflow where the editor captures a runtime snapshot, the user plays the simulation forward, and the editor restores the prior state when play ends. The snapshot must capture enough state that the post-restore world is byte-identically the pre-play world.

PLAN §6.13 + §13.2 frame the snapshot as the union of three layers:

- **Parameter** — operator graphs, kernel state. Participant: `cad-core.cad-graph`.
- **Identity** — entity-cad mapping, persistent IDs. Participant: `cad-projection.brep-handles`.
- **Entity-state** — ECS components. Layer: `World::serialize_snapshot` (registered via `World::register_snapshot_component<T>`).

Together these three layers form the **PIE snapshot**: one byte-blob that round-trips byte-identically through `to_bytes` / `from_bytes`. PLAN §13.2 commits to "all stateful Tier-2 has `SnapshotParticipate`" as a workspace quality gate; this substrate is the mechanism that makes that gate concrete.

## 2. The `SnapshotParticipate` trait

Lives at `kernel/ecs/src/participate.rs`. Subsystem-level capture / restore via opaque byte payload, identified by stable `ParticipantId`. The trait surface (3 methods, no defaults):

```rust
pub trait SnapshotParticipate {
    fn participant_id(&self) -> ParticipantId;
    fn capture(&self) -> Result<Vec<u8>, ParticipateError>;
    fn restore(&mut self, bytes: &[u8]) -> Result<(), ParticipateError>;
}
```

The `Vec<u8>` payload is intentionally opaque to the substrate — implementors choose their own serialization format (see §8 format policy). The substrate guarantees deterministic ordering of participants in the envelope (sorted by `ParticipantId` lexicographically); the participant guarantees its payload bytes themselves are stable (no randomness, no timestamps).

### Contract

Per the trait module-doc:

- `participant_id()` MUST return the same value on every call for a given instance.
- `capture()` followed by `restore()` on a fresh instance MUST produce a state byte-identical to the original.

Determinism is the **implementor's responsibility** — the envelope sorts; the payload's own byte-stability is up to the impl.

## 3. `ParticipantId` — stable subsystem identity

```rust
pub struct ParticipantId(pub String);

impl ParticipantId {
    pub fn new(s: impl Into<String>) -> Self;
    pub fn as_str(&self) -> &str;
}
```

String wrapper for cross-version identity stability. Convention per the type's module-doc: `"<crate-name>.<subsystem>"` or `"<subsystem>.<concrete-impl>"`. Examples:

- `cad-core.cad-graph` — the cad-graph participant.
- `cad-projection.brep-handles` — the cad-projection participant.
- (future) `audio.kira-mixer`, `physics.rapier-rigid-bodies`, etc.

`ParticipantId` is `Hash + Ord`, so it doubles as a `BTreeMap` key for the envelope's deterministic iteration. Must be unique within a `PieSnapshot`'s participant set; duplicates are rejected at capture time as `ParticipateError::DuplicateParticipant`.

The naming convention mirrors `kernel/plugin-host::PluginId` (`PLUGIN_API.md` §1) so a subsystem that ships both a `Plugin` adapter and a `SnapshotParticipate` impl uses parallel ids — `rge-cad-projection.brep-handles-plugin` (the plugin) and `cad-projection.brep-handles` (the participant).

## 4. `ParticipateError`

```rust
pub enum ParticipateError {
    World(SnapshotError),
    CaptureFailed { id: ParticipantId, message: String },
    RestoreFailed { id: ParticipantId, message: String },
    UnknownParticipant(ParticipantId),
    DuplicateParticipant(ParticipantId),
    Serde(String),
    BadMagic([u8; 4]),
    BadVersion(u16),
    Truncated(usize),
    Custom(String),
}
```

`World(SnapshotError)` wraps the `kernel/ecs` snapshot layer. `CaptureFailed` / `RestoreFailed` carry the participant's id + message for blame attribution. `UnknownParticipant` fires when the snapshot contains a payload for an id with no registered restore handler on the receiving side. `BadMagic` / `BadVersion` / `Truncated` are envelope-level deserialization errors. `Custom(String)` is the implementor-surfaced free-form variant — `cad-core.cad-graph` and `cad-projection.brep-handles` both wrap their serialization errors via `ParticipateError::Custom(e.to_string())`.

## 5. `PieSnapshot` aggregator

```rust
pub struct PieSnapshot {
    pub world_bytes: Vec<u8>,
    pub participants: BTreeMap<ParticipantId, Vec<u8>>,
}
```

The aggregate Play-in-Editor snapshot. `world_bytes` is produced by `World::serialize_snapshot` (the ECS-side `SnapshotComponent` registry layer). `participants` is a `BTreeMap` so iteration is deterministic in `ParticipantId` ascending order; `to_bytes` writes participants in that order, so identical logical state always produces byte-identical envelope output regardless of registration order.

### Wire format — the `RGEP` envelope

Per the trait module-doc at `kernel/ecs/src/participate.rs`:

```text
magic:             [u8; 4]   = b"RGEP"
version:           u16 LE    = 1
world_bytes_len:   u32 LE
world_bytes:       [u8; world_bytes_len]
participant_count: u32 LE
per participant (sorted by ParticipantId ascending):
  id_len:          u32 LE
  id_bytes:        [u8; id_len]   (UTF-8)
  payload_len:     u32 LE
  payload:         [u8; payload_len]
```

All integers are little-endian. `to_bytes` / `from_bytes` are the disk/wire transfer entry points. The deterministic ordering is the keystone: two captures of the same logical state produce byte-identical bytes (see the `to_bytes_is_deterministic_across_two_captures` regression test).

### Envelope determinism — three layers

Byte-identical output is delivered by three layered guarantees:

1. **Substrate-level ordering.** `BTreeMap` iteration is ascending by key; `ParticipantId` is `Ord` (lexicographic on the inner string). The substrate guarantees that a given logical participant set always serializes in the same order.
2. **Per-participant payload stability.** The participant's own `capture()` must produce stable bytes. RON serialization of `BTreeMap`-backed CAD types is deterministic because `BTreeMap` iterates in key order; postcard serialization is deterministic by construction (no field-ordering ambiguity, no key-set hashing).
3. **`world_bytes` stability.** The ECS-side `SnapshotComponent` registry serializes components in entity-id order (entity id is `Ord`) and component-type-id order. See `kernel/ecs/src/snapshot.rs` for the registry's iteration discipline.

Violating any of the three breaks byte-identity. The most common violation in implementer tests: using `HashMap` instead of `BTreeMap` in a participant payload type. The regression test `to_bytes_is_deterministic_across_two_captures` catches the substrate-level layer; participant-level stability is validated in implementer-side tests (e.g. `cad-core` capture/restore round-trip suite).

### Magic / version / truncation validation

`from_bytes` rejects malformed envelopes with structured errors:

- **`BadMagic([u8; 4])`** — first 4 bytes ≠ `b"RGEP"`. Catches the wrong-format-file case.
- **`BadVersion(u16)`** — version field ≠ 1. Reserved for future format upgrades; today the only valid version is 1.
- **`Truncated(usize)`** — byte stream ends before all declared fields are consumed. `usize` carries the offset where the stream ran out, useful for debugging.
- **`Serde(String)`** — a participant id field is not valid UTF-8.

The validation is layered: magic comes first (cheapest reject), then version, then per-field length-checked reads. The `from_bytes` implementation uses internal `read_bytes!` / `read_u16!` / `read_u32!` macros that bound-check before slicing, so OOB reads on a truncated buffer are impossible by construction.

## 6. `PieSnapshot::capture` — orchestrating capture

```rust
pub fn capture(
    world: &World,
    participants: &[&dyn SnapshotParticipate],
) -> Result<Self, ParticipateError>;
```

Step-by-step:

1. **Serialize world.** Calls `world.serialize_snapshot()` to produce `world_bytes`. Any `SnapshotError` propagates as `ParticipateError::World`.
2. **Iterate participants.** For each `&dyn SnapshotParticipate` in caller order: query its `participant_id()`; reject duplicate ids with `ParticipateError::DuplicateParticipant`; call `capture()` and wrap any error as `ParticipateError::CaptureFailed { id, message }`.
3. **Insert into BTreeMap.** The map sorts by `ParticipantId` automatically, so `to_bytes` iteration order is deterministic regardless of caller order.

The caller passes participants as `&[&dyn SnapshotParticipate]` — a slice of trait-object borrows, not owned. The aggregator does NOT take ownership of participants; it just calls `capture()` on each.

### Worked capture example

Capturing both currently-live participants for a CAD scene with operator graph + projection bookkeeping:

```rust
let cad: CadGraph = /* the scene's operator graph */;
let projection: CadProjection = /* the scene's ECS bridge */;
let world: World = /* the scene's ECS world */;

let snap = PieSnapshot::capture(
    &world,
    &[
        &cad as &dyn SnapshotParticipate,
        &projection as &dyn SnapshotParticipate,
    ],
)?;
let bytes = snap.to_bytes();   // → disk / network / replay buffer
```

The two participants register their own ids (`cad-core.cad-graph` and `cad-projection.brep-handles`) and serialize their own state — the orchestrator doesn't have to thread per-subsystem types through the substrate.

## 7. `PieSnapshot::restore` — orchestrating restore

```rust
pub fn restore(
    &self,
    world: &mut World,
    participants_by_id: &mut [(&ParticipantId, &mut dyn SnapshotParticipate)],
) -> Result<(), ParticipateError>;
```

Step-by-step:

1. **Restore world first.** Calls `world.restore_from_snapshot(&self.world_bytes)`. Any `SnapshotError` propagates as `ParticipateError::World`.
2. **Build id → handler lookup.** Converts the caller-supplied `participants_by_id` slice into a `BTreeMap<&ParticipantId, &mut dyn SnapshotParticipate>` for O(log n) lookup.
3. **Iterate snapshot's participant payloads.** For each `(id, payload)` in `self.participants` (BTreeMap iteration → ascending id order): look up the handler; call `restore(payload)` on it; wrap any error as `ParticipateError::RestoreFailed { id, message }`. Missing handlers produce `ParticipateError::UnknownParticipant`.

### Caller responsibility — pass the matching participant list

The substrate does NOT do automatic participant discovery. The caller is responsible for passing in the matching `&mut dyn SnapshotParticipate` set. A superset of handlers is fine — handlers for ids NOT in the snapshot are left untouched. A subset is rejected — ids in the snapshot but absent from the slice trigger `UnknownParticipant`.

This deliberate caller-driven design means the orchestrator decides recovery policy. For example, an editor that loads a PIE snapshot from disk knows which subsystems are alive; it can prepare their `&mut` borrows and pass them in. A test fixture loading a saved snapshot can substitute mock participants for the same ids.

### Worked restore example

Restoring the two currently-live participants from disk:

```rust
let snap = PieSnapshot::from_bytes(&bytes)?;

let mut cad: CadGraph = CadGraph::new();
let mut projection: CadProjection = CadProjection::new();
let mut world: World = World::new();

let cad_id = ParticipantId::new("cad-core.cad-graph");
let proj_id = ParticipantId::new("cad-projection.brep-handles");

snap.restore(
    &mut world,
    &mut [
        (&cad_id, &mut cad as &mut dyn SnapshotParticipate),
        (&proj_id, &mut projection as &mut dyn SnapshotParticipate),
    ],
)?;

// Post-restore handle validation per CAD_PROJECTION.md §7:
let orphans = projection.validate_handles(&cad);
if !orphans.is_empty() {
    // divergent-state PIE — orchestrator decides recovery
}
```

The restore is in-place: the existing `&mut CadGraph` and `&mut CadProjection` instances are mutated to the snapshot's state. After restore the orchestrator runs the cross-subsystem coherence check (§10).

## 8. Format policy — postcard default, RON exception

Per the `SnapshotParticipate` trait's module-level "Serialization-format policy" doc-comment at `kernel/ecs/src/participate.rs`: the workspace default is **postcard** (compact, fast, non-self-describing). Document the exception when an alternative format is required.

The current exception is `cad-core.cad-graph`: `OperatorNode` derives `#[serde(tag = "kind")]` for forward-compat across new variants, and postcard explicitly does NOT support internally-tagged enum deserialization (it's a non-self-describing format that needs the encoder and decoder to agree statically on the discriminant layout). RON is self-describing and round-trips the tagged enum cleanly. See `CAD_CORE_MODEL.md` §5 for the rationale.

Future participants should default to postcard and explicitly justify other format choices in the impl's module-level doc.

## 9. Current participants registry

| ParticipantId | Crate | Format | Purpose |
|---|---|---|---|
| `cad-core.cad-graph` | cad-core | RON | OperatorGraph + CheckpointHistory + in-progress operation (the full transactional model) |
| `cad-projection.brep-handles` | cad-projection | postcard | EntityCadMap + last_seen_checkpoint (entity↔node bridge bookkeeping) |
| `physics.rapier-rigid-bodies` | physics | postcard | Rapier `World` arena state — `RigidBodySet` + `ColliderSet` + `IslandManager` + broadphase + narrowphase + joint sets + `IntegrationParameters` + gravity + tick. `PhysicsPipeline` is reconstructed via `new()` per rapier's own "workspace data, no point in serializing" comment |

The three impls live alongside their substrate types — `cad-core.cad-graph` at `crates/cad-core/src/checkpoints/participate.rs`, `cad-projection.brep-handles` at the bottom of `crates/cad-projection/src/lib.rs`, `physics.rapier-rigid-bodies` at `crates/physics/src/participate.rs`. See the implementer-side §18 docs (`CAD_CORE_MODEL.md` §5, `CAD_PROJECTION.md` §8) for impl detail.

## 10. Divergent-restore tolerance

Restoring a `CadGraph` that doesn't include nodes referenced by current `BRepHandle` mappings must NOT panic. The two participants share state across the cad-core ↔ cad-projection layer boundary, and snapshot capture is two distinct calls — the cad-graph and the projection can in principle be captured at different times (or only one of the two may be captured). Without tolerance machinery, a restored projection holding `EntityCadMap` entries pointing at nodes the restored cad-graph doesn't have would fail later with `ProjectionError::NodeNotInGraph` on the next tick.

The substrate-level tolerance:

- The participant impls themselves do NOT cross-validate at restore time. `cad-projection`'s `restore` clean-slates the cache and re-marks every entity dirty; it does not inspect the (possibly mismatched) cad-graph.
- The orchestrator runs `CadProjection::validate_handles(&CadGraph) -> Vec<(EntityId, NodeId)>` (see `CAD_PROJECTION.md` §7) AFTER the restore. The method returns orphan handles whose cad-node references no longer resolve. An empty `Vec` means consistent state; non-empty means a divergent-state PIE payload — the orchestrator decides recovery (log diagnostic / re-project / error).

The `cad_graph_corruption_recovery` test fixture (referenced in HANDOFF.md / cad-projection test suites) exercises this tolerance end-to-end: capture both participants, mutate the cad-graph in a way that drops nodes referenced by handles, restore, validate. The expected outcome is a non-empty orphan list — recovery is the orchestrator's responsibility, not the substrate's.

## 11. Future participants

Per PLAN §13.2 the quality gate scales: every Tier-2 subsystem with stateful runtime gets a `SnapshotParticipate` impl as part of its first real implementation per IMPLEMENTATION.md phase order. Tracked in HANDOFF.md as those phases land.

Anticipated future participants:

- **gfx.render-snapshot** — render-tier separation per PLAN §1.5.2; lands alongside the future frame-graph + sim/render-thread split (Phase 6 deferred work per `GFX_RENDER_TIER.md` §11). Today's gfx state is single-threaded GPU resource state (wgpu device / queue / pipelines / buffers) reproducible from upstream scene state — there is no PIE-state to capture today; the participant is meaningful only once the §1.5.2 staged-snapshot pattern ships.
- **particles.simulation** — emitter state, particle pools (crate does not exist yet; lint pre-registers the bare name `particles` for forward-compatibility).
- **sculpt.brush-strokes** — sculpt-tool persistent brush state (crate does not exist yet; lint pre-registers the bare name `sculpt`).

Each subsystem's impl lands as part of its first concrete substrate dispatch, alongside its tests in the same crate. The `kernel/ecs::participate` substrate itself does not change — the substrate's surface is stable; growth happens in the participant set.

### 11.1 Crates audited and confirmed NOT-PIE-participants (post-2026-05-09 discriminate-vs-implement audit)

The H3 supplementary lint's heuristic over-classified four Tier-2 crates as "stateful Tier-2 expected to impl `SnapshotParticipate`". A per-crate audit against the inclusion criterion ("owns state that should round-trip with `PieSnapshot::capture` / `PieSnapshot::restore`") closed each as NOT-a-PIE-participant. The audit cites pre-existing PLAN / §18 doctrine for each:

| Crate | Removed from `STATEFUL_TIER2_CRATES` because |
|---|---|
| `audio` | RECOVERY_MODEL.md §9: "Audio state does NOT participate in PIE (it's transient: a paused-then-resumed editor restarts the audio mixer from scratch)". EXECUTION_DOMAINS.md §4: failure class `recoverable` *because* state is transient. PLAN §6.13's pre-v0.8 "required" list is superseded by the §18 doctrine. |
| `editor-actions` | PLAN §6.16.5: "Stop restores pre-play snapshot" — undo stack is NOT modified during Play. PLAN §13.7's "history serialized + restored" is project-file persistence, not PIE round-trip. The `Action` trait is `dyn Action` with no `Serialize` / `Deserialize`; PIE round-trip would require typetag substrate that does not exist in v0.8. Snapshot-recoverable-class subsystems use the bus to *recover from* failures, they aren't *captured* by PIE. |
| `editor-state` | PLAN §1.15 line 674 explicit: "PIE (§6.13) \| Editor-state persists across Play/Stop (selection survives, tool persists); does NOT participate in `WorldSnapshot`". EDITOR_STATE_MODEL.md §10: "no PIE-state at this level". Coordination state (Selection / Hover / ActiveTool) is session-scoped UI bookkeeping referenced via IDs/handles. |
| `gfx` | PLAN §1.5.2 + GFX_RENDER_TIER.md §11: today's gfx is single-threaded headless rendering with no §1.5.2 sim/render-thread split; the `gfx.render-snapshot` participant is "Pending Phase 6 work" alongside the future frame-graph. Today's gfx owns only GPU resource state (wgpu non-`Send`-serializable handles) reproducible from upstream scene state on next render. The future entry above tracks this. |

Re-add a crate to `STATEFUL_TIER2_CRATES` only when it ships a concrete impl that satisfies the inclusion criterion — at which point both the lint update and the crate's first impl land in the same dispatch (mirroring the cad-core + cad-projection + physics dispatch pattern).

## 12. Failure class

`kernel/ecs` declares `//! Failure class: recoverable` (substrate-level — the substrate itself doesn't fail catastrophically). Participant-side failures (`CaptureFailed` / `RestoreFailed` / `Custom`) are recoverable per-subsystem — the caller can decide whether to skip the failed participant, retry, or surface the error to the user.

The substrate guarantees: a failing capture / restore call returns `Err` rather than panicking, leaks no resources, leaves the world in a defined state (capture is read-only on world; restore writes world first then iterates participants — a participant restore failure leaves the world in its pre-restore state from this call only).

The `architecture-lints` `failure-class` lint enforces the `kernel/ecs` declaration; the substrate does not appear in the failure-class exemptions table.

## 13. References

- **PLAN.md §6.13** — `SnapshotParticipate` substrate definition.
- **PLAN.md §13.2** — cross-architecture coherence quality gate ("all stateful Tier-2 has `SnapshotParticipate`").
- **PLAN.md §1.13** — failure-class taxonomy.
- **PLAN.md §1.5.2** — render-snapshot separation (informs the future `gfx.render-snapshot` participant).
- **`CAD_CORE_MODEL.md`** — sibling §18 doc; the `cad-core.cad-graph` participant implementer.
- **`CAD_PROJECTION.md`** — sibling §18 doc; the `cad-projection.brep-handles` participant implementer.
- **CRITICAL #1 audit closure** (HANDOFF.md, 2026-05-07) — `CadGraph` `SnapshotParticipate` landing closes the silent-inconsistency window.
- **`kernel/ecs/src/participate.rs`** — `SnapshotParticipate` trait + `PieSnapshot` + `RGEP` envelope + `ParticipateError` + `ParticipantId`.
- **`kernel/ecs/src/snapshot.rs`** — `World::serialize_snapshot` / `World::restore_from_snapshot` / `SnapshotComponent` registry (the `world_bytes` layer).
- **`crates/cad-core/src/checkpoints/participate.rs`** — `cad-core.cad-graph` impl (RON exception).
- **`crates/cad-projection/src/lib.rs`** — `cad-projection.brep-handles` impl (postcard default).
