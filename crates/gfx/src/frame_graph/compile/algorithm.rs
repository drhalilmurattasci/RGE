use std::collections::{BTreeMap, BTreeSet};

use rge_kernel_graph_foundation::NodeId;

use super::error::CompileError;
use super::types::{AliasingGroup, CompiledFrameGraph, ResourceLifetime};
use crate::frame_graph::descriptor::ResourceClassDescriptor;
use crate::frame_graph::pass::PassNode;
use crate::frame_graph::resource::ResourceId;

/// Compile a `BTreeMap<NodeId, PassNode>` into a [`CompiledFrameGraph`].
///
/// `descriptors` is the per-`ResourceId` [`ResourceClassDescriptor`]
/// sidecar collected at
/// [`FrameGraph::add_pass`](super::super::FrameGraph::add_pass) time; the compile
/// step copies it through verbatim. Descriptor consistency is validated
/// at `add_pass` time (see
/// [`FrameGraphError::DescriptorMismatch`](super::super::FrameGraphError::DescriptorMismatch)),
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

    Ok(CompiledFrameGraph::new(
        order,
        lifetimes,
        aliasing_groups,
        descriptors.clone(),
    ))
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

    fn tex_d_size(side: u32) -> ResourceClassDescriptor {
        ResourceClassDescriptor::Texture(crate::frame_graph::descriptor::TextureDescriptor {
            width: side,
            height: side,
            depth_or_array_layers: 1,
            mip_level_count: 1,
            sample_count: 1,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            dimension: wgpu::TextureDimension::D2,
            view_dimension: wgpu::TextureViewDimension::D2,
        })
    }

    fn buf_d_size(bytes: u64) -> ResourceClassDescriptor {
        ResourceClassDescriptor::Buffer(crate::frame_graph::descriptor::BufferDescriptor {
            size_bytes: bytes,
            usage: wgpu::BufferUsages::UNIFORM,
        })
    }

    // AG-MAX-1: textures-only group; largest (4096²) wins.
    #[test]
    fn aliasing_group_max_descriptor_picks_largest_texture() {
        let g = AliasingGroup(vec![r(1), r(2), r(3)]);
        let mut descriptors = BTreeMap::new();
        descriptors.insert(r(1), tex_d_size(1024));
        descriptors.insert(r(2), tex_d_size(4096));
        descriptors.insert(r(3), tex_d_size(2048));
        let max = g
            .max_descriptor(&descriptors)
            .expect("non-empty descriptors");
        assert_eq!(*max, tex_d_size(4096), "max must be the 4096² texture");
    }

    // AG-MAX-2: buffers-only group; largest by size_bytes wins.
    #[test]
    fn aliasing_group_max_descriptor_picks_largest_buffer() {
        let g = AliasingGroup(vec![r(1), r(2), r(3)]);
        let mut descriptors = BTreeMap::new();
        descriptors.insert(r(1), buf_d_size(1024));
        descriptors.insert(r(2), buf_d_size(16_384));
        descriptors.insert(r(3), buf_d_size(4096));
        let max = g
            .max_descriptor(&descriptors)
            .expect("non-empty descriptors");
        assert_eq!(*max, buf_d_size(16_384), "max must be the 16 KiB buffer");
    }

    // AG-MAX-3: empty group → None.
    #[test]
    fn aliasing_group_max_descriptor_returns_none_for_empty_group() {
        let g = AliasingGroup(vec![]);
        let mut descriptors = BTreeMap::new();
        descriptors.insert(r(1), tex_d_size(1024));
        assert!(
            g.max_descriptor(&descriptors).is_none(),
            "empty group has no max descriptor"
        );
    }

    // AG-MAX-4: non-empty group with empty descriptors → None (the
    // precondition that surfaces as
    // ResourceMapError::MissingDescriptorForGroup at the builder layer).
    #[test]
    fn aliasing_group_max_descriptor_returns_none_for_missing_descriptors() {
        let g = AliasingGroup(vec![r(1), r(2)]);
        let descriptors: BTreeMap<ResourceId, ResourceClassDescriptor> = BTreeMap::new();
        assert!(
            g.max_descriptor(&descriptors).is_none(),
            "group with no descriptor entries returns None"
        );
    }
}
