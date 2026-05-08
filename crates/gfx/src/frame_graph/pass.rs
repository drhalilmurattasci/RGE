//! Frame-graph pass-node payload.
//!
//! [`PassNode`] is the value stored at each [`rge_kernel_graph_foundation::NodeId`]
//! in the underlying graph-foundation `Graph<PassNode, ()>`. Pass-node identity
//! is content-derived via [`rge_kernel_graph_foundation::StableHash`]: two
//! passes with the same name + same reads + same writes hash to the same
//! `NodeId` and collide on `add_pass`.
//!
//! Frame-graph v0 stores no edge payload (`Graph<_, ()>`) — dependency edges
//! are derived from `reads` / `writes` declarations at compile time, not
//! materialised in the graph at runtime. A typed `EdgeKind` will land if and
//! when v1 introduces explicit edge construction.

use rge_kernel_graph_foundation::StableHash;
use serde::{Deserialize, Serialize};

use crate::frame_graph::resource::ResourceId;

/// Frame-graph pass — a unit of work that reads / writes resources.
///
/// `name` is a human-readable label (e.g. `"shadow_pass"`). Two passes with
/// identical content (same `name`, same `reads`, same `writes`) collide on
/// content-derived `NodeId`; callers wanting distinct pass instances with
/// the same shape must vary the `name`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PassNode {
    /// Human-readable label for the pass.
    pub name: String,
    /// Resources read by this pass. Each must be matched by at least one
    /// preceding pass that writes it (validated at compile time).
    pub reads: Vec<ResourceId>,
    /// Resources written by this pass. Becomes a dependency target for any
    /// later pass that reads the same resource.
    pub writes: Vec<ResourceId>,
}

impl PassNode {
    /// Construct from owned data.
    #[must_use]
    pub fn new(name: String, reads: Vec<ResourceId>, writes: Vec<ResourceId>) -> Self {
        Self {
            name,
            reads,
            writes,
        }
    }
}

impl StableHash for PassNode {
    fn hash_into(&self, hasher: &mut blake3::Hasher) {
        hasher.update(b"frame_graph::PassNode/v1\0");
        hasher.update(self.name.as_bytes());
        hasher.update(b"\0reads\0");
        for r in &self.reads {
            hasher.update(r.as_bytes());
        }
        hasher.update(b"\0writes\0");
        for w in &self.writes {
            hasher.update(w.as_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use rge_kernel_graph_foundation::stable_node_id;

    use super::*;

    fn r(b: u8) -> ResourceId {
        ResourceId::from_bytes([b; 16])
    }

    #[test]
    fn stable_hash_distinct_for_distinct_names() {
        let a = PassNode::new("a".to_string(), vec![], vec![]);
        let b = PassNode::new("b".to_string(), vec![], vec![]);
        assert_ne!(stable_node_id(&a), stable_node_id(&b));
    }

    #[test]
    fn stable_hash_distinct_for_distinct_reads() {
        let a = PassNode::new("p".to_string(), vec![r(1)], vec![]);
        let b = PassNode::new("p".to_string(), vec![r(2)], vec![]);
        assert_ne!(stable_node_id(&a), stable_node_id(&b));
    }

    #[test]
    fn stable_hash_distinct_for_distinct_writes() {
        let a = PassNode::new("p".to_string(), vec![], vec![r(1)]);
        let b = PassNode::new("p".to_string(), vec![], vec![r(2)]);
        assert_ne!(stable_node_id(&a), stable_node_id(&b));
    }

    #[test]
    fn stable_hash_distinct_for_reads_vs_writes() {
        // Same resource in `reads` vs `writes` must produce distinct hashes —
        // the domain separator `\0reads\0` / `\0writes\0` enforces this.
        let a = PassNode::new("p".to_string(), vec![r(1)], vec![]);
        let b = PassNode::new("p".to_string(), vec![], vec![r(1)]);
        assert_ne!(stable_node_id(&a), stable_node_id(&b));
    }

    #[test]
    fn stable_hash_deterministic() {
        let a = PassNode::new("p".to_string(), vec![r(1)], vec![r(2)]);
        let b = PassNode::new("p".to_string(), vec![r(1)], vec![r(2)]);
        assert_eq!(stable_node_id(&a), stable_node_id(&b));
    }
}
