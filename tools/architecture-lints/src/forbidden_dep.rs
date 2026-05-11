//! Forbidden-dependency DAG lint. See PLAN.md §1.8.
//!
//! Enforces six dependency rules using the cargo metadata dep graph (direct
//! workspace-internal deps only; external registry deps are ignored):
//!
//! 1. Tier 1 (`kernel/*`) cannot depend on Tier 2 (`crates/*`).
//! 2. Tier 2 cannot depend on Tier 3 (no Tier-3 crates today; encoded defensively).
//! 3. `cad-core` stands alone — cannot depend on any other Tier-2 crate.
//! 4. `editor-ui` cannot depend on `physics`, `audio`, or `input` directly.
//! 5. `physics` cannot depend on `script-host`.
//! 6. Renderer crates (`gfx`, `gfx-ir`, `brep-render`) cannot depend on
//!    game-domain crates.
//!
//! # Workspace package naming
//!
//! Every workspace member's `name = "..."` field in its `Cargo.toml` carries
//! the `rge-` prefix (e.g. `rge-cad-core`, `rge-physics`, `rge-gfx`,
//! `rge-editor-ui`). `cargo_metadata::Package::name` returns this exact name,
//! so all literal package-name comparisons in this lint must include the
//! `rge-` prefix. Audit-6 (2026-05-09) caught that rules 3-6 were dead code
//! because they compared against the bare names (`"cad-core"`, `"physics"`,
//! etc.) which never match the prefixed reality.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::Result;
use cargo_metadata::Package;

use crate::common::{
    cargo_metadata, classify, relativize, workspace_members, LintReport, Tier, Violation,
};

// ---------------------------------------------------------------------------
// Constants — package name sets
// ---------------------------------------------------------------------------

/// Renderer crate package names that must not touch game-domain crates.
const RENDERER_CRATES: &[&str] = &["rge-gfx", "rge-gfx-ir", "rge-brep-render"];

/// Package names that are considered "game-domain" for rule 6.
///
/// Renderer crates may only depend on Tier-1 kernel crates plus
/// `math`, `errors`, `resources`, and `macros-reflect`.
const GAME_DOMAIN_PREFIXES: &[&str] = &[
    "rge-components-",
    "rge-cad-",
    "rge-anim-",
    "rge-script-",
    "rge-editor-",
    "rge-io-",
];

const GAME_DOMAIN_EXACT: &[&str] = &[
    "rge-physics",
    "rge-audio",
    "rge-input",
    "rge-asset-store",
    "rge-pak-format",
    // `rge-material-*` was originally a prefix; narrowed 2026-05-11 to
    // exact matches when `rge-gfx → rge-material-runtime` became a real
    // edge for the §6.3 material-intent adapter. `rge-material-runtime`
    // is pure-intent / GPU-agnostic / utility-tier (zero wgpu dep), so
    // the prefix sweep was over-broad — only the two graph-editor
    // crates below are genuinely game-domain.
    "rge-material-graph",
    "rge-material-graph-editor",
];

/// Returns `true` when `name` is a game-domain crate per rule 6.
#[must_use]
fn is_game_domain(name: &str) -> bool {
    if GAME_DOMAIN_EXACT.contains(&name) {
        return true;
    }
    GAME_DOMAIN_PREFIXES
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Collect the set of workspace package names from `meta` for fast lookup.
#[must_use]
fn workspace_pkg_names(pkgs: &[&Package]) -> HashSet<String> {
    pkgs.iter().map(|p| p.name.clone()).collect()
}

/// Return the direct workspace dependencies of `pkg` as a `Vec` of package
/// names (only deps whose resolved `id` appears in `workspace_ids`).
#[must_use]
fn direct_workspace_deps<'a>(pkg: &'a Package, workspace_names: &HashSet<String>) -> Vec<&'a str> {
    pkg.dependencies
        .iter()
        .filter(|d| workspace_names.contains(&d.name))
        .map(|d| d.name.as_str())
        .collect()
}

/// Apply rules 1–6 to a single (`pkg`, `dep`) pair and append any violations
/// to `report`.
///
/// Pure logic — extracted from `run` so it can be unit-tested without spinning
/// up a real workspace.  All package names are expected to be the
/// `rge-`-prefixed names that `cargo_metadata::Package::name` returns; rules
/// 3–6 compare against `rge-`-prefixed literals (see module doc).
fn check_dep_against_rules(
    pkg_name: &str,
    pkg_tier: Tier,
    dep_name: &str,
    dep_tier: Tier,
    manifest_rel: &Path,
    report: &mut LintReport,
) {
    let manifest_rel: PathBuf = manifest_rel.to_path_buf();

    // Rule 1: Tier 1 cannot depend on Tier 2.
    if pkg_tier == Tier::One && dep_tier == Tier::Two {
        report.push(Violation {
            file: manifest_rel.clone(),
            line: None,
            message: format!(
                "rule 1 (Tier 1 cannot depend on Tier 2) — `{pkg_name}` depends on `{dep_name}`"
            ),
        });
    }

    // Rule 2: Tier 2 cannot depend on Tier 3 (defensive; no Tier-3 today).
    if pkg_tier == Tier::Two && dep_tier == Tier::Three {
        report.push(Violation {
            file: manifest_rel.clone(),
            line: None,
            message: format!(
                "rule 2 (Tier 2 cannot depend on Tier 3) — `{pkg_name}` depends on `{dep_name}`"
            ),
        });
    }

    // Rule 3: `rge-cad-core` stands alone — no Tier-2 deps allowed.
    if pkg_name == "rge-cad-core" && dep_tier == Tier::Two {
        report.push(Violation {
            file: manifest_rel.clone(),
            line: None,
            message: format!("rule 3 (cad-core stands alone) — depends on `{dep_name}`"),
        });
    }

    // Rule 4: `rge-editor-ui` cannot depend on `rge-physics`, `rge-audio`, or
    // `rge-input`.
    if pkg_name == "rge-editor-ui" && matches!(dep_name, "rge-physics" | "rge-audio" | "rge-input")
    {
        report.push(Violation {
            file: manifest_rel.clone(),
            line: None,
            message: format!(
                "rule 4 (editor-ui cannot depend on physics/audio/input) — depends on `{dep_name}`"
            ),
        });
    }

    // Rule 5: `rge-physics` cannot depend on `rge-script-host`.
    if pkg_name == "rge-physics" && dep_name == "rge-script-host" {
        report.push(Violation {
            file: manifest_rel.clone(),
            line: None,
            message: "rule 5 (physics cannot depend on script-host) — depends on `rge-script-host`"
                .to_owned(),
        });
    }

    // Rule 6: Renderer crates cannot depend on game-domain crates.
    if RENDERER_CRATES.contains(&pkg_name) && is_game_domain(dep_name) {
        report.push(Violation {
            file: manifest_rel,
            line: None,
            message: format!(
                "rule 6 (renderer cannot depend on game-domain crates) — `{pkg_name}` depends on \
                 `{dep_name}`"
            ),
        });
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Run the forbidden-dependency DAG lint against the workspace at `workspace_root`.
///
/// Returns a [`LintReport`] whose violations list is empty on a clean workspace.
pub(crate) fn run(workspace_root: &Path) -> Result<LintReport> {
    let mut report = LintReport::new("forbidden-dep");

    let meta = cargo_metadata(workspace_root)?;
    let members = workspace_members(&meta);
    let ws_names = workspace_pkg_names(&members);

    for pkg in &members {
        let tier = classify(pkg, workspace_root);
        let manifest_rel = relativize(pkg.manifest_path.as_std_path(), workspace_root);
        let deps = direct_workspace_deps(pkg, &ws_names);

        for dep_name in &deps {
            let dep_pkg = members.iter().find(|p| p.name == *dep_name);
            let dep_tier = dep_pkg.map_or(Tier::Other, |d| classify(d, workspace_root));

            check_dep_against_rules(
                pkg.name.as_str(),
                tier,
                dep_name,
                dep_tier,
                &manifest_rel,
                &mut report,
            );
        }
    }

    Ok(report)
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------
//
// Two layers of test coverage:
//
// 1. Pure-logic tests over `check_dep_against_rules` — drive each of the six
//    rules with synthetic (pkg_name, tier, dep_name, dep_tier) tuples, no
//    cargo invocation needed. These are the fixtures that would have caught
//    audit-6's prefix-mismatch dead-code finding: pre-fix, rule 3-6 fixtures
//    asserting "FAIL" would have actually returned 0 violations and tripped.
//
// 2. End-to-end regression tests over `run` — synthesize a tiny cargo
//    workspace under a tempdir, invoke `run` against it, assert the violation
//    set. The "`current_workspace_passes_all_six_rules`" test runs against
//    the real workspace and is the canary that confirms rules 3-6 are now
//    actually firing (and that the real workspace remains clean).

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use tempfile::TempDir;

    use super::{check_dep_against_rules, is_game_domain, run, RENDERER_CRATES};
    use crate::common::{LintReport, Tier};

    // -----------------------------------------------------------------------
    // Layer 1 — pure logic fixtures (no cargo metadata)
    // -----------------------------------------------------------------------

    fn fresh_report() -> LintReport {
        LintReport::new("forbidden-dep")
    }

    /// Convenience: run `check_dep_against_rules` with a stub manifest path.
    fn run_check(pkg_name: &str, pkg_tier: Tier, dep_name: &str, dep_tier: Tier) -> Vec<String> {
        let mut report = fresh_report();
        let manifest = Path::new("crates/stub/Cargo.toml");
        check_dep_against_rules(
            pkg_name,
            pkg_tier,
            dep_name,
            dep_tier,
            manifest,
            &mut report,
        );
        report.violations.into_iter().map(|v| v.message).collect()
    }

    #[test]
    fn rule1_kernel_to_kernel_passes_clean() {
        // kernel-app -> kernel-ecs is Tier 1 -> Tier 1: no violation.
        let msgs = run_check("rge-kernel-app", Tier::One, "rge-kernel-ecs", Tier::One);
        assert!(msgs.is_empty(), "expected no violations, got: {msgs:?}");
    }

    #[test]
    fn rule1_kernel_to_crate_fails() {
        // Tier 1 -> Tier 2 must fail rule 1.
        let msgs = run_check("rge-kernel-app", Tier::One, "rge-cad-core", Tier::Two);
        assert!(
            msgs.iter().any(|m| m.contains("rule 1")),
            "expected rule 1 violation, got: {msgs:?}"
        );
    }

    #[test]
    fn rule2_crate_to_tier3_fails() {
        // Tier 2 -> Tier 3 (no Tier-3 crates today, but encoded defensively).
        let msgs = run_check("rge-cad-core", Tier::Two, "rge-some-tier3", Tier::Three);
        assert!(
            msgs.iter().any(|m| m.contains("rule 2")),
            "expected rule 2 violation, got: {msgs:?}"
        );
    }

    #[test]
    fn rule3_cad_core_stands_alone_fails_when_importing_other_tier2() {
        // rge-cad-core depending on any Tier-2 crate should trip rule 3.
        let msgs = run_check("rge-cad-core", Tier::Two, "rge-math", Tier::Two);
        assert!(
            msgs.iter().any(|m| m.contains("rule 3")),
            "expected rule 3 violation, got: {msgs:?}"
        );
    }

    #[test]
    fn rule3_cad_core_kernel_dep_passes() {
        // rge-cad-core depending on Tier-1 (rge-kernel-ecs) is allowed.
        let msgs = run_check("rge-cad-core", Tier::Two, "rge-kernel-ecs", Tier::One);
        assert!(
            !msgs.iter().any(|m| m.contains("rule 3")),
            "rule 3 should not fire on kernel deps, got: {msgs:?}"
        );
    }

    #[test]
    fn rule3_other_crate_to_tier2_does_not_trigger_rule3() {
        // Only rge-cad-core is bound by rule 3; other crates are not.
        let msgs = run_check("rge-physics", Tier::Two, "rge-math", Tier::Two);
        assert!(
            !msgs.iter().any(|m| m.contains("rule 3")),
            "rule 3 should not fire for non-cad-core packages, got: {msgs:?}"
        );
    }

    #[test]
    fn rule4_editor_ui_to_physics_fails() {
        let msgs = run_check("rge-editor-ui", Tier::Two, "rge-physics", Tier::Two);
        assert!(
            msgs.iter().any(|m| m.contains("rule 4")),
            "expected rule 4 violation, got: {msgs:?}"
        );
    }

    #[test]
    fn rule4_editor_ui_to_audio_fails() {
        let msgs = run_check("rge-editor-ui", Tier::Two, "rge-audio", Tier::Two);
        assert!(
            msgs.iter().any(|m| m.contains("rule 4")),
            "expected rule 4 violation, got: {msgs:?}"
        );
    }

    #[test]
    fn rule4_editor_ui_to_input_fails() {
        let msgs = run_check("rge-editor-ui", Tier::Two, "rge-input", Tier::Two);
        assert!(
            msgs.iter().any(|m| m.contains("rule 4")),
            "expected rule 4 violation, got: {msgs:?}"
        );
    }

    #[test]
    fn rule4_editor_ui_to_unrelated_tier2_passes() {
        // editor-ui is allowed to depend on, e.g., rge-ui-theme.
        let msgs = run_check("rge-editor-ui", Tier::Two, "rge-ui-theme", Tier::Two);
        assert!(
            !msgs.iter().any(|m| m.contains("rule 4")),
            "rule 4 should not fire for non-physics/audio/input deps, got: {msgs:?}"
        );
    }

    #[test]
    fn rule5_physics_to_script_host_fails() {
        let msgs = run_check("rge-physics", Tier::Two, "rge-script-host", Tier::Two);
        assert!(
            msgs.iter().any(|m| m.contains("rule 5")),
            "expected rule 5 violation, got: {msgs:?}"
        );
    }

    #[test]
    fn rule5_physics_to_kernel_passes() {
        // physics -> kernel is allowed (canary plugin pattern, ADR-114).
        let msgs = run_check(
            "rge-physics",
            Tier::Two,
            "rge-kernel-plugin-host",
            Tier::One,
        );
        assert!(
            !msgs.iter().any(|m| m.contains("rule 5")),
            "rule 5 should not fire on kernel deps, got: {msgs:?}"
        );
    }

    #[test]
    fn rule6_renderer_to_game_domain_fails() {
        // gfx -> physics (game-domain via GAME_DOMAIN_EXACT) trips rule 6.
        for renderer in RENDERER_CRATES {
            let msgs = run_check(renderer, Tier::Two, "rge-physics", Tier::Two);
            assert!(
                msgs.iter().any(|m| m.contains("rule 6")),
                "expected rule 6 violation for {renderer} -> rge-physics, got: {msgs:?}"
            );
        }
    }

    #[test]
    fn rule6_renderer_to_cad_prefix_fails() {
        // gfx -> rge-cad-core (game-domain via GAME_DOMAIN_PREFIXES "rge-cad-")
        // trips rule 6.
        let msgs = run_check("rge-gfx", Tier::Two, "rge-cad-core", Tier::Two);
        assert!(
            msgs.iter().any(|m| m.contains("rule 6")),
            "expected rule 6 violation, got: {msgs:?}"
        );
    }

    #[test]
    fn rule6_renderer_to_kernel_passes() {
        // gfx -> kernel-diagnostics is allowed (real workspace pattern).
        let msgs = run_check("rge-gfx", Tier::Two, "rge-kernel-diagnostics", Tier::One);
        assert!(
            !msgs.iter().any(|m| m.contains("rule 6")),
            "rule 6 should not fire on kernel deps, got: {msgs:?}"
        );
    }

    #[test]
    fn rule6_renderer_to_math_passes() {
        // gfx -> rge-math is on the renderer's allowlist (math, errors,
        // resources, macros-reflect) and does not match any game-domain
        // prefix or exact name.
        let msgs = run_check("rge-gfx", Tier::Two, "rge-math", Tier::Two);
        assert!(
            !msgs.iter().any(|m| m.contains("rule 6")),
            "rule 6 should not fire for renderer -> math, got: {msgs:?}"
        );
    }

    #[test]
    fn is_game_domain_classifies_correctly() {
        // exact matches
        assert!(is_game_domain("rge-physics"));
        assert!(is_game_domain("rge-audio"));
        assert!(is_game_domain("rge-input"));
        assert!(is_game_domain("rge-asset-store"));
        assert!(is_game_domain("rge-pak-format"));
        // prefix matches
        assert!(is_game_domain("rge-components-spatial"));
        assert!(is_game_domain("rge-cad-core"));
        assert!(is_game_domain("rge-anim-clip"));
        assert!(is_game_domain("rge-material-graph"));
        assert!(is_game_domain("rge-material-graph-editor"));
        assert!(is_game_domain("rge-script-host"));
        assert!(is_game_domain("rge-editor-ui"));
        assert!(is_game_domain("rge-io-gltf"));
        // negatives
        assert!(!is_game_domain("rge-math"));
        assert!(!is_game_domain("rge-errors"));
        assert!(!is_game_domain("rge-resources"));
        assert!(!is_game_domain("rge-macros-reflect"));
        assert!(!is_game_domain("rge-kernel-ecs"));
        // `rge-material-runtime` is pure-intent / GPU-agnostic / utility-tier
        // (zero wgpu dep) — NOT game-domain (the `rge-material-` prefix was
        // narrowed 2026-05-11 to exact matches `rge-material-graph` +
        // `rge-material-graph-editor`).
        assert!(!is_game_domain("rge-material-runtime"));
        // bare names (no rge- prefix) must NOT match — guards against a
        // regression to the pre-fix dead-code state.
        assert!(!is_game_domain("physics"));
        assert!(!is_game_domain("cad-core"));
        assert!(!is_game_domain("components-spatial"));
    }

    // -----------------------------------------------------------------------
    // Layer 2 — end-to-end against synthetic / real workspaces
    // -----------------------------------------------------------------------

    /// Spin up a synthetic cargo workspace under a tempdir, with the given
    /// (member-relative-path, member-cargo-toml-text) pairs. Each member's
    /// `src/lib.rs` is created as an empty file.
    fn make_workspace(members: &[(&str, &str)]) -> TempDir {
        let tmp = TempDir::new().expect("tempdir");
        let root = tmp.path();

        // Workspace Cargo.toml
        let mut ws_toml = String::from("[workspace]\nresolver = \"2\"\nmembers = [\n");
        for (path, _) in members {
            ws_toml.push_str(&format!("    \"{path}\",\n"));
        }
        ws_toml.push_str("]\n");
        fs::write(root.join("Cargo.toml"), ws_toml).expect("write workspace toml");

        // Each member
        for (path, toml) in members {
            let dir = root.join(path);
            fs::create_dir_all(dir.join("src")).expect("create member src");
            fs::write(dir.join("Cargo.toml"), toml).expect("write member toml");
            fs::write(dir.join("src/lib.rs"), "").expect("write member lib.rs");
        }

        tmp
    }

    #[test]
    fn synthetic_workspace_cad_core_alone_passes() {
        // Minimal good workspace: rge-cad-core depends only on rge-kernel-ecs.
        let tmp = make_workspace(&[
            (
                "kernel/ecs",
                r#"
[package]
name = "rge-kernel-ecs"
version = "0.1.0"
edition = "2021"
[lib]
path = "src/lib.rs"
"#,
            ),
            (
                "crates/cad-core",
                r#"
[package]
name = "rge-cad-core"
version = "0.1.0"
edition = "2021"
[lib]
path = "src/lib.rs"
[dependencies]
rge-kernel-ecs = { path = "../../kernel/ecs" }
"#,
            ),
        ]);
        let report = run(tmp.path()).expect("run lint");
        assert!(
            report.ok(),
            "expected clean workspace, got: {:?}",
            report
                .violations
                .iter()
                .map(|v| &v.message)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn synthetic_workspace_cad_core_to_other_tier2_fails_rule3() {
        // rge-cad-core -> rge-math should fire rule 3.
        let tmp = make_workspace(&[
            (
                "crates/math",
                r#"
[package]
name = "rge-math"
version = "0.1.0"
edition = "2021"
[lib]
path = "src/lib.rs"
"#,
            ),
            (
                "crates/cad-core",
                r#"
[package]
name = "rge-cad-core"
version = "0.1.0"
edition = "2021"
[lib]
path = "src/lib.rs"
[dependencies]
rge-math = { path = "../math" }
"#,
            ),
        ]);
        let report = run(tmp.path()).expect("run lint");
        assert!(
            report
                .violations
                .iter()
                .any(|v| v.message.contains("rule 3")),
            "expected rule 3 violation, got: {:?}",
            report
                .violations
                .iter()
                .map(|v| &v.message)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn synthetic_workspace_renderer_to_physics_fails_rule6() {
        // rge-gfx -> rge-physics should fire rule 6.
        let tmp = make_workspace(&[
            (
                "crates/physics",
                r#"
[package]
name = "rge-physics"
version = "0.1.0"
edition = "2021"
[lib]
path = "src/lib.rs"
"#,
            ),
            (
                "crates/gfx",
                r#"
[package]
name = "rge-gfx"
version = "0.1.0"
edition = "2021"
[lib]
path = "src/lib.rs"
[dependencies]
rge-physics = { path = "../physics" }
"#,
            ),
        ]);
        let report = run(tmp.path()).expect("run lint");
        assert!(
            report
                .violations
                .iter()
                .any(|v| v.message.contains("rule 6")),
            "expected rule 6 violation, got: {:?}",
            report
                .violations
                .iter()
                .map(|v| &v.message)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn synthetic_workspace_kernel_to_crate_fails_rule1() {
        // rge-kernel-app -> rge-cad-core (Tier 1 -> Tier 2) trips rule 1.
        let tmp = make_workspace(&[
            (
                "crates/cad-core",
                r#"
[package]
name = "rge-cad-core"
version = "0.1.0"
edition = "2021"
[lib]
path = "src/lib.rs"
"#,
            ),
            (
                "kernel/app",
                r#"
[package]
name = "rge-kernel-app"
version = "0.1.0"
edition = "2021"
[lib]
path = "src/lib.rs"
[dependencies]
rge-cad-core = { path = "../../crates/cad-core" }
"#,
            ),
        ]);
        let report = run(tmp.path()).expect("run lint");
        assert!(
            report
                .violations
                .iter()
                .any(|v| v.message.contains("rule 1")),
            "expected rule 1 violation, got: {:?}",
            report
                .violations
                .iter()
                .map(|v| &v.message)
                .collect::<Vec<_>>()
        );
    }

    /// Regression test against the live workspace.
    ///
    /// Pre-audit-6, rules 3-6 were dead code (compared against unprefixed
    /// names). The lint reported "PASS 0 violations" but that status was
    /// meaningless. Post-fix, this test asserts the real workspace still
    /// passes — which now genuinely means rules 3-6 ran and found no
    /// violations, rather than silently no-op'd.
    ///
    /// If this test ever fails, an actual rule 3-6 violation has been
    /// introduced into the real workspace and must be fixed.
    #[test]
    fn current_workspace_passes_all_six_rules() {
        // Walk up from the test binary location to the workspace root.
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        // CARGO_MANIFEST_DIR for this crate is `<root>/tools/architecture-lints`;
        // pop two segments to reach the workspace root.
        let workspace_root = manifest_dir
            .parent()
            .and_then(Path::parent)
            .expect("workspace root");
        assert!(
            workspace_root.join("Cargo.toml").is_file(),
            "expected workspace root Cargo.toml at {}",
            workspace_root.display()
        );

        let report = run(workspace_root).expect("run lint against real workspace");
        assert!(
            report.ok(),
            "real workspace must pass forbidden-dep; violations: {:?}",
            report
                .violations
                .iter()
                .map(|v| &v.message)
                .collect::<Vec<_>>()
        );
    }
}
