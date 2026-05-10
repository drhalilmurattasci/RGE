//! `brep-render` sub-α end-to-end smoke for [`RenderMesh::from_buffers`].
//!
//! Exercises the buffer-typed conversion against synthetic CAD-shaped
//! tessellation buffers. The fixtures reproduce the canonical-shape
//! contracts of:
//!
//! * `CuboidOp::evaluate` (sub-α D-projection-α) — 8 verts / 12 triangles /
//!   6 faces in NegZ→PosZ→NegY→PosY→NegX→PosX order;
//! * `ExtrudeOp::evaluate` for an `n=4` square profile (sub-β D-projection-β)
//!   — `4n - 4 = 12` triangles, Bottom+Top+N Sides face structure;
//! * `RevolveOp::evaluate_partial` for `n=4` profile + `segments=8` (sub-γδ
//!   D-projection-γ) — `2*n*segments + 2*(n-2) = 68` triangles, ring-major
//!   side-wall + start-cap + end-cap face structure.
//!
//! The fixtures are duplicated inline rather than imported from cad-core
//! because `forbidden-dep` rule 6 forbids `rge-brep-render` from depending
//! on `rge-cad-core` (renderer crates cannot depend on game-domain
//! crates per PLAN.md §1.3). The substrate's API ingests opaque buffer
//! triples; the conversion contract is byte-for-byte the same regardless
//! of where the input came from.

use rge_brep_render::RenderMesh;

// ---------------------------------------------------------------------------
// Test 1 — Cuboid face normals match six axis directions.
// ---------------------------------------------------------------------------

/// For a 1×1×1 cuboid with the canonical `CuboidOp` face emission order
/// (`NegZ → PosZ → NegY → PosY → NegX → PosX`; `TopologyFaceId(0..6)`),
/// every triangle in face group `i` must share the expected axis-aligned
/// outward normal:
/// * Tag(0)=NegZ → `(0, 0, -1)`
/// * Tag(1)=PosZ → `(0, 0, +1)`
/// * Tag(2)=NegY → `(0, -1, 0)`
/// * Tag(3)=PosY → `(0, +1, 0)`
/// * Tag(4)=NegX → `(-1, 0, 0)`
/// * Tag(5)=PosX → `(+1, 0, 0)`
///
/// Tolerance `1e-5`.
#[test]
fn cuboid_render_mesh_face_normals_match_six_axis_directions() {
    let (positions, indices, labels) = synthetic_cuboid_buffers();
    let mesh = RenderMesh::from_buffers(&positions, &indices, Some(&labels));

    let expected_normals: [(u64, [f32; 3]); 6] = [
        (0, [0.0, 0.0, -1.0]),
        (1, [0.0, 0.0, 1.0]),
        (2, [0.0, -1.0, 0.0]),
        (3, [0.0, 1.0, 0.0]),
        (4, [-1.0, 0.0, 0.0]),
        (5, [1.0, 0.0, 0.0]),
    ];

    let labels_out = mesh.face_labels.as_ref().expect("labeled");
    assert_eq!(labels_out.len(), 12);

    for (tag, expected) in expected_normals {
        let mut tris_in_group = 0_usize;
        for (tri_idx, label) in labels_out.iter().enumerate() {
            if *label != tag {
                continue;
            }
            tris_in_group += 1;
            // All 3 output vertices for this triangle share the same
            // flat-shaded normal.
            for v_offset in 0..3 {
                let n = mesh.normals[tri_idx * 3 + v_offset];
                for axis in 0..3 {
                    assert!(
                        (n[axis] - expected[axis]).abs() < 1e-5,
                        "tag {tag} tri {tri_idx} v_offset {v_offset} axis {axis}: \
                         expected {expected:?} got {n:?}"
                    );
                }
            }
        }
        assert_eq!(
            tris_in_group, 2,
            "Cuboid face tag {tag} must have exactly 2 triangles"
        );
    }
}

// ---------------------------------------------------------------------------
// Test 2 — Extrude (square, n=4) triangle count matches D-projection-β.
// ---------------------------------------------------------------------------

/// `ExtrudeOp::evaluate` for an `n=4` square profile produces a labeled
/// `Tessellation` with `4n - 4 = 12` triangles per the D-projection-β
/// contract: `n - 2 = 2` Bottom-cap triangles tagged `TopologyFaceId(0)`,
/// `n - 2 = 2` Top-cap triangles tagged `TopologyFaceId(1)`, and `2`
/// triangles per side wall tagged `TopologyFaceId(2..6)`.
///
/// The converted [`RenderMesh`] has:
/// * `36` positions (12 triangles × 3 vertex tripling)
/// * `36` normals (one per output vertex, flat-shaded)
/// * `36` indices (dense `0..36`)
/// * `12` face_labels (one per input triangle, preserved 1:1).
#[test]
fn extrude_square_render_mesh_triangle_count_matches_d_projection_beta_contract() {
    let (positions, indices, labels) = synthetic_extrude_square_buffers();
    assert_eq!(indices.len() / 3, 12, "n=4 square ⇒ 4n-4 = 12 triangles");
    assert_eq!(labels.len(), 12);

    let mesh = RenderMesh::from_buffers(&positions, &indices, Some(&labels));
    assert_eq!(mesh.positions.len(), 36);
    assert_eq!(mesh.normals.len(), 36);
    assert_eq!(mesh.indices.len(), 36);
    assert_eq!(mesh.face_labels.as_ref().expect("labeled").len(), 12);
    // Output indices are dense 0..36.
    assert_eq!(mesh.indices, (0_u32..36).collect::<Vec<u32>>());
    // face_labels are preserved 1:1 (same length, same values, same order).
    assert_eq!(
        mesh.face_labels.as_ref().expect("labeled"),
        &labels,
        "face_labels must be preserved 1:1 by from_buffers"
    );
}

// ---------------------------------------------------------------------------
// Test 3 — Revolve Partial (n=4, segments=8) face_labels count round-trip.
// ---------------------------------------------------------------------------

/// `RevolveOp::evaluate_partial` for `n=4` profile + `segments=8` produces
/// a labeled `Tessellation` with `2 * n * segments + 2 * (n - 2) = 64 + 4
/// = 68` triangles per the D-projection-γ contract: `64` side-wall
/// triangles in ring-major order tagged `TopologyFaceId(0..n)` (each
/// Side(i) appearing `2*segments` times) followed by `n - 2 = 2`
/// start-cap fan triangles tagged `TopologyFaceId(n)` and `n - 2 = 2`
/// end-cap fan triangles tagged `TopologyFaceId(n+1)`.
///
/// The converted [`RenderMesh`] has:
/// * `204` positions (68 × 3 vertex tripling)
/// * `204` normals
/// * `204` indices (dense `0..204`)
/// * `68` face_labels (one per input triangle, preserved 1:1).
#[test]
fn revolve_partial_render_mesh_round_trips_face_labels_count_per_d_projection_gamma() {
    let (positions, indices, labels) = synthetic_revolve_partial_buffers();
    let triangle_count = indices.len() / 3;
    assert_eq!(
        triangle_count, 68,
        "n=4 segments=8 ⇒ 2*n*segments + 2*(n-2) = 68 triangles"
    );
    assert_eq!(labels.len(), 68);

    let mesh = RenderMesh::from_buffers(&positions, &indices, Some(&labels));
    assert_eq!(mesh.positions.len(), 204);
    assert_eq!(mesh.normals.len(), 204);
    assert_eq!(mesh.indices.len(), 204);
    assert_eq!(mesh.face_labels.as_ref().expect("labeled").len(), 68);
    assert_eq!(mesh.indices, (0_u32..204).collect::<Vec<u32>>());

    // Per D-projection-γ, the canonical face_label distribution is:
    //   - tags 0..n (Sides)        → 2*segments each = 16 each, total 64
    //   - tag n   (StartCap)       → n - 2          = 2
    //   - tag n+1 (EndCap)         → n - 2          = 2
    let labels_out = mesh.face_labels.as_ref().expect("labeled");
    let mut counts = [0_usize; 6]; // tags 0..6 = 4 sides + 2 caps
    for label in labels_out {
        let idx = *label as usize;
        assert!(
            idx < counts.len(),
            "face_label {label} out of expected range 0..6"
        );
        counts[idx] += 1;
    }
    assert_eq!(counts[0], 16, "Side(0) must appear 2*segments = 16 times");
    assert_eq!(counts[1], 16, "Side(1) must appear 2*segments = 16 times");
    assert_eq!(counts[2], 16, "Side(2) must appear 2*segments = 16 times");
    assert_eq!(counts[3], 16, "Side(3) must appear 2*segments = 16 times");
    assert_eq!(counts[4], 2, "StartCap must appear n - 2 = 2 times");
    assert_eq!(counts[5], 2, "EndCap must appear n - 2 = 2 times");
}

// ===========================================================================
// Synthetic CAD-shaped fixtures — duplicated inline (rule 6: brep-render
// cannot depend on cad-core).
// ===========================================================================

/// Canonical `CuboidOp::evaluate` shape: 8 verts of the
/// [-0.5, +0.5]^3 cube + 12 indices in NegZ→PosZ→NegY→PosY→NegX→PosX
/// face emission order + 12 face_labels (`TopologyFaceId(0..6)`, 2 per
/// face).
fn synthetic_cuboid_buffers() -> (Vec<[f32; 3]>, Vec<u32>, Vec<u64>) {
    // Vertex layout:
    //   0: (-0.5, -0.5, -0.5)   4: (-0.5, -0.5, +0.5)
    //   1: (+0.5, -0.5, -0.5)   5: (+0.5, -0.5, +0.5)
    //   2: (+0.5, +0.5, -0.5)   6: (+0.5, +0.5, +0.5)
    //   3: (-0.5, +0.5, -0.5)   7: (-0.5, +0.5, +0.5)
    let positions = vec![
        [-0.5_f32, -0.5, -0.5],
        [0.5, -0.5, -0.5],
        [0.5, 0.5, -0.5],
        [-0.5, 0.5, -0.5],
        [-0.5, -0.5, 0.5],
        [0.5, -0.5, 0.5],
        [0.5, 0.5, 0.5],
        [-0.5, 0.5, 0.5],
    ];
    // Indices in CCW order viewed from each face's outward normal. Face
    // order matches `CuboidOp::evaluate`: NegZ → PosZ → NegY → PosY →
    // NegX → PosX.
    let indices = vec![
        // NegZ (face 0): viewed from -Z toward origin, CCW winding.
        0, 2, 1, 0, 3, 2, // PosZ (face 1).
        4, 5, 6, 4, 6, 7, // NegY (face 2).
        0, 1, 5, 0, 5, 4, // PosY (face 3).
        2, 3, 7, 2, 7, 6, // NegX (face 4).
        0, 4, 7, 0, 7, 3, // PosX (face 5).
        1, 2, 6, 1, 6, 5,
    ];
    let labels = vec![0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5_u64];
    (positions, indices, labels)
}

/// Synthetic `ExtrudeOp::evaluate` shape for an `n=4` square profile,
/// `length=1`. Returns positions / indices / labels matching the
/// D-projection-β contract: 2 Bottom + 2 Top + 8 Side triangles, totaling
/// 12 = 4n - 4. Geometry: unit square at z=0 (bottom) and z=1 (top), CCW
/// when viewed from +Z; face_labels in `Bottom → Top → Side(0..4)` order
/// (the canonical `impl BRepProvider for ExtrudeOp`'s emission order).
fn synthetic_extrude_square_buffers() -> (Vec<[f32; 3]>, Vec<u32>, Vec<u64>) {
    // 8 positions: 4 bottom + 4 top, in profile-iteration order.
    //   0: (0, 0, 0)    4: (0, 0, 1)
    //   1: (1, 0, 0)    5: (1, 0, 1)
    //   2: (1, 1, 0)    6: (1, 1, 1)
    //   3: (0, 1, 0)    7: (0, 1, 1)
    let positions = vec![
        [0.0_f32, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [1.0, 0.0, 1.0],
        [1.0, 1.0, 1.0],
        [0.0, 1.0, 1.0],
    ];
    // Bottom cap: 2 triangles (CCW from -Z), face_label TopologyFaceId(0).
    // Top cap:    2 triangles (CCW from +Z), face_label TopologyFaceId(1).
    // Sides 0..4: 2 triangles each, face_label TopologyFaceId(2..6).
    let indices = vec![
        // Bottom (TopologyFaceId(0)).
        0, 2, 1, 0, 3, 2, // Top (TopologyFaceId(1)).
        4, 5, 6, 4, 6, 7, // Side 0 (edge 0→1, TopologyFaceId(2)).
        0, 1, 5, 0, 5, 4, // Side 1 (edge 1→2, TopologyFaceId(3)).
        1, 2, 6, 1, 6, 5, // Side 2 (edge 2→3, TopologyFaceId(4)).
        2, 3, 7, 2, 7, 6, // Side 3 (edge 3→0, TopologyFaceId(5)).
        3, 0, 4, 3, 4, 7,
    ];
    let labels = vec![0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5_u64];
    (positions, indices, labels)
}

/// Synthetic `RevolveOp::evaluate_partial` shape for `n=4` profile +
/// `segments=8` (partial mode angle < 2π). Returns positions / indices /
/// labels matching the D-projection-γ contract: 64 side-wall triangles
/// in ring-major order tagged `TopologyFaceId(0..4)` (each Side(i)
/// appearing `2*segments=16` times) + 2 start-cap fan triangles tagged
/// `TopologyFaceId(4)` + 2 end-cap fan triangles tagged
/// `TopologyFaceId(5)`. Total: 68 triangles, `2 * n * segments + 2 * (n-2)`.
///
/// The geometry shape (positions) is illustrative — the load-bearing test
/// is on the *count* of each `face_label` group, NOT the geometric
/// validity of each ring. (The substrate's `from_buffers` only reads
/// position values to compute normals; correctness against the real
/// `RevolveOp` emission order would require a cad-core dep, which rule 6
/// forbids.)
fn synthetic_revolve_partial_buffers() -> (Vec<[f32; 3]>, Vec<u32>, Vec<u64>) {
    const N: usize = 4;
    const SEGMENTS: usize = 8;

    // Build a position layout that's structurally usable as a triangle
    // soup: 1 position per (segment, profile_vertex) combination, plus
    // the trailing ring at segment SEGMENTS, plus 1 axis-on-Y position
    // for the cap fans.
    //   Side wall positions: (segments+1) rings × N profile verts.
    //   Cap-fan vertices reuse existing positions.
    let mut positions: Vec<[f32; 3]> = Vec::new();
    for ring in 0..=SEGMENTS {
        let theta = (ring as f32) * std::f32::consts::FRAC_PI_2 / (SEGMENTS as f32); // arbitrary partial angle ~π/2
        let cos_t = theta.cos();
        let sin_t = theta.sin();
        // Profile vertices in XY plane (x >= 0 per RevolveOp's +X-side
        // restriction); 4 verts of a square of side 1 stuck against the
        // y-axis at x in [0.5, 1.5], y in [0, 1].
        let profile = [[0.5_f32, 0.0], [1.5, 0.0], [1.5, 1.0], [0.5, 1.0]];
        for [px, py] in profile {
            // Sweep the profile around the Y-axis by angle theta.
            positions.push([px * cos_t, py, px * sin_t]);
        }
    }

    let mut indices: Vec<u32> = Vec::new();
    let mut labels: Vec<u64> = Vec::new();

    // Side-wall triangles in ring-major order:
    //   for ring r in 0..segments
    //     for edge i in 0..N
    //       emit 2 triangles tagged TopologyFaceId(i)
    for ring in 0..SEGMENTS {
        for edge_i in 0..N {
            let next_v = (edge_i + 1) % N;
            // Ring r: positions[ring*N + edge_i]; ring r+1: positions[(ring+1)*N + edge_i].
            let v00 = (ring * N + edge_i) as u32;
            let v01 = (ring * N + next_v) as u32;
            let v10 = ((ring + 1) * N + edge_i) as u32;
            let v11 = ((ring + 1) * N + next_v) as u32;
            // Triangle 1: (v00, v01, v11).
            indices.extend_from_slice(&[v00, v01, v11]);
            labels.push(edge_i as u64);
            // Triangle 2: (v00, v11, v10).
            indices.extend_from_slice(&[v00, v11, v10]);
            labels.push(edge_i as u64);
        }
    }
    // 4 sides * 8 segments * 2 triangles = 64 side-wall triangles.

    // Start-cap fan: n - 2 = 2 triangles, all tagged TopologyFaceId(N).
    // Use ring 0 profile: positions[0..N].
    for tri in 0..(N - 2) {
        let v0 = 0_u32;
        let v1 = (tri + 1) as u32;
        let v2 = (tri + 2) as u32;
        indices.extend_from_slice(&[v0, v1, v2]);
        labels.push(N as u64);
    }
    // End-cap fan: n - 2 = 2 triangles, tagged TopologyFaceId(N+1).
    // Use ring SEGMENTS profile: positions[SEGMENTS*N..(SEGMENTS+1)*N].
    let base = (SEGMENTS * N) as u32;
    for tri in 0..(N - 2) {
        let v0 = base;
        let v1 = base + (tri + 1) as u32;
        let v2 = base + (tri + 2) as u32;
        indices.extend_from_slice(&[v0, v1, v2]);
        labels.push((N + 1) as u64);
    }
    // 64 + 2 + 2 = 68 triangles total.

    (positions, indices, labels)
}
