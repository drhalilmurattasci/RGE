//! ISSUE-94 — opt-in adapter that backs the local typed
//! [`crate::Cache`] surface with a `dyn rge_asset_store::Cache` byte
//! store.
//!
//! This bridge intentionally **does not replace** [`crate::MemoryCache`]:
//! every existing editor / loader / importer / exporter / integration
//! test still uses `MemoryCache` (the default). Callers that want a
//! persistent or otherwise byte-oriented backing opt in by constructing
//! [`AssetStoreCache`] with a `Box<dyn rge_asset_store::Cache>` (e.g.
//! [`rge_asset_store::LocalCache`] for filesystem persistence or
//! [`rge_asset_store::InMemoryCache`] for unit tests).
//!
//! ## Shape mismatch resolution
//!
//! The io-gltf `Cache` trait stores typed assets and returns borrowed
//! `Option<&T>`, while `rge_asset_store::Cache` stores fallible owned
//! byte blobs and returns owned `Vec<u8>`. The adapter keeps a small
//! typed mirror per asset family so the existing borrowed-get lifetime
//! shape continues to compile unchanged. The same insert path *also*
//! writes a content-addressed byte form through the backing cache, so
//! BLAKE3-keyed dedup and persistence both happen through the
//! asset-store seam.
//!
//! ## Error surfacing
//!
//! The existing trait `crate::Cache` is intentionally infallible. To
//! keep that contract while still letting callers learn about
//! asset-store I/O failures, the adapter offers an additive fallible
//! family ([`try_insert_mesh`](AssetStoreCache::try_insert_mesh), etc.)
//! that surfaces a [`CacheError`] as
//! [`GltfError::Cache`]. The infallible trait methods still attempt the
//! backing write but discard the error — the typed mirror update always
//! succeeds, so the trait contract (insert returns a handle, get
//! returns the asset) holds even when the backing fails.

use std::collections::HashMap;

use rge_asset_store::{AssetId, Bytes, Cache as ByteCache, CacheError};

use crate::animation::AnimationClip;
use crate::cache_stub::Cache;
use crate::handles::{AnimationHandle, ImageHandle, MaterialHandle, MeshHandle, SkeletonHandle};
use crate::image::ImageAsset;
use crate::material::MaterialAsset;
use crate::mesh::MeshAsset;
use crate::skeleton::Skeleton;
use crate::GltfError;

/// Opt-in adapter that implements [`crate::Cache`] on top of a caller-
/// supplied `dyn rge_asset_store::Cache` byte store.
///
/// The adapter keeps a typed mirror for each asset family so the
/// infallible borrowed-get shape of [`crate::Cache`] is preserved
/// without changes. Inserts also push a content-addressed canonical
/// byte form through the backing cache, so byte-level dedup happens
/// through the asset-store seam.
pub struct AssetStoreCache {
    backing: Box<dyn ByteCache>,
    meshes: HashMap<MeshHandle, MeshAsset>,
    materials: HashMap<MaterialHandle, MaterialAsset>,
    animations: HashMap<AnimationHandle, AnimationClip>,
    skeletons: HashMap<SkeletonHandle, Skeleton>,
    images: HashMap<ImageHandle, ImageAsset>,
}

impl std::fmt::Debug for AssetStoreCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AssetStoreCache")
            .field("meshes", &self.meshes.len())
            .field("materials", &self.materials.len())
            .field("animations", &self.animations.len())
            .field("skeletons", &self.skeletons.len())
            .field("images", &self.images.len())
            .field("backing_len", &self.backing.len())
            .finish()
    }
}

impl AssetStoreCache {
    /// Construct an adapter that forwards storage through `backing`.
    ///
    /// Accepts any concrete `rge_asset_store::Cache` impl (e.g.
    /// `LocalCache`, `InMemoryCache`, or a test double).
    pub fn new<C: ByteCache + 'static>(backing: C) -> Self {
        Self::from_box(Box::new(backing))
    }

    /// Construct an adapter from an already-boxed backing cache.
    ///
    /// Useful when the backing is selected dynamically (e.g. behind a
    /// CLI flag or config).
    #[must_use]
    pub fn from_box(backing: Box<dyn ByteCache>) -> Self {
        Self {
            backing,
            meshes: HashMap::new(),
            materials: HashMap::new(),
            animations: HashMap::new(),
            skeletons: HashMap::new(),
            images: HashMap::new(),
        }
    }

    /// Borrow the underlying byte cache. Tests probe this seam to
    /// assert that dedup and content addressing happened through the
    /// asset-store backing rather than only the typed mirror.
    #[must_use]
    pub fn backing(&self) -> &dyn ByteCache {
        &*self.backing
    }

    /// [`AssetId`] of the canonical bytes corresponding to a
    /// [`MeshHandle`]. The adapter's canonical byte form is chosen so
    /// `AssetId::from_bytes(mesh_canonical_bytes(asset)) ==
    /// AssetId::from_raw(asset.content_hash().0)`, which means the
    /// asset-store entry and the typed handle share their BLAKE3 hash.
    #[must_use]
    pub fn asset_id_for_mesh(h: MeshHandle) -> AssetId {
        AssetId::from_raw(h.0)
    }

    /// [`AssetId`] of the canonical bytes for a [`MaterialHandle`].
    #[must_use]
    pub fn asset_id_for_material(h: MaterialHandle) -> AssetId {
        AssetId::from_raw(h.0)
    }

    /// [`AssetId`] of the canonical bytes for an [`AnimationHandle`].
    #[must_use]
    pub fn asset_id_for_animation(h: AnimationHandle) -> AssetId {
        AssetId::from_raw(h.0)
    }

    /// [`AssetId`] of the canonical bytes for a [`SkeletonHandle`].
    #[must_use]
    pub fn asset_id_for_skeleton(h: SkeletonHandle) -> AssetId {
        AssetId::from_raw(h.0)
    }

    /// [`AssetId`] of the canonical bytes for an [`ImageHandle`].
    #[must_use]
    pub fn asset_id_for_image(h: ImageHandle) -> AssetId {
        AssetId::from_raw(h.0)
    }

    /// Fallible insert that surfaces backing I/O failures as
    /// [`GltfError::Cache`].
    ///
    /// # Errors
    ///
    /// Returns [`GltfError::Cache`] when the underlying
    /// `rge_asset_store::Cache::put` fails (filesystem out of space,
    /// permission denied, etc.). The typed mirror is not updated on
    /// failure, so the adapter's observable state is unchanged.
    pub fn try_insert_mesh(&mut self, asset: MeshAsset) -> Result<MeshHandle, GltfError> {
        let bytes = mesh_canonical_bytes(&asset);
        self.backing.put(bytes).map_err(GltfError::from)?;
        let h = asset.content_hash();
        self.meshes.entry(h).or_insert(asset);
        Ok(h)
    }

    /// Fallible insert for a material.
    ///
    /// # Errors
    ///
    /// See [`AssetStoreCache::try_insert_mesh`].
    pub fn try_insert_material(
        &mut self,
        asset: MaterialAsset,
    ) -> Result<MaterialHandle, GltfError> {
        let bytes = material_canonical_bytes(&asset);
        self.backing.put(bytes).map_err(GltfError::from)?;
        let h = asset.content_hash();
        self.materials.entry(h).or_insert(asset);
        Ok(h)
    }

    /// Fallible insert for an animation clip.
    ///
    /// # Errors
    ///
    /// See [`AssetStoreCache::try_insert_mesh`].
    pub fn try_insert_animation(
        &mut self,
        clip: AnimationClip,
    ) -> Result<AnimationHandle, GltfError> {
        let bytes = animation_canonical_bytes(&clip);
        self.backing.put(bytes).map_err(GltfError::from)?;
        let h = clip.content_hash();
        self.animations.entry(h).or_insert(clip);
        Ok(h)
    }

    /// Fallible insert for a skeleton.
    ///
    /// # Errors
    ///
    /// See [`AssetStoreCache::try_insert_mesh`].
    pub fn try_insert_skeleton(&mut self, skel: Skeleton) -> Result<SkeletonHandle, GltfError> {
        let bytes = skeleton_canonical_bytes(&skel);
        self.backing.put(bytes).map_err(GltfError::from)?;
        let h = skel.content_hash();
        self.skeletons.entry(h).or_insert(skel);
        Ok(h)
    }

    /// Fallible insert for an image.
    ///
    /// # Errors
    ///
    /// See [`AssetStoreCache::try_insert_mesh`].
    pub fn try_insert_image(&mut self, asset: ImageAsset) -> Result<ImageHandle, GltfError> {
        let bytes = image_canonical_bytes(&asset);
        self.backing.put(bytes).map_err(GltfError::from)?;
        let h = asset.content_hash();
        self.images.entry(h).or_insert(asset);
        Ok(h)
    }
}

impl Cache for AssetStoreCache {
    fn insert_mesh(&mut self, asset: MeshAsset) -> MeshHandle {
        // Best-effort write-through to the backing — preserves the
        // trait's infallible contract while still exercising the
        // asset-store seam. Callers that need to detect backing
        // failures use `try_insert_mesh`.
        drop(self.backing.put(mesh_canonical_bytes(&asset)));
        let h = asset.content_hash();
        self.meshes.entry(h).or_insert(asset);
        h
    }

    fn get_mesh(&self, h: &MeshHandle) -> Option<&MeshAsset> {
        self.meshes.get(h)
    }

    fn insert_material(&mut self, asset: MaterialAsset) -> MaterialHandle {
        drop(self.backing.put(material_canonical_bytes(&asset)));
        let h = asset.content_hash();
        self.materials.entry(h).or_insert(asset);
        h
    }

    fn get_material(&self, h: &MaterialHandle) -> Option<&MaterialAsset> {
        self.materials.get(h)
    }

    fn insert_animation(&mut self, clip: AnimationClip) -> AnimationHandle {
        drop(self.backing.put(animation_canonical_bytes(&clip)));
        let h = clip.content_hash();
        self.animations.entry(h).or_insert(clip);
        h
    }

    fn get_animation(&self, h: &AnimationHandle) -> Option<&AnimationClip> {
        self.animations.get(h)
    }

    fn insert_skeleton(&mut self, skel: Skeleton) -> SkeletonHandle {
        drop(self.backing.put(skeleton_canonical_bytes(&skel)));
        let h = skel.content_hash();
        self.skeletons.entry(h).or_insert(skel);
        h
    }

    fn get_skeleton(&self, h: &SkeletonHandle) -> Option<&Skeleton> {
        self.skeletons.get(h)
    }

    fn insert_image(&mut self, asset: ImageAsset) -> ImageHandle {
        drop(self.backing.put(image_canonical_bytes(&asset)));
        let h = asset.content_hash();
        self.images.entry(h).or_insert(asset);
        h
    }

    fn get_image(&self, h: &ImageHandle) -> Option<&ImageAsset> {
        self.images.get(h)
    }
}

impl From<CacheError> for GltfError {
    fn from(e: CacheError) -> Self {
        Self::Cache(e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Canonical byte form helpers — chosen so the BLAKE3 of the bytes matches
// the corresponding `*Handle::content_hash()` digest. That keeps the
// asset-store `AssetId` and the io-gltf `*Handle` byte-for-byte identical,
// and lets the typed handle round-trip through `AssetId::from_raw`.
// ---------------------------------------------------------------------------

fn mesh_canonical_bytes(asset: &MeshAsset) -> Bytes {
    let mut out = Vec::new();
    for v in &asset.positions {
        for c in v {
            out.extend_from_slice(&c.to_le_bytes());
        }
    }
    out.push(b'|');
    for v in &asset.normals {
        for c in v {
            out.extend_from_slice(&c.to_le_bytes());
        }
    }
    out.push(b'|');
    for v in &asset.texcoords {
        for c in v {
            out.extend_from_slice(&c.to_le_bytes());
        }
    }
    out.push(b'|');
    for i in &asset.indices {
        out.extend_from_slice(&i.to_le_bytes());
    }
    out
}

fn material_canonical_bytes(asset: &MaterialAsset) -> Bytes {
    let mut out = Vec::new();
    out.extend_from_slice(asset.name.as_bytes());
    out.push(b'|');
    for c in asset.base_color {
        out.extend_from_slice(&c.to_le_bytes());
    }
    out.extend_from_slice(&asset.metallic.to_le_bytes());
    out.extend_from_slice(&asset.roughness.to_le_bytes());
    for c in asset.emissive {
        out.extend_from_slice(&c.to_le_bytes());
    }
    out.push(u8::from(asset.double_sided));
    out.extend_from_slice(asset.alpha_mode.as_gltf_str().as_bytes());
    out.extend_from_slice(&asset.alpha_cutoff.to_le_bytes());
    out.extend_from_slice(&(asset.base_color_texture.unwrap_or(usize::MAX) as u64).to_le_bytes());
    out.extend_from_slice(&(asset.normal_texture.unwrap_or(usize::MAX) as u64).to_le_bytes());
    out.extend_from_slice(
        &(asset.metallic_roughness_texture.unwrap_or(usize::MAX) as u64).to_le_bytes(),
    );
    out
}

fn animation_canonical_bytes(clip: &AnimationClip) -> Bytes {
    let mut out = Vec::new();
    out.extend_from_slice(clip.name.as_bytes());
    out.push(b'|');
    for s in &clip.samplers {
        out.extend_from_slice(&(s.target_node as u64).to_le_bytes());
        out.extend_from_slice(s.interpolation.as_gltf_str().as_bytes());
        out.extend_from_slice(s.channel.as_path_str().as_bytes());
        for t in &s.times {
            out.extend_from_slice(&t.to_le_bytes());
        }
        match &s.channel {
            crate::animation::BoneChannel::Translation(v)
            | crate::animation::BoneChannel::Scale(v) => {
                for c in v {
                    for x in c {
                        out.extend_from_slice(&x.to_le_bytes());
                    }
                }
            }
            crate::animation::BoneChannel::Rotation(v) => {
                for q in v {
                    for x in q {
                        out.extend_from_slice(&x.to_le_bytes());
                    }
                }
            }
            crate::animation::BoneChannel::Weights(v) => {
                for x in v {
                    out.extend_from_slice(&x.to_le_bytes());
                }
            }
        }
        out.push(b'|');
    }
    out
}

fn skeleton_canonical_bytes(skel: &Skeleton) -> Bytes {
    let mut out = Vec::new();
    out.extend_from_slice(skel.name.as_bytes());
    out.push(b'|');
    for j in &skel.joints {
        out.extend_from_slice(&(*j as u64).to_le_bytes());
    }
    out.push(b'|');
    for m in &skel.inverse_bind_matrices {
        for c in m {
            out.extend_from_slice(&c.to_le_bytes());
        }
    }
    out.push(b'|');
    out.extend_from_slice(&(skel.root.unwrap_or(usize::MAX) as u64).to_le_bytes());
    out
}

fn image_canonical_bytes(asset: &ImageAsset) -> Bytes {
    let mut out = Vec::new();
    out.extend_from_slice(&asset.width().to_le_bytes());
    out.extend_from_slice(&asset.height().to_le_bytes());
    let pf_tag: u8 = match asset.pixel_format() {
        rge_io_image::PixelFormat::Rgba8 => 0,
        rge_io_image::PixelFormat::Rgba16 => 1,
        rge_io_image::PixelFormat::Rgba32F => 2,
    };
    out.push(pf_tag);
    out.extend_from_slice(asset.pixels());
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use rge_asset_store::InMemoryCache;

    use super::*;
    use crate::animation::AnimationSampler;
    use crate::cache_stub::MemoryCache;

    fn sample_mesh() -> MeshAsset {
        MeshAsset {
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
            normals: vec![[0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0]],
            texcoords: vec![],
            indices: vec![0, 1, 2],
            material_index: None,
        }
    }

    fn sample_material() -> MaterialAsset {
        MaterialAsset {
            name: "negative-test-mat".into(),
            ..MaterialAsset::default()
        }
    }

    fn sample_animation() -> AnimationClip {
        AnimationClip {
            name: "negative-test-anim".into(),
            samplers: vec![AnimationSampler {
                target_node: 0,
                times: vec![0.0, 1.0],
                channel: crate::animation::BoneChannel::Translation(vec![
                    [0.0, 0.0, 0.0],
                    [1.0, 0.0, 0.0],
                ]),
                interpolation: crate::animation::Interpolation::Linear,
            }],
        }
    }

    fn sample_skeleton() -> Skeleton {
        Skeleton {
            name: "negative-test-skel".into(),
            joints: vec![0, 1, 2],
            inverse_bind_matrices: Vec::new(),
            root: None,
        }
    }

    // -------------------------------------------------------------
    // Parity with MemoryCache
    // -------------------------------------------------------------

    #[test]
    fn insert_mesh_returns_same_handle_as_memory_cache() {
        let mesh = sample_mesh();
        let mut mem = MemoryCache::new();
        let mut adapter = AssetStoreCache::new(InMemoryCache::new());

        let h_mem = mem.insert_mesh(mesh.clone());
        let h_adapter = adapter.insert_mesh(mesh.clone());

        assert_eq!(
            h_mem, h_adapter,
            "adapter must produce the same content-hash handle as MemoryCache"
        );

        let fetched = adapter.get_mesh(&h_adapter).expect("get_mesh");
        assert_eq!(*fetched, mesh, "get returns the same typed mesh");
    }

    #[test]
    fn re_inserting_same_mesh_yields_stable_handle() {
        let mut adapter = AssetStoreCache::new(InMemoryCache::new());
        let mesh = sample_mesh();

        let h1 = adapter.insert_mesh(mesh.clone());
        let h2 = adapter.insert_mesh(mesh.clone());

        assert_eq!(h1, h2, "stable handle on re-insert");
        // Typed mirror dedup parity with MemoryCache.
        assert_eq!(adapter.meshes.len(), 1);
    }

    // -------------------------------------------------------------
    // Asset-store-backed byte dedup
    // -------------------------------------------------------------

    #[test]
    fn duplicate_inserts_yield_one_asset_store_entry() {
        let mut adapter = AssetStoreCache::new(InMemoryCache::new());
        let mesh = sample_mesh();

        let h = adapter.insert_mesh(mesh.clone());
        let _ = adapter.insert_mesh(mesh.clone());

        // Verify dedup happened through the asset-store backing path —
        // not just the typed mirror.
        let backing = adapter.backing();
        assert_eq!(
            backing.len(),
            1,
            "asset-store must dedup identical canonical bytes to a single entry"
        );

        // Same AssetId as the typed handle (canonical bytes share the
        // BLAKE3 digest with `MeshAsset::content_hash`).
        let expected_id = AssetStoreCache::asset_id_for_mesh(h);
        let bytes = backing
            .get(&expected_id)
            .expect("asset-store get")
            .expect("entry present under expected AssetId");
        assert_eq!(bytes, mesh_canonical_bytes(&mesh));
    }

    #[test]
    fn distinct_meshes_get_distinct_asset_ids() {
        let mut adapter = AssetStoreCache::new(InMemoryCache::new());
        let mut a = sample_mesh();
        let mut b = sample_mesh();
        // Mutate `b` so the canonical bytes differ.
        b.positions[0][0] = 99.0;
        a.material_index = None;
        b.material_index = None;

        let h_a = adapter.insert_mesh(a);
        let h_b = adapter.insert_mesh(b);

        assert_ne!(h_a, h_b);
        assert_eq!(adapter.backing().len(), 2);
    }

    // -------------------------------------------------------------
    // Underlying I/O / cache errors → `GltfError::Cache`
    // -------------------------------------------------------------

    #[derive(Default)]
    struct FailingBacking;

    impl ByteCache for FailingBacking {
        fn get(&self, _id: &AssetId) -> Result<Option<Bytes>, CacheError> {
            Err(CacheError::Io("synthetic: get failed".into()))
        }
        fn put(&mut self, _bytes: Bytes) -> Result<AssetId, CacheError> {
            Err(CacheError::Io("synthetic: put failed".into()))
        }
        fn evict_lru(&mut self, _max: u64) -> Result<(), CacheError> {
            Err(CacheError::Io("synthetic: evict failed".into()))
        }
        fn total_size(&self) -> u64 {
            0
        }
        fn len(&self) -> usize {
            0
        }
    }

    #[test]
    fn try_insert_mesh_surfaces_backing_error_as_gltf_cache() {
        let mut adapter = AssetStoreCache::new(FailingBacking);
        let mesh = sample_mesh();

        let err = adapter.try_insert_mesh(mesh).expect_err("must fail");
        assert!(
            matches!(err, GltfError::Cache(_)),
            "backing failures must surface as GltfError::Cache, got {err:?}"
        );
        // Mirror must not be touched on failure.
        assert_eq!(adapter.meshes.len(), 0);
    }

    #[test]
    fn try_insert_image_surfaces_backing_error_as_gltf_cache() {
        let mut adapter = AssetStoreCache::new(FailingBacking);
        let img = ImageAsset::from_inner(rge_io_image::Image::from_rgba8(1, 1, vec![0, 0, 0, 255]));

        let err = adapter.try_insert_image(img).expect_err("must fail");
        assert!(matches!(err, GltfError::Cache(_)));
    }

    // Sanity: BadAssetId errors round-trip through `From<CacheError>`
    // without leaking into another error variant.
    #[test]
    fn cache_error_bad_asset_id_maps_to_gltf_cache_variant() {
        let e: GltfError = CacheError::BadAssetId("zzz".into()).into();
        assert!(matches!(e, GltfError::Cache(_)));
    }

    #[test]
    fn try_insert_material_surfaces_backing_error_as_gltf_cache() {
        let mut adapter = AssetStoreCache::new(FailingBacking);
        let asset = sample_material();
        let expected = asset.content_hash();

        let err = adapter.try_insert_material(asset).expect_err("must fail");
        assert!(
            matches!(err, GltfError::Cache(_)),
            "backing failures must surface as GltfError::Cache, got {err:?}"
        );
        assert_eq!(adapter.materials.len(), 0);
        assert!(adapter.get_material(&expected).is_none());
    }

    #[test]
    fn try_insert_animation_surfaces_backing_error_as_gltf_cache() {
        let mut adapter = AssetStoreCache::new(FailingBacking);
        let clip = sample_animation();
        let expected = clip.content_hash();

        let err = adapter.try_insert_animation(clip).expect_err("must fail");
        assert!(
            matches!(err, GltfError::Cache(_)),
            "backing failures must surface as GltfError::Cache, got {err:?}"
        );
        assert_eq!(adapter.animations.len(), 0);
        assert!(adapter.get_animation(&expected).is_none());
    }

    #[test]
    fn try_insert_skeleton_surfaces_backing_error_as_gltf_cache() {
        let mut adapter = AssetStoreCache::new(FailingBacking);
        let skel = sample_skeleton();
        let expected = skel.content_hash();

        let err = adapter.try_insert_skeleton(skel).expect_err("must fail");
        assert!(
            matches!(err, GltfError::Cache(_)),
            "backing failures must surface as GltfError::Cache, got {err:?}"
        );
        assert_eq!(adapter.skeletons.len(), 0);
        assert!(adapter.get_skeleton(&expected).is_none());
    }

    // Switchable backing: delegates to `InMemoryCache` for every method,
    // but fails `put` while the shared flag is set. Tests flip the flag
    // across calls on the same `AssetStoreCache` to prove the adapter
    // recovers without being rebuilt.
    struct SwitchableBacking {
        inner: InMemoryCache,
        fail: Rc<Cell<bool>>,
    }

    impl SwitchableBacking {
        fn new(fail: Rc<Cell<bool>>) -> Self {
            Self {
                inner: InMemoryCache::new(),
                fail,
            }
        }
    }

    impl ByteCache for SwitchableBacking {
        fn get(&self, id: &AssetId) -> Result<Option<Bytes>, CacheError> {
            self.inner.get(id)
        }
        fn put(&mut self, bytes: Bytes) -> Result<AssetId, CacheError> {
            if self.fail.get() {
                Err(CacheError::Io("synthetic: switchable put failed".into()))
            } else {
                self.inner.put(bytes)
            }
        }
        fn evict_lru(&mut self, max: u64) -> Result<(), CacheError> {
            self.inner.evict_lru(max)
        }
        fn total_size(&self) -> u64 {
            self.inner.total_size()
        }
        fn len(&self) -> usize {
            self.inner.len()
        }
    }

    #[test]
    fn try_insert_recovers_when_backing_toggled_from_fail_to_succeed() {
        let flag = Rc::new(Cell::new(true));
        let backing = SwitchableBacking::new(Rc::clone(&flag));
        let mut adapter = AssetStoreCache::new(backing);
        let mesh = sample_mesh();
        let expected = mesh.content_hash();

        // First call: backing fails, typed mirror must stay empty.
        let err = adapter
            .try_insert_mesh(mesh.clone())
            .expect_err("must fail while flag is set");
        assert!(
            matches!(err, GltfError::Cache(_)),
            "first call must surface backing failure as GltfError::Cache, got {err:?}"
        );
        assert_eq!(adapter.meshes.len(), 0);
        assert!(adapter.get_mesh(&expected).is_none());

        // Toggle the SAME backing to success on the SAME adapter.
        flag.set(false);
        let h = adapter
            .try_insert_mesh(mesh.clone())
            .expect("retry must succeed after toggling backing");
        assert_eq!(
            h, expected,
            "successful retry must return the asset's content_hash handle"
        );
        assert_eq!(adapter.meshes.len(), 1);
        assert!(adapter.get_mesh(&h).is_some());
    }
}
