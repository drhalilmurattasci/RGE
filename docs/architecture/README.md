# RGE Architecture — Doctrine Tier

This directory holds **Doctrine-tier** docs: binding architectural rules and
invariants that govern subsystem design across the workspace. Distinct from
the operational-contract `docs/§18/` companion-doc tier, the architectural-
decision `docs/adr/` tier, and the roadmap `plans/PLAN.md` tier.

## Authority hierarchy

Per the [2026-05-10 cross-review #1 archive in `change.md`](../../change.md),
workspace docs operate at five authority tiers (descending precedence; lower
tiers cannot override higher tiers, but higher tiers may defer to lower tiers
via explicit "defer to" cross-refs):

| Tier | Path | Authority | Stability |
|---|---|---|---|
| **Doctrine** | [`docs/architecture/`](.) | Invariant law (binding architectural rules) | Stable; changes require ADR amendment |
| **ADR** | [`docs/adr/`](../adr) | Architectural decisions + rejected alternatives | Stable; superseded entries flagged in-place |
| **Spec / Companion** | [`docs/§18/`](../§18) | Substrate operational contracts | Tracks source-truth (updated as substrate evolves) |
| **PLAN** | [`plans/PLAN.md`](../../plans/PLAN.md) | Roadmap (frozen at v0.8) | Frozen; v0.9+ requires §0.6 gate |
| **STATUS** | [`Status.md`](../../Status.md) / [`HANDOFF.md`](../../HANDOFF.md) / [`change.md`](../../change.md) | Implementation truth (live snapshot + continuation + audit trail) | Updated per dispatch |

`RFC` (experimental proposals) is reserved for post-v1.0 use; unused at v0.8.

## Doctrine docs (6 of 6 landed)

- **[`REACTIVE_INVALIDATION.md`](./REACTIVE_INVALIDATION.md)** (254L) — 4-layer
  invalidation hierarchy (graph mutations → topology evolution → tessellation
  rebuilds → GPU uploads); 4 invariants (explicit / revision-aware /
  observable / deterministic); 4 critical constraints. Companion to PLAN
  §1.6 + §1.5.4.3 + ADR-115.
- **[`SCENE_EXTRACTION_CONTRACT.md`](./SCENE_EXTRACTION_CONTRACT.md)** (252L)
  — canonical pipeline (CAD Graph → Topology → Geometry Eval → Tessellation
  → Scene Extraction → GPU Resources); ownership rules ("renderer NEVER owns
  authoritative geometry"); 4 extraction principles (pure / incremental /
  deterministic / replaceable). Companion to PLAN §1.5.2 + ADR-114.
- **[`NON_GOALS.md`](./NON_GOALS.md)** (138L) — 8 explicit non-goals (NOT
  general-purpose game engine / NOT competing CAD authoring tool v1 / NOT
  distributed runtime today / NOT AI-native CAD / NOT procedural-content-
  generation platform / NOT simulation platform v1 / NOT marketplace / NOT
  Bevy-or-Fyrox fork); anti-sprawl criteria; 8 explicit deferrals.
  Companion to PLAN §0.2 + §0.6 + 2026-05-10 cross-review #1.
- **[`INVARIANT_ENFORCEMENT_STRATEGY.md`](./INVARIANT_ENFORCEMENT_STRATEGY.md)**
  (263L) — Phase 1 Executable Governance Architecture first deliverable;
  6-term vocabulary table separating invariant / enforcement / escalation /
  authority surface / verification surface / advisory guidance (importance
  ≠ executability); 7-class invariant taxonomy + 5-tier enforcement
  taxonomy; conservative graduation criteria (ALL-of-four, walk-every-cost
  veto); load-bearing 7-cost catalog (rigidity / compatibility burden /
  false authority / test substrate ossification / enforcement coupling /
  abstraction pressure / future evolution cost); 7 system-level failure
  modes of over-mechanization (ossified architecture / false confidence /
  enforcement gaming / test-substrate coupling / framework gravity /
  accidental public contracts / inability to evolve semantics safely);
  6 prose-only doctrines; 7 anti-patterns; 5 round-9 deferred items framed
  as classification probes (not backlog) plus 3 workspace-state probes
  (warning-tier as legitimate steady state / exemption philosophy / rollout
  debt as transitional equilibrium). Companion to ARCHITECTURE_LINTS.md +
  ADR-115 phase-2.5 amendment + ADR-116.
- **[`ARCHITECTURAL_TEST_TAXONOMY.md`](./ARCHITECTURAL_TEST_TAXONOMY.md)**
  (284L) — Phase 1 Executable Governance Architecture second deliverable;
  subdivides sub-1's tests-tier into shape-aware classification.
  6-term vocabulary distinguishing architectural / semantic / fixture /
  regression / smoke / canary tests (importance ≠ test coverage);
  7-class test-class taxonomy + 5-shape test-shape taxonomy ordered by
  ossification cost; ALL-of-four graduation criteria with walk-every-cost
  veto inheriting sub-1's discipline; 7-cost catalog (substrate
  ossification / maintenance debt / false authority / implementation
  coupling / refactor friction / discoverability rot / test-substrate
  gravity); 7 system-level failure modes of over-testing (substrate-
  becomes-architecture / false confidence / flaky-triage / gravity-at-
  scale / test-as-spec / gating overuse / inability to evolve substrate);
  6 prose-only doctrine categories; 7 anti-patterns (universal harness /
  meta-tests / registries / TDD / DSLs / coverage thresholds / separate
  architectural-test directory); 8 workspace probes anchoring class ×
  shape choices already made (architecture-lint fixtures / 50-iter PIE
  soak / 5 plugin canaries / PluginError × PluginPhase 4-cell / ADR-116
  retroactive harness / host_tests sub-module split / failure-class
  declaration walker / non_exhaustive SemVer hardening). Companion to
  INVARIANT_ENFORCEMENT_STRATEGY.md + ARCHITECTURE_LINTS.md + ADR-116.
- **[`SEMANTIC_ARCHITECTURE_LAWS.md`](./SEMANTIC_ARCHITECTURE_LAWS.md)**
  (doctrine-tier v0) — consolidated semantic law set imported from the
  user-provided semantic architecture fragments: semantic authority,
  mutation, projection, identity continuity, replay, propagation, drift
  detection, cascade preview, the semantic constitution, semantic-runtime
  improvement direction, near-theoretical maturity horizon, and qualitative
  semantic-runtime gap assessment. The doc
  operationalizes the law set without creating a new semantic-runtime crate,
  lint, ADR, or global datastore; cascade preview is explicitly prose-only
  until a concrete preview substrate lands. Companion to REACTIVE_INVALIDATION,
  SCENE_EXTRACTION_CONTRACT, PIE_SNAPSHOT, EDITOR_ACTIONS_COMMAND_BUS,
  CAD_TOPOLOGY_LINEAGE, and CAD_CORE_MODEL.

## Parked and historical design notes

Short design notes that capture an open architectural question,
record candidate dispositions, and explicitly state what would
trigger the question becoming an active dispatch or ADR. These are
**NOT** doctrine docs (no binding rule) and **NOT** ADRs (no
decision by themselves). They live here to keep open questions and
their supersession trail visible without pretending the note itself is
the decision record.

- **[`FILLET_OUTPUT_IDENTITY.md`](./FILLET_OUTPUT_IDENTITY.md)** —
  historical note for chamfer `FilletOp` output identity. ADR-120
  unparked tessellation face-label propagation for cad-projection:
  inherited upstream triangles keep their labels and chamfer caps are
  `TopologyFaceId::DEGENERATE`. Stable B-Rep IDs for chamfer caps
  remain deferred until a cap-face consumer appears.
- **[`RGE_Multi_Agent_Orchestration.md`](./RGE_Multi_Agent_Orchestration.md)** —
  codified workflow protocol for multi-agent orchestration: the
  Decision / Orchestrator / Execution role split, `NEXT_ACTION` labels,
  the bounded execution contract template (TASK / COMMAND BUDGET /
  FORBIDDEN / STOP IF / OUTPUT), the auto-execute rule for read-only
  bounded tasks, and the execution-continuation rule. Manual markdown
  protocol — no orchestration code or runtime enforcement.

## How to navigate

- Reading the **architecture's invariants**? Start here. Each doctrine doc
  states load-bearing rules + cites the substrate refs that realize them.
- Reading **why a specific design was chosen**? Read the corresponding ADR
  in [`docs/adr/`](../adr). **11 ADR files landed**: 097 (cad-projection
  split — applied and backfilled in #78) / 098 / 104 / 112 / 114 / 115 (with
  2026-05-10 amendment) / 116 / 117 (render-handoff mechanism for Gate C) /
  118 (frame-graph transient-resource allocator policy) / 119 / 120.
  **1 accepted-deferred ADR file not yet authored**: 113-deferred (truck
  cad-native backend). **3 deferred per §18 doctrine**: 099 / 101 / 102.
- Reading **how a substrate works today**? Read the corresponding §18 doc
  in [`docs/§18/`](../§18). 27 of 27 companion docs landed (cumulative LoC
  ~7,700+).
- Reading **the roadmap or phase order**? Read
  [`plans/PLAN.md`](../../plans/PLAN.md) +
  [`plans/IMPLEMENTATION.md`](../../plans/IMPLEMENTATION.md).
- Reading **what's true right now**? Read [`Status.md`](../../Status.md)
  for the live snapshot, [`HANDOFF.md`](../../HANDOFF.md) for the next-
  session continuation pointer, [`change.md`](../../change.md) for the
  full session-by-session audit trail.

## Subsystem maturity (post-2026-05-10 cross-review #1 framing)

| Subsystem | Maturity (2026-05-10) |
|---|---|
| ECS / runtime core | Strong experimental |
| Plugin substrate | Early production-grade direction |
| CAD operator architecture | Advanced prototype (5 operators + topology lineage prototype) |
| Deterministic infrastructure | Exceptionally strong for stage (1000-tick replay byte-identical; 3-way PIE composition byte-identical) |
| Reflection / tooling | Incomplete (kernel/types substrate done; downstream consumers limited) |
| Editor architecture | Pre-stabilization (editor-state coordination layer landed; full editor app not yet) |
| Fault isolation | Partially mature (5-class taxonomy + plugin-fatal isolation + H3 fault-injection harness) |
| GPU abstraction | Early (gfx canary running; render-snapshot separation pending Phase 6) |
| Multi-user / distributed future readiness | Surprisingly strong (PIE byte-identity is foundational) |
| Kernel-grade topology infrastructure | Emerging (lineage substrate prototype; persistent IDs deferred) |

## Adding a new doctrine doc

The doctrine tier is **deliberately small**. Per the cross-review's framing:
"DO NOT over-document speculative systems. Avoid giant docs for AI CAD,
cloud architecture, future networking, advanced distributed runtime,
commercial ecosystem plans. Those become stale quickly."

A new doctrine doc is justified when:
1. Multiple subsystems depend on the same architectural invariant
2. The invariant is currently implicit (encoded in code; not documented)
3. Drift between implementations would be catastrophic
4. The decision can be expressed as a binding rule (not a sketch or roadmap)

Otherwise prefer: an ADR (decision), a §18 spec doc (substrate contract),
or a PLAN.md amendment (roadmap).
