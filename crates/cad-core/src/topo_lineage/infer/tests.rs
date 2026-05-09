//! Unit tests for [`crate::topo_lineage::infer`].
//!
//! Sub-module of [`crate::topo_lineage::infer`]; see the parent module's
//! `//!` docs for the design rationale.

use super::{infer_lineage, label_by_plane};
use crate::operators::{CuboidOp, ExtrudeOp, Operator, Polygon2D};
use crate::tessellation::{Tessellation, TopologyFaceId};
use crate::topo_lineage::types::{LineageError, TopologyEvolution};

/// Helper for the labeled-path tests: build a hand-rolled labeled
/// [`Tessellation`] from positions / indices / labels.
fn labeled_mesh(
    positions: Vec<[f32; 3]>,
    indices: Vec<u32>,
    labels: Vec<TopologyFaceId>,
) -> Tessellation {
    Tessellation::with_labels(positions, indices, labels).expect("test labeled mesh ctor")
}

// --- label_by_plane ---------------------------------------------------

#[test]
fn label_by_plane_unit_cube_yields_6_face_groups() {
    // CuboidOp::default() is 1×1×1 origin-centered → 12 triangles in 6
    // plane groups (the cube's 6 axis-aligned faces).
    let cube = CuboidOp::default();
    let tess = cube.evaluate(&[]).expect("cube tess");
    assert_eq!(tess.triangle_count(), 12);
    let labeled = label_by_plane(&tess, 0).expect("label cube");
    assert!(labeled.is_labeled(), "label_by_plane returns labeled tess");
    assert_eq!(
        labeled.face_count(),
        Some(6),
        "cube should have 6 plane groups"
    );
    assert_eq!(labeled.triangle_count(), 12);
}

#[test]
fn label_by_plane_extrude_triangle_yields_5_face_groups() {
    // Triangle profile extruded by 1.0 → 1 bottom cap + 1 top cap + 3
    // side walls = 5 plane groups. Triangle profile produces 1
    // triangle/cap + 2 triangles/side wall (2 per quad) = 1 + 1 + 2*3 =
    // 8 triangles total.
    let triangle =
        Polygon2D::new(vec![[0.0, 0.0], [1.0, 0.0], [0.5, 1.0]]).expect("triangle profile");
    let extrude = ExtrudeOp::new(triangle, 1.0).expect("extrude op");
    let tess = extrude.evaluate(&[]).expect("extrude tess");
    assert_eq!(tess.triangle_count(), 8, "triangle prism = 8 triangles");
    let labeled = label_by_plane(&tess, 0).expect("label extrude");
    assert_eq!(
        labeled.face_count(),
        Some(5),
        "triangle prism should have 5 plane groups (2 caps + 3 walls)"
    );
}

// --- infer_lineage error path -----------------------------------------

#[test]
fn infer_lineage_with_unlabeled_input_returns_invalid_input_error() {
    // Caller passes unlabeled input → must return InvalidInput.
    //
    // Post-D-projection-α (2026-05-09): `CuboidOp::evaluate` now emits
    // labeled output, so we construct an unlabeled fixture directly via
    // `Tessellation::new` over the cube's buffers to exercise the
    // unlabeled-input path.
    let cube = CuboidOp::default();
    let labeled_tess = cube.evaluate(&[]).expect("cube tess");
    let tess = Tessellation::new(labeled_tess.positions.clone(), labeled_tess.indices.clone())
        .expect("rebuild unlabeled");
    // tess is unlabeled (constructed via Tessellation::new).
    assert!(!tess.is_labeled());
    let err = infer_lineage(&tess, &tess, 100).unwrap_err();
    match err {
        LineageError::InvalidInput(msg) => {
            assert!(
                msg.contains("requires labeled input"),
                "expected 'requires labeled input' message; got: {msg}"
            );
        }
        other => panic!("expected InvalidInput, got {other:?}"),
    }
}

// --- infer_lineage with unlabeled output (plane-heuristic path) -------

#[test]
fn infer_lineage_with_labeled_input_unlabeled_output_uses_plane_heuristic() {
    // input == output (same cube) → identity preserves all 6 plane
    // groups. Output is unlabeled, so the plane heuristic kicks in.
    //
    // Post-D-projection-α (2026-05-09): `CuboidOp::evaluate` now emits
    // labeled output, so we strip labels via a `Tessellation::new`
    // round-trip to keep the plane-heuristic path exercised here.
    let cube = CuboidOp::default();
    let labeled_tess = cube.evaluate(&[]).expect("cube tess");
    let tess = Tessellation::new(labeled_tess.positions.clone(), labeled_tess.indices.clone())
        .expect("rebuild unlabeled");
    let labeled_input = label_by_plane(&tess, 0).expect("label cube");
    // tess is unlabeled (constructed via Tessellation::new).
    assert!(!tess.is_labeled());

    let (labeled_output, lineage) =
        infer_lineage(&labeled_input, &tess, 100).expect("identity lineage");
    assert!(
        labeled_output.is_labeled(),
        "infer_lineage returns labeled output"
    );
    assert_eq!(
        labeled_output.face_count(),
        Some(6),
        "output has same 6 face groups"
    );
    // 6 input faces ⇒ 6 Preserved edges; 0 Reinterpreted (no new
    // planes).
    let preserved_count = lineage
        .edges_by_evolution(TopologyEvolution::Preserved)
        .count();
    assert_eq!(
        preserved_count, 6,
        "expected 6 Preserved edges, got {preserved_count}"
    );
    let reint_count = lineage
        .edges_by_evolution(TopologyEvolution::Reinterpreted)
        .count();
    assert_eq!(
        reint_count, 0,
        "expected 0 Reinterpreted edges for identity, got {reint_count}"
    );
    // All Preserved edges should have confidence 1.0.
    for edge in lineage.edges_by_evolution(TopologyEvolution::Preserved) {
        assert!(
            (edge.confidence - 1.0).abs() < 1e-6,
            "Preserved confidence should be 1.0, got {}",
            edge.confidence
        );
    }
}

#[test]
fn infer_lineage_deletion_records_deleted_edge() {
    // Input: cube → 6 plane groups.
    // Output: synthesize a "smaller" mesh with one input plane removed
    // (drop the +Z face's two triangles).
    let cube = CuboidOp::default();
    let tess = cube.evaluate(&[]).expect("cube tess");
    let labeled_input = label_by_plane(&tess, 0).expect("label cube");

    // Find the +Z plane's face_id by inspecting input.face_labels.
    // We'll drop all triangles whose plane has +Z normal (z == +0.5
    // offset) — the easiest way is to just check vertex z coords on
    // each triangle: if all three z-coords are +0.5 we're on the +Z
    // face. Then build a mesh from the remaining triangles only.
    let mut shrunk_indices = Vec::new();
    for tri_idx in 0..tess.triangle_count() {
        let i0 = tess.indices[tri_idx * 3] as usize;
        let i1 = tess.indices[tri_idx * 3 + 1] as usize;
        let i2 = tess.indices[tri_idx * 3 + 2] as usize;
        let z0 = tess.positions[i0][2];
        let z1 = tess.positions[i1][2];
        let z2 = tess.positions[i2][2];
        let on_plus_z =
            (z0 - 0.5).abs() < 1e-5 && (z1 - 0.5).abs() < 1e-5 && (z2 - 0.5).abs() < 1e-5;
        if !on_plus_z {
            shrunk_indices.push(tess.indices[tri_idx * 3]);
            shrunk_indices.push(tess.indices[tri_idx * 3 + 1]);
            shrunk_indices.push(tess.indices[tri_idx * 3 + 2]);
        }
    }
    let shrunk = Tessellation::new(tess.positions.clone(), shrunk_indices).expect("shrunk tess");

    let (_labeled_output, lineage) = infer_lineage(&labeled_input, &shrunk, 100).expect("lineage");
    let deleted_count = lineage
        .edges_by_evolution(TopologyEvolution::Deleted)
        .count();
    assert_eq!(
        deleted_count, 1,
        "expected exactly 1 Deleted edge (the +Z face), got {deleted_count}"
    );
    // The Deleted edge should have to=None and confidence=1.0.
    let deleted_edge = lineage
        .edges_by_evolution(TopologyEvolution::Deleted)
        .next()
        .unwrap();
    assert!(deleted_edge.from.is_some());
    assert!(deleted_edge.to.is_none());
    assert!((deleted_edge.confidence - 1.0).abs() < 1e-6);
}

#[test]
fn infer_lineage_reinterpretation_records_new_face() {
    // Input: cube → 6 plane groups.
    // Output: cube + an extra triangle on a NEW plane (e.g. the y=0
    // plane diagonal — a plane that does not match any cube face).
    let cube = CuboidOp::default();
    let tess = cube.evaluate(&[]).expect("cube tess");
    let labeled_input = label_by_plane(&tess, 0).expect("label cube");

    // Build an output that's the cube + a single extra triangle on a
    // tilted plane (no axis-aligned, so it cannot match any of the
    // cube's 6 faces).
    let mut positions = tess.positions.clone();
    let v_a = u32::try_from(positions.len()).expect("position count fits u32");
    positions.push([0.0, 0.0, 1.0]);
    positions.push([1.0, 0.0, 1.5]);
    positions.push([0.0, 1.0, 1.7]);
    let mut indices = tess.indices.clone();
    indices.push(v_a);
    indices.push(v_a + 1);
    indices.push(v_a + 2);
    let augmented = Tessellation::new(positions, indices).expect("augmented");

    let (_labeled_output, lineage) =
        infer_lineage(&labeled_input, &augmented, 100).expect("lineage");
    let reint_count = lineage
        .edges_by_evolution(TopologyEvolution::Reinterpreted)
        .count();
    assert_eq!(
        reint_count, 1,
        "expected exactly 1 Reinterpreted edge (the new tilted plane), got {reint_count}"
    );
    let reint_edge = lineage
        .edges_by_evolution(TopologyEvolution::Reinterpreted)
        .next()
        .unwrap();
    assert!(reint_edge.from.is_none());
    assert!(reint_edge.to.is_some());
    assert!((reint_edge.confidence - 1.0).abs() < 1e-6);
    // All 6 cube faces should still be Preserved.
    let preserved_count = lineage
        .edges_by_evolution(TopologyEvolution::Preserved)
        .count();
    assert_eq!(
        preserved_count, 6,
        "expected 6 Preserved edges (cube unchanged), got {preserved_count}"
    );
}

#[test]
fn infer_lineage_split_edge_when_input_has_more_triangles_on_plane() {
    // Build a minimal scenario where the input has 2 triangles on a
    // plane and the output has 1 triangle on the same plane — the
    // detector should fire `Split` (input tri count > output tri
    // count).
    let positions = vec![
        [0.0_f32, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
    ];
    // Input: 2 triangles forming a quad on z=0 plane.
    let in_tess = Tessellation::new(positions.clone(), vec![0_u32, 1, 2, 0, 2, 3]).expect("input");
    let labeled_in = label_by_plane(&in_tess, 0).expect("label input");
    // Output: 1 triangle on the same z=0 plane.
    let out_tess = Tessellation::new(positions, vec![0_u32, 1, 2]).expect("output");
    let (_labeled_out, lineage) = infer_lineage(&labeled_in, &out_tess, 100).expect("lineage");
    let split_count = lineage.edges_by_evolution(TopologyEvolution::Split).count();
    assert_eq!(
        split_count, 1,
        "expected 1 Split edge (input had more triangles), got {split_count}"
    );
}

// --- infer_lineage with labeled output (high-confidence path) ---------

#[test]
fn infer_lineage_with_labeled_input_labeled_output_uses_label_tracking() {
    // input == output (same labels everywhere) → all Preserved.
    let positions = vec![
        [0.0_f32, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
    ];
    let indices = vec![0_u32, 1, 2, 0, 2, 3];
    let labels = vec![TopologyFaceId(7), TopologyFaceId(7)];
    let mesh = labeled_mesh(positions, indices, labels);
    assert!(mesh.is_labeled());

    let (_out, lineage) = infer_lineage(&mesh, &mesh, 100).expect("identity labeled");
    let preserved = lineage
        .edges_by_evolution(TopologyEvolution::Preserved)
        .count();
    assert_eq!(preserved, 1, "expected 1 Preserved edge for face 7");
    // No other classifications.
    for ev in [
        TopologyEvolution::Split,
        TopologyEvolution::Merged,
        TopologyEvolution::Deleted,
        TopologyEvolution::Reinterpreted,
    ] {
        assert_eq!(
            lineage.edges_by_evolution(ev).count(),
            0,
            "expected 0 {ev:?} edges for identity labeled mesh"
        );
    }
    // Confidence on the labeled path is 1.0 across the board.
    for edge in &lineage.edges {
        assert!((edge.confidence - 1.0).abs() < 1e-6);
    }
}

#[test]
fn infer_lineage_labeled_difference_classifies_as_split_not_merged() {
    // Hand-construct an input + output where one input face has FEWER
    // output triangles than input — the labeled path must classify
    // this as Split (not Merged, which is the v0 plane-only false-
    // positive class). This is the **central correctness validation**
    // of the metadata-passthrough integration.
    let positions = vec![
        [0.0_f32, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
    ];
    // Input: 4 triangles on the same plane, all sharing label 0.
    let input = labeled_mesh(
        positions.clone(),
        vec![0, 1, 2, 0, 2, 3, 0, 1, 3, 1, 2, 3],
        vec![
            TopologyFaceId(0),
            TopologyFaceId(0),
            TopologyFaceId(0),
            TopologyFaceId(0),
        ],
    );
    // Output: only 2 triangles survived with label 0 (others consumed
    // by Difference).
    let output = labeled_mesh(
        positions,
        vec![0, 1, 2, 0, 2, 3],
        vec![TopologyFaceId(0), TopologyFaceId(0)],
    );

    let (_out, lineage) = infer_lineage(&input, &output, 100).expect("labeled diff lineage");
    let split_count = lineage.edges_by_evolution(TopologyEvolution::Split).count();
    let merged_count = lineage
        .edges_by_evolution(TopologyEvolution::Merged)
        .count();
    assert_eq!(
        split_count, 1,
        "labeled path must classify input>output triangle count as Split, got {split_count}"
    );
    assert_eq!(
        merged_count, 0,
        "labeled path must NOT classify input>output as Merged (v0 plane-only false positive); got {merged_count}"
    );
}

#[test]
fn infer_lineage_labeled_deletion_records_deleted() {
    // Input has labels {0, 1}; output has only {0}. Label 1 should
    // surface as a single Deleted edge.
    let positions = vec![
        [0.0_f32, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [1.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
    ];
    let input = labeled_mesh(
        positions.clone(),
        vec![0, 1, 2, 0, 2, 3],
        vec![TopologyFaceId(0), TopologyFaceId(1)],
    );
    let output = labeled_mesh(positions, vec![0, 1, 2], vec![TopologyFaceId(0)]);

    let (_out, lineage) = infer_lineage(&input, &output, 100).expect("labeled del lineage");
    let deleted = lineage
        .edges_by_evolution(TopologyEvolution::Deleted)
        .count();
    assert_eq!(
        deleted, 1,
        "expected exactly 1 Deleted edge for missing label 1; got {deleted}"
    );
    let deleted_edge = lineage
        .edges_by_evolution(TopologyEvolution::Deleted)
        .next()
        .unwrap();
    assert_eq!(deleted_edge.from, Some(TopologyFaceId(1)));
    assert!(deleted_edge.to.is_none());
    assert!((deleted_edge.confidence - 1.0).abs() < 1e-6);
    // Label 0 is Preserved (1 input tri, 1 output tri).
    let preserved = lineage
        .edges_by_evolution(TopologyEvolution::Preserved)
        .count();
    assert_eq!(preserved, 1);
}

#[test]
fn infer_lineage_labeled_reinterpretation_records_new_face() {
    // Output has a face label not in input → Reinterpreted edge.
    let positions = vec![[0.0_f32, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]];
    let input = labeled_mesh(positions.clone(), vec![0, 1, 2], vec![TopologyFaceId(0)]);
    let output = labeled_mesh(
        positions,
        vec![0, 1, 2, 0, 1, 2],
        vec![TopologyFaceId(0), TopologyFaceId(99)],
    );

    let (_out, lineage) = infer_lineage(&input, &output, 100).expect("labeled reint lineage");
    let reint = lineage
        .edges_by_evolution(TopologyEvolution::Reinterpreted)
        .count();
    assert_eq!(
        reint, 1,
        "expected exactly 1 Reinterpreted edge for new label 99; got {reint}"
    );
    let edge = lineage
        .edges_by_evolution(TopologyEvolution::Reinterpreted)
        .next()
        .unwrap();
    assert!(edge.from.is_none());
    assert_eq!(edge.to, Some(TopologyFaceId(99)));
    // Label 0 is Preserved (1 input tri, 1 output tri matching label 0).
    let preserved = lineage
        .edges_by_evolution(TopologyEvolution::Preserved)
        .count();
    assert_eq!(preserved, 1);
}

#[test]
fn infer_lineage_labeled_distinguishes_lhs_rhs_labels() {
    // Input has labels {0,1,2} (lhs face range) ∪ {10,11,12} (rhs
    // face range), simulating a Boolean op where lhs labels were 0..3
    // and rhs labels were 10..13. The output drops face 1 and face 11
    // but keeps the rest. Verify both sides surface independently in
    // the lineage:
    //  * lhs: 0, 2 → Preserved; 1 → Deleted
    //  * rhs: 10, 12 → Preserved; 11 → Deleted
    let positions = vec![[0.0_f32, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]];
    let lhs_indices = vec![
        0, 1, 2, // tri 0 → label 0
        0, 1, 2, // tri 1 → label 1
        0, 1, 2, // tri 2 → label 2
        0, 1, 2, // tri 3 → label 10
        0, 1, 2, // tri 4 → label 11
        0, 1, 2, // tri 5 → label 12
    ];
    let lhs_labels = vec![
        TopologyFaceId(0),
        TopologyFaceId(1),
        TopologyFaceId(2),
        TopologyFaceId(10),
        TopologyFaceId(11),
        TopologyFaceId(12),
    ];
    let input = labeled_mesh(positions.clone(), lhs_indices, lhs_labels);

    // Output: keep 0, 2, 10, 12 (drop 1 and 11).
    let out_indices = vec![0, 1, 2, 0, 1, 2, 0, 1, 2, 0, 1, 2];
    let out_labels = vec![
        TopologyFaceId(0),
        TopologyFaceId(2),
        TopologyFaceId(10),
        TopologyFaceId(12),
    ];
    let output = labeled_mesh(positions, out_indices, out_labels);

    let (_out, lineage) = infer_lineage(&input, &output, 100).expect("labeled lhs/rhs lineage");
    // 4 Preserved (0, 2, 10, 12); 2 Deleted (1, 11); 0 Reinterpreted.
    assert_eq!(
        lineage
            .edges_by_evolution(TopologyEvolution::Preserved)
            .count(),
        4,
        "expected 4 Preserved edges across lhs+rhs"
    );
    assert_eq!(
        lineage
            .edges_by_evolution(TopologyEvolution::Deleted)
            .count(),
        2
    );
    assert_eq!(
        lineage
            .edges_by_evolution(TopologyEvolution::Reinterpreted)
            .count(),
        0
    );

    // Verify edges from the lhs range (0..3) and rhs range (10..13)
    // both exist with deterministic order.
    let preserved_from: Vec<TopologyFaceId> = lineage
        .edges_by_evolution(TopologyEvolution::Preserved)
        .filter_map(|e| e.from)
        .collect();
    // BTreeSet iteration → ascending: 0, 2, 10, 12.
    assert_eq!(
        preserved_from,
        vec![
            TopologyFaceId(0),
            TopologyFaceId(2),
            TopologyFaceId(10),
            TopologyFaceId(12),
        ]
    );

    let deleted_from: Vec<TopologyFaceId> = lineage
        .edges_by_evolution(TopologyEvolution::Deleted)
        .filter_map(|e| e.from)
        .collect();
    assert_eq!(deleted_from, vec![TopologyFaceId(1), TopologyFaceId(11)]);
}

#[test]
fn infer_lineage_labeled_difference_degenerate_metadata_surfaces_as_reinterpreted() {
    // Simulates Boolean::Difference's lhs-retag csgrs quirk: rhs-
    // derived faces arrive at the output labeled DEGENERATE (the
    // unmetadata sentinel from the boolean bridge — see
    // csgrs_to_tessellation in operators::boolean). Verify the
    // labeled-inference treats those collectively as a single
    // Reinterpreted edge.
    let positions = vec![[0.0_f32, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 1.0, 0.0]];
    let input = labeled_mesh(positions.clone(), vec![0, 1, 2], vec![TopologyFaceId(0)]);
    let output = labeled_mesh(
        positions,
        vec![0, 1, 2, 0, 1, 2, 0, 1, 2],
        vec![
            TopologyFaceId(0),
            TopologyFaceId::DEGENERATE,
            TopologyFaceId::DEGENERATE,
        ],
    );

    let (_out, lineage) = infer_lineage(&input, &output, 100).expect("labeled degenerate lineage");
    // Label 0: Preserved (1 input, 1 output).
    let preserved = lineage
        .edges_by_evolution(TopologyEvolution::Preserved)
        .count();
    assert_eq!(preserved, 1);
    // DEGENERATE on output: collectively 1 Reinterpreted edge.
    let reint = lineage
        .edges_by_evolution(TopologyEvolution::Reinterpreted)
        .count();
    assert_eq!(
        reint, 1,
        "DEGENERATE-labeled rhs faces should surface as 1 Reinterpreted edge; got {reint}"
    );
    let edge = lineage
        .edges_by_evolution(TopologyEvolution::Reinterpreted)
        .next()
        .unwrap();
    assert_eq!(edge.to, Some(TopologyFaceId::DEGENERATE));
    assert!(edge.from.is_none());
}
