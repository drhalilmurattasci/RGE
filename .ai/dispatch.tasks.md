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

27. **Implement Ctrl+4 max-fast-forward CommandBus time-scale action.**
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
