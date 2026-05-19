//! `rge-anim-graph` — animation graph foundation wrapper.
//!
//! Failure class: snapshot-recoverable
//!
//! Phase 8 foundation slice. [`AnimGraph`] is a thin wrapper over
//! `rge_kernel_graph_foundation::Graph` that stores data-only animation
//! states keyed by content-derived [`NodeId`] and connects them with
//! data-only [`AnimTransition`] payloads keyed by content-derived
//! [`EdgeId`]. Like `crates/material-graph`'s wrapper, the animation graph
//! is rebuildable structural state that participates in snapshot/restore —
//! a rejected mutation is recovered by restoring the last good snapshot
//! rather than terminating the session.
//!
//! This crate is the foundation layer only: it carries no traversal,
//! evaluation, blend trees, clip sampling, transition conditions, runtime
//! state-machine scheduling, editor behavior, or renderer-tier integration.
//! [`AnimState`] and [`AnimTransition`] are data-only payloads used solely
//! for deterministic ID derivation and duplicate detection.

use rge_kernel_graph_foundation::{
    EdgeId, EdgeView, Graph, GraphError, NodeId, NodeView, VizAdapter,
};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Error returned by mutating operations on an [`AnimGraph`].
///
/// A thin newtype over the substrate [`GraphError`]; the wrapped value
/// preserves the exact graph-foundation failure (duplicate node, duplicate
/// edge, or dangling endpoint) for callers that need to inspect it.
#[derive(Debug, PartialEq, Eq)]
pub struct AnimGraphError(pub GraphError);

impl std::fmt::Display for AnimGraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "anim graph error: {}", self.0)
    }
}

impl std::error::Error for AnimGraphError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

impl From<GraphError> for AnimGraphError {
    fn from(err: GraphError) -> Self {
        Self(err)
    }
}

// ---------------------------------------------------------------------------
// State payload
// ---------------------------------------------------------------------------

/// Data-only animation state payload.
///
/// The wrapper treats the state `key` as an uninterpreted string; the
/// substrate [`NodeId`] is derived deterministically from its bytes. The
/// payload carries no playback, blending, sampling, runtime scheduling,
/// editor, renderer, ECS, or asset-loading behavior.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnimState {
    /// Uninterpreted, deterministic key identifying the state.
    pub key: String,
}

impl AnimState {
    /// Construct an animation state with the given uninterpreted `key`.
    #[must_use]
    pub fn new(key: &str) -> Self {
        Self {
            key: key.to_owned(),
        }
    }
}

// ---------------------------------------------------------------------------
// Transition payload
// ---------------------------------------------------------------------------

/// Data-only animation transition payload.
///
/// The wrapper treats the transition `trigger` as an uninterpreted string;
/// it participates in the transition's content-derived [`EdgeId`], so two
/// transitions between the same states that use different triggers are
/// distinct edges. The payload carries no conditions, guards, weights,
/// timing, or runtime behavior.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnimTransition {
    /// Uninterpreted, deterministic trigger key identifying the transition.
    pub trigger: String,
}

impl AnimTransition {
    /// Construct an animation transition with the given uninterpreted
    /// `trigger`.
    #[must_use]
    pub fn new(trigger: &str) -> Self {
        Self {
            trigger: trigger.to_owned(),
        }
    }
}

// ---------------------------------------------------------------------------
// AnimGraph
// ---------------------------------------------------------------------------

/// Minimal animation graph: data-only states connected by data-only
/// transition edges, backed by `rge_kernel_graph_foundation::Graph`.
#[derive(Debug, Clone)]
pub struct AnimGraph {
    graph: Graph<AnimState, AnimTransition>,
}

impl AnimGraph {
    /// Construct an empty animation graph.
    #[must_use]
    pub fn new() -> Self {
        Self {
            graph: Graph::new(),
        }
    }

    /// Add an animation state identified by `key`.
    ///
    /// The returned [`NodeId`] is derived deterministically from the key, so
    /// the same key yields the same id in any [`AnimGraph`] instance.
    ///
    /// # Errors
    ///
    /// Returns [`AnimGraphError`] wrapping [`GraphError::DuplicateNode`]
    /// when a state with the same key (hence the same [`NodeId`]) is already
    /// present in this graph.
    pub fn add_state(&mut self, key: &str) -> Result<NodeId, AnimGraphError> {
        let id = NodeId::from_bytes(key.as_bytes());
        self.graph.insert_node(id, AnimState::new(key))?;
        Ok(id)
    }

    /// Add a directed transition from `src` to `dst` carrying `transition`.
    ///
    /// The returned [`EdgeId`] is derived deterministically from the endpoint
    /// ids together with the transition trigger.
    ///
    /// # Errors
    ///
    /// Returns [`AnimGraphError`] wrapping:
    /// - [`GraphError::DuplicateEdge`] when an identical transition (same
    ///   endpoints and same trigger) already exists; or
    /// - [`GraphError::DanglingEndpoint`] when `src` or `dst` is not
    ///   currently a state in this graph.
    pub fn add_transition(
        &mut self,
        src: NodeId,
        dst: NodeId,
        transition: AnimTransition,
    ) -> Result<EdgeId, AnimGraphError> {
        let id = anim_transition_id(src, dst, &transition);
        self.graph.insert_edge(id, src, dst, transition)?;
        Ok(id)
    }

    /// Returns the number of states currently in the graph.
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Returns the number of transitions currently in the graph.
    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }
}

impl Default for AnimGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Derive the content-stable [`EdgeId`] for a transition from its endpoints
/// and trigger, so identical transitions collide (duplicate detection) while
/// transitions that differ only in trigger stay distinct.
fn anim_transition_id(src: NodeId, dst: NodeId, transition: &AnimTransition) -> EdgeId {
    let mut bytes = Vec::with_capacity(32 + transition.trigger.len());
    bytes.extend_from_slice(&src.0.to_le_bytes());
    bytes.extend_from_slice(&dst.0.to_le_bytes());
    bytes.extend_from_slice(transition.trigger.as_bytes());
    EdgeId::from_bytes(&bytes)
}

// ---------------------------------------------------------------------------
// Graph-viewer adapter
// ---------------------------------------------------------------------------

/// Exposes the animation graph structure to editor graph-viewer widgets.
///
/// This is a read-only view surface only: it adds no traversal, evaluation,
/// blend trees, clip sampling, transition conditions, runtime state-machine
/// scheduling, editor behavior, or renderer-tier integration. Counts delegate
/// straight to the substrate counters, and node/edge views borrow the existing
/// substrate records — no duplicate structural state is introduced.
impl VizAdapter for AnimGraph {
    fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    fn nodes(&self) -> Box<dyn Iterator<Item = NodeView<'_>> + '_> {
        Box::new(self.graph.nodes().map(|(id, state)| NodeView {
            id,
            display_name: state.key.as_str(),
            kind: "AnimState",
        }))
    }

    fn edges(&self) -> Box<dyn Iterator<Item = EdgeView<'_>> + '_> {
        Box::new(self.graph.edges().map(|(id, record)| EdgeView {
            id,
            src: record.src,
            dst: record.dst,
            label: record.data.trigger.as_str(),
        }))
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use rge_kernel_graph_foundation::{
        EdgeRecord, GraphDiff, GraphError, GraphSnapshot, Invalidation, InvalidationListener,
    };

    use super::*;

    #[test]
    fn state_ids_are_stable_across_graphs() {
        let mut a = AnimGraph::new();
        let mut b = AnimGraph::new();
        let id_a = a.add_state("idle").unwrap();
        let id_b = b.add_state("idle").unwrap();
        assert_eq!(
            id_a, id_b,
            "the same key in two fresh graphs must yield the same NodeId"
        );
    }

    #[test]
    fn distinct_keys_get_distinct_ids() {
        let mut g = AnimGraph::new();
        let idle = g.add_state("idle").unwrap();
        let run = g.add_state("run").unwrap();
        assert_ne!(idle, run, "distinct keys must yield distinct NodeIds");
    }

    #[test]
    fn add_transition_succeeds_and_updates_counts() {
        let mut g = AnimGraph::new();
        let idle = g.add_state("idle").unwrap();
        let run = g.add_state("run").unwrap();
        assert_eq!(g.node_count(), 2);
        assert_eq!(g.edge_count(), 0);

        g.add_transition(idle, run, AnimTransition::new("start_run"))
            .unwrap();

        assert_eq!(
            g.edge_count(),
            1,
            "a successful transition increments edge count"
        );
        assert_eq!(g.node_count(), 2, "add_transition must preserve node count");
    }

    #[test]
    fn duplicate_state_is_rejected() {
        let mut g = AnimGraph::new();
        g.add_state("idle").unwrap();
        let err = g
            .add_state("idle")
            .expect_err("re-adding the same state key must fail");
        assert!(
            matches!(err.0, GraphError::DuplicateNode(_)),
            "expected DuplicateNode, got {err:?}"
        );
    }

    #[test]
    fn duplicate_transition_is_rejected() {
        let mut g = AnimGraph::new();
        let idle = g.add_state("idle").unwrap();
        let run = g.add_state("run").unwrap();
        g.add_transition(idle, run, AnimTransition::new("start_run"))
            .unwrap();

        let err = g
            .add_transition(idle, run, AnimTransition::new("start_run"))
            .expect_err("re-adding an identical transition must fail");
        assert!(
            matches!(err.0, GraphError::DuplicateEdge(_)),
            "expected DuplicateEdge, got {err:?}"
        );
        assert_eq!(
            g.edge_count(),
            1,
            "rejected transition must not add an edge"
        );
    }

    #[test]
    fn differing_triggers_are_not_duplicates() {
        let mut g = AnimGraph::new();
        let idle = g.add_state("idle").unwrap();
        let run = g.add_state("run").unwrap();
        g.add_transition(idle, run, AnimTransition::new("start_run"))
            .unwrap();
        g.add_transition(idle, run, AnimTransition::new("sprint"))
            .unwrap();
        assert_eq!(
            g.edge_count(),
            2,
            "same endpoints with different triggers are distinct edges"
        );
    }

    #[test]
    fn dangling_endpoint_is_rejected() {
        let mut g = AnimGraph::new();
        let idle = g.add_state("idle").unwrap();
        let ghost = NodeId::from_bytes(b"never-added");

        let err = g
            .add_transition(idle, ghost, AnimTransition::new("start_run"))
            .expect_err("transitioning to an absent state must fail");
        assert!(
            matches!(err.0, GraphError::DanglingEndpoint { .. }),
            "expected DanglingEndpoint, got {err:?}"
        );
        assert_eq!(
            g.edge_count(),
            0,
            "rejected transition must not add an edge"
        );
    }

    #[test]
    fn empty_graph_has_zero_counts() {
        let g = AnimGraph::default();
        assert_eq!(g.node_count(), 0);
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn viz_adapter_counts_observed_through_trait() {
        let mut g = AnimGraph::new();
        let idle = g.add_state("idle").unwrap();
        let run = g.add_state("run").unwrap();
        let jump = g.add_state("jump").unwrap();
        g.add_transition(idle, run, AnimTransition::new("start_run"))
            .unwrap();
        g.add_transition(run, jump, AnimTransition::new("leap"))
            .unwrap();

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

    #[test]
    fn viz_adapter_node_views_match_substrate_order() {
        let mut g = AnimGraph::new();
        let idle = g.add_state("idle").unwrap();
        let run = g.add_state("run").unwrap();
        let jump = g.add_state("jump").unwrap();

        // Expected order is the deterministic substrate (BTreeMap) order,
        // i.e. sorted by NodeId — not the insertion order above.
        let mut expected: Vec<(NodeId, &str)> = vec![(idle, "idle"), (run, "run"), (jump, "jump")];
        expected.sort_by_key(|&(id, _)| id);

        let adapter: &dyn VizAdapter = &g;
        let views: Vec<(NodeId, String, String)> = adapter
            .nodes()
            .map(|n| (n.id, n.display_name.to_owned(), n.kind.to_owned()))
            .collect();

        assert_eq!(views.len(), 3, "one node view per animation state");
        for (view, &(exp_id, exp_name)) in views.iter().zip(expected.iter()) {
            assert_eq!(view.0, exp_id, "node view id is the substrate NodeId");
            assert_eq!(
                view.1, exp_name,
                "node view display_name is the animation state key"
            );
            assert_eq!(
                view.2, "AnimState",
                "every animation node view has the static kind string"
            );
        }
    }

    #[test]
    fn snapshot_ron_round_trip_preserves_anim_payloads() {
        // Build a populated animation graph: three states joined by two
        // transitions carrying distinct trigger payloads.
        let mut g = AnimGraph::new();
        let idle = g.add_state("idle").unwrap();
        let run = g.add_state("run").unwrap();
        let jump = g.add_state("jump").unwrap();
        let e_run = g
            .add_transition(idle, run, AnimTransition::new("start_run"))
            .unwrap();
        let e_jump = g
            .add_transition(run, jump, AnimTransition::new("leap"))
            .unwrap();

        // Capture the private substrate graph and round-trip it through the
        // full path: Graph -> GraphSnapshot -> RON text -> GraphSnapshot -> Graph.
        let snapshot = GraphSnapshot::from_graph(&g.graph);
        let ron = snapshot.to_ron().expect("snapshot serializes to RON");
        let restored_snapshot: GraphSnapshot<AnimState, AnimTransition> =
            GraphSnapshot::from_ron(&ron).expect("snapshot deserializes from RON");
        let restored = restored_snapshot.to_graph();

        // Structure survives the round trip.
        assert_eq!(
            restored.node_count(),
            g.graph.node_count(),
            "node count survives the snapshot RON round trip"
        );
        assert_eq!(
            restored.edge_count(),
            g.graph.edge_count(),
            "edge count survives the snapshot RON round trip"
        );

        // Node identity: every NodeId paired with its stored AnimState payload
        // (the uninterpreted key) is preserved, in the same deterministic order.
        let original_nodes: Vec<(NodeId, AnimState)> =
            g.graph.nodes().map(|(id, n)| (id, n.clone())).collect();
        let restored_nodes: Vec<(NodeId, AnimState)> =
            restored.nodes().map(|(id, n)| (id, n.clone())).collect();
        assert_eq!(
            restored_nodes, original_nodes,
            "every NodeId and AnimState payload is restored unchanged"
        );

        // Edge identity: every EdgeId paired with its full record — source
        // node, destination node, and AnimTransition trigger payload — is
        // preserved, in the same deterministic order.
        let original_edges: Vec<(EdgeId, EdgeRecord<AnimTransition>)> =
            g.graph.edges().map(|(id, r)| (id, r.clone())).collect();
        let restored_edges: Vec<(EdgeId, EdgeRecord<AnimTransition>)> =
            restored.edges().map(|(id, r)| (id, r.clone())).collect();
        assert_eq!(
            restored_edges, original_edges,
            "every EdgeId, endpoint pair, and AnimTransition payload is restored unchanged"
        );

        // Spot-check the concrete transitions built above, addressed by the
        // EdgeId returned at construction time.
        let restored_run = restored
            .edge(e_run)
            .expect("the idle->run transition is present after restore");
        assert_eq!(restored_run.src, idle, "run transition source restored");
        assert_eq!(restored_run.dst, run, "run transition destination restored");
        assert_eq!(
            restored_run.data.trigger, "start_run",
            "run transition trigger payload restored"
        );
        let restored_jump = restored
            .edge(e_jump)
            .expect("the run->jump transition is present after restore");
        assert_eq!(restored_jump.src, run, "jump transition source restored");
        assert_eq!(
            restored_jump.dst, jump,
            "jump transition destination restored"
        );
        assert_eq!(
            restored_jump.data.trigger, "leap",
            "jump transition trigger payload restored"
        );
    }

    #[test]
    fn viz_adapter_edge_views_match_substrate_order() {
        let mut g = AnimGraph::new();
        let idle = g.add_state("idle").unwrap();
        let run = g.add_state("run").unwrap();
        let jump = g.add_state("jump").unwrap();

        let e1 = g
            .add_transition(idle, run, AnimTransition::new("start_run"))
            .unwrap();
        let e2 = g
            .add_transition(run, jump, AnimTransition::new("leap"))
            .unwrap();

        // Expected order is the deterministic substrate (BTreeMap) order,
        // i.e. sorted by EdgeId — not the insertion order above.
        let mut expected: Vec<(EdgeId, NodeId, NodeId, &str)> =
            vec![(e1, idle, run, "start_run"), (e2, run, jump, "leap")];
        expected.sort_by_key(|&(id, ..)| id);

        let adapter: &dyn VizAdapter = &g;
        let views: Vec<(EdgeId, NodeId, NodeId, String)> = adapter
            .edges()
            .map(|e| (e.id, e.src, e.dst, e.label.to_owned()))
            .collect();

        assert_eq!(views.len(), 2, "one edge view per transition");
        for (view, &(exp_id, exp_src, exp_dst, exp_label)) in views.iter().zip(expected.iter()) {
            assert_eq!(view.0, exp_id, "edge view id is the substrate EdgeId");
            assert_eq!(view.1, exp_src, "edge view src is the record source");
            assert_eq!(view.2, exp_dst, "edge view dst is the record destination");
            assert_eq!(
                view.3, exp_label,
                "edge view label is the transition trigger string"
            );
        }
    }

    #[test]
    fn structural_diff_reports_added_anim_state_and_transition() {
        // Build an initial animation graph: two states joined by one
        // transition, then snapshot it as the "old" state.
        let mut g = AnimGraph::new();
        let idle = g.add_state("idle").unwrap();
        let run = g.add_state("run").unwrap();
        g.add_transition(idle, run, AnimTransition::new("start_run"))
            .unwrap();
        let old_snapshot = GraphSnapshot::from_graph(&g.graph);

        // Mutate the same graph: add exactly one new animation state and
        // exactly one new transition connected to that new state.
        let jump = g.add_state("jump").unwrap();
        let new_edge_id = g
            .add_transition(run, jump, AnimTransition::new("leap"))
            .unwrap();
        let new_snapshot = GraphSnapshot::from_graph(&g.graph);

        // The graph-foundation structural diff over the old → new snapshots.
        let diff = GraphDiff::between(&old_snapshot, &new_snapshot);

        // Exactly the one new state is reported as added, with its payload.
        assert_eq!(
            diff.added_nodes.len(),
            1,
            "exactly one animation state was added"
        );
        assert_eq!(
            diff.added_nodes.get(&jump),
            Some(&AnimState {
                key: "jump".to_owned(),
            }),
            "the added node is the new 'jump' animation state, by id and payload"
        );

        // Exactly the one new transition is reported as added, with its
        // full record.
        assert_eq!(
            diff.added_edges.len(),
            1,
            "exactly one animation transition was added"
        );
        assert_eq!(
            diff.added_edges.get(&new_edge_id),
            Some(&EdgeRecord {
                src: run,
                dst: jump,
                data: AnimTransition::new("leap"),
            }),
            "the added edge record carries the source, new node, and trigger payload"
        );

        // Nothing was removed and no pre-existing state or transition record
        // changed.
        assert!(
            diff.removed_nodes.is_empty(),
            "no animation state was removed"
        );
        assert!(
            diff.removed_edges.is_empty(),
            "no animation transition was removed"
        );
        assert!(
            diff.changed_nodes.is_empty(),
            "no existing animation state record changed"
        );
        assert!(
            diff.changed_edges.is_empty(),
            "no existing animation transition record changed"
        );
    }

    #[test]
    fn invalidation_propagates_through_anim_outgoing_edges() {
        use std::sync::{Arc, Mutex};

        // Diamond plus a downstream path:
        //   root -> left, root -> right, left -> join, right -> join,
        //   join -> tail.
        // The shared convergence state `join` exercises the router's
        // per-call visited-set dedup; the two outgoing edges from `root`
        // exercise BFS scheduling order; the trailing `join -> tail` edge
        // exercises BFS propagation past the dedup point.
        let mut g = AnimGraph::new();
        let root = g.add_state("root").unwrap();
        let left = g.add_state("left").unwrap();
        let right = g.add_state("right").unwrap();
        let join = g.add_state("join").unwrap();
        let tail = g.add_state("tail").unwrap();

        let e_left = g
            .add_transition(root, left, AnimTransition::new("to_left"))
            .unwrap();
        let e_right = g
            .add_transition(root, right, AnimTransition::new("to_right"))
            .unwrap();
        g.add_transition(left, join, AnimTransition::new("left_join"))
            .unwrap();
        g.add_transition(right, join, AnimTransition::new("right_join"))
            .unwrap();
        g.add_transition(join, tail, AnimTransition::new("to_tail"))
            .unwrap();

        // The substrate iterates `outgoing(root)` in EdgeId-sorted order
        // (BTreeSet<EdgeId>), which fixes whether `left` or `right` is
        // enqueued first. Derive the expected BFS sequence from those
        // concrete ids rather than guessing from declaration order.
        let (first_child, second_child) = if e_left < e_right {
            (left, right)
        } else {
            (right, left)
        };
        let expected = vec![root, first_child, second_child, join, tail];

        struct Recorder(Arc<Mutex<Vec<NodeId>>>);
        impl InvalidationListener for Recorder {
            fn on_invalidated(&mut self, node: NodeId) {
                self.0.lock().unwrap().push(node);
            }
        }

        let log = Arc::new(Mutex::new(Vec::<NodeId>::new()));
        let mut inv = Invalidation::new();
        inv.register(Box::new(Recorder(Arc::clone(&log))));

        // The dependents closure reads downstream states straight off the
        // animation graph's substrate via outgoing edges + edge-record dst,
        // proving the wrapper's substrate is sufficient to drive
        // graph-foundation invalidation without any ad hoc side table.
        inv.mark_dirty(root, |node| {
            g.graph
                .outgoing(node)
                .filter_map(|eid| g.graph.edge(eid).map(|record| record.dst))
                .collect()
        });

        let calls = log.lock().unwrap().clone();
        assert_eq!(
            calls, expected,
            "listener log is the dirty root followed by every downstream animation state exactly once in BFS order"
        );
        assert_eq!(
            calls.iter().filter(|&&n| n == join).count(),
            1,
            "diamond convergence state is deduplicated to a single delivery"
        );
    }
}
