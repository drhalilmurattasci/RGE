//! Per-operator face-tag enums.
//!
//! In sub-7.2-α only [`CuboidFaceTag`] existed. Sub-7.2-β added
//! [`ExtrudeFaceTag`] — the second per-operator face-tag enum, this time
//! with **variable topology** (`N + 2` faces depending on profile vertex
//! count). Sub-7.2-γ added [`RevolveMode`] + [`RevolveFaceTag`] — the third
//! per-operator face-tag enum, exercising a topology axis no prior dispatch
//! had touched: a **categorical mode change** (Full vs Partial revolution)
//! that alters the *face set itself* (Full has no caps; Partial has caps).
//! Sub-7.2-δ adds [`LoftFaceTag`] — the fourth per-operator face-tag enum,
//! exercising the **two-input local-provider** topology axis: `LoftOp` is
//! the first operator with two profiles, and the substrate handles this
//! without leaking into chain-composition territory (sub-7.2-ε). Per-
//! operator face-tag enums for the remaining operators (`BooleanOp` /
//! `SweepOp` / `TransformOp`) are explicitly OUT OF SCOPE for sub-7.2-δ and
//! land in subsequent sub-dispatches when each operator's `BRepProvider`
//! impl ships.

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

    // -----------------------------------------------------------------------
    // ExtrudeFaceTag tests (sub-7.2-β)
    // -----------------------------------------------------------------------

    #[test]
    fn extrude_face_tag_serde_round_trip() {
        for tag in [
            ExtrudeFaceTag::Bottom,
            ExtrudeFaceTag::Top,
            ExtrudeFaceTag::Side {
                edge_index: 0,
                profile_count: 4,
            },
            ExtrudeFaceTag::Side {
                edge_index: 3,
                profile_count: 4,
            },
            ExtrudeFaceTag::Side {
                edge_index: 0,
                profile_count: 5,
            },
        ] {
            let s = ron::to_string(&tag).expect("serialize");
            let decoded: ExtrudeFaceTag = ron::from_str(&s).expect("deserialize");
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
    fn extrude_face_tag_non_exhaustive_pattern_compiles() {
        let tag = ExtrudeFaceTag::Bottom;
        let _label = match tag {
            ExtrudeFaceTag::Bottom => "bottom",
            ExtrudeFaceTag::Top => "top",
            ExtrudeFaceTag::Side { .. } => "side",
            _ => "future-variant",
        };
    }

    #[test]
    fn extrude_side_distinct_for_distinct_edge_indices() {
        // Constructor-level — verify `Side { edge_index: 0, count: 4 }` and
        // `Side { edge_index: 1, count: 4 }` produce distinct tag values via
        // PartialEq. This pins the tag-level distinctness independently of
        // the BLAKE3 derivation that face_id.rs tests cover.
        let s0 = ExtrudeFaceTag::Side {
            edge_index: 0,
            profile_count: 4,
        };
        let s1 = ExtrudeFaceTag::Side {
            edge_index: 1,
            profile_count: 4,
        };
        assert_ne!(s0, s1);

        // Cross-check: same (edge_index, profile_count) ARE equal.
        let s0_again = ExtrudeFaceTag::Side {
            edge_index: 0,
            profile_count: 4,
        };
        assert_eq!(s0, s0_again);

        // Same edge_index, different profile_count, also distinct.
        let s0_other_count = ExtrudeFaceTag::Side {
            edge_index: 0,
            profile_count: 5,
        };
        assert_ne!(s0, s0_other_count);

        // Bottom and Top are distinct from any Side and from each other.
        assert_ne!(ExtrudeFaceTag::Bottom, ExtrudeFaceTag::Top);
        assert_ne!(ExtrudeFaceTag::Bottom, s0);
        assert_ne!(ExtrudeFaceTag::Top, s0);
    }

    // -----------------------------------------------------------------------
    // RevolveFaceTag + RevolveMode tests (sub-7.2-γ)
    // -----------------------------------------------------------------------

    #[test]
    fn revolve_face_tag_serde_round_trip() {
        // Cover all 4 distinct test points: Full Side, Partial Side, StartCap,
        // EndCap.
        for tag in [
            RevolveFaceTag::Side {
                side_index: 0,
                profile_count: 4,
                segment_count: 8,
                mode: RevolveMode::Full,
            },
            RevolveFaceTag::Side {
                side_index: 2,
                profile_count: 4,
                segment_count: 8,
                mode: RevolveMode::Partial,
            },
            RevolveFaceTag::StartCap { profile_count: 4 },
            RevolveFaceTag::EndCap { profile_count: 4 },
        ] {
            let s = ron::to_string(&tag).expect("serialize");
            let decoded: RevolveFaceTag = ron::from_str(&s).expect("deserialize");
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
    fn revolve_face_tag_non_exhaustive_pattern_compiles() {
        let tag = RevolveFaceTag::StartCap { profile_count: 4 };
        let _label = match tag {
            RevolveFaceTag::Side { .. } => "side",
            RevolveFaceTag::StartCap { .. } => "start-cap",
            RevolveFaceTag::EndCap { .. } => "end-cap",
            _ => "future-variant",
        };
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
    fn revolve_mode_non_exhaustive_pattern_compiles() {
        let mode = RevolveMode::Full;
        let _label = match mode {
            RevolveMode::Full => "full",
            RevolveMode::Partial => "partial",
            _ => "future-variant",
        };
    }

    #[test]
    fn revolve_side_distinct_for_distinct_modes() {
        // Side {Full} and Side {Partial} with otherwise-identical inner data
        // must produce distinct tag values via PartialEq. Pins tag-level
        // mode-distinctness independently of the BLAKE3 derivation that
        // face_id.rs tests cover.
        let s_full = RevolveFaceTag::Side {
            side_index: 0,
            profile_count: 4,
            segment_count: 8,
            mode: RevolveMode::Full,
        };
        let s_partial = RevolveFaceTag::Side {
            side_index: 0,
            profile_count: 4,
            segment_count: 8,
            mode: RevolveMode::Partial,
        };
        assert_ne!(s_full, s_partial);
    }

    // -----------------------------------------------------------------------
    // LoftFaceTag tests (sub-7.2-δ)
    // -----------------------------------------------------------------------

    #[test]
    fn loft_face_tag_serde_round_trip() {
        // Cover all 3 distinct test points: Bottom, Top, Side {0, 4, 4},
        // plus a Side variant with unequal counts (substrate-honesty: even
        // though LoftOp::evaluate rejects unequal counts at runtime, the tag
        // serde round-trips cleanly through the constructor surface).
        for tag in [
            LoftFaceTag::Bottom,
            LoftFaceTag::Top,
            LoftFaceTag::Side {
                edge_index: 0,
                profile_a_count: 4,
                profile_b_count: 4,
            },
            LoftFaceTag::Side {
                edge_index: 3,
                profile_a_count: 5,
                profile_b_count: 5,
            },
            LoftFaceTag::Side {
                edge_index: 0,
                profile_a_count: 4,
                profile_b_count: 5,
            },
        ] {
            let s = ron::to_string(&tag).expect("serialize");
            let decoded: LoftFaceTag = ron::from_str(&s).expect("deserialize");
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
    fn loft_face_tag_non_exhaustive_pattern_compiles() {
        let tag = LoftFaceTag::Bottom;
        let _label = match tag {
            LoftFaceTag::Bottom => "bottom",
            LoftFaceTag::Top => "top",
            LoftFaceTag::Side { .. } => "side",
            _ => "future-variant",
        };
    }

    #[test]
    fn loft_side_distinct_for_distinct_edge_indices() {
        // Constructor-level — verify Side {edge_index: 0, 4, 4} and
        // Side {edge_index: 1, 4, 4} produce distinct tag values via
        // PartialEq. Pins tag-level distinctness independently of the
        // BLAKE3 derivation that face_id.rs tests cover.
        let s0 = LoftFaceTag::Side {
            edge_index: 0,
            profile_a_count: 4,
            profile_b_count: 4,
        };
        let s1 = LoftFaceTag::Side {
            edge_index: 1,
            profile_a_count: 4,
            profile_b_count: 4,
        };
        assert_ne!(s0, s1);

        // Cross-check: same fields ARE equal.
        let s0_again = LoftFaceTag::Side {
            edge_index: 0,
            profile_a_count: 4,
            profile_b_count: 4,
        };
        assert_eq!(s0, s0_again);

        // Bottom and Top are distinct from any Side and from each other.
        assert_ne!(LoftFaceTag::Bottom, LoftFaceTag::Top);
        assert_ne!(LoftFaceTag::Bottom, s0);
        assert_ne!(LoftFaceTag::Top, s0);
    }

    /// **Substrate-honesty test #1 at the tag level (sub-7.2-δ).**
    ///
    /// `Side { edge_index, profile_a_count, profile_b_count }` MUST treat
    /// BOTH counts as independent distinguishers. The four combinations
    /// `(4, 4) / (5, 4) / (4, 5) / (5, 5)` produce four distinct tags via
    /// PartialEq. This pins the substrate-honesty guardrail at the enum
    /// level: a hypothetical malformed in-memory `LoftOp` (mid-mutation
    /// through pub fields) MUST NOT collide tag identity by silently
    /// collapsing one count into the other.
    #[test]
    fn loft_side_distinct_for_distinct_profile_counts() {
        let s_4_4 = LoftFaceTag::Side {
            edge_index: 0,
            profile_a_count: 4,
            profile_b_count: 4,
        };
        let s_5_4 = LoftFaceTag::Side {
            edge_index: 0,
            profile_a_count: 5,
            profile_b_count: 4,
        };
        let s_4_5 = LoftFaceTag::Side {
            edge_index: 0,
            profile_a_count: 4,
            profile_b_count: 5,
        };
        let s_5_5 = LoftFaceTag::Side {
            edge_index: 0,
            profile_a_count: 5,
            profile_b_count: 5,
        };
        assert_ne!(s_4_4, s_5_4);
        assert_ne!(s_4_4, s_4_5);
        assert_ne!(s_4_4, s_5_5);
        assert_ne!(s_5_4, s_4_5);
        assert_ne!(s_5_4, s_5_5);
        assert_ne!(s_4_5, s_5_5);
    }
}

// ---------------------------------------------------------------------------
// ExtrudeFaceTag (sub-7.2-β)
// ---------------------------------------------------------------------------

/// Face-tag enumeration for [`crate::operators::ExtrudeOp`].
///
/// `ExtrudeOp` has **variable topology**: a profile of `N` vertices produces
/// `N + 2` faces in the canonical emission order
/// `Bottom (1 face) → Top (1 face) → Side(0..N-1) (N faces)`. The variant
/// order matches that emission order; the discriminant pinned in
/// [`ExtrudeFaceTag::discriminant`] freezes
/// `Bottom = 0`, `Top = 1`, `Side = 2`. The inner data of `Side` is
/// BLAKE3-hashed (NOT used as the discriminant byte).
///
/// # Stability contract (load-bearing)
///
/// 1. **Bottom and Top IDs are stable across `length` parameter changes.**
///    The substrate hashes only the discriminant byte for these two
///    variants, not the operator's parameters, so changing `length` from
///    `1.0` to `2.0` does NOT invalidate face identity for the caps.
/// 2. **`Side { edge_index, profile_count }` IDs are stable when both
///    `edge_index` and `profile_count` are unchanged.** Changing only
///    `length` does not invalidate any side's identity, mirroring the cap
///    behaviour.
/// 3. **Profile-count changes break `Side` IDs by construction.** The
///    `profile_count` field is hashed into the BLAKE3 input; a square
///    (`profile_count = 4`) and a pentagon (`profile_count = 5`) produce
///    disjoint side-identity spaces because the input bytes differ. This is
///    the load-bearing design choice — topology changes are NOT silently
///    preserved.
/// 4. **Profile-vertex-order rotation at the same count preserves `Side`
///    IDs.** The substrate does NOT inspect profile coordinates, so a
///    profile rotated from `[A, B, C, D]` to `[B, C, D, A]` will produce
///    the same `Side(0)` ID. This is an explicit limit of the v0
///    substrate; coordinate-aware identity (rotation detection, vertex
///    matching across re-ordering) is OUT OF SCOPE for sub-7.2-β.
///
/// **Do not reorder** the variants in future revisions — the discriminant
/// (and therefore the derived [`crate::topology::BRepFaceId`]) is byte-stable
/// only as long as the variant ordering is preserved. Rebuild-stability for
/// callers who already serialized old IDs depends on this invariant.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ExtrudeFaceTag {
    /// `-Z` cap (bottom of the prism, outward normal `(0, 0, -1)`).
    ///
    /// The discriminant for `Bottom` is `0`. The BLAKE3 input ends after
    /// the discriminant byte — no inner data — so `Bottom` IDs are stable
    /// across `length` AND `profile_count` changes.
    Bottom,
    /// `+Z` cap (top of the prism, outward normal `(0, 0, +1)`).
    ///
    /// The discriminant for `Top` is `1`. The BLAKE3 input ends after the
    /// discriminant byte — no inner data — so `Top` IDs are stable across
    /// `length` AND `profile_count` changes.
    Top,
    /// One side wall of the prism, indexed by the profile edge it spans.
    ///
    /// `edge_index` is the index of the profile edge `(i, i + 1)` mod
    /// `profile_count`, with the canonical emission order
    /// `Side(0), Side(1), ..., Side(profile_count - 1)`. `profile_count` is
    /// the total profile vertex count and is hashed into the BLAKE3 input
    /// alongside `edge_index` so topology changes (square → pentagon) break
    /// face identity by construction.
    ///
    /// The discriminant for `Side` is `2`. The BLAKE3 input appends
    /// `edge_index.to_le_bytes()` (4 bytes) followed by
    /// `profile_count.to_le_bytes()` (4 bytes) after the discriminant byte.
    Side {
        /// Index of the profile edge `(i, i + 1) mod profile_count` this
        /// side wall spans. Range `0..profile_count`.
        edge_index: u32,
        /// Total profile vertex count. Hashed into the BLAKE3 input so
        /// topology changes (e.g. square → pentagon) break face identity
        /// for `Side` variants by construction.
        profile_count: u32,
    },
}

impl ExtrudeFaceTag {
    /// Frozen `u8` discriminant that feeds the BLAKE3 derivation in
    /// [`crate::topology::BRepFaceId::for_extrude_face`].
    ///
    /// Frozen at:
    ///
    /// ```text
    /// Bottom = 0, Top = 1, Side = 2
    /// ```
    ///
    /// The inner data of `Side` (`edge_index`, `profile_count`) is NOT used
    /// as the discriminant — it is appended to the BLAKE3 input separately.
    /// These discriminants are part of the stable id substrate's wire
    /// surface and MUST NOT change without a `v2` migration in the domain
    /// separator.
    #[must_use]
    pub const fn discriminant(self) -> u8 {
        match self {
            ExtrudeFaceTag::Bottom => 0,
            ExtrudeFaceTag::Top => 1,
            ExtrudeFaceTag::Side { .. } => 2,
        }
    }
}

// ---------------------------------------------------------------------------
// RevolveMode (sub-7.2-γ)
// ---------------------------------------------------------------------------

/// Revolution-mode discriminator for [`crate::operators::RevolveOp`].
///
/// Derived at the `BRepProvider` impl site from
/// [`crate::operators::RevolveOp::is_full_revolution`] — NOT a free parameter.
/// `Full` corresponds to `angle == 2π` (no caps emitted; `n` faces total);
/// `Partial` corresponds to `angle < 2π` (caps emitted; `n + 2` faces total).
///
/// # Why mode is hashed into Side identity
///
/// The two modes produce categorically different face sets — `Full` has no
/// caps; `Partial` has start/end caps. This is a topology change in the
/// substrate's identity model. Hashing the mode byte into the `Side` BLAKE3
/// input ensures Side IDs are disjoint across the `Full`/`Partial` boundary
/// (e.g. crossing 359° → 360° flips the mode and produces disjoint Side
/// IDs).
///
/// Discriminants frozen: `Full = 0`, `Partial = 1`. **Do not reorder** these
/// variants in future revisions; the discriminant is part of the stable id
/// substrate's wire surface.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RevolveMode {
    /// `angle == 2π` — full revolution. No caps; surface closes via index
    /// wrap.
    Full,
    /// `angle < 2π` — partial revolution. Start cap (θ=0) and end cap
    /// (θ=angle) are fan-triangulated.
    Partial,
}

impl RevolveMode {
    /// Frozen `u8` discriminant that feeds the BLAKE3 derivation in
    /// [`crate::topology::BRepFaceId::for_revolve_face`] for the `Side`
    /// variant.
    ///
    /// Frozen at:
    ///
    /// ```text
    /// Full = 0, Partial = 1
    /// ```
    ///
    /// These discriminants are part of the stable id substrate's wire
    /// surface and MUST NOT change without a `v2` migration in the domain
    /// separator.
    #[must_use]
    pub const fn discriminant(self) -> u8 {
        match self {
            RevolveMode::Full => 0,
            RevolveMode::Partial => 1,
        }
    }
}

// ---------------------------------------------------------------------------
// RevolveFaceTag (sub-7.2-γ)
// ---------------------------------------------------------------------------

/// Face-tag enumeration for [`crate::operators::RevolveOp`].
///
/// `RevolveOp` introduces a topology axis no prior dispatch has touched: a
/// **categorical mode change** (`Full` vs `Partial` revolution) that alters
/// the *face set itself*, not just the face count. `Full` revolution emits
/// `n` side faces only (no caps; index wrap closes the surface); `Partial`
/// revolution emits `n` side faces + `StartCap` + `EndCap` for `n + 2` total
/// faces. The variant order matches the canonical emission order from
/// [`crate::operators::RevolveOp::evaluate`] for `Partial` mode (sides first,
/// then start cap, then end cap); `Full` mode emits only the side variants.
/// The discriminant pinned in [`RevolveFaceTag::discriminant`] freezes
/// `Side = 0`, `StartCap = 1`, `EndCap = 2`. The inner data of each variant
/// is BLAKE3-hashed (NOT used as the discriminant byte).
///
/// # Stability contract (load-bearing)
///
/// 1. **Side IDs are stable across `angle` numeric changes within the same
///    `mode`.** Within `Partial` mode, three different angles (e.g. 45° →
///    90° → 135°) all preserve Side IDs at the same
///    `(side_index, profile_count, segment_count, mode = Partial)`.
/// 2. **Side IDs break across `mode` changes by construction.** The `mode`
///    byte is hashed into the BLAKE3 input. Crossing the `Full`/`Partial`
///    boundary at 359° → 360° produces disjoint Side IDs.
/// 3. **Side IDs break across `segment_count` changes by construction.**
///    Mirrors [`ExtrudeFaceTag`]'s `profile_count` design — the substrate
///    treats segment count as topology in its identity model. Changing
///    segment_count from 8 to 16 produces disjoint Side IDs.
/// 4. **Cap IDs (`StartCap`, `EndCap`) are stable across `angle` and
///    `segment_count` changes within `Partial` mode.** Caps depend on
///    `profile_count` only — segment count and angle do not affect cap
///    geometry (caps are fan-triangulations of the profile polygon).
///    Including `segment_count` in cap tags would over-encode; substrate
///    honesty says don't.
/// 5. **Cap IDs do not exist in `Full` mode.**
///    [`crate::topology::BRepProvider::brep_face_ids`] returns `n` pairs for
///    `Full` mode and `n + 2` pairs for `Partial` mode.
/// 6. **The substrate does NOT inspect profile coordinates.** Like
///    [`ExtrudeFaceTag`], profile-vertex-order rotation at the same
///    `profile_count` preserves Side IDs. Coordinate-aware identity
///    (rotation detection, axis orientation, vertex matching across
///    re-ordering) is OUT OF SCOPE for v0 — the same explicit limit as
///    [`ExtrudeFaceTag`].
///
/// **Do not reorder** the variants in future revisions — the discriminant
/// (and therefore the derived [`crate::topology::BRepFaceId`]) is byte-stable
/// only as long as the variant ordering is preserved. Rebuild-stability for
/// callers who already serialized old IDs depends on this invariant.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RevolveFaceTag {
    /// One side face of the revolved surface — one per profile edge.
    /// Always present in both `Full` and `Partial` modes.
    ///
    /// `side_index` is the index of the profile edge spanned by this side
    /// face, in canonical emission order `Side(0)..Side(profile_count - 1)`.
    /// `profile_count` is the total profile vertex count and is hashed into
    /// the BLAKE3 input alongside `side_index`. `segment_count` is the
    /// number of rotational steps (per the substrate's "Break IDs across
    /// segment-count changes" directive — the substrate treats segment
    /// count as topology in its identity model). `mode` is hashed in so
    /// crossing the `Full`/`Partial` boundary breaks Side IDs.
    ///
    /// The discriminant for `Side` is `0`. The BLAKE3 input appends
    /// `side_index.to_le_bytes()` (4 bytes) `||
    /// profile_count.to_le_bytes()` (4 bytes) `||
    /// segment_count.to_le_bytes()` (4 bytes) `|| mode.discriminant()`
    /// (1 byte) after the discriminant byte.
    Side {
        /// Index of the profile edge this side face spans. Range
        /// `0..profile_count`.
        side_index: u32,
        /// Total profile vertex count. Hashed into the BLAKE3 input so
        /// profile-count changes break face identity for `Side` variants
        /// by construction (mirrors [`ExtrudeFaceTag::Side`]'s
        /// `profile_count`).
        profile_count: u32,
        /// Number of rotational steps. Hashed into the BLAKE3 input so
        /// segment-count changes break face identity for `Side` variants
        /// by construction. The substrate treats segment count as topology
        /// in its identity model.
        segment_count: u32,
        /// Categorical revolution mode. Hashed into the BLAKE3 input so
        /// crossing the `Full`/`Partial` boundary (e.g. 359° → 360°)
        /// breaks Side IDs by construction.
        mode: RevolveMode,
    },
    /// Start cap (θ = 0). **Partial mode ONLY** — `Full` revolution emits
    /// no caps. The cap is a fan-triangulation of the profile polygon at
    /// θ = 0.
    ///
    /// Caps depend on `profile_count` only — `segment_count` and `angle`
    /// do not affect cap geometry, so they are NOT hashed into the BLAKE3
    /// input. Including them would over-encode; substrate honesty says
    /// don't.
    ///
    /// The discriminant for `StartCap` is `1`. The BLAKE3 input appends
    /// `profile_count.to_le_bytes()` (4 bytes) after the discriminant byte.
    StartCap {
        /// Total profile vertex count. Hashed into the BLAKE3 input so
        /// profile-count changes break cap face identity by construction.
        profile_count: u32,
    },
    /// End cap (θ = `RevolveOp::angle`). **Partial mode ONLY** — `Full`
    /// revolution emits no caps. The cap is a fan-triangulation of the
    /// profile polygon at the end angle.
    ///
    /// Caps depend on `profile_count` only — `segment_count` and `angle`
    /// do not affect cap geometry, so they are NOT hashed into the BLAKE3
    /// input. Including them would over-encode; substrate honesty says
    /// don't.
    ///
    /// The discriminant for `EndCap` is `2`. The BLAKE3 input appends
    /// `profile_count.to_le_bytes()` (4 bytes) after the discriminant byte.
    EndCap {
        /// Total profile vertex count. Hashed into the BLAKE3 input so
        /// profile-count changes break cap face identity by construction.
        profile_count: u32,
    },
}

impl RevolveFaceTag {
    /// Frozen `u8` discriminant that feeds the BLAKE3 derivation in
    /// [`crate::topology::BRepFaceId::for_revolve_face`].
    ///
    /// Frozen at:
    ///
    /// ```text
    /// Side = 0, StartCap = 1, EndCap = 2
    /// ```
    ///
    /// The inner data of each variant (`side_index`, `profile_count`,
    /// `segment_count`, `mode` for `Side`; `profile_count` for caps) is NOT
    /// used as the discriminant — it is appended to the BLAKE3 input
    /// separately. These discriminants are part of the stable id
    /// substrate's wire surface and MUST NOT change without a `v2`
    /// migration in the domain separator.
    #[must_use]
    pub const fn discriminant(self) -> u8 {
        match self {
            RevolveFaceTag::Side { .. } => 0,
            RevolveFaceTag::StartCap { .. } => 1,
            RevolveFaceTag::EndCap { .. } => 2,
        }
    }
}

// ---------------------------------------------------------------------------
// LoftFaceTag (sub-7.2-δ)
// ---------------------------------------------------------------------------

/// Face-tag enumeration for [`crate::operators::LoftOp`].
///
/// `LoftOp` is the first operator with **two profile inputs**. v0 pairs
/// `profile_a[i]` with `profile_b[i]` for every `i` and emits faces in the
/// canonical order `Bottom cap → Top cap → Side(0..N-1)`, structurally
/// mirroring [`ExtrudeFaceTag`]. The variant order matches that emission
/// order; the discriminant pinned in [`LoftFaceTag::discriminant`] freezes
/// `Bottom = 0`, `Top = 1`, `Side = 2`. The inner data of `Side` is
/// BLAKE3-hashed (NOT used as the discriminant byte).
///
/// # Substrate-honesty guardrail (load-bearing)
///
/// Even though [`crate::operators::LoftOp::evaluate`] enforces equal
/// profile point counts at runtime (rejects `InvalidParameter` if
/// `profile_a.len() != profile_b.len()`), the `Side` variant carries BOTH
/// `profile_a_count` AND `profile_b_count` independently. This looks
/// redundant today, but it makes the tag self-describing — the substrate
/// does not depend on the validation rule living elsewhere in `LoftOp`.
/// The constructor [`crate::topology::BRepFaceId::for_loft_face`] handles
/// unequal counts directly, even though `LoftOp` itself can never produce
/// such an input. If the equal-count rule is ever loosened, the tag and
/// constructor remain correct without further substrate change.
///
/// # Stability contract (load-bearing)
///
/// 1. **Bottom and Top IDs are stable across `length` parameter changes**
///    and across **shape-size changes** (numeric coordinate changes within
///    `profile_a` / `profile_b` that preserve point count). The substrate
///    hashes only the discriminant byte for these two variants — no inner
///    data — so changing only `length` or scaling profile coordinates does
///    NOT alter cap face identity.
/// 2. **Bottom and Top IDs are categorical** (no inner data) — identical
///    between Loft variants with the same owner regardless of profile
///    shape. This matches the [`ExtrudeFaceTag::Bottom`] /
///    [`ExtrudeFaceTag::Top`] precedent: caps are a v0 limit; topology-
///    distinct caps between operators with different profile shapes are
///    OUT OF SCOPE for v0.
/// 3. **Side IDs are stable across `length` changes** and across
///    **coordinate-only profile changes** (when `profile_a.len()` and
///    `profile_b.len()` stay constant). The BLAKE3 input feeds only
///    `(edge_index, profile_a_count, profile_b_count)` for `Side`; numeric
///    profile coordinates are never inspected.
/// 4. **Side IDs break across either profile count changing.** Square →
///    pentagon for either profile (or both) → disjoint Side ID sets by
///    construction. The `LoftOp` validation rule today ties the two counts
///    together (must be equal), but the tag does not depend on that —
///    BOTH counts are independently hashed into the BLAKE3 input. This is
///    the load-bearing self-description guarantee that distinguishes this
///    dispatch from a naive single-`profile_count` design.
/// 5. **Side IDs depend on profile-A-count vs profile-B-count ordering.**
///    Swapping `profile_a` and `profile_b` produces different IDs for
///    `Side(i)` even when the resulting topology is similar. This reflects
///    the geometric reality that swapping profiles flips the top and
///    bottom of the loft (top and bottom positions swap, side winding
///    flips), so the IDs SHOULD differ.
/// 6. **The substrate does NOT inspect profile coordinates.** Profile-
///    vertex-order rotation at the same count, profile-vertex-pairing
///    offset (e.g. shifting which `profile_a[i]` matches `profile_b[j]`),
///    and any other coordinate-aware concern is OUT OF SCOPE for v0 — the
///    same explicit limit as [`ExtrudeFaceTag`] and [`RevolveFaceTag`].
/// 7. **No twist-matching is implied.** `LoftOp` v0 pairs `profile_a[i]`
///    with `profile_b[i]`; a future twist parameter would require a new
///    [`LoftFaceTag`] variant or new field, NOT a quiet behavior change in
///    the existing tag.
///
/// **Do not reorder** the variants in future revisions — the discriminant
/// (and therefore the derived [`crate::topology::BRepFaceId`]) is byte-stable
/// only as long as the variant ordering is preserved. Rebuild-stability for
/// callers who already serialized old IDs depends on this invariant.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LoftFaceTag {
    /// Bottom cap (`profile_a` lifted to `z = 0`). Categorical — no inner
    /// data, matching [`ExtrudeFaceTag::Bottom`] precedent.
    ///
    /// The discriminant for `Bottom` is `0`. The BLAKE3 input ends after
    /// the discriminant byte — no inner data — so `Bottom` IDs are stable
    /// across `length`, profile-coordinate, and profile-count changes
    /// (per the v0 categorical-cap contract; same explicit limit as
    /// [`ExtrudeFaceTag`]).
    Bottom,
    /// Top cap (`profile_b` lifted to `z = length`). Categorical — no
    /// inner data, matching [`ExtrudeFaceTag::Top`] precedent.
    ///
    /// The discriminant for `Top` is `1`. The BLAKE3 input ends after the
    /// discriminant byte — no inner data — so `Top` IDs are stable across
    /// `length`, profile-coordinate, and profile-count changes (per the v0
    /// categorical-cap contract; same explicit limit as [`ExtrudeFaceTag`]).
    Top,
    /// Side face — one per profile-edge pair `(profile_a[i], profile_b[i])`.
    /// Encodes BOTH profile counts independently per substrate-honesty
    /// guardrail (see module-doc); the runtime equal-count validation in
    /// [`crate::operators::LoftOp::evaluate`] makes them identical in
    /// practice today, but the tag remains self-describing if that rule
    /// ever changes.
    ///
    /// The discriminant for `Side` is `2`. The BLAKE3 input appends
    /// `edge_index.to_le_bytes()` (4 bytes) `||
    /// profile_a_count.to_le_bytes()` (4 bytes) `||
    /// profile_b_count.to_le_bytes()` (4 bytes) after the discriminant
    /// byte — in that order. The A→B ordering is **load-bearing** because
    /// swapping a Loft's `profile_a` and `profile_b` produces a
    /// geometrically-different mesh (top and bottom swap, side winding
    /// flips), so the IDs SHOULD differ to reflect that.
    Side {
        /// Index of the profile-edge pair `(profile_a[i], profile_b[i])`
        /// this side face spans, in canonical emission order
        /// `Side(0)..Side(N - 1)` where `N` is the shared profile point
        /// count.
        edge_index: u32,
        /// Profile-A vertex count. Hashed into the BLAKE3 input
        /// independently of `profile_b_count` per the substrate-honesty
        /// guardrail — even though [`crate::operators::LoftOp::evaluate`]
        /// enforces `profile_a_count == profile_b_count` at runtime, the
        /// tag remains self-describing and does NOT depend on that
        /// validation rule.
        profile_a_count: u32,
        /// Profile-B vertex count. Hashed into the BLAKE3 input
        /// independently of `profile_a_count` per the substrate-honesty
        /// guardrail (see [`Self::profile_a_count`] for the same
        /// rationale).
        profile_b_count: u32,
    },
}

impl LoftFaceTag {
    /// Frozen `u8` discriminant that feeds the BLAKE3 derivation in
    /// [`crate::topology::BRepFaceId::for_loft_face`].
    ///
    /// Frozen at:
    ///
    /// ```text
    /// Bottom = 0, Top = 1, Side = 2
    /// ```
    ///
    /// The inner data of `Side` (`edge_index`, `profile_a_count`,
    /// `profile_b_count`) is NOT used as the discriminant — it is appended
    /// to the BLAKE3 input separately. These discriminants are part of the
    /// stable id substrate's wire surface and MUST NOT change without a
    /// `v2` migration in the domain separator.
    #[must_use]
    pub const fn discriminant(self) -> u8 {
        match self {
            LoftFaceTag::Bottom => 0,
            LoftFaceTag::Top => 1,
            LoftFaceTag::Side { .. } => 2,
        }
    }
}
