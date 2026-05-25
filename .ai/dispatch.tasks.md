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

1. **[DONE 2026-05-22 via PR #86 / commit `87d15b5`] Add automatic `--glb` file watching on top of the R-key reload path.**
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

2. **[DONE 2026-05-22 via PR #88 / commit `168aab9`] Add a smooth-normal glTF fixture + extend visual acceptance for M3.**
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

3. **[DONE 2026-05-22 via PR #90 / commit `6ea878a`] Add malformed-GLB reload regression coverage.**
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


17. **[DONE 2026-05-23 via PR #119 / commit `aa3916c`] Docs-only reconciliation: editor-shell render-frame perf-harness deferral text.**
   Reconcile stale present-tense documentation now that task #16
   established that `crates/editor-shell/src/render_frame_e2e_perf.rs`
   exists and has committed recorder-host result evidence in
   `ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_EXEC_2026-05-14_21-51-40+0300.md`.
   This is the exact smallest follow-up named by #16 Q5: update the
   current/stale deferral wording without changing harness source,
   without adding a new `plans/BASELINE.md` measurement row, and
   without rewriting dated history.

   **Allowed file surface**:
   - EDIT `plans/BASELINE.md` only to reconcile the §6.3
     "Post-depth Gate A" paragraph's stale "blocked on
     `EditorShell::render_frame` accepting a mock event loop" and
     "What's still deferred: option (b) non-winit editor-shell perf
     harness" wording. Record that option (b) landed at commit
     `f8b8ed4` via `crates/editor-shell/src/render_frame_e2e_perf.rs`
     and cross-reference the POSTV0 EXEC packet. Do not add a new
     measurement row or copy P95 numbers into BASELINE.
   - EDIT `Status.md` only by prepending a new dated snapshot that
     records the stale `editor-shell mock-event-loop perf harness`
     deferral as landed. Preserve the existing dated snapshots.
   - EDIT `HANDOFF.md` only by prepending a matching new dated
     snapshot. Preserve the existing dated snapshots.
   - EDIT `change.md` only by appending one new chronological entry
     for this docs reconciliation. Preserve all existing entries.
   - MAY add this dispatch's own `ai_handoffs/ISSUE-*_EXEC_*.md`
     packet plus `.meta.json` sidecar if produced by the orchestrator.

   **Files that MUST NOT be touched**:
   - `plans/IMPLEMENTATION.md`.
   - `crates/editor-shell/**` and every other `crates/**` source,
     test, bench, or fixture path.
   - `Cargo.toml`, `Cargo.lock`, workspace manifests, workflows,
     scripts, schemas, architecture-lint code, ADRs, doctrine docs,
     existing handoff packets, and unrelated docs.
   - Existing historical entries in `change.md`, `Status.md`, and
     `HANDOFF.md`; this task is prepend/append reconciliation only.

   **Cargo.lock policy**:
   - Zero Cargo metadata changes. If `Cargo.toml` or `Cargo.lock`
     changes at all, halt with `NEEDS_HUMAN`.

   **Halt conditions**:
   - Reconciling the text appears to require editing
     `plans/IMPLEMENTATION.md`, any source, any test, or the perf
     harness itself. Halt with `NEEDS_HUMAN`.
   - The change would require adding a new `plans/BASELINE.md`
     measurement row, copying recorder-host P95 numbers into BASELINE,
     or selecting hard thresholds for the editor-shell harness. Halt;
     that is a measurement-record dispatch, not this reconciliation.
   - The change would require retroactively rewriting any existing
     `change.md` entry, dated `Status.md` snapshot, dated `HANDOFF.md`
     snapshot, or existing `ai_handoffs/` packet. Halt.
   - Any tracked file outside `plans/BASELINE.md`, `Status.md`,
     `HANDOFF.md`, `change.md`, and this dispatch's own
     `ai_handoffs/` packet shows a diff after execution. Halt rather
     than clean up unrelated changes.
   - The executor cannot verify the task #16 Q5 basis from the landed
     `ISSUE-116_EXEC` packet and current docs without running cargo or
     the release-only perf harness. Halt; do not rerun measurements.

   **Verbatim review-gate strings** - the autonomous selector MUST
   copy these seven strings, character-for-character, into the filed
   GitHub issue body. No paraphrasing, no substitution, no reflowing.
   A packet that lacks any one of them verbatim is bounced at review:

   ```
   MUST edit only plans/BASELINE.md, Status.md, HANDOFF.md, and change.md (except the dispatch's own ai_handoffs/ packet)
   MUST NOT modify plans/IMPLEMENTATION.md
   MUST NOT modify crates/editor-shell/**, any other source/test/bench/fixture path, Cargo.toml, or Cargo.lock
   MUST preserve existing dated history by prepending Status.md / HANDOFF.md snapshots and appending one change.md entry rather than rewriting old entries
   MUST NOT add a new BASELINE measurement row, copy recorder-host P95 numbers into BASELINE, or choose hard thresholds for the editor-shell harness
   MUST record that the non-winit editor-shell perf harness landed at commit f8b8ed4 via crates/editor-shell/src/render_frame_e2e_perf.rs and cross-reference ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_EXEC_2026-05-14_21-51-40+0300.md
   MUST halt with NEEDS_HUMAN rather than running cargo commands, the release-only perf harness, or fresh recorder-host measurements
   ```

   **Done-criterion**:
   - `plans/BASELINE.md` no longer claims the non-winit
     editor-shell perf harness is still blocked on a mock event loop or
     still deferred; it records that the harness landed at `f8b8ed4`
     and points to the harness file + POSTV0 EXEC packet.
   - `Status.md` and `HANDOFF.md` have new prepended dated snapshots
     that update the present-tense deferral list while preserving old
     dated snapshots.
   - `change.md` has one new append-only chronological entry for the
     reconciliation; old entries remain byte-for-byte historical
     records.
   - `plans/IMPLEMENTATION.md`, source/test/bench/fixture files,
     Cargo files, workflows, scripts, schemas, and existing packets are
     untouched.
   - Verification: `git diff --check` exits 0; `git status
     --short --untracked-files=no` before/after shows only the four
     allowed docs once staged/committed by the queue; the orchestrator's
     canonical `.ai/dispatch.verify.ps1` gate exits 0. The executor
     does not manually run cargo commands, the release-only perf
     harness, or fresh recorder-host measurements.

18. **[DONE 2026-05-23 via PR #121 / commit `5b770bf`] Read-only preflight: script-bench memory-soak `peak_rss` / `vss_delta` deferral reconciliation.**
   **NO source edits.** Audit whether the current documentation still
   accurately treats `peak_rss` / `vss_delta` soak-harness evidence as
   a future improvement, given that `crates/script-bench/BASELINE.md`
   appears to contain a 2026-05-17 "process-memory metrics enabled"
   one-hour run with `peak_rss_bytes` and `vss_delta_bytes`, and
   `crates/script-bench/src/script_host.rs` appears to contain
   process-memory sampling support. This mirrors task #16's
   reconciliation-audit shape: determine whether the deferral is stale,
   narrower than it looks, or still open, then name exactly one
   smallest follow-up.

   **Allowed read-only scope**:
   - MAY read `crates/script-bench/BASELINE.md`.
   - MAY read `crates/script-bench/METHODOLOGY.md`.
   - MAY read `crates/script-bench/src/script_host.rs`.
   - MAY read `crates/script-bench/Cargo.toml`.
   - MAY read `plans/IMPLEMENTATION.md`, `Status.md`, `HANDOFF.md`,
     and `change.md` only for deferral/status wording.
   - MAY read prior `ai_handoffs/` packets only if directly referenced
     by the script-bench baseline or methodology notes.
   - MAY use read-only `rg`, `git grep`, `git show`, `git log`, and
     file-read commands. Do not run cargo commands or the one-hour soak
     harness; this is a documentary reconciliation audit only.

   **Allowed file surface**:
   - MAY add exactly one execution report packet:
     `ai_handoffs/ISSUE-*_EXEC_*.md`, plus its `.meta.json` sidecar
     if produced by the orchestrator.

   **Files that MUST NOT be touched**:
   - Any tracked repository file outside this dispatch's own
     `ai_handoffs/` EXEC packet.
   - Any source file, test file, fixture, Cargo manifest,
     `Cargo.lock`, workflow file, script, schema, lint file, doc,
     ADR, `Status.md`, `HANDOFF.md`, `change.md`, or existing handoff
     packet.

   **Five-question memory-soak reconciliation answer block**:
   The EXEC report must contain a section titled exactly
   `## 5-Question Memory-Soak Reconciliation Answer Block` and answer
   exactly these headings:
   - `Q1. What process-memory metrics support exists today in script-bench, and what exact path records peak_rss / vss_delta?`
   - `Q2. What formal one-hour memory-soak evidence exists today, and does it include peak_rss / vss_delta values?`
   - `Q3. Which Status / HANDOFF / IMPLEMENTATION / BASELINE deferral text is stale, still accurate, or narrower than the current harness?`
   - `Q4. Is any source or harness change still needed before docs can stop listing peak_rss / vss_delta as a future improvement?`
   - `Q5. What is the smallest safe follow-up dispatch: docs-only reconciliation, measurement rerun, harness change, or NEEDS_HUMAN?`

   **Acceptance criteria**:
   - Q1 cites the source and methodology paths that implement or
     describe `ProcessMemoryMetrics`, `peak_rss_bytes`, and
     `vss_delta_bytes`.
   - Q2 cites any committed formal one-hour run evidence, including
     exact run date, invocation, and whether `peak_rss_bytes` /
     `vss_delta_bytes` were captured. Do not infer a result from code
     existence alone.
   - Q3 cites stale-or-current deferral text by file and line context,
     and classifies each as stale, still accurate, or requiring
     narrower wording.
   - Q4 decides whether the current substrate is already enough for a
     docs-only reconciliation, or whether a fresh recorder-host run /
     harness change is required first.
   - Q5 names exactly one smallest safe follow-up with proposed allowed
     files, must-not-touch surfaces, verification gates, and halt
     conditions. If a human one-hour recorder-host run is needed before
     any repository edit is justified, recommend `NEEDS_HUMAN`.

   **Halt conditions**:
   - Answering Q1-Q5 requires running `cargo`, the one-hour soak, tests,
     formatters, architecture lints, or `.ai/dispatch.verify.ps1`.
     Halt; this dispatch is read-only.
   - The audit discovers that no committed one-hour metrics-enabled
     run exists and that only harness code exists. Halt with
     `NEEDS_HUMAN` unless Q5 can name a measurement-rerun follow-up
     without editing source.
   - Q5 would require changing script-bench source and docs in the
     same follow-up dispatch. Halt; harness changes and documentation
     reconciliation must stay separable unless a human explicitly
     widens scope.
   - The smallest follow-up would require running a one-hour
     recorder-host soak on specific hardware. Halt with `NEEDS_HUMAN`;
     do not fake or extrapolate memory evidence.
   - Any tracked file is already dirty in a way that makes the
     read-only audit ambiguous.

   **Scope-preserving halt clause** - the orchestrator's canonical
   verify gate (`.ai/dispatch.verify.ps1`) runs after Claude execute
   even on read-only audits. If verify fails on a target OUTSIDE the
   audit scope (anything beyond `crates/script-bench/**`,
   `plans/IMPLEMENTATION.md`, `Status.md`, `HANDOFF.md`, `change.md`,
   directly referenced prior `ai_handoffs/` packets, or this dispatch's
   own `ai_handoffs/` packet), the orchestrator may auto-route a
   CORRECTION packet asking the executor to fix the failure. When that
   happens **the executor MUST halt**: write an EXECUTION_REPORT with
   `EXEC_STATUS: blocked` and `STATUS: NEEDS_HUMAN`, do NOT execute
   the correction. Read-only intent is the entire reason this task is
   in the brief; a correction-round source fix to an unrelated
   code/test failure expands a memory-soak reconciliation audit into a
   source-fix dispatch and must become its own ticket.

   **Verbatim review-gate strings** - the autonomous selector MUST
   copy these seven strings, character-for-character, into the filed
   GitHub issue body. No paraphrasing, no substitution, no reflowing.
   A packet that lacks any one of them verbatim is bounced at review:

   ```
   MUST be a read-only memory-soak peak_rss / vss_delta reconciliation audit; do not edit source, tests, docs, Cargo.toml, Cargo.lock, workflows, scripts, or existing packets
   MUST produce a 5-question memory-soak reconciliation answer block covering metrics support, formal one-hour evidence, stale-or-current deferral text, remaining source/harness need, and smallest follow-up
   MUST inspect crates/script-bench/BASELINE.md, crates/script-bench/METHODOLOGY.md, crates/script-bench/src/script_host.rs, crates/script-bench/Cargo.toml, plans/IMPLEMENTATION.md, Status.md, HANDOFF.md, and change.md
   MUST NOT run cargo commands, tests, formatters, architecture lints, .ai/dispatch.verify.ps1, or the one-hour memory soak
   MUST distinguish committed harness code from committed one-hour peak_rss / vss_delta result evidence
   MUST halt with NEEDS_HUMAN if the smallest follow-up requires a fresh one-hour recorder-host soak before any repository edit is justified
   MUST halt rather than combine script-bench source changes and documentation reconciliation in one follow-up dispatch
   ```

   **Done-criterion**:
   - One `ISSUE-*_EXEC_*.md` report with the exact
     `## 5-Question Memory-Soak Reconciliation Answer Block` section
     and Q1-Q5 headings above.
   - No source, test, doc, Cargo, workflow, lint, schema, script,
     status, or existing handoff packet edits.
   - `git status --short --untracked-files=no` is clean before and
     after writing the EXEC report.
   - Verification claims are read-only only: document the `rg`,
     `git grep`, `git show`, `git log`, and file-read commands used
     for the audit; do not manually run cargo tests, builds, fmt,
     architecture lints, `.ai/dispatch.verify.ps1`, or the one-hour
     memory soak. The orchestrator will still run its canonical
     verification gate after execution.
   - Q5 names one smallest next dispatch and includes its proposed
     allowed files, must-not-touch surfaces, verification gates, and
     halt conditions, unless the correct outcome is `NEEDS_HUMAN`.

19. **[DONE 2026-05-23 via PR #123 / commit `2f83ef7`] Docs-only reconciliation: script-bench memory-soak `peak_rss` / `vss_delta` deferral text.**
   Reconcile stale current-state documentation now that task #18
   established that script-bench has both committed process-memory
   harness support and committed one-hour recorder-host result evidence
   for `peak_rss_bytes` / `vss_delta_bytes` in
   `crates/script-bench/BASELINE.md`. This is the exact smallest
   follow-up named by #18 Q5: add forward-current reconciliation text
   while preserving dated 2026-05-12 / 2026-05-14 history.

   **Allowed file surface**:
   - EDIT `plans/IMPLEMENTATION.md` only to reconcile the Phase 3.4
     memory-soak exit-criterion bullet around line 318: preserve the
     2026-05-12 CLOSED record, but add forward-current text that the
     process-memory metrics were re-validated on 2026-05-17 with
     `peak_rss_bytes` / `vss_delta_bytes` captured; point to
     `crates/script-bench/BASELINE.md`.
   - EDIT `Status.md` only by prepending a new dated snapshot that
     records the `peak_rss` / `vss_delta` soak-harness deferral as
     resolved by the 2026-05-17 metrics-enabled run. Preserve existing
     dated snapshots.
   - EDIT `HANDOFF.md` only by prepending a matching new dated
     snapshot. Preserve existing dated snapshots.
   - EDIT `change.md` only by appending one new chronological entry
     for this docs reconciliation. Preserve all existing entries.
   - EDIT `crates/script-bench/BASELINE.md` only to append a minimal
     forward cross-reference at the end of the 2026-05-16
     "Memory-soak process-memory metrics" section's future-tense
     closing paragraph, pointing to the 2026-05-17 formal one-hour run
     already recorded earlier in the same file.
   - MAY add this dispatch's own `ai_handoffs/ISSUE-*_EXEC_*.md`
     packet plus `.meta.json` sidecar if produced by the orchestrator.

   **Files that MUST NOT be touched**:
   - `crates/script-bench/src/**`, `crates/script-bench/benches/**`,
     `crates/script-bench/tests/**`, and `crates/script-bench/Cargo.toml`.
   - Any other source, test, bench, fixture, Cargo manifest,
     `Cargo.lock`, workflow, script, schema, lint file, ADR, doctrine
     doc, existing handoff packet, or unrelated doc.
   - Existing historical entries in `change.md`, `Status.md`, and
     `HANDOFF.md`; this task is prepend/append reconciliation only.

   **Cargo.lock policy**:
   - Zero Cargo metadata changes. If `Cargo.toml` or `Cargo.lock`
     changes at all, halt with `NEEDS_HUMAN`.

   **Halt conditions**:
   - Reconciling the text appears to require editing script-bench
     source, tests, benches, `Cargo.toml`, or the memory-soak harness
     itself. Halt with `NEEDS_HUMAN`.
   - The change would require running a fresh one-hour memory soak,
     selecting a new pass/fail threshold for memory growth, or copying
     all one-hour recorder-host metrics into Status/HANDOFF/change.
     Halt; this is forward reconciliation, not a new certification.
   - The change would require retroactively rewriting any existing
     `change.md` entry, dated `Status.md` snapshot, dated `HANDOFF.md`
     snapshot, or existing `ai_handoffs/` packet. Halt.
   - Any tracked file outside `plans/IMPLEMENTATION.md`, `Status.md`,
     `HANDOFF.md`, `change.md`, `crates/script-bench/BASELINE.md`, and
     this dispatch's own `ai_handoffs/` packet shows a diff after
     execution. Halt rather than clean up unrelated changes.
   - The executor cannot verify the task #18 Q5 basis from the landed
     `ISSUE-120_EXEC` packet and current docs without running cargo or
     the one-hour soak. Halt; do not rerun measurements.

   **Verbatim review-gate strings** - the autonomous selector MUST
   copy these seven strings, character-for-character, into the filed
   GitHub issue body. No paraphrasing, no substitution, no reflowing.
   A packet that lacks any one of them verbatim is bounced at review:

   ```
   MUST edit only plans/IMPLEMENTATION.md, Status.md, HANDOFF.md, change.md, and crates/script-bench/BASELINE.md (except the dispatch's own ai_handoffs/ packet)
   MUST NOT modify crates/script-bench/src/**, crates/script-bench/benches/**, crates/script-bench/tests/**, crates/script-bench/Cargo.toml, Cargo.toml, or Cargo.lock
   MUST preserve existing dated history by prepending Status.md / HANDOFF.md snapshots and appending one change.md entry rather than rewriting old entries
   MUST preserve the 2026-05-12 memory-soak CLOSED record while adding forward-current 2026-05-17 peak_rss / vss_delta evidence text
   MUST add only a minimal forward cross-reference in crates/script-bench/BASELINE.md from the 2026-05-16 harness-revision section to the existing 2026-05-17 formal one-hour run
   MUST NOT run cargo commands, the one-hour memory soak, fresh recorder-host measurements, or select new memory-growth thresholds
   MUST halt with NEEDS_HUMAN rather than changing script-bench source/harness code or rewriting historical Status.md / HANDOFF.md / change.md entries
   ```

   **Done-criterion**:
   - `plans/IMPLEMENTATION.md` no longer leaves the Phase 3.4
     memory-soak bullet as a present-tense "no explicit peak_rss /
     vss_delta capture" limitation; it preserves the 2026-05-12 CLOSED
     record and adds forward-current 2026-05-17 evidence text.
   - `Status.md` and `HANDOFF.md` have new prepended dated snapshots
     that update the present-tense deferral list while preserving old
     dated snapshots.
   - `change.md` has one new append-only chronological entry for the
     reconciliation; old entries remain byte-for-byte historical
     records.
   - `crates/script-bench/BASELINE.md` gains only a minimal
     forward-reference from the 2026-05-16 harness-revision section to
     the existing 2026-05-17 formal one-hour run section.
   - Script-bench source, tests, benches, Cargo files, workflows,
     scripts, schemas, and existing packets are untouched.
   - Verification: `git diff --check` exits 0; `git status
     --short --untracked-files=no` before/after shows only the five
     allowed docs once staged/committed by the queue; the orchestrator's
     canonical `.ai/dispatch.verify.ps1` gate exits 0. The executor
     does not manually run cargo commands, the one-hour soak, or fresh
     recorder-host measurements.

20. **[DONE 2026-05-23 via PR #125 / commit `a955e08`] Read-only preflight: `frame_graph/compile.rs` legibility split plan.**
   **NO source edits.** Audit the optional `compile.rs` legibility
   refactor deferral before any source movement. The target file is
   `crates/gfx/src/frame_graph/compile.rs` (~29 KB). Determine whether
   it can be split mechanically into smaller modules without behavior
   changes, or whether the file is cohesive enough that the correct
   follow-up is `NEEDS_HUMAN` / no-op.

   **Allowed read-only scope**:
   - MAY read `crates/gfx/src/frame_graph/compile.rs`.
   - MAY read sibling frame-graph files needed to understand module
     boundaries: `crates/gfx/src/frame_graph/mod.rs`,
     `crates/gfx/src/frame_graph/resource_map.rs`,
     `crates/gfx/src/frame_graph/texture_pool.rs`, and other
     `crates/gfx/src/frame_graph/**` files only if `compile.rs`
     imports or documents them.
   - MAY read `crates/gfx/tests/**` and `crates/gfx/src/**` only to
     identify tests/callers that would verify a mechanical split.
   - MAY read architecture-lint code/docs that define line-count or
     `SPLIT-EXEMPTION` rules.
   - MAY read `plans/IMPLEMENTATION.md`, `Status.md`, `HANDOFF.md`,
     and `change.md` only for the existing deferral wording.
   - MAY use read-only `rg`, `git grep`, `git show`, `git log`,
     `wc`, and file-read commands. Do not run cargo commands; this is
     a documentary split-plan audit only.

   **Allowed file surface**:
   - MAY add exactly one execution report packet:
     `ai_handoffs/ISSUE-*_EXEC_*.md`, plus its `.meta.json` sidecar
     if produced by the orchestrator.

   **Files that MUST NOT be touched**:
   - Any tracked repository file outside this dispatch's own
     `ai_handoffs/` EXEC packet.
   - Any source file, test file, fixture, Cargo manifest,
     `Cargo.lock`, workflow file, script, schema, lint file, doc,
     ADR, `Status.md`, `HANDOFF.md`, `change.md`, or existing handoff
     packet.

   **Five-question compile split answer block**:
   The EXEC report must contain a section titled exactly
   `## 5-Question Compile Split Preflight Answer Block` and answer
   exactly these headings:
   - `Q1. What responsibilities currently live in crates/gfx/src/frame_graph/compile.rs, and how large is each region?`
   - `Q2. What are the natural module boundaries, if any, that preserve public API and behavior?`
   - `Q3. What tests or callers cover each proposed split boundary today?`
   - `Q4. Is a mechanical split safe without changing algorithms, data structures, serialization, or frame-graph behavior?`
   - `Q5. What is the smallest safe follow-up dispatch: mechanical split, docs-only/no-op, or NEEDS_HUMAN?`

   **Acceptance criteria**:
   - Q1 cites line ranges and names the major responsibilities in
     `compile.rs` (types, validation, aliasing, ordering, tests, or
     equivalent actual regions discovered by the audit).
   - Q2 proposes concrete module names and file paths if a split is
     safe, or explains why no split is justified.
   - Q3 identifies existing tests/callers that would catch regressions,
     including exact test names or commands if a follow-up is proposed.
   - Q4 explicitly states whether the split can be mechanical and
     behavior-preserving; any need to change algorithms or public API
     must route to `NEEDS_HUMAN`.
   - Q5 names exactly one smallest safe follow-up with proposed allowed
     files, must-not-touch surfaces, verification gates, and halt
     conditions. If the split would be broad or design-sensitive,
     recommend `NEEDS_HUMAN` instead.

   **Halt conditions**:
   - Answering Q1-Q5 requires editing source, running cargo, running
     architecture lints, or changing the `SPLIT-EXEMPTION` doctrine.
     Halt; this dispatch is read-only.
   - The audit discovers `compile.rs` already has a current
     `SPLIT-EXEMPTION` that intentionally defers or rejects splitting
     and there is no new pressure beyond old docs. Halt with
     `NEEDS_HUMAN` or recommend no-op; do not force a split.
   - Q5 would require changing algorithms, data structures,
     serialization formats, frame-graph semantics, public APIs, or test
     expectations. Halt with `NEEDS_HUMAN`; this is not a mechanical
     legibility split.
   - The smallest follow-up would touch more than the frame-graph
     module plus tests. Halt with `NEEDS_HUMAN`.
   - Any tracked file is already dirty in a way that makes the
     read-only audit ambiguous.

   **Scope-preserving halt clause** - the orchestrator's canonical
   verify gate (`.ai/dispatch.verify.ps1`) runs after Claude execute
   even on read-only audits. If verify fails on a target OUTSIDE the
   audit scope (anything beyond `crates/gfx/src/frame_graph/**`,
   `crates/gfx/tests/**`, architecture-lint split-rule files,
   `plans/IMPLEMENTATION.md`, `Status.md`, `HANDOFF.md`, `change.md`,
   or this dispatch's own `ai_handoffs/` packet), the orchestrator may
   auto-route a CORRECTION packet asking the executor to fix the
   failure. When that happens **the executor MUST halt**: write an
   EXECUTION_REPORT with `EXEC_STATUS: blocked` and
   `STATUS: NEEDS_HUMAN`, do NOT execute the correction. Read-only
   intent is the entire reason this task is in the brief; a
   correction-round source fix to an unrelated code/test failure
   expands a split-plan audit into a source-fix dispatch and must
   become its own ticket.

   **Verbatim review-gate strings** - the autonomous selector MUST
   copy these seven strings, character-for-character, into the filed
   GitHub issue body. No paraphrasing, no substitution, no reflowing.
   A packet that lacks any one of them verbatim is bounced at review:

   ```
   MUST be a read-only compile.rs legibility split preflight; do not edit source, tests, docs, Cargo.toml, Cargo.lock, workflows, scripts, or existing packets
   MUST produce a 5-question compile split preflight answer block covering current responsibilities, natural module boundaries, test/caller coverage, mechanical-safety judgment, and smallest follow-up
   MUST inspect crates/gfx/src/frame_graph/compile.rs, crates/gfx/src/frame_graph/mod.rs, crates/gfx/src/frame_graph/resource_map.rs, crates/gfx/src/frame_graph/texture_pool.rs, crates/gfx/tests/**, and the architecture-lint split-rule files
   MUST NOT run cargo commands, tests, formatters, architecture lints, .ai/dispatch.verify.ps1, or source-generation commands
   MUST halt with NEEDS_HUMAN if the split would change algorithms, data structures, serialization formats, frame-graph semantics, public APIs, or test expectations
   MUST name exact proposed follow-up file paths if and only if the split is mechanical and behavior-preserving
   MUST halt rather than combine a split implementation with this audit
   ```

   **Done-criterion**:
   - One `ISSUE-*_EXEC_*.md` report with the exact
     `## 5-Question Compile Split Preflight Answer Block` section and
     Q1-Q5 headings above.
   - No source, test, doc, Cargo, workflow, lint, schema, script,
     status, or existing handoff packet edits.
   - `git status --short --untracked-files=no` is clean before and
     after writing the EXEC report.
   - Verification claims are read-only only: document the `rg`,
     `git grep`, `git show`, `git log`, `wc`, and file-read commands
     used for the audit; do not manually run cargo tests, builds, fmt,
     architecture lints, or `.ai/dispatch.verify.ps1`. The
     orchestrator will still run its canonical verification gate after
     execution.
   - Q5 names one smallest next dispatch and includes its proposed
     allowed files, must-not-touch surfaces, verification gates, and
     halt conditions, unless the correct outcome is `NEEDS_HUMAN`.

21. **[DONE 2026-05-23 via PR #127 / commit `02a241a`] Mechanically split `crates/gfx/src/frame_graph/compile.rs` into a directory module.**
   Implement the exact mechanical split identified by task #20 Q5.
   This is a source refactor only: move code into smaller modules
   without changing algorithms, public API, serialization, structural
   hash bytes, frame-graph semantics, or test expectations.

   **Allowed file surface**:
   - DELETE `crates/gfx/src/frame_graph/compile.rs`.
   - ADD `crates/gfx/src/frame_graph/compile/mod.rs`.
   - ADD `crates/gfx/src/frame_graph/compile/error.rs`.
   - ADD `crates/gfx/src/frame_graph/compile/types.rs`.
   - ADD `crates/gfx/src/frame_graph/compile/algorithm.rs`.
   - MAY edit rustdoc intra-doc links inside the new
     `crates/gfx/src/frame_graph/compile/**` files only if the module
     path depth changes require it.
   - MAY add this dispatch's own `ai_handoffs/ISSUE-*_EXEC_*.md`
     packet plus `.meta.json` sidecar if produced by the orchestrator.

   **Expected module contents**:
   - `compile/mod.rs`: existing module doc from `compile.rs`, `mod`
     declarations for `error`, `types`, and `algorithm`; public
     re-exports of `CompileError`, `AliasingGroup`,
     `CompiledFrameGraph`, and `ResourceLifetime`; `pub(crate)` re-export
     of `compile_passes`.
   - `compile/error.rs`: `CompileError` enum and derives.
   - `compile/types.rs`: `ResourceLifetime`, `AliasingGroup`,
     `CompiledFrameGraph`, their impl blocks, `structural_hash`, and
     type-targeted unit tests.
   - `compile/algorithm.rs`: `compile_passes` and algorithm-targeted
     unit tests.

   **Files that MUST NOT be touched**:
   - `crates/gfx/src/frame_graph/mod.rs`, unless a compiler error proves
     the existing `mod compile;` declaration cannot resolve the new
     directory module shape. If that happens, halt before editing it.
   - `crates/gfx/src/lib.rs`.
   - Any `crates/gfx/src/frame_graph/**` file outside the five allowed
     compile-module paths above.
   - Any `crates/gfx/tests/**` file.
   - Any source/test/bench/fixture path outside `crates/gfx/src/frame_graph/compile/**`.
   - `Cargo.toml`, `Cargo.lock`, workflows, scripts, schemas, lints,
     docs, ADRs, status files, and existing handoff packets.

   **Cargo.lock policy**:
   - Zero Cargo metadata changes. If `Cargo.toml` or `Cargo.lock`
     changes at all, halt with `NEEDS_HUMAN`.

   **Halt conditions**:
   - The split requires changing algorithm logic in `compile_passes`,
     public API names/visibility, `FrameGraph::compile`, re-export
     paths in `crates/gfx/src/lib.rs`, serialization derives/fields, or
     `structural_hash` byte order. Halt with `NEEDS_HUMAN`.
   - The split requires changing test expectations, adding tests,
     deleting tests, or touching integration tests. Halt; this dispatch
     is a mechanical move only.
   - The split requires adding `// SPLIT-EXEMPTION` or changing
     architecture-lint rules. Halt.
   - Any tracked file outside the five allowed compile-module paths and
     this dispatch's own `ai_handoffs/` packet shows a diff after
     execution. Halt rather than clean up unrelated changes.
   - `cargo test -p rge-gfx --lib --no-fail-fast` or the named
     frame-graph smoke tests fail due to anything other than the
     mechanical split. Halt rather than broadening scope.

   **Verbatim review-gate strings** - the autonomous selector MUST
   copy these eight strings, character-for-character, into the filed
   GitHub issue body. No paraphrasing, no substitution, no reflowing.
   A packet that lacks any one of them verbatim is bounced at review:

   ```
   MUST replace crates/gfx/src/frame_graph/compile.rs with crates/gfx/src/frame_graph/compile/mod.rs, error.rs, types.rs, and algorithm.rs
   MUST preserve CompileError, ResourceLifetime, AliasingGroup, CompiledFrameGraph, compile_passes, and all public re-exports with no public API change
   MUST preserve compile_passes algorithm behavior, structural_hash byte order, serde derives, and the #[serde(skip)] descriptors field exactly
   MUST NOT modify crates/gfx/src/frame_graph/mod.rs, crates/gfx/src/lib.rs, crates/gfx/tests/**, Cargo.toml, or Cargo.lock
   MUST NOT add SPLIT-EXEMPTION, change architecture-lint rules, add tests, delete tests, or change test expectations
   MUST only adjust rustdoc intra-doc links inside crates/gfx/src/frame_graph/compile/** if module path depth requires it
   MUST halt with NEEDS_HUMAN if the split requires algorithm, API, serialization, frame-graph semantics, or test-expectation changes
   MUST run cargo +nightly fmt --check -p rge-gfx, cargo test -p rge-gfx --lib --no-fail-fast, cargo test -p rge-gfx --test frame_graph_smoke, cargo test -p rge-gfx --test frame_graph_umbrella_smoke, and .ai/dispatch.verify.ps1
   ```

   **Done-criterion**:
   - `crates/gfx/src/frame_graph/compile.rs` is gone, replaced by the
     four new files under `crates/gfx/src/frame_graph/compile/`.
   - Existing public imports continue to work:
     `frame_graph::{AliasingGroup, CompileError, CompiledFrameGraph,
     ResourceLifetime}` and `FrameGraph::compile` are unchanged.
   - `structural_hash` implementation is byte-for-byte semantically
     identical: same prefix, separators, iteration order, field order,
     and descriptor orthogonality.
   - No source/test/Cargo/docs/lint files outside the allowed file
     surface are modified.
   - Verification exits 0 for:
     `cargo +nightly fmt --check -p rge-gfx`;
     `cargo test -p rge-gfx --lib --no-fail-fast`;
     `cargo test -p rge-gfx --test frame_graph_smoke`;
     `cargo test -p rge-gfx --test frame_graph_umbrella_smoke`;
     `.ai/dispatch.verify.ps1`.

22. **[DONE 2026-05-23 via PR #129 / commit `1f4219f`] Read-only cap/deferral stop-state audit before the autonomous count reaches 100.**
   **NO source or doc edits.** Produce one planning artifact that
   records the exact automation state at the 99/100 boundary, separates
   live deferrals from superseded historical notes after tasks #16-#21,
   and recommends the smallest safe next step after a human cap/policy
   decision. This task deliberately spends the last available autonomous
   issue slot on situational clarity, not on a change that could require
   another follow-up under the current hard cap.

   **Allowed read-only scope**:
   - MAY read `AI_DISPATCH_AUTOMATION.md`.
   - MAY read `ai_handoffs/AI_HANDOFF_PROTOCOL.md`.
   - MAY read `Invoke-AiDispatchAuto.ps1`.
   - MAY read `Register-AiDispatchSchedule.ps1`.
   - MAY read `.ai/dispatch.tasks.md`.
   - MAY read `Status.md`, `HANDOFF.md`, `change.md`,
     `plans/BASELINE.md`, and `plans/IMPLEMENTATION.md`.
   - MAY read the recent task #16-#21 dispatch packets under
     `ai_handoffs/` only to classify follow-up recommendations.
   - MAY use read-only `gh issue list`, `gh issue view`,
     `gh pr view`, `git log`, `git diff`, `git status`, `rg`,
     `git grep`, `Get-ScheduledTask`, and file-read commands.
   - MUST NOT run cargo commands, tests, formatters, architecture
     lints, `.ai/dispatch.verify.ps1`, or any dispatch launcher.

   **Allowed file surface**:
   - MAY add exactly one execution report packet:
     `ai_handoffs/ISSUE-*_EXEC_*.md`, plus its `.meta.json` sidecar
     if produced by `new-handoff.ps1 -Finalize`.

   **Files that MUST NOT be touched**:
   - Any tracked repository file outside this dispatch's own
     `ai_handoffs/` EXEC packet.
   - Any source file, test file, fixture, Cargo manifest,
     `Cargo.lock`, workflow file, script, schema, lint file, doc,
     ADR, status file, task brief entry, or existing handoff packet.

   **Five-question cap/deferral stop-state answer block**:
   The EXEC report must contain a section titled exactly
   `## 5-Question Cap/Deferral Stop-State Answer Block` and answer
   exactly these headings:
   - `Q1. What is the current autonomous cap state, and what will the next tick do?`
   - `Q2. Which recent deferrals were closed or superseded by tasks #16-#21?`
   - `Q3. Which remaining follow-ups are live, and which require NEEDS_HUMAN before dispatch?`
   - `Q4. What automation-policy decision is required before running more autonomous work beyond 100 ai-auto issues?`
   - `Q5. What is the smallest safe next action after this audit?`

   **Acceptance criteria**:
   - Q1 verifies, rather than assumes, the current `ai-auto` issue count,
     the configured scheduled-task state, the scheduled command's
     `-MaxAutonomousTasks` value, and the driver behavior at the cap.
   - Q2 cites concrete docs or dispatch packets for each task #16-#21
     effect and distinguishes closed/superseded historical notes from
     still-live work.
   - Q3 classifies each remaining candidate as one of:
     `autonomous-friendly`, `needs-human-architecture-decision`,
     `human-admin-only`, `blocked-by-cap-policy`, or `historical-only`.
   - Q4 reads the relevant automation script/docs and states whether
     raising the count beyond 100 is a human policy change, a script
     change, both, or neither. Do not modify the policy or script.
   - Q5 names exactly one smallest safe next action. If the correct next
     action is "stop and make a human cap/policy decision," say that
     plainly rather than seeding or implementing work.

   **Halt conditions**:
   - Answering Q1-Q5 requires editing source, docs, scripts, workflows,
     Cargo metadata, the task brief, or existing packets. Halt; this
     dispatch is read-only.
   - Answering Q1-Q5 requires running cargo, tests, formatters,
     architecture lints, `.ai/dispatch.verify.ps1`, or a dispatch
     launcher. Halt; this is documentary operations preflight only.
   - Q4 reveals that continuing beyond 100 requires changing the driver,
     scheduler, doctrine, or cap policy. Halt with `NEEDS_HUMAN` after
     documenting the finding; do not make the change.
   - Q5 would require seeding a new task, editing the brief, changing the
     scheduler, or implementing a follow-up. Halt; the output is a
     recommendation, not the follow-up.
   - Any tracked file is already dirty in a way that makes the read-only
     audit ambiguous.

   **Scope-preserving halt clause** - the orchestrator's canonical
   verify gate (`.ai/dispatch.verify.ps1`) runs after Claude execute
   even on read-only audits. If verify fails on a target OUTSIDE the
   audit scope (anything beyond automation docs/scripts, the task brief,
   status/planning docs, recent task #16-#21 dispatch packets, GitHub
   issue/PR metadata, scheduled-task metadata, or this dispatch's own
   `ai_handoffs/` packet), the orchestrator may auto-route a CORRECTION
   packet asking the executor to fix the failure. When that happens
   **the executor MUST halt**: write an EXECUTION_REPORT with
   `EXEC_STATUS: blocked` and `STATUS: NEEDS_HUMAN`, do NOT execute
   the correction. Read-only stop-state accounting is the entire reason
   this task is in the brief; a correction-round source/script/doc fix
   expands it into a policy or implementation dispatch and must become
   its own ticket.

   **Verbatim review-gate strings** - the autonomous selector MUST
   copy these seven strings, character-for-character, into the filed
   GitHub issue body. No paraphrasing, no substitution, no reflowing.
   A packet that lacks any one of them verbatim is bounced at review:

   ```
   MUST be a read-only cap/deferral stop-state audit; do not edit source, tests, docs, scripts, workflows, Cargo.toml, Cargo.lock, the task brief, or existing packets
   MUST produce a 5-question cap/deferral stop-state answer block covering cap state, tasks #16-#21 deferrals, remaining live follow-ups, beyond-100 policy, and smallest next action
   MUST verify the ai-auto issue count, scheduled-task state, scheduled command MaxAutonomousTasks value, and cap behavior using read-only commands
   MUST classify remaining candidates as autonomous-friendly, needs-human-architecture-decision, human-admin-only, blocked-by-cap-policy, or historical-only
   MUST inspect AI_DISPATCH_AUTOMATION.md, ai_handoffs/AI_HANDOFF_PROTOCOL.md, Invoke-AiDispatchAuto.ps1, Register-AiDispatchSchedule.ps1, .ai/dispatch.tasks.md, Status.md, HANDOFF.md, change.md, plans/BASELINE.md, and plans/IMPLEMENTATION.md
   MUST NOT run cargo commands, tests, formatters, architecture lints, .ai/dispatch.verify.ps1, Invoke-AiDispatchAuto.ps1, or Invoke-AiDispatchLoop.ps1
   MUST halt with NEEDS_HUMAN rather than changing cap policy, scheduler configuration, automation scripts, the task brief, source, docs, workflows, or Cargo metadata
   ```

   **Done-criterion**:
   - One `ISSUE-*_EXEC_*.md` report with the exact
     `## 5-Question Cap/Deferral Stop-State Answer Block` section and
     Q1-Q5 headings above.
   - No source, test, doc, Cargo, workflow, lint, schema, script,
     status, task-brief, or existing handoff packet edits.
   - `git status --short --untracked-files=no` is clean before and
     after writing the EXEC report.
   - Verification claims are read-only only: document the `rg`,
     `git grep`, `git log`, `git diff`, `git status`, `gh`, and
     scheduled-task read commands used for the audit; do not manually
     run cargo tests, builds, fmt, architecture lints,
     `.ai/dispatch.verify.ps1`, or dispatch launchers. The orchestrator
     will still run its canonical verification gate after execution.
   - Q5 names one smallest safe next action and explicitly states
     whether another autonomous task can run under the current cap.

16. **[DONE 2026-05-23 via PR #117 / commit `26a9ba1`] Read-only preflight: editor-shell render-frame perf-harness reconciliation.**
   **NO source edits.** Audit the apparent mismatch between the older
   V0 / baseline deferral that says a non-winit editor-shell
   `render_frame` perf harness remains deferred and the current source
   tree, which contains `crates/editor-shell/src/render_frame_e2e_perf.rs`
   gated from `crates/editor-shell/src/lib.rs`. The goal is to decide
   whether the deferral is stale documentation, whether the harness
   exists but lacks recorder-host baseline evidence, or whether it
   measures a narrower path that still leaves the original deferral open.

   **Allowed read-only scope**:
   - MAY read `crates/editor-shell/src/render_frame_e2e_perf.rs`.
   - MAY read `crates/editor-shell/src/lib.rs`.
   - MAY read `crates/editor-shell/src/render_path.rs`.
   - MAY read `crates/editor-shell/src/lifecycle/**`.
   - MAY read `crates/editor-shell/tests/**` only to compare existing
     editor-shell frame/test harness precedent.
   - MAY read `plans/BASELINE.md`, `plans/IMPLEMENTATION.md`,
     `change.md`, `Status.md`, and `HANDOFF.md` only for the V0 /
     post-V0 measurement-deferral record.
   - MAY read prior `ai_handoffs/` packets only if directly referenced
     by the harness comments or baseline notes.
   - MAY use read-only `rg`, `git grep`, `git show`, `git log`, and
     file-read commands. Do not run cargo commands or the release-only
     perf harness; this is a documentary reconciliation audit only.

   **Allowed file surface**:
   - MAY add exactly one execution report packet:
     `ai_handoffs/ISSUE-*_EXEC_*.md`, plus its `.meta.json` sidecar
     if produced by the orchestrator.

   **Files that MUST NOT be touched**:
   - Any tracked repository file outside this dispatch's own
     `ai_handoffs/` EXEC packet.
   - Any source file, test file, fixture, Cargo manifest,
     `Cargo.lock`, workflow file, script, schema, lint file, doc,
     ADR, `Status.md`, `HANDOFF.md`, `change.md`, or existing handoff
     packet.

   **Five-question render-frame perf reconciliation answer block**:
   The EXEC report must contain a section titled exactly
   `## 5-Question Render-Frame Perf Reconciliation Answer Block` and
   answer exactly these headings:
   - `Q1. What render-frame perf harness exists today, and what exact production path does it measure?`
   - `Q2. Which V0 or post-V0 deferral text is now stale, still accurate, or narrower than the current harness?`
   - `Q3. Is there recorded recorder-host evidence for the current harness, or only harness code?`
   - `Q4. What invocation, hardware assumptions, and gates should future maintainers use for this harness?`
   - `Q5. What is the smallest safe follow-up dispatch: docs-only reconciliation, recorder-host rerun, harness change, or NEEDS_HUMAN?`

   **Acceptance criteria**:
   - Q1 cites the harness file and names the measured path in terms of
     `acquire_depth_view`, `render_frame_to_target`, encoder/submit,
     and explicitly states whether winit surface acquire/present is in
     or out of scope.
   - Q2 cites the stale-or-current baseline/implementation/status text
     by file and line context, and classifies each as stale, still
     accurate, or requiring narrower wording.
   - Q3 distinguishes committed harness code from committed
     recorder-host result evidence. Do not infer a result from the
     harness's existence.
   - Q4 records the exact documented invocation, release/ignored-test
     constraints, variance gate, soft or hard P95 target semantics, and
     hardware limits of the evidence.
   - Q5 names exactly one smallest safe follow-up with proposed allowed
     files, must-not-touch surfaces, verification gates, and halt
     conditions. If a human recorder-host run is needed before any
     repository edit is justified, recommend `NEEDS_HUMAN`.

   **Halt conditions**:
   - Answering Q1-Q5 requires running the release-only perf harness,
     any cargo command, tests, formatters, architecture lints, or
     `.ai/dispatch.verify.ps1`. Halt; this dispatch is read-only.
   - The audit discovers the harness is absent, unreachable, or
     impossible to understand without source edits. Halt with
     `NEEDS_HUMAN` rather than implementing or repairing it.
   - Q5 would require changing source and docs in the same follow-up
     dispatch. Halt; harness changes and documentation reconciliation
     must stay separable unless a human explicitly widens scope.
   - The smallest follow-up would require a recorder-host measurement
     run on specific hardware. Halt with `NEEDS_HUMAN`; do not fake or
     extrapolate perf evidence.
   - Any tracked file is already dirty in a way that makes the
     read-only audit ambiguous.

   **Scope-preserving halt clause** - the orchestrator's canonical
   verify gate (`.ai/dispatch.verify.ps1`) runs after Claude execute
   even on read-only audits. If verify fails on a target OUTSIDE the
   audit scope (anything beyond `crates/editor-shell/**`,
   `plans/BASELINE.md`, `plans/IMPLEMENTATION.md`, `change.md`,
   `Status.md`, `HANDOFF.md`, directly referenced prior
   `ai_handoffs/` packets, or this dispatch's own `ai_handoffs/`
   packet), the orchestrator may auto-route a CORRECTION packet asking
   the executor to fix the failure. When that happens **the executor
   MUST halt**: write an EXECUTION_REPORT with `EXEC_STATUS: blocked`
   and `STATUS: NEEDS_HUMAN`, do NOT execute the correction. Read-only
   intent is the entire reason this task is in the brief; a
   correction-round source fix to an unrelated code/test failure
   expands a perf-harness reconciliation audit into a source-fix
   dispatch and must become its own ticket.

   **Verbatim review-gate strings** - the autonomous selector MUST
   copy these seven strings, character-for-character, into the filed
   GitHub issue body. No paraphrasing, no substitution, no reflowing.
   A packet that lacks any one of them verbatim is bounced at review:

   ```
   MUST be a read-only render-frame perf-harness reconciliation audit; do not edit source, tests, docs, Cargo.toml, Cargo.lock, workflows, scripts, or existing packets
   MUST produce a 5-question render-frame perf reconciliation answer block covering existing harness scope, stale-or-current deferral text, recorded evidence, invocation/gates, and smallest follow-up
   MUST inspect crates/editor-shell/src/render_frame_e2e_perf.rs, crates/editor-shell/src/lib.rs, crates/editor-shell/src/render_path.rs, crates/editor-shell/src/lifecycle/**, plans/BASELINE.md, plans/IMPLEMENTATION.md, change.md, Status.md, and HANDOFF.md
   MUST NOT run cargo commands, tests, formatters, architecture lints, .ai/dispatch.verify.ps1, or the release-only perf harness
   MUST distinguish committed harness code from committed recorder-host result evidence
   MUST halt with NEEDS_HUMAN if the smallest follow-up requires a recorder-host measurement run before any repository edit is justified
   MUST halt rather than combine harness source changes and documentation reconciliation in one follow-up dispatch
   ```

   **Done-criterion**:
   - One `ISSUE-*_EXEC_*.md` report with the exact
     `## 5-Question Render-Frame Perf Reconciliation Answer Block`
     section and Q1-Q5 headings above.
   - No source, test, doc, Cargo, workflow, lint, schema, script,
     status, or existing handoff packet edits.
   - `git status --short --untracked-files=no` is clean before and
     after writing the EXEC report.
   - Verification claims are read-only only: document the `rg`,
     `git grep`, `git show`, `git log`, and file-read commands used
     for the audit; do not manually run cargo tests, builds, fmt,
     architecture lints, `.ai/dispatch.verify.ps1`, or the release-only
     perf harness. The orchestrator will still run its canonical
     verification gate after execution.
   - Q5 names one smallest next dispatch and includes its proposed
     allowed files, must-not-touch surfaces, verification gates, and
     halt conditions, unless the correct outcome is `NEEDS_HUMAN`.

14. **[DONE 2026-05-23 via PR #113 / commit `7d9d088`] Read-only preflight: remaining io-* format metadata declarations.**
   **NO source edits.** Companion audit to task #13. The #13 EXEC
   packet and its verification log showed the `kernel_isolation` lint
   warns on seven io crates, not only the original four. This dispatch
   audits the remaining detected io crates — `rge-io-obj`,
   `rge-io-audio`, and `rge-io-3mf` — so the eventual manifest fix can
   address the full warning set in one bounded edit.

   **Allowed read-only scope**:
   - MAY read `ai_handoffs/ISSUE-110_EXEC_2026-05-23_10-32-36+0300.md`.
   - MAY read `.ai/dispatch-ISSUE-110/verification.round0.log` if
     present locally, only to confirm the seven warning crates.
   - MAY read `tools/architecture-lints/src/kernel_isolation.rs`.
   - MAY read `tools/architecture-lints/tests/kernel_isolation_test.rs`.
   - MAY read root `Cargo.toml`.
   - MAY read `crates/io-obj/**`, `crates/io-audio/**`, and
     `crates/io-3mf/**`.
   - MAY use read-only `rg`, `git grep`, `git diff`, `git status`,
     and file-read commands. Do not run cargo commands; this is a
     metadata preflight only.

   **Allowed file surface**:
   - MAY add exactly one execution report packet:
     `ai_handoffs/ISSUE-*_EXEC_*.md`, plus its `.meta.json` sidecar
     if produced by `new-handoff.ps1 -Finalize`.

   **Files that MUST NOT be touched**:
   - Any tracked repository file outside this dispatch's own
     `ai_handoffs/` EXEC packet.
   - Any source file, test file, fixture, Cargo manifest,
     `Cargo.lock`, workflow file, script, schema, lint file, doc,
     ADR, `Status.md`, `HANDOFF.md`, `change.md`, or existing handoff
     packet.

   **Five-question remaining io-format metadata preflight answer block**:
   The EXEC report must contain a section titled exactly
   `## 5-Question Remaining Io Formats Metadata Preflight Answer Block`
   and answer exactly these headings:
   - `Q1. What did task #13 prove, and why is this companion audit needed?`
   - `Q2. Are rge-io-obj, rge-io-audio, and rge-io-3mf real lint-detected io crates?`
   - `Q3. What exact format strings should rge-io-obj, rge-io-audio, and rge-io-3mf declare?`
   - `Q4. Are there ownership ambiguities or aliases, including obj/mtl, wav/ogg/oga, mp3/mpeg, flac, and 3mf?`
   - `Q5. What is the smallest safe follow-up dispatch covering all seven io crates?`

   **Acceptance criteria**:
   - Q1 cites task #13's finding that the verify gate warned on seven
     io crates and explains why the four-crate implementation follow-up
     is insufficient.
   - Q2 confirms the three remaining crates are lint-detected by
     package name or manifest path and currently lack
     `package.metadata.rge.formats`.
   - Q3 proposes exact `formats = [...]` arrays for the three
     remaining crates, with evidence from crate manifests and
     crate-level docs.
   - Q4 explicitly resolves or halts on at least these ambiguity
     points: whether OBJ should claim `mtl`, whether OGG should include
     `oga` or `opus`, whether MP3 should include `mpeg`, and whether
     3MF has any extension alias beyond `3mf`.
   - Q5 names exactly one smallest safe follow-up manifest-only
     dispatch covering all seven io crates from task #13 plus this
     audit. If exact metadata cannot be chosen without a policy
     decision, recommend `NEEDS_HUMAN` instead.

   **Halt conditions**:
   - The audit cannot identify exact arrays for `rge-io-obj`,
     `rge-io-audio`, and `rge-io-3mf` from current manifests/docs.
     Halt with `NEEDS_HUMAN`; do not guess.
   - The audit finds an unavoidable ownership conflict between any of
     the three remaining crates and the four arrays proposed by task
     #13. Halt with `NEEDS_HUMAN`.
   - The audit cannot decide whether aliases such as `mtl`, `oga`,
     `opus`, or `mpeg` should be declared separately. Halt with
     `NEEDS_HUMAN` rather than guessing.
   - Answering Q1-Q5 requires editing source, Cargo metadata, lints,
     docs, workflows, or tests. Halt; this dispatch is read-only.
   - The audit cannot be answered without running local cargo
     commands, tests, formatters, architecture lints, or
     `.ai/dispatch.verify.ps1`. Halt; this is documentary preflight
     only.
   - Any tracked file is already dirty in a way that makes the
     read-only audit ambiguous.

   **Scope-preserving halt clause** - the orchestrator's canonical
   verify gate (`.ai/dispatch.verify.ps1`) runs after Claude execute
   even on read-only audits. If verify fails on a target OUTSIDE the
   audit scope (anything beyond task #13's EXEC packet, the local
   task #13 verification log if present, `tools/architecture-lints/src/
   kernel_isolation.rs`, `tools/architecture-lints/tests/
   kernel_isolation_test.rs`, root `Cargo.toml`, `crates/io-obj/**`,
   `crates/io-audio/**`, `crates/io-3mf/**`, or this dispatch's own
   `ai_handoffs/` packet), the orchestrator may auto-route a
   CORRECTION packet asking the executor to fix the failure. When that
   happens **the executor MUST halt**: write an EXECUTION_REPORT with
   `EXEC_STATUS: blocked` and `STATUS: NEEDS_HUMAN`, do NOT execute
   the correction. Read-only intent is the entire reason this task is
   in the brief; a correction-round source fix to an unrelated
   code/test failure expands an io-format metadata audit into a
   source-fix dispatch and must become its own ticket.

   **Verbatim review-gate strings** - the autonomous selector MUST
   copy these seven strings, character-for-character, into the filed
   GitHub issue body. No paraphrasing, no substitution, no reflowing.
   A packet that lacks any one of them verbatim is bounced at review:

   ```
   MUST be a read-only remaining-io-format metadata preflight; do not edit source, tests, docs, Cargo.toml, Cargo.lock, workflows, scripts, lints, or existing packets
   MUST produce a 5-question remaining io formats metadata preflight answer block covering task #13 findings, lint detection, exact three-crate format strings, ownership ambiguities, and a seven-crate follow-up
   MUST inspect task #13's EXEC packet, tools/architecture-lints/src/kernel_isolation.rs, tools/architecture-lints/tests/kernel_isolation_test.rs, root Cargo.toml, crates/io-obj/**, crates/io-audio/**, and crates/io-3mf/**
   MUST identify exact formats arrays for rge-io-obj, rge-io-audio, and rge-io-3mf or halt with NEEDS_HUMAN
   MUST explicitly resolve or halt on aliases including obj/mtl, wav/ogg/oga, mp3/mpeg, flac, and 3mf
   MUST NOT run local cargo commands, tests, formatters, architecture lints, or .ai/dispatch.verify.ps1
   MUST halt rather than edit any Cargo.toml in this dispatch
   ```

   **Done-criterion**:
   - One `ISSUE-*_EXEC_*.md` report with the exact
     `## 5-Question Remaining Io Formats Metadata Preflight Answer Block`
     section and Q1-Q5 headings above.
   - No source, test, doc, Cargo, workflow, lint, schema, script,
     status, or existing handoff packet edits.
   - `git status --short --untracked-files=no` is clean before and
     after writing the EXEC report.
   - Verification claims are read-only only: document the `rg`,
     `git grep`, and file-read commands used for the audit; do not
     manually run cargo tests, builds, fmt, architecture lints, or
     `.ai/dispatch.verify.ps1`. The orchestrator will still run its
     canonical verification gate after execution.
   - Q5 names one smallest next dispatch covering all seven io crates
     and includes its proposed allowed files, must-not-touch surfaces,
     verification gates, and halt conditions, unless the correct
     outcome is `NEEDS_HUMAN`.

15. **[DONE 2026-05-23 via PR #115 / commit `1c3e16c`] Add seven `package.metadata.rge.formats` declarations to io manifests.**
   Manifest-only fix following task #13 and task #14. Add the
   `kernel_isolation` ownership metadata blocks to all seven workspace
   io crates so the architecture lint no longer emits missing-metadata
   warnings for format owners.

   **Allowed file surface**:
   - EDIT `crates/io-gltf/Cargo.toml`.
   - EDIT `crates/io-image/Cargo.toml`.
   - EDIT `crates/io-step/Cargo.toml`.
   - EDIT `crates/io-stl/Cargo.toml`.
   - EDIT `crates/io-obj/Cargo.toml`.
   - EDIT `crates/io-audio/Cargo.toml`.
   - EDIT `crates/io-3mf/Cargo.toml`.
   - MAY add this dispatch's own `ai_handoffs/ISSUE-*_EXEC_*.md`
     packet plus `.meta.json` sidecar if produced by the orchestrator.

   **Required metadata blocks**:
   Add exactly these TOML blocks, preserving each array's string order:

   ```toml
   # crates/io-gltf/Cargo.toml
   [package.metadata.rge]
   formats = ["gltf", "glb"]

   # crates/io-image/Cargo.toml
   [package.metadata.rge]
   formats = ["png", "jpg", "jpeg", "exr", "hdr"]

   # crates/io-step/Cargo.toml
   [package.metadata.rge]
   formats = ["step", "stp", "iges", "igs"]

   # crates/io-stl/Cargo.toml
   [package.metadata.rge]
   formats = ["stl"]

   # crates/io-obj/Cargo.toml
   [package.metadata.rge]
   formats = ["obj", "mtl"]

   # crates/io-audio/Cargo.toml
   [package.metadata.rge]
   formats = ["wav", "ogg", "oga", "flac", "mp3", "mpeg"]

   # crates/io-3mf/Cargo.toml
   [package.metadata.rge]
   formats = ["3mf"]
   ```

   **Files that MUST NOT be touched**:
   - Any file outside the seven Cargo manifests listed above, except
     this dispatch's own `ai_handoffs/` packet.
   - `Cargo.lock`, root `Cargo.toml`, source files, tests, fixtures,
     workflow files, scripts, lint implementation files, docs, ADRs,
     `Status.md`, `HANDOFF.md`, `change.md`, and existing handoff
     packets.

   **Cargo.lock policy**:
   - Zero lockfile changes. Manifest package metadata must not affect
     dependency resolution. If `Cargo.lock` changes at all, halt with
     `NEEDS_HUMAN`.

   **Halt conditions**:
   - Any of the seven target manifests already contains a
     `[package.metadata.rge]` section or `formats = [...]` entry that
     would require merging or rewriting existing metadata. Halt and
     report the existing section; do not guess how to merge.
   - Adding the exact arrays above causes the `kernel-isolation` lint
     to report an overlap violation. Halt with `NEEDS_HUMAN`; do not
     modify the lint or remove aliases to make the violation disappear.
   - `Cargo.lock`, root `Cargo.toml`, `tools/architecture-lints/**`, or
     any source/test/doc/workflow/script file changes. Halt rather than
     clean up unrelated changes.
   - The implementation appears to require alias normalization,
     metadata schema changes, or edits to the lint implementation.
     Halt; this task is manifest-only.

   **Verbatim review-gate strings** - the autonomous selector MUST
   copy these seven strings, character-for-character, into the filed
   GitHub issue body. No paraphrasing, no substitution, no reflowing.
   A packet that lacks any one of them verbatim is bounced at review:

   ```
   MUST add package.metadata.rge.formats blocks to exactly seven io Cargo.toml files: io-gltf, io-image, io-step, io-stl, io-obj, io-audio, and io-3mf
   MUST use exactly the arrays from tasks #13 and #14, including obj/mtl, wav/ogg/oga/flac/mp3/mpeg, and 3mf
   MUST NOT modify root Cargo.toml, Cargo.lock, tools/architecture-lints/**, source files, tests, docs, workflows, scripts, or existing packets
   MUST NOT change alias policy, normalize format strings, or edit the kernel-isolation lint
   MUST halt if any target manifest already has package.metadata.rge metadata that would require merging
   MUST halt if cargo run -q -p rge-tool-architecture-lints -- kernel-isolation reports any overlap violation
   MUST run .ai/dispatch.verify.ps1 and require it to exit 0
   ```

   **Done-criterion**:
   - All seven listed `Cargo.toml` files contain exactly one
     `[package.metadata.rge]` section with the required `formats`
     array.
   - `cargo run -q -p rge-tool-architecture-lints -- kernel-isolation`
     exits 0 and emits no `missing package.metadata.rge.formats`
     warning for any `rge-io-*` crate.
   - `.ai/dispatch.verify.ps1` exits 0.
   - `Cargo.lock` is unchanged.
   - Diff stat is limited to the seven target `Cargo.toml` files plus
     this dispatch's own `ai_handoffs/` packet. Zero root Cargo,
     source, test, workflow, script, lint, status, doc, or existing
     packet edits.

12. **[DONE 2026-05-23 via PR #109 / commit `ba90b04`] Read-only preflight: CommandBus integration context for editor user actions.**
   **NO source edits.** Audit the smallest safe design shape for
   connecting `editor-actions::CommandBus` to real editor-shell user
   input without breaking its current `World`-only action contract or
   faking a visible CAD command. This follows the Phase 9
   editor-usability preflight recorded in `plans/BASELINE.md` and
   `change.md`: `CommandBus::submit` is implemented and tested, but
   no editor-shell / rge-editor user path currently drives it.

   **Allowed read-only scope**:
   - MAY read `plans/BASELINE.md` and `change.md` entries related to
     Phase 9 editor usability and CommandBus integration.
   - MAY read `crates/editor-actions/**`.
   - MAY read `crates/editor-shell/**`.
   - MAY read `editor/rge-editor/**`.
   - MAY read `crates/editor-state/**`.
   - MAY read `crates/cad-core/**` and `crates/cad-projection/**`
     only to understand the current `CadGraph` / `CadProjection`
     ownership boundary for visible CAD scene mutations.
   - MAY read `kernel/ecs/**` only to understand the `World` mutation
     surface that `Action::apply` currently accepts.
   - MAY read relevant crate `Cargo.toml` files and architecture-lint
     configuration only to reason about dep edges and lint
     implications.
   - MAY use read-only `rg`, `git grep`, `git diff`, and file-read
     commands. Do not run cargo commands; this is design preflight
     only.

   **Allowed file surface**:
   - MAY add exactly one execution report packet:
     `ai_handoffs/ISSUE-*_EXEC_*.md`, plus its `.meta.json` sidecar
     if produced by `new-handoff.ps1 -Finalize`.

   **Files that MUST NOT be touched**:
   - Any tracked repository file outside this dispatch's own
     `ai_handoffs/` EXEC packet.
   - Any source file, test file, fixture, Cargo manifest,
     `Cargo.lock`, workflow file, script, schema, lint file, doc,
     ADR, `Status.md`, `HANDOFF.md`, `change.md`, or existing handoff
     packet.

   **Five-question CommandBus preflight answer block**:
   The EXEC report must contain a section titled exactly
   `## 5-Question CommandBus Preflight Answer Block` and answer
   exactly these headings:
   - `Q1. What is CommandBus's current public contract and where is it tested?`
   - `Q2. Which editor input paths could realistically dispatch commands first, and which are out of scope?`
   - `Q3. Should the command context stay World-only, grow Action::apply/revert, or add an editor-specific adapter/context layer?`
   - `Q4. How would undo, audit-ledger recording, CadGraph/CadProjection mutation, and render refresh compose for the first visible command?`
   - `Q5. What is the smallest safe follow-up dispatch?`

   **Acceptance criteria**:
   - Q1 cites the current `CommandBus::submit`, undo/redo/coalescing,
     `Action::apply` / `Action::revert`, and relevant test coverage.
   - Q2 compares at least three candidate first user paths, including
     a visible CAD mutation, a non-CAD editor shortcut, and a
     no-op/menu-only path; it must reject any path that would be sham
     progress.
   - Q3 compares at least three context shapes: keep `World`-only,
     widen `Action` to a richer editor context, or add an adapter
     layer around `CommandBus` without changing the trait.
   - Q4 explains how the chosen or rejected shapes affect undo,
     audit-ledger semantics, `CadGraph`, `CadProjection`, `World`, and
     render refresh.
   - Q5 names exactly one smallest safe follow-up with proposed
     allowed files, must-not-touch surfaces, verification gates, and
     halt conditions. If no autonomous-friendly follow-up exists,
     recommend `NEEDS_HUMAN` instead.

   **Halt conditions**:
   - The audit discovers a production editor-shell / rge-editor user
     path already calls `CommandBus::submit`. Halt with
     `NEEDS_HUMAN`; this preflight's premise is stale.
   - The audit discovers `CommandBus` or `Action` no longer exists or
     no longer has a `World`-only apply/revert surface. Halt with
     `NEEDS_HUMAN`; the premise is stale.
   - Answering Q1-Q5 requires editing source, Cargo metadata, lints,
     docs, workflows, or tests. Halt; this dispatch is read-only.
   - Q5 would require changing the `Action` trait and wiring a
     user-visible command in the same follow-up dispatch. Halt;
     context design and visible command wiring must stay separable
     unless a human explicitly widens scope.
   - The audit cannot be answered without running local cargo
     commands, tests, formatters, architecture lints, or
     `.ai/dispatch.verify.ps1`. Halt; this is documentary design
     preflight only.
   - Any tracked file is already dirty in a way that makes the
     read-only audit ambiguous.

   **Scope-preserving halt clause** - the orchestrator's canonical
   verify gate (`.ai/dispatch.verify.ps1`) runs after Claude execute
   even on read-only audits. If verify fails on a target OUTSIDE the
   audit scope (anything beyond `crates/editor-actions/**`,
   `crates/editor-shell/**`, `editor/rge-editor/**`,
   `crates/editor-state/**`, `crates/cad-core/**`,
   `crates/cad-projection/**`, `kernel/ecs/**`, relevant manifests,
   architecture-lint config, `plans/BASELINE.md`, `change.md`, or
   this dispatch's own `ai_handoffs/` packet), the orchestrator may
   auto-route a CORRECTION packet asking the executor to fix the
   failure. When that happens **the executor MUST halt**: write an
   EXECUTION_REPORT with `EXEC_STATUS: blocked` and
   `STATUS: NEEDS_HUMAN`, do NOT execute the correction. Read-only
   intent is the entire reason this task is in the brief; a
   correction-round source fix to an unrelated code/test failure
   expands a CommandBus audit into a source-fix dispatch and must
   become its own ticket.

   **Verbatim review-gate strings** - the autonomous selector MUST
   copy these seven strings, character-for-character, into the filed
   GitHub issue body. No paraphrasing, no substitution, no reflowing.
   A packet that lacks any one of them verbatim is bounced at review:

   ```
   MUST be a read-only CommandBus integration preflight; do not edit source, tests, docs, Cargo.toml, Cargo.lock, workflows, scripts, or existing packets
   MUST produce a 5-question CommandBus preflight answer block covering current contract, first input paths, command-context shape, undo/audit/render composition, and smallest follow-up
   MUST inspect crates/editor-actions/**, crates/editor-shell/**, editor/rge-editor/**, crates/editor-state/**, crates/cad-core/**, crates/cad-projection/**, and kernel/ecs/**
   MUST compare World-only Action, widened editor-context Action, and adapter-layer approaches before naming a follow-up
   MUST halt if a production editor user path already calls CommandBus::submit or if CommandBus no longer has a World-only Action surface
   MUST NOT run local cargo commands, tests, formatters, architecture lints, or .ai/dispatch.verify.ps1
   MUST halt rather than combine command-context design and visible command wiring into one follow-up dispatch
   ```

   **Done-criterion**:
   - One `ISSUE-*_EXEC_*.md` report with the exact
     `## 5-Question CommandBus Preflight Answer Block` section and
     Q1-Q5 headings above.
   - No source, test, doc, Cargo, workflow, lint, schema, script,
     status, or existing handoff packet edits.
   - `git status --short --untracked-files=no` is clean before and
     after writing the EXEC report.
   - Verification claims are read-only only: document the `rg`,
     `git grep`, and file-read commands used for the audit; do not
     manually run cargo tests, builds, fmt, architecture lints, or
     `.ai/dispatch.verify.ps1`. The orchestrator will still run its
     canonical verification gate after execution.
   - Q5 names one smallest next dispatch and includes its proposed
     allowed files, must-not-touch surfaces, verification gates, and
     halt conditions, unless the correct outcome is `NEEDS_HUMAN`.

13. **[DONE 2026-05-23 via PR #111 / commit `4258991`] Read-only preflight: io-* `package.metadata.rge.formats` declarations.**
   **NO source edits.** Audit the `kernel_isolation` architecture-lint
   warning for io-format ownership metadata before any Cargo manifest
   changes are made. Task #9/#10 verify output showed four real
   workspace io crates warning that they lack
   `package.metadata.rge.formats`: `rge-io-gltf`, `rge-io-image`,
   `rge-io-step`, and `rge-io-stl`. This dispatch determines the
   canonical metadata shape and the exact format strings each crate
   should declare, then scopes the smallest safe follow-up manifest
   edit.

   **Allowed read-only scope**:
   - MAY read `tools/architecture-lints/src/kernel_isolation.rs`.
   - MAY read `tools/architecture-lints/tests/kernel_isolation_test.rs`.
   - MAY read `tools/architecture-lints/src/main.rs` only for the
     lint name / grouping.
   - MAY read root `Cargo.toml`.
   - MAY read `crates/io-gltf/**`, `crates/io-image/**`,
     `crates/io-step/**`, and `crates/io-stl/**`.
   - MAY read `plans/PLAN.md` / `plans/BASELINE.md` references to
     PLAN section 1.6.4 / one-import-path-per-format only if needed
     to interpret the lint's doctrine.
   - MAY use read-only `rg`, `git grep`, `git diff`, `git status`,
     and file-read commands. Do not run cargo commands; this is a
     metadata preflight only.

   **Allowed file surface**:
   - MAY add exactly one execution report packet:
     `ai_handoffs/ISSUE-*_EXEC_*.md`, plus its `.meta.json` sidecar
     if produced by `new-handoff.ps1 -Finalize`.

   **Files that MUST NOT be touched**:
   - Any tracked repository file outside this dispatch's own
     `ai_handoffs/` EXEC packet.
   - Any source file, test file, fixture, Cargo manifest,
     `Cargo.lock`, workflow file, script, schema, lint file, doc,
     ADR, `Status.md`, `HANDOFF.md`, `change.md`, or existing handoff
     packet.

   **Five-question io-format metadata preflight answer block**:
   The EXEC report must contain a section titled exactly
   `## 5-Question Io Formats Metadata Preflight Answer Block` and
   answer exactly these headings:
   - `Q1. What metadata shape does kernel_isolation expect, and how does it treat missing metadata?`
   - `Q2. Which workspace io-* crates are detected by the lint today, and which already declare formats?`
   - `Q3. What exact format strings should rge-io-gltf, rge-io-image, rge-io-step, and rge-io-stl declare?`
   - `Q4. Are there ambiguous ownership cases or overlaps, including embedded glTF images and image extension aliases?`
   - `Q5. What is the smallest safe follow-up dispatch?`

   **Acceptance criteria**:
   - Q1 quotes or paraphrases the canonical TOML shape and confirms
     strings are lowercase extension names with no leading dot.
   - Q2 enumerates every current workspace crate that the lint will
     classify as an io crate by package name or manifest path.
   - Q3 proposes exact `formats = [...]` arrays for each of the four
     warning crates, with evidence from crate docs, descriptions,
     format detectors, or public APIs.
   - Q4 explicitly handles at least these ambiguity points:
     `gltf` vs `glb`, `jpg` vs `jpeg`, `tif`/`tiff` if present,
     `step` vs `stp`, `iges` vs `igs`, and glTF embedded raster
     images owned by `rge-io-image`.
   - Q5 names exactly one smallest safe follow-up with proposed
     allowed files, must-not-touch surfaces, verification gates, and
     halt conditions. If exact metadata cannot be chosen without a
     policy decision, recommend `NEEDS_HUMAN` instead.

   **Halt conditions**:
   - The audit discovers any crate besides `rge-io-gltf`,
     `rge-io-image`, `rge-io-step`, or `rge-io-stl` is classified as
     an io crate by the lint and needs metadata. Halt with
     `NEEDS_HUMAN`; the assumed four-crate follow-up scope is stale.
   - The audit discovers an unavoidable overlap where two io crates
     should claim the same extension. Halt with `NEEDS_HUMAN`; do not
     paper over ownership conflict in the follow-up.
   - The audit cannot decide whether aliases such as `jpg`/`jpeg`,
     `stp`/`step`, or `igs`/`iges` should be declared separately.
     Halt with `NEEDS_HUMAN` rather than guessing.
   - Answering Q1-Q5 requires editing source, Cargo metadata, lints,
     docs, workflows, or tests. Halt; this dispatch is read-only.
   - The audit cannot be answered without running local cargo
     commands, tests, formatters, architecture lints, or
     `.ai/dispatch.verify.ps1`. Halt; this is documentary preflight
     only.
   - Any tracked file is already dirty in a way that makes the
     read-only audit ambiguous.

   **Scope-preserving halt clause** - the orchestrator's canonical
   verify gate (`.ai/dispatch.verify.ps1`) runs after Claude execute
   even on read-only audits. If verify fails on a target OUTSIDE the
   audit scope (anything beyond `tools/architecture-lints/src/
   kernel_isolation.rs`, `tools/architecture-lints/tests/
   kernel_isolation_test.rs`, `tools/architecture-lints/src/main.rs`,
   root `Cargo.toml`, `crates/io-gltf/**`, `crates/io-image/**`,
   `crates/io-step/**`, `crates/io-stl/**`, relevant PLAN/BASELINE
   references, or this dispatch's own `ai_handoffs/` packet), the
   orchestrator may auto-route a CORRECTION packet asking the executor
   to fix the failure. When that happens **the executor MUST halt**:
   write an EXECUTION_REPORT with `EXEC_STATUS: blocked` and
   `STATUS: NEEDS_HUMAN`, do NOT execute the correction. Read-only
   intent is the entire reason this task is in the brief; a
   correction-round source fix to an unrelated code/test failure
   expands an io-format metadata audit into a source-fix dispatch and
   must become its own ticket.

   **Verbatim review-gate strings** - the autonomous selector MUST
   copy these seven strings, character-for-character, into the filed
   GitHub issue body. No paraphrasing, no substitution, no reflowing.
   A packet that lacks any one of them verbatim is bounced at review:

   ```
   MUST be a read-only io-format metadata preflight; do not edit source, tests, docs, Cargo.toml, Cargo.lock, workflows, scripts, lints, or existing packets
   MUST produce a 5-question io formats metadata preflight answer block covering lint metadata shape, detected io crates, exact four-crate format strings, ownership ambiguities, and smallest follow-up
   MUST inspect tools/architecture-lints/src/kernel_isolation.rs, tools/architecture-lints/tests/kernel_isolation_test.rs, root Cargo.toml, crates/io-gltf/**, crates/io-image/**, crates/io-step/**, and crates/io-stl/**
   MUST identify exact formats arrays for rge-io-gltf, rge-io-image, rge-io-step, and rge-io-stl or halt with NEEDS_HUMAN
   MUST explicitly resolve or halt on aliases including jpg/jpeg, step/stp, iges/igs, and gltf/glb embedded raster ownership
   MUST NOT run local cargo commands, tests, formatters, architecture lints, or .ai/dispatch.verify.ps1
   MUST halt rather than edit any Cargo.toml in this dispatch
   ```

   **Done-criterion**:
   - One `ISSUE-*_EXEC_*.md` report with the exact
     `## 5-Question Io Formats Metadata Preflight Answer Block`
     section and Q1-Q5 headings above.
   - No source, test, doc, Cargo, workflow, lint, schema, script,
     status, or existing handoff packet edits.
   - `git status --short --untracked-files=no` is clean before and
     after writing the EXEC report.
   - Verification claims are read-only only: document the `rg`,
     `git grep`, and file-read commands used for the audit; do not
     manually run cargo tests, builds, fmt, architecture lints, or
     `.ai/dispatch.verify.ps1`. The orchestrator will still run its
     canonical verification gate after execution.
   - Q5 names one smallest next dispatch and includes its proposed
     allowed files, must-not-touch surfaces, verification gates, and
     halt conditions, unless the correct outcome is `NEEDS_HUMAN`.

9. **[DONE 2026-05-23 via PR #103 / commit `4fa1e60`] Add `bench.yml` parity to `.ai/dispatch.verify.ps1`.**
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

10. **[DONE 2026-05-23 via PR #105 / commit `7ca7895`] Delete dead `rge-io-image` asset-store stub.**
   Source-cleanup dispatch, pre-audited by the #98 / #99 read-only
   `rge-io-image` cache-surface preflight. That audit found
   `crates/io-image/src/asset_store_stub.rs` is reachable only as a
   public module declaration and has zero in-tree consumers. W16's
   real asset-store cache substrate now exists, and keeping this
   aspirational stub creates misleading API surface.

   **Allowed file surface**:
   - DELETE `crates/io-image/src/asset_store_stub.rs`.
   - EDIT `crates/io-image/src/lib.rs` only to remove
     `pub mod asset_store_stub;`.
   - MAY add this dispatch's own `ai_handoffs/ISSUE-*_EXEC_*.md`
     packet plus `.meta.json` sidecar if produced by the orchestrator.

   **Files that MUST NOT be touched**:
   - Any file outside the two `crates/io-image/src/**` files named
     above, except this dispatch's own `ai_handoffs/` packet.
   - `crates/asset-store/**`, `crates/io-gltf/**`, `editor/**`,
     `crates/editor-shell/**`, `crates/gfx/**`, `kernel/**`,
     `.github/**`, `Cargo.toml`, `Cargo.lock`, docs, ADRs, status
     files, existing handoff packets, and automation scripts.

   **Cargo.lock policy**:
   - Zero Cargo metadata changes. If `Cargo.toml` or `Cargo.lock`
     changes at all, halt with `NEEDS_HUMAN`.

   **Halt conditions**:
   - Any current in-tree Rust code imports or references
     `rge_io_image::asset_store_stub`, `crate::asset_store_stub`, its
     `Cache`, `MemoryCache`, or `AssetId` symbols. Halt and report the
     consumers; do not migrate them in this dispatch.
   - Removing the public module declaration causes compile failures
     outside `rge-io-image` itself. Halt; that means the #98 audit's
     reachability finding is stale.
   - The deletion appears to require replacing the stub with a real
     `rge-asset-store` adapter, adding a dependency, or changing
     image-loading/cache behavior. Halt; this task is deletion only.
   - Any tracked file outside `crates/io-image/src/lib.rs` and
     `crates/io-image/src/asset_store_stub.rs` shows a diff after
     execution, except this dispatch's own `ai_handoffs/` packet. Halt
     rather than clean up unrelated changes.

   **Verbatim review-gate strings** - the autonomous selector MUST
   copy these six strings, character-for-character, into the filed
   GitHub issue body. No paraphrasing, no substitution, no reflowing.
   A packet that lacks any one of them verbatim is bounced at review:

   ```
   MUST delete crates/io-image/src/asset_store_stub.rs
   MUST remove pub mod asset_store_stub; from crates/io-image/src/lib.rs
   MUST NOT modify any file outside crates/io-image/src/lib.rs and crates/io-image/src/asset_store_stub.rs (except the dispatch's own ai_handoffs/ packet)
   MUST NOT add or modify any dependency, Cargo.toml, or Cargo.lock
   MUST halt if any in-tree Rust code still references rge_io_image::asset_store_stub, crate::asset_store_stub, Cache, MemoryCache, or AssetId from that stub
   MUST halt rather than replace the stub with a real asset-store adapter or change image loading/cache behavior
   ```

   **Done-criterion**:
   - `crates/io-image/src/asset_store_stub.rs` is deleted.
   - `crates/io-image/src/lib.rs` no longer declares
     `pub mod asset_store_stub;`.
   - A repo-wide search for `asset_store_stub` finds no Rust-source
     references outside the dispatch packet.
   - `cargo test -p rge-io-image --all-targets --no-fail-fast`
     exits 0.
   - `.ai/dispatch.verify.ps1` exits 0.
   - Diff stat is limited to the deleted stub, the one-line module
     removal in `crates/io-image/src/lib.rs`, and this dispatch's own
     `ai_handoffs/` packet. Zero Cargo, workflow, source-crate,
     status, doc, or automation edits elsewhere.

11. **[DONE 2026-05-23 via PR #107 / commit `2cf5619`] Read-only preflight: egui host integration shape for the editor.**
   **NO source edits.** Audit the smallest safe design shape for adding
   a real egui host to the editor so the already-landed editor-ui
   widgets, dock state, and inspector surface can become reachable
   without forcing a premature implementation. This follows the
   Phase 9 live-inspector preflight recorded in `plans/BASELINE.md`
   and `change.md`: `editor-ui` has egui widgets, but no production
   host currently constructs `egui::Context`, `egui_winit::State`, or
   `egui_wgpu::Renderer`.

   **Allowed read-only scope**:
   - MAY read `plans/BASELINE.md` and `change.md` entries related to
     Phase 9 editor usability and live-inspector preflights.
   - MAY read `editor/rge-editor/**`.
   - MAY read `crates/editor-shell/**`.
   - MAY read `crates/editor-ui/**`.
   - MAY read `crates/editor-state/**`.
   - MAY read `crates/editor-actions/**`.
   - MAY read root `Cargo.toml`, relevant crate `Cargo.toml` files,
     and architecture-lint configuration only to reason about dep
     edges and lint implications.
   - MAY use read-only `rg`, `git grep`, `git diff`, and file-read
     commands. Do not run cargo commands; this is design preflight
     only.

   **Allowed file surface**:
   - MAY add exactly one execution report packet:
     `ai_handoffs/ISSUE-*_EXEC_*.md`, plus its `.meta.json` sidecar
     if produced by `new-handoff.ps1 -Finalize`.

   **Files that MUST NOT be touched**:
   - Any tracked repository file outside this dispatch's own
     `ai_handoffs/` EXEC packet.
   - Any source file, test file, fixture, Cargo manifest,
     `Cargo.lock`, workflow file, script, schema, lint file, doc,
     ADR, `Status.md`, `HANDOFF.md`, `change.md`, or existing handoff
     packet.

   **Five-question egui host preflight answer block**:
   The EXEC report must contain a section titled exactly
   `## 5-Question Egui Host Preflight Answer Block` and answer exactly
   these headings:
   - `Q1. Where should the egui host live: editor-shell, a new editor-egui-host crate, or the rge-editor binary?`
   - `Q2. How should egui_winit input routing compose with existing editor-shell keyboard and mouse handling?`
   - `Q3. How should egui_wgpu rendering compose with the current cuboid, depth, and selection-highlight render path?`
   - `Q4. Who should own DockState, TabBody construction, and inspector snapshot delivery once the host exists?`
   - `Q5. What is the smallest safe follow-up dispatch?`

   **Acceptance criteria**:
   - Q1 compares all three placement options and includes dep-edge /
     forbidden-dep / editor-state-ownership lint implications for each.
   - Q2 cites the current `WindowEvent` / keyboard / cursor / mouse
     handling code paths and identifies which events egui must consume
     before editor-shell sees them.
   - Q3 cites the current render-frame path and decides whether the
     first host should share the existing encoder/pass, use a second
     pass, or stay binary-only until render composition is explicit.
   - Q4 compares at least two snapshot-delivery mechanisms, including
     the existing render-handoff style precedent if applicable.
   - Q5 names exactly one smallest safe follow-up with proposed
     allowed files, must-not-touch surfaces, verification gates, and
     halt conditions. If no autonomous-friendly follow-up exists,
     recommend `NEEDS_HUMAN` instead.

   **Halt conditions**:
   - The audit discovers a production egui host already exists. Halt
     with `NEEDS_HUMAN`; this preflight's premise is stale.
   - Answering Q1-Q5 requires editing source, Cargo metadata, lints,
     docs, workflows, or tests. Halt; this dispatch is read-only.
   - Q5 would require adding an egui host and inspector wiring in the
     same follow-up dispatch. Halt; host substrate and consumer wiring
     must stay separable unless a human explicitly widens scope.
   - The audit cannot be answered without running local cargo
     commands, tests, formatters, architecture lints, or
     `.ai/dispatch.verify.ps1`. Halt; this is documentary design
     preflight only.
   - Any tracked file is already dirty in a way that makes the
     read-only audit ambiguous.

   **Scope-preserving halt clause** - the orchestrator's canonical
   verify gate (`.ai/dispatch.verify.ps1`) runs after Claude execute
   even on read-only audits. If verify fails on a target OUTSIDE the
   audit scope (anything beyond `editor/rge-editor/**`,
   `crates/editor-shell/**`, `crates/editor-ui/**`,
   `crates/editor-state/**`, `crates/editor-actions/**`, root
   `Cargo.toml`, relevant crate manifests, architecture-lint config,
   `plans/BASELINE.md`, `change.md`, or this dispatch's own
   `ai_handoffs/` packet), the orchestrator may auto-route a
   CORRECTION packet asking the executor to fix the failure. When that
   happens **the executor MUST halt**: write an EXECUTION_REPORT with
   `EXEC_STATUS: blocked` and `STATUS: NEEDS_HUMAN`, do NOT execute
   the correction. Read-only intent is the entire reason this task is
   in the brief; a correction-round source fix to an unrelated
   code/test failure expands an egui-host audit into a source-fix
   dispatch and must become its own ticket.

   **Verbatim review-gate strings** - the autonomous selector MUST
   copy these seven strings, character-for-character, into the filed
   GitHub issue body. No paraphrasing, no substitution, no reflowing.
   A packet that lacks any one of them verbatim is bounced at review:

   ```
   MUST be a read-only egui host design preflight; do not edit source, tests, docs, Cargo.toml, Cargo.lock, workflows, scripts, or existing packets
   MUST produce a 5-question egui host preflight answer block covering host placement, input routing, render composition, DockState/snapshot ownership, and smallest follow-up
   MUST inspect editor/rge-editor/**, crates/editor-shell/**, crates/editor-ui/**, crates/editor-state/**, and crates/editor-actions/**
   MUST compare editor-shell vs new editor-egui-host crate vs rge-editor binary placement, including dep-edge and architecture-lint implications
   MUST halt if a production egui host already exists or if answering the audit requires source/Cargo edits
   MUST NOT run local cargo commands, tests, formatters, architecture lints, or .ai/dispatch.verify.ps1
   MUST halt rather than combine egui host substrate and inspector-tab wiring into one follow-up dispatch
   ```

   **Done-criterion**:
   - One `ISSUE-*_EXEC_*.md` report with the exact
     `## 5-Question Egui Host Preflight Answer Block` section and
     Q1-Q5 headings above.
   - No source, test, doc, Cargo, workflow, lint, schema, script,
     status, or existing handoff packet edits.
   - `git status --short --untracked-files=no` is clean before and
     after writing the EXEC report.
   - Verification claims are read-only only: document the `rg`,
     `git grep`, and file-read commands used for the audit; do not
     manually run cargo tests, builds, fmt, architecture lints, or
     `.ai/dispatch.verify.ps1`. The orchestrator will still run its
     canonical verification gate after execution.
   - Q5 names one smallest next dispatch and includes its proposed
     allowed files, must-not-touch surfaces, verification gates, and
     halt conditions, unless the correct outcome is `NEEDS_HUMAN`.

23. **[DONE 2026-05-23 via PR #131 / commit `381d25e`] Read-only cap-v2 / `ai-auto` label-aging audit.**
   The autonomous cap circuit (`-MaxAutonomousTasks` in
   `Invoke-AiDispatchAuto.ps1`) currently counts every `ai-auto`-
   labelled issue regardless of age or state. After 100 dispatches the
   cap was hit (see ISSUE-128 cap-stop-state audit, 2026-05-23, PR
   #129), forcing a policy decision: lifetime cap vs rolling-window
   with age-based label cleanup. This task is the read-only audit
   that produces the recommendation; the actual implementation (if
   any) is a separate bounded task surfaced via Q5.

   The audit must NOT change any script, doctrine, label state, or
   GitHub issue; it produces a single EXEC packet with a 5-question
   answer block and one Q5 follow-up dispatch proposal (or
   `NEEDS_HUMAN` if the answer requires architecture-tier arbitration).

   **Runtime invocation note**: this task is a deliberate named +1 on
   top of the freeze-at-100 posture. Run as
   `.\Invoke-AiDispatchAuto.ps1 -PublishMode branch -MaxAutonomousTasks 101`
   so the cap accommodates exactly this one dispatch. The script
   `ValidateRange(1, 200)` was widened in commit `e7104c0` to permit
   the +1 without re-registering the scheduler; the scheduler remains
   disabled and its persisted argument is unchanged.

   **Allowed file surface**:
   - INSPECT (read-only) `Invoke-AiDispatchAuto.ps1` (the cap-counting
     code path)
   - INSPECT (read-only) `Register-AiDispatchSchedule.ps1` (cap
     argument plumbing)
   - INSPECT (read-only) `AI_DISPATCH_AUTOMATION.md` (cap doctrine,
     esp. §17.4)
   - INSPECT (read-only) `.ai/dispatch.tasks.md` (task selection
     brief)
   - INSPECT (read-only) GitHub `ai-auto`-labelled issues via
     `gh issue list --label ai-auto --state all` and
     `gh api repos/RustCADs/RGE/issues?labels=ai-auto&state=all`
   - MAY add this dispatch's own `ai_handoffs/ISSUE-*_EXEC_*.md`
     packet plus `.meta.json` sidecar if produced by the orchestrator.

   **Files that MUST NOT be touched**:
   - Any `.ps1` script, including the two scripts inspected
   - Any `.md` doctrine, including `AI_DISPATCH_AUTOMATION.md` and
     `.ai/dispatch.tasks.md`
   - Any Rust source / test / fixture / Cargo / lint / workflow
   - Any existing GitHub label or issue (no `gh issue edit` /
     `gh label edit` / `gh label create` / `gh label delete`)
   - Any existing handoff packet
   - `change.md`, `HANDOFF.md`, `Status.md`, `DocAuto.md`, or any
     other root-level doc

   **Cargo.lock policy**:
   - Zero Cargo metadata changes. If `Cargo.toml` or `Cargo.lock`
     changes at all, halt with `NEEDS_HUMAN`.

   **Halt conditions**:
   - Q5 reveals that lifetime-vs-rolling is an architecture-tier
     decision that cannot be resolved from read-only inspection alone.
     Halt with `EXEC_STATUS: blocked` and `STATUS: NEEDS_HUMAN`,
     `NEXT_ROLE: HUMAN_ARBITER`.
   - The audit requires more than one EXEC packet, label edits, script
     edits, or doctrine edits to answer any of Q1-Q5. Halt with
     `NEEDS_HUMAN`.
   - The audit cannot be answered without running local cargo
     commands, formatters, architecture lints, or
     `.ai/dispatch.verify.ps1`. Halt; this is a documentary read-only
     audit.
   - Any tracked file is already dirty in a way that makes the
     read-only audit ambiguous.

   **Scope-preserving halt clause** - the orchestrator's canonical
   verify gate (`.ai/dispatch.verify.ps1`) runs after Claude execute
   even on read-only audits. If verify fails on a target OUTSIDE the
   audit scope, the orchestrator may auto-route a CORRECTION packet
   asking the executor to fix the failure. When that happens **the
   executor MUST halt**: write an EXECUTION_REPORT with
   `EXEC_STATUS: blocked` and `STATUS: NEEDS_HUMAN`, do NOT execute
   the correction. Read-only intent is the entire reason this task
   is in the brief. Precedent: ISSUE-92, ISSUE-98, ISSUE-100,
   ISSUE-120, ISSUE-128 validated this halt path.

   **Verbatim review-gate strings** - the autonomous selector MUST
   copy these seven strings, character-for-character, into the filed
   GitHub issue body. No paraphrasing, no substitution, no reflowing.
   A packet that lacks any one of them verbatim is bounced at review:

   ```
   MUST be a read-only cap-v2 audit; do not edit scripts, doctrine, labels, issues, source, tests, Cargo, or existing packets
   MUST produce a 5-question cap-v2 answer block covering count semantics, age threshold, cleanup mechanism, label policy, and smallest follow-up
   MUST inspect Invoke-AiDispatchAuto.ps1, Register-AiDispatchSchedule.ps1, AI_DISPATCH_AUTOMATION.md, .ai/dispatch.tasks.md, and ai-auto-labelled GitHub issues
   MUST use read-only gh commands (gh issue list, gh label list, gh api repos/.../issues); no gh issue edit / gh label edit / gh label create / gh label delete
   MUST cite verbatim the exact cap-related lines in both PowerShell scripts and the relevant AI_DISPATCH_AUTOMATION.md doctrine sections
   MUST NOT run local cargo commands, tests, formatters, architecture lints, or .ai/dispatch.verify.ps1
   MUST halt rather than implement any label-aging mechanism, script change, or doctrine edit in this dispatch
   ```

   **Done-criterion**:
   - One `ISSUE-*_EXEC_*.md` report with the exact
     `## 5-Question Cap-v2 / Label-Aging Answer Block` section and
     Q1-Q5 subheadings:
     - `### Q1. Lifetime vs rolling-window count semantics?`
     - `### Q2. If rolling, what age threshold (30 / 60 / 90 days)?`
     - `### Q3. Cleanup mechanism: manual command, script mode, or doctrine only?`
     - `### Q4. What labels should remain on aged-out issues?`
     - `### Q5. Smallest safe implementation task?`
   - No source, test, doc, Cargo, workflow, lint, schema, script,
     status, label, GitHub issue, or existing handoff packet edits.
   - `git status --short --untracked-files=no` is clean before and
     after writing the EXEC report.
   - Verification claims are read-only only: document the `gh`, `rg`,
     `git grep`, and file-read commands used for the audit; do not
     manually run cargo tests, builds, fmt, architecture lints, or
     `.ai/dispatch.verify.ps1`. The orchestrator will still run its
     canonical verification gate after execution.
   - Q5 names one smallest next dispatch and includes its proposed
     allowed files, must-not-touch surfaces, verification gates, and
     halt conditions, unless the correct outcome is `NEEDS_HUMAN`.

24. **[DONE 2026-05-23 via PR #133 / commit `6661cee`] Read-only preflight: first World-only CommandBus-backed editor action.**
   ISSUE-108 (task #12, PR #109, commit `ba90b04`) audited CommandBus
   integration shape and landed Q5 = `NEEDS_HUMAN` because picking the
   first editor action through the bus required arbitration between
   three approaches: (A) World-only Action, (B) widened editor-context
   Action with Tier-2 promotion of `editor-actions`, (C) adapter layer
   with permanent dual-ledger. **The architectural arbitration has now
   been made: Approach A (World-only) is the chosen direction for the
   first CommandBus-backed action.** This preflight identifies the
   smallest concrete user-visible action that fits Approach A and
   produces one bounded implementation task surface via Q5.

   The audit must NOT change any source, script, doctrine, label
   state, GitHub issue, or test; it produces a single EXEC packet
   with a 5-question answer block and one Q5 follow-up dispatch
   proposal (or `NEEDS_HUMAN` if no World-only user-visible action
   candidate exists in the current codebase).

   **Runtime invocation note**: this task is a deliberate named +1 on
   top of the freeze-at-101 posture (set by task #23 spending the
   first +1). Run as
   `.\Invoke-AiDispatchAuto.ps1 -PublishMode branch -MaxAutonomousTasks 102`
   so the cap accommodates exactly this one dispatch. The script
   `ValidateRange(1, 200)` accepts 102 without re-registering the
   scheduler; the scheduler remains disabled and its persisted
   argument is unchanged.

   **Allowed file surface**:
   - INSPECT (read-only) `crates/editor-actions/**` (the bus + Action
     trait + current Action impls including `SetTimeScale`)
   - INSPECT (read-only) `crates/editor-shell/**` (current bus wiring
     and submission points)
   - INSPECT (read-only) `editor/rge-editor/**` (current user-visible
     action invocation points, especially keyboard/menu handlers)
   - INSPECT (read-only) `crates/editor-state/**` (to confirm what
     editor-state surface is excluded from candidates)
   - INSPECT (read-only) `kernel/ecs/**` (to identify which `World`
     resources / components are candidate mutation targets)
   - INSPECT (read-only) `ai_handoffs/ISSUE-108_EXEC_*.md` (the prior
     CommandBus audit's Q1-Q5 and approach-A description)
   - MAY add this dispatch's own `ai_handoffs/ISSUE-*_EXEC_*.md`
     packet plus `.meta.json` sidecar if produced by the orchestrator.

   **Files that MUST NOT be touched**:
   - Any `.rs` source file (including `editor-actions`, `editor-shell`,
     `editor/rge-editor`, `editor-state`, `kernel/ecs`, all others)
   - Any `.ps1` script
   - Any `.md` doctrine, including `AI_DISPATCH_AUTOMATION.md` and
     `.ai/dispatch.tasks.md`
   - Any `Cargo.toml` / `Cargo.lock` / `.toml` / `.yml` workflow
   - Any test, fixture, lint, schema, status file
   - Any existing GitHub label or issue
   - Any existing handoff packet (the ISSUE-108 EXEC packet is
     treated as read-only provenance)
   - `change.md`, `HANDOFF.md`, `Status.md`, `DocAuto.md`, or any
     other root-level doc

   **Cargo.lock policy**:
   - Zero Cargo metadata changes. If `Cargo.toml` or `Cargo.lock`
     changes at all, halt with `NEEDS_HUMAN`.

   **Architectural constraints (Approach A, baked in)**:
   - The first CommandBus-backed editor action MUST be a pure
     `World` mutation: `Action::apply(&mut World)` and
     `Action::revert(&mut World)` operate exclusively on
     `World` resources and/or components.
   - The chosen action MUST NOT mutate `editor-state` (Selection,
     Hover, ActiveTool, FaceSelection, or any `EditorShell.coord`
     field).
   - The chosen action MUST NOT touch DockState, egui state, the
     render-path, the asset-reload path, the watcher path, or
     `cad-core`'s `CadGraph`.
   - The implementation MUST NOT promote `editor-actions` to a
     Tier-2 editor-domain coordinator (no `rge-cad-core` or
     `rge-editor-state` added to `crates/editor-actions/Cargo.toml`).
   - The implementation MUST NOT add an adapter-layer dual ledger
     (no parallel undo timeline; no parallel audit-ledger projection).

   **Halt conditions**:
   - No user-visible action candidate exists that can be expressed
     as a pure `World` mutation under the Approach A constraints
     above. Halt with `EXEC_STATUS: blocked` and `STATUS:
     NEEDS_HUMAN`, `NEXT_ROLE: HUMAN_ARBITER`.
   - The audit requires more than one EXEC packet, source edits,
     test edits, or Cargo edits to answer any of Q1-Q5. Halt with
     `NEEDS_HUMAN`.
   - The audit cannot be answered without running local cargo
     commands, formatters, architecture lints, or
     `.ai/dispatch.verify.ps1`. Halt; this is a documentary
     read-only preflight.
   - Any tracked file is already dirty in a way that makes the
     read-only audit ambiguous.

   **Scope-preserving halt clause** - the orchestrator's canonical
   verify gate (`.ai/dispatch.verify.ps1`) runs after Claude execute
   even on read-only preflights. If verify fails on a target OUTSIDE
   the audit scope, the orchestrator may auto-route a CORRECTION
   packet asking the executor to fix the failure. When that happens
   **the executor MUST halt**: write an EXECUTION_REPORT with
   `EXEC_STATUS: blocked` and `STATUS: NEEDS_HUMAN`, do NOT execute
   the correction. Read-only intent is the entire reason this task
   is in the brief. Precedent: ISSUE-92, ISSUE-98, ISSUE-100,
   ISSUE-108, ISSUE-120, ISSUE-128, ISSUE-130 validated this halt
   path.

   **Verbatim review-gate strings** - the autonomous selector MUST
   copy these seven strings, character-for-character, into the filed
   GitHub issue body. No paraphrasing, no substitution, no reflowing.
   A packet that lacks any one of them verbatim is bounced at review:

   ```
   MUST be a read-only World-only CommandBus action preflight; do not edit source, tests, Cargo, scripts, doctrine, labels, issues, or existing packets
   MUST produce a 5-question World-only CommandBus action answer block covering candidate actions, smallest pick, implementation shape, verification plan, and follow-up task surface
   MUST inspect crates/editor-actions/**, crates/editor-shell/**, editor/rge-editor/**, crates/editor-state/**, kernel/ecs/**, and ai_handoffs/ISSUE-108_EXEC_*.md
   MUST adopt Approach A from ISSUE-108 (World-only Action) as the chosen architectural direction; do not propose Approach B (Tier-2 promotion of editor-actions) or Approach C (adapter-layer dual ledger) as alternatives in Q2 or Q3
   MUST exclude action candidates that mutate editor-state, selection, hover, active tool, FaceSelection, DockState, egui state, render-path, asset-reload path, watcher path, or cad-core CadGraph
   MUST NOT run local cargo commands, tests, formatters, architecture lints, or .ai/dispatch.verify.ps1
   MUST halt with EXEC_STATUS blocked and STATUS NEEDS_HUMAN if no World-only user-visible action candidate exists
   ```

   **Done-criterion**:
   - One `ISSUE-*_EXEC_*.md` report with the exact
     `## 5-Question World-only CommandBus Action Preflight Answer Block`
     section and Q1-Q5 subheadings:
     - `### Q1. What user-visible actions in the editor today can be expressed as pure World mutations?`
     - `### Q2. Smallest pick (with rationale: code surface, dependency edges, risk profile)?`
     - `### Q3. Implementation shape — Action struct fields, apply impl, revert impl, submission point?`
     - `### Q4. Verification plan — substrate tests + bus audit-ledger assertion shape?`
     - `### Q5. Smallest follow-up implementation task — allowed files, must-not-touch surfaces, verification gates, halt conditions?`
   - No source, test, doc, Cargo, workflow, lint, schema, script,
     status, label, GitHub issue, or existing handoff packet edits.
   - `git status --short --untracked-files=no` is clean before and
     after writing the EXEC report.
   - Verification claims are read-only only: document the `gh`, `rg`,
     `git grep`, and file-read commands used for the audit; do not
     manually run cargo tests, builds, fmt, architecture lints, or
     `.ai/dispatch.verify.ps1`. The orchestrator will still run its
     canonical verification gate after execution.
   - Q5 names one smallest next dispatch and includes its proposed
     allowed files, must-not-touch surfaces, verification gates, and
     halt conditions, unless the correct outcome is `NEEDS_HUMAN`.

25. **[DONE 2026-05-24 via PR #135 / commit `e23378e`] Implement first World-only CommandBus editor action: Ctrl+2 time-scale preset.**
   ISSUE-132 (task #24, PR #133, commit `6661cee`) completed the
   Approach-A preflight and named one smallest implementation: bind
   `Ctrl+2` to `EditorShell::set_time_scale(2.0)`, so a normal fresh
   editor (`TimeScale::DEFAULT == 1.0`) submits a real non-noop
   `SetTimeScale { from: 1.0, to: 2.0 }` through
   `CommandBus::submit`. This is the first user-visible editor command
   that reaches the existing World-only CommandBus submit path without
   widening `editor-actions` or introducing an adapter ledger.

   The implementation must preserve ISSUE-108 Approach A: use the
   existing `Action::apply(&mut World)` / `Action::revert(&mut World)`
   contract and the existing `SetTimeScale` action. Do not add a new
   `Action` trait shape, do not touch editor-state, and do not route any
   CAD graph or render state through this task.

   **Runtime invocation note**: this task is a deliberate named +1 on
   top of the freeze-at-102 posture set by task #24. Run as
   `.\Invoke-AiDispatchAuto.ps1 -PublishMode branch -MaxAutonomousTasks 103`
   so the cap accommodates exactly this one dispatch. The scheduler
   remains disabled and must not be re-enabled by this task.

   **Allowed file surface**:
   - EDIT `crates/editor-shell/src/lifecycle/commands.rs`
     - Add one `EditorKeyCommand::SetTimeScaleDoubleSpeed` variant.
     - Add one `EditorKeyCommand::from_key_press` arm mapping
       `(KeyCode::Digit2, ctrl=true, shift=false)` to
       `Some(Self::SetTimeScaleDoubleSpeed)`.
     - Add one `EditorShell::handle_key_command` match arm that calls
       `self.set_time_scale(2.0)`.
   - EDIT `crates/editor-shell/tests/keyboard_command_bus_round_trip.rs`
     and/or `crates/editor-shell/tests/time_scale_test.rs`
     - Prefer extending the existing tests rather than adding a new
       test file.
     - Add focused tests for the key mapping and bus-routed time-scale
       behavior described below.
   - MAY add this dispatch's own `ai_handoffs/ISSUE-*_TASK_*.md`,
     `ai_handoffs/ISSUE-*_EXEC_*.md`, `ai_handoffs/ISSUE-*_CORRECT_*.md`
     packets plus `.meta.json` sidecars if produced by the orchestrator,
     and the queue-runner's own `ai_dispatch_logs/log_*.md`.

   **Files that MUST NOT be touched**:
   - `crates/editor-actions/**` (no trait widening, no bus signature
     change, no payload format change, no `CompoundAction` change)
   - `crates/editor-state/**`
   - `editor/rge-editor/**`
   - `kernel/ecs/**`
   - `crates/editor-shell/src/lifecycle/mod.rs`
   - Any other `crates/editor-shell/src/**` file besides
     `crates/editor-shell/src/lifecycle/commands.rs`
   - Any Cargo file (`Cargo.toml`, `Cargo.lock`, workspace manifests)
   - Any workflow, architecture-lint, script, doctrine, status, ADR,
     fixture, generated asset, or root-level doc file
   - Any existing handoff packet or dispatch log
   - Any GitHub label or issue metadata except the queue runner's normal
     issue lifecycle for this dispatch

   **Cargo.lock policy**:
   - Zero Cargo metadata changes. If `Cargo.toml` or `Cargo.lock`
     changes at all, halt with `NEEDS_HUMAN`.

   **Implementation constraints**:
   - Keep the command World-only and use the existing `SetTimeScale`
     action through `EditorShell::set_time_scale(2.0)`.
   - Do not add any new public `Action` impl for this task.
   - Do not call `CommandBus::submit` directly from keyboard handling;
     route through `EditorShell::handle_key_command` and the existing
     `set_time_scale` helper.
   - Do not add a `Ctrl+0` reset binding, preset trio, step-up/step-down
     binding, UI control, menu item, toolbar button, or egui callback.
   - Do not modify undo/redo/mark-saved semantics except through the
     natural behavior of submitting `SetTimeScale`.
   - Re-read `ai_handoffs/ISSUE-132_EXEC_2026-05-23_23-01-08+0300.md`
     before editing and treat its corrected Q5 as the source of truth.

   **Required tests / assertions**:
   - A key-mapping test proves
     `EditorKeyCommand::from_key_press(KeyCode::Digit2, true, false)
     == Some(EditorKeyCommand::SetTimeScaleDoubleSpeed)`.
   - The same mapping coverage proves `ctrl=false` and `shift=true`
     do not map to the new command.
   - A handler test builds a fresh `EditorShell`, calls
     `shell.handle_key_command(EditorKeyCommand::SetTimeScaleDoubleSpeed)`,
     and asserts:
     - `shell.time_scale().value() == 2.0` within the existing float
       tolerance style.
     - `shell.command_bus().stack().cursor()` advanced by exactly 1.
     - `shell.command_bus().is_dirty()` is true.
     - The shell audit ledger gained exactly one
       `TimeScaleChanged { from: 1.0, to: 2.0 }` event.
   - An undo assertion proves `shell.undo_command()` after the new command
     restores `TimeScale` to `1.0` byte-identically / within the existing
     tolerance style.
   - A repeat-press assertion proves pressing `Ctrl+2` again while already
     at `2.0` is a no-op: no cursor advance, no extra dirty transition, and
     no extra `TimeScaleChanged` audit event.

   **Halt conditions**:
   - `rge_input::KeyCode::Digit2` no longer exists or the winit-to-RGE
     translation for `Digit2` is no longer present.
   - An existing `Ctrl+2` / `KeyCode::Digit2` editor binding is discovered
     that would be shadowed.
   - `EditorKeyCommand`, `EditorKeyCommand::from_key_press`,
     `EditorShell::handle_key_command`, or `EditorShell::set_time_scale`
     have been moved, renamed, or restructured enough that the change is no
     longer a single-file command-surface edit.
   - Implementing the binding requires editing any file listed in
     "Files that MUST NOT be touched".
   - Any verification gate reveals failure outside this task's scope that
     would require source/test/Cargo/workflow edits outside the allowed file
     surface. Halt rather than broadening scope.

   **Verbatim review-gate strings** - the autonomous selector MUST copy
   these eight strings, character-for-character, into the filed GitHub issue
   body. No paraphrasing, no substitution, no reflowing. A packet that lacks
   any one of them verbatim is bounced at review:

   ```
   MUST implement Ctrl+2 as EditorKeyCommand::SetTimeScaleDoubleSpeed routed through EditorShell::set_time_scale(2.0)
   MUST keep the implementation inside crates/editor-shell/src/lifecycle/commands.rs plus focused tests in crates/editor-shell/tests/keyboard_command_bus_round_trip.rs and/or crates/editor-shell/tests/time_scale_test.rs
   MUST use the existing SetTimeScale Action and existing CommandBus::submit path; do not add a new Action trait shape, new Action impl, adapter ledger, or CompoundAction wrapper
   MUST NOT modify crates/editor-actions/**, crates/editor-state/**, editor/rge-editor/**, kernel/ecs/**, crates/editor-shell/src/lifecycle/mod.rs, Cargo.toml, or Cargo.lock
   MUST NOT add Ctrl+0, preset trio, step-up/step-down, UI, menu, toolbar, or egui wiring in this dispatch
   MUST add tests for Digit2 key mapping, fresh-shell Ctrl+2 submit to TimeScale 2.0, undo back to 1.0, and repeat Ctrl+2 no-op behavior
   MUST halt with NEEDS_HUMAN if KeyCode::Digit2 is unavailable, an existing Ctrl+2 binding would be shadowed, or the command surface has moved enough to require broader edits
   MUST run cargo build -p rge-editor-shell, cargo +nightly fmt --all -- --check, cargo clippy -p rge-editor-shell --all-targets -- -D warnings, cargo test -p rge-editor-shell --test keyboard_command_bus_round_trip, cargo test -p rge-editor-shell --test time_scale_test, cargo run -q -p rge-tool-architecture-lints -- all, and .ai/dispatch.verify.ps1
   ```

   **Done-criterion**:
   - `Ctrl+2` maps to `EditorKeyCommand::SetTimeScaleDoubleSpeed`.
   - `EditorKeyCommand::SetTimeScaleDoubleSpeed` calls
     `EditorShell::set_time_scale(2.0)`.
   - No files outside the allowed source/test surface and this dispatch's
     own generated handoff/log artifacts are modified.
   - Cargo files remain unchanged.
   - All required tests / assertions above are present and pass.
   - All verification gates listed in the final MUST string exit 0.

26. **[DONE 2026-05-24 via PR #137 / commit `bb4f557`] Implement Ctrl+0 reset-to-default CommandBus time-scale action.**
   Task #25 made `Ctrl+2` the first user-visible World-only
   `CommandBus::submit` path by routing through the existing
   `SetTimeScale` action. That unlocks the reset half that ISSUE-132
   correctly rejected before `Ctrl+2` existed: `Ctrl+0` is a no-op on a
   fresh editor, but after a non-default time-scale value it becomes a
   real `SetTimeScale { from: 2.0, to: TimeScale::DEFAULT }` submit.

   Implement exactly that reset binding. Preserve the existing
   `SetTimeScale` coalescing model: immediate preset changes within the
   500 ms coalesce window may still merge like slider drags. This task
   must not change the bus, the coalesce window, action ids, payload
   encoding, undo-stack internals, or any editor-state/CAD/render surface.

   **Runtime invocation note**: this task is a deliberate named +1 on
   top of the freeze-at-103 posture set by task #25. Run as
   `.\Invoke-AiDispatchAuto.ps1 -PublishMode branch -MaxAutonomousTasks 104`
   so the cap accommodates exactly this one dispatch. The scheduler
   remains disabled and must not be re-enabled by this task.

   **Allowed file surface**:
   - EDIT `crates/editor-shell/src/lifecycle/commands.rs`
     - Add one `EditorKeyCommand::ResetTimeScaleDefault` variant.
     - Add one `EditorKeyCommand::from_key_press` arm mapping
       `(KeyCode::Digit0, ctrl=true, shift=false)` to
       `Some(Self::ResetTimeScaleDefault)`.
     - Add one `EditorShell::handle_key_command` match arm that calls
       `self.set_time_scale(TimeScale::DEFAULT)`.
   - EDIT `crates/editor-shell/tests/keyboard_command_bus_round_trip.rs`
     and/or `crates/editor-shell/tests/time_scale_test.rs`
     - Prefer extending the existing tests rather than adding a new
       test file.
     - Add focused tests for the key mapping, fresh-default no-op
       behavior, and post-`Ctrl+2` reset behavior described below.
   - MAY add this dispatch's own `ai_handoffs/ISSUE-*_TASK_*.md`,
     `ai_handoffs/ISSUE-*_EXEC_*.md`, `ai_handoffs/ISSUE-*_CORRECT_*.md`
     packets plus `.meta.json` sidecars if produced by the orchestrator,
     and the queue-runner's own `ai_dispatch_logs/log_*.md`.

   **Files that MUST NOT be touched**:
   - `crates/editor-actions/**` (no trait widening, no bus signature
     change, no coalesce-window change, no action-id change, no payload
     format change, no `CompoundAction` change)
   - `crates/editor-state/**`
   - `editor/rge-editor/**`
   - `kernel/ecs/**`
   - `crates/editor-shell/src/lifecycle/mod.rs`
   - Any other `crates/editor-shell/src/**` file besides
     `crates/editor-shell/src/lifecycle/commands.rs`
   - Any Cargo file (`Cargo.toml`, `Cargo.lock`, workspace manifests)
   - Any workflow, architecture-lint, script, doctrine, status, ADR,
     fixture, generated asset, or root-level doc file
   - Any existing handoff packet or dispatch log
   - Any GitHub label or issue metadata except the queue runner's normal
     issue lifecycle for this dispatch

   **Cargo.lock policy**:
   - Zero Cargo metadata changes. If `Cargo.toml` or `Cargo.lock`
     changes at all, halt with `NEEDS_HUMAN`.

   **Implementation constraints**:
   - Keep the command World-only and use the existing `SetTimeScale`
     action through `EditorShell::set_time_scale(TimeScale::DEFAULT)`.
   - Do not add any new public `Action` impl for this task.
   - Do not call `CommandBus::submit` directly from keyboard handling;
     route through `EditorShell::handle_key_command` and the existing
     `set_time_scale` helper.
   - Do not add a preset trio, step-up/step-down binding, UI control,
     menu item, toolbar button, or egui callback.
   - Do not modify undo/redo/mark-saved semantics except through the
     natural behavior of submitting `SetTimeScale`.
   - Do not alter the 500 ms coalesce behavior. Tests that require a
     separate reset stack entry after `Ctrl+2` must wait past the current
     coalesce window (for example 600 ms) rather than changing bus code.
   - Re-read task #25's landed code before editing and treat
     `EditorKeyCommand::SetTimeScaleDoubleSpeed` as the existing
     companion binding.

   **Required tests / assertions**:
   - A key-mapping test proves
     `EditorKeyCommand::from_key_press(KeyCode::Digit0, true, false)
     == Some(EditorKeyCommand::ResetTimeScaleDefault)`.
   - The same mapping coverage proves `ctrl=false` and `shift=true`
     do not map to the new command.
   - A fresh-shell no-op test calls
     `shell.handle_key_command(EditorKeyCommand::ResetTimeScaleDefault)`
     on a fresh `EditorShell` and asserts no stack entry, no cursor
     advance, no dirty flip, no time-scale change, and no
     `TimeScaleChanged` audit event.
   - A reset-after-preset test first calls
     `shell.handle_key_command(EditorKeyCommand::SetTimeScaleDoubleSpeed)`,
     waits past the existing coalesce window, then calls
     `shell.handle_key_command(EditorKeyCommand::ResetTimeScaleDefault)`
     and asserts:
     - `shell.time_scale().value() == TimeScale::DEFAULT` within the
       existing float tolerance style.
     - `shell.command_bus().stack().cursor()` advanced by exactly 1 for
       the reset submit.
     - The shell audit ledger gained exactly one additional
       `TimeScaleChanged { from: 2.0, to: 1.0 }` event for the reset.
   - An undo assertion proves `shell.undo_command()` after the reset
     restores `TimeScale` to `2.0` within the existing tolerance style.

   **Halt conditions**:
   - `rge_input::KeyCode::Digit0` no longer exists or the winit-to-RGE
     translation for `Digit0` is no longer present.
   - An existing `Ctrl+0` / `KeyCode::Digit0` editor binding is discovered
     that would be shadowed.
   - `EditorKeyCommand`, `EditorKeyCommand::from_key_press`,
     `EditorShell::handle_key_command`, `EditorShell::set_time_scale`,
     or task #25's `SetTimeScaleDoubleSpeed` path have been moved,
     renamed, or restructured enough that the change is no longer a
     single-file command-surface edit.
   - Implementing the binding requires editing any file listed in
     "Files that MUST NOT be touched".
   - Any verification gate reveals failure outside this task's scope that
     would require source/test/Cargo/workflow edits outside the allowed file
     surface. Halt rather than broadening scope.

   **Verbatim review-gate strings** - the autonomous selector MUST copy
   these eight strings, character-for-character, into the filed GitHub issue
   body. No paraphrasing, no substitution, no reflowing. A packet that lacks
   any one of them verbatim is bounced at review:

   ```
   MUST implement Ctrl+0 as EditorKeyCommand::ResetTimeScaleDefault routed through EditorShell::set_time_scale(TimeScale::DEFAULT)
   MUST keep the implementation inside crates/editor-shell/src/lifecycle/commands.rs plus focused tests in crates/editor-shell/tests/keyboard_command_bus_round_trip.rs and/or crates/editor-shell/tests/time_scale_test.rs
   MUST use the existing SetTimeScale Action and existing CommandBus::submit path; do not add a new Action trait shape, new Action impl, adapter ledger, CompoundAction wrapper, or coalesce-window change
   MUST NOT modify crates/editor-actions/**, crates/editor-state/**, editor/rge-editor/**, kernel/ecs/**, crates/editor-shell/src/lifecycle/mod.rs, Cargo.toml, or Cargo.lock
   MUST NOT add preset trio, step-up/step-down, UI, menu, toolbar, or egui wiring in this dispatch
   MUST add tests for Digit0 key mapping, fresh-shell Ctrl+0 no-op behavior, Ctrl+2 then Ctrl+0 reset-to-default behavior after the coalesce window, and undo back to 2.0
   MUST halt with NEEDS_HUMAN if KeyCode::Digit0 is unavailable, an existing Ctrl+0 binding would be shadowed, or the command surface has moved enough to require broader edits
   MUST run cargo build -p rge-editor-shell, cargo +nightly fmt --all -- --check, cargo test -p rge-editor-shell --test keyboard_command_bus_round_trip, cargo test -p rge-editor-shell --test time_scale_test, cargo run -q -p rge-tool-architecture-lints -- all, and .ai/dispatch.verify.ps1
   ```

   **Done-criterion**:
   - `Ctrl+0` maps to `EditorKeyCommand::ResetTimeScaleDefault`.
   - `EditorKeyCommand::ResetTimeScaleDefault` calls
     `EditorShell::set_time_scale(TimeScale::DEFAULT)`.
   - Fresh-shell `Ctrl+0` is pinned as a no-op.
   - `Ctrl+2` followed by `Ctrl+0` after the coalesce window resets to
     default and undo restores `2.0`.
   - No files outside the allowed source/test surface and this dispatch's
     own generated handoff/log artifacts are modified.
   - Cargo files remain unchanged.
   - All required tests / assertions above are present and pass.
   - All verification gates listed in the final MUST string exit 0.

27. **[DONE 2026-05-24 via PR #139 / commit `fa2f9a0`] Implement Ctrl+4 max-fast-forward CommandBus time-scale action.**
   Tasks #25 and #26 established the first World-only `CommandBus`
   editor action pair by routing `Ctrl+2` and `Ctrl+0` through the
   existing `SetTimeScale` action. Add one more bounded preset:
   `Ctrl+4` sets the time scale to `TimeScale::MAX` (4.0x), giving the
   user the maximum fast-forward shortcut without adding a new bus
   concept, action type, UI surface, or editor-state dependency.

   This task is deliberately a small source dispatch in the proven
   time-scale lane. It must preserve the existing `SetTimeScale`
   coalescing model: immediate preset changes within the 500 ms
   coalesce window may merge like slider drags. Do not change the bus,
   coalesce window, action id, payload encoding, undo-stack internals,
   or any editor-state/CAD/render surface.

   **Runtime invocation note**: this task is a deliberate named +1 on
   top of the freeze-at-104 posture set by task #26. Run as
   `.\Invoke-AiDispatchAuto.ps1 -PublishMode branch -MaxAutonomousTasks 105`
   so the cap accommodates exactly this one dispatch. The scheduler
   remains disabled and must not be re-enabled by this task.

   **Allowed file surface**:
   - EDIT `crates/editor-shell/src/lifecycle/commands.rs`
     - Add one `EditorKeyCommand::SetTimeScaleMaxFastForward` variant.
     - Add one `EditorKeyCommand::from_key_press` arm mapping
       `(KeyCode::Digit4, ctrl=true, shift=false)` to
       `Some(Self::SetTimeScaleMaxFastForward)`.
     - Add one `EditorShell::handle_key_command` match arm that calls
       `self.set_time_scale(TimeScale::MAX)`.
   - EDIT `crates/editor-shell/tests/keyboard_command_bus_round_trip.rs`
     and/or `crates/editor-shell/tests/time_scale_test.rs`
     - Prefer extending the existing tests rather than adding a new
       test file.
     - Add focused tests for the key mapping, fresh-shell max preset,
       repeat no-op behavior, and undo behavior described below.
   - MAY add this dispatch's own `ai_handoffs/ISSUE-*_TASK_*.md`,
     `ai_handoffs/ISSUE-*_EXEC_*.md`, `ai_handoffs/ISSUE-*_CORRECT_*.md`
     packets plus `.meta.json` sidecars if produced by the orchestrator,
     and the queue-runner's own `ai_dispatch_logs/log_*.md`.

   **Files that MUST NOT be touched**:
   - `crates/editor-actions/**` (no trait widening, no bus signature
     change, no coalesce-window change, no action-id change, no payload
     format change, no `CompoundAction` change)
   - `crates/editor-state/**`
   - `editor/rge-editor/**`
   - `kernel/ecs/**`
   - `crates/editor-shell/src/lifecycle/mod.rs`
   - Any other `crates/editor-shell/src/**` file besides
     `crates/editor-shell/src/lifecycle/commands.rs`
   - Any Cargo file (`Cargo.toml`, `Cargo.lock`, workspace manifests)
   - Any workflow, architecture-lint, script, doctrine, status, ADR,
     fixture, generated asset, or root-level doc file
   - Any existing handoff packet or dispatch log
   - Any GitHub label or issue metadata except the queue runner's normal
     issue lifecycle for this dispatch

   **Cargo.lock policy**:
   - Zero Cargo metadata changes. If `Cargo.toml` or `Cargo.lock`
     changes at all, halt with `NEEDS_HUMAN`.

   **Implementation constraints**:
   - Keep the command World-only and use the existing `SetTimeScale`
     action through `EditorShell::set_time_scale(TimeScale::MAX)`.
   - Do not add any new public `Action` impl for this task.
   - Do not call `CommandBus::submit` directly from keyboard handling;
     route through `EditorShell::handle_key_command` and the existing
     `set_time_scale` helper.
   - Do not add preset trio, step-up/step-down binding, UI control,
     menu item, toolbar button, egui callback, or any non-time-scale
     command in this dispatch.
   - Do not modify undo/redo/mark-saved semantics except through the
     natural behavior of submitting `SetTimeScale`.
   - Do not alter the 500 ms coalesce behavior. Tests that require
     separate preset stack entries must wait past the current coalesce
     window rather than changing bus code.
   - Re-read tasks #25 and #26's landed code before editing and treat
     `EditorKeyCommand::SetTimeScaleDoubleSpeed` and
     `EditorKeyCommand::ResetTimeScaleDefault` as the companion
     binding patterns.

   **Required tests / assertions**:
   - A key-mapping test proves
     `EditorKeyCommand::from_key_press(KeyCode::Digit4, true, false)
     == Some(EditorKeyCommand::SetTimeScaleMaxFastForward)`.
   - The same mapping coverage proves `ctrl=false` and `shift=true`
     do not map to the new command.
   - A fresh-shell submit test calls
     `shell.handle_key_command(EditorKeyCommand::SetTimeScaleMaxFastForward)`
     on a fresh `EditorShell` and asserts:
     - `shell.time_scale().value() == TimeScale::MAX` within the
       existing float tolerance style.
     - The bus stack length is exactly 1.
     - The bus cursor advanced by exactly 1.
     - The dirty flag is true.
     - The shell audit ledger gained exactly one
       `TimeScaleChanged { from: 1.0, to: TimeScale::MAX }` event.
   - A repeat no-op test calls the new command twice and asserts the
     second call does not add a stack entry, advance the cursor, flip any
     additional dirty state, or add another `TimeScaleChanged` event.
   - An undo assertion proves `shell.undo_command()` after the first
     `Ctrl+4` restores `TimeScale` to `TimeScale::DEFAULT` within the
     existing tolerance style.

   **Halt conditions**:
   - `rge_input::KeyCode::Digit4` no longer exists or the winit-to-RGE
     translation for `Digit4` is no longer present.
   - An existing `Ctrl+4` / `KeyCode::Digit4` editor binding is discovered
     that would be shadowed.
   - `EditorKeyCommand`, `EditorKeyCommand::from_key_press`,
     `EditorShell::handle_key_command`, `EditorShell::set_time_scale`,
     or the task #25/#26 time-scale command patterns have been moved,
     renamed, or restructured enough that the change is no longer a
     single-file command-surface edit.
   - Implementing the binding requires editing any file listed in
     "Files that MUST NOT be touched".
   - Any verification gate reveals failure outside this task's scope that
     would require source/test/Cargo/workflow edits outside the allowed file
     surface. Halt rather than broadening scope.

   **Verbatim review-gate strings** - the autonomous selector MUST copy
   these eight strings, character-for-character, into the filed GitHub issue
   body. No paraphrasing, no substitution, no reflowing. A packet that lacks
   any one of them verbatim is bounced at review:

   ```
   MUST implement Ctrl+4 as EditorKeyCommand::SetTimeScaleMaxFastForward routed through EditorShell::set_time_scale(TimeScale::MAX)
   MUST keep the implementation inside crates/editor-shell/src/lifecycle/commands.rs plus focused tests in crates/editor-shell/tests/keyboard_command_bus_round_trip.rs and/or crates/editor-shell/tests/time_scale_test.rs
   MUST use the existing SetTimeScale Action and existing CommandBus::submit path; do not add a new Action trait shape, new Action impl, adapter ledger, CompoundAction wrapper, or coalesce-window change
   MUST NOT modify crates/editor-actions/**, crates/editor-state/**, editor/rge-editor/**, kernel/ecs/**, crates/editor-shell/src/lifecycle/mod.rs, Cargo.toml, or Cargo.lock
   MUST NOT add preset trio, step-up/step-down, UI, menu, toolbar, egui wiring, or any non-time-scale command in this dispatch
   MUST add tests for Digit4 key mapping, fresh-shell Ctrl+4 submit to TimeScale::MAX, repeated Ctrl+4 no-op behavior, and undo back to TimeScale::DEFAULT
   MUST halt with NEEDS_HUMAN if KeyCode::Digit4 is unavailable, an existing Ctrl+4 binding would be shadowed, or the command surface has moved enough to require broader edits
   MUST run cargo build -p rge-editor-shell, cargo +nightly fmt --all -- --check, cargo test -p rge-editor-shell --test keyboard_command_bus_round_trip, cargo test -p rge-editor-shell --test time_scale_test, cargo run -q -p rge-tool-architecture-lints -- all, and .ai/dispatch.verify.ps1
   ```

   **Done-criterion**:
   - `Ctrl+4` maps to `EditorKeyCommand::SetTimeScaleMaxFastForward`.
   - `EditorKeyCommand::SetTimeScaleMaxFastForward` calls
     `EditorShell::set_time_scale(TimeScale::MAX)`.
   - Fresh-shell `Ctrl+4` submits exactly one bus action and records the
     expected `TimeScaleChanged` event.
   - Repeated `Ctrl+4` at `TimeScale::MAX` is pinned as a no-op.
   - Undo after `Ctrl+4` restores `TimeScale::DEFAULT`.
   - No files outside the allowed source/test surface and this dispatch's
     own generated handoff/log artifacts are modified.
   - Cargo files remain unchanged.
   - All required tests / assertions above are present and pass.
   - All verification gates listed in the final MUST string exit 0.

28. **[DONE 2026-05-24 via PR #141 / commit `91a123e`] Read-only preflight: next non-time-scale World-only CommandBus action.**
   Tasks #25, #26, and #27 proved the `CommandBus` integration path by
   routing `Ctrl+2`, `Ctrl+0`, and `Ctrl+4` through the existing
   `SetTimeScale` action. Do not add another time-scale shortcut by
   guesswork. Audit the current editor-shell surface and identify whether
   there is a smallest **non-time-scale**, **World-only** user action that
   can safely become the next CommandBus-backed implementation task.

   This task is read-only. It exists to prevent recursive scope drift:
   the chosen follow-up must fit the already-selected Approach A from
   ISSUE-108 / task #24 (`Action::apply(&mut rge_kernel_ecs::World)`,
   no editor-state context, no adapter ledger, no Tier-2 promotion). If no
   such candidate exists today, say that directly and end with
   `NEEDS_HUMAN`; do not propose a source edit that widens the bus.

   **Runtime invocation note**: this task is a deliberate named +1 on top
   of the freeze-at-105 posture set by task #27. Run as
   `.\Invoke-AiDispatchAuto.ps1 -PublishMode branch -MaxAutonomousTasks 106`
   so the cap accommodates exactly this one dispatch. The scheduler
   remains disabled and must not be re-enabled by this task.

   **Allowed file surface**:
   - MAY add this dispatch's own `ai_handoffs/ISSUE-*_TASK_*.md`,
     `ai_handoffs/ISSUE-*_EXEC_*.md`, `ai_handoffs/ISSUE-*_CORRECT_*.md`
     packets plus `.meta.json` sidecars if produced by the orchestrator,
     and the queue-runner's own `ai_dispatch_logs/log_*.md`.
   - NO source, test, Cargo, workflow, script, doctrine, status, handoff
     rewrite, generated asset, or issue-label edit is allowed from the
     executor.

   **Read-only scope to inspect**:
   - `crates/editor-shell/src/lifecycle/commands.rs`
   - `crates/editor-shell/src/lifecycle/playback.rs`
   - `crates/editor-shell/src/lifecycle/asset_reload.rs`
   - `crates/editor-shell/src/lifecycle/mod.rs`
   - `crates/editor-shell/tests/**`
   - `crates/editor-actions/src/**`
   - `crates/editor-state/src/**`
   - Cross-reference tasks #12, #24, #25, #26, and #27 in this brief and
     their landed EXEC packets only as precedent; do not edit them.

   **Files that MUST NOT be touched**:
   - Any `crates/**` file
   - `editor/**`
   - `kernel/**`
   - `.github/**`
   - Any Cargo file (`Cargo.toml`, `Cargo.lock`, workspace manifests)
   - Any PowerShell script
   - Any doctrine/status/planning doc (`AI_DISPATCH_AUTOMATION.md`,
     `HANDOFF.md`, `Status.md`, `change.md`, ADRs, plans)
   - Any existing handoff packet or dispatch log
   - Any GitHub label or issue metadata except the queue runner's normal
     issue lifecycle for this dispatch

   **Five-question answer block**:
   The EXEC report must contain a literal
   `## 5-Question Non-Time-Scale CommandBus Preflight Answer Block`
   section with these exact Q1-Q5 headings:

   - `Q1. What non-time-scale editor user actions exist today, and where are they handled?`
     Inventory keyboard, toolbar, reload, selection, playback, tool, and
     other obvious editor-shell user actions. Include file/line evidence.
   - `Q2. Which candidates are genuinely World-only under the current Action trait?`
     Classify each candidate as `world-only`, `editor-shell-field`,
     `editor-state`, `cad/editor-wrapper-world`, `render/gfx`, or
     `needs-new-context`. Explain why each classification follows from
     the current code, not from desired architecture.
   - `Q3. Excluding time-scale, is there a smallest candidate that can use Approach A without widening CommandBus?`
     Apply Approach A strictly. Do not propose Approach B, Approach C, or
     any adapter/context broadening. If the only viable World-only action
     is still `SetTimeScale`, answer `no candidate`.
   - `Q4. If a candidate exists, what is the smallest implementation surface and verification plan?`
     Name the exact files, tests, MUST NOT list, halt conditions, and
     canonical/focused gates for the follow-up implementation. If no
     candidate exists, identify the smallest human architecture decision
     needed before implementation can proceed.
   - `Q5. What should task #29 be?`
     Recommend exactly one of: a bounded implementation task for the
     chosen non-time-scale World-only candidate, a follow-up read-only
     audit with a narrower scope, or `NEEDS_HUMAN` because no candidate
     exists without changing CommandBus/editor-state architecture.

   **Halt conditions**:
   - The audit requires source/test/Cargo/workflow/script/doc edits to
     answer the questions.
   - The answer would require changing `rge_editor_actions::Action`,
     `CommandBus`, `CompoundAction`, or the `EditorShell::submit_action`
     signature.
   - The smallest candidate requires editor-state, editor-shell field,
     render/gfx, CAD wrapper-world, or broader editor context mutation.
   - The answer tries to pick another time-scale preset, step-up/step-down
     binding, UI/menu/toolbar/egui callback, or preset trio instead of a
     non-time-scale action.
   - The audit discovers that the task #25/#26/#27 time-scale lane no
     longer compiles or that the current `SetTimeScale` precedent has
     moved enough that candidate classification cannot be trusted.

   **Verbatim review-gate strings** - the autonomous selector MUST copy
   these seven strings, character-for-character, into the filed GitHub issue
   body. No paraphrasing, no substitution, no reflowing. A packet that lacks
   any one of them verbatim is bounced at review:

   ```
   MUST be a read-only non-time-scale CommandBus preflight; do not edit source, tests, Cargo, workflows, scripts, doctrine, status docs, issues, labels, or existing packets
   MUST produce a 5-question Non-Time-Scale CommandBus Preflight Answer Block with Q1-Q5 headings exactly as specified in the brief
   MUST classify each candidate as world-only, editor-shell-field, editor-state, cad/editor-wrapper-world, render/gfx, or needs-new-context using file/line evidence
   MUST exclude SetTimeScale, time-scale presets, step-up/step-down bindings, preset trio work, UI/menu/toolbar/egui wiring, and any other time-scale follow-up from the recommended task #29
   MUST apply Approach A strictly; do not propose Approach B, Approach C, Action trait widening, adapter ledger, editor-state context, or CommandBus signature changes as alternatives
   MUST end with NEEDS_HUMAN if no non-time-scale World-only candidate exists under the current Action trait rather than inventing an implementation task
   MUST run git status --short --untracked-files=no before and after EXEC and confirm only this dispatch's own ai_handoffs/log artifacts changed
   ```

   **Done-criterion**:
   - EXEC report contains the exact five-question heading and Q1-Q5
     sub-headings.
   - Q1 inventories current non-time-scale user actions with file/line
     evidence.
   - Q2 classifies every candidate under the current code shape.
   - Q3 excludes all time-scale follow-ups and applies Approach A strictly.
   - Q4 names an implementation surface only if a true non-time-scale
     World-only candidate exists.
   - Q5 recommends exactly one task #29 route or `NEEDS_HUMAN`.
   - No tracked source/test/Cargo/workflow/script/doc/status file changes.

29. **[DONE 2026-05-24 via PR #143 / commit `485e2e3`] Read-only preflight: D-Fillet output-identity remaining gap.**
   The current repository no longer has a generic "D-Fillet blocker":
   `Status.md` records ADR-119 D1-D8 closed, chamfer `FilletOp` has
   graph-level face inheritance plus filtered edge inheritance, and
   `RoundFilletOp` has landed across Cuboid / Extrude / Revolve / Loft
   with multi-edge corner handling. The remaining CAD-critical question
   is narrower: what output-identity gap, if any, should be addressed
   next for D-Fillet outputs, especially `RoundFilletOp`'s nameless
   cylinder-cap and corner-patch surfaces.

   This is a read-only audit. It must not implement provider arms,
   mint IDs, edit topology resolvers, or alter tessellation labels. The
   result is one answer block that states the current identity state,
   separates chamfer `FilletOp` from real `RoundFilletOp`, and recommends
   exactly one bounded task #30 or `NEEDS_HUMAN`.

   **Runtime invocation note**: this task is a deliberate named +1 on top
   of the freeze-at-106 posture set by task #28. Run as
   `.\Invoke-AiDispatchAuto.ps1 -PublishMode branch -MaxAutonomousTasks 107`
   so the cap accommodates exactly this one dispatch. The scheduler
   remains disabled and must not be re-enabled by this task.

   **Allowed file surface**:
   - MAY add this dispatch's own `ai_handoffs/ISSUE-*_TASK_*.md`,
     `ai_handoffs/ISSUE-*_EXEC_*.md`, `ai_handoffs/ISSUE-*_CORRECT_*.md`
     packets plus `.meta.json` sidecars if produced by the orchestrator,
     and the queue-runner's own `ai_dispatch_logs/log_*.md`.
   - NO source, test, Cargo, workflow, script, doctrine, status, handoff
     rewrite, generated asset, or issue-label edit is allowed from the
     executor.

   **Read-only scope to inspect**:
   - `docs/adr/ADR-119-real-round-fillet-substrate.md`
   - `docs/architecture/FILLET_OUTPUT_IDENTITY.md` if present
   - `crates/cad-core/src/operators/fillet/**`
   - `crates/cad-core/src/operators/round_fillet/**`
   - `crates/cad-core/src/topology/resolve.rs`
   - `crates/cad-core/src/topology/edge_resolve.rs`
   - `crates/cad-core/src/topology/{face_id.rs,edge_id.rs,provider.rs}`
     or their current equivalents if the modules moved
   - `crates/cad-core/tests/*fillet*`
   - `crates/cad-projection/src/{lib.rs,picking.rs,render_adapter.rs}`
   - `Status.md`, `HANDOFF.md`, and `change.md` as historical evidence
     only; do not edit them.

   **Files that MUST NOT be touched**:
   - Any `crates/**` file
   - `editor/**`
   - `kernel/**`
   - `.github/**`
   - Any Cargo file (`Cargo.toml`, `Cargo.lock`, workspace manifests)
   - Any PowerShell script
   - Any doctrine/status/planning doc (`AI_DISPATCH_AUTOMATION.md`,
     `HANDOFF.md`, `Status.md`, `change.md`, ADRs, architecture docs,
     plans)
   - Any existing handoff packet or dispatch log
   - Any GitHub label or issue metadata except the queue runner's normal
     issue lifecycle for this dispatch

   **Five-question answer block**:
   The EXEC report must contain a literal
   `## 5-Question D-Fillet Output-Identity Preflight Answer Block`
   section with these exact Q1-Q5 headings:

   - `Q1. What output identity does chamfer FilletOp have today?`
     Summarize graph-level face inheritance, filtered edge inheritance,
     any direct `BRepProvider` / `BRepEdgeProvider` non-goals, and how
     chamfer caps are represented in labels/projection. Include file/line
     evidence.
   - `Q2. What output identity does RoundFilletOp have today?`
     Summarize graph-level resolver behavior, inherited faces/edges,
     nameless cylinder/cap/corner surfaces, `TopologyFaceId::DEGENERATE`
     usage, and direct provider non-goals. Include file/line evidence.
   - `Q3. Which remaining gap is actually load-bearing for the next CAD product step?`
     Classify each plausible gap as `consumer-pressure-present`,
     `pressure-deferred`, `already-solved`, or `needs-ADR`. Plausible
     gaps include direct provider impls, stable IDs for generated round
     surfaces, face-label propagation, edge inheritance behavior, picking
     and selection, and projection/highlight support.
   - `Q4. What is the smallest safe follow-up if the gap is bounded?`
     If a bounded follow-up exists, name exact allowed files, exact tests,
     focused gates, canonical gates, MUST NOT list, and halt conditions.
     If the smallest next step is a policy decision, state the decision
     instead of inventing an implementation task.
   - `Q5. What should task #30 be?`
     Recommend exactly one of: a bounded implementation task, a narrower
     read-only audit, a docs/doctrine update, or `NEEDS_HUMAN` because the
     remaining identity choice requires product/architecture arbitration.

   **Halt conditions**:
   - The audit requires source/test/Cargo/workflow/script/doc edits to
     answer the questions.
   - The answer would require changing `BRepFaceId`, `BRepEdgeId`,
     `BRepProvider`, `BRepEdgeProvider`, `TopologyFaceId`, resolver
     signatures, operator graph structure, or projection contracts before
     a human chooses the identity policy.
   - The smallest follow-up would mint stable IDs for generated
     `RoundFilletOp` surfaces without evidence of consumer pressure.
   - The smallest follow-up would add direct provider impls when the
     graph-level resolver already provides the honest identity surface.
   - The audit finds that D-Fillet output identity is already fully
     solved and no bounded product follow-up exists; route to
     `NEEDS_HUMAN` rather than inventing work.
   - Any verification or repository-state check reveals tracked changes
     outside this dispatch's own handoff/log artifacts.

   **Verbatim review-gate strings** - the autonomous selector MUST copy
   these seven strings, character-for-character, into the filed GitHub issue
   body. No paraphrasing, no substitution, no reflowing. A packet that lacks
   any one of them verbatim is bounced at review:

   ```
   MUST be a read-only D-Fillet output-identity preflight; do not edit source, tests, Cargo, workflows, scripts, doctrine, status docs, issues, labels, existing packets, or architecture docs
   MUST produce a 5-question D-Fillet Output-Identity Preflight Answer Block with Q1-Q5 headings exactly as specified in the brief
   MUST separate chamfer FilletOp identity from RoundFilletOp identity and cite file/line evidence for both
   MUST classify each remaining identity gap as consumer-pressure-present, pressure-deferred, already-solved, or needs-ADR
   MUST NOT propose minting stable IDs for generated RoundFilletOp cap/cylinder/corner surfaces unless Q3 shows concrete consumer pressure in current code
   MUST end with NEEDS_HUMAN if the next step requires choosing an identity policy rather than applying an already-bounded implementation
   MUST run git status --short --untracked-files=no before and after EXEC and confirm only this dispatch's own ai_handoffs/log artifacts changed
   ```

   **Done-criterion**:
   - EXEC report contains the exact five-question heading and Q1-Q5
     sub-headings.
   - Q1 states the current chamfer `FilletOp` identity surface with
     file/line evidence.
   - Q2 states the current `RoundFilletOp` identity surface with
     file/line evidence.
   - Q3 classifies every plausible remaining gap using the required
     four labels.
   - Q4 names a bounded implementation surface only if consumer pressure
     and policy are already present.
   - Q5 recommends exactly one task #30 route or `NEEDS_HUMAN`.
   - No tracked source/test/Cargo/workflow/script/doc/status file changes.

30. **[DONE 2026-05-24 via PR #145 / commit `988a626`] Fix Wait-GitHubActions CodeQL workflow-name matching.**
   The publish lane now repeatedly needs a second manual `gh run watch` for
   CodeQL after `Wait-GitHubActions.ps1` reports the five in-repo workflow
   mirrors green. That helper intentionally reads `.github/workflows/*.yml`
   by default, so omitting repo-level CodeQL from the default list is fine.
   The bug is narrower: when a caller explicitly passes
   `-WorkflowName CodeQL`, the helper still keys runs by the GitHub run
   `name` field (`Push on main`, PR ref names, etc.) instead of the
   `workflowName` field (`CodeQL`). This makes the explicit CodeQL wait
   path report `missing` even though the run exists.

   Fix the helper so expected workflow names can match either the workflow
   name or the run name, while preserving the existing latest-per-workflow,
   per-commit, deadline, and exit-code behavior. This is a single-script
   implementation task. Do not touch dispatch doctrine, workflow files, or
   the autonomous driver.

   **Runtime invocation note**: this task is a deliberate named +1 on top
   of the freeze-at-107 posture set by task #29. Run as
   `.\Invoke-AiDispatchAuto.ps1 -PublishMode branch -MaxAutonomousTasks 108`
   so the cap accommodates exactly this one dispatch. The scheduler
   remains disabled and must not be re-enabled by this task.

   **Allowed file surface**:
   - EDIT `Wait-GitHubActions.ps1` only.
   - MAY add this dispatch's own `ai_handoffs/ISSUE-*_TASK_*.md`,
     `ai_handoffs/ISSUE-*_EXEC_*.md`, `ai_handoffs/ISSUE-*_CORRECT_*.md`
     packets plus `.meta.json` sidecars if produced by the orchestrator,
     and the queue-runner's own `ai_dispatch_logs/log_*.md`.

   **Files that MUST NOT be touched**:
   - `.github/**`
   - `.ai/dispatch.verify.ps1`
   - `Invoke-AiDispatchAuto.ps1`
   - `Register-AiDispatchSchedule.ps1`
   - Any `crates/**`, `editor/**`, `kernel/**`, or Cargo file
   - `AI_DISPATCH_AUTOMATION.md`, `HANDOFF.md`, `Status.md`, `change.md`,
     ADRs, architecture docs, plans, or any existing handoff packet/log
   - Any GitHub label or issue metadata except the queue runner's normal
     issue lifecycle for this dispatch

   **Implementation shape**:
   - Add `workflowName` to the `gh run list --json ...` field list.
   - Build the latest-run map so a run is addressable by its
     `workflowName` when present, and by `name` for compatibility with
     existing callers.
   - Preserve the displayed row shape (`Name`, `Status`, `Conclusion`,
     `RunId`, `Url`), but it may show either the matched expected workflow
     name or a useful `workflowName`/`name` label as long as the output is
     understandable.
   - Preserve exit codes: `0` for success/skipped/neutral, `1` for failed
     workflows, `2` for timeout.

   **Halt conditions**:
   - The fix requires editing any file besides `Wait-GitHubActions.ps1`.
   - The fix requires changing the helper's default expected workflow list
     to include repo-level CodeQL automatically.
   - The fix requires changing timeout semantics, branch/commit filtering,
     latest-run selection, or exit-code meanings.
   - The explicit CodeQL regression command below cannot be made to pass
     against the current green main commit without broader workflow or
     GitHub configuration changes.

   **Verbatim review-gate strings** - the autonomous selector MUST copy
   these six strings, character-for-character, into the filed GitHub issue
   body. No paraphrasing, no substitution, no reflowing. A packet that lacks
   any one of them verbatim is bounced at review:

   ```
   MUST edit only Wait-GitHubActions.ps1 plus this dispatch's own ai_handoffs/log artifacts
   MUST add workflowName to the gh run list JSON fields and allow expected workflow names to match workflowName as well as name
   MUST preserve default expected workflows from .github/workflows/*.yml; do not add CodeQL to the default list automatically
   MUST preserve branch/commit filtering, latest-run selection, timeout enforcement, and exit-code meanings
   MUST verify .\Wait-GitHubActions.ps1 -Repo RustCADs/RGE -Branch main -Commit <current-main-sha> -WorkflowName CodeQL -TimeoutMinutes 2 -PollSeconds 5 exits 0 on the current green main commit
   MUST run git status --short --untracked-files=no before and after EXEC and confirm only Wait-GitHubActions.ps1 plus this dispatch's own ai_handoffs/log artifacts changed
   ```

   **Done-criterion**:
   - `Wait-GitHubActions.ps1` matches explicit `-WorkflowName CodeQL`
     against the CodeQL run's `workflowName`.
   - Existing in-repo default workflow waiting still works for the current
     commit.
   - The explicit CodeQL regression command in the fifth MUST exits 0.
   - No tracked file outside `Wait-GitHubActions.ps1` changes, except this
     dispatch's own handoff/log artifacts.

31. **[DONE 2026-05-24 via PR #147 / commit `c09dddb`] Read-only preflight: golden-projects simple-scene scaffold.**
   The golden-project suite is product-facing regression infrastructure:
   `golden-projects/README.md` says `simple-scene/` should cover basic load,
   transform, camera, and light render. Today `golden-projects/simple-scene/`
   has a README and `.rge-project`, but the manifest's `scenes: []` list is
   empty and there is no scene file under the project. Before implementing a
   scaffold, audit the current data schema and loader/test surfaces so the
   follow-up is precise instead of inventing an unsupported golden-project
   format.

   This is a read-only audit. It must not add scene files, change manifests,
   write golden fixtures, add CI workflows, or edit source. The output is one
   5-question answer block naming the smallest safe task #32 route.

   **Runtime invocation note**: this task is a deliberate named +1 on top
   of the freeze-at-108 posture set by task #30. Run as
   `.\Invoke-AiDispatchAuto.ps1 -PublishMode branch -MaxAutonomousTasks 109`
   so the cap accommodates exactly this one dispatch. The scheduler
   remains disabled and must not be re-enabled by this task.

   **Allowed file surface**:
   - MAY add this dispatch's own `ai_handoffs/ISSUE-*_TASK_*.md`,
     `ai_handoffs/ISSUE-*_EXEC_*.md`, `ai_handoffs/ISSUE-*_CORRECT_*.md`
     packets plus `.meta.json` sidecars if produced by the orchestrator,
     and the queue-runner's own `ai_dispatch_logs/log_*.md`.
   - NO source, test, Cargo, workflow, script, doctrine, status, handoff
     rewrite, golden-project file, generated asset, or issue-label edit is
     allowed from the executor.

   **Read-only scope to inspect**:
   - `golden-projects/README.md`
   - `golden-projects/simple-scene/README.md`
   - `golden-projects/simple-scene/.rge-project`
   - Other `golden-projects/*/.rge-project` files for current placeholder
     conventions only
   - `crates/rge-data/src/**`
   - `crates/rge-data/tests/**`
   - `crates/rge-data/examples/bake_fixtures.rs`
   - Editor or shell project/scene loading code only as needed to identify
     real consumers; cite file/line evidence if inspected
   - `Status.md`, `HANDOFF.md`, and `change.md` as historical evidence
     only; do not edit them

   **Files that MUST NOT be touched**:
   - `golden-projects/**`
   - Any `crates/**` file
   - `editor/**`
   - `kernel/**`
   - `.github/**`
   - Any Cargo file (`Cargo.toml`, `Cargo.lock`, workspace manifests)
   - Any PowerShell script
   - Any doctrine/status/planning doc (`AI_DISPATCH_AUTOMATION.md`,
     `HANDOFF.md`, `Status.md`, `change.md`, ADRs, architecture docs,
     plans)
   - Any existing handoff packet or dispatch log
   - Any GitHub label or issue metadata except the queue runner's normal
     issue lifecycle for this dispatch

   **Five-question answer block**:
   The EXEC report must contain a literal
   `## 5-Question Golden Simple-Scene Preflight Answer Block`
   section with these exact Q1-Q5 headings:

   - `Q1. What exists today under golden-projects/simple-scene?`
     State the manifest, README, scene-list, and missing-file state with
     file/line evidence.
   - `Q2. What is the current rge-data project and scene schema surface?`
     Summarize the concrete `Project`, `Scene`, entity, component, relation,
     parse/serialize, and fixture conventions that a simple-scene scaffold
     must obey. Include file/line evidence.
   - `Q3. Which runtime or editor consumers would actually exercise a golden simple-scene today?`
     Identify whether a load-only fixture, render-frame smoke, bake-fixture
     path, or no current consumer exists. Classify candidates as
     `consumer-present`, `test-only-consumer`, `future-consumer`, or
     `needs-design`.
   - `Q4. What is the smallest safe follow-up if the scaffold is bounded?`
     If bounded, name exact allowed files, test additions, gates, MUST NOT
     list, and halt conditions. If not bounded, state the design decision
     required rather than inventing files.
   - `Q5. What should task #32 be?`
     Recommend exactly one of: a bounded scaffold implementation task, a
     narrower read-only audit, a docs-only clarification, or `NEEDS_HUMAN`.

   **Halt conditions**:
   - The audit requires source/test/Cargo/workflow/script/doc edits to
     answer the questions.
   - The answer requires inventing new rge-data schema, new component type
     IDs, new renderer expectations, or a new golden-project runner before
     a human chooses the policy.
   - The smallest follow-up would need generated binary assets, screenshot
     baselines, or a real renderer comparison harness.
   - The current schema can express only a load-only scene and the README's
     camera/light/render promise needs a broader design decision.
   - Any verification or repository-state check reveals tracked changes
     outside this dispatch's own handoff/log artifacts.

   **Verbatim review-gate strings** - the autonomous selector MUST copy
   these seven strings, character-for-character, into the filed GitHub issue
   body. No paraphrasing, no substitution, no reflowing. A packet that lacks
   any one of them verbatim is bounced at review:

   ```
   MUST be a read-only golden simple-scene preflight; do not edit source, tests, Cargo, workflows, scripts, doctrine, status docs, golden-project files, issues, labels, or existing packets
   MUST produce a 5-question Golden Simple-Scene Preflight Answer Block with Q1-Q5 headings exactly as specified in the brief
   MUST cite file/line evidence for the current simple-scene manifest state and the rge-data project/scene schema surface
   MUST classify candidate consumers as consumer-present, test-only-consumer, future-consumer, or needs-design
   MUST NOT propose generated binary assets, screenshot baselines, or a renderer comparison harness unless Q3 shows a current consumer already exists
   MUST end with NEEDS_HUMAN if the next step requires choosing a golden-project policy rather than applying an already-bounded scaffold
   MUST run git status --short --untracked-files=no before and after EXEC and confirm only this dispatch's own ai_handoffs/log artifacts changed
   ```

   **Done-criterion**:
   - EXEC report contains the exact five-question heading and Q1-Q5
     sub-headings.
   - Q1 states the current `golden-projects/simple-scene` state with
     file/line evidence.
   - Q2 states the current rge-data schema surface with file/line evidence.
   - Q3 classifies every plausible consumer using the required four labels.
   - Q4 names a bounded scaffold surface only if current schema and consumer
     pressure already make it safe.
   - Q5 recommends exactly one task #32 route or `NEEDS_HUMAN`.
   - No tracked source/test/Cargo/workflow/script/doc/status/golden-project
     file changes.

32. **[DONE-BLOCKED 2026-05-24 via PR #149 / commit `ee7c4a0`] Add schema-load-only golden simple-scene regression test.**
   Human policy decision after task #31: choose the schema-load-only rung of
   the golden-project evolution chain. Do not attempt load+tick, renderer
   comparison, screenshot baselines, cook output, asset loading, or typed
   component bridging in this task. The purpose is to build the first shared
   harness layer that later load+tick and render-comparison work can reuse.

   Add a test-only regression under `crates/rge-data/tests/` that reads the
   existing `golden-projects/simple-scene/.rge-project` manifest, parses it
   as the current `rge_data::Project` schema, and asserts the schema-level
   fields that are valid today. The current manifest may keep `scenes: []`;
   this task is not required to add a scene file. It must include a
   deliberate-break negative assertion proving the test fails if a required
   project field is renamed or removed.

   **Runtime invocation note**: this task is a deliberate named +1 on top
   of the freeze-at-109 posture set by task #31. Run as
   `.\Invoke-AiDispatchAuto.ps1 -PublishMode branch -MaxAutonomousTasks 110`
   so the cap accommodates exactly this one dispatch. The scheduler
   remains disabled and must not be re-enabled by this task.

   **Allowed file surface**:
   - EDIT or ADD exactly one integration test file under
     `crates/rge-data/tests/`, preferably
     `crates/rge-data/tests/golden_simple_scene_schema.rs`.
   - MAY add this dispatch's own `ai_handoffs/ISSUE-*_TASK_*.md`,
     `ai_handoffs/ISSUE-*_EXEC_*.md`, `ai_handoffs/ISSUE-*_CORRECT_*.md`
     packets plus `.meta.json` sidecars if produced by the orchestrator,
     and the queue-runner's own `ai_dispatch_logs/log_*.md`.

   **Files that MUST NOT be touched**:
   - `golden-projects/**`
   - Any `crates/**` file outside the single new/edited
     `crates/rge-data/tests/*.rs` integration test
   - `editor/**`
   - `kernel/**`
   - `.github/**`
   - Any Cargo file (`Cargo.toml`, `Cargo.lock`, workspace manifests)
   - Any PowerShell script
   - Any doctrine/status/planning doc (`AI_DISPATCH_AUTOMATION.md`,
     `HANDOFF.md`, `Status.md`, `change.md`, ADRs, architecture docs,
     plans)
   - Any existing handoff packet or dispatch log
   - Any GitHub label or issue metadata except the queue runner's normal
     issue lifecycle for this dispatch

   **Required test shape**:
   - Include the manifest via a stable relative path from the test file,
     for example `include_str!("../../../golden-projects/simple-scene/.rge-project")`.
   - Parse it as `rge_data::Project`.
   - Assert `name == "simple-scene"`, `version == SchemaVersion::V0_1_0`,
     `description` is non-empty, `target_tiers` contains `TargetTier::Desktop`,
     `plugins` is empty, and the `scenes` vector is present and currently
     empty.
   - Add a deliberate-break negative assertion by mutating the manifest text
     in memory to rename or remove one required field, then assert
     `ron::from_str::<Project>(...)` returns an error.
   - Do not add or require any `.rge-scene` file, renderer, GPU, asset-store,
     cook, screenshot, or typed-component behavior.

   **Halt conditions**:
   - The current `golden-projects/simple-scene/.rge-project` cannot parse as
     `rge_data::Project` without editing the manifest or production schema.
   - The test requires adding a new dependency, changing Cargo manifests, or
     changing the `rge-data` public schema.
   - The implementation wants to add a scene file, binary asset, screenshot
     baseline, renderer comparison, GPU test, asset-store integration, or
     typed component bridge.
   - Any tracked file outside the single allowed `crates/rge-data/tests/*.rs`
     test file changes, excluding this dispatch's own handoff/log artifacts.

   **Verbatim review-gate strings** - the autonomous selector MUST copy
   these seven strings, character-for-character, into the filed GitHub issue
   body. No paraphrasing, no substitution, no reflowing. A packet that lacks
   any one of them verbatim is bounced at review:

   ```
   MUST keep scope to a schema-load-only rge-data integration test
   MUST add or edit exactly one test file under crates/rge-data/tests and no production code
   MUST read golden-projects/simple-scene/.rge-project and parse it as rge_data::Project
   MUST assert the current schema-level fields including name, version, non-empty description, desktop target tier, empty plugins, and currently-empty scenes vector
   MUST add a deliberate-break negative test variant that mutates a required field in memory and asserts parsing fails
   MUST NOT touch renderer, GPU, asset-store, cook output, screenshot baselines, typed component bridging, golden-project files, Cargo files, workflows, scripts, doctrine, or status docs
   MUST run cargo test -p rge-data --test golden_simple_scene_schema and the canonical .ai/dispatch.verify.ps1 gate successfully
   ```

   **Done-criterion**:
   - One `crates/rge-data/tests/golden_simple_scene_schema.rs` test file
     exists, or one equivalent single integration test file under
     `crates/rge-data/tests/` is updated.
   - The test parses the existing golden simple-scene manifest and asserts
     the current schema-load-only contract.
   - The deliberate-break negative variant proves a required-field rename or
     removal fails parsing.
   - `cargo test -p rge-data --test golden_simple_scene_schema` exits 0.
   - `.ai/dispatch.verify.ps1` exits 0.
   - No tracked file outside the single `crates/rge-data/tests/*.rs` file
     changes, except this dispatch's own handoff/log artifacts.

33. **[DONE 2026-05-24 via PR #151 / commit `2035a00`] Reconcile golden project manifests to rge-data schema and add simple-scene schema-load test.**
   Task #32 proved the schema-load-only policy is correct but not yet
   implementable because every golden `.rge-project` manifest is still in
   placeholder form: `target_tiers: ["desktop"]` is rejected by the current
   `TargetTier` enum wire format, and the extra `schema_version` field is
   not part of `rge_data::Project`. The human policy choice is to align the
   placeholder golden manifests to the current `rge-data` schema rather
   than widening production schema for placeholder files.

   This is a bounded fixture/test reconciliation. Update all six golden
   project manifests to parse as `rge_data::Project`, then add the
   schema-load-only simple-scene integration test that task #32 attempted.
   Do not add scene files, renderer behavior, screenshot baselines, cook
   output, typed component bridging, or workflow changes.

   **Runtime invocation note**: this task is a deliberate named +1 on top
   of the freeze-at-110 posture set by task #32. Run as
   `.\Invoke-AiDispatchAuto.ps1 -PublishMode branch -MaxAutonomousTasks 111`
   so the cap accommodates exactly this one dispatch. The scheduler
   remains disabled and must not be re-enabled by this task.

   **Allowed file surface**:
   - EDIT these six files only:
     - `golden-projects/simple-scene/.rge-project`
     - `golden-projects/material-zoo/.rge-project`
     - `golden-projects/skinned-character/.rge-project`
     - `golden-projects/physics-puzzle/.rge-project`
     - `golden-projects/cad-parametric/.rge-project`
     - `golden-projects/stress-world/.rge-project`
   - ADD exactly one integration test file:
     `crates/rge-data/tests/golden_simple_scene_schema.rs`.
   - MAY add this dispatch's own `ai_handoffs/ISSUE-*_TASK_*.md`,
     `ai_handoffs/ISSUE-*_EXEC_*.md`, `ai_handoffs/ISSUE-*_CORRECT_*.md`
     packets plus `.meta.json` sidecars if produced by the orchestrator,
     and the queue-runner's own `ai_dispatch_logs/log_*.md`.

   **Manifest edits required**:
   - In each of the six golden `.rge-project` files, change the
     `target_tiers` entry from the quoted placeholder form to the current
     `TargetTier` RON enum wire form, e.g. `target_tiers: [desktop],` or
     the equivalent pretty-printed bare-identifier list.
   - In each of the six golden `.rge-project` files, remove the extra
     `schema_version: "0.1.0"` field because `Project` already carries
     `version: "0.1.0"` and has no separate `schema_version` field.
   - Preserve names, descriptions, plugins, and currently-empty `scenes`
     lists.

   **Test required**:
   - `crates/rge-data/tests/golden_simple_scene_schema.rs` includes
     `golden-projects/simple-scene/.rge-project` via a stable relative
     path.
   - It parses the manifest as `rge_data::Project`.
   - It asserts `name == "simple-scene"`, `version == SchemaVersion::V0_1_0`,
     `description` is non-empty, `target_tiers` contains `TargetTier::Desktop`,
     `plugins` is empty, and `scenes` is currently empty.
   - It adds a deliberate-break negative assertion by mutating the manifest
     text in memory to rename or remove one required field, then asserts
     `ron::from_str::<Project>(...)` returns an error.

   **Files that MUST NOT be touched**:
   - Any `golden-projects/**` file outside the six listed manifests
   - Any `crates/**` file outside
     `crates/rge-data/tests/golden_simple_scene_schema.rs`
   - `editor/**`
   - `kernel/**`
   - `.github/**`
   - Any Cargo file (`Cargo.toml`, `Cargo.lock`, workspace manifests)
   - Any PowerShell script
   - Any doctrine/status/planning doc (`AI_DISPATCH_AUTOMATION.md`,
     `HANDOFF.md`, `Status.md`, `change.md`, ADRs, architecture docs,
     plans)
   - Any existing handoff packet or dispatch log
   - Any GitHub label or issue metadata except the queue runner's normal
     issue lifecycle for this dispatch

   **Halt conditions**:
   - Any golden manifest still fails to parse as `rge_data::Project` after
     only the required placeholder-to-schema edits.
   - The implementation requires changing `rge-data` production schema,
     Cargo files, workflows, scripts, renderer code, asset-store code, or
     typed component bridging.
   - The implementation wants to add `.rge-scene` files, generated binary
     assets, screenshot baselines, cook output, or a renderer comparison
     harness.
   - The focused test or canonical verification gate fails for any reason
     outside the allowed file surface.

   **Verbatim review-gate strings** - the autonomous selector MUST copy
   these seven strings, character-for-character, into the filed GitHub issue
   body. No paraphrasing, no substitution, no reflowing. A packet that lacks
   any one of them verbatim is bounced at review:

   ```
   MUST align the six golden-projects/*.rge-project manifests to the current rge_data::Project schema without changing production schema
   MUST change quoted target_tiers placeholders to the current bare TargetTier enum wire form in all six golden manifests
   MUST remove the extra schema_version field from all six golden manifests while preserving the existing version field
   MUST add exactly one rge-data integration test file at crates/rge-data/tests/golden_simple_scene_schema.rs
   MUST keep the test schema-load-only and assert the current simple-scene Project fields plus a deliberate-break parse-failure variant
   MUST NOT add scene files, renderer/GPU behavior, asset-store behavior, cook output, screenshot baselines, typed component bridging, Cargo changes, workflow changes, scripts, doctrine, or status docs
   MUST run cargo test -p rge-data --test golden_simple_scene_schema and the canonical .ai/dispatch.verify.ps1 gate successfully
   ```

   **Done-criterion**:
   - All six golden `.rge-project` manifests parse as `rge_data::Project`.
   - `crates/rge-data/tests/golden_simple_scene_schema.rs` exists and
     proves the simple-scene manifest's schema-load-only contract.
   - The deliberate-break negative variant fails parsing as intended.
   - `cargo test -p rge-data --test golden_simple_scene_schema` exits 0.
   - `.ai/dispatch.verify.ps1` exits 0.
   - No tracked file outside the six listed manifests plus the single new
     test file changes, except this dispatch's own handoff/log artifacts.

34. **[DONE 2026-05-24 via PR #153 / commit `a15086d`] Add first simple-scene `.rge-scene` fixture and schema-load scene-path test.**
   Task #33 made every golden project manifest parse as `rge_data::Project`
   and added a schema-load-only test for the simple-scene manifest. The next
   rung is still schema-only: make `golden-projects/simple-scene` contain
   exactly one current-schema `.rge-scene` file, reference it from the
   manifest, and extend the existing rge-data integration test to load the
   referenced scene path and parse it as `rge_data::Scene`.

   This is not the load+tick rung. Do not instantiate editor state, systems,
   renderer, GPU resources, asset-store, cook output, screenshot baselines,
   or typed component bridging. The scene file may contain one simple root
   entity with a reflection-neutral `ComponentValue` payload, but the test
   must assert only the `rge_data` schema envelope.

   **Runtime invocation note**: this task is a deliberate named +1 on top
   of the freeze-at-111 posture set by task #33. Run as
   `.\Invoke-AiDispatchAuto.ps1 -PublishMode branch -MaxAutonomousTasks 112`
   so the cap accommodates exactly this one dispatch. The scheduler
   remains disabled and must not be re-enabled by this task.

   **Allowed file surface**:
   - EDIT `golden-projects/simple-scene/.rge-project` only to replace the
     currently-empty `scenes: []` list with exactly one relative scene path,
     preferably `"scenes/main.rge-scene"`.
   - ADD exactly one scene fixture file under
     `golden-projects/simple-scene/scenes/`, preferably
     `golden-projects/simple-scene/scenes/main.rge-scene`.
   - EDIT exactly one existing test file:
     `crates/rge-data/tests/golden_simple_scene_schema.rs`.
   - MAY add this dispatch's own `ai_handoffs/ISSUE-*_TASK_*.md`,
     `ai_handoffs/ISSUE-*_EXEC_*.md`, `ai_handoffs/ISSUE-*_CORRECT_*.md`
     packets plus `.meta.json` sidecars if produced by the orchestrator,
     and the queue-runner's own `ai_dispatch_logs/log_*.md`.

   **Scene fixture shape**:
   - The new `.rge-scene` file must parse as current `rge_data::Scene`.
   - It must use `version: "0.1.0"` and a stable name such as `"main"`.
   - It must contain at least one root entity so the scene is not a pure
     empty placeholder.
   - Entity ids must use the current `EntityId` wire form already accepted
     by `rge-data` fixtures.
   - Component payloads, if any, must stay reflection-neutral
     `ComponentValue` strings. Do not introduce typed component parsing.

   **Test required**:
   - Keep the existing project schema assertions for `simple-scene`, updated
     to expect exactly one scene path instead of an empty scenes vector.
   - Resolve the scene path relative to `golden-projects/simple-scene/`.
   - Read the referenced `.rge-scene` file and parse it as
     `rge_data::Scene`.
   - Assert scene-level schema facts: version, name, non-empty entities,
     non-empty roots, and that every root entity id exists in the entities
     list.
   - Keep the existing deliberate-break project parse-failure variant and
     add a scene deliberate-break parse-failure variant by mutating a required
     scene field in memory.

   **Files that MUST NOT be touched**:
   - Any `golden-projects/**` file outside
     `golden-projects/simple-scene/.rge-project` and the single new
     `golden-projects/simple-scene/scenes/main.rge-scene` fixture
   - Any `crates/**` file outside
     `crates/rge-data/tests/golden_simple_scene_schema.rs`
   - `editor/**`
   - `kernel/**`
   - `.github/**`
   - Any Cargo file (`Cargo.toml`, `Cargo.lock`, workspace manifests)
   - Any PowerShell script
   - Any doctrine/status/planning doc (`AI_DISPATCH_AUTOMATION.md`,
     `HANDOFF.md`, `Status.md`, `change.md`, ADRs, architecture docs,
     plans)
   - Any existing handoff packet or dispatch log
   - Any GitHub label or issue metadata except the queue runner's normal
     issue lifecycle for this dispatch

   **Halt conditions**:
   - The scene fixture requires changing `rge-data` production schema,
     EntityId parsing, Cargo files, workflows, scripts, renderer code,
     asset-store code, or typed component bridging.
   - The implementation wants to add more than one scene file, any binary
     asset, screenshot baseline, cook output, renderer comparison, GPU test,
     editor runtime load, or system tick.
   - The test cannot resolve and parse the referenced scene using only the
     existing `rge_data::Project` and `rge_data::Scene` schema.
   - The focused test or canonical verification gate fails for any reason
     outside the allowed file surface.

   **Verbatim review-gate strings** - the autonomous selector MUST copy
   these seven strings, character-for-character, into the filed GitHub issue
   body. No paraphrasing, no substitution, no reflowing. A packet that lacks
   any one of them verbatim is bounced at review:

   ```
   MUST keep scope to schema-load-only project-to-scene loading under rge-data tests
   MUST edit only golden-projects/simple-scene/.rge-project, add exactly one golden-projects/simple-scene/scenes/main.rge-scene fixture, and edit crates/rge-data/tests/golden_simple_scene_schema.rs
   MUST update simple-scene scenes from empty to exactly one relative scene path and resolve that path from the test
   MUST parse the referenced .rge-scene as rge_data::Scene and assert schema facts only
   MUST keep both project and scene deliberate-break parse-failure variants
   MUST NOT add load+tick, editor runtime, renderer/GPU behavior, asset-store behavior, cook output, screenshot baselines, typed component bridging, Cargo changes, workflow changes, scripts, doctrine, or status docs
   MUST run cargo test -p rge-data --test golden_simple_scene_schema and the canonical .ai/dispatch.verify.ps1 gate successfully
   ```

   **Done-criterion**:
   - `golden-projects/simple-scene/.rge-project` lists exactly one scene path.
   - `golden-projects/simple-scene/scenes/main.rge-scene` exists and parses
     as current `rge_data::Scene`.
   - `crates/rge-data/tests/golden_simple_scene_schema.rs` loads the project,
     resolves the scene path, parses the scene, and asserts schema-only facts.
   - Both deliberate-break variants fail parsing as intended.
   - `cargo test -p rge-data --test golden_simple_scene_schema` exits 0.
   - `.ai/dispatch.verify.ps1` exits 0.
   - No tracked file outside the one manifest, one scene fixture, and one
     test file changes, except this dispatch's own handoff/log artifacts.

35. **[DONE 2026-05-24 via PR #155 / commit `dbe6f84`] Audit golden simple-scene load+tick harness shape.**
   Task #34 landed the first non-empty schema-load-only
   `golden-projects/simple-scene` fixture: a manifest with one scene path,
   a current-schema `.rge-scene`, and an rge-data test that parses both.
   The next rung in the evolution chain is load+tick, but current search
   suggests there may be no existing `rge_data::Scene` -> runtime `World`
   bridge or golden-project consumer. This task is a read-only preflight to
   determine the smallest safe follow-up before any implementation.

   **Runtime invocation note**: this task is a deliberate named +1 on top
   of the freeze-at-112 posture set by task #34. Run as
   `.\Invoke-AiDispatchAuto.ps1 -PublishMode branch -MaxAutonomousTasks 113`
   so the cap accommodates exactly this one dispatch. The scheduler
   remains disabled and must not be re-enabled by this task.

   **Scope (read-only)**:
   - `golden-projects/simple-scene/**`
   - `crates/rge-data/src/**`
   - `crates/rge-data/tests/**`
   - `kernel/ecs/**`
   - `kernel/events/**`
   - `kernel/schedule/**`
   - `kernel/plugin-host/**`
   - `crates/editor-shell/**`
   - `editor/rge-editor/**`
   - `crates/script-host/**`
   - `crates/script-bench/**`
   - Cross-reference task #31, #33, and #34 handoff packets only as
     historical context if useful.

   **Allowed file surface**:
   - This is read-only. The dispatch may add only its own
     `ai_handoffs/ISSUE-*_TASK_*.md`, `ai_handoffs/ISSUE-*_EXEC_*.md`,
     optional `ai_handoffs/ISSUE-*_CORRECT_*.md`, `.meta.json` sidecars,
     and `ai_dispatch_logs/log_*.md`.
   - No source, test, golden-project, Cargo, workflow, script, doctrine,
     status, plan, or README file may be edited.

   **Required answer format**:
   - The EXEC report must include the exact heading
     `## 5-Question Load+Tick Preflight Answer Block`.
   - It must answer Q1 through Q5 with file/line evidence.

   **Questions to answer**:
   1. What exactly does `golden-projects/simple-scene` contain after task #34,
      and which schema-only facts are now validated?
   2. Which existing code paths, if any, already load `rge_data::Project` /
      `rge_data::Scene` into a runtime or editor structure that can be ticked?
      Classify each candidate as `usable-now`, `schema-only`, `different-scene-type`,
      `renderer-only`, or `not-a-consumer`.
   3. Does a direct `rge_data::Scene` -> `rge_kernel_ecs::World` bridge already
      exist? If not, what exact bridge shape would be needed before load+tick
      can be meaningful?
   4. What would a renderer-free, GPU-free load+tick regression assert on the
      current simple-scene fixture without typed component bridging or asset
      loading?
   5. What is the smallest safe follow-up dispatch: implement an existing
      load+tick path, add a narrow schema-to-World bridge, add a narrower
      pre-bridge test, or stop with `NEEDS_HUMAN` because the next step is an
      architecture decision?

   **Halt conditions**:
   - If answering Q3 or Q4 requires writing code, changing fixture shape, or
     inventing a new bridge during this dispatch, halt with `NEEDS_HUMAN`.
   - If the only viable follow-up requires renderer/GPU, asset-store, cook
     output, screenshot baselines, typed component bridging, or editor UI,
     halt with `NEEDS_HUMAN`.
   - If verify fails on a target outside this audit scope, halt with
     `NEEDS_HUMAN` rather than fixing it.
   - If the audit cannot be answered in one EXEC packet, halt with
     `NEEDS_HUMAN`.

   **Verbatim review-gate strings** - the autonomous selector MUST copy
   these seven strings, character-for-character, into the filed GitHub issue
   body. No paraphrasing, no substitution, no reflowing. A packet that lacks
   any one of them verbatim is bounced at review:

   ```
   MUST be a read-only preflight audit; do not modify source, tests, golden-project files, Cargo files, workflows, scripts, doctrine, status docs, or existing handoff/log artifacts
   MUST include the exact heading ## 5-Question Load+Tick Preflight Answer Block and answer Q1 through Q5
   MUST inspect golden-projects/simple-scene, rge-data Project/Scene, kernel ECS/events/schedule/plugin-host, editor-shell/editor, and script-host/script-bench surfaces with file/line evidence
   MUST classify candidate load+tick paths as usable-now, schema-only, different-scene-type, renderer-only, or not-a-consumer
   MUST determine whether a direct rge_data::Scene to rge_kernel_ecs::World bridge exists before recommending implementation
   MUST recommend exactly one smallest safe follow-up dispatch or NEEDS_HUMAN
   MUST halt rather than fix if the audit reveals an implementation requirement outside the read-only scope
   ```

   **Done-criterion**:
   - The EXEC packet contains the exact required heading and Q1-Q5 answers.
   - Q2 classifies every plausible existing path with concrete file/line
     evidence.
   - Q3 states whether the bridge already exists or what shape is missing.
   - Q5 names exactly one smallest next dispatch or `NEEDS_HUMAN`.
   - `git status --short --untracked-files=no` is clean before and after
     execution, except for this dispatch's own packet/log artifacts.

36. **[DONE 2026-05-24 via PR #157 / commit `d2a679f`] Add simple-scene minimal load+tick regression with test-local Scene to World bridge.**
   Task #35 found no existing `rge_data::Scene` -> runtime consumer, but it
   identified a bounded first load+tick step that does not choose the
   production bridge architecture: add a test-local identity-only bridge in
   a new `rge-data` integration test. The bridge copies only entity ids from
   parsed `rge_data::Scene` into a fresh `rge_kernel_ecs::World`, then asserts
   the world is tickable. This is intentionally not a production loader.

   **Runtime invocation note**: this task is a deliberate named +1 on top
   of the freeze-at-113 posture set by task #35. Run as
   `.\Invoke-AiDispatchAuto.ps1 -PublishMode branch -MaxAutonomousTasks 114`
   so the cap accommodates exactly this one dispatch. The scheduler
   remains disabled and must not be re-enabled by this task.

   **Allowed file surface**:
   - ADD exactly one new integration test file:
     `crates/rge-data/tests/golden_simple_scene_load_tick.rs`.
   - EDIT `crates/rge-data/Cargo.toml` only to add
     `rge-kernel-ecs = { workspace = true }` under `[dev-dependencies]`.
   - MAY edit `Cargo.lock` only for the mechanical `rge-kernel-ecs`
     dependency edge under the `rge-data` package, if Cargo updates the lock.
   - MAY add this dispatch's own `ai_handoffs/ISSUE-*_TASK_*.md`,
     `ai_handoffs/ISSUE-*_EXEC_*.md`, `ai_handoffs/ISSUE-*_CORRECT_*.md`
     packets plus `.meta.json` sidecars if produced by the orchestrator,
     and the queue-runner's own `ai_dispatch_logs/log_*.md`.

   **Test required**:
   - Read `golden-projects/simple-scene/.rge-project` and parse it as
     `rge_data::Project`.
   - Resolve the first scene path relative to `golden-projects/simple-scene/`
     and parse that `.rge-scene` as `rge_data::Scene`.
   - Define a helper inside the test file, not production code, that builds
     `rge_kernel_ecs::World::new()` and calls
     `world.spawn_with_id(rge_kernel_ecs::EntityId::from_ulid(*entity.id.as_ulid()))`
     once for each `scene.entities` entry.
   - Assert `world.entity_count() == scene.entities.len()`.
   - For each scene entity id, assert the converted ECS entity exists in the
     world via the public `World::entity(...)` surface.
   - Capture `current_tick()` and `last_tick()`, call `advance_tick()`, and
     assert `current_tick()` incremented by one and `last_tick()` equals the
     pre-advance tick.
   - Assert this test intentionally ignores `ComponentValue` payloads and
     scene relations until a production bridge decision is made.

   **Files that MUST NOT be touched**:
   - Any `golden-projects/**` file
   - Any `crates/rge-data/src/**` production source
   - Any `crates/rge-data/tests/**` file outside the single new
     `golden_simple_scene_load_tick.rs`
   - Any `kernel/**`, `editor/**`, `crates/editor-shell/**`,
     `crates/script-host/**`, or `crates/script-bench/**` file
   - Any Cargo file outside `crates/rge-data/Cargo.toml` and the narrowly
     allowed `Cargo.lock` edge
   - Any workflow under `.github/**`
   - Any PowerShell script
   - Any doctrine/status/planning doc (`AI_DISPATCH_AUTOMATION.md`,
     `HANDOFF.md`, `Status.md`, `change.md`, ADRs, architecture docs,
     plans)
   - Any existing handoff packet or dispatch log
   - Any GitHub label or issue metadata except the queue runner's normal
     issue lifecycle for this dispatch

   **Cargo.lock policy**:
   - If `Cargo.lock` changes, the only acceptable diff is adding
     `rge-kernel-ecs` to the dependency list for the `rge-data` package.
   - Any package version, checksum, source, or unrelated dependency change
     must halt with `NEEDS_HUMAN`.

   **Halt conditions**:
   - The implementation requires production code, a production crate
     dependency, a new workspace member, or moving the bridge outside the
     new test file.
   - The implementation requires typed component parsing, asset loading,
     renderer/GPU behavior, cook output, screenshot baselines, editor UI,
     script execution, relation storage, or schedule/plugin-host integration.
   - The test cannot prove load+tick using only parsed `rge_data::Scene`,
     `rge_kernel_ecs::World`, identity conversion, `entity_count`,
     `World::entity`, `current_tick`, `last_tick`, and `advance_tick`.
   - Cargo.lock changes beyond the single allowed edge.
   - The focused test or canonical verification gate fails for any reason
     outside the allowed file surface.

   **Verbatim review-gate strings** - the autonomous selector MUST copy
   these seven strings, character-for-character, into the filed GitHub issue
   body. No paraphrasing, no substitution, no reflowing. A packet that lacks
   any one of them verbatim is bounced at review:

   ```
   MUST add exactly one new test file at crates/rge-data/tests/golden_simple_scene_load_tick.rs
   MUST add rge-kernel-ecs as a dev-dependency of rge-data only, not a production dependency
   MUST implement the rge_data::Scene to rge_kernel_ecs::World bridge as a test-local helper only
   MUST load golden-projects/simple-scene through the Project scene path, spawn one ECS entity per parsed scene entity, and assert entity_count plus entity existence
   MUST assert World advance_tick updates current_tick and last_tick on the loaded world
   MUST NOT touch golden-project files, production source, kernel/editor/script crates, renderer/GPU, asset-store, cook output, screenshot baselines, typed component bridging, workflows, scripts, doctrine, or status docs
   MUST run cargo test -p rge-data --test golden_simple_scene_load_tick and the canonical .ai/dispatch.verify.ps1 gate successfully
   ```

   **Done-criterion**:
   - The new test file exists and is the only `crates/rge-data/tests/**`
     file changed by this dispatch.
   - `crates/rge-data/Cargo.toml` has exactly one new dev-dependency edge
     to `rge-kernel-ecs`.
   - The test parses the existing simple-scene project and referenced scene,
     bridges identities into a fresh ECS World, verifies entity count and
     entity existence, advances the world tick, and asserts tick bookkeeping.
   - `cargo test -p rge-data --test golden_simple_scene_load_tick` exits 0.
   - `.ai/dispatch.verify.ps1` exits 0.
   - No tracked file outside the allowed test file, `crates/rge-data/Cargo.toml`,
     and narrowly allowed `Cargo.lock` edge changes, except this dispatch's
     own handoff/log artifacts.

37. **[DONE-BLOCKED 2026-05-24 via PR #159 / commit `4bd1a23`] Read-only preflight: typed component payload shape for golden simple-scene.**
   Audit landed; EXEC was `NEEDS_HUMAN` because the audit could only prove
   the canonical `ComponentValue.type_id` string for `Transform`. The arbiter
   decision was recorded in issue #160 (closed): use rge-data scene-envelope
   strings (`"rge::components::Camera"`, `"rge::components::Light"`,
   `"rge::components::Visibility"`), explicit `Visibility::Visible` on the
   camera entity, and `fov_y_radians: 1.0471976`. Task #38 below implements
   the bounded fixture + schema-only test follow-up against those decisions.

   *(original task brief preserved below for context)*
   Task #36 proved a renderer-free load+tick path for
   `golden-projects/simple-scene` by parsing the project and scene, copying
   scene entity ids into a test-local ECS World, and advancing the world tick.
   The next rung toward the README's "basic load + transform + camera + light
   render" target is adding typed component payloads to the fixture, but the
   current production bridge still intentionally ignores `ComponentValue`
   payloads. This task is a read-only preflight to determine the exact safe
   payload shape before any fixture or loader edit.

   **Runtime invocation note**: this task is a deliberate named +1 on top
   of the freeze-at-114 posture set by task #36. Run as
   `.\Invoke-AiDispatchAuto.ps1 -PublishMode branch -MaxAutonomousTasks 115`
   so the cap accommodates exactly this one dispatch. The scheduler
   remains disabled and must not be re-enabled by this task.

   **Scope (read-only)**:
   - `golden-projects/simple-scene/**`
   - `crates/rge-data/src/**`
   - `crates/rge-data/tests/**`
   - `crates/components-spatial/**`
   - `crates/components-render/**`
   - `crates/components-visibility/**`
   - `kernel/types/**`
   - `crates/macros-reflect/**`
   - Cross-reference task #31 through #36 handoff packets only as
     historical context if useful.

   **Allowed file surface**:
   - This is read-only. The dispatch may add only its own
     `ai_handoffs/ISSUE-*_TASK_*.md`, `ai_handoffs/ISSUE-*_EXEC_*.md`,
     optional `ai_handoffs/ISSUE-*_CORRECT_*.md`, `.meta.json` sidecars,
     and `ai_dispatch_logs/log_*.md`.
   - No source, test, golden-project, Cargo, workflow, script, doctrine,
     status, plan, or README file may be edited.

   **Required answer format**:
   - The EXEC report must include the exact heading
     `## 5-Question Typed Component Payload Preflight Answer Block`.
   - It must answer Q1 through Q5 with file/line evidence.

   **Questions to answer**:
   1. What exactly does `golden-projects/simple-scene` contain after task #36,
      and which "transform + camera + light" fixture facts are still missing?
   2. Which current component crates define serializable Transform, Camera,
      Light, and Visibility payloads, and what exact RON payload strings would
      round-trip for a minimal simple-scene fixture?
   3. What should the `ComponentValue.type_id` strings be for those payloads?
      Identify any existing convention, test, or comment for canonical type
      paths versus `kernel/types::TypeId` values.
   4. Can raw `ComponentValue` payloads be added to the simple-scene fixture
      now without typed component bridging or production loader changes, and
      what schema-only tests would validate parse/round-trip behavior?
   5. What is the smallest safe follow-up dispatch: add raw ComponentValue
      fixture payloads plus schema tests, run a narrower preflight, or stop
      with `NEEDS_HUMAN` because type identity or payload encoding is an
      architecture decision?

   **Halt conditions**:
   - If `ComponentValue.type_id` naming cannot be inferred from current code,
     comments, tests, or protocol docs, halt with `NEEDS_HUMAN`.
   - If the next useful step requires production loader code, typed component
     bridging, renderer/GPU behavior, asset loading, cook output, screenshot
     baselines, editor UI, or a new workspace dependency, halt with
     `NEEDS_HUMAN`.
   - If answering the audit requires editing source, tests, fixtures, Cargo
     files, workflows, scripts, or doctrine, halt with `NEEDS_HUMAN`.
   - If verify fails on a target outside this audit scope, halt with
     `NEEDS_HUMAN` rather than fixing it.
   - If the audit cannot be answered in one EXEC packet, halt with
     `NEEDS_HUMAN`.

   **Verbatim review-gate strings** - the autonomous selector MUST copy
   these eight strings, character-for-character, into the filed GitHub issue
   body. No paraphrasing, no substitution, no reflowing. A packet that lacks
   any one of them verbatim is bounced at review:

   ```
   MUST be a read-only preflight audit; do not modify source, tests, golden-project files, Cargo files, workflows, scripts, doctrine, status docs, or existing handoff/log artifacts
   MUST include the exact heading ## 5-Question Typed Component Payload Preflight Answer Block and answer Q1 through Q5
   MUST answer Q1 current simple-scene contents and missing transform/camera/light facts, Q2 exact Transform/Camera/Light/Visibility RON payload strings, Q3 canonical ComponentValue.type_id strings, Q4 raw-payload fixture safety plus schema-only tests, and Q5 exactly one next dispatch or NEEDS_HUMAN
   MUST inspect golden-projects/simple-scene, rge-data ComponentValue/Scene tests, components-spatial, components-render, components-visibility, kernel/types, and macros-reflect surfaces with file/line evidence
   MUST identify exact candidate RON payload strings for Transform, Camera, Light, and Visibility or halt with NEEDS_HUMAN if any cannot be justified from current code
   MUST determine the canonical ComponentValue.type_id strings before recommending any fixture implementation
   MUST recommend exactly one smallest safe follow-up dispatch or NEEDS_HUMAN
   MUST halt rather than fix if the audit reveals an implementation requirement outside the read-only scope
   ```

   **Done-criterion**:
   - The EXEC packet contains the exact required heading and Q1-Q5 answers.
   - Q2 names the component crate/file evidence and exact candidate RON
     payload strings for Transform, Camera, Light, and Visibility, or explains
     why a specific payload cannot be justified.
   - Q3 states the exact `ComponentValue.type_id` strings to use, or states
     `NEEDS_HUMAN` if the current repo does not define a canonical convention.
   - Q4 states whether adding raw payloads to the fixture is schema-only safe
     without production bridge work, and names the schema-only tests that would
     validate it.
   - Q5 names exactly one smallest next dispatch or `NEEDS_HUMAN`.
   - `git status --short --untracked-files=no` is clean before and after
     execution, except for this dispatch's own packet/log artifacts.

38. **[DONE 2026-05-25 via PR #162 / commit `1bd5b8d`] Add typed `ComponentValue` payloads to golden simple-scene fixture and extend schema test.**
   Task #37 produced a `NEEDS_HUMAN` audit for typed component payload
   shape; issue #160 closed the arbiter decision with the rge-data
   scene-envelope convention for `ComponentValue.type_id` strings, explicit
   `Visibility::Visible` on the camera entity, and the `FRAC_PI_3`-compatible
   `f32` literal `1.0471976` for `Camera.fov_y_radians`. This task implements
   the bounded fixture + schema-only test follow-up against those decisions.
   No typed bridging, no renderer/GPU, no asset loading, no production
   loader changes.

   **Runtime invocation note**: this task is a deliberate named +1 on top
   of the freeze-at-115 posture set by task #37. Run as
   `.\Invoke-AiDispatchAuto.ps1 -PublishMode branch -MaxAutonomousTasks 116`
   so the cap accommodates exactly this one dispatch. The scheduler
   remains disabled and must not be re-enabled by this task.

   **Arbiter decisions to encode** (from #160 resolution; these are
   scene-envelope identity strings, NOT Rust module paths and NOT final
   Reflect runtime identity):
   - Canonical `ComponentValue.type_id` strings:
       - `"rge::components::Transform"`
       - `"rge::components::Camera"`
       - `"rge::components::Light"`
       - `"rge::components::Visibility"`
   - Camera entity carries `Visibility::Visible` explicitly (fixture
     readability and schema pinning); not the `Inherited` default.
   - Camera projection uses `fov_y_radians: 1.0471976` as the
     `FRAC_PI_3`-compatible `f32` literal already justified by the #158
     audit (`crates/components-render/src/camera.rs:31-39`).

   **Allowed file surface**:
   - EDIT `golden-projects/simple-scene/scenes/main.rge-scene` to add
     typed `ComponentValue` payloads to the existing `Camera` entity and
     to add one new `Light` entity (both top-level roots; no relations).
   - EDIT `crates/rge-data/tests/golden_simple_scene_schema.rs` to extend
     the existing `simple_scene_referenced_scene_parses_as_scene` test
     and to add new tests asserting per-entity `ComponentValue` count,
     verbatim `type_id` strings, and verbatim raw `data` strings.
   - MAY add this dispatch's own `ai_handoffs/ISSUE-*_TASK_*.md`,
     `ai_handoffs/ISSUE-*_EXEC_*.md`, `ai_handoffs/ISSUE-*_CORRECT_*.md`
     packets plus `.meta.json` sidecars if produced by the orchestrator,
     and the queue-runner's own `ai_dispatch_logs/log_*.md`.

   **Fixture content required**:
   - Camera entity (preserve existing id `"0000000000000G000000000000"`,
     name `"Camera"`, empty `relations: []`) gains three components, in
     this order:
       - `ComponentValue(type_id: "rge::components::Transform", data: "(translation:(0.0,0.0,5.0),rotation:(0.0,0.0,0.0,1.0),scale:(1.0,1.0,1.0))")`
       - `ComponentValue(type_id: "rge::components::Camera", data: "(projection:Perspective(fov_y_radians:1.0471976,near:0.05,far:1000.0),viewport:(0.0,0.0,1.0,1.0),priority:0,is_active:true)")`
       - `ComponentValue(type_id: "rge::components::Visibility", data: "Visible")`
   - New Light entity (any valid 26-char ULID that does NOT collide with
     the Camera entity's id; name `"Light"`; empty `relations: []`) gains
     three components, in this order:
       - `ComponentValue(type_id: "rge::components::Transform", data: "(translation:(0.0,0.0,0.0),rotation:(0.0,0.0,0.0,1.0),scale:(1.0,1.0,1.0))")`
       - `ComponentValue(type_id: "rge::components::Light", data: "(color:(1.0,1.0,1.0),kind:Directional(illuminance_lux:100000.0),affects_indirect:true)")`
       - `ComponentValue(type_id: "rge::components::Visibility", data: "Visible")`
   - `root_entities` MUST list both the existing Camera id and the new
     Light id; both are top-level roots.

   **Schema test changes required**:
   - Replace the current `assert!(root.components.is_empty(), "schema fixture stays untyped")`
     line (currently at
     `crates/rge-data/tests/golden_simple_scene_schema.rs:114`) with
     assertions that the Camera entity has exactly three components, in
     the order above, with verbatim `(type_id, data)` pairs.
   - Add a new test that locates the `Light` entity by name in
     `scene.entities`, asserts it has exactly three components in the
     order above with verbatim `(type_id, data)` pairs, and asserts its
     `relations` is empty.
   - Add a new test that iterates every `ComponentValue` in every entity
     and asserts `ron::from_str::<ron::Value>(&cv.data)` returns
     `Ok(_)` for shape integrity. This uses only the existing `ron`
     workspace dep already available to `rge-data` tests; do NOT add
     any component crate to `rge-data`'s dev-deps.
   - Update entity-count and root-count assertions to reflect two
     entities and two root entities.

   **Files that MUST NOT be touched**:
   - Any `crates/rge-data/src/**` production source.
   - Any other `crates/rge-data/tests/**` file (extend the existing
     schema test only; do not touch `golden_simple_scene_load_tick.rs`,
     `round_trip.rs`, or any other test file).
   - Any other `golden-projects/**` file (`cad-parametric`,
     `material-zoo`, `physics-puzzle`, `skinned-character`,
     `stress-world`, or the `simple-scene` `README.md` /
     `.rge-project` manifest).
   - Any `crates/components-spatial/**`, `crates/components-render/**`,
     `crates/components-visibility/**`, or any other component crate
     source/test/Cargo file. This task is schema-only and does NOT pull
     component crates into `rge-data`'s dev-deps.
   - Any `kernel/**`, `editor/**`, `crates/editor-shell/**`,
     `crates/script-host/**`, `crates/script-bench/**`,
     `crates/macros-reflect/**`, `crates/gfx/**`, `crates/brep-render/**`,
     any `crates/io-*/**`, `crates/asset-store/**`, or any other crate.
   - Any Cargo file (`Cargo.toml`, `Cargo.lock`, workspace manifests,
     feature flags).
   - Any workflow under `.github/**`.
   - Any PowerShell script.
   - Any doctrine / status / planning doc
     (`AI_DISPATCH_AUTOMATION.md`, `HANDOFF.md`, `Status.md`,
     `change.md`, ADRs, architecture docs, plans, READMEs).
   - Any existing handoff packet or dispatch log.
   - Any GitHub label or issue metadata except the queue runner's
     normal issue lifecycle for this dispatch.

   **Cargo.lock policy**:
   - Zero Cargo metadata changes. If `Cargo.toml` or `Cargo.lock`
     changes at all, halt with `NEEDS_HUMAN`.

   **Halt conditions**:
   - Adding the typed payloads requires touching production code in
     `crates/rge-data/src/**`, any component crate, or any other
     production source.
   - The schema test requires a new dependency (e.g. any component
     crate in dev-deps). Use `ron::Value` for raw-payload shape
     validation; the existing `ron` workspace dep is sufficient.
   - The fixture or test requires typed component bridging, an
     `rge_data::Scene` -> ECS payload bridge, asset loading, cook
     output, renderer/GPU behavior, screenshot baselines, editor UI,
     script execution, or any production loader change.
   - Any `(type_id, data)` string would deviate from the arbiter
     decisions above. Use the exact verbatim strings; do NOT
     substitute Rust crate-path strings, `kernel/types::TypeId` hash
     forms, or any other convention.
   - The Camera entity's existing id `"0000000000000G000000000000"`
     would need to change. Preserve it verbatim; only add components
     and add the new Light entity alongside.
   - The fixture would need to rename the Camera entity, drop the
     existing root invariant, or break any other invariant the current
     `simple_scene_referenced_scene_parses_as_scene` test asserts
     besides the `root.components.is_empty()` flip. Extend assertions;
     do not rewrite history.
   - Cargo.lock changes for any reason.
   - The focused test or canonical verification gate fails for any
     reason outside the allowed file surface.

   **Scope-preserving halt clause** - the orchestrator's canonical
   verify gate (`.ai/dispatch.verify.ps1`) runs after Claude execute.
   If verify fails on a target OUTSIDE the allowed file surface
   (anything beyond
   `golden-projects/simple-scene/scenes/main.rge-scene`,
   `crates/rge-data/tests/golden_simple_scene_schema.rs`, or this
   dispatch's own `ai_handoffs/` packet), the orchestrator may
   auto-route a CORRECTION packet asking the executor to fix the
   failure. When that happens **the executor MUST halt**: write an
   EXECUTION_REPORT with `EXEC_STATUS: blocked` and
   `STATUS: NEEDS_HUMAN`, do NOT execute the correction. Scope
   discipline is the entire reason this task is bounded narrowly;
   a correction-round source fix to an unrelated failure expands a
   fixture + test dispatch into a source-fix dispatch and must
   become its own ticket.

   **Verbatim review-gate strings** - the autonomous selector MUST
   copy these eight strings, character-for-character, into the filed
   GitHub issue body. No paraphrasing, no substitution, no
   reflowing. A packet that lacks any one of them verbatim is
   bounced at review:

   ```
   MUST use the rge-data scene-envelope ComponentValue.type_id strings exactly: "rge::components::Transform", "rge::components::Camera", "rge::components::Light", "rge::components::Visibility"
   MUST add Visibility::Visible explicitly on the camera entity, not rely on the Inherited default
   MUST use fov_y_radians: 1.0471976 as the FRAC_PI_3-compatible f32 literal for the camera's Perspective projection
   MUST edit only golden-projects/simple-scene/scenes/main.rge-scene and crates/rge-data/tests/golden_simple_scene_schema.rs (except the dispatch's own ai_handoffs/ packet)
   MUST NOT pull any component crate (components-spatial, components-render, components-visibility) into rge-data's dev-dependencies; use ron::Value for raw-payload shape validation
   MUST NOT touch any kernel, editor, script, gfx, brep-render, io-*, asset-store, macros-reflect, or any production source crate
   MUST NOT modify Cargo.toml or Cargo.lock; halt with NEEDS_HUMAN if either changes
   MUST run cargo test -p rge-data --test golden_simple_scene_schema and the canonical .ai/dispatch.verify.ps1 gate successfully
   ```

   **Done-criterion**:
   - `golden-projects/simple-scene/scenes/main.rge-scene` contains the
     existing Camera entity (id `"0000000000000G000000000000"`, name
     `"Camera"`, empty relations) with three `ComponentValue` payloads
     using the four arbiter-approved canonical `type_id` strings in the
     order above, and a new Light entity (valid ULID id distinct from
     Camera, name `"Light"`, empty relations) with three
     `ComponentValue` payloads including a directional light in the
     order above. `root_entities` lists both ids.
   - `crates/rge-data/tests/golden_simple_scene_schema.rs` asserts the
     per-entity `ComponentValue` counts and verbatim `(type_id, data)`
     pairs for both entities, validates each raw `data` string parses
     successfully as `ron::Value` for shape integrity, and updates
     entity-count/root-count assertions to reflect two entities and two
     root entities.
   - `cargo test -p rge-data --test golden_simple_scene_schema` exits 0.
   - `.ai/dispatch.verify.ps1` exits 0.
   - No tracked file outside the two allowed files changes, except
     this dispatch's own handoff/log artifacts.

39. **[DONE-BLOCKED 2026-05-25 via PR #164 / commit `60a0332`] Read-only preflight: typed `ComponentValue` bridge for simple-scene.**
   Audit landed; EXEC was `NEEDS_HUMAN` because current code does not
   name a bridge owner, does not provide a production `type_id` ->
   component-type mapping surface, and the four scene component types do
   not currently implement `rge_kernel_ecs::Component`. The next step is
   an arbiter decision on bridge ownership, mapping strategy, and ECS
   attachment strategy before any implementation dispatch is queued; the
   arbiter decision issue is #165.

   *(original task brief preserved below for context)*
   Task #38 pinned the file-format shape of typed `ComponentValue`
   envelopes on the simple-scene fixture (Transform / Camera / Light /
   Visibility) and proved they round-trip through `rge-data`'s
   schema-only parser. The next real blocker is **not** another schema
   test — it is deciding the smallest safe bridge from
   `rge_data::ComponentValue { type_id, data }` into actual component /
   runtime state without breaking the current rule that `rge-data` stays
   schema-only. This task is an audit-only preflight to decide that
   bridge architecture before any executor writes runtime code.

   The sequencing principle is the same one that worked for #158 → #160:
   no executor guesses on load-bearing conventions. #38 pinned
   file-format shape; #39 decides loader / bridge architecture; only
   then does any implementation dispatch land.

   **Runtime invocation note**: this task is a deliberate named +1 on
   top of the freeze-at-116 posture set by task #38. Run as
   `.\Invoke-AiDispatchAuto.ps1 -PublishMode branch -MaxAutonomousTasks 117`
   so the cap accommodates exactly this one dispatch. The scheduler
   remains disabled and must not be re-enabled by this task.

   **Scope (read-only)**:
   - `crates/rge-data/src/**` and `crates/rge-data/tests/**` (current
     `ComponentValue` / `Scene` / `Entity` surface plus simple-scene
     schema + load-tick tests).
   - `crates/components-spatial/**`, `crates/components-render/**`,
     `crates/components-visibility/**` (current public Rust types,
     `Serialize` / `Deserialize` derives, and any `Reflect` derives).
   - `kernel/types/**` (`TypeId`, `Reflect`, `FQ_TYPE_NAME`,
     `serde_bridge`, derive macro current state).
   - `kernel/ecs/**` (`World`, `EntityId`, `spawn_with_id`, any
     existing typed-component attach surface).
   - `crates/macros-reflect/**` (current derive state, particularly
     what `FQ_TYPE_NAME` / `TYPE_ID` actually emit today and whether
     enums are accepted).
   - Cross-reference prior audit packets from #37 / #158 (ISSUE-158
     EXEC) and #160 (arbiter resolution comment) as historical
     context only.

   **Allowed file surface**:
   - This is read-only. The dispatch may add only its own
     `ai_handoffs/ISSUE-*_TASK_*.md`, `ai_handoffs/ISSUE-*_EXEC_*.md`,
     optional `ai_handoffs/ISSUE-*_CORRECT_*.md`, `.meta.json`
     sidecars, and `ai_dispatch_logs/log_*.md`.
   - No source, test, golden-project, Cargo, workflow, script,
     doctrine, status, plan, or README file may be edited.

   **Required answer format**:
   - The EXEC report must include the exact heading
     `## 5-Question Typed ComponentValue Bridge Preflight Answer Block`.
   - It must answer Q1 through Q5 with file/line evidence.

   **Questions to answer**:
   1. **Bridge owner + dependency direction.** Where should the typed
      `rge_data::Scene` → component-runtime bridge live? Identify
      every plausible crate location (e.g. a new `rge-scene-loader`
      crate, an `rge-runtime` crate, a `kernel/scene-bridge` cavity,
      the existing component crates, the editor binary, or a
      test-local helper) and the dependency edges each candidate
      would create. Validate against forbidden-dep rule 6
      (renderer-tier crates MUST NOT depend on game-domain crates;
      `rge-data` stays schema-only and MUST NOT depend on component
      crates). Identify which candidate keeps the existing graph
      acyclic and which would force a new substrate cavity.
   2. **Type-id → component-type mapping mechanism.** What mechanism
      does current code provide to map a canonical
      `ComponentValue.type_id` string (e.g.
      `"rge::components::Transform"`) to a concrete Rust component
      type? Cite the exact surface: a registry, a match table, a
      `Reflect`-emitted `FQ_TYPE_NAME` / `TYPE_ID` constant, a
      `serde_bridge::from_ron` keyed dispatch, or none of the above.
      If a `Reflect` derive emits a stable mapping today, cite the
      derive call site and the emitted constant; if the four typed
      components (`Transform`, `Camera`, `Light`, `Visibility`) do
      NOT derive `Reflect` today, state that explicitly and cite
      each component's actual derive list.
   3. **ECS insertion target.** How would the bridge actually attach
      a typed component to an ECS entity once the `type_id` is
      mapped? Cite the current `kernel/ecs::World` surface for
      `spawn_with_id`, any component-attach API (or its absence), and
      the ECS storage shape. If `World` does NOT currently expose a
      typed-component attach surface for arbitrary types, state that
      explicitly and identify what minimal surface would be required.
   4. **Justified-from-code check.** Are both (a) the bridge owner +
      dependency direction (Q1) and (b) the type-id → component-type
      mapping mechanism (Q2) already justified from current code
      today? "Justified from code" means a concrete current-code
      surface that the bridge can use as-is without inventing a new
      registry, derive, trait, cavity, or convention. Answer
      Q4 = YES only if both Q1 and Q2 have a current-code answer
      that doesn't require new design work. Answer Q4 = NO if
      either Q1 or Q2 would require inventing architecture.
   5. **Smallest safe follow-up — strict Q5 gate.** Name exactly one
      smallest implementation dispatch **ONLY IF** Q4 = YES. If
      Q4 = NO, end with `NEEDS_HUMAN` and identify which specific
      decision an arbiter must make (e.g. "registry vs Reflect
      mapping," "bridge crate location," "ECS attach API design").
      Do NOT invent a bridge architecture, registry mechanism, trait
      surface, or cavity that does not already exist in current
      code; do NOT recommend an implementation dispatch on the
      basis of "the executor could plausibly..." prose. This Q5
      gate is the key line that prevents the audit from laundering
      a design choice into an implementation task.

   **Halt conditions**:
   - Answering Q1 through Q4 cannot be done from current code,
     comments, tests, or protocol docs. Halt with `NEEDS_HUMAN`.
   - Q4 = NO. Halt; Q5 must be `NEEDS_HUMAN`. Do not propose a
     speculative implementation dispatch.
   - The audit requires editing source, tests, fixtures, Cargo
     files, workflows, scripts, doctrine, status docs, or existing
     handoff packets to answer the five questions.
   - The audit reveals that the only viable bridge owner is a brand
     new crate, kernel cavity, or workspace-level architectural
     change. Halt with `NEEDS_HUMAN`; that is design work, not an
     autonomous implementation task.
   - The audit reveals that any of the four typed components would
     need to gain a `Reflect` derive, a new trait, or a new serde
     surface for the bridge to work. Halt with `NEEDS_HUMAN`; a
     derive change is a source change in the wrong crate for a
     bridge dispatch.
   - The audit cannot be answered in one EXEC packet, requires a
     second artifact, generated log, scratch file, or any packet
     other than the single EXEC report.
   - If verify fails on a target outside the read-only audit scope,
     halt with `NEEDS_HUMAN` rather than fixing it.

   **Scope-preserving halt clause** - the orchestrator's canonical
   verify gate (`.ai/dispatch.verify.ps1`) runs after Claude execute
   even on read-only audits. If verify fails on a target OUTSIDE
   the audit scope, the orchestrator may auto-route a CORRECTION
   packet asking the executor to fix the failure. When that
   happens **the executor MUST halt**: write an EXECUTION_REPORT
   with `EXEC_STATUS: blocked` and `STATUS: NEEDS_HUMAN`, do NOT
   execute the correction. Read-only intent is the entire reason
   this task is in the brief; a correction-round source fix to an
   unrelated failure expands a bridge-architecture audit into a
   source-fix dispatch and must become its own ticket. Precedent:
   ISSUE-158 (2026-05-24) validated this path by preserving the
   typed-payload audit while routing the type-id canonicalization
   question to `HUMAN_ARBITER`.

   **Verbatim review-gate strings** - the autonomous selector MUST
   copy these eight strings, character-for-character, into the
   filed GitHub issue body. No paraphrasing, no substitution, no
   reflowing. A packet that lacks any one of them verbatim is
   bounced at review:

   ```
   MUST be a read-only preflight audit; do not modify source, tests, golden-project files, Cargo files, workflows, scripts, doctrine, status docs, or existing handoff/log artifacts
   MUST include the exact heading ## 5-Question Typed ComponentValue Bridge Preflight Answer Block and answer Q1 through Q5
   MUST answer Q1 bridge owner plus dependency direction with forbidden-dep rule 6 check, Q2 type_id to component-type mapping mechanism with concrete current-code surface, Q3 ECS insertion target with current World/EntityId/spawn_with_id surface, Q4 strict yes-or-no on whether bridge owner/dependency direction AND type-id mapping are both already justified from current code, Q5 exactly one smallest implementation dispatch OR NEEDS_HUMAN
   MUST inspect rge-data Scene/Entity/ComponentValue surface, components-spatial / components-render / components-visibility public Rust types and current derives, kernel/types TypeId/Reflect/serde_bridge surface, kernel/ecs World/EntityId/spawn_with_id and any typed-attach surface, and crates/macros-reflect current derive emission state for FQ_TYPE_NAME / TYPE_ID
   MUST identify every plausible bridge crate location candidate with concrete dependency-edge implications so forbidden-dep rule 6 and rge-data's schema-only posture stay intact
   MUST NOT recommend an implementation dispatch unless Q4 = YES (both dependency direction and type-id mapping already justified from current code); if Q4 = NO, Q5 must end NEEDS_HUMAN and identify which specific arbiter decision is required
   MUST NOT invent a bridge architecture, registry mechanism, trait surface, kernel cavity, or convention that does not already exist in current code
   MUST halt rather than fix if verify fails outside the read-only audit scope or if any of the four typed components would need a new derive, trait, or serde surface for the bridge to work
   ```

   **Done-criterion**:
   - The EXEC packet contains the exact required heading and
     Q1–Q5 answers with file/line evidence.
   - Q1 names every plausible bridge crate location candidate with
     a concrete dependency-edge analysis against forbidden-dep
     rule 6.
   - Q2 cites the exact current-code surface for type_id →
     component-type mapping, or states explicitly that no such
     surface exists today.
   - Q3 cites the exact `kernel/ecs::World` typed-component attach
     surface, or states explicitly that no such surface exists
     today.
   - Q4 is a clear YES or NO with concrete reasoning anchored in
     Q1 and Q2 answers; speculative or "could-plausibly" answers
     count as NO.
   - Q5 names exactly one smallest implementation dispatch ONLY if
     Q4 = YES; otherwise Q5 = `NEEDS_HUMAN` and names the specific
     arbiter decision required.
   - `git status --short --untracked-files=no` is clean before and
     after execution, except for this dispatch's own packet/log
     artifacts.

40. **[DONE 2026-05-25 via PR #167 / commit `a4da354`] Make the four simple-scene component types ECS-attachable via direct `impl rge_kernel_ecs::Component`.**
   Landed via PR #167 after the #166 retry clarification: the generated
   TASK was allowed to change `Cargo.lock` only for the three mechanical
   dependency edges already permitted by this brief. The final merge added
   four direct `Component` impls plus focused ECS attach/retrieve tests in
   the three owning component crates. The loader crate remains the next
   dispatch (#41); the original brief is preserved below.

   Task #39 produced a `NEEDS_HUMAN` audit for the typed
   `ComponentValue` bridge; issue #165 closed the arbiter decision with
   the default recommendation: new `rge-scene-loader` Tier-2 crate as
   the bridge owner, explicit match table for the four canonical
   `type_id` strings, direct `impl rge_kernel_ecs::Component` for the
   four component types in their owning crates, no global runtime
   registry, Reflect-driven loading deferred.

   This task implements the **first load-bearing source change** from
   that decision: the `impl Component` edge in the three component
   crates. It is deliberately separated from the `rge-scene-loader`
   crate creation (task #41 follow-up) because the new
   `rge-kernel-ecs` dependency edges from `components-spatial`,
   `components-render`, and `components-visibility` are the load-bearing
   dependency-direction change and want their own bounded dispatch
   before the loader crate lands on top.

   **Runtime invocation note**: this task is a deliberate named +1 on
   top of the freeze-at-117 posture set by task #39. Run as
   `.\Invoke-AiDispatchAuto.ps1 -PublishMode branch -MaxAutonomousTasks 118`
   so the cap accommodates exactly this one dispatch. The scheduler
   remains disabled and must not be re-enabled by this task.

   **Arbiter decisions to encode** (from #165 resolution):
   - Direct `impl rge_kernel_ecs::Component` for `Transform`, `Camera`,
     `Light`, and `Visibility` in their owning component crates. No
     wrapper types, no `SnapshotComponent` indirection for these four.
   - No `#[derive(Reflect)]`, no global runtime registry, no derive-emitted
     mapping — the bridge that consumes these impls (task #41) will use
     an explicit match table, not a Reflect lookup.
   - `rge-data` stays schema-only; this task adds no dep edge to or
     from `rge-data`.

   **Allowed file surface**:
   - EDIT `crates/components-spatial/Cargo.toml` to add
     `rge-kernel-ecs = { workspace = true }` (or
     `{ path = "../../kernel/ecs" }`, whichever matches workspace
     convention) as a regular (non-dev) dependency.
   - EDIT `crates/components-render/Cargo.toml` same way.
   - EDIT `crates/components-visibility/Cargo.toml` same way.
   - EDIT the source files that own the four component types to add
     `impl rge_kernel_ecs::Component for <TypeName>` blocks:
     - `crates/components-spatial/src/transform.rs` or
       `crates/components-spatial/src/lib.rs` — for `Transform`.
     - `crates/components-render/src/camera.rs` — for `Camera`.
     - `crates/components-render/src/light.rs` — for `Light`.
     - `crates/components-visibility/src/visibility.rs` or
       `crates/components-visibility/src/lib.rs` — for `Visibility`.
   - MAY add focused tests in the same crates, either as a
     `#[cfg(test)] mod tests` block in the source file or as a new
     `crates/components-*/tests/*.rs` integration test file.
   - MAY edit `Cargo.lock` only for the three mechanical new dep
     edges from each component crate to `rge-kernel-ecs`.
   - MAY add this dispatch's own `ai_handoffs/ISSUE-*_TASK_*.md`,
     `ai_handoffs/ISSUE-*_EXEC_*.md`, `ai_handoffs/ISSUE-*_CORRECT_*.md`
     packets plus `.meta.json` sidecars if produced by the orchestrator,
     and the queue-runner's own `ai_dispatch_logs/log_*.md`.

   **Tests required**:
   - At least one focused test per component crate (three tests
     minimum, four ideal — one per impl) proving each component type
     can be inserted into a fresh `rge_kernel_ecs::World` and
     retrieved through the current ECS API. The exact attach/retrieve
     surface is whichever public API `rge_kernel_ecs::World` currently
     exposes for `Component`-implementing types — the ISSUE-163 audit
     (`ai_handoffs/ISSUE-163_EXEC_*.md`) is the authoritative
     reference for that surface.
   - Each test spawns an `EntityId`, attaches the component, retrieves
     it, and asserts the retrieved value equals what was attached. No
     scene-loading, no `rge-data` round-trip, no editor/runtime
     integration — pure component-trait acceptance.

   **Files that MUST NOT be touched**:
   - `crates/rge-data/**` — `rge-data` stays schema-only; no
     bidirectional dep edge.
   - `crates/rge-scene-loader/**` — that crate is the **next**
     dispatch (#41), not this one. Do not create it now.
   - `kernel/**` — this dispatch consumes the existing `kernel/ecs`
     surface; it does NOT modify the kernel, including no extension to
     `Component` trait, no new `World` API, no kernel-side helper.
   - `crates/macros-reflect/**` — Reflect-driven loading is explicitly
     deferred by the #165 decision.
   - `editor/**`, `crates/editor-shell/**`, `runtime/**`,
     `crates/script-host/**`, `crates/script-bench/**`, `crates/gfx/**`,
     `crates/brep-render/**`, any `crates/io-*/**`,
     `crates/asset-store/**`, or any other crate.
   - `golden-projects/**` — the simple-scene fixture from task #38
     stays as-is; no fixture edits in this task.
   - Workspace root `Cargo.toml` (no new workspace member, no
     `[workspace.dependencies]` additions beyond what already exists for
     `rge-kernel-ecs`). If `rge-kernel-ecs` is not currently a
     `[workspace.dependencies]` entry and adding it there is required to
     reach `{ workspace = true }`, that single workspace-manifest line
     is permitted; halt if any other workspace-manifest change becomes
     necessary.
   - Any `.github/**` workflow, PowerShell script, schema, doctrine,
     status doc, ADR, plan, README, or existing handoff packet.
   - Any GitHub label or issue metadata except the queue runner's
     normal issue lifecycle for this dispatch.

   **Cargo.lock policy**:
   - The three new dep edges from each component crate to
     `rge-kernel-ecs` are the only permitted lockfile changes. Any
     other package addition, version bump, checksum change, or source
     change halts with `NEEDS_HUMAN`.

   **Halt conditions**:
   - `rge_kernel_ecs::Component` trait does not exist today, is sealed,
     `#[non_exhaustive]` in a way that blocks downstream impls, or
     has a method shape that cannot be satisfied by `Transform` /
     `Camera` / `Light` / `Visibility` without modifying the kernel.
     Halt with `NEEDS_HUMAN`; the kernel-side fix is its own dispatch.
   - Implementing `Component` for any of the four types requires
     touching `kernel/ecs` source, the kernel `Component` trait, or
     any other kernel surface beyond consumption.
   - The impl requires `#[derive(Reflect)]`, a `register_component!`
     macro call, a global runtime registry, or any indirection beyond
     a direct trait impl in the owning crate. The #165 decision was
     explicitly direct-impl-only; halt rather than re-interpret.
   - The test surface requires a new dev-dependency beyond what's
     already available to the component crates (or what
     `rge-kernel-ecs` re-exports).
   - The bridge-loader crate `rge-scene-loader` must be created in this
     dispatch for the impls to compile. The `Component` impl should
     compile and test purely on its own (kernel-ecs surface is all the
     impls touch); if compilation requires the loader crate, halt.
   - Adding the dep edge causes a workspace-level architectural change
     (new workspace member, new `[workspace.dependencies]` entry beyond
     the single `rge-kernel-ecs` entry if not already present,
     forbidden-dep rule violation).
   - Cargo.lock churn beyond the three new dep edges.
   - The focused test or canonical verification gate fails for any
     reason outside the allowed file surface.

   **Scope-preserving halt clause** - the orchestrator's canonical
   verify gate (`.ai/dispatch.verify.ps1`) runs after Claude execute.
   If verify fails on a target OUTSIDE the allowed file surface
   (anything beyond the three component crates' `Cargo.toml`, the
   four component source files, focused test additions in those
   three crates, the single permitted workspace-manifest line if
   strictly required, the three permitted Cargo.lock dep-edge
   additions, or this dispatch's own `ai_handoffs/` packet), the
   orchestrator may auto-route a CORRECTION packet asking the
   executor to fix the failure. When that happens **the executor MUST
   halt**: write an EXECUTION_REPORT with `EXEC_STATUS: blocked` and
   `STATUS: NEEDS_HUMAN`, do NOT execute the correction. Scope
   discipline is the entire reason this task is bounded narrowly; a
   correction-round source fix to an unrelated failure expands a
   four-impl + three-dep-edge dispatch into a multi-crate fix and
   must become its own ticket.

   **Verbatim review-gate strings** - the autonomous selector MUST
   copy these eight strings, character-for-character, into the filed
   GitHub issue body. No paraphrasing, no substitution, no
   reflowing. A packet that lacks any one of them verbatim is
   bounced at review:

   ```
   MUST add rge-kernel-ecs as a regular (non-dev) workspace-path dependency to exactly three Cargo.toml files: crates/components-spatial/Cargo.toml, crates/components-render/Cargo.toml, and crates/components-visibility/Cargo.toml
   MUST impl rge_kernel_ecs::Component for Transform in components-spatial, Camera and Light in components-render, and Visibility in components-visibility — four impls total
   MUST NOT use Reflect, derive_Reflect, a register_component macro, a global runtime registry, or any indirection — direct trait impl in the owning crate is the entire decision being implemented per issue #165
   MUST NOT touch crates/rge-data/**, kernel/**, editor/**, runtime/**, crates/editor-shell/**, crates/script-host/**, crates/script-bench/**, crates/gfx/**, crates/brep-render/**, crates/io-*/**, crates/asset-store/**, crates/macros-reflect/**, golden-projects/**, or any production source outside the three component crates
   MUST NOT create the rge-scene-loader crate or any other new workspace member; the loader crate is the NEXT dispatch (#41), not this one
   MUST add at least one focused test per component crate proving each component type can be inserted into a fresh rge_kernel_ecs::World and retrieved through the current ECS API
   MUST halt with NEEDS_HUMAN if rge_kernel_ecs::Component trait does not exist, is sealed, is non_exhaustive in a way blocking downstream impls, or has a shape that requires modifying the kernel to satisfy
   MUST run cargo test -p rge-components-spatial -p rge-components-render -p rge-components-visibility and the canonical .ai/dispatch.verify.ps1 gate successfully
   ```

   **Done-criterion**:
   - `crates/components-spatial/Cargo.toml`,
     `crates/components-render/Cargo.toml`, and
     `crates/components-visibility/Cargo.toml` each gain exactly one
     new regular dep entry on `rge-kernel-ecs` (workspace path).
   - Four `impl rge_kernel_ecs::Component` blocks land:
     - `Transform` in `crates/components-spatial/src/...`,
     - `Camera` in `crates/components-render/src/camera.rs`,
     - `Light` in `crates/components-render/src/light.rs`,
     - `Visibility` in `crates/components-visibility/src/...`.
   - Each of the three component crates has at least one focused test
     proving its component type(s) can be inserted into a fresh
     `rge_kernel_ecs::World` and retrieved through the current ECS API.
   - `cargo test -p rge-components-spatial -p rge-components-render -p rge-components-visibility`
     exits 0.
   - `.ai/dispatch.verify.ps1` exits 0 (architecture-lint
     forbidden-dep rules stay green; the new edges go up the
     dependency tree, not down).
   - Cargo.lock has exactly three new dep edges and no other change.
   - No tracked file outside the allowed surface changes, except this
     dispatch's own handoff/log artifacts.

41. **[DONE-BLOCKED 2026-05-25 via ISSUE-168 local blocked commit `85cfcc0`] Create first `rge-scene-loader` bridge for simple-scene typed `ComponentValue` payloads.**
   Dispatch #168 correctly halted before publish. The scaffolded loader
   crate proved the match-table bridge shape, but `cargo test -p
   rge-scene-loader` exposed a prerequisite `kernel/ecs` storage bug:
   the single catch-all archetype cannot currently attach heterogeneous
   component sets because a component column panics or misaligns when its
   first value belongs to a nonzero entity row. The local branch
   `ai-dispatch/ISSUE-168` is retained as evidence/scaffold, but is not a
   merge candidate. Issue #168 was closed as not planned; task #42 queues
   the kernel prerequisite, and the loader retry must be a later task after
   that lands. The original loader brief is preserved below.

   Tasks #38-#40 pinned the file-format shape and made the four typed
   component structs ECS-attachable. This task implements the first
   runtime bridge crate from the #165 resolution: a Tier-2
   `rge-scene-loader` crate with an explicit match table for the four
   canonical `ComponentValue.type_id` strings in the golden simple-scene.

   **Runtime invocation note**: this task is a deliberate named +1 on top
   of the cap-118 posture used by task #40. Run as
   `.\Invoke-AiDispatchAuto.ps1 -PublishMode branch -MaxAutonomousTasks 119`
   so the cap accommodates exactly this one dispatch. The scheduler
   remains disabled and must not be re-enabled by this task.

   **Arbiter decisions to encode** (from #165 resolution):
   - Bridge owner: new Tier-2 crate `crates/rge-scene-loader`.
   - Type-id mapping: explicit match table, no global runtime registry,
     no Reflect-driven lookup, no derive-emitted type-name lookup.
   - ECS attachment: use the direct `impl rge_kernel_ecs::Component`
     blocks landed in task #40 and the current public `World` API.
   - `rge-data` remains schema-only. It may be consumed by
     `rge-scene-loader`; it must not depend on `rge-scene-loader`.

   **Allowed file surface**:
   - EDIT root `Cargo.toml` only to add exactly one workspace member:
     `crates/rge-scene-loader`.
   - ADD new files under `crates/rge-scene-loader/**`.
   - MAY edit `Cargo.lock` only for the new workspace package metadata
     and its direct internal/workspace dependency edges. No external
     package addition, version bump, checksum change, or source change.
   - MAY add this dispatch's own `ai_handoffs/ISSUE-*_TASK_*.md`,
     `ai_handoffs/ISSUE-*_EXEC_*.md`, `ai_handoffs/ISSUE-*_CORRECT_*.md`
     packets plus `.meta.json` sidecars if produced by the orchestrator,
     and the queue-runner's own `ai_dispatch_logs/log_*.md`.

   **Crate shape required**:
   - New crate package name: `rge-scene-loader`.
   - New crate dependencies:
     - `rge-data` via path dependency on `../rge-data`.
     - `rge-kernel-ecs = { workspace = true }`.
     - `rge-components-spatial` via path dependency on
       `../components-spatial`.
     - `rge-components-render` via path dependency on
       `../components-render`.
     - `rge-components-visibility` via path dependency on
       `../components-visibility`.
     - `ron = { workspace = true }`.
     - `thiserror = { workspace = true }` only if used for the public
       error type; otherwise implement `Display`/`Error` manually.
   - Public API: expose a small function such as
     `pub fn load_scene_into_world(scene: &rge_data::Scene) -> Result<rge_kernel_ecs::World, SceneLoadError>`
     (exact name may vary if local conventions point to a better one).
   - `SceneLoadError` must distinguish at least unsupported
     `ComponentValue.type_id` from typed RON payload parse failures.
     Include enough entity/type context in the error for diagnosis.

   **Bridge behavior required**:
   - Spawn all scene entities first, preserving ULIDs exactly with
     `rge_kernel_ecs::EntityId::from_ulid(entity.id.0)` and
     `World::spawn_with_id`.
   - Then walk each entity's `components` and match exactly these four
     strings:
     - `rge::components::Transform` ->
       `ron::from_str::<rge_components_spatial::Transform>(&component.data)`
     - `rge::components::Camera` ->
       `ron::from_str::<rge_components_render::Camera>(&component.data)`
     - `rge::components::Light` ->
       `ron::from_str::<rge_components_render::Light>(&component.data)`
     - `rge::components::Visibility` ->
       `ron::from_str::<rge_components_visibility::Visibility>(&component.data)`
   - Insert each parsed value through the current public
     `rge_kernel_ecs::World::insert` API.
   - Unknown `type_id` values are errors, not silent skips.
   - Relations, assets, scene-tree/root semantics, renderer resources,
     editor state, scripts, and runtime integration are non-goals for
     this dispatch.

   **Tests required**:
   - Add focused tests in `crates/rge-scene-loader/tests/`.
   - The main test must parse `golden-projects/simple-scene/.rge-project`,
     load the referenced scene, run the new loader, and assert:
     - `world.entity_count() == scene.entities.len()`.
     - The Camera entity with ULID `0000000000000G000000000000` exists and
       has attached `Transform`, `Camera`, and `Visibility::Visible`.
     - The `KeyLight` entity with ULID `00000000000010000000000000` exists
       and has attached `Transform` and `Light`.
   - Add at least one negative test proving unsupported `type_id` returns
     the loader error instead of being ignored.
   - Add a malformed-payload negative test if the public error shape can
     expose it without broadening the implementation.

   **Files that MUST NOT be touched**:
   - `crates/rge-data/**` - keep the existing test-local identity-only
     load/tick test unchanged in this dispatch. Replacing that helper with
     `rge-scene-loader` is a later integration task, not this first bridge.
   - `crates/components-spatial/**`, `crates/components-render/**`, and
     `crates/components-visibility/**` - task #40 already landed the
     required `Component` impls.
   - `kernel/**` - consume the existing ECS API; do not add or change
     kernel types, traits, or world methods.
   - `crates/macros-reflect/**`, `kernel/types/**`, any registry/inventory
     crate, or any macro crate.
   - `golden-projects/**` - the simple-scene fixture stays byte-for-byte
     as pinned by task #38.
   - `editor/**`, `runtime/**`, `crates/editor-shell/**`,
     `crates/script-host/**`, `crates/script-bench/**`, `crates/gfx/**`,
     `crates/brep-render/**`, any `crates/io-*/**`,
     `crates/asset-store/**`, or any other crate.
   - `.github/**`, PowerShell automation scripts, schema/doctrine/status
     docs, ADRs, READMEs, or existing handoff/log artifacts.
   - Any GitHub label or issue metadata except the queue runner's normal
     issue lifecycle for this dispatch.

   **Cargo.lock policy**:
   - The only permitted lockfile changes are the new
     `rge-scene-loader` package stanza and dependency-edge references
     already implied by the allowed crate dependencies above.
   - Any external package addition, version bump, checksum change, source
     change, or unrelated package stanza churn halts with `NEEDS_HUMAN`.

   **Halt conditions**:
   - Any of the four golden simple-scene payloads fails typed
     `ron::from_str::<Transform|Camera|Light|Visibility>` parsing. Halt;
     do not edit the fixture or weaken the typed payload assertion.
   - The current ECS API cannot insert one of the four direct `Component`
     impl types without changing `kernel/ecs`.
   - Duplicate entity IDs, relation loading, root-entity semantics, or
     asset/resource loading become necessary to satisfy the tests.
   - Implementing the bridge requires `Reflect`, `kernel/types`, a global
     registry, `inventory`, `linkme`, a registration macro, wrapper types,
     `SnapshotComponent`, or type-erased component insertion.
   - Adding `rge-scene-loader` creates a forbidden dependency direction or
     architecture-lint failure that cannot be fixed wholly inside the new
     crate/root workspace-member addition.
   - Any edit outside the allowed file surface is needed.

   **Scope-preserving halt clause** - the orchestrator's canonical verify
   gate (`.ai/dispatch.verify.ps1`) runs after Claude execute. If verify
   fails on a target outside the allowed file surface (root `Cargo.toml`
   member addition, `crates/rge-scene-loader/**`, permitted `Cargo.lock`
   new-package/edge entries, or this dispatch's own handoff/log packet),
   the orchestrator may auto-route a CORRECTION packet asking the executor
   to fix the failure. When that happens the executor MUST halt: write an
   EXECUTION_REPORT with `EXEC_STATUS: blocked` and `STATUS: NEEDS_HUMAN`,
   do NOT execute the correction.

   **Verbatim review-gate strings** - the autonomous selector MUST copy
   these eight strings, character-for-character, into the filed GitHub
   issue body. No paraphrasing, no substitution, no reflowing. A packet
   that lacks any one of them verbatim is bounced at review:

   ```
   MUST create exactly one new workspace member, crates/rge-scene-loader, and add exactly that member to root Cargo.toml
   MUST expose a Scene-to-World loader API that preserves rge_data::EntityId ULIDs through rge_kernel_ecs::EntityId::from_ulid and World::spawn_with_id
   MUST use an explicit match table for exactly these four ComponentValue.type_id strings: rge::components::Transform, rge::components::Camera, rge::components::Light, and rge::components::Visibility
   MUST deserialize payloads with typed ron::from_str::<Transform|Camera|Light|Visibility> calls and insert parsed values through the current rge_kernel_ecs::World::insert API
   MUST NOT use Reflect, kernel/types, inventory, linkme, a global registry, a registration macro, SnapshotComponent, wrapper component types, or type-erased component insertion
   MUST NOT modify crates/rge-data/**, kernel/**, crates/components-spatial/**, crates/components-render/**, crates/components-visibility/**, golden-projects/**, editor/**, runtime/**, crates/editor-shell/**, crates/script-host/**, crates/script-bench/**, crates/gfx/**, crates/brep-render/**, crates/io-*/**, crates/asset-store/**, crates/macros-reflect/**, or any production source outside crates/rge-scene-loader/**
   MUST add focused rge-scene-loader tests proving the golden simple-scene Camera entity has Transform plus Camera plus Visibility::Visible and the KeyLight entity has Transform plus Light in the loaded World
   MUST run cargo test -p rge-scene-loader and the canonical .ai/dispatch.verify.ps1 gate successfully
   ```

   **Done-criterion**:
   - `crates/rge-scene-loader` exists as the only new workspace member.
   - The crate exposes a fallible `Scene -> World` loader API.
   - The loader preserves entity ULIDs, attaches all five typed
     components present in the current simple-scene fixture, and errors
     on unsupported component type IDs.
   - Tests in the new crate prove the golden simple-scene bridge for
     Camera and KeyLight plus unsupported-type behavior.
   - `cargo test -p rge-scene-loader` exits 0.
   - `.ai/dispatch.verify.ps1` exits 0.
   - No file outside the allowed surface changes, except this dispatch's
     own handoff/log artifacts.

42. **[DONE 2026-05-25 via PR #170 / commit `7f66914`] Fix `kernel/ecs` sparse component columns for heterogeneous entity component sets.**
   Landed via PR #170 after one correction round. The fix converted the
   catch-all archetype to sparse component rows, made `EntityRef::contains`
   row-specific, and added heterogeneous insert/query/remove/replace/despawn
   plus snapshot restore coverage. This unblocks the loader retry in task
   #43. The original prerequisite brief is preserved below.

   Task #41 / ISSUE-168 showed that the loader cannot land until the
   current single catch-all ECS archetype can represent sparse component
   membership. The kernel docs already say queries iterate the full entity
   list and skip entities that do not carry the queried component. The
   implementation does not yet satisfy that contract: component columns are
   dense `Vec<ColumnRow>` values, so the first insert for a component at
   row `> 0` either trips `debug_assert_eq!(row, col.len())` or stores the
   value at the wrong row if the assertion is disabled.

   **Runtime invocation note**: this task is a deliberate named +1 on top
   of the cap-119 posture used by task #41. Run as
   `.\Invoke-AiDispatchAuto.ps1 -PublishMode branch -MaxAutonomousTasks 120`
   so the cap accommodates exactly this one dispatch. The scheduler
   remains disabled and must not be re-enabled by this task.

   **Required behavior**:
   - The current single catch-all archetype must support rows where a
     component type is absent for some entities and present for others.
   - `World::insert<C>` must work when component `C` first appears on any
     existing entity row, including nonzero rows.
   - `World::insert_erased` must have the same sparse-row behavior for the
     snapshot restore path.
   - `EntityRef::get<C>()`, `EntityMut::get<C>()`, and queries must return
     `None` / skip rows where component `C` is absent.
   - `EntityRef::contains<C>()` must become row-specific. It must be true
     only when that entity has `C`, not merely when any entity in the
     archetype has a `C` column.
   - `World::remove<C>`, `World::replace<C>`, `EntityMut::remove<C>()`, and
     `World::despawn` / archetype swap-remove must preserve row-to-entity
     alignment for sparse columns.
   - Snapshot serialize/restore must support heterogeneous registered
     component sets without changing the snapshot wire format.

   **Allowed file surface**:
   - EDIT `kernel/ecs/src/archetype.rs`.
   - EDIT `kernel/ecs/src/entity.rs` only if needed for row-specific
     `EntityRef::contains<C>()`.
   - EDIT `kernel/ecs/src/world.rs` only for focused tests or call-site
     adaptation required by the sparse column API.
   - EDIT `kernel/ecs/src/snapshot.rs` only if the existing snapshot
     call sites need a no-format-change adaptation to sparse columns.
   - EDIT existing `kernel/ecs/tests/*.rs` or add new focused tests under
     `kernel/ecs/tests/`.
   - MAY add this dispatch's own `ai_handoffs/ISSUE-*_TASK_*.md`,
     `ai_handoffs/ISSUE-*_EXEC_*.md`, `ai_handoffs/ISSUE-*_CORRECT_*.md`
     packets plus `.meta.json` sidecars if produced by the orchestrator,
     and the queue-runner's own `ai_dispatch_logs/log_*.md`.

   **Files that MUST NOT be touched**:
   - `Cargo.toml`, `Cargo.lock`, or any crate manifest.
   - `crates/rge-scene-loader/**` - the blocked scaffold from #168 is not
     part of this dispatch; do not create or modify the loader crate here.
   - `crates/rge-data/**`, `golden-projects/**`, and all component crates.
   - `kernel/types/**`, `crates/macros-reflect/**`, any registry/inventory
     crate, any macro crate, or any type-erased public loader surface.
   - `editor/**`, `runtime/**`, `crates/editor-shell/**`,
     `crates/script-host/**`, `crates/script-bench/**`, `crates/gfx/**`,
     `crates/brep-render/**`, any `crates/io-*/**`,
     `crates/asset-store/**`, or any other crate.
   - `.github/**`, PowerShell automation scripts, schema/doctrine/status
     docs, ADRs, READMEs, or existing handoff/log artifacts.
   - Any GitHub label or issue metadata except the queue runner's normal
     issue lifecycle for this dispatch.

   **Implementation guidance**:
   - A safe-Rust `Vec<Option<ColumnRow>>` sparse-column representation is
     acceptable if it is the smallest coherent fix. Other safe-Rust sparse
     representations are acceptable if they preserve the existing public ECS
     API and pass the required tests.
   - Do not introduce `unsafe`.
   - Do not implement real per-component-set archetype migration in this
     dispatch; that is larger than the bug being fixed.
   - Do not change public `World` / `EntityRef` / `EntityMut` API names or
     signatures unless the current code makes a row-specific correctness fix
     impossible without it. If a public API break is required, halt.
   - Do not change snapshot serialization format. Existing snapshot tests
     must remain valid.

   **Tests required**:
   - Add a kernel ECS test where two entities carry different component
     sets (for example entity A has only `A`, entity B has only `B`), and
     prove `insert`, `get`, `contains`, and `query` are row-correct.
   - Add a test where the first value for a component type is inserted on a
     nonzero row and does not panic or misalign.
   - Add a test covering sparse-column `remove` / `replace` behavior.
   - Add a test covering `despawn` swap-remove when a component exists only
     on the row that moves.
   - Add or extend a snapshot round-trip test proving heterogeneous
     registered components serialize and restore with the same per-entity
     component membership.

   **Halt conditions**:
   - The fix requires `unsafe`, global registries, reflection, wrapper
     component types, or a public type-erased insertion API.
   - The fix requires real archetype migration or a broad ECS redesign
     rather than sparse rows in the existing catch-all archetype.
   - The fix requires changing snapshot wire format or weakening existing
     snapshot determinism tests.
   - The fix requires any edit outside the allowed file surface.
   - `cargo test -p rge-kernel-ecs` or `.ai/dispatch.verify.ps1` fails for
     reasons that cannot be fixed inside the allowed file surface.

   **Scope-preserving halt clause** - the orchestrator's canonical verify
   gate (`.ai/dispatch.verify.ps1`) runs after Claude execute. If verify
   fails on a target outside the allowed file surface (`kernel/ecs/src/*`,
   `kernel/ecs/tests/*`, or this dispatch's own handoff/log packet), the
   orchestrator may auto-route a CORRECTION packet asking the executor to
   fix the failure. When that happens the executor MUST halt: write an
   EXECUTION_REPORT with `EXEC_STATUS: blocked` and `STATUS: NEEDS_HUMAN`,
   do NOT execute the correction.

   **Verbatim review-gate strings** - the autonomous selector MUST copy
   these eight strings, character-for-character, into the filed GitHub
   issue body. No paraphrasing, no substitution, no reflowing. A packet
   that lacks any one of them verbatim is bounced at review:

   ```
   MUST make the existing single catch-all kernel/ecs archetype support sparse component rows where an entity may lack a component column value that another entity has
   MUST make World::insert and World::insert_erased handle the first insertion of a component type at a nonzero entity row without panic or row misalignment
   MUST make EntityRef::contains<C>() row-specific, not a global column-existence check across the whole archetype
   MUST preserve row-to-entity alignment for get, get_mut, query, remove, replace, despawn swap-remove, and snapshot serialize/restore
   MUST add kernel/ecs tests covering heterogeneous typed insert/get/contains/query, nonzero-row first insert, sparse remove/replace, sparse despawn swap-remove, and heterogeneous snapshot round-trip
   MUST NOT edit Cargo.toml, Cargo.lock, crates/rge-scene-loader/**, crates/rge-data/**, golden-projects/**, component crates, kernel/types/**, crates/macros-reflect/**, editor/**, runtime/**, crates/gfx/**, crates/brep-render/**, crates/io-*/**, crates/asset-store/**, or any production source outside kernel/ecs/**
   MUST NOT introduce unsafe code, Reflect, global registries, wrapper component types, SnapshotComponent changes, snapshot wire-format changes, or public ECS API breaks
   MUST run cargo test -p rge-kernel-ecs and the canonical .ai/dispatch.verify.ps1 gate successfully
   ```

   **Done-criterion**:
   - `kernel/ecs` supports heterogeneous component membership in the
     existing catch-all archetype without row misalignment.
   - Row-specific `contains`, `get`, query, remove/replace, despawn, and
     snapshot restore behavior is covered by focused tests.
   - `cargo test -p rge-kernel-ecs` exits 0.
   - `.ai/dispatch.verify.ps1` exits 0.
   - No file outside the allowed surface changes, except this dispatch's
     own handoff/log artifacts.

43. **[DONE 2026-05-25 via PR #172 / commit `1d32fd3`] Retry `rge-scene-loader` bridge after sparse ECS columns.**
   Landed via PR #172. The new `rge-scene-loader` crate loads the typed
   simple-scene `ComponentValue` envelopes through an explicit four-arm
   match table and preserves scene ULIDs in `World`. A reviewer follow-up
   commit tightened the golden integration test to parse `.rge-project` and
   follow its scene reference, matching this brief exactly. The original
   retry brief is preserved below.

   Task #41 / ISSUE-168 produced a good loader scaffold but correctly
   blocked because the kernel could not attach heterogeneous component
   sets. Task #42 / PR #170 fixed that kernel prerequisite on `main`.
   This task retries the first production `rge-scene-loader` crate from a
   clean branch, with the same #165 decisions: new Tier-2 bridge crate,
   explicit match table, direct ECS insertion, no registry, no Reflect.

   **Runtime invocation note**: this task is a deliberate named +1 on top
   of the cap-120 posture used by task #42. Run as
   `.\Invoke-AiDispatchAuto.ps1 -PublishMode branch -MaxAutonomousTasks 121`
   so the cap accommodates exactly this one dispatch. The scheduler
   remains disabled and must not be re-enabled by this task.

   **Allowed file surface**:
   - EDIT root `Cargo.toml` only to add exactly one workspace member:
     `crates/rge-scene-loader`.
   - ADD new files under `crates/rge-scene-loader/**`.
   - MAY edit `Cargo.lock` only for the new workspace package stanza and
     its direct internal/workspace dependency-edge references. No external
     package addition, version bump, checksum change, or source change.
   - MAY add this dispatch's own `ai_handoffs/ISSUE-*_TASK_*.md`,
     `ai_handoffs/ISSUE-*_EXEC_*.md`, `ai_handoffs/ISSUE-*_CORRECT_*.md`
     packets plus `.meta.json` sidecars if produced by the orchestrator,
     and the queue-runner's own `ai_dispatch_logs/log_*.md`.

   **Crate shape required**:
   - New crate package name: `rge-scene-loader`.
   - New crate dependencies:
     - `rge-data` via path dependency on `../rge-data`.
     - `rge-kernel-ecs = { workspace = true }`.
     - `rge-components-spatial` via path dependency on
       `../components-spatial`.
     - `rge-components-render` via path dependency on
       `../components-render`.
     - `rge-components-visibility` via path dependency on
       `../components-visibility`.
     - `ron = { workspace = true }`.
     - `thiserror = { workspace = true }` only if used for the public
       error type; otherwise implement `Display`/`Error` manually.
   - Public API: expose a fallible scene loader, for example
     `load_scene_into_world(scene: &rge_data::Scene, world: &mut rge_kernel_ecs::World) -> Result<(), SceneLoadError>`
     or a similarly small local-convention name.
   - `SceneLoadError` must distinguish unsupported `ComponentValue.type_id`
     from typed RON payload parse failures and include entity/type context.

   **Bridge behavior required**:
   - Spawn all scene entities first, preserving ULIDs exactly with
     `rge_kernel_ecs::EntityId::from_ulid(entity.id.0)` and
     `World::spawn_with_id`.
   - Then walk each entity's `components` and match exactly these four
     strings:
     - `rge::components::Transform` ->
       `ron::from_str::<rge_components_spatial::Transform>(&component.data)`
     - `rge::components::Camera` ->
       `ron::from_str::<rge_components_render::Camera>(&component.data)`
     - `rge::components::Light` ->
       `ron::from_str::<rge_components_render::Light>(&component.data)`
     - `rge::components::Visibility` ->
       `ron::from_str::<rge_components_visibility::Visibility>(&component.data)`
   - Insert each parsed value through the current public
     `rge_kernel_ecs::World::insert` API.
   - Unknown `type_id` values are errors, not silent skips.
   - Relations, assets, scene-tree/root semantics, renderer resources,
     editor state, scripts, and runtime integration are non-goals.

   **Tests required**:
   - Add focused tests in `crates/rge-scene-loader/tests/`.
   - The main test must parse `golden-projects/simple-scene/.rge-project`,
     load the referenced scene, run the new loader, and assert:
     - `world.entity_count() == scene.entities.len()`.
     - The Camera entity with ULID `0000000000000G000000000000` exists and
       has attached `Transform`, `Camera`, and `Visibility::Visible`.
     - The `KeyLight` entity with ULID `00000000000010000000000000` exists
       and has attached `Transform` and `Light`.
   - Add at least one negative test proving unsupported `type_id` returns
     the loader error instead of being ignored.
   - Add a malformed-payload negative test if the public error shape can
     expose it without broadening the implementation.

   **Files that MUST NOT be touched**:
   - `crates/rge-data/**` - keep the existing test-local identity-only
     load/tick test unchanged in this dispatch.
   - `kernel/**` - task #42 already landed the sparse-row prerequisite;
     this dispatch consumes the current ECS API and must not modify it.
   - `crates/components-spatial/**`, `crates/components-render/**`, and
     `crates/components-visibility/**`.
   - `golden-projects/**` - the simple-scene fixture stays byte-for-byte
     as pinned by task #38.
   - `crates/macros-reflect/**`, `kernel/types/**`, registry/inventory
     crates, macro crates, editor/runtime/gfx/io/assets crates, schema/docs,
     scripts, workflows, or existing handoff/log artifacts.
   - Any GitHub label or issue metadata except the queue runner's normal
     issue lifecycle for this dispatch.

   **Halt conditions**:
   - Any of the four golden simple-scene payloads fails typed
     `ron::from_str::<Transform|Camera|Light|Visibility>` parsing. Halt;
     do not edit the fixture or weaken the typed payload assertion.
   - The current ECS API still cannot attach the heterogeneous simple-scene
     component set without modifying `kernel/**`. Halt; do not patch the
     kernel in this dispatch.
   - Duplicate entity IDs, relation loading, root-entity semantics, or
     asset/resource loading become necessary to satisfy the tests.
   - Implementing the bridge requires `Reflect`, `kernel/types`, a global
     registry, `inventory`, `linkme`, a registration macro, wrapper types,
     `SnapshotComponent`, or type-erased component insertion.
   - Any edit outside the allowed file surface is needed.

   **Verbatim review-gate strings** - the autonomous selector MUST copy
   these eight strings, character-for-character, into the filed GitHub
   issue body. No paraphrasing, no substitution, no reflowing. A packet
   that lacks any one of them verbatim is bounced at review:

   ```
   MUST create exactly one new workspace member, crates/rge-scene-loader, and add exactly that member to root Cargo.toml
   MUST expose a Scene-to-World loader API that preserves rge_data::EntityId ULIDs through rge_kernel_ecs::EntityId::from_ulid and World::spawn_with_id
   MUST use an explicit match table for exactly these four ComponentValue.type_id strings: rge::components::Transform, rge::components::Camera, rge::components::Light, and rge::components::Visibility
   MUST deserialize payloads with typed ron::from_str::<Transform|Camera|Light|Visibility> calls and insert parsed values through the current rge_kernel_ecs::World::insert API
   MUST NOT use Reflect, kernel/types, inventory, linkme, a global registry, a registration macro, SnapshotComponent, wrapper component types, or type-erased component insertion
   MUST NOT modify crates/rge-data/**, kernel/**, crates/components-spatial/**, crates/components-render/**, crates/components-visibility/**, golden-projects/**, editor/**, runtime/**, crates/editor-shell/**, crates/script-host/**, crates/script-bench/**, crates/gfx/**, crates/brep-render/**, crates/io-*/**, crates/asset-store/**, crates/macros-reflect/**, or any production source outside crates/rge-scene-loader/**
   MUST add focused rge-scene-loader tests proving the golden simple-scene Camera entity has Transform plus Camera plus Visibility::Visible and the KeyLight entity has Transform plus Light in the loaded World
   MUST run cargo test -p rge-scene-loader and the canonical .ai/dispatch.verify.ps1 gate successfully
   ```

   **Done-criterion**:
   - `crates/rge-scene-loader` exists as the only new workspace member.
   - The crate exposes a fallible scene-to-world loader API.
   - The loader preserves entity ULIDs, attaches all typed components
     present in the current simple-scene fixture, and errors on unsupported
     component type IDs.
   - Tests in the new crate prove the golden simple-scene bridge for Camera
     and KeyLight plus unsupported-type behavior.
   - `cargo test -p rge-scene-loader` exits 0.
   - `.ai/dispatch.verify.ps1` exits 0.
   - No file outside the allowed surface changes, except this dispatch's
     own handoff/log artifacts.

44. **[DONE 2026-05-25 via PR #174 / commit `39f33ee`] Add `rge-scene-loader` golden simple-scene load+tick regression.**
   Landed via PR #174. The loader crate now owns a golden simple-scene
   load+tick regression that preserves the existing `rge-data`
   identity-only test boundary and avoids a dev-dependency cycle. The
   original brief is preserved below.

   The original `crates/rge-data` load+tick regression remains
   identity-only by design, and `rge-data` must not gain a dev-dependency
   cycle back to `rge-scene-loader`. Now that task #43 landed the loader
   crate, add the equivalent load+tick regression in the loader crate
   itself: parse the tracked golden project manifest, resolve the scene,
   load it through `rge_scene_loader`, assert typed component presence, and
   prove `World::advance_tick` behavior on the loaded world.

   **Runtime invocation note**: this task is a deliberate named +1 on top
   of the cap-121 posture used by task #43. Run as
   `.\Invoke-AiDispatchAuto.ps1 -PublishMode branch -MaxAutonomousTasks 122`
   so the cap accommodates exactly this one dispatch. The scheduler
   remains disabled and must not be re-enabled by this task.

   **Allowed file surface**:
   - EDIT only files under `crates/rge-scene-loader/tests/**`.
   - MAY add one new focused test file under `crates/rge-scene-loader/tests/`
     if that is cleaner than extending `simple_scene.rs`.
   - MAY add this dispatch's own `ai_handoffs/ISSUE-*_TASK_*.md`,
     `ai_handoffs/ISSUE-*_EXEC_*.md`, `ai_handoffs/ISSUE-*_CORRECT_*.md`
     packets plus `.meta.json` sidecars if produced by the orchestrator,
     and the queue-runner's own `ai_dispatch_logs/log_*.md`.

   **Files that MUST NOT be touched**:
   - `Cargo.toml`, `Cargo.lock`, or any crate manifest.
   - `crates/rge-data/**` - no dev-dependency cycle and no migration of the
     existing identity-only test in this task.
   - `kernel/**`, component crates, `golden-projects/**`, editor/runtime/gfx
     crates, scripts, workflows, docs, schemas, or existing handoff/log
     artifacts.
   - Any GitHub label or issue metadata except the queue runner's normal
     issue lifecycle for this dispatch.

   **Test behavior required**:
   - Parse `golden-projects/simple-scene/.rge-project` as `rge_data::Project`.
   - Resolve the first scene reference relative to the project manifest
     directory and parse it as `rge_data::Scene`.
   - Load the parsed scene through `rge_scene_loader::load_scene_into_world`.
   - Assert `world.entity_count() == scene.entities.len()`.
   - Assert the Camera entity still has `Transform`, `Camera`, and
     `Visibility::Visible`.
   - Assert the KeyLight entity still has `Transform` and `Light`.
   - Capture `current_tick()` and `last_tick()`, call `advance_tick()`, then
     assert current tick increments by one and last tick captures the prior
     current tick.

   **Halt conditions**:
   - The regression cannot be added without editing `crates/rge-data/**`,
     manifests, Cargo.lock, kernel, component crates, or the golden fixture.
   - The current loader API cannot support the tick regression without a
     production-code change.
   - The test requires relation/root-entity semantics or runtime/editor
     integration. That belongs to a later preflight and implementation task.

   **Verbatim review-gate strings** - the autonomous selector MUST copy
   these eight strings, character-for-character, into the filed GitHub
   issue body. No paraphrasing, no substitution, no reflowing. A packet
   that lacks any one of them verbatim is bounced at review:

   ```
   MUST edit only crates/rge-scene-loader/tests/** plus this dispatch's own ai_handoffs and ai_dispatch_logs artifacts
   MUST parse golden-projects/simple-scene/.rge-project as rge_data::Project and resolve the referenced scene path relative to the manifest directory
   MUST load the referenced scene through rge_scene_loader::load_scene_into_world and assert world.entity_count() equals scene.entities.len()
   MUST assert the golden Camera entity has Transform, Camera, and Visibility::Visible after loader import
   MUST assert the golden KeyLight entity has Transform and Light after loader import
   MUST assert World::advance_tick increments current_tick by one and sets last_tick to the prior current_tick on the loaded world
   MUST NOT modify Cargo.toml, Cargo.lock, crates/rge-data/**, kernel/**, component crates, golden-projects/**, editor/**, runtime/**, scripts, workflows, docs, schemas, or production source
   MUST run cargo test -p rge-scene-loader and the canonical .ai/dispatch.verify.ps1 gate successfully
   ```

   **Done-criterion**:
   - `rge-scene-loader` owns a golden simple-scene load+tick regression.
   - The regression proves project-manifest scene resolution, typed loader
     import, entity-count parity, Camera/KeyLight component presence, and
     `advance_tick` behavior.
   - `cargo test -p rge-scene-loader` exits 0.
   - `.ai/dispatch.verify.ps1` exits 0.
   - No file outside the allowed surface changes, except this dispatch's
     own handoff/log artifacts.

45. **[DONE 2026-05-25 via PR #176 / commit `d21aca5`] Read-only preflight: first `rge-scene-loader` runtime/editor consumer.**
   Landed via PR #176 after manual salvage removed the unrelated
   `AUTOMATION_IMPROVEMENTS.md` contamination from the branch diff. The audit
   selects `runtime/runtime-headless` as the smallest justified first consumer
   and names the bounded follow-up implementation dispatch in Q4. The original
   brief is preserved below.

   `rge-scene-loader` now has typed import coverage and a loader-owned
   load+tick regression. Before wiring it into an app/runtime/editor path,
   audit the current project/scene loading surfaces and dependency graph so
   the first consumer is chosen deliberately. This is an audit-only task;
   do not implement a consumer.

   **Runtime invocation note**: this task is a deliberate named +1 on top
   of the cap-122 posture used by task #44. Run as
   `.\Invoke-AiDispatchAuto.ps1 -PublishMode branch -MaxAutonomousTasks 123`
   so the cap accommodates exactly this one dispatch. The scheduler
   remains disabled and must not be re-enabled by this task.

   **Allowed file surface**:
   - READ only. Inspect `crates/rge-scene-loader/**`, `crates/rge-data/**`,
     `runtime/**`, `editor/**`, `crates/editor-*`, `crates/asset-*`,
     `crates/io-*`, `kernel/app`, `kernel/asset*`, and any existing tests
     or docs needed to identify current scene/project load surfaces.
   - MAY add this dispatch's own `ai_handoffs/ISSUE-*_TASK_*.md`,
     `ai_handoffs/ISSUE-*_EXEC_*.md`, and `.meta.json` sidecars if produced
     by the orchestrator, plus the queue-runner's own
     `ai_dispatch_logs/log_*.md`.

   **Files that MUST NOT be touched**:
   - No production source, tests, fixtures, manifests, Cargo.lock, docs,
     scripts, workflows, schemas, or status files.
   - No `CORRECT` packet should be needed because there is no code change.
   - Any GitHub label or issue metadata except the queue runner's normal
     issue lifecycle for this dispatch.

   **Audit questions required**:
   - Q1: What current code path(s), if any, parse `rge_data::Project` or
     `rge_data::Scene` outside tests? Include file/line references.
   - Q2: Which runtime/editor/application crate is the smallest valid first
     consumer of `rge-scene-loader` under the current dependency rules?
     Explicitly rule out invalid directions, including any path that would
     make `rge-data` depend on `rge-scene-loader`.
   - Q3: Is there already an app/runtime/editor API surface where a loaded
     `World` can be produced or handed off without introducing a new global
     service or registry? Include file/line references.
   - Q4: What exact implementation dispatch should come next if Q1-Q3 are
     already justified by current code? It must name one smallest source
     change, allowed files, and tests.
   - Q5: If any of Q1-Q3 is missing or ambiguous, return `NEEDS_HUMAN` and
     identify the specific owner/dependency/API decision required. Do not
     invent a consumer architecture.

   **Halt conditions**:
   - If the executor cannot answer Q1-Q3 from current code with line-cited
     evidence, the packet must end `NEEDS_HUMAN`.
   - If more than one plausible first consumer remains after current-code
     evidence, the packet must end `NEEDS_HUMAN` and list the options.
   - If answering requires modifying code or running a speculative prototype,
     halt; this is read-only.

   **Verbatim review-gate strings** - the autonomous selector MUST copy
   these eight strings, character-for-character, into the filed GitHub
   issue body. No paraphrasing, no substitution, no reflowing. A packet
   that lacks any one of them verbatim is bounced at review:

   ```
   MUST be read-only except for this dispatch's own ai_handoffs and ai_dispatch_logs artifacts
   MUST answer Q1 with file/line evidence for every current non-test rge_data::Project or rge_data::Scene parse/load surface found
   MUST answer Q2 by naming the smallest valid first consumer crate for rge-scene-loader or returning NEEDS_HUMAN if current code does not justify exactly one
   MUST explicitly rule out any dependency direction that makes rge-data depend on rge-scene-loader
   MUST answer Q3 with file/line evidence for any current app/runtime/editor API that can receive or produce a loaded World without a new global registry
   MUST name exactly one smallest implementation dispatch in Q4 only if Q1, Q2, and Q3 are all justified from current code
   MUST return NEEDS_HUMAN in Q5 if the first consumer, dependency direction, or World handoff API is ambiguous
   MUST NOT modify source, tests, fixtures, manifests, Cargo.lock, docs, scripts, workflows, schemas, status files, or existing handoff/log artifacts
   ```

   **Done-criterion**:
   - EXEC packet answers Q1-Q5 with line-cited evidence.
   - Either Q4 names one bounded implementation dispatch, or Q5 returns
     `NEEDS_HUMAN` with the exact arbiter decision needed.
   - No tracked file changes outside this dispatch's own handoff/log
     artifacts.
