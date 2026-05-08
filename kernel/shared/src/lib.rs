//! `rge-kernel-shared` — Tier-1 kernel substrate: shared cross-kernel utilities (deliberately minimal).
//!
//! Cross-references `plans/fileandfolderstructure.md` §95-100 (the
//! suspect-by-default discipline this crate operationalizes) and PLAN.md
//! §1.3 Rule 3 (the no-utils-files rule which this crate operationalizes
//! at folder granularity rather than file granularity).
//!
//! Failure class: recoverable
//!
//! # What this crate is
//!
//! `kernel/shared` is the suspect-by-default folder for cross-kernel
//! utilities. Per `fileandfolderstructure.md` §95-100, it exists so the
//! workspace has a designated home for primitives that cannot live in a
//! specific kernel crate — and the existence of that home is structurally
//! gated on demonstrated cross-kernel duplication. Entry is gated by
//! observed duplication of a type or function across two or more
//! implemented kernel crates that cannot be removed by relocating to one
//! of those crates. Nothing currently lives here because no such
//! duplication has surfaced. Empty is the substrate's success state, not
//! its provisional state.
//!
//! # Admission gate
//!
//! Adding ANY public item to this crate (`pub struct`, `pub enum`,
//! `pub trait`, `pub fn`, `pub use`, `pub mod`, `pub const`) requires
//! ALL of the following before the change lands:
//!
//! 1. The type or function is already duplicated across **≥ 2 implemented
//!    kernel crates** at the time of the proposal. Anticipated future
//!    duplication does NOT qualify; only observed present duplication.
//! 2. A written justification — at ADR-level rigor, in `docs/adr/` —
//!    arguing why **neither owning kernel crate** is the natural home for
//!    the duplicated item. The default disposition is to relocate the
//!    duplicate into the kernel that already has the strongest semantic
//!    claim, NOT to move it here. Choosing `kernel/shared` over
//!    relocation is the load-bearing decision the ADR must justify.
//! 3. Sign-off explicitly noting that the addition expands
//!    `kernel/shared`'s public surface, because doing so is structurally
//!    suspect by the contract above.
//!
//! Anything weaker — convenience, anticipation, taxonomical neatness,
//! "it might be useful later", "it would clean up imports" — is rejected
//! by the admission gate. The gate exists because the alternative is
//! that `kernel/shared` becomes a `utils.rs`-as-crate, which is the
//! exact failure mode PLAN.md §1.3 Rule 3 prohibits at file granularity
//! and which this crate prohibits at folder granularity.
//!
//! # NON-GOALS
//!
//! v0 establishes the admission contract and the failure-class
//! declaration. It deliberately does NOT introduce any public surface.
//! The strongest part of v0 is the list of what `kernel/shared`
//! intentionally is **not**, even at v0.x or v1.0:
//!
//! - Not a utils crate. The whole point of the suspect-by-default
//!   discipline is that this is NOT where utilities go by default.
//!   Utilities go in the kernel crate that owns them.
//! - Not a marker-trait registry. Marker traits and trait-based
//!   discriminants for cross-kernel reflection live in the crate that
//!   defines the domain (e.g. `kernel/types`, `kernel/ecs`), not here.
//! - Not a math primitives crate. `math/` lives at Tier 2 per
//!   `fileandfolderstructure.md` L148 (`crates/math/`); shared math
//!   primitives are a Tier-2 concern.
//! - Not an error-types crate. Cross-crate error types live at Tier 2
//!   per `fileandfolderstructure.md` L149 (`crates/errors/`); kernel
//!   error types live in the kernel that produces them.
//! - Not a re-export hub for other kernels. Re-exporting `kernel/diagnostics`
//!   types or `kernel/ecs` types from here would create a parallel-authority
//!   surface against those crates' own public APIs and would directly violate
//!   the [`rge_authority_fragmentation_risk`](../../../) principle that
//!   each substrate has exactly one canonical owner.
//! - Not a feature-flag dispatch surface. Conditional-compilation
//!   surfaces live in the consuming crate so the surface and the
//!   feature flags are local to the same crate's `Cargo.toml`.
//! - Not a Tier-2 dependency target. Per PLAN §1.8 Tier-1 isolation,
//!   Tier-2 crates depend on the specific kernel crate that owns the
//!   substrate they need. They do not depend on `kernel/shared` to pull
//!   in incidental utility types.
//! - Not a place to park "future" types ahead of demonstrated need.
//!   Speculative substrate is the gravity well that turns a suspect-by-
//!   default folder into a `utils.rs`-as-crate. Types land here when
//!   the duplication is observed across implemented kernels, not before.
//! - Not a substrate for ADR-shaped governance content. ADRs and
//!   doctrine documents live in `docs/adr/` and `docs/architecture/`.
//!   `kernel/shared` does not host governance prose; it operationalizes
//!   the governance contract that those documents establish elsewhere.
//! - Not a test-fixture crate. Test fixtures live alongside the code
//!   they exercise, in the consuming crate's own `tests/` or
//!   `src/.../tests.rs` foot. Cross-crate fixtures are a Tier-2 concern
//!   when they emerge.
//! - Not an "every kernel imports this" crate. The graph of kernel-to-
//!   kernel imports stays sparse by design; `kernel/shared` does NOT
//!   become a hub that other Tier-1 crates import as a matter of course.
//! - No new architecture lint, no new ADR, no new doctrine doc, no new
//!   §18 companion, no new taxonomy, no new validation framework.
//!   The doctrine IS the substrate; expanding the doctrinal surface
//!   would itself violate the suspect-by-default discipline this crate
//!   codifies.
//!
//! # Why empty is correct
//!
//! Empty is the substrate's success state. As long as no item legitimately
//! satisfies the admission gate above, `kernel/shared` MUST stay empty.
//! The moment something does justify entry, that addition will require
//! explicit ADR sign-off precisely because adding to `kernel/shared` is
//! structurally suspect. The contract scales: the more disciplined the
//! workspace is at locating utilities in their owning kernel crates, the
//! more this crate stays empty, and the more its emptiness becomes an
//! enforced architectural property rather than a placeholder waiting to
//! be filled.
