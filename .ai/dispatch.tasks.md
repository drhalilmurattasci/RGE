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

5. **[DONE 2026-05-22 via PR #95 / commit `87ec3a6`] Add opt-in `io-gltf` → `rge-asset-store` adapter (from #92 audit Q5).**
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

   **Adapter landed**: `crates/io-gltf/src/asset_store_cache.rs`
   on `main` (`AssetStoreCache` struct + `impl Cache` +
   `try_insert_*` fallible family + per-family canonical byte
   encoders aligned with the existing `content_hash()` digests).
   Comment-only softening of `cache_stub.rs:11-12` landed in the
   same commit. Cargo.lock delta was exactly one line
   (`+ "rge-asset-store"` under `rge-io-gltf`'s dependency list);
   the verbatim "MUST halt if Cargo.lock changes extend beyond the
   single new asset-store dep edge" clause held. Test coverage:
   round-trip equality with `MemoryCache`, BLAKE3 dedup preserved
   through the asset-store backing, `GltfError::Cache` surfacing
   on backing failure. Codex control: `pass` /
   `commit_readiness: ready_for_publish`. Full canonical verify
   gate (6/6 steps) green on the branch.

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

6. **[DONE 2026-05-22 via PR #97 / commit `8879ac3`] Add negative-test coverage for `AssetStoreCache::try_insert_*` failure behavior.**
   The #94 adapter
   (`crates/io-gltf/src/asset_store_cache.rs`) introduced a fallible
   `try_insert_*` family (mesh / material / animation / skeleton /
   image) that surfaces backing-cache failures as
   `GltfError::Cache`. The existing test suite covers two members
   (`try_insert_mesh_surfaces_backing_error_as_gltf_cache` and
   `try_insert_image_surfaces_backing_error_as_gltf_cache`) plus
   one `From<CacheError>` round-trip
   (`cache_error_bad_asset_id_maps_to_gltf_cache_variant`). That
   leaves three `try_insert_*` methods uncovered against backing
   failure, the mirror-update-on-failure assertion only checked
   for `try_insert_mesh`, and no test for the recovery path. Close
   those gaps with one additional `#[cfg(test) mod tests` block in
   the same file, using a deliberately-failing in-memory
   `ByteCache` test double in the same shape as the existing
   `FailingBacking` struct.

   **Allowed file surface**:
   - EDIT: `crates/io-gltf/src/asset_store_cache.rs` — additions
     to the existing `#[cfg(test)] mod tests` block ONLY. No
     changes outside that module.

   **Files that MUST NOT be touched**:
   - Anything OUTSIDE `crates/io-gltf/src/asset_store_cache.rs`
     except the dispatch's own `ai_handoffs/` packet.
   - Specifically: `crates/asset-store/**`, `crates/io-image/**`,
     `editor/**`, `crates/editor-shell/**`, `crates/gfx/**`,
     `crates/brep-render/**`, `kernel/**`, any other crate.
   - `Cargo.toml`, `Cargo.lock`, workspace dependency
     declarations, feature flags.
   - The non-test code in `asset_store_cache.rs` itself — adapter
     struct, `impl Cache`, `try_insert_*` bodies, canonical-byte
     encoders, `From<CacheError> for GltfError`. ALL test-only
     additions.
   - Existing tests in the module. Add new tests; do not modify
     or rename existing ones.

   **Cargo.lock policy**: NO changes. This task adds zero new
   dependencies; if Cargo.lock shows any diff at all, halt.

   **Halt conditions**:
   - The test plan requires touching production code in
     `asset_store_cache.rs` (e.g. a "while I'm here, let me
     refactor the encoder" — out of scope).
   - The test plan requires a new dependency (e.g. `mockall`,
     `pretty_assertions`). Out of scope; use plain
     `assert!` / `assert_matches!` / `matches!` only.
   - The test plan requires changes to `crates/asset-store/**`
     (e.g. extending `CacheError` with a new variant). Out of
     scope — re-use existing `CacheError::Io` /
     `CacheError::BadAssetId` for failure simulation.
   - AssetId-collision simulation is explicitly **out of scope**
     for this task: collision semantics through the typed-handle
     bridge are awkward to construct without API design work, and
     a narrow negative-test task is not the place for that design
     discussion. If the test plan requires forcing an AssetId
     collision, halt with `NEEDS_HUMAN`.

   **Verbatim review-gate strings** — the autonomous selector
   MUST copy these six strings, character-for-character, into the
   filed GitHub issue body. No paraphrasing, no substitution, no
   reflowing. A packet that lacks any one of them verbatim is
   bounced at review:

   ```
   MUST be test-only additions inside the existing #[cfg(test)] mod tests block of crates/io-gltf/src/asset_store_cache.rs
   MUST NOT modify any file outside crates/io-gltf/src/asset_store_cache.rs (except the dispatch's own ai_handoffs/ packet)
   MUST NOT add any new dependency or modify Cargo.toml / Cargo.lock
   MUST add a try_insert_* negative test for each of the three currently-uncovered families (material, animation, skeleton)
   MUST assert that the typed mirror is NOT updated when the backing write fails
   MUST add a recovery test using a switchable backing pattern (e.g. Cell<bool> or AtomicBool) that toggles the same backing from fail to succeed across try_insert_* calls, AND asserts the post-recovery returned handle equals the asset's content_hash() digest
   ```

   **Done-criterion**:
   - Three new `try_insert_<family>_surfaces_backing_error_as_gltf_cache`-style
     tests for the currently-uncovered families: `material`,
     `animation`, `skeleton`. Each test asserts (a) the call
     returns `Err(GltfError::Cache(_))`, and (b) the
     corresponding typed-mirror `HashMap` length stays at 0
     after the failed insert.
   - One recovery test (suggested name
     `try_insert_recovers_after_prior_backing_failure`) that
     uses a `ByteCache` test double whose failure mode is
     switchable (e.g. a `Cell<bool>` / `AtomicBool` flag the
     test toggles). The test:
       1. Toggles the backing to fail; calls `try_insert_mesh`;
          asserts `Err(GltfError::Cache(_))` and
          `adapter.meshes.len() == 0`.
       2. Toggles the backing to succeed; calls `try_insert_mesh`
          on the same `MeshAsset`; asserts `Ok(handle)` and
          `adapter.meshes.len() == 1`.
       3. Asserts the returned handle equals the asset's
          `content_hash()` digest (the canonical-bytes contract
          documented in `asset_store_cache.rs`).
     The recovery test proves the adapter is not left in a
     poisoned state by a prior backing failure — every
     `try_insert_*` is independently transactional w.r.t. the
     typed mirror.
   - Verification gates that MUST pass:
     - `cargo test -p rge-io-gltf --lib --no-fail-fast` → exit 0
     - `cargo run -q -p rge-tool-architecture-lints -- all` →
       exit 0
     - `.ai/dispatch.verify.ps1` → all 6 steps PASS, exit 0
   - Diff stat: a single file changed
     (`crates/io-gltf/src/asset_store_cache.rs`) plus the
     dispatch's own `ai_handoffs/` packet. Zero Cargo.lock
     changes.

   **Why this is the right next task** (context for the
   reviewer, NOT required in the filed issue body): #94 was the
   first real production-source autonomous dispatch and it
   landed cleanly. Task #6 stays inside the SAME file the loop
   just successfully edited and stress-tests the new code's
   robustness without expanding scope — the safest possible
   next dispatch, matching the post-#94 readiness posture (no
   `-PublishMode main` yet; selective use only for
   docs/test-only or very narrow source tasks).

7. **[DONE 2026-05-22 via PR #99 / commit `012f119`] Read-only preflight: `rge-io-image` cache-surface follow-up after #92/#94.**
   **NO source edits.** Audit whether `crates/io-image/` has a real
   cache surface that should route through `rge-asset-store`, or
   whether its `asset_store_stub` is dead/stale scaffolding that
   should be deleted or documented as intentionally deferred. This is
   the explicit follow-up deferred by the #92 audit and task #5 halt
   conditions: "`rge-io-image` needs changes to make its codec output
   cacheable" belongs in its own scoping dispatch, not inside the
   `io-gltf` adapter work.

   **Allowed file surface**:
   - MAY add exactly one execution report packet:
     `ai_handoffs/ISSUE-*_EXEC_*.md`, plus its `.meta.json` sidecar
     if produced by `new-handoff.ps1 -Finalize`.
   - MAY read `crates/io-image/**`, `crates/asset-store/**`, and
     `crates/io-gltf/src/asset_store_cache.rs` as precedent only.
   - MAY read existing #92/#94 handoff packets if needed to keep the
     follow-up aligned with the audit/adaptor precedent.

   **Files that MUST NOT be touched**:
   - Any tracked repository file.
   - Any source file, test file, fixture, Cargo manifest, Cargo.lock,
     workspace dependency declaration, feature flag, generated file,
     script, schema, lint file, CI/workflow file, doc, ADR,
     `Status.md`, `HANDOFF.md`, `change.md`, or existing handoff
     packet.
   - Anything under `kernel/**`, `editor/**`,
     `crates/editor-shell/**`, `crates/gfx/**`,
     `crates/brep-render/**`, `crates/io-gltf/**` except read-only
     inspection of `crates/io-gltf/src/asset_store_cache.rs`, or any
     crate outside the stated audit scope.

   **Five-question preflight answer block**:
   The EXEC report must contain a section titled exactly
   `## 5-Question Preflight Answer Block` and answer exactly these
   headings:
   - `Q1. Is crates/io-image/src/asset_store_stub.rs reachable today, or pure dead code?`
   - `Q2. What cache trait or cache-like surface does rge-io-image expose today, if any, and how does it compare to rge-asset-store::Cache?`
   - `Q3. Is there a meaningful BLAKE3/content-addressed identity for rge-io-image Image outputs analogous to io-gltf asset content_hash()?`
   - `Q4. Which current call sites would benefit from an asset-store-backed image cache, if any, and is any editor-side cache already sufficient?`
   - `Q5. What is the smallest safe follow-up dispatch: adapter, stub deletion, no-op-and-document, or design preflight?`

   **Acceptance criteria**:
   - Q1 cites concrete search evidence for every current reference to
     `asset_store_stub`, `Cache`, `MemoryCache`, and
     `rge_asset_store` in `crates/io-image/**`.
   - Q2 distinguishes the local `asset_store_stub::Cache` shape
     (`put`/`get`/`has`, `AssetId = Vec<u8>`, infallible methods,
     synthetic IDs) from `rge_asset_store::Cache` (`Result`,
     `AssetId`, `Bytes`, `put`/`get`/`evict_lru`, `LocalCache`).
   - Q3 answers from code, not aspiration: inspect
     `crates/io-image/src/image_data.rs` and codec modules for an
     `Image` / pixel-data canonical identity, hash, or equivalent.
   - Q4 is non-speculative: cite in-repo call sites or state clearly
     that no real call site currently consumes the stub/cache surface.
   - Q5 proposes one smallest follow-up with allowed files,
     must-not-touch surfaces, verification gates, halt conditions, and
     why it is safer than the alternatives. If no autonomous-friendly
     follow-up exists, say so and recommend a design preflight instead.

   **Halt conditions**:
   - Answering the five questions appears to require editing source,
     tests, docs, generated files, Cargo metadata, scripts, schemas,
     lints, or existing packets.
   - The audit requires a second artifact, generated log, scratch file,
     benchmark output, or packet other than the single EXEC report.
   - Q2 reveals that `rge-io-image` has no cache trait/cache-like
     surface that can be evaluated from current code. In that case,
     halt with `NEEDS_HUMAN` rather than inventing an adapter task.
   - Q4 reveals no reachable cache consumer and Q5 would have to
     invent new image-cache product semantics instead of deleting
     stale scaffolding or documenting deferral. Halt; that is design
     work, not an autonomous implementation task.
   - Any tracked file is already dirty in a way that makes the
     read-only audit ambiguous.

   **Scope-preserving halt clause** - the orchestrator's canonical
   verify gate (`.ai/dispatch.verify.ps1`) runs after Claude execute
   even on read-only audits. If verify fails on a target OUTSIDE the
   audit scope (anything beyond `crates/io-image/`,
   `crates/asset-store/`, `crates/io-gltf/src/asset_store_cache.rs`,
   or this dispatch's own `ai_handoffs/` packet), the orchestrator
   may auto-route a CORRECTION packet asking the executor to fix the
   failure. When that happens **the executor MUST halt**: write an
   EXECUTION_REPORT with `EXEC_STATUS: blocked` and
   `STATUS: NEEDS_HUMAN`, do NOT execute the correction. Read-only
   intent is the entire reason this task is in the brief; a
   correction-round source fix to an unrelated test bug expands a
   read-only audit into a source-fix dispatch and must become its own
   ticket. Precedent: ISSUE-92 (2026-05-22) validated this path by
   preserving the W16 audit while routing an unrelated `rge-gfx`
   verify failure to `NEEDS_HUMAN`.

   **Verbatim review-gate strings** - the autonomous selector MUST
   copy these six strings, character-for-character, into the filed
   GitHub issue body. No paraphrasing, no substitution, no reflowing.
   A packet that lacks any one of them verbatim is bounced at review:

   ```
   MUST be a read-only preflight; do not edit source, tests, docs, Cargo.toml, Cargo.lock, or existing packets
   MUST produce a 5-question preflight answer block covering reachability, cache surface, content-addressed identity, real consumers, and smallest follow-up
   MUST inspect crates/io-image/src/asset_store_stub.rs, crates/io-image/src/lib.rs, crates/io-image/src/image_data.rs, and crates/asset-store/src/cache.rs
   MUST cross-reference crates/io-gltf/src/asset_store_cache.rs only as adapter precedent, not as permission to edit io-gltf
   MUST halt if rge-io-image has no real cache trait/cache-like surface or no reachable cache consumer and an adapter would become design work
   MUST halt rather than fix if verification fails outside crates/io-image/**, crates/asset-store/**, crates/io-gltf/src/asset_store_cache.rs, or this dispatch's ai_handoffs packet
   ```

   **Done-criterion**:
   - One `ISSUE-*_EXEC_*.md` report with the exact
     `## 5-Question Preflight Answer Block` section and Q1-Q5
     headings above.
   - No source, test, doc, Cargo, lint, schema, workflow, status, or
     existing handoff packet edits.
   - `git status --short --untracked-files=no` is clean before and
     after writing the EXEC report.
   - Verification claims are read-only only: document the grep/read
     commands used for the audit; do not manually run cargo tests,
     builds, fmt, or `.ai/dispatch.verify.ps1`. The orchestrator will
     still run its canonical verification gate after execution.

8. **[DONE 2026-05-23 via PR #101 / commit `7d6d9a8`] Read-only preflight: GitHub Actions CI failure boundary and gate parity.**
   **NO source edits.** Audit why GitHub Actions has been red on
   `main` while the seven-task autonomous arc treated the local
   `.ai/dispatch.verify.ps1` gate as authoritative. This dispatch is
   diagnostic only: identify the first failing boundary, classify each
   workflow's failure mode, compare CI coverage against the local
   verify gate, and propose the smallest safe follow-up. Do not fix
   workflow files, Cargo metadata, source, tests, scripts, or docs.

   **Allowed read-only scope**:
   - MAY read `.github/workflows/**`.
   - MAY read the workspace root `Cargo.toml`.
   - MAY read `rust-toolchain.toml` if present.
   - MAY read `.ai/dispatch.verify.ps1`.
   - MAY use read-only `git` commands to inspect commit history and
     diffs, including the commit range between the last green and
     first red GitHub Actions runs.
   - MAY use read-only `gh api`, `gh run view`, and `gh workflow`
     commands to inspect workflow run history, check conclusions, and
     stream failed-job logs. Do not download artifacts or write logs
     to disk.

   **Allowed file surface**:
   - MAY add exactly one execution report packet:
     `ai_handoffs/ISSUE-*_EXEC_*.md`, plus its `.meta.json` sidecar
     if produced by `new-handoff.ps1 -Finalize`.

   **Files that MUST NOT be touched**:
   - Any tracked repository file.
   - Any source file, test file, fixture, Cargo manifest,
     `Cargo.lock`, workflow file, script, schema, lint file, doc, ADR,
     `Status.md`, `HANDOFF.md`, `change.md`, or existing handoff
     packet.
   - Anything under `crates/**`, `editor/**`, `kernel/**`,
     `runtime/**`, `examples/**`, `tools/**`, or any generated
     directory.

   **Five-question CI preflight answer block**:
   The EXEC report must contain a section titled exactly
   `## 5-Question CI Preflight Answer Block` and answer exactly these
   headings:
   - `Q1. When did CI start failing, and what changed at that boundary?`
   - `Q2. What is the error pattern in each failing workflow?`
   - `Q3. What failure category best explains each workflow: stale config, toolchain drift, missing infrastructure, or real repo breakage?`
   - `Q4. What does GitHub Actions catch that .ai/dispatch.verify.ps1 does not, and what does local verify catch that CI does not?`
   - `Q5. What is the smallest safe follow-up dispatch?`

   **Acceptance criteria**:
   - Q1 verifies, rather than assumes, the last-green and first-red
     `main` commits. The preliminary human note names last green
     `058e26d` (2026-05-07); confirm or correct that with GitHub
     Actions run data. Inspect and summarize the diff between the
     verified last-green and first-red commits.
   - Q2 covers every failing workflow in the current CI surface and
     includes the first 20 relevant lines from one representative
     failed-job log per workflow. If the raw first 20 log lines are
     pure setup noise, include the first 20 lines at the failure site
     and say so.
   - Q3 classifies each workflow into one of these categories:
     stale config, toolchain drift, missing infrastructure, or real
     repo breakage. If a workflow has mixed causes, list the primary
     cause first and the secondary cause second.
   - Q4 compares `.github/workflows/**` against
     `.ai/dispatch.verify.ps1` concretely by command/job, not by
     prose impression. Identify meaningful blind spots in either
     direction. If the local gate is not safe as the authoritative
     gate, say that explicitly.
   - Q5 proposes exactly one smallest safe follow-up: workflow-file
     edit, toolchain pin, infrastructure restore, source/test fix,
     delete-obsolete-workflow cleanup, or no-op-and-document. If no
     autonomous-friendly follow-up exists, recommend a human-owned
     design/CI policy dispatch instead.

   **Halt conditions**:
   - Q3 reveals a real repo breakage requiring source or test edits.
     Halt with `NEEDS_HUMAN`; the source/test fix must be a separate
     dispatch after the audit lands.
   - Q5 would require editing workflow files, Cargo metadata, scripts,
     source, or tests to answer the audit. Halt; this dispatch is
     read-only.
   - Q4 reveals meaningful blind spots in `.ai/dispatch.verify.ps1`
     that would require a substantive verify-gate rewrite. Halt with
     `NEEDS_HUMAN` and document the gap.
   - The audit requires more than one EXEC packet, any generated log
     file, downloaded artifact, scratch file, or second handoff
     artifact.
   - The audit cannot be answered without running local `cargo`
     commands, `.ai/dispatch.verify.ps1`, formatters, architecture
     lints, or tests. Halt; this is a documentary read-only audit.
   - Any tracked file is already dirty in a way that makes the
     read-only audit ambiguous.

   **Scope-preserving halt clause** - the orchestrator's canonical
   verify gate (`.ai/dispatch.verify.ps1`) runs after Claude execute
   even on read-only audits. If verify fails on a target OUTSIDE the
   audit scope (anything beyond `.github/workflows/**`, root
   `Cargo.toml`, `rust-toolchain.toml`, `.ai/dispatch.verify.ps1`, or
   this dispatch's own `ai_handoffs/` packet), the orchestrator may
   auto-route a CORRECTION packet asking the executor to fix the
   failure. When that happens **the executor MUST halt**: write an
   EXECUTION_REPORT with `EXEC_STATUS: blocked` and
   `STATUS: NEEDS_HUMAN`, do NOT execute the correction. Read-only
   intent is the entire reason this task is in the brief; a
   correction-round source fix to an unrelated code/test failure
   expands a CI audit into a source-fix dispatch and must become its
   own ticket. Precedent: ISSUE-92 and ISSUE-98 validated that
   blocked read-only audits are valid deliverables when they preserve
   scope.

   **Verbatim review-gate strings** - the autonomous selector MUST
   copy these seven strings, character-for-character, into the filed
   GitHub issue body. No paraphrasing, no substitution, no reflowing.
   A packet that lacks any one of them verbatim is bounced at review:

   ```
   MUST be a read-only CI audit; do not edit workflows, source, tests, docs, Cargo.toml, Cargo.lock, scripts, or existing packets
   MUST produce a 5-question CI preflight answer block covering failure boundary, workflow error patterns, failure categories, local-vs-CI gate parity, and smallest follow-up
   MUST inspect .github/workflows/**, root Cargo.toml, rust-toolchain.toml if present, and .ai/dispatch.verify.ps1
   MUST use read-only GitHub Actions evidence via gh api / gh run view / gh workflow commands, not assumptions from memory
   MUST include the first 20 relevant lines from one representative failed-job log per failing workflow
   MUST NOT run local cargo commands, tests, formatters, architecture lints, or .ai/dispatch.verify.ps1
   MUST halt rather than fix if the audit discovers real repo breakage, verify-gate blind spots requiring rewrite, or any need to edit CI/workflow files
   ```

   **Done-criterion**:
   - One `ISSUE-*_EXEC_*.md` report with the exact
     `## 5-Question CI Preflight Answer Block` section and Q1-Q5
     headings above.
   - No source, test, doc, Cargo, workflow, lint, schema, script,
     status, or existing handoff packet edits.
   - `git status --short --untracked-files=no` is clean before and
     after writing the EXEC report.
   - Verification claims are read-only only: document the `git` and
     `gh` commands used for the audit; do not manually run cargo
     tests, builds, fmt, architecture lints, or `.ai/dispatch.verify.ps1`.
     The orchestrator will still run its canonical verification gate
     after execution.
   - Q5 names one smallest next dispatch and includes its proposed
     allowed files, must-not-touch surfaces, verification gates, and
     halt conditions, unless the correct outcome is `NEEDS_HUMAN`.

9. **Add `bench.yml` parity to `.ai/dispatch.verify.ps1`.**
   Single-file verification-gate edit. The #100 CI audit Q4 found
   that the local canonical dispatch gate mirrors `fmt.yml`,
   `architecture.yml`, `deny.yml`, and `tests.yml`, but does not
   mirror `bench.yml`'s compile-only bench check. Add the missing
   bench compile step to the local gate so future dispatches exercise
   the same in-repo bench target that CI intends to cover.

   This is behavior-changing for every future dispatch because the
   file being edited is the gate itself. Keep `-PublishMode branch`
   for this task even though the expected diff is a single script
   edit.

   **Allowed file surface**:
   - EDIT `.ai/dispatch.verify.ps1` only.
   - MAY add exactly one new `Invoke-Step` invocation for
     `cargo bench -p rge-script-bench --no-run`, matching the
     established `Invoke-Step` pattern already used in the file.
   - MAY update the script's docstring/header from "four GitHub
     Actions workflows" to "five" and enumerate `bench.yml` alongside
     `fmt.yml`, `architecture.yml`, `deny.yml`, and `tests.yml`.
   - MAY add this dispatch's own `ai_handoffs/ISSUE-*_EXEC_*.md`
     packet plus `.meta.json` sidecar if produced by the orchestrator.

   **Files that MUST NOT be touched**:
   - Any file outside `.ai/dispatch.verify.ps1`, except this
     dispatch's own `ai_handoffs/` packet.
   - Any Rust source file, test file, fixture, workflow, other script,
     doc, ADR, status file, existing handoff packet, `Cargo.toml`, or
     `Cargo.lock`.
   - The existing four `Invoke-Step` invocations must not be
     restructured; this task is additive only.

   **Cargo.lock policy**:
   - Zero Cargo metadata changes. If `Cargo.toml` or `Cargo.lock`
     changes at all, halt with `NEEDS_HUMAN`.

   **Halt conditions**:
   - The new `Invoke-Step` fails locally, meaning
     `cargo bench -p rge-script-bench --no-run` does not compile
     cleanly on this machine. Halt with `NEEDS_HUMAN`; do not fix
     bench-target breakage in this dispatch.
   - The script's structure requires more than one `Invoke-Step`
     addition or any non-trivial refactor of the existing four
     `Invoke-Step` invocations. Halt; this task is only the smallest
     closing edit.
   - The script's docstring/header cannot be updated from the
     described "four GitHub Actions workflows" wording to "five" with
     `bench.yml` named. Halt; the #100 audit evidence would be stale.
   - Any tracked file outside `.ai/dispatch.verify.ps1` shows a diff
     after execution, except this dispatch's own `ai_handoffs/`
     packet. Halt rather than clean up unrelated changes.

   **Verbatim review-gate strings** - the autonomous selector MUST
   copy these five strings, character-for-character, into the filed
   GitHub issue body. No paraphrasing, no substitution, no reflowing.
   A packet that lacks any one of them verbatim is bounced at review:

   ```
   MUST add exactly one new Invoke-Step invocation for cargo bench -p rge-script-bench --no-run
   MUST update the script's docstring/header from "four GitHub Actions workflows" to "five" and enumerate bench.yml alongside fmt.yml / architecture.yml / deny.yml / tests.yml
   MUST NOT modify any file outside .ai/dispatch.verify.ps1 (except the dispatch's own ai_handoffs/ packet)
   MUST NOT add any new dependency or modify Cargo.toml / Cargo.lock
   MUST halt with NEEDS_HUMAN if the new Invoke-Step fails locally rather than attempting to fix any bench-target breakage in this dispatch
   ```

   **Done-criterion**:
   - Exactly one new `Invoke-Step` invocation appears in
     `.ai/dispatch.verify.ps1` for
     `cargo bench -p rge-script-bench --no-run`.
   - The script docstring/header says it mirrors five GitHub Actions
     workflows and names `bench.yml` alongside `fmt.yml`,
     `architecture.yml`, `deny.yml`, and `tests.yml`.
   - `.ai/dispatch.verify.ps1` exits 0 when run end-to-end; the new
     bench compile step passes alongside the existing gate steps.
   - Diff stat is limited to `.ai/dispatch.verify.ps1` plus this
     dispatch's own `ai_handoffs/` packet. Zero Cargo, source, test,
     fixture, workflow, status, or unrelated doc edits.
