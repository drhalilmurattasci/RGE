// SPLIT-EXEMPTION: cohesive RoundFilletOp substrate — `RoundFilletError`
// enum + `RoundFilletSpec` struct + `RoundFilletUpstream` trait +
// `RoundFilletOp` struct + `Operator` impl (general-dihedral,
// multi-segment path, and sub-epsilon shared-corner coordination)
// + the unit tests that pin sub-alpha's 90-degree baseline,
// sub-beta.gamma's general-dihedral invariants, sub-zeta's path
// branch, and sub-epsilon's corner-patch behavior. Splitting would
// force the test module to consume `pub(super) round_specs` /
// `pub(crate) RoundFilletSpec` through a public shim, breaking the
// "the operator owns its identity recipe" contract that
// `extrude.rs::SPLIT-EXEMPTION` and `loft.rs::SPLIT-EXEMPTION` cite
// at the same line-cap boundary. Per PLAN.md section 1.3 Rule 3
// (above the 1000-line hard cap; growth from sub-beta.gamma
// general-dihedral formulas, sub-zeta path support, and sub-epsilon
// corner blending).
//
//! `RoundFilletOp` — real round fillet substrate (chapter sub-alpha
//! through sub-epsilon).
//!
//! Failure class: snapshot-recoverable.
//!
//! Per ADR-119 (real round fillet substrate), `RoundFilletOp` is a
//! NEW operator beside chamfer [`crate::operators::FilletOp`], not an
//! in-place evolution. The two operators share NO substrate (no
//! shared spec, no shared trait, no shared resolver arms); chamfer's
//! `FilletOp` + `ChamferSpec` + `FilletUpstream` + sub-ε.α/β resolver
//! arms are byte-identical to their pre-this-dispatch state (per ADR
//! D6).
//!
//! # Chapter scope
//!
//! - Substrate: `RoundFilletOp` struct + `RoundFilletSpec` /
//!   `RoundFilletPathSpec` + `pub(crate) trait RoundFilletUpstream`
//!   + `OperatorNode::RoundFillet(_)` resolver arms in BOTH face +
//!   edge resolvers.
//! - Upstreams: Cuboid, Extrude, Revolve, and Loft edge coverage.
//! - Two-endpoint geometry: rolled circular surface with N=8
//!   segments per selected edge using the general-dihedral inset
//!   math proven by sub-beta.gamma.
//! - Path geometry: multi-ring swept-cylinder support for Revolve
//!   circular side-side paths, including partial open arcs and full
//!   closed loops.
//! - Sub-epsilon corner coordination: face-strip substitution is
//!   deferred until all selected specs emit candidate insets; two
//!   specs that replace the same upstream `(face, vertex)` slot are
//!   resolved to one shared face-corner inset, and vertices touched
//!   by 2+ selected specs receive nameless corner-patch fan geometry.
//! - Identity: cylinder caps and corner patches are nameless
//!   ([`TopologyFaceId::DEGENERATE`]) per ADR D3; inherited upstream
//!   surface identity continues to flow through the graph resolvers.
//!
//! # NON-GOALS
//!
//! - **No analytic/G2 torus patch** at multi-edge corners; sub-
//!   epsilon emits a deterministic nameless fan patch, not a CAD-
//!   kernel-quality corner surface.
//! - **No variable-radius / setback solver** across meeting edges;
//!   all selected specs still use one `RoundFilletOp::radius`.
//! - **No cap-face / corner-patch `BRepFaceId`**; caps + corner
//!   patches remain nameless in v0 (ADR D3; pressure-deferred), and
//!   their triangles emit `TopologyFaceId::DEGENERATE`.
//! - **No `impl BRepProvider for RoundFilletOp`** — face identity
//!   flows via the graph-level resolver per ADR D4 (`OperatorNode::RoundFillet(_)`
//!   face-resolver arm recurses to upstream and returns upstream
//!   `BRepFaceId`s unchanged; faces retain identity under face-strip
//!   removal because identity = semantic surface, not mesh shape)
//! - **No `impl BRepEdgeProvider for RoundFilletOp`** — edge identity
//!   flows via the graph-level resolver per ADR D2 (`OperatorNode::RoundFillet(_)`
//!   edge-resolver arm recurses to upstream and returns ALL upstream
//!   edges including filleted ones; curved-edge inheritance via the
//!   shape-agnostic `BRepEdgeId::for_face_pair` derivation)
//! - **No `Strategy::Winch` / engine-default change** — orthogonal
//!   to this dispatch
//! - **No chamfer `FilletOp` change** — byte-identical per ADR D6
//! - **No new architecture lint, no new ADR, no new doctrine doc**

use serde::{Deserialize, Serialize};

use crate::operators::{OpError, OpKind, Operator};
use crate::tessellation::{Tessellation, TopologyFaceId};
use crate::topology::{BRepEdgeId, BRepEdgeProvider, BRepOwnerId};

mod cuboid;
mod extrude;
mod loft;
mod revolve;

// ---------------------------------------------------------------------------
// RoundFilletError
// ---------------------------------------------------------------------------

/// Construction-time errors for [`RoundFilletOp::new`].
///
/// Marked `#[non_exhaustive]` so future variant additions
/// (e.g., circular-path-Revolve support per ADR D8 / sub-ζ) are
/// non-breaking. Variants intentionally mirror
/// [`crate::operators::FilletError`] so callers can apply the same
/// error-handling patterns to both operators.
#[derive(Clone, Copy, Debug, PartialEq, thiserror::Error)]
#[non_exhaustive]
pub enum RoundFilletError {
    /// `radius` must be finite and strictly positive.
    #[error("round fillet radius must be finite and > 0; got {radius}")]
    InvalidRadius {
        /// The offending radius value.
        radius: f32,
    },

    /// Caller passed an empty edge selection — degenerate operator.
    #[error("round fillet edge list is empty; degenerate operator")]
    EmptyEdgeSelection,

    /// One of the supplied [`BRepEdgeId`]s does not match any edge
    /// emitted by the upstream's [`BRepEdgeProvider`].
    #[error("edge id {edge:?} does not appear in upstream's BRepEdgeProvider output")]
    EdgeNotInUpstream {
        /// The unknown edge id.
        edge: BRepEdgeId,
    },

    /// The supplied edge ID is valid against the upstream's
    /// [`BRepEdgeProvider`], but its geometry is not supported by
    /// `RoundFilletOp`'s v0 rolled-cylinder pattern.
    ///
    /// Reserved for future use (e.g., circular-path Revolve edges
    /// per ADR D8 / sub-ζ). Cuboid upstream in sub-α never produces
    /// this error — every Cuboid edge is a clean 2-endpoint
    /// adjacency.
    #[error("edge id {edge:?} has unsupported geometry: {reason}")]
    UnsupportedEdgeGeometry {
        /// The offending edge id.
        edge: BRepEdgeId,
        /// Static description of why the geometry is not supported.
        reason: &'static str,
    },
}

// ---------------------------------------------------------------------------
// RoundFilletSpec — per-filleted-edge data
// ---------------------------------------------------------------------------

/// Per-filleted-edge data used at evaluation. Stored in the order the
/// caller supplied edges. Computed at construction time so evaluation
/// is upstream-agnostic.
///
/// Distinct from [`crate::operators::ChamferSpec`] (chamfer's spec):
/// round fillet needs two in-plane inward directions (one per
/// adjacent face) for the inset vertices AND to compute the cylinder
/// axis center. Chamfer carries one fused inward direction; round
/// fillet's per-face split is load-bearing for the face-strip removal
/// substitution.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct RoundFilletSpec {
    /// Index of edge endpoint 1 in upstream's `positions` array.
    pub(crate) vertex_a: u32,
    /// Index of edge endpoint 2 in upstream's `positions` array.
    pub(crate) vertex_b: u32,
    /// `TopologyFaceId` of adjacent face A (one of the two faces
    /// sharing this edge). Used to locate face A's triangles in the
    /// upstream's `face_labels` for the face-strip-removal
    /// substitution.
    pub(crate) face_a_id: TopologyFaceId,
    /// Same for adjacent face B.
    pub(crate) face_b_id: TopologyFaceId,
    /// In-plane inward direction for face A's inset — perpendicular
    /// to the filleted edge, lying in face A's plane, pointing INTO
    /// face A's interior from the filleted edge. Unit vector.
    pub(crate) face_a_inward: [f32; 3],
    /// Same for face B.
    pub(crate) face_b_inward: [f32; 3],
}

// ---------------------------------------------------------------------------
// RoundFilletPathSpec — multi-segment swept-cylinder spec (sub-ζ Commit 2)
// ---------------------------------------------------------------------------

/// Multi-segment path spec for round-fillet operations on edges that
/// span more than 2 endpoints — specifically Revolve side-side edges
/// (sub-ζ Commit 2):
///
/// * **Partial-mode Revolve side-side**: an open arc with `segments + 1`
///   ring positions; the path runs from `bot_{(i+1)%N}` at ring 0
///   (θ=0) through intermediate rings to `top_{(i+1)%N}` at ring
///   `segments` (θ=angle). `closed_loop = false`.
/// * **Full-mode Revolve side-side**: a closed circular loop with
///   `segments` ring positions; ring `M-1` wraps back to ring 0
///   (θ = 2π = θ_0). `closed_loop = true`.
///
/// The dihedral angle along the path is **constant** (equal to the
/// profile's interior polygon angle at the shared profile-vertex),
/// but the inward DIRECTIONS rotate around the Y-axis as θ varies.
/// Hence `path_face_a_inwards` and `path_face_b_inwards` carry one
/// unit-vector per ring; the per-ring inward direction is the
/// rotated copy of the XY-plane inward direction.
///
/// `pub(crate)` only — same boundary as [`RoundFilletSpec`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct RoundFilletPathSpec {
    /// Vertex indices along the path in the upstream tessellation's
    /// `positions` array. For open paths size is M+1 (= segments+1
    /// for Revolve partial side-side); for closed loops size is M
    /// (= segments for Revolve full-mode side-side; ring M-1 wraps
    /// to ring 0 implicitly via `closed_loop`).
    pub(crate) path_vertices: Vec<u32>,
    /// `TopologyFaceId` of adjacent face A (same across all rings of
    /// the path — Side(i) doesn't change identity as θ varies).
    pub(crate) face_a_id: TopologyFaceId,
    /// Same for adjacent face B (Side((i+1)%N)).
    pub(crate) face_b_id: TopologyFaceId,
    /// Per-ring inward direction in face A's tangent plane at that
    /// ring, perpendicular to the edge tangent at that ring, pointing
    /// INTO face A's interior. Unit vector at each ring. Size matches
    /// `path_vertices.len()`.
    pub(crate) path_face_a_inwards: Vec<[f32; 3]>,
    /// Per-ring inward direction in face B's tangent plane.
    /// Size matches `path_vertices.len()`.
    pub(crate) path_face_b_inwards: Vec<[f32; 3]>,
    /// True for closed-loop paths (full-mode Revolve side-side);
    /// `RoundFilletOp::evaluate`'s Path branch wraps ring `M-1` to
    /// ring `0` for the closing segment. False for open paths
    /// (partial-mode Revolve side-side).
    pub(crate) closed_loop: bool,
}

// ---------------------------------------------------------------------------
// RoundFilletSpecKind — variant carrier for 2-endpoint vs path specs
// ---------------------------------------------------------------------------

/// Discriminator over the two per-edge spec shapes the round-fillet
/// substrate carries.
///
/// At sub-ζ Commit 1 the enum had ONE variant `TwoEndpoint`. Sub-ζ
/// Commit 2 adds the `Path(RoundFilletPathSpec)` variant for
/// multi-segment swept-cylinder fillets on Revolve side-side edges.
/// The `RoundFilletOp::evaluate` body's `for spec_kind in
/// &self.round_specs` loop dispatches via `match` — Rust's
/// exhaustive-match requirement forced the `Path` arm to be added
/// alongside `TwoEndpoint` here.
///
/// `pub(crate)` only — same boundary as [`RoundFilletSpec`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) enum RoundFilletSpecKind {
    /// Straight 2-endpoint edge fillet (sub-α Cuboid + sub-β Extrude
    /// + sub-γ Revolve cap-side + sub-β.γ-extend Extrude vertical-
    /// seam + sub-δ.revisit Loft cases). Inner [`RoundFilletSpec`]
    /// shape preserved byte-identical from sub-β.γ-extend `978f507`.
    TwoEndpoint(RoundFilletSpec),
    /// Multi-segment swept-cylinder fillet (sub-ζ Commit 2 — Revolve
    /// side-side circular paths, both partial-mode open arcs and
    /// full-mode closed loops). See [`RoundFilletPathSpec`] for the
    /// per-ring data layout.
    Path(RoundFilletPathSpec),
}

impl RoundFilletSpecKind {
    /// Test-only accessor: panics if the variant is not `TwoEndpoint`.
    ///
    /// Used at test call-sites that direct-call
    /// `RoundFilletUpstream::resolve_round_spec(idx)` and then access
    /// spec fields like `spec.face_a_id`. Production code paths
    /// (`RoundFilletOp::evaluate`'s match dispatch +
    /// `from_upstream`'s storage push) don't need this accessor —
    /// they handle the variant explicitly.
    #[cfg(test)]
    pub(crate) fn expect_two_endpoint(&self) -> &RoundFilletSpec {
        match self {
            RoundFilletSpecKind::TwoEndpoint(spec) => spec,
            RoundFilletSpecKind::Path(_) => {
                panic!("expected RoundFilletSpecKind::TwoEndpoint, got Path")
            }
        }
    }

    /// Test-only accessor: panics if the variant is not `Path`.
    /// Sibling of [`Self::expect_two_endpoint`] for sub-ζ Path-branch
    /// tests.
    #[cfg(test)]
    pub(crate) fn expect_path(&self) -> &RoundFilletPathSpec {
        match self {
            RoundFilletSpecKind::Path(path) => path,
            RoundFilletSpecKind::TwoEndpoint(_) => {
                panic!("expected RoundFilletSpecKind::Path, got TwoEndpoint")
            }
        }
    }
}

// ---------------------------------------------------------------------------
// RoundFilletUpstream — internal trait abstracting per-upstream resolution
// ---------------------------------------------------------------------------

/// Internal trait that abstracts the per-upstream-operator pieces of
/// `RoundFilletOp` construction. Sibling to [`crate::operators::FilletUpstream`]
/// (chamfer's trait) per ADR D5 — substrate is PARALLEL to chamfer's,
/// not shared.
///
/// Per ADR D6, the existing chamfer `FilletUpstream` trait + its 4
/// per-upstream impls (Cuboid + Extrude + Revolve + Loft) are
/// byte-identical to their pre-this-dispatch state; this new trait
/// adds round-fillet-specific resolution alongside.
///
/// Currently `pub(crate)` only — abstraction earned when the second
/// implementation lands (sub-β Extrude); for sub-α Cuboid is the only
/// implementor. External consumer plug-in is a separate ADR-level
/// decision.
pub(crate) trait RoundFilletUpstream: BRepEdgeProvider {
    /// Resolve a canonical edge index (the position in
    /// `brep_edge_ids` output) to the data needed for round-fillet
    /// evaluation.
    ///
    /// # Errors
    ///
    /// Returns `Err(reason)` when the edge's geometry is not
    /// supported by `RoundFilletOp`'s v0 rolled-cylinder pattern.
    /// The caller wraps this with the edge ID into
    /// [`RoundFilletError::UnsupportedEdgeGeometry`].
    ///
    /// Cuboid implementation always returns `Ok(spec)` — every Cuboid
    /// edge is a clean 2-endpoint adjacency between two perpendicular
    /// axis-aligned faces.
    fn resolve_round_spec(
        &self,
        canonical_index: usize,
    ) -> Result<RoundFilletSpecKind, &'static str>;
}

// ---------------------------------------------------------------------------
// RoundFilletOp
// ---------------------------------------------------------------------------

/// Tessellation segments around the cylinder cross-section arc.
///
/// 8 subdivisions of the arc span `π − φ` (where φ is the interior
/// dihedral angle between the two adjacent face inward directions).
/// For 90° dihedrals (sub-α Cuboid + sub-β Extrude cap-perimeter +
/// sub-γ Revolve cap-side) this is a quarter-arc; for general
/// dihedrals (sub-β.γ onward) the same `N=8` subdivides the actual
/// arc span — finer tessellation for acute dihedrals comes "for free"
/// because the arc spans more radians. Can be raised by a future LoD
/// knob.
const ROUND_FILLET_SEGMENTS: usize = 8;

/// Threshold on `sin²(φ) = 1 − (a · b)²` below which the dihedral is
/// considered degenerate (faces near-coplanar same-side or near-anti-
/// parallel knife-edge). At this threshold `|sin(φ)| < 1e-3`, i.e.
/// φ is within ~0.057° of 0° or 180° — well below any meaningful
/// fillet geometry. Below threshold, the inset / axis_center / radial
/// formulas all involve division by `sin(φ)` and would produce NaN
/// or unbounded magnitudes; we reject at evaluation time with
/// [`OpError::InvalidParameter`] (the same path the existing
/// vertex-index-out-of-bounds + Tessellation-construction-failure
/// cases use — no new error variant required, per ADR-119 D-α scope).
///
/// No current sub-α/β/γ upstream produces a degenerate dihedral
/// (Cuboid axis-aligned faces are 90° exactly; Extrude / Revolve
/// cap-perimeter and cap-side dihedrals are 90° by algebraic
/// construction). The threshold is defense-in-depth for synthetic
/// specs and future upstreams.
const DIHEDRAL_EPSILON_SQ: f32 = 1e-6;

#[derive(Clone, Copy, Debug)]
struct FaceVertexReplacement {
    face_id: TopologyFaceId,
    vertex: u32,
    inset_idx: u32,
    inward: [f32; 3],
    distance: f32,
}

#[derive(Clone, Debug)]
struct CornerEndpointPatch {
    vertex: u32,
    arc_indices: Vec<u32>,
}

#[derive(Clone, Copy, Debug)]
struct ResolvedFaceVertexReplacement {
    vertex: u32,
    replacement_idx: u32,
    is_corner: bool,
}

/// `RoundFilletOp` — real round fillet along selected upstream edges.
///
/// Constructed via [`RoundFilletOp::new`] (Cuboid upstream in sub-α);
/// validates each edge against the upstream's
/// [`crate::topology::BRepEdgeProvider`] and resolves each
/// [`BRepEdgeId`] back to a [`RoundFilletSpec`] so evaluation can
/// locate the geometry without holding a graph reference. Arity 1 —
/// takes the upstream's tessellation as input.
///
/// Per ADR D1 + D6: distinct from chamfer
/// [`crate::operators::FilletOp`]; both operators coexist in v0,
/// serving distinct use cases (chamfer for fast preview /
/// constant-time-per-edge; round for production geometry with
/// face-strip removal).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RoundFilletOp {
    /// Selected edges by stable identity. Mirrors the user-facing
    /// API surface of chamfer's `FilletOp`.
    pub(super) edges: Vec<BRepEdgeId>,
    /// Resolved per-edge round-fillet spec — one per selected edge,
    /// in the same order. Used at evaluation time to locate vertices
    /// and apply the rolled-cylinder geometry. Computed at
    /// construction time.
    pub(super) round_specs: Vec<RoundFilletSpecKind>,
    /// Fillet radius (cylinder cross-section radius), in world units.
    pub(super) radius: f32,
    /// Owner the substrate-resolved IDs were derived against.
    pub(super) owner: BRepOwnerId,
}

impl RoundFilletOp {
    /// Borrow the validated edge selection.
    #[must_use]
    pub fn edges(&self) -> &[BRepEdgeId] {
        &self.edges
    }

    /// Returns the round-fillet radius.
    #[must_use]
    pub fn radius(&self) -> f32 {
        self.radius
    }

    /// Returns the owner the edge IDs were validated against.
    #[must_use]
    pub fn owner(&self) -> BRepOwnerId {
        self.owner
    }

    /// Generic constructor over any [`RoundFilletUpstream`].
    ///
    /// Performs the shared validation (radius finiteness, non-empty
    /// edge selection, per-edge upstream lookup) and per-upstream
    /// round-spec resolution.
    ///
    /// # Errors
    ///
    /// * [`RoundFilletError::InvalidRadius`] if `radius` is non-finite
    ///   or `<= 0`.
    /// * [`RoundFilletError::EmptyEdgeSelection`] if `edges` is empty.
    /// * [`RoundFilletError::EdgeNotInUpstream`] if any edge ID does
    ///   not appear in `upstream.brep_edge_ids(owner)`.
    /// * [`RoundFilletError::UnsupportedEdgeGeometry`] if a known edge
    ///   ID has geometry `RoundFilletOp` cannot round in v0 (reserved
    ///   for future circular-path Revolve support; Cuboid never
    ///   produces this).
    pub(super) fn from_upstream<U: RoundFilletUpstream>(
        upstream: &U,
        owner: BRepOwnerId,
        edges: Vec<BRepEdgeId>,
        radius: f32,
    ) -> Result<Self, RoundFilletError> {
        if !radius.is_finite() || radius <= 0.0 {
            return Err(RoundFilletError::InvalidRadius { radius });
        }
        if edges.is_empty() {
            return Err(RoundFilletError::EmptyEdgeSelection);
        }

        let upstream_edges = upstream.brep_edge_ids(owner);
        let mut round_specs = Vec::with_capacity(edges.len());
        for &edge_id in &edges {
            let canonical_index = upstream_edges
                .iter()
                .position(|id| *id == edge_id)
                .ok_or(RoundFilletError::EdgeNotInUpstream { edge: edge_id })?;
            let spec = upstream
                .resolve_round_spec(canonical_index)
                .map_err(|reason| RoundFilletError::UnsupportedEdgeGeometry {
                    edge: edge_id,
                    reason,
                })?;
            round_specs.push(spec);
        }

        Ok(Self {
            edges,
            round_specs,
            radius,
            owner,
        })
    }
}

impl Operator for RoundFilletOp {
    fn op_kind(&self) -> OpKind {
        OpKind::RoundFillet
    }

    fn arity(&self) -> usize {
        1
    }

    fn structural_hash(&self) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"round_fillet:");
        hasher.update(&self.radius.to_le_bytes());
        hasher.update(self.owner.as_bytes());
        hasher.update(
            &u32::try_from(self.edges.len())
                .unwrap_or(u32::MAX)
                .to_le_bytes(),
        );
        for edge in &self.edges {
            hasher.update(edge.as_bytes());
        }
        let hash = hasher.finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(hash.as_bytes());
        out
    }

    fn evaluate(&self, inputs: &[&Tessellation]) -> Result<Tessellation, OpError> {
        if inputs.len() != self.arity() {
            return Err(OpError::WrongArity {
                expected: self.arity(),
                got: inputs.len(),
            });
        }
        let upstream = inputs[0];
        let mut positions = upstream.positions.clone();
        let mut indices = upstream.indices.clone();
        // Round fillet preserves labeled-ness — every Cuboid input is
        // labeled (CuboidOp always emits face_labels); the modified
        // face triangles inherit their original TopologyFaceId, and
        // new cylinder triangles get TopologyFaceId::DEGENERATE per
        // ADR D3 (nameless cap surfaces in v0). If upstream is
        // unlabeled (None), we don't fabricate labels; the
        // face-strip substitution can't run without labels (it needs
        // face_id to locate triangles), so unlabeled upstream
        // produces an unlabeled output with cylinder geometry
        // appended but NO face-strip removal — degenerate case for
        // v0 Cuboid (Cuboid is always labeled).
        let mut face_labels = upstream.face_labels.clone();
        let mut face_replacements: Vec<FaceVertexReplacement> = Vec::new();
        let mut corner_endpoint_patches: Vec<CornerEndpointPatch> = Vec::new();

        for spec_kind in &self.round_specs {
            // Sub-ζ Commit 1 wrapper dispatch. At Commit 1 the enum
            // has ONE variant (`TwoEndpoint`); the existing sub-β.γ
            // general-dihedral body executes byte-identical inside
            // the `TwoEndpoint` arm. When sub-ζ Commit 2 adds
            // `Path(RoundFilletPathSpec)`, Rust's exhaustive-match
            // requirement will force this `match` to grow a `Path`
            // arm — the boundary catch that signals the multi-segment
            // swept-cylinder math must be added.
            let spec = match spec_kind {
                RoundFilletSpecKind::TwoEndpoint(spec) => spec,
                RoundFilletSpecKind::Path(path_spec) => {
                    // Sub-ζ Commit 2 Path branch: multi-segment swept-
                    // cylinder fillet along a circular-path edge
                    // (Revolve partial side-side open arc; Revolve
                    // full-mode side-side closed loop).
                    //
                    // For each ring r along the path:
                    //   1. Compute sub-β.γ general-dihedral inset +
                    //      axis_center + cross-section vertices at
                    //      that ring's position pos_r using per-ring
                    //      inward directions a_r, b_r.
                    //   2. Append 2 inset vertices (face A / face B
                    //      tangent foot on each face's plane at pos_r).
                    //   3. Append (N+1) cross-section vertices forming
                    //      a quarter/general arc around axis_center_r
                    //      in the perpendicular-to-edge cross-section
                    //      plane.
                    //
                    // Then stitch cross-sections at consecutive rings
                    // with 2*N triangles per ring-step (M ring-steps
                    // for closed loops, M-1 for open paths). Total
                    // cylinder triangles = 2 * M_segments * N where
                    // M_segments = M for closed loops, M-1 for open
                    // (M = path_vertices.len()).
                    //
                    // Face-strip substitution sweeps ALL face A /
                    // face B triangles and replaces ANY occurrence of
                    // a path-vertex index with the corresponding
                    // ring's inset index. Side(i) and Side((i+1)%N)
                    // span every ring along the seam, so each ring's
                    // substitution is keyed by the path-vertex →
                    // inset-index mapping built up per ring.
                    let m = path_spec.path_vertices.len();
                    if m == 0 {
                        return Err(OpError::InvalidParameter(
                            "round fillet path spec has zero ring positions".to_string(),
                        ));
                    }
                    if path_spec.path_face_a_inwards.len() != m
                        || path_spec.path_face_b_inwards.len() != m
                    {
                        return Err(OpError::InvalidParameter(format!(
                            "round fillet path spec inward-arrays length mismatch: \
                             path_vertices.len() = {}, path_face_a_inwards.len() = {}, \
                             path_face_b_inwards.len() = {}",
                            m,
                            path_spec.path_face_a_inwards.len(),
                            path_spec.path_face_b_inwards.len(),
                        )));
                    }

                    let mut inset_a_indices: Vec<u32> = Vec::with_capacity(m);
                    let mut inset_b_indices: Vec<u32> = Vec::with_capacity(m);
                    let mut cross_section_indices: Vec<Vec<u32>> = Vec::with_capacity(m);

                    for r in 0..m {
                        let path_vertex_usize = path_spec.path_vertices[r] as usize;
                        if path_vertex_usize >= positions.len() {
                            return Err(OpError::InvalidParameter(format!(
                                "round fillet path vertex index out of bounds at ring {}: \
                                 vertex = {}, positions.len = {}",
                                r,
                                path_spec.path_vertices[r],
                                positions.len()
                            )));
                        }
                        let pos_r = positions[path_vertex_usize];
                        let a = path_spec.path_face_a_inwards[r];
                        let b = path_spec.path_face_b_inwards[r];

                        let dot_ab_raw = a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
                        let dot_ab = dot_ab_raw.clamp(-1.0, 1.0);
                        let sin_phi_sq = 1.0 - dot_ab * dot_ab;
                        if sin_phi_sq < DIHEDRAL_EPSILON_SQ {
                            return Err(OpError::InvalidParameter(format!(
                                "round fillet path-ring {r} near-degenerate dihedral: \
                                 a·b = {dot_ab_raw} (sin²(φ) = {sin_phi_sq} < {DIHEDRAL_EPSILON_SQ})"
                            )));
                        }
                        let sin_phi = sin_phi_sq.sqrt();
                        let phi = dot_ab.acos();
                        let inv_sin_phi = 1.0 / sin_phi;
                        let inset_scale = (1.0 + dot_ab) * inv_sin_phi;
                        let axis_scale = inv_sin_phi;

                        let inset_a = vec_add(pos_r, vec_scale(a, self.radius * inset_scale));
                        let inset_b = vec_add(pos_r, vec_scale(b, self.radius * inset_scale));
                        let inset_a_idx = u32::try_from(positions.len()).unwrap_or(u32::MAX);
                        positions.push(inset_a);
                        inset_a_indices.push(inset_a_idx);
                        let inset_b_idx = u32::try_from(positions.len()).unwrap_or(u32::MAX);
                        positions.push(inset_b);
                        inset_b_indices.push(inset_b_idx);

                        let two_inward_sum = vec_add(a, b);
                        let axis_center =
                            vec_add(pos_r, vec_scale(two_inward_sum, self.radius * axis_scale));
                        let arc_span = std::f32::consts::PI - phi;

                        #[allow(
                            clippy::cast_precision_loss,
                            reason = "ROUND_FILLET_SEGMENTS is 8; precision loss in usize→f32 is well below tessellation tolerance"
                        )]
                        let segments_f = ROUND_FILLET_SEGMENTS as f32;
                        let mut ring_cs_indices = Vec::with_capacity(ROUND_FILLET_SEGMENTS + 1);
                        for k in 0..=ROUND_FILLET_SEGMENTS {
                            #[allow(
                                clippy::cast_precision_loss,
                                reason = "k bounded by ROUND_FILLET_SEGMENTS; precision loss negligible"
                            )]
                            let t = k as f32 / segments_f;
                            let theta = arc_span * t;
                            let cos_t = theta.cos();
                            let cos_t_plus_phi = (theta + phi).cos();
                            let coef_a = cos_t_plus_phi * inv_sin_phi;
                            let coef_b = -cos_t * inv_sin_phi;
                            let radial = [
                                coef_a * a[0] + coef_b * b[0],
                                coef_a * a[1] + coef_b * b[1],
                                coef_a * a[2] + coef_b * b[2],
                            ];
                            let cs_pos = vec_add(axis_center, vec_scale(radial, self.radius));
                            let cs_idx = u32::try_from(positions.len()).unwrap_or(u32::MAX);
                            positions.push(cs_pos);
                            ring_cs_indices.push(cs_idx);
                        }
                        cross_section_indices.push(ring_cs_indices);
                    }

                    // Defer face-strip substitution until every spec has
                    // contributed its candidate insets. Sub-ε corner blending
                    // resolves collisions where two selected specs want to
                    // replace the same (face, original-vertex) slot.
                    for r in 0..m {
                        let path_vertex = path_spec.path_vertices[r];
                        push_face_replacement(
                            &mut face_replacements,
                            &positions,
                            path_vertex,
                            path_spec.face_a_id,
                            path_spec.path_face_a_inwards[r],
                            inset_a_indices[r],
                        );
                        push_face_replacement(
                            &mut face_replacements,
                            &positions,
                            path_vertex,
                            path_spec.face_b_id,
                            path_spec.path_face_b_inwards[r],
                            inset_b_indices[r],
                        );
                    }

                    if !path_spec.closed_loop {
                        corner_endpoint_patches.push(CornerEndpointPatch {
                            vertex: path_spec.path_vertices[0],
                            arc_indices: cross_section_indices[0].clone(),
                        });
                        corner_endpoint_patches.push(CornerEndpointPatch {
                            vertex: path_spec.path_vertices[m - 1],
                            arc_indices: cross_section_indices[m - 1].clone(),
                        });
                    }

                    // Stitch consecutive cross-sections with
                    // quad-strip triangles. For open paths iterate r
                    // in [0, M-1); for closed loops iterate r in
                    // [0, M) with r_next = (r+1) % M wrap.
                    let num_stitches = if path_spec.closed_loop { m } else { m - 1 };
                    for r in 0..num_stitches {
                        let r_next = (r + 1) % m;
                        for k in 0..ROUND_FILLET_SEGMENTS {
                            let a1 = cross_section_indices[r][k];
                            let a2 = cross_section_indices[r][k + 1];
                            let b1 = cross_section_indices[r_next][k];
                            let b2 = cross_section_indices[r_next][k + 1];
                            indices.push(a1);
                            indices.push(b1);
                            indices.push(b2);
                            indices.push(a1);
                            indices.push(b2);
                            indices.push(a2);
                            if let Some(labels) = face_labels.as_mut() {
                                labels.push(TopologyFaceId::DEGENERATE);
                                labels.push(TopologyFaceId::DEGENERATE);
                            }
                        }
                    }

                    // Path branch is self-contained; skip the
                    // TwoEndpoint body below by continuing the outer
                    // for-loop.
                    continue;
                }
            };
            let vertex_a_usize = spec.vertex_a as usize;
            let vertex_b_usize = spec.vertex_b as usize;
            if vertex_a_usize >= positions.len() || vertex_b_usize >= positions.len() {
                return Err(OpError::InvalidParameter(format!(
                    "round fillet vertex index out of bounds: a={}, b={}, positions.len={}",
                    spec.vertex_a,
                    spec.vertex_b,
                    positions.len()
                )));
            }

            let pos_a = positions[vertex_a_usize];
            let pos_b = positions[vertex_b_usize];

            // General-dihedral cylinder math (sub-β.γ; supersedes
            // sub-α/β/γ's 90°-only formulas while preserving them
            // exactly at φ = 90°).
            //
            // For unit inward vectors `a` and `b` with interior
            // dihedral angle φ = arccos(a · b) in the perpendicular-
            // to-edge cross-section:
            //
            //   inset_a     = pos + r · (1 + a·b) / sin(φ) · a
            //                 (= r · cot(φ/2) · a; at φ=90°: r · a)
            //   inset_b     = pos + r · (1 + a·b) / sin(φ) · b
            //   axis_center = pos + r · (a + b) / sin(φ)
            //                 (at φ=90°: r · (a + b))
            //   vertex(θ)   = axis_center + r · (cos(θ+φ)·a − cos(θ)·b)/sin(φ)
            //                 for θ ∈ [0, π − φ]
            //                 (at φ=90°: −sin(θ)·a − cos(θ)·b, i.e.
            //                 sub-α's `-b·cos(θ) - a·sin(θ)`)
            //
            // `dot_ab_raw` is clamped to [-1.0, 1.0] before acos/sqrt
            // to prevent NaN on tiny float overshoot (future upstream
            // impls computing non-unit-length normals or accumulating
            // ULP-level drift). The near-degenerate guard then catches
            // dihedrals within √DIHEDRAL_EPSILON_SQ of 0° or 180°
            // where the formulas divide by sin(φ) → 0. Per ADR-119
            // sub-β.γ green-light: no new error variant — the existing
            // OpError::InvalidParameter path carries the rejection
            // (same shape as the vertex-index-out-of-bounds + Tessellation-
            // construction-failure paths above/below).
            //
            // Face-strip substitution semantics UNCHANGED: insets'
            // INDICES and their pairing with vertex_a/vertex_b /
            // face_a_id/face_b_id are byte-identical to sub-α; only
            // the POSITIONS of the 4 inset vertices change (no longer
            // pos + r·a but pos + r·cot(φ/2)·a). The substitution
            // loop below operates on indices/face_ids, not positions
            // — face-strip identity contract preserved by construction.
            let a = spec.face_a_inward;
            let b = spec.face_b_inward;
            let dot_ab_raw = a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
            let dot_ab = dot_ab_raw.clamp(-1.0, 1.0);
            let sin_phi_sq = 1.0 - dot_ab * dot_ab;
            if sin_phi_sq < DIHEDRAL_EPSILON_SQ {
                return Err(OpError::InvalidParameter(format!(
                    "round fillet face inward vectors near-degenerate dihedral: \
                     a·b = {dot_ab_raw} (sin²(φ) = {sin_phi_sq} < {DIHEDRAL_EPSILON_SQ}); \
                     faces are near-coplanar same-side (φ→0) or near-anti-parallel \
                     knife-edge (φ→π)"
                )));
            }
            let sin_phi = sin_phi_sq.sqrt();
            let phi = dot_ab.acos();
            let inv_sin_phi = 1.0 / sin_phi;
            let inset_scale = (1.0 + dot_ab) * inv_sin_phi;
            let axis_scale = inv_sin_phi;

            // Inset vertices: 4 per filleted edge (one per
            // adjacent-face-per-endpoint combination). Position
            // formula generalizes sub-α's `pos + r·a` to
            // `pos + r·cot(φ/2)·a`; reduces to sub-α at φ=90°
            // (cot(45°) = 1).
            let inset_a1 = vec_add(pos_a, vec_scale(a, self.radius * inset_scale));
            let inset_a2 = vec_add(pos_b, vec_scale(a, self.radius * inset_scale));
            let inset_b1 = vec_add(pos_a, vec_scale(b, self.radius * inset_scale));
            let inset_b2 = vec_add(pos_b, vec_scale(b, self.radius * inset_scale));

            let inset_a1_idx = u32::try_from(positions.len()).unwrap_or(u32::MAX);
            positions.push(inset_a1);
            let inset_a2_idx = u32::try_from(positions.len()).unwrap_or(u32::MAX);
            positions.push(inset_a2);
            let inset_b1_idx = u32::try_from(positions.len()).unwrap_or(u32::MAX);
            positions.push(inset_b1);
            let inset_b2_idx = u32::try_from(positions.len()).unwrap_or(u32::MAX);
            positions.push(inset_b2);

            // Cylinder axis_center for each endpoint: the unique
            // point in the perpendicular-to-edge cross-section at
            // distance `r` from BOTH adjacent face planes (=
            // r / sin(φ/2) along the inward bisector from the edge
            // endpoint, equivalently r · (a + b) / sin(φ)).
            let two_inward_sum = vec_add(a, b);
            let axis_center_1 = vec_add(pos_a, vec_scale(two_inward_sum, self.radius * axis_scale));
            let axis_center_2 = vec_add(pos_b, vec_scale(two_inward_sum, self.radius * axis_scale));

            // Arc parameterization: θ sweeps the EXTERIOR dihedral
            // π − φ from inset_a (at θ=0) to inset_b (at θ=π−φ).
            // The radial formula
            //   (cos(θ + φ)·a − cos(θ)·b) / sin(φ)
            // is the orthonormal cylinder-cross-section parameterization
            // in the (u_a, −a) basis where
            //   u_a = (cos(φ)·a − b) / sin(φ)
            // is the unit vector from axis_center toward inset_a.
            // At θ=0: radial = u_a → vertex = inset_a. At
            // θ=π−φ: radial = u_b → vertex = inset_b. Cylinder
            // surface radius preserved (|radial| = 1 for all θ).
            //
            // ROUND_FILLET_SEGMENTS subdivisions of the arc span:
            // for 90° dihedrals this is a quarter-arc (matches sub-α);
            // for 60° dihedrals it's 120° (sweeping more); for 120°
            // dihedrals it's 60° (sweeping less). Subdivision count
            // stays at N=8 regardless of dihedral — substrate
            // simplicity beats per-dihedral-adaptive subdivision in
            // v0 (future LoD knob can adapt).
            let arc_span = std::f32::consts::PI - phi;

            let mut endpoint_1_cylinder_indices = Vec::with_capacity(ROUND_FILLET_SEGMENTS + 1);
            let mut endpoint_2_cylinder_indices = Vec::with_capacity(ROUND_FILLET_SEGMENTS + 1);

            #[allow(
                clippy::cast_precision_loss,
                reason = "ROUND_FILLET_SEGMENTS is 8; precision loss in usize→f32 is well below tessellation tolerance"
            )]
            let segments_f = ROUND_FILLET_SEGMENTS as f32;
            for k in 0..=ROUND_FILLET_SEGMENTS {
                #[allow(
                    clippy::cast_precision_loss,
                    reason = "k bounded by ROUND_FILLET_SEGMENTS; precision loss negligible"
                )]
                let t = k as f32 / segments_f;
                let theta = arc_span * t;
                let cos_t = theta.cos();
                let cos_t_plus_phi = (theta + phi).cos();
                let coef_a = cos_t_plus_phi * inv_sin_phi;
                let coef_b = -cos_t * inv_sin_phi;
                let radial = [
                    coef_a * a[0] + coef_b * b[0],
                    coef_a * a[1] + coef_b * b[1],
                    coef_a * a[2] + coef_b * b[2],
                ];

                let pos_1 = vec_add(axis_center_1, vec_scale(radial, self.radius));
                let pos_2 = vec_add(axis_center_2, vec_scale(radial, self.radius));

                endpoint_1_cylinder_indices
                    .push(u32::try_from(positions.len()).unwrap_or(u32::MAX));
                positions.push(pos_1);
                endpoint_2_cylinder_indices
                    .push(u32::try_from(positions.len()).unwrap_or(u32::MAX));
                positions.push(pos_2);
            }

            // Face-strip removal is applied after all specs have emitted
            // their candidate insets. That lets sub-ε resolve shared-corner
            // collisions by replacing a face corner with the intersection of
            // its two offset-edge lines instead of whichever edge happened to
            // run first.
            push_face_replacement(
                &mut face_replacements,
                &positions,
                spec.vertex_a,
                spec.face_a_id,
                a,
                inset_a1_idx,
            );
            push_face_replacement(
                &mut face_replacements,
                &positions,
                spec.vertex_b,
                spec.face_a_id,
                a,
                inset_a2_idx,
            );
            push_face_replacement(
                &mut face_replacements,
                &positions,
                spec.vertex_a,
                spec.face_b_id,
                b,
                inset_b1_idx,
            );
            push_face_replacement(
                &mut face_replacements,
                &positions,
                spec.vertex_b,
                spec.face_b_id,
                b,
                inset_b2_idx,
            );
            corner_endpoint_patches.push(CornerEndpointPatch {
                vertex: spec.vertex_a,
                arc_indices: endpoint_1_cylinder_indices.clone(),
            });
            corner_endpoint_patches.push(CornerEndpointPatch {
                vertex: spec.vertex_b,
                arc_indices: endpoint_2_cylinder_indices.clone(),
            });

            // Append cylinder surface triangles. For each quad between
            // adjacent angular positions (k, k+1) and the two
            // endpoints (1, 2), two triangles form the quad surface.
            // Winding chosen so the outward-facing normal points
            // RADIALLY OUTWARD from the cylinder axis (away from the
            // cube body) — i.e., toward the original edge corner
            // direction. CCW from outside the cylinder.
            for k in 0..ROUND_FILLET_SEGMENTS {
                let a1 = endpoint_1_cylinder_indices[k];
                let a2 = endpoint_1_cylinder_indices[k + 1];
                let b1 = endpoint_2_cylinder_indices[k];
                let b2 = endpoint_2_cylinder_indices[k + 1];

                indices.push(a1);
                indices.push(b1);
                indices.push(b2);
                indices.push(a1);
                indices.push(b2);
                indices.push(a2);

                if let Some(labels) = face_labels.as_mut() {
                    labels.push(TopologyFaceId::DEGENERATE);
                    labels.push(TopologyFaceId::DEGENERATE);
                }
            }
        }

        let resolved_replacements = if let Some(labels) = face_labels.as_ref() {
            apply_face_vertex_replacements(&mut positions, &mut indices, labels, &face_replacements)
        } else {
            Vec::new()
        };
        append_corner_patches(
            &mut positions,
            &mut indices,
            face_labels.as_mut(),
            &corner_endpoint_patches,
            &resolved_replacements,
        );

        let result = if let Some(labels) = face_labels {
            Tessellation::with_labels(positions, indices, labels)
        } else {
            Tessellation::new(positions, indices)
        };
        result.map_err(|e| OpError::InvalidParameter(format!("round fillet output invalid: {e}")))
    }

    /// Round fillet preserves labeled output when the input was
    /// labeled — the modified upstream-face triangles inherit their
    /// original `TopologyFaceId`, and new cylinder triangles get
    /// `TopologyFaceId::DEGENERATE`. For unlabeled input (no Cuboid
    /// case in sub-α — Cuboid always labels — but future upstreams
    /// may produce unlabeled output), output is unlabeled.
    fn output_is_labeled(&self, inputs_labeled: &[bool]) -> bool {
        inputs_labeled.first().copied().unwrap_or(false)
    }
}

// ---------------------------------------------------------------------------
// Small vector helpers (kept local to avoid a glam / nalgebra dep
// for this module — chamfer FilletOp uses the same pattern).
// ---------------------------------------------------------------------------

fn vec_add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn vec_scale(v: [f32; 3], s: f32) -> [f32; 3] {
    [v[0] * s, v[1] * s, v[2] * s]
}

fn vec_sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn vec_dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn vec_cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn vec_len_sq(v: [f32; 3]) -> f32 {
    vec_dot(v, v)
}

fn vec_normalize_or(v: [f32; 3], fallback: [f32; 3]) -> [f32; 3] {
    let len_sq = vec_len_sq(v);
    if len_sq < 1e-12 {
        fallback
    } else {
        vec_scale(v, 1.0 / len_sq.sqrt())
    }
}

fn push_face_replacement(
    replacements: &mut Vec<FaceVertexReplacement>,
    positions: &[[f32; 3]],
    vertex: u32,
    face_id: TopologyFaceId,
    inward: [f32; 3],
    inset_idx: u32,
) {
    let base = positions[vertex as usize];
    let inset = positions[inset_idx as usize];
    let offset = vec_sub(inset, base);
    replacements.push(FaceVertexReplacement {
        face_id,
        vertex,
        inset_idx,
        inward,
        distance: vec_dot(offset, inward),
    });
}

fn apply_face_vertex_replacements(
    positions: &mut Vec<[f32; 3]>,
    indices: &mut [u32],
    labels: &[TopologyFaceId],
    replacements: &[FaceVertexReplacement],
) -> Vec<ResolvedFaceVertexReplacement> {
    let mut resolved: Vec<(TopologyFaceId, u32, u32, bool)> = Vec::new();
    for (tri_idx, label) in labels.iter().enumerate() {
        for j in 0..3 {
            let idx_pos = tri_idx * 3 + j;
            let original = indices[idx_pos];
            let replacement_idx = resolve_face_vertex_replacement(
                positions,
                &mut resolved,
                *label,
                original,
                replacements,
            );
            if let Some(replacement_idx) = replacement_idx {
                indices[idx_pos] = replacement_idx;
            }
        }
    }

    resolved
        .into_iter()
        .map(
            |(_face_id, vertex, replacement_idx, is_corner)| ResolvedFaceVertexReplacement {
                vertex,
                replacement_idx,
                is_corner,
            },
        )
        .collect()
}

fn resolve_face_vertex_replacement(
    positions: &mut Vec<[f32; 3]>,
    resolved: &mut Vec<(TopologyFaceId, u32, u32, bool)>,
    face_id: TopologyFaceId,
    vertex: u32,
    replacements: &[FaceVertexReplacement],
) -> Option<u32> {
    if let Some((_, _, replacement_idx, _)) =
        resolved
            .iter()
            .find(|(resolved_face, resolved_vertex, _, _)| {
                *resolved_face == face_id && *resolved_vertex == vertex
            })
    {
        return Some(*replacement_idx);
    }

    let mut matches = replacements
        .iter()
        .filter(|replacement| replacement.face_id == face_id && replacement.vertex == vertex);
    let first = matches.next()?;
    let Some(second) = matches.next() else {
        resolved.push((face_id, vertex, first.inset_idx, false));
        return Some(first.inset_idx);
    };
    // Current direct upstreams are manifold at a selected vertex:
    // for any one face, at most two selected edge/path specs can
    // touch the same original corner. Future arbitrary B-Rep inputs
    // may need an N-way corner solver here instead of this two-offset
    // line intersection.

    let n1 = first.inward;
    let n2 = second.inward;
    let dot = vec_dot(n1, n2).clamp(-1.0, 1.0);
    let denom = 1.0 - dot * dot;
    let replacement_idx = if denom < 1e-6 {
        first.inset_idx
    } else {
        let base = positions[vertex as usize];
        let x = (first.distance - dot * second.distance) / denom;
        let y = (second.distance - dot * first.distance) / denom;
        let corner = vec_add(base, vec_add(vec_scale(n1, x), vec_scale(n2, y)));
        let idx = u32::try_from(positions.len()).unwrap_or(u32::MAX);
        positions.push(corner);
        idx
    };
    resolved.push((face_id, vertex, replacement_idx, true));
    Some(replacement_idx)
}

fn append_corner_patches(
    positions: &mut Vec<[f32; 3]>,
    indices: &mut Vec<u32>,
    face_labels: Option<&mut Vec<TopologyFaceId>>,
    endpoint_patches: &[CornerEndpointPatch],
    resolved_replacements: &[ResolvedFaceVertexReplacement],
) {
    let mut face_labels = face_labels;
    let mut processed_vertices: Vec<u32> = Vec::new();
    for patch in endpoint_patches {
        if processed_vertices.contains(&patch.vertex) {
            continue;
        }
        let matching: Vec<&CornerEndpointPatch> = endpoint_patches
            .iter()
            .filter(|candidate| candidate.vertex == patch.vertex)
            .collect();
        processed_vertices.push(patch.vertex);
        if matching.len() < 2 {
            continue;
        }

        let mut boundary: Vec<u32> = Vec::new();
        for candidate in matching {
            for &idx in &candidate.arc_indices {
                push_unique_position_index(&mut boundary, positions, idx);
            }
        }
        for replacement in resolved_replacements
            .iter()
            .filter(|replacement| replacement.vertex == patch.vertex && replacement.is_corner)
        {
            push_unique_position_index(&mut boundary, positions, replacement.replacement_idx);
        }
        if boundary.len() < 3 {
            continue;
        }

        sort_corner_boundary(positions, patch.vertex, &mut boundary);
        let center = average_positions(positions, &boundary);
        let center_idx = u32::try_from(positions.len()).unwrap_or(u32::MAX);
        positions.push(center);

        for i in 0..boundary.len() {
            let curr = boundary[i];
            let next = boundary[(i + 1) % boundary.len()];
            indices.push(center_idx);
            indices.push(next);
            indices.push(curr);
            if let Some(labels) = face_labels.as_mut() {
                (*labels).push(TopologyFaceId::DEGENERATE);
            }
        }
    }
}

fn push_unique_position_index(boundary: &mut Vec<u32>, positions: &[[f32; 3]], idx: u32) {
    let pos = positions[idx as usize];
    if boundary.iter().any(|&existing| {
        let diff = vec_sub(positions[existing as usize], pos);
        vec_len_sq(diff) < 1e-10
    }) {
        return;
    }
    boundary.push(idx);
}

fn average_positions(positions: &[[f32; 3]], indices: &[u32]) -> [f32; 3] {
    let mut sum = [0.0_f32, 0.0, 0.0];
    for &idx in indices {
        sum = vec_add(sum, positions[idx as usize]);
    }
    let inv = 1.0 / indices.len() as f32;
    vec_scale(sum, inv)
}

fn sort_corner_boundary(positions: &[[f32; 3]], vertex: u32, boundary: &mut [u32]) {
    let base = positions[vertex as usize];
    let center = average_positions(positions, boundary);
    let axis = vec_normalize_or(vec_sub(center, base), [0.0, 0.0, 1.0]);
    let seed = if axis[0].abs() < 0.8 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let u = vec_normalize_or(vec_cross(axis, seed), [1.0, 0.0, 0.0]);
    let v = vec_cross(axis, u);
    boundary.sort_by(|&left, &right| {
        let dl = vec_sub(positions[left as usize], center);
        let dr = vec_sub(positions[right as usize], center);
        let al = vec_dot(dl, v).atan2(vec_dot(dl, u));
        let ar = vec_dot(dr, v).atan2(vec_dot(dr, u));
        al.partial_cmp(&ar).unwrap_or(std::cmp::Ordering::Equal)
    });
}

// ---------------------------------------------------------------------------
// Operator-trait + accessor unit tests (upstream-agnostic).
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operators::CuboidOp;

    fn unit_cube() -> CuboidOp {
        CuboidOp {
            width: 1.0,
            height: 1.0,
            depth: 1.0,
        }
    }

    fn owner() -> BRepOwnerId {
        BRepOwnerId::from_bytes([0xed; 16])
    }

    #[test]
    fn op_kind_is_round_fillet() {
        let cube = unit_cube();
        let edge = cube.brep_edge_ids(owner())[0];
        let op = RoundFilletOp::new(&cube, owner(), vec![edge], 0.1).expect("ok");
        assert_eq!(op.op_kind(), OpKind::RoundFillet);
    }

    #[test]
    fn arity_is_one() {
        let cube = unit_cube();
        let edge = cube.brep_edge_ids(owner())[0];
        let op = RoundFilletOp::new(&cube, owner(), vec![edge], 0.1).expect("ok");
        assert_eq!(op.arity(), 1);
    }

    #[test]
    fn structural_hash_changes_with_radius() {
        let cube = unit_cube();
        let edge = cube.brep_edge_ids(owner())[0];
        let a = RoundFilletOp::new(&cube, owner(), vec![edge], 0.1).expect("a");
        let b = RoundFilletOp::new(&cube, owner(), vec![edge], 0.2).expect("b");
        assert_ne!(a.structural_hash(), b.structural_hash());
    }

    #[test]
    fn structural_hash_changes_with_edge_selection() {
        let cube = unit_cube();
        let edges = cube.brep_edge_ids(owner());
        let a = RoundFilletOp::new(&cube, owner(), vec![edges[0]], 0.1).expect("a");
        let b = RoundFilletOp::new(&cube, owner(), vec![edges[0], edges[1]], 0.1).expect("b");
        assert_ne!(a.structural_hash(), b.structural_hash());
    }

    #[test]
    fn structural_hash_is_deterministic() {
        let cube = unit_cube();
        let edge = cube.brep_edge_ids(owner())[0];
        let a = RoundFilletOp::new(&cube, owner(), vec![edge], 0.1).expect("a");
        let b = RoundFilletOp::new(&cube, owner(), vec![edge], 0.1).expect("b");
        assert_eq!(a.structural_hash(), b.structural_hash());
    }

    #[test]
    fn structural_hash_distinct_from_chamfer_fillet_byte_string() {
        // Chamfer FilletOp uses b"fillet:" prefix; RoundFilletOp uses
        // b"round_fillet:". Even with identical edges + radius +
        // owner, the structural_hashes MUST differ — distinct
        // operator types in the BLAKE3 derivation.
        use crate::operators::FilletOp;
        let cube = unit_cube();
        let edge = cube.brep_edge_ids(owner())[0];
        let chamfer = FilletOp::new(&cube, owner(), vec![edge], 0.1).expect("chamfer");
        let round = RoundFilletOp::new(&cube, owner(), vec![edge], 0.1).expect("round");
        assert_ne!(
            chamfer.structural_hash(),
            round.structural_hash(),
            "chamfer FilletOp and RoundFilletOp must produce distinct structural_hashes \
             even with identical edges / radius / owner — the BLAKE3 domain-separator \
             bytestring (b\"fillet:\" vs b\"round_fillet:\") is load-bearing for cache \
             non-collision."
        );
    }

    #[test]
    fn evaluate_rejects_wrong_arity_zero_inputs() {
        let cube = unit_cube();
        let edge = cube.brep_edge_ids(owner())[0];
        let op = RoundFilletOp::new(&cube, owner(), vec![edge], 0.1).expect("ok");
        let err = op.evaluate(&[]).unwrap_err();
        assert!(matches!(
            err,
            OpError::WrongArity {
                expected: 1,
                got: 0
            }
        ));
    }

    #[test]
    fn output_is_labeled_preserves_input_labeling() {
        let cube = unit_cube();
        let edge = cube.brep_edge_ids(owner())[0];
        let op = RoundFilletOp::new(&cube, owner(), vec![edge], 0.1).expect("ok");
        assert!(op.output_is_labeled(&[true]));
        assert!(!op.output_is_labeled(&[false]));
    }

    // -----------------------------------------------------------------------
    // sub-β.γ — general-dihedral cylinder math
    //
    // Tests pin the new general-dihedral evaluate body at three
    // dihedral angles (60° / 90° / 120°) plus radius/endpoint
    // invariants and the degenerate-dihedral rejection. The 90° case
    // is preserved within tight geometry-equivalence epsilon (Posture
    // A — single code path, no byte-identical fast path) so any
    // future float-drift gets caught here rather than silently
    // shifting downstream geometry.
    //
    // The synthetic upstream isolates the evaluate body's geometry
    // math from any per-upstream `RoundFilletUpstream::resolve_round_spec`
    // logic — current sub-α/β/γ upstreams emit only 90° specs by
    // construction, so a hand-crafted spec is the only way to
    // exercise the general-dihedral code paths until sub-β.γ-extend
    // dispatches lift per-upstream restrictions.
    // -----------------------------------------------------------------------

    /// Build a minimal labeled upstream tessellation suitable for
    /// driving `RoundFilletOp::evaluate` with a synthetic
    /// `RoundFilletSpec`. Vertices 0/1 are the filleted-edge
    /// endpoints; vertices 2/3 are dummy third-points so two
    /// triangles (one per adjacent face) reference vertex_a/vertex_b.
    fn synthetic_upstream_for_general_dihedral_tests() -> Tessellation {
        let positions = vec![
            [0.0, 0.0, 0.0],  // vertex_a
            [0.0, 1.0, 0.0],  // vertex_b (edge along +Y)
            [1.0, 0.5, 0.0],  // dummy for face_a triangle
            [-1.0, 0.5, 0.0], // dummy for face_b triangle
        ];
        let indices = vec![
            0, 1, 2, // triangle labeled face_a (TopologyFaceId(0))
            0, 1, 3, // triangle labeled face_b (TopologyFaceId(1))
        ];
        let labels = vec![TopologyFaceId(0), TopologyFaceId(1)];
        Tessellation::with_labels(positions, indices, labels).expect("synthetic upstream")
    }

    /// Build a synthetic `RoundFilletOp` carrying a single
    /// hand-crafted `RoundFilletSpec`. Bypasses
    /// `RoundFilletUpstream::resolve_round_spec` so tests can
    /// exercise non-90° dihedrals that no current upstream impl
    /// produces.
    fn make_synthetic_op(
        face_a_inward: [f32; 3],
        face_b_inward: [f32; 3],
        radius: f32,
    ) -> RoundFilletOp {
        RoundFilletOp {
            // edges field is unused at evaluate time (validation
            // happens at construction; we're bypassing it here).
            edges: Vec::new(),
            round_specs: vec![RoundFilletSpecKind::TwoEndpoint(RoundFilletSpec {
                vertex_a: 0,
                vertex_b: 1,
                face_a_id: TopologyFaceId(0),
                face_b_id: TopologyFaceId(1),
                face_a_inward,
                face_b_inward,
            })],
            radius,
            owner: BRepOwnerId::from_bytes([0xee; 16]),
        }
    }

    /// 60° dihedral: `cot(30°) = √3 ≈ 1.732` ⇒ inset offset distance
    /// from edge endpoint is `r · √3` along each face's inward
    /// direction. Pins the general-dihedral inset formula
    /// `pos + r · (1 + a·b) / sin(φ) · a` for acute dihedrals where
    /// the inset reaches FURTHER from the edge than the 90° case.
    #[test]
    fn evaluate_60_degree_dihedral_inset_distance_matches_cot_half_phi() {
        let sqrt3_over_2 = 3.0_f32.sqrt() / 2.0;
        let a = [1.0, 0.0, 0.0];
        let b = [0.5, sqrt3_over_2, 0.0]; // 60° from a (a·b = 0.5)
        let r = 1.0_f32;
        let op = make_synthetic_op(a, b, r);
        let upstream = synthetic_upstream_for_general_dihedral_tests();
        let out = op.evaluate(&[&upstream]).expect("evaluate 60°");

        // Inset_a1 at offset upstream.len() = 4. Expected:
        //   pos_a + r · cot(30°) · a = (0,0,0) + 1·√3·(1,0,0) = (√3, 0, 0).
        let expected_scale = 3.0_f32.sqrt();
        let inset_a1 = out.positions[upstream.positions.len()];
        assert!(
            (inset_a1[0] - expected_scale).abs() < 1e-5,
            "60° inset_a1.x: expected {expected_scale}, got {}",
            inset_a1[0]
        );
        assert!(inset_a1[1].abs() < 1e-5);
        assert!(inset_a1[2].abs() < 1e-5);

        // Inset_b1 at offset upstream.len() + 2. Expected:
        //   pos_a + r · cot(30°) · b = √3·(0.5, √3/2, 0) = (√3/2, 3/2, 0).
        let inset_b1 = out.positions[upstream.positions.len() + 2];
        assert!((inset_b1[0] - expected_scale * 0.5).abs() < 1e-5);
        assert!((inset_b1[1] - expected_scale * sqrt3_over_2).abs() < 1e-5);
        assert!(inset_b1[2].abs() < 1e-5);
    }

    /// 90° dihedral (sub-α/β/γ regression): inset_scale = cot(45°) =
    /// 1.0, so `inset_a1 = pos_a + r·a` byte-for-byte equivalent to
    /// the sub-α formula (within 1e-5 — float drift from the new
    /// arc_span computation is negligible at the inset placement
    /// step, which uses only `inset_scale` not `arc_span`). Pins
    /// the regression invariant — any change to the 90° inset
    /// placement breaks this assertion.
    #[test]
    fn evaluate_90_degree_dihedral_geometry_equivalence_with_sub_alpha_formula() {
        let a = [1.0, 0.0, 0.0];
        let b = [0.0, 1.0, 0.0]; // a·b = 0 (exact in f32; axis-aligned)
        let r = 0.3_f32;
        let op = make_synthetic_op(a, b, r);
        let upstream = synthetic_upstream_for_general_dihedral_tests();
        let out = op.evaluate(&[&upstream]).expect("evaluate 90°");

        // Inset_a1: sub-α formula = pos_a + r·a = (r, 0, 0).
        let inset_a1 = out.positions[upstream.positions.len()];
        assert!((inset_a1[0] - r).abs() < 1e-5, "got {inset_a1:?}");
        assert!(inset_a1[1].abs() < 1e-5);
        assert!(inset_a1[2].abs() < 1e-5);

        // Inset_b1: sub-α formula = pos_a + r·b = (0, r, 0).
        let inset_b1 = out.positions[upstream.positions.len() + 2];
        assert!(inset_b1[0].abs() < 1e-5);
        assert!((inset_b1[1] - r).abs() < 1e-5);
        assert!(inset_b1[2].abs() < 1e-5);

        // axis_center_1: sub-α formula = pos_a + r·(a+b) = (r, r, 0).
        // Verified via cylinder-radius invariant: every cylinder
        // endpoint-1 vertex sits at distance r from (r, r, 0).
        let axis_center_1 = [r, r, 0.0];
        let cylinder_start = upstream.positions.len() + 4;
        for k in 0..=ROUND_FILLET_SEGMENTS {
            let pos = out.positions[cylinder_start + 2 * k];
            let dx = pos[0] - axis_center_1[0];
            let dy = pos[1] - axis_center_1[1];
            let dz = pos[2] - axis_center_1[2];
            let dist = (dx * dx + dy * dy + dz * dz).sqrt();
            assert!(
                (dist - r).abs() < 1e-5,
                "90° cylinder vert k={k} dist {dist} != r {r}"
            );
        }
    }

    /// 120° dihedral: `cot(60°) = 1/√3 ≈ 0.577` ⇒ inset offset is
    /// CLOSER to the edge endpoint than the 90° case. Pins the
    /// obtuse-dihedral half of the general-dihedral inset formula.
    #[test]
    fn evaluate_120_degree_dihedral_inset_distance_matches_cot_half_phi() {
        let sqrt3_over_2 = 3.0_f32.sqrt() / 2.0;
        let a = [1.0, 0.0, 0.0];
        let b = [-0.5, sqrt3_over_2, 0.0]; // 120° from a (a·b = -0.5)
        let r = 1.0_f32;
        let op = make_synthetic_op(a, b, r);
        let upstream = synthetic_upstream_for_general_dihedral_tests();
        let out = op.evaluate(&[&upstream]).expect("evaluate 120°");

        let expected_scale = 1.0 / 3.0_f32.sqrt(); // cot(60°) = 1/√3
        let inset_a1 = out.positions[upstream.positions.len()];
        assert!(
            (inset_a1[0] - expected_scale).abs() < 1e-5,
            "120° inset_a1.x: expected {expected_scale}, got {}",
            inset_a1[0]
        );

        let inset_b1 = out.positions[upstream.positions.len() + 2];
        assert!((inset_b1[0] - expected_scale * -0.5).abs() < 1e-5);
        assert!((inset_b1[1] - expected_scale * sqrt3_over_2).abs() < 1e-5);
    }

    /// Cylinder-radius invariant across all three dihedral angles:
    /// every cylinder vertex sits at distance EXACTLY `r` from its
    /// endpoint's axis_center. Pins the orthonormal cross-section
    /// parameterization — if the radial formula
    /// `(cos(θ+φ)·a − cos(θ)·b) / sin(φ)` ever produces a non-unit-
    /// length vector, this assertion catches it before downstream
    /// consumers see a non-cylindrical "cylinder".
    #[test]
    fn evaluate_cylinder_vertex_radius_invariant_across_dihedrals() {
        for &angle_deg in &[60.0_f32, 90.0_f32, 120.0_f32] {
            let theta = angle_deg.to_radians();
            let a = [1.0, 0.0, 0.0];
            let b = [theta.cos(), theta.sin(), 0.0];

            let r = 0.2_f32;
            let op = make_synthetic_op(a, b, r);
            let upstream = synthetic_upstream_for_general_dihedral_tests();
            let out = op.evaluate(&[&upstream]).expect("evaluate");

            // axis_center_1 = pos_a + r/sin(φ) · (a + b).
            let pos_a = [0.0_f32, 0.0, 0.0];
            let sin_phi = theta.sin();
            let axis_center_1 = [
                pos_a[0] + r / sin_phi * (a[0] + b[0]),
                pos_a[1] + r / sin_phi * (a[1] + b[1]),
                pos_a[2] + r / sin_phi * (a[2] + b[2]),
            ];

            let cylinder_start = upstream.positions.len() + 4;
            for k in 0..=ROUND_FILLET_SEGMENTS {
                let pos = out.positions[cylinder_start + 2 * k];
                let dx = pos[0] - axis_center_1[0];
                let dy = pos[1] - axis_center_1[1];
                let dz = pos[2] - axis_center_1[2];
                let dist = (dx * dx + dy * dy + dz * dz).sqrt();
                assert!(
                    (dist - r).abs() < 1e-4,
                    "φ={angle_deg}° cylinder vert k={k} dist {dist} != r {r}"
                );
            }
        }
    }

    /// Arc endpoint coincidence: `vertex(θ=0)` must coincide with
    /// `inset_a` and `vertex(θ=arc_span)` must coincide with
    /// `inset_b`, within float epsilon, across multiple dihedrals.
    /// Pins the consistency between the arc parameterization and
    /// the inset placement — if either formula drifts independently,
    /// the cylinder surface would no longer tangent the two cap
    /// faces at the inset points and the rolled surface would
    /// "miss" the geometry.
    #[test]
    fn evaluate_arc_endpoints_coincide_with_inset_vertices() {
        for &angle_deg in &[60.0_f32, 90.0_f32, 120.0_f32] {
            let theta = angle_deg.to_radians();
            let a = [1.0, 0.0, 0.0];
            let b = [theta.cos(), theta.sin(), 0.0];
            let r = 0.5_f32;
            let op = make_synthetic_op(a, b, r);
            let upstream = synthetic_upstream_for_general_dihedral_tests();
            let out = op.evaluate(&[&upstream]).expect("evaluate");

            let inset_a1 = out.positions[upstream.positions.len()];
            let inset_b1 = out.positions[upstream.positions.len() + 2];
            let cylinder_start = upstream.positions.len() + 4;
            let cyl_first = out.positions[cylinder_start];
            let cyl_last = out.positions[cylinder_start + 2 * ROUND_FILLET_SEGMENTS];

            let dist_first_to_a = ((cyl_first[0] - inset_a1[0]).powi(2)
                + (cyl_first[1] - inset_a1[1]).powi(2)
                + (cyl_first[2] - inset_a1[2]).powi(2))
            .sqrt();
            let dist_last_to_b = ((cyl_last[0] - inset_b1[0]).powi(2)
                + (cyl_last[1] - inset_b1[1]).powi(2)
                + (cyl_last[2] - inset_b1[2]).powi(2))
            .sqrt();
            assert!(
                dist_first_to_a < 1e-4,
                "φ={angle_deg}° vertex(θ=0) at {cyl_first:?} should coincide with \
                 inset_a1 at {inset_a1:?} (dist {dist_first_to_a})"
            );
            assert!(
                dist_last_to_b < 1e-4,
                "φ={angle_deg}° vertex(θ=arc_span) at {cyl_last:?} should coincide \
                 with inset_b1 at {inset_b1:?} (dist {dist_last_to_b})"
            );
        }
    }

    /// Degenerate dihedrals (faces coplanar same-side `φ→0°` OR
    /// anti-parallel knife-edge `φ→180°`) reject at evaluate time
    /// with [`OpError::InvalidParameter`]. The `dot_ab.clamp(-1, 1)`
    /// before `acos`/`sqrt` prevents NaN on tiny float overshoot
    /// from non-unit-length inputs; the `sin_phi_sq <
    /// DIHEDRAL_EPSILON_SQ` guard then catches the degenerate case
    /// uniformly for both `dot_ab → +1` and `dot_ab → −1`. No new
    /// `RoundFilletError` variant per ADR-119 D-α scope; existing
    /// `OpError::InvalidParameter` carries the rejection signal.
    #[test]
    fn evaluate_rejects_near_degenerate_dihedral_at_zero_and_pi() {
        let upstream = synthetic_upstream_for_general_dihedral_tests();

        // a · b = 1: faces coplanar same-side (φ=0°). Exact unit
        // vectors, no float ambiguity.
        let op_parallel = make_synthetic_op([1.0, 0.0, 0.0], [1.0, 0.0, 0.0], 0.1);
        match op_parallel.evaluate(&[&upstream]).unwrap_err() {
            OpError::InvalidParameter(msg) => {
                assert!(
                    msg.contains("degenerate dihedral") || msg.contains("near-coplanar"),
                    "parallel-inward case: expected degenerate-dihedral message, got: {msg}"
                );
            }
            other => panic!("parallel-inward: expected InvalidParameter, got {other:?}"),
        }

        // a · b = -1: anti-parallel knife edge (φ=π).
        let op_anti = make_synthetic_op([1.0, 0.0, 0.0], [-1.0, 0.0, 0.0], 0.1);
        match op_anti.evaluate(&[&upstream]).unwrap_err() {
            OpError::InvalidParameter(msg) => {
                assert!(
                    msg.contains("degenerate dihedral") || msg.contains("knife-edge"),
                    "anti-parallel case: expected degenerate-dihedral message, got: {msg}"
                );
            }
            other => panic!("anti-parallel: expected InvalidParameter, got {other:?}"),
        }

        // a · b = 1 + tiny overshoot (e.g., non-unit-length input):
        // the clamp catches this before acos / sqrt would NaN. Tests
        // the clamp guard itself, not just the geometric degeneracy.
        let op_overshoot = make_synthetic_op([1.000001, 0.0, 0.0], [1.0, 0.0, 0.0], 0.1);
        let err = op_overshoot.evaluate(&[&upstream]).unwrap_err();
        assert!(
            matches!(err, OpError::InvalidParameter(_)),
            "ULP-overshoot input must clamp + reject, got {err:?}"
        );
    }
}
