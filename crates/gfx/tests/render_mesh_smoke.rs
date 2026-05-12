//! Sub-δ.1.A integration test — full pipeline integration of
//! `RenderMesh → LitMesh::from_render_mesh → record_lit_mesh_pass → readback`.
//!
//! Constructs a hand-built 1×1×1 cuboid RenderMesh in canonical form
//! (8 unique corners, 12 triangles in NegZ→PosZ→NegY→PosY→NegX→PosX
//! face order, 12 face_labels) and proves the chain renders something
//! visible into a HeadlessTarget. This is the LOAD-BEARING test for
//! sub-δ.1.A — if it passes, sub-δ.1.B can wire the chain to a real
//! Surface and the visual moment lands.
//!
//! The test is hardware-gated via `ctx_or_skip` (mirrors the unit-test
//! pattern in `lit_mesh_pipeline.rs`); CI runners without a GPU adapter
//! skip-and-report.

use rge_brep_render::RenderMesh;
use rge_gfx::{
    record_lit_mesh_pass, Camera, DirectionalLight, GfxContext, HeadlessTarget, LitMesh,
    LitMeshPipeline, Material, ReadbackBuffer,
};

// ---------------------------------------------------------------------------
// Hardware gate
// ---------------------------------------------------------------------------

fn ctx_or_skip() -> Option<GfxContext> {
    match GfxContext::new_headless() {
        Ok(c) => Some(c),
        Err(_) => {
            eprintln!("SKIP: no GPU adapter — skipping render_mesh_smoke test");
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Synthetic 1×1×1 cuboid `RenderMesh` (canonical 12-triangle layout)
// ---------------------------------------------------------------------------

/// Build a 1×1×1 cuboid spanning `[-0.5, 0.5]` in all three axes, in the
/// canonical `CuboidOp` face emission order (NegZ→PosZ→NegY→PosY→NegX→PosX,
/// `TopologyFaceId(0..6)`). 12 triangles total, 12 face_labels.
///
/// Mirrors the layout `crates/brep-render/tests/render_mesh_smoke.rs` uses
/// for the cuboid face-normal verification — no cross-crate import (would
/// break the renderer-tier game-domain rule), but the structure is
/// identical so the resulting `RenderMesh` is wire-compatible with
/// brep-render's contract.
fn unit_cuboid_render_mesh() -> RenderMesh {
    // 8 unique corners.
    let positions: Vec<[f32; 3]> = vec![
        [-0.5, -0.5, -0.5], // 0 — NNN (bottom-back-left)
        [0.5, -0.5, -0.5],  // 1 — PNN
        [0.5, 0.5, -0.5],   // 2 — PPN
        [-0.5, 0.5, -0.5],  // 3 — NPN
        [-0.5, -0.5, 0.5],  // 4 — NNP
        [0.5, -0.5, 0.5],   // 5 — PNP
        [0.5, 0.5, 0.5],    // 6 — PPP
        [-0.5, 0.5, 0.5],   // 7 — NPP
    ];

    // 12 triangles — 2 per face — in canonical face order. Each pair
    // shares the face's outward normal. Winding is CCW when viewed from
    // outside the cuboid (matching `LitMeshPipeline`'s
    // `front_face: Ccw`).
    let indices: Vec<u32> = vec![
        // NegZ face (back, normal = -Z): viewed from -Z side, CCW is
        // 0 → 3 → 2, 0 → 2 → 1.
        0, 3, 2, 0, 2, 1, //
        // PosZ face (front, normal = +Z): CCW from +Z is 4 → 5 → 6,
        // 4 → 6 → 7.
        4, 5, 6, 4, 6, 7, //
        // NegY face (bottom, normal = -Y): CCW from -Y is 0 → 1 → 5,
        // 0 → 5 → 4.
        0, 1, 5, 0, 5, 4, //
        // PosY face (top, normal = +Y): CCW from +Y is 3 → 7 → 6,
        // 3 → 6 → 2.
        3, 7, 6, 3, 6, 2, //
        // NegX face (left, normal = -X): CCW from -X is 0 → 4 → 7,
        // 0 → 7 → 3.
        0, 4, 7, 0, 7, 3, //
        // PosX face (right, normal = +X): CCW from +X is 1 → 2 → 6,
        // 1 → 6 → 5.
        1, 2, 6, 1, 6, 5,
    ];

    let face_labels: Option<Vec<u64>> = Some(vec![
        0, 0, // NegZ (TopologyFaceId(0))
        1, 1, // PosZ (TopologyFaceId(1))
        2, 2, // NegY (TopologyFaceId(2))
        3, 3, // PosY (TopologyFaceId(3))
        4, 4, // NegX (TopologyFaceId(4))
        5, 5, // PosX (TopologyFaceId(5))
    ]);

    RenderMesh::from_buffers(&positions, &indices, face_labels.as_deref())
}

// ---------------------------------------------------------------------------
// LOAD-BEARING integration test
// ---------------------------------------------------------------------------

/// Construct a 1×1×1 cuboid RenderMesh, build a LitMesh via
/// `LitMesh::from_render_mesh`, render the lit mesh into a 64×64
/// HeadlessTarget with a known orthographic camera looking down -Z onto
/// the cuboid's `PosZ` face, read back the pixels, and assert that at
/// least one center-region pixel differs from the clear color.
///
/// This is the load-bearing test for sub-δ.1.A: if it passes, the
/// `RenderMesh → LitMesh → render-pass → pixels` chain is end-to-end
/// correct, and sub-δ.1.B can wire it to a real Surface to put the
/// triangle on screen.
#[test]
fn render_cuboid_via_from_render_mesh_pixel_readback() {
    let Some(ctx) = ctx_or_skip() else {
        return;
    };

    // 1. Hand-build the cuboid RenderMesh.
    let render_mesh = unit_cuboid_render_mesh();
    assert_eq!(render_mesh.positions.len(), 36, "12 triangles × 3 vertices");
    assert_eq!(render_mesh.normals.len(), 36);
    assert_eq!(render_mesh.indices.len(), 36);
    assert_eq!(render_mesh.face_labels.as_ref().unwrap().len(), 12);

    // 2. Construct GfxContext (already done) + LitMeshPipeline + HeadlessTarget.
    let target = HeadlessTarget::new(&ctx, 64, 64).expect("headless target");

    // 3. Build LitMesh via the new adapter.
    let lit_mesh = LitMesh::from_render_mesh(&ctx, &render_mesh).expect("from_render_mesh");
    assert_eq!(lit_mesh.vertex_buffer().vertex_count(), 36);
    assert_eq!(lit_mesh.index_buffer().unwrap().index_count(), 36);

    // 4. Camera: orthographic looking down -Z, sized to fit the cuboid in
    //    the viewport. Camera at (0, 0, +5), looking at origin with up=+Y.
    //    Ortho range [-1, 1] × [-1, 1] × [0.1, 20] easily covers the unit
    //    cuboid centered at origin.
    let camera = Camera::new(&ctx).expect("camera");
    let view = glam::Mat4::look_at_rh(
        glam::Vec3::new(0.0, 0.0, 5.0),
        glam::Vec3::ZERO,
        glam::Vec3::Y,
    );
    let proj = glam::Mat4::orthographic_rh(-1.0, 1.0, -1.0, 1.0, 0.1, 20.0);
    camera.update(&ctx, proj * view, glam::Mat4::IDENTITY);

    // 5. Default DirectionalLight pointing in -Z — illuminates the +Z face
    //    that the camera looks at. Color = white (1, 1, 1).
    let light = DirectionalLight::new(&ctx).expect("light");
    light.update(&ctx, glam::Vec3::new(0.0, 0.0, -1.0), glam::Vec3::ONE);

    // 6. Default Material: 4×4 white texture (matches existing tests'
    //    `white_4x4` pattern; texture is sampled by the shader, default
    //    base_color is white, default phong factors).
    let white_4x4: Vec<u8> = vec![255u8; 4 * 4 * 4];
    let material = Material::new(&ctx, &white_4x4, 4, 4).expect("material");

    // 7. Pipeline matching the target's color format.
    let pipeline = LitMeshPipeline::new(
        &ctx,
        camera.bind_group_layout(),
        light.bind_group_layout(),
        material.bind_group_layout(),
        target.format(),
    )
    .expect("pipeline");

    // 8. Encode + submit the render pass; clear color = pure black so any
    //    rasterised pixel is unambiguously "not clear color".
    let mut encoder = ctx
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("RenderMeshSmokeEncoder"),
        });
    record_lit_mesh_pass(
        &mut encoder,
        &target,
        &pipeline,
        &camera,
        &light,
        &material,
        &lit_mesh,
        wgpu::Color::BLACK,
        None,
    );
    ctx.queue().submit(std::iter::once(encoder.finish()));

    // 9. Read back HeadlessTarget pixels.
    let buf = ReadbackBuffer::from_target(&ctx, &target).expect("readback");

    // 10. Assert at least one pixel in the center 16×16 region is NOT the
    //     clear color (pure black). The cuboid's projected silhouette in
    //     the `[-1,1]` ortho range fills almost the entire 64×64 viewport,
    //     so any of those center pixels should be lit by the +Z face.
    let mut non_clear_count: u32 = 0;
    for y in 24..40 {
        for x in 24..40 {
            if let Some((r, g, b, a)) = buf.pixel(x, y) {
                if r > 0 || g > 0 || b > 0 || a != 255 {
                    // Either a non-zero RGB (cuboid lit something) or alpha
                    // != 255 (writing 1.0 should give 255). The shader
                    // writes alpha=1.0 ⇒ 255 in Rgba8Unorm; we look for
                    // RGB > 0 specifically.
                    if r > 0 || g > 0 || b > 0 {
                        non_clear_count += 1;
                    }
                }
            }
        }
    }
    assert!(
        non_clear_count > 0,
        "expected at least one non-clear pixel in the center region — \
         RenderMesh → LitMesh → render-pass chain produced no rasterised pixels"
    );
}
