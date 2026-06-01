//! `rge-scene-loader` — bidirectional bridge between an `rge_data::Scene` and
//! an `rge_kernel_ecs::World`.
//!
//! Failure class: recoverable
//!
//! Narrow Scene↔World bridge: load per GitHub issue #171, save as the inverse.
//!
//! # Load (`Scene` → `World`)
//!
//! The caller parses an `.rge-scene` file into an [`rge_data::Scene`];
//! [`load_scene_into_world`] walks the scene and lands every entity + component
//! into a fresh [`rge_kernel_ecs::World`]. [`load_scene_world_from_path`] adds
//! the `.rge-project` / `.rge-scene` file-read + RON-parse front end.
//!
//! # Save (`World` → `Scene`)
//!
//! [`extract_scene_from_world`] is the inverse: it unions the supported-
//! component queries over a live [`World`] and rebuilds an [`rge_data::Scene`];
//! [`save_scene_world_to_path`] writes that scene to a `*.rge-scene` file as
//! pretty RON. v0 save fidelity is intentionally narrow (entity IDs + the four
//! supported components only) — see [`extract_scene_from_world`].
//!
//! # Identity preservation
//!
//! Every [`rge_data::EntityId`] (a ULID) is converted via
//! [`rge_kernel_ecs::EntityId::from_ulid`] and spawned through
//! [`rge_kernel_ecs::World::spawn_with_id`] before any component is inserted,
//! so the scene's stable identity round-trips through the load; save recovers
//! the ULID via [`rge_kernel_ecs::EntityId::ulid`].
//!
//! # Supported components
//!
//! The bridge is intentionally limited to the four simple-scene component
//! types named in issue #171, named once in the shared `TYPE_ID_*` constants
//! so the load (decode) and save (encode) directions cannot drift:
//!
//! - `rge::components::Transform` → [`rge_components_spatial::Transform`]
//! - `rge::components::Camera`    → [`rge_components_render::Camera`]
//! - `rge::components::Light`     → [`rge_components_render::Light`]
//! - `rge::components::Visibility` → [`rge_components_visibility::Visibility`]
//!
//! On load, any other `ComponentValue.type_id` is surfaced as
//! [`SceneLoadError::UnsupportedComponent`] — unknown components are never
//! silently dropped. On save, only these four component types are emitted; an
//! entity carrying none of them is not written (see
//! [`extract_scene_from_world`]).

use rge_components_render::{Camera, Light};
use rge_components_spatial::Transform;
use rge_components_visibility::Visibility;
use rge_data::{ComponentValue, Project, Scene};
use rge_kernel_ecs::{EntityId, World};

/// `type_id` string for [`rge_components_spatial::Transform`].
///
/// The four `TYPE_ID_*` constants name the supported component type paths once,
/// shared by [`load_scene_into_world`] (decode, via `insert_component`) and
/// [`extract_scene_from_world`] (encode), so the two directions cannot drift to
/// different spellings of the same canonical path.
pub(crate) const TYPE_ID_TRANSFORM: &str = "rge::components::Transform";
/// `type_id` string for [`rge_components_render::Camera`]. See [`TYPE_ID_TRANSFORM`].
pub(crate) const TYPE_ID_CAMERA: &str = "rge::components::Camera";
/// `type_id` string for [`rge_components_render::Light`]. See [`TYPE_ID_TRANSFORM`].
pub(crate) const TYPE_ID_LIGHT: &str = "rge::components::Light";
/// `type_id` string for [`rge_components_visibility::Visibility`]. See [`TYPE_ID_TRANSFORM`].
pub(crate) const TYPE_ID_VISIBILITY: &str = "rge::components::Visibility";

/// Errors that can occur while loading a [`Scene`] into a [`World`].
#[derive(Debug, thiserror::Error)]
pub enum SceneLoadError {
    /// A `ComponentValue` carried a `type_id` outside the supported set.
    #[error(
        "unsupported component type_id `{type_id}` on entity `{entity}` (loader supports only \
         Transform / Camera / Light / Visibility)"
    )]
    UnsupportedComponent {
        /// The unrecognized `type_id` string from the scene file.
        type_id: String,
        /// Canonical (26-char) ULID of the entity that carried the component.
        entity: String,
    },

    /// Typed RON deserialization of a `ComponentValue.data` payload failed.
    #[error("failed to deserialize component `{type_id}` on entity `{entity}` as RON: {source}")]
    Deserialize {
        /// The recognized component type_id the loader was decoding.
        type_id: String,
        /// Canonical (26-char) ULID of the entity that carried the component.
        entity: String,
        /// Underlying RON parse error.
        #[source]
        source: ron::de::SpannedError,
    },
}

/// Load `scene` into a fresh [`World`].
///
/// Spawns every scene entity with its original ULID, then walks each entity's
/// component envelope through a typed RON parse and inserts the resulting
/// component value through the typed [`World::insert`] API. Returns the
/// populated world, or a [`SceneLoadError`] on the first unsupported component
/// type_id or failed typed deserialization.
///
/// Scene relations and root-entity lists are **not** materialized — that
/// belongs to a future hierarchy / propagation pass and is out of scope for
/// this bridge.
///
/// # Errors
///
/// - [`SceneLoadError::UnsupportedComponent`] if any component carries a
///   `type_id` outside the four-string allowlist.
/// - [`SceneLoadError::Deserialize`] if a supported component's payload is
///   not valid RON for its target type.
pub fn load_scene_into_world(scene: &Scene) -> Result<World, SceneLoadError> {
    let mut world = World::new();

    // Spawn every entity first so later component insertions always target a
    // live entity, regardless of component-ordering quirks in the source file.
    for entity in &scene.entities {
        let ecs_id = EntityId::from_ulid(*entity.id.as_ulid());
        world.spawn_with_id(ecs_id);
    }

    for entity in &scene.entities {
        let ecs_id = EntityId::from_ulid(*entity.id.as_ulid());
        for component in &entity.components {
            insert_component(&mut world, ecs_id, &entity.id, component)?;
        }
    }

    Ok(world)
}

/// Errors from resolving a `.rge-project` / `.rge-scene` **path** into a
/// [`World`].
///
/// This is the path-level wrapper around [`load_scene_into_world`]: it adds the
/// file-read, RON-parse, and `.rge-project` scene-resolution failures that the
/// in-memory [`SceneLoadError`] boundary deliberately does not cover. The
/// underlying `Scene -> World` failure is preserved verbatim via
/// [`SceneWorldLoadError::Loader`] — this enum never broadens [`SceneLoadError`].
/// Messages are CLI-neutral; a binary caller (e.g. the editor's `--scene`
/// branch) supplies any flag framing.
#[derive(Debug, thiserror::Error)]
pub enum SceneWorldLoadError {
    /// File name was neither `.rge-project` nor `*.rge-scene`.
    #[error("{} has unsupported extension (expected .rge-project or .rge-scene)", .0.display())]
    UnsupportedExtension(std::path::PathBuf),
    /// Reading a `.rge-project` / `.rge-scene` file from disk failed.
    #[error("read {}: {source}", .path.display())]
    Read {
        /// The path that failed to read.
        path: std::path::PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// RON parse of the `.rge-project` manifest failed.
    #[error("parse .rge-project {}: {source}", .path.display())]
    ParseProject {
        /// The `.rge-project` path that failed to parse.
        path: std::path::PathBuf,
        /// Underlying RON parse error.
        #[source]
        source: ron::de::SpannedError,
    },
    /// RON parse of a `.rge-scene` file failed.
    #[error("parse .rge-scene {}: {source}", .path.display())]
    ParseScene {
        /// The `.rge-scene` path that failed to parse.
        path: std::path::PathBuf,
        /// Underlying RON parse error.
        #[source]
        source: ron::de::SpannedError,
    },
    /// The `.rge-project` `scenes` list was empty (no scene to load).
    #[error(".rge-project {} has no scenes (expected at least one entry in `scenes`)", .0.display())]
    EmptyProjectScenes(std::path::PathBuf),
    /// The `.rge-project` path has no parent directory to resolve relative
    /// scene paths against (e.g. a bare filename).
    #[error(
        ".rge-project {} has no parent directory to resolve relative scene paths against",
        .0.display()
    )]
    ProjectHasNoParentDir(std::path::PathBuf),
    /// [`load_scene_into_world`] returned an error.
    #[error("load scene into world: {0}")]
    Loader(#[source] SceneLoadError),
}

/// Load a `.rge-project` (resolving its first scene relative to the project
/// directory) or a `.rge-scene` (parsed directly) into a fresh [`World`] via
/// [`load_scene_into_world`].
///
/// Dispatch is on [`std::path::Path::file_name`], NOT
/// [`std::path::Path::extension`]: the canonical project file name
/// `.rge-project` is a leading-dot-only name that Rust treats as having no
/// extension. A literal `.rge-project` is parsed as an [`Project`] and its
/// first `scenes` entry is resolved relative to the manifest's parent
/// directory; a `*.rge-scene` name is parsed directly as a [`Scene`]; any
/// other name is rejected as [`SceneWorldLoadError::UnsupportedExtension`].
///
/// Pure I/O + RON + loader call — no GPU, no winit — so it can be exercised
/// headlessly (see `tests/scene_path_loader.rs`).
///
/// # Errors
///
/// Returns a [`SceneWorldLoadError`] on unsupported extension, file-read
/// failure, RON parse failure, an empty project `scenes` list, a missing
/// project parent directory, or a wrapped [`SceneLoadError`] from the
/// `Scene -> World` load itself.
pub fn load_scene_world_from_path(path: &std::path::Path) -> Result<World, SceneWorldLoadError> {
    let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    let kind = if file_name == ".rge-project" {
        "rge-project"
    } else if file_name.ends_with(".rge-scene") {
        "rge-scene"
    } else {
        ""
    };
    let scene: Scene = match kind {
        "rge-project" => {
            let raw = std::fs::read_to_string(path).map_err(|e| SceneWorldLoadError::Read {
                path: path.to_path_buf(),
                source: e,
            })?;
            let project: Project =
                ron::from_str(&raw).map_err(|e| SceneWorldLoadError::ParseProject {
                    path: path.to_path_buf(),
                    source: e,
                })?;
            let scene_rel = project
                .scenes
                .first()
                .ok_or_else(|| SceneWorldLoadError::EmptyProjectScenes(path.to_path_buf()))?;
            let project_dir = path
                .parent()
                .ok_or_else(|| SceneWorldLoadError::ProjectHasNoParentDir(path.to_path_buf()))?;
            let scene_path = project_dir.join(scene_rel.as_str());
            let scene_raw =
                std::fs::read_to_string(&scene_path).map_err(|e| SceneWorldLoadError::Read {
                    path: scene_path.clone(),
                    source: e,
                })?;
            ron::from_str(&scene_raw).map_err(|e| SceneWorldLoadError::ParseScene {
                path: scene_path,
                source: e,
            })?
        }
        "rge-scene" => {
            let raw = std::fs::read_to_string(path).map_err(|e| SceneWorldLoadError::Read {
                path: path.to_path_buf(),
                source: e,
            })?;
            ron::from_str(&raw).map_err(|e| SceneWorldLoadError::ParseScene {
                path: path.to_path_buf(),
                source: e,
            })?
        }
        _ => {
            return Err(SceneWorldLoadError::UnsupportedExtension(
                path.to_path_buf(),
            ))
        }
    };

    load_scene_into_world(&scene).map_err(SceneWorldLoadError::Loader)
}

/// Read the declared `name` from a `.rge-project` manifest at `path`, for
/// display use (window title / status bar) — e.g. a project whose folder is
/// `my-game` but whose manifest `name` is `"My Cool Game"` reads as the latter.
///
/// Returns `None` on any I/O or RON-parse failure, so a missing or malformed
/// manifest degrades gracefully (the caller falls back to the project folder
/// name) rather than surfacing an error. Read-only and world-free — it does NOT
/// load a world, so it is cheap enough for the one-shot call an Open / launch
/// makes (it is NOT meant for a per-frame path). An empty `name` is returned
/// verbatim; the display layer (editor-shell's `SaveSource::display_name`)
/// treats an empty name as absent.
#[must_use]
pub fn read_project_name(path: &std::path::Path) -> Option<String> {
    let raw = std::fs::read_to_string(path).ok()?;
    let project: Project = ron::from_str(&raw).ok()?;
    Some(project.name)
}

/// Decode one `ComponentValue` and insert the resulting typed component into
/// `world` against `ecs_id`. The `scene_id` is used only for error reporting.
fn insert_component(
    world: &mut World,
    ecs_id: EntityId,
    scene_id: &rge_data::EntityId,
    component: &ComponentValue,
) -> Result<(), SceneLoadError> {
    match component.type_id.as_str() {
        TYPE_ID_TRANSFORM => {
            let value = ron::from_str::<Transform>(&component.data).map_err(|source| {
                SceneLoadError::Deserialize {
                    type_id: component.type_id.clone(),
                    entity: scene_id.to_canonical(),
                    source,
                }
            })?;
            world.insert(ecs_id, value);
        }
        TYPE_ID_CAMERA => {
            let value = ron::from_str::<Camera>(&component.data).map_err(|source| {
                SceneLoadError::Deserialize {
                    type_id: component.type_id.clone(),
                    entity: scene_id.to_canonical(),
                    source,
                }
            })?;
            world.insert(ecs_id, value);
        }
        TYPE_ID_LIGHT => {
            let value = ron::from_str::<Light>(&component.data).map_err(|source| {
                SceneLoadError::Deserialize {
                    type_id: component.type_id.clone(),
                    entity: scene_id.to_canonical(),
                    source,
                }
            })?;
            world.insert(ecs_id, value);
        }
        TYPE_ID_VISIBILITY => {
            let value = ron::from_str::<Visibility>(&component.data).map_err(|source| {
                SceneLoadError::Deserialize {
                    type_id: component.type_id.clone(),
                    entity: scene_id.to_canonical(),
                    source,
                }
            })?;
            world.insert(ecs_id, value);
        }
        other => {
            return Err(SceneLoadError::UnsupportedComponent {
                type_id: other.to_owned(),
                entity: scene_id.to_canonical(),
            });
        }
    }
    Ok(())
}

// ───────────────────────────── Save (World → Scene) ─────────────────────────
// The encode inverse of the load path above. `save_scene_world_to_path` is the
// path-level wrapper a binary calls; `extract_scene_from_world` is the in-memory
// core. v0 fidelity is intentionally narrow — see `extract_scene_from_world`.

/// Errors that can occur while extracting a [`Scene`] from a [`World`].
///
/// The encode inverse of [`SceneLoadError`]. There is no "unsupported
/// component" variant: extraction queries by type and only ever emits the four
/// allowlisted components, so RON serialization of a payload is the only
/// failure mode.
#[derive(Debug, thiserror::Error)]
pub enum SceneSaveError {
    /// Typed RON serialization of a component payload failed.
    #[error("failed to serialize component `{type_id}` on entity `{entity}` as RON: {source}")]
    Serialize {
        /// The component type_id being encoded.
        type_id: String,
        /// Canonical (26-char) ULID of the entity that carried the component.
        entity: String,
        /// Underlying RON serialization error.
        #[source]
        source: ron::Error,
    },
}

/// Extract a [`Scene`] (named `name`) from a live [`World`] — the inverse of
/// [`load_scene_into_world`].
///
/// Unions the entity IDs returned by the four supported-component queries
/// (Transform, Camera, Light, Visibility), orders entities by ULID ascending
/// for deterministic, diffable output, and emits each entity's present
/// components in that same fixed order.
///
/// # v0 fidelity (intentionally narrow)
///
/// Only entity IDs and the four supported components round-trip:
///
/// - Enumeration is the union of the four component queries — the live
///   [`World`] exposes no all-entity iterator — so an entity with **none** of
///   the four supported components (whether truly component-less or carrying
///   only out-of-allowlist components) is **not** emitted.
/// - The loaded [`World`] retains no entity names, relations, or roots, so every
///   emitted entity has an empty `name`, an empty `relations` list, and the
///   returned [`Scene`] has empty `root_entities`.
/// - The scene `name` is the `name` argument; the scene `version` is stamped
///   `SchemaVersion::V0_1_0`.
///
/// # Errors
///
/// [`SceneSaveError::Serialize`] if RON serialization of any component payload
/// fails.
pub fn extract_scene_from_world(
    world: &World,
    name: impl Into<String>,
) -> Result<Scene, SceneSaveError> {
    // Keyed by raw ULID u128 so iteration is sorted ascending (deterministic,
    // diffable). Running the four queries in a fixed order also makes each
    // entity's component list deterministic.
    let mut by_entity: std::collections::BTreeMap<u128, Vec<ComponentValue>> =
        std::collections::BTreeMap::new();

    for (id, value) in world.query::<Transform>() {
        let entity = rge_data::EntityId::from_u128(id.ulid().0);
        let component = component_value(ron::to_string(value), TYPE_ID_TRANSFORM, entity)?;
        by_entity
            .entry(entity.to_u128())
            .or_default()
            .push(component);
    }
    for (id, value) in world.query::<Camera>() {
        let entity = rge_data::EntityId::from_u128(id.ulid().0);
        let component = component_value(ron::to_string(value), TYPE_ID_CAMERA, entity)?;
        by_entity
            .entry(entity.to_u128())
            .or_default()
            .push(component);
    }
    for (id, value) in world.query::<Light>() {
        let entity = rge_data::EntityId::from_u128(id.ulid().0);
        let component = component_value(ron::to_string(value), TYPE_ID_LIGHT, entity)?;
        by_entity
            .entry(entity.to_u128())
            .or_default()
            .push(component);
    }
    for (id, value) in world.query::<Visibility>() {
        let entity = rge_data::EntityId::from_u128(id.ulid().0);
        let component = component_value(ron::to_string(value), TYPE_ID_VISIBILITY, entity)?;
        by_entity
            .entry(entity.to_u128())
            .or_default()
            .push(component);
    }

    let entities = by_entity
        .into_iter()
        .map(|(raw, components)| rge_data::Entity {
            id: rge_data::EntityId::from_u128(raw),
            name: String::new(),
            components,
            relations: Vec::new(),
        })
        .collect();

    Ok(Scene {
        version: rge_data::SchemaVersion::V0_1_0,
        name: name.into(),
        entities,
        root_entities: Vec::new(),
    })
}

/// Wrap an already-serialized RON `data` result into a [`ComponentValue`]
/// envelope, mapping a failure to [`SceneSaveError::Serialize`].
///
/// The encode counterpart of the per-type arms in `insert_component`. Kept
/// non-generic on purpose: this crate has no `serde` dependency to name a
/// `Serialize` bound, so each caller serializes its concrete component type
/// (`ron::to_string`, no trait named) and hands the `Result` in.
fn component_value(
    data: Result<String, ron::Error>,
    type_id: &str,
    entity: rge_data::EntityId,
) -> Result<ComponentValue, SceneSaveError> {
    let data = data.map_err(|source| SceneSaveError::Serialize {
        type_id: type_id.to_owned(),
        entity: entity.to_canonical(),
        source,
    })?;
    Ok(ComponentValue {
        type_id: type_id.to_owned(),
        data,
    })
}

/// Errors from extracting a [`World`] and writing it to a `.rge-scene` **path**.
///
/// The path-level encode inverse of [`SceneWorldLoadError`].
#[derive(Debug, thiserror::Error)]
pub enum SceneWorldSaveError {
    /// File name was not `*.rge-scene` (use [`save_project_world_to_path`] for
    /// a `.rge-project`).
    #[error("{} has unsupported extension (expected .rge-scene)", .0.display())]
    UnsupportedExtension(std::path::PathBuf),
    /// [`extract_scene_from_world`] returned an error.
    #[error("extract scene from world: {0}")]
    Extract(#[source] SceneSaveError),
    /// Pretty-RON serialization of the whole [`Scene`] failed.
    #[error("serialize .rge-scene {}: {source}", .path.display())]
    Serialize {
        /// The target path being written.
        path: std::path::PathBuf,
        /// Underlying RON serialization error.
        #[source]
        source: ron::Error,
    },
    /// Writing the `.rge-scene` file to disk failed.
    #[error("write {}: {source}", .path.display())]
    Write {
        /// The path that failed to write.
        path: std::path::PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
}

/// Extract `world` into a [`Scene`] named `name` and write it as pretty RON to
/// `path`, which MUST be a `*.rge-scene` file. The save-side inverse of
/// [`load_scene_world_from_path`].
///
/// A `.rge-project` path (or any non-`*.rge-scene` name) is rejected as
/// [`SceneWorldSaveError::UnsupportedExtension`]: writing a project (emitting a
/// scene file *and* updating the manifest) is [`save_project_world_to_path`].
/// Dispatch is on [`std::path::Path::file_name`], matching
/// [`load_scene_world_from_path`].
///
/// Pure extract + RON + I/O — no GPU, no winit — so it is exercised headlessly
/// (see `tests/scene_save_round_trip.rs`). v0 save fidelity is the narrow
/// contract documented on [`extract_scene_from_world`].
///
/// # Errors
///
/// Returns a [`SceneWorldSaveError`] on unsupported extension, a wrapped
/// [`SceneSaveError`] from extraction, RON serialization failure, or a
/// file-write failure.
pub fn save_scene_world_to_path(
    world: &World,
    path: &std::path::Path,
    name: &str,
) -> Result<(), SceneWorldSaveError> {
    let file_name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    if !file_name.ends_with(".rge-scene") {
        return Err(SceneWorldSaveError::UnsupportedExtension(
            path.to_path_buf(),
        ));
    }

    let scene = extract_scene_from_world(world, name).map_err(SceneWorldSaveError::Extract)?;
    let text = ron::ser::to_string_pretty(&scene, ron::ser::PrettyConfig::default()).map_err(
        |source| SceneWorldSaveError::Serialize {
            path: path.to_path_buf(),
            source,
        },
    )?;
    std::fs::write(path, text).map_err(|source| SceneWorldSaveError::Write {
        path: path.to_path_buf(),
        source,
    })
}

/// Errors from writing a live [`World`] back to an existing `.rge-project`
/// (overwrite its first scene + re-write the manifest). The save-side inverse
/// of the `.rge-project` branch of [`load_scene_world_from_path`].
#[derive(Debug, thiserror::Error)]
pub enum ProjectWorldSaveError {
    /// File name was not exactly `.rge-project`.
    #[error("{} has unsupported extension (expected .rge-project)", .0.display())]
    UnsupportedExtension(std::path::PathBuf),
    /// Reading the existing `.rge-project` manifest from disk failed.
    #[error("read .rge-project {}: {source}", .path.display())]
    Read {
        /// The manifest path that failed to read.
        path: std::path::PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// RON parse of the `.rge-project` manifest failed.
    #[error("parse .rge-project {}: {source}", .path.display())]
    ParseProject {
        /// The manifest path that failed to parse.
        path: std::path::PathBuf,
        /// Underlying RON parse error.
        #[source]
        source: ron::de::SpannedError,
    },
    /// The manifest's `scenes` list is empty — no scene to write the world to.
    #[error("{} has no scenes to save the world into", .0.display())]
    EmptyProjectScenes(std::path::PathBuf),
    /// The `.rge-project` path has no parent directory to resolve scenes against.
    #[error("{} has no parent directory", .0.display())]
    ProjectHasNoParentDir(std::path::PathBuf),
    /// Writing the resolved `.rge-scene` failed (the scene-write half).
    #[error("write project scene: {0}")]
    Scene(#[source] SceneWorldSaveError),
    /// Pretty-RON serialization of the manifest failed.
    #[error("serialize .rge-project {}: {source}", .path.display())]
    SerializeManifest {
        /// The manifest path being written.
        path: std::path::PathBuf,
        /// Underlying RON serialization error.
        #[source]
        source: ron::Error,
    },
    /// Writing the `.rge-project` manifest to disk failed.
    #[error("write .rge-project {}: {source}", .path.display())]
    WriteManifest {
        /// The manifest path that failed to write.
        path: std::path::PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
}

/// Overwrite an existing `.rge-project`'s first scene with `world` and re-write
/// the manifest. The save-side inverse of the `.rge-project` branch of
/// [`load_scene_world_from_path`]: it mirrors that reader's resolution exactly
/// (exact `.rge-project` file name; `scenes.first()`; project-parent-relative
/// scene path) so a save → load round-trips. The scene write reuses
/// [`save_scene_world_to_path`]; the manifest is re-serialized (version
/// re-stamped [`rge_data::SchemaVersion::V0_1_0`]) and written back.
///
/// `project_path` MUST be an existing `.rge-project` — its manifest is read to
/// resolve the target scene. This is the **overwrite-open-project** case;
/// creating a NEW project tree (Save-As) is [`save_world_as_new_project`].
///
/// Pure read + extract + RON + I/O — no GPU, no winit — so it is exercised
/// headlessly (see `tests/project_save_round_trip.rs`).
///
/// # Errors
///
/// Returns a [`ProjectWorldSaveError`] on a non-`.rge-project` name, manifest
/// read / RON-parse failure, an empty `scenes` list, a missing parent
/// directory, a wrapped [`SceneWorldSaveError`] from the scene write, or a
/// manifest serialize / write failure.
pub fn save_project_world_to_path(
    world: &World,
    project_path: &std::path::Path,
) -> Result<(), ProjectWorldSaveError> {
    // Exact-name gate — mirror `load_scene_world_from_path`'s
    // `file_name == ".rge-project"`.
    let file_name = project_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    if file_name != ".rge-project" {
        return Err(ProjectWorldSaveError::UnsupportedExtension(
            project_path.to_path_buf(),
        ));
    }

    // Read + RON-parse the existing manifest.
    let raw =
        std::fs::read_to_string(project_path).map_err(|source| ProjectWorldSaveError::Read {
            path: project_path.to_path_buf(),
            source,
        })?;
    let mut project: Project =
        ron::from_str(&raw).map_err(|source| ProjectWorldSaveError::ParseProject {
            path: project_path.to_path_buf(),
            source,
        })?;

    // Resolve the target scene exactly as the reader does: first scene,
    // relative to the project's parent directory.
    let scene_rel = project
        .scenes
        .first()
        .ok_or_else(|| ProjectWorldSaveError::EmptyProjectScenes(project_path.to_path_buf()))?;
    let project_dir = project_path
        .parent()
        .ok_or_else(|| ProjectWorldSaveError::ProjectHasNoParentDir(project_path.to_path_buf()))?;
    let scene_path = project_dir.join(scene_rel.as_str());

    // v0 scene name = file stem, matching the `save_scene_world_to_path` caller
    // convention.
    let name = scene_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("scene");

    // Write the live world to the resolved scene (reuse the scene writer).
    save_scene_world_to_path(world, &scene_path, name).map_err(ProjectWorldSaveError::Scene)?;

    // Re-write the manifest: re-stamp the current schema version and
    // re-serialize (idempotent for an unchanged manifest; the hook for future
    // manifest changes). Matches the scene writer's pretty-config.
    project.version = rge_data::SchemaVersion::V0_1_0;
    let text = ron::ser::to_string_pretty(&project, ron::ser::PrettyConfig::default()).map_err(
        |source| ProjectWorldSaveError::SerializeManifest {
            path: project_path.to_path_buf(),
            source,
        },
    )?;
    std::fs::write(project_path, text).map_err(|source| ProjectWorldSaveError::WriteManifest {
        path: project_path.to_path_buf(),
        source,
    })
}

/// Errors from creating a brand-new `.rge-project` tree (manifest +
/// `scenes/main.rge-scene`) from a live [`World`] via
/// [`save_world_as_new_project`]. The create-side companion to
/// [`ProjectWorldSaveError`] (which overwrites an *existing* project).
#[derive(Debug, thiserror::Error)]
pub enum NewProjectWorldSaveError {
    /// `project_dir` has no usable (UTF-8) final component to derive a project
    /// name from.
    #[error("{} has no directory name to derive a project name from", .0.display())]
    NoProjectDirName(std::path::PathBuf),
    /// A `.rge-project` already exists in `project_dir` — refuse to clobber an
    /// existing project (Save-As targets a fresh tree).
    #[error("{} already exists (refusing to overwrite an existing project)", .0.display())]
    ProjectAlreadyExists(std::path::PathBuf),
    /// The target scene file already exists — refuse to overwrite a user file in
    /// a non-project folder.
    #[error("{} already exists (refusing to overwrite an existing file)", .0.display())]
    SceneAlreadyExists(std::path::PathBuf),
    /// Creating the project's `scenes/` subdirectory (or the project directory
    /// itself) failed.
    #[error("create project scenes dir {}: {source}", .path.display())]
    CreateSceneDir {
        /// The directory that failed to create.
        path: std::path::PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// Writing the first `.rge-scene` failed (the scene-write half).
    #[error("write project scene: {0}")]
    Scene(#[source] SceneWorldSaveError),
    /// Pretty-RON serialization of the new manifest failed.
    #[error("serialize .rge-project {}: {source}", .path.display())]
    SerializeManifest {
        /// The manifest path being written.
        path: std::path::PathBuf,
        /// Underlying RON serialization error.
        #[source]
        source: ron::Error,
    },
    /// Writing the new `.rge-project` manifest to disk failed.
    #[error("write .rge-project {}: {source}", .path.display())]
    WriteManifest {
        /// The manifest path that failed to write.
        path: std::path::PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
}

/// Create a brand-new `.rge-project` tree from a live [`World`] — the Save-As
/// (new project) writer, complementary to [`save_project_world_to_path`] (which
/// overwrites an *existing* project). Writes:
///
/// - `project_dir/.rge-project` — a manifest with a folder-derived `name`,
///   [`rge_data::SchemaVersion::V0_1_0`], `target_tiers: [Desktop]`, no plugins,
///   and a single scene `"scenes/main.rge-scene"`;
/// - `project_dir/scenes/main.rge-scene` — the extracted world (reuses
///   [`save_scene_world_to_path`]).
///
/// Returns the created `.rge-project` path, so a caller can adopt it as the new
/// save source. The layout round-trips through [`load_scene_world_from_path`] by
/// construction (first scene, project-parent-relative).
///
/// **No clobber:** refuses — before any write — if either
/// `project_dir/.rge-project` or `project_dir/scenes/main.rge-scene` already
/// exists, so an existing project (or a user file in a non-project folder) is
/// never overwritten.
///
/// Pure path + extract + RON + I/O — no GPU, no winit — so it is exercised
/// headlessly (see `tests/new_project_save_round_trip.rs`).
///
/// # Errors
///
/// Returns a [`NewProjectWorldSaveError`] when no name can be derived, when the
/// manifest or the scene file already exists, or on a directory-create,
/// scene-write, manifest-serialize, or manifest-write failure.
pub fn save_world_as_new_project(
    world: &World,
    project_dir: &std::path::Path,
) -> Result<std::path::PathBuf, NewProjectWorldSaveError> {
    // (1) Folder-derived project name (pure; fail fast before any I/O).
    let name = project_dir
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| NewProjectWorldSaveError::NoProjectDirName(project_dir.to_path_buf()))?
        .to_owned();

    // (2) Tree layout. The scene path is stored forward-slashed in the manifest
    //     (RON convention); the loader `join`s it portably on re-load.
    const SCENE_REL: &str = "scenes/main.rge-scene";
    let manifest_path = project_dir.join(".rge-project");
    let scenes_dir = project_dir.join("scenes");
    let scene_path = scenes_dir.join("main.rge-scene");

    // (3) No-clobber guards — before any write — so neither an existing project
    //     nor a stray user file in the target folder is ever overwritten.
    if manifest_path.exists() {
        return Err(NewProjectWorldSaveError::ProjectAlreadyExists(
            manifest_path,
        ));
    }
    if scene_path.exists() {
        return Err(NewProjectWorldSaveError::SceneAlreadyExists(scene_path));
    }

    // (4) Materialise project_dir/scenes/ (and project_dir if missing).
    std::fs::create_dir_all(&scenes_dir).map_err(|source| {
        NewProjectWorldSaveError::CreateSceneDir {
            path: scenes_dir.clone(),
            source,
        }
    })?;

    // (5) Write the first scene (reuse the scene writer; v0 scene name = "main").
    save_scene_world_to_path(world, &scene_path, "main")
        .map_err(NewProjectWorldSaveError::Scene)?;

    // (6) Build the default manifest.
    let project = Project {
        version: rge_data::SchemaVersion::V0_1_0,
        name,
        description: String::new(),
        target_tiers: vec![rge_data::TargetTier::Desktop],
        plugins: Vec::new(),
        scenes: vec![rge_data::ScenePath(SCENE_REL.to_owned())],
    };

    // (7) Serialize + write the manifest (match the other writers' pretty-config).
    let text = ron::ser::to_string_pretty(&project, ron::ser::PrettyConfig::default()).map_err(
        |source| NewProjectWorldSaveError::SerializeManifest {
            path: manifest_path.clone(),
            source,
        },
    )?;
    std::fs::write(&manifest_path, text).map_err(|source| {
        NewProjectWorldSaveError::WriteManifest {
            path: manifest_path.clone(),
            source,
        }
    })?;

    Ok(manifest_path)
}

#[cfg(test)]
mod tests {
    use rge_data::{Entity, SchemaVersion};

    use super::*;

    fn entity_with(type_id: &str, data: &str) -> Scene {
        let id = rge_data::EntityId::from_u128(0x1234);
        Scene {
            version: SchemaVersion::V0_1_0,
            name: "t".into(),
            entities: vec![Entity {
                id,
                name: "x".into(),
                components: vec![ComponentValue {
                    type_id: type_id.into(),
                    data: data.into(),
                }],
                relations: vec![],
            }],
            root_entities: vec![id],
        }
    }

    #[test]
    fn unsupported_component_errors() {
        let scene = entity_with("rge::components::Mystery", "()");
        let err = load_scene_into_world(&scene).expect_err("must reject unknown type_id");
        assert!(matches!(err, SceneLoadError::UnsupportedComponent { .. }));
    }

    #[test]
    fn malformed_payload_errors() {
        let scene = entity_with("rge::components::Visibility", "not-a-variant");
        let err = load_scene_into_world(&scene).expect_err("must reject bad RON");
        assert!(matches!(err, SceneLoadError::Deserialize { .. }));
    }

    #[test]
    fn empty_scene_yields_empty_world() {
        let scene = Scene::empty("blank", SchemaVersion::V0_1_0);
        let world = load_scene_into_world(&scene).expect("load");
        assert_eq!(world.entity_count(), 0);
    }
}
