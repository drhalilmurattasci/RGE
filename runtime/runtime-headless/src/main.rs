//! `rge-runtime-headless` — load a project's first scene and advance one tick.
//!
//! GitHub issue #177: thin binary wiring `rge_data::{Project, Scene}` →
//! `rge_scene_loader::load_scene_into_world` → `World::advance_tick()`. The
//! CLI accepts exactly one positional `<project-path>` and prints a stable
//! success line carrying `entity_count` and `current_tick` evidence.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::{env, fs};

use rge_data::{Project, Scene};
use rge_scene_loader::load_scene_into_world;

const USAGE: &str = "usage: rge-runtime-headless <project-path>";

fn main() -> ExitCode {
    let mut args = env::args_os();
    let _exe = args.next();

    let first = match args.next() {
        Some(a) => a,
        None => {
            eprintln!("rge-runtime-headless: missing required <project-path> argument");
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };

    if args.next().is_some() {
        eprintln!("rge-runtime-headless: too many arguments; expected exactly one <project-path>");
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }

    if looks_like_flag(&first) {
        eprintln!(
            "rge-runtime-headless: unrecognized flag `{}`; the only accepted shape is one \
             positional <project-path>",
            first.to_string_lossy(),
        );
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }

    let project_path = PathBuf::from(first);
    match run(&project_path) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("rge-runtime-headless: {err}");
            ExitCode::FAILURE
        }
    }
}

fn looks_like_flag(arg: &OsString) -> bool {
    arg.to_string_lossy().starts_with('-')
}

fn run(project_path: &Path) -> Result<(), String> {
    let project_text = fs::read_to_string(project_path)
        .map_err(|e| format!("failed to read project `{}`: {e}", project_path.display()))?;
    let project: Project = ron::from_str(&project_text).map_err(|e| {
        format!(
            "failed to parse `{}` as Project RON: {e}",
            project_path.display()
        )
    })?;

    let first_scene = project.scenes.first().ok_or_else(|| {
        format!(
            "project `{}` has no scenes; expected at least one entry in Project.scenes",
            project_path.display()
        )
    })?;

    let project_dir = project_path.parent().ok_or_else(|| {
        format!(
            "project path `{}` has no parent directory to resolve scene paths against",
            project_path.display()
        )
    })?;
    let scene_path = project_dir.join(first_scene.as_str());

    let scene_text = fs::read_to_string(&scene_path)
        .map_err(|e| format!("failed to read scene `{}`: {e}", scene_path.display()))?;
    let scene: Scene = ron::from_str(&scene_text).map_err(|e| {
        format!(
            "failed to parse `{}` as Scene RON: {e}",
            scene_path.display()
        )
    })?;

    let mut world = load_scene_into_world(&scene)
        .map_err(|e| format!("failed to load scene into world: {e}"))?;

    world.advance_tick();

    println!(
        "rge-runtime-headless: loaded project={project_name} scene={scene_name} \
         entity_count={entity_count} current_tick={current_tick}",
        project_name = project.name,
        scene_name = scene.name,
        entity_count = world.entity_count(),
        current_tick = world.current_tick(),
    );

    Ok(())
}
