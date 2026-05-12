# ADR-118: Frame-graph transient-resource allocator policy

| Status | Accepted 2026-05-12 (binding semantic decision; implementation primitive deferred to dispatch 119) |
|---|---|
| Date | 2026-05-12 |
| Deciders | (RGE architecture review) |
| PLAN references | §6.1 (frame-graph minimal — `IMPLEMENTATION.md:431` verbatim "Frame-graph (minimal — transient resource lifetimes computed at frame begin)"), §6 / §8 (renderer architecture brief — `PLAN.md:617` "Render \| same \| frame-graph compile, transient resource alloc"; `PLAN.md:999` "Render graph compiler · double-buffered scene state with cad-core checkpoint reference · transient resources (aliased VRAM)") |
| §18 references | `GFX_RENDER_TIER.md:24` ("Frame-graph — transient resource lifetimes computed at frame begin; `TexturePool` / `BufferPool` keyed on frame index"); `GFX_RENDER_TIER.md:234` ("Transient resource lifetimes computed at frame begin. `TexturePool` / `BufferPool` keyed on frame index so allocator pressure stabilises. Declarative pass DAG with read/write resource declarations") |
| ADR references | ADR-114 (PluginContext owned-handoff — owned-cross-boundary substrate precedent), ADR-115 (graph-metrics substrate design — semantics-first, implementation-deferred precedent), ADR-117 (render-handoff mechanism — sibling design ADR for the same Phase 6 chapter) |
| Doctrine refs | `docs/architecture/SCENE_EXTRACTION_CONTRACT.md` §3 (renderer NEVER owns authoritative geometry — Layer-4 downstream-only), `docs/architecture/REACTIVE_INVALIDATION.md` §1 (Layer 4 — GPU upload), `docs/§18/GFX_RENDER_TIER.md` §1 / §11 / §12 |
| Implementation phase | Dispatch 119 (this ADR; design-only). Dispatch 119+ lands `ResourceDescriptor` + the per-D7 first implementation per the policy pinned here. |

## Context

Phase 6.1's frame-graph minimal substrate shipped at `crates/gfx/src/frame_graph/{mod,pass,resource,compile}.rs` as the analytical layer of the chapter: vocabulary + ownership boundaries + future-safe seams. The module-doc names the deliverables explicitly:

> `CompiledFrameGraph` — analysis output: execution order + per-resource lifetimes + transient aliasing groups + a `structural_hash` for determinism testing.

and equally explicitly names the NON-GOAL the present ADR addresses:

> No GPU resource allocation. This module produces ordering and lifetime metadata only; an eventual transient-resource allocator (out of scope) consumes `AliasingGroup` to size and free transient backing storage.

PLAN §6.1 names the requirement verbatim at `IMPLEMENTATION.md:431`:

> Frame-graph (minimal — transient resource lifetimes computed at frame begin)

PLAN §1.14 names the renderer's specific responsibility within graph-foundation at `PLAN.md:617`:

> Render \| same \| frame-graph compile, transient resource alloc

PLAN §8 (renderer architecture brief) names the substrate's role at `PLAN.md:999`:

> Render graph compiler · double-buffered scene state with cad-core checkpoint reference · transient resources (aliased VRAM) · shader permutations on demand · PSO cache · async shader compile daemon · bindless · frustum + portal at v1.0 · Hi-Z at Phase 5 · GPU device-lost recovery.

`docs/§18/GFX_RENDER_TIER.md` §1 names the chapter's incremental build at `:24`:

> Frame-graph — transient resource lifetimes computed at frame begin; `TexturePool` / `BufferPool` keyed on frame index; declarative pass DAG with read/write resource declarations.

and §11.1 elaborates at `:234`:

> Transient resource lifetimes computed at frame begin. `TexturePool` / `BufferPool` keyed on frame index so allocator pressure stabilises. Declarative pass DAG with read/write resource declarations — the frame graph derives the per-pass barriers and resource lifetimes from the declarations rather than letting the gfx author hand-write them. Necessary precursor for the §1.5.2 sim/render thread split because the frame graph is what owns the per-frame lifetime decisions.

ADR-117 (2026-05-11) is the sibling design ADR for the same Phase 6 chapter — render-input handoff mechanism for §6.3 Gate C. It pinned the semantics of how a render-thread-bound snapshot crosses the sim/render boundary; this ADR pins the **transient-resource allocator policy** that the same chapter's frame-graph substrate uses to translate analytical `AliasingGroup`s into physical GPU allocations. The two ADRs together close the chapter's design pressure: ADR-117 binds the sim → render boundary; ADR-118 binds the renderer's allocator semantics.

The present substrate state at 2026-05-11: analytical layer shipped (`FrameGraph::compile` → `CompiledFrameGraph { execution_order, resource_lifetimes, aliasing_groups, structural_hash }`); deterministic structural hash; 13 unit tests + 1 integration smoke covering the analytical surface. The `ResourceId` is opaque 16-byte; the substrate carries no `ResourceDescriptor`; the substrate carries no integration with `FrameRecorder` / `LitMeshPipeline`. Allocation was deliberately deferred per the NON-GOAL list — that deferral is what this ADR closes.

The load-bearing semantic this ADR pins in one sentence: **physical allocation reuses GPU resources within and across frames per the analytical aliasing groups; the policy is descriptor-keyed pools, ring-buffered across frames-in-flight, with wgpu's hazard tracking trusted for barrier insertion.**

Seven sub-decisions follow.

## Decision

**Transient resources allocate from descriptor-keyed pools partitioned per-frame-in-flight; `ResourceDescriptor` is the pool key (descriptor + aliasing-group identity); pools are ring-buffered across N frames-in-flight (initial N=2); wgpu's auto-tracked barriers are trusted; texture and buffer pools are separate types (NOT a unified enum); the analytical `AliasingGroup` from `compile.rs` is the contract that determines physical reuse within a frame; `unsafe` is forbidden at this layer per workspace policy.**

### D1. `ResourceDescriptor` fields for textures and buffers

Two concrete descriptor types live alongside `ResourceId` in `crates/gfx/src/frame_graph/resource.rs` (or sibling `descriptor.rs` — dispatch-119's file-shape call):

**`TextureDescriptor`** (wgpu 29 types):

```rust
pub struct TextureDescriptor {
    pub width: u32,
    pub height: u32,
    pub depth_or_array_layers: u32,
    pub mip_level_count: u32,
    pub sample_count: u32,
    pub format: wgpu::TextureFormat,
    pub usage: wgpu::TextureUsages,
    pub dimension: wgpu::TextureDimension,
    pub view_dimension: wgpu::TextureViewDimension,
}
```

**`BufferDescriptor`** (wgpu 29 types):

```rust
pub struct BufferDescriptor {
    pub size_bytes: u64,
    pub usage: wgpu::BufferUsages,
}
```

Fields are concrete and match `wgpu::TextureDescriptor` / `wgpu::BufferDescriptor` shapes. `label` is not part of the descriptor (it would defeat pool-key matching — see D3 below); the allocator may synthesize a label per allocation for diagnostic purposes. `mapped_at_creation` is omitted from `BufferDescriptor` — transient resources are always GPU-resident and never CPU-mapped at creation; that workflow belongs to staging buffers, which are outside this allocator's lane.

Both descriptors derive `Clone, Copy, Debug, PartialEq, Eq, Hash` so they can be used as `BTreeMap` keys without `Cow`/heap. Texture-format and usage bitflags satisfy `Hash` via wgpu 29's derived impls.

### D2. Separate vs unified texture/buffer descriptors

**Separate.** `TextureDescriptor` and `BufferDescriptor` are distinct types with distinct fields. The allocator exposes `TexturePool` and `BufferPool` as distinct types.

A unified `ResourceDescriptor { Texture(TextureDescriptor), Buffer(BufferDescriptor) }` enum was considered. Rejected: the variants share no fields beyond the abstract notion of "size and usage"; every operation against the unified enum requires a match arm; both descriptors have a single natural pool implementation; the analytical `ResourceId` already supplies the cross-type abstraction layer for the `CompiledFrameGraph`. The unified enum would add code surface without simplifying any consumer.

A future audio/compute/raytracing tier (acceleration structures, blas/tlas) may surface a third descriptor type; the separate-pool shape extends linearly (one new pool per resource class) where the unified enum would require touching every match arm.

### D3. Pool key

**Descriptor + aliasing-group identity.** A pool key is `(TextureDescriptor, AliasingGroupId)` for `TexturePool` and `(BufferDescriptor, AliasingGroupId)` for `BufferPool`. The `AliasingGroupId` is a stable identifier minted at `compile()` time — `AliasingGroupId = usize` keyed on the position in `CompiledFrameGraph::aliasing_groups()`, or a content-derived hash over the group's `ResourceId`s for cross-frame stability when the graph is rebuilt with the same shape.

The descriptor-only key was considered (any descriptor-matching slot reuses; risk: wgpu auto-tracked barriers may insert false-RAW or false-WAW barriers across analytically-independent reuses). Rejected: the analytical `AliasingGroup` already proves non-overlap within a frame; the cross-group reuse the descriptor-only key would unlock comes with implicit hazard-tracking pressure that is not visible to the compile-time analysis.

The descriptor + frame-index key (per-frame partitioning, no cross-frame reuse) was considered. Rejected as primary key: it implies allocating fresh resources every frame, which contradicts the "TexturePool / BufferPool keyed on frame index so allocator pressure stabilises" requirement at `GFX_RENDER_TIER.md:234`. The frame-index dimension is captured by D4's ring-buffered lifetime policy, NOT by the pool key.

### D4. Lifetime policy

**Ring-buffered across N frames-in-flight; initial N=2.** Each frame-in-flight slot owns one set of `TexturePool` + `BufferPool` instances. The renderer rotates slots per frame; on frame F, slot `F % N` is the active allocation arena. A slot's pools are reset (resources returned to the pool's free-list, NOT released to wgpu) at the start of the frame that re-enters that slot, AFTER waiting on the previous frame's submission fence.

The initial `N=2` (single-buffered + 1 in-flight) is the minimum that decouples CPU recording from GPU execution; wgpu typically operates with N=2–3 depending on platform. The policy permits dispatch 119 to start with `N=2` and a future amendment to bump to N=3 if the empirical fence-wait surfaces as a hotspot; the contract is "the slot is safe to reuse once the corresponding GPU submission has retired".

Per-frame-reset (drop every allocation at frame end) was considered. Rejected: contradicts `GFX_RENDER_TIER.md:234` ("allocator pressure stabilises"); wastes wgpu device allocations; defeats the cross-frame reuse the chapter explicitly asks for.

LRU eviction was considered. Rejected as v0: adds a heuristic ("how recently was this descriptor used?") that obscures the deterministic structural-hash discipline the frame-graph substrate is built on. May be revisited if memory-pressure measurement surfaces it as needed.

Explicit-release (caller marks resource done) was considered. Rejected: the analytical `ResourceLifetime` already names first/last use; explicit release would re-derive that information from caller-side state, duplicating the substrate's source of truth.

### D5. AliasingGroup → physical allocation mapping

**Greedy per-group allocation; the descriptor with the largest size in a group governs the physical slot.** At the start of each frame, the allocator walks `CompiledFrameGraph::aliasing_groups()` in order; for each group, it computes the maximum descriptor size in the group (the resources share the slot — the slot has to fit the largest); it queries the pool for a matching `(MaxDescriptor, AliasingGroupId)` slot; on hit, reuses; on miss, allocates fresh and inserts into the pool.

The group's resources thereafter share the physical slot — `ResourceId` → physical-handle resolution is `O(1)` via a per-frame `BTreeMap<ResourceId, PhysicalHandle>` populated at the start of the frame. Resources in different groups MUST have different physical handles (the analytical contract is that overlapping lifetimes never share a slot).

Lazy mapping (resolve `ResourceId` → physical-handle on first use during pass recording) was considered. Rejected: introduces ordering coupling between pass recording and allocation, complicating the recording API; the eager-at-frame-start approach is the simplest realization of "transient resource lifetimes computed at frame begin" from `IMPLEMENTATION.md:431`.

Eager pre-allocation of the largest descriptor across ALL groups (a single physical slot per group, sized to the union of all groups' largest descriptors) was considered. Rejected: over-allocates when groups have heterogeneous descriptors (e.g. a shadow map group with one 4K depth texture and a thumbnail group with one 128² color texture should NOT share a 4K slot).

### D6. Trust wgpu hazard tracking, or enforce non-overlap from frame-graph analysis?

**Trust wgpu.** wgpu 29 auto-tracks resource state and inserts barriers as needed across draw / compute / copy boundaries. The allocator does NOT emit manual barriers; the analytical `AliasingGroup` is a *hint* to the allocator about reuse opportunity, not a *contract* with the GPU about hazard absence.

The explicit alternative — manual `wgpu::BufferBindingType` / texture-layout transitions emitted per pass per the analytical lifetime metadata — was considered. Rejected for v0: (a) wgpu's auto-tracking is the documented API surface for safe usage at the wgpu 29 level; bypassing it requires `wgpu::CommandEncoder`-level barrier emission that is undocumented for safe use; (b) the v0 substrate has zero passes today, so the empirical pressure for manual barrier insertion is zero; (c) per-frame validation is the wgpu validation layer's job — manual barrier emission would force the substrate to duplicate that validation logic.

A future amendment may revisit if measurement surfaces wgpu's barrier insertion as a hotspot (the canonical "auto-tracking inserted too many barriers and we want to elide them" scenario). The trigger for the amendment is empirical evidence from a profiling pass against the 60fps simple-scene golden; not speculation.

The audit cadence: dispatch 119's first non-trivial pass (mesh + lit-mesh + shadow + lighting) will surface whether wgpu's auto-tracking degrades performance. If it does, the amendment-trigger is met; if it doesn't, the trust policy stands.

### D7. First implementation dispatch (dispatch 119)

**`ResourceDescriptor` types + lift `ResourceId` to a frame-allocated table.** Dispatch 119 lands the smallest substrate change that unblocks the rest of the chapter:

- NEW `crates/gfx/src/frame_graph/descriptor.rs` (or co-located in `resource.rs`) — the `TextureDescriptor` + `BufferDescriptor` types per D1; ~80-140 LOC including derived impls + unit tests.
- API addition on `FrameGraph` / `CompiledFrameGraph` — a `BTreeMap<ResourceId, ResourceClassDescriptor>` (where `ResourceClassDescriptor` is an internal enum `Texture(TextureDescriptor) | Buffer(BufferDescriptor)` used only at the allocator interface, NOT exposed in the public ResourceDescriptor surface per D2). Callers declare a descriptor per `ResourceId` when they call `add_pass`; the analytical substrate carries the metadata through to compile. ~40-80 LOC delta in `pass.rs` + `mod.rs`.
- NO pool implementation in dispatch 119. The pools (D3 / D4 / D5) are dispatch 120; the integration with `FrameRecorder` is dispatch 121.

Footprint estimate: ~150-250 LOC net; ~3 new tests covering descriptor identity + descriptor flow through compile. No `Cargo.toml` change; no new dep; no architecture-lint touch.

The smallest-first sequencing keeps the substrate semantics-first, code-second per the ADR-115 / ADR-117 precedent. The reasoning: the descriptor lift is the only change to the present substrate's `ResourceId` shape; landing it first means dispatch 120's `TexturePool` can be built against a stable descriptor type instead of needing to introduce both at once.

The alternatives considered for the dispatch 119 choice:

- (b) `TexturePool` MVP without `BufferPool` — rejected: requires `TextureDescriptor` to land in the same dispatch, doubling the change set.
- (c) Full `TexturePool` + `BufferPool` + allocator in one dispatch — rejected: too large for a single dispatch; the substrate's design risk concentrates in a single change.
- (d) Integration with `FrameRecorder` first — rejected: depends on pool implementation which depends on descriptors; cannot precede them.

## Alternatives considered

| Alternative | Pros | Cons | Why rejected |
|---|---|---|---|
| **Descriptor-keyed pools + ring-buffered + trust wgpu (this ADR)** | Mirrors GFX_RENDER_TIER.md:234 verbatim; minimal code surface; wgpu validation is the safety net; analytical AliasingGroup is the reuse contract | One allocation per descriptor shape per slot; non-trivial pool warmup on the first N frames | **Chosen.** Matches the chapter's verbatim requirement; substrate-shape minimum that satisfies all named invariants. |
| Unified `ResourceDescriptor { Texture(...), Buffer(...) }` enum | One type for ergonomics | Match-arm proliferation; no shared fields; two natural pools anyway | Rejected per D2. |
| Descriptor-only pool key (no aliasing-group identity) | Maximal cross-group reuse | wgpu auto-tracking may insert hazard barriers that defeat the reuse savings; the analytical group becomes ornamental | Rejected per D3. |
| Per-frame reset (no cross-frame reuse) | Simple; one allocation cycle per frame | Allocator pressure does NOT stabilise per GFX_RENDER_TIER.md:234; contradicts the chapter requirement | Rejected per D4. |
| LRU eviction lifetime policy | Heuristic memory-pressure relief | Introduces a non-deterministic eviction policy that obscures the structural-hash discipline | Rejected per D4. |
| Eager union-allocation (one slot per group, sized to global max) | Single allocation per group | Over-allocates heterogeneous groups | Rejected per D5. |
| Manual barrier insertion (D6 alternative) | Maximum determinism | Requires duplicating wgpu's hazard validation; v0 has zero passes to justify the cost | Rejected per D6; revisitable if measurement says so. |
| Explicit-release (caller marks resource done) | Caller controls lifetime | Re-derives the substrate's source of truth; duplicates `ResourceLifetime` metadata | Rejected per D4. |
| Allocate `TexturePool` + `BufferPool` + integration in dispatch 119 | One commit, one chapter close | Too large for a single dispatch; design risk concentrates | Rejected per D7. |

The decision matrix collapses to: the chosen option is the unique shape that satisfies (a) the verbatim chapter requirement at `GFX_RENDER_TIER.md:234`, (b) the analytical `AliasingGroup` contract from `compile.rs`, (c) the workspace `unsafe_code = "forbid"` policy, and (d) the substrate-shape minimum principle from ADR-115's "kernel-tier, library-crate, layered on `kernel/graph-foundation`" framing. Manual barrier emission and unified enums are revisitable; the rest are stable for the chapter's duration.

## Consequences

### Positive

- **Phase 6 frame-graph chapter unblocked.** The remaining substrate work is mechanically derivable from this ADR's decisions: dispatch 119 lands descriptors, 120 lands pools, 121 wires to `FrameRecorder`. No further design pressure between here and the chapter's exit gate.
- **Analytical and physical layers remain composable.** The `CompiledFrameGraph`'s analytical `AliasingGroup` is the source of truth for reuse opportunity; the allocator consumes it without modifying it. Future analytical extensions (multi-queue, WAR / WAW, async compute) propagate naturally to the allocator's pool-key shape.
- **`unsafe_code = "forbid"` is preserved.** Trust-wgpu-hazard-tracking (D6) and safe descriptor types (D1) together mean dispatch 119 / 120 / 121 require zero `unsafe` blocks. Workspace pledge intact.
- **Cross-frame stability via ring-buffered pools (D4) matches PLAN §8 verbatim.** `PLAN.md:999` names "transient resources (aliased VRAM)" as a renderer responsibility; the ring-buffered policy realises this with descriptor-keyed pools that amortise allocation across frames.
- **Dispatch 119's smallest-first scope (D7) preserves substrate semantics-first discipline.** Mirrors ADR-115's "5-line stub stays; phase-1 dispatch references this ADR" pattern and ADR-117's "design ADR, then implementation dispatch" pattern.

### Negative / risks

- **Initial N=2 frames-in-flight may need adjustment.** wgpu's submission-retire fence behavior on different backends (Vulkan / Metal / DX12) may surface a need for N=3 on platforms with deeper pipelines. The ring-buffered shape extends linearly; the trigger for amendment is fence-wait measurement on the 60fps simple-scene gate.
- **Memory implications: peak resident ≈ N × Σ_groups(MaxDescriptorSizeInGroup).** For a 4-pass frame with one 4K depth-buffer aliasing group, one 4K color-buffer group, and one 1K shadow-map group, N=2 gives peak ≈ 2 × (4K depth + 4K color + 1K shadow) ≈ ~256 MB. Acceptable for the simple-scene gate; revisited at the bindless / virtualized-geometry tier (Phase 7+, NOT this chapter).
- **wgpu barrier inserter may over-barrier.** Documented risk per D6; the amendment trigger is empirical measurement, not speculation.
- **Pool fragmentation across many distinct descriptor shapes is unbounded.** A frame that uses 100 unique texture descriptors produces 100 pool slots per frame-in-flight. The mitigation is descriptor normalization (e.g. dimension rounding to standard sizes) which is OUT OF SCOPE for v0 — the substrate ships honest fragmentation, and a future amendment may add normalization if a real workload surfaces it.
- **Cross-thread allocator behavior is unaddressed.** This ADR is single-threaded execution by default (the renderer runs inline on `WindowEvent::RedrawRequested` per ADR-117 non-decision #2). Cross-thread allocator behavior (Send / Sync bounds on `TexturePool`, lock-free pool access patterns, etc.) defers to the renderer-thread spawn ADR — sibling to ADR-117, anticipated when sim/render thread split lands.

### Mitigations

- **The substrate's deterministic structural-hash discipline carries through.** The `CompiledFrameGraph::structural_hash` proves the analytical layer is byte-identical across compiles; the allocator's pool-state is per-frame transient and does NOT factor into the hash. Cross-frame determinism remains a property of the *graph* (the source of truth), not the *allocator* (the consumer of the graph).
- **wgpu validation layer is the safety net for hazard correctness.** If D6's "trust wgpu" turns out to mask a real hazard (e.g. an analytical AliasingGroup that the allocator reuses across a barrier the analytical layer didn't model), wgpu's validation will fire — visible as a `LOG_LEVEL=WARN` from wgpu, not as silent corruption.
- **The single-threaded execution boundary is preserved.** This ADR composes with ADR-117 — both ADRs operate in single-threaded execution today, with the cross-thread future deferred to a renderer-thread spawn ADR. Neither ADR introduces threading.

## Explicit non-decisions

This ADR deliberately does NOT decide:

1. **Cross-thread allocator behavior.** Send / Sync bounds, lock-free pool patterns, cross-thread resource handoff — defer to the renderer-thread spawn ADR (anticipated when sim/render thread split lands). Today's single-threaded renderer makes this orthogonal; pinning a cross-thread pool shape speculatively would constrain future design space without empirical pressure.
2. **Dynamic resource creation (non-transient resources).** Persistent textures (e.g. material base-colour textures, environment maps), persistent buffers (e.g. mesh vertex/index buffers, transform UBOs), persistent samplers — all of these live OUTSIDE the transient-resource allocator. They are allocated once per asset-load via the existing `crates/gfx/src/{buffer,mesh,material}.rs` surface and the allocator does not touch them. The transient-vs-persistent distinction is the chapter's load-bearing scope boundary.
3. **VRAM aliasing primitives (placed-resource / VK_KHR_dedicated_allocation).** wgpu 29 does not expose memory placement primitives; the trust-wgpu-hazard-tracking decision (D6) implies trusting wgpu's resource model end-to-end. A future amendment may revisit if wgpu surfaces placed-resource APIs and a real workload demonstrates need.
4. **Specific shader / pipeline integration with `FrameRecorder` / `LitMeshPipeline`.** That is `gfx::intent_adapter`'s and `record_lit_mesh_pass`'s lane; the allocator hands physical handles to those layers, and they orchestrate the per-pass recording. Dispatch 121 implements this wiring; this ADR does not pin it.
5. **`label` field on descriptors.** Diagnostic labels per allocation are dispatch 119's concrete-API call. Excluded from the descriptor type so pool-key matching does not over-fragment.
6. **`mapped_at_creation` on buffer descriptors.** Transient resources never CPU-map at creation; CPU-mapping belongs to staging-buffer workflows (outside this allocator's lane).
7. **Multi-queue / async compute scheduling.** Single-queue model per the frame-graph substrate's `# NON-GOALS` declaration; multi-queue allocation policy defers to a future amendment.
8. **WAR / WAW dependency tracking for allocator hazard awareness.** Substrate-level non-goal per `frame_graph::mod.rs:35`; the allocator inherits this constraint and does NOT attempt to derive WAR / WAW barriers from `AliasingGroup` metadata.
9. **External-input resources (e.g. swapchain back-buffer as a frame-graph resource).** Substrate-level non-goal per `frame_graph::mod.rs:39`; the allocator does NOT manage swapchain integration. That is dispatch 121+'s concern when `FrameRecorder` integration lands.
10. **Determinism of pool free-list ordering.** The pool's internal free-list ordering is an implementation detail of dispatch 120; the ADR pins descriptor identity and reuse contracts, NOT the free-list's data structure choice.

## Future work

- **Dispatch 119 — `ResourceDescriptor` types + ResourceId-to-descriptor flow.** Per D7. Footprint ~150-250 LOC net; no `Cargo.toml` change; no new dep; no architecture-lint touch. Lands `TextureDescriptor` + `BufferDescriptor` + descriptor-flow through `add_pass` / `compile`. NEW unit tests in `frame_graph::descriptor` + extension to `frame_graph_smoke.rs`. ~3 new tests.
- **Dispatch 120 — `TexturePool` MVP per D3 / D4 / D5; dispatch 121 — `BufferPool` MVP (clean mirror).** NEW `crates/gfx/src/frame_graph/{texture_pool,buffer_pool}.rs`; per-frame-in-flight pool instances; descriptor-keyed reuse with aliasing-group identity; greedy max-size policy. Footprint ~250-400 LOC each including unit tests covering pool reuse + ring-buffer rotation + descriptor matching. NO `wgpu::Device` actually called in unit tests (mock the descriptor → physical-handle surface); integration smoke covers real wgpu pool behavior.
- **Dispatch 121 — `FrameRecorder` integration.** Wires the pools to the existing `record_lit_mesh_pass` surface; pass-recording consumes `(ResourceId, PassContext)` and the allocator resolves `ResourceId → wgpu::TextureView / wgpu::BufferBinding` per the pre-frame allocation cycle. Footprint ~200-300 LOC including a real GPU integration test against `ctx_or_skip!`-style availability.
- **Phase 6 umbrella exit gate.** The 60 fps simple-scene golden gate at `IMPLEMENTATION.md:468` becomes the chapter's exit criterion once dispatch 121 lands. The frame-graph allocator + ADR-117 handoff together make this measurable.
- **Renderer-thread spawn ADR (sibling).** When sim/render thread split lands, a sibling ADR codifies cross-thread allocator behavior (Send bounds, lock-free pool access, cross-thread resource handoff). This ADR's single-threaded execution model is preserved; the renderer-thread ADR layers cross-thread semantics on top without re-litigating descriptor / pool shape.
- **WorldSnapshot wire format ADR (sibling).** ADR-117 anticipates this; it covers what fields beyond `editor_camera` populate the render-side snapshot. The allocator pinned here is orthogonal — `RenderInputOwned` is the sim → render boundary; the allocator is the render-thread-local GPU substrate downstream of the boundary.
- **Audit trigger: wgpu barrier hotspot.** If a profiling pass on the 60 fps gate surfaces wgpu auto-tracked barriers as a hotspot, an amendment to D6 introduces manual barrier emission for the affected pass class.

## References

- **`IMPLEMENTATION.md:431`** — frame-graph minimal requirement: "transient resource lifetimes computed at frame begin".
- **`PLAN.md:617`** — graph-foundation adoption table: render owns "frame-graph compile, transient resource alloc".
- **`PLAN.md:999`** — renderer architecture brief: "transient resources (aliased VRAM)".
- **`docs/§18/GFX_RENDER_TIER.md:24`** — Phase-6 substrate incremental list: "TexturePool / BufferPool keyed on frame index".
- **`docs/§18/GFX_RENDER_TIER.md:234`** — Phase-6 frame-graph elaboration: "allocator pressure stabilises".
- **`crates/gfx/src/frame_graph/mod.rs`** — analytical substrate root; module-doc NON-GOAL list pins "No GPU resource allocation" deferral.
- **`crates/gfx/src/frame_graph/compile.rs`** — `CompiledFrameGraph` + `ResourceLifetime` + `AliasingGroup` — the analytical contracts D3 / D5 consume.
- **`crates/gfx/src/frame_graph/resource.rs`** — opaque `ResourceId` substrate D1's descriptor extends.
- **`crates/gfx/src/frame_graph/pass.rs`** — `PassNode` surface D7's dispatch-119 descriptor-flow extends.
- **`crates/gfx/tests/frame_graph_smoke.rs`** — analytical smoke that D7's dispatch 119 extends with descriptor-flow assertions.
- **ADR-114** — PluginContext owned-handoff (owned-cross-boundary substrate precedent; the `Send`-bound discipline carries through to dispatch 120's `TexturePool: Send`).
- **ADR-115** — graph-metrics substrate design (semantics-first, implementation-deferred precedent; this ADR's section structure mirrors §1 / §2 / §3).
- **ADR-117** — render-handoff mechanism (sibling design ADR; both ADRs bound the Phase 6 chapter; both defer cross-thread to a future renderer-thread spawn ADR).
- **`docs/architecture/SCENE_EXTRACTION_CONTRACT.md`** §3 — renderer NEVER owns authoritative geometry; the transient allocator's lane is downstream-only.
- **`docs/architecture/REACTIVE_INVALIDATION.md`** §1 — Layer 4 GPU-upload boundary; transient resources are per-frame downstream products of the projection pipeline.
- **`docs/§18/GFX_RENDER_TIER.md`** §11 — pending Phase 6 work catalog; this ADR closes the frame-graph item.
- **`docs/§18/GFX_RENDER_TIER.md`** §12 — failure-class declaration "recoverable"; transient-allocator failures inherit this class (pool exhaustion → fallback to fresh allocation, NOT hard fail).
