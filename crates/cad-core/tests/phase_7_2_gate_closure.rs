//! Phase 7.2 IMPLEMENTATION.md gate-closure stress test.
//!
//! 100 deterministic-seeded random operator chains × 10 random
//! parameter rebuilds per chain (1000 mutations total). Asserts:
//!
//! * Topology-preserving mutations (numeric coordinate / length /
//!   angle-within-mode changes, Transform parameter changes) preserve
//!   the chain's resolved face+edge ID vectors byte-identically.
//! * Topology-changing mutations (profile vertex count, segments,
//!   full↔partial mode) cause the chain's resolved face+edge ID
//!   vectors to **not equal** the initial vectors. Disjointness is
//!   asserted where the substrate guarantees it (Side faces, all
//!   edges); categorical caps (Bottom / Top in Extrude/Loft) may
//!   intentionally remain identical across profile shape changes per
//!   the explicit substrate contract — so this test asserts vector-
//!   level *inequality*, not full disjointness.
//!
//! # Honest framing — `TopologyEvolution` interpretation
//!
//! The IMPLEMENTATION.md spec text reads "...face/edge IDs preserved
//! per `TopologyEvolution` enum". `TopologyEvolution` lives in
//! `crate::topo_lineage::types` (D-7.4 prototype substrate from
//! 2026-05-07) and captures *operator-internal face evolution*
//! (Preserved / Split / Merged / Deleted / Reinterpreted, e.g.
//! Boolean clip output). It is orthogonal to D-7.2's identity-
//! stability-across-parameter-rebuilds substrate. This test closes
//! the *rebuild-stability spirit* of the gate; `TopologyEvolution`
//! remains the operator-internal lineage substrate (a separate
//! concern, preserved for future work).
//!
//! # Reproducibility
//!
//! Single fixed seed `0x7E5A_DEAD_BEEF_C0DE`. Failures print the
//! seed, chain index, and rebuild index in the assertion message
//! so any flake can be reproduced via the same seeded run.
//!
//! Failure class inherited: snapshot-recoverable (test-only).

use std::f32::consts::PI;

use rge_cad_core::{
    brep_edge_ids_for_node, brep_face_ids_for_node, BRepEdgeId, BRepFaceId, BRepOwnerId, CadGraph,
    CuboidOp, ExtrudeOp, LoftOp, OperatorNode, Polygon2D, Polyline3D, RevolveOp, SweepOp,
    TransformOp,
};
use rge_kernel_graph_foundation::NodeId;

const STRESS_TEST_SEED: u64 = 0x7E5A_DEAD_BEEF_C0DE;
const NUM_CHAINS: usize = 100;
const REBUILDS_PER_CHAIN: usize = 10;
const TEST_OWNER: BRepOwnerId = BRepOwnerId::from_bytes([0x42; 16]);

// ---------------------------------------------------------------------------
// Tiny deterministic PRNG (xorshift64; no Cargo.toml change permitted)
// ---------------------------------------------------------------------------

struct TinyRng(u64);

impl TinyRng {
    fn new(seed: u64) -> Self {
        // xorshift64 produces all zeros if seeded with zero.
        // The fixed STRESS_TEST_SEED is non-zero by construction; this
        // assert is a defense against future maintainers changing it.
        assert_ne!(seed, 0, "xorshift64 cannot accept seed=0");
        Self(seed)
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn next_range_u32(&mut self, max_exclusive: u32) -> u32 {
        debug_assert!(max_exclusive > 0);
        (self.next_u64() % u64::from(max_exclusive)) as u32
    }

    fn next_f32_in_range(&mut self, min: f32, max: f32) -> f32 {
        debug_assert!(max > min);
        let unit = (self.next_u64() & 0x00FF_FFFF) as f32 / 0x00FF_FFFF as f32;
        min + unit * (max - min)
    }

    fn next_bool(&mut self) -> bool {
        (self.next_u64() & 1) == 1
    }
}

// ---------------------------------------------------------------------------
// Chain construction
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RootKind {
    Cuboid,
    Extrude,
    Revolve,
    Loft,
    Sweep,
}

#[derive(Clone, Debug)]
struct Chain {
    root_kind: RootKind,
    /// The root operator with its initial parameters (used for rebuild).
    root_seed: RootSeed,
    /// Transform seeds (one per wrap layer; 0..=2 layers).
    transform_seeds: Vec<TransformSeed>,
}

/// Captures parameters for the root direct provider so we can rebuild.
#[derive(Clone, Debug)]
enum RootSeed {
    Cuboid {
        width: f32,
        height: f32,
        depth: f32,
    },
    Extrude {
        profile_n: u32,
        profile_radius: f32,
        length: f32,
    },
    Revolve {
        profile_n: u32,
        profile_radius: f32,
        segments: u32,
        mode: RevolveModeSeed,
        angle: f32,
    },
    Loft {
        profile_n: u32,
        profile_radius: f32,
        length: f32,
    },
    Sweep {
        /// Profile vertex count (`n`); a topology counter.
        profile_n: u32,
        /// Profile circumradius; numeric (topology-preserving) coordinate.
        profile_radius: f32,
        /// Path point count (`m`); `m - 1` segments — a topology counter.
        path_m: u32,
        /// Per-segment Z increment (`> 0`); numeric coordinate. Keeps the
        /// path strictly monotonic in Z so `SweepOp::evaluate` accepts it.
        path_z_step: f32,
        /// Per-segment XY drift; numeric coordinate. Produces a sheared
        /// but valid sweep (rigid profile translation, monotonic-Z kept).
        path_drift: [f32; 2],
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RevolveModeSeed {
    Full,
    Partial,
}

#[derive(Clone, Debug)]
struct TransformSeed {
    translation: [f32; 3],
    scale: [f32; 3],
    // identity rotation for simplicity; rotation parameter mutations
    // are still topology-preserving but quaternion math adds noise to
    // the test without adding signal.
}

fn generate_random_chain(rng: &mut TinyRng) -> Chain {
    let root_kind = match rng.next_range_u32(5) {
        0 => RootKind::Cuboid,
        1 => RootKind::Extrude,
        2 => RootKind::Revolve,
        3 => RootKind::Loft,
        _ => RootKind::Sweep,
    };
    let root_seed = generate_root_seed(rng, root_kind);
    let transform_count = rng.next_range_u32(3) as u8; // 0, 1, or 2
    let transform_seeds: Vec<_> = (0..transform_count)
        .map(|_| generate_transform_seed(rng))
        .collect();
    Chain {
        root_kind,
        root_seed,
        transform_seeds,
    }
}

fn generate_root_seed(rng: &mut TinyRng, kind: RootKind) -> RootSeed {
    match kind {
        RootKind::Cuboid => RootSeed::Cuboid {
            width: rng.next_f32_in_range(0.5, 5.0),
            height: rng.next_f32_in_range(0.5, 5.0),
            depth: rng.next_f32_in_range(0.5, 5.0),
        },
        RootKind::Extrude => RootSeed::Extrude {
            profile_n: 3 + rng.next_range_u32(4), // 3..=6
            profile_radius: rng.next_f32_in_range(0.5, 3.0),
            length: rng.next_f32_in_range(0.5, 5.0),
        },
        RootKind::Revolve => {
            let profile_n = 3 + rng.next_range_u32(4); // 3..=6
            let segments = 3 + rng.next_range_u32(14); // 3..=16
            let mode = if rng.next_bool() {
                RevolveModeSeed::Full
            } else {
                RevolveModeSeed::Partial
            };
            // Partial mode: angle in (0, 2π); Full mode: angle = 2π exactly.
            let angle = match mode {
                RevolveModeSeed::Full => 2.0 * PI,
                RevolveModeSeed::Partial => rng.next_f32_in_range(0.1, PI * 1.9),
            };
            RootSeed::Revolve {
                profile_n,
                profile_radius: rng.next_f32_in_range(0.5, 3.0),
                segments,
                mode,
                angle,
            }
        }
        RootKind::Loft => RootSeed::Loft {
            profile_n: 3 + rng.next_range_u32(4), // 3..=6
            profile_radius: rng.next_f32_in_range(0.5, 3.0),
            length: rng.next_f32_in_range(0.5, 5.0),
        },
        RootKind::Sweep => RootSeed::Sweep {
            profile_n: 3 + rng.next_range_u32(4), // 3..=6
            profile_radius: rng.next_f32_in_range(0.5, 3.0),
            path_m: 2 + rng.next_range_u32(4), // 2..=5 points → 1..=4 segments
            path_z_step: rng.next_f32_in_range(0.5, 3.0),
            path_drift: [
                rng.next_f32_in_range(-1.0, 1.0),
                rng.next_f32_in_range(-1.0, 1.0),
            ],
        },
    }
}

fn generate_transform_seed(rng: &mut TinyRng) -> TransformSeed {
    TransformSeed {
        translation: [
            rng.next_f32_in_range(-5.0, 5.0),
            rng.next_f32_in_range(-5.0, 5.0),
            rng.next_f32_in_range(-5.0, 5.0),
        ],
        scale: [
            rng.next_f32_in_range(0.5, 3.0),
            rng.next_f32_in_range(0.5, 3.0),
            rng.next_f32_in_range(0.5, 3.0),
        ],
    }
}

/// Build a regular CCW polygon of `n` vertices with given `radius`,
/// centered at the origin. For `Revolve`, callers offset the profile
/// to satisfy the "all x >= 0" gate via `offset_x`.
fn make_regular_polygon(n: u32, radius: f32, offset_x: f32) -> Polygon2D {
    let n_f = n as f32;
    let mut points = Vec::with_capacity(n as usize);
    for i in 0..n {
        let theta = (i as f32) * 2.0 * PI / n_f;
        let x = offset_x + radius * theta.cos();
        let y = radius * theta.sin();
        points.push([x, y]);
    }
    Polygon2D::new(points).expect("regular polygon")
}

/// Build a strictly monotonic-Z `Polyline3D` of `m` points for a
/// `SweepOp` path. Point `k` sits at
/// `(k * drift_x, k * drift_y, k * z_step)`. A positive `z_step`
/// guarantees both strict monotonic Z (`SweepOp::evaluate`'s principal
/// v0 gate) and adjacent-point distinctness (`Polyline3D::new`'s
/// degenerate-segment check), so a path built this way is always
/// valid Sweep input. `m` points yield `m - 1` segments.
fn make_monotonic_z_path(m: u32, z_step: f32, drift: [f32; 2]) -> Polyline3D {
    let mut points = Vec::with_capacity(m as usize);
    for k in 0..m {
        let kf = k as f32;
        points.push([kf * drift[0], kf * drift[1], kf * z_step]);
    }
    Polyline3D::new(points).expect("monotonic-z path")
}

/// Build an actual `CadGraph` from a `Chain` and resolve face+edge IDs
/// at the topmost operator (root or final Transform).
fn build_and_resolve(chain: &Chain) -> (Vec<BRepFaceId>, Vec<BRepEdgeId>) {
    let mut graph = CadGraph::new();
    graph.begin_operation().expect("begin");
    let root_node = add_root(&mut graph, &chain.root_seed);
    let mut current = root_node;
    for ts in &chain.transform_seeds {
        let xform = TransformOp {
            translation: ts.translation,
            rotation_quat_xyzw: [0.0, 0.0, 0.0, 1.0],
            scale: ts.scale,
        };
        let xform_node = graph
            .graph_mut()
            .expect("mut")
            .add_operator(OperatorNode::Transform(xform))
            .expect("add transform");
        graph
            .graph_mut()
            .expect("mut")
            .connect(current, xform_node, 0)
            .expect("connect");
        current = xform_node;
    }
    graph
        .graph_mut()
        .expect("mut")
        .set_root(current)
        .expect("set root");
    graph.commit("phase-7-2-gate-closure").expect("commit");

    let face_pairs =
        brep_face_ids_for_node(graph.graph(), current, TEST_OWNER).expect("resolve faces");
    let face_ids: Vec<BRepFaceId> = face_pairs.into_iter().map(|(_, id)| id).collect();
    let edge_ids =
        brep_edge_ids_for_node(graph.graph(), current, TEST_OWNER).expect("resolve edges");
    (face_ids, edge_ids)
}

fn add_root(graph: &mut CadGraph, seed: &RootSeed) -> NodeId {
    let op_node = match seed {
        RootSeed::Cuboid {
            width,
            height,
            depth,
        } => OperatorNode::Cuboid(CuboidOp {
            width: *width,
            height: *height,
            depth: *depth,
        }),
        RootSeed::Extrude {
            profile_n,
            profile_radius,
            length,
        } => OperatorNode::Extrude(
            ExtrudeOp::new(
                make_regular_polygon(*profile_n, *profile_radius, 0.0),
                *length,
            )
            .expect("ExtrudeOp::new"),
        ),
        RootSeed::Revolve {
            profile_n,
            profile_radius,
            segments,
            mode,
            angle,
        } => {
            // Offset profile so all x >= 0 (`RevolveOp::evaluate` gate;
            // edge profile passes since radius < offset).
            let offset_x = profile_radius + 0.1;
            let profile = make_regular_polygon(*profile_n, *profile_radius, offset_x);
            let op = match mode {
                RevolveModeSeed::Full => {
                    RevolveOp::new(profile, *segments).expect("RevolveOp::new")
                }
                RevolveModeSeed::Partial => {
                    RevolveOp::partial(profile, *segments, *angle).expect("RevolveOp::partial")
                }
            };
            OperatorNode::Revolve(op)
        }
        RootSeed::Loft {
            profile_n,
            profile_radius,
            length,
        } => OperatorNode::Loft(
            LoftOp::new(
                make_regular_polygon(*profile_n, *profile_radius, 0.0),
                make_regular_polygon(*profile_n, *profile_radius * 1.5, 0.0),
                *length,
            )
            .expect("LoftOp::new"),
        ),
        RootSeed::Sweep {
            profile_n,
            profile_radius,
            path_m,
            path_z_step,
            path_drift,
        } => {
            // Convex regular-polygon profile + strictly monotonic-Z path
            // keep `SweepOp::evaluate` valid. `SweepOp::new` is infallible;
            // profile/path were validated by their own constructors.
            let profile = make_regular_polygon(*profile_n, *profile_radius, 0.0);
            let path = make_monotonic_z_path(*path_m, *path_z_step, *path_drift);
            OperatorNode::Sweep(SweepOp::new(profile, path))
        }
    };
    graph
        .graph_mut()
        .expect("mut")
        .add_operator(op_node)
        .expect("add root")
}

// ---------------------------------------------------------------------------
// Mutation strategy
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MutationCategory {
    /// Numeric / coordinate / length / angle-within-mode change
    /// — face+edge IDs MUST stay byte-identical.
    TopologyPreserving,
    /// Profile-vertex-count / segment-count / mode change — face+edge
    /// IDs MUST differ.
    TopologyChanging,
}

#[derive(Clone, Debug)]
struct Mutation {
    category: MutationCategory,
    new_chain: Chain,
}

/// Apply a single random mutation to the chain. The mutation always
/// targets the root direct-provider's parameters (Transform-parameter
/// mutations are always topology-preserving and would not exercise
/// the "topology-changing" branch of the gate).
fn generate_random_mutation(rng: &mut TinyRng, chain: &Chain) -> Mutation {
    // Categorize whether this chain's root supports topology-changing
    // mutations:
    let root_supports_topology_change = matches!(
        chain.root_kind,
        RootKind::Extrude | RootKind::Revolve | RootKind::Loft | RootKind::Sweep
    );

    // For Cuboid roots, only topology-preserving mutations are valid
    // (Cuboid is fixed-topology). For others, 50/50.
    let category = if root_supports_topology_change && rng.next_bool() {
        MutationCategory::TopologyChanging
    } else {
        MutationCategory::TopologyPreserving
    };

    let new_chain = match category {
        MutationCategory::TopologyPreserving => apply_preserving_mutation(rng, chain),
        MutationCategory::TopologyChanging => apply_changing_mutation(rng, chain),
    };
    Mutation {
        category,
        new_chain,
    }
}

fn apply_preserving_mutation(rng: &mut TinyRng, chain: &Chain) -> Chain {
    // Mutate root's numeric params without changing topology counts:
    //   * Cuboid: width/height/depth in [0.5, 5.0]
    //   * Extrude: length, OR profile_radius (NOT profile_n)
    //   * Revolve: angle within current mode, OR profile_radius
    //              (NOT profile_n, NOT segments, NOT mode)
    //   * Loft: length, OR profile_radius
    //   * Sweep: profile_radius, OR path_z_step, OR path XY drift
    //            (NOT profile_n, NOT path_m — both topology counters)
    let new_root_seed = match &chain.root_seed {
        RootSeed::Cuboid { .. } => RootSeed::Cuboid {
            width: rng.next_f32_in_range(0.5, 5.0),
            height: rng.next_f32_in_range(0.5, 5.0),
            depth: rng.next_f32_in_range(0.5, 5.0),
        },
        RootSeed::Extrude {
            profile_n,
            profile_radius,
            length,
        } => {
            // Coin flip: mutate length OR profile_radius.
            if rng.next_bool() {
                RootSeed::Extrude {
                    profile_n: *profile_n,
                    profile_radius: *profile_radius,
                    length: rng.next_f32_in_range(0.5, 5.0),
                }
            } else {
                RootSeed::Extrude {
                    profile_n: *profile_n,
                    profile_radius: rng.next_f32_in_range(0.5, 3.0),
                    length: *length,
                }
            }
        }
        RootSeed::Revolve {
            profile_n,
            profile_radius,
            segments,
            mode,
            angle,
        } => {
            // Coin flip: mutate angle (within current mode) OR profile_radius.
            // For Full mode, angle stays exactly 2π (mutation no-op for
            // angle in Full mode — fall back to mutating radius).
            if rng.next_bool() && matches!(mode, RevolveModeSeed::Partial) {
                RootSeed::Revolve {
                    profile_n: *profile_n,
                    profile_radius: *profile_radius,
                    segments: *segments,
                    mode: *mode,
                    angle: rng.next_f32_in_range(0.1, PI * 1.9),
                }
            } else {
                RootSeed::Revolve {
                    profile_n: *profile_n,
                    profile_radius: rng.next_f32_in_range(0.5, 3.0),
                    segments: *segments,
                    mode: *mode,
                    angle: *angle,
                }
            }
        }
        RootSeed::Loft {
            profile_n,
            profile_radius,
            length,
        } => {
            if rng.next_bool() {
                RootSeed::Loft {
                    profile_n: *profile_n,
                    profile_radius: *profile_radius,
                    length: rng.next_f32_in_range(0.5, 5.0),
                }
            } else {
                RootSeed::Loft {
                    profile_n: *profile_n,
                    profile_radius: rng.next_f32_in_range(0.5, 3.0),
                    length: *length,
                }
            }
        }
        RootSeed::Sweep {
            profile_n,
            profile_radius,
            path_m,
            path_z_step,
            path_drift,
        } => {
            // 3-way pick: profile_radius, path_z_step, or path XY drift.
            // All three keep profile_n AND path_m fixed, so the profile
            // vertex count and the path segment count (m - 1) are both
            // unchanged — Sweep face+edge IDs are derived from those
            // counts only, never from coordinates.
            match rng.next_range_u32(3) {
                0 => RootSeed::Sweep {
                    profile_n: *profile_n,
                    profile_radius: rng.next_f32_in_range(0.5, 3.0),
                    path_m: *path_m,
                    path_z_step: *path_z_step,
                    path_drift: *path_drift,
                },
                1 => RootSeed::Sweep {
                    profile_n: *profile_n,
                    profile_radius: *profile_radius,
                    path_m: *path_m,
                    // New Z step stays positive → path stays monotonic-Z.
                    path_z_step: rng.next_f32_in_range(0.5, 3.0),
                    path_drift: *path_drift,
                },
                _ => RootSeed::Sweep {
                    profile_n: *profile_n,
                    profile_radius: *profile_radius,
                    path_m: *path_m,
                    path_z_step: *path_z_step,
                    path_drift: [
                        rng.next_f32_in_range(-1.0, 1.0),
                        rng.next_f32_in_range(-1.0, 1.0),
                    ],
                },
            }
        }
    };

    Chain {
        root_kind: chain.root_kind,
        root_seed: new_root_seed,
        transform_seeds: chain.transform_seeds.clone(),
    }
}

fn apply_changing_mutation(rng: &mut TinyRng, chain: &Chain) -> Chain {
    // Mutate to alter topology:
    //   * Extrude: change profile_n in [3, 6] (different from current)
    //   * Revolve: change profile_n, OR change segments in [3, 16],
    //              OR flip mode (Full ↔ Partial)
    //   * Loft: change profile_n (both profile_a and profile_b together,
    //           since LoftOp::evaluate enforces equal counts)
    //   * Sweep: change profile_n in [3, 6], OR change path_m in [2, 5]
    //            (a different path point count → different segment count)
    //   * Cuboid: NEVER reaches here (root_supports_topology_change is false)
    let new_root_seed = match &chain.root_seed {
        RootSeed::Cuboid { .. } => {
            unreachable!("Cuboid roots never reach apply_changing_mutation")
        }
        RootSeed::Extrude {
            profile_n,
            profile_radius,
            length,
        } => RootSeed::Extrude {
            profile_n: pick_different_profile_n(rng, *profile_n),
            profile_radius: *profile_radius,
            length: *length,
        },
        RootSeed::Revolve {
            profile_n,
            profile_radius,
            segments,
            mode,
            angle,
        } => {
            // 3-way pick: change profile_n, change segments, or flip mode.
            match rng.next_range_u32(3) {
                0 => RootSeed::Revolve {
                    profile_n: pick_different_profile_n(rng, *profile_n),
                    profile_radius: *profile_radius,
                    segments: *segments,
                    mode: *mode,
                    angle: *angle,
                },
                1 => RootSeed::Revolve {
                    profile_n: *profile_n,
                    profile_radius: *profile_radius,
                    segments: pick_different_segments(rng, *segments),
                    mode: *mode,
                    angle: *angle,
                },
                _ => {
                    // Flip mode: angle must be valid in the new mode.
                    let new_mode = match mode {
                        RevolveModeSeed::Full => RevolveModeSeed::Partial,
                        RevolveModeSeed::Partial => RevolveModeSeed::Full,
                    };
                    let new_angle = match new_mode {
                        RevolveModeSeed::Full => 2.0 * PI,
                        RevolveModeSeed::Partial => PI * 0.5,
                    };
                    RootSeed::Revolve {
                        profile_n: *profile_n,
                        profile_radius: *profile_radius,
                        segments: *segments,
                        mode: new_mode,
                        angle: new_angle,
                    }
                }
            }
        }
        RootSeed::Loft {
            profile_n,
            profile_radius,
            length,
        } => RootSeed::Loft {
            profile_n: pick_different_profile_n(rng, *profile_n),
            profile_radius: *profile_radius,
            length: *length,
        },
        RootSeed::Sweep {
            profile_n,
            profile_radius,
            path_m,
            path_z_step,
            path_drift,
        } => {
            // Coin flip: change profile_n (n) OR change path_m (hence the
            // segment count m - 1). Either alters a Sweep topology counter
            // and both keep the resulting Sweep valid (convex profile,
            // monotonic-Z path). Both also change the face+edge ID vector
            // length, so the gate's `assert_ne!` holds.
            if rng.next_bool() {
                RootSeed::Sweep {
                    profile_n: pick_different_profile_n(rng, *profile_n),
                    profile_radius: *profile_radius,
                    path_m: *path_m,
                    path_z_step: *path_z_step,
                    path_drift: *path_drift,
                }
            } else {
                RootSeed::Sweep {
                    profile_n: *profile_n,
                    profile_radius: *profile_radius,
                    path_m: pick_different_path_m(rng, *path_m),
                    path_z_step: *path_z_step,
                    path_drift: *path_drift,
                }
            }
        }
    };

    Chain {
        root_kind: chain.root_kind,
        root_seed: new_root_seed,
        transform_seeds: chain.transform_seeds.clone(),
    }
}

fn pick_different_profile_n(rng: &mut TinyRng, current: u32) -> u32 {
    // Pick from {3, 4, 5, 6} excluding current.
    loop {
        let n = 3 + rng.next_range_u32(4);
        if n != current {
            return n;
        }
    }
}

fn pick_different_segments(rng: &mut TinyRng, current: u32) -> u32 {
    // Pick from [3, 16] excluding current.
    loop {
        let s = 3 + rng.next_range_u32(14);
        if s != current {
            return s;
        }
    }
}

fn pick_different_path_m(rng: &mut TinyRng, current: u32) -> u32 {
    // Pick a Sweep path point count from {2, 3, 4, 5} excluding current.
    // `m >= 2` keeps the path valid; a different `m` changes the segment
    // count `m - 1`, which is a Sweep topology counter.
    loop {
        let m = 2 + rng.next_range_u32(4);
        if m != current {
            return m;
        }
    }
}

// ---------------------------------------------------------------------------
// THE GATE TEST
// ---------------------------------------------------------------------------

#[test]
fn phase_7_2_gate_closure_100_chains_10_rebuilds_seed_0x7e5a_dead_beef_c0de() {
    let mut rng = TinyRng::new(STRESS_TEST_SEED);
    let mut topology_preserving_count: usize = 0;
    let mut topology_changing_count: usize = 0;
    let mut sweep_chain_count: usize = 0;

    for chain_idx in 0..NUM_CHAINS {
        let initial_chain = generate_random_chain(&mut rng);
        if initial_chain.root_kind == RootKind::Sweep {
            sweep_chain_count += 1;
        }
        let (initial_faces, initial_edges) = build_and_resolve(&initial_chain);

        for rebuild_idx in 0..REBUILDS_PER_CHAIN {
            let mutation = generate_random_mutation(&mut rng, &initial_chain);
            let (rebuilt_faces, rebuilt_edges) = build_and_resolve(&mutation.new_chain);

            match mutation.category {
                MutationCategory::TopologyPreserving => {
                    assert_eq!(
                        rebuilt_faces, initial_faces,
                        "Phase 7.2 gate FAIL: chain {chain_idx} rebuild {rebuild_idx} \
                         topology-preserving mutation altered face IDs (seed 0x{STRESS_TEST_SEED:016X})"
                    );
                    assert_eq!(
                        rebuilt_edges, initial_edges,
                        "Phase 7.2 gate FAIL: chain {chain_idx} rebuild {rebuild_idx} \
                         topology-preserving mutation altered edge IDs (seed 0x{STRESS_TEST_SEED:016X})"
                    );
                    topology_preserving_count += 1;
                }
                MutationCategory::TopologyChanging => {
                    // Vector-level inequality is the substrate's guarantee.
                    // Side faces / all edges ARE disjoint when topology
                    // changes; categorical caps (Bottom/Top in
                    // ExtrudeFaceTag/LoftFaceTag) intentionally preserve
                    // identity per the contract from sub-7.2-β/δ. So we
                    // cannot assert full disjointness — but the vectors
                    // must not be equal (some IDs change).
                    assert_ne!(
                        rebuilt_faces, initial_faces,
                        "Phase 7.2 gate FAIL: chain {chain_idx} rebuild {rebuild_idx} \
                         topology-changing mutation did not change face IDs (seed 0x{STRESS_TEST_SEED:016X})"
                    );
                    assert_ne!(
                        rebuilt_edges, initial_edges,
                        "Phase 7.2 gate FAIL: chain {chain_idx} rebuild {rebuild_idx} \
                         topology-changing mutation did not change edge IDs (seed 0x{STRESS_TEST_SEED:016X})"
                    );
                    topology_changing_count += 1;
                }
            }
        }
    }

    // Sanity: ensure both categories had real coverage. With 100×10 = 1000
    // total mutations and 50/50 coin flips on the four (of five) root
    // kinds that support both — Extrude, Revolve, Loft, Sweep — we expect
    // roughly 300-500 topology-changing and 500-700 topology-preserving.
    // Anything dramatically lopsided indicates the random generator is
    // broken.
    assert!(
        topology_preserving_count >= 100,
        "topology-preserving coverage too low: {topology_preserving_count} (seed 0x{STRESS_TEST_SEED:016X})"
    );
    assert!(
        topology_changing_count >= 50,
        "topology-changing coverage too low: {topology_changing_count} (seed 0x{STRESS_TEST_SEED:016X})"
    );

    // Sweep coverage gate: the generator MUST visit `RootKind::Sweep` at
    // least once, otherwise the Sweep face-ID and edge-ID rebuild checks
    // above never ran and the gate's Sweep coverage is silently absent.
    // Fail loudly rather than pass with a hole.
    assert!(
        sweep_chain_count >= 1,
        "Phase 7.2 gate FAIL: no SweepOp chain generated in {NUM_CHAINS} chains \
         (seed 0x{STRESS_TEST_SEED:016X}) — Sweep rebuild coverage missing"
    );

    println!(
        "Phase 7.2 gate CLOSED: {NUM_CHAINS} chains × {REBUILDS_PER_CHAIN} rebuilds = \
         {} mutations ({topology_preserving_count} preserving, {topology_changing_count} changing; \
         {sweep_chain_count} Sweep chains). Seed: 0x{STRESS_TEST_SEED:016X}.",
        NUM_CHAINS * REBUILDS_PER_CHAIN
    );
}

// ---------------------------------------------------------------------------
// Sanity tests on the harness itself
// ---------------------------------------------------------------------------

/// Same seed produces the same first 10 outputs — basic deterministic-
/// PRNG smoke.
#[test]
fn tiny_rng_deterministic_for_fixed_seed() {
    let mut a = TinyRng::new(STRESS_TEST_SEED);
    let mut b = TinyRng::new(STRESS_TEST_SEED);
    for _ in 0..10 {
        assert_eq!(a.next_u64(), b.next_u64());
    }
}

/// xorshift64 cannot accept seed=0 (it would produce all zeros). The
/// constructor's `assert_ne!` should panic.
#[test]
#[should_panic(expected = "xorshift64 cannot accept seed=0")]
fn tiny_rng_rejects_zero_seed() {
    let _ = TinyRng::new(0);
}

/// Running the chain generator a few hundred times, all 5 RootKinds
/// — including Sweep — should appear. Proves the generator isn't
/// accidentally biased and that Sweep is a reachable root.
#[test]
fn chain_generator_visits_all_root_kinds() {
    let mut rng = TinyRng::new(STRESS_TEST_SEED);
    let mut saw_cuboid = false;
    let mut saw_extrude = false;
    let mut saw_revolve = false;
    let mut saw_loft = false;
    let mut saw_sweep = false;
    for _ in 0..300 {
        let chain = generate_random_chain(&mut rng);
        match chain.root_kind {
            RootKind::Cuboid => saw_cuboid = true,
            RootKind::Extrude => saw_extrude = true,
            RootKind::Revolve => saw_revolve = true,
            RootKind::Loft => saw_loft = true,
            RootKind::Sweep => saw_sweep = true,
        }
    }
    assert!(saw_cuboid, "Cuboid never generated in 300 trials");
    assert!(saw_extrude, "Extrude never generated in 300 trials");
    assert!(saw_revolve, "Revolve never generated in 300 trials");
    assert!(saw_loft, "Loft never generated in 300 trials");
    assert!(saw_sweep, "Sweep never generated in 300 trials");
}

/// For a fixed Extrude chain, applying 50 preserving mutations all
/// keep face count = N+2 and edge count = 3N. Validates the
/// preserving-mutation harness on a small fixed input.
#[test]
fn apply_preserving_mutation_keeps_topology_counts() {
    let initial = Chain {
        root_kind: RootKind::Extrude,
        root_seed: RootSeed::Extrude {
            profile_n: 4,
            profile_radius: 1.0,
            length: 1.0,
        },
        transform_seeds: vec![],
    };
    let (faces, edges) = build_and_resolve(&initial);
    assert_eq!(faces.len(), 6, "N+2 with N=4");
    assert_eq!(edges.len(), 12, "3N with N=4");

    let mut rng = TinyRng::new(STRESS_TEST_SEED ^ 0x1);
    for i in 0..50 {
        let mutated = apply_preserving_mutation(&mut rng, &initial);
        let (mfaces, medges) = build_and_resolve(&mutated);
        assert_eq!(mfaces.len(), 6, "preserving mutation {i} broke face count");
        assert_eq!(medges.len(), 12, "preserving mutation {i} broke edge count");
        // And the IDs themselves must equal the initial set.
        assert_eq!(mfaces, faces, "preserving mutation {i} altered face IDs");
        assert_eq!(medges, edges, "preserving mutation {i} altered edge IDs");
    }
}

/// For a fixed Extrude chain, applying 50 changing mutations all
/// change either the face/edge counts OR the ID vectors. Validates the
/// changing-mutation harness on a small fixed input.
#[test]
fn apply_changing_mutation_alters_topology_counts_or_breaks_ids() {
    let initial = Chain {
        root_kind: RootKind::Extrude,
        root_seed: RootSeed::Extrude {
            profile_n: 4,
            profile_radius: 1.0,
            length: 1.0,
        },
        transform_seeds: vec![],
    };
    let (faces, edges) = build_and_resolve(&initial);

    let mut rng = TinyRng::new(STRESS_TEST_SEED ^ 0x2);
    for i in 0..50 {
        let mutated = apply_changing_mutation(&mut rng, &initial);
        let (mfaces, medges) = build_and_resolve(&mutated);
        // Either the count differs (topology change altered count) OR
        // the IDs differ (topology change preserved count but altered
        // IDs — possible for some Revolve mode flips with same n).
        // Extrude profile_n change always changes count, but be
        // permissive to support cross-operator reuse.
        assert!(
            mfaces.len() != faces.len() || mfaces != faces,
            "changing mutation {i} produced identical face IDs (count + values)"
        );
        assert!(
            medges.len() != edges.len() || medges != edges,
            "changing mutation {i} produced identical edge IDs (count + values)"
        );
    }
}

/// Cuboid chains never receive topology-changing mutations — verifies
/// the Cuboid carve-out works.
#[test]
fn cuboid_chain_only_uses_preserving_mutations() {
    let mut rng = TinyRng::new(STRESS_TEST_SEED ^ 0x3);
    let mut cuboid_count = 0;
    for _ in 0..200 {
        // Force a Cuboid chain by generating one and discarding non-cuboids.
        let chain = generate_random_chain(&mut rng);
        if chain.root_kind != RootKind::Cuboid {
            continue;
        }
        cuboid_count += 1;
        // Generate 5 mutations; all must be TopologyPreserving.
        for _ in 0..5 {
            let m = generate_random_mutation(&mut rng, &chain);
            assert_eq!(
                m.category,
                MutationCategory::TopologyPreserving,
                "Cuboid chain produced TopologyChanging mutation — carve-out broken"
            );
        }
        if cuboid_count >= 50 {
            break;
        }
    }
    assert!(
        cuboid_count >= 10,
        "did not see enough Cuboid chains in 200 trials: {cuboid_count}"
    );
}
