# ADR-115: Graph-metrics substrate design

| Status | Accepted 2026-05-10 (binding architectural design; implementation deferred to phase-1 Tier-A counters dispatch) |
|---|---|
| Date | 2026-05-10 |
| Deciders | (RGE architecture review; informed by 2026-05-10 ChatGPT cross-review #2 archived in `change.md`) |
| PLAN references | §1.10.4 (architecture entropy metrics — current placement rationale), §1.14 (graph-foundation substrate — primary upstream consumer), §13.10 (entropy gates — current mechanical dependency), §13.14 (graph-foundation gates — substrate-discipline parent), §1.13 (failure containment), §13.2 (snapshot integration) |
| ADR references | ADR-098 (topology lineage substrate — topology-volatility metric consumer), ADR-104 (capability surface — doc-comment-canonical pattern as deferral precedent), ADR-114 (PluginContext owned-handoff — plugin-host integration peer; same cross-review provenance) |
| Implementation phase | Tier-1 kernel substrate (recommended `kernel/graph-metrics`); precondition for editor reactive overlays + rebuild scheduling + future probabilistic recovery |

## Context

Today's `tools/graph-metrics` is a 5-line stub binary at `tools/graph-metrics/src/main.rs`:

```rust
//! `rge-tool-graph-metrics` — stub binary.

fn main() {
    println!("rge-tool-graph-metrics: stub. Implementation pending per IMPLEMENTATION.md.");
}
```

Its `Cargo.toml` describes it as an "Architecture entropy metrics tracker (per PLAN.md §1.10.4)." That framing is **narrow**: PLAN §1.10.4 is the workspace's architecture-entropy table — cross-crate dep edges, ECS archetype counts, async boundary counts, plugin ABI churn, incremental invalidation radius, etc. — workspace-property metrics tracked at minor version bumps and surfaced via the §13.10 entropy gates. None of the §1.10.4 metrics are runtime properties of an in-memory graph.

The 2026-05-10 ChatGPT cross-review #2 (archived in `change.md` as the second 2026-05-10 entry, paired with the cross-review #1 that drove ADR-114 amendment work) reframes the C1 graph-metrics dispatch from "implement the §1.10.4 entropy tracker" to **"the kernel's semantic-introspection nervous system"** — a runtime substrate that observes node/edge/operator/constraint/invalidation counts on every graph the workspace owns (the cad-core operator graph, the kernel/asset dependency graph, the lineage graph from ADR-098, future material/anim/script editor graphs, the future asset-store dep graph, etc.) and exposes them to consumers including the editor's overlays, the rebuild scheduler, future probabilistic recovery, distributed-CAD synchronization, and any AI-assisted CAD repair surface that needs a confidence signal. The cross-review's framing is not an extension of §1.10.4; it is a **scope reframing**, surfaced explicitly so this ADR doesn't paper over the divergence: the §1.10.4 entropy tracker remains a real downstream consumer, but the substrate the cross-review describes is broader and lives at a different layer.

The cross-review carries a "**catastrophic later**" warning: ad-hoc full-graph-scan metrics computed synchronously inside hot mutation paths cause rebuild latency to explode, reactive pipelines to stall, editor responsiveness to die, and distributed synchronization to break. The remediation is event-sourced + snapshot-versioned + tier-stratified, mechanically aligned with the kernel/audit-ledger event-sourcing precedent (`Event` + `EventId(BLAKE3)` + monotonic sequence) and with the ADR-104 doc-comment-canonical deferral pattern. Without the binding architectural decisions captured here before the phase-1 dispatch fires, the implementation will accrete the catastrophic shape and remediation later will require a substrate rewrite. ADR-115 is the architectural commitment that prevents the rewrite.

This ADR is **pure-docs** (no source-code change in this dispatch). The 5-line stub stays in place; the phase-1 implementation dispatch will reference this ADR's decisions verbatim.

## Decision

**The graph-metrics subsystem is a kernel-tier substrate (recommended path: `kernel/graph-metrics`) layered on `kernel/graph-foundation`. Metrics MUST be tier-stratified (A / B / C), MUST carry a `(graph_revision, metrics_revision)` snapshot version pair, MUST update via an event-sourced `GraphEvent` stream consumed in transactional alignment with the underlying `Graph<N, E>` mutation, and MUST be split across `core / structural / topology / invalidation / recovery / analytics` sub-modules. Ad-hoc full-graph scans for metric computation inside hot mutation paths are forbidden.**

Five sub-decisions follow.

### Sub-decision 1 — substrate placement: kernel-level, not tools-level

The current `tools/graph-metrics` binary placement reflects the §1.10.4 framing: the entropy tracker is a CI-time tool that walks the workspace, not a runtime substrate. Under the cross-review's reframing, that placement is **wrong**: a runtime substrate that observes every graph the workspace owns CANNOT live in a binary crate that has no link path back into the kernel + Tier-2 crates that produce the events. Moving to a Tier-1 library crate is structurally required.

Two placement candidates were considered:

- **Keep `tools/graph-metrics` and reach back into kernel crates from the binary.** Rejected: the architecture-lints' `kernel-isolation` + `forbidden-dep` checks reject a `tools/*` binary depending on `kernel/*` for runtime-substrate purposes; the binary-tier is for CI tools (architecture-lints, dependency-auditor, schema-diff) that consume the workspace as a static artifact, not for runtime infrastructure. Doing this would require either an exemption (which the lint precedent rejects without a written justification) or smuggling the substrate through a different crate (which defeats the discoverability point).
- **`kernel/graph-metrics` Tier-1 library crate.** Recommended: parallels `kernel/graph-foundation` (the substrate it consumes), `kernel/audit-ledger` (the event-sourcing precedent it follows), and `kernel/plugin-host` (the cross-tier infrastructure it eventually serves). Tier-1 placement satisfies the runtime-substrate requirement, makes the substrate addressable from every Tier-2 graph consumer (cad-core / kernel/asset / future material / future anim), and lets the §1.10.4 tracker tool eventually become a thin CLI front-end over the kernel substrate's serialized snapshots.

The crate-name decision (`kernel/graph-metrics` vs `kernel/graph-observability` vs another) is left to the phase-1 dispatch — the architectural commitment is **kernel-tier, library-crate, layered on `kernel/graph-foundation`**, and the rename of `tools/graph-metrics` to a CLI front-end (or its outright removal) is a phase-6 concern.

### Sub-decision 2 — three-tier metric architecture (A / B / C) with explicit invariants per tier

Metrics MUST be partitioned into exactly three tiers by computational cost class. The partition is an architectural invariant, not a guideline:

**Tier A — O(1) incremental counters.** Cheap, transactional, mutation-triggered. Stored directly in graph state (or in a per-graph counter struct that hangs off `Graph<N, E>` via a side-table). Update cost: one integer increment/decrement per matching `GraphEvent`. Invariant: Tier-A counters MUST be queryable in constant time and MUST NOT allocate on the hot path. Examples (initial set): `node_count`, `edge_count`, `operator_count`, `constraint_count`, `invalidation_count`.

**Tier B — Incremental structural metrics.** Maintained during graph mutations via partial recomputation around the affected region. Update cost: bounded by the changed subgraph's size, NOT by the whole-graph size. Invariant: Tier-B metrics MUST NOT trigger a full-graph scan on a single-node mutation; the partial-recomputation strategy MUST be documented per-metric and MUST be exercised by a test that mutates a 1k-node graph one node at a time and asserts the per-mutation cost is sublinear. Examples: `max_depth`, `average fanout`, `SCC count`, `dependency diameter`, `topology lineage breadth`.

**Tier C — Expensive analytical metrics.** Computed in a background pass or via an explicit `analyze()` call. NEVER computed synchronously inside a hot mutation path. Invariant: Tier-C metrics MUST surface their cost-class through their type (e.g. `analyze::SemanticEntropy` rather than `metrics.semantic_entropy`) so callers cannot accidentally invoke them on the hot path. Examples: entropy estimates, rebuild instability prediction, semantic volatility, probabilistic recovery confidence, graph centrality.

The tier assignment of each metric is part of its public API. Re-tiering a metric (e.g. promoting a Tier-B to Tier-A by inventing an incremental update path, or demoting a Tier-A by abandoning incremental tracking) is a breaking change that requires an ADR amendment.

### Sub-decision 3 — snapshot-versioning: every metric snapshot carries `(graph_revision, metrics_revision)`

```rust
pub struct GraphMetrics {
    pub graph_revision: RevisionId,
    pub metrics_revision: RevisionId,
    // Tier-A counters (in line)
    pub node_count: u64,
    pub edge_count: u64,
    // ... Tier-A continues
    // Tier-B / Tier-C carried by reference (not embedded — see sub-decision 5)
}
```

The two-revision pair is **load-bearing for three workspace invariants**:

- **Determinism + replay integrity** (PLAN §13.2 / kernel/audit-ledger). Replaying a captured event stream MUST produce byte-identical metrics; without `metrics_revision` separate from `graph_revision`, two replays that differ only in the ordering of intra-tick `GraphEvent`s could converge on the same `graph_revision` but expose intermediate metric snapshots that diverge. The two-revision pair lets a downstream consumer detect "same graph, different metrics-snapshot lineage" and refuse to compare them.
- **Editor overlay drift prevention.** Editor reactive overlays subscribe to metric snapshots; without a revision pair, an overlay that captures a `GraphMetrics` at frame N and re-evaluates at frame N+5 has no way to detect that the underlying graph mutated mid-frame and the metrics it's displaying are stale. Drift surfaces as silently-wrong UI numbers — the worst kind of regression to debug.
- **Distributed comparison invariants.** Cross-process metric sync (deferred to a later substrate) MUST refuse to compare snapshots whose `graph_revision`s differ; without the explicit revision, two participants in a distributed CAD session could "agree" on a metric value that was computed against different graph states.

`RevisionId` is the workspace's existing 128-bit content-derived ID type per `kernel/graph-foundation` (the same type family as `NodeId` / `EdgeId`); the phase-1 implementation reuses it rather than inventing a parallel revision type.

### Sub-decision 4 — event-sourced metric updates via a `GraphEvent` enum

Metric updates MUST be triggered by events drained from a `GraphEvent` stream emitted by `Graph<N, E>` mutations, NOT by full-graph scans:

```rust
#[non_exhaustive]
pub enum GraphEvent {
    NodeAdded { id: NodeId, kind: NodeKind },
    NodeRemoved { id: NodeId },
    EdgeAdded { id: EdgeId, src: NodeId, dst: NodeId },
    EdgeRemoved { id: EdgeId },
    ConstraintAdded { id: ConstraintId, scope: ConstraintScope },
    OperatorExecuted { node: NodeId, duration_micros: u64 },
    TopologySplit { source: NodeId, descendants: SmallVec<NodeId, 4> },
    // ...
}
```

The `#[non_exhaustive]` annotation is mandatory (per the `OpKind` / `PluginError` / `TopologyEvolution` precedent locked in by the 2026-05-10 cross-review #1) so the variant set can grow without breaking downstream pattern-match consumers. Metric subscribers consume events via a thin trait surface that mirrors the `kernel/graph-foundation::InvalidationListener` shape:

```rust
pub trait MetricSubscriber: Send + 'static {
    fn on_event(&mut self, event: &GraphEvent);
}
```

Event-sourcing rather than polling is **load-bearing for** (a) determinism — `GraphEvent` ordering is the canonical mutation log, replayable byte-identically per the kernel/audit-ledger precedent; (b) parallelizability — independent metric streams subscribe without contending on graph-state locks; (c) testability — a metric impl is exercised by feeding a synthetic `Vec<GraphEvent>` and asserting the resulting counters; (d) serialization-safety — the event stream is the single thing that needs to round-trip through capture/restore, not the entire metric collection. It aligns mechanically with the existing `Event` + `EventId(BLAKE3)` infrastructure in `kernel/audit-ledger/src/event.rs`.

### Sub-decision 5 — module split: `core / structural / topology / invalidation / recovery / analytics`

The substrate's source is split across six sub-modules from day one:

```
kernel/graph-metrics/
├── core/         — Tier-A counters; GraphMetrics struct; RevisionId integration
├── structural/   — Tier-B structural metrics (depth, fanout, SCC, diameter)
├── topology/     — topology-volatility + lineage-breadth (consumes ADR-098)
├── invalidation/ — invalidation-propagation pressure metrics
├── recovery/     — probabilistic recovery confidence + repair-ambiguity
└── analytics/    — Tier-C: entropy, centrality, prediction
```

Rationale: monolithic metrics systems become impossible to evolve. Eventually some metrics become editor-only (visualizations of pressure / volatility) / runtime-only (rebuild scheduling) / CI-only (entropy gates) / debugging-only (probabilistic-recovery confidence) / heuristic-only (semantic-entropy estimates). A monolithic crate forces every consumer to depend on every metric; the module split lets consumers depend on the sub-modules they need and lets each sub-module evolve at its own velocity. This mirrors the kernel/asset / kernel/asset-view / kernel/asset-streaming split that was justified by the same evolutionary cost.

The split also gates the phase-rollout: phase-1 ships `core/` only; phase-2 ships `structural/`; phase-3 retro-fits `core/` with `RevisionId`; phase-4 lifts both into the event-sourced shape; phase-5 adds `analytics/`. The directory structure is the rollout's TOC.

## Consequences

### Positive

- **The "catastrophic later" trap is closed before it bites.** Locking in the tier-stratified + event-sourced + snapshot-versioned shape before any metric ships means no metric will ever be implemented as an ad-hoc full-graph scan inside a hot mutation path. The cross-review's warning is converted from a future regret into a structural impossibility.
- **Mechanical alignment with the existing kernel substrate doctrine.** Tier-1 placement, `non_exhaustive` enums, BLAKE3-derived 128-bit ids, BTreeMap-backed deterministic iteration, and event-sourcing all reuse infrastructure that already exists in `kernel/graph-foundation` and `kernel/audit-ledger`. Phase-1 implementation effort is bounded; design effort here is the only novel work.
- **Editor visualization, rebuild scheduling, and probabilistic recovery all gain a stable consumer surface.** Each is currently blocked on "we don't have a metrics substrate to read"; the phase-1 dispatch unblocks all three (with phase-6 specifically targeting the editor overlay layer).
- **§1.10.4 entropy tracker becomes a thin downstream consumer.** Today `tools/graph-metrics`'s purpose is the §1.10.4 tracker; under this ADR, the §1.10.4 tracker becomes a CLI front-end that serializes the kernel substrate's `GraphMetrics` snapshot and computes derived workspace-level entropy from it. The narrow original mandate is satisfied without distorting the broader substrate.
- **AI-assisted CAD repair gains a confidence-signal substrate.** The `recovery/` sub-module's probabilistic-recovery-confidence metric is the natural input to any future repair-suggestion surface; locking in its tier (Tier-C, analytical) and its tier-invariants here means downstream consumers know what kind of compute budget the metric demands.

### Negative / risks

- **Six sub-modules from day one is overkill for phase-1.** Phase-1 ships only `core/`. The remaining five sub-modules are empty until their phase fires. There is a real risk that the empty-skeleton boilerplate accretes failure-class declarations + crate-info matter that costs more than the architectural-discoverability win it buys. Mitigation: the sub-module skeletons are documented as "phase-N landing site" in their `mod.rs` comments and stay genuinely empty (no `pub fn` shells) until their phase dispatches.
- **Event-sourcing has a baseline cost vs polling.** Every graph mutation now emits a `GraphEvent` regardless of whether any subscriber is listening. The cost is a single allocation-free enum construction and a vec-push into a per-graph event buffer, but it IS non-zero and adds to every mutation hot-path. Mitigation: `GraphEvent` is `Copy` for the small variants and uses `SmallVec<NodeId, 4>` for the variants that carry collections (the `TopologySplit` variant is the only currently-foreseen multi-id case); the per-tick buffer is drained synchronously by the metric subscribers and reused.
- **Two-revision snapshot vs one is harder to reason about.** Consumers must understand when to compare on `graph_revision` only vs `(graph_revision, metrics_revision)` jointly. Mitigation: the substrate exposes typed comparison helpers (`GraphMetrics::same_graph(other) -> bool` and `GraphMetrics::byte_identical(other) -> bool`) so callers don't hand-roll the comparison.
- **Tier-stratification is a doctrine call, not a compile-time invariant.** Nothing in the type system mechanically prevents a future implementer from writing a Tier-A metric that internally walks the whole graph. Mitigation: each tier-A counter's update path MUST be a single increment/decrement (or equivalent constant-time op); a phase-2 architecture-lint can scan for `for n in graph.nodes()` patterns inside `core/` source and reject them. (Lint deferred until first violation surfaces; doc-comment-canonical pattern per ADR-104 in the meantime.)

### Mitigations

- **Phase-rollout is the discovery vehicle.** Each phase has a specific scope + acceptance criterion (see Implementation guidance below). A phase that cannot meet its acceptance criterion is signal that an architectural assumption is wrong; the phase is paused for an ADR amendment rather than implementation force-through.
- **Doctrine-level invariants captured in module-doc comments.** Each sub-module's `mod.rs` carries a "Tier and tier-invariants" doc-block listing which tier the metrics in this module belong to and what the per-tier invariants are. Future implementers reading the module's source see the constraints inline.
- **Cross-link with ADR-098.** The `topology/` sub-module's metrics consume the `LineageGraph` from ADR-098; the cross-link is bidirectional (this ADR cross-links there for the topology-volatility metric source; ADR-098's followups list links here once phase-3 lands).
- **Cross-link with ADR-104 deferral pattern.** Tier-B and Tier-C metrics may initially ship in the doc-comment-canonical form (declared in module doc-comments, materialized as real code only when phase fires) — exactly the pattern ADR-104 codified for `KernelCapabilities`. The ADR-104 deferral discipline is cited explicitly here as precedent.

## Alternatives explicitly NOT chosen and why

**Ad-hoc full-graph-scan metrics computed inside hot mutation paths.** This is the cross-review's "catastrophic later" pattern: a `compute_max_depth(&graph)` call inside the editor's rebuild scheduler walks the whole graph once per mutation. Cost analysis: a 10k-node operator graph with 60Hz editor refresh rate would burn 600k node visits/second on a single metric; rebuild latency would explode under load, reactive pipelines would stall on metric-recomputation barriers, editor responsiveness would die during interactive editing of medium-complexity scenes, and distributed CAD synchronization would break because metric computations would consume the bulk of the determinism-window budget. REJECTED unconditionally; the binding decision (Sub-decision 2 + Sub-decision 4) prevents this shape from ever being implemented.

**Monolithic single-module metrics crate.** A single `kernel/graph-metrics/src/lib.rs` containing all six conceptual modules' metrics is structurally simpler and ships faster. REJECTED per the evolutionary cost analysis: the cross-review's "monolithic metrics systems become impossible to evolve" claim is borne out by every workspace's metrics history; consumers eventually depend on subsets, and a monolith forces over-coupling. The kernel/asset / kernel/asset-view / kernel/asset-streaming split is the workspace's existing precedent for the same pressure; replicating that pattern from day one is cheaper than splitting later.

**Untracked synchronous metrics (no `RevisionId` snapshot pair).** A `GraphMetrics` that's just a bag of counters, with no version tracking, is the simplest possible shape. REJECTED per three workspace-invariants: replay integrity (PLAN §13.2 — replays must be byte-identical including their intermediate metric snapshots), distributed-comparison validity (cross-process metric sync requires a stable joint version), and editor-overlay drift prevention (overlays subscribed to a stale snapshot must detect staleness). Each invariant is non-negotiable; the snapshot-version pair is the cheapest mechanism that satisfies all three.

**Polling-based metric recomputation.** Subscribers periodically poll the graph for current state and recompute metrics from scratch. REJECTED on three counts: (a) it CANNOT be made deterministic for replay (poll timing is wall-clock-dependent); (b) it forces every Tier-A metric to be a full-graph-scan (the worst possible pattern; see "Ad-hoc full-graph-scan" above); (c) it cannot meaningfully expose intra-tick metric trajectories (every poll captures a single point in time, missing the mutation history). Event-sourcing is strictly superior on all three axes.

**Defer the entire substrate; let `tools/graph-metrics` stay a stub indefinitely.** This is the path the workspace is currently on. REJECTED because (a) the cross-review explicitly elevates graph-metrics to the second-priority cross-review item (after H3 fault-injection, which has now landed); (b) the editor reactive-overlay layer cannot land without a metric-source surface; (c) future probabilistic-recovery work (PLAN §1.13's `snapshot-recoverable` failure class extension) needs a confidence-metric input. Indefinite deferral converts strategic infrastructure debt into rebuild-blocking technical debt; phase-1 should land within the next-session horizon.

## Implementation guidance

### Seven-phase rollout

Each phase has a bounded scope, an acceptance criterion, and explicit cross-references. Phases ship in order; a phase that fails its acceptance criterion is paused for an ADR amendment, not force-through.

**Phase 1 — Tier A counters.** Scope: `core/` sub-module only. Initial 5 metrics: `node_count`, `edge_count`, `operator_count`, `constraint_count`, `invalidation_count`. Storage: per-`Graph<N, E>` side-table (or a `GraphCounters` struct mounted on `Graph` via a new `graph.counters()` accessor). Update path: O(1) increment/decrement triggered by direct `Graph::insert_node` / `insert_edge` / `remove_*` callsites (event-sourcing comes in phase 4; phase 1 uses direct callsite hooks). Acceptance criterion: ≥5 unit tests covering each counter's correctness across mutation patterns; ≥1 1000-node-mutation soak asserting per-mutation cost is constant; integration test from cad-core::OperatorGraph confirms Tier-A counters reflect operator-graph state. Cross-references: PLAN §1.14, `kernel/graph-foundation` module-doc.

**Phase 2 — Structural incremental metrics.** Scope: `structural/` sub-module. Metrics: `max_depth`, `average_fanout`, `scc_count`, `dependency_diameter`. Update path: partial recomputation around the mutation's affected region (NOT full-graph scan). Acceptance criterion: per-mutation cost on a 1k-node-DAG is ≤O(log n) for at least three of four metrics (the SCC metric may require O(n) on certain mutation classes — documented per-metric); soak test asserting the cost ceiling. Cross-references: ADR-098 (lineage breadth deferred to phase 3); kernel/graph-foundation::Graph adjacency caches.

**Phase 3 — Revision/version integration.** Scope: retro-fit `core/` Tier-A counters with `(graph_revision, metrics_revision)` snapshot pair; lift `GraphMetrics` to a versioned snapshot type. Update path: `metrics_revision` increments on every counter update; `graph_revision` reuses `kernel/graph-foundation`'s revision-id type (or, if not yet present, materializes it as part of this phase). Acceptance criterion: replay determinism gate — capturing a 100-mutation event stream + replaying it produces byte-identical `GraphMetrics` snapshots; same-graph-different-revision detection passes. Cross-references: PLAN §13.2 (snapshot determinism gates); ADR-114 (the joint replay-determinism + plugin-host pairing).

**Phase 4 — Event sourcing.** Scope: introduce `GraphEvent` enum + `MetricSubscriber` trait; migrate phase-1 direct-callsite-hook updates to event-stream subscribers; refactor `Graph<N, E>` to emit `GraphEvent`s as part of mutation. Acceptance criterion: zero behavioural delta on Tier-A counters (replay determinism gate from phase 3 still passes byte-identically); subscriber-decoupling test confirms a metric subscriber can be added/removed without modifying `Graph` source. Cross-references: kernel/audit-ledger's `Event` + `EventId` precedent; cross-review's #[non_exhaustive] doctrine from cross-review #1.

**Phase 5 — Analytical metrics.** Scope: `analytics/` sub-module. Metrics: entropy estimate (PLAN §13.10's "graph invalidation propagation depth" maps here); rebuild instability prediction; semantic volatility; graph centrality. Update path: explicit `analyze()` call OR background pass; NEVER synchronous on hot path. Acceptance criterion: each metric documents its compute-cost class in its rustdoc; a "no-Tier-C-on-hot-path" architecture lint is added (or, if deferred, the doc-comment-canonical pattern per ADR-104 is followed). Cross-references: PLAN §13.10 entropy gates; ADR-104 deferral discipline.

**Phase 6 — Editor visualization.** Scope: `viz_adapter`-style trait surface for editor overlays consuming Tier-A and Tier-B metrics. Each metric becomes addressable through a `MetricView` reading `GraphMetrics` snapshots. Acceptance criterion: an editor overlay widget renders a Tier-A counter and a Tier-B structural metric without coupling to the kernel substrate's concrete types; PLAN §1.15 editor-state coordination is documented. Cross-references: docs/§18/GRAPH_FOUNDATION.md `VizAdapter` precedent; PLAN §1.15.

**Phase 7 — Predictive/recovery metrics.** Scope: `recovery/` sub-module. Metrics: probabilistic recovery confidence; repair ambiguity score; intent-degradation estimate. These are the highest-research-grade metrics and may warrant their own ADR per metric class. Acceptance criterion: at least one metric ships end-to-end with a documented use-case (e.g. "feeds the snapshot-recoverable failure-class diagnostic stream per PLAN §1.13"). Cross-references: PLAN §1.13 failure containment; ADR-098 (topology-volatility input).

### Canonical patterns

The snippets below are intentionally illustrative and SHOULD be the starting point for the phase-1 dispatch's actual implementation. They mirror the patterns in `kernel/graph-foundation::Invalidation` and `kernel/audit-ledger::Event` rather than introducing novel shapes.

#### `GraphMetrics` snapshot (phase 1 + phase 3 shape)

```rust
// kernel/graph-metrics/src/core/snapshot.rs
use kernel_graph_foundation::RevisionId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphMetrics {
    /// Revision of the graph this metrics snapshot was computed against.
    /// Two metric snapshots with different `graph_revision`s describe
    /// different graphs; comparing them numerically is undefined.
    pub graph_revision: RevisionId,

    /// Monotonic revision of the metric collection itself. Increments on
    /// every counter update. Two snapshots with the same `graph_revision`
    /// and different `metrics_revision`s capture the same graph at
    /// different points in the intra-tick mutation sequence.
    pub metrics_revision: RevisionId,

    // Tier-A counters (in-line; never allocate to read).
    pub node_count: u64,
    pub edge_count: u64,
    pub operator_count: u64,
    pub constraint_count: u64,
    pub invalidation_count: u64,
    // ... Tier-B / Tier-C carried via separate accessors per the
    // module split (see Sub-decision 5).
}

impl GraphMetrics {
    /// Returns true iff the two snapshots describe the same graph
    /// revision (i.e. were computed against the same `Graph<N, E>`
    /// content). Cheap O(1) compare.
    pub fn same_graph(&self, other: &Self) -> bool {
        self.graph_revision == other.graph_revision
    }

    /// Returns true iff the two snapshots are byte-identical (same
    /// graph + same metric-collection revision). Used by replay
    /// determinism gates (PLAN §13.2).
    pub fn byte_identical(&self, other: &Self) -> bool {
        self == other
    }
}
```

#### `GraphEvent` enum (phase 4 shape)

```rust
// kernel/graph-metrics/src/event.rs
use kernel_graph_foundation::{NodeId, EdgeId};
use smallvec::SmallVec;

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GraphEvent {
    NodeAdded { id: NodeId, kind: NodeKind },
    NodeRemoved { id: NodeId },
    EdgeAdded { id: EdgeId, src: NodeId, dst: NodeId },
    EdgeRemoved { id: EdgeId },
    ConstraintAdded { id: ConstraintId, scope: ConstraintScope },
    OperatorExecuted { node: NodeId, duration_micros: u64 },
    TopologySplit {
        source: NodeId,
        // SmallVec keeps the small-N case alloc-free (typical splits
        // are 2-4 descendants).
        descendants: SmallVec<[NodeId; 4]>,
    },
    // ... future variants land here; #[non_exhaustive] makes new
    // variants non-breaking for downstream pattern-match consumers.
}
```

`#[non_exhaustive]` is mandatory per the cross-review #1 doctrine (locked in by `OpKind` / `PluginError` / `TopologyEvolution`). Pattern-match consumers MUST include a `_ => ...` arm; missing arms are a compile error in downstream crates only when the variant set grows.

#### `MetricSubscriber` trait (phase 4 shape)

```rust
// kernel/graph-metrics/src/subscriber.rs
pub trait MetricSubscriber: Send + 'static {
    /// Called once per `GraphEvent` emitted by the upstream `Graph<N, E>`.
    /// Implementations MUST NOT allocate on this path (see engineering
    /// constraints). Tier-A subscribers do a single counter update;
    /// Tier-B subscribers do a bounded partial recomputation; Tier-C
    /// subscribers MUST defer to a background `analyze()` pass and
    /// only buffer events here.
    fn on_event(&mut self, event: &GraphEvent);
}
```

Mirrors `kernel/graph-foundation::InvalidationListener` shape; the trait is intentionally minimal so subscriber registration / unregistration mechanics can reuse the `ListenerHandle` pattern from `kernel/graph-foundation::Invalidation`.

### Test recipes (mandatory per phase)

Each phase's acceptance criterion includes the test recipes below. The recipes are anchored to the phase that lands them; later phases extend rather than replace.

**Phase 1 (Tier A counters).**

1. **Counter correctness across mutation patterns.** Insert 100 nodes, remove 50; assert `node_count == 50`. Insert 200 edges across them, remove 100; assert `edge_count == 100`. Repeat for `operator_count`, `constraint_count`, `invalidation_count`.
2. **Constant-cost soak.** Mutate a 1000-node graph one node at a time, 10000 mutations total. Per-mutation wall-clock cost variance MUST stay within 2× of the median; non-constant-cost behaviour fails the test.
3. **OperatorGraph integration.** Mount counters on `cad-core::OperatorGraph`; build a 50-operator graph; assert `operator_count == 50` and the Tier-A view matches the operator graph's actual state.

**Phase 3 (revision-pair).**

4. **Replay determinism gate.** Capture a 100-mutation event stream + serialize the resulting `GraphMetrics`. Re-play the same stream into a fresh graph; serialize again. Assert byte-identity. Repeat 10× with the same input + assert all 10 produce identical bytes.
5. **Same-graph-different-revision detection.** Construct two `GraphMetrics` with the same `graph_revision` but different `metrics_revision`. Assert `same_graph` returns `true` and `byte_identical` returns `false`. Construct two with the same `metrics_revision` but different `graph_revision`. Assert `same_graph` returns `false` (this case shouldn't arise in practice — a fresh `metrics_revision` implies a fresh graph state — but the typed comparison MUST handle it).

**Phase 4 (event sourcing).**

6. **Subscriber-decoupling.** Build a graph; emit 1000 `GraphEvent`s; register a Tier-A counter subscriber after event 500; assert it receives events 501-1000 only (no replay of historical events). Unregister; assert no further events. Add/remove subscribers without recompiling the graph crate (i.e. via the `Box<dyn MetricSubscriber>` trait).
7. **Behavioural delta = 0 from phase 3.** Run the phase-3 replay-determinism gate against the event-sourced phase-4 implementation. Assert byte-identical `GraphMetrics` snapshots. The migration from direct-callsite to event-sourced MUST be transparent.

**Phase 5+ (analytical / editor / recovery metrics).**

Per-metric test recipes live in the per-metric ADR amendments; each amendment names its recipe set as part of phase acceptance.

### Four highest-long-term-value future metric categories

The cross-review identifies four categories whose long-term value most justifies the infrastructure investment. Each is sketched below; their phase-anchored ADR sub-references will materialize in later amendments as each category's metric class lands.

1. **Invalidation propagation pressure** — measures how expensive a typical change becomes (i.e. how many downstream operators / nodes / edges a single mutation forces to recompute). Critical for reactive rebuild scheduling, incremental evaluation, and viewport responsiveness. Concretely: a Tier-B structural metric measuring the average BFS depth of `kernel/graph-foundation::Invalidation::mark_dirty` propagation, plus a Tier-A counter for total propagation events. Phase: 2-4. Why high-value: it's the metric that lets the rebuild scheduler decide between eager and lazy recomputation, which directly drives editor responsiveness during interactive editing.

2. **Topology volatility** — measures how unstable the persistent semantic identity (PersistentFaceId, ADR-098 lineage) is across rebuilds. Critical for persistent references, topology lineage, and downstream operator survivability. Concretely: a Tier-B metric over `LineageGraph::edges` measuring the ratio of `Reinterpreted` + `Deleted` evolutions to total edges per rebuild; surfaces "this scene has 30% identity-instability" as a user-visible quality signal. Phase: 3-5 (consumes ADR-098). Why high-value: **highly differentiated infrastructure** — no game engine and few CAD kernels expose a comparable metric; it is the input to "should this entity be tracked persistently or is its identity too unstable to be useful" decisions.

3. **Constraint tension / conflict density** — measures how close the graph is to an unstable / over-constrained state. Useful for solver optimization, predictive repair, and AI-assisted diagnostics. Concretely: a Tier-B metric counting `ConstraintAdded` events vs DOF-budget; a Tier-C analytical metric estimating conflict resolution difficulty. Phase: 4-5. Why high-value: it's the metric the constraint solver uses to decide between fast-path and slow-path solving, and the metric an AI-assisted diagnostic surface would consume to suggest "this constraint is the highest-tension; relaxing it would resolve N downstream conflicts".

4. **Semantic entropy** — probably the most important research-oriented metric. Estimates graph disorder, intent degradation, reconstruction difficulty, and repair ambiguity. Connects to probabilistic recovery, intent graphs, and semantic constraints. Concretely: a Tier-C analytical metric in `analytics/` consuming the full graph + lineage + constraint state and emitting a scalar entropy estimate plus a per-region breakdown. Phase: 7. Why high-value: it is the load-bearing input to any future AI-assisted CAD repair surface; without it, repair-suggestion systems have no confidence signal to rank suggestions by.

### Engineering constraints (load-bearing)

Five constraints apply uniformly across all tiers and all phases:

- **Optional.** The substrate MUST be optional at the workspace level — a no-op subscriber set leaves all `GraphEvent` dispatch as a single zero-cost branch. Workspaces that don't consume metrics pay only the event-construction cost.
- **Feature-gated.** Each tier (A / B / C) MUST be feature-gated. A workspace that wants only Tier-A counters MUST be able to compile out Tier-B and Tier-C entirely. Feature gates: `tier-a` (always-on), `tier-b`, `tier-c`.
- **Alloc-aware.** No metric update path may allocate on the hot path. `SmallVec<NodeId, 4>` for the `TopologySplit` variant; counter updates MUST be primitive integer ops; subscriber dispatch MUST reuse a pre-allocated buffer per `Graph<N, E>` instance.
- **Deterministic.** All metric updates MUST be deterministic given a deterministic event ordering. No wall-clock time, no system entropy, no thread-local state. Replay determinism gates per PLAN §13.2 apply.
- **Serialization-safe.** `GraphMetrics` snapshots MUST round-trip byte-identically through RON serialization (the workspace's snapshot format), exactly like `GraphSnapshot<N, E>` does in `kernel/graph-foundation::snapshot`. The `(graph_revision, metrics_revision)` pair is part of the wire format.

Violation of any of the five constraints is a substrate-level architectural regression and MUST be reverted, not patched.

## Followups / open questions

- **Crate-name decision: `kernel/graph-metrics` vs `kernel/graph-observability` vs another.** Defer to phase-1 dispatch. The kernel-tier placement is binding; the exact name is a phase-1 affordance choice. ADR-115 amendment will land the name once phase-1 is in flight.
- **Tier-B incremental algorithms.** Each Tier-B metric needs a per-metric partial-recomputation strategy; some (SCC count) are research-grade rather than mechanical. Phase-2 dispatch needs a detailed-design pass per metric, possibly a sub-ADR for the SCC-incremental strategy.
- **Tier-C analytical algorithms.** The four high-value metric categories above each warrant their own ADR in the limit; phase-5 and phase-7 dispatches may need to amend ADR-115 or branch a sub-ADR per metric class (ADR-098's relationship to ADR-104 is the precedent).
- **`tools/graph-metrics` rename / removal.** Once the kernel substrate exists, the `tools/graph-metrics` binary either becomes a thin CLI front-end (e.g. `rge-tool-graph-metrics serialize-snapshot`) or is removed entirely with §1.10.4 entropy tracking moving to a different tool. Decision deferred to phase-6 dispatch.
- **Editor visualization layer coordination.** Phase-6's editor-overlay surface must coordinate with `editor-state` per PLAN §1.15. The overlay surface is a `VizAdapter`-style trait modeled after `kernel/graph-foundation::VizAdapter`, but the editor-state ↔ runtime-state synchronization edge needs explicit design (PLAN §1.10.4 lists it as an entropy metric).
- **Cross-process metric sync (distributed CAD case).** The two-revision pair is necessary-but-not-sufficient for distributed CAD; full cross-process sync needs a CRDT-style merge story or a leader-elects-canonical story. Defer until the distributed substrate lands.
- **Architecture-lint for "no-Tier-C-on-hot-path".** Mechanical enforcement of the tier invariants. Defer until first violation surfaces; doc-comment-canonical per ADR-104 in the meantime.
- **§1.10.4 entropy-tracker convergence.** PLAN §1.10.4 lists 16 architecture-entropy metrics; some (graph invalidation propagation depth — line 525) align directly with the cross-review's "invalidation propagation pressure" category; others (cross-crate dep edges, archetype counts) are workspace-static and don't fit the runtime substrate. The convergence design — which §1.10.4 metrics become Tier-C analytical metrics, which stay workspace-static, which split — is a phase-5 concern. Documented here as the explicit source-truth point: the cross-review's reframing **does not subsume** §1.10.4; it sits alongside it.

## References

- **PLAN.md §1.10.4** — current "architecture entropy metrics" placement; the 16-metric workspace-static table that the cross-review's reframing extends rather than subsumes.
- **PLAN.md §1.14** — `kernel/graph-foundation` substrate doctrine; the upstream consumer this ADR layers on.
- **PLAN.md §13.10** — entropy gates; the current mechanical dependency on §1.10.4 metrics.
- **PLAN.md §13.14** — graph-foundation gates; the parent substrate-discipline framework ("graph-foundation API additions require ADR — avoid god-substrate"). ADR-115 explicitly extends this discipline to graph-metrics.
- **PLAN.md §1.13** — failure containment; phase-7 recovery metrics feed `snapshot-recoverable` failure-class diagnostics.
- **PLAN.md §13.2** — CAD validation gates; phase-3 replay-determinism integration.
- **ADR-098** — topology lineage substrate; phase-3's topology-volatility metric reads `LineageGraph::edges`. Cross-link is bidirectional: ADR-098 followups list the `kernel/graph-foundation::Graph` migration, which becomes the input to ADR-115's topology metrics.
- **ADR-104** — capability surface; the doc-comment-canonical deferral pattern is reused for Tier-B / Tier-C metrics that ship in doc form before code form.
- **ADR-114** — PluginContext owned-handoff; same cross-review provenance (cross-review #1 ↔ ADR-114 amendments; cross-review #2 ↔ this ADR), and the plugin-host substrate is a future consumer of metrics for plugin-isolation diagnostics.
- **`docs/§18/GRAPH_FOUNDATION.md`** — companion doc for `kernel/graph-foundation`; ADR-115 will gain a sibling `docs/§18/GRAPH_METRICS.md` once phase-1 lands.
- **`tools/graph-metrics/src/main.rs`** — current 5-line stub binary cited as the starting point; phase-1 dispatch retires this stub in favor of a `kernel/graph-metrics` library crate (with the binary becoming a CLI front-end or being removed in phase 6).
- **`kernel/audit-ledger/src/event.rs`** — event-sourcing precedent (`Event`, `EventId`, BLAKE3-derived deterministic identifiers). Phase-4 reuses this infrastructure for `GraphEvent`.
- **`kernel/graph-foundation/src/invalidation.rs`** — `Invalidation` + `InvalidationListener` precedent; `MetricSubscriber` mirrors this trait shape.
- **change.md 2026-05-10 02:00 entry** — the ChatGPT cross-review #2 archived in full, mirroring the audit-1 cross-review precedent cited inline in ADR-114. The cross-review is the source-of-truth for the reframing; this ADR captures the binding decisions consistent with the workspace's existing architecture.
