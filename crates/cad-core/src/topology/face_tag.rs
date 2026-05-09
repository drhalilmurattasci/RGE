//! Per-operator face-tag enums.
//!
//! In v0 only [`CuboidFaceTag`] exists. Per-operator face-tag enums for
//! `ExtrudeOp` / `RevolveOp` / `BooleanOp` / `LoftOp` / `SweepOp` / `TransformOp`
//! are explicitly OUT OF SCOPE for sub-7.2-α and land in subsequent
//! sub-dispatches when each operator's `BRepProvider` impl ships.

use serde::{Deserialize, Serialize};

/// Face-tag enumeration for [`crate::operators::CuboidOp`].
///
/// The variant order matches the canonical face-emission order of
/// `CuboidOp::evaluate`:
///
/// ```text
/// 0: NegZ  (back, -Z normal)
/// 1: PosZ  (front, +Z normal)
/// 2: NegY  (bottom, -Y normal)
/// 3: PosY  (top, +Y normal)
/// 4: NegX  (left, -X normal)
/// 5: PosX  (right, +X normal)
/// ```
///
/// **Do not reorder** these variants in future revisions — the discriminant
/// (and therefore the derived [`crate::topology::BRepFaceId`]) is byte-stable
/// only as long as the variant ordering is preserved. Rebuild-stability for
/// callers who already serialized old IDs depends on this invariant.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CuboidFaceTag {
    /// `-Z` face — back of the box (outward normal `(0, 0, -1)`).
    NegZ,
    /// `+Z` face — front of the box (outward normal `(0, 0, +1)`).
    PosZ,
    /// `-Y` face — bottom of the box (outward normal `(0, -1, 0)`).
    NegY,
    /// `+Y` face — top of the box (outward normal `(0, +1, 0)`).
    PosY,
    /// `-X` face — left side of the box (outward normal `(-1, 0, 0)`).
    NegX,
    /// `+X` face — right side of the box (outward normal `(+1, 0, 0)`).
    PosX,
}

impl CuboidFaceTag {
    /// Frozen `u8` discriminant that feeds the BLAKE3 derivation in
    /// [`crate::topology::BRepFaceId::for_cuboid_face`].
    ///
    /// Frozen at:
    ///
    /// ```text
    /// NegZ = 0, PosZ = 1, NegY = 2, PosY = 3, NegX = 4, PosX = 5
    /// ```
    ///
    /// These discriminants are part of the stable id substrate's wire surface
    /// and MUST NOT change without a `v2` migration in the domain separator.
    #[must_use]
    pub const fn discriminant(self) -> u8 {
        match self {
            CuboidFaceTag::NegZ => 0,
            CuboidFaceTag::PosZ => 1,
            CuboidFaceTag::NegY => 2,
            CuboidFaceTag::PosY => 3,
            CuboidFaceTag::NegX => 4,
            CuboidFaceTag::PosX => 5,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discriminant_matches_canonical_emission_order() {
        // The frozen discriminants must match the order CuboidOp emits faces
        // in `evaluate` (-Z, +Z, -Y, +Y, -X, +X). This test pins the contract.
        assert_eq!(CuboidFaceTag::NegZ.discriminant(), 0);
        assert_eq!(CuboidFaceTag::PosZ.discriminant(), 1);
        assert_eq!(CuboidFaceTag::NegY.discriminant(), 2);
        assert_eq!(CuboidFaceTag::PosY.discriminant(), 3);
        assert_eq!(CuboidFaceTag::NegX.discriminant(), 4);
        assert_eq!(CuboidFaceTag::PosX.discriminant(), 5);
    }

    #[test]
    fn serde_round_trip_preserves_variant() {
        for tag in [
            CuboidFaceTag::NegZ,
            CuboidFaceTag::PosZ,
            CuboidFaceTag::NegY,
            CuboidFaceTag::PosY,
            CuboidFaceTag::NegX,
            CuboidFaceTag::PosX,
        ] {
            let s = ron::to_string(&tag).expect("serialize");
            let decoded: CuboidFaceTag = ron::from_str(&s).expect("deserialize");
            assert_eq!(tag, decoded);
        }
    }

    #[test]
    #[allow(
        unreachable_patterns,
        reason = "intentional: simulates cross-crate consumer pattern; \
                  same-crate compilation sees the enum as exhaustive so the \
                  wildcard arm is unreachable from inside the crate, but the \
                  `#[non_exhaustive]` SemVer barrier requires it for external \
                  consumers"
    )]
    fn non_exhaustive_pattern_match_compiles() {
        let tag = CuboidFaceTag::NegZ;
        let _label = match tag {
            CuboidFaceTag::NegZ => "neg-z",
            CuboidFaceTag::PosZ => "pos-z",
            CuboidFaceTag::NegY => "neg-y",
            CuboidFaceTag::PosY => "pos-y",
            CuboidFaceTag::NegX => "neg-x",
            CuboidFaceTag::PosX => "pos-x",
            _ => "future-variant",
        };
    }
}
