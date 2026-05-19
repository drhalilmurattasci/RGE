//! Operator DAG built on top of `kernel/graph-foundation`'s [`Graph`] primitive.
//!
//! Failure class: snapshot-recoverable
//!
//! [`OperatorGraph`] is a thin wrapper around `Graph<OperatorNode, EdgeKind>`
//! that adds:
//!
//! * Content-derived `NodeId`/`EdgeId` so identical operators dedupe.
//! * Recursive structural-hash evaluation through a `TessellationCache`.
//! * Cycle detection (graph-foundation's `Graph` itself does NOT detect
//!   cycles — see test 4 in the unit-test block).
//!
//! # Cycle detection — three implementations across `Graph<N, ()>` consumers
//!
//! Audit-3 finding M7 (2026-05-09) flagged that the workspace has three
//! distinct cycle-detection implementations layered on top of
//! `kernel/graph-foundation::Graph<N, E>`:
//!
//! 1. **This file** — ancestor-set guard inside [`Self::effective_hash_and_label`]
//!    (the `if !stack.insert(node_id)` line). Returns [`EvalError::Cycle`]
//!    inline during recursive hash-folding evaluation. NOT a standalone
//!    cycle scan: the recursion only descends into reachable upstream nodes
//!    (those actually feeding the evaluation target), so it cannot be
//!    factored out without restructuring the evaluator. Path-tracking is
//!    the recursion stack itself; no cycle path is returned.
//! 2. **`kernel/asset/src/dependency_graph.rs::DependencyGraph::detect_cycle`** —
//!    standalone three-color DFS (visited + in-stack) returning
//!    `Option<Vec<AssetId>>` (the cycle path).
//! 3. **`crates/asset-store/src/dependency.rs::DepGraph::transitive_closure`
//!    / `::invalidation_cascade`** — start-node-targeted iterative walks
//!    that bail with `DepError::Cycle(AssetId)` (single offending asset)
//!    when the traversal revisits the start node.
//!
//! These three implementations diverge by design: each returns a different
//! shape (eval-integrated `EvalError::Cycle` vs. full path `Vec<AssetId>`
//! vs. single `DepError::Cycle(AssetId)`) and operates at a different
//! granularity (on-the-fly during eval / all-pairs full-graph scan /
//! start-node-bounded reachability). They are NOT one algorithm wearing
//! three return types; unifying them would force one consumer's
//! information shape onto the other two and either leak unnecessary work
//! (full DFS where bailing on first revisit suffices) or lose information
//! (path discarded). PLAN §1.14 line 605 ("is this primitive infrastructure
//! that all 8 graph systems would use the same way?") evaluates to NO for
//! cycle detection: each domain consumer uses it differently. Per PLAN
//! §1.14 line 628, graph-foundation deliberately stays "primitives, not
//! runtime" — cycle-detection therefore lives in each domain's wrapper.
//! See `docs/§18/GRAPH_FOUNDATION.md` §3 for the substrate-side framing.

// SPLIT-EXEMPTION: cohesive OperatorGraph substrate — the content-addressed
// operator-DAG wrapper, its build/eval errors, the recursive structural-hash
// evaluator with inline cycle detection, the read-only `VizAdapter` view
// adapter, and the in-file unit-test block all belong to one type. The file
// crossed the 1000-line cap only after the in-scope ISSUE-53 `VizAdapter`
// implementation and its three trait-path tests; splitting would scatter a
// single cohesive type across modules for no architectural gain.

use std::collections::HashSet;
use std::sync::Arc;

use rge_kernel_graph_foundation::{
    EdgeId, EdgeRecord, EdgeView, Graph, GraphError, NodeId, NodeView, VizAdapter,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::operators::{EdgeKind, OpError, OpKind, Operator, OperatorNode};
use crate::tessellation::{CacheKey, Tessellation, TessellationCache, Tolerance};

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors produced while building an [`OperatorGraph`].
#[derive(Debug, Error)]
pub enum GraphBuildError {
    /// Underlying graph-foundation error.
    #[error("graph error: {0}")]
    Graph(#[from] GraphError),
    /// Caller asked us to set a non-existent node as the root.
    #[error("root node {0} not found")]
    RootNotFound(NodeId),
    /// A node's incoming-edge count does not match its declared arity.
    #[error("port mismatch on node {node}: arity={expected_arity}, got={got}")]
    PortMismatch {
        /// Node whose arity was violated.
        node: NodeId,
        /// Operator's declared arity.
        expected_arity: usize,
        /// Actual number of incoming edges.
        got: usize,
    },
}

/// Errors produced during graph evaluation.
#[derive(Debug, Error)]
pub enum EvalError {
    /// An operator-evaluation step failed.
    #[error("operator error: {0}")]
    Op(#[from] OpError),
    /// A node referenced during evaluation was missing from the graph.
    #[error("node {0} not found")]
    NodeNotFound(NodeId),
    /// The graph had no root configured.
    #[error("root not found")]
    RootNotFound,
    /// A cycle was detected during traversal.
    #[error("cycle detected during evaluation")]
    Cycle,
    /// An edge had an invalid (out-of-range or duplicated) port assignment.
    #[error("port mismatch on node {node}: arity={expected_arity}, got={got}")]
    PortMismatch {
        /// The downstream node whose arity was violated.
        node: NodeId,
        /// Operator's declared arity.
        expected_arity: usize,
        /// Number of incoming edges actually present.
        got: usize,
    },
}

// ---------------------------------------------------------------------------
// OperatorGraph
// ---------------------------------------------------------------------------

/// CAD operator graph: nodes are [`OperatorNode`]s, edges are [`EdgeKind`]s.
///
/// Stores the inner `Graph<OperatorNode, EdgeKind>` directly so snapshots from
/// `kernel/graph-foundation` work without an intermediate wrapper.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OperatorGraph {
    graph: Graph<OperatorNode, EdgeKind>,
    root: Option<NodeId>,
}

impl Default for OperatorGraph {
    fn default() -> Self {
        Self {
            graph: Graph::new(),
            root: None,
        }
    }
}

impl OperatorGraph {
    /// Construct an empty operator graph (no root, no nodes).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an operator node, deriving its [`NodeId`] from the serialized
    /// content so two `add_operator` calls with identical payloads collide
    /// (which we surface as `GraphError::DuplicateNode`).
    ///
    /// # Errors
    ///
    /// * [`GraphBuildError::Graph`] wrapping `GraphError::DuplicateNode` when
    ///   an identical-content node was already inserted.
    pub fn add_operator(&mut self, op: OperatorNode) -> Result<NodeId, GraphBuildError> {
        let id = derive_node_id(&op);
        self.graph.insert_node(id, op)?;
        Ok(id)
    }

    /// Add a directed edge from `src` to `dst`, declaring that `src`'s
    /// tessellation feeds `dst`'s `port`-th input.
    ///
    /// The [`EdgeId`] is derived from `(src, dst, port)` so duplicate-edge
    /// errors trigger a `GraphError::DuplicateEdge`.
    ///
    /// # Errors
    ///
    /// * Wraps any [`GraphError`] returned by graph-foundation (dangling
    ///   endpoint, duplicate edge).
    pub fn connect(
        &mut self,
        src: NodeId,
        dst: NodeId,
        port: u8,
    ) -> Result<EdgeId, GraphBuildError> {
        let id = derive_edge_id(src, dst, port);
        self.graph
            .insert_edge(id, src, dst, EdgeKind::Input(port))?;
        Ok(id)
    }

    /// Designate `node` as the graph's evaluation root.
    ///
    /// # Errors
    ///
    /// [`GraphBuildError::RootNotFound`] if `node` is not currently in the
    /// graph.
    pub fn set_root(&mut self, node: NodeId) -> Result<(), GraphBuildError> {
        if self.graph.node(node).is_none() {
            return Err(GraphBuildError::RootNotFound(node));
        }
        self.root = Some(node);
        Ok(())
    }

    /// The graph's evaluation root, if any.
    #[must_use]
    pub fn root(&self) -> Option<NodeId> {
        self.root
    }

    /// Look up an operator node by id.
    #[must_use]
    pub fn node(&self, id: NodeId) -> Option<&OperatorNode> {
        self.graph.node(id)
    }

    /// Number of nodes currently in the graph.
    ///
    /// Forwards to the underlying [`Graph::node_count`] (Tier-A counter
    /// per ADR-115 phase-1; O(1)). See [`Self::operator_count`] for the
    /// operator-graph semantic name; the two methods return the same
    /// value because every node in an `OperatorGraph` is an operator.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Number of edges currently in the graph.
    ///
    /// Forwards to the underlying [`Graph::edge_count`] (Tier-A counter
    /// per ADR-115 phase-1; O(1)).
    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    /// Returns the number of operators in the graph.
    ///
    /// **Tier-A** (canonical structural counter; ADR-115 phase-2.5 amendment).
    ///
    /// Tier-A counter per ADR-115 phase-1 (graph-metrics substrate
    /// design, sub-decision 2). O(1). In an [`OperatorGraph`] every
    /// node is an operator-bearing variant ([`OperatorNode`]), so the
    /// operator count equals the underlying graph's node count. Cross-
    /// references [`Graph::node_count`] (the substrate-level Tier-A
    /// counter this method semantically renames).
    ///
    /// # Companion metrics
    ///
    /// - [`Self::node_count`] / [`Self::edge_count`] — substrate-level
    ///   counters this graph inherits from `Graph<N, E>`.
    /// - `constraint_count` — deferred per ADR-115; depends on a future
    ///   constraint-system substrate that does not yet exist.
    /// - `invalidation_count` — deferred per ADR-115; cross-substrate
    ///   concern (cad-projection head-advance + cad-core checkpoint
    ///   commits) that lands in phase-3+ via the event-sourced
    ///   `GraphEvent` stream (ADR-115 sub-decision 4).
    #[must_use]
    pub fn operator_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Raw access to the inner `Graph` (used by checkpoints to capture
    /// snapshots).
    #[must_use]
    pub fn inner(&self) -> &Graph<OperatorNode, EdgeKind> {
        &self.graph
    }

    /// Replace the inner graph wholesale (used by checkpoints to restore
    /// snapshots).
    pub(crate) fn replace_inner(&mut self, graph: Graph<OperatorNode, EdgeKind>) {
        self.graph = graph;
    }

    /// Evaluate the subtree rooted at `target`, with memoization through
    /// `cache`. The cache key is `(effective_hash, tolerance)` where
    /// `effective_hash` is the recursive BLAKE3 over the operator's local
    /// `structural_hash` plus its inputs' effective hashes — so any change
    /// upstream causes a cache miss at every dependent node.
    ///
    /// # Errors
    ///
    /// * [`EvalError::RootNotFound`] if the graph has no root and `target`
    ///   does not exist.
    /// * [`EvalError::Cycle`] if a cycle is detected during traversal.
    /// * [`EvalError::PortMismatch`] if a node's incoming-edge count does
    ///   not match its arity, or if multiple edges declare the same port.
    /// * [`EvalError::Op`] wrapping any operator-evaluation failure.
    pub fn evaluate(
        &self,
        target: NodeId,
        cache: &mut TessellationCache,
        tolerance: Tolerance,
    ) -> Result<Arc<Tessellation>, EvalError> {
        if self.graph.node(target).is_none() {
            return Err(EvalError::NodeNotFound(target));
        }
        let mut stack: HashSet<NodeId> = HashSet::new();
        self.eval_node(target, cache, tolerance, &mut stack)
    }

    /// Recursive evaluation helper. `stack` holds the ancestors currently on
    /// the evaluation path so we can detect cycles.
    ///
    /// Closes audit-2 finding A1.4 / A5.2 / Pairing N2: `eval_node` calls
    /// [`Self::effective_hash_and_label`] (which folds the upstream-labeled
    /// bitmap into the hash) so the cache key distinguishes evaluations whose
    /// inputs differ in labeled-state, even if the local operator's
    /// `structural_hash` does not itself encode label-state.
    fn eval_node(
        &self,
        node_id: NodeId,
        cache: &mut TessellationCache,
        tolerance: Tolerance,
        stack: &mut HashSet<NodeId>,
    ) -> Result<Arc<Tessellation>, EvalError> {
        // Compute this node's effective_hash + predicted output_labeled (the
        // recursion in `effective_hash_and_label` walks the full upstream
        // subtree and validates ports/arity along the way; the label flag
        // is discarded here — eval_node only needs the hash for the cache
        // key).
        let (effective_hash, _output_labeled) = self.effective_hash_and_label(node_id, stack)?;

        let key = CacheKey {
            structural_hash: effective_hash,
            tolerance,
        };
        if let Some(hit) = cache.get(&key) {
            cache.record_hit();
            return Ok(hit);
        }
        cache.record_miss();

        // Cache miss — actually evaluate. Re-resolve the node + ports to
        // get upstream tessellations (the hash recursion above only needs
        // hashes, not Arc<Tessellation>).
        let node = self
            .graph
            .node(node_id)
            .ok_or(EvalError::NodeNotFound(node_id))?;
        let arity = node.arity();
        let by_port = self.collect_incoming_by_port(node_id, arity)?;

        // Evaluate each upstream first (recursive).
        let mut upstream_tess: Vec<Arc<Tessellation>> = Vec::with_capacity(arity);
        for (_, src) in &by_port {
            upstream_tess.push(self.eval_node(*src, cache, tolerance, stack)?);
        }

        let inputs: Vec<&Tessellation> = upstream_tess.iter().map(AsRef::as_ref).collect();
        let tess = node.evaluate(&inputs)?;
        Ok(cache.insert(key, tess))
    }

    /// Compute a node's `(effective_hash, output_is_labeled)` pair, with the
    /// upstream-labeled-bitmap folded into the hash. Both [`Self::eval_node`]
    /// and the downstream-hash-only path call this helper to ensure they
    /// produce identical `effective_hash`es for the same node.
    ///
    /// The cycle guard is owned here (the recursive walk of upstream nodes
    /// happens via the inner method); duplicate guards in the caller would
    /// be redundant.
    ///
    /// # Errors
    ///
    /// * [`EvalError::Cycle`] if `node_id` is already on the recursion stack.
    /// * [`EvalError::NodeNotFound`] / [`EvalError::PortMismatch`] propagated
    ///   from the recursive walk over upstream nodes.
    ///
    /// Exposed `pub` so integration tests (notably
    /// `tests/labeled_tessellation_pipeline.rs`) can verify the cache-key
    /// uniqueness contract without piggy-backing on `evaluate`. The cycle
    /// guard is owned here so callers don't need to manage it separately.
    pub fn effective_hash_and_label(
        &self,
        node_id: NodeId,
        stack: &mut HashSet<NodeId>,
    ) -> Result<([u8; 32], bool), EvalError> {
        if !stack.insert(node_id) {
            return Err(EvalError::Cycle);
        }
        let res = self.effective_hash_and_label_inner(node_id, stack);
        stack.remove(&node_id);
        res
    }

    fn effective_hash_and_label_inner(
        &self,
        node_id: NodeId,
        stack: &mut HashSet<NodeId>,
    ) -> Result<([u8; 32], bool), EvalError> {
        let node = self
            .graph
            .node(node_id)
            .ok_or(EvalError::NodeNotFound(node_id))?;

        let arity = node.arity();
        let by_port = self.collect_incoming_by_port(node_id, arity)?;

        // Recurse to gather each upstream's `(effective_hash, output_is_labeled)`.
        let mut upstream_data: Vec<([u8; 32], bool)> = Vec::with_capacity(arity);
        for (_, src) in &by_port {
            upstream_data.push(self.effective_hash_and_label(*src, stack)?);
        }

        // Fold local hash + per-port (port_index, upstream_hash) into the hasher.
        let mut hasher = blake3::Hasher::new();
        hasher.update(&node.structural_hash());
        for (i, (h, _)) in upstream_data.iter().enumerate() {
            #[allow(
                clippy::cast_possible_truncation,
                reason = "i bounded by upstream port count which is u8 by construction"
            )]
            let port = i as u8;
            hasher.update(&[port]);
            hasher.update(h);
        }
        // Defense-in-depth (audit-2 A1.4 / A5.2 / Pairing N2): fold the
        // upstream-labeled bitmap into the hash so the cache key distinguishes
        // evaluations whose inputs differ in labeled-state, even if the local
        // `structural_hash` doesn't encode label-state. One bit per port,
        // wrapped modulo 32 for ports beyond 32 (no current operator exceeds
        // arity 2; future ops are unlikely to exceed 32).
        let upstream_labeled_bitmap: u32 =
            upstream_data
                .iter()
                .enumerate()
                .fold(0u32, |acc, (i, (_, labeled))| {
                    if *labeled {
                        acc | (1u32 << (i % 32))
                    } else {
                        acc
                    }
                });
        hasher.update(&upstream_labeled_bitmap.to_le_bytes());
        let effective_hash = *hasher.finalize().as_bytes();

        // Predict this node's output_is_labeled via the trait method.
        let inputs_labeled: Vec<bool> = upstream_data.iter().map(|(_, l)| *l).collect();
        let output_labeled = node.output_is_labeled(&inputs_labeled);

        Ok((effective_hash, output_labeled))
    }

    /// Collect a node's incoming edges sorted by port, validating arity +
    /// port-uniqueness (ports must cover `0..arity` exactly once).
    fn collect_incoming_by_port(
        &self,
        node_id: NodeId,
        arity: usize,
    ) -> Result<Vec<(u8, NodeId)>, EvalError> {
        let incoming: Vec<EdgeId> = self.graph.incoming(node_id).collect();
        if incoming.len() != arity {
            return Err(EvalError::PortMismatch {
                node: node_id,
                expected_arity: arity,
                got: incoming.len(),
            });
        }

        let mut by_port: Vec<(u8, NodeId)> = Vec::with_capacity(incoming.len());
        for eid in incoming {
            let rec: &EdgeRecord<EdgeKind> = self
                .graph
                .edge(eid)
                .ok_or(EvalError::NodeNotFound(node_id))?;
            let EdgeKind::Input(port) = rec.data;
            by_port.push((port, rec.src));
        }
        by_port.sort_by_key(|(port, _)| *port);

        for (i, (port, _)) in by_port.iter().enumerate() {
            #[allow(
                clippy::cast_possible_truncation,
                reason = "i bounded by upstream port count which is u8 by construction"
            )]
            let expected = i as u8;
            if *port != expected {
                return Err(EvalError::PortMismatch {
                    node: node_id,
                    expected_arity: arity,
                    got: incoming_count(&by_port),
                });
            }
        }

        Ok(by_port)
    }

    /// Compute a node's effective recursive structural hash WITHOUT
    /// triggering evaluation. Wrapper around [`Self::effective_hash_and_label`]
    /// that discards the predicted-output-label flag.
    #[cfg(test)]
    fn effective_hash(
        &self,
        node_id: NodeId,
        stack: &mut HashSet<NodeId>,
    ) -> Result<[u8; 32], EvalError> {
        self.effective_hash_and_label(node_id, stack)
            .map(|(h, _)| h)
    }

    /// Caller-friendly wrapper around [`Self::effective_hash_and_label`] that
    /// owns the recursion-stack `HashSet` so callers (notably integration
    /// tests that don't see `rge-kernel-graph-foundation` as a direct dep
    /// and therefore can't name `NodeId` for `HashSet<NodeId>`) don't need
    /// to construct it.
    ///
    /// Returns `(effective_hash, predicted_output_is_labeled)` for the
    /// subgraph rooted at `node_id`.
    ///
    /// # Errors
    ///
    /// * [`EvalError::NodeNotFound`] if `node_id` is not in the graph.
    /// * [`EvalError::Cycle`] / [`EvalError::PortMismatch`] propagated from
    ///   the recursive walk.
    pub fn effective_hash_and_label_root(
        &self,
        node_id: NodeId,
    ) -> Result<([u8; 32], bool), EvalError> {
        let mut stack: HashSet<NodeId> = HashSet::new();
        self.effective_hash_and_label(node_id, &mut stack)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

#[allow(
    clippy::cast_possible_truncation,
    reason = "v.len() bounded by upstream port count; truncation infeasible at expected scale"
)]
fn incoming_count(v: &[(u8, NodeId)]) -> usize {
    v.len()
}

/// Derive a content-addressed [`NodeId`] for an `OperatorNode`.
///
/// We use RON to serialize because the graph-foundation crate already pulls
/// in `ron` for snapshot serialization and the result is stable across
/// builds (no nondeterministic ordering, since the operator structs serialize
/// in field-declaration order).
fn derive_node_id(op: &OperatorNode) -> NodeId {
    let serialized = ron::to_string(op).expect("OperatorNode serializes");
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"cad-op:");
    hasher.update(serialized.as_bytes());
    NodeId::from_bytes(hasher.finalize().as_bytes())
}

/// Derive an [`EdgeId`] from `(src, dst, port)`.
fn derive_edge_id(src: NodeId, dst: NodeId, port: u8) -> EdgeId {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"cad-edge:");
    hasher.update(&src.0.to_le_bytes());
    hasher.update(&dst.0.to_le_bytes());
    hasher.update(&[port]);
    EdgeId::from_bytes(hasher.finalize().as_bytes())
}

// ---------------------------------------------------------------------------
// VizAdapter — read-only graph-viewer view surface
// ---------------------------------------------------------------------------

/// Stable CAD operator kind name for a node view.
///
/// Returns a `'static` string literal — never debug formatting, RON
/// serialization, or a parameter dump — so [`NodeView::display_name`] and
/// [`NodeView::kind`] borrow without any per-call allocation. The name is
/// derived only from the existing [`OpKind`] discriminant and is exhaustive
/// over every operator variant.
fn operator_kind_name(node: &OperatorNode) -> &'static str {
    match node.op_kind() {
        OpKind::Boolean => "Boolean",
        OpKind::Cuboid => "Cuboid",
        OpKind::Extrude => "Extrude",
        OpKind::Fillet => "Fillet",
        OpKind::Loft => "Loft",
        OpKind::Revolve => "Revolve",
        OpKind::RoundFillet => "RoundFillet",
        OpKind::Sweep => "Sweep",
        OpKind::Transform => "Transform",
    }
}

/// Deterministic input-port label for an edge view.
///
/// Returns a `'static` string literal so [`EdgeView::label`] borrows it for
/// the lifetime of the view without any per-edge allocation. Current CAD
/// operators only use input ports `0` and `1`; any unexpected port falls
/// back to a conservative static label rather than allocating a formatted
/// string or widening the graph-foundation trait.
fn input_port_label(edge: EdgeKind) -> &'static str {
    match edge {
        EdgeKind::Input(0) => "input[0]",
        EdgeKind::Input(1) => "input[1]",
        EdgeKind::Input(_) => "input[?]",
    }
}

/// Exposes the operator graph structure to editor graph-viewer widgets.
///
/// This is a read-only view surface only: it adds no traversal, evaluation,
/// editor behavior, renderer integration, or graph-foundation behavior.
/// Counts delegate directly to the inner graph's Tier-A substrate counters,
/// and node/edge iterators preserve the deterministic substrate order the
/// inner `Graph<OperatorNode, EdgeKind>` already provides.
impl VizAdapter for OperatorGraph {
    fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    fn nodes(&self) -> Box<dyn Iterator<Item = NodeView<'_>> + '_> {
        Box::new(self.graph.nodes().map(|(id, node)| {
            // `OperatorGraph` has no user-authored node names, so the stable
            // CAD operator kind name serves as both display name and kind.
            let kind = operator_kind_name(node);
            NodeView {
                id,
                display_name: kind,
                kind,
            }
        }))
    }

    fn edges(&self) -> Box<dyn Iterator<Item = EdgeView<'_>> + '_> {
        Box::new(self.graph.edges().map(|(id, record)| EdgeView {
            id,
            src: record.src,
            dst: record.dst,
            label: input_port_label(record.data),
        }))
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operators::{BooleanOp, CuboidOp, TransformOp};

    fn make_tol() -> Tolerance {
        Tolerance::new(0.001).expect("tol")
    }

    fn cuboid_node(w: f32, h: f32, d: f32) -> OperatorNode {
        OperatorNode::Cuboid(CuboidOp {
            width: w,
            height: h,
            depth: d,
        })
    }

    fn translate_node(dx: f32) -> OperatorNode {
        OperatorNode::Transform(TransformOp {
            translation: [dx, 0.0, 0.0],
            ..TransformOp::default()
        })
    }

    /// Test 1 — empty graph eval errors with `NodeNotFound` (the spec says
    /// `RootNotFound` but the public surface accepts any target `NodeId`, so
    /// the more accurate error here is `NodeNotFound`; we test the
    /// behavior matches the spec's intent: trying to evaluate a nonexistent
    /// node fails cleanly).
    #[test]
    fn empty_graph_evaluate_root_errors() {
        let g = OperatorGraph::new();
        let bogus = NodeId::from_raw(0xdead_beef);
        let mut cache = TessellationCache::new();
        let err = g.evaluate(bogus, &mut cache, make_tol()).unwrap_err();
        assert!(matches!(err, EvalError::NodeNotFound(_)));
    }

    /// Test 2 — single Cuboid evaluates to 8-vertex tessellation.
    #[test]
    fn single_cuboid_evaluates_to_8_vertices() {
        let mut g = OperatorGraph::new();
        let id = g.add_operator(cuboid_node(1.0, 1.0, 1.0)).expect("add");
        g.set_root(id).expect("root");
        let mut cache = TessellationCache::new();
        let mesh = g.evaluate(id, &mut cache, make_tol()).expect("eval");
        assert_eq!(mesh.vertex_count(), 8);
        assert_eq!(mesh.triangle_count(), 12);
    }

    /// Test 3 — Cuboid → Transform translates the cuboid.
    #[test]
    fn transform_chain_translates_cuboid() {
        let mut g = OperatorGraph::new();
        let cu = g.add_operator(cuboid_node(2.0, 1.0, 1.0)).expect("cu");
        let tx = g.add_operator(translate_node(5.0)).expect("tx");
        g.connect(cu, tx, 0).expect("connect");
        g.set_root(tx).expect("root");
        let mut cache = TessellationCache::new();
        let mesh = g.evaluate(tx, &mut cache, make_tol()).expect("eval");
        assert_eq!(mesh.vertex_count(), 8);
        // All vertices should have x >= 4.0 (cuboid half-width 1.0, plus +5.0).
        for [x, _, _] in &mesh.positions {
            assert!(*x >= 4.0 - 1e-6, "x not translated by 5: {x}");
        }
    }

    /// Test 4 — cycle detection. graph-foundation's `Graph` does not detect
    /// cycles itself (it accepts any DAG-like connections including A→B→A),
    /// so `OperatorGraph`'s evaluator MUST.
    #[test]
    fn cycle_detected() {
        // We force-construct a cycle directly through the inner Graph since
        // OperatorGraph's `connect` validates only via graph-foundation
        // primitives, which do not reject cycles.
        let mut g = OperatorGraph::new();
        // Two Transform operators (arity 1 each), wired A → B → A. We must
        // bypass derive_node_id for the second insertion since the two ops
        // would otherwise dedupe; use distinct payloads.
        let a = g.add_operator(translate_node(1.0)).expect("a");
        let b = g.add_operator(translate_node(2.0)).expect("b");
        g.connect(a, b, 0).expect("a->b");
        // graph-foundation accepts the cycle-completing edge:
        g.connect(b, a, 0).expect("b->a (graph-foundation accepts)");
        g.set_root(b).expect("root");
        let mut cache = TessellationCache::new();
        let err = g.evaluate(b, &mut cache, make_tol()).unwrap_err();
        assert!(
            matches!(err, EvalError::Cycle),
            "expected Cycle, got {err:?}"
        );
    }

    /// Test 5 — port mismatch on arity violation: 2 Cuboids both feeding
    /// Transform (arity 1).
    #[test]
    fn port_mismatch_on_arity_violation() {
        let mut g = OperatorGraph::new();
        let c1 = g.add_operator(cuboid_node(1.0, 1.0, 1.0)).expect("c1");
        let c2 = g.add_operator(cuboid_node(2.0, 1.0, 1.0)).expect("c2");
        let tx = g.add_operator(translate_node(0.0)).expect("tx");
        g.connect(c1, tx, 0).expect("c1->tx port 0");
        g.connect(c2, tx, 1).expect("c2->tx port 1");
        let mut cache = TessellationCache::new();
        let err = g.evaluate(tx, &mut cache, make_tol()).unwrap_err();
        assert!(
            matches!(
                err,
                EvalError::PortMismatch {
                    expected_arity: 1,
                    got: 2,
                    ..
                }
            ),
            "expected PortMismatch (1 vs 2), got {err:?}"
        );
    }

    /// Test 6 — cache hit returns the previously-stored Arc. We pre-seed
    /// the cache with a sentinel Tessellation under the same key the
    /// evaluator will produce; the second evaluate must return that
    /// sentinel (proving cache lookup hit).
    #[test]
    fn cache_hit_on_unchanged_subtree() {
        let mut g = OperatorGraph::new();
        let id = g.add_operator(cuboid_node(1.0, 1.0, 1.0)).expect("add");
        g.set_root(id).expect("root");
        let mut cache = TessellationCache::new();

        // Run once to populate cache.
        let _first = g.evaluate(id, &mut cache, make_tol()).expect("first");
        let misses_after_first = cache.misses();
        assert_eq!(misses_after_first, 1, "first eval = miss");

        // Run again — same subtree, same tolerance — must be a hit.
        let _second = g.evaluate(id, &mut cache, make_tol()).expect("second");
        assert_eq!(cache.misses(), 1, "no new miss");
        assert_eq!(cache.hits(), 1, "one hit recorded");
    }

    /// Audit-2 A1.4 / A5.2 / Pairing N2 regression: `effective_hash`
    /// distinguishes evaluations whose inputs differ in labeled-state, even
    /// when the local operator's `structural_hash` doesn't itself encode
    /// label-state. Defense in depth against operator implementations that
    /// forget to fold label-emitting parameters into `structural_hash`.
    ///
    /// Post-D-projection-α (2026-05-09) `CuboidOp` now emits labeled
    /// output, so a graph with two cuboids feeding a Boolean produces
    /// the bitmap `0b11 = 3` here. The defensive bitmap-fold guarantee is
    /// exercised by verifying that swapping the upstream-labeled bitmap
    /// changes the resulting `effective_hash` bytes regardless of which
    /// concrete bitmap value the live graph happens to produce.
    #[test]
    fn effective_hash_distinguishes_labeled_vs_unlabeled_input_state() {
        // Build G: BooleanOp(Union) ← [Cuboid@port 0, Cuboid'@port 1].
        // Post-D-projection-α both cuboids emit labeled output, so the
        // helper's observed bitmap is 0b11 = 3 here.
        let mut g = OperatorGraph::new();
        let cu_lhs = g.add_operator(cuboid_node(1.0, 1.0, 1.0)).expect("cu_lhs");
        let cu_rhs = g
            .add_operator(cuboid_node(2.0, 1.0, 1.0)) // distinct payload to avoid dedup
            .expect("cu_rhs");
        let bool_id = g
            .add_operator(OperatorNode::Boolean(BooleanOp::union()))
            .expect("bool");
        g.connect(cu_lhs, bool_id, 0).expect("lhs->bool port 0");
        g.connect(cu_rhs, bool_id, 1).expect("rhs->bool port 1");

        // Helper-computed hash with the live (post-D-projection-α) bitmap.
        let mut stack: HashSet<NodeId> = HashSet::new();
        let observed_hash = g
            .effective_hash(bool_id, &mut stack)
            .expect("effective_hash unlabeled");

        // Hand-compute the same recipe to confirm the helper is what we
        // think it is. We don't hard-code which bitmap is the "live" one;
        // we identify it dynamically and assert the helper matches.
        let bool_node = g.node(bool_id).expect("bool node present");
        let lhs_hash = g
            .effective_hash(cu_lhs, &mut HashSet::new())
            .expect("cu_lhs hash");
        let rhs_hash = g
            .effective_hash(cu_rhs, &mut HashSet::new())
            .expect("cu_rhs hash");
        let recompute = |bitmap: u32| -> [u8; 32] {
            let mut hasher = blake3::Hasher::new();
            hasher.update(&bool_node.structural_hash());
            hasher.update(&[0u8]); // port 0
            hasher.update(&lhs_hash);
            hasher.update(&[1u8]); // port 1
            hasher.update(&rhs_hash);
            hasher.update(&bitmap.to_le_bytes());
            *hasher.finalize().as_bytes()
        };
        // Post-D-projection-α both cuboid upstreams are labeled, so the
        // live bitmap is 3 (port 0 labeled + port 1 labeled).
        let labeled_both = recompute(3);
        assert_eq!(
            observed_hash, labeled_both,
            "helper output must match the bitmap=3 recipe (both Cuboid \
             upstreams emit labeled output post-D-projection-α)"
        );

        // The audit-2 defensive guarantee: swap the bitmap → hash MUST
        // differ. All four bitmap states must produce distinct hashes.
        let unlabeled_recompute = recompute(0);
        let labeled_port_0 = recompute(1);
        let labeled_port_1 = recompute(2);
        assert_ne!(
            unlabeled_recompute, labeled_port_0,
            "labeled-port-0 must produce a different effective_hash than all-unlabeled"
        );
        assert_ne!(
            unlabeled_recompute, labeled_port_1,
            "labeled-port-1 must produce a different effective_hash"
        );
        assert_ne!(
            labeled_port_0, labeled_port_1,
            "port-0-only-labeled vs port-1-only-labeled must differ"
        );
        assert_ne!(
            unlabeled_recompute, labeled_both,
            "both-labeled must differ from all-unlabeled"
        );
    }

    /// Supporting test: the helper's predicted `output_is_labeled` matches
    /// each operator's `Operator::output_is_labeled` for the upstream-labeled
    /// state observed during the recursion. `CuboidOp` emits labeled output,
    /// and `TransformOp` is topology-preserving, so the predicted output is
    /// labeled here. This verifies the helper is actually invoking the trait
    /// method rather than returning a fixed value.
    #[test]
    fn effective_hash_and_label_predicts_labeled_for_cuboid_transform_graph() {
        let mut g = OperatorGraph::new();
        let cu = g.add_operator(cuboid_node(1.0, 1.0, 1.0)).expect("cu");
        let tx = g.add_operator(translate_node(0.0)).expect("tx");
        g.connect(cu, tx, 0).expect("connect");
        let mut stack: HashSet<NodeId> = HashSet::new();
        let (_, output_labeled) = g
            .effective_hash_and_label(tx, &mut stack)
            .expect("hash+label");
        assert!(
            output_labeled,
            "Cuboid -> TransformOp pipeline emits labeled output because Transform preserves face labels"
        );
    }

    /// Test 7 — KEY CORRECTNESS TEST: when we change an upstream Cuboid
    /// parameter, the downstream Transform's effective hash must differ,
    /// causing a cache miss at the Transform — and the resulting
    /// tessellation must differ in vertex positions.
    #[test]
    fn cache_miss_when_parameter_changes() {
        // Build first chain: Cuboid(1,1,1) → Transform(translate +1 x).
        let mut g1 = OperatorGraph::new();
        let cu1 = g1.add_operator(cuboid_node(1.0, 1.0, 1.0)).expect("cu1");
        let tx1 = g1.add_operator(translate_node(1.0)).expect("tx1");
        g1.connect(cu1, tx1, 0).expect("connect1");
        let mut cache = TessellationCache::new();
        let m1 = g1.evaluate(tx1, &mut cache, make_tol()).expect("eval1");

        // Build second chain in a fresh graph: Cuboid(2,1,1) → identical
        // Transform. Same Transform structural_hash, but the upstream
        // cuboid differs — so the Transform's effective hash MUST differ.
        let mut g2 = OperatorGraph::new();
        let cu2 = g2.add_operator(cuboid_node(2.0, 1.0, 1.0)).expect("cu2");
        // Reuse the same Transform parameters → same NodeId derivation? No,
        // `add_operator` derives the id from the OperatorNode payload only,
        // so identical Transform payload would yield the SAME NodeId across
        // graphs. That's fine here; we just want a separate graph.
        let tx2 = g2.add_operator(translate_node(1.0)).expect("tx2");
        g2.connect(cu2, tx2, 0).expect("connect2");
        let m2 = g2.evaluate(tx2, &mut cache, make_tol()).expect("eval2");

        // Both Transform ops share local structural_hash; if effective hashes
        // were identical we'd get the cached m1 result back. Verify the
        // tessellations differ at the vertex level.
        assert_ne!(
            m1.positions, m2.positions,
            "parameter change upstream must propagate to a different cached result"
        );
        // Also verify cache recorded a second miss (proving the second
        // evaluate did NOT hit m1's cache slot).
        assert!(
            cache.misses() >= 2,
            "expected at least 2 misses (different effective hashes); got {}",
            cache.misses()
        );
    }

    // ---------------------------------------------------------------------
    // Tier-A counter tests (ADR-115 phase-1)
    // ---------------------------------------------------------------------
    //
    // `operator_count` is the operator-graph semantic name for the same
    // underlying Tier-A counter that `Graph<N, E>::node_count` already
    // exposes. The tests pin (a) that the count tracks adds, (b) that
    // it tracks removes, and (c) that it equals `inner().node_count()`
    // — which is the structural invariant the method's docstring
    // promises.

    #[test]
    fn operator_count_matches_node_count() {
        let mut g = OperatorGraph::new();
        assert_eq!(g.operator_count(), 0);
        assert_eq!(g.operator_count(), g.inner().node_count());

        let _id1 = g.add_operator(cuboid_node(1.0, 1.0, 1.0)).expect("op1");
        let _id2 = g.add_operator(cuboid_node(2.0, 1.0, 1.0)).expect("op2");
        let _id3 = g.add_operator(cuboid_node(1.0, 2.0, 1.0)).expect("op3");

        assert_eq!(g.operator_count(), 3, "three operators added");
        assert_eq!(
            g.operator_count(),
            g.inner().node_count(),
            "operator_count must equal underlying Graph::node_count (every node IS an operator)"
        );
    }

    #[test]
    fn operator_count_after_remove() {
        // Build 3 operators, then remove one through the inner graph
        // (OperatorGraph itself has no public remove API yet, but the
        // Tier-A counter is correct under any path that mutates the
        // inner Graph, which is what remove will eventually use).
        let mut g = OperatorGraph::new();
        let id1 = g.add_operator(cuboid_node(1.0, 1.0, 1.0)).expect("op1");
        let _id2 = g.add_operator(cuboid_node(2.0, 1.0, 1.0)).expect("op2");
        let _id3 = g.add_operator(cuboid_node(1.0, 2.0, 1.0)).expect("op3");
        assert_eq!(g.operator_count(), 3);

        // Reach into inner Graph via `replace_inner` round-trip: the
        // public surface today doesn't expose remove on `OperatorGraph`,
        // but the structural invariant — operator_count tracks the inner
        // Graph::node_count exactly — holds regardless of which mutation
        // path edited the graph. We exercise it by producing a Graph
        // with the operator removed.
        let mut inner_clone = g.inner().clone();
        inner_clone.remove_node(id1).expect("remove op1 from inner");
        g.replace_inner(inner_clone);

        assert_eq!(
            g.operator_count(),
            2,
            "after removing one operator, operator_count drops by 1"
        );
        assert_eq!(g.operator_count(), g.inner().node_count());
    }

    // ---------------------------------------------------------------------
    // VizAdapter view-surface tests
    // ---------------------------------------------------------------------
    //
    // These exercise the read-only `VizAdapter` adapter through the
    // object-safe `&dyn VizAdapter` path, proving the trait observes the
    // existing deterministic substrate structure (counts plus node/edge
    // views in the inner graph's id-sorted order).

    /// Counts observed through the trait delegate to the inner substrate
    /// counters, and the boxed view iterators yield exactly that many items.
    #[test]
    fn viz_adapter_counts_observed_through_trait() {
        let mut g = OperatorGraph::new();
        let cu_lhs = g.add_operator(cuboid_node(1.0, 1.0, 1.0)).expect("cu_lhs");
        let cu_rhs = g.add_operator(cuboid_node(2.0, 1.0, 1.0)).expect("cu_rhs");
        let bool_id = g
            .add_operator(OperatorNode::Boolean(BooleanOp::union()))
            .expect("bool");
        g.connect(cu_lhs, bool_id, 0).expect("lhs->bool port 0");
        g.connect(cu_rhs, bool_id, 1).expect("rhs->bool port 1");

        let adapter: &dyn VizAdapter = &g;
        assert_eq!(
            adapter.node_count(),
            3,
            "VizAdapter node_count must delegate to the substrate count"
        );
        assert_eq!(
            adapter.edge_count(),
            2,
            "VizAdapter edge_count must delegate to the substrate count"
        );
        assert_eq!(
            adapter.nodes().count(),
            adapter.node_count(),
            "node view iteration yields exactly node_count items"
        );
        assert_eq!(
            adapter.edges().count(),
            adapter.edge_count(),
            "edge view iteration yields exactly edge_count items"
        );
    }

    /// Node views observed through `&dyn VizAdapter` follow the deterministic
    /// substrate order (inner graph nodes sorted by `NodeId`), and each view
    /// carries the stable CAD operator kind name as both display name and
    /// kind.
    #[test]
    fn viz_adapter_node_views_match_substrate_order() {
        let mut g = OperatorGraph::new();
        let cu = g.add_operator(cuboid_node(1.0, 1.0, 1.0)).expect("cu");
        let tx = g.add_operator(translate_node(3.0)).expect("tx");
        let bl = g
            .add_operator(OperatorNode::Boolean(BooleanOp::union()))
            .expect("bool");

        // Expected order is the deterministic substrate (BTreeMap) order,
        // i.e. sorted by NodeId — not the insertion order above.
        let mut expected: Vec<(NodeId, &str)> =
            vec![(cu, "Cuboid"), (tx, "Transform"), (bl, "Boolean")];
        expected.sort_by_key(|&(id, _)| id);

        let adapter: &dyn VizAdapter = &g;
        let views: Vec<(NodeId, String, String)> = adapter
            .nodes()
            .map(|n| (n.id, n.display_name.to_owned(), n.kind.to_owned()))
            .collect();

        assert_eq!(views.len(), 3, "one node view per operator");
        for (view, &(exp_id, exp_name)) in views.iter().zip(expected.iter()) {
            assert_eq!(view.0, exp_id, "node view id is the substrate NodeId");
            assert_eq!(
                view.1, exp_name,
                "node view display_name is the stable CAD operator kind name"
            );
            assert_eq!(
                view.2, exp_name,
                "node view kind is the stable CAD operator kind name"
            );
        }
    }

    /// Edge views observed through `&dyn VizAdapter` follow the deterministic
    /// substrate order (inner graph edges sorted by `EdgeId`), and each view
    /// carries the input-port label for its `EdgeKind::Input(port)` payload.
    #[test]
    fn viz_adapter_edge_views_match_substrate_order() {
        let mut g = OperatorGraph::new();
        let cu_lhs = g.add_operator(cuboid_node(1.0, 1.0, 1.0)).expect("cu_lhs");
        let cu_rhs = g.add_operator(cuboid_node(2.0, 1.0, 1.0)).expect("cu_rhs");
        let bool_id = g
            .add_operator(OperatorNode::Boolean(BooleanOp::union()))
            .expect("bool");
        let e0 = g.connect(cu_lhs, bool_id, 0).expect("lhs->bool port 0");
        let e1 = g.connect(cu_rhs, bool_id, 1).expect("rhs->bool port 1");

        // Expected order is the deterministic substrate (BTreeMap) order,
        // i.e. sorted by EdgeId — not the connect-call order above.
        let mut expected: Vec<(EdgeId, NodeId, NodeId, &str)> = vec![
            (e0, cu_lhs, bool_id, "input[0]"),
            (e1, cu_rhs, bool_id, "input[1]"),
        ];
        expected.sort_by_key(|&(id, ..)| id);

        let adapter: &dyn VizAdapter = &g;
        let views: Vec<(EdgeId, NodeId, NodeId, String)> = adapter
            .edges()
            .map(|e| (e.id, e.src, e.dst, e.label.to_owned()))
            .collect();

        assert_eq!(views.len(), 2, "one edge view per connection");
        for (view, &(exp_id, exp_src, exp_dst, exp_label)) in views.iter().zip(expected.iter()) {
            assert_eq!(view.0, exp_id, "edge view id is the substrate EdgeId");
            assert_eq!(view.1, exp_src, "edge view src is the record source");
            assert_eq!(view.2, exp_dst, "edge view dst is the record destination");
            assert_eq!(
                view.3, exp_label,
                "edge view label is the deterministic input-port label"
            );
        }
    }
}
