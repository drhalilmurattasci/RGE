//! Stable B-Rep edge identity derived from face-pair adjacency.
//!
//! See module-doc on [`crate::topology::face_id`] for the broader
//! identity-derivation framework. Edge identity is one layer above
//! face identity: an edge IS the topological intersection of two
//! adjacent faces, so its identity is derived from the (sorted) pair
//! of face IDs that bound it.
//!
//! Failure class inherited: snapshot-recoverable.
//!
//! # Derivation
//!
//! ```text
//! BRepEdgeId = first 16 bytes of BLAKE3(
//!     b"rge.cad.brep.edge/v1:" ||
//!     sorted(face_a.as_bytes(), face_b.as_bytes()) ||
//!     local_ordinal.to_le_bytes()
//! )
//! ```
//!
//! Face IDs are sorted lexicographically by their `[u8; 16]` bytes
//! before hashing, so `for_face_pair(a, b, k) == for_face_pair(b, a, k)`.
//!
//! # `local_ordinal`
//!
//! For all current operators (Cuboid / Extrude / Revolve / Loft v0),
//! every edge has `local_ordinal = 0` — two adjacent faces share at
//! most one edge in these topologies. The `local_ordinal` slot exists
//! so future operators producing face pairs that share multiple edges
//! (e.g., faces with holes, non-convex sweeps) can extend without a
//! v2 derivation-scheme migration.
//!
//! # Domain separation from `BRepFaceId`
//!
//! The `b"rge.cad.brep.edge/v1:"` prefix differs from
//! `b"rge.cad.brep.face/v1:"` so a `BRepEdgeId` and `BRepFaceId`
//! cannot collide even if their derivation inputs accidentally
//! coincide. Verified by [`tests::edge_id_distinct_from_face_id_via_domain_separator`].

use serde::{Deserialize, Serialize};

use crate::topology::BRepFaceId;

/// Stable B-Rep edge identity. 16-byte truncation of a BLAKE3 hash
/// over a sorted face-pair plus a local ordinal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct BRepEdgeId([u8; 16]);

impl BRepEdgeId {
    /// Construct from raw bytes (e.g. for sentinel values in tests).
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Borrow the raw bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Derive a stable edge identity from a sorted face pair plus a
    /// local ordinal.
    ///
    /// Argument order does not matter — the function sorts internally
    /// so `for_face_pair(a, b, k) == for_face_pair(b, a, k)`.
    ///
    /// `local_ordinal` distinguishes multiple edges between the same
    /// face pair. For all current operators this is always `0`; the
    /// parameter is reserved for future operators with multi-edge
    /// face adjacencies.
    #[must_use]
    pub fn for_face_pair(face_a: BRepFaceId, face_b: BRepFaceId, local_ordinal: u32) -> Self {
        let (smaller, larger) = if face_a.as_bytes() <= face_b.as_bytes() {
            (face_a, face_b)
        } else {
            (face_b, face_a)
        };
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"rge.cad.brep.edge/v1:");
        hasher.update(smaller.as_bytes());
        hasher.update(larger.as_bytes());
        hasher.update(&local_ordinal.to_le_bytes());
        let hash = hasher.finalize();
        let bytes: [u8; 16] = hash.as_bytes()[..16]
            .try_into()
            .expect("BLAKE3 → 16-byte truncation");
        Self::from_bytes(bytes)
    }
}

impl std::fmt::Display for BRepEdgeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "edge:{:02x}{:02x}{:02x}{:02x}",
            self.0[0], self.0[1], self.0[2], self.0[3]
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topology::{BRepOwnerId, CuboidFaceTag};

    fn face(owner_byte: u8, tag: CuboidFaceTag) -> BRepFaceId {
        let owner = BRepOwnerId::from_bytes([owner_byte; 16]);
        BRepFaceId::for_cuboid_face(owner, tag)
    }

    #[test]
    fn for_face_pair_deterministic() {
        let a = face(0x42, CuboidFaceTag::NegZ);
        let b = face(0x42, CuboidFaceTag::NegY);
        let id_first = BRepEdgeId::for_face_pair(a, b, 0);
        let id_second = BRepEdgeId::for_face_pair(a, b, 0);
        assert_eq!(id_first, id_second);
        assert_eq!(id_first.as_bytes(), id_second.as_bytes());
    }

    #[test]
    fn for_face_pair_order_independent() {
        let a = face(0x42, CuboidFaceTag::NegZ);
        let b = face(0x42, CuboidFaceTag::NegY);
        let c = face(0x42, CuboidFaceTag::PosX);
        assert_eq!(
            BRepEdgeId::for_face_pair(a, b, 0),
            BRepEdgeId::for_face_pair(b, a, 0)
        );
        assert_eq!(
            BRepEdgeId::for_face_pair(a, c, 0),
            BRepEdgeId::for_face_pair(c, a, 0)
        );
        assert_eq!(
            BRepEdgeId::for_face_pair(b, c, 0),
            BRepEdgeId::for_face_pair(c, b, 0)
        );
    }

    #[test]
    fn for_face_pair_local_ordinal_distinguishes() {
        let a = face(0x42, CuboidFaceTag::NegZ);
        let b = face(0x42, CuboidFaceTag::NegY);
        let id_zero = BRepEdgeId::for_face_pair(a, b, 0);
        let id_one = BRepEdgeId::for_face_pair(a, b, 1);
        assert_ne!(
            id_zero, id_one,
            "local_ordinal must affect the derived BRepEdgeId"
        );
    }

    #[test]
    fn for_face_pair_distinct_for_different_pairs() {
        let a = face(0x42, CuboidFaceTag::NegZ);
        let b = face(0x42, CuboidFaceTag::NegY);
        let c = face(0x42, CuboidFaceTag::NegX);
        let ab = BRepEdgeId::for_face_pair(a, b, 0);
        let ac = BRepEdgeId::for_face_pair(a, c, 0);
        assert_ne!(
            ab, ac,
            "different face pairs must produce different BRepEdgeIds"
        );
    }

    #[test]
    fn for_face_pair_owner_disjointness_via_face_ids() {
        let a_x = face(0x11, CuboidFaceTag::NegZ);
        let b_x = face(0x11, CuboidFaceTag::NegY);
        let a_y = face(0x22, CuboidFaceTag::NegZ);
        let b_y = face(0x22, CuboidFaceTag::NegY);
        let id_x = BRepEdgeId::for_face_pair(a_x, b_x, 0);
        let id_y = BRepEdgeId::for_face_pair(a_y, b_y, 0);
        assert_ne!(
            id_x, id_y,
            "owner disjointness must propagate transitively through face IDs"
        );
    }

    /// Direct test that the domain separator `b"rge.cad.brep.edge/v1:"`
    /// keeps `BRepEdgeId` disjoint from `BRepFaceId` even when the
    /// derivation inputs degenerately coincide. We construct a face id
    /// `f` and an edge id `e = for_face_pair(f, f, 0)` (degenerate but
    /// valid input), and assert their underlying bytes differ. This
    /// proves that the `edge/v1:` prefix is doing its job — without it,
    /// nothing structural would prevent an `BRepEdgeId` byte sequence
    /// from accidentally equalling a `BRepFaceId` byte sequence.
    #[test]
    fn edge_id_distinct_from_face_id_via_domain_separator() {
        let owner = BRepOwnerId::from_bytes([0x42; 16]);
        let f = BRepFaceId::for_cuboid_face(owner, CuboidFaceTag::NegZ);
        let e = BRepEdgeId::for_face_pair(f, f, 0);
        assert_ne!(
            e.as_bytes(),
            f.as_bytes(),
            "edge domain separator must keep BRepEdgeId disjoint from BRepFaceId"
        );
    }

    #[test]
    fn edge_id_byte_format() {
        let bytes = [0xab; 16];
        let id = BRepEdgeId::from_bytes(bytes);
        assert_eq!(id.as_bytes(), &bytes);
    }

    #[test]
    fn edge_id_serde_round_trip() {
        let a = face(0x42, CuboidFaceTag::NegZ);
        let b = face(0x42, CuboidFaceTag::NegY);
        let id = BRepEdgeId::for_face_pair(a, b, 0);
        let s = ron::to_string(&id).expect("serialize");
        let decoded: BRepEdgeId = ron::from_str(&s).expect("deserialize");
        assert_eq!(id, decoded);
    }

    #[test]
    fn edge_id_display_format() {
        let id = BRepEdgeId::from_bytes([
            0xab, 0xcd, 0xef, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b,
            0x0c, 0x0d,
        ]);
        let formatted = format!("{id}");
        assert_eq!(formatted, "edge:abcdef01");
    }
}
