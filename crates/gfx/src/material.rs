//! Material UBO + base-colour 2D texture + linear sampler bound at `@group(2)`.
//!
//! Layout:
//!
//! | binding | resource              | size / format       |
//! |---------|-----------------------|---------------------|
//! | 0       | uniform buffer        | 32 bytes (2× vec4) |
//! | 1       | 2D texture (sampled)  | `Rgba8UnormSrgb`    |
//! | 2       | sampler (filtering)   | linear / repeat     |
//!
//! UBO contents (column-major / packed):
//!
//! | offset | field        | type         |
//! |--------|--------------|--------------|
//! | 0      | `base_color` | `vec4<f32>`  |
//! | 16     | `phong`      | `vec4<f32>`  |
//!
//! `phong` packs `(ambient, diffuse, specular, shininess)` in `(x,y,z,w)`.
//!
//! WGSL std140 alignment: `vec4<f32>` is 16 bytes naturally aligned, so no
//! explicit padding is required.

use bytemuck::{Pod, Zeroable};
use rge_material_runtime::MaterialDescriptor;

use crate::context::GfxContext;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Size of the material UBO in bytes: 2 × `vec4<f32>` = 32 bytes.
const MATERIAL_UBO_SIZE: u64 = 32;

/// Bytes per pixel for `Rgba8UnormSrgb`.
const RGBA_BYTES_PER_PIXEL: u32 = 4;

/// 1×1 white sRGB texel — placeholder texture for [`Material::from_descriptor`].
///
/// `MaterialDescriptor` v0 carries no texture axis; the gfx-side adapter
/// uploads this 4-byte all-`0xFF` texel so the existing texture-bound
/// [`Material`] bind group stays valid. A later `TextureId` axis on
/// `MaterialDescriptor` will replace this placeholder.
const WHITE_1X1_RGBA: &[u8] = &[0xFF, 0xFF, 0xFF, 0xFF];

// ---------------------------------------------------------------------------
// MaterialUbo (POD struct uploaded to GPU)
// ---------------------------------------------------------------------------

/// POD layout of the material UBO — 32 bytes, two vec4s.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
struct MaterialUbo {
    base_color: [f32; 4],
    phong: [f32; 4],
}

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors that can occur when creating a [`Material`].
#[derive(Debug, thiserror::Error)]
pub enum MaterialError {
    /// `data.len()` did not equal `width * height * 4` (the byte count for
    /// an `Rgba8UnormSrgb` texture).
    #[error("texture data length mismatch: got {got} bytes, expected {expected}")]
    DataLengthMismatch {
        /// The actual length of the supplied byte slice.
        got: usize,
        /// The required length (`width × height × 4`).
        expected: usize,
    },

    /// Width or height was zero.
    #[error("invalid texture size: {0}x{1}")]
    InvalidSize(u32, u32),
}

/// Errors that can occur in the [`upload_rgba8_srgb_2d`] helper.
#[derive(Debug, thiserror::Error)]
pub enum TextureUploadError {
    /// `data.len()` did not equal `width * height * 4`.
    #[error("texture data length mismatch: got {got} bytes, expected {expected}")]
    DataLengthMismatch {
        /// The actual length of the supplied byte slice.
        got: usize,
        /// The required length (`width × height × 4`).
        expected: usize,
    },

    /// Width or height was zero.
    #[error("invalid texture size: {0}x{1}")]
    InvalidSize(u32, u32),
}

// ---------------------------------------------------------------------------
// Texture upload helper
// ---------------------------------------------------------------------------

/// Upload tightly-packed `Rgba8UnormSrgb` texel data as a 2D texture and
/// return the texture, its default view, and a linear/repeat sampler.
///
/// `data` length must equal `width × height × 4`.  No mipmaps are generated.
///
/// # Errors
///
/// - [`TextureUploadError::InvalidSize`] when `width` or `height` is zero.
/// - [`TextureUploadError::DataLengthMismatch`] when `data.len()` ≠ `width *
///   height * 4`.
pub fn upload_rgba8_srgb_2d(
    ctx: &GfxContext,
    data: &[u8],
    width: u32,
    height: u32,
    label: &str,
) -> Result<(wgpu::Texture, wgpu::TextureView, wgpu::Sampler), TextureUploadError> {
    if width == 0 || height == 0 {
        return Err(TextureUploadError::InvalidSize(width, height));
    }
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|v| v.checked_mul(RGBA_BYTES_PER_PIXEL as usize))
        .unwrap_or(usize::MAX);
    if data.len() != expected {
        return Err(TextureUploadError::DataLengthMismatch {
            got: data.len(),
            expected,
        });
    }

    let device = ctx.device();
    let queue = ctx.queue();

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });

    // wgpu 29: write_texture takes TexelCopyTextureInfo + TexelCopyBufferLayout
    // by value (renamed from ImageCopyTexture / ImageDataLayout in older wgpu).
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        data,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(width * RGBA_BYTES_PER_PIXEL),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );

    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: Some("Material.sampler"),
        address_mode_u: wgpu::AddressMode::Repeat,
        address_mode_v: wgpu::AddressMode::Repeat,
        address_mode_w: wgpu::AddressMode::Repeat,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::MipmapFilterMode::Nearest,
        lod_min_clamp: 0.0,
        lod_max_clamp: 0.0,
        compare: None,
        anisotropy_clamp: 1,
        border_color: None,
    });

    Ok((texture, view, sampler))
}

// ---------------------------------------------------------------------------
// Material
// ---------------------------------------------------------------------------

/// Material bind group: UBO + base-colour 2D texture + linear sampler.
///
/// Bound at `@group(2)` during a lit render pass.  The texture is uploaded
/// once at construction; the UBO can be refreshed via [`Material::update_color`].
#[derive(Debug)]
pub struct Material {
    buffer: wgpu::Buffer,
    // The texture/view/sampler fields keep the underlying GPU resources alive
    // for as long as `bind_group` references them. wgpu's bind-group entry
    // resolves the resource handles at descriptor time but does not extend
    // their lifetime; dropping these fields would invalidate the bind group.
    #[allow(dead_code, reason = "GPU resource keep-alive for bind_group lifetime")]
    texture: wgpu::Texture,
    #[allow(dead_code, reason = "GPU resource keep-alive for bind_group lifetime")]
    view: wgpu::TextureView,
    #[allow(dead_code, reason = "GPU resource keep-alive for bind_group lifetime")]
    sampler: wgpu::Sampler,
    bind_group: wgpu::BindGroup,
    bind_group_layout: wgpu::BindGroupLayout,
}

impl Material {
    /// Create a [`Material`] with an `Rgba8UnormSrgb` 2D base-colour texture
    /// and default factors (white base colour, ambient 0.1, diffuse 1.0,
    /// specular 0.5, shininess 32).
    ///
    /// # Errors
    ///
    /// - [`MaterialError::InvalidSize`] when `width` or `height` is zero.
    /// - [`MaterialError::DataLengthMismatch`] when `texture_rgba8_srgb_data.
    ///   len()` ≠ `width × height × 4`.
    pub fn new(
        ctx: &GfxContext,
        texture_rgba8_srgb_data: &[u8],
        width: u32,
        height: u32,
    ) -> Result<Self, MaterialError> {
        let device = ctx.device();
        let queue = ctx.queue();

        // Validate + upload the texture via the helper, mapping its error type.
        let (texture, view, sampler) = upload_rgba8_srgb_2d(
            ctx,
            texture_rgba8_srgb_data,
            width,
            height,
            "Material.texture",
        )
        .map_err(|e| match e {
            TextureUploadError::InvalidSize(w, h) => MaterialError::InvalidSize(w, h),
            TextureUploadError::DataLengthMismatch { got, expected } => {
                MaterialError::DataLengthMismatch { got, expected }
            }
        })?;

        // Allocate + initialise the UBO.
        let initial = MaterialUbo {
            base_color: [1.0, 1.0, 1.0, 1.0],
            phong: [0.1, 1.0, 0.5, 32.0],
        };
        let bytes: &[u8] = bytemuck::bytes_of(&initial);

        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Material.buffer"),
            size: MATERIAL_UBO_SIZE,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        queue.write_buffer(&buffer, 0, bytes);

        // Bind group layout: UBO at 0 (FRAGMENT), texture at 1, sampler at 2.
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Material.bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(MATERIAL_UBO_SIZE),
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Material.bind_group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        Ok(Self {
            buffer,
            texture,
            view,
            sampler,
            bind_group,
            bind_group_layout,
        })
    }

    /// Construct a [`Material`] from a semantic [`MaterialDescriptor`].
    ///
    /// Wraps [`Material::new`] + [`Material::update_color`] over the
    /// `MaterialParams` carried by the descriptor:
    ///
    /// 1. Allocates the bind group + UBO via [`Material::new`] using a 1×1
    ///    placeholder white sRGB texel (`WHITE_1X1_RGBA`). v0
    ///    `MaterialDescriptor` carries no texture axis; a future `TextureId`
    ///    axis will replace this placeholder.
    /// 2. Calls [`Material::update_color`] to overwrite the UBO with
    ///    `desc.params.base_color` + `desc.params.phong`.
    ///
    /// The `shader_id` / `vertex_layout` / `color_target` / `depth` axes of
    /// the descriptor are **not** consumed here — they govern PSO identity
    /// (see [`crate::intent_adapter::intent_to_pso_key`]). `Material` only
    /// owns the texture + UBO + bind group, which depend on `params` alone.
    ///
    /// # Errors
    ///
    /// Currently infallible in practice (the 1×1 placeholder always validates),
    /// but the constructor still returns `Result` for symmetry with
    /// [`Material::new`] and for forward-compatibility once a real texture
    /// axis is plumbed through `MaterialDescriptor`.
    pub fn from_descriptor(
        ctx: &GfxContext,
        desc: &MaterialDescriptor,
    ) -> Result<Self, MaterialError> {
        let material = Self::new(ctx, WHITE_1X1_RGBA, 1, 1)?;
        material.update_color(
            ctx,
            glam::Vec4::from_array(desc.params.base_color),
            glam::Vec4::from_array(desc.params.phong),
        );
        Ok(material)
    }

    /// Refresh the UBO with a new `base_color` (rgba) and `phong` factors
    /// (`(ambient, diffuse, specular, shininess)`).
    ///
    /// The texture is unchanged; the bind group remains valid after this call.
    pub fn update_color(&self, ctx: &GfxContext, base_color: glam::Vec4, phong: glam::Vec4) {
        let ubo = MaterialUbo {
            base_color: base_color.to_array(),
            phong: phong.to_array(),
        };
        let bytes: &[u8] = bytemuck::bytes_of(&ubo);
        ctx.queue().write_buffer(&self.buffer, 0, bytes);
    }

    /// Return the bind group layout (needed when building a pipeline layout).
    #[must_use]
    pub fn bind_group_layout(&self) -> &wgpu::BindGroupLayout {
        &self.bind_group_layout
    }

    /// Return the bind group to bind at `@group(2)` during a render pass.
    #[must_use]
    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! ctx_or_skip {
        () => {{
            match crate::context::GfxContext::new_headless() {
                Ok(c) => c,
                Err(_) => {
                    eprintln!("SKIP: no GPU adapter");
                    return;
                }
            }
        }};
    }

    /// 2×2 white texture (16 bytes).
    fn white_2x2() -> Vec<u8> {
        vec![255u8; 16]
    }

    #[test]
    fn new_with_2x2_white_texture() {
        let ctx = ctx_or_skip!();
        let mat = Material::new(&ctx, &white_2x2(), 2, 2).expect("material");
        let _bg = mat.bind_group();
        let _bgl = mat.bind_group_layout();
    }

    #[test]
    fn data_length_mismatch_errors() {
        let ctx = ctx_or_skip!();
        // 2x2 needs 16 bytes; supply 8.
        let err = Material::new(&ctx, &[0u8; 8], 2, 2).unwrap_err();
        assert!(matches!(
            err,
            MaterialError::DataLengthMismatch {
                got: 8,
                expected: 16,
            }
        ));
    }

    #[test]
    fn update_color_does_not_invalidate_bind_group() {
        let ctx = ctx_or_skip!();
        let mat = Material::new(&ctx, &white_2x2(), 2, 2).expect("material");
        mat.update_color(
            &ctx,
            glam::Vec4::new(0.5, 0.6, 0.7, 1.0),
            glam::Vec4::new(0.1, 1.0, 0.5, 32.0),
        );
        let _bg = mat.bind_group();
        // Second update with different values — bind group must still be valid.
        mat.update_color(
            &ctx,
            glam::Vec4::new(1.0, 0.0, 0.0, 1.0),
            glam::Vec4::new(0.2, 0.8, 0.3, 16.0),
        );
        let _bg2 = mat.bind_group();
    }
}
