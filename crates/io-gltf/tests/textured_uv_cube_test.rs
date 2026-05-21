//! Dispatch M2 — textured-UV-cube fixture integration tests.
//!
//! Pairs UV propagation (M1) with embedded image extraction (L) +
//! material image-handle wiring. The fixture is a UV-mapped cube
//! whose `base_color_texture` is a real 4×4 red/blue checkerboard
//! PNG embedded inline as a `data:image/png;base64,...` URI (the
//! `make_textured_uv_cube_glb` post-processes the placeholder URI
//! that `export_glb` emits).

mod common;

use rge_io_gltf::{import_glb_bytes, Cache, MemoryCache, PixelFormat};

#[test]
fn textured_uv_cube_fixture_imports_with_uvs_and_image_handle() {
    let path = common::textured_uv_cube_fixture_path();
    let bytes = std::fs::read(&path).expect("read textured_uv_cube.glb");
    let mut cache = MemoryCache::new();
    let scene = import_glb_bytes(&bytes, &mut cache).expect("import");

    // Exactly one entity with mesh + material.
    assert_eq!(scene.entities.len(), 1);
    let comps = &scene.entities[0];
    let mh = comps.mesh.expect("mesh handle");
    let mat_h = comps.material.expect("material handle");

    // Mesh carries UVs (from dispatch-M1's uv_cube geometry).
    let mesh = cache.get_mesh(&mh).expect("mesh in cache");
    assert_eq!(mesh.positions.len(), 24);
    assert_eq!(mesh.texcoords.len(), 24, "UV-cube has UVs");

    // Material carries base_color_image_handle pointing at the cached
    // 4×4 checkerboard.
    let mat = cache.get_material(&mat_h).expect("material in cache");
    let img_handle = mat
        .base_color_image_handle
        .expect("material has base_color_image_handle");
    let img = cache.get_image(&img_handle).expect("image in cache");
    assert_eq!(img.width(), 4, "4×4 checkerboard");
    assert_eq!(img.height(), 4);
    assert_eq!(img.pixel_format(), PixelFormat::Rgba8);
    assert_eq!(img.pixels().len(), 4 * 4 * 4);
}

#[test]
fn textured_uv_cube_checkerboard_corner_pixel_matches_expected() {
    // The 4×4 checkerboard's top-left pixel is red, second-pixel is
    // blue (per the make_checker_4x4_png in tests/common/mod.rs).
    let path = common::textured_uv_cube_fixture_path();
    let bytes = std::fs::read(&path).expect("read");
    let mut cache = MemoryCache::new();
    let scene = import_glb_bytes(&bytes, &mut cache).expect("import");
    let mat_h = scene.entities[0].material.expect("material");
    let mat = cache.get_material(&mat_h).expect("mat");
    let img_handle = mat.base_color_image_handle.expect("handle");
    let img = cache.get_image(&img_handle).expect("image");
    let pixels = img.pixels();
    // Tolerance ±1 for PNG encoder noise (same posture as
    // textured_cube_test::approx_eq_pixels).
    let approx = |a: u8, b: u8| (i32::from(a) - i32::from(b)).abs() <= 1;
    let p00 = &pixels[0..4]; // top-left, expected red
    let p10 = &pixels[4..8]; // second pixel, expected blue
    assert!(
        approx(p00[0], 255) && approx(p00[1], 0) && approx(p00[2], 0) && approx(p00[3], 255),
        "top-left should be red ~ [255, 0, 0, 255], got {p00:?}"
    );
    assert!(
        approx(p10[0], 0) && approx(p10[1], 0) && approx(p10[2], 255) && approx(p10[3], 255),
        "second pixel should be blue ~ [0, 0, 255, 255], got {p10:?}"
    );
}
