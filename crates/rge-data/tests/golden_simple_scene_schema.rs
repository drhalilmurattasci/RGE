//! Schema-load regression test for `golden-projects/simple-scene/.rge-project`.
//!
//! Reads the tracked golden manifest from disk and parses it against the
//! current `rge_data::Project` schema. Asserts only the load-time shape
//! (version, name, description, target tiers, empty plugins / scenes) — no
//! scene-content, renderer, GPU, asset-store, cook, screenshot, or typed
//! component bridging assertions live here.

use std::fs;
use std::path::PathBuf;

use rge_data::{Project, SchemaVersion, TargetTier};

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
    assert!(project.scenes.is_empty(), "scenes must be empty");
}

#[test]
fn simple_scene_manifest_with_required_field_removed_fails_to_parse() {
    let text = read_simple_scene_manifest();
    let required_line = "    name: \"simple-scene\",\n";
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
