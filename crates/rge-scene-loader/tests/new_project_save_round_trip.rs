//! Create-side tests for `save_world_as_new_project` (NEWPROJECT-SAVE-SUBSTRATE
//! dispatch) — the Save-As-to-a-NEW-`.rge-project`-tree writer.
//!
//! Mirrors `project_save_round_trip.rs`: worlds are seeded from the tracked
//! golden simple-scene (Camera + KeyLight, all four supported components) so the
//! tests never hand-author component RON, and the round-trip anchor is a create
//! followed by `load_scene_world_from_path` against the SAME `.rge-project`. The
//! writer creates a fresh tree (`<dir>/.rge-project` + `<dir>/scenes/main.rge-scene`)
//! whose layout the loader resolves by construction (first scene,
//! project-parent-relative).

use std::collections::BTreeMap;
use std::path::PathBuf;

use rge_components_render::{Camera, Light};
use rge_components_spatial::Transform;
use rge_components_visibility::Visibility;
use rge_kernel_ecs::World;
use rge_scene_loader::{
    load_scene_world_from_path, save_world_as_new_project, NewProjectWorldSaveError,
};

type ComponentSets = (
    BTreeMap<u128, Transform>,
    BTreeMap<u128, Camera>,
    BTreeMap<u128, Light>,
    BTreeMap<u128, Visibility>,
);

/// Path to the tracked golden simple-scene `.rge-project` (two levels under the
/// repo root), matching `project_save_round_trip.rs`.
fn golden_project_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("golden-projects")
        .join("simple-scene")
        .join(".rge-project")
}

/// Load the golden simple-scene into a `World` via the public path resolver.
fn golden_world() -> World {
    load_scene_world_from_path(&golden_project_path()).expect("load golden simple-scene")
}

/// Collect the supported-component value sets of `world`, keyed by raw ULID.
fn component_sets(world: &World) -> ComponentSets {
    (
        world
            .query::<Transform>()
            .map(|(id, v)| (id.ulid().0, *v))
            .collect(),
        world
            .query::<Camera>()
            .map(|(id, v)| (id.ulid().0, *v))
            .collect(),
        world
            .query::<Light>()
            .map(|(id, v)| (id.ulid().0, *v))
            .collect(),
        world
            .query::<Visibility>()
            .map(|(id, v)| (id.ulid().0, *v))
            .collect(),
    )
}

/// A unique, **empty** temp directory to host a throwaway new project. The
/// process id + a caller label keep concurrent test threads / `cargo test`
/// processes from colliding (the crate has no `tempfile` dev-dependency); the
/// pre-clean guarantees a clean slate even if a prior run crashed mid-test.
fn fresh_temp_project_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "rge_newproject_save_{}_{label}",
        std::process::id()
    ));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).expect("create temp project dir");
    dir
}

#[test]
fn new_project_manifest_has_expected_defaults() {
    let dir = fresh_temp_project_dir("manifest");
    let expected_name = dir
        .file_name()
        .and_then(|s| s.to_str())
        .expect("temp dir has a UTF-8 name")
        .to_string();

    let manifest_path =
        save_world_as_new_project(&golden_world(), &dir).expect("create new project");
    assert_eq!(
        manifest_path,
        dir.join(".rge-project"),
        "returns the created .rge-project path"
    );

    let raw = std::fs::read_to_string(&manifest_path).expect("read manifest back");
    let project: rge_data::Project = ron::from_str(&raw).expect("parse manifest");
    assert_eq!(project.name, expected_name, "name is folder-derived");
    assert_eq!(project.version, rge_data::SchemaVersion::V0_1_0);
    assert_eq!(
        project.target_tiers,
        vec![rge_data::TargetTier::Desktop],
        "default target tier is Desktop"
    );
    assert!(project.plugins.is_empty(), "no plugins by default");
    assert_eq!(project.description, "", "empty description by default");
    assert_eq!(
        project.scenes,
        vec![rge_data::ScenePath("scenes/main.rge-scene".to_string())],
        "single nested scene path"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn new_project_round_trips_through_load() {
    let world_a = golden_world();
    let dir = fresh_temp_project_dir("round_trip");

    let manifest_path = save_world_as_new_project(&world_a, &dir).expect("create new project");
    assert_eq!(manifest_path, dir.join(".rge-project"));
    assert!(
        dir.join("scenes").join("main.rge-scene").exists(),
        "the world is written under scenes/main.rge-scene"
    );

    // Round-trip: load the created project back (its first scene) → world.
    let world_b = load_scene_world_from_path(&manifest_path).expect("reload created project");
    assert_eq!(
        component_sets(&world_a),
        component_sets(&world_b),
        "create -> load must preserve entity ids and the four supported component values"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn existing_manifest_is_refused_without_clobber() {
    let dir = fresh_temp_project_dir("existing_manifest");
    let manifest_path = dir.join(".rge-project");
    std::fs::write(&manifest_path, b"SENTINEL-MANIFEST").expect("pre-create manifest");

    let err = save_world_as_new_project(&World::new(), &dir)
        .expect_err("must refuse to overwrite an existing project");
    assert!(
        matches!(err, NewProjectWorldSaveError::ProjectAlreadyExists(_)),
        "an existing .rge-project is refused (got {err:?})"
    );
    assert!(
        !dir.join("scenes").join("main.rge-scene").exists(),
        "no scene file is created when the manifest already exists"
    );
    assert_eq!(
        std::fs::read(&manifest_path).expect("read manifest"),
        b"SENTINEL-MANIFEST",
        "the existing manifest is left byte-unchanged"
    );

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn existing_scene_file_is_refused_without_clobber() {
    let dir = fresh_temp_project_dir("existing_scene");
    let scenes_dir = dir.join("scenes");
    std::fs::create_dir_all(&scenes_dir).expect("pre-create scenes dir");
    let scene_path = scenes_dir.join("main.rge-scene");
    std::fs::write(&scene_path, b"USER-FILE").expect("pre-create a user file at the scene path");

    let err = save_world_as_new_project(&golden_world(), &dir)
        .expect_err("must refuse to overwrite an existing scene file");
    assert!(
        matches!(err, NewProjectWorldSaveError::SceneAlreadyExists(_)),
        "a pre-existing scenes/main.rge-scene is refused (got {err:?})"
    );
    assert!(
        !dir.join(".rge-project").exists(),
        "no manifest is written when the scene file already exists"
    );
    assert_eq!(
        std::fs::read(&scene_path).expect("read scene file"),
        b"USER-FILE",
        "the pre-existing user file is left byte-unchanged"
    );

    std::fs::remove_dir_all(&dir).ok();
}
