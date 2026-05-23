use thiserror::Error;

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
