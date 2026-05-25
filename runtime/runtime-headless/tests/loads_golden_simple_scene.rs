//! Integration test for `rge-runtime-headless` per GitHub issue #177.
//!
//! Runs the compiled binary against the tracked
//! `golden-projects/simple-scene/.rge-project` fixture and asserts the
//! success stdout carries the expected `entity_count` and `current_tick`
//! evidence. Also covers the negative CLI contract: zero, multi-arg, and
//! flag-shaped invocations must fail without printing success evidence.

use std::path::PathBuf;
use std::process::Command;

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_rge-runtime-headless")
}

fn golden_project() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("golden-projects")
        .join("simple-scene")
        .join(".rge-project")
}

#[test]
fn loads_simple_scene_and_advances_one_tick() {
    let project = golden_project();
    assert!(
        project.exists(),
        "golden project fixture missing at {}",
        project.display()
    );

    let output = Command::new(binary())
        .arg(&project)
        .output()
        .expect("spawn rge-runtime-headless");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "binary exited with {:?}\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}",
        output.status,
    );
    assert!(
        stdout.contains("entity_count=2"),
        "stdout missing `entity_count=2`; got:\n{stdout}"
    );
    assert!(
        stdout.contains("current_tick=2"),
        "stdout missing `current_tick=2`; got:\n{stdout}"
    );
}

#[test]
fn rejects_zero_arguments() {
    let output = Command::new(binary())
        .output()
        .expect("spawn rge-runtime-headless");
    assert!(
        !output.status.success(),
        "zero-arg invocation should fail but exited cleanly"
    );
    assert_no_success_evidence(&output.stdout, "zero-arg");
}

#[test]
fn rejects_extra_positional_argument() {
    let output = Command::new(binary())
        .arg(golden_project())
        .arg("extra")
        .output()
        .expect("spawn rge-runtime-headless");
    assert!(
        !output.status.success(),
        "multi-arg invocation should fail but exited cleanly"
    );
    assert_no_success_evidence(&output.stdout, "multi-arg");
}

#[test]
fn rejects_flag_shaped_arguments() {
    for flag in ["--help", "-h", "--version"] {
        let output = Command::new(binary())
            .arg(flag)
            .output()
            .expect("spawn rge-runtime-headless");
        assert!(
            !output.status.success(),
            "flag-shaped invocation `{flag}` should fail but exited cleanly"
        );
        assert_no_success_evidence(&output.stdout, flag);
    }
}

fn assert_no_success_evidence(stdout_bytes: &[u8], label: &str) {
    let stdout = String::from_utf8_lossy(stdout_bytes);
    assert!(
        !stdout.contains("entity_count="),
        "[{label}] stdout leaked entity_count evidence:\n{stdout}"
    );
    assert!(
        !stdout.contains("current_tick="),
        "[{label}] stdout leaked current_tick evidence:\n{stdout}"
    );
}
