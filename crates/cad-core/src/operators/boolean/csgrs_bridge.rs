//! csgrs ↔ cad-core triangle-soup conversion bridge.
//!
//! Failure class: snapshot-recoverable (inherited via the cad-core lib root).
//!
//! Sub-module of [`crate::operators::boolean`]; see that module's `//!` docs
//! for the design rationale (ADR-112) + the full csgrs feature surface.
//!
//! This file owns the f32 ↔ f64 conversion at the csgrs boundary, the
//! degenerate-triangle pre-filter (csgrs's BSP panics on coincident-vertex
//! input), the outward-normal computation from CCW winding, and the panic-
//! catching dispatch wrapper for the three `CSG` boolean operations.
//!
//! Helpers exposed here are `pub(super)` so the labeled / unlabeled paths in
//! sibling modules can call them; nothing escapes the `boolean` module.

use std::fmt::Debug;
use std::panic::AssertUnwindSafe;

use csgrs::mesh::polygon::Polygon as CsgrsPolygon;
use csgrs::mesh::vertex::Vertex as CsgrsVertex;
use csgrs::mesh::Mesh as CsgrsMesh;
use csgrs::traits::CSG;
use nalgebra::{Point3, Vector3};

use crate::operators::boolean::BooleanMode;
use crate::operators::OpError;

/// Compute the right-hand-rule outward normal of a triangle defined by three
/// f64 positions in CCW order. Returns a zero-vector for degenerate triangles
/// (the caller filters those before reaching here, but be defensive).
fn triangle_normal_f64(a: Point3<f64>, b: Point3<f64>, c: Point3<f64>) -> Vector3<f64> {
    let ab = b - a;
    let ac = c - a;
    let n = ab.cross(&ac);
    let n_sq = n.norm_squared();
    if n_sq > 0.0 {
        n / n_sq.sqrt()
    } else {
        Vector3::zeros()
    }
}

/// Convert a cad-core triangle-soup mesh to a csgrs [`Mesh<M>`] carrying
/// per-polygon metadata `M`.
///
/// Each input triangle becomes a 3-vertex csgrs [`CsgrsPolygon`]. Per-vertex
/// normals are the triangle face normal (right-hand rule from CCW winding);
/// uniform per-face normals are correct for triangle soup.
///
/// The `metadata` closure is invoked once per **input triangle** (in
/// input-order, even for filtered-degenerate triangles — they advance
/// `triangle_idx` but do not consume a metadata slot). Pass `|_| ()` for
/// the no-metadata path.
///
/// Degenerate triangles (zero-area / zero-length edges) are filtered before
/// [`CsgrsPolygon::new`] since csgrs panics on degenerate planes.
pub(super) fn tessellation_to_csgrs<M>(
    positions: &[[f32; 3]],
    indices: &[u32],
    metadata: impl Fn(usize) -> M,
) -> CsgrsMesh<M>
where
    M: Clone + Send + Sync + Debug + 'static,
{
    // Pre-allocate with the upper bound: `triangle_count` polygons (some may
    // be filtered for degeneracy, so the actual count may be lower).
    let triangle_count = indices.len() / 3;
    let mut polygons: Vec<CsgrsPolygon<M>> = Vec::with_capacity(triangle_count);

    for (tri_idx, tri) in indices.chunks_exact(3).enumerate() {
        let i0 = tri[0] as usize;
        let i1 = tri[1] as usize;
        let i2 = tri[2] as usize;

        // Tessellation::new validated bounds, but be defensive.
        if i0 >= positions.len() || i1 >= positions.len() || i2 >= positions.len() {
            continue;
        }

        let p0 = positions[i0];
        let p1 = positions[i1];
        let p2 = positions[i2];

        // Convert f32 → f64 at the boundary.
        let a = Point3::new(f64::from(p0[0]), f64::from(p0[1]), f64::from(p0[2]));
        let b = Point3::new(f64::from(p1[0]), f64::from(p1[1]), f64::from(p1[2]));
        let c = Point3::new(f64::from(p2[0]), f64::from(p2[1]), f64::from(p2[2]));

        // Filter degenerate triangles: any pair coincident OR area near zero.
        let normal = triangle_normal_f64(a, b, c);
        if normal == Vector3::zeros() {
            continue;
        }

        let v0 = CsgrsVertex::new(a, normal);
        let v1 = CsgrsVertex::new(b, normal);
        let v2 = CsgrsVertex::new(c, normal);

        polygons.push(CsgrsPolygon::new(vec![v0, v1, v2], Some(metadata(tri_idx))));
    }

    CsgrsMesh::from_polygons(&polygons, None)
}

/// Triangle-soup output of [`csgrs_to_tessellation`]: positions, indices,
/// per-output-triangle metadata. Aliased so the function signature isn't a
/// clippy `type_complexity` violation.
pub(super) type TriangleSoupWithLabels<M> = (Vec<[f32; 3]>, Vec<u32>, Vec<M>);

/// Convert a csgrs [`Mesh<M>`] back to triangle-soup buffers + a per-output-
/// triangle metadata vector. Polygons with `N > 3` vertices are
/// fan-triangulated from `vertex[0]` (csgrs polygons are coplanar). Each
/// output triangle clones its source polygon's metadata.
///
/// Vertex dedup uses exact f32 bit equality (12-byte LE-byte key) after the
/// f64 → f32 conversion — required for BLAKE3-determinism.
///
/// Polygons with csgrs `metadata = None` (rhs-derived under Difference's
/// lhs-retag quirk) yield `unmetadata_label()`: `()` (unlabeled) or
/// [`crate::tessellation::TopologyFaceId::DEGENERATE`] (labeled, routed
/// through Reinterpreted).
///
/// # Errors
///
/// * [`OpError::InvalidParameter`] on non-finite output position or
///   `u32::MAX` vertex count overflow.
pub(super) fn csgrs_to_tessellation<M>(
    mesh: &CsgrsMesh<M>,
    unmetadata_label: impl Fn() -> M,
) -> Result<TriangleSoupWithLabels<M>, OpError>
where
    M: Clone + Send + Sync + Debug + 'static,
{
    use std::collections::BTreeMap;

    // Vertex de-dup map keyed on the 12-byte f32 LE bit pattern of (x,y,z)
    // — exact-equality so determinism is bit-stable across iterations.
    let mut dedup: BTreeMap<[u8; 12], u32> = BTreeMap::new();
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut labels: Vec<M> = Vec::new();

    let mut intern = |pos_f32: [f32; 3]| -> Option<u32> {
        let mut key = [0u8; 12];
        key[0..4].copy_from_slice(&pos_f32[0].to_le_bytes());
        key[4..8].copy_from_slice(&pos_f32[1].to_le_bytes());
        key[8..12].copy_from_slice(&pos_f32[2].to_le_bytes());

        if let Some(&existing) = dedup.get(&key) {
            return Some(existing);
        }
        // u32::MAX is used as "no more slots"; we never expect to hit that
        // for realistic boolean output but guard against overflow.
        let new_index = u32::try_from(positions.len()).ok()?;
        positions.push(pos_f32);
        dedup.insert(key, new_index);
        Some(new_index)
    };

    for poly in &mesh.polygons {
        let n = poly.vertices.len();
        if n < 3 {
            // csgrs shouldn't emit < 3 vertex polygons but be defensive.
            continue;
        }

        // Pre-intern all vertex indices for this polygon.
        let mut vertex_indices: Vec<u32> = Vec::with_capacity(n);
        let mut had_overflow = false;
        for v in &poly.vertices {
            // f64 → f32 conversion at the boundary.
            #[allow(
                clippy::cast_possible_truncation,
                reason = "f64 → f32 boundary conversion at csgrs ↔ cad-core boundary; csgrs output coordinates fit f32 precision for any realistic mesh; NaN/inf cases trapped via is_finite check below"
            )]
            let pos_f32 = [v.pos.x as f32, v.pos.y as f32, v.pos.z as f32];
            // Reject NaN / infinite outputs from csgrs (snap to error).
            if !pos_f32[0].is_finite() || !pos_f32[1].is_finite() || !pos_f32[2].is_finite() {
                return Err(OpError::InvalidParameter(format!(
                    "boolean failed: csgrs produced non-finite vertex {pos_f32:?}"
                )));
            }
            if let Some(idx) = intern(pos_f32) {
                vertex_indices.push(idx);
            } else {
                had_overflow = true;
                break;
            }
        }
        if had_overflow {
            return Err(OpError::InvalidParameter(
                "boolean failed: vertex count exceeds u32::MAX".to_string(),
            ));
        }

        // Pull the polygon's metadata once; clone per emitted fan triangle.
        // csgrs's Polygon::metadata is Option<M>; if None (which Difference
        // can produce for rhs-derived faces per the lhs-retag quirk), fall
        // back to the caller-supplied unmetadata sentinel. For the labeled
        // path that's TopologyFaceId::DEGENERATE — a sentinel that's
        // distinct from every real input face id and is treated as
        // "Reinterpreted" by the downstream inference.
        let poly_meta: M = poly.metadata.clone().unwrap_or_else(&unmetadata_label);

        // Fan-triangulate from vertex 0: (0, i, i+1) for i in 1..n-1.
        for i in 1..n - 1 {
            let i0 = vertex_indices[0];
            let i1 = vertex_indices[i];
            let i2 = vertex_indices[i + 1];
            // Skip fan triangles that collapse to coincident indices (can
            // happen if two polygon vertices ended up bit-identical after
            // f32 conversion).
            if i0 == i1 || i1 == i2 || i0 == i2 {
                continue;
            }
            indices.push(i0);
            indices.push(i1);
            indices.push(i2);
            labels.push(poly_meta.clone());
        }
    }

    Ok((positions, indices, labels))
}

/// Shared implementation of the boolean dispatch + panic catch — used by
/// both the unlabeled and labeled paths.
pub(super) fn run_boolean<S>(
    mode: BooleanMode,
    lhs_mesh: &CsgrsMesh<S>,
    rhs_mesh: &CsgrsMesh<S>,
) -> Result<CsgrsMesh<S>, OpError>
where
    S: Clone + Send + Sync + Debug + 'static,
{
    // Run the boolean inside catch_unwind. csgrs's BSP can panic on
    // pathological input (e.g. all-coincident vertices that survived our
    // pre-filter, very-near-degenerate triangles). We surface those as
    // InvalidParameter rather than poisoning the caller.
    std::panic::catch_unwind(AssertUnwindSafe(|| match mode {
        BooleanMode::Union => lhs_mesh.union(rhs_mesh),
        BooleanMode::Intersection => lhs_mesh.intersection(rhs_mesh),
        BooleanMode::Difference => lhs_mesh.difference(rhs_mesh),
    }))
    // UNTESTABLE-DEFENSIVE: panic shield over csgrs 0.20.1 (Cargo.lock pin);
    // defensive-only-no-known-trigger after pre-filters in `tessellation_to_csgrs`
    // + the f32-finiteness post-check narrow what could still panic. NOT
    // unreachable. Real behavioral coverage lives in
    // `tests/boolean_panic_recovery.rs`, which asserts panic-free behavior
    // under pathological input. Re-run those fixtures if the csgrs pin
    // changes before trusting this classification.
    .map_err(|_| {
        OpError::InvalidParameter(
            "boolean failed: csgrs panicked on pathological input".to_string(),
        )
    })
}
