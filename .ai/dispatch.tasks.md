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

4. **[DONE 2026-05-22 via PR #93 / commit `df8ec26`] Read-only preflight: W16 `rge-asset-store` integration shape.**
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

   **Audit landed**: `ai_handoffs/ISSUE-92_EXEC_2026-05-22_16-52-05+0300.md`
   on `main` carries the 5-question answer block. Salvaged via #93 after
   the orchestrator's verify gate caught an unrelated `STATUS_ACCESS_VIOLATION`
   in `cargo test -p rge-gfx --lib` (now fixed on main in `a533b48`); the
   scope-preserving halt clause above correctly refused the auto-routed
   CORRECTION packet that would have expanded scope. Q5 of the audit
   specifies the smallest follow-up dispatch — an opt-in
   `crates/io-gltf/src/asset_store_cache.rs` adapter forwarding through
   `dyn rge_asset_store::Cache`. That follow-up belongs in a fresh task
   entry (5+) added below when ready to dispatch.

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

5. **Add opt-in `io-gltf` → `rge-asset-store` adapter (from #92 audit Q5).**
   The #92 audit
   (`ai_handoffs/ISSUE-92_EXEC_2026-05-22_16-52-05+0300.md`, Q5)
   identified the smallest follow-up dispatch as an opt-in adapter
   inside `rge-io-gltf` that implements the typed
   `crate::cache_stub::Cache` trait by forwarding through
   `dyn rge_asset_store::Cache`. The existing `MemoryCache` impl
   remains the default; callers opt into the asset-store-backed
   variant by name. No call site of `import_glb` / `export_glb` /
   `build_scene` changes — the trait surface they consume is
   unchanged. The migration comment at
   `crates/io-gltf/src/cache_stub.rs:11-12` ("this file is deleted
   and `crate::Cache` becomes a re-export") is aspirational; the
   audit found six concrete shape mismatches (typed-family vs
   byte-oriented, borrow vs owned, infallible vs `Result`, handle
   shape, serialization boundary, LRU/persistence semantics) that
   require an adapter, not a re-export.

   **Allowed file surface** (copied verbatim from #92 audit Q5):
   - NEW: `crates/io-gltf/src/asset_store_cache.rs` containing the
     adapter struct + `Cache` impl + unit tests.
   - EDIT: `crates/io-gltf/src/lib.rs` — module declaration + `pub
     use` for the adapter type only. No change to existing
     re-exports of `Cache` / `MemoryCache`.
   - EDIT: `crates/io-gltf/Cargo.toml` — add
     `rge-asset-store = { path = "../asset-store" }`. Serialization
     crate choice (RON via existing dev-dep at `Cargo.toml:33`, or
     `postcard` / `bincode` as a new dep) is the executor's call.
   - EDIT: `crates/io-gltf/src/cache_stub.rs` ONLY to soften the
     re-export comment at `:11-12` to reflect the audit's finding
     (adapter, not re-export). Comment-only — no API change.
   - OPTIONAL: new `GltfError::Cache(String)` variant in
     `crates/io-gltf/src/lib.rs:117-131` if the adapter needs to
     surface asset-store `CacheError`. Additive, non-breaking
     because `GltfError` is already `#[non_exhaustive]` (`:116`).

   **Files that MUST NOT be touched** (verbatim from audit Q5):
   - Anything under `kernel/**`. asset-view stays a vocabulary
     substrate; the audit's Q4 explicitly excludes asset-view from
     this dispatch's scope.
   - Anything under `editor/**` or `crates/editor-shell/**`. The
     editor's `MemoryCache::new()` call at
     `editor/rge-editor/src/main.rs:429` and the `AssetReloadHook`
     callback at
     `crates/editor-shell/src/lifecycle/asset_reload.rs:60-92` must
     remain bound to the existing `MemoryCache` — that is the
     *opt-in* discipline that keeps the swap narrow.
   - `crates/io-image/**`. The unused
     `crates/io-image/src/asset_store_stub.rs` is out of scope;
     deleting or migrating it is a separate dispatch.
   - `crates/asset-store/**`. The follow-up is an io-gltf-side
     adapter only — asset-store's trait and impls are untouched.
   - `crates/components-render/**`, `crates/components-animation/**`,
     `crates/cad-core/**`, any pak / data / brep-render crate.
   - All status / handoff / ADR / lint exemption / roadmap files.

   **Cargo.lock policy**: the single new dep edge in
   `crates/io-gltf/Cargo.toml` is permitted to add its
   corresponding lockfile entry. NO new packages or version
   changes beyond that single edge. Per-task carve-out matching
   the same allowance granted to task #1 for `notify` — the
   one-line dependency-edge update pattern.

   **Halt conditions** (verbatim from audit Q5):
   - Adapter requires changing `crate::Cache`'s public trait method
     signatures (`insert_*` / `get_*`) — promotes the scope to a
     workspace-wide breaking change.
   - Adapter requires editing `editor/**` (e.g. swapping the
     editor's default cache type) — out of scope; the editor stays
     on `MemoryCache` and the asset-store-backed variant is opt-in.
   - Adapter requires extending `kernel/asset-view` (e.g. new
     `ViewKind` variants, real `byte_len` semantics) — out of
     scope per Q4.
   - Cargo-lockfile churn beyond the new asset-store edge —
     investigate before proceeding; do not silently accept broader
     manifest drift.
   - `rge-io-image` needs changes to make its codec output
     cacheable — out of scope; that is the separate "make io-image
     a real asset-store consumer" follow-up.

   **Verbatim review-gate strings** — the autonomous selector MUST
   copy these five strings, character-for-character, into the filed
   GitHub issue body. No paraphrasing, no substitution, no
   reflowing into different sentence shapes. A packet that lacks
   any one of them verbatim is bounced at review without further
   reading. The five clauses together encode all five halt
   conditions from the #92 audit Q5 as mechanically enforceable
   constraints — covering the scope-fence (clauses 1, 2, 4),
   the trait-signature halt (clause 3), and the Cargo.lock-drift
   halt (clause 5):

   ```
   MUST be an opt-in adapter inside rge-io-gltf, not a trait re-export
   MUST leave the existing MemoryCache as the default for editor / tests / loaders
   MUST NOT change the public trait method signatures of crate::Cache (insert_* / get_*)
   MUST NOT modify kernel/**, editor/**, crates/editor-shell/**, crates/io-image/**, or crates/asset-store/**
   MUST halt if Cargo.lock changes extend beyond the single new asset-store dep edge
   ```

   **Done-criterion**: a new `crates/io-gltf/src/asset_store_cache.rs`
   adapter committed with unit tests asserting (a) round-trip
   equality through the adapter matches round-trip through
   `MemoryCache` for a synthetic triangle / cube scene; (b) blake3
   content-addressed dedup is preserved when serialising /
   deserialising typed assets through asset-store bytes; (c)
   asset-store I/O errors surface as the new `GltfError::Cache`
   variant rather than panic. Verification: `cargo build -p
   rge-io-gltf` + `cargo test -p rge-io-gltf --lib --no-fail-fast`
   + `cargo run -q -p rge-tool-architecture-lints -- all` +
   `.ai/dispatch.verify.ps1` all exit 0. Diff stat: only
   `crates/io-gltf/*` + Cargo.lock (the single dep edge). The
   default editor path remains on `MemoryCache` — the opt-in
   adapter is unreachable from the editor binary without explicit
   caller opt-in.
