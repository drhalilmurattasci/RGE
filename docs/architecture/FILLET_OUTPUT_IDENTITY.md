# FILLET_OUTPUT_IDENTITY

| Status | **PARKED** — design note awaiting consumer pressure. NOT a doctrine doc; NOT an ADR. |
|---|---|
| Question | Whether topology-changing operators like `FilletOp` should implement output-side `BRepProvider` / `BRepEdgeProvider`. |
| Triggered by | D-Fillet sub-α through sub-δ (2026-05-09): four direct providers (Cuboid + Extrude + Revolve + Loft) consume `BRepEdgeId` for input-side selection; `FilletOp::evaluate` produces a `Tessellation` whose new triangles have no identity scheme. |
| Blocked on | A real consumer of output-side identity. Candidates: `cad-projection` per-triangle face-id queries, editor selection persistence across rebuilds, GFX picking, chained fillets, or Boolean lineage propagation through `topo_lineage`. |
| Today's behavior | `FilletOp` falls into the `_` catch-all in both face and edge resolvers and returns `BRepResolveError::TopologyChangingOperator { kind: OpKind::Fillet }`. Filleted output is identity-opaque. |

## Open question

After D-Fillet sub-α through sub-δ, `FilletOp` accepts `BRepEdgeId`s as
constructor inputs and validates them against the upstream operator's
`BRepEdgeProvider`. The input-side consumer contract is closed across all
four direct providers (Cuboid + Extrude + Revolve + Loft).

But `FilletOp::evaluate` produces a `Tessellation` whose new triangles
(the 2 chamfer-cap triangles per filleted edge) have no identity
attribution. The unmodified upstream triangles also have no IDs in the
projected output today, but they could in principle inherit from the
upstream operator's `BRepProvider`. The new chamfer triangles, by
contrast, have nowhere to inherit from — they are fabricated by the
fillet operation.

The substrate question: **does `FilletOp` (and topology-changing
operators in general) implement output-side `BRepProvider` /
`BRepEdgeProvider`?** If yes, what identity scheme attributes
identities to:

- The unfilleted upstream faces and edges (clearly survivors).
- The filleted edge itself (now consumed by a chamfer face).
- The chamfer face (newly fabricated; no upstream parent).
- The 2 boundary edges of the chamfer face (newly fabricated).
- The adjacent upstream faces (slightly different shape; still "the
  same face" semantically, or different?).

## Candidate dispositions (no decision yet)

The `topo_lineage` substrate (D-7.4 prototype, 2026-05-07) introduced a
`TopologyEvolution` enum: `Preserved` / `Split` / `Merged` / `Deleted`
/ `Reinterpreted`. These are the natural vocabulary for the
dispositions:

- **Unaffected upstream faces and edges** → `Preserved`. The 11
  unfilleted edges of a Cuboid keep their original `BRepEdgeId`. The
  4 unaffected faces keep their original `BRepFaceId`.
- **The filleted edge** → either `Split` (the edge becomes 2 new
  edges — the chamfer face's 2 long sides; the original ID maps to
  both successors) or `Deleted` / `Reinterpreted` (the original ID
  has no successor; downstream consumers see the edge as gone).
  These are different downstream semantics for selection persistence.
- **The chamfer face** → `Reinterpreted` (newly-introduced; no
  upstream parent). What's the `BRepFaceId` derivation? Owner-seeded
  from the fillet operation's own owner? From the filleted edge's
  ID? These are different.
- **Adjacent upstream faces** → likely `Preserved` (still "the +X
  face of the cuboid" semantically), but their boundaries have
  changed. Whether the boundary change matters for identity is the
  decision.

## Why parked

The substrate-doctrine principle is: **no substrate before pressure**.
Today, no substrate consumes output-side identity from `FilletOp`:

- `cad-projection` does not yet propagate per-triangle face IDs
  through projection.
- Editor / selection / picking layers do not yet exist.
- GFX rendering does not consume face IDs.
- Chained fillets work in the operator graph but the resolver
  returns `TopologyChangingOperator` so no consumer can read
  through the chain.
- Boolean lineage is also `TopologyChangingOperator`-classified.

Designing output-side identity now would invent semantics for a
contract that has no current consumer. The designed semantics would
likely diverge from what the eventual real consumer needs.

## Trigger for un-parking

This design note becomes an active substrate dispatch (or an ADR)
when ONE of these consumer pressures lands:

1. **`cad-projection` integration** — when projection wants to answer
   "what stable face/edge does this triangle correspond to?" for
   filleted output, the chamfer triangles need identity.
2. **Editor selection persistence** — when editor selection wants
   to survive across rebuilds, including rebuilds that introduce
   or remove fillets.
3. **GFX picking** — when GFX picks a triangle and asks "what
   semantic face was that?" through filleted output.
4. **Chained fillets** — when `FilletOp::new_for_fillet(&FilletOp, …)`
   becomes a real use case (currently rejected at the resolver
   layer).
5. **Boolean lineage propagation** — when Boolean output identity
   forces the same semantic decisions on a parallel substrate, and
   the question can be answered consistently across both topology-
   changing operator families.

The most likely first trigger is `cad-projection` integration.

## Today's behavior (for callers)

`FilletOp` is a topology-changing operator from the resolver's
perspective. Both `topology::brep_face_ids_for_node` and
`topology::brep_edge_ids_for_node` return:

```rust
Err(BRepResolveError::TopologyChangingOperator {
    kind: OpKind::Fillet,
})
```

when the resolved node is a `Fillet`. Callers who want stable
identity from filleted output need to either:

- Resolve identity at the upstream operator (before the fillet) and
  accept that the chamfer geometry is identity-opaque; OR
- Wait for output-side identity to be designed (this note).

This behavior MUST be preserved until output-side identity is
designed. Adding a `BRepProvider` / `BRepEdgeProvider` impl to
`FilletOp` ahead of the trigger conditions above would invent
semantics that the eventual consumer might not want.

## Companion docs

- `SEMANTIC_ARCHITECTURE_LAWS.md` Section 6 (Identity Continuity) —
  the doctrine-tier framing of identity-survival semantics.
- `topo_lineage::types::TopologyEvolution` — the prototype enum
  that supplies the vocabulary for the candidate dispositions above.
- `cad-core::topology::resolve` and `edge_resolve` — the resolvers
  that today return `TopologyChangingOperator` for `FilletOp`.
- D-7.2 sub-α through sub-ζ.ζ — the input-side substrate that this
  question complements.
- D-Fillet sub-α through sub-δ — the input-side consumer dispatches
  that surfaced this question.
