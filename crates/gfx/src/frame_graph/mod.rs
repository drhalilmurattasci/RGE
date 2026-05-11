//! Frame-graph minimal substrate (Phase 6 fill-in; first dispatch).
//!
//! Failure class: recoverable
//!
//! # What this module IS
//!
//! Vocabulary + ownership boundaries + future-safe seams for a frame-graph:
//!
//! - [`PassNode`] — graph-foundation node payload (wraps `name`, `reads`,
//!   `writes`). Implements [`rge_kernel_graph_foundation::StableHash`] so
//!   `NodeId`s are content-derived (BLAKE3 prefix; mirrors `cad-core`'s
//!   `OperatorGraph` pattern).
//! - [`ResourceId`] — opaque 16-byte caller-supplied identifier (mirrors
//!   `kernel/io-scheduler::IoRequestId`).
//! - [`ResourceUsage`] — `#[non_exhaustive]` enum tagging declarations.
//! - [`FrameGraph`] — wraps `Graph<PassNode, ()>` from
//!   `kernel/graph-foundation`; offers [`add_pass`](FrameGraph::add_pass)
//!   and [`compile`](FrameGraph::compile).
//! - [`CompiledFrameGraph`] — analysis output: execution order +
//!   per-resource lifetimes + transient aliasing groups + a
//!   [`structural_hash`](CompiledFrameGraph::structural_hash) for
//!   determinism testing.
//!
//! # NON-GOALS (load-bearing for v0 scope)
//!
//! - **No GPU resource allocation.** This module produces ordering and
//!   lifetime metadata only; an eventual transient-resource allocator
//!   (out of scope) consumes [`crate::frame_graph::AliasingGroup`] to
//!   size and free transient backing storage.
//! - **No render-snapshot separation.** Phase 6.2 work; separate dispatch.
//! - **No material-runtime / pipeline cache.** Phase 6.3 work; separate
//!   dispatch.
//! - **No 60fps benchmark.** Phase 6 exit gate; substrate-only here.
//! - **No WAR / WAW dependency tracking.** Multiple writers of a single
//!   resource are accepted but produce over-constrained ordering. RAW
//!   (read-after-write) is the only dependency edge v0 derives.
//! - **No async / multi-queue scheduling.** Single-queue model implicit;
//!   queue / family selection is out of scope.
//! - **No external-input resources.** Every read must be matched by at
//!   least one declared write; resources not produced by any pass do
//!   not exist in the model and produce a compile-time error.
//! - **No `FrameRecorder` / pipeline integration.** The substrate stands
//!   alone; integration with `record_lit_mesh_pass` / `MeshPipeline` /
//!   `LitMeshPipeline` lands in a follow-up dispatch.
//! - **No edge payload at runtime.** The underlying graph stores
//!   `Graph<PassNode, ()>`; dependency edges are derived at
//!   [`compile`](FrameGraph::compile) time from `reads` / `writes`
//!   declarations, not materialised in the graph. A typed `EdgeKind` will
//!   land if and when v1 introduces explicit edge construction.
//! - **No new architecture lint, no new ADR, no new doctrine doc, no new
//!   §18 companion.** Routine substrate cavity-shaping per PLAN §0.6
//!   freeze policy.

pub mod compile;
pub mod descriptor;
pub mod pass;
pub mod resource;
pub mod texture_pool;

use std::collections::BTreeMap;

pub use compile::{AliasingGroup, CompileError, CompiledFrameGraph, ResourceLifetime};
pub use descriptor::{BufferDescriptor, ResourceClassDescriptor, TextureDescriptor};
pub use pass::PassNode;
pub use resource::{ResourceId, ResourceUsage};
use rge_kernel_graph_foundation::{stable_node_id, Graph, GraphError, NodeId};
pub use texture_pool::{AliasingGroupId, TexturePool};
use thiserror::Error;

/// Errors raised by [`FrameGraph`] construction or compilation.
#[derive(Debug, Error)]
pub enum FrameGraphError {
    /// Underlying `kernel/graph-foundation` error (e.g. duplicate
    /// content-derived node insertion).
    #[error("graph error: {0}")]
    Graph(#[from] GraphError),
    /// Compile-time error — see [`CompileError`].
    #[error("compile error: {0}")]
    Compile(#[from] CompileError),
    /// Two passes declared writes for the same [`ResourceId`] with
    /// conflicting [`ResourceClassDescriptor`] payloads. The first
    /// declaration is the recorded `first_decl`; the second is
    /// `conflicting`. Per ADR-118 D3, the pool key includes the
    /// descriptor — a single `ResourceId` must therefore route to exactly
    /// one descriptor across the whole frame.
    #[error(
        "descriptor mismatch for resource {resource}: first declared as {first_decl:?}, \
         conflicting later declaration {conflicting:?}"
    )]
    DescriptorMismatch {
        /// The resource whose descriptors collided.
        resource: ResourceId,
        /// First descriptor recorded (from the earlier `add_pass` call).
        first_decl: ResourceClassDescriptor,
        /// Conflicting descriptor seen in a later `add_pass` call.
        conflicting: ResourceClassDescriptor,
    },
}

/// Frame-graph DAG.
///
/// Wraps `Graph<PassNode, ()>` from `kernel/graph-foundation`. Callers add
/// passes via [`Self::add_pass`]; [`Self::compile`] performs topological
/// sort + lifetime + aliasing analysis to produce a [`CompiledFrameGraph`].
///
/// Pass `NodeId`s are content-derived (BLAKE3 prefix over the
/// [`StableHash`](rge_kernel_graph_foundation::StableHash) of `(name,
/// reads, writes)`); two passes with identical content collide on
/// `NodeId`, mirroring the cad-core `OperatorGraph` pattern.
///
/// # Descriptor sidecar (dispatch 119 / ADR-118 D7)
///
/// Each pass's writes carry a [`ResourceClassDescriptor`] declaring the
/// resource's GPU shape. These descriptors live in a sidecar
/// `BTreeMap<ResourceId, ResourceClassDescriptor>` rather than on
/// [`PassNode`] — keeping them off [`PassNode`] preserves the
/// content-derived `NodeId` stability across descriptor variations (two
/// passes with the same topology but different descriptors still collide
/// on `NodeId`, which is the contract for graph-foundation's
/// content-derived identity). Descriptors are validated at `add_pass`
/// time for consistency (every write of a given `ResourceId` must use the
/// same descriptor); a violation surfaces as
/// [`FrameGraphError::DescriptorMismatch`].
#[derive(Debug)]
pub struct FrameGraph {
    inner: Graph<PassNode, ()>,
    /// Per-resource descriptor sidecar — populated lazily as `add_pass`
    /// observes write declarations.
    descriptors: BTreeMap<ResourceId, ResourceClassDescriptor>,
}

impl Default for FrameGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl FrameGraph {
    /// Construct an empty frame-graph.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Graph::new(),
            descriptors: BTreeMap::new(),
        }
    }

    /// Add a pass with the given `name` + `reads` + `writes`. Returns the
    /// new pass's content-derived [`NodeId`].
    ///
    /// Each entry in `writes` is a `(ResourceId, ResourceClassDescriptor)`
    /// tuple. Per ADR-118 D7, descriptors flow into a sidecar map on
    /// [`FrameGraph`] (NOT onto [`PassNode`] — keeping descriptors off
    /// the content-hashed node payload preserves `NodeId` stability under
    /// descriptor variation). The substrate validates that any resource
    /// declared by multiple passes carries the same descriptor across
    /// declarations; conflicting declarations surface as
    /// [`FrameGraphError::DescriptorMismatch`].
    ///
    /// # Errors
    ///
    /// - [`FrameGraphError::Graph`] wrapping
    ///   [`GraphError::DuplicateNode`] if the pass content collides with
    ///   an existing pass (same name + same reads + same write-resource
    ///   list — same content-hash; descriptor payload is NOT part of the
    ///   hash).
    /// - [`FrameGraphError::DescriptorMismatch`] if any write declaration
    ///   contradicts a previously-recorded descriptor for the same
    ///   [`ResourceId`].
    pub fn add_pass(
        &mut self,
        name: impl Into<String>,
        reads: Vec<ResourceId>,
        writes: Vec<(ResourceId, ResourceClassDescriptor)>,
    ) -> Result<NodeId, FrameGraphError> {
        // Validate descriptor consistency BEFORE mutating the sidecar so
        // a mid-vector conflict leaves no partial state behind.
        for (rid, new_desc) in &writes {
            if let Some(existing) = self.descriptors.get(rid) {
                if existing != new_desc {
                    return Err(FrameGraphError::DescriptorMismatch {
                        resource: *rid,
                        first_decl: *existing,
                        conflicting: *new_desc,
                    });
                }
            }
        }
        let write_ids: Vec<ResourceId> = writes.iter().map(|(r, _)| *r).collect();
        let pass = PassNode::new(name.into(), reads, write_ids);
        let id = stable_node_id(&pass);
        self.inner.insert_node(id, pass)?;
        // Insert descriptors AFTER successful node insertion so a
        // duplicate-node failure doesn't pollute the sidecar.
        for (rid, desc) in writes {
            self.descriptors.entry(rid).or_insert(desc);
        }
        Ok(id)
    }

    /// Number of passes currently in the graph.
    #[must_use]
    pub fn pass_count(&self) -> usize {
        self.inner.node_count()
    }

    /// Borrow the per-resource descriptor sidecar collected so far. Used
    /// by tests and by [`Self::compile`]; external consumers normally
    /// reach descriptors through [`CompiledFrameGraph::descriptors`].
    #[must_use]
    pub fn descriptors(&self) -> &BTreeMap<ResourceId, ResourceClassDescriptor> {
        &self.descriptors
    }

    /// Compile the registered passes into a [`CompiledFrameGraph`].
    ///
    /// See [`compile_passes`](compile::compile_passes) for the algorithm.
    /// Compilation does not mutate `self`; the underlying graph stores
    /// only nodes, and dependency adjacency is built in a working copy
    /// during compile. The descriptor sidecar is cloned into the
    /// compiled output verbatim.
    ///
    /// # Errors
    ///
    /// - [`FrameGraphError::Compile`] wrapping [`CompileError::Cycle`]
    ///   if the dependency adjacency contains a cycle.
    /// - [`FrameGraphError::Compile`] wrapping
    ///   [`CompileError::UnwrittenResource`] if any pass reads a resource
    ///   never written by any pass.
    pub fn compile(&self) -> Result<CompiledFrameGraph, FrameGraphError> {
        let passes: BTreeMap<NodeId, PassNode> = self
            .inner
            .nodes()
            .map(|(id, node)| (id, node.clone()))
            .collect();
        Ok(compile::compile_passes(&passes, &self.descriptors)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tex_desc() -> ResourceClassDescriptor {
        ResourceClassDescriptor::Texture(TextureDescriptor {
            width: 512,
            height: 512,
            depth_or_array_layers: 1,
            mip_level_count: 1,
            sample_count: 1,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            dimension: wgpu::TextureDimension::D2,
            view_dimension: wgpu::TextureViewDimension::D2,
        })
    }

    fn buf_desc() -> ResourceClassDescriptor {
        ResourceClassDescriptor::Buffer(BufferDescriptor {
            size_bytes: 1024,
            usage: wgpu::BufferUsages::UNIFORM,
        })
    }

    #[test]
    fn default_is_empty() {
        let fg = FrameGraph::default();
        assert_eq!(fg.pass_count(), 0);
    }

    #[test]
    fn new_is_empty() {
        let fg = FrameGraph::new();
        assert_eq!(fg.pass_count(), 0);
    }

    #[test]
    fn add_pass_returns_content_derived_node_id() {
        let mut fg = FrameGraph::new();
        let r1 = ResourceId::from_bytes([1u8; 16]);
        let id = fg
            .add_pass("p", vec![], vec![(r1, tex_desc())])
            .expect("add");
        assert_eq!(fg.pass_count(), 1);

        let pass2 = PassNode::new("p".to_string(), vec![], vec![r1]);
        let expected = stable_node_id(&pass2);
        assert_eq!(id, expected);
    }

    #[test]
    fn duplicate_pass_returns_graph_error() {
        let mut fg = FrameGraph::new();
        let r1 = ResourceId::from_bytes([1u8; 16]);
        fg.add_pass("p", vec![], vec![(r1, tex_desc())])
            .expect("first");
        let err = fg
            .add_pass("p", vec![], vec![(r1, tex_desc())])
            .expect_err("duplicate");
        assert!(
            matches!(err, FrameGraphError::Graph(GraphError::DuplicateNode(_))),
            "expected DuplicateNode; got {err:?}"
        );
    }

    #[test]
    fn empty_compiles_clean() {
        let fg = FrameGraph::new();
        let compiled = fg.compile().expect("compile");
        assert_eq!(compiled.pass_count(), 0);
        assert!(compiled.descriptors().is_empty());
    }

    #[test]
    fn compile_chain_yields_topological_order() {
        let mut fg = FrameGraph::new();
        let r1 = ResourceId::from_bytes([1u8; 16]);
        let r2 = ResourceId::from_bytes([2u8; 16]);
        let a = fg.add_pass("a", vec![], vec![(r1, tex_desc())]).expect("a");
        let b = fg
            .add_pass("b", vec![r1], vec![(r2, tex_desc())])
            .expect("b");
        let c = fg.add_pass("c", vec![r2], vec![]).expect("c");
        let compiled = fg.compile().expect("compile");
        let order = compiled.execution_order();
        assert_eq!(order.len(), 3);
        let pa = order.iter().position(|n| *n == a).unwrap();
        let pb = order.iter().position(|n| *n == b).unwrap();
        let pc = order.iter().position(|n| *n == c).unwrap();
        assert!(pa < pb && pb < pc);
    }

    #[test]
    fn compile_cycle_errors() {
        let mut fg = FrameGraph::new();
        let r1 = ResourceId::from_bytes([1u8; 16]);
        let r2 = ResourceId::from_bytes([2u8; 16]);
        fg.add_pass("a", vec![r2], vec![(r1, tex_desc())])
            .expect("a");
        fg.add_pass("b", vec![r1], vec![(r2, tex_desc())])
            .expect("b");
        let err = fg.compile().expect_err("cycle");
        assert!(matches!(err, FrameGraphError::Compile(CompileError::Cycle)));
    }

    // FG-D1: descriptors declared at add_pass time appear on the compiled
    // output keyed by ResourceId.
    #[test]
    fn add_pass_with_descriptors_compiles_and_carries_them() {
        let mut fg = FrameGraph::new();
        let r1 = ResourceId::from_bytes([1u8; 16]);
        let r2 = ResourceId::from_bytes([2u8; 16]);
        fg.add_pass("a", vec![], vec![(r1, tex_desc())]).expect("a");
        fg.add_pass("b", vec![r1], vec![(r2, buf_desc())])
            .expect("b");
        fg.add_pass("c", vec![r2], vec![]).expect("c");
        let compiled = fg.compile().expect("compile");
        assert_eq!(compiled.descriptors().len(), 2);
        assert_eq!(compiled.descriptor(r1), Some(&tex_desc()));
        assert_eq!(compiled.descriptor(r2), Some(&buf_desc()));
    }

    // FG-D2: re-declaring the same ResourceId with a conflicting
    // descriptor → DescriptorMismatch.
    #[test]
    fn duplicate_resource_id_with_conflicting_descriptor_errors() {
        let mut fg = FrameGraph::new();
        let r1 = ResourceId::from_bytes([1u8; 16]);
        fg.add_pass("a", vec![], vec![(r1, tex_desc())])
            .expect("first");
        // Second pass declares R1 with a buffer descriptor — must fail.
        let err = fg
            .add_pass("b", vec![], vec![(r1, buf_desc())])
            .expect_err("conflicting descriptor must be rejected");
        match err {
            FrameGraphError::DescriptorMismatch {
                resource,
                first_decl,
                conflicting,
            } => {
                assert_eq!(resource, r1);
                assert_eq!(first_decl, tex_desc());
                assert_eq!(conflicting, buf_desc());
            }
            other => panic!("expected DescriptorMismatch; got {other:?}"),
        }
        // Mismatch must leave the graph state untouched — only the first
        // pass was successfully added.
        assert_eq!(fg.pass_count(), 1);
        assert_eq!(fg.descriptors().get(&r1), Some(&tex_desc()));
    }

    #[test]
    fn duplicate_resource_id_with_matching_descriptor_succeeds() {
        // Two passes both writing R1 with the SAME descriptor is allowed —
        // the substrate only complains on conflicting descriptors.
        let mut fg = FrameGraph::new();
        let r1 = ResourceId::from_bytes([1u8; 16]);
        fg.add_pass("a", vec![], vec![(r1, tex_desc())])
            .expect("first");
        fg.add_pass("b", vec![], vec![(r1, tex_desc())])
            .expect("second-with-matching-descriptor");
        assert_eq!(fg.pass_count(), 2);
        assert_eq!(fg.descriptors().get(&r1), Some(&tex_desc()));
    }

    #[test]
    fn descriptor_mismatch_mid_writes_vector_leaves_no_partial_state() {
        // First pass writes R1 + R2 with their respective descriptors;
        // second pass attempts to write R3 (new) AND R1 with a conflicting
        // descriptor → error before R3 enters the sidecar.
        let mut fg = FrameGraph::new();
        let r1 = ResourceId::from_bytes([1u8; 16]);
        let r2 = ResourceId::from_bytes([2u8; 16]);
        let r3 = ResourceId::from_bytes([3u8; 16]);
        fg.add_pass("a", vec![], vec![(r1, tex_desc()), (r2, buf_desc())])
            .expect("first");
        let err = fg
            .add_pass("b", vec![], vec![(r3, tex_desc()), (r1, buf_desc())])
            .expect_err("mid-vector descriptor conflict");
        assert!(matches!(err, FrameGraphError::DescriptorMismatch { .. }));
        // R3 must NOT be in the sidecar — validation runs before any
        // mutation.
        assert!(fg.descriptors().get(&r3).is_none());
        assert_eq!(fg.descriptors().len(), 2);
        assert_eq!(fg.pass_count(), 1);
    }
}
