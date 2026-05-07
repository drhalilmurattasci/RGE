# NON_GOALS

| Companion to | PLAN.md §0.2 (the moat statement — current scope), §0.6 (architecture freeze policy — defers everything past v0.8), §1.9 (Non-goals until v2 — partial; this doc extends with current-scope-boundary framing); IMPLEMENTATION.md (phase order); the 2026-05-10 ChatGPT cross-review #1 archive in `change.md` (the architectural repositioning that motivated this doc) |
|---|---|
| Status | Doctrine-tier v0; binding architectural rule. The non-goals enumerated here are commitments under the §0.6 freeze policy: each is deferred (not rejected forever), but expansion beyond the boundary requires demonstrated implementation pressure per §0.6's four conditions. |
| Audience | Reviewers deciding whether a proposed feature crosses the scope boundary; future ADR authors proposing new first-class subsystems; prospective contributors needing the explicit "this is what RGE is, this is what RGE is NOT". |
| Sibling docs | `docs/architecture/REACTIVE_INVALIDATION.md` (the doctrine inside the boundary), `docs/architecture/SCENE_EXTRACTION_CONTRACT.md` (the contract inside the boundary), `docs/§18/EXECUTION_DOMAINS.md` (4-domain commitment per ADR-099; new domains require ADR), `docs/§18/RECOVERY_MODEL.md` (failure containment within current scope) |
| Reference impls | PLAN.md §0.2 — the moat statement that defines current scope; PLAN.md §0.6 — the freeze policy that gates expansion; PLAN.md §1.9 — non-goals-until-v2 partial table (this doc extends); 2026-05-10 ChatGPT cross-review #1 archive in `change.md` lines 992-1007 — competitive positioning; IMPLEMENTATION.md phase order — the deferred items |

> *What RGE is NOT, currently, by design. Doctrine-tier doc — binding architectural rule, not a substrate reference. The intent is to prevent architectural sprawl by making scope choices explicit, not to permanently exclude any of these directions.*

## 1. Current scope

The cross-review #2 2026-05-10 archive in `change.md` reframes RGE as "**deterministic reactive CAD runtime with engine characteristics**" rather than "game engine with CAD features". The competitive frame is Houdini / Siemens Parasolid / Autodesk Fusion / Blender Geometry Nodes / Open Cascade Technology — "but with deterministic replay + ECS-native orchestration + topology lineage + semantic constraints + pluginized kernels". The frame is NOT Bevy or Fyrox; the engine substrate is shared (ECS + plugins) but the moat is CAD-native first-class B-Rep with the four-pillar discipline of PLAN §0.1.

This positioning matters for non-goals. A "game engine with CAD features" might add general-purpose AAA renderer support, audio mixing for game-quality, or a combat-physics product. A "deterministic reactive CAD runtime" doesn't — those features are out of scope unless there is demonstrated implementation pressure under §0.6.

## 2. Explicit non-goals

Each non-goal below is a commitment for v1.0 (Phase 4-Foundation through 5-Scale per IMPLEMENTATION.md). Crossing any boundary requires the §0.6 four-condition gate.

### 2.1 NOT a general-purpose game engine

RGE shares ECS + plugin pattern with Bevy / Fyrox / Godot but is NOT API-compatible with any of them. Not in scope for v1:

- **AAA-class renderer** — PLAN §0.4 commits to "industrial-quality realtime editor, not photoreal" as the v1.0 floor; UE5-class photoreal parity is post-v2 per the §1.9 reach-product table.
- **Audio mixing for game quality** — `crates/audio` ships as a substrate (Kira-backed) for editor canary purposes; the v1.0 floor is editor audio playback + diagnostics, not the spatialised game-quality mixer typical of game engines.
- **Physics-for-fun-gameplay** — `crates/physics` ships as a Rapier wrapper for the v1.0 floor (rigid bodies + character controller); vehicle physics is a 4-Polish stretch per §1.9, soft bodies / cloth is 5-Scale, and the substrate is canary-class deterministic — NOT a competitor to PhysX or Havok for AAA gameplay.
- **Animation studio** — `anim-graph` + `anim-clip` + `anim-ik` + `anim-retarget` ship per PLAN §6.11 with humanoid-only retargeting at v1.0; free-form retargeting is post-v1; DQS skinning is reach product.

The constraint is the four-pillar discipline (PLAN §0.1): native, multi-platform, realtime, fastest script engine. Within the four pillars there is no AAA-renderer pillar; the moat is CAD-native first-class B-Rep, not "shipping-quality game engine renderer". This is what justifies excluding entire game-engine subsystems from v1 even when their underlying substrate (rapier, kira, wgpu) is workspace-resident.

### 2.2 NOT a competitor to traditional CAD authoring tools at v1

The cad-core substrate is the moat (PLAN §0.2); the productized authoring environment is NOT v1 scope. Not in scope:

- **Parametric sketcher UI** — cad-core ships the operator graph + 5 operator catalog (Cuboid + Transform + Extrude + Revolve + Boolean per HANDOFF.md "Phase 7 operator catalog"); the editor-side sketcher panel that lets users build sketches by drawing constraints is not v1.
- **Constraint-solver UI** — cad-core declares the constraint module and persistent IDs survive constraint changes per §1.5.4.3; the interactive constraint manipulation surface is not v1.
- **DRC / DFM** — design-rule-check / design-for-manufacturing surfaces are entirely post-v1.
- **PMI** — product manufacturing information layer (3D dimensions / GD&T / annotations) is post-v1.
- **STEP / IGES round-trip fidelity** — `io-step` is a stub crate; the deferral is documented per §1.6.5 + IMPLEMENTATION.md phase order. STEP-fidelity sewing is OCCT-only per PLAN §1.5.4.4 doctrine, and the OCCT kernel itself ships as the `cad-occt` stub crate per the freeze policy.
- **Drawing / 2D-projection generation** — the "make me a drafting view" feature typical of Solidworks / Fusion / Inventor is post-v1.
- **Assembly mate-and-constraint UI** — interactive assembly mating is post-v1; the substrate (lineage + persistent IDs) makes it tractable but the UI is not in scope.
- **What v1 IS** — substrate + canary plugins (per the 4-canary multi-canary integration test post-round-6); a productized authoring environment lands incrementally on top of that substrate.

### 2.3 NOT a multi-user / distributed / cloud runtime today

PLAN §1.6 anticipates determinism-stable distributed execution eventually; current scope is single-process. Not in scope:

- **Authoritative-server CAD live multiplayer** — the conceptual scaffolding ships at v1.0 per PLAN §6.17 (zero-cost markers `Replicated` / `NetworkOwner(PeerId)` / `Authoritative` / etc.) precisely so the retrofit is not painful, but the actual networking impl is Phase 5-Scale work.
- **Cloud orchestration** — the cross-review's "future cloud orchestration" framing is post-v1; the substrate (deterministic replay + content-addressed assets + lineage) is the precondition, but the orchestration layer itself is not v1.
- **Lockstep-Stable cross-machine** — requires soft-floats + bit-identical math libraries per PLAN §1.6.8; deferred to Phase 5-Scale.
- **Replicated topology state** — explicitly REJECTED in v0.7 per PLAN §1.9 line 487 "replaced by authoritative CAD serialization (§6.17) — much narrower"; the per-peer divergence problem is fundamental, and the model is "authoritative operation graph + per-peer rebuild + reconciliation" not "shared topology".

### 2.4 NOT an AI-native CAD platform

The cross-review notes "future AI-assisted CAD repair" as a long-horizon possibility; explicitly NOT v1. Not in scope:

- **AI-assisted operator suggestion** — no LLM in the editor for v1.
- **AI-driven topology repair** — ADR-115 §"Followups" defers AI-related metrics; the `recovery/` module's probabilistic-recovery-confidence metric per ADR-115 phase-7 is the substrate that would feed an AI repair surface, but the repair surface itself is post-v1.
- **AI scene generation** — entirely post-v1.
- **AI-assisted constraint solving** — post-v1.

The v1 substrate (lineage + persistent IDs + deterministic replay) is the precondition for any future AI work; locking AI features out of v1 is what keeps the substrate's design clean.

### 2.5 NOT a procedural-content-generation platform

The cross-review identifies Houdini Geometry Nodes as a competitor frame; RGE's reactive substrate could grow into procedural-PCG but v1 is CAD authoring, not procedural-PCG. Not in scope:

- **Visual scripting graphs that generate scene content** — `script-graph` ships as a stub (per PLAN §10.1 + IMPLEMENTATION.md phase order); a full visual-scripting authoring environment is not v1.
- **Geometry-node-style procedural authoring** — the ADR-115 cross-review explicitly notes this future; v1 ships the reactive substrate, not the procedural-graph editor.
- **Hair / vegetation / crowd procedural generation** — entirely post-v1.

### 2.6 NOT a simulation platform v1

A physics canary exists for determinism-gate purposes (the `cross_substrate_determinism::pie_three_participant_round_trip_50_iter` test is the 3-way PIE byte-identity that the cross-review flagged as the strongest round-6 architectural success), NOT as a simulation product. Not in scope:

- **CFD / FEA / structural-analysis surfaces** — entirely post-v1.
- **Discrete-event simulation** — not v1.
- **Multi-body dynamics for analysis** — Rapier rigid bodies ship for editor / canary purposes; the analysis-grade simulation product is not v1.

### 2.7 NOT a marketplace at v1

PLAN §0.6 freeze policy + the marketplace + marketplace-server stub crates explicitly defer indefinitely. Not in scope:

- **Plugin marketplace** — `marketplace` and `marketplace-server` are stub crates per the failure-class rollout-debt registry (HANDOFF.md "36 exemptions left in place — all verified empty stubs … 31 Tier-2 crates (… marketplace / marketplace-server …)").
- **Plugin signing infrastructure** — Ed25519 over manifest + WASM bytecode is the reach-product framing per PLAN §0.4 and §9; CI infrastructure for the v1 floor exists but the marketplace governance product is not v1.
- **Trust levels / revocation infrastructure** — same.
- **Plugin discovery / search / rating** — `plugin-discovery` is a stub crate; entirely post-v1.
- **OIDC author identity** — the §9.1 description includes OIDC; the substrate exists at the design level (capability-gated WASM Tier-3 plugins per PLAN §1.1) but the OIDC integration is post-v1.

### 2.8 NOT a fork-of-Bevy / fork-of-Fyrox

The cross-review frames the engine substrate (ECS + plugin pattern) as shared-shape with Bevy / Fyrox; RGE is NOT API-compatible with either. Not in scope:

- **Bevy plugin compatibility** — Bevy plugins do not load into RGE; RGE plugins use the kernel/plugin-host substrate per ADR-114.
- **Fyrox scene compatibility** — Fyrox `.rgs` scenes do not load into RGE; RGE source format is `.rge-scene` (RON) per PLAN §1.6.
- **Direct Bevy ECS API parity** — RGE's `kernel/ecs` is its own substrate (specialized relation storage per PLAN §1.2); the surface area diverges.

The differentiator is CAD-native first-class B-Rep — a load-bearing architectural commitment that no game engine substrate has, and which makes API parity with game engines counter-productive.

## 3. Anti-sprawl criteria for future scope additions

PLAN §0.6's four-condition gate is the doctrine; the criteria are:

- **Demonstrated architectural collision** — at least one failure case observed in code, not forecast from analogy with other tools. Precedent: v0.6 cad-core split (responding to demonstrated CAD/ECS impedance); v0.7 graph-foundation (responding to demonstrated graph-domain fragmentation across material / anim / script / cad / render); both responses to demonstrated pressure that produced concrete architectural collisions.
- **Forecasted failure mode crossing the §1.15-style threshold** — v0.8 editor-state was the first round responding to a *forecasted* failure mode without implementation evidence. PLAN §0.6 line 130 explicitly notes "That's the line — past v0.8, only demonstrated pressure justifies new subsystems."
- **Past v0.8** — additional first-class subsystems require ALL FOUR §0.6 conditions: implementation pressure + 3+ reproducer failure scenarios + cost/benefit vs alternatives + justification why a smaller primitive wouldn't suffice.

The doctrine's reviewer rule: "abstraction addiction — every observed risk gets a subsystem" is named explicitly as the meta-risk in PLAN §14 line 1257. The non-goals list above is the discipline that prevents the meta-risk from materialising.

### 3.1 The three response classes

When a new architectural pressure is observed, the candidate responses (in order of preference):

1. **No structural change.** Add a doc-block, a test, an existing-substrate consumer; no new crate, no new lint, no new module. Preferred default; lowest risk.
2. **Existing-crate extension.** Add a module to an existing crate; add a method to an existing trait; expand an existing test. Lower than promotion to first-class.
3. **First-class promotion.** New crate / new substrate / new lint. Highest cost; requires §0.6 four-condition gate.

The pattern is documented (PLAN §0.6 + §1.15) and the precedent shows it works: the v0.5–v0.8 minor-version-bump history is a sequence of (1)/(2)/(3) responses graduated by demonstrated pressure. The non-goals list is the visible artifact of the (1)+(2) responses to date.

## 4. What was deferred and why

Concrete enumeration of currently-deferred items, each pinned to its source-truth.

- **36 stub crates** — Tier-2 + Tier-1 stubs per HANDOFF.md "36 exemptions left in place — all verified empty stubs". The list: `anim-*` / `asset-pipeline` / `brep-render` / `build-pipeline` / `cad-native` / `cad-occt` / `errors` / `gfx-ir` / `hot-reload-watcher` / `input-gestures` / `io-{audio,obj,step,stl}` / `marketplace` / `marketplace-server` / `material-{graph,graph-editor,runtime}` / `math` / `physics-debug` / `plugin-discovery` / `replication` / `resources` / `script-{aot,graph}` (31 Tier-2) + `kernel/{shared, asset-view, asset-streaming, io-scheduler, job-system}` (5 Tier-1). Each lib.rs is verbatim `//! \`rge-<name>\` — stub crate. Architecture frozen at v0.8; implementation pending per IMPLEMENTATION.md.`
- **ADR-099 / ADR-101 / ADR-102** — formal ADR creation deferred per §18 companion-doc-suffices framing per PLAN §1.14 line 630 footnote. The companion docs (`EXECUTION_DOMAINS.md` for ADR-099, `GRAPH_FOUNDATION.md` for ADR-101, `RECOVERY_MODEL.md` for ADR-102) suffice until decision pressure exceeds what they capture.
- **WASM cold-start re-validation** — measured at 904µs on wasmtime 23 per Status.md line 104; not re-validated post bump to wasmtime 44. Tracked as Status.md "Waiting" item; not blocking v1.0 but flagged for a future re-baseline dispatch.
- **ui-theme + editor-ui missing-docs** — ~130 warnings per Status.md line 105; deferred to v0.0.1 docs pass per the same row. Not actionable until then; not blocking.
- **AssemblyScript decision** — gated on ≥10 community requests per ADR-068 (referenced in PLAN §1.9 line 477 "AssemblyScript at v1.0 launch | deferred | demand-gated"). The escape clause exists; the trigger has not fired.
- **`io-3mf` crate** — entirely missing per Status.md line 104; PLAN §1.6.5 lists it among the format-handler crates; deferred per freeze policy until format-handler implementation pressure surfaces.
- **C1 graph-metrics phase-1 implementation** — ADR-115 landed 2026-05-10 with the binding architectural decisions; phase-1 (Tier A counters: `node_count` / `edge_count` / `operator_count` / `constraint_count` / `invalidation_count`) is bounded + small + ready-to-execute, but NOT yet executed. Implementation work is blocked only on a "go" signal, not on design.
- **H5 canary accessor symmetry** — design decision deferred per HANDOFF.md "round-6 deferred items"; implement OR remove from gfx/physics/audio.
- **M2 editor-ui::Plugin canary** — defer until editor-ui Phase 5 stabilisation.
- **Reach product features per PLAN §0.4** — Lumen-equivalent / VSM / TSR (selective) / DQS skinning / compute-shader skinning / free-form retargeting / Tier-1 platform previews (iOS / Android / web) / multi-monitor workspaces / sub-graph composition / full theme editor / script debugger.

## 5. Why explicit non-goals matter

The cross-review's framing in `change.md` 2026-05-10 02:00 archive: "explicitly defining non-goals / current scope boundaries / deferred ambitions helps prevent architectural sprawl." Three concrete reasons:

- **Reviewer guidance.** When a proposed PR or ADR adds a feature, the non-goals list lets the reviewer answer "is this in scope?" without re-deriving the moat statement and the §0.6 freeze conditions every time. The doctrine pre-loads the answer.
- **Contributor expectations.** Prospective contributors reading the workspace need the "this is what RGE is, this is what RGE is NOT" delineation upfront. Without it, every contributor invents their own model of the project and the surface area drifts as community-PRs accumulate.
- **Scope-boundary integrity.** The §0.6 four-condition gate is the gate; this doc is the LIST of currently-gated items. Without the list, the gate is theoretical; with the list, every gated item is named and its deferral rationale is captured.

The four-condition gate + the non-goals list + the architecture-lints + the §1.10.4 entropy metrics + the Rhai-test (PLAN §0.3 principle 7 + §1.4) compose into the discipline that makes "Unreal-scale architecture with indie staffing" tractable per PLAN §15 closing paragraph.

### 5.1 Re-evaluation cadence

The non-goals list is not permanent. PLAN §0.5 review cadence applies: at every minor version bump, the non-goals list is re-evaluated against demonstrated pressure. Items move OFF the list when the §0.6 gate fires (precedent: v0.7 graph-foundation was on no-goal-list-equivalent until graph-domain fragmentation produced demonstrated collision). Items move ON the list when newly-considered features fail the gate.

The cadence is what keeps the doc honest: a stale non-goals list that doesn't reflect the workspace's actual scope is worse than no list. Each minor-version-bump dispatch's HANDOFF.md commit includes a non-goals-list audit per the v0.5+ review precedent.

### 5.2 Workspace state pinning

The non-goals list is grounded in the workspace's verifiable state today. The cumulative pin per HANDOFF.md 2026-05-10:

- **94 workspace members; 43 IMPLEMENTED / 3 PARTIAL / 48 EMPTY-STUB** — the EMPTY-STUB count IS the non-goals enumeration in concrete form.
- **Tier 1 kernel: 10 of 15 implemented** — 5 stubs (`shared`, `asset-view`, `asset-streaming`, `io-scheduler`, `job-system`) on the deferral list.
- **Tier 2: ~32 implemented / 30+ stub** — the stub count grounds non-goals 2.6 / 2.7.
- **7 ADRs landed + 3 deferred** — the deferred ADRs (099 / 101 / 102) are NOT non-goals; they are companion-doc-suffices deferrals with materialization triggers documented.
- **§18 companion docs: 27 of 27 landed** — substrate documentation is in scope; doctrine docs (this doc + the two siblings) ARE the new tier introduced by this dispatch.

The list grounds the doctrine in implementation reality. A non-goal that the workspace already partially implements is misclassified; a non-goal whose corresponding stub crate is empty is correctly classified.

### 5.3 Reviewer rejection-response template

When a PR proposes a feature crossing the boundary, the reviewer's response template:

> *This proposal adds [feature] which crosses the §X.Y non-goal boundary documented in `docs/architecture/NON_GOALS.md`. Per PLAN §0.6, expansion requires (1) demonstrated implementation pressure, (2) 3+ reproducer failure scenarios, (3) cost/benefit vs alternatives, (4) justification why a smaller primitive wouldn't suffice. The proposal does not currently include [missing condition]. Suggested next step: [add doc-block / extend existing crate / open ADR with implementation evidence].*

The template makes the gate visible without making it adversarial. The boundary is mechanical, not personal — every contributor's PR meets the same conditions; the §0.6 gate applies symmetrically.

## 6. Source / spec inconsistencies

Mirroring the §18-pack honesty discipline (every pack surfaces 4–6; this scope-boundary doc surfaces three).

- **PLAN §1.9 partial coverage**: PLAN §1.9 ("Non-goals until v2") is a 17-row table covering specific-feature-level non-goals (OpenXR / VR / AR; USD import; DLSS / FSR; etc.). This doc is BROADER — it covers subsystem-level scope boundaries (NOT a game engine; NOT an AI-native platform; NOT a marketplace). The two are complementary: §1.9 is the specific-features list; this doc is the subsystem-level positioning. They agree on the items they overlap (e.g. AssemblyScript / photoreal rendering / multi-monitor workspaces).
- **Cross-review reframing vs PLAN §0.2 wording**: PLAN §0.2 line 22 says "Unified CAD-native deterministic WASM-scripted authoring environment". The cross-review's "deterministic reactive CAD runtime with engine characteristics" is a more specific framing emphasising the *runtime* (not just authoring) and the *reactive* (not just deterministic) properties. This doc treats the cross-review framing as authoritative for non-goal positioning because it captures the competitive frame more precisely; PLAN §0.2 is preserved as the moat definition.
- **`replication` crate vs PLAN §6.17 markers**: source-truth at the workspace lists `replication` as a stub crate (per HANDOFF.md "marketplace / marketplace-server / material-{graph,graph-editor,runtime} / math / physics-debug / plugin-discovery / replication / resources / script-{aot,graph}"). PLAN §6.17 calls out reserved components (`Replicated` / `NetworkOwner` / etc.) as zero-cost markers in `crates/components/`. The two are consistent: the markers exist in `crates/components/`; the actual networking impl lives in a `replication` stub deferred to Phase 5-Scale. The non-goal "NOT a multi-user / distributed runtime today" applies to both layers.

## 7. Cross-references

- **PLAN.md §0.1** — four pillars (the in-scope list).
- **PLAN.md §0.2** — moat statement (current-scope definition).
- **PLAN.md §0.3** — engine constitution (eight immutable principles).
- **PLAN.md §0.4** — floor vs reach product (the cut order).
- **PLAN.md §0.6** — architecture freeze policy (the four-condition gate).
- **PLAN.md §1.9** — non-goals until v2 (the specific-features table; this doc extends with subsystem-level positioning).
- **PLAN.md §1.10.4** — architecture entropy metrics (the measurement layer that catches sprawl quantitatively).
- **PLAN.md §1.13** — failure containment model (the in-scope recovery story).
- **PLAN.md §1.14** — graph-foundation substrate (in-scope substrate).
- **PLAN.md §1.15** — editor state coordination (the last architecture commitment before freeze; the precedent for "narrow scope, last commitment").
- **PLAN.md §6.17** — networking; explicit narrow-scope framing.
- **PLAN.md §14** — risk table; "abstraction addiction" + "premature ossification" meta-risks named.
- **PLAN.md §15** — open decisions; the explicitly-locked vs explicitly-open frame.
- **2026-05-10 ChatGPT cross-review #1 archive in `change.md` lines 992-1007** — the architectural repositioning that motivated the cross-review-reframe-as-authoritative decision.
- **2026-05-10 ChatGPT cross-review #2 archive in `change.md` line 1020+** — the C1 graph-metrics design guidance + competitive frame.
- **HANDOFF.md** "What just shipped" history — the v0.6 / v0.7 / v0.8 minor-version-bump precedents demonstrating §0.6's "demonstrated pressure" pattern in action.
- **Status.md "Waiting" section** — the explicit deferral list for currently-deferred items.
- **`docs/§18/EXECUTION_DOMAINS.md`** — 4-domain commitment per ADR-099; new domains require ADR (a specific application of the §0.6 gate).
- **`docs/§18/GRAPH_FOUNDATION.md`** — the substrate that absorbed graph-domain fragmentation pressure (precedent for §0.6's "demonstrated architectural collision" condition).
- **`docs/§18/RECOVERY_MODEL.md`** — the in-scope recovery taxonomy.
- **`docs/architecture/REACTIVE_INVALIDATION.md`** — sibling doctrine doc; the reactive runtime rules INSIDE the boundary.
- **`docs/architecture/SCENE_EXTRACTION_CONTRACT.md`** — sibling doctrine doc; the extraction contract INSIDE the boundary.
- **`tools/architecture-lints/exemptions.toml`** — the failure-class + split-exemption registry that pins which crates are stubs (the "NOT in scope today" enumeration at the lint level).
