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
//!
//! [`NodeId`]: rge_kernel_graph_foundation::NodeId
//! [`ResourceId`]: crate::frame_graph::resource::ResourceId

mod algorithm;
mod error;
mod types;

pub(crate) use algorithm::compile_passes;
pub use error::CompileError;
pub use types::{AliasingGroup, CompiledFrameGraph, ResourceLifetime};
