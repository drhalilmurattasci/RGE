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
  after a task fails its run *and* its one automatic retry — and also once
  `-MaxAutonomousTasks` tasks exist. Both need a human to clear/raise before
  it resumes.
- In `branch` publish mode, finished work waits on an `ai-dispatch/ISSUE-*`
  branch for you to merge. In `main` mode it auto-publishes to `origin/main`.
- Keep tasks bounded. The autonomous loop will plan, execute, verify, and
  (depending on mode) publish whatever is selected here.

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

1. **Add automatic `--glb` file watching on top of the R-key reload path.**
   Use the workspace `notify = "8"` dep (debounce ~200ms, watch the parent
   directory non-recursively per the `editor-ui::layout::hot_reload`
   precedent). Drain events at `RedrawRequested`-time and call
   `EditorShell::handle_asset_reload` for modify events targeting the
   `glb_source_path` file. Loader stays in `rge-editor`; no
   `editor-shell -> io-gltf` edge. No asset-store integration, no kernel
   cavity fill, no new crate — fits inside `rge-editor` (binary) plus a
   tiny `editor-shell` watcher hook field if needed.

   **Verbatim review-gate strings** — the autonomous selector MUST
   copy these two strings, character-for-character, into the filed
   GitHub issue body. No paraphrasing, no substitution of
   `EditorShell::handle_asset_reload` for `handle_asset_reload`, no
   reflowing into different sentence shapes. The strings are the
   human review gate; a packet that lacks either string verbatim is
   bounced at review without further reading:

   ```
   MUST call `handle_asset_reload`
   MUST NOT mutate render assets directly
   ```

   **Done-criterion**: Automatic reload is only a producer of reload
   requests. The watcher MUST call `handle_asset_reload` for all reload
   semantics: Editing-state gate, failure retention, atomic
   render-asset swap, and warn-log. The watcher MUST NOT mutate render
   assets directly or duplicate reload logic. Watcher responsibilities
   are limited to: observe filesystem events, debounce, filter to
   `glb_source_path`, and enqueue/trigger the existing handler.
   Failure-mode test: malformed-bytes write -> warn-logged Err,
   previous frame retained, and watcher remains live for the next
   valid write.

2. **Add a smooth-normal glTF fixture + extend visual acceptance for M3.**
   New io-gltf test fixture where the `NORMAL` accessor encodes per-vertex
   smooth normals (e.g. UV-sphere or rounded cube) that differ
   meaningfully from `from_buffers`'s flat-recompute output. Add ONE
   pixel/readback test in `editor-shell::visual_smoke` or
   `rge-editor::tests` that renders the fixture once with imported
   normals (M3 path) and once with `None` (flat recompute), and asserts
   the central-row pixel distribution differs by more than driver noise.
   Closes the M3 visual evidence gap that logs alone don't certify.

   **Fixture target**: the fixture MUST be committed at the exact
   path `crates/io-gltf/tests/fixtures/smooth_normal_sphere.glb`.
   The selector MUST cite this exact path — including the leaf
   filename `smooth_normal_sphere.glb` — in the filed issue body.
   No leaf substitution: not `smooth_normal_cube.glb`, not
   `smooth_normal_quad.glb`, not any other name. The path is part
   of the review gate, not an executor choice. The fixture MUST use
   smooth per-vertex normals that differ from triangle face
   normals; a planar quad is not acceptable, because every vertex's
   smooth-averaged normal would equal the face normal and the M3
   visible-difference threshold assertion would fail by
   construction. A UV-sphere is the canonical shape — imported
   vertex normals approximate radial smooth normals, while
   `None`/flat recompute forces per-triangle face normals, giving
   the largest signal for the central-row pixel-distribution
   delta.

   **Verbatim review-gate strings** — the autonomous selector MUST
   copy these three strings, character-for-character, into the filed
   GitHub issue body. No paraphrasing, no substitution, no reflowing
   into different sentence shapes. The strings are the human review
   gate; a packet that lacks any one of them verbatim is bounced at
   review without further reading:

   ```
   MUST keep scope to fixture/test coverage only
   MUST NOT change shaders, render pipelines, material APIs, asset-store, or kernel crates
   MUST add a measurable pixel/readback assertion, not a visual-only claim
   ```

   **Done-criterion**: One new fixture under
   `crates/io-gltf/tests/fixtures/` whose imported `NORMAL` accessor
   produces shading meaningfully different from `from_buffers`'s
   flat-recompute output. One new pixel/readback test (in
   `editor-shell::visual_smoke` or `rge-editor::tests`) that renders
   the fixture twice — once with imported normals (M3 path), once
   with `None` (flat recompute) — and asserts a numeric central-row
   pixel-distribution threshold committed in the test source (e.g.
   mean per-channel delta or pixel-count delta above a named
   `const` threshold, not a prose claim). Scope strictly fixture +
   test only: no changes to shaders, render pipelines,
   `Material`/render API, `rge-asset-store`, or any `kernel/` crate.

3. **Add malformed-GLB reload regression coverage.**
   End-to-end test in `rge-editor::tests` parallel to
   `r_key_reload_on_missing_file_preserves_prior_frame`: start from a
   valid `cube.glb`, render frame 1, attach a hook pointing at a path
   whose CONTENT is malformed (truncated bytes, wrong magic, invalid
   JSON chunk), call `handle_asset_reload`, render frame 2. Assert frame
   2's pixel signature matches frame 1's within driver tolerance AND
   that the hook returned `Err` (verify via tracing capture or by
   exercising the hook directly first). Distinct from missing-file: this
   is the parser-failure path. No asset-store work, no new crate.

   **Malformed variant**: the test MUST use one of two concrete
   parser-failure shapes — (a) wrong-magic bytes such as
   `b"not a glb"` (first 4 bytes mismatch the glTF `b"glTF"`
   header), OR (b) a truncated GLB header (valid `b"glTF"` magic +
   version + a length field, but the file ends before any JSON
   chunk). The selector MUST cite one of these two variants in the
   filed issue body; "any malformed bytes" without specifying which
   kind is not acceptable — the executor needs to know which
   parser-failure path is being exercised.

   **Verbatim review-gate strings** — the autonomous selector MUST
   copy these two strings, character-for-character, into the filed
   GitHub issue body. No paraphrasing, no substitution, no reflowing
   into different sentence shapes. The strings are the human review
   gate; a packet that lacks either string verbatim is bounced at
   review without further reading:

   ```
   MUST retain the prior rendered frame after the malformed reload
   MUST follow the failed reload with a valid reload that proves the hook still works
   ```

   **Done-criterion**: One new test parallel to
   `r_key_reload_on_missing_file_preserves_prior_frame` that
   exercises a parser-failure malformed-GLB write. Assertions: (a)
   the post-malformed frame's pixel signature matches the
   pre-malformed frame's within the existing `CUBE_THRESHOLD`
   tolerance; (b) the hook returned `Err` for the malformed write
   (verified via tracing capture, direct hook exercise, or
   equivalent); (c) a subsequent VALID write through the same hook
   succeeds and produces a third-frame pixel signature that
   differs from the prior frames within the same tolerance band —
   proving the failed reload did not poison the watcher/hook path.
   Scope strictly test-only: no changes to
   `EditorShell::handle_asset_reload`, the watcher, or any
   production code.

4. **Read-only preflight: W16 `rge-asset-store` integration shape.**
   **NO source edits.** Audit how `io-gltf::cache_stub::MemoryCache` and
   `io-image`'s cache surface (if any) relate to the `asset-store::Cache`
   trait + `LocalCache`. Determine: (a) which io-* crates' caches should
   route through `rge-asset-store` rather than holding their own
   `MemoryCache`-style stub; (b) whether `kernel/asset-view` becomes a
   genuine consumer once io-gltf binds to `rge-asset-store`; (c) the
   smallest dispatch that swaps one io-* crate's cache to the asset-store
   `Cache` trait without churning the kernel cavity. Produces a 5-question
   answer block (per the existing preflight format) — no code, no tests.
   Caller decides next dispatch from that.

   **Scope-preserving halt clause** — the orchestrator's canonical
   verify gate (`.ai/dispatch.verify.ps1`) runs after Claude execute
   even on read-only audits. If verify fails on a target OUTSIDE the
   audit scope (anything beyond `crates/io-gltf/`, `crates/io-image/`,
   `crates/asset-store/`, `kernel/asset-view/`, or this dispatch's own
   `ai_handoffs/` packet), the orchestrator may auto-route a
   CORRECTION packet asking the executor to fix the failure. When that
   happens **the executor MUST halt**: write an EXECUTION_REPORT with
   `EXEC_STATUS: blocked` and `STATUS: NEEDS_HUMAN`, do NOT execute
   the correction. Read-only intent is the entire reason this task is
   in the brief; a correction-round source fix to an unrelated test
   bug expands a read-only audit into a source-fix dispatch and must
   become its own ticket. Precedent: ISSUE-91 (2026-05-22) was
   salvaged this way — the audit content landed but a
   correction-round GPU-test serialization fix was extracted to a
   separate infra commit on main rather than being accepted as part of
   the read-only dispatch.
