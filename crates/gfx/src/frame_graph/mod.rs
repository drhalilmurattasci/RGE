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
pub mod pass;
pub mod resource;

use std::collections::BTreeMap;

pub use compile::{AliasingGroup, CompileError, CompiledFrameGraph, ResourceLifetime};
pub use pass::PassNode;
pub use resource::{ResourceId, ResourceUsage};
use rge_kernel_graph_foundation::{stable_node_id, Graph, GraphError, NodeId};
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
#[derive(Debug)]
pub struct FrameGraph {
    inner: Graph<PassNode, ()>,
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
        }
    }

    /// Add a pass with the given `name` + `reads` + `writes`. Returns the
    /// new pass's content-derived [`NodeId`].
    ///
    /// # Errors
    ///
    /// Returns [`FrameGraphError::Graph`] wrapping
    /// [`GraphError::DuplicateNode`] if the pass content collides with an
    /// existing pass (same name + same reads + same writes — same
    /// content-hash).
    pub fn add_pass(
        &mut self,
        name: impl Into<String>,
        reads: Vec<ResourceId>,
        writes: Vec<ResourceId>,
    ) -> Result<NodeId, FrameGraphError> {
        let pass = PassNode::new(name.into(), reads, writes);
        let id = stable_node_id(&pass);
        self.inner.insert_node(id, pass)?;
        Ok(id)
    }

    /// Number of passes currently in the graph.
    #[must_use]
    pub fn pass_count(&self) -> usize {
        self.inner.node_count()
    }

    /// Compile the registered passes into a [`CompiledFrameGraph`].
    ///
    /// See [`compile_passes`](compile::compile_passes) for the algorithm.
    /// Compilation does not mutate `self`; the underlying graph stores
    /// only nodes, and dependency adjacency is built in a working copy
    /// during compile.
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
        Ok(compile::compile_passes(&passes)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let id = fg.add_pass("p", vec![], vec![r1]).expect("add");
        assert_eq!(fg.pass_count(), 1);

        let pass2 = PassNode::new("p".to_string(), vec![], vec![r1]);
        let expected = stable_node_id(&pass2);
        assert_eq!(id, expected);
    }

    #[test]
    fn duplicate_pass_returns_graph_error() {
        let mut fg = FrameGraph::new();
        let r1 = ResourceId::from_bytes([1u8; 16]);
        fg.add_pass("p", vec![], vec![r1]).expect("first");
        let err = fg.add_pass("p", vec![], vec![r1]).expect_err("duplicate");
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
    }

    #[test]
    fn compile_chain_yields_topological_order() {
        let mut fg = FrameGraph::new();
        let r1 = ResourceId::from_bytes([1u8; 16]);
        let r2 = ResourceId::from_bytes([2u8; 16]);
        let a = fg.add_pass("a", vec![], vec![r1]).expect("a");
        let b = fg.add_pass("b", vec![r1], vec![r2]).expect("b");
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
        fg.add_pass("a", vec![r2], vec![r1]).expect("a");
        fg.add_pass("b", vec![r1], vec![r2]).expect("b");
        let err = fg.compile().expect_err("cycle");
        assert!(matches!(err, FrameGraphError::Compile(CompileError::Cycle)));
    }
}
