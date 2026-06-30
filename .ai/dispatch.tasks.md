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
167. **[DONE-SUPERSEDED 2026-06-21 via RustCADs/RGE #428 (commit cd2b2e2); pre-migration] Audit the "Delete Current CAD Cuboid" menu-affordance boundary (source/docs-read-only, gated).**

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

168. **[DONE-SUPERSEDED 2026-06-21 via RustCADs/RGE #430 / PR #431 (commit c82d02f); pre-migration] Add a Ctrl+Shift+Delete keyboard accelerator for the existing "Delete Current
   CAD Cuboid" command (editor-ui menu definition + fixtures only; dispatched through
   the existing generic menu route).**

   Bind the already-existing `Command::DeleteCurrentCadCuboid` to a new, non-colliding
   keyboard accelerator **Ctrl+Shift+Delete**, surfaced through the same generic menu
   route, projection, and shortcut-help machinery every other accelerator uses. This is
   the human-approved activation affordance for the command implemented in task 166 and
   audited in task 167. It changes ONLY the menu-entry shortcut binding plus the
   tests/fixtures that currently assert this entry has no shortcut and that the resolved
   table has eighteen accelerators. It adds NO new command, NO new dispatch wiring, and
   NO shell/CAD/editor-actions change.

   **Source-grounded facts (2026-06-20):**
   - `Command::DeleteCurrentCadCuboid`, `PredicateContext::has_current_cad_cuboid_selection`,
     the `route_menu_command` arm with the exact-tracked-CAD guard
     (`delete_menu_selection_is_exact_tracked_cad_entity`), and the
     `delete_current_cad_cuboid` action ALL already exist and need NO change.
   - A menu shortcut dispatches through the EXISTING generic path (`keystroke ->
     keycode_to_shortcut -> enabled_command_for_shortcut -> route_menu_command`);
     `keycode_to_shortcut(KeyCode::Delete, ctrl=true, shift=true)` already yields
     `Shortcut::new(Modifiers::CTRL | Modifiers::SHIFT, Key::Delete)`. No editor-shell
     accelerator wiring or keycode-table change is required.
   - `shortcut_help.rs` and `shortcut_conflicts.rs` are data-driven projections; the new
     accelerator surfaces automatically. Only their test expectations change, not logic.
   - Ctrl+Shift+Delete is free: it collides with none of the 18 existing accelerators
     (incl. the bare-Delete accelerator on `Command::Delete`), and mirrors the app's
     existing `Ctrl+Shift+S` (Save-As) / `Ctrl+Shift+P` (Command Palette) convention.

   **Scope guard (operator decision - non-negotiable):**
   - MAY edit (implementation + the tests that assert the old "no shortcut"/"18" state):
     - `crates/editor-ui/src/menus/default_menu.rs` — add
       `.with_shortcut(Shortcut::new(Modifiers::CTRL | Modifiers::SHIFT, Key::Delete))` to the
       `edit.delete_current_cad_cuboid` MenuEntry (constructed like the existing Ctrl+Shift+
       accelerators). Update EVERY assertion that hard-codes the accelerator count `18` to
       `19` (there are three: `executable_accelerators_have_no_conflicts_and_bind_exactly_eighteen`
       and the two Play-entry tests) plus their enumerated-list messages, and update
       `edit_delete_current_cad_cuboid_entry_has_no_shortcut` to assert the entry now carries
       the Ctrl+Shift+Delete shortcut with no conflict.
     - `crates/editor-ui/tests/menus_ordering.rs` — update
       `default_edit_menu_contains_no_shortcut_current_cad_cuboid_delete` (and any
       accelerator-count/ordering assertion) to expect the Ctrl+Shift+Delete accelerator.
     - `crates/editor-egui-host/src/menu_tests.rs` — change the Delete-Current-CAD-Cuboid
       shortcut element from `None` to `Some("Ctrl+Shift+Delete")` in BOTH the menu fixture
       and the `file_and_edit_items_carry_accelerators...` accelerator-vector test (and its
       message).
     - `crates/editor-egui-host/src/shortcut_help.rs` — update ONLY the test expectation that
       currently records this command's shortcut as empty
       (`shortcut_help_rows_include_passive_hints_and_empty_shortcuts`); do NOT change the
       projection logic.
     - `.ai/dispatch.tasks.md` — the self-re-arm append (task 169) only.
   - MUST NOT edit: `crates/editor-ui/src/menus/command.rs`,
     `crates/editor-ui/src/menus/predicate.rs`; any `editor-shell` source (`render_path.rs`,
     `lifecycle/mod.rs`, `lifecycle/commands.rs`, `lifecycle/accelerator.rs`);
     `editor-actions`; CAD crates (`cad-core`, `cad-graph`, `cad-projection`); `editor-state`;
     egui-host production rendering/projection (`menu.rs`, `lib.rs`, `handoff.rs`) and the
     `shortcut_help.rs`/`shortcut_conflicts.rs` projection logic; `Cargo.toml`/`Cargo.lock`;
     schemas; workflows; scripts; packet templates; or any docs outside `.ai/dispatch.tasks.md`.
   - The accelerator MUST be Ctrl+Shift+Delete and MUST NOT collide with any existing
     accelerator; `AcceleratorTable::detect_conflicts()` MUST report zero conflicts and the
     resolved accelerator table MUST contain exactly nineteen entries.

   **Required steps:**
   - Add the Ctrl+Shift+Delete accelerator to the existing `DeleteCurrentCadCuboid` menu entry
     only (no new entry, no new `Command` variant). The accelerator MUST route the SAME
     `Command::DeleteCurrentCadCuboid` through the SAME generic route and exact-tracked-CAD
     guard; rejection stays warn-and-swallow (no fallback to `delete_selected_entities`, no
     selection/face-selection mutation, no bus-stack growth).
   - Enablement is unchanged: the accelerator fires only when the menu item is enabled
     (`is_editing && has_current_cad_cuboid_selection`).
   - Update the assertions/fixtures listed above so the suite reflects exactly one added
     accelerator (18 -> 19) with no conflicts; leave the Delete-key accelerator, Cut,
     wrapper-world delete, and all other accelerators unchanged.

   **Verification:**
   - `cargo test -p rge-editor-ui`
   - `cargo test -p rge-editor-egui-host`
   - `cargo test -p rge-editor-shell --lib -- route_menu_command`
   - `cargo test -p rge-editor-shell --lib -- delete_current_cad_cuboid`
   - `cargo check -p rge-editor-ui -p rge-editor-shell -p rge-editor-egui-host`
   - `git diff -- crates/editor-shell crates/editor-actions crates/cad-core crates/cad-graph crates/cad-projection crates/editor-state crates/editor-ui/src/menus/command.rs crates/editor-ui/src/menus/predicate.rs crates/editor-egui-host/src/menu.rs crates/editor-egui-host/src/lib.rs crates/editor-egui-host/src/handoff.rs Cargo.toml Cargo.lock`
     EXPECTING no changes.
   - `rg -n "with_shortcut|Modifiers::CTRL|Key::Delete" crates/editor-ui/src/menus/default_menu.rs`
     showing the new Ctrl+Shift+Delete binding on the delete_current_cad_cuboid entry.
   - `cargo +nightly fmt --all -- --check`
   - `git diff --check`
   - `rg -n "^167\.|^168\.|^169\.|NEEDS_HUMAN_RECORDED" .ai/dispatch.tasks.md`
     EXPECTING exactly one task 168 and exactly one task 169; no leftover
     `NEEDS_HUMAN_RECORDED` marker.

   **Done criteria:**
   - `Command::DeleteCurrentCadCuboid` carries a single Ctrl+Shift+Delete accelerator that
     dispatches through the existing generic route + guard; the resolved accelerator table has
     nineteen entries with zero conflicts.
   - No change to `command.rs`/`predicate.rs`, editor-shell, editor-actions, CAD crates,
     editor-state, or egui-host rendering/projection logic.
   - Exactly one bounded AUDIT task 169 is appended per the self-re-arm protocol; no
     `NEEDS_HUMAN_RECORDED` marker remains.

   **Self-re-arm (final step, required):**
   After implementation and verification, APPEND exactly one bounded source/docs-read-only
   AUDIT task as task 169 — a "Post-shortcut Phase 9 next-task source audit" mirroring the
   task-167 audit block: confirm the new Ctrl+Shift+Delete accelerator routes ONLY to
   `Command::DeleteCurrentCadCuboid` via the generic route + exact-tracked-CAD guard, collides
   with no existing accelerator (`detect_conflicts()` empty; table = 19), leaves the bare-Delete
   accelerator / Cut / wrapper-world delete and `editor-actions`/CAD/`editor-state`/egui-host
   rendering unchanged, and that only the menu-definition + fixtures changed. Task 169 is
   docs/source-read-only (its `MAY edit` includes `.ai/dispatch.tasks.md`, `Status.md`,
   `HANDOFF.md`, `plans/BASELINE.md`, `change.md`; it MUST NOT edit Rust source, tests, or
   automation). Task 169's final step appends the next bounded FEATURE task (or, if none is
   in-policy, records a single `NEEDS_HUMAN_RECORDED: <ISO-date> - <reason>` marker plus a
   "Recommendation for human approval" block). Copy this Self-re-arm requirement verbatim into
   the task 169 block you author. Edit `.ai/dispatch.tasks.md` to do this.

169. **[DONE-SUPERSEDED 2026-06-21 via RustCADs/RGE #433 / PR #434 (commit 50fcd3b); pre-migration] Post-shortcut Phase 9 next-task source audit (source/docs-read-only, gated).**

   Audit the newly bound Ctrl+Shift+Delete accelerator without changing Rust source,
   tests, Cargo metadata, schemas, workflows, scripts, automation, packet templates,
   generated non-current-dispatch artifacts, CAD crates, editor-actions, editor-state,
   editor-shell production logic, or editor-egui-host production rendering/projection
   logic. This is a bounded source/docs-read-only audit of task 168's implementation:
   confirm the new accelerator routes ONLY to `Command::DeleteCurrentCadCuboid` through
   the existing generic menu path and exact-tracked-CAD guard, collides with no existing
   accelerator, leaves the bare Delete accelerator / Cut / wrapper-world delete and
   `editor-actions`/CAD/`editor-state`/egui-host rendering unchanged, and that only the
   menu definition plus expected fixtures changed.

   **Scope guard (operator decision - non-negotiable):**
   - MAY edit only:
     - `.ai/dispatch.tasks.md`
     - `Status.md`
     - `HANDOFF.md`
     - `plans/BASELINE.md`
     - `change.md`
   - Audit/read source and docs as needed, including the dispatcher-provided GitHub
     state snapshot for GitHub queue evidence and local source reads for source
     evidence.
   - MUST NOT edit Rust source, tests, Cargo files, schemas, workflows, scripts,
     automation, packet templates, generated non-current-dispatch artifacts,
     editor-actions, CAD crates, editor-state, editor-shell source, or
     editor-egui-host production rendering/projection logic.
   - MUST NOT append task 170 except as the final-step next bounded FEATURE task if
     it is in-policy; otherwise record exactly one
     `NEEDS_HUMAN_RECORDED: <ISO-date> - <reason>` marker plus a "Recommendation for
     human approval" block.

   **Required audit checks:**
   - Confirm `edit.delete_current_cad_cuboid` binds exactly
     `Shortcut::new(Modifiers::CTRL | Modifiers::SHIFT, Key::Delete)` and that
     `Ctrl+Shift+Delete` resolves only to `Command::DeleteCurrentCadCuboid`.
   - Confirm the accelerator uses the existing generic route:
     `keycode_to_shortcut` -> `enabled_command_for_shortcut` ->
     `route_menu_command(Command::DeleteCurrentCadCuboid)`, and that the route guard
     remains the exact-tracked-CAD selection helper with no fallback deletion,
     selection mutation, face-selection pruning, stale-CAD cleanup, or bus-stack growth.
   - Confirm `AcceleratorTable::detect_conflicts()` / resolved conflicts remain empty,
     the accelerator table has exactly 19 entries, and the bare `Delete` accelerator
     still resolves to `Command::Delete`.
   - Confirm `Command::Cut`, `cut_selected_entities`, wrapper-world
     `delete_selected_entities`, editor-actions, CAD crates, editor-state, Cargo
     metadata, and editor-egui-host production rendering/projection logic are unchanged.
   - Confirm only the menu definition and expected fixtures changed for task 168, plus
     the current dispatch handoff/sidecar artifacts.

   **Verification:**
   - `git diff -- crates/editor-ui/src/menus/default_menu.rs crates/editor-ui/tests/menus_ordering.rs crates/editor-egui-host/src/menu_tests.rs crates/editor-egui-host/src/shortcut_help.rs .ai/dispatch.tasks.md`
   - `git diff -- crates/editor-shell crates/editor-actions crates/cad-core crates/cad-graph crates/cad-projection crates/editor-state crates/editor-ui/src/menus/command.rs crates/editor-ui/src/menus/predicate.rs crates/editor-egui-host/src/menu.rs crates/editor-egui-host/src/lib.rs crates/editor-egui-host/src/handoff.rs crates/editor-egui-host/src/shortcut_conflicts.rs crates/editor-egui-host/src/palette_*.rs Cargo.toml Cargo.lock`
     EXPECTING no changes.
   - `rg -n "Ctrl\\+Shift\\+Delete|Modifiers::CTRL \\| Modifiers::SHIFT, Key::Delete|Shortcut::new\\(Modifiers::CTRL \\| Modifiers::SHIFT, Key::Delete\\)|Shortcut::plain\\(Key::Delete\\)|Command::Delete,|Command::Cut|cut_selected_entities|delete_selected_entities|route_menu_command|delete_menu_selection_is_exact_tracked_cad_entity|has_current_cad_cuboid_selection" crates/editor-ui/src/menus/default_menu.rs crates/editor-ui/tests/menus_ordering.rs crates/editor-ui/src/menus/command.rs crates/editor-ui/src/menus/predicate.rs crates/editor-shell/src/render_path.rs crates/editor-shell/src/lifecycle/mod.rs crates/editor-shell/src/lifecycle/accelerator.rs crates/editor-shell/src/lifecycle/commands.rs crates/editor-shell/src/lifecycle/tests.rs crates/editor-egui-host/src/menu_tests.rs crates/editor-egui-host/src/shortcut_help.rs`
   - `cargo test -p rge-editor-ui`
   - `cargo test -p rge-editor-egui-host`
   - `cargo test -p rge-editor-shell --lib -- route_menu_command`
   - `cargo test -p rge-editor-shell --lib -- delete_current_cad_cuboid`
   - `cargo check -p rge-editor-ui -p rge-editor-shell -p rge-editor-egui-host`
   - `cargo +nightly fmt --all -- --check`
   - `git diff --check`
   - `rg -n "^168\\.|^169\\.|^170\\.|NEEDS_HUMAN_RECORDED" .ai/dispatch.tasks.md`
     EXPECTING exactly one task 168 and exactly one task 169; no task 170; no completed
     current `NEEDS_HUMAN_RECORDED` marker.

   **Done criteria:**
   - Audit findings are source-grounded and no Rust source/test changes are made by
     this audit task.
   - The task records whether a next bounded FEATURE task is in-policy; if not,
     exactly one `NEEDS_HUMAN_RECORDED: <ISO-date> - <reason>` marker plus a
     "Recommendation for human approval" block is recorded.
   - No task 170 is appended unless it is the final bounded FEATURE task permitted by
     this task's final step.

   **Carried self-rearm instruction from task 168 (verbatim):**

   **Self-re-arm (final step, required):**
   After implementation and verification, APPEND exactly one bounded source/docs-read-only
   AUDIT task as task 169 - a "Post-shortcut Phase 9 next-task source audit" mirroring the
   task-167 audit block: confirm the new Ctrl+Shift+Delete accelerator routes ONLY to
   `Command::DeleteCurrentCadCuboid` via the generic route + exact-tracked-CAD guard, collides
   with no existing accelerator (`detect_conflicts()` empty; table = 19), leaves the bare-Delete
   accelerator / Cut / wrapper-world delete and `editor-actions`/CAD/`editor-state`/egui-host
   rendering unchanged, and that only the menu-definition + fixtures changed. Task 169 is
   docs/source-read-only (its `MAY edit` includes `.ai/dispatch.tasks.md`, `Status.md`,
   `HANDOFF.md`, `plans/BASELINE.md`, `change.md`; it MUST NOT edit Rust source, tests, or
   automation). Task 169's final step appends the next bounded FEATURE task (or, if none is
   in-policy, records a single `NEEDS_HUMAN_RECORDED: <ISO-date> - <reason>` marker plus a
   "Recommendation for human approval" block). Copy this Self-re-arm requirement verbatim into
   the task 169 block you author. Edit `.ai/dispatch.tasks.md` to do this.

   **Audit result (2026-06-21 / ISSUE-433):**
   - Local source reads confirm `edit.delete_current_cad_cuboid` binds exactly
     `Shortcut::new(Modifiers::CTRL | Modifiers::SHIFT, Key::Delete)` in
     `crates/editor-ui/src/menus/default_menu.rs`, and the default-menu plus
     integration tests resolve Ctrl+Shift+Delete only to
     `Command::DeleteCurrentCadCuboid`; bare Delete still resolves to
     `Command::Delete`.
   - The live accelerator remains on the generic menu route:
     `keycode_to_shortcut` -> `enabled_command_for_shortcut` ->
     `route_menu_command(Command::DeleteCurrentCadCuboid)`. The dedicated route
     still checks `delete_menu_selection_is_exact_tracked_cad_entity()` first,
     returns on a false guard, and only calls `delete_current_cad_cuboid()` after
     the exact tracked-CAD guard passes.
   - Focused route tests still cover stale and false-guard rejection with no
     fallback to wrapper-world delete, no selection or face-selection mutation,
     and no bus-stack growth; Cut remains wrapper-world only for an exact tracked
     CAD selection.
   - `AcceleratorTable` conflict assertions remain empty and the resolved table
     remains exactly 19 entries. Host menu and shortcut-help projections carry
     the `Ctrl+Shift+Delete` display string through data-driven projection.
   - Required prohibited-surface diffs returned no output for editor-shell,
     editor-actions, CAD crates, editor-state, command/predicate menu source,
     editor-egui-host production projection/rendering files, Cargo metadata, and
     palette files. The `editor-actions` dependency search returned no
     CAD/editor-shell/projection matches.
   - GitHub queue evidence came only from the dispatcher snapshot in
     `.ai/dispatch-ISSUE-433/codex.plan.rev0.log`, generated
     `2026-06-21T11:46:21.9185688+03:00`; no `gh`, browser, network, GitHub API,
     or web lookup was used.
   - Final self-rearm decision: no task 170 is appended. This audit confirmed the
     scoped shortcut boundary and did not expose a new independently safe feature
     slice. Current local status/baseline reads still classify the remaining
     Phase 9 candidates as requiring human product/architecture choice across
     remapping/fatal policy, route ownership, real plugin runtime, OS/typed
     clipboard, or CAD/CommandBus authority.

170. **[DONE-SUPERSEDED 2026-06-21 via drhalilmurattasci/RGE PR #1 (commit 200e7a6); original dispatch #436 stalled, salvaged] Establish keybinding ownership + a fatal/non-fatal accelerator-conflict policy
   (editor-ui owner + single host-startup enforcement; NO remapping runtime).**

   Narrow Phase-9 groundwork — keybinding POLICY only, not remapping UI/runtime. Two
   deliverables: (a) make default-accelerator OWNERSHIP explicit, and (b) ENFORCE the
   conflict policy: DEFAULT (developer-owned) accelerator conflicts are FATAL (fail-fast
   at startup), while runtime conflict handling stays NON-FATAL (unchanged). Adds NO
   persistence, NO user-editable/remappable bindings, NO remapping runtime, NO plugin
   hooks, NO settings UI, NO OS/clipboard integration, and changes NO accelerator VALUE.

   **Source-grounded facts (2026-06-21):**
   - `default_editor_menu()` in `crates/editor-ui/src/menus/default_menu.rs` is the single
     source of truth for default accelerators (module doc lines 1-37). Its only PRODUCTION
     call sites are the host startup build at `crates/editor-egui-host/src/lib.rs:455`
     (`let menu_registry = default_editor_menu();`, built ONCE in `EguiHost::new()`) and the
     per-keystroke resolve at `crates/editor-shell/src/lifecycle/mod.rs:2956`; every other
     call site is a test.
   - Conflicts are detected by `AcceleratorTable::detect_conflicts()`
     (`crates/editor-ui/src/menus/shortcut.rs:293`) and surfaced NON-FATALLY via
     `MenuRegistry::resolve()`'s `ResolveResult.conflicts`; the per-keystroke path
     suppresses a conflicted shortcut (`enabled_command_for_shortcut`) and the host only
     DISPLAYS conflicts (`crates/editor-egui-host/src/shortcut_conflicts.rs`). There is NO
     fatal/startup enforcement today; the zero-conflict invariant is only a test
     (`default_menu.rs:537` `executable_accelerators_have_no_conflicts_and_bind_exactly_nineteen`).
   - Only default (developer-owned) accelerators exist — there are no user-editable or
     runtime-mutable bindings (`PredicateContext` holds only state predicates; `MenuEntry`
     has no mutable-binding field).

   **Scope guard (operator decision - non-negotiable):**
   - MAY edit:
     - `crates/editor-ui/src/menus/default_menu.rs` — add a small, PURE, testable
       enforcement helper that resolves the registry against `PredicateContext::default()`
       and FATAL-panics (clear message listing the offending shortcut(s)) when
       `resolved.conflicts` is non-empty; state the ownership + fatal/non-fatal policy in
       the module/function doc; extend the test module (keep the real-menu zero-conflict
       assertion; add a `#[should_panic]` test that an injected duplicate-shortcut registry
       trips the helper, mirroring `shortcut.rs:367` / `registry.rs:727`).
     - `crates/editor-egui-host/src/lib.rs` — at the SINGLE startup build site (~line 455,
       `EguiHost::new()`), invoke that editor-ui enforcement helper on the canonical
       `default_editor_menu()` registry so a default-accelerator conflict is FATAL at
       startup. No other host change.
     - `.ai/dispatch.tasks.md` — the self-re-arm append (task 171) only.
   - MUST NOT edit / add: any accelerator VALUE or menu entry (the 19 accelerators stay
     exactly as-is); the per-keystroke routing in `crates/editor-shell/src/lifecycle/mod.rs`
     or `lifecycle/accelerator.rs`; the non-fatal conflict DISPLAY in
     `crates/editor-egui-host/src/shortcut_conflicts.rs`; `Command`/`predicate` logic; CAD
     crates; `editor-actions`; `editor-state`; Cargo metadata; schemas; workflows;
     automation scripts; packet templates. NO persistence / config file, NO user-editable
     or remappable bindings, NO remapping runtime or keybinding-mutation API, NO plugin
     discovery/loading hooks, NO settings UI, NO OS/typed-clipboard integration.
   - If the fatal policy cannot be enforced without crossing into a MUST-NOT surface (new
     runtime/persistence/UI), STOP and record a `NEEDS_HUMAN_RECORDED` marker instead of
     forcing the change.

   **Required steps:**
   - Define the policy in code + docs: DEFAULT accelerator conflicts are a developer error
     and FATAL at startup; runtime/user conflicts (none today) remain NON-FATAL (the
     existing suppression + display path), so the fatal check applies ONLY to the default
     accelerator set.
   - Enforce it ONCE at the host startup build site (no per-keystroke panic; the
     per-frame / per-keystroke resolves stay non-fatal). The real default menu is
     conflict-free, so the check never fires in normal operation — it only fail-fasts a
     future regression.
   - Keep all new tests in `editor-ui` (PURE; construct no `wgpu`/`GfxContext`, so the
     `test_lock::guard()` pattern is not required).

   **Verification:**
   - `cargo test -p rge-editor-ui`
   - `cargo test -p rge-editor-egui-host`
   - `cargo check -p rge-editor-ui -p rge-editor-egui-host -p rge-editor-shell`
   - `git diff -- crates/editor-shell/src/lifecycle/mod.rs crates/editor-shell/src/lifecycle/accelerator.rs crates/editor-egui-host/src/shortcut_conflicts.rs crates/editor-actions crates/cad-core crates/cad-graph crates/cad-projection crates/editor-state crates/editor-ui/src/menus/command.rs crates/editor-ui/src/menus/predicate.rs Cargo.toml Cargo.lock`
     EXPECTING no changes.
   - `rg -n "detect_conflicts|conflicts\.is_empty|should_panic|fatal" crates/editor-ui/src/menus/default_menu.rs`
     showing the new enforcement helper + tests.
   - Confirm the resolved default accelerator table still has exactly 19 entries and zero
     conflicts (no accelerator value changed).
   - `cargo +nightly fmt --all -- --check`
   - `git diff --check`
   - `rg -n "^169\.|^170\.|^171\.|NEEDS_HUMAN_RECORDED" .ai/dispatch.tasks.md`
     EXPECTING exactly one task 170 and exactly one task 171; no leftover
     `NEEDS_HUMAN_RECORDED` marker.

   **Done criteria:**
   - Default-accelerator conflicts are FATAL at the single host startup build site (via a
     pure editor-ui enforcement helper); runtime conflict handling (per-keystroke
     suppression + UI display) is unchanged; no accelerator value changed; table = 19
     entries, zero conflicts.
   - New editor-ui tests cover both the conflict-free real menu and the fatal path on an
     injected conflict.
   - No change to any MUST-NOT surface.
   - Exactly one bounded AUDIT task 171 is appended per the self-re-arm protocol; no
     `NEEDS_HUMAN_RECORDED` marker remains.

   **Self-re-arm (final step, required):**
   After implementation and verification, APPEND exactly one bounded source/docs-read-only
   AUDIT task as task 171 — a "Post-keybinding-policy Phase 9 next-task source audit"
   mirroring the task-169 audit block: confirm default-accelerator conflicts are fatal at
   the single host startup site, the per-keystroke/non-fatal display paths are unchanged,
   no accelerator value changed (table = 19, zero conflicts), and that only editor-ui
   (enforcement + tests) and the editor-egui-host startup call site changed. Task 171 is
   docs/source-read-only (its `MAY edit` includes `.ai/dispatch.tasks.md`, `Status.md`,
   `HANDOFF.md`, `plans/BASELINE.md`, `change.md`; it MUST NOT edit Rust source, tests, or
   automation). Task 171's final step appends the next bounded FEATURE task (or, if none is
   in-policy, records a single `NEEDS_HUMAN_RECORDED: <ISO-date> - <reason>` marker plus a
   "Recommendation for human approval" block). Copy this Self-re-arm requirement verbatim
   into the task 171 block you author. Edit `.ai/dispatch.tasks.md` to do this.

171. **Post-keybinding-policy Phase 9 next-task source audit
   (docs/source-read-only).**

   Audit the completed keybinding ownership/conflict-policy dispatch. Confirm
   default-accelerator conflicts are fatal only at the single host startup site,
   the per-keystroke/non-fatal display paths are unchanged, no accelerator value
   changed (table = 19, zero conflicts), and only editor-ui enforcement/tests plus
   the editor-egui-host startup call site changed.

   **Scope guard (operator decision - non-negotiable):**
   - MAY edit only:
     - `.ai/dispatch.tasks.md`
     - `Status.md`
     - `HANDOFF.md`
     - `plans/BASELINE.md`
     - `change.md`
   - Audit/read source and docs as needed, including the dispatcher-provided GitHub
     state snapshot for GitHub queue evidence and local source reads for source
     evidence.
   - MUST NOT edit Rust source, tests, Cargo files, schemas, workflows, scripts,
     automation, packet templates, generated non-current-dispatch artifacts,
     editor-actions, CAD crates, editor-state, editor-shell source, or
     editor-egui-host production rendering/projection logic.
   - MUST NOT append task 172 except as the final-step next bounded FEATURE task if
     it is in-policy; otherwise record exactly one
     `NEEDS_HUMAN_RECORDED: <ISO-date> - <reason>` marker plus a "Recommendation for
     human approval" block.

   **Required audit checks:**
   - Confirm `default_editor_menu()` owns the developer-maintained default
     accelerator set and the code docs state that default conflicts are
     startup-fatal developer errors while runtime conflict projection/display and
     per-keystroke suppression remain non-fatal.
   - Confirm `assert_default_accelerators_conflict_free()` is pure, resolves a
     supplied default `MenuRegistry` against `PredicateContext::default()`, and
     panics only when conflicts exist with each offending shortcut and entry id
     visible in the panic message.
   - Confirm `EguiHost::new()` calls the helper once immediately after
     `let menu_registry = default_editor_menu();` and imports the helper through
     `rge_editor_ui::menus::default_menu`.
   - Confirm `crates/editor-shell/src/lifecycle/mod.rs`,
     `crates/editor-shell/src/lifecycle/accelerator.rs`, and
     `crates/editor-egui-host/src/shortcut_conflicts.rs` remain unchanged: no
     per-keystroke panic, no fatal runtime display behavior, no accelerator
     remapping, no persistence, no settings UI, and no plugin hook was added.
   - Confirm the real default menu still resolves to exactly 19 accelerator
     entries, `detect_conflicts()` / `resolved.conflicts` are empty, and no
     accelerator value, command id, menu entry id, or menu order changed.
   - Confirm task 170 changed only the allowed implementation surfaces plus the
     current dispatch handoff/sidecar artifacts.

   **Verification:**
   - `git diff -- crates/editor-ui/src/menus/default_menu.rs crates/editor-egui-host/src/lib.rs .ai/dispatch.tasks.md`
   - `git diff -- crates/editor-shell/src/lifecycle/mod.rs crates/editor-shell/src/lifecycle/accelerator.rs crates/editor-shell/src/lifecycle/commands.rs crates/editor-egui-host/src/shortcut_conflicts.rs crates/editor-egui-host/src/menu.rs crates/editor-egui-host/src/menu_tests.rs crates/editor-egui-host/src/shortcut_help.rs crates/editor-ui/src/menus/mod.rs crates/editor-ui/src/menus/command.rs crates/editor-ui/src/menus/predicate.rs crates/editor-ui/tests/menus_ordering.rs crates/editor-actions crates/cad-core crates/cad-graph crates/cad-projection crates/editor-state Cargo.toml Cargo.lock .github/workflows .ai/*.schema.json`
     EXPECTING no changes.
   - `rg -n "assert_default_accelerators_conflict_free|default accelerator|startup-fatal|runtime conflict|conflicts\.is_empty|should_panic|panic!" crates/editor-ui/src/menus/default_menu.rs crates/editor-egui-host/src/lib.rs`
   - `rg -n "default_editor_menu\(|enabled_command_for_shortcut|shortcut_conflicts|ResolveResult|conflicts" crates/editor-ui/src/menus/default_menu.rs crates/editor-ui/src/menus/registry.rs crates/editor-egui-host/src/lib.rs crates/editor-egui-host/src/shortcut_conflicts.rs crates/editor-shell/src/lifecycle/mod.rs crates/editor-shell/src/lifecycle/accelerator.rs`
   - `(Get-Content -LiteralPath 'crates/editor-ui/src/menus/default_menu.rs' | Measure-Object -Line).Lines` plus `rg -n "SPLIT-EXEMPTION" crates/editor-ui/src/menus/default_menu.rs`
     EXPECTING the file to remain under 1000 lines or carry a valid
     `// SPLIT-EXEMPTION:` annotation.
   - `cargo test -p rge-editor-ui`
   - `cargo test -p rge-editor-egui-host`
   - `cargo check -p rge-editor-ui -p rge-editor-egui-host -p rge-editor-shell`
   - `cargo run -q -p rge-tool-architecture-lints -- all`
   - `cargo +nightly fmt --all -- --check`
   - `git diff --check`
   - `rg -n "^170\.|^171\.|^172\.|NEEDS_HUMAN_RECORDED" .ai/dispatch.tasks.md`
     EXPECTING exactly one task 171; no task 172 unless it is the final bounded
     FEATURE task appended by this audit, and no completed current
     `NEEDS_HUMAN_RECORDED` marker.

   **Done criteria:**
   - Audit findings are source-grounded and no Rust source/test changes are made by
     this audit task.
   - The task records whether a next bounded FEATURE task is in-policy; if not,
     exactly one `NEEDS_HUMAN_RECORDED: <ISO-date> - <reason>` marker plus a
     "Recommendation for human approval" block is recorded.
   - No task 172 is appended unless it is the final bounded FEATURE task permitted by
     this task's final step.

   **Carried self-rearm instruction from task 170 (verbatim):**

   **Self-re-arm (final step, required):**
   After implementation and verification, APPEND exactly one bounded source/docs-read-only AUDIT task as task 171 - a "Post-keybinding-policy Phase 9 next-task source audit" mirroring the task-169 audit block: confirm default-accelerator conflicts are fatal at the single host startup site, the per-keystroke/non-fatal display paths are unchanged, no accelerator value changed (table = 19, zero conflicts), and that only editor-ui (enforcement + tests) and the editor-egui-host startup call site changed. Task 171 is docs/source-read-only (its `MAY edit` includes `.ai/dispatch.tasks.md`, `Status.md`, `HANDOFF.md`, `plans/BASELINE.md`, `change.md`; it MUST NOT edit Rust source, tests, or automation). Task 171's final step appends the next bounded FEATURE task (or, if none is in-policy, records a single `NEEDS_HUMAN_RECORDED: <ISO-date> - <reason>` marker plus a "Recommendation for human approval" block). Copy this Self-re-arm requirement verbatim into the task 171 block you author. Edit `.ai/dispatch.tasks.md` to do this.

### Human/Codex product-owner approval record for task 172

2026-06-26: Codex, acting in the delegated product-owner / human-supervisor role,
resolved the task-171 NEEDS_HUMAN boundary by selecting the smallest bounded
Phase 9 feature slice: an in-memory keybinding remap data-plane API in
`editor-ui` only. This supersedes the 2026-06-25 marker; the marker is removed so
the autonomous selector can file task 172.

Selected surface and behavior: a pure resolver-layer keybinding override profile.
Given a `KeybindingTarget` for a known menu entry, an in-memory override can remap
that entry to a new executable shortcut or unbind it for the current resolve.
The resolver reports the active shortcuts and conflicts deterministically.

Publish posture: task 172 is production Rust source under `crates/**` and edits
the dispatch brief for self-rearm, so it must land as a reviewable PR. Under a
delegated-human scheduler using `-PublishMode main`, keep `-SurfaceSplitPublish`;
this task's high-risk paths must downgrade to PR and must not fast-forward
`origin/main`.

172. **In-memory keybinding remap API for menu resolve (editor-ui only, PR-only).**

   Add the minimal in-memory keybinding override layer for the menu registry.
   This is a data-plane/resolver slice only: no settings UI, no persistence,
   no host/shell wiring, no command-bus changes, and no default accelerator value
   changes.

   **Scope guard (operator decision - non-negotiable):**
   - MAY edit only:
     - `crates/editor-ui/src/menus/keybinding.rs` (new)
     - `crates/editor-ui/src/menus/mod.rs`
     - `crates/editor-ui/src/menus/registry.rs`
     - `crates/editor-ui/tests/menus_ordering.rs`
     - `.ai/dispatch.tasks.md` only for the required final self-rearm audit task
   - MUST NOT edit:
     - `crates/editor-ui/src/menus/default_menu.rs`
     - `crates/editor-ui/src/menus/command.rs`
     - `crates/editor-ui/src/menus/predicate.rs`
     - `crates/editor-ui/src/menus/shortcut.rs`
     - `crates/editor-shell/**`
     - `crates/editor-egui-host/**`
     - `crates/editor-actions/**`
     - `crates/editor-state/**`
     - `crates/plugin-host/**`
     - `crates/runtime-wasmtime/**`
     - `crates/cad-*` / `crates/cad-*/**`
     - `Cargo.toml`, `Cargo.lock`, any `**/Cargo.toml`
     - `.github/workflows/**`, `.ai/*.schema.json`, `.ai/dispatch.verify.ps1`
     - root automation scripts, packet templates, generated non-current-dispatch
       artifacts
   - MUST NOT add UI, persistence, import/export, host/shell route ownership,
     `CommandBus`, plugin runtime behavior, OS clipboard behavior, CAD behavior,
     or any new dependency.
   - MUST NOT change any shipped default menu entry id, command id, menu order,
     label, shortcut, shortcut hint, predicate, enablement predicate, or
     accelerator count.
   - If the implementation requires broader route ownership, persistence/settings
     UI, host/shell wiring, `CommandBus`, Cargo metadata, or any forbidden path,
     HALT instead of broadening scope.

   **Required implementation:**
   - Add `menus::keybinding` with public types for targeting and overriding
     menu-entry bindings, including:
     - `KeybindingTarget` identifying a menu entry by extension point id plus
       entry id;
     - `KeybindingOverride` / `KeybindingOverrides`, where `Some(Shortcut)`
       remaps the target and `None` clears/unbinds it for this resolve.
   - Export the new types through `menus::mod.rs` without changing existing public
     exports' names or behavior.
   - Add `MenuRegistry::resolve_with_keybinding_overrides(...)` in
     `registry.rs`. `MenuRegistry::resolve(&PredicateContext)` must remain the
     default behavior and must be equivalent to resolving with an empty override
     set.
   - Unknown override targets are diagnostics, not panics, and do not mutate the
     resolved menu tree.
   - Conflicts introduced by overrides remain non-fatal resolve-time data:
     `ResolveResult::conflicts` reports them deterministically and
     `enabled_command_for_shortcut` continues to suppress conflicted shortcuts.
   - Unbinding an entry removes its executable shortcut from the accelerator table
     for that resolve but leaves the visible menu entry present.

   **Verification:**
   - `cargo test -p rge-editor-ui`
   - `cargo test -p rge-editor-ui --test menus_ordering`
   - `cargo check -p rge-editor-ui -p rge-editor-egui-host -p rge-editor-shell`
   - `cargo run -q -p rge-tool-architecture-lints -- all`
   - `cargo +nightly fmt --all -- --check`
   - `git diff --check`
   - `git diff --name-only` and `git diff --stat`, expecting no files outside the
     MAY-edit set except current-dispatch handoff/log artifacts
   - `git diff -- crates/editor-ui/src/menus/default_menu.rs crates/editor-ui/src/menus/command.rs crates/editor-ui/src/menus/predicate.rs crates/editor-ui/src/menus/shortcut.rs crates/editor-shell crates/editor-egui-host crates/editor-actions crates/editor-state crates/plugin-host crates/runtime-wasmtime crates/cad-core crates/cad-graph crates/cad-projection Cargo.toml Cargo.lock .github/workflows .ai/*.schema.json .ai/dispatch.verify.ps1`
     EXPECTING no changes.
   - `rg -n "KeybindingTarget|KeybindingOverride|resolve_with_keybinding_overrides|unknown.*keybinding|conflict" crates/editor-ui/src/menus crates/editor-ui/tests/menus_ordering.rs`
   - `rg -n "^171\.|^172\.|^173\.|NEEDS_HUMAN_RECORDED" .ai/dispatch.tasks.md`
     EXPECTING exactly one live task 172, no live `NEEDS_HUMAN_RECORDED` marker,
     and task 173 only if appended as the required final audit task.

   **Behavioral coverage required in `menus_ordering.rs`:**
   - Empty overrides preserve default behavior: the default editor menu still
     resolves to 19 executable accelerators and zero conflicts.
   - Remapping a known target changes that entry's active executable shortcut for
     this resolve only and leaves the registry/default menu unchanged for a later
     normal `resolve`.
   - Unbinding a known target removes its executable shortcut from the accelerator
     table and from `command_for_shortcut` / `enabled_command_for_shortcut` for
     that resolve.
   - Remapping a known target onto another live shortcut reports a deterministic
     conflict and suppresses keyboard execution for that conflicted shortcut while
     keeping display/introspection deterministic.
   - An unknown target produces a diagnostic without changing the resolved menu
     tree, accelerator count, or conflict set.

   **Done criteria:**
   - The public API is limited to in-memory data types and a resolver entry point.
   - All required behavioral tests pass and default behavior remains unchanged.
   - The prohibited-surface diff gate shows no forbidden edits.
   - The dispatch result is PR-routed, not auto-merged to `origin/main`.
   - The final step appends exactly one bounded source/docs-read-only AUDIT task
     as task 173: a "Post-keybinding-remap-api Phase 9 next-task source audit"
     that verifies task 172 stayed editor-ui-only, default accelerator behavior
     stayed unchanged, override diagnostics/conflicts/unbinding are covered, no
     host/shell/persistence/UI/Cargo/automation surface changed, and then appends
     the next bounded FEATURE task or records a single `NEEDS_HUMAN_RECORDED`
     marker with a recommendation for human approval.

   **Self-re-arm (final step, required):**
   After implementation and verification, APPEND exactly one bounded
   source/docs-read-only AUDIT task as task 173 - a "Post-keybinding-remap-api
   Phase 9 next-task source audit" mirroring the task-171 audit block: confirm
   the in-memory remap API stayed confined to editor-ui menus, default resolve
   still has 19 executable accelerators and zero conflicts, remap/unbind/
   conflict/unknown-target diagnostics are covered, and no host/shell,
   persistence, settings UI, plugin runtime, CAD, Cargo, workflow, schema, or
   automation surface changed. Task 173 is docs/source-read-only (its `MAY edit`
   includes `.ai/dispatch.tasks.md`, `Status.md`, `HANDOFF.md`,
   `plans/BASELINE.md`, `change.md`; it MUST NOT edit Rust source, tests, or
   automation). Task 173's final step appends the next bounded FEATURE task (or,
   if none is in-policy, records a single `NEEDS_HUMAN_RECORDED: <ISO-date> -
   <reason>` marker plus a "Recommendation for human approval" block). Copy this
   Self-re-arm requirement verbatim into the task 173 block you author. Edit
   `.ai/dispatch.tasks.md` to do this.

173. **Post-keybinding-remap-api Phase 9 next-task source audit
   (docs/source-read-only).**

   Audit the completed in-memory keybinding remap API dispatch. Confirm the
   remap/unbind override data plane stayed confined to `editor-ui` menus,
   default resolve still has exactly 19 executable accelerators and zero
   conflicts, the override behaviors and diagnostics are covered by tests, and
   no host/shell, persistence, settings UI, plugin runtime, CAD, Cargo,
   workflow, schema, or automation surface changed.

   **Scope guard (operator decision - non-negotiable):**
   - MAY edit only:
     - `.ai/dispatch.tasks.md`
     - `Status.md`
     - `HANDOFF.md`
     - `plans/BASELINE.md`
     - `change.md`
   - Audit/read source and docs as needed, including the current dispatch
     packets and local source reads for source evidence.
   - MUST NOT edit Rust source, tests, Cargo files, schemas, workflows, scripts,
     automation, packet templates, generated non-current-dispatch artifacts,
     editor-shell, editor-egui-host, editor-actions, editor-state, plugin-host,
     runtime-wasmtime, CAD crates, or any host/shell/runtime integration surface.
   - MUST NOT append task 174 except as the final-step next bounded FEATURE task
     if it is in-policy; otherwise record exactly one
     `NEEDS_HUMAN_RECORDED: <ISO-date> - <reason>` marker plus a
     "Recommendation for human approval" block.

   **Required audit checks:**
   - Confirm `menus::keybinding` contains only in-memory data types for
     `KeybindingTarget`, `KeybindingOverride`, `KeybindingOverrides`, and the
     resolve-time diagnostic data; it must not read or write disk, settings,
     environment variables, host state, plugin runtime, or Cargo-configured
     resources.
   - Confirm `MenuRegistry::resolve(&PredicateContext)` remains the default path
     and is equivalent to
     `resolve_with_keybinding_overrides(..., &KeybindingOverrides::default())`
     for the asserted default fields.
   - Confirm overrides apply only to cloned resolved entries for one resolve and
     do not mutate the registry, default menu definition, or later normal
     resolves.
   - Confirm unbinding removes only the executable `shortcut` from the resolved
     entry, accelerator table, `command_for_shortcut`, and
     `enabled_command_for_shortcut`; it must not remove the visible menu entry,
     command, label, order/section data, enablement result, or passive
     `shortcut_hint`.
   - Confirm remapping updates the active executable shortcut and that
     override-induced conflicts remain non-fatal `ResolveResult::conflicts`
     data with `enabled_command_for_shortcut` suppressing conflicted execution.
   - Confirm unknown targets are deterministic diagnostics for extension
     point/entry id pairs unknown before visibility filtering, and that known
     but hidden/disabled entries are not reported as unknown.
   - Confirm the shipped default menu is unchanged: exactly 19 executable
     accelerators, zero conflicts, and no changed entry ids, command ids, labels,
     order, predicates, enablement predicates, shortcut values, or passive
     shortcut hints.
   - Confirm task 172 changed only the allowed implementation surfaces plus the
     current dispatch handoff/sidecar artifacts.

   **Verification:**
   - `git diff -- crates/editor-ui/src/menus/keybinding.rs crates/editor-ui/src/menus/mod.rs crates/editor-ui/src/menus/registry.rs crates/editor-ui/tests/menus_ordering.rs .ai/dispatch.tasks.md`
   - `git diff -- crates/editor-ui/src/menus/default_menu.rs crates/editor-ui/src/menus/command.rs crates/editor-ui/src/menus/predicate.rs crates/editor-ui/src/menus/shortcut.rs crates/editor-shell crates/editor-egui-host crates/editor-actions crates/editor-state crates/plugin-host crates/runtime-wasmtime crates/cad-core crates/cad-graph crates/cad-projection Cargo.toml Cargo.lock .github/workflows .ai/*.schema.json .ai/dispatch.verify.ps1`
     EXPECTING no changes.
   - `rg -n "KeybindingTarget|KeybindingOverride|KeybindingOverrides|resolve_with_keybinding_overrides|UnknownTarget|keybinding_diagnostics|conflict" crates/editor-ui/src/menus crates/editor-ui/tests/menus_ordering.rs`
   - `rg -n "host|shell|settings|persist|plugin|cad|Cargo|workflow|schema|dispatch.verify" crates/editor-ui/src/menus/keybinding.rs crates/editor-ui/src/menus/registry.rs crates/editor-ui/tests/menus_ordering.rs`
     EXPECTING matches only in comments/test descriptions that preserve the
     no-host/no-persistence/no-Cargo boundary, not implementation wiring.
   - `cargo test -p rge-editor-ui`
   - `cargo test -p rge-editor-ui --test menus_ordering`
   - `cargo check -p rge-editor-ui -p rge-editor-egui-host -p rge-editor-shell`
   - `cargo run -q -p rge-tool-architecture-lints -- all`
   - `cargo +nightly fmt --all -- --check`
   - `git diff --check`
   - `rg -n "^171\.|^172\.|^173\.|^174\.|NEEDS_HUMAN_RECORDED" .ai/dispatch.tasks.md`
     EXPECTING exactly one task 172 and exactly one task 173; no task 174 unless
     it is the final bounded FEATURE task appended by this audit, and no
     completed current `NEEDS_HUMAN_RECORDED` marker.

   **Done criteria:**
   - Audit findings are source-grounded and no Rust source/test changes are made
     by this audit task.
   - The task records whether a next bounded FEATURE task is in-policy; if not,
     exactly one `NEEDS_HUMAN_RECORDED: <ISO-date> - <reason>` marker plus a
     "Recommendation for human approval" block is recorded.
   - No task 174 is appended unless it is the final bounded FEATURE task
     permitted by this task's final step.

   **Carried self-rearm instruction from task 172 (verbatim):**

   **Self-re-arm (final step, required):**
   After implementation and verification, APPEND exactly one bounded source/docs-read-only AUDIT task as task 173 - a "Post-keybinding-remap-api Phase 9 next-task source audit" mirroring the task-171 audit block: confirm the in-memory remap API stayed confined to editor-ui menus, default resolve still has 19 executable accelerators and zero conflicts, remap/unbind/conflict/unknown-target diagnostics are covered, and no host/shell, persistence, settings UI, plugin runtime, CAD, Cargo, workflow, schema, or automation surface changed. Task 173 is docs/source-read-only (its `MAY edit` includes `.ai/dispatch.tasks.md`, `Status.md`, `HANDOFF.md`, `plans/BASELINE.md`, `change.md`; it MUST NOT edit Rust source, tests, or automation). Task 173's final step appends the next bounded FEATURE task (or, if none is in-policy, records a single `NEEDS_HUMAN_RECORDED: <ISO-date> - <reason>` marker plus a "Recommendation for human approval" block). Copy this Self-re-arm requirement verbatim into the task 173 block you author. Edit `.ai/dispatch.tasks.md` to do this.

### Task 173 audit outcome

2026-06-29: Source-read-only audit completed for the task 172 in-memory
keybinding remap API.

- `crates/editor-ui/src/menus/keybinding.rs` is an in-memory data-only module
  for `KeybindingTarget`, `KeybindingOverride`, `KeybindingOverrides`, and
  `KeybindingDiagnostic`; inverse searches found only boundary comments for
  settings/persistence/host/shell terms, not implementation wiring.
- `MenuRegistry::resolve(&PredicateContext)` remains the default resolver path
  and delegates to
  `resolve_with_keybinding_overrides(..., &KeybindingOverrides::default())`.
  Overrides are applied to cloned resolved entries for the current resolve only.
- The default menu remains pinned by tests at 19 executable accelerators and
  zero conflicts; prohibited-surface diffs for default menu, command,
  predicate, shortcut, host/shell/runtime, plugin runtime, CAD, Cargo,
  workflow, schema, and automation paths were empty.
- Editor-ui tests cover empty overrides, resolve-scoped remap, unbind while
  preserving the visible entry, non-fatal conflict data with execution
  suppression, unknown-target diagnostics, and known-hidden target behavior.
- Verification passed:
  `cargo test -p rge-editor-ui`,
  `cargo test -p rge-editor-ui --test menus_ordering`,
  `cargo check -p rge-editor-ui -p rge-editor-egui-host -p rge-editor-shell`,
  `cargo run -q -p rge-tool-architecture-lints -- all`,
  `cargo +nightly fmt --all -- --check`, and `git diff --check`.

### Task 173 gate resolved

2026-06-29: The operator approved the next Phase 9 feature surface — extend the
editor-ui keybinding-override diagnostics with non-fatal no-op / redundant
signals. Filed as task 174 below; the prior NEEDS_HUMAN pause is cleared. The
deferred alternatives (reverse effective-binding accessors; last-wins query
helpers on `KeybindingOverrides`) remain unscheduled API polish, not task 174.

174. **No-op and redundant keybinding-override diagnostics (editor-ui-only).**

   Extend the in-memory keybinding-override data plane (tasks 169-173) so a
   future remap UI can tell "already bound to this shortcut" from "nothing to
   unbind". Today `apply_keybinding_overrides` (`crates/editor-ui/src/menus/registry.rs`)
   assigns the override shortcut UNCONDITIONALLY with no comparison, so a remap
   to the shortcut a target already has, and an unbind of a target that is
   already unbound, both succeed SILENTLY with no diagnostic. Add two new
   non-fatal `KeybindingDiagnostic` variants that surface exactly those two
   no-effect cases, changing no resolve behavior.

   **Scope guard (operator decision - non-negotiable):**
   - MAY edit only:
     - `crates/editor-ui/src/menus/keybinding.rs`
     - `crates/editor-ui/src/menus/registry.rs`
     - `crates/editor-ui/tests/menus_ordering.rs`
     - `.ai/dispatch.tasks.md`
     - the current dispatch handoff/sidecar artifacts for this task.
   - MUST NOT edit Rust source, tests, Cargo files, schemas, workflows, scripts,
     automation, packet templates, generated non-current-dispatch artifacts, or
     any other crate. Specifically MUST NOT change
     `crates/editor-ui/src/menus/mod.rs` (no new exported symbol beyond the two
     enum variants, which ride the existing `KeybindingDiagnostic` re-export),
     `default_menu.rs`, `command.rs`, `predicate.rs`, `shortcut.rs`,
     `editor-shell`, `editor-egui-host`, `editor-actions`, `editor-state`,
     `plugin-host`, `runtime-wasmtime`, the CAD crates, `Cargo.toml`,
     `Cargo.lock`, `.github/workflows`, `.ai/*.schema.json`, or
     `.ai/dispatch.verify.ps1`.

   **Required implementation:**
   - Add two non-fatal variants to `KeybindingDiagnostic`
     (`crates/editor-ui/src/menus/keybinding.rs:137`), keeping the existing
     `#[derive(Debug, Clone, PartialEq, Eq)]` and the "diagnostics are data,
     never an error" contract:
     - `NoOpRemap { target: KeybindingTarget, shortcut: Shortcut }` — a remap
       override whose requested shortcut equals the shortcut the matched entry
       already has.
     - `RedundantUnbind { target: KeybindingTarget }` — an unbind override whose
       matched entry already has no shortcut (`None`).
   - Emit the new diagnostics from where the current shortcut is known —
     `apply_keybinding_overrides` (`crates/editor-ui/src/menus/registry.rs:380-399`):
     compare `resolved_entry.entry.shortcut` to `keybinding_override.shortcut`
     BEFORE assigning; when the override targets a RESOLVED (visible, registered)
     entry and the value is unchanged, record the matching diagnostic instead of
     a no-op write. Surface them into `ResolveResult.keybinding_diagnostics`
     alongside the existing `UnknownTarget` diagnostics, preserving deterministic
     insertion order (override order × point declaration order). Emit at most one
     diagnostic per override.
   - Boundary: the new diagnostics apply ONLY to overrides that match a resolved
     entry. An override against an unregistered target stays `UnknownTarget`
     (unchanged); an override against a registered-but-hidden / predicate-filtered
     target is neither applied nor flagged (do NOT reach into non-visible entries
     — that would touch visibility logic and is out of scope).
   - Semantics unchanged: resolve still succeeds; conflict detection and
     conflict-suppression behavior are unchanged; the shipped default menu is
     unchanged (still exactly 19 executable accelerators, zero conflicts); a real
     value-CHANGING remap or unbind must NOT emit the new diagnostics; existing
     `UnknownTarget` detection and behavior are unchanged. No serde, no disk,
     settings, environment, host/shell, plugin runtime, CAD, or Cargo surface.

   **Required checks:**
   - `rg -n "KeybindingDiagnostic|NoOpRemap|RedundantUnbind|UnknownTarget" crates`
     EXPECTING matches only in `crates/editor-ui/src/menus/{keybinding.rs,registry.rs}`
     and `crates/editor-ui/tests/menus_ordering.rs` (no host/shell/egui-host/
     plugin/CAD consumer).
   - `rg -n "host|shell|settings|persist|plugin|cad|Cargo|serde|workflow|schema|dispatch.verify" crates/editor-ui/src/menus/keybinding.rs crates/editor-ui/src/menus/registry.rs`
     EXPECTING matches only in comments/boundary notes, not new implementation
     wiring.

   **Verification:**
   - `git diff -- crates/editor-ui/src/menus/keybinding.rs crates/editor-ui/src/menus/registry.rs crates/editor-ui/tests/menus_ordering.rs .ai/dispatch.tasks.md`
   - `git diff -- crates/editor-ui/src/menus/mod.rs crates/editor-ui/src/menus/default_menu.rs crates/editor-ui/src/menus/command.rs crates/editor-ui/src/menus/predicate.rs crates/editor-ui/src/menus/shortcut.rs crates/editor-shell crates/editor-egui-host crates/editor-actions crates/editor-state crates/plugin-host crates/runtime-wasmtime crates/cad-core crates/cad-graph crates/cad-projection Cargo.toml Cargo.lock .github/workflows .ai/*.schema.json .ai/dispatch.verify.ps1`
     EXPECTING no changes.
   - `cargo test -p rge-editor-ui`
   - `cargo test -p rge-editor-ui --test menus_ordering`
   - `cargo check -p rge-editor-ui -p rge-editor-egui-host -p rge-editor-shell`
   - `cargo run -q -p rge-tool-architecture-lints -- all`
   - `cargo +nightly fmt --all -- --check`
   - `git diff --check`
   - `rg -n "^172\.|^173\.|^174\.|^175\.|NEEDS_HUMAN_RECORDED" .ai/dispatch.tasks.md`
     EXPECTING exactly one task 174 and exactly one task 175 (the audit appended
     by this task's self-re-arm); no completed current `NEEDS_HUMAN_RECORDED`
     marker.

   **Done criteria:**
   - `NoOpRemap` and `RedundantUnbind` are emitted in exactly the no-effect cases
     and are covered by tests; a real value-changing remap and a real unbind do
     NOT emit them; `UnknownTarget` behavior is unchanged.
   - The default menu remains pinned at 19 executable accelerators and zero
     conflicts; no prohibited surface changed.
   - Tests in `menus_ordering.rs` cover: no-op remap, redundant unbind, a real
     remap (no new diagnostic), a real unbind (no new diagnostic), and that the
     existing unknown-target path is unaffected.

   **Self-re-arm (final step, required):**
   After implementation and verification, APPEND exactly one bounded
   source/docs-read-only AUDIT task as task 175 - a "Post-no-op-diagnostics
   Phase 9 next-task source audit" mirroring the task-173 audit block: confirm
   the new `NoOpRemap` / `RedundantUnbind` diagnostics stayed in-memory in
   editor-ui, default resolve still has 19 executable accelerators and zero
   conflicts, the no-op / redundant / real-remap / real-unbind / unknown-target
   cases are covered, and no host/shell, persistence, settings UI, plugin
   runtime, CAD, Cargo, workflow, schema, or automation surface changed. Task 175
   is docs/source-read-only (its `MAY edit` includes `.ai/dispatch.tasks.md`,
   `Status.md`, `HANDOFF.md`, `plans/BASELINE.md`, `change.md`; it MUST NOT edit
   Rust source, tests, or automation). Task 175's final step appends the next
   bounded FEATURE task (or, if none is in-policy, records a single
   `NEEDS_HUMAN_RECORDED: <ISO-date> - <reason>` marker plus a "Recommendation for
   human approval" block). Copy this Self-re-arm requirement verbatim into the
   task 175 block you author. Edit `.ai/dispatch.tasks.md` to do this.

175. **Post-no-op-diagnostics Phase 9 next-task source audit
   (docs/source-read-only).**

   Audit the completed no-op/redundant keybinding override diagnostics dispatch.
   Confirm the new `NoOpRemap` / `RedundantUnbind` diagnostics stayed in-memory
   in `editor-ui`, default resolve still has exactly 19 executable
   accelerators and zero conflicts, the no-op / redundant / real-remap /
   real-unbind / unknown-target cases are covered, and no host/shell,
   persistence, settings UI, plugin runtime, CAD, Cargo, workflow, schema, or
   automation surface changed.

   **Scope guard (operator decision - non-negotiable):**
   - MAY edit only:
     - `.ai/dispatch.tasks.md`
     - `Status.md`
     - `HANDOFF.md`
     - `plans/BASELINE.md`
     - `change.md`
   - Audit/read source and docs as needed, including the current dispatch
     packets and local source reads for source evidence.
   - MUST NOT edit Rust source, tests, Cargo files, schemas, workflows, scripts,
     automation, packet templates, generated non-current-dispatch artifacts,
     editor-shell, editor-egui-host, editor-actions, editor-state, plugin-host,
     runtime-wasmtime, CAD crates, or any host/shell/runtime integration surface.
   - MUST NOT append task 176 except as the final-step next bounded FEATURE task
     if it is in-policy; otherwise record exactly one
     `NEEDS_HUMAN_RECORDED: <ISO-date> - <reason>` marker plus a
     "Recommendation for human approval" block.

   **Required audit checks:**
   - Confirm `KeybindingDiagnostic` contains only non-fatal resolve-time data
     variants for `UnknownTarget`, `NoOpRemap`, and `RedundantUnbind`, with no
     settings, persistence, serde, host/shell, plugin runtime, CAD, Cargo,
     workflow, schema, or automation wiring.
   - Confirm `MenuRegistry::resolve(&PredicateContext)` remains the default
     resolver path and delegates to
     `resolve_with_keybinding_overrides(..., &KeybindingOverrides::default())`.
   - Confirm `NoOpRemap` is emitted only when a remap override matches a
     resolved visible entry whose executable shortcut already equals the
     requested shortcut.
   - Confirm `RedundantUnbind` is emitted only when an unbind override matches a
     resolved visible entry whose executable shortcut is already `None`.
   - Confirm real value-changing remaps and unbinds still apply without the new
     diagnostics, unknown targets remain `UnknownTarget`, and known hidden /
     predicate-filtered targets remain silent.
   - Confirm conflict behavior and execution suppression are unchanged, and the
     shipped default menu is unchanged: exactly 19 executable accelerators, zero
     conflicts, and no changed entry ids, command ids, labels, order,
     predicates, enablement predicates, shortcut values, or passive shortcut
     hints.
   - Confirm task 174 changed only the allowed implementation/test/task-brief
     surfaces plus the current dispatch handoff/sidecar artifacts.

   **Verification:**
   - `git diff -- crates/editor-ui/src/menus/keybinding.rs crates/editor-ui/src/menus/registry.rs crates/editor-ui/tests/menus_ordering.rs .ai/dispatch.tasks.md`
   - `git diff -- crates/editor-ui/src/menus/mod.rs crates/editor-ui/src/menus/default_menu.rs crates/editor-ui/src/menus/command.rs crates/editor-ui/src/menus/predicate.rs crates/editor-ui/src/menus/shortcut.rs crates/editor-shell crates/editor-egui-host crates/editor-actions crates/editor-state crates/plugin-host crates/runtime-wasmtime crates/cad-core crates/cad-graph crates/cad-projection Cargo.toml Cargo.lock .github/workflows .ai/*.schema.json .ai/dispatch.verify.ps1`
     EXPECTING no changes.
   - `rg -n "KeybindingDiagnostic|NoOpRemap|RedundantUnbind|UnknownTarget|keybinding_diagnostics|resolve_with_keybinding_overrides" crates/editor-ui/src/menus crates/editor-ui/tests/menus_ordering.rs`
   - `rg -n "host|shell|settings|persist|plugin|cad|Cargo|serde|workflow|schema|dispatch\\.verify" crates/editor-ui/src/menus/keybinding.rs crates/editor-ui/src/menus/registry.rs crates/editor-ui/tests/menus_ordering.rs`
     EXPECTING matches only in comments/test descriptions that preserve the
     no-host/no-persistence/no-Cargo boundary, not implementation wiring.
   - `cargo test -p rge-editor-ui`
   - `cargo test -p rge-editor-ui --test menus_ordering`
   - `cargo check -p rge-editor-ui -p rge-editor-egui-host -p rge-editor-shell`
   - `cargo run -q -p rge-tool-architecture-lints -- all`
   - `cargo +nightly fmt --all -- --check`
   - `git diff --check`
   - `rg -n "^172\\.|^173\\.|^174\\.|^175\\.|^176\\.|NEEDS_HUMAN_RECORDED" .ai/dispatch.tasks.md`
     EXPECTING exactly one task 174 and exactly one task 175; no task 176 unless
     it is the final bounded FEATURE task appended by this audit, and no
     completed current `NEEDS_HUMAN_RECORDED` marker.

   **Done criteria:**
   - Audit findings are source-grounded and no Rust source/test changes are made
     by this audit task.
   - The task records whether a next bounded FEATURE task is in-policy; if not,
     exactly one `NEEDS_HUMAN_RECORDED: <ISO-date> - <reason>` marker plus a
     "Recommendation for human approval" block is recorded.
   - No task 176 is appended unless it is the final bounded FEATURE task
     permitted by this task's final step.

   **Carried self-rearm instruction from task 174 (verbatim):**

   **Self-re-arm (final step, required):**
   After implementation and verification, APPEND exactly one bounded
   source/docs-read-only AUDIT task as task 175 - a "Post-no-op-diagnostics
   Phase 9 next-task source audit" mirroring the task-173 audit block: confirm
   the new `NoOpRemap` / `RedundantUnbind` diagnostics stayed in-memory in
   editor-ui, default resolve still has 19 executable accelerators and zero
   conflicts, the no-op / redundant / real-remap / real-unbind / unknown-target
   cases are covered, and no host/shell, persistence, settings UI, plugin
   runtime, CAD, Cargo, workflow, schema, or automation surface changed. Task 175
   is docs/source-read-only (its `MAY edit` includes `.ai/dispatch.tasks.md`,
   `Status.md`, `HANDOFF.md`, `plans/BASELINE.md`, `change.md`; it MUST NOT edit
   Rust source, tests, or automation). Task 175's final step appends the next
   bounded FEATURE task (or, if none is in-policy, records a single
   `NEEDS_HUMAN_RECORDED: <ISO-date> - <reason>` marker plus a "Recommendation for
   human approval" block). Copy this Self-re-arm requirement verbatim into the
   task 175 block you author. Edit `.ai/dispatch.tasks.md` to do this.

### Task 175 resolved

2026-06-30: The formal task-175 audit dispatch (ISSUE-16) was abandoned. Its
canonical verify gate went red on two NEW supply-chain advisories unrelated to
the audit (RUSTSEC-2026-0190 anyhow 1.0.102 UB; RUSTSEC-2026-0192 ttf-parser
unmaintained), which an audit-only task cannot remediate. The gate was fixed
out-of-band on main at `d577aaf` (anyhow 1.0.103 + a dated `deny.toml` ignore),
and task 174's no-op/redundant diagnostics were independently confirmed sound by
the cross-AI guard review of PR #14: in-memory `editor-ui` only; default resolve
still has exactly 19 executable accelerators and zero conflicts;
`NoOpRemap`/`RedundantUnbind` are emitted only for no-effect overrides matching a
resolved visible entry; real remaps/unbinds and `UnknownTarget` are unchanged; no
prohibited surface changed. Task 175 is therefore closed as guard-verified; the
operator approved the next feature surface (task 176) below. No live NEEDS_HUMAN
marker.

176. **Reverse effective-binding accessors on `ResolveResult` (editor-ui-only).**

   Add read-only inverse-lookup accessors to `ResolveResult`
   (`crates/editor-ui/src/menus/registry.rs`) so a future remap UI / keymap view
   can ask "what shortcut is this command bound to?" after overrides. Today only
   the forward direction exists - `command_for_shortcut(&Shortcut) -> Option<&Command>`
   (registry.rs:348) - and the backing `shortcut_commands` map (registry.rs:317)
   is private, so there is no command->shortcut lookup and no way to enumerate the
   resolved bindings. Expose the existing data the other direction WITHOUT changing
   any resolve behavior.

   **Scope guard (operator decision - non-negotiable):**
   - MAY edit only:
     - `crates/editor-ui/src/menus/registry.rs`
     - `crates/editor-ui/tests/menus_ordering.rs`
     - `.ai/dispatch.tasks.md`
     - the current dispatch handoff/sidecar artifacts for this task.
   - MUST NOT edit Rust source, tests, Cargo files, schemas, workflows, scripts,
     automation, packet templates, generated non-current-dispatch artifacts, or
     any other crate. Specifically MUST NOT change
     `crates/editor-ui/src/menus/keybinding.rs` (no enum/type change needed),
     `crates/editor-ui/src/menus/mod.rs` (no new exported symbol - the accessors
     are methods on the already-exported `ResolveResult`), `default_menu.rs`,
     `command.rs`, `predicate.rs`, `shortcut.rs`, `editor-shell`,
     `editor-egui-host`, `editor-actions`, `editor-state`, `plugin-host`,
     `runtime-wasmtime`, the CAD crates, `Cargo.toml`, `Cargo.lock`,
     `.github/workflows`, `.ai/*.schema.json`, or `.ai/dispatch.verify.ps1`.

   **Required implementation:**
   - Add to `impl ResolveResult` (registry.rs) two read-only accessors backed by
     the EXISTING private `shortcut_commands` field (do NOT add a new field or
     second index, and do NOT change `resolve` / override application):
     - `shortcut_for_command(&self, command: &Command) -> Option<&Shortcut>` - the
       inverse of `command_for_shortcut`. A binding/display lookup (mirror
       `command_for_shortcut`'s contract: ignores enablement/conflict state).
     - `bindings(&self) -> impl Iterator<Item = (&Shortcut, &Command)>` -
       enumerate the resolved shortcut->command pairs.
   - **Determinism is mandatory.** `shortcut_commands` is a `HashMap`, whose
     iteration order is non-deterministic; both accessors MUST expose a STABLE,
     documented order and MUST NOT leak raw `HashMap` iteration order. When a
     `Command` is bound to more than one shortcut (many shortcuts -> one command
     is possible), `shortcut_for_command` MUST return a deterministically chosen
     shortcut and document the rule, consistent with the first-registered-winner
     resolve semantics documented at registry.rs:308-317. `bindings()` MUST yield
     pairs in that same stable order. (A documented sort - e.g. by `Shortcut` -
     is acceptable if resolve/registration order is not readily available.)
   - No new exported symbol beyond the methods on `ResolveResult`
     (`mod.rs` re-export unchanged); no serde, disk, settings, host/shell, plugin
     runtime, CAD, or Cargo surface.
   - Semantics unchanged: no change to `resolve`, conflict detection/suppression,
     the accelerator table, `keybinding_diagnostics`, or the shipped default menu
     (still exactly 19 executable accelerators, zero conflicts).

   **Required checks:**
   - `rg -n "shortcut_for_command|fn bindings|command_for_shortcut|shortcut_commands" crates`
     EXPECTING matches only in `crates/editor-ui/src/menus/registry.rs` and
     `crates/editor-ui/tests/menus_ordering.rs`.
   - `rg -n "host|shell|settings|persist|plugin|cad|Cargo|serde|workflow|schema|dispatch\\.verify" crates/editor-ui/src/menus/registry.rs`
     EXPECTING matches only in comments/boundary notes, not new implementation
     wiring.

   **Verification:**
   - `git diff -- crates/editor-ui/src/menus/registry.rs crates/editor-ui/tests/menus_ordering.rs .ai/dispatch.tasks.md`
   - `git diff -- crates/editor-ui/src/menus/keybinding.rs crates/editor-ui/src/menus/mod.rs crates/editor-ui/src/menus/default_menu.rs crates/editor-ui/src/menus/command.rs crates/editor-ui/src/menus/predicate.rs crates/editor-ui/src/menus/shortcut.rs crates/editor-shell crates/editor-egui-host crates/editor-actions crates/editor-state crates/plugin-host crates/runtime-wasmtime crates/cad-core crates/cad-graph crates/cad-projection Cargo.toml Cargo.lock .github/workflows .ai/*.schema.json .ai/dispatch.verify.ps1`
     EXPECTING no changes.
   - `cargo test -p rge-editor-ui`
   - `cargo test -p rge-editor-ui --test menus_ordering`
   - `cargo check -p rge-editor-ui -p rge-editor-egui-host -p rge-editor-shell`
   - `cargo run -q -p rge-tool-architecture-lints -- all`
   - `cargo +nightly fmt --all -- --check`
   - `git diff --check`
   - `rg -n "^174\\.|^175\\.|^176\\.|^177\\.|NEEDS_HUMAN_RECORDED" .ai/dispatch.tasks.md`
     EXPECTING exactly one task 176 and exactly one task 177 (the audit appended
     by this task's self-re-arm); no completed current `NEEDS_HUMAN_RECORDED`
     marker.

   **Done criteria:**
   - `shortcut_for_command` and `bindings()` are present, read-only over the
     existing `shortcut_commands` map, and deterministic (no raw `HashMap` order
     leaked); the many-shortcuts-per-command rule is documented.
   - Tests in `menus_ordering.rs` cover: a bound command resolves to its shortcut;
     an unbound command returns `None`; `bindings()` enumerates all resolved pairs
     in the documented stable order; the accessors reflect override results (a
     remapped command returns its NEW shortcut, an unbound command returns `None`);
     and the result is deterministic across repeated resolves.
   - The default menu remains pinned at 19 executable accelerators and zero
     conflicts; no prohibited surface changed; `mod.rs` export unchanged.

   **Self-re-arm (final step, required):**
   After implementation and verification, APPEND exactly one bounded
   source/docs-read-only AUDIT task as task 177 - a "Post-reverse-accessors
   Phase 9 next-task source audit" mirroring the task-175 audit block: confirm the
   new `shortcut_for_command` / `bindings()` accessors are read-only over the
   existing `shortcut_commands` map (no new field, no resolve-behavior change),
   deterministic (no raw `HashMap` order leaked, many-shortcuts rule documented),
   default resolve still has 19 executable accelerators and zero conflicts, the
   bound / unbound / override-reflected / determinism cases are covered, and no
   host/shell, persistence, settings UI, plugin runtime, CAD, Cargo, workflow,
   schema, or automation surface changed. Task 177 is docs/source-read-only (its
   `MAY edit` includes `.ai/dispatch.tasks.md`, `Status.md`, `HANDOFF.md`,
   `plans/BASELINE.md`, `change.md`; it MUST NOT edit Rust source, tests, or
   automation). Task 177's final step appends the next bounded FEATURE task (or, if
   none is in-policy, records a single `NEEDS_HUMAN_RECORDED: <ISO-date> - <reason>`
   marker plus a "Recommendation for human approval" block). Copy this Self-re-arm
   requirement verbatim into the task 177 block you author. Edit
   `.ai/dispatch.tasks.md` to do this.

177. **Post-reverse-accessors Phase 9 next-task source audit
   (docs/source-read-only).**

   Audit the completed reverse effective-binding accessors dispatch. Confirm the
   new `shortcut_for_command` / `bindings()` accessors are read-only over the
   existing `shortcut_commands` map, add no new field or secondary index, do not
   change resolve behavior, expose deterministic ordering without leaking raw
   `HashMap` order, document the many-shortcuts-to-one-command rule, preserve the
   default 19 executable accelerators and zero conflicts, cover bound / unbound /
   override-reflected / deterministic cases, and leave host/shell, persistence,
   settings UI, plugin runtime, CAD, Cargo, workflow, schema, and automation
   surfaces unchanged.

   **Scope guard (operator decision - non-negotiable):**
   - MAY edit only:
     - `.ai/dispatch.tasks.md`
     - `Status.md`
     - `HANDOFF.md`
     - `plans/BASELINE.md`
     - `change.md`
   - Audit/read source and docs as needed, including the current dispatch
     packets and local source reads for source evidence.
   - MUST NOT edit Rust source, tests, Cargo files, schemas, workflows, scripts,
     automation, packet templates, generated non-current-dispatch artifacts,
     editor-shell, editor-egui-host, editor-actions, editor-state, plugin-host,
     runtime-wasmtime, CAD crates, or any host/shell/runtime integration surface.
   - MUST NOT append task 178 except as the final-step next bounded FEATURE task
     if it is in-policy; otherwise record exactly one
     `NEEDS_HUMAN_RECORDED: <ISO-date> - <reason>` marker plus a
     "Recommendation for human approval" block.

   **Required audit checks:**
   - Confirm `ResolveResult::shortcut_for_command` and `ResolveResult::bindings`
     are methods on the already-exported `ResolveResult` and no `mod.rs`,
     public type, enum, field, serde, settings, persistence, host/shell, plugin
     runtime, CAD, Cargo, workflow, schema, or automation surface changed.
   - Confirm both accessors read only the existing private `shortcut_commands`
     map at accessor call time and no new field, secondary index, cached vector,
     global state, resolve-time side table, or resolve behavior change was added.
   - Confirm `bindings()` sorts the resolved shortcut-command pairs in a
     documented stable order and does not expose raw `HashMap` iteration order.
   - Confirm `shortcut_for_command(&Command)` uses the same stable order as
     `bindings()` and documents which shortcut wins when multiple shortcuts
     resolve to the same command.
   - Confirm `resolve`, `resolve_with_keybinding_overrides`, override
     application, conflict detection/suppression, `keybinding_diagnostics`,
     `AcceleratorTable`, default menu entries, labels, order, predicates,
     enablement predicates, shortcut values, and passive shortcut hints are
     unchanged.
   - Confirm tests cover a bound command, an unbound command, full stable
     enumeration, remap and unbind override reflection, repeated-resolve
     determinism, and the default 19 accelerators / zero conflicts invariant.
   - Confirm task 176 changed only the allowed implementation/test/task-brief
     surfaces plus current dispatch handoff/sidecar artifacts.

   **Verification:**
   - `git diff -- crates/editor-ui/src/menus/registry.rs crates/editor-ui/tests/menus_ordering.rs .ai/dispatch.tasks.md`
   - `git diff -- crates/editor-ui/src/menus/keybinding.rs crates/editor-ui/src/menus/mod.rs crates/editor-ui/src/menus/default_menu.rs crates/editor-ui/src/menus/command.rs crates/editor-ui/src/menus/predicate.rs crates/editor-ui/src/menus/shortcut.rs crates/editor-shell crates/editor-egui-host crates/editor-actions crates/editor-state crates/plugin-host crates/runtime-wasmtime crates/cad-core crates/cad-graph crates/cad-projection Cargo.toml Cargo.lock .github/workflows .ai/*.schema.json .ai/dispatch.verify.ps1 Invoke-AiDispatchLoop.ps1 Invoke-AiDispatchQueue.ps1 Invoke-AiDispatchAuto.ps1 Register-AiDispatchSchedule.ps1 Get-AiDispatchHealth.ps1 Wait-GitHubActions.ps1 Watch-AiDispatch.ps1 new-handoff.ps1`
     EXPECTING no changes.
   - `rg -n "shortcut_for_command|fn bindings|\\.bindings\\(|shortcut_commands" crates`
     EXPECTING matches only in `crates/editor-ui/src/menus/registry.rs` and
     `crates/editor-ui/tests/menus_ordering.rs`.
   - `rg -n "host|shell|settings|persist|plugin|cad|Cargo|serde|workflow|schema|dispatch\\.verify" crates/editor-ui/src/menus/registry.rs crates/editor-ui/tests/menus_ordering.rs`
     EXPECTING matches only in comments/test descriptions that preserve the
     no-host/no-persistence/no-Cargo boundary, not implementation wiring.
   - `cargo test -p rge-editor-ui`
   - `cargo test -p rge-editor-ui --test menus_ordering`
   - `cargo check -p rge-editor-ui -p rge-editor-egui-host -p rge-editor-shell`
   - `cargo run -q -p rge-tool-architecture-lints -- all`
   - `cargo +nightly fmt --all -- --check`
   - `git diff --check`
   - `rg -n "^174\\.|^175\\.|^176\\.|^177\\.|^178\\.|NEEDS_HUMAN_RECORDED" .ai/dispatch.tasks.md`
     EXPECTING exactly one task 176 and exactly one task 177; no task 178
     unless it is the final bounded FEATURE task appended by this audit, and no
     completed current `NEEDS_HUMAN_RECORDED` marker.

   **Done criteria:**
   - Audit findings are source-grounded and no Rust source/test changes are made
     by this audit task.
   - The task records whether a next bounded FEATURE task is in-policy; if not,
     exactly one `NEEDS_HUMAN_RECORDED: <ISO-date> - <reason>` marker plus a
     "Recommendation for human approval" block is recorded.
   - No task 178 is appended unless it is the final bounded FEATURE task
     permitted by this task's final step.

   **Carried self-rearm instruction from task 176 (verbatim):**

   **Self-re-arm (final step, required):**
   After implementation and verification, APPEND exactly one bounded
   source/docs-read-only AUDIT task as task 177 - a "Post-reverse-accessors
   Phase 9 next-task source audit" mirroring the task-175 audit block: confirm the
   new `shortcut_for_command` / `bindings()` accessors are read-only over the
   existing `shortcut_commands` map (no new field, no resolve-behavior change),
   deterministic (no raw `HashMap` order leaked, many-shortcuts rule documented),
   default resolve still has 19 executable accelerators and zero conflicts, the
   bound / unbound / override-reflected / determinism cases are covered, and no
   host/shell, persistence, settings UI, plugin runtime, CAD, Cargo, workflow,
   schema, or automation surface changed. Task 177 is docs/source-read-only (its
   `MAY edit` includes `.ai/dispatch.tasks.md`, `Status.md`, `HANDOFF.md`,
   `plans/BASELINE.md`, `change.md`; it MUST NOT edit Rust source, tests, or
   automation). Task 177's final step appends the next bounded FEATURE task (or, if
   none is in-policy, records a single `NEEDS_HUMAN_RECORDED: <ISO-date> - <reason>`
   marker plus a "Recommendation for human approval" block). Copy this Self-re-arm
   requirement verbatim into the task 177 block you author. Edit
   `.ai/dispatch.tasks.md` to do this.
