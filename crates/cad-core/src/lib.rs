//! `rge-cad-core` — CAD transactional graph core.
//!
//! Failure class: snapshot-recoverable
//!
//! Per [PLAN.md §1.5.4](../../PLAN.md). Owns the authoritative CAD graph state,
//! persistent topology IDs, lineage, history, and tessellation cache.
//! ECS view layer is `cad-projection` (Tier 2).
//!
//! # Phase 7.1 D-prime substrate
//!
//! Implemented modules:
//!
//! * [`operators`] — operator type system + [`CuboidOp`] / [`TransformOp`].
//! * [`graph`] — [`OperatorGraph`] DAG built on `kernel/graph-foundation`.
//! * [`checkpoints`] — [`CadGraph`] transactional `begin/commit/rollback/restore_to`.
//! * [`tessellation`] — output `Tessellation` mesh + memoization cache.
//! * [`topo_lineage`] — Phase 7.4 v0 topology lineage prototype: per-face
//!   identity ([`TopologyFaceId`]) + plane-equation-based labeling +
//!   [`LineageGraph`] inference between input and output meshes.
//!
//! Other modules remain stubs pending later phases (constraints, persistence,
//! …).

#![forbid(unsafe_code)]

pub mod adapters;
pub mod checkpoints;
pub mod constraints;
pub mod diagnostics;
pub mod graph;
pub mod history;
mod internals;
pub mod operators;
pub mod persistence;
pub mod tessellation;
pub mod topo_lineage;
pub mod topology;

pub use checkpoints::{CadGraph, Checkpoint, CheckpointError, CheckpointHistory, CheckpointId};
pub use graph::{EvalError, GraphBuildError, OperatorGraph};
pub use operators::{
    BooleanMode, BooleanOp, CuboidOp, EdgeKind, ExtrudeOp, LoftOp, OpError, OpKind, Operator,
    OperatorNode, Polygon2D, Polygon2DError, Polyline3D, Polyline3DError, RevolveOp, SweepOp,
    TransformOp,
};
pub use tessellation::{
    CacheKey, Tessellation, TessellationCache, TessellationError, Tolerance, ToleranceError,
    TopologyFaceId,
};
pub use topo_lineage::{
    infer_lineage, label_by_plane, LineageEdge, LineageError, LineageGraph, TopologyEvolution,
};
pub use topology::{
    brep_face_ids_for_node, BRepEdgeId, BRepEdgeProvider, BRepFaceId, BRepOwnerId, BRepProvider,
    BRepResolveError, CuboidFaceTag, ExtrudeFaceTag, LoftFaceTag, RevolveFaceTag, RevolveMode,
};
