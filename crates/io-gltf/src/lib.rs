// adapted from rustforge::crates::io-gltf on 2026-05-05 — re-targeted to rge asset-store::Cache trait
//! `rge-io-gltf` — glTF 2.0 import + export.
//!
//! Failure class: recoverable
//!
//! Per PLAN §1.13: glTF import/export failures (malformed `.glb` magic,
//! unsupported accessor type, missing buffer, JSON serde error, schema
//! constraint violation) are transient and recoverable in-place — the
//! caller surfaces the error to the user, retries with a different file,
//! or skips the asset. No PIE state is owned by io-gltf itself; it's a
//! stateless format adapter. Matches pak-format + io-image + asset-store
//! (transient I/O / parse failures).
//!
//! Phase-4 deliverable per [`PLAN.md`](../../plans/PLAN.md) §1.6.4. CI lint
//! enforces this is the **only** path through which the workspace touches the
//! `gltf` crate (one-import-path-per-format rule, §1.6.5).
//!
//! # Surfaces
//!
//! - [`import_glb`] — load a `.glb` (binary glTF) → an in-memory [`Scene`]
//!   plus inserts mesh / material / animation / skeleton assets into the
//!   supplied [`Cache`].
//! - [`export_glb`] — reverse: serialise a [`Scene`] (resolved against a
//!   [`Cache`]) to a `.glb` byte vector. Used by the editor "Export glTF"
//!   command.
//!
//! # Stub policy (W17 vs W01/W14/W16)
//!
//! W17 ships ahead of (or parallel with) W14 (`rge-data`), W16 (`asset-store`)
//! and parts of W01 (`components-render` / `components-animation`). Per the
//! W17 dispatch package, the four types listed below are **local stubs**
//! inside this crate; when those waves merge, downstream callers will replace
//! the imports — the public surface (`Scene`, `Cache`, the four `*Handle`
//! types and the asset structs) stays shape-compatible.
//!
//! - [`Cache`] — in-place trait stub for `rge-asset-store::Cache`.
//! - [`MeshHandle`], [`MaterialHandle`], [`AnimationHandle`],
//!   [`SkeletonHandle`] — content-hashed handles, structurally identical to
//!   the `components-render` / `components-animation` types W01 will export.
//! - [`Scene`] — temporary stand-in for the `rge-data::Scene` type W14 will
//!   define (entities + components-by-id table). Keeps W17's public API
//!   stable across the W14 merge.
//!
//! # Round-trip guarantee
//!
//! `import_glb → export_glb → import_glb` produces a [`Scene`] equivalent to
//! the original within an explicit tolerance:
//!
//! - vertex / triangle counts match exactly,
//! - material PBR parameters round-trip within `1e-5`,
//! - transforms (TRS) round-trip within `1e-5`.
//!
//! See `tests/cube_round_trip.rs`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
// Pedantic-level lints flag the importer/exporter style that's intrinsic
// to a binary-format codec — strict f32 array compares (we *want* exact
// bit equality on default-fast-path), `usize`→`u32` casts (well-bounded by
// glTF 2.0 spec ≤ 2³² accessor counts), match-arm bodies that look
// identical because they parameterise on type but not action, and `# Errors`
// stanzas on every `Result`-returning fn (we use `thiserror` which already
// documents the error variants). Allow them at the crate level rather than
// papering each call site.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::float_cmp,
    clippy::float_arithmetic,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::too_many_lines,
    clippy::match_same_arms,
    clippy::similar_names,
    clippy::needless_pass_by_value,
    clippy::map_unwrap_or
)]

pub mod animation;
pub mod export;
pub mod image;
pub mod import;
pub mod material;
pub mod mesh;
pub mod scene_builder;
pub mod skeleton;

// Local stubs (replace when W01 / W14 land — see crate-level docs). W16's
// `rge-asset-store::Cache` is reached through the opt-in
// [`AssetStoreCache`] adapter rather than by replacing `cache_stub`.
mod asset_store_cache;
mod cache_stub;
mod handles;
mod scene_stub;

pub use animation::{
    extract_animations, AnimationClip, AnimationSampler, BoneChannel, Interpolation,
};
pub use asset_store_cache::AssetStoreCache;
pub use cache_stub::{Cache, MemoryCache};
pub use export::{export_glb, export_glb_to_file};
pub use handles::{AnimationHandle, ImageHandle, MaterialHandle, MeshHandle, SkeletonHandle};
pub use image::{extract_images, ImageAsset};
pub use import::{import_glb, import_glb_bytes};
pub use material::{extract_materials, AlphaMode, MaterialAsset};
pub use mesh::{extract_meshes, MeshAsset, Primitive};
// Dispatch M2 — re-export `PixelFormat` so editor-tier consumers can
// match on the decoded image's storage format without taking a
// direct `rge-io-image` dep. The glTF importer's public surface is
// the single boundary downstream callers reach for; the
// pixel-format enum belongs alongside `ImageAsset` here.
pub use rge_io_image::PixelFormat;
pub use scene_builder::build_scene;
pub use scene_stub::{Entity, EntityComponents, Scene, Transform};
pub use skeleton::{extract_skeletons, Skeleton};
use thiserror::Error;

/// Errors surfaced by the importer / exporter.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum GltfError {
    /// I/O failure (file open, read, write).
    #[error("io: {0}")]
    Io(String),
    /// Underlying `gltf` crate parser surfaced an error.
    #[error("gltf parse: {0}")]
    Parse(String),
    /// Document violates a constraint we enforce (e.g. missing buffer, bad
    /// accessor type, primitive without POSITION).
    #[error("schema: {0}")]
    Schema(String),
    /// JSON layer error during export.
    #[error("json: {0}")]
    Json(String),
    /// Failure surfaced by the opt-in [`AssetStoreCache`] adapter when
    /// the underlying `dyn rge_asset_store::Cache` reports an I/O or
    /// asset-id error. Reserved for asset-store-backed cache failures —
    /// ordinary file / parse / schema / json errors keep their existing
    /// variants.
    #[error("cache: {0}")]
    Cache(String),
}

impl From<std::io::Error> for GltfError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

impl From<gltf::Error> for GltfError {
    fn from(e: gltf::Error) -> Self {
        Self::Parse(e.to_string())
    }
}

impl From<serde_json::Error> for GltfError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e.to_string())
    }
}
