//! Owner seed + derived face identity for B-Rep faces.
//!
//! This module ships [`BRepOwnerId`] (the caller-supplied 16-byte owner seed)
//! and [`BRepFaceId`] (the BLAKE3-derived stable face identity).

use serde::{Deserialize, Serialize};

mod cuboid;
mod extrude;
mod loft;
mod revolve;
mod sweep;

// ---------------------------------------------------------------------------
// BRepOwnerId
// ---------------------------------------------------------------------------

/// Opaque, caller-supplied 16-byte owner seed for B-Rep face identity.
///
/// # Why an owner seed exists
///
/// Naively, [`BRepFaceId`] could be derived from `(operator_kind, face_tag)`
/// alone — but that collides the moment a graph contains two cuboids. The
/// owner seed disambiguates: each independent CAD model the caller wants to
/// give a stable identity space gets its own [`BRepOwnerId`].
///
/// # Two non-negotiable constraints (load-bearing for **rebuild stability**)
///
/// 1. **Caller-supplied, not auto-derived from cad-core internals.** v0
///    takes the owner seed as an explicit constructor argument from the
///    caller (test or downstream consumer). cad-core does NOT mint owner
///    seeds and does NOT carry an `From<NodeId> for BRepOwnerId` impl.
/// 2. **The owner seed must NOT be derived from `NodeId` or
///    `effective_hash`.** The whole point of this substrate is to prove
///    rebuild stability across parameter changes. `NodeId` is derived from
///    `(local_hash || port || upstream)` and `effective_hash` from the
///    recursive structural hash of the operator + its parameters; both
///    *change when parameters change* — using either as the owner seed
///    would defeat the rebuild stability property the substrate is built
///    to prove.
///
/// # Wire format
///
/// 16 bytes is enough entropy for the v0 vocabulary substrate; if
/// downstream callers want stronger collision resistance, they can derive
/// their 16-byte owner from a wider source (UUID v4, BLAKE3 over a
/// caller-defined string, etc.) externally. cad-core does not prescribe
/// the upstream derivation, only the byte width.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BRepOwnerId([u8; 16]);

impl BRepOwnerId {
    /// Construct a [`BRepOwnerId`] from 16 raw bytes.
    ///
    /// `const` so callers can build well-known sentinel ids at compile
    /// time. Mirrors `kernel/asset-view::AssetViewId` /
    /// `kernel/io-scheduler::IoRequestId` / `kernel/job-system::JobId`
    /// precedent.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Returns a borrow of the underlying byte array.
    ///
    /// `const` so the borrow flows through `const fn` consumers without
    /// needing a runtime-evaluated copy.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

// ---------------------------------------------------------------------------
// BRepFaceId
// ---------------------------------------------------------------------------

/// BLAKE3-derived stable face identity. 16 bytes.
///
/// Derivation:
///
/// ```text
/// BRepFaceId = first 16 bytes of BLAKE3(
///     b"rge.cad.brep.face/v1:" || owner.as_bytes() || kind_tag_bytes
/// )
/// ```
///
/// where `kind_tag_bytes` is a deterministic byte encoding of
/// `(operator_kind_string, face_tag_discriminant)`.
///
/// The `operator_kind_string` is a literal byte-string (e.g. `b"cuboid:"`)
/// — NOT [`crate::operators::OpKind`]'s numeric discriminant — so that
/// future variant reordering on `OpKind` does not break stability for
/// callers who already serialized old [`BRepFaceId`]s.
///
/// 16 bytes matches the owner-id width and is overkill for collision
/// resistance at the per-graph scale (a graph with 2^32 unique faces still
/// has ~2^-32 collision probability per pair).
///
/// # Wire stability
///
/// The byte layout is opaque. Callers should treat this type as a pure
/// handle. The derivation contract above is the substrate's stable surface,
/// not the byte order itself. The `v1` suffix in the domain separator
/// reserves room for a future migration; consumers MUST NOT decode the
/// bytes back into operator-kind / face-tag components.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BRepFaceId([u8; 16]);

impl BRepFaceId {
    /// Domain separator prefix. **Do not change** without a coordinated
    /// `v2` migration — every previously-serialized [`BRepFaceId`] depends
    /// on these bytes being byte-identical across processes.
    const DOMAIN: &'static [u8] = b"rge.cad.brep.face/v1:";

    /// Operator-kind tag for `CuboidOp`. A literal byte-string so future
    /// `OpKind` variant reordering cannot break stability.
    const KIND_CUBOID: &'static [u8] = b"cuboid:";

    /// Operator-kind tag for `ExtrudeOp`. A literal byte-string so future
    /// `OpKind` variant reordering cannot break stability. Distinct from
    /// [`Self::KIND_CUBOID`] so the two operator-kind identity spaces are
    /// disjoint even under the same `(owner, tag-discriminant)` pair.
    const KIND_EXTRUDE: &'static [u8] = b"extrude:";

    /// Operator-kind tag for `RevolveOp`. A literal byte-string so future
    /// `OpKind` variant reordering cannot break stability. Distinct from
    /// [`Self::KIND_CUBOID`] and [`Self::KIND_EXTRUDE`] so the three
    /// operator-kind identity spaces are disjoint even under the same
    /// `(owner, tag-discriminant)` pair.
    const KIND_REVOLVE: &'static [u8] = b"revolve:";

    /// Operator-kind tag for `LoftOp`. A literal byte-string so future
    /// `OpKind` variant reordering cannot break stability. Distinct from
    /// [`Self::KIND_CUBOID`], [`Self::KIND_EXTRUDE`], and
    /// [`Self::KIND_REVOLVE`] so the four operator-kind identity spaces
    /// are disjoint even under the same `(owner, tag-discriminant)` pair.
    const KIND_LOFT: &'static [u8] = b"loft:";

    /// Operator-kind tag for `SweepOp`. A literal byte-string so future
    /// `OpKind` variant reordering cannot break stability. Distinct from
    /// [`Self::KIND_CUBOID`], [`Self::KIND_EXTRUDE`], [`Self::KIND_REVOLVE`],
    /// and [`Self::KIND_LOFT`] so the five operator-kind identity spaces are
    /// disjoint even under the same `(owner, tag-discriminant)` pair.
    const KIND_SWEEP: &'static [u8] = b"sweep:";

    /// Returns a borrow of the underlying 16-byte identity.
    ///
    /// Exposed for low-level use cases (test cross-checks, byte-level
    /// equality assertions). Most callers should compare [`BRepFaceId`]
    /// values directly via `==`.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topology::face_tag::CuboidFaceTag;

    #[test]
    fn owner_id_round_trips_through_bytes() {
        let bytes = [
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
            0xee, 0xff,
        ];
        let owner = BRepOwnerId::from_bytes(bytes);
        assert_eq!(owner.as_bytes(), &bytes);
    }

    #[test]
    fn owner_id_serde_round_trips() {
        let owner = BRepOwnerId::from_bytes([0xa5u8; 16]);
        let s = ron::to_string(&owner).expect("serialize");
        let decoded: BRepOwnerId = ron::from_str(&s).expect("deserialize");
        assert_eq!(owner, decoded);
    }

    #[test]
    fn face_id_serde_round_trips() {
        let owner = BRepOwnerId::from_bytes([0x42u8; 16]);
        let id = BRepFaceId::for_cuboid_face(owner, CuboidFaceTag::PosZ);
        let s = ron::to_string(&id).expect("serialize");
        let decoded: BRepFaceId = ron::from_str(&s).expect("deserialize");
        assert_eq!(id, decoded);
    }

    #[test]
    fn owner_zero_max_distinct() {
        let zero = BRepOwnerId::from_bytes([0u8; 16]);
        let max = BRepOwnerId::from_bytes([0xffu8; 16]);
        assert_ne!(zero, max);
        let id_zero = BRepFaceId::for_cuboid_face(zero, CuboidFaceTag::NegZ);
        let id_max = BRepFaceId::for_cuboid_face(max, CuboidFaceTag::NegZ);
        assert_ne!(id_zero, id_max);
    }
}
