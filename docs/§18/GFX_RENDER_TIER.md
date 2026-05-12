# GFX_RENDER_TIER

| Companion to | PLAN.md §1.5.2 (render-side snapshot staging) + PLAN.md §6 (frame loop) + PLAN.md §13.6 (render-side snapshot gate) |
|---|---|
| Status | Stable v1 for Phase 6.1 substrate + Phase 6 PBR-lite (60 tests; Lambert+Phong + texture sampling shipped 2026-05-06; canary `GfxPlugin` 16 tests). Frame-graph + render-snapshot separation per §1.5.2 + material-runtime / PSO cache + 60 fps simple-scene gate are still pending. |
| Audience | gfx authors extending the render tier; canary authors needing GPU resources via `PluginContext` owned-handoff; future PIE participant authors implementing `gfx.render-snapshot` |
| Sibling doc | `PLUGIN_HOST_PATTERNS.md` Pattern B (lazy-build) — `GfxPlugin` is the canonical lazy-build canary; `PIE_SNAPSHOT.md` — future `gfx.render-snapshot` participant for the §1.5.2 sim/render thread boundary |
| Reference impls | `crates/gfx/src/{lib,context,target,frame,pipeline,buffer,vertex,vertex_lit,mesh,mesh_pipeline,transform,camera,light,material,lit_mesh_pipeline,plugin_adapter}.rs` (substrate + canary) · `crates/gfx/tests/{headless_triangle,mesh_quad,plugin_adapter_smoke}.rs` (integration) |

> Convention defined by `PLUGIN_HOST_PATTERNS.md` §header. This doc is the workspace-wide reference for the gfx render-tier substrate as it stands today (Phase 6.1 + PBR-lite shipped); subsystem-specific render integrations (the future `editor-ui` viewport, the future material-graph runtime) will document their consumer surfaces in their own §18 docs.

**Elaborates**: SCENE_EXTRACTION_CONTRACT.md §3 (Layer-4 — render state is downstream-only) + REACTIVE_INVALIDATION.md §1 (Layer 4 — GPU upload).

## 1. Render-tier separation per §1.5.2

PLAN §1.5.2 specifies the render-thread / sim-thread separation: render thread sees an immutable snapshot of `(ECS_tick_N, CadCheckpointId_N)` while sim builds N+1. The substrate is being built **incrementally**:

- **Phase 6.1 substrate** — wgpu init + headless triangle + mesh rendering + transforms via `Transform` UBO. Shipped pre-2026-05-06.
- **Phase 6 PBR-lite** — single-light Lambert+Phong + texture sampling, with pixel-level lit/backlit/checker assertions passing on RTX 4060 Ti / Vulkan. Shipped 2026-05-06.
- **GfxPlugin canary** — 16 tests including ContractViolation paths + multi-plugin isolation. Shipped 2026-05-06.

Pending Phase 6 work (deferred to follow-up dispatches):

- **Frame-graph** — transient resource lifetimes computed at frame begin; `TexturePool` / `BufferPool` keyed on frame index; declarative pass DAG with read/write resource declarations.
- **Render-snapshot separation** — gfx implements `SnapshotParticipate` for render-side state replicated across the sim/render thread boundary; the future `gfx.render-snapshot` participant per PLAN §13.2.
- **Material-runtime + PSO cache** — pipeline state objects keyed on shader hash + vertex layout so 100 material instances share one PSO.
- **60 fps simple-scene golden gate** — 1k cubes + 1 directional light at 60 fps target.

Today's substrate is therefore **single-threaded headless rendering with a UBO-driven mesh + lit-mesh pipeline** — sufficient for canary tests + headless CI + PBR-lite pixel verification, but not yet the §1.5.2 render-thread / sim-thread split.

## 2. `GfxContext`

Lives at `crates/gfx/src/context.rs`. Wraps `wgpu::Instance` / `wgpu::Adapter` / `wgpu::Device` / `wgpu::Queue`:

```rust
pub struct GfxContext {
    instance: wgpu::Instance,
    adapter: wgpu::Adapter,
    device: wgpu::Device,
    queue: wgpu::Queue,
}
```

`new_headless()` runs `pollster::block_on(init_async())` so the crate stays sync-only at the public surface (no `tokio` / `futures` dep). `init_async` requests the high-performance adapter via `Backends::all()` so the best backend per-platform is selected (Vulkan on Win/Linux; Metal on macOS; DX12 / OpenGL / WebGL fallbacks). Returns:

- **`GfxContextError::NoAdapter`** when no GPU adapter is available — headless CI runners without a virtual GPU surface this. Callers MUST check for it and skip GPU-dependent work gracefully (the test fixtures use a `ctx_or_skip!` macro pattern).
- **`GfxContextError::DeviceRequest(String)`** when adapter found but device creation failed. Recoverable by callers (e.g. retry with a lower feature set).

`Send + Sync` is documented at the wgpu 29 level; `GfxContext` inherits these bounds and satisfies the `PluginContext::insert<T: Any + Send>` requirement (cross-ref `PLUGIN_API.md` §2.3 for the registry surface). The `GfxPlugin` canary uses this property to take/insert `GfxContext` through the registry without `Mutex` wrapping.

`adapter_info()` returns the `wgpu::AdapterInfo` (name, backend, driver version) for diagnostics / logging without taking a mutable borrow. `instance()` is exposed for future surface-creation work (winit integration).

## 3. `HeadlessTarget`

Lives at `crates/gfx/src/target.rs`. A GPU texture suitable for use as a render target with CPU readback:

```rust
pub struct HeadlessTarget {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
}
```

Format is always [`wgpu::TextureFormat::Rgba8Unorm`]. Usage flags: `RENDER_ATTACHMENT | COPY_SRC` so the same texture can be (a) the destination of a render pass and (b) the source of a `copy_texture_to_buffer` for CPU readback. Size is bounded `1..=8192` per axis; out-of-range returns `TargetError::InvalidSize(w, h)`.

Used as the destination for canary frames + the future render-snapshot participant's transient frame buffer. The `Rgba8Unorm` (linear) format is deliberately distinct from `Rgba8UnormSrgb` (the format used by `Material`'s base-colour texture in PBR-lite §8) — the target is linear because shaders write linear values and the GPU's sRGB-aware target format would otherwise apply double gamma.

## 4. `FrameRecorder` + `ReadbackBuffer`

Lives at `crates/gfx/src/frame.rs`. Single-frame command recording + CPU readback:

```rust
pub struct FrameRecorder<'ctx> { ctx: &'ctx GfxContext, encoder: wgpu::CommandEncoder }
pub struct ReadbackBuffer { pub pixels: Vec<u8>, pub width: u32, pub height: u32 }
```

`FrameRecorder::new(ctx)` allocates a `wgpu::CommandEncoder`. `render_triangle(&mut self, &target, &pipeline, clear)` records one render pass that clears `target` to `clear` then draws the triangle via `pipeline`. `submit(self)` is consumed-by-value to prevent double-submission; it submits the encoder's command buffer to `ctx.queue()`.

`ReadbackBuffer::from_target(ctx, target)` allocates a CPU-visible staging buffer, copies the texture into it, maps + reads, and **strips the row-alignment padding** that wgpu requires internally (`COPY_BYTES_PER_ROW_ALIGNMENT` = 256 bytes). The result is a tightly-packed `width × height × 4` byte vector — what tests assert against. The synchronous map uses `device.poll(PollType::wait_indefinitely())` per the wgpu 29 quirk (cross-ref §10).

`pixel(x, y) -> Option<(u8, u8, u8, u8)>` accessor for tests to sample arbitrary pixels with bounds checking.

`FrameError::Readback(String)` covers buffer-map failures; `FrameError::InvalidClearColor` is reserved for out-of-range clear-color components (currently unused but preserved in the API for future validation).

## 5. `Vertex` + `VertexBuffer` + `Mesh`

Lives at `crates/gfx/src/vertex.rs`, `buffer.rs`, `mesh.rs`. The unlit vertex pipeline used by the `MeshPipeline`:

```rust
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct Vertex { pub position: [f32; 3], pub color: [f32; 3] }
```

24 bytes, `#[repr(C)]`, `Pod + Zeroable` so `bytemuck::cast_slice` produces the GPU upload bytes directly. `layout()` returns the `wgpu::VertexBufferLayout` with `position` at `@location(0)` (`Float32x3`) and `color` at `@location(1)` (`Float32x3`); stride = 24.

`VertexBuffer::new(ctx, vertices) -> Result<Self, BufferError>` allocates a `VERTEX | COPY_DST` buffer and uploads via `create_buffer_init`. `IndexBuffer::new(ctx, indices)` is symmetric for `INDEX | COPY_DST`. Empty slices return `BufferError::Empty`.

`Mesh::from_vertices(ctx, &vertices)` creates a non-indexed mesh; `Mesh::from_indexed(ctx, &vertices, &indices)` creates an indexed mesh (vertex buffer + index buffer in one struct).

## 6. `Transform` UBO

Lives at `crates/gfx/src/transform.rs`. A single `mat4x4<f32>` uniform buffer with bind group + bind group layout, intended for the unlit `MeshPipeline` at `@group(0) @binding(0)`:

```rust
pub struct Transform {
    buffer: wgpu::Buffer,             // 64 bytes; UNIFORM | COPY_DST
    bind_group: wgpu::BindGroup,
    bind_group_layout: wgpu::BindGroupLayout,
}
```

`Transform::new(ctx)` allocates the 64-byte buffer pre-filled with the identity matrix. Visibility = `ShaderStages::VERTEX`. `min_binding_size` is the matrix size (64).

`update(&self, ctx, matrix: glam::Mat4)` reuploads the matrix via `Queue::write_buffer`. WGSL `mat4x4<f32>` is column-major and `glam::Mat4` is column-major internally, so `mat.to_cols_array()` produces the correct byte layout with no transposition. The bind group remains valid across `update` calls — the buffer is the same; only its contents change.

Used by `MeshPipeline` to pose a mesh per-frame. Cross-ref the `mesh_pipeline.rs` source for the canonical bind-group-0 layout.

## 7. PBR-lite types (Phase 6 dispatch)

The PBR-lite stack landed 2026-05-06 with pixel-level Lambert+Phong + texture sampling on RTX 4060 Ti / Vulkan.

### `VertexLit`

Lives at `vertex_lit.rs`. Lit vertex format:

```rust
#[repr(C)]
#[derive(Pod, Zeroable, ...)]
pub struct VertexLit { pub position: [f32; 3], pub normal: [f32; 3], pub uv: [f32; 2] }
```

32 bytes. Locations: position@0 (`Float32x3`), normal@1 (`Float32x3`), uv@2 (`Float32x2`). Used by `LitMeshPipeline` for Lambert+Phong shading with a base-colour texture.

### `Camera` UBO

Lives at `camera.rs`. 128-byte UBO at `@group(0) @binding(0)`:

| offset | field           | type            | size |
|--------|-----------------|-----------------|------|
| 0      | `view_proj`     | `mat4x4<f32>`   | 64   |
| 64     | `normal_matrix` | `mat4x4<f32>`   | 64   |

`normal_matrix` is `(model.inverse().transpose())` — the standard correction for transforming surface normals through non-uniform scale. Stored as a full 4×4 even though only the top-left 3×3 is used by the shader (WGSL `mat4x4<f32>` aligns at 16 bytes; a 3×3 would cost the same after padding).

### `DirectionalLight` UBO

Lives at `light.rs`. 32-byte UBO at `@group(1) @binding(0)`:

| offset | field       | type        |
|--------|-------------|-------------|
| 0      | `direction` | `vec3<f32>` |
| 12     | `_pad0`     | `f32`       |
| 16     | `color`     | `vec3<f32>` |
| 28     | `_pad1`     | `f32`       |

The `_pad0` / `_pad1` fields are explicit because WGSL std140 alignment reserves 16 bytes per `vec3<f32>` (the trailing 4 are padding). Mirroring this on the CPU side ensures `bytemuck::bytes_of` produces exactly the layout WGSL expects.

### `Material` (UBO + texture + sampler)

Lives at `material.rs`. Three bindings at `@group(2)`:

| binding | resource              | format / size       |
|---------|-----------------------|---------------------|
| 0       | uniform buffer        | 32 bytes (2× vec4)  |
| 1       | 2D texture (sampled)  | `Rgba8UnormSrgb`    |
| 2       | sampler (filtering)   | linear / repeat     |

UBO contents pack `(base_color: vec4, phong: vec4)` where `phong` packs `(ambient, diffuse, specular, shininess)` in `(x, y, z, w)`. `Rgba8UnormSrgb` (sRGB) is intentional for the base-colour texture so the GPU performs gamma decode on sample. Sampler is `MipmapFilterMode::Nearest` + `min/mag = Linear`. Cross-ref `upload_rgba8_srgb_2d` helper for one-shot texture upload.

### `LitMeshPipeline`

Lives at `lit_mesh_pipeline.rs`. The Lambert+Phong WGSL pipeline that consumes all five resources above. Shader does Lambert + Phong in **world space**; view direction is computed from `world_pos` assuming the camera sits at the world origin (a v0 simplification — when an explicit camera position is added in a later phase, replace `view_dir = normalize(-world_pos)` with `normalize(camera_pos - world_pos)`).

Output target is **linear** — caller writes linear `(r, g, b, 1.0)` and the `HeadlessTarget`'s `Rgba8Unorm` (linear) format passes the values straight through to readback. Pixel-level lit/backlit/checker assertions in the integration tests cover the lit-frame golden case.

`record_lit_mesh_pass(...)` is the top-level helper that records one render pass with all bind groups + draw call + cleanup. Cross-ref the source for the full bind-group setup sequence.

## 8. `GfxPlugin` canary

Lives at `crates/gfx/src/plugin_adapter.rs`. Tier-2 plugin canary per PLAN §10.4 dogfood rule. Implements [`Plugin`] with the lazy-build pattern (cross-ref `PLUGIN_HOST_PATTERNS.md` §4 for Pattern B):

```rust
pub struct GfxPlugin {
    clear: wgpu::Color,
    pipeline: Option<TrianglePipeline>,   // lazy: built on first tick
    frames_recorded: u64,
}
pub const GFX_PLUGIN_ID: &str = "rge-gfx.headless-triangle-plugin";
```

**Why lazy-build:** `TrianglePipeline::new(ctx, format)` requires a live `&GfxContext` (the device handle) and the target's format. Both are caller-staged into the `PluginContext` registry; neither is available at `GfxPlugin` construction time. Plugin construction therefore stores `pipeline: None` and `init` is a no-op (returns `Ok(())`). The first `tick` takes `GfxContext` + `HeadlessTarget` from the registry, lazily builds the pipeline against the target's format, records a frame, submits, and puts both resources back.

**Resource contract on `tick`:** the `PluginContext` MUST contain `GfxContext` + `HeadlessTarget`. Missing either surfaces as `PluginError::ContractViolation` (caller-supplied resource missing — NOT a plugin-side bug; auto-emit downgrades to a warning per audit-2 A5.1, cross-ref `KERNEL_DIAGNOSTICS.md` §9). Pipeline build / queue submission errors surface as `PluginError::RuntimeFault` — the plugin code itself misbehaved or the GPU rejected the work. In every error path the resources that WERE supplied are put back into the context before the error propagates (idempotent failure semantics, matching the cad-projection precedent).

**Send + 'static bound:** wgpu 29's `Device` / `Queue` / `Texture` / etc. are all `Send + Sync`, so `GfxContext` and `HeadlessTarget` satisfy the `PluginContext::insert<T: Any + Send>` requirement without `Mutex` wrapping. This is a key data point for ADR-114 — the design generalises cleanly to GPU resources without forcing a non-Send compromise.

**Test surface:** 16 tests in `plugin_adapter.rs` + integration tests in `tests/plugin_adapter_smoke.rs`. Coverage includes `ContractViolation` paths for both missing resources, `RuntimeFault` for pipeline build failures, success paths verifying `frames_recorded` increment, multi-plugin isolation (running two `GfxPlugin` instances side-by-side in the same orchestrator).

## 9. Test surface

Total **60 tests** across the gfx crate per the verification run. Distribution (verified via `cargo test -p rge-gfx`):

- **48 unit tests** — across `context.rs`, `target.rs`, `frame.rs`, `pipeline.rs`, `vertex.rs`, `vertex_lit.rs`, `buffer.rs`, `mesh.rs`, `transform.rs`, `mesh_pipeline.rs`, `camera.rs`, `light.rs`, `material.rs`, `lit_mesh_pipeline.rs`, `plugin_adapter.rs` modules.
- **3 + 2 + 7 integration tests** — `tests/headless_triangle.rs` (3), `tests/mesh_quad.rs` (2), `tests/plugin_adapter_smoke.rs` (7).

GPU-dependent tests use the `ctx_or_skip!` macro pattern: if `GfxContext::new_headless()` returns `NoAdapter`, the test prints `SKIP: no GPU adapter` and returns early. This keeps the test surface CI-runnable on machines without virtual GPUs.

## 10. wgpu 29 quirks

A list of wgpu 29 quirks discovered during Phase 6.1 + PBR-lite dispatches, captured here so future gfx authors don't re-discover the same friction:

- **`Queue::write_texture`** takes `TexelCopyTextureInfo` by value, not by ref (was `ImageCopyTexture` reference in earlier wgpu). See `material.rs:144` for the canonical use.
- **`SamplerDescriptor.mipmap_filter`** is `MipmapFilterMode` — a distinct re-exported type from `FilterMode`. `material.rs:172` uses `MipmapFilterMode::Nearest`.
- **`bytemuck::cast_slice(&[ubo])` lifetime issue** — `cast_slice` over a single-element array creates a temporary slice that the borrow checker rejects as a lifetime conflict. Use `bytemuck::bytes_of(&ubo)` instead. Verified across `camera.rs`, `light.rs`, `material.rs`, `vertex_lit.rs`.
- **`Instance::new_without_display_handle()`** — `InstanceDescriptor` has no `Default` impl in wgpu 29; use `..wgpu::InstanceDescriptor::new_without_display_handle()` to fill safe defaults then override `backends`. See `context.rs:62`.
- **`request_adapter` returns `Result<_, RequestAdapterError>`** (was `Option<Adapter>`). Map the error to `GfxContextError::NoAdapter`. See `context.rs:74`.
- **`multiview` → `multiview_mask`** on `RenderPassDescriptor` (and the `RenderPipelineDescriptor`'s analogous field). `None` for non-multiview passes. See `frame.rs:78`, `lit_mesh_pipeline.rs:327` / `:377`, `mesh_pipeline.rs:147` / `:196`.
- **`Maintain::Wait` → `PollType::wait_indefinitely()`** for synchronous device-poll. See `frame.rs:178`.
- **`PipelineLayoutDescriptor.bind_group_layouts`** is `&[Option<&BindGroupLayout>]` (NOT `&[&BindGroupLayout]`). Empty layouts use `&[]`; layouts with `None` slots leave that bind-group index unused.
- **`PipelineLayoutDescriptor.push_constant_ranges`** has been replaced with `immediate_size: u32` (for the `IMMEDIATES` feature). See `pipeline.rs:99`.
- **`BufferViewMut`** doesn't impl `IndexMut` — use slice access patterns instead of bracket indexing on a mapped mutable view.
- **`RenderPassColorAttachment.depth_slice: Option<u32>`** is a new field; `None` for non-3D textures. See `frame.rs:69`.

## 11. Pending Phase 6 work

The follow-up work tracked by HANDOFF.md / Status.md per the canonical Phase 6 roadmap:

### Frame-graph (minimal)

Transient resource lifetimes computed at frame begin. `TexturePool` / `BufferPool` keyed on frame index so allocator pressure stabilises. Declarative pass DAG with read/write resource declarations — the frame graph derives the per-pass barriers and resource lifetimes from the declarations rather than letting the gfx author hand-write them. Necessary precursor for the §1.5.2 sim/render thread split because the frame graph is what owns the per-frame lifetime decisions. **Allocator policy pinned by ADR-118** (descriptor-keyed pools, ring-buffered across N frames-in-flight, trust wgpu hazard tracking) — substrate-shipped (analytical) + ADR-118 (policy) + dispatch 119 (descriptors per ADR-118 D7) + dispatch 120 (`TexturePool`) + dispatch 121 (`BufferPool` clean mirror) + dispatch 122 (`ResourceMap` builder + `AliasingGroup::max_descriptor`) + dispatch 123 (umbrella analytical-composition smoke) is the implementation arc. Pass-record-site integration (`FrameRecorder` / `record_lit_mesh_pass` consuming transient resources) is intentional future work — those sites have no transient-resource consumers today, and `FrameRecorder` is currently triangle-only and bypassed by `editor-shell::render_frame`; the substrate is complete enough to enable that wiring at zero cost when consumer pressure surfaces.

### Render-snapshot separation per §1.5.2

gfx implements `SnapshotParticipate` for the render-side state replicated across the sim/render thread boundary. Future participant id: `gfx.render-snapshot`. Cross-ref `PIE_SNAPSHOT.md` §2 for the trait surface; the participant payload format is gfx's choice (likely a postcard-encoded snapshot of camera + light + material handles + mesh handles, all cad-projection-anchored to keep cross-architecture coherence per PLAN §13.2).

### Material-runtime + PSO cache

Pipeline state objects keyed on `(shader_hash, vertex_layout)` so 100 material instances of the same shader share one PSO. Material-runtime is the registry that resolves `MaterialId` → `&PipelineState`. Naga validation runs ahead-of-time per PLAN §1.13's "shader compile timeout" recoverable failure class — a malformed shader becomes a placeholder pipeline, not a hard fail.

### 60 fps simple-scene golden gate

The PLAN §13.6 gate — 1k cubes + 1 directional light at 60 fps target. Until this lands the gfx crate has no perf regression detection.

## 12. Failure class — recoverable

Per the `//! Failure class: recoverable` declaration at `crates/gfx/src/lib.rs` and PLAN §1.13. The substrate's failure modes are recoverable:

- GPU init failure (`NoAdapter`) — the editor falls back to a software path or surfaces a diagnostic.
- Pipeline compile error (`PipelineError::Wgsl`) — recoverable; callers may substitute custom WGSL or use the embedded fallback.
- Buffer / texture / sampler creation errors — surface as the typed `Error` returns (`BufferError`, `TargetError`, `MaterialError`, `TextureUploadError`, `MeshPipelineError`, `LitMeshPipelineError`, `TransformError`, `CameraError`, `LightError`).
- Readback failures (`FrameError::Readback`) — recoverable; the next frame can retry.

The `architecture-lints` `failure-class` lint enforces the declaration; `crates/gfx` does not appear in the failure-class exemptions table. Diagnostic emission for runtime faults at the canary level routes through `PluginContext::emit_diagnostic` per the auto-emit policy in `KERNEL_DIAGNOSTICS.md` §9.

## 13. References

- **PLAN.md §1.5.2** — render-side snapshot staging (the design target the substrate is building toward).
- **PLAN.md §6** — frame loop (overall lifecycle).
- **PLAN.md §10.4** — Tier-2 plugin canary dogfood rule.
- **PLAN.md §13.2** — cross-architecture coherence quality gate (`gfx.render-snapshot` future participant).
- **PLAN.md §13.6** — render-side snapshot + 1k-cube golden gate (pending).
- **PLAN.md §1.13** — failure-class taxonomy (recoverable definition).
- **`PLUGIN_HOST_PATTERNS.md`** — sibling §18 doc; Pattern B (lazy-build) described against `GfxPlugin` as the canonical example.
- **`PLUGIN_API.md`** — sibling §18 doc; `Plugin` trait + `PluginContext` resource registry surface that `GfxPlugin` consumes.
- **`PIE_SNAPSHOT.md`** — sibling §18 doc; future `gfx.render-snapshot` participant target.
- **`KERNEL_DIAGNOSTICS.md`** — sibling §18 doc; auto-emit policy for `ContractViolation` warnings on missing-resource canary errors.
- **`crates/gfx/src/lib.rs`** — module roots + failure-class declaration + Phase 6.1 / PBR-lite / canary module map.
- **`crates/gfx/src/context.rs`** — `GfxContext` + `GfxContextError` + `new_headless` + adapter selection.
- **`crates/gfx/src/target.rs`** — `HeadlessTarget` + `TargetError`.
- **`crates/gfx/src/frame.rs`** — `FrameRecorder` + `ReadbackBuffer` + `FrameError` + `COPY_BYTES_PER_ROW_ALIGNMENT` row padding handling.
- **`crates/gfx/src/pipeline.rs`** — `TrianglePipeline` + embedded WGSL.
- **`crates/gfx/src/{vertex,buffer,mesh,transform,mesh_pipeline}.rs`** — unlit pipeline.
- **`crates/gfx/src/{vertex_lit,camera,light,material,lit_mesh_pipeline}.rs`** — PBR-lite pipeline.
- **`crates/gfx/src/plugin_adapter.rs`** — `GfxPlugin` canary + `GFX_PLUGIN_ID` + lazy-build pattern + `Plugin` impl.
- **`crates/gfx/tests/{headless_triangle,mesh_quad,plugin_adapter_smoke}.rs`** — integration test surface.
