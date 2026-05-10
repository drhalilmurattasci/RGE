//! Lit-mesh render pipeline: Lambert+Phong with a base-colour texture sample.
//!
//! Shader bindings (must match [`Camera`], [`DirectionalLight`], [`Material`]):
//!
//! - `@group(0) @binding(0)` — `Camera` UBO (`view_proj` + `normal_matrix`)
//! - `@group(1) @binding(0)` — `DirectionalLight` UBO (`direction` + `color`)
//! - `@group(2) @binding(0)` — `Material` UBO (`base_color` + `phong`)
//! - `@group(2) @binding(1)` — base-colour texture (`Rgba8UnormSrgb`)
//! - `@group(2) @binding(2)` — filtering sampler
//!
//! The shader does Lambert + Phong in **world space**.  View direction is
//! computed from `world_pos` assuming the camera sits at the world origin
//! (this is a simplification suitable for v0 — when an explicit camera
//! position is added in a later phase, replace `view_dir = normalize(-world_pos)`
//! with `normalize(camera_pos - world_pos)`).
//!
//! Output target is **linear** — caller writes linear `(r,g,b,1.0)` and lets
//! the GPU's sRGB-aware target format (or lack thereof) decide gamma encoding.
//! The existing [`HeadlessTarget`] uses `Rgba8Unorm` (linear), so writing 1.0
//! returns 255 in the readback — that's what the pixel tests assume.
//!
//! [`Camera`]: crate::camera::Camera
//! [`DirectionalLight`]: crate::light::DirectionalLight
//! [`Material`]: crate::material::Material
//! [`HeadlessTarget`]: crate::target::HeadlessTarget

use bytemuck::cast_slice;
use rge_brep_render::RenderMesh;
use wgpu::util::DeviceExt as _;

use crate::buffer::{BufferError, IndexBuffer};
use crate::camera::Camera;
use crate::context::GfxContext;
use crate::light::DirectionalLight;
use crate::material::Material;
use crate::target::HeadlessTarget;
use crate::vertex_lit::VertexLit;

// ---------------------------------------------------------------------------
// CPU-side adapter — `RenderMesh` → `Vec<VertexLit>`
// ---------------------------------------------------------------------------

/// CPU-side conversion from a [`rge_brep_render::RenderMesh`] (positions +
/// normals + face_labels + indices) into the [`VertexLit`] layout
/// `LitMesh::from_indexed` expects.
///
/// `RenderMesh` carries position + normal but no UV — this helper emits
/// the placeholder UV `[0.0, 0.0]` for every output vertex (see
/// [`LitMesh::from_render_mesh`] for the v0 contract rationale).
///
/// Trusts brep-render's invariants: `positions.len() == normals.len()`
/// (per `RenderMesh::from_buffers`'s "vertex tripling" output shape).
/// No defensive length-checking — same posture as the picker and
/// brep-render itself.
fn vertex_lit_from_render_mesh(render_mesh: &RenderMesh) -> Vec<VertexLit> {
    render_mesh
        .positions
        .iter()
        .zip(render_mesh.normals.iter())
        .map(|(&position, &normal)| VertexLit {
            position,
            normal,
            uv: [0.0, 0.0],
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Embedded WGSL
// ---------------------------------------------------------------------------

/// WGSL shader source for the lit-mesh pipeline.
///
/// - Vertex stage: clip = `camera.view_proj * vec4(position, 1.0)`; passes
///   world-space position, world-space normal (transformed by the camera's
///   `normal_matrix`), and uv to the fragment.
/// - Fragment stage: samples the base texture, computes Lambert + Phong,
///   and modulates by the light colour and material factors.
const LIT_MESH_WGSL: &str = r"
struct CameraUbo {
    view_proj:     mat4x4<f32>,
    normal_matrix: mat4x4<f32>,
};

struct LightUbo {
    direction: vec3<f32>,
    color:     vec3<f32>,
};

struct MaterialUbo {
    base_color: vec4<f32>,
    phong:      vec4<f32>, // x=ambient, y=diffuse, z=specular, w=shininess
};

@group(0) @binding(0) var<uniform> u_camera:   CameraUbo;
@group(1) @binding(0) var<uniform> u_light:    LightUbo;
@group(2) @binding(0) var<uniform> u_material: MaterialUbo;
@group(2) @binding(1) var t_base: texture_2d<f32>;
@group(2) @binding(2) var s_base: sampler;

struct VsIn {
    @location(0) position: vec3<f32>,
    @location(1) normal:   vec3<f32>,
    @location(2) uv:       vec2<f32>,
};

struct VsOut {
    @builtin(position) clip:         vec4<f32>,
    @location(0)       world_pos:    vec3<f32>,
    @location(1)       world_normal: vec3<f32>,
    @location(2)       uv:           vec2<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    out.clip         = u_camera.view_proj * vec4<f32>(in.position, 1.0);
    out.world_pos    = in.position;
    out.world_normal = (u_camera.normal_matrix * vec4<f32>(in.normal, 0.0)).xyz;
    out.uv           = in.uv;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let n          = normalize(in.world_normal);
    let l_dir      = normalize(u_light.direction);
    let neg_l      = -l_dir;
    let view_dir   = normalize(-in.world_pos);
    let tex_sample = textureSample(t_base, s_base, in.uv).rgb;
    let tex_color  = tex_sample * u_material.base_color.rgb;

    let lambert = max(dot(n, neg_l), 0.0);

    // Phong via half-vector — only contribute when the surface faces the light.
    var spec: f32 = 0.0;
    if (lambert > 0.0) {
        let half_dir = normalize(neg_l + view_dir);
        spec = pow(max(dot(n, half_dir), 0.0), u_material.phong.w);
    }

    let ambient_term  = u_material.phong.x * tex_color;
    let diffuse_term  = u_material.phong.y * lambert * tex_color;
    let specular_term = u_material.phong.z * spec * vec3<f32>(1.0, 1.0, 1.0);

    let lit = (ambient_term + diffuse_term + specular_term) * u_light.color;
    return vec4<f32>(lit, 1.0);
}
";

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur when building a [`LitMeshPipeline`].
#[derive(Debug, thiserror::Error)]
pub enum LitMeshPipelineError {
    /// The WGSL source failed to parse or compile.
    #[error("WGSL parse error: {0}")]
    Wgsl(String),
}

// ---------------------------------------------------------------------------
// LitVertexBuffer
// ---------------------------------------------------------------------------

/// A GPU vertex buffer holding [`VertexLit`] data.
///
/// Usage flags: `VERTEX | COPY_DST`.  Parallel to
/// [`VertexBuffer`](crate::buffer::VertexBuffer) for the lit pipeline so the
/// existing non-lit `Mesh` / `VertexBuffer` types can stay untouched.
pub struct LitVertexBuffer {
    buffer: wgpu::Buffer,
    vertex_count: u32,
}

impl LitVertexBuffer {
    /// Allocate a vertex buffer and upload `vertices` to the GPU.
    ///
    /// # Errors
    ///
    /// Returns [`BufferError::Empty`] if `vertices` is empty.
    pub fn new(ctx: &GfxContext, vertices: &[VertexLit]) -> Result<Self, BufferError> {
        if vertices.is_empty() {
            return Err(BufferError::Empty);
        }
        let vertex_count = u32::try_from(vertices.len()).unwrap_or(u32::MAX);
        let bytes: &[u8] = cast_slice(vertices);

        let buffer = ctx
            .device()
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("LitVertexBuffer"),
                contents: bytes,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });

        Ok(Self {
            buffer,
            vertex_count,
        })
    }

    /// Borrow the underlying [`wgpu::Buffer`].
    #[must_use]
    pub fn buffer(&self) -> &wgpu::Buffer {
        &self.buffer
    }

    /// Number of vertices in this buffer.
    #[must_use]
    pub fn vertex_count(&self) -> u32 {
        self.vertex_count
    }
}

// ---------------------------------------------------------------------------
// LitMesh
// ---------------------------------------------------------------------------

/// A renderable lit mesh: owns one [`LitVertexBuffer`] and an optional
/// [`IndexBuffer`] (the index buffer type is shared with the unlit `Mesh`).
pub struct LitMesh {
    vertex_buffer: LitVertexBuffer,
    index_buffer: Option<IndexBuffer>,
}

impl LitMesh {
    /// Create a non-indexed lit mesh from a slice of [`VertexLit`].
    ///
    /// # Errors
    ///
    /// Returns [`BufferError::Empty`] if `vertices` is empty.
    pub fn from_vertices(ctx: &GfxContext, vertices: &[VertexLit]) -> Result<Self, BufferError> {
        let vertex_buffer = LitVertexBuffer::new(ctx, vertices)?;
        Ok(Self {
            vertex_buffer,
            index_buffer: None,
        })
    }

    /// Create an indexed lit mesh from a vertex slice and an index slice.
    ///
    /// # Errors
    ///
    /// Returns [`BufferError::Empty`] if either slice is empty.
    pub fn from_indexed(
        ctx: &GfxContext,
        vertices: &[VertexLit],
        indices: &[u32],
    ) -> Result<Self, BufferError> {
        let vertex_buffer = LitVertexBuffer::new(ctx, vertices)?;
        let index_buffer = IndexBuffer::new(ctx, indices)?;
        Ok(Self {
            vertex_buffer,
            index_buffer: Some(index_buffer),
        })
    }

    /// Build a [`LitMesh`] (GPU-uploaded vertex + index buffers) from a
    /// [`rge_brep_render::RenderMesh`] (CPU flat-shaded mesh).
    ///
    /// # Vertex format mapping
    ///
    /// [`VertexLit`] requires `(position, normal, uv)`. `RenderMesh`
    /// carries position + normal but no UV — the adapter generates a
    /// **placeholder UV `[0.0, 0.0]` for every vertex**. This is fine for
    /// the Lambert+Phong lit pipeline: UV is consumed only for optional
    /// texture sampling, which the default material does not exercise
    /// (the WGSL fragment shader multiplies `tex_sample * base_color`,
    /// and a default-white texture leaves the lit colour unchanged at
    /// `(0, 0)` — same as anywhere else when the texture is uniform).
    /// Real UV generation is a future dispatch when a texturing pipeline
    /// lands.
    ///
    /// `RenderMesh.face_labels` is **not** uploaded to GPU. Face labels
    /// are CPU-side metadata for the future selection-highlight path
    /// (sub-ε of the chapter); sub-δ.1.A does not thread them into the
    /// render.
    ///
    /// # Errors
    ///
    /// Returns [`BufferError::Empty`] if `render_mesh.positions` or
    /// `render_mesh.indices` is empty (delegated to
    /// [`LitMesh::from_indexed`]).
    ///
    /// # Invariants
    ///
    /// * Output vertex count == `render_mesh.positions.len()`.
    /// * Output index count == `render_mesh.indices.len()`.
    /// * Every output `VertexLit.uv` is `[0.0, 0.0]`.
    /// * Output positions and normals match the input arrays
    ///   element-for-element.
    pub fn from_render_mesh(
        ctx: &GfxContext,
        render_mesh: &RenderMesh,
    ) -> Result<Self, BufferError> {
        let vertices = vertex_lit_from_render_mesh(render_mesh);
        Self::from_indexed(ctx, &vertices, &render_mesh.indices)
    }

    /// Borrow the mesh's [`LitVertexBuffer`].
    #[must_use]
    pub fn vertex_buffer(&self) -> &LitVertexBuffer {
        &self.vertex_buffer
    }

    /// Borrow the mesh's [`IndexBuffer`], if any.
    #[must_use]
    pub fn index_buffer(&self) -> Option<&IndexBuffer> {
        self.index_buffer.as_ref()
    }
}

// ---------------------------------------------------------------------------
// LitMeshPipeline
// ---------------------------------------------------------------------------

/// A compiled wgpu render pipeline for lit-mesh rendering.
///
/// Wires three bind groups (camera/light/material) and the [`VertexLit`]
/// vertex layout.  Compile errors in the embedded WGSL surface as
/// [`LitMeshPipelineError::Wgsl`].
pub struct LitMeshPipeline {
    pipeline: wgpu::RenderPipeline,
}

impl LitMeshPipeline {
    /// Compile the embedded WGSL and create the render pipeline.
    ///
    /// `camera_layout`, `light_layout`, `material_layout` must be the layouts
    /// produced by [`Camera::bind_group_layout`], [`DirectionalLight::bind_group_layout`],
    /// and [`Material::bind_group_layout`] respectively.  `color_format` must
    /// match the render target the pipeline will draw into.
    ///
    /// # Errors
    ///
    /// Returns [`LitMeshPipelineError::Wgsl`] if the embedded WGSL fails to
    /// parse (should not occur with the built-in shader).
    #[allow(clippy::unnecessary_wraps)]
    pub fn new(
        ctx: &GfxContext,
        camera_layout: &wgpu::BindGroupLayout,
        light_layout: &wgpu::BindGroupLayout,
        material_layout: &wgpu::BindGroupLayout,
        color_format: wgpu::TextureFormat,
    ) -> Result<Self, LitMeshPipelineError> {
        let device = ctx.device();

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("lit_mesh.wgsl"),
            source: wgpu::ShaderSource::Wgsl(LIT_MESH_WGSL.into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("LitMeshPipelineLayout"),
            bind_group_layouts: &[
                Some(camera_layout),
                Some(light_layout),
                Some(material_layout),
            ],
            immediate_size: 0,
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("LitMeshPipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[VertexLit::layout()],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: color_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview_mask: None,
            cache: None,
        });

        Ok(Self { pipeline })
    }

    /// Borrow the compiled [`wgpu::RenderPipeline`].
    #[must_use]
    pub fn pipeline(&self) -> &wgpu::RenderPipeline {
        &self.pipeline
    }
}

// ---------------------------------------------------------------------------
// record_lit_mesh_pass
// ---------------------------------------------------------------------------

/// Record a single render pass that clears `target`, sets up the camera /
/// light / material bind groups, then draws `mesh` with `pipeline`.
///
/// `clear` is the background colour applied via `LoadOp::Clear`.  This is a
/// free function (parallel to [`record_mesh_pass`]) so the existing unlit
/// pipeline remains untouched.
///
/// [`record_mesh_pass`]: crate::mesh_pipeline::record_mesh_pass
pub fn record_lit_mesh_pass(
    encoder: &mut wgpu::CommandEncoder,
    target: &HeadlessTarget,
    pipeline: &LitMeshPipeline,
    camera: &Camera,
    light: &DirectionalLight,
    material: &Material,
    mesh: &LitMesh,
    clear: wgpu::Color,
) {
    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("LitMeshPass"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target.view(),
            resolve_target: None,
            depth_slice: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Clear(clear),
                store: wgpu::StoreOp::Store,
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });

    pass.set_pipeline(pipeline.pipeline());
    pass.set_bind_group(0, camera.bind_group(), &[]);
    pass.set_bind_group(1, light.bind_group(), &[]);
    pass.set_bind_group(2, material.bind_group(), &[]);
    pass.set_vertex_buffer(0, mesh.vertex_buffer().buffer().slice(..));

    if let Some(ib) = mesh.index_buffer() {
        pass.set_index_buffer(ib.buffer().slice(..), ib.index_format());
        pass.draw_indexed(0..ib.index_count(), 0, 0..1);
    } else {
        pass.draw(0..mesh.vertex_buffer().vertex_count(), 0..1);
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::ReadbackBuffer;

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

    /// 16-pixel white sRGB texture (4×4, all 0xFF).  Conservative
    /// over-allocation so callers can pass any 4×4 region.
    fn white_4x4() -> Vec<u8> {
        vec![255u8; 4 * 4 * 4]
    }

    /// 2×2 checkerboard sRGB texture: top-left + bottom-right red, others
    /// blue.  Layout (rows, top-to-bottom):
    /// row 0: red, blue
    /// row 1: blue, red
    fn checker_2x2() -> Vec<u8> {
        let red: [u8; 4] = [255, 0, 0, 255];
        let blue: [u8; 4] = [0, 0, 255, 255];
        let mut data = Vec::with_capacity(16);
        data.extend_from_slice(&red);
        data.extend_from_slice(&blue);
        data.extend_from_slice(&blue);
        data.extend_from_slice(&red);
        data
    }

    /// Build a screen-aligned 1×1 quad at z=-2 with normal +Z and uv 0..1.
    /// Six vertices, two triangles (CCW from camera looking down -Z).
    fn screen_quad() -> Vec<VertexLit> {
        // Quad corners in world space, z=-2, normal=+Z (facing camera at origin).
        // CCW winding when viewed from +Z (the camera side):
        //   tri 1: BL, BR, TR
        //   tri 2: BL, TR, TL
        let bl = VertexLit::new([-1.0, -1.0, -2.0], [0.0, 0.0, 1.0], [0.0, 1.0]);
        let br = VertexLit::new([1.0, -1.0, -2.0], [0.0, 0.0, 1.0], [1.0, 1.0]);
        let tr = VertexLit::new([1.0, 1.0, -2.0], [0.0, 0.0, 1.0], [1.0, 0.0]);
        let tl = VertexLit::new([-1.0, 1.0, -2.0], [0.0, 0.0, 1.0], [0.0, 0.0]);
        vec![bl, br, tr, bl, tr, tl]
    }

    /// Standard ortho view*proj for the pixel tests: camera at origin, looking
    /// down -Z, ortho `[-1,1]×[-1,1]×[0.1,10]`.  Quad at z=-2 lands fully in
    /// the frustum, normalized to fill the viewport.
    fn ortho_view_proj() -> glam::Mat4 {
        let proj = glam::Mat4::orthographic_rh(-1.0, 1.0, -1.0, 1.0, 0.1, 10.0);
        let view = glam::Mat4::look_at_rh(
            glam::Vec3::new(0.0, 0.0, 0.0),
            glam::Vec3::new(0.0, 0.0, -1.0),
            glam::Vec3::Y,
        );
        proj * view
    }

    /// Render a quad and return the readback buffer.
    fn render_lit_quad(
        ctx: &GfxContext,
        light_dir: glam::Vec3,
        texture: &[u8],
        tex_w: u32,
        tex_h: u32,
        clear: wgpu::Color,
    ) -> ReadbackBuffer {
        let target = HeadlessTarget::new(ctx, 64, 64).expect("target");
        let camera = Camera::new(ctx).expect("camera");
        camera.update(ctx, ortho_view_proj(), glam::Mat4::IDENTITY);

        let light = DirectionalLight::new(ctx).expect("light");
        light.update(ctx, light_dir, glam::Vec3::ONE);

        let material = Material::new(ctx, texture, tex_w, tex_h).expect("material");

        let pipeline = LitMeshPipeline::new(
            ctx,
            camera.bind_group_layout(),
            light.bind_group_layout(),
            material.bind_group_layout(),
            target.format(),
        )
        .expect("pipeline");

        let mesh = LitMesh::from_vertices(ctx, &screen_quad()).expect("mesh");

        let mut encoder = ctx
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("LitTestEncoder"),
            });
        record_lit_mesh_pass(
            &mut encoder,
            &target,
            &pipeline,
            &camera,
            &light,
            &material,
            &mesh,
            clear,
        );
        ctx.queue().submit(std::iter::once(encoder.finish()));

        ReadbackBuffer::from_target(ctx, &target).expect("readback")
    }

    #[test]
    fn pipeline_compiles() {
        let ctx = ctx_or_skip!();
        let camera = Camera::new(&ctx).expect("camera");
        let light = DirectionalLight::new(&ctx).expect("light");
        let material = Material::new(&ctx, &white_4x4(), 4, 4).expect("material");
        let pipeline = LitMeshPipeline::new(
            &ctx,
            camera.bind_group_layout(),
            light.bind_group_layout(),
            material.bind_group_layout(),
            wgpu::TextureFormat::Rgba8Unorm,
        )
        .expect("pipeline compiles");
        let _p = pipeline.pipeline();
    }

    #[test]
    fn lit_white_quad_center_is_bright() {
        let ctx = ctx_or_skip!();
        // Light traveling in -Z direction; quad normal +Z. Lambert max ≈ 1.0.
        let buf = render_lit_quad(
            &ctx,
            glam::Vec3::new(0.0, 0.0, -1.0),
            &white_4x4(),
            4,
            4,
            wgpu::Color::BLACK,
        );
        let center = buf.pixel(32, 32).expect("center pixel exists");
        assert!(
            center.0 > 200 && center.1 > 200 && center.2 > 200,
            "lit center should be bright white, got rgba={center:?}"
        );
    }

    #[test]
    fn backlit_quad_center_is_ambient_only() {
        let ctx = ctx_or_skip!();
        // Light traveling in +Z direction; quad normal +Z; lambert clamps to 0.
        // Output ≈ ambient (0.1) × white * white_light = 0.1 → ~25/255.
        let buf = render_lit_quad(
            &ctx,
            glam::Vec3::new(0.0, 0.0, 1.0),
            &white_4x4(),
            4,
            4,
            wgpu::Color::BLACK,
        );
        let center = buf.pixel(32, 32).expect("center pixel exists");
        assert!(
            center.0 < 80 && center.1 < 80 && center.2 < 80,
            "backlit center should be dim (ambient only), got rgba={center:?}"
        );
    }

    #[test]
    fn checker_texture_produces_two_colors_in_quad() {
        let ctx = ctx_or_skip!();
        // Same lit setup; 2x2 checkerboard texture.  With AddressMode::Repeat
        // and FilterMode::Linear, the four texels span the quad — uv (0,0) is
        // the top-left of the texture (red), uv(1,1) is the bottom-right (red),
        // uv(0,1)+uv(1,0) are the off-diagonal (blue).
        //
        // The quad's uv layout (tl=0,0  tr=1,0  br=1,1  bl=0,1) means:
        //   - top-left pixel of the screen quad → uv near (0,0) → red texel
        //   - bottom-left pixel of the screen quad → uv near (0,1) → blue texel
        //   - top-right pixel of the screen quad → uv near (1,0) → blue texel
        //   - bottom-right pixel of the screen quad → uv near (1,1) → red texel
        //
        // We sample two corners (well inside the quad's texel cells) and expect
        // them to differ in their R-channel ordering.
        let buf = render_lit_quad(
            &ctx,
            glam::Vec3::new(0.0, 0.0, -1.0),
            &checker_2x2(),
            2,
            2,
            wgpu::Color::BLACK,
        );

        // Top-left region → red dominant (more R than B).
        let tl = buf.pixel(8, 8).expect("tl pixel exists");
        // Top-right region → blue dominant.
        let tr = buf.pixel(56, 8).expect("tr pixel exists");

        // The two regions must differ noticeably in red and blue channels —
        // proves the texture sampling is wired up correctly.
        let left_red = i32::from(tl.0);
        let left_blue = i32::from(tl.2);
        let right_red = i32::from(tr.0);
        let right_blue = i32::from(tr.2);
        assert!(
            (left_red - right_red).abs() > 30 || (left_blue - right_blue).abs() > 30,
            "top-left and top-right regions should differ: tl={tl:?} tr={tr:?}"
        );
    }

    #[test]
    fn lit_white_quad_alpha_is_opaque() {
        let ctx = ctx_or_skip!();
        let buf = render_lit_quad(
            &ctx,
            glam::Vec3::new(0.0, 0.0, -1.0),
            &white_4x4(),
            4,
            4,
            wgpu::Color::BLACK,
        );
        let center = buf.pixel(32, 32).expect("center pixel exists");
        assert_eq!(center.3, 255, "fragment writes alpha=1.0 (opaque)");
    }

    #[test]
    fn record_lit_pass_with_indexed_mesh_succeeds() {
        let ctx = ctx_or_skip!();
        // Same six vertices, but expressed as 4 + index buffer.
        let bl = VertexLit::new([-1.0, -1.0, -2.0], [0.0, 0.0, 1.0], [0.0, 1.0]);
        let br = VertexLit::new([1.0, -1.0, -2.0], [0.0, 0.0, 1.0], [1.0, 1.0]);
        let tr = VertexLit::new([1.0, 1.0, -2.0], [0.0, 0.0, 1.0], [1.0, 0.0]);
        let tl = VertexLit::new([-1.0, 1.0, -2.0], [0.0, 0.0, 1.0], [0.0, 0.0]);
        let verts = [bl, br, tr, tl];
        let indices: [u32; 6] = [0, 1, 2, 0, 2, 3];

        let mesh = LitMesh::from_indexed(&ctx, &verts, &indices).expect("indexed mesh");
        assert!(mesh.index_buffer().is_some());
        assert_eq!(mesh.index_buffer().unwrap().index_count(), 6);
        assert_eq!(mesh.vertex_buffer().vertex_count(), 4);

        let target = HeadlessTarget::new(&ctx, 64, 64).expect("target");
        let camera = Camera::new(&ctx).expect("camera");
        camera.update(&ctx, ortho_view_proj(), glam::Mat4::IDENTITY);
        let light = DirectionalLight::new(&ctx).expect("light");
        light.update(&ctx, glam::Vec3::new(0.0, 0.0, -1.0), glam::Vec3::ONE);
        let material = Material::new(&ctx, &white_4x4(), 4, 4).expect("material");
        let pipeline = LitMeshPipeline::new(
            &ctx,
            camera.bind_group_layout(),
            light.bind_group_layout(),
            material.bind_group_layout(),
            target.format(),
        )
        .expect("pipeline");

        let mut encoder = ctx
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("IndexedLitTestEncoder"),
            });
        record_lit_mesh_pass(
            &mut encoder,
            &target,
            &pipeline,
            &camera,
            &light,
            &material,
            &mesh,
            wgpu::Color::BLACK,
        );
        ctx.queue().submit(std::iter::once(encoder.finish()));

        // Readback and verify pixel is bright (proves indexed draw worked).
        let buf = ReadbackBuffer::from_target(&ctx, &target).expect("readback");
        let center = buf.pixel(32, 32).expect("center pixel exists");
        assert!(
            center.0 > 200,
            "indexed lit quad center should be bright, got rgba={center:?}"
        );
    }

    // -----------------------------------------------------------------------
    // `LitMesh::from_render_mesh` — sub-δ.1.A adapter
    //
    // Tests 1 + 5 are GPU-gated (they construct a real `LitMesh` via
    // `from_indexed`'s internal buffer upload). Tests 2 / 3 / 4 inspect the
    // CPU-side adapter `vertex_lit_from_render_mesh` directly — no GPU work,
    // no LitVertexBuffer field changes; the helper is `pub(super)`-visible
    // through the parent module's private free-function declaration.
    // -----------------------------------------------------------------------

    /// Build a single-triangle CCW `RenderMesh` (positions in the XY plane,
    /// normals = +Z) for use across the from_render_mesh tests.
    fn unit_triangle_render_mesh() -> RenderMesh {
        let positions: Vec<[f32; 3]> = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let normals: Vec<[f32; 3]> = vec![[0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0]];
        let indices: Vec<u32> = vec![0, 1, 2];
        let face_labels: Option<Vec<u64>> = Some(vec![42]);

        RenderMesh {
            positions,
            normals,
            indices,
            face_labels,
        }
    }

    #[test]
    fn from_render_mesh_creates_correct_vertex_count() {
        let ctx = ctx_or_skip!();
        let mesh_in = unit_triangle_render_mesh();
        let lit_mesh = LitMesh::from_render_mesh(&ctx, &mesh_in).expect("from_render_mesh");

        assert_eq!(
            lit_mesh.vertex_buffer().vertex_count(),
            3,
            "vertex count should match positions.len()"
        );
        assert!(lit_mesh.index_buffer().is_some(), "index buffer present");
        assert_eq!(
            lit_mesh.index_buffer().unwrap().index_count(),
            3,
            "index count should match indices.len()"
        );
    }

    #[test]
    fn from_render_mesh_uses_placeholder_uv_zero_zero() {
        let mesh_in = unit_triangle_render_mesh();
        let vertices = vertex_lit_from_render_mesh(&mesh_in);

        assert_eq!(vertices.len(), 3);
        for v in &vertices {
            assert_eq!(
                v.uv,
                [0.0, 0.0],
                "UV must be the documented placeholder [0.0, 0.0]"
            );
        }
    }

    #[test]
    fn from_render_mesh_preserves_positions() {
        let mesh_in = unit_triangle_render_mesh();
        let vertices = vertex_lit_from_render_mesh(&mesh_in);

        assert_eq!(vertices.len(), mesh_in.positions.len());
        for (out_v, &in_pos) in vertices.iter().zip(mesh_in.positions.iter()) {
            assert_eq!(
                out_v.position, in_pos,
                "position must round-trip element-for-element"
            );
        }
    }

    #[test]
    fn from_render_mesh_preserves_normals() {
        let mesh_in = unit_triangle_render_mesh();
        let vertices = vertex_lit_from_render_mesh(&mesh_in);

        assert_eq!(vertices.len(), mesh_in.normals.len());
        for (out_v, &in_norm) in vertices.iter().zip(mesh_in.normals.iter()) {
            assert_eq!(
                out_v.normal, in_norm,
                "normal must round-trip element-for-element"
            );
        }
    }

    #[test]
    fn from_render_mesh_returns_error_for_empty_input() {
        let ctx = ctx_or_skip!();
        let empty_mesh = RenderMesh {
            positions: vec![],
            normals: vec![],
            indices: vec![],
            face_labels: None,
        };

        let result = LitMesh::from_render_mesh(&ctx, &empty_mesh);
        assert!(matches!(result, Err(BufferError::Empty)));
    }
}
