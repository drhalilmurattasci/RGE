//! `cad_core::topology` — minimum B-Rep face-identity substrate (sub-7.2-α).
//!
//! Failure class: snapshot-recoverable (inherited from crate-level).
//!
//! # What this module is
//!
//! The vocabulary substrate that proves **stable face identity across parameter
//! rebuilds** for one CAD operator (`CuboidOp`), faces only. It introduces:
//!
//! * [`BRepOwnerId`] — opaque, caller-supplied 16-byte owner seed.
//! * [`CuboidFaceTag`] — 6-variant `#[non_exhaustive]` tag enumerating the
//!   faces of an axis-aligned cuboid in the operator's actual emission order
//!   (`NegZ, PosZ, NegY, PosY, NegX, PosX` — per `CuboidOp::evaluate`).
//! * [`BRepFaceId`] — derived stable face identity computed via
//!   `BLAKE3(b"rge.cad.brep.face/v1:" || owner.as_bytes() || kind_tag_bytes)`
//!   truncated to 16 bytes.
//! * [`BRepProvider`] — sibling trait to `crate::operators::Operator` that
//!   pairs the existing per-tessellation [`crate::tessellation::TopologyFaceId`]
//!   (sequential, post-evaluate) with the new rebuild-stable [`BRepFaceId`].
//!   Implemented for `CuboidOp` only in v0.
//!
//! # Domain separator + version suffix
//!
//! The BLAKE3 input is prefixed with `b"rge.cad.brep.face/v1:"`. The literal
//! string `"rge.cad.brep.face"` is the domain separator (preventing collision
//! with future BLAKE3-derived id schemes — operator structural-hashes,
//! kernel/graph-foundation node ids, etc. — that share the same crate's
//! BLAKE3 surface). The `v1` suffix reserves room for migration if the
//! derivation scheme changes; building the migration substrate itself is a
//! separate-dispatch concern, not pre-built here.
//!
//! # v0 scope (sub-7.2-α only)
//!
//! Per-operator face-tag enums other than [`CuboidFaceTag`] are explicitly
//! out of scope. Edges, vertices, second operator's `BRepProvider` impl,
//! chain composition across an `OperatorGraph`, and projection / gfx
//! integration are all subsequent sub-7.2 dispatches. The full Phase 7.2
//! exit criterion ("100 operator chains × 10 random parameter rebuilds with
//! face/edge IDs preserved per `TopologyEvolution`") is NOT closed by this
//! substrate.

mod face_id;
mod face_tag;
mod provider;

pub use face_id::{BRepFaceId, BRepOwnerId};
pub use face_tag::CuboidFaceTag;
pub use provider::BRepProvider;
