//! Unit tests for [`crate::operators::revolve::RevolveOp`].
//!
//! Sub-module of [`crate::operators::revolve`]; see that module's `//!` docs
//! for the design rationale.

use super::*;

fn ccw_right_triangle_on_plus_x() -> Polygon2D {
    // Right triangle on +X side: (1,0) → (2,0) → (1,1) → close.
    // signed_area = 0.5 (CCW).
    Polygon2D::new(vec![[1.0, 0.0], [2.0, 0.0], [1.0, 1.0]]).expect("right triangle")
}

fn ccw_square_on_plus_x() -> Polygon2D {
    // Unit square, x in [1, 2], y in [0, 1] — CCW.
    Polygon2D::new(vec![[1.0, 0.0], [2.0, 0.0], [2.0, 1.0], [1.0, 1.0]]).expect("ccw +x square")
}

fn cw_square_on_plus_x() -> Polygon2D {
    // Same +X-side square footprint, listed CW.
    Polygon2D::new(vec![[1.0, 0.0], [1.0, 1.0], [2.0, 1.0], [2.0, 0.0]]).expect("cw +x square")
}

fn ccw_concave_l_on_plus_x() -> Polygon2D {
    // L-shape on +X side: outer corners (1,0)..(3,0)..(3,1)..(2,1)..(2,2)..(1,2).
    // signed_area > 0 (CCW); concave at (2,1).
    Polygon2D::new(vec![
        [1.0, 0.0],
        [3.0, 0.0],
        [3.0, 1.0],
        [2.0, 1.0],
        [2.0, 2.0],
        [1.0, 2.0],
    ])
    .expect("ccw +x L-shape")
}

fn ccw_axis_touching_triangle() -> Polygon2D {
    // Right triangle with one vertex on the Y-axis: (0,0) → (1,0) → (0,1).
    Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]).expect("axis-touching triangle")
}

// -- RevolveOp::new ------------------------------------------------------

#[test]
fn revolve_new_rejects_segments_below_3() {
    let err = RevolveOp::new(ccw_square_on_plus_x(), 2).unwrap_err();
    match err {
        OpError::InvalidParameter(msg) => {
            assert!(msg.contains("segments"), "msg = {msg}");
        }
        other => panic!("expected InvalidParameter, got {other:?}"),
    }
    let err = RevolveOp::new(ccw_square_on_plus_x(), 0).unwrap_err();
    assert!(matches!(err, OpError::InvalidParameter(_)));
    let err = RevolveOp::new(ccw_square_on_plus_x(), 1).unwrap_err();
    assert!(matches!(err, OpError::InvalidParameter(_)));
}

#[test]
fn revolve_new_accepts_segments_3() {
    let op = RevolveOp::new(ccw_square_on_plus_x(), 3).expect("min valid");
    assert_eq!(op.segments(), 3);
}

#[test]
fn revolve_new_defaults_to_full_revolution() {
    let op = RevolveOp::new(ccw_square_on_plus_x(), 4).expect("op");
    assert!(op.is_full_revolution());
    assert!((op.angle() - 2.0 * PI).abs() < 1e-6);
}

// -- evaluate rejection paths --------------------------------------------

#[test]
fn revolve_evaluate_rejects_inputs_for_arity_0() {
    let op = RevolveOp::new(ccw_square_on_plus_x(), 4).expect("op");
    let bogus = Tessellation::new(vec![[0.0_f32, 0.0, 0.0]], vec![]).expect("ok");
    let err = op.evaluate(&[&bogus]).unwrap_err();
    assert!(matches!(
        err,
        OpError::WrongArity {
            expected: 0,
            got: 1
        }
    ));
}

#[test]
fn revolve_evaluate_rejects_negative_x_in_profile() {
    // Square that crosses the Y-axis (x in [-0.5, 0.5]).
    let crossing = Polygon2D::new(vec![[-0.5, 0.0], [0.5, 0.0], [0.5, 1.0], [-0.5, 1.0]])
        .expect("crossing square");
    let op = RevolveOp::new(crossing, 4).expect("op");
    let err = op.evaluate(&[]).unwrap_err();
    match err {
        OpError::InvalidParameter(msg) => {
            assert!(msg.contains("x >= 0"), "msg = {msg}");
        }
        other => panic!("expected InvalidParameter, got {other:?}"),
    }
}

#[test]
fn revolve_post_construction_segments_corruption_rejected() {
    // Post-construction mutation: `segments` is pub.
    let mut op = RevolveOp::new(ccw_square_on_plus_x(), 4).expect("op");
    op.segments = 2;
    let err = op.evaluate(&[]).unwrap_err();
    assert!(matches!(err, OpError::InvalidParameter(_)));
}

#[test]
fn revolve_post_construction_angle_corruption_rejected() {
    // `angle` is also a pub field — defensively re-checked.
    let mut op = RevolveOp::new(ccw_square_on_plus_x(), 4).expect("op");
    op.angle = -1.0;
    let err = op.evaluate(&[]).unwrap_err();
    match err {
        OpError::InvalidParameter(msg) => {
            assert!(msg.contains("angle"), "msg = {msg}");
        }
        other => panic!("expected InvalidParameter, got {other:?}"),
    }

    let mut op2 = RevolveOp::new(ccw_square_on_plus_x(), 4).expect("op");
    op2.angle = f32::NAN;
    let err = op2.evaluate(&[]).unwrap_err();
    assert!(matches!(err, OpError::InvalidParameter(_)));
}

// -- vertex / triangle counts (full revolution) -------------------------

#[test]
fn revolve_triangle_profile_4_segments() {
    // n=3 × 4 segments → 12 verts, 24 tris, 72 indices.
    let op = RevolveOp::new(ccw_right_triangle_on_plus_x(), 4).expect("op");
    let mesh = op.evaluate(&[]).expect("evaluate");
    assert_eq!(mesh.vertex_count(), 12);
    assert_eq!(mesh.triangle_count(), 24);
    assert_eq!(mesh.indices.len(), 72);
}

#[test]
fn revolve_square_profile_6_segments() {
    // n=4 × 6 segments → 24 verts, 48 tris, 144 indices.
    let op = RevolveOp::new(ccw_square_on_plus_x(), 6).expect("op");
    let mesh = op.evaluate(&[]).expect("evaluate");
    assert_eq!(mesh.vertex_count(), 24);
    assert_eq!(mesh.triangle_count(), 48);
    assert_eq!(mesh.indices.len(), 144);
}

#[test]
fn revolve_concave_profile_accepted() {
    // L-shape (concave) — full revolution doesn't fan-triangulate caps,
    // so concavity is allowed. Verify a non-empty mesh comes back with
    // expected counts: n=6 × 5 = 30 verts, 2*6*5 = 60 tris.
    let op = RevolveOp::new(ccw_concave_l_on_plus_x(), 5).expect("op");
    let mesh = op.evaluate(&[]).expect("evaluate concave");
    assert_eq!(mesh.vertex_count(), 30);
    assert_eq!(mesh.triangle_count(), 60);
    assert_eq!(mesh.indices.len(), 180);
}

#[test]
fn revolve_axis_touching_profile_yields_degenerate_triangles_but_valid_mesh() {
    // Profile touches the axis at (0,0) and (0,1). Revolved 4 segments:
    // n=3 × 4 = 12 verts. Some triangles are degenerate (zero area at
    // the axis-collapsed vertices) but Tessellation::new only validates
    // index bounds + multiple-of-3, so the mesh constructs fine.
    let op = RevolveOp::new(ccw_axis_touching_triangle(), 4).expect("op");
    let mesh = op.evaluate(&[]).expect("evaluate");
    assert_eq!(mesh.vertex_count(), 12);
    assert_eq!(mesh.triangle_count(), 24);
    assert_eq!(mesh.indices.len(), 72);

    // Spot-check axis vertices: profile points (0,0) and (0,1) collapse
    // to the same 3D position across all rings.
    // Profile is `ordered` after winding correction. signed_area of
    // [(0,0),(1,0),(0,1)] = 0.5 → CCW already, no reversal.
    // ring s=0 vertices: (0,0,0), (1,0,0), (0,1,0).
    // ring s=1 vertices: (0,0,0), (cos π/2, 0, sin π/2)=(0,0,1), (0,1,0).
    // Axis-touching vertices (indices 0, 2 in each ring) all share x=0, z=0.
    for s in 0..4 {
        let axis0 = mesh.positions[s * 3];
        let axis2 = mesh.positions[s * 3 + 2];
        assert!(axis0[0].abs() < 1e-5 && axis0[2].abs() < 1e-5);
        assert!(axis2[0].abs() < 1e-5 && axis2[2].abs() < 1e-5);
    }
}

#[test]
fn revolve_cw_profile_handled() {
    // Same square footprint, CW order — algorithm reverses internally.
    let op = RevolveOp::new(cw_square_on_plus_x(), 6).expect("op");
    let mesh = op.evaluate(&[]).expect("evaluate cw");
    assert_eq!(mesh.vertex_count(), 24);
    assert_eq!(mesh.triangle_count(), 48);
    assert_eq!(mesh.indices.len(), 144);
}

// -- structural_hash (full + parameter sensitivity) ---------------------

#[test]
fn revolve_structural_hash_deterministic() {
    let a = RevolveOp::new(ccw_square_on_plus_x(), 8).expect("a");
    let b = RevolveOp::new(ccw_square_on_plus_x(), 8).expect("b");
    assert_eq!(a.structural_hash(), b.structural_hash());
}

#[test]
fn revolve_structural_hash_changes_with_segments() {
    let a = RevolveOp::new(ccw_square_on_plus_x(), 4).expect("a");
    let b = RevolveOp::new(ccw_square_on_plus_x(), 8).expect("b");
    assert_ne!(a.structural_hash(), b.structural_hash());
}

#[test]
fn revolve_structural_hash_changes_with_profile_perturbation() {
    let a = RevolveOp::new(ccw_square_on_plus_x(), 6).expect("a");
    let perturbed = Polygon2D::new(vec![
        [1.0, 0.0],
        [2.0 + 1.0e-3, 0.0],
        [2.0, 1.0],
        [1.0, 1.0],
    ])
    .expect("perturbed");
    let b = RevolveOp::new(perturbed, 6).expect("b");
    assert_ne!(a.structural_hash(), b.structural_hash());
}

// -- geometric correctness (full revolution) -----------------------------

#[test]
fn revolve_at_segment_zero_first_ring_lies_in_xy_plane() {
    // At s=0, theta=0 so cos=1, sin=0. Ring-0 vertices should be
    // (x, y, 0) — i.e. z = 0 with x, y matching the (winding-corrected)
    // profile coords.
    let op = RevolveOp::new(ccw_square_on_plus_x(), 8).expect("op");
    let mesh = op.evaluate(&[]).expect("evaluate");
    let ordered = ccw_square_on_plus_x().points().to_vec(); // already CCW
    for (i, [x, y]) in ordered.iter().enumerate() {
        let v = mesh.positions[i];
        assert!((v[0] - x).abs() < 1.0e-5, "x mismatch at {i}: {v:?}");
        assert!((v[1] - y).abs() < 1.0e-5, "y mismatch at {i}: {v:?}");
        assert!(v[2].abs() < 1.0e-5, "z != 0 at ring 0 idx {i}: {v:?}");
    }
}

#[test]
fn revolve_op_kind_is_revolve() {
    let op = RevolveOp::new(ccw_square_on_plus_x(), 4).expect("op");
    assert_eq!(op.op_kind(), OpKind::Revolve);
    assert_eq!(op.arity(), 0);
}

#[test]
fn revolve_full_2pi_closes_seamlessly() {
    // Last segment must wrap back to ring 0 (closure check). Triangles
    // emitted across the s=segments-1 → s=0 seam reference indices in
    // ring 0 directly. Every vertex should lie on a circle of correct
    // radius.
    let op = RevolveOp::new(ccw_square_on_plus_x(), 12).expect("op");
    let mesh = op.evaluate(&[]).expect("evaluate");
    // Each of the 4 profile points has its own radius (1, 2, 2, 1); the
    // 12 rings produce 12 vertices on each circle. Verify every vertex
    // is on circle of radius 1 or 2.
    for [x, y, z] in &mesh.positions {
        let r2 = x * x + z * z;
        let near_1 = (r2 - 1.0).abs() < 1.0e-4;
        let near_4 = (r2 - 4.0).abs() < 1.0e-4;
        assert!(near_1 || near_4, "unexpected r²={r2} at vertex {x},{y},{z}");
        assert!(*y >= -1.0e-5 && *y <= 1.0 + 1.0e-5);
    }
}

#[test]
fn revolve_first_quad_has_outward_radial_normal() {
    // Triangle profile [(1,0),(2,0),(1,1)] × 4 segs. The first side-wall
    // triangle sits on the y=0 bottom rim — its outward normal must
    // point in -Y (away from the +Y interior of the closed prism).
    let op = RevolveOp::new(ccw_right_triangle_on_plus_x(), 4).expect("op");
    let mesh = op.evaluate(&[]).expect("evaluate");
    // First triangle indices (a, b, c) for s=0, p=0:
    let i0 = mesh.indices[0] as usize;
    let i1 = mesh.indices[1] as usize;
    let i2 = mesh.indices[2] as usize;
    let a = mesh.positions[i0];
    let b = mesh.positions[i1];
    let c = mesh.positions[i2];
    let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let n = [
        e1[1] * e2[2] - e1[2] * e2[1],
        e1[2] * e2[0] - e1[0] * e2[2],
        e1[0] * e2[1] - e1[1] * e2[0],
    ];
    // For this triangle on the y=0 rim, expect a strongly -Y component
    // in the normal (face points downward = away from the +Y interior).
    assert!(
        n[1] < 0.0,
        "expected -Y outward normal on bottom-rim quad, got {n:?}"
    );
}

// -----------------------------------------------------------------------
// Partial-revolution tests (D-Partial-Revolve)
// -----------------------------------------------------------------------

#[test]
fn revolve_partial_rejects_zero_angle() {
    let err = RevolveOp::partial(ccw_square_on_plus_x(), 4, 0.0).unwrap_err();
    match err {
        OpError::InvalidParameter(msg) => {
            assert!(msg.contains("angle"), "msg = {msg}");
        }
        other => panic!("expected InvalidParameter, got {other:?}"),
    }
}

#[test]
fn revolve_partial_rejects_negative_angle() {
    let err = RevolveOp::partial(ccw_square_on_plus_x(), 4, -1.0).unwrap_err();
    match err {
        OpError::InvalidParameter(msg) => {
            assert!(msg.contains("angle"), "msg = {msg}");
        }
        other => panic!("expected InvalidParameter, got {other:?}"),
    }
}

#[test]
fn revolve_partial_rejects_non_finite_angle() {
    let err = RevolveOp::partial(ccw_square_on_plus_x(), 4, f32::NAN).unwrap_err();
    match err {
        OpError::InvalidParameter(msg) => {
            assert!(msg.contains("angle"), "msg = {msg}");
        }
        other => panic!("expected InvalidParameter, got {other:?}"),
    }
    let err = RevolveOp::partial(ccw_square_on_plus_x(), 4, f32::INFINITY).unwrap_err();
    assert!(matches!(err, OpError::InvalidParameter(_)));
    let err = RevolveOp::partial(ccw_square_on_plus_x(), 4, f32::NEG_INFINITY).unwrap_err();
    assert!(matches!(err, OpError::InvalidParameter(_)));
}

#[test]
fn revolve_partial_rejects_angle_exceeding_2pi() {
    // Just above 2π+1e-5 must reject — anything in [2π, 2π+1e-5] is
    // tolerated by the constructor and clamped to exactly 2π.
    let err = RevolveOp::partial(ccw_square_on_plus_x(), 4, 2.0 * PI + 0.01).unwrap_err();
    match err {
        OpError::InvalidParameter(msg) => {
            assert!(msg.contains("angle"), "msg = {msg}");
        }
        other => panic!("expected InvalidParameter, got {other:?}"),
    }
}

#[test]
fn revolve_partial_clamps_near_2pi_to_full_revolution() {
    // Tiny epsilon above 2π → constructor accepts it and clamps to exactly
    // 2π. is_full_revolution() returns true.
    let op =
        RevolveOp::partial(ccw_square_on_plus_x(), 4, 2.0 * PI + 1.0e-7).expect("clamps to 2π");
    assert!(op.is_full_revolution());
    assert!((op.angle() - 2.0 * PI).abs() < 1e-6);
}

#[test]
fn revolve_full_2pi_via_partial_constructor_matches_new() {
    // partial(p, segs, 2π) and new(p, segs) must produce byte-identical
    // tessellations. We compare via `to_bits` to satisfy clippy::float_cmp
    // — exact bitwise equality is what we genuinely want here (both paths
    // run the same algorithm with the same inputs, so the outputs MUST
    // match bit-for-bit).
    let a = RevolveOp::partial(ccw_square_on_plus_x(), 6, 2.0 * PI).expect("partial");
    let b = RevolveOp::new(ccw_square_on_plus_x(), 6).expect("new");
    let mesh_a = a.evaluate(&[]).expect("eval a");
    let mesh_b = b.evaluate(&[]).expect("eval b");
    assert_eq!(mesh_a.positions.len(), mesh_b.positions.len());
    assert_eq!(mesh_a.indices, mesh_b.indices);
    for (va, vb) in mesh_a.positions.iter().zip(mesh_b.positions.iter()) {
        for (a_i, b_i) in va.iter().zip(vb.iter()) {
            assert_eq!(
                a_i.to_bits(),
                b_i.to_bits(),
                "vertex bit-mismatch: {va:?} vs {vb:?}"
            );
        }
    }
}

#[test]
fn revolve_partial_pi_triangle_profile_yields_correct_counts() {
    // Triangle profile (n=3) × 4 segments × angle=π:
    //   vertex count: 3 * (4+1) = 15
    //   side tris: 2*3*4 = 24
    //   cap tris: 2*(3-2) = 2
    //   total tris: 26
    //   indices: 78
    let op = RevolveOp::partial(ccw_right_triangle_on_plus_x(), 4, PI).expect("op");
    let mesh = op.evaluate(&[]).expect("evaluate");
    assert_eq!(mesh.vertex_count(), 15);
    assert_eq!(mesh.triangle_count(), 26);
    assert_eq!(mesh.indices.len(), 78);
}

#[test]
fn revolve_partial_half_pi_square_profile_yields_correct_counts() {
    // Square (n=4) × 8 segments × angle=π/2:
    //   vertex count: 4 * (8+1) = 36
    //   side tris: 2*4*8 = 64
    //   cap tris: 2*(4-2) = 4
    //   total tris: 68
    //   indices: 204
    let op = RevolveOp::partial(ccw_square_on_plus_x(), 8, PI / 2.0).expect("op");
    let mesh = op.evaluate(&[]).expect("evaluate");
    assert_eq!(mesh.vertex_count(), 36);
    assert_eq!(mesh.triangle_count(), 68);
    assert_eq!(mesh.indices.len(), 204);
}

#[test]
fn revolve_partial_concave_profile_rejected() {
    // L-shape (concave) is disallowed for partial revolution because the
    // caps would need a non-fan triangulation.
    let op = RevolveOp::partial(ccw_concave_l_on_plus_x(), 4, PI).expect("op");
    let err = op.evaluate(&[]).unwrap_err();
    match err {
        OpError::InvalidParameter(msg) => {
            assert!(msg.contains("convex"), "msg = {msg}");
        }
        other => panic!("expected InvalidParameter, got {other:?}"),
    }
}

#[test]
fn revolve_full_concave_profile_still_accepted() {
    // Regression check: full revolution should still allow concave
    // profiles (no caps emitted).
    let op = RevolveOp::new(ccw_concave_l_on_plus_x(), 4).expect("op");
    let mesh = op.evaluate(&[]).expect("evaluate concave full");
    assert_eq!(mesh.vertex_count(), 24); // n=6 × 4 = 24
    assert_eq!(mesh.triangle_count(), 48); // 2*6*4 = 48
}

#[test]
fn revolve_partial_start_cap_lies_in_xy_plane() {
    // For angle=π/2, ring 0 vertices have z=0 and (x,y) match (winding-
    // corrected) profile coords.
    let op = RevolveOp::partial(ccw_square_on_plus_x(), 8, PI / 2.0).expect("op");
    let mesh = op.evaluate(&[]).expect("evaluate");
    let ordered = ccw_square_on_plus_x().points().to_vec(); // already CCW
    for (i, [x, y]) in ordered.iter().enumerate() {
        let v = mesh.positions[i];
        assert!((v[0] - x).abs() < 1e-5, "x mismatch at {i}: {v:?}");
        assert!((v[1] - y).abs() < 1e-5, "y mismatch at {i}: {v:?}");
        assert!(v[2].abs() < 1e-5, "z != 0 at ring 0 idx {i}: {v:?}");
    }
}

#[test]
fn revolve_partial_end_cap_at_angle_pi_lies_in_minus_x_plane() {
    // For angle=π, the end ring (s=segments) has cos(π)=-1, sin(π)=0, so
    // every end-ring vertex (x', y, z') satisfies x' = -x_profile,
    // y = y_profile, z' ≈ 0.
    let segments: u32 = 6;
    let op = RevolveOp::partial(ccw_square_on_plus_x(), segments, PI).expect("op");
    let mesh = op.evaluate(&[]).expect("evaluate");
    let n = ccw_square_on_plus_x().len();
    let ordered = ccw_square_on_plus_x().points().to_vec();
    let end_base = (segments as usize) * n;
    for (i, [x, y]) in ordered.iter().enumerate() {
        let v = mesh.positions[end_base + i];
        assert!(
            (v[0] + x).abs() < 1e-5,
            "x' should equal -x_profile at end ring idx {i}: v={v:?} expected x'≈{}",
            -x
        );
        assert!(
            (v[1] - y).abs() < 1e-5,
            "y mismatch at end ring idx {i}: {v:?}"
        );
        assert!(v[2].abs() < 1e-4, "z should be ≈ 0 at θ=π, idx {i}: {v:?}");
    }
}

#[test]
fn revolve_partial_structural_hash_changes_with_angle() {
    let a = RevolveOp::partial(ccw_square_on_plus_x(), 6, PI / 2.0).expect("a");
    let b = RevolveOp::partial(ccw_square_on_plus_x(), 6, PI).expect("b");
    assert_ne!(a.structural_hash(), b.structural_hash());
}

#[test]
fn revolve_partial_structural_hash_deterministic_across_constructions() {
    // Same params via partial() twice → identical hash.
    let a = RevolveOp::partial(ccw_square_on_plus_x(), 6, PI / 2.0).expect("a");
    let b = RevolveOp::partial(ccw_square_on_plus_x(), 6, PI / 2.0).expect("b");
    assert_eq!(a.structural_hash(), b.structural_hash());
    // And new() and partial(2π) also identical-hash since `clamped`
    // behavior gives both exactly 2π.
    let c = RevolveOp::new(ccw_square_on_plus_x(), 6).expect("c");
    let d = RevolveOp::partial(ccw_square_on_plus_x(), 6, 2.0 * PI).expect("d");
    assert_eq!(c.structural_hash(), d.structural_hash());
}

#[test]
fn revolve_partial_cw_profile_handled() {
    // CW-ordered square + partial revolution. Algorithm reverses
    // internally; vertex/tri counts must match the CCW case.
    let op = RevolveOp::partial(cw_square_on_plus_x(), 8, PI / 2.0).expect("op");
    let mesh = op.evaluate(&[]).expect("evaluate cw partial");
    assert_eq!(mesh.vertex_count(), 36);
    assert_eq!(mesh.triangle_count(), 68);
    assert_eq!(mesh.indices.len(), 204);
}

#[test]
fn revolve_partial_start_cap_normal_points_minus_z() {
    // Start cap at θ=0 should have outward normal -Z (away from sweep).
    let op = RevolveOp::partial(ccw_right_triangle_on_plus_x(), 4, PI / 2.0).expect("op");
    let mesh = op.evaluate(&[]).expect("evaluate");
    // Side walls (2*3*4=24 tris=72 indices) precede the start-cap fan.
    let cap_start = 2 * 3 * 4 * 3;
    let i0 = mesh.indices[cap_start] as usize;
    let i1 = mesh.indices[cap_start + 1] as usize;
    let i2 = mesh.indices[cap_start + 2] as usize;
    let a = mesh.positions[i0];
    let b = mesh.positions[i1];
    let c = mesh.positions[i2];
    let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let n = [
        e1[1] * e2[2] - e1[2] * e2[1],
        e1[2] * e2[0] - e1[0] * e2[2],
        e1[0] * e2[1] - e1[1] * e2[0],
    ];
    assert!(n[2] < 0.0, "start-cap normal should be -Z, got {n:?}");
}

/// Arity 0 + unlabeled output ⇒ default returns `false` on empty inputs.
#[test]
fn revolve_output_is_labeled_returns_false() {
    assert!(!RevolveOp::new(ccw_square_on_plus_x(), 6)
        .expect("op")
        .output_is_labeled(&[]));
}

// ---------------------------------------------------------------------------
// BRepProvider impl tests (sub-7.2-γ)
// ---------------------------------------------------------------------------

/// Full revolution (n=4 profile, no caps) returns exactly `n` pairs.
#[test]
fn brep_provider_full_revolution_returns_n_pairs() {
    let owner = BRepOwnerId::from_bytes([0x42u8; 16]);
    let op = RevolveOp::new(ccw_square_on_plus_x(), 8).expect("op");
    let pairs = op.brep_face_ids(owner);
    assert_eq!(
        pairs.len(),
        4,
        "Full revolution (n=4) should yield exactly n=4 pairs (no caps)"
    );
}

/// Partial revolution (n=4 profile) returns exactly `n + 2` pairs (sides +
/// start cap + end cap).
#[test]
fn brep_provider_partial_revolution_returns_n_plus_2_pairs() {
    let owner = BRepOwnerId::from_bytes([0x42u8; 16]);
    let op = RevolveOp::partial(ccw_square_on_plus_x(), 8, PI / 2.0).expect("op");
    let pairs = op.brep_face_ids(owner);
    assert_eq!(
        pairs.len(),
        6,
        "Partial revolution (n=4) should yield n+2=6 pairs (sides + 2 caps)"
    );
}

/// `TopologyFaceId(0..n-1)` correspond to Side faces, `TopologyFaceId(n)` to
/// StartCap, `TopologyFaceId(n + 1)` to EndCap (Partial only). Pins the
/// canonical emission order byte-for-byte.
#[test]
fn brep_provider_topology_face_ids_are_canonical_emission_order() {
    let owner = BRepOwnerId::from_bytes([0x42u8; 16]);
    let segments: u32 = 8;
    let op = RevolveOp::partial(ccw_square_on_plus_x(), segments, PI / 2.0).expect("op");
    let pairs = op.brep_face_ids(owner);

    // Sides at indices 0..4 with TopologyFaceId(0..4) and side_index 0..4.
    for i in 0u32..4 {
        let idx = i as usize;
        assert_eq!(pairs[idx].0 .0, u64::from(i));
        assert_eq!(
            pairs[idx].1,
            BRepFaceId::for_revolve_face(
                owner,
                RevolveFaceTag::Side {
                    side_index: i,
                    profile_count: 4,
                    segment_count: segments,
                    mode: RevolveMode::Partial,
                },
            ),
            "side at index {idx} (side_index {i}) does not match canonical mapping"
        );
    }

    // Start cap at TopologyFaceId(n=4).
    assert_eq!(pairs[4].0 .0, 4);
    assert_eq!(
        pairs[4].1,
        BRepFaceId::for_revolve_face(owner, RevolveFaceTag::StartCap { profile_count: 4 })
    );

    // End cap at TopologyFaceId(n + 1 = 5).
    assert_eq!(pairs[5].0 .0, 5);
    assert_eq!(
        pairs[5].1,
        BRepFaceId::for_revolve_face(owner, RevolveFaceTag::EndCap { profile_count: 4 })
    );
}

// ---------------------------------------------------------------------------
// BRepEdgeProvider impl tests (sub-7.2-ζ.γ)
// ---------------------------------------------------------------------------

/// Full revolution (n=4 profile) returns exactly `n` edges (one per
/// `Side(i) ∩ Side((i + 1) % n)` adjacency, no cap-perimeter edges).
/// Partial revolution (n=4 profile) returns exactly `3 * n` edges
/// (n axial seams + n start-cap-perimeter + n end-cap-perimeter).
#[test]
fn brep_edge_provider_returns_expected_edge_count() {
    let owner = BRepOwnerId::from_bytes([0x42u8; 16]);

    // Full mode: triangle (n=3) → 3 edges; square (n=4) → 4 edges.
    let full_tri = RevolveOp::new(ccw_right_triangle_on_plus_x(), 8).expect("full tri");
    assert_eq!(
        full_tri.brep_edge_ids(owner).len(),
        3,
        "full triangle n=3 → n=3 edges"
    );

    let full_sq = RevolveOp::new(ccw_square_on_plus_x(), 8).expect("full sq");
    assert_eq!(
        full_sq.brep_edge_ids(owner).len(),
        4,
        "full square n=4 → n=4 edges"
    );

    // Partial mode: triangle (n=3) → 9 edges; square (n=4) → 12 edges.
    let partial_tri =
        RevolveOp::partial(ccw_right_triangle_on_plus_x(), 8, PI / 2.0).expect("partial tri");
    assert_eq!(
        partial_tri.brep_edge_ids(owner).len(),
        9,
        "partial triangle n=3 → 3*3=9 edges"
    );

    let partial_sq = RevolveOp::partial(ccw_square_on_plus_x(), 8, PI / 2.0).expect("partial sq");
    assert_eq!(
        partial_sq.brep_edge_ids(owner).len(),
        12,
        "partial square n=4 → 3*4=12 edges"
    );
}

/// Same profile, n=4. Full mode yields 4 edges; Partial mode yields
/// 12 edges. Mode-driven topology change must surface in edge count.
#[test]
fn brep_edge_provider_full_and_partial_yield_different_counts() {
    let owner = BRepOwnerId::from_bytes([0xa1u8; 16]);
    let full = RevolveOp::new(ccw_square_on_plus_x(), 8).expect("full");
    let partial = RevolveOp::partial(ccw_square_on_plus_x(), 8, PI).expect("partial");

    assert_eq!(
        full.brep_edge_ids(owner).len(),
        4,
        "Full mode (n=4) = n edges"
    );
    assert_eq!(
        partial.brep_edge_ids(owner).len(),
        12,
        "Partial mode (n=4) = 3n edges"
    );
}

/// Every `BRepEdgeId` minted by `RevolveOp` uses `local_ordinal = 0`.
/// Verified by reconstructing the same edge directly via
/// `BRepEdgeId::for_face_pair(.., .., 0)` and checking byte equality.
#[test]
fn brep_edge_ids_use_local_ordinal_zero() {
    let owner = BRepOwnerId::from_bytes([0x99u8; 16]);

    // Partial mode covers all three edge categories (Side-Side, StartCap-Side,
    // EndCap-Side). Verify two representative edges across categories.
    let op = RevolveOp::partial(ccw_square_on_plus_x(), 8, PI / 2.0).expect("op");
    let face_ids: Vec<BRepFaceId> = op
        .brep_face_ids(owner)
        .into_iter()
        .map(|(_, id)| id)
        .collect();
    let edges = op.brep_edge_ids(owner);

    // Edge 0: Side(0) ∩ Side(1) — face_ids[0] ∩ face_ids[1].
    assert_eq!(
        edges[0],
        BRepEdgeId::for_face_pair(face_ids[0], face_ids[1], 0),
        "edge 0 must be derived with local_ordinal = 0"
    );
    // Edge 4 (= n): StartCap ∩ Side(0) — face_ids[4] ∩ face_ids[0].
    assert_eq!(
        edges[4],
        BRepEdgeId::for_face_pair(face_ids[4], face_ids[0], 0),
        "edge 4 must be derived with local_ordinal = 0"
    );
}

/// The edges for a partial revolution align with the canonical
/// adjacency table documented in the `impl BRepEdgeProvider for
/// RevolveOp` block. We verify three representative edges across
/// the three categories (Side-Side, StartCap-Side, EndCap-Side).
#[test]
fn brep_edge_ids_align_with_canonical_adjacency_table() {
    let owner = BRepOwnerId::from_bytes([0x42u8; 16]);
    let op = RevolveOp::partial(ccw_square_on_plus_x(), 8, PI / 2.0).expect("op");
    let face_ids: Vec<BRepFaceId> = op
        .brep_face_ids(owner)
        .into_iter()
        .map(|(_, id)| id)
        .collect();
    let edges = op.brep_edge_ids(owner);

    // Side-Side, edge 0: Side(0) ∩ Side(1).
    assert_eq!(
        edges[0],
        BRepEdgeId::for_face_pair(face_ids[0], face_ids[1], 0),
        "edge 0 must be Side(0) ∩ Side(1)"
    );
    // StartCap-Side, edge 4 (= n): StartCap ∩ Side(0).
    assert_eq!(
        edges[4],
        BRepEdgeId::for_face_pair(face_ids[4], face_ids[0], 0),
        "edge 4 must be StartCap ∩ Side(0)"
    );
    // EndCap-Side, edge 8 (= 2n): EndCap ∩ Side(0).
    assert_eq!(
        edges[8],
        BRepEdgeId::for_face_pair(face_ids[5], face_ids[0], 0),
        "edge 8 must be EndCap ∩ Side(0)"
    );
}
