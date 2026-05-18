//! `BRepFaceId` constructor + tests for `SweepOp` (Sweep face-identity slice).

use super::{BRepFaceId, BRepOwnerId};
use crate::topology::face_tag::SweepFaceTag;

impl BRepFaceId {
    /// Construct a [`BRepFaceId`] for one face of a `SweepOp` instance
    /// (Sweep face-identity slice).
    ///
    /// `owner` is the caller-supplied owner seed (see [`BRepOwnerId`] for
    /// the non-negotiable constraints on its provenance). `tag` selects
    /// which face of the swept solid this id represents — `FirstCap`,
    /// `LastCap`, or `Side { segment_index, edge_index, profile_count,
    /// path_segment_count }`.
    ///
    /// # BLAKE3 input layout
    ///
    /// ```text
    /// BLAKE3(
    ///     b"rge.cad.brep.face/v1:" ||  // domain separator
    ///     owner.as_bytes() ||           // 16 bytes
    ///     b"sweep:" ||                  // operator-kind separator
    ///     tag_discriminant_byte ||      // 0 = FirstCap, 1 = LastCap, 2 = Side
    ///     /* Side ONLY: */ segment_index.to_le_bytes() ||       // 4 bytes
    ///     /* Side ONLY: */ edge_index.to_le_bytes() ||          // 4 bytes
    ///     /* Side ONLY: */ profile_count.to_le_bytes() ||       // 4 bytes
    ///     /* Side ONLY: */ path_segment_count.to_le_bytes()     // 4 bytes
    /// )
    /// ```
    ///
    /// then truncated to the first 16 bytes. For `FirstCap` / `LastCap` the
    /// BLAKE3 input ends after the discriminant byte (no inner data) — so
    /// caps are categorical stable identities for the owner and operator
    /// kind, unaffected by any Sweep numeric path coordinate and by
    /// profile-count / path-segment-count topology changes (same explicit
    /// limit as [`crate::topology::ExtrudeFaceTag`]'s caps). For `Side`, all
    /// four of `segment_index`, `edge_index`, `profile_count`, and
    /// `path_segment_count` are appended in little-endian order; both count
    /// fields are hashed so profile-count and path-segment-count topology
    /// changes break `Side` IDs by construction (see [`SweepFaceTag`] for
    /// the full stability contract).
    #[must_use]
    pub fn for_sweep_face(owner: BRepOwnerId, tag: SweepFaceTag) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(Self::DOMAIN);
        hasher.update(owner.as_bytes());
        hasher.update(Self::KIND_SWEEP);
        hasher.update(&[tag.discriminant()]);
        if let SweepFaceTag::Side {
            segment_index,
            edge_index,
            profile_count,
            path_segment_count,
        } = tag
        {
            hasher.update(&segment_index.to_le_bytes());
            hasher.update(&edge_index.to_le_bytes());
            hasher.update(&profile_count.to_le_bytes());
            hasher.update(&path_segment_count.to_le_bytes());
        }
        let full = hasher.finalize();
        let mut truncated = [0u8; 16];
        truncated.copy_from_slice(&full.as_bytes()[..16]);
        Self(truncated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::topology::face_tag::ExtrudeFaceTag;

    fn side(
        segment_index: u32,
        edge_index: u32,
        profile_count: u32,
        path_segment_count: u32,
    ) -> SweepFaceTag {
        SweepFaceTag::Side {
            segment_index,
            edge_index,
            profile_count,
            path_segment_count,
        }
    }

    #[test]
    fn for_sweep_face_deterministic() {
        // Same `(owner, tag)` produces identical bytes across calls. Repeats
        // for FirstCap / LastCap / Side to make the determinism contract
        // per-variant.
        let owner = BRepOwnerId::from_bytes([0x42u8; 16]);
        for tag in [
            SweepFaceTag::FirstCap,
            SweepFaceTag::LastCap,
            side(0, 0, 4, 1),
            side(2, 3, 5, 3),
        ] {
            let a = BRepFaceId::for_sweep_face(owner, tag);
            let b = BRepFaceId::for_sweep_face(owner, tag);
            assert_eq!(a, b, "for_sweep_face({tag:?}) is not deterministic");
            assert_eq!(a.as_bytes(), b.as_bytes());
        }
    }

    #[test]
    fn for_sweep_face_distinct_across_tag_kinds() {
        // FirstCap, LastCap, and Side {0, 0, 4, 1} all distinct under the
        // same owner.
        let owner = BRepOwnerId::from_bytes([0x42u8; 16]);
        let first = BRepFaceId::for_sweep_face(owner, SweepFaceTag::FirstCap);
        let last = BRepFaceId::for_sweep_face(owner, SweepFaceTag::LastCap);
        let s = BRepFaceId::for_sweep_face(owner, side(0, 0, 4, 1));
        assert_ne!(first, last);
        assert_ne!(first, s);
        assert_ne!(last, s);
    }

    #[test]
    fn for_sweep_face_distinct_across_owners() {
        // Same tag, different owners → different ID. Mirrors the cuboid /
        // extrude / revolve / loft owner-disambiguation precedent.
        let owner_a = BRepOwnerId::from_bytes([0x11u8; 16]);
        let owner_b = BRepOwnerId::from_bytes([0x22u8; 16]);
        for tag in [
            SweepFaceTag::FirstCap,
            SweepFaceTag::LastCap,
            side(0, 0, 4, 1),
        ] {
            let id_a = BRepFaceId::for_sweep_face(owner_a, tag);
            let id_b = BRepFaceId::for_sweep_face(owner_b, tag);
            assert_ne!(id_a, id_b, "owner-disambiguation failed for {tag:?}");
        }
    }

    /// **Substrate-honesty test #1 for the Sweep slice: profile-count
    /// changes break `Side` IDs.**
    ///
    /// `Side { profile_count: 4, .. }` and `Side { profile_count: 5, .. }`
    /// (all other fields equal) MUST produce DIFFERENT [`BRepFaceId`]s. The
    /// substrate hashes `profile_count.to_le_bytes()` into the BLAKE3 input
    /// precisely so a square → pentagon topology change is NOT silently
    /// preserved.
    #[test]
    fn for_sweep_face_profile_count_change_breaks_side_id() {
        let owner = BRepOwnerId::from_bytes([0x42u8; 16]);
        let side_count_4 = BRepFaceId::for_sweep_face(owner, side(0, 0, 4, 1));
        let side_count_5 = BRepFaceId::for_sweep_face(owner, side(0, 0, 5, 1));
        assert_ne!(
            side_count_4, side_count_5,
            "side IDs must NOT be preserved across profile-count changes"
        );
    }

    /// **Substrate-honesty test #2 for the Sweep slice: path-segment-count
    /// changes break `Side` IDs.**
    ///
    /// `Side { path_segment_count: 1, .. }` and
    /// `Side { path_segment_count: 2, .. }` (all other fields equal) MUST
    /// produce DIFFERENT [`BRepFaceId`]s. This is the second topology axis
    /// Sweep has that `ExtrudeFaceTag` never did — extending the path with
    /// an extra point changes the side-identity space by construction.
    #[test]
    fn for_sweep_face_path_segment_count_change_breaks_side_id() {
        let owner = BRepOwnerId::from_bytes([0x42u8; 16]);
        let segs_1 = BRepFaceId::for_sweep_face(owner, side(0, 0, 4, 1));
        let segs_2 = BRepFaceId::for_sweep_face(owner, side(0, 0, 4, 2));
        assert_ne!(
            segs_1, segs_2,
            "side IDs must NOT be preserved across path-segment-count changes"
        );
    }

    /// Cross-operator separator check: the literal byte-strings
    /// `b"extrude:"` (sub-7.2-β) and `b"sweep:"` (Sweep slice) MUST produce
    /// disjoint identity spaces even when the BLAKE3 input is otherwise
    /// identical. This pins the operator-kind separator's load-bearing role
    /// for the fifth per-operator face-tag substrate — Sweep `Side` IDs are
    /// disjoint from at least one existing non-Sweep namespace.
    #[test]
    fn for_sweep_face_distinct_from_for_extrude_face_with_same_owner() {
        let owner = BRepOwnerId::from_bytes([0x42u8; 16]);
        // Both Extrude `Bottom` and Sweep `FirstCap` carry no inner data and
        // share discriminant byte 0. The only thing distinguishing the
        // BLAKE3 inputs is the operator-kind separator.
        let extrude_bottom = BRepFaceId::for_extrude_face(owner, ExtrudeFaceTag::Bottom);
        let sweep_first = BRepFaceId::for_sweep_face(owner, SweepFaceTag::FirstCap);
        assert_ne!(
            extrude_bottom, sweep_first,
            "operator-kind separator must produce disjoint identity spaces \
             across extrude and sweep"
        );
    }
}
