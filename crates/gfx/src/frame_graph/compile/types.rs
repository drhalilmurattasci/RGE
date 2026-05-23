use std::collections::BTreeMap;

use rge_kernel_graph_foundation::NodeId;
use serde::{Deserialize, Serialize};

use crate::frame_graph::descriptor::ResourceClassDescriptor;
use crate::frame_graph::resource::ResourceId;

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

impl AliasingGroup {
    /// Returns the largest descriptor in this group by
    /// [`ResourceClassDescriptor::byte_size_estimate`], suitable for pool
    /// acquisition per ADR-118 D5 ("the descriptor with the largest size in
    /// a group governs the physical slot").
    ///
    /// Returns `None` if no member of the group has a descriptor registered
    /// in `descriptors`. Well-formed `CompiledFrameGraph`s produced by
    /// [`FrameGraph::add_pass`](super::super::FrameGraph::add_pass) always have a
    /// descriptor per `ResourceId`, but the helper is robust to partial
    /// maps for diagnostic use.
    #[must_use]
    pub fn max_descriptor<'a>(
        &self,
        descriptors: &'a BTreeMap<ResourceId, ResourceClassDescriptor>,
    ) -> Option<&'a ResourceClassDescriptor> {
        self.0
            .iter()
            .filter_map(|rid| descriptors.get(rid))
            .max_by_key(|d| d.byte_size_estimate())
    }
}

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
/// [`FrameGraph::add_pass`]: super::super::FrameGraph::add_pass
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
    pub(super) fn new(
        execution_order: Vec<NodeId>,
        resource_lifetimes: BTreeMap<ResourceId, ResourceLifetime>,
        aliasing_groups: Vec<AliasingGroup>,
        descriptors: BTreeMap<ResourceId, ResourceClassDescriptor>,
    ) -> Self {
        Self {
            execution_order,
            resource_lifetimes,
            aliasing_groups,
            descriptors,
        }
    }

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
    /// [`FrameGraph::add_pass`](super::super::FrameGraph::add_pass) time. The
    /// downstream allocator (dispatch 120 per ADR-118 D3 / D5) keys its
    /// `TexturePool` / `BufferPool` on `(Descriptor, AliasingGroupId)`
    /// drawn from this map combined with [`Self::aliasing_groups`].
    ///
    /// Resources that appear in [`Self::resource_lifetimes`] always appear
    /// here too — every `ResourceId` flowing through the graph is declared
    /// against exactly one [`ResourceClassDescriptor`] at its write site
    /// (descriptor consistency is validated at compile time via
    /// [`FrameGraphError::DescriptorMismatch`](super::super::FrameGraphError::DescriptorMismatch)).
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
