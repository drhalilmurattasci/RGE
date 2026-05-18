//! End-to-end Phase 7.4 lineage prototype smoke test.
//!
//! Per [PLAN.md §1.5.4.3](../../../plans/PLAN.md). Feeds a Boolean operation
//! through the full cad-core stack and the lineage inference pipeline,
//! verifying the [`LineageGraph`] captures the expected
//! [`TopologyEvolution`] kinds (Preserved, Reinterpreted, Split / Deleted)
//! end-to-end.
//!
//! These smokes specifically exercise the pipeline:
//!
//!   `CadGraph` → `OperatorGraph::evaluate` (Boolean output) →
//!   `label_by_plane` (input mesh) → `infer_lineage` →
//!   classify input faces vs output planes.
//!
//! After the 2026-05-08 unified-mesh refactor, the labeled and unlabeled
//! paths share a single [`infer_lineage`] entry point that dispatches on
//! `output.is_labeled()`. The labeled-path smoke (third test below)
//! constructs labeled inputs via [`label_by_plane`] and then invokes
//! [`BooleanOp::evaluate`] (no separate `evaluate_labeled` anymore — the
//! `BooleanOp` detects labeled inputs automatically and propagates).

use rge_cad_core::{
    infer_lineage, label_by_plane, BooleanMode, BooleanOp, CadGraph, CuboidOp, Operator,
    OperatorNode, TessellationCache, Tolerance, TopologyEvolution, TransformOp,
};

fn tol() -> Tolerance {
    Tolerance::new(0.001).expect("tol")
}

/// Cube ∪ `translated_cube` → expect labeled-path lineage for each `cube_a`
/// input face. `TransformOp` preserves `cube_b` labels, so the Boolean output
/// is labeled and `infer_lineage` uses label tracking rather than the
/// unlabeled plane heuristic.
#[test]
fn cube_boolean_union_with_transform_labels_tracks_input_faces() {
    let mut cad = CadGraph::new();
    cad.begin_operation().expect("begin");

    let g = cad.graph_mut().expect("mut");
    let cube_a = g
        .add_operator(OperatorNode::Cuboid(CuboidOp::default())) // 1×1×1
        .expect("cube_a");
    // Slightly perturbed depth so cube_b's content-derived NodeId differs.
    let cube_b = g
        .add_operator(OperatorNode::Cuboid(CuboidOp {
            width: 1.0,
            height: 1.0,
            depth: 1.0001,
        }))
        .expect("cube_b");
    let xform = g
        .add_operator(OperatorNode::Transform(TransformOp {
            translation: [0.5, 0.5, 0.5],
            rotation_quat_xyzw: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        }))
        .expect("xform");
    g.connect(cube_b, xform, 0).expect("c1");

    let union = g
        .add_operator(OperatorNode::Boolean(BooleanOp::new(BooleanMode::Union)))
        .expect("union");
    g.connect(cube_a, union, 0).expect("c2");
    g.connect(xform, union, 1).expect("c3");
    g.set_root(union).expect("set_root");
    cad.commit("cube ∪ translated_cube").expect("commit");

    // Evaluate cube_a alone to get its tessellation as the lineage input.
    let mut cache = TessellationCache::new();
    let cube_a_tess = cad
        .graph()
        .evaluate(cube_a, &mut cache, tol())
        .expect("eval cube_a");
    let labeled_input = label_by_plane(&cube_a_tess, 0).expect("label cube_a");
    assert_eq!(
        labeled_input.face_count(),
        Some(6),
        "cube_a should label into 6 plane groups (the 6 cube faces)"
    );

    // Evaluate the Union output.
    let union_tess = cad
        .graph()
        .evaluate(union, &mut cache, tol())
        .expect("eval union");

    assert!(
        union_tess.is_labeled(),
        "Cuboid -> Transform -> Boolean union should stay labeled"
    );

    // Run lineage inference. The output is labeled, so this uses the
    // high-confidence label-tracking path rather than synthesizing new
    // plane-heuristic output labels.
    let (labeled_output, lineage) =
        infer_lineage(&labeled_input, &union_tess, 100).expect("lineage");

    assert!(
        labeled_output.face_count().expect("labeled output") > 0,
        "union output has no labeled faces"
    );
    assert!(!lineage.is_empty(), "lineage graph is empty");

    // In the labeled path, output labels are consumed directly. This graph's
    // output labels are already covered by the input label set, so the
    // inference should not manufacture plane-heuristic Reinterpreted edges.
    let reint_count = lineage
        .edges_by_evolution(TopologyEvolution::Reinterpreted)
        .count();
    assert_eq!(
        reint_count,
        0,
        "labeled-path Union should not synthesize Reinterpreted edges; \
         lineage = {} edges, output face_count = {:?}",
        lineage.len(),
        labeled_output.face_count()
    );

    let preserved_count = lineage
        .edges_by_evolution(TopologyEvolution::Preserved)
        .count();
    let split_count = lineage.edges_by_evolution(TopologyEvolution::Split).count();
    assert!(
        preserved_count + split_count >= 1,
        "labeled-path Union should preserve or split at least one input face"
    );

    // Every input face must have at least one outgoing edge.
    for input_face_id in 0..6 {
        let face = rge_cad_core::TopologyFaceId(input_face_id);
        let count = lineage.edges_from(face).count();
        assert!(
            count >= 1,
            "input face {face} must have at least one lineage edge"
        );
    }
}

/// Cube ∖ `translated_cube` → expect Split or Deleted edges for the
/// `cube_a` faces partially carved away by `cube_b`. With label-preserving
/// `TransformOp`, this exercises the labeled output path through
/// `OperatorGraph::evaluate`.
#[test]
fn cube_minus_offset_cube_records_deletion_or_split_for_consumed_faces() {
    let mut cad = CadGraph::new();
    cad.begin_operation().expect("begin");

    let g = cad.graph_mut().expect("mut");
    let cube_a = g
        .add_operator(OperatorNode::Cuboid(CuboidOp::default()))
        .expect("cube_a");
    let cube_b = g
        .add_operator(OperatorNode::Cuboid(CuboidOp {
            width: 1.0,
            height: 1.0,
            depth: 1.0001,
        }))
        .expect("cube_b");
    let xform = g
        .add_operator(OperatorNode::Transform(TransformOp {
            translation: [0.5, 0.5, 0.5],
            rotation_quat_xyzw: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        }))
        .expect("xform");
    g.connect(cube_b, xform, 0).expect("c1");

    let diff = g
        .add_operator(OperatorNode::Boolean(BooleanOp::new(
            BooleanMode::Difference,
        )))
        .expect("diff");
    g.connect(cube_a, diff, 0).expect("c2");
    g.connect(xform, diff, 1).expect("c3");
    g.set_root(diff).expect("set_root");
    cad.commit("cube ∖ translated_cube").expect("commit");

    let mut cache = TessellationCache::new();
    let cube_a_tess = cad
        .graph()
        .evaluate(cube_a, &mut cache, tol())
        .expect("eval cube_a");
    let labeled_input = label_by_plane(&cube_a_tess, 0).expect("label cube_a");

    let diff_tess = cad
        .graph()
        .evaluate(diff, &mut cache, tol())
        .expect("eval diff");

    let (labeled_output, lineage) =
        infer_lineage(&labeled_input, &diff_tess, 100).expect("lineage");
    assert!(labeled_output.face_count().expect("labeled output") > 0);
    assert!(!lineage.is_empty());

    // The Difference removes a corner from cube_a (the `[0,0.5]³` octant is
    // carved). cube_a's three faces that border the carve (+X, +Y, +Z) are
    // partially consumed.
    //
    // **Heuristic-vs-csgrs reality**: BSP-tree CSG can produce MORE
    // triangles per surviving plane than the 2-triangle input (each cube
    // face's two triangles get re-triangulated into many slivers when the
    // BSP partitions the face). With our v0 triangle-count heuristic
    // (input > output → Split; input < output → Merged), this means
    // partially-consumed faces frequently classify as **Merged** rather
    // than Split. Merged is a v0 false positive that the future
    // boundary-precision detector will fix.
    //
    // The integration smoke therefore asserts the looser invariant:
    // SOMETHING about the cube_a faces was non-trivially affected (Split,
    // Deleted, OR Merged — i.e. not all 6 faces are Preserved). This
    // surfaces the unknown "what does the heuristic actually do on
    // Difference?" as documented behavior rather than a test failure.
    let split_count = lineage.edges_by_evolution(TopologyEvolution::Split).count();
    let deleted_count = lineage
        .edges_by_evolution(TopologyEvolution::Deleted)
        .count();
    let merged_count = lineage
        .edges_by_evolution(TopologyEvolution::Merged)
        .count();
    let preserved_count = lineage
        .edges_by_evolution(TopologyEvolution::Preserved)
        .count();
    assert!(
        split_count + deleted_count + merged_count >= 1,
        "expected at least one Split, Deleted, or Merged edge from Difference, got 0 of each \
         (preserved={preserved_count}); cube_a faces should be consumed/altered in some way. \
         Lineage edge total = {}",
        lineage.len()
    );
    // Sanity: not every cube_a face should be Preserved — the carved
    // corner must have affected at least one face.
    assert!(
        preserved_count < 6,
        "Difference left all 6 cube_a faces Preserved, which contradicts the carve-away geometry"
    );

    // Difference also introduces new internal faces (the carved octant's
    // walls). At least one Reinterpreted edge.
    let reint_count = lineage
        .edges_by_evolution(TopologyEvolution::Reinterpreted)
        .count();
    assert!(
        reint_count > 0,
        "expected Reinterpreted edges from Difference output but found 0"
    );
}

/// Cube ∖ `translated_cube` via the **labeled path** (csgrs metadata-
/// passthrough). After the 2026-05-08 unified-mesh refactor, the labeled
/// path is engaged simply by passing labeled [`Tessellation`] inputs to
/// the unified [`BooleanOp::evaluate`] — there is no longer a separate
/// `evaluate_labeled` method. The op detects `is_labeled()` and threads
/// per-input-triangle [`TopologyFaceId`] through csgrs's polygon metadata.
///
/// The plane-only path classifies partially-consumed Difference faces as
/// **Merged** (a v0 false-positive — csgrs's BSP triangulation produces
/// more output triangles per surviving plane than the 2-tri input). The
/// labeled path uses the per-polygon csgrs metadata so input face
/// identity survives the BSP, and the unified inference's triangle-count
/// comparison correctly fires `Split` (input>output) for consumed faces.
///
/// This smoke verifies: the labeled path surfaces **at least one Split
/// edge** for partial consumption (where the plane-only path would
/// over-count as Merged).
#[test]
fn cube_minus_offset_cube_with_labeled_inputs_classifies_as_split_not_merged() {
    let mut cad = CadGraph::new();
    cad.begin_operation().expect("begin");

    let g = cad.graph_mut().expect("mut");
    let cube_a = g
        .add_operator(OperatorNode::Cuboid(CuboidOp::default()))
        .expect("cube_a");
    let cube_b = g
        .add_operator(OperatorNode::Cuboid(CuboidOp {
            width: 1.0,
            height: 1.0,
            depth: 1.0001,
        }))
        .expect("cube_b");
    let xform = g
        .add_operator(OperatorNode::Transform(TransformOp {
            translation: [0.5, 0.5, 0.5],
            rotation_quat_xyzw: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        }))
        .expect("xform");
    g.connect(cube_b, xform, 0).expect("c1");
    g.set_root(xform).expect("set_root xform");
    cad.commit("cube_b xformed").expect("commit");

    // Evaluate both lhs (cube_a) and rhs (translated cube_b) as
    // Tessellations through the existing graph path, then upgrade them
    // to labeled tessellations via label_by_plane (lhs ids start at 0,
    // rhs ids start at 1000 so the two ranges are visibly disjoint).
    let mut cache = TessellationCache::new();
    let cube_a_tess = cad
        .graph()
        .evaluate(cube_a, &mut cache, tol())
        .expect("eval cube_a");
    let xform_tess = cad
        .graph()
        .evaluate(xform, &mut cache, tol())
        .expect("eval xform");

    let labeled_input = label_by_plane(&cube_a_tess, 0).expect("label cube_a");
    let labeled_rhs = label_by_plane(&xform_tess, 1000).expect("label rhs");

    assert_eq!(
        labeled_input.face_count(),
        Some(6),
        "cube_a labels into 6 plane groups (axis-aligned faces)"
    );
    assert_eq!(
        labeled_rhs.face_count(),
        Some(6),
        "translated cube_b labels into 6 plane groups"
    );

    // Run the labeled Difference: lhs labels carried as csgrs polygon
    // metadata, rhs labels carried similarly (csgrs retags rhs polygons
    // with lhs's mesh-level metadata = None, surfacing as DEGENERATE
    // labels on those rhs-derived output faces).
    let diff_op = BooleanOp::new(BooleanMode::Difference);
    let labeled_output = diff_op
        .evaluate(&[&labeled_input, &labeled_rhs])
        .expect("difference labeled");
    assert!(labeled_output.is_labeled(), "labeled output expected");
    assert!(
        labeled_output.triangle_count() > 0,
        "labeled Difference output should be non-empty"
    );

    // Now run the unified inference between cube_a labeled and the
    // labeled output (lhs perspective: which cube_a faces survived /
    // split / disappeared?). Output is labeled → label-tracking path.
    let (_out, lineage) =
        infer_lineage(&labeled_input, &labeled_output, 100).expect("labeled lineage");
    assert!(
        !lineage.is_empty(),
        "labeled-path lineage should not be empty"
    );

    // Central correctness: at least one Split edge should fire for the
    // cube_a faces partially consumed by the carve. The plane-only path
    // classifies these as Merged (the v0 false positive); the labeled
    // path must classify them as Split — and the labeled inference
    // never emits Merged from a single-input-label scan, so Merged
    // count must be exactly zero.
    let split_count = lineage.edges_by_evolution(TopologyEvolution::Split).count();
    let merged_count = lineage
        .edges_by_evolution(TopologyEvolution::Merged)
        .count();
    assert!(
        split_count >= 1,
        "labeled path must surface at least one Split edge for partial consumption \
         (the v0 plane-only false-positive Merged class is fixed by metadata-passthrough); \
         got split={split_count}, merged={merged_count}. Lineage = {} edges total",
        lineage.len()
    );
    assert_eq!(
        merged_count, 0,
        "labeled-path inference must NEVER emit Merged from a per-input-label scan; got {merged_count}"
    );

    // Sanity: not every cube_a face should be Preserved — the carved
    // octant must have affected at least one input face.
    let preserved_count = lineage
        .edges_by_evolution(TopologyEvolution::Preserved)
        .count();
    assert!(
        preserved_count < 6,
        "labeled-path Difference left all 6 cube_a faces Preserved, contradicting the carve geometry"
    );

    // The DEGENERATE-tagged rhs-derived faces (csgrs lhs-retag quirk)
    // surface as a Reinterpreted edge collectively, OR a real rhs label
    // (1000..1006) appears on the output. Either way, at least one
    // Reinterpreted edge should be present (new internal face from the
    // boolean carve).
    let reint_count = lineage
        .edges_by_evolution(TopologyEvolution::Reinterpreted)
        .count();
    assert!(
        reint_count >= 1,
        "expected at least one Reinterpreted edge for the new carve-induced internal faces; got {reint_count}"
    );
}
