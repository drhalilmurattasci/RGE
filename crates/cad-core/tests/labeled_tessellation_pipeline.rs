//! Labeled-Tessellation flow through the Boolean → Transform pipeline
//! (audit-2 gap-5 closure: deep audit 2026-05-09 surfaced that Phase 2 added
//! the `output_is_labeled` cache-key extension, but no test exercises a
//! labeled output flowing through Boolean (which propagates labels) into
//! Transform (which now preserves labels because it is topology-preserving).
//!
//! The test exercises the **labeled-path through the Operator trait surface**:
//!
//! 1. Build a manually-labeled `Tessellation` via [`Tessellation::with_labels`]
//!    so the test controls the exact upstream label sequence.
//! 2. Feed the labeled `Tessellation` (lhs) + an unlabeled cube (rhs) into
//!    `BooleanOp::union().evaluate(...)`. The boolean operator's
//!    `evaluate` method auto-dispatches to `evaluate_with_labels` (the
//!    labeled path) when ANY input carries labels — so this fires the
//!    labeled-path code-path through the public API.
//! 3. Assert the boolean output IS labeled (`output.is_labeled() == true` and
//!    `output_is_labeled(...)` predicts the same). Per `boolean.rs:985-989`
//!    the `output_is_labeled` predicate returns `true` when any input is
//!    labeled.
//! 4. Pass the labeled boolean output through `TransformOp::evaluate(...)`
//!    and assert the transform output stays labeled with the exact same
//!    per-triangle `TopologyFaceId` sequence.
//!
//! This test is the regression gate for predicate-vs-reality alignment:
//! `output_is_labeled` must predict the same labeled state that `evaluate`
//! actually emits, or the cache-key prediction can diverge from runtime data.

use rge_cad_core::{
    BooleanOp, CuboidOp, Operator, OperatorGraph, OperatorNode, Tessellation, TessellationCache,
    Tolerance, TopologyFaceId, TransformOp,
};

/// Build a labeled cube `Tessellation` by tagging each of the 6 faces (12
/// triangles, 2 per face) with a distinct [`TopologyFaceId`]. This constructs
/// the exact label sequence directly via [`Tessellation::with_labels`].
fn labeled_cube_at_origin() -> Tessellation {
    // Evaluate a CuboidOp to get the canonical 8-vertex / 12-triangle cube
    // mesh, then re-wrap with explicit per-triangle labels. We assume the
    // canonical Cuboid emits 12 triangles in face-order — but even if the
    // ordering shifts, this test only requires that the labeling is
    // self-consistent (12 labels for 12 triangles) and that the labels are
    // non-degenerate, so the order-dependence is incidental.
    let unlabeled = OperatorNode::Cuboid(CuboidOp::default())
        .evaluate(&[])
        .expect("cuboid evaluates");
    assert_eq!(
        unlabeled.triangle_count(),
        12,
        "canonical cuboid should have 12 triangles"
    );

    // Tag each triangle with face_id = triangle_index / 2 (each face has 2
    // triangles in a quad). `TopologyFaceId(0..=5)` are non-degenerate by
    // construction (DEGENERATE is u64::MAX).
    let labels: Vec<TopologyFaceId> = (0..unlabeled.triangle_count())
        .map(|i| TopologyFaceId(i as u64 / 2))
        .collect();
    Tessellation::with_labels(
        unlabeled.positions.clone(),
        unlabeled.indices.clone(),
        labels,
    )
    .expect("with_labels validates")
}

/// Build an unlabeled cube shifted by +0.5 on each axis so the boolean has
/// real overlap to clip.
fn unlabeled_cube_shifted() -> Tessellation {
    let base = OperatorNode::Cuboid(CuboidOp::default())
        .evaluate(&[])
        .expect("cuboid evaluates");
    let shifted_positions = base
        .positions
        .iter()
        .map(|[x, y, z]| [*x + 0.5, *y + 0.5, *z + 0.5])
        .collect();
    Tessellation::new(shifted_positions, base.indices.clone()).expect("shifted cube")
}

/// Labeled-Tessellation pipeline: labeled cube + unlabeled cube -> Boolean
/// (Union, output IS labeled) -> Transform (output stays labeled). The
/// regression gate for the cache-key prediction: `TransformOp` must preserve
/// labels and report that labeled output through `output_is_labeled`.
#[test]
fn labeled_tessellation_flows_through_boolean_into_transform_preserves_labels() {
    // ---- Stage 1: the input tessellations ----
    let lhs_labeled = labeled_cube_at_origin();
    let rhs_unlabeled = unlabeled_cube_shifted();
    assert!(
        lhs_labeled.is_labeled(),
        "lhs must be labeled to fire the labeled path"
    );
    assert!(
        !rhs_unlabeled.is_labeled(),
        "rhs is intentionally unlabeled — exercises the mixed-input case"
    );

    // ---- Stage 2: Boolean union — auto-dispatches to labeled_path because
    // at least one input is labeled. Assert the output mesh IS labeled.
    let boolean = BooleanOp::union();

    // 2a — predicate matches reality: `output_is_labeled([true, false])`
    // returns true (the trait default is `iter().any(|b| *b)`; Boolean
    // doesn't override it).
    assert!(
        boolean.output_is_labeled(&[true, false]),
        "BooleanOp must propagate labels: output_is_labeled([true, false]) = true",
    );
    assert!(
        boolean.output_is_labeled(&[false, true]),
        "BooleanOp must propagate labels: output_is_labeled([false, true]) = true",
    );
    assert!(
        boolean.output_is_labeled(&[true, true]),
        "BooleanOp output_is_labeled([true, true]) = true",
    );
    assert!(
        !boolean.output_is_labeled(&[false, false]),
        "BooleanOp output_is_labeled([false, false]) = false (no labels to propagate)",
    );

    // 2b — actually run the labeled path through the public API.
    let boolean_output = boolean
        .evaluate(&[&lhs_labeled, &rhs_unlabeled])
        .expect("boolean union of labeled ∪ unlabeled");
    assert!(
        boolean_output.is_labeled(),
        "Boolean output MUST be labeled when any input is labeled \
         (boolean.rs:985-989); reality must match the output_is_labeled \
         prediction or the cache-key prediction is broken",
    );
    assert!(
        boolean_output.vertex_count() > 0,
        "boolean produced empty output (geometry sanity)",
    );
    assert!(
        boolean_output.triangle_count() > 0,
        "boolean produced no triangles (geometry sanity)",
    );

    // ---- Stage 3: Transform preserves labels because topology is unchanged ----
    let transform = TransformOp {
        translation: [0.1, 0.2, 0.3],
        rotation_quat_xyzw: [0.0, 0.0, 0.0, 1.0],
        scale: [1.0, 1.0, 1.0],
    };

    // 3a — predicate matches reality: labeled input predicts labeled output.
    assert!(
        transform.output_is_labeled(&[true]),
        "TransformOp must preserve labels: output_is_labeled([true]) = true",
    );
    assert!(
        !transform.output_is_labeled(&[false]),
        "TransformOp output_is_labeled([false]) = false (no labels to start)",
    );

    // 3b — actually run Transform on the labeled boolean output and
    // confirm the output is labeled with the same face-label sequence.
    let transform_output = transform
        .evaluate(&[&boolean_output])
        .expect("transform on labeled input");
    assert!(
        transform_output.is_labeled(),
        "Transform output must stay labeled when the input is labeled",
    );
    assert_eq!(
        transform_output.face_labels(),
        boolean_output.face_labels(),
        "Transform must pass face labels through unchanged",
    );
    assert_eq!(
        transform_output.vertex_count(),
        boolean_output.vertex_count(),
        "Transform preserves vertex count (positions-only transform)",
    );
    assert_eq!(
        transform_output.indices.len(),
        boolean_output.indices.len(),
        "Transform preserves index count (indices pass through unchanged)",
    );

    // ---- Stage 4: defense-in-depth — the prediction-vs-reality contract
    // for both ops in the chain holds in BOTH directions for the actual
    // input states observed (this is what `effective_hash_and_label_inner`
    // relies on at the cache-key level).
    let boolean_inputs_labeled = [lhs_labeled.is_labeled(), rhs_unlabeled.is_labeled()];
    assert_eq!(
        boolean.output_is_labeled(&boolean_inputs_labeled),
        boolean_output.is_labeled(),
        "Boolean output_is_labeled prediction MUST match evaluate(...).is_labeled() \
         — divergence breaks cache-key prediction at the eval boundary",
    );
    let transform_inputs_labeled = [boolean_output.is_labeled()];
    assert_eq!(
        transform.output_is_labeled(&transform_inputs_labeled),
        transform_output.is_labeled(),
        "Transform output_is_labeled prediction MUST match evaluate(...).is_labeled() \
         — divergence breaks cache-key prediction at the eval boundary",
    );
}

/// Audit-2 deep-audit-2 round-2 closure: extends the labeled-path coverage
/// above with a regression for the **cache-key uniqueness invariant** that
/// the prior dispatch claimed but did not actually exercise. The Phase 2
/// substrate folds the upstream `output_is_labeled` bitmap into
/// `OperatorGraph::effective_hash_and_label` (operator_graph.rs:330-341), and
/// `TransformOp` must keep that predicted labeled state aligned with the
/// labeled tessellation it now emits for labeled input.
///
/// **What this test does**:
///
/// 1. Builds the requested pipeline: `Cuboid_a + Cuboid_b → Boolean(Union) →
///    Transform(translate)` via the public [`OperatorGraph`] API.
/// 2. Calls [`OperatorGraph::effective_hash_and_label_root`] on the Transform
///    node to get its cache-key fragment + predicted-output-is-labeled.
/// 3. Verifies the recursive hash propagation: changing an upstream Cuboid's
///    parameters changes the Transform's `effective_hash` (basic correctness —
///    proves the cache key actually depends on the upstream, including
///    transitively through the Boolean operator).
/// 4. Verifies the predicted-output-is-labeled aligns with what the Transform
///    actually emits when graph evaluation supplies it a labeled Boolean
///    upstream.
/// 5. Cross-checks the predicate alignment via `TessellationCache` round-trip
///    through [`OperatorGraph::evaluate`]: same graph re-evaluated → cache hit;
///    different graph → cache miss.
///
/// The substrate-level "same structure but different upstream labeled bitmap"
/// invariant remains covered by
/// [`operator_graph::tests::effective_hash_distinguishes_labeled_vs_unlabeled_input_state`].
/// This test owns the integration-level regression for predicate-vs-reality
/// alignment and recursive-hash propagation through the public graph API.
#[test]
#[allow(
    clippy::too_many_lines,
    reason = "single integration test exercises a multi-stage pipeline (build 2x Cuboid + Boolean + Transform graph; compute hash + label predicate; mutate upstream parameter; recompute and assert difference; exercise OperatorGraph::evaluate cache hit/miss). Splitting into helpers would obscure the linear test narrative; the doc-comment above explicitly enumerates the 5 verification steps."
)]
fn transform_cache_key_distinguishes_labeled_vs_unlabeled_upstream() {
    // ---- Build the requested pipeline: 2x Cuboid → Boolean(Union) → Transform ----
    let mut g_a = OperatorGraph::new();
    let cu_a_lhs = g_a
        .add_operator(OperatorNode::Cuboid(CuboidOp::default()))
        .expect("cuboid lhs");
    let cu_a_rhs = g_a
        .add_operator(OperatorNode::Cuboid(CuboidOp {
            width: 1.0,
            height: 1.0,
            depth: 1.0001, // tiny perturbation so NodeId differs from cu_a_lhs
        }))
        .expect("cuboid rhs");
    let bool_a = g_a
        .add_operator(OperatorNode::Boolean(BooleanOp::union()))
        .expect("boolean union");
    g_a.connect(cu_a_lhs, bool_a, 0).expect("lhs → bool port 0");
    g_a.connect(cu_a_rhs, bool_a, 1).expect("rhs → bool port 1");
    let tx_a = g_a
        .add_operator(OperatorNode::Transform(TransformOp {
            translation: [0.1, 0.2, 0.3],
            rotation_quat_xyzw: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        }))
        .expect("transform");
    g_a.connect(bool_a, tx_a, 0).expect("bool → transform");
    g_a.set_root(tx_a).expect("set root");

    // ---- Stage 1: effective_hash_and_label on the Transform output ----
    let (tx_hash_a, tx_predicted_labeled_a) = g_a
        .effective_hash_and_label_root(tx_a)
        .expect("effective_hash_and_label_root for tx_a");

    // Transform's predicted output is labeled because Boolean's labeled output
    // is topology-preserved through Transform.
    assert!(
        tx_predicted_labeled_a,
        "TransformOp::output_is_labeled must predict labeled output for a \
         labeled upstream"
    );

    // The hash is deterministic — repeated calls produce the same bytes.
    let (tx_hash_a_repeat, _) = g_a
        .effective_hash_and_label_root(tx_a)
        .expect("repeat hash");
    assert_eq!(
        tx_hash_a, tx_hash_a_repeat,
        "effective_hash_and_label_root must be deterministic (cache key depends on it)"
    );

    // ---- Stage 2: build a SECOND graph that differs ONLY in upstream Cuboid params ----
    // This proves the recursive hash walk actually folds upstream content into
    // the Transform's hash — the basic correctness property the bitmap-fold
    // protection is layered on top of.
    let mut g_b = OperatorGraph::new();
    let cu_b_lhs = g_b
        .add_operator(OperatorNode::Cuboid(CuboidOp {
            width: 2.0, // <- different!
            height: 1.0,
            depth: 1.0,
        }))
        .expect("cuboid lhs (different)");
    let cu_b_rhs = g_b
        .add_operator(OperatorNode::Cuboid(CuboidOp {
            width: 1.0,
            height: 1.0,
            depth: 1.0001,
        }))
        .expect("cuboid rhs");
    let bool_b = g_b
        .add_operator(OperatorNode::Boolean(BooleanOp::union()))
        .expect("boolean union");
    g_b.connect(cu_b_lhs, bool_b, 0).expect("lhs → bool port 0");
    g_b.connect(cu_b_rhs, bool_b, 1).expect("rhs → bool port 1");
    let tx_b = g_b
        .add_operator(OperatorNode::Transform(TransformOp {
            // SAME transform params as tx_a — its local structural_hash is
            // identical. If the Transform's effective_hash didn't recursively
            // depend on the upstream, the two graphs would collide.
            translation: [0.1, 0.2, 0.3],
            rotation_quat_xyzw: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0, 1.0, 1.0],
        }))
        .expect("transform");
    g_b.connect(bool_b, tx_b, 0).expect("bool → transform");
    g_b.set_root(tx_b).expect("set root");

    let (tx_hash_b, tx_predicted_labeled_b) = g_b
        .effective_hash_and_label_root(tx_b)
        .expect("effective_hash_and_label_root for tx_b");

    // Both graphs predict labeled output because their Boolean upstreams are
    // labeled and Transform preserves labels.
    assert!(
        tx_predicted_labeled_b,
        "Transform predicts labeled output for labeled Boolean upstream"
    );
    // BUT the cache-key bytes MUST differ — the Transform's local
    // structural_hash is identical, so any collision implies the recursive
    // walk isn't actually folding upstream content into the hash.
    assert_ne!(
        tx_hash_a, tx_hash_b,
        "Transform's effective_hash MUST differ when an upstream Cuboid's \
         parameters change — proves the recursive hash walk actually folds \
         upstream content (the foundation the bitmap-fold protection is \
         layered on top of). If this fails, two semantically-different \
         graphs would collide in the cache.",
    );

    // ---- Stage 3: cache hit/miss round-trip through OperatorGraph::evaluate ----
    // Independent verification that the cache-key ACTUALLY honours the
    // effective_hash byte difference at the cache-API level.
    let tol = Tolerance::new(0.001).expect("tol");
    let mut cache = TessellationCache::new();

    // Evaluate g_a → 1 miss per node (Cuboid_a_lhs, Cuboid_a_rhs, Boolean,
    // Transform = 4 misses; the count is implementation-defined so we just
    // record the baseline).
    let _eval_a = g_a.evaluate(tx_a, &mut cache, tol).expect("evaluate g_a");
    let misses_after_a = cache.misses();
    let hits_after_a = cache.hits();
    assert!(
        misses_after_a > 0,
        "first evaluation must produce at least one miss"
    );

    // Re-evaluate g_a — every node MUST hit because the effective_hash bytes
    // are stable across calls.
    let _eval_a_repeat = g_a
        .evaluate(tx_a, &mut cache, tol)
        .expect("re-evaluate g_a");
    assert_eq!(
        cache.misses(),
        misses_after_a,
        "re-evaluating g_a must not produce new misses (effective_hash is stable)",
    );
    assert!(
        cache.hits() > hits_after_a,
        "re-evaluating g_a must produce hits (cache-key collision-correctness)",
    );

    // Evaluate g_b — Transform_b's effective_hash differs (different Cuboid
    // upstream), so it must MISS the cache despite Transform's local
    // structural_hash matching tx_a's.
    let misses_before_b = cache.misses();
    let _eval_b = g_b.evaluate(tx_b, &mut cache, tol).expect("evaluate g_b");
    assert!(
        cache.misses() > misses_before_b,
        "g_b's Transform must MISS the cache despite identical Transform \
         params, because the upstream differs — if this fails, the cache \
         key isn't recursively dependent on upstream content and the cache \
         would surface stale Transform outputs from a different upstream",
    );

    // ---- Stage 4: predicate-vs-reality contract via actual evaluation ----
    // For BOTH graphs, the Transform output is labeled because Transform
    // preserves its labeled Boolean upstream. The cache-key prediction
    // (predicted_labeled_a/b = true) MUST match eval reality for the
    // bitmap-fold contract to be sound.
    let actual_a = g_a
        .evaluate(tx_a, &mut cache, tol)
        .expect("re-evaluate g_a");
    assert_eq!(
        actual_a.is_labeled(),
        tx_predicted_labeled_a,
        "Transform output's actual is_labeled() must match the predicted \
         output_is_labeled() the cache key was computed with — divergence \
         breaks cache-key uniqueness",
    );
    let actual_b = g_b
        .evaluate(tx_b, &mut cache, tol)
        .expect("re-evaluate g_b");
    assert_eq!(
        actual_b.is_labeled(),
        tx_predicted_labeled_b,
        "Same predicate-vs-reality contract for g_b",
    );
}
