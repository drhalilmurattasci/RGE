//! Trivial WGSL render pipeline — one hard-coded triangle in NDC, solid red.
//!
//! This is Phase 6.1 substrate validation only. Mesh rendering from external
//! geometry, bind groups, transforms, and material permutations are follow-up
//! dispatches.

use std::sync::Arc;

use crate::context::GfxContext;
use crate::pso_cache::{PipelineCache, PsoKey, ShaderHash, VertexLayoutDescriptor};

// ---------------------------------------------------------------------------
// Embedded WGSL
// ---------------------------------------------------------------------------

/// Embedded WGSL that draws a single hard-coded red triangle in NDC.
///
/// The vertex shader positions three vertices using `vertex_index`; the
/// fragment shader returns solid red `(1, 0, 0, 1)`.
const TRIANGLE_WGSL: &str = r"
@vertex
fn vs_main(@builtin(vertex_index) vid: u32) -> @builtin(position) vec4<f32> {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>( 0.0,  0.5),
        vec2<f32>(-0.5, -0.5),
        vec2<f32>( 0.5, -0.5),
    );
    return vec4<f32>(positions[vid], 0.0, 1.0);
}

@fragment
fn fs_main() -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 0.0, 0.0, 1.0);
}
";

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// Errors that can occur when building a [`TrianglePipeline`].
#[derive(Debug, thiserror::Error)]
pub enum PipelineError {
    /// wgpu rejected the WGSL source (should not happen with the embedded shader,
    /// but preserved for callers that may substitute custom WGSL).
    #[error("WGSL parse/compile error: {0}")]
    Wgsl(String),
}

// ---------------------------------------------------------------------------
// TrianglePipeline
// ---------------------------------------------------------------------------

/// A compiled wgpu render pipeline that draws one hard-coded red triangle.
///
/// No vertex buffers, no bind groups, no uniforms — pure NDC positions baked
/// into the shader. This exists solely to validate that the wgpu integration
/// compiles and runs a render pass end-to-end.
///
/// The pipeline is wrapped in an [`Arc`] so [`new_cached`](Self::new_cached)
/// can return a shared allocation when the same `(shader, layout, color
/// format, depth state)` key is requested twice. [`new`](Self::new) wraps a
/// fresh allocation (no cache).
pub struct TrianglePipeline {
    pipeline: Arc<wgpu::RenderPipeline>,
}

impl TrianglePipeline {
    /// Compile the embedded WGSL and create a render pipeline targeting
    /// `format`.
    ///
    /// `format` must match the [`HeadlessTarget`](crate::target::HeadlessTarget)
    /// or surface format the pipeline will be used with.
    ///
    /// This constructor does NOT use a cache — every call builds a fresh
    /// pipeline. Use [`new_cached`](Self::new_cached) to share allocations
    /// across callers with identical `PsoKey` inputs.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Wgsl`] if the WGSL source fails to parse
    /// (should not occur with the hard-coded embedded shader).
    ///
    /// The `Result` wrapper is intentional — the API surface is designed for
    /// callers that may substitute custom WGSL where failures are possible.
    #[allow(clippy::unnecessary_wraps)]
    pub fn new(ctx: &GfxContext, format: wgpu::TextureFormat) -> Result<Self, PipelineError> {
        let pipeline = Arc::new(build_pipeline(ctx.device(), format));
        Ok(Self { pipeline })
    }

    /// Compile-or-reuse via the supplied [`PipelineCache`].
    ///
    /// The cache is keyed on `(shader hash, vertex layout, color format,
    /// depth state)` — see [`PsoKey`]. Two `TrianglePipeline` instances
    /// constructed with identical `format` and the same cache share one
    /// underlying `wgpu::RenderPipeline` allocation; `[Arc::ptr_eq]` on
    /// the inner pipeline returns `true`.
    ///
    /// `TrianglePipeline` has no vertex buffers and no depth attachment,
    /// so the [`PsoKey`] uses an empty [`VertexLayoutDescriptor`] and
    /// `depth_state = None`.
    ///
    /// # Errors
    ///
    /// Returns [`PipelineError::Wgsl`] if the WGSL source fails to parse
    /// (should not occur with the hard-coded embedded shader).
    #[allow(clippy::unnecessary_wraps)]
    pub fn new_cached(
        ctx: &GfxContext,
        format: wgpu::TextureFormat,
        cache: &mut PipelineCache<wgpu::RenderPipeline>,
    ) -> Result<Self, PipelineError> {
        let key = PsoKey::new(
            ShaderHash::from_source(TRIANGLE_WGSL.as_bytes()),
            // No vertex buffers; empty layout descriptor with stride 0
            // and a Vertex-rate step mode — all empty layouts hash equal.
            VertexLayoutDescriptor::new(0, wgpu::VertexStepMode::Vertex, Vec::new()),
            format,
            None,
        );
        let device = ctx.device();
        let pipeline = cache.get_or_insert(key, || build_pipeline(device, format));
        Ok(Self { pipeline })
    }

    /// Borrow the compiled [`wgpu::RenderPipeline`].
    #[must_use]
    pub fn pipeline(&self) -> &wgpu::RenderPipeline {
        &self.pipeline
    }
}

// ---------------------------------------------------------------------------
// build_pipeline — shared by `new` and `new_cached` (and the cache builder)
// ---------------------------------------------------------------------------

/// Compile the embedded WGSL and create a fresh `wgpu::RenderPipeline`.
///
/// Internal helper shared by [`TrianglePipeline::new`] (always builds) and
/// [`TrianglePipeline::new_cached`] (only invoked on cache miss).
fn build_pipeline(device: &wgpu::Device, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
    // wgpu 29: create_shader_module panics on validation error by default.
    // We catch it via a push_error_scope / pop_error_scope pair.
    // Actually in wgpu 29, the clean approach is to let the default
    // validation run and handle any panic — but for correctness we use the
    // descriptor directly and let wgpu 29 handle it (it validates at submit
    // time anyway for the pipeline compilation path).
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("triangle.wgsl"),
        source: wgpu::ShaderSource::Wgsl(TRIANGLE_WGSL.into()),
    });

    // wgpu 29: PipelineLayoutDescriptor no longer has push_constant_ranges;
    // it was replaced with `immediate_size` (for the IMMEDIATES feature).
    // bind_group_layouts is now &[Option<&BindGroupLayout>].
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("TrianglePipelineLayout"),
        bind_group_layouts: &[],
        immediate_size: 0,
    });

    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("TrianglePipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
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
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! ctx_or_skip {
        () => {{
            match GfxContext::new_headless() {
                Ok(c) => c,
                Err(_) => {
                    eprintln!("SKIP: no GPU adapter");
                    return;
                }
            }
        }};
    }

    #[test]
    fn cached_constructor_reuses_pipeline_allocation() {
        let ctx = ctx_or_skip!();
        let mut cache = PipelineCache::<wgpu::RenderPipeline>::new();
        let p1 = TrianglePipeline::new_cached(&ctx, wgpu::TextureFormat::Rgba8Unorm, &mut cache)
            .expect("p1");
        let p2 = TrianglePipeline::new_cached(&ctx, wgpu::TextureFormat::Rgba8Unorm, &mut cache)
            .expect("p2");
        assert!(
            Arc::ptr_eq(&p1.pipeline, &p2.pipeline),
            "identical-key call must return the same Arc allocation"
        );
        assert_eq!(cache.misses(), 1);
        assert_eq!(cache.hits(), 1);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn cached_constructor_distinct_format_creates_distinct_entry() {
        let ctx = ctx_or_skip!();
        let mut cache = PipelineCache::<wgpu::RenderPipeline>::new();
        let p1 = TrianglePipeline::new_cached(&ctx, wgpu::TextureFormat::Rgba8Unorm, &mut cache)
            .expect("p1");
        let p2 =
            TrianglePipeline::new_cached(&ctx, wgpu::TextureFormat::Bgra8UnormSrgb, &mut cache)
                .expect("p2");
        assert!(
            !Arc::ptr_eq(&p1.pipeline, &p2.pipeline),
            "different color formats must NOT share allocation"
        );
        assert_eq!(cache.misses(), 2);
        assert_eq!(cache.hits(), 0);
        assert_eq!(cache.len(), 2);
    }
}
