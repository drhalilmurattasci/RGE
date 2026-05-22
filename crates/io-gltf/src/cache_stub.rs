// adapted from rustforge::crates::io-gltf on 2026-05-05 — re-targeted to rge asset-store::Cache trait
//! Local typed asset cache for io-gltf — the default `crate::Cache` /
//! `crate::MemoryCache` surface used by every existing editor, loader,
//! importer, exporter, and integration-test call site.
//!
//! The trait keeps its own typed shape: a content-addressed insert per
//! asset family that returns a typed hash handle, plus a borrowed-get
//! lookup keyed by handle. The five asset families (mesh / material /
//! animation / skeleton / image) each get their own get/insert pair
//! because their stored types differ — a single `Any`-typed cache would
//! erase the type information `export.rs` needs at borrow-and-iterate
//! sites.
//!
//! **W16 policy (ISSUE-94):** this file is not deleted and `crate::Cache`
//! is not turned into a trait re-export of `rge_asset_store::Cache`. The
//! local typed trait + `MemoryCache` remain the public contract for
//! io-gltf. Callers that want persistent byte backing opt in explicitly
//! via [`crate::AssetStoreCache`], the bridge defined in
//! `asset_store_cache.rs`, which forwards storage through a caller-
//! supplied `dyn rge_asset_store::Cache` while preserving this trait's
//! existing infallible borrowed-get shape.

use crate::animation::AnimationClip;
use crate::handles::{AnimationHandle, ImageHandle, MaterialHandle, MeshHandle, SkeletonHandle};
use crate::image::ImageAsset;
use crate::material::MaterialAsset;
use crate::mesh::MeshAsset;
use crate::skeleton::Skeleton;

/// Content-addressed asset cache.
///
/// Insert returns a handle keyed by the blake3 hash of the asset's
/// canonical-byte form. Look-ups are O(1). All four asset families share the
/// same hashing rule, so handle collisions across families are
/// vanishingly improbable but could in theory happen — call sites are
/// expected to use the typed accessor matching the asset family.
pub trait Cache {
    /// Insert a mesh asset; returns the content-hash handle.
    fn insert_mesh(&mut self, asset: MeshAsset) -> MeshHandle;
    /// Look up a mesh by handle.
    fn get_mesh(&self, h: &MeshHandle) -> Option<&MeshAsset>;

    /// Insert a material asset; returns the content-hash handle.
    fn insert_material(&mut self, asset: MaterialAsset) -> MaterialHandle;
    /// Look up a material by handle.
    fn get_material(&self, h: &MaterialHandle) -> Option<&MaterialAsset>;

    /// Insert an animation clip; returns the content-hash handle.
    fn insert_animation(&mut self, clip: AnimationClip) -> AnimationHandle;
    /// Look up an animation clip by handle.
    fn get_animation(&self, h: &AnimationHandle) -> Option<&AnimationClip>;

    /// Insert a skeleton; returns the content-hash handle.
    fn insert_skeleton(&mut self, skel: Skeleton) -> SkeletonHandle;
    /// Look up a skeleton by handle.
    fn get_skeleton(&self, h: &SkeletonHandle) -> Option<&Skeleton>;

    /// Dispatch L — insert an image asset; returns the content-hash
    /// handle. Provided as a default-method-style addition with no
    /// `default` body so existing custom `Cache` impls don't silently
    /// regress (none exist outside `MemoryCache` today; this trait is
    /// the W17 stub for the future W16 `rge-asset-store::Cache`).
    fn insert_image(&mut self, asset: ImageAsset) -> ImageHandle;
    /// Dispatch L — look up an image by handle.
    fn get_image(&self, h: &ImageHandle) -> Option<&ImageAsset>;
}

/// Reference [`Cache`] impl: HashMap-backed in-memory store. Used by the
/// crate's own tests and by callers that don't need W16's persistent
/// disk-backed cache.
#[derive(Debug, Default, Clone)]
pub struct MemoryCache {
    meshes: std::collections::HashMap<MeshHandle, MeshAsset>,
    materials: std::collections::HashMap<MaterialHandle, MaterialAsset>,
    animations: std::collections::HashMap<AnimationHandle, AnimationClip>,
    skeletons: std::collections::HashMap<SkeletonHandle, Skeleton>,
    /// Dispatch L — decoded image bytes keyed by content hash.
    images: std::collections::HashMap<ImageHandle, ImageAsset>,
}

impl MemoryCache {
    /// Construct an empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of cached meshes.
    #[must_use]
    pub fn mesh_count(&self) -> usize {
        self.meshes.len()
    }

    /// Number of cached materials.
    #[must_use]
    pub fn material_count(&self) -> usize {
        self.materials.len()
    }

    /// Number of cached animation clips.
    #[must_use]
    pub fn animation_count(&self) -> usize {
        self.animations.len()
    }

    /// Number of cached skeletons.
    #[must_use]
    pub fn skeleton_count(&self) -> usize {
        self.skeletons.len()
    }

    /// Dispatch L — number of cached images.
    #[must_use]
    pub fn image_count(&self) -> usize {
        self.images.len()
    }
}

impl Cache for MemoryCache {
    fn insert_mesh(&mut self, asset: MeshAsset) -> MeshHandle {
        let h = asset.content_hash();
        self.meshes.entry(h).or_insert(asset);
        h
    }
    fn get_mesh(&self, h: &MeshHandle) -> Option<&MeshAsset> {
        self.meshes.get(h)
    }

    fn insert_material(&mut self, asset: MaterialAsset) -> MaterialHandle {
        let h = asset.content_hash();
        self.materials.entry(h).or_insert(asset);
        h
    }
    fn get_material(&self, h: &MaterialHandle) -> Option<&MaterialAsset> {
        self.materials.get(h)
    }

    fn insert_animation(&mut self, clip: AnimationClip) -> AnimationHandle {
        let h = clip.content_hash();
        self.animations.entry(h).or_insert(clip);
        h
    }
    fn get_animation(&self, h: &AnimationHandle) -> Option<&AnimationClip> {
        self.animations.get(h)
    }

    fn insert_skeleton(&mut self, skel: Skeleton) -> SkeletonHandle {
        let h = skel.content_hash();
        self.skeletons.entry(h).or_insert(skel);
        h
    }
    fn get_skeleton(&self, h: &SkeletonHandle) -> Option<&Skeleton> {
        self.skeletons.get(h)
    }

    fn insert_image(&mut self, asset: ImageAsset) -> ImageHandle {
        let h = asset.content_hash();
        self.images.entry(h).or_insert(asset);
        h
    }
    fn get_image(&self, h: &ImageHandle) -> Option<&ImageAsset> {
        self.images.get(h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_cache_counts_zero() {
        let c = MemoryCache::new();
        assert_eq!(c.mesh_count(), 0);
        assert_eq!(c.material_count(), 0);
        assert_eq!(c.animation_count(), 0);
        assert_eq!(c.skeleton_count(), 0);
    }

    #[test]
    fn insert_mesh_dedupes() {
        let mut c = MemoryCache::new();
        let a = MeshAsset {
            positions: vec![[0.0, 0.0, 0.0]],
            normals: vec![[0.0, 1.0, 0.0]],
            texcoords: vec![],
            indices: vec![0],
            material_index: None,
        };
        let b = a.clone();
        let h1 = c.insert_mesh(a);
        let h2 = c.insert_mesh(b);
        assert_eq!(h1, h2);
        assert_eq!(c.mesh_count(), 1);
    }

    #[test]
    fn cache_insert_image_get_image_roundtrip() {
        let mut c = MemoryCache::new();
        let asset = ImageAsset::from_inner(rge_io_image::Image::from_rgba8(
            2,
            2,
            vec![
                255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 255,
            ],
        ));
        let h = c.insert_image(asset.clone());
        assert_eq!(c.image_count(), 1);
        let fetched = c.get_image(&h).expect("get_image");
        assert_eq!(fetched.width(), 2);
        assert_eq!(fetched.height(), 2);
        assert_eq!(fetched.pixels(), asset.pixels());

        // Re-insert the same asset — handle is stable, count unchanged.
        let h2 = c.insert_image(asset);
        assert_eq!(h, h2);
        assert_eq!(c.image_count(), 1);
    }
}
