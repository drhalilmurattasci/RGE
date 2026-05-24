//! Schema-load regression test for `golden-projects/simple-scene`.
//!
//! Reads the tracked golden `.rge-project` manifest from disk and parses it
//! against the current `rge_data::Project` schema, then resolves the single
//! scene reference relative to the project manifest directory and parses
//! the referenced `.rge-scene` as `rge_data::Scene`. Asserts only schema
//! facts on both sides (versions, names, empty plugins, non-empty scene roots,
//! the one expected scene path) — no scene-content, renderer,
//! GPU, asset-store, cook, screenshot, or typed component bridging
//! assertions live here.

use std::fs;
use std::path::{Path, PathBuf};

use rge_data::{Project, Scene, ScenePath, SchemaVersion, TargetTier};

fn simple_scene_manifest_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("golden-projects")
        .join("simple-scene")
        .join(".rge-project")
}

fn read_simple_scene_manifest() -> String {
    let path = simple_scene_manifest_path();
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn read_scene_referenced_by(project: &Project, manifest_path: &Path) -> (PathBuf, String) {
    let project_dir = manifest_path
        .parent()
        .expect("project manifest path has a parent directory");
    let scene_rel = project
        .scenes
        .first()
        .expect("project must reference at least one scene");
    let scene_path = project_dir.join(scene_rel.as_str());
    let text = fs::read_to_string(&scene_path)
        .unwrap_or_else(|e| panic!("read {}: {e}", scene_path.display()));
    (scene_path, text)
}

#[test]
fn simple_scene_manifest_parses_as_project() {
    let text = read_simple_scene_manifest();
    let project: Project = ron::from_str(&text).expect("parse simple-scene manifest as Project");

    assert_eq!(project.version, SchemaVersion::V0_1_0);
    assert_eq!(project.name, "simple-scene");
    assert!(
        !project.description.trim().is_empty(),
        "simple-scene description must be non-empty after trimming"
    );
    assert_eq!(project.target_tiers, vec![TargetTier::Desktop]);
    assert!(project.plugins.is_empty(), "plugins must be empty");
    assert_eq!(
        project.scenes,
        vec![ScenePath("scenes/main.rge-scene".to_string())],
        "scenes must be exactly one relative path to scenes/main.rge-scene"
    );
}

#[test]
fn simple_scene_manifest_with_required_field_removed_fails_to_parse() {
    let text = read_simple_scene_manifest();
    let required_line = "    name: \"simple-scene\",";
    assert!(
        text.contains(required_line),
        "expected required `name` field line in manifest text"
    );
    let mutated = text.replace(required_line, "");

    let result: Result<Project, _> = ron::from_str(&mutated);
    assert!(
        result.is_err(),
        "manifest with required `name` field removed must fail to parse as Project, got: {result:?}"
    );
}

#[test]
fn simple_scene_referenced_scene_parses_as_scene() {
    let manifest_path = simple_scene_manifest_path();
    let manifest_text = read_simple_scene_manifest();
    let project: Project =
        ron::from_str(&manifest_text).expect("parse simple-scene manifest as Project");

    let (_scene_path, scene_text) = read_scene_referenced_by(&project, &manifest_path);
    let scene: Scene = ron::from_str(&scene_text).expect("parse referenced scene as Scene");

    assert_eq!(scene.version, SchemaVersion::V0_1_0);
    assert_eq!(scene.name, "main");
    assert!(
        !scene.entities.is_empty(),
        "scene must contain at least one entity"
    );
    assert!(
        !scene.root_entities.is_empty(),
        "scene must contain at least one root entity"
    );

    for root in &scene.root_entities {
        assert!(
            scene.entities.iter().any(|entity| entity.id == *root),
            "root entity id {root} must exist in scene.entities"
        );
    }

    let root = scene
        .find_entity(scene.root_entities[0])
        .expect("first root entity must exist");
    assert_eq!(root.name, "Camera");
    assert!(root.components.is_empty(), "schema fixture stays untyped");
    assert!(root.relations.is_empty(), "root entity has no relations");
}

#[test]
fn simple_scene_referenced_scene_with_required_field_removed_fails_to_parse() {
    let manifest_path = simple_scene_manifest_path();
    let manifest_text = read_simple_scene_manifest();
    let project: Project =
        ron::from_str(&manifest_text).expect("parse simple-scene manifest as Project");

    let (_scene_path, scene_text) = read_scene_referenced_by(&project, &manifest_path);
    let required_line = "    name: \"main\",";
    assert!(
        scene_text.contains(required_line),
        "expected required `name` field line in scene text"
    );
    let mutated = scene_text.replace(required_line, "");

    let result: Result<Scene, _> = ron::from_str(&mutated);
    assert!(
        result.is_err(),
        "scene with required `name` field removed must fail to parse as Scene, got: {result:?}"
    );
}
