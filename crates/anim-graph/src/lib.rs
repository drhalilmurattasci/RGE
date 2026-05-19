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

use rge_kernel_graph_foundation::{EdgeId, Graph, GraphError, NodeId};

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
#[derive(Debug, Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
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
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use rge_kernel_graph_foundation::GraphError;

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
}
