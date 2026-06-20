# RGE Autonomous Dispatch — Task Brief

This file is the **authorized source of work** for the autonomous dispatch
loop (`Invoke-AiDispatchAuto.ps1`). When the queue is empty, Codex reads this
file, selects the next task, and files it as a GitHub issue that the hardened
dispatch queue then runs (plan → Claude gate → execute → verification gate →
control → publish).

> **The loop is INERT until the "Tasks" section below is armed.**
> While the `DISPATCH-TASKS-UNARMED` marker line is present, the driver
> selects nothing — a deterministic check, not a judgement call. Arming the
> loop is a deliberate act: delete that marker and fill in real tasks.

## How to fill this in

Pick **one** style:

### Style A — explicit task list (recommended, safest)
List concrete, **small, independently-shippable** tasks in priority order.
Codex takes the next un-filed one — or an earlier one if it is a dependency
("sequence necessity"). One file or one tight area per task, with a clear
done-criterion. Vague entries become vague commits.

### Style B — roadmap pointer
Instead of a list, write instructions telling Codex where to choose from,
e.g.: *"Pick the next unstarted job from the 'Next Jobs' section of
HANDOFF.md. Choose the smallest bounded one. Skip anything marked BLOCKED."*
Codex reads the repo (read-only) and decides. More autonomy, more drift risk —
prefer Style A until the loop has proven itself.

## Safety reminders

- The loop **halts** when a task is marked `ai-dispatch-failed` — that is,
  after a task fails its run *and* its one automatic retry. A human clears the
  label to resume.
- **Continuity & seatbelt** — the loop runs non-stop: the binding cap counts
  only *open* `ai-auto` issues (`-MaxAutonomousTasks` is an open-backlog
  ceiling, not a lifetime wall), and a periodic **seatbelt**
  (`-SeatbeltInterval`, default 50) pauses for human review every N new tasks by
  writing `.ai/dispatch.auto-halt` and filing a `needs-human` issue. Delete the
  sentinel to resume the next interval.
- In `branch` publish mode, finished work waits on an `ai-dispatch/ISSUE-*`
  branch for you to merge. In `main` mode it auto-publishes to `origin/main`.
- Keep tasks bounded. The autonomous loop will plan, execute, verify, and
  (depending on mode) publish whatever is selected here.
- **Salvage protocol** — when manually closing or salvaging an
  autonomous dispatch that did not auto-publish cleanly, you MUST
  remove the `ai-auto` label in addition to scrubbing
  `ai-dispatch-failed` / `ai-dispatch-retry`. Title renaming alone
  is not enough: `Invoke-AiDispatchAuto.ps1` builds Codex's
  "already filed" list via `--label ai-auto --state all`, and an
  `ai-auto`-labelled closed issue keeps the task semantically
  "consumed" in the selector's view. See
  `AI_DISPATCH_AUTOMATION.md` §14.8.
- **GPU test serialization** — any task that introduces a test crate
  (or new unit tests in an existing crate) which constructs real
  `wgpu::Instance` / `wgpu::Device` / `GfxContext` resources MUST
  include the per-binary `test_lock::guard()` pattern. Concurrent
  wgpu lifecycle inside a single test binary triggers Windows
  `STATUS_ACCESS_VIOLATION (0xc0000005)` in post-test teardown,
  which the canonical verify gate catches. See
  `AI_DISPATCH_AUTOMATION.md` §14.9 for the canonical pattern;
  reference implementations live in
  `editor/rge-editor/src/main.rs` and
  `crates/gfx/src/lib.rs::test_lock`.
- **DONE-SUPERSEDED semantics** — task entries prefixed
  `[DONE-SUPERSEDED ...]` are intentionally consumed or superseded by
  a later task or issue (their substantive work either landed under a
  different dispatch or was retired). `Invoke-AiDispatchAuto.ps1` MUST
  NOT select them as new dispatches; the original task text is
  preserved verbatim for provenance, not as a live work item.

## Self-re-arm protocol (keeps the loop non-stop)

Every dispatched task, as its **final step**, must leave exactly one un-filed
next task anchored in "## Tasks", **alternating kind**, and must edit
`.ai/dispatch.tasks.md` to do so (this file is in every task's `MAY edit` list
for that purpose):

- an **AUDIT** task appends the next **FEATURE** task (as before);
- a **FEATURE** task appends the next bounded **AUDIT** task — a
  "Post-<feature> Phase 9 next-task source audit" mirroring the most recent
  audit block (docs/source-read-only; its `MAY edit` includes
  `.ai/dispatch.tasks.md`, `Status.md`, `HANDOFF.md`, `plans/BASELINE.md`,
  `change.md`; it MUST NOT edit Rust source, tests, or automation);
- **copy this Self-re-arm requirement into the task you author** so the chain
  continues in both directions.

**Caution — do not mirror pre-protocol audit blocks' "no successor" rule.**
Audit blocks authored before this protocol (e.g. task 140) carry
"no task N+1 is appended" / "rg ^N+1 returns no matches" criteria. Mirror their
section structure and scope discipline, but do **not** copy those no-successor
criteria: under this protocol every audit task appends the next feature task.

If no bounded, in-policy next task exists, do **not** append one. Instead append
a single line to "## Tasks", verbatim in this form, and stop:

    NEEDS_HUMAN_RECORDED: <ISO-date> — <reason>

The autonomous driver detects that marker (or a dry brief), files a
`needs-human` review issue, and pauses by writing `.ai/dispatch.auto-halt`.
A human (or, per operator policy, Codex) resolves it, removes the marker /
appends the next task, and deletes the sentinel to resume.

## Tasks

Style A — explicit, ordered, one dispatch per entry. Codex selects the next
un-filed one (or an earlier blocker per "sequence necessity"). Each entry is
deliberately narrow — one workflow slice, one file area, one verifiable
done-criterion. Stale broad pointers ("Next-job options", "scene tree UI",
"undo/redo", "asset hot-reload") are intentionally excluded: they read as
sub-projects, not dispatches.

**Publish mode: `branch` until at least two automated selections land
cleanly.** Do NOT raise to `-PublishMode main` before that. Reviewer-on-merge
is the only safeguard against selector drift.

Historical task entries 1-166 were archived to .ai/dispatch.tasks.archive.md on 2026-06-20 after the selector prompt exceeded Codex's 1 MiB input limit. They are provenance only; the live queue continues from task 167 below.
167. **Audit the "Delete Current CAD Cuboid" menu-affordance boundary (source/docs-read-only, gated).**

   Audit the newly exposed menu-affordance CAD delete boundary without changing
   production code, tests, Cargo metadata, schemas, workflows, scripts, CAD crates,
   editor-actions, editor-state, or editor-egui-host projection/rendering logic.
   This is a GATED source/docs-read-only audit of task 166's implementation: the
   new `Command::DeleteCurrentCadCuboid` must route only to
   `delete_current_cad_cuboid`; menu enablement and the route guard must share the
   one exact-tracked-CAD rule; rejection must be warn-and-swallow with no fallback
   or stack growth; existing `Command::Delete`, the Delete-key accelerator, Cut,
   and wrapper-world delete must be unchanged; `editor-actions` must remain
   generic; the frozen `DeleteCurrentCadCuboid` action and
   `delete_current_cad_cuboid` entry point must be unchanged; and
   editor-egui-host rendering/projection logic must remain unchanged with only
   `menu_tests.rs` / `shortcut_help.rs` fixtures updated for the new entry.

   **Scope guard (operator decision - non-negotiable):**
   - Audit/read only:
     - `crates/editor-ui/src/menus/command.rs`
     - `crates/editor-ui/src/menus/predicate.rs`
     - `crates/editor-ui/src/menus/default_menu.rs`
     - `crates/editor-ui/tests/menus_ordering.rs`
     - `crates/editor-shell/src/render_path.rs`
     - `crates/editor-shell/src/lifecycle/mod.rs`
     - `crates/editor-shell/src/lifecycle/tests.rs`
     - `crates/editor-shell/src/lifecycle/commands.rs`
     - `crates/editor-egui-host/src/menu_tests.rs`
     - `crates/editor-egui-host/src/shortcut_help.rs`
     - `.ai/dispatch.tasks.md`
   - MAY write only the audit handoff/log artifacts for the current dispatch and,
     as the final gated-pause step, one `NEEDS_HUMAN_RECORDED: <ISO-date> - <reason>`
     marker plus a "Recommendation for human approval" block in
     `.ai/dispatch.tasks.md`.
   - MUST NOT edit source, tests, Cargo files, schemas, workflows, scripts, docs
     outside `.ai/dispatch.tasks.md`, packet templates, editor-actions, CAD crates,
     editor-state, `crates/editor-shell/src/lifecycle/commands.rs`, or any
     editor-egui-host production rendering/projection file.
   - MUST NOT append task 168 or any feature task.

   **Required audit checks:**
   - Confirm `Command::DeleteCurrentCadCuboid` has diagnostic id
     `delete_current_cad_cuboid`, no shortcut, and appears as "Delete Current CAD
     Cuboid" in the Edit menu and as "Edit: Delete Current CAD Cuboid" in the
     command palette through generic projection.
   - Confirm `PredicateContext::has_current_cad_cuboid_selection` is default false
     and `EditorShell::predicate_context()` populates it from
     `delete_menu_selection_is_exact_tracked_cad_entity()`.
   - Confirm `route_menu_command(Command::DeleteCurrentCadCuboid)` uses the same
     helper as its guard, calls only `delete_current_cad_cuboid()` on guard pass,
     and logs/swallows false guard or entry-point rejection without fallback to
     `delete_selected_entities`, selection clearing, face-selection pruning, or bus
     stack growth.
   - Confirm existing `Command::Delete`, the `"edit.delete"` Delete-key accelerator,
     `Command::Cut`, `cut_selected_entities`, and wrapper-world delete behavior are
     unchanged except for tests that assert the same behavior.
   - Confirm `crates/editor-shell/src/lifecycle/commands.rs`, editor-actions, CAD
     crates, Cargo metadata, and editor-egui-host rendering/projection logic are
     unchanged.

   **Verification:**
   - `git diff -- crates/editor-ui/src/menus/command.rs crates/editor-ui/src/menus/predicate.rs crates/editor-ui/src/menus/default_menu.rs crates/editor-ui/tests/menus_ordering.rs crates/editor-shell/src/render_path.rs crates/editor-shell/src/lifecycle/mod.rs crates/editor-shell/src/lifecycle/tests.rs crates/editor-egui-host/src/menu_tests.rs crates/editor-egui-host/src/shortcut_help.rs`
   - `git diff -- crates/editor-shell/src/lifecycle/commands.rs crates/editor-actions crates/cad-core crates/cad-graph crates/cad-projection crates/editor-state crates/editor-egui-host/src/menu.rs crates/editor-egui-host/src/lib.rs crates/editor-egui-host/src/handoff.rs crates/editor-egui-host/src/shortcut_conflicts.rs Cargo.toml Cargo.lock`
     EXPECTING no changes.
   - `rg -n "DeleteCurrentCadCuboid|delete_current_cad_cuboid|delete_menu_selection_is_exact_tracked_cad_entity|has_current_cad_cuboid_selection|edit.delete|Shortcut::plain\\(Key::Delete\\)|Command::Cut|cut_selected_entities|delete_selected_entities" crates/editor-ui/src/menus/command.rs crates/editor-ui/src/menus/predicate.rs crates/editor-ui/src/menus/default_menu.rs crates/editor-shell/src/render_path.rs crates/editor-shell/src/lifecycle/mod.rs crates/editor-shell/src/lifecycle/tests.rs crates/editor-egui-host/src/menu_tests.rs crates/editor-egui-host/src/shortcut_help.rs`
   - `rg -n "editor_shell|editor-shell|cad_core|cad-core|cad_projection|cad-projection|CadGraph|CadProjection|cad_world" crates/editor-actions/src crates/editor-actions/Cargo.toml`
     EXPECTING NO MATCHES.
   - `cargo test -p rge-editor-ui`
   - `cargo test -p rge-editor-egui-host`
   - `cargo test -p rge-editor-shell --lib -- route_menu_command`
   - `cargo test -p rge-editor-shell --lib -- predicate_context`
   - `cargo test -p rge-editor-shell --lib -- delete_current_cad_cuboid`
   - `cargo check -p rge-editor-ui -p rge-editor-shell -p rge-editor-egui-host`
   - `cargo +nightly fmt --all -- --check`
   - `git diff --check`
   - `rg -n "^166\\.|^167\\.|^168\\.|NEEDS_HUMAN_RECORDED" .ai/dispatch.tasks.md`
     EXPECTING exactly one task 166 and exactly one task 167; no task 168; no
     task-166 direct `NEEDS_HUMAN_RECORDED` marker.

   **Done criteria:**
   - Audit findings are source-grounded and no source/test changes are made by this
     task.
   - Exactly one `NEEDS_HUMAN_RECORDED: <ISO-date> - <reason>` marker is appended
     for the gated pause, followed by a "Recommendation for human approval" block
     with proposed next feature, exact edit surface, risks, verification, and why
     smallest.
   - No task 168 and no feature task is appended.

   **Carried self-rearm instruction from task 166 (verbatim):**

   **Self-re-arm (final step, required) - BRIEF-ENFORCED PAUSE (operator decision):**
   After implementation and verification, APPEND exactly one bounded
   source/docs-read-only AUDIT task as task 167 - an audit of the menu-affordance CAD
   delete boundary (the new `Command::DeleteCurrentCadCuboid` routes ONLY to
   `delete_current_cad_cuboid`; its menu enablement and the route guard share the ONE
   exact-tracked-CAD rule; rejection is warn-and-swallow with no fallback/stack
   growth; the existing `Command::Delete`, Delete-key accelerator, Cut, and
   wrapper-world delete are unchanged; `editor-actions` stays generic; the
   `DeleteCurrentCadCuboid` action + `delete_current_cad_cuboid` entry point are
   unchanged; editor-egui-host rendering/projection logic is unchanged with only its
   `menu_tests.rs` / `shortcut_help.rs` fixtures updated for the new entry). Task 167
   is a GATED audit: it MUST
   NOT append task 168 or any feature task, and its final step MUST record the next
   `NEEDS_HUMAN_RECORDED: <ISO-date> - <reason>` marker plus a "Recommendation for
   human approval" block (proposed next feature, exact edit surface, risks,
   verification, why smallest). Carry this gated-audit instruction verbatim into the
   task 167 block it authors. Task 166 itself MUST NOT directly record a
   `NEEDS_HUMAN_RECORDED` marker UNLESS it cannot safely append the task 167 audit -
   appending task 167 is the required primary outcome. Edit `.ai/dispatch.tasks.md`
   to do this.

NEEDS_HUMAN_RECORDED: 2026-06-20 - Source-grounded task-167 audit found the Delete Current CAD Cuboid menu boundary matches the task-166 intent, so human approval is required before filing any next feature task.

Recommendation for human approval

Proposed next feature: Decide whether the dedicated Delete Current CAD Cuboid command should stay menu/palette-only or receive a human-approved shortcut affordance.

Exact edit surface: If approved, keep the work to the canonical menu definition and fixture/assertion surface: `crates/editor-ui/src/menus/default_menu.rs`, `crates/editor-ui/tests/menus_ordering.rs`, `crates/editor-egui-host/src/menu_tests.rs`, and `crates/editor-egui-host/src/shortcut_help.rs`; add shell accelerator assertions only if the approved shortcut is executable through the existing generic menu route.

Risks: A shortcut could collide with the generic Delete key, weaken the exact-tracked-CAD guard, or imply editor-actions/CAD coupling if the implementation expands beyond the menu registry boundary.

Verification: Re-run the menu/UI/host/shell command tests for shortcut projection and routing, `cargo check -p rge-editor-ui -p rge-editor-shell -p rge-editor-egui-host`, `cargo +nightly fmt --all -- --check`, `git diff --check`, and the task-marker searches proving no task 168 was appended.

Why it is the smallest coherent next step: The audited command, predicate, and route already exist; the only unresolved product choice is the user-facing activation affordance, so the next human decision can stay focused on shortcut policy instead of re-opening CAD delete semantics.
