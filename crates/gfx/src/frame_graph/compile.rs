//! Frame-graph compilation: topological sort + resource-lifetime analysis +
//! transient aliasing groups.
//!
//! Inputs: a `BTreeMap<NodeId, PassNode>` of registered passes.
//!
//! Outputs (the [`CompiledFrameGraph`] payload):
//! 1. **Execution order** — topologically sorted [`NodeId`]s; running the
//!    passes in this order honors all RAW dependencies declared by the
//!    `reads` / `writes` fields.
//! 2. **Resource lifetimes** — for each [`ResourceId`] referenced by any
//!    pass, the index range `[first_use, last_use]` within the execution
//!    order. An eventual GPU allocator (out of scope) uses this to size and
//!    free transient backing storage.
//! 3. **Aliasing groups** — disjoint groups of resources whose lifetimes
//!    do not overlap. Each group's resources may share a single underlying
//!    allocation; this is the central space-saving primitive in transient
//!    resource management.
//!
//! # Algorithm
//!
//! 1. Collect writers per resource (`BTreeMap<ResourceId, Vec<NodeId>>`).
//! 2. Validate every read has at least one writer (else
//!    [`CompileError::UnwrittenResource`]).
//! 3. Build dependency adjacency (RAW only): for each pass P that reads R,
//!    every prior writer of R is a predecessor.
//! 4. Topologically sort via Kahn's algorithm with `BTreeSet` ordering for
//!    deterministic tiebreak (matches `kernel/schedule`'s convention).
//! 5. Walk execution order; for each resource, record `first_use` /
//!    `last_use` indices.
//! 6. Greedy aliasing assignment: iterate resources by [`ResourceId`] order;
//!    place each resource into the first group all of whose members are
//!    non-overlapping; otherwise create a new group.

use std::collections::{BTreeMap, BTreeSet};

use rge_kernel_graph_foundation::NodeId;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::frame_graph::descriptor::ResourceClassDescriptor;
use crate::frame_graph::pass::PassNode;
use crate::frame_graph::resource::ResourceId;

/// Errors raised during compilation.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum CompileError {
    /// Two reachable passes form a cycle through their RAW dependencies.
    /// Frame-graphs are DAGs by construction; a cycle indicates a bug in
    /// the caller's declaration.
    #[error("cycle detected during topological sort")]
    Cycle,
    /// A pass declared a read of a resource never written by any other
    /// pass. Frame-graph v0 has no "external input" tier; every resource
    /// must be produced by at least one declared pass.
    #[error("resource {0} read but never written by any pass")]
    UnwrittenResource(ResourceId),
}

/// Lifetime of a resource within an execution order.
///
/// Indices reference [`CompiledFrameGraph::execution_order`]. `first_use`
/// is the index of the earliest pass that reads or writes the resource;
/// `last_use` is the latest. A resource used by exactly one pass has
/// `first_use == last_use`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResourceLifetime {
    /// Index of first pass referencing the resource.
    pub first_use: usize,
    /// Index of last pass referencing the resource.
    pub last_use: usize,
}

impl ResourceLifetime {
    /// True iff the two lifetime ranges share at least one index.
    #[must_use]
    pub fn overlaps(&self, other: &Self) -> bool {
        !(self.last_use < other.first_use || other.last_use < self.first_use)
    }
}

/// A group of resources whose lifetimes are pairwise non-overlapping.
///
/// Members may share a single underlying GPU allocation. Memory savings are
/// proportional to the size of the largest member; the group itself owns no
/// allocation — the eventual allocator (out of scope for v0) consumes
/// `AliasingGroup` to drive backing-buffer reuse.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AliasingGroup(pub Vec<ResourceId>);

/// Compiled (analysed) frame-graph.
///
/// Produced by [`crate::frame_graph::FrameGraph::compile`]. The substrate's
/// public deliverable.
///
/// Carries the per-resource descriptors collected at [`FrameGraph::add_pass`]
/// time so the downstream transient-resource allocator (dispatch 120,
/// `TexturePool` / `BufferPool` per ADR-118 D3 / D5) can compute the
/// "largest descriptor in each aliasing group" from one source of truth.
/// Descriptors are NOT factored into [`Self::structural_hash`] — the
/// analytical layer's determinism is orthogonal to descriptor metadata, so
/// two compiles with the same pass topology but different descriptors
/// still produce equal structural hashes (the analytical substrate
/// remains the source of truth; descriptors are pool-shaping inputs).
///
/// [`FrameGraph::add_pass`]: super::FrameGraph::add_pass
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompiledFrameGraph {
    execution_order: Vec<NodeId>,
    resource_lifetimes: BTreeMap<ResourceId, ResourceLifetime>,
    aliasing_groups: Vec<AliasingGroup>,
    /// Per-resource descriptors collected from each pass's write
    /// declarations. wgpu types are NOT serde-derived in the workspace's
    /// wgpu feature set, so this field is `#[serde(skip)]` and round-trips
    /// as an empty map under serialization — substrate-honest given
    /// dispatch 119 ships no compiled-graph serialization path.
    #[serde(skip)]
    descriptors: BTreeMap<ResourceId, ResourceClassDescriptor>,
}

impl CompiledFrameGraph {
    /// Topologically-sorted pass execution order.
    #[must_use]
    pub fn execution_order(&self) -> &[NodeId] {
        &self.execution_order
    }

    /// Lifetime of a resource, or `None` if the resource was never declared
    /// in any pass.
    #[must_use]
    pub fn resource_lifetime(&self, id: ResourceId) -> Option<ResourceLifetime> {
        self.resource_lifetimes.get(&id).copied()
    }

    /// All aliasing groups (resources whose lifetimes do not overlap).
    #[must_use]
    pub fn aliasing_groups(&self) -> &[AliasingGroup] {
        &self.aliasing_groups
    }

    /// Per-resource descriptors collected at
    /// [`FrameGraph::add_pass`](super::FrameGraph::add_pass) time. The
    /// downstream allocator (dispatch 120 per ADR-118 D3 / D5) keys its
    /// `TexturePool` / `BufferPool` on `(Descriptor, AliasingGroupId)`
    /// drawn from this map combined with [`Self::aliasing_groups`].
    ///
    /// Resources that appear in [`Self::resource_lifetimes`] always appear
    /// here too — every `ResourceId` flowing through the graph is declared
    /// against exactly one [`ResourceClassDescriptor`] at its write site
    /// (descriptor consistency is validated at compile time via
    /// [`FrameGraphError::DescriptorMismatch`](super::FrameGraphError::DescriptorMismatch)).
    #[must_use]
    pub fn descriptors(&self) -> &BTreeMap<ResourceId, ResourceClassDescriptor> {
        &self.descriptors
    }

    /// Descriptor for a specific resource, or `None` if the resource was
    /// never declared in any pass.
    #[must_use]
    pub fn descriptor(&self, id: ResourceId) -> Option<&ResourceClassDescriptor> {
        self.descriptors.get(&id)
    }

    /// Number of compiled passes.
    #[must_use]
    pub fn pass_count(&self) -> usize {
        self.execution_order.len()
    }

    /// 32-byte BLAKE3 hash over `(execution_order || resource_lifetimes ||
    /// aliasing_groups)`. Deterministic — equal inputs produce equal
    /// hashes. Used by the determinism tests in this module and by
    /// `tests/frame_graph_smoke.rs` to assert recompile stability.
    #[must_use]
    pub fn structural_hash(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"CompiledFrameGraph/v1\0");
        for id in &self.execution_order {
            hasher.update(id.to_string().as_bytes());
            hasher.update(b"\0");
        }
        hasher.update(b"\0lifetimes\0");
        for (rid, lt) in &self.resource_lifetimes {
            hasher.update(rid.as_bytes());
            hasher.update(
                &u32::try_from(lt.first_use)
                    .unwrap_or(u32::MAX)
                    .to_le_bytes(),
            );
            hasher.update(&u32::try_from(lt.last_use).unwrap_or(u32::MAX).to_le_bytes());
        }
        hasher.update(b"\0groups\0");
        for group in &self.aliasing_groups {
            hasher.update(b"group\0");
            for rid in &group.0 {
                hasher.update(rid.as_bytes());
            }
        }
        let hash = hasher.finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(hash.as_bytes());
        out
    }
}

/// Compile a `BTreeMap<NodeId, PassNode>` into a [`CompiledFrameGraph`].
///
/// `descriptors` is the per-`ResourceId` [`ResourceClassDescriptor`]
/// sidecar collected at
/// [`FrameGraph::add_pass`](super::FrameGraph::add_pass) time; the compile
/// step copies it through verbatim. Descriptor consistency is validated
/// at `add_pass` time (see
/// [`FrameGraphError::DescriptorMismatch`](super::FrameGraphError::DescriptorMismatch)),
/// so compile assumes the map is already consistent.
///
/// See module-doc for the algorithm.
///
/// # Errors
///
/// - [`CompileError::Cycle`] if the dependency adjacency contains a cycle.
/// - [`CompileError::UnwrittenResource`] if any pass reads a resource that
///   no pass writes.
pub(crate) fn compile_passes(
    passes: &BTreeMap<NodeId, PassNode>,
    descriptors: &BTreeMap<ResourceId, ResourceClassDescriptor>,
) -> Result<CompiledFrameGraph, CompileError> {
    // Step 1: writers map.
    let mut writers: BTreeMap<ResourceId, Vec<NodeId>> = BTreeMap::new();
    for (id, pass) in passes {
        for w in &pass.writes {
            writers.entry(*w).or_default().push(*id);
        }
    }

    // Step 2: validate every read has a writer. (A pass that reads a
    // resource it also writes counts as having a writer — itself.)
    for pass in passes.values() {
        for r in &pass.reads {
            if !writers.contains_key(r) {
                return Err(CompileError::UnwrittenResource(*r));
            }
        }
    }

    // Step 3: dependency adjacency. For each (consumer, R) where consumer
    // reads R, every other writer of R precedes consumer (RAW). Self-loops
    // (consumer writes R AND reads R) are filtered.
    let mut adj: BTreeMap<NodeId, BTreeSet<NodeId>> = BTreeMap::new();
    let mut indeg: BTreeMap<NodeId, u32> = BTreeMap::new();
    for id in passes.keys() {
        adj.insert(*id, BTreeSet::new());
        indeg.insert(*id, 0);
    }
    for (consumer_id, pass) in passes {
        for r in &pass.reads {
            if let Some(writer_list) = writers.get(r) {
                for writer_id in writer_list {
                    if writer_id == consumer_id {
                        continue;
                    }
                    let outgoing = adj.get_mut(writer_id).expect("seeded above");
                    if outgoing.insert(*consumer_id) {
                        let entry = indeg.get_mut(consumer_id).expect("seeded above");
                        *entry = entry.saturating_add(1);
                    }
                }
            }
        }
    }

    // Step 4: Kahn's algorithm with deterministic BTreeSet ordering.
    let mut ready: BTreeSet<NodeId> = indeg
        .iter()
        .filter_map(|(id, d)| (*d == 0).then_some(*id))
        .collect();
    let mut order: Vec<NodeId> = Vec::with_capacity(passes.len());
    while let Some(&id) = ready.iter().next() {
        ready.remove(&id);
        order.push(id);
        let neighbors: Vec<NodeId> = adj[&id].iter().copied().collect();
        for next in neighbors {
            let entry = indeg.get_mut(&next).expect("seeded above");
            *entry = entry.saturating_sub(1);
            if *entry == 0 {
                ready.insert(next);
            }
        }
    }
    if order.len() != passes.len() {
        return Err(CompileError::Cycle);
    }

    // Step 5: resource lifetimes, indexed by execution-order position.
    let mut lifetimes: BTreeMap<ResourceId, ResourceLifetime> = BTreeMap::new();
    for (idx, id) in order.iter().enumerate() {
        let pass = &passes[id];
        for r in pass.reads.iter().chain(pass.writes.iter()) {
            lifetimes
                .entry(*r)
                .and_modify(|lt| {
                    lt.last_use = idx;
                })
                .or_insert(ResourceLifetime {
                    first_use: idx,
                    last_use: idx,
                });
        }
    }

    // Step 6: greedy aliasing assignment, ordered by ResourceId for
    // determinism.
    let mut groups: Vec<Vec<ResourceId>> = Vec::new();
    let mut group_lifetimes: Vec<Vec<ResourceLifetime>> = Vec::new();
    for (rid, lt) in &lifetimes {
        let mut placed = false;
        for (gi, gl) in group_lifetimes.iter().enumerate() {
            if gl.iter().all(|other| !lt.overlaps(other)) {
                groups[gi].push(*rid);
                group_lifetimes[gi].push(*lt);
                placed = true;
                break;
            }
        }
        if !placed {
            groups.push(vec![*rid]);
            group_lifetimes.push(vec![*lt]);
        }
    }
    let aliasing_groups: Vec<AliasingGroup> = groups.into_iter().map(AliasingGroup).collect();

    Ok(CompiledFrameGraph {
        execution_order: order,
        resource_lifetimes: lifetimes,
        aliasing_groups,
        descriptors: descriptors.clone(),
    })
}

#[cfg(test)]
mod tests {
    use rge_kernel_graph_foundation::stable_node_id;

    use super::*;

    fn r(b: u8) -> ResourceId {
        ResourceId::from_bytes([b; 16])
    }

    fn pn(name: &str, reads: Vec<u8>, writes: Vec<u8>) -> PassNode {
        PassNode::new(
            name.to_string(),
            reads.into_iter().map(r).collect(),
            writes.into_iter().map(r).collect(),
        )
    }

    fn build_passes(items: Vec<PassNode>) -> BTreeMap<NodeId, PassNode> {
        items.into_iter().map(|p| (stable_node_id(&p), p)).collect()
    }

    #[test]
    fn empty_compiles_to_empty() {
        let compiled = compile_passes(&BTreeMap::new(), &BTreeMap::new()).expect("compile");
        assert!(compiled.execution_order().is_empty());
        assert!(compiled.aliasing_groups().is_empty());
        assert_eq!(compiled.pass_count(), 0);
    }

    #[test]
    fn single_writer_pass_compiles() {
        let p = pn("solo", vec![], vec![1]);
        let pid = stable_node_id(&p);
        let passes = build_passes(vec![p]);
        let compiled = compile_passes(&passes, &BTreeMap::new()).expect("compile");
        assert_eq!(compiled.execution_order(), &[pid]);
        assert_eq!(compiled.pass_count(), 1);
    }

    #[test]
    fn writer_precedes_reader() {
        let writer = pn("writer", vec![], vec![1]);
        let reader = pn("reader", vec![1], vec![]);
        let wid = stable_node_id(&writer);
        let rid = stable_node_id(&reader);
        let passes = build_passes(vec![writer, reader]);
        let compiled = compile_passes(&passes, &BTreeMap::new()).expect("compile");
        let order = compiled.execution_order();
        let wpos = order
            .iter()
            .position(|n| *n == wid)
            .expect("writer in order");
        let rpos = order
            .iter()
            .position(|n| *n == rid)
            .expect("reader in order");
        assert!(wpos < rpos, "writer must precede reader");
    }

    #[test]
    fn three_pass_chain_orders_transitively() {
        // a writes R1; b reads R1, writes R2; c reads R2.
        let a = pn("a", vec![], vec![1]);
        let b = pn("b", vec![1], vec![2]);
        let c = pn("c", vec![2], vec![]);
        let aid = stable_node_id(&a);
        let bid = stable_node_id(&b);
        let cid = stable_node_id(&c);
        let passes = build_passes(vec![a, b, c]);
        let compiled = compile_passes(&passes, &BTreeMap::new()).expect("compile");
        let order = compiled.execution_order();
        let pa = order.iter().position(|n| *n == aid).unwrap();
        let pb = order.iter().position(|n| *n == bid).unwrap();
        let pc = order.iter().position(|n| *n == cid).unwrap();
        assert!(pa < pb && pb < pc, "transitive RAW order must hold");
    }

    #[test]
    fn cycle_detected() {
        // A writes R1, reads R2. B writes R2, reads R1. RAW cycle.
        let a = pn("a", vec![2], vec![1]);
        let b = pn("b", vec![1], vec![2]);
        let passes = build_passes(vec![a, b]);
        let err = compile_passes(&passes, &BTreeMap::new()).expect_err("expected cycle");
        assert_eq!(err, CompileError::Cycle);
    }

    #[test]
    fn unwritten_resource_detected() {
        let p = pn("reader", vec![1], vec![]);
        let passes = build_passes(vec![p]);
        let err = compile_passes(&passes, &BTreeMap::new()).expect_err("expected unwritten");
        assert_eq!(err, CompileError::UnwrittenResource(r(1)));
    }

    #[test]
    fn lifetime_first_use_is_writer_index() {
        let writer = pn("writer", vec![], vec![1]);
        let reader = pn("reader", vec![1], vec![]);
        let passes = build_passes(vec![writer, reader]);
        let compiled = compile_passes(&passes, &BTreeMap::new()).expect("compile");
        let lt = compiled.resource_lifetime(r(1)).expect("lifetime");
        assert_eq!(lt.first_use, 0);
        assert_eq!(lt.last_use, 1);
    }

    #[test]
    fn lifetime_unknown_resource_returns_none() {
        let writer = pn("writer", vec![], vec![1]);
        let passes = build_passes(vec![writer]);
        let compiled = compile_passes(&passes, &BTreeMap::new()).expect("compile");
        assert!(compiled.resource_lifetime(r(99)).is_none());
    }

    #[test]
    fn lifetime_overlaps_helper() {
        let a = ResourceLifetime {
            first_use: 0,
            last_use: 2,
        };
        let b = ResourceLifetime {
            first_use: 1,
            last_use: 3,
        };
        let c = ResourceLifetime {
            first_use: 4,
            last_use: 5,
        };
        let touch = ResourceLifetime {
            first_use: 2,
            last_use: 4,
        };
        assert!(a.overlaps(&b));
        assert!(b.overlaps(&a));
        assert!(!a.overlaps(&c));
        assert!(!c.overlaps(&a));
        // Lifetimes that share an endpoint count as overlapping (the index
        // at the boundary belongs to both ranges).
        assert!(a.overlaps(&touch));
        assert!(touch.overlaps(&a));
    }

    #[test]
    fn aliasing_groups_combine_non_overlapping_resources() {
        // a writes R1; b reads R1 + writes R3 (chains b after a; ends R1
        // lifetime at index 1). c reads R3 + writes R2; d reads R2.
        // R1 lifetime [0, 1]; R2 lifetime [2, 3]; R3 lifetime [1, 2].
        // R1 and R2 are non-overlapping → can alias.
        let a = pn("a", vec![], vec![1]);
        let b = pn("b", vec![1], vec![3]);
        let c = pn("c", vec![3], vec![2]);
        let d = pn("d", vec![2], vec![]);
        let passes = build_passes(vec![a, b, c, d]);
        let compiled = compile_passes(&passes, &BTreeMap::new()).expect("compile");
        let lt1 = compiled.resource_lifetime(r(1)).unwrap();
        let lt2 = compiled.resource_lifetime(r(2)).unwrap();
        assert!(!lt1.overlaps(&lt2));
        let groups = compiled.aliasing_groups();
        let r1_group = groups
            .iter()
            .find(|g| g.0.contains(&r(1)))
            .expect("r1 in some group");
        assert!(
            r1_group.0.contains(&r(2)),
            "r1 and r2 should share an aliasing group; groups={groups:?}"
        );
    }

    #[test]
    fn aliasing_groups_separate_overlapping_resources() {
        // a writes R1 + R2; b reads R1 + R2.
        // Both R1 and R2 have lifetime [0, 1] — they overlap, so they MUST
        // NOT share an aliasing group.
        let a = pn("a", vec![], vec![1, 2]);
        let b = pn("b", vec![1, 2], vec![]);
        let passes = build_passes(vec![a, b]);
        let compiled = compile_passes(&passes, &BTreeMap::new()).expect("compile");
        let groups = compiled.aliasing_groups();
        let g1 = groups.iter().position(|g| g.0.contains(&r(1))).unwrap();
        let g2 = groups.iter().position(|g| g.0.contains(&r(2))).unwrap();
        assert_ne!(g1, g2, "overlapping resources must occupy distinct groups");
    }

    #[test]
    fn structural_hash_deterministic_across_recompile() {
        let a = pn("a", vec![], vec![1]);
        let b = pn("b", vec![1], vec![]);
        let p1 = build_passes(vec![a.clone(), b.clone()]);
        let p2 = build_passes(vec![a, b]);
        let c1 = compile_passes(&p1, &BTreeMap::new()).expect("c1");
        let c2 = compile_passes(&p2, &BTreeMap::new()).expect("c2");
        assert_eq!(c1.structural_hash(), c2.structural_hash());
    }

    #[test]
    fn structural_hash_changes_with_added_pass() {
        let a = pn("a", vec![], vec![1]);
        let b = pn("b", vec![1], vec![]);
        let c = pn("c", vec![1], vec![2]);
        let p1 = build_passes(vec![a.clone(), b.clone()]);
        let p2 = build_passes(vec![a, b, c]);
        let c1 = compile_passes(&p1, &BTreeMap::new()).expect("c1");
        let c2 = compile_passes(&p2, &BTreeMap::new()).expect("c2");
        assert_ne!(c1.structural_hash(), c2.structural_hash());
    }

    #[test]
    fn self_read_and_write_does_not_create_self_loop() {
        // pass `rw` reads R1 AND writes R1; pass `seed` writes R1 first.
        // RAW edge seed → rw is the only edge; no self-edge on rw.
        let seed = pn("seed", vec![], vec![1]);
        let rw = pn("rw", vec![1], vec![1]);
        let passes = build_passes(vec![seed, rw]);
        let compiled = compile_passes(&passes, &BTreeMap::new()).expect("compile");
        assert_eq!(compiled.pass_count(), 2);
    }

    fn sample_texture_d() -> ResourceClassDescriptor {
        ResourceClassDescriptor::Texture(crate::frame_graph::descriptor::TextureDescriptor {
            width: 256,
            height: 256,
            depth_or_array_layers: 1,
            mip_level_count: 1,
            sample_count: 1,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            dimension: wgpu::TextureDimension::D2,
            view_dimension: wgpu::TextureViewDimension::D2,
        })
    }

    fn sample_buffer_d() -> ResourceClassDescriptor {
        ResourceClassDescriptor::Buffer(crate::frame_graph::descriptor::BufferDescriptor {
            size_bytes: 1024,
            usage: wgpu::BufferUsages::UNIFORM,
        })
    }

    // FG-D1 (compile-side half): descriptors collected at add_pass time
    // round-trip through compile and remain accessible per-ResourceId.
    #[test]
    fn descriptors_round_trip_through_compile() {
        let writer = pn("writer", vec![], vec![1]);
        let reader = pn("reader", vec![1], vec![]);
        let passes = build_passes(vec![writer, reader]);
        let mut descriptors = BTreeMap::new();
        descriptors.insert(r(1), sample_texture_d());
        let compiled = compile_passes(&passes, &descriptors).expect("compile");
        assert_eq!(compiled.descriptors().len(), 1);
        assert_eq!(compiled.descriptor(r(1)), Some(&sample_texture_d()));
        assert!(compiled.descriptor(r(99)).is_none());
    }

    #[test]
    fn descriptors_mixed_texture_and_buffer_round_trip() {
        // Two writers; one declares a texture, the other a buffer.
        let wt = pn("wt", vec![], vec![1]);
        let wb = pn("wb", vec![], vec![2]);
        let rb = pn("rb", vec![1, 2], vec![]);
        let passes = build_passes(vec![wt, wb, rb]);
        let mut descriptors = BTreeMap::new();
        descriptors.insert(r(1), sample_texture_d());
        descriptors.insert(r(2), sample_buffer_d());
        let compiled = compile_passes(&passes, &descriptors).expect("compile");
        assert_eq!(compiled.descriptors().len(), 2);
        assert!(matches!(
            compiled.descriptor(r(1)),
            Some(ResourceClassDescriptor::Texture(_))
        ));
        assert!(matches!(
            compiled.descriptor(r(2)),
            Some(ResourceClassDescriptor::Buffer(_))
        ));
    }

    #[test]
    fn structural_hash_orthogonal_to_descriptors() {
        // Same pass topology with empty vs populated descriptors → same
        // structural hash (descriptors do NOT enter the analytical
        // determinism contract).
        let writer = pn("writer", vec![], vec![1]);
        let reader = pn("reader", vec![1], vec![]);
        let passes = build_passes(vec![writer, reader]);
        let mut descriptors_a = BTreeMap::new();
        descriptors_a.insert(r(1), sample_texture_d());
        let mut descriptors_b = BTreeMap::new();
        descriptors_b.insert(r(1), sample_buffer_d());
        let c1 = compile_passes(&passes, &BTreeMap::new()).expect("c1");
        let c2 = compile_passes(&passes, &descriptors_a).expect("c2");
        let c3 = compile_passes(&passes, &descriptors_b).expect("c3");
        assert_eq!(c1.structural_hash(), c2.structural_hash());
        assert_eq!(c2.structural_hash(), c3.structural_hash());
    }
}
