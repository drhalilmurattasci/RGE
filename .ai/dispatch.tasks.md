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

46. **[DONE 2026-05-25 via PR #178 / commit `4c03e88`] Wire `runtime-headless` as the first `rge-scene-loader` consumer.**
   Landed via PR #178. `runtime-headless` now parses a project, resolves and
   loads the first scene through `rge-scene-loader`, advances the returned
   `World` once, and reports entity/tick evidence. The approved TASK corrected
   the brief's initial `current_tick=1` expectation to `current_tick=2`
   because `World::new()` starts at tick 1 and `advance_tick()` increments it.
   The original brief is preserved below.

   Task #45 / ISSUE-175 selected `runtime/runtime-headless` as the smallest
   justified first non-test consumer for `rge-scene-loader`. Implement that
   bounded follow-up only: parse a project manifest, resolve the first scene,
   load it through the existing scene loader, advance one tick, and report the
   resulting world count/tick.

   **Runtime invocation note**: this task is a deliberate named +1 after the
   recovered task #45. Current `ai-auto` count is 122 after #175 salvage label
   cleanup. Run as
   `.\Invoke-AiDispatchAuto.ps1 -PublishMode branch -MaxAutonomousTasks 123`
   so the cap accommodates exactly this one dispatch. The scheduler remains
   disabled and must not be re-enabled by this task.

   **Allowed file surface**:
   - EDIT `runtime/runtime-headless/Cargo.toml`.
   - EDIT `runtime/runtime-headless/src/main.rs`.
   - MAY add exactly one integration test under
     `runtime/runtime-headless/tests/**`.
   - MAY edit `Cargo.lock` only for the mechanical dependency-list update
     caused by adding deps to `rge-runtime-headless`; no unrelated lockfile
     churn.
   - MAY add this dispatch's own `ai_handoffs/ISSUE-*_TASK_*.md`,
     `ai_handoffs/ISSUE-*_EXEC_*.md`, `ai_handoffs/ISSUE-*_CORRECT_*.md`
     packets plus `.meta.json` sidecars if produced by the orchestrator,
     and the queue-runner's own `ai_dispatch_logs/log_*.md`.

   **Files that MUST NOT be touched**:
   - No changes under `crates/rge-scene-loader/**`, `crates/rge-data/**`,
     `kernel/**`, `crates/components-*/**`, `editor/**`,
     `crates/editor-*`, `crates/asset-*`, `crates/io-*`, other
     `runtime/runtime-{desktop,mobile,web}/**` stubs, tools, workflows,
     docs, schemas, plans, ADRs, status files, golden fixtures, or existing
     handoff/log artifacts.
   - No workspace member changes, no `[workspace.dependencies]` changes, no
     feature flags, no global registry, no reflection machinery, no
     snapshot-restore path, and no editor-shell constructor work.
   - Any GitHub label or issue metadata except the queue runner's normal
     issue lifecycle for this dispatch.

   **Implementation behavior required**:
   - `runtime-headless` accepts exactly one positional `<project-path>`
     argument. No optional flags or multi-argument CLI in this task.
   - Read the project file with `std::fs::read_to_string` and parse it as
     `rge_data::Project` using `ron::from_str`.
   - Resolve the first scene reference relative to the project manifest
     directory, read that scene file, and parse it as `rge_data::Scene`.
   - Call `rge_scene_loader::load_scene_into_world(&scene)`.
   - Call `world.advance_tick()` exactly once after loading.
   - Print one concise stdout line containing the loaded entity count and the
     current tick after the advance, using `world.entity_count()` and
     `world.current_tick()`.
   - Return a non-zero process exit on parse, I/O, missing-scene, or loader
     errors.

   **Test behavior required**:
   - Add one focused integration test for the `runtime-headless` binary.
   - The test invokes the binary with
     `golden-projects/simple-scene/.rge-project`.
   - The test asserts successful exit and stdout evidence that the loaded
     world has 2 entities and `current_tick` is 1 after the tick advance.
   - The test must not edit or regenerate any golden fixture.

   **Halt conditions**:
   - Halt if this cannot be implemented without editing `rge-scene-loader`,
     `rge-data`, kernel, component crates, editor/editor-shell, other runtime
     stubs, golden fixtures, workspace-level dependency configuration, or any
     source outside `runtime/runtime-headless/**`.
   - Halt if the current loader API cannot load the golden simple-scene
     fixture without a loader/schema/kernel change.
   - Halt if architecture lints reject `runtime-headless -> rge-data` or
     `runtime-headless -> rge-scene-loader`.
   - Halt if a global registry, reflection, snapshot restore, editor shell
     handoff, or CLI convention decision is required.

   **Verbatim review-gate strings** - the autonomous selector MUST copy
   these eight strings, character-for-character, into the filed GitHub
   issue body. No paraphrasing, no substitution, no reflowing. A packet
   that lacks any one of them verbatim is bounced at review:

   ```
   MUST edit only runtime/runtime-headless/Cargo.toml, runtime/runtime-headless/src/main.rs, one optional runtime/runtime-headless/tests/** integration test, Cargo.lock mechanical rge-runtime-headless dependency-list churn, and this dispatch's own ai_handoffs and ai_dispatch_logs artifacts
   MUST make runtime-headless accept exactly one positional <project-path> argument and no optional flags
   MUST parse the project as rge_data::Project, resolve the first scene relative to the project manifest directory, and parse that scene as rge_data::Scene
   MUST load the parsed scene through rge_scene_loader::load_scene_into_world and call World::advance_tick exactly once after load
   MUST print stdout evidence of entity_count 2 and current_tick 1 when run against golden-projects/simple-scene/.rge-project
   MUST add one runtime-headless integration test that invokes the binary against golden-projects/simple-scene/.rge-project and asserts successful exit plus the stdout count/tick evidence
   MUST NOT modify crates/rge-scene-loader/**, crates/rge-data/**, kernel/**, component crates, editor/**, other runtime stubs, golden-projects/**, workspace membership/dependency configuration, scripts, workflows, docs, schemas, plans, status files, or existing handoff/log artifacts
   MUST run cargo build -p rge-runtime-headless, cargo test -p rge-runtime-headless --no-fail-fast, cargo run -q -p rge-tool-architecture-lints -- all, and the canonical .ai/dispatch.verify.ps1 gate successfully
   ```

   **Done-criterion**:
   - `runtime-headless` is the first non-test consumer of `rge-scene-loader`.
   - Running the binary with the golden simple-scene project path loads the
     scene into a `World`, advances one tick, and reports 2 entities at
     current tick 1.
   - The focused runtime-headless integration test passes.
   - The required verification gates pass.
   - No file outside the allowed surface changes, except this dispatch's own
     handoff/log artifacts.

47. **[DONE 2026-05-25 via PR #181 / commit `f8b2246`] Guard queue staging against out-of-scope dispatch files.**
   Landed via PR #181. Queue now runs a fail-closed scope guard after
   `Write-DispatchLog` and before `git add -A`; it allows active dispatch
   handoff artifacts, the exact queue log, and positive TASK packet allowlist
   paths, and rejects out-of-scope changed/untracked paths before staging. The
   original brief is preserved below.

   ISSUE-175 was blocked because an unrelated live-root
   `AUTOMATION_IMPROVEMENTS.md` file was swept into the queue commit by the
   broad `git add -A` publish path. Add a commit-path guard so the queue
   refuses to stage or commit files outside the active TASK packet's positive
   allowed file surface plus the current dispatch's own handoff/log artifacts.

   **Runtime invocation note**: this task is a deliberate named +1 after task
   #46. Current `ai-auto` count is 123. Run as
   `.\Invoke-AiDispatchAuto.ps1 -PublishMode branch -MaxAutonomousTasks 124`
   so the cap accommodates exactly this one dispatch. The scheduler remains
   disabled and must not be re-enabled by this task.

   **Allowed file surface**:
   - EDIT only `Invoke-AiDispatchQueue.ps1`.
   - MAY add this dispatch's own `ai_handoffs/ISSUE-*_TASK_*.md`,
     `ai_handoffs/ISSUE-*_EXEC_*.md`, `ai_handoffs/ISSUE-*_CORRECT_*.md`
     packets plus `.meta.json` sidecars if produced by the orchestrator,
     and the queue-runner's own `ai_dispatch_logs/log_*.md`.

   **Files that MUST NOT be touched**:
   - Do not edit `Invoke-AiDispatchAuto.ps1`, `Invoke-AiDispatchLoop.ps1`,
     `Wait-GitHubActions.ps1`, scheduler scripts, docs, task brief,
     workflows, schemas, Rust source, Cargo files, golden fixtures, status
     files, existing handoff/log artifacts, or sandbox worktrees.
   - Do not introduce an external parser dependency or require PowerShell 7.
     The automation remains Windows PowerShell 5.1 compatible.
   - Do not change publish semantics except for blocking out-of-scope staging.

   **Implementation behavior required**:
   - Before the queue stages dispatch output for its branch commit, enumerate
     changed and untracked paths with `git status --short --untracked-files=all`
     or a safer equivalent.
   - Build an allowlist from:
     - the active dispatch's own artifacts:
       `ai_handoffs/<DispatchId>_TASK_*`,
       `ai_handoffs/<DispatchId>_EXEC_*`,
       `ai_handoffs/<DispatchId>_CORRECT_*`,
       matching `.meta.json` sidecars, and the queue's own
       `ai_dispatch_logs/log_*.md`;
     - positive allowed-path text in the active TASK packet, limited to
       sections/headings such as `Allowed file surface`, `MAY edit`,
       `MAY add`, or equivalent positive wording. Do not extract paths from
       `MUST NOT`, forbidden, halt-condition, or negative sections.
   - Support exact file paths and directory/glob-like prefixes already used in
     task packets, including `path/**`, `path/`, and backticked paths.
   - If every changed/untracked path is allowed, preserve current behavior:
     stage and commit the dispatch branch normally.
   - If any path is outside the allowlist, print a clear scope-guard failure
     listing the disallowed paths, do not stage or commit those paths, and do
     not publish the branch. The result must be visible to the operator and
     must not silently succeed.
   - The guard must have caught ISSUE-175's root-level
     `AUTOMATION_IMPROVEMENTS.md` as disallowed while still allowing the
     ISSUE-175 TASK/EXEC packets, sidecars, and queue log.

   **Halt conditions**:
   - Halt if the guard cannot be implemented in `Invoke-AiDispatchQueue.ps1`
     without editing the auto runner, loop runner, task brief, docs, tests,
     workflows, or Rust workspace files.
   - Halt if parsing the TASK packet's positive allowed surface is too
     ambiguous to avoid accidentally allowing paths from `MUST NOT` sections.
   - Halt if the change would allow out-of-scope files by default when no
     allowed surface can be parsed. Fail closed instead.
   - Halt if preserving issue/result visibility would require a broader queue
     lifecycle refactor.

   **Verbatim review-gate strings** - the autonomous selector MUST copy these
   eight strings, character-for-character, into the filed GitHub issue body.
   No paraphrasing, no substitution, no reflowing. A packet that lacks any one
   of them verbatim is bounced at review:

   ```
   MUST edit only Invoke-AiDispatchQueue.ps1 plus this dispatch's own ai_handoffs and ai_dispatch_logs artifacts
   MUST add a queue staging guard before broad dispatch branch staging or commit
   MUST always allow only the active dispatch's own ai_handoffs/<DispatchId>_{TASK,EXEC,CORRECT}_* artifacts, matching .meta.json sidecars, queue ai_dispatch_logs/log_*.md, and positive allowed paths parsed from the active TASK packet
   MUST parse positive allowed-path sections only and MUST NOT extract allowed paths from MUST NOT, forbidden, halt-condition, or negative sections
   MUST fail closed when no positive allowed file surface can be parsed
   MUST list any disallowed changed or untracked paths clearly and MUST NOT stage, commit, or publish those disallowed paths
   MUST preserve existing behavior when all changed and untracked paths are inside the allowlist
   MUST run PowerShell parser validation for Invoke-AiDispatchQueue.ps1, git diff --check, and the canonical .ai/dispatch.verify.ps1 gate successfully
   ```

   **Done-criterion**:
   - `Invoke-AiDispatchQueue.ps1` blocks out-of-scope files before staging.
   - ISSUE-175's contamination pattern would be rejected before commit.
   - Valid dispatch artifacts and task-allowed paths still commit normally.
   - Verification gates pass.
   - No file outside the allowed surface changes, except this dispatch's own
     handoff/log artifacts.

48. **[DONE 2026-05-25 via PR #183 / commit `5fe0321`] Add Codex stall watchdog to `Invoke-WithTimeout`.**
   Landed via PR #183. `Invoke-WithTimeout` now has an opt-in stall watchdog
   used only by `Invoke-CodexPrompt`: it arms after first non-zero log output,
   treats `OutFile.Length` growth as progress, returns
   `Stalled=$true`/`TimedOut=$true`/`Code=125` on stall, and preserves the
   original hard-timeout control flow when `StallThresholdSec` is zero. The
   original brief is preserved below.

   `Invoke-AiDispatchLoop.ps1` currently caps Codex CLI calls with only the
   wall-clock `-ModelTimeoutSec` timeout. ISSUE-180 attempt 1 showed a more
   specific failure mode: Codex stayed alive while the log stopped growing,
   forcing the queue to wait for the full timeout before retrying the whole
   dispatch from plan rev0. Add a Codex-only log-stall watchdog so this
   terminal infrastructure failure is caught in about five minutes instead of
   thirty, without changing legacy timeout behavior for other callers.

   **Runtime invocation note**: this task is a deliberate named +1 after task
   #47. Current `ai-auto` count is 124. Run as
   `.\Invoke-AiDispatchAuto.ps1 -PublishMode branch -MaxAutonomousTasks 125`
   so the cap accommodates exactly this one dispatch. The scheduler remains
   disabled and must not be re-enabled by this task.

   **Required TASK packet shape**:
   - The generated TASK packet MUST include a `### MAY edit` section listing
     exactly `Invoke-AiDispatchLoop.ps1`.
   - The generated TASK packet MAY include a `### MAY add new files` section
     only for this dispatch's own `ai_handoffs/ISSUE-*_TASK_*.md`,
     `ai_handoffs/ISSUE-*_EXEC_*.md`, `ai_handoffs/ISSUE-*_CORRECT_*.md`
     packets, matching `.meta.json` sidecars, and its own
     `ai_dispatch_logs/log_*.md` file.
   - The generated TASK packet MUST NOT include the sandbox worktree,
     `CLAUDE_REVIEW.md`, `SANDBOX_REVIEW.md`, or `TASK_PACKETS.md` in any
     positive MAY-edit/MAY-add surface.

   **Allowed file surface**:
   - EDIT only `Invoke-AiDispatchLoop.ps1`.
   - MAY add this dispatch's own handoff packets, handoff sidecars, and queue
     log as produced by the orchestrator/queue.

   **Files that MUST NOT be touched**:
   - Do not edit `Invoke-AiDispatchAuto.ps1`, `Invoke-AiDispatchQueue.ps1`,
     `Wait-GitHubActions.ps1`, scheduler scripts, docs, task brief, workflows,
     schemas, Rust source, Cargo files, golden fixtures, status files,
     existing handoff/log artifacts, or sandbox worktrees.
   - Do not implement the Codex pre-flight pitfall audit in this dispatch.
     That is the next task after the watchdog lands.
   - Do not change Claude readiness, Claude execute, Claude control, or
     verification invocation paths in this dispatch.

   **Implementation behavior required**:
   - Extend `Invoke-WithTimeout` with optional parameters:
     `-StallThresholdSec` (default `0`, disabled) and `-PollIntervalSec`
     (default `5`).
   - When `StallThresholdSec <= 0`, preserve the existing
     `WaitForExit($TimeoutSec * 1000)` / else `taskkill + Code=124` behavior
     for all callers.
   - When `StallThresholdSec > 0`, poll the command process at
     `PollIntervalSec` while also enforcing the wall-clock timeout.
   - The stall timer must arm only after `OutFile` has non-zero size. A
     legitimate long silent first Codex response must fall back to the normal
     wall-clock timeout, not the stall watchdog.
   - If `OutFile` has produced output and then stops growing for
     `StallThresholdSec` consecutive seconds while the process remains alive,
     kill the process tree with `taskkill /T /F`, and return
     `Stalled=$true`, `TimedOut=$true`, `Code=125`.
   - Add script parameter
     `[ValidateRange(0, 3600)] [int]$CodexStallThresholdSec = 300`.
   - Wire only `Invoke-CodexPrompt` to pass
     `-StallThresholdSec $CodexStallThresholdSec` to `Invoke-WithTimeout`.
   - In `Invoke-CodexPrompt`, check `$r.Stalled` before `$r.TimedOut` and
     emit a distinct fail message:
     `codex exec stalled: no log growth for ${CodexStallThresholdSec}s after first output. Killed process tree. See $LogPath`
   - Keep `Code` and `TimedOut` populated on every return path. Additive
     field `Stalled` must exist on every returned object.

   **Halt conditions**:
   - Halt if preserving exact legacy behavior for `-StallThresholdSec 0` is
     not possible.
   - Halt if any caller other than `Invoke-CodexPrompt` would need to opt into
     the watchdog in this dispatch.
   - Halt if the process polling approach is not compatible with Windows
     PowerShell 5.1.
   - Halt if the implementation would require editing any file other than
     `Invoke-AiDispatchLoop.ps1` plus this dispatch's own generated artifacts.

   **Verbatim review-gate strings** - the autonomous selector MUST copy these
   eight strings, character-for-character, into the filed GitHub issue body.
   No paraphrasing, no substitution, no reflowing. A packet that lacks any one
   of them verbatim is bounced at review:

   ```
   MUST edit only Invoke-AiDispatchLoop.ps1 plus this dispatch's own ai_handoffs and ai_dispatch_logs artifacts
   MUST add optional Invoke-WithTimeout parameters StallThresholdSec and PollIntervalSec while preserving exact legacy behavior when StallThresholdSec is 0
   MUST arm the stall watchdog only after OutFile has non-zero size
   MUST return Stalled=true TimedOut=true Code=125 when the watchdog kills a stalled Codex process
   MUST wire the watchdog only through Invoke-CodexPrompt using CodexStallThresholdSec default 300
   MUST NOT implement the pre-flight audit, structured checklist, prompt injection, or Codex control checklist in this dispatch
   MUST preserve PowerShell 5.1 compatibility and avoid changing Claude, verification, queue, auto, scheduler, Rust, Cargo, docs, or schema files
   MUST run PowerShell parser validation for Invoke-AiDispatchLoop.ps1, git diff --check, and the canonical .ai/dispatch.verify.ps1 gate successfully
   ```

   **Verification required**:
   - PowerShell parser validation for `Invoke-AiDispatchLoop.ps1` reports
     zero errors.
   - `git diff --check` reports no whitespace errors.
   - `.ai/dispatch.verify.ps1` passes.
   - The executor explicitly notes that `Invoke-WithTimeout` returns a
     top-level `Stalled` field on every code path.
   - No file outside the allowed surface changes, except this dispatch's own
     handoff/log artifacts.

   **Notes for executor**:
   - A reviewed sandbox draft exists at
     `A:\RCAD\dispatch-worktrees\sandbox-improvements-002\Invoke-AiDispatchLoop.ps1`.
     It is read-only reference material. Implement against current `main`;
     do not rebase or merge the sandbox branch.
   - The sandbox draft also contains the separate pre-flight audit work. Do
     not land that code in this task.

49. **[DONE 2026-05-25 via PR #185 / commit `1b0798f`] Add opt-in Codex pre-flight pitfall audit.**
   Landed via PR #185. `Invoke-AiDispatchLoop.ps1` now supports opt-in
   `-EnablePreflightAudit`: after TASK finalization it can run a read-only
   Codex audit, validate a marker-delimited `# Pre-flight Audit` checklist
   with stable `P#`/`V#` IDs, write `codex.preflight.md` only after
   validation, inject the checklist into Claude execute round 0, and pass it
   to Codex control on every round. Default behavior is unchanged when the
   switch is omitted. The original brief is preserved below.

   ISSUE-180 showed repeated correction rounds for known automation pitfalls
   that could have been surfaced before Claude executed. Add an opt-in
   Codex pre-flight audit to `Invoke-AiDispatchLoop.ps1`: after TASK approval
   and before Claude execute round 0, Codex may produce a bounded in-scope
   checklist that Claude receives during execution and Codex receives during
   control review. The TASK packet remains authoritative; the checklist is a
   guardrail against known mistakes, not a scope-expansion mechanism.

   **Runtime invocation note**: this task is a deliberate named +1 after task
   #48. Current `ai-auto` count is 125. Run as
   `.\Invoke-AiDispatchAuto.ps1 -PublishMode branch -MaxAutonomousTasks 126`
   so the cap accommodates exactly this one dispatch. The scheduler remains
   disabled and must not be re-enabled by this task.

   **Required TASK packet shape**:
   - The generated TASK packet MUST include a `### MAY edit` section listing
     exactly `Invoke-AiDispatchLoop.ps1`.
   - The generated TASK packet MAY include a `### MAY add new files` section
     only for this dispatch's own `ai_handoffs/ISSUE-*_TASK_*.md`,
     `ai_handoffs/ISSUE-*_EXEC_*.md`, `ai_handoffs/ISSUE-*_CORRECT_*.md`
     packets, matching `.meta.json` sidecars, and its own
     `ai_dispatch_logs/log_*.md` file.
   - The generated TASK packet MUST NOT include the sandbox worktree,
     `CLAUDE_REVIEW.md`, `SANDBOX_REVIEW.md`, or `TASK_PACKETS.md` in any
     positive MAY-edit/MAY-add surface.

   **Allowed file surface**:
   - EDIT only `Invoke-AiDispatchLoop.ps1`.
   - MAY add this dispatch's own handoff packets, handoff sidecars, and queue
     log as produced by the orchestrator/queue.

   **Files that MUST NOT be touched**:
   - Do not edit `Invoke-AiDispatchAuto.ps1`, `Invoke-AiDispatchQueue.ps1`,
     `Wait-GitHubActions.ps1`, scheduler scripts, docs, task brief, workflows,
     schemas, Rust source, Cargo files, golden fixtures, status files,
     existing handoff/log artifacts, or sandbox worktrees.
   - Do not change the Codex stall watchdog that landed in task #48 except
     where calling existing `Invoke-CodexPrompt` is necessary.
   - Do not add structured JSON output, change `codex_control.schema.json`,
     add per-task-class enabling, add memory/knowledge-base files, or alter
     correction-round semantics.

   **Implementation behavior required**:
   - Add script parameter `[switch]$EnablePreflightAudit`, default off.
   - Add `Invoke-CodexPreflightAudit -TaskPacket <FileInfo>` that:
     - runs Codex through existing `Invoke-CodexPrompt` with sandbox
       `read-only`;
     - asks for concrete in-scope pitfalls for the approved TASK only;
     - requires a strict Markdown block headed `# Pre-flight Audit`, with
       `## Why this matters`, `## Checklist`, and `## Verification hints`;
     - requires stable checklist IDs such as `P1`, `P2`, `V1`, and `V2`;
     - writes the extracted block to `<run-dir>/codex.preflight.md`;
     - returns the path, or `$null` if extraction fails.
   - The pre-flight prompt MUST state that the TASK packet remains
     authoritative and MUST NOT invent deliverables, allowed files,
     dependencies, gates, or scope.
   - Extraction failure must be fail-soft and operator-visible: continue the
     dispatch without prompt injection and write a clear status line.
   - Modify `Invoke-ClaudeExecute` to accept optional `-PitfallsPath`. On
     round 0 only, when the path exists, inject a clearly labelled
     `Pre-flight checklist` section. The injection text MUST state that the
     TASK packet remains authoritative, conflicts are resolved in favor of the
     TASK, and the checklist must not cause extra files, deliverables, or
     gates.
   - Modify `Invoke-CodexControl` to accept optional `-PreflightAuditPath`.
     When the path exists, include the same checklist in the control prompt
     with scope-protection wording so control reviews in-scope checklist items
     but does not fail work omitted because the TASK did not allow it.
   - In the main flow, when `-EnablePreflightAudit` is set, run the pre-flight
     audit once after TASK finalize and before the execute loop. Pass the
     resulting path to Claude only on execute round 0 and to Codex control on
     every round.
   - When `-EnablePreflightAudit` is not set, there must be no new Codex
     pre-flight call, no `codex.preflight.*` files, and no pre-flight prompt
     injection.

   **Halt conditions**:
   - Halt if implementing this requires editing any file other than
     `Invoke-AiDispatchLoop.ps1` plus this dispatch's own generated artifacts.
   - Halt if `Invoke-CodexPreflightAudit` cannot run in `read-only` sandbox.
   - Halt if the Markdown extraction must be loosened enough that unrelated
     transcript content could be ingested as the checklist.
   - Halt if the design would require changes to `Invoke-CodexPrompt`, the
     stall watchdog, `codex_control.schema.json`, queue/auto scripts, or any
     Rust workspace files.
   - Halt if default behavior with `-EnablePreflightAudit` unset cannot remain
     behaviorally identical to current main.

   **Verbatim review-gate strings** - the autonomous selector MUST copy these
   eight strings, character-for-character, into the filed GitHub issue body.
   No paraphrasing, no substitution, no reflowing. A packet that lacks any one
   of them verbatim is bounced at review:

   ```
   MUST edit only Invoke-AiDispatchLoop.ps1 plus this dispatch's own ai_handoffs and ai_dispatch_logs artifacts
   MUST add EnablePreflightAudit as an opt-in switch with no default behavior change
   MUST run the pre-flight Codex audit in read-only sandbox and write codex.preflight.md only when extraction succeeds
   MUST use stable P-number and V-number checklist IDs in the Pre-flight Audit Markdown shape
   MUST inject the checklist into Claude execute round 0 only and into Codex control on every round when a checklist exists
   MUST state in both execute and control prompts that the TASK packet remains authoritative and the checklist must not expand scope
   MUST NOT change Invoke-CodexPrompt, the stall watchdog, codex_control.schema.json, queue, auto, scheduler, Rust, Cargo, docs, or schema files
   MUST run PowerShell parser validation for Invoke-AiDispatchLoop.ps1, git diff --check, and the canonical .ai/dispatch.verify.ps1 gate successfully
   ```

   **Verification required**:
   - PowerShell parser validation for `Invoke-AiDispatchLoop.ps1` reports
     zero errors.
   - `git diff --check` reports no whitespace errors.
   - `.ai/dispatch.verify.ps1` passes.
   - The executor explicitly notes whether it performed inspection or a safe
     local canary proving the default path does not create
     `codex.preflight.md`/`.log` or inject `Pre-flight checklist`.
   - The executor explicitly notes whether it performed inspection or a safe
     local canary proving the opt-in path creates `codex.preflight.md` and
     injects `Pre-flight checklist` into both Claude execute and Codex
     control prompts.
   - No file outside the allowed surface changes, except this dispatch's own
     handoff/log artifacts.

   **Notes for executor**:
   - A reviewed sandbox draft exists at
     `A:\RCAD\dispatch-worktrees\sandbox-improvements-002\Invoke-AiDispatchLoop.ps1`.
     It is read-only reference material. Implement against current `main`;
     do not rebase or merge the sandbox branch.
   - The watchdog from task #48 is already on main. This task must not
     re-introduce or reshape it.

50. **[DONE 2026-05-25 via PR #187 / commit `d10e8ab`] Persist automation timing traces as JSONL events.**
   Landed via PR #187. `Invoke-AiDispatchAuto.ps1` and
   `Invoke-AiDispatchQueue.ps1` now persist existing opt-in timing traces as
   best-effort JSONL under ignored `.ai/dispatch-trace/`, using the existing
   `-TraceTiming` / `RGE_AI_DISPATCH_TRACE_TIMING` enablement and preserving
   the console trace output. Each line carries timestamp, elapsed seconds,
   script, pid, event, message, and repo context. The original brief is
   preserved below.

   The automation already has opt-in human-readable timing traces in
   `Invoke-AiDispatchAuto.ps1` and `Invoke-AiDispatchQueue.ps1`. Persist the
   same events to JSONL so later dispatches can tune watchdog thresholds,
   classify failure rates, measure empty-tick latency, and evaluate same-phase
   retry success without scraping console text. This task is emitter-only:
   add durable trace files and do not add aggregation, alerts, taxonomy labels,
   retry policy, or speedup behavior.

   **Runtime invocation note**: this task is a deliberate named +1 after task
   #49. Current `ai-auto` count is 126. Run as
   `.\Invoke-AiDispatchAuto.ps1 -PublishMode branch -MaxAutonomousTasks 127`
   so the cap accommodates exactly this one dispatch. The scheduler remains
   disabled and must not be re-enabled by this task.

   **Required TASK packet shape**:
   - The generated TASK packet MUST include a `### MAY edit` section listing
     exactly `Invoke-AiDispatchAuto.ps1` and `Invoke-AiDispatchQueue.ps1`.
   - The generated TASK packet MAY include a `### MAY add new files` section
     only for this dispatch's own `ai_handoffs/ISSUE-*_TASK_*.md`,
     `ai_handoffs/ISSUE-*_EXEC_*.md`, `ai_handoffs/ISSUE-*_CORRECT_*.md`
     packets, matching `.meta.json` sidecars, and its own
     `ai_dispatch_logs/log_*.md` file.

   **Allowed file surface**:
   - EDIT only `Invoke-AiDispatchAuto.ps1` and `Invoke-AiDispatchQueue.ps1`.
   - MAY add this dispatch's own handoff packets, handoff sidecars, and queue
     log as produced by the orchestrator/queue.

   **Files that MUST NOT be touched**:
   - Do not edit `Invoke-AiDispatchLoop.ps1`, watchdog/preflight code,
     `Wait-GitHubActions.ps1`, scheduler scripts, health scripts, docs, task
     brief, workflows, schemas, Rust source, Cargo files, golden fixtures,
     status files, existing handoff/log artifacts, or sandbox worktrees.
   - Do not add an aggregator, trend command, alerting, failure taxonomy,
     labels, retry policy, recovery routes, speedup behavior, or dashboard.
   - Do not change the meaning or visibility of current console trace lines.

   **Implementation behavior required**:
   - Add JSONL trace persistence to both Auto and Queue using the existing
     `Write-TimingTrace` call sites.
   - Preserve the existing `-TraceTiming` switch and
     `RGE_AI_DISPATCH_TRACE_TIMING` environment fallback. JSONL persistence
     must be opt-in through the same trace-enabled condition unless a narrower
     explicit trace-file parameter is added.
   - Each trace event must be a single compact JSON object on one line.
   - Each event must include at least: ISO timestamp, elapsed seconds,
     script name (`auto` or `queue`), process id, event message, and current
     working directory or repo root.
   - Queue events should include dispatch id and branch when those are known,
     either by parsing the message or by setting script-scope context near the
     queue's selected issue/branch. Do not perform GitHub API calls solely for
     tracing.
   - Trace files should be written under an existing gitignored local scratch
     path, preferably `.ai/trace/`, with deterministic per-process filenames
     such as `auto_<timestamp>_<pid>.jsonl` and
     `queue_<timestamp>_<pid>.jsonl`.
   - File writes must be append-only, UTF-8, and best-effort: a JSONL write
     failure must not fail the dispatch unless it is caused by a syntax/runtime
     error in the script itself.
   - Console trace output must remain unchanged for existing operators.
   - When tracing is disabled, no JSONL file should be created and no extra
     trace work should run beyond trivial boolean checks.

   **Halt conditions**:
   - Halt if this cannot be implemented by editing only
     `Invoke-AiDispatchAuto.ps1` and `Invoke-AiDispatchQueue.ps1` plus this
     dispatch's own generated artifacts.
   - Halt if writing JSONL would require changing queue publish semantics,
     auto-selection behavior, watchdog/preflight behavior, labels, retry
     policy, scheduler behavior, schemas, Rust/Cargo files, or docs.
   - Halt if the implementation would make a JSONL write failure block normal
     dispatch progress.
   - Halt if PowerShell 5.1 compatibility cannot be preserved.

   **Verbatim review-gate strings** - the autonomous selector MUST copy these
   eight strings, character-for-character, into the filed GitHub issue body.
   No paraphrasing, no substitution, no reflowing. A packet that lacks any one
   of them verbatim is bounced at review:

   ```
   MUST edit only Invoke-AiDispatchAuto.ps1 and Invoke-AiDispatchQueue.ps1 plus this dispatch's own ai_handoffs and ai_dispatch_logs artifacts
   MUST persist existing timing trace events as JSONL only when TraceTiming or RGE_AI_DISPATCH_TRACE_TIMING enables tracing
   MUST preserve all existing console trace output unchanged
   MUST write one compact JSON object per line with timestamp elapsed seconds script pid event message and repo context
   MUST write trace files under an existing gitignored local scratch path such as .ai/trace and MUST NOT add those files to the dispatch commit
   MUST make JSONL writes best-effort so trace write failures do not fail dispatch progress
   MUST NOT add aggregation alerts taxonomy labels retry policy recovery routes speedups dashboards docs schemas Rust Cargo or watchdog/preflight changes
   MUST run PowerShell parser validation for Invoke-AiDispatchAuto.ps1 and Invoke-AiDispatchQueue.ps1, git diff --check, and the canonical .ai/dispatch.verify.ps1 gate successfully
   ```

   **Verification required**:
   - PowerShell parser validation for both changed scripts reports zero
     errors.
   - `git diff --check` reports no whitespace errors.
   - `.ai/dispatch.verify.ps1` passes.
   - A safe dry run or inspection shows tracing disabled creates no JSONL
     trace file.
   - A safe dry run with `-TraceTiming` or
     `RGE_AI_DISPATCH_TRACE_TIMING=1` creates a JSONL file with valid
     one-object-per-line JSON containing the required fields.
   - No file outside the allowed surface changes, except this dispatch's own
     handoff/log artifacts.

   **Notes for executor**:
   - This is item 1 of the self-improving automation sequence. It must land
     before empty-tick speedup, failure taxonomy, retry policy, or trend
     aggregation.
   - Keep this task measurement-only. Future tasks will consume the JSONL
     data; this task only emits it.

51. **[DONE 2026-05-25 via PR #189 / commit `8e38df7`] Speed up empty autonomous ticks by removing steady-state sleeps.**
   Landed via PR #189. `Invoke-AiDispatchAuto.ps1` now removes the two
   steady-state five-second primary queue retries before the cap check and
   immediately falls through to the existing REST queue cross-check when the
   primary query returns zero. Queued-work-before-cap semantics,
   ambiguous-queue skip behavior, post-issue-creation visibility polling, and
   existing console/JSONL trace behavior were preserved. The original brief is
   preserved below.

   The JSONL/trace data from recent ticks shows an avoidable 10-second delay
   on empty/cap ticks: `Invoke-AiDispatchAuto.ps1` retries the primary
   `gh issue list --label ai-dispatch` query twice with 5-second sleeps before
   running the REST cross-check. That retry is only needed after this script
   creates a new issue, where GitHub label indexing can lag. For steady-state
   empty/cap ticks, use one primary query plus the existing REST cross-check
   and then proceed to cap check or skip. Preserve the issue-creation
   visibility wait loop unchanged.

   **Runtime invocation note**: this task is a deliberate named +1 after task
   #50. Current `ai-auto` count is 127. Run as
   `.\Invoke-AiDispatchAuto.ps1 -PublishMode branch -MaxAutonomousTasks 128`
   so the cap accommodates exactly this one dispatch. The scheduler remains
   disabled and must not be re-enabled by this task.

   **Required TASK packet shape**:
   - The generated TASK packet MUST include a `### MAY edit` section listing
     exactly `Invoke-AiDispatchAuto.ps1`.
   - The generated TASK packet MAY include a `### MAY add new files` section
     only for this dispatch's own `ai_handoffs/ISSUE-*_TASK_*.md`,
     `ai_handoffs/ISSUE-*_EXEC_*.md`, `ai_handoffs/ISSUE-*_CORRECT_*.md`
     packets, matching `.meta.json` sidecars, and its own
     `ai_dispatch_logs/log_*.md` file.

   **Allowed file surface**:
   - EDIT only `Invoke-AiDispatchAuto.ps1`.
   - MAY add this dispatch's own handoff packets, handoff sidecars, and queue
     log as produced by the orchestrator/queue.

   **Files that MUST NOT be touched**:
   - Do not edit `Invoke-AiDispatchQueue.ps1`, `Invoke-AiDispatchLoop.ps1`,
     watchdog/preflight code, JSONL trace emitter code except where the
     existing trace call placement naturally remains, scheduler scripts, docs,
     task brief, workflows, schemas, Rust source, Cargo files, golden fixtures,
     status files, existing handoff/log artifacts, or sandbox worktrees.
   - Do not add retry policy, taxonomy labels, recovery routes, aggregators,
     dashboards, trace schema changes, or queue behavior changes.

   **Implementation behavior required**:
   - In the queue-empty check before cap selection, remove the two
     `Start-Sleep -Seconds 5` primary-query retry loop.
   - The steady-state queue check must become: primary `gh issue list` once;
     if it returns zero, immediately run the existing REST cross-check; if
     REST sees queued issues, drain them; if REST confirms zero, continue to
     cap check; if REST fails, keep the current ambiguous-state skip behavior.
   - Preserve the existing post-issue-creation visibility wait loop that polls
     for the newly-created issue before running the queue. That wait handles
     real GitHub label-index lag and must not be removed in this task.
   - Preserve all current console output meanings, timing trace semantics, and
     JSONL trace emission behavior.
   - Preserve the cap semantics: queued work drains before cap; cap only gates
     creating new autonomous issues.
   - Preserve dry-run behavior and no-queue/no-brief/no-selection exits.

   **Halt conditions**:
   - Halt if the speedup cannot be implemented by editing only
     `Invoke-AiDispatchAuto.ps1` plus this dispatch's own generated artifacts.
   - Halt if removing the steady-state sleeps would require changing issue
     creation, queue invocation, queue runner behavior, labels, recovery,
     scheduler, watchdog/preflight, JSONL emitter schema, or Rust/Cargo files.
   - Halt if the REST cross-check cannot remain the fallback for empty primary
     queue results.
   - Halt if the post-issue-creation visibility wait loop cannot remain
     intact.
   - Halt if PowerShell 5.1 compatibility cannot be preserved.

   **Verbatim review-gate strings** - the autonomous selector MUST copy these
   eight strings, character-for-character, into the filed GitHub issue body.
   No paraphrasing, no substitution, no reflowing. A packet that lacks any one
   of them verbatim is bounced at review:

   ```
   MUST edit only Invoke-AiDispatchAuto.ps1 plus this dispatch's own ai_handoffs and ai_dispatch_logs artifacts
   MUST remove the two steady-state five-second queue-check sleeps before the cap check
   MUST keep the REST issues cross-check as the immediate fallback when the primary queue query returns zero
   MUST preserve the post-issue-creation visibility wait loop unchanged
   MUST preserve queued-work-before-cap semantics and ambiguous-queue skip behavior
   MUST preserve existing console trace and JSONL trace behavior without changing the trace schema
   MUST NOT change queue runner loop runner watchdog preflight scheduler labels retry policy recovery routes Rust Cargo docs schemas or dashboards
   MUST run PowerShell parser validation for Invoke-AiDispatchAuto.ps1, git diff --check, and the canonical .ai/dispatch.verify.ps1 gate successfully
   ```

   **Verification required**:
   - PowerShell parser validation for `Invoke-AiDispatchAuto.ps1` reports zero
     errors.
   - `git diff --check` reports no whitespace errors.
   - `.ai/dispatch.verify.ps1` passes.
   - A safe dry run with the cap already reached and `-TraceTiming` enabled
     demonstrates the queue-check to cap-check path no longer pays the two
     5-second sleeps. The executor should report the observed elapsed time
     from `auto.queue-check: primary done` to `auto.cap-check: start`.
   - Inspection confirms the post-issue-creation visibility wait loop remains
     present.
   - No file outside the allowed surface changes, except this dispatch's own
     handoff/log artifacts.

   **Notes for executor**:
   - This is item 2 of the self-improving automation sequence. It should use
     the JSONL/trace emitter from task #50 for measurement but must not change
     that emitter's schema.
   - Keep this task speed-only. Failure taxonomy, retry policy, and trend
     aggregation are separate follow-up dispatches.

52. **[DONE 2026-05-25 via PR #191 / commit `23d1bca`] Add label-only dispatch failure taxonomy.**
   Landed via PR #191. `Invoke-AiDispatchQueue.ps1` now creates terminal
   failure taxonomy labels for stall, timeout, blocked, verification, control,
   publish, and unknown failures, applies them only to non-retry terminal
   failed issues alongside `ai-dispatch-failed`, and verifies their presence
   during label finalization. Success, retry, queue selection, cap, publish,
   branch, watchdog/preflight, and JSONL trace behavior were preserved. The
   original brief is preserved below.

   The queue currently collapses terminal failures into the single
   `ai-dispatch-failed` label. That is enough to halt automation, but it loses
   the signal needed to tune watchdog thresholds, retry policy, and later
   recovery routes. Add a small, label-only taxonomy in
   `Invoke-AiDispatchQueue.ps1` so terminal failed issues carry one or more
   specific failure-class labels. This task must not change recovery behavior:
   no new retry routes, no same-phase retry, no JSONL schema changes, and no
   changes to which failures halt the autonomous loop.

   **Runtime invocation note**: this task is a deliberate named +1 after task
   #51. Current `ai-auto` count is 128. Run as
   `.\Invoke-AiDispatchAuto.ps1 -PublishMode branch -MaxAutonomousTasks 129`
   so the cap accommodates exactly this one dispatch. The scheduler remains
   disabled and must not be re-enabled by this task.

   **Required TASK packet shape**:
   - The generated TASK packet MUST include a `### MAY edit` section listing
     exactly `Invoke-AiDispatchQueue.ps1`.
   - The generated TASK packet MAY include a `### MAY add new files` section
     only for this dispatch's own `ai_handoffs/ISSUE-*_TASK_*.md`,
     `ai_handoffs/ISSUE-*_EXEC_*.md`, `ai_handoffs/ISSUE-*_CORRECT_*.md`
     packets, matching `.meta.json` sidecars, and its own
     `ai_dispatch_logs/log_*.md` file.

   **Allowed file surface**:
   - EDIT only `Invoke-AiDispatchQueue.ps1`.
   - MAY add this dispatch's own handoff packets, handoff sidecars, and queue
     log as produced by the orchestrator/queue.

   **Files that MUST NOT be touched**:
   - Do not edit `Invoke-AiDispatchAuto.ps1`, `Invoke-AiDispatchLoop.ps1`,
     watchdog/preflight code, JSONL trace emitter schema or path policy,
     scheduler scripts, health scripts, docs, task brief, workflows, schemas,
     Rust source, Cargo files, golden fixtures, status files, existing
     handoff/log artifacts, or sandbox worktrees.
   - Do not add same-phase retry, recovery routes, alerting, aggregation,
     dashboards, queue ordering changes, cap changes, publish behavior
     changes, or any new autonomous selection behavior.

   **Implementation behavior required**:
   - Add idempotent creation for these taxonomy labels in the existing label
     setup path:
     - `ai-dispatch-failure-stall`
     - `ai-dispatch-failure-timeout`
     - `ai-dispatch-failure-blocked`
     - `ai-dispatch-failure-verification`
     - `ai-dispatch-failure-control`
     - `ai-dispatch-failure-publish`
     - `ai-dispatch-failure-unknown`
   - Add a small helper that classifies terminal failed runs from data the
     queue already has after the loop: `$loopExit`, `$loopText`, `$verdict`,
     `$execStatus`, `$publishFailed`, and `$publishHardFailed`.
   - Classification order must avoid misclassifying stalls as generic
     timeouts: publish failure first when `$publishFailed` is true; blocked
     when `$execStatus -eq 'blocked'`; stall when loop text contains the
     Codex watchdog stall wording (`codex exec stalled` or `no log growth`);
     timeout when loop text contains timeout wording; verification when loop
     text contains verification-gate failure wording; control when loop text
     contains control-block or exhausted-control-change wording; otherwise
     unknown.
   - Apply taxonomy labels only to terminal failed issues: cases where
     `$runFailed` is true and `$willRetry` is false. Keep the existing
     `ai-dispatch-failed` label unchanged.
   - Do not apply taxonomy labels to successful issues. Do not make taxonomy
     labels participate in issue selection, cap counting, halt checks,
     retry eligibility, publishing, branch archival, or cleanup.
   - Extend label-finalization verification so terminal failed issues must
     contain the selected taxonomy label(s) in addition to the existing
     expected labels.
   - It is acceptable to include the selected taxonomy labels in the result
     comment for human readability, but the labels themselves are the required
     durable output.

   **Halt conditions**:
   - Halt if the taxonomy cannot be added by editing only
     `Invoke-AiDispatchQueue.ps1` plus this dispatch's own generated
     artifacts.
   - Halt if the implementation would require changing `Invoke-AiDispatchLoop.ps1`
     or the Codex watchdog/preflight code.
   - Halt if the implementation would change retry eligibility,
     `ai-dispatch-failed` halt behavior, auto-publish behavior, queue
     selection, cap counting, branch archival, or JSONL trace schema.
   - Halt if terminal failure labels cannot be added idempotently through the
     existing `gh label create --force` setup style.
   - Halt if PowerShell 5.1 compatibility cannot be preserved.

   **Verbatim review-gate strings** - the autonomous selector MUST copy these
   eight strings, character-for-character, into the filed GitHub issue body.
   No paraphrasing, no substitution, no reflowing. A packet that lacks any one
   of them verbatim is bounced at review:

   ```
   MUST edit only Invoke-AiDispatchQueue.ps1 plus this dispatch's own ai_handoffs and ai_dispatch_logs artifacts
   MUST add label-only terminal failure taxonomy labels without changing ai-dispatch-failed halt behavior
   MUST classify Codex watchdog stalls separately from generic timeouts
   MUST apply taxonomy labels only when runFailed is true and willRetry is false
   MUST NOT change retry eligibility same-phase retry recovery routes queue selection cap counting publish behavior branch policy Auto Loop watchdog preflight JSONL schema Rust Cargo docs or dashboards
   MUST preserve successful issue labels and successful publish behavior unchanged
   MUST extend label finalization verification to require taxonomy labels on terminal failed issues
   MUST run PowerShell parser validation for Invoke-AiDispatchQueue.ps1, git diff --check, and the canonical .ai/dispatch.verify.ps1 gate successfully
   ```

   **Verification required**:
   - PowerShell parser validation for `Invoke-AiDispatchQueue.ps1` reports
     zero errors.
   - `git diff --check` reports no whitespace errors.
   - `.ai/dispatch.verify.ps1` passes.
   - Static inspection or a non-mutating dry run confirms successful runs still
     relabel only with the existing done path and do not add taxonomy labels.
   - Static inspection confirms terminal failed runs add
     `ai-dispatch-failed` plus the selected taxonomy label(s).
   - Static inspection confirms `$willRetry` runs keep the existing retry path
     and do not become terminal solely because taxonomy labels exist.
   - No file outside the allowed surface changes, except this dispatch's own
     handoff/log artifacts.

   **Notes for executor**:
   - This is item 3 of the self-improving automation sequence. It is
     deliberately label-only so later dispatches can tune retry/recovery
     policy from observed failure classes without changing routes yet.
   - Prefer a compact helper and the existing queue-local variables over
     introducing new files, schemas, global state, or issue-query paths.

53. **[DONE 2026-05-26 via PR #193 / commit `62683f5`] Add same-phase retry for read-only plan-gate and control calls.**
   Landed via PR #193. `Invoke-AiDispatchLoop.ps1` now wraps only the
   read-only Claude plan-gate and Codex control model-review calls in a
   bounded same-phase retry helper. Mutation phases and semantic verdicts keep
   their existing flow; retry exhaustion preserves the original failure
   message, including Codex stall/timeout wording, and `RetryCount=0`
   preserves single-attempt behavior for debugging. The original brief is
   preserved below.

   The queue now records terminal failure classes, but safe transient recovery
   should happen before a whole dispatch is marked failed. Add one bounded
   same-phase retry for the two read-only model-review phases:
   Claude plan-gate review and Codex control review. These phases do not edit
   the worktree, so retrying an infrastructure failure in-place is safer than
   restarting the whole dispatch. Do not retry mutation phases yet:
   Codex plan-fill, Claude execute, verification correction, control
   correction, preflight audit, and verification remain unchanged until a
   later snapshot/restore task.

   **Runtime invocation note**: this task is a deliberate named +1 after task
   #52. Current `ai-auto` count is 129. Run as
   `.\Invoke-AiDispatchAuto.ps1 -PublishMode branch -MaxAutonomousTasks 130`
   so the cap accommodates exactly this one dispatch. The scheduler remains
   disabled and must not be re-enabled by this task.

   **Required TASK packet shape**:
   - The generated TASK packet MUST include a `### MAY edit` section listing
     exactly `Invoke-AiDispatchLoop.ps1`.
   - The generated TASK packet MAY include a `### MAY add new files` section
     only for this dispatch's own `ai_handoffs/ISSUE-*_TASK_*.md`,
     `ai_handoffs/ISSUE-*_EXEC_*.md`, `ai_handoffs/ISSUE-*_CORRECT_*.md`
     packets, matching `.meta.json` sidecars, and its own
     `ai_dispatch_logs/log_*.md` file.

   **Allowed file surface**:
   - EDIT only `Invoke-AiDispatchLoop.ps1`.
   - MAY add this dispatch's own handoff packets, handoff sidecars, and queue
     log as produced by the orchestrator/queue.

   **Files that MUST NOT be touched**:
   - Do not edit `Invoke-AiDispatchAuto.ps1`, `Invoke-AiDispatchQueue.ps1`,
     JSONL trace emitters, failure taxonomy labels, scheduler scripts, health
     scripts, docs, task brief, workflows, schemas, Rust source, Cargo files,
     golden fixtures, status files, existing handoff/log artifacts, or
     sandbox worktrees.
   - Do not add retries for Codex plan-fill, Claude execute, preflight audit,
     verification, correction-packet generation, correction execution, queue
     publish, queue relabel, issue creation, or GitHub API calls.

   **Implementation behavior required**:
   - Add a small, explicit same-phase retry mechanism for read-only model
     phases only. The implementation may be a parameterized helper or bounded
     retry support in the existing model-invocation helpers, but it must be
     enabled only at the Claude plan-gate call site and the Codex control call
     site.
   - Default behavior should retry each of those read-only phases at most once
     after an infrastructure/model-call failure. Provide an internal
     `0`-retry path or parameter so the previous single-attempt behavior is
     still available for debugging.
   - Retry only transport/infrastructure/model-call failures such as timeout,
     stall, non-zero CLI exit, missing Claude output, Claude envelope error,
     missing required plan-gate marker, or Codex invocation failure before a
     valid control JSON is available.
   - Do not retry semantic verdicts. `GATE_VERDICT: needs_changes`,
     `GATE_VERDICT: block`, control `needs_changes`, and control `block` must
     retain their existing flow.
   - Do not retry any phase that can mutate the worktree or write a new
     handoff packet. In particular, `Invoke-PlanFill`, `Invoke-ClaudeExecute`,
     `Invoke-CodexPreflightAudit`, `Invoke-Verification`, and
     `Invoke-CorrectionPacket` must keep current retry behavior.
   - Preserve the existing Codex stall watchdog behavior and messages for
     non-retried Codex calls. If the Codex control retry exhausts, the final
     failure should still contain the existing stall/timeout wording so the
     queue taxonomy from task #52 can classify it.
   - Emit clear loop stdout when a same-phase retry is attempted and whether
     it succeeds or exhausts, so the queue log captures retry success/failure
     without adding a new schema.

   **Halt conditions**:
   - Halt if the same-phase retry cannot be implemented by editing only
     `Invoke-AiDispatchLoop.ps1` plus this dispatch's own generated artifacts.
   - Halt if the implementation would require changing queue labels, queue
     retry policy, queue publish behavior, JSONL trace schema, Auto behavior,
     Rust/Cargo files, or any GitHub issue workflow.
   - Halt if a mutation phase would need to be retried to complete this task.
   - Halt if semantic verdicts would be retried or reinterpreted as
     infrastructure failures.
   - Halt if PowerShell 5.1 compatibility cannot be preserved.

   **Verbatim review-gate strings** - the autonomous selector MUST copy these
   eight strings, character-for-character, into the filed GitHub issue body.
   No paraphrasing, no substitution, no reflowing. A packet that lacks any one
   of them verbatim is bounced at review:

   ```
   MUST edit only Invoke-AiDispatchLoop.ps1 plus this dispatch's own ai_handoffs and ai_dispatch_logs artifacts
   MUST add same-phase retry only for Claude plan-gate and Codex control read-only phases
   MUST retry each eligible read-only phase at most once after infrastructure or model-call failure
   MUST NOT retry semantic needs_changes block pass or approve verdicts
   MUST NOT retry Codex plan-fill Claude execute preflight verification correction generation correction execution queue publish queue relabel issue creation or GitHub API calls
   MUST preserve Codex stall watchdog behavior and final stall timeout wording when retries exhaust
   MUST emit loop output for same-phase retry attempts successes and exhaustion without adding schemas or JSONL trace schema changes
   MUST run PowerShell parser validation for Invoke-AiDispatchLoop.ps1, git diff --check, and the canonical .ai/dispatch.verify.ps1 gate successfully
   ```

   **Verification required**:
   - PowerShell parser validation for `Invoke-AiDispatchLoop.ps1` reports zero
     errors.
   - `git diff --check` reports no whitespace errors.
   - `.ai/dispatch.verify.ps1` passes.
   - Static inspection confirms only the plan-gate and control read-only call
     sites enable same-phase retry.
   - Static inspection confirms execute/correction/verification/preflight and
     queue-side paths retain their existing retry behavior.
   - Static inspection confirms semantic verdicts still flow through the
     existing plan revision and correction logic rather than being retried.
   - No file outside the allowed surface changes, except this dispatch's own
     handoff/log artifacts.

   **Notes for executor**:
   - This is item 4 of the self-improving automation sequence. It deliberately
     limits retries to the two phases where re-running is read-only and
     side-effect safe.
   - Keep this a retry-policy task only. Aggregation, alerts, recovery routes,
     execute/correction retries, and any UI/dashboard work are later tasks.

54. **[DONE 2026-05-26 via PR #195 / commit `93bbcad`] Add JSONL dispatch trend aggregator and local alerts CLI.**
   Landed via PR #195. Added `Get-AiDispatchTrends.ps1`, a read-only local
   CLI that consumes existing `.ai/dispatch-trace/*.jsonl` files and reports
   Summary, Phase Durations, and Alerts blocks with average/p50/p95/max phase
   metrics. It handles no-data and malformed-line cases, supports threshold
   alerts plus `-FailOnAlert`, and does not alter emitters, trace schema, or
   dispatch behavior. The original brief is preserved below.

   The trace emitter now writes opt-in JSONL files under
   `.ai/dispatch-trace/`, and several automation upgrades have produced real
   timing data. Add a read-only local CLI that aggregates those JSONL events
   into phase-duration metrics and threshold alerts. This task must not add a
   UI dashboard, must not change emitters or trace schema, and must not change
   any dispatch behavior. It is an operator/reporting tool only.

   **Runtime invocation note**: this task is a deliberate named +1 after task
   #53. Current `ai-auto` count is 130. Run as
   `.\Invoke-AiDispatchAuto.ps1 -PublishMode branch -MaxAutonomousTasks 131`
   so the cap accommodates exactly this one dispatch. The scheduler remains
   disabled and must not be re-enabled by this task.

   **Required TASK packet shape**:
   - The generated TASK packet MUST include a `### MAY edit` section listing
     exactly `Get-AiDispatchTrends.ps1`.
   - The generated TASK packet MAY include a `### MAY add new files` section
     listing exactly `Get-AiDispatchTrends.ps1` plus this dispatch's own
     `ai_handoffs/ISSUE-*_TASK_*.md`,
     `ai_handoffs/ISSUE-*_EXEC_*.md`,
     `ai_handoffs/ISSUE-*_CORRECT_*.md` packets, matching `.meta.json`
     sidecars, and its own `ai_dispatch_logs/log_*.md` file.

   **Allowed file surface**:
   - ADD or EDIT only `Get-AiDispatchTrends.ps1`.
   - MAY add this dispatch's own handoff packets, handoff sidecars, and queue
     log as produced by the orchestrator/queue.

   **Files that MUST NOT be touched**:
   - Do not edit `Invoke-AiDispatchAuto.ps1`, `Invoke-AiDispatchQueue.ps1`,
     `Invoke-AiDispatchLoop.ps1`, `Get-AiDispatchHealth.ps1`, trace emitters,
     failure taxonomy labels, scheduler scripts, docs, task brief, workflows,
     schemas, Rust source, Cargo files, golden fixtures, status files,
     existing handoff/log artifacts, or sandbox worktrees.
   - Do not add alerting services, scheduled tasks, CI jobs, dashboards,
     network calls, GitHub issue/label mutations, recovery routes, retry
     policy changes, or any dispatch behavior change.

   **Implementation behavior required**:
   - Create `Get-AiDispatchTrends.ps1` as a PowerShell 5.1-compatible,
     read-only CLI.
   - Default input is `.ai/dispatch-trace/*.jsonl` under `-RepoRoot`, with
     parameters for `-RepoRoot`, `-TraceDir`, and `-SinceHours`. If no trace
     files exist, print a clear no-data message and exit 0.
   - Parse JSONL line-by-line. Invalid JSON lines must be counted and reported
     but must not abort the whole report unless `-FailOnAlert` is set and the
     invalid-line threshold is exceeded.
   - Aggregate phase durations by pairing start/done style messages within
     each trace file and process: at minimum report samples, average, p50, p95,
     max for these spans when present:
     - auto tick total: `auto.tick: start` to `auto.tick: end`
     - empty/cap path gap: `auto.queue-check: primary done` to
       `auto.cap-check: start`
     - auto queue invocation: `auto.tick: queue-invocation start` to
       `auto.tick: queue-invocation done`
     - queue loop: `queue.loop: start` to `queue.loop: done`
     - queue publish block: `queue.publish: block-entry` to
       `queue.publish: block-exit`
     - queue GitHub finalize: `queue.github: comment start` to
       `queue.github: relabel done`
   - Provide threshold parameters with conservative defaults and emit `ALERT`
     lines when thresholds are exceeded. Include at least:
     `-WarnEmptyCapGapSec`, `-WarnQueueLoopSec`, `-WarnPublishSec`,
     `-WarnGithubFinalizeSec`, and `-WarnInvalidJsonLines`.
   - Add `-FailOnAlert`. Without it, alerts are informational and the script
     exits 0. With it, any alert exits non-zero after printing the report.
   - Keep the output plain text and scriptable: include a Summary block, a
     Phase Durations block, and an Alerts block. Do not require external
     modules.

   **Halt conditions**:
   - Halt if the aggregator cannot be implemented by adding/editing only
     `Get-AiDispatchTrends.ps1` plus this dispatch's own generated artifacts.
   - Halt if the implementation would require changing trace emitters,
     existing JSONL schema, Auto/Queue/Loop behavior, GitHub labels/issues,
     scheduled tasks, CI, Rust/Cargo files, or docs.
   - Halt if PowerShell 5.1 compatibility cannot be preserved.
   - Halt if no safe read-only behavior is possible when trace files are
     missing or partially malformed.

   **Verbatim review-gate strings** - the autonomous selector MUST copy these
   eight strings, character-for-character, into the filed GitHub issue body.
   No paraphrasing, no substitution, no reflowing. A packet that lacks any one
   of them verbatim is bounced at review:

   ```
   MUST add or edit only Get-AiDispatchTrends.ps1 plus this dispatch's own ai_handoffs and ai_dispatch_logs artifacts
   MUST read existing .ai/dispatch-trace/*.jsonl events without changing trace emitters or JSONL schema
   MUST report phase duration samples average p50 p95 and max for auto tick empty-cap gap queue invocation queue loop publish block and GitHub finalize spans when present
   MUST count malformed JSONL lines without aborting the report unless FailOnAlert is set and the invalid-line alert threshold is exceeded
   MUST emit plain-text Summary Phase Durations and Alerts blocks
   MUST support FailOnAlert while leaving default informational alerts exit code 0
   MUST NOT add dashboards scheduled tasks CI jobs network calls GitHub mutations recovery routes retry policy changes Auto Queue Loop Rust Cargo docs or schema edits
   MUST run PowerShell parser validation for Get-AiDispatchTrends.ps1, git diff --check, and a no-data or synthetic-trace smoke test successfully
   ```

   **Verification required**:
   - PowerShell parser validation for `Get-AiDispatchTrends.ps1` reports zero
     errors.
   - `git diff --check` reports no whitespace errors.
   - A no-data smoke test against an empty temporary trace directory exits 0
     and prints a no-data message.
   - A synthetic-trace smoke test using a temporary JSONL directory reports at
     least one phase duration and one alert when thresholds are set low.
   - If `-FailOnAlert` is used with the low-threshold synthetic trace, the
     script exits non-zero after printing the report.
   - No file outside the allowed surface changes, except this dispatch's own
     handoff/log artifacts.

   **Notes for executor**:
   - This is item 5 of the self-improving automation sequence. It consumes
     the JSONL stream but must not alter producers.
   - Keep this a CLI reporting tool. Failure taxonomy recovery routes and
     execute/correction retry work are later tasks.

55. **[DONE 2026-05-26 via PR #197 / commit `b8ae199`] Add one-shot transient recovery route for taxonomy-labelled autonomous failures.**
   Landed via PR #197. Auto now has a bounded one-shot recovery route for a
   single open autonomous terminal failure labelled `ai-dispatch-failure-stall`
   or `ai-dispatch-failure-timeout`, guarded by
   `ai-dispatch-recovered-transient`. The local `.ai/dispatch.auto-halt`
   sentinel remains first, non-transient or ambiguous failures still halt, and
   stale post-recovery queue visibility either drains the recovered issue or
   exits before new task selection. The original brief is preserved below.

   The queue now applies terminal failure taxonomy labels, and the autonomous
   driver still halts on every `ai-dispatch-failed` issue. Add the first
   conservative recovery route: an optically visible, one-shot Auto-side
   requeue for a single open autonomous issue whose terminal failure taxonomy
   is transient (`ai-dispatch-failure-stall` or
   `ai-dispatch-failure-timeout`). This is a recovery-policy task only. It
   must not change queue failure classification, queue retry policy, the
   dispatch loop, or any Rust/project source.

   **Runtime invocation note**: this task is a deliberate named +1 after task
   #54. Current `ai-auto` count is 131. Run as
   `.\Invoke-AiDispatchAuto.ps1 -PublishMode branch -MaxAutonomousTasks 132`
   so the cap accommodates exactly this one dispatch. The scheduler remains
   disabled and must not be re-enabled by this task.

   **Required TASK packet shape**:
   - The generated TASK packet MUST include a `### MAY edit` section listing
     exactly `Invoke-AiDispatchAuto.ps1`.
   - The generated TASK packet MAY include a `### MAY add new files` section
     only for this dispatch's own `ai_handoffs/ISSUE-*_TASK_*.md`,
     `ai_handoffs/ISSUE-*_EXEC_*.md`,
     `ai_handoffs/ISSUE-*_CORRECT_*.md` packets, matching `.meta.json`
     sidecars, and its own `ai_dispatch_logs/log_*.md` file.

   **Allowed file surface**:
   - EDIT only `Invoke-AiDispatchAuto.ps1`.
   - MAY add this dispatch's own handoff packets, handoff sidecars, and queue
     log as produced by the orchestrator/queue.

   **Files that MUST NOT be touched**:
   - Do not edit `Invoke-AiDispatchQueue.ps1`, `Invoke-AiDispatchLoop.ps1`,
     `Get-AiDispatchTrends.ps1`, `Get-AiDispatchHealth.ps1`, scheduler
     scripts, trace emitters, failure taxonomy classifiers, docs, task brief,
     workflows, schemas, Rust source, Cargo files, golden fixtures, status
     files, existing handoff/log artifacts, or sandbox worktrees.
   - Do not add new retry logic to the queue or loop, do not change publish
     behavior, and do not change the meaning or creation point of any existing
     taxonomy label.

   **Implementation behavior required**:
   - Preserve the `.ai/dispatch.auto-halt` sentinel behavior exactly. A local
     halt sentinel must still stop before any GitHub recovery logic.
   - In the existing Auto halt check for `ai-auto` + `ai-dispatch-failed`,
     fetch failed issues with at least `number`, `title`, `state`, and
     `labels`.
   - If zero failed autonomous issues exist, continue the current normal Auto
     flow unchanged.
   - If more than one failed autonomous issue exists, halt with a clear message
     and do not recover any of them.
   - If exactly one failed autonomous issue exists, recover it only when all of
     these are true:
     - issue state is open;
     - it has `ai-dispatch-failure-stall` or `ai-dispatch-failure-timeout`;
     - it does not have the recovery marker label
       `ai-dispatch-recovered-transient`;
     - it does not have any non-transient failure taxonomy label such as
       `ai-dispatch-failure-blocked`,
       `ai-dispatch-failure-verification`,
       `ai-dispatch-failure-control`, `ai-dispatch-failure-publish`, or
       `ai-dispatch-failure-unknown`.
   - Recovery must be visible and bounded: idempotently ensure the
     `ai-dispatch-recovered-transient` label exists, then remove
     `ai-dispatch-failed`, remove `ai-dispatch-done` if present, and add
     `ai-dispatch`, `ai-dispatch-retry`, and
     `ai-dispatch-recovered-transient`. Leave the transient taxonomy label in
     place for auditability.
   - After a successful recovery mutation, continue the existing Auto flow so
     the queue can pick up the requeued issue before any new task selection.
   - In `-DryRun`, print the same recovery decision but do not mutate GitHub
     labels. A dry run must not clear the halt label or requeue an issue.
   - If the single failed issue is closed, already recovered, non-transient, or
     mixed with a non-transient taxonomy label, preserve halt behavior and
     print the reason.
   - If the GitHub label mutation fails, preserve halt behavior and do not
     proceed to task selection.

   **Halt conditions**:
   - Halt if the recovery route cannot be implemented by editing only
     `Invoke-AiDispatchAuto.ps1` plus this dispatch's own generated artifacts.
   - Halt if implementing recovery would require changing queue/loop retry
     policy, queue publish behavior, failure taxonomy classification, JSONL
     trace schema, scheduled tasks, CI, Rust/Cargo files, docs, or task brief.
   - Halt if recovery cannot be made one-shot using
     `ai-dispatch-recovered-transient`.
   - Halt if non-transient failure classes would be auto-recovered.
   - Halt if `-DryRun` would need to mutate GitHub labels.
   - Halt if PowerShell 5.1 compatibility cannot be preserved.

   **Verbatim review-gate strings** - the autonomous selector MUST copy these
   eight strings, character-for-character, into the filed GitHub issue body.
   No paraphrasing, no substitution, no reflowing. A packet that lacks any one
   of them verbatim is bounced at review:

   ```
   MUST edit only Invoke-AiDispatchAuto.ps1 plus this dispatch's own ai_handoffs and ai_dispatch_logs artifacts
   MUST preserve the .ai/dispatch.auto-halt sentinel as the first halt check before any GitHub recovery logic
   MUST recover only one open ai-auto ai-dispatch-failed issue when it has ai-dispatch-failure-stall or ai-dispatch-failure-timeout and lacks ai-dispatch-recovered-transient
   MUST NOT recover closed issues multiple simultaneous failed issues already-recovered issues or issues with blocked verification control publish unknown or mixed non-transient taxonomy labels
   MUST requeue recovery by removing ai-dispatch-failed removing ai-dispatch-done if present and adding ai-dispatch ai-dispatch-retry and ai-dispatch-recovered-transient while keeping the transient taxonomy label
   MUST make DryRun print the recovery decision without mutating GitHub labels
   MUST NOT change Invoke-AiDispatchQueue.ps1 Invoke-AiDispatchLoop.ps1 taxonomy classification queue retry policy publish behavior JSONL schema scheduler CI Rust Cargo docs or task brief
   MUST run PowerShell parser validation for Invoke-AiDispatchAuto.ps1, git diff --check, and the canonical .ai/dispatch.verify.ps1 gate successfully
   ```

   **Verification required**:
   - PowerShell parser validation for `Invoke-AiDispatchAuto.ps1` reports zero
     errors.
   - `git diff --check` reports no whitespace errors.
   - `.ai/dispatch.verify.ps1` passes.
   - Static inspection confirms `.ai/dispatch.auto-halt` remains the first
     halt path.
   - Static inspection confirms only stall/timeout taxonomy labels can trigger
     recovery, and only when exactly one open failed issue exists.
   - Static inspection confirms blocked/verification/control/publish/unknown,
     closed, mixed, already-recovered, and multiple-failure cases still halt.
   - Static inspection confirms `-DryRun` performs no label mutations.
   - No file outside the allowed surface changes, except this dispatch's own
     handoff/log artifacts.

   **Notes for executor**:
   - This is item 6 of the self-improving automation sequence. It consumes the
     failure taxonomy from task #52 and creates the first bounded recovery
     route.
   - Keep this Auto-side and one-shot. Execute/correction snapshot retry work
     is a later task.

56. **[DONE 2026-05-26 via PR #199 / commit `94c1254`] Add snapshot-backed same-phase retry for execute and correction mutation phases.**
   Landed via PR #199. `Invoke-AiDispatchLoop.ps1` now has a bounded
   `MutationRetryCount` path for Claude execution and Codex correction-packet
   generation only, backed by phase-entry snapshot/restore for tracked changes
   plus untracked non-ignored files. Semantic statuses and verdicts remain
   outside the retry envelope. The queue scope-guard publish hiccup was a
   TASK-packet token-format issue and was repaired in the generated artifact;
   the implementation and control verdict were unchanged. The original brief
   is preserved below.

   The loop now retries read-only model review phases, but mutation phases
   still rely on the outer queue retry after any infrastructure failure. Add
   the final self-improving automation piece: same-phase retry for Claude
   execution and Codex correction-packet generation only, backed by an
   explicit worktree snapshot/restore guard so a failed partial mutation
   cannot smear into the retry attempt. This task must not retry semantic
   verdicts, verification failures, or queue/publish/GitHub operations.

   **Runtime invocation note**: this task is a deliberate named +1 after task
   #55. Current `ai-auto` count is 132. Run as
   `.\Invoke-AiDispatchAuto.ps1 -PublishMode branch -MaxAutonomousTasks 133`
   so the cap accommodates exactly this one dispatch. The scheduler remains
   disabled and must not be re-enabled by this task.

   **Required TASK packet shape**:
   - The generated TASK packet MUST include a `### MAY edit` section listing
     exactly `Invoke-AiDispatchLoop.ps1`.
   - The generated TASK packet MAY include a `### MAY add new files` section
     only for this dispatch's own `ai_handoffs/ISSUE-*_TASK_*.md`,
     `ai_handoffs/ISSUE-*_EXEC_*.md`,
     `ai_handoffs/ISSUE-*_CORRECT_*.md` packets, matching `.meta.json`
     sidecars, and its own `ai_dispatch_logs/log_*.md` file.

   **Allowed file surface**:
   - EDIT only `Invoke-AiDispatchLoop.ps1`.
   - MAY add this dispatch's own handoff packets, handoff sidecars, and queue
     log as produced by the orchestrator/queue.

   **Files that MUST NOT be touched**:
   - Do not edit `Invoke-AiDispatchAuto.ps1`, `Invoke-AiDispatchQueue.ps1`,
     `Get-AiDispatchTrends.ps1`, `Get-AiDispatchHealth.ps1`, scheduler
     scripts, trace emitters, failure taxonomy labels, docs, task brief,
     workflows, schemas, Rust source, Cargo files, golden fixtures, status
     files, existing handoff/log artifacts, or sandbox worktrees.
   - Do not add queue retries, Auto recovery routes, publish retries, GitHub
     API retries, verification retries, preflight retries, JSONL schema
     changes, dashboards, scheduled tasks, or new agents.

   **Implementation behavior required**:
   - Add a mutation-phase retry wrapper distinct from the existing
     read-only `Invoke-WithSamePhaseRetry` helper. It may share small internal
     helpers, but its restore behavior must be explicit and mutation-aware.
   - Add a bounded retry count for mutation phases, defaulting to one retry
     and allowing `0` to preserve previous single-attempt behavior for
     debugging.
   - Apply mutation retry only to:
     - `Invoke-ClaudeExecute` for TASK execution and CORRECTION execution;
     - `Invoke-CorrectionPacket` for Codex-authored correction packets after
       verification failure or control `needs_changes`.
   - Do not apply mutation retry to Codex plan-fill, Claude plan-gate,
     Codex preflight audit, verification, Codex control, queue runner,
     Auto driver, publish, relabel/comment/close, issue creation, branch
     operations, or any GitHub API call.
   - Retry only infrastructure/model-call/tooling failures that surface as a
     thrown `Fail` path before a valid semantic result is available, such as
     timeout, stall, non-zero CLI exit, missing output, malformed/missing
     required markers, missing generated packet, failed packet finalize, or
     transient planner tool failure while writing a correction packet.
   - Do not retry semantic results. `EXEC_STATUS: blocked`,
     `EXEC_STATUS: failed`, verification failure, control `needs_changes`,
     control `block`, plan-gate `needs_changes`, and plan-gate `block` must
     retain their existing flow.
   - Before each eligible mutation phase, capture a restore point for the
     current worktree state, including tracked changes and untracked
     non-ignored files that are already present at phase entry. If a failed
     attempt is retried, restore exactly that phase-entry state before the
     next attempt.
   - The snapshot/restore path must be guarded: resolve and verify the repo
     root before any restore, reject staged changes unless the implementation
     can preserve them exactly, and never operate outside the current repo
     root. If a safe restore cannot be established, halt rather than retry.
   - Generated files from a failed attempt, including partial EXEC/CORRECT
     packets and sidecars, must not survive into the retry unless they were
     part of the phase-entry snapshot.
   - On successful attempt, discard the temporary restore point without
     altering the successful worktree.
   - Emit clear loop stdout when a mutation retry attempt fails, when restore
     begins/ends, when retry succeeds, and when retries exhaust, so queue logs
     capture the recovery path without changing JSONL schema.
   - Preserve the final failure wording as much as practical when retries
     exhaust so the queue failure taxonomy can still classify stall/timeout
     failures.

   **Halt conditions**:
   - Halt if the mutation retry cannot be implemented by editing only
     `Invoke-AiDispatchLoop.ps1` plus this dispatch's own generated artifacts.
   - Halt if safe phase-entry snapshot/restore cannot be implemented for
     tracked changes plus untracked non-ignored files.
   - Halt if the restore logic would need to operate outside the current repo
     root or cannot guard against staged-change loss.
   - Halt if semantic statuses or verdicts would be retried.
   - Halt if the implementation would require changing Auto, Queue, publish,
     GitHub issue/label behavior, failure taxonomy labels, JSONL schema,
     verification gates, Rust/Cargo files, docs, CI, scheduler scripts, or the
     task brief.
   - Halt if PowerShell 5.1 compatibility cannot be preserved.

   **Verbatim review-gate strings** - the autonomous selector MUST copy these
   eight strings, character-for-character, into the filed GitHub issue body.
   No paraphrasing, no substitution, no reflowing. A packet that lacks any one
   of them verbatim is bounced at review:

   ```
   MUST edit only Invoke-AiDispatchLoop.ps1 plus this dispatch's own ai_handoffs and ai_dispatch_logs artifacts
   MUST add snapshot-backed same-phase retry only for Invoke-ClaudeExecute and Invoke-CorrectionPacket mutation phases
   MUST preserve a zero-retry path that matches the previous single-attempt mutation behavior
   MUST restore the exact phase-entry worktree state before any mutation retry attempt including tracked changes and untracked non-ignored files
   MUST NOT retry semantic EXEC_STATUS blocked failed verification failure control needs_changes control block plan-gate needs_changes or plan-gate block outcomes
   MUST NOT change Auto Queue publish GitHub issue or label behavior failure taxonomy JSONL schema verification gates Rust Cargo docs CI scheduler or task brief
   MUST emit loop output for mutation retry failure restore retry success and exhaustion without adding schemas or JSONL trace schema changes
   MUST run PowerShell parser validation for Invoke-AiDispatchLoop.ps1, git diff --check, a focused restore harness, and the canonical .ai/dispatch.verify.ps1 gate successfully
   ```

   **Verification required**:
   - PowerShell parser validation for `Invoke-AiDispatchLoop.ps1` reports zero
     errors.
   - `git diff --check` reports no whitespace errors.
   - `.ai/dispatch.verify.ps1` passes.
   - A focused non-mutating or temporary-repo restore harness proves that a
     failed mutation attempt can create/modify/delete tracked and untracked
     non-ignored files, then the retry restore returns the worktree to the
     exact phase-entry state before the second attempt. The harness must not
     mutate live GitHub issues or labels, and any temporary files/repos must
     be removed before commit.
   - Static inspection confirms only `Invoke-ClaudeExecute` and
     `Invoke-CorrectionPacket` call sites enable mutation retry.
   - Static inspection confirms plan-fill, plan-gate, preflight, verification,
     control, queue, Auto, publish, relabel/comment/close, issue creation, and
     branch operations retain their previous retry behavior.
   - Static inspection confirms semantic statuses/verdicts are not retried.
   - No file outside the allowed surface changes, except this dispatch's own
     handoff/log artifacts.

   **Notes for executor**:
   - This is item 7 of the self-improving automation sequence. It is the only
     item in the sequence that may retry mutation phases, so the restore guard
     is the main correctness requirement.
   - Keep this loop-local. Do not move retry policy into the queue, do not add
     new recovery labels, and do not change autonomous selection behavior.

57. **[DONE 2026-05-26 via PR #201 / commit `4f85bdd`] Read-only audit: post-sequence automation safety and throughput validation.**
   Landed via PR #201. The audit found no unsafe publish, semantic-retry,
   non-transient recovery, or user-work-loss path in the current automation
   stack. It identified the next smallest safe follow-up as a planner-prompt
   hardening task: require backtick-quoted path tokens in `### MAY edit` and
   `### MAY add new files` so the queue scope guard's positive-token parser
   does not fail closed on otherwise valid control-passed dispatches. The
   original brief is preserved below.

   The seven self-improving automation dispatches have landed:
   watchdog, opt-in preflight audit, JSONL trace persistence, empty-tick
   speedup, failure taxonomy, read-only same-phase retry, transient recovery,
   and snapshot-backed mutation retry. Before wiring the next implementation
   change, run a read-only audit over the current automation state and produce
   an EXEC packet with concrete findings and exactly one smallest safe
   follow-up, or `NEEDS_HUMAN` if the next step requires arbitration.

   **Runtime invocation note**: current `ai-auto` count is 133. Run as
   `.\Invoke-AiDispatchAuto.ps1 -PublishMode branch -MaxAutonomousTasks 134 -TraceTiming`
   so the cap accommodates exactly this one audit and the trace stream records
   the new tick. The scheduler remains disabled and must not be re-enabled by
   this task.

   **Required TASK packet shape**:
   - The generated TASK packet MUST make this an audit-only dispatch.
   - The generated TASK packet MUST NOT list any production source, test,
     docs, Cargo, workflow, schema, task brief, or automation script path in
     `### MAY edit`.
   - The generated TASK packet MAY add only this dispatch's own
     `ai_handoffs/ISSUE-*_TASK_*.md`,
     `ai_handoffs/ISSUE-*_EXEC_*.md`,
     `ai_handoffs/ISSUE-*_CORRECT_*.md` packets, matching `.meta.json`
     sidecars, and its own `ai_dispatch_logs/log_*.md` queue log.
   - Because the queue scope guard currently requires at least one positive
     allowed-path token in a `### MAY edit` or `### MAY add new files` section,
     the generated TASK packet MUST include this optional ignored run-dir
     scratch token in `### MAY add new files`:
     `.ai/dispatch-ISSUE-*/automation-audit-scratch.md`. The executor does
     not need to create it; it exists only to keep read-only audit tasks
     compatible with the current fail-closed guard.

   **Allowed file surface**:
   - EDIT no tracked files.
   - MAY add this dispatch's own handoff packets, handoff sidecars, and queue
     log as produced by the orchestrator/queue.
   - MAY create the optional ignored run-dir scratch file
     `.ai/dispatch-ISSUE-*/automation-audit-scratch.md`, but only if useful
     for local notes. It must not be staged or published.

   **Files that MUST NOT be touched**:
   - Do not edit `Invoke-AiDispatchAuto.ps1`, `Invoke-AiDispatchQueue.ps1`,
     `Invoke-AiDispatchLoop.ps1`, `Get-AiDispatchTrends.ps1`,
     `Get-AiDispatchHealth.ps1`, scheduler scripts, trace emitters, failure
     taxonomy labels, recovery logic, scope guard logic, docs, task brief,
     workflows, schemas, Rust source, Cargo files, tests, fixtures, status
     files, existing handoff/log artifacts, or sandbox worktrees.
   - Do not add code, tests, dashboards, new agents, scheduled tasks, GitHub
     labels, GitHub comments outside the queue's normal bookkeeping, or new
     automation behavior.

   **Audit questions required**:
   - Q1. Feature inventory: confirm from current code whether each recently
     landed automation feature is present, scoped to its intended files, and
     still opt-in or bounded where specified: Codex stall watchdog, opt-in
     preflight audit, JSONL trace persistence, empty-tick speedup, failure
     taxonomy labels, read-only same-phase retry, one-shot transient recovery,
     and snapshot-backed mutation retry.
   - Q2. Safety invariants: inspect the interaction between queue scope guard,
     retry paths, transient recovery, branch/publish flow, and snapshot
     restore. Identify any path where automation could stage or publish work
     outside the active TASK surface, retry a semantic failure, auto-recover a
     non-transient failure, or lose user work. If none are found, say so
     explicitly.
   - Q3. Throughput and trace evidence: use current `.ai/dispatch-trace/*.jsonl`
     plus recent dispatch artifacts for issues #182 through #198 to summarize
     observed wall-clock, queue-loop, empty-cap, GitHub finalize, correction,
     stall, and timeout behavior. Name the current bottleneck based on data,
     not intuition.
   - Q4. Activation gaps: verify whether `-EnablePreflightAudit` is currently
     wired through Auto and Queue into Loop. Verify whether `-TraceTiming` is
     wired through Auto into Queue. Name any other landed-but-not-activated
     automation capability discovered from current code.
   - Q5. Smallest safe follow-up: name exactly one smallest implementation
     dispatch only if it is justified by Q1-Q4. Include title, allowed files,
     must-not-touch surfaces, verification gates, and halt conditions. If the
     safest next step needs human arbitration, end `NEEDS_HUMAN` and state the
     decision required. Do not propose a broad rewrite, dashboard, new agent,
     product task, or multi-item bundle as Q5.

   **Halt conditions**:
   - Halt if answering the audit requires any tracked file edit outside this
     dispatch's own generated artifacts.
   - Halt if the current code or trace data is insufficient to answer Q1-Q4
     without inventing facts; report the missing evidence.
   - Halt if Q5 cannot name exactly one smallest safe follow-up from current
     evidence.
   - Halt if the executor would need to mutate GitHub labels or comments
     outside normal queue bookkeeping.

   **Verbatim review-gate strings** - the autonomous selector MUST copy these
   eight strings, character-for-character, into the filed GitHub issue body.
   No paraphrasing, no substitution, no reflowing. A packet that lacks any one
   of them verbatim is bounced at review:

   ```
   MUST perform read-only automation audit only and MUST NOT edit source tests docs Cargo workflows schemas task brief or automation scripts
   MUST inspect current implementations of watchdog preflight scope guard JSONL trace empty-tick speedup taxonomy read-only retry transient recovery and mutation retry
   MUST answer whether EnablePreflightAudit is currently wired through Auto and Queue to Loop
   MUST use current JSONL traces and recent dispatch artifacts to summarize bottlenecks correction rounds stalls and timeouts
   MUST identify exactly one smallest safe follow-up with allowed files verification gates and halt conditions or return NEEDS_HUMAN
   MUST NOT propose broad rewrites dashboards new agents product work or multi-item bundles as the immediate follow-up
   MUST leave only this dispatch's own handoff/log artifacts plus optional ignored .ai dispatch scratch
   MUST run git diff --check and report git status showing no tracked source/test/doc/Cargo/script changes
   ```

   **Verification required**:
   - `git diff --check` reports no whitespace errors.
   - `git status --short --untracked-files=all` shows no tracked source,
     test, docs, Cargo, workflow, schema, task brief, or automation script
     changes.
   - The EXEC packet answers Q1-Q5 explicitly and names the exact evidence
     consulted for each answer.
   - Static inspection confirms the audit did not edit production files.
   - If Q5 names a follow-up task, it includes exact allowed files,
     must-not-touch surfaces, verification gates, and halt conditions.

58. **[DONE 2026-05-26 via PR #203 / commit `532de34`] Harden planner prompt path-token grammar for queue scope guard compatibility.**
   Landed via PR #203. `Invoke-AiDispatchLoop.ps1` now tells the Codex
   planner that every path or glob token in `### MAY edit` and
   `### MAY add new files` must be Markdown-backtick quoted, with bare-bulleted
   paths explicitly invalid for the queue scope guard. The original brief is
   preserved below.

   Implement the single follow-up recommended by ISSUE-200: make the Codex
   planner prompt require backtick-quoted path tokens in `### MAY edit` and
   `### MAY add new files`. This prevents the queue scope guard from failing
   closed on a control-passed dispatch whose TASK packet lists bare paths, for
   example `- Invoke-AiDispatchLoop.ps1`, instead of the required
   backtick-quoted form shown below.

   **Runtime invocation note**: current `ai-auto` count is 134. Run as
   `.\Invoke-AiDispatchAuto.ps1 -PublishMode branch -MaxAutonomousTasks 135 -TraceTiming`
   so the cap accommodates exactly this one dispatch and the trace stream
   records the tick. The scheduler remains disabled and must not be re-enabled
   by this task.

   **Required TASK packet shape**:
   - The generated TASK packet MUST include a `### MAY edit` section listing
     exactly `Invoke-AiDispatchLoop.ps1`.
   - The generated TASK packet MAY include a `### MAY add new files` section
     only for this dispatch's own `ai_handoffs/ISSUE-*_TASK_*.md`,
     `ai_handoffs/ISSUE-*_EXEC_*.md`,
     `ai_handoffs/ISSUE-*_CORRECT_*.md` packets, matching `.meta.json`
     sidecars, and its own `ai_dispatch_logs/log_*.md` queue log.
   - The generated TASK packet MUST state that the implementation is scoped to
     the `Invoke-PlanFill` planner prompt Rules list only.

   **Allowed file surface**:
   - EDIT only `Invoke-AiDispatchLoop.ps1`.
   - MAY add this dispatch's own handoff packets, handoff sidecars, and queue
     log as produced by the orchestrator/queue.

   **Files that MUST NOT be touched**:
   - Do not edit `Invoke-AiDispatchAuto.ps1`, `Invoke-AiDispatchQueue.ps1`,
     `Get-AiDispatchTrends.ps1`, `Get-AiDispatchHealth.ps1`, scheduler
     scripts, scope guard parser logic, queue publish logic, failure taxonomy,
     retry/recovery behavior, docs, task brief, workflows, schemas, Rust
     source, Cargo files, tests, fixtures, status files, existing handoff/log
     artifacts, or sandbox worktrees.
   - Do not edit `ai_handoffs/templates/TASK_PACKET.md` in this dispatch.
     Template hardening is a separate policy decision if the prompt-only fix
     proves insufficient.

   **Implementation behavior required**:
   - In the `Invoke-PlanFill` Rules list, add an explicit rule that every path
     or glob token inside `### MAY edit` and `### MAY add new files` MUST be
     wrapped in backticks.
   - The rule MUST say that bare-bulleted paths are invalid for the queue
     scope guard.
   - Include a tiny worked example in the prompt text, such as
     ``- `Invoke-AiDispatchLoop.ps1` ``.
   - Do not change the scope guard parser, the queue script, task templates,
     schemas, or generated TASK packet grammar outside this prompt rule.
   - Do not change Loop runtime behavior outside the literal planner prompt
     string.

   **Halt conditions**:
   - Halt if the change requires editing anything except
     `Invoke-AiDispatchLoop.ps1` plus this dispatch's own generated artifacts.
   - Halt if the fix requires relaxing or modifying `Invoke-QueueScopeGuard`,
     `Get-TaskPositiveAllowedTokens`, or the queue parser regex.
   - Halt if the fix requires editing `ai_handoffs/templates/TASK_PACKET.md`,
     `.ai/codex_control.schema.json`, `.ai/handoff.schema.json`, Auto, Queue,
     scheduler, docs, Rust/Cargo files, workflows, or the task brief.
   - Halt if the prompt-string edit cannot be localized to the
     `Invoke-PlanFill` Rules list.
   - Halt if PowerShell parser validation, `git diff --check`, or canonical
     `.ai/dispatch.verify.ps1` fails.

   **Verbatim review-gate strings** - the autonomous selector MUST copy these
   eight strings, character-for-character, into the filed GitHub issue body.
   No paraphrasing, no substitution, no reflowing. A packet that lacks any one
   of them verbatim is bounced at review:

   ```
   MUST edit only Invoke-AiDispatchLoop.ps1 plus this dispatch's own ai_handoffs and ai_dispatch_logs artifacts
   MUST scope the implementation to the Invoke-PlanFill planner prompt Rules list only
   MUST require every path or glob token in ### MAY edit and ### MAY add new files to be wrapped in backticks
   MUST state that bare-bulleted paths are invalid for the queue scope guard
   MUST include a tiny worked example of a backtick-quoted allowed path token
   MUST NOT edit Invoke-AiDispatchQueue.ps1 Invoke-AiDispatchAuto.ps1 scope guard parser templates schemas docs Rust Cargo workflows scheduler or task brief
   MUST NOT change runtime behavior outside the literal planner prompt string
   MUST run PowerShell parser validation for Invoke-AiDispatchLoop.ps1, git diff --check, canonical .ai/dispatch.verify.ps1, and static inspection proving the diff is prompt-string-only
   ```

   **Verification required**:
   - PowerShell parser validation for `Invoke-AiDispatchLoop.ps1` reports zero
     parser errors.
   - `git diff --check` reports no whitespace errors.
   - `.ai/dispatch.verify.ps1` passes.
   - Static inspection confirms the diff is limited to the `Invoke-PlanFill`
     Rules list prompt string.
   - Static inspection confirms no changes to Auto, Queue, scope guard parser,
     templates, schemas, docs, Rust/Cargo files, workflows, scheduler scripts,
     or the task brief.

59. **[DONE 2026-05-26 via PR #205 / commit `438ec39`] Wire EnablePreflightAudit through Auto and Queue into Loop.**
   Landed via PR #205. `Invoke-AiDispatchAuto.ps1` and
   `Invoke-AiDispatchQueue.ps1` now accept the opt-in
   `-EnablePreflightAudit` switch and forward it through the autonomous
   Auto -> Queue -> Loop path only when explicitly set. The original brief is
   preserved below.

   The opt-in Codex preflight audit exists in `Invoke-AiDispatchLoop.ps1`, but
   ISSUE-200 confirmed it is not reachable from autonomous dispatches because
   neither Auto nor Queue accepts or forwards `-EnablePreflightAudit`. Add the
   narrow passthrough only. Keep default behavior unchanged when the switch is
   omitted.

   **Runtime invocation note**: current `ai-auto` count is 135. Run as
   `.\Invoke-AiDispatchAuto.ps1 -PublishMode branch -MaxAutonomousTasks 136 -TraceTiming`
   so the cap accommodates exactly this one dispatch. The scheduler remains
   disabled and must not be re-enabled by this task.

   **Required TASK packet shape**:
   - The generated TASK packet MUST include a `### MAY edit` section listing
     exactly `Invoke-AiDispatchAuto.ps1` and `Invoke-AiDispatchQueue.ps1`.
   - The generated TASK packet MAY include a `### MAY add new files` section
     only for this dispatch's own `ai_handoffs/ISSUE-*_TASK_*.md`,
     `ai_handoffs/ISSUE-*_EXEC_*.md`,
     `ai_handoffs/ISSUE-*_CORRECT_*.md` packets, matching `.meta.json`
     sidecars, and its own `ai_dispatch_logs/log_*.md` queue log.
   - The generated TASK packet MUST state that `Invoke-AiDispatchLoop.ps1` is
     already implemented and MUST NOT be edited.

   **Allowed file surface**:
   - EDIT only `Invoke-AiDispatchAuto.ps1` and `Invoke-AiDispatchQueue.ps1`.
   - MAY add this dispatch's own handoff packets, handoff sidecars, and queue
     log as produced by the orchestrator/queue.

   **Files that MUST NOT be touched**:
   - Do not edit `Invoke-AiDispatchLoop.ps1`, `Get-AiDispatchTrends.ps1`,
     `Get-AiDispatchHealth.ps1`, `Register-AiDispatchSchedule.ps1`,
     `Wait-GitHubActions.ps1`, `Watch-AiDispatch.ps1`, `.ai/**`, schemas,
     scope guard parser logic, trace JSONL schema, failure taxonomy, retry or
     recovery behavior, publish behavior, docs, task brief, workflows, Rust
     source, Cargo files, tests, fixtures, status files, existing handoff/log
     artifacts, or sandbox worktrees.

   **Implementation behavior required**:
   - Add `[switch]$EnablePreflightAudit` to `Invoke-AiDispatchAuto.ps1`.
   - When Auto receives `-EnablePreflightAudit`, append
     `-EnablePreflightAudit` to the Queue invocation arguments.
   - Add `[switch]$EnablePreflightAudit` to `Invoke-AiDispatchQueue.ps1`.
   - When Queue receives `-EnablePreflightAudit`, append
     `-EnablePreflightAudit` to the Loop invocation arguments.
   - With the switch omitted, Auto and Queue behavior must remain unchanged:
     no preflight audit is requested and existing command lines stay otherwise
     equivalent.
   - Preserve existing `-TraceTiming` passthrough behavior and JSONL schema.
   - Do not add a default-on mode, environment-variable fallback, scheduler
     flag, template change, schema change, or new retry/recovery route.

   **Halt conditions**:
   - Halt if the passthrough cannot be implemented by editing only
     `Invoke-AiDispatchAuto.ps1` and `Invoke-AiDispatchQueue.ps1` plus this
     dispatch's own generated artifacts.
   - Halt if implementing the passthrough requires editing
     `Invoke-AiDispatchLoop.ps1`, scope guard parser logic, schemas, templates,
     docs, scheduler scripts, Rust/Cargo files, workflows, or the task brief.
   - Halt if default unset behavior would change.
   - Halt if the change would make preflight audit default-on.
   - Halt if PowerShell parser validation, `git diff --check`, or canonical
     `.ai/dispatch.verify.ps1` fails.

   **Verbatim review-gate strings** - the autonomous selector MUST copy these
   eight strings, character-for-character, into the filed GitHub issue body.
   No paraphrasing, no substitution, no reflowing. A packet that lacks any one
   of them verbatim is bounced at review:

   ```
   MUST edit only Invoke-AiDispatchAuto.ps1 and Invoke-AiDispatchQueue.ps1 plus this dispatch's own ai_handoffs and ai_dispatch_logs artifacts
   MUST add an opt-in EnablePreflightAudit switch to Auto and Queue
   MUST forward EnablePreflightAudit from Auto to Queue and from Queue to Loop only when explicitly set
   MUST preserve default unset behavior with no preflight audit requested
   MUST NOT edit Invoke-AiDispatchLoop.ps1 scope guard parser schemas templates docs scheduler Rust Cargo workflows task brief retry recovery taxonomy publish or trace JSONL schema
   MUST preserve existing TraceTiming passthrough behavior
   MUST NOT add default-on behavior environment fallback scheduler flag new retry route recovery route or dashboard
   MUST run PowerShell parser validation for Auto and Queue, git diff --check, canonical .ai/dispatch.verify.ps1, and static inspection proving Loop is untouched
   ```

   **Verification required**:
   - PowerShell parser validation for `Invoke-AiDispatchAuto.ps1` and
     `Invoke-AiDispatchQueue.ps1` reports zero parser errors.
   - `git diff --check` reports no whitespace errors.
   - `.ai/dispatch.verify.ps1` passes.
   - Static inspection confirms Auto accepts `-EnablePreflightAudit` and passes
     it to Queue only when set.
   - Static inspection confirms Queue accepts `-EnablePreflightAudit` and
     passes it to Loop only when set.
   - Static inspection confirms `Invoke-AiDispatchLoop.ps1` and all forbidden
     surfaces are untouched.

60. **[DONE 2026-05-26 via PR #207 / commit `bf5f62d`] Read-only audit: PublishMode branch / NoPublish propagation path.**
   Landed via PR #207. The audit classified the observed
   `queue.publish: skipped (NoPublish=true, eligibleForPublish=true)` behavior
   as intended documented branch-mode behavior: Auto appends `-NoPublish` when
   `-PublishMode branch` is selected, Queue computes publish eligibility
   independently, and branch mode leaves the ready commit local for human PR
   review. No automation code follow-up was recommended. The original brief is
   preserved below.

   ISSUE-202 and ISSUE-204 both completed control-passed dispatches with local
   ready-for-publish commits, but Queue traced
   `queue.publish: skipped (NoPublish=true, eligibleForPublish=true)` even
   though Auto was invoked with `-PublishMode branch -TraceTiming`. Audit the
   current automation path before changing behavior. The goal is to classify
   whether this is intended by documented `PublishMode branch` semantics, a
   naming/expectation mismatch, or a real propagation bug, then name the
   smallest safe follow-up.

   **Runtime invocation note**: current `ai-auto` count is 136. Run as
   `.\Invoke-AiDispatchAuto.ps1 -PublishMode branch -MaxAutonomousTasks 137 -TraceTiming`
   so the cap accommodates exactly this one read-only dispatch. The scheduler
   remains disabled and must not be re-enabled by this task.

   **Required TASK packet shape**:
   - The generated TASK packet MUST state this is a read-only audit.
   - The generated TASK packet MUST include no `### MAY edit` section for
     production files.
   - The generated TASK packet MAY include a `### MAY add new files` section
     only for this dispatch's own `ai_handoffs/ISSUE-*_TASK_*.md`,
     `ai_handoffs/ISSUE-*_EXEC_*.md`,
     `ai_handoffs/ISSUE-*_CORRECT_*.md` packets, matching `.meta.json`
     sidecars, and its own `ai_dispatch_logs/log_*.md` queue log.

   **Allowed file surface**:
   - Do not edit tracked source, tests, docs, Cargo files, workflows, schemas,
     task brief, or automation scripts.
   - MAY add this dispatch's own handoff packets, handoff sidecars, and queue
     log as produced by the orchestrator/queue.
   - MAY write ignored scratch only under this dispatch's own `.ai/dispatch-*`
     run directory if needed.

   **Questions to answer in the EXEC packet**:
   - Q1. What do `AI_DISPATCH_AUTOMATION.md`, script help text, and current
     Auto/Queue code say `PublishMode branch` is supposed to do?
   - Q2. Where exactly does Auto translate `-PublishMode branch` into Queue
     arguments, and where exactly does Queue set or consume `NoPublish`?
   - Q3. Why did ISSUE-202 and ISSUE-204 produce
     `NoPublish=true, eligibleForPublish=true` despite being invoked with
     `-PublishMode branch -TraceTiming`? Cite the trace/log evidence.
   - Q4. Is the observed behavior intended behavior, a docs/help/name
     mismatch, or an implementation bug? If the evidence is insufficient,
     return `NEEDS_HUMAN` and identify the missing decision.
   - Q5. Name exactly one smallest safe follow-up task, with allowed files,
     verification gates, and halt conditions. If Q4 is not decisive, the
     follow-up must be an arbiter/docs decision task rather than code.

   **Halt conditions**:
   - Halt if the audit would need to edit Auto, Queue, Loop, scheduler,
     trace JSONL, retry/recovery, docs, task brief, Rust/Cargo files,
     workflows, schemas, or existing handoff/log artifacts.
   - Halt if the answer cannot be grounded in current code, docs/help text,
     and ISSUE-202 / ISSUE-204 dispatch artifacts.
   - Halt if the audit cannot distinguish current documented behavior from
     desired behavior without an explicit human decision.

   **Verbatim review-gate strings** - the autonomous selector MUST copy these
   eight strings, character-for-character, into the filed GitHub issue body.
   No paraphrasing, no substitution, no reflowing. A packet that lacks any one
   of them verbatim is bounced at review:

   ```
   MUST perform read-only automation audit only and MUST NOT edit source tests docs Cargo workflows schemas task brief or automation scripts
   MUST inspect AI_DISPATCH_AUTOMATION.md script help text Invoke-AiDispatchAuto.ps1 Invoke-AiDispatchQueue.ps1 and ISSUE-202 ISSUE-204 dispatch artifacts
   MUST identify where Auto translates PublishMode branch into Queue arguments and where Queue sets or consumes NoPublish
   MUST explain why ISSUE-202 and ISSUE-204 traced NoPublish=true eligibleForPublish=true after Auto was invoked with PublishMode branch TraceTiming
   MUST classify the observed behavior as intended documented behavior docs/help/name mismatch implementation bug or NEEDS_HUMAN
   MUST name exactly one smallest safe follow-up with allowed files verification gates and halt conditions or return NEEDS_HUMAN
   MUST leave only this dispatch's own handoff/log artifacts plus optional ignored .ai dispatch scratch
   MUST run git diff --check and report git status showing no tracked source/test/doc/Cargo/script changes
   ```

   **Verification required**:
   - `git diff --check` reports no whitespace errors.
   - `git status --short --untracked-files=all` shows no tracked source,
     test, docs, Cargo, workflow, schema, task brief, or automation script
     changes.
   - The EXEC packet answers Q1-Q5 explicitly with line-cited evidence.
   - Static inspection confirms the audit did not edit production files.

61. **[DONE 2026-05-26 via PR #209 / commit `0a5e435`] Document delegated-human policy for fully unattended auto-publish mode.**
   Landed via PR #209. `AI_DISPATCH_AUTOMATION.md` now documents delegated
   human mode as bounded opt-in authorization for `-PublishMode main`, with
   branch mode preserved as the default, finite cap discipline, stop
   conditions, rollback behavior, audit requirements, and allowed-surface
   tiers. No automation runtime behavior was changed. The original brief is
   preserved below.

   Before running autonomous dispatch indefinitely with Codex acting as the
   delegated human publisher, add an explicit policy section to the automation
   documentation. This task records the risk model, allowed surfaces, cap
   discipline, rollback path, and stop conditions for any future
   `-PublishMode main` / fully unattended scheduled run. It is policy-only:
   do not enable or schedule unattended main publishing in this dispatch.

   **Runtime invocation note**: current `ai-auto` count is 137. Run as
   `.\Invoke-AiDispatchAuto.ps1 -PublishMode branch -MaxAutonomousTasks 138 -TraceTiming`
   so the cap accommodates exactly this one policy dispatch. The scheduler
   remains disabled and must not be re-enabled by this task.

   **Required TASK packet shape**:
   - The generated TASK packet MUST include a `### MAY edit` section listing
     exactly `AI_DISPATCH_AUTOMATION.md`.
   - The generated TASK packet MAY include a `### MAY add new files` section
     only for this dispatch's own `ai_handoffs/ISSUE-*_TASK_*.md`,
     `ai_handoffs/ISSUE-*_EXEC_*.md`,
     `ai_handoffs/ISSUE-*_CORRECT_*.md` packets, matching `.meta.json`
     sidecars, and its own `ai_dispatch_logs/log_*.md` queue log.
   - The generated TASK packet MUST state that this dispatch is documentation
     policy only and MUST NOT change automation runtime behavior.

   **Allowed file surface**:
   - EDIT only `AI_DISPATCH_AUTOMATION.md`.
   - MAY add this dispatch's own handoff packets, handoff sidecars, and queue
     log as produced by the orchestrator/queue.

   **Files that MUST NOT be touched**:
   - Do not edit `Invoke-AiDispatchAuto.ps1`, `Invoke-AiDispatchQueue.ps1`,
     `Invoke-AiDispatchLoop.ps1`, `Register-AiDispatchSchedule.ps1`,
     `Wait-GitHubActions.ps1`, `Watch-AiDispatch.ps1`, `Get-AiDispatch*.ps1`,
     `.ai/**`, schemas, workflows, Rust source, Cargo files, tests, fixtures,
     status files, existing handoff/log artifacts, task brief, or sandbox
     worktrees.
   - Do not register, unregister, enable, disable, or modify the Windows
     Scheduled Task.
   - Do not run Auto in `-PublishMode main` in this dispatch.

   **Policy content required**:
   - Add a clearly named section such as "Delegated Human Mode" or "Fully
     Unattended Auto-Publish Policy" to `AI_DISPATCH_AUTOMATION.md`.
   - Define delegated-human mode as explicit human authorization for the
     automation/Codex workflow to use `-PublishMode main` for a bounded batch,
     with Codex control review and CI/verification gates acting as the publish
     gate.
   - State that branch mode remains the default and safest mode; delegated
     main publish is opt-in per run or scheduled-registration decision.
   - Record the risk model: no PR review before merge, CI/control can miss
     product/design mistakes, source changes have higher blast radius than
     docs/audit/tooling changes, and bad merges require revert rather than PR
     rejection.
   - Define allowed surfaces by risk tier: docs/audit/generated-dispatch
     artifacts, automation tooling, low-risk tests/fixtures, and production
     Rust/runtime/editor/kernel code. Each tier must say whether it is allowed
     by default, requires explicit human batch authorization, or should remain
     branch-mode only.
   - Define cap rules: `-MaxAutonomousTasks` must remain finite; the cap must
     be raised deliberately between batches; "indefinite" operation means
     repeated bounded batches, not removing stop conditions.
   - Define stop conditions: any `ai-dispatch-failed` issue, CodeQL/CI failure,
     blocked/NEEDS_HUMAN EXEC status, scope-guard violation, dirty worktree,
     unexplained publish/trace anomaly, or trend-alert regression halts the
     delegated run until reviewed.
   - Define rollback behavior: stop or unregister the scheduler, capture the
     trace/log evidence, identify the merge commit(s), prefer `git revert`
     commits over history rewrites, and record the rollback in the relevant
     issue/brief/status note.
   - Define audit requirements: every delegated run must leave JSONL traces,
     issue comments, handoff packets, queue logs, merge commits, and a final
     human/Codex summary naming the cap, mode, tasks landed, failures, and
     rollback decisions.

   **Halt conditions**:
   - Halt if the policy cannot be added by editing only
     `AI_DISPATCH_AUTOMATION.md` plus this dispatch's own generated artifacts.
   - Halt if the policy would require changing scripts, schemas, scheduler
     registration, CI workflows, Rust/Cargo files, tests, or task selection
     behavior.
   - Halt if the documentation would imply `-PublishMode main` is now safe as
     the default for all source work without explicit bounded authorization.
   - Halt if the docs would remove or weaken the existing cap/halt semantics.
   - Halt if `git diff --check` or canonical `.ai/dispatch.verify.ps1` fails.

   **Verbatim review-gate strings** - the autonomous selector MUST copy these
   eight strings, character-for-character, into the filed GitHub issue body.
   No paraphrasing, no substitution, no reflowing. A packet that lacks any one
   of them verbatim is bounced at review:

   ```
   MUST edit only AI_DISPATCH_AUTOMATION.md plus this dispatch's own ai_handoffs and ai_dispatch_logs artifacts
   MUST document delegated human mode as explicit bounded opt-in authorization for PublishMode main
   MUST state branch mode remains the default and safest mode
   MUST document risk model allowed surfaces cap rules stop conditions rollback behavior and audit requirements
   MUST state MaxAutonomousTasks remains finite and indefinite operation means repeated bounded batches not removing stop conditions
   MUST NOT edit Auto Queue Loop scheduler scripts schemas workflows Rust Cargo tests task brief existing artifacts or sandbox worktrees
   MUST NOT enable register schedule run or default PublishMode main in this dispatch
   MUST run git diff --check, canonical .ai/dispatch.verify.ps1, and static inspection proving only AI_DISPATCH_AUTOMATION.md changed outside generated artifacts
   ```

   **Verification required**:
   - `git diff --check` reports no whitespace errors.
   - `.ai/dispatch.verify.ps1` passes.
   - Static inspection confirms the only tracked production/documentation
     change is `AI_DISPATCH_AUTOMATION.md`.
   - Static inspection confirms no automation runtime script, scheduler,
     schema, workflow, Rust/Cargo, test, fixture, or task-selection behavior
     changed.

62. **[DONE 2026-05-26 via ISSUE-210 / commit `20518c0`] Read-only Phase 9 audit: freeze-validity pressure.**
   Delegated-main smoke batch task 1 of 10. Audit Phase 9's "freeze validity"
   pressure axis against current `plans/PLAN.md`, `plans/IMPLEMENTATION.md`,
   `Status.md`, `HANDOFF.md`, recent `change.md`, and current source layout.
   Answer whether the frozen architecture is still coherent, where evidence
   supports or weakens the freeze, and name exactly one smallest safe follow-up
   or `NEEDS_HUMAN`.

63. **[DONE 2026-05-26 via ISSUE-212 / commit `e8219e0`] Read-only Phase 9 audit: abstraction-pain pressure.**
   Delegated-main smoke batch task 2 of 10. Audit current abstraction pain
   across the most active substrates (`cad-core`, `cad-projection`,
   `editor-shell`, `gfx`, `rge-scene-loader`, automation scripts) using only
   existing code and dispatch artifacts. Identify the sharpest pain point and
   name exactly one smallest safe follow-up or `NEEDS_HUMAN`.

64. **[DONE 2026-05-26 via ISSUE-214 / commit `283cc19`] Read-only Phase 9 audit: invalidation-economics pressure.**
   Delegated-main smoke batch task 3 of 10. Audit cache invalidation and
   recompute economics across graph-foundation, cad-core tessellation/projection
   caches, frame-graph resource maps, and script hot-reload evidence. Identify
   whether any measured or structural invalidation cost deserves the next
   dispatch, and name exactly one smallest safe follow-up or `NEEDS_HUMAN`.

65. **[DONE 2026-05-26 via ISSUE-216 / commit `b3bfb76`] Read-only Phase 9 audit: reflection-scale pressure.**
   Delegated-main smoke batch task 4 of 10. Re-audit reflection adoption after
   the recent typed scene-loader work: search production and test usage of
   `kernel/types`, `rge-macros-reflect`, typed `ComponentValue`, and any
   schema/loader bridge references. Decide whether reflection-scale pressure is
   still untriggered, and name exactly one smallest safe follow-up or
   `NEEDS_HUMAN`.

66. **[DONE 2026-05-26 via ISSUE-217 / commit `2650320`] Read-only Phase 9 audit: async-orchestration pressure.**
   Delegated-main smoke batch task 5 of 10. Audit whether job-system,
   io-scheduler, asset-streaming, asset-view, or shared kernel cavities have
   gained concrete consumer pressure from current runtime/editor/asset paths.
   Do not invent substrate work. Name exactly one smallest safe follow-up or
   `NEEDS_HUMAN`.

67. **[DONE 2026-05-26 via ISSUE-218 / commit `93b7fa6`] Read-only Phase 9 audit: compile-time pressure.**
   Delegated-main smoke batch task 6 of 10. Audit current compile-time signals
   from CI logs, local dispatch verify logs, crate/test fanout, and recent
   dependency churn. Identify whether compile-time pressure has a bounded
   autonomous follow-up, and name exactly one smallest safe follow-up or
   `NEEDS_HUMAN`.

68. **[DONE 2026-05-26 via ISSUE-219 / commit `6df1bef`] Read-only Phase 9 audit: editor-usability pressure.**
   Delegated-main smoke batch task 7 of 10. Audit current editor usability
   substrate, including editor-shell, editor-ui, command-bus, runtime-headless,
   and scene-loader consumers. Distinguish user-visible gaps from substrate
   prerequisites. Name exactly one smallest safe follow-up or `NEEDS_HUMAN`.

69. **[DONE 2026-05-26 via ISSUE-220 / commit `7760bb9`] Read-only Phase 9 audit: GPU-pressure axis.**
   Delegated-main smoke batch task 8 of 10. Audit GPU/render pressure from gfx,
   frame-graph, render-handoff, editor-shell render path, and baseline docs.
   Determine whether the next GPU task is measurement, integration, docs
   reconciliation, or no-op. Name exactly one smallest safe follow-up or
   `NEEDS_HUMAN`.

70. **[DONE 2026-05-26 via ISSUE-221 / commit `7c100e2`] Read-only audit: persistent kernel cavity pressure.**
   Delegated-main smoke batch task 9 of 10. Re-audit the five persistent
   Tier-1 kernel v0 cavities (`shared`, `asset-streaming`, `io-scheduler`,
   `job-system`, `asset-view`) against current code and docs. Classify each as
   still pressure-deferred or newly triggered. Name exactly one smallest safe
   follow-up or `NEEDS_HUMAN`.

71. **[DONE 2026-05-27 via ISSUE-222 / commit `ab7229c`] Read-only audit: delegated-main batch outcome synthesis.**
   Delegated-main smoke batch task 10 of 10. After tasks #62-#70 are filed or
   completed, synthesize their issue/EXEC outcomes plus JSONL timing traces.
   Report which Phase 9 axis, if any, should become the next branch-mode
   implementation task. Name exactly one smallest safe follow-up or
   `NEEDS_HUMAN`.

   **Common constraints for tasks #62-#71**:
   - These are read-only audit tasks intended for the first delegated
     `-PublishMode main` smoke batch.
   - Do not edit tracked source, tests, docs, Cargo files, workflows, schemas,
     automation scripts, scheduler configuration, or `.ai/dispatch.tasks.md`.
   - MAY add only each dispatch's own handoff packets, sidecars, queue log, and
     optional ignored `.ai/dispatch-*` scratch.
   - The EXEC packet must answer the task-specific audit question with
     line-cited evidence and must name exactly one smallest safe follow-up or
     `NEEDS_HUMAN`.
   - If a task recommends implementation touching production Rust, scripts,
     workflows, Cargo, or scheduler behavior, the follow-up must explicitly be
     branch-mode unless a later human authorization says otherwise.
   - Verification: `git diff --check`, `git status --short
     --untracked-files=all`, and static inspection proving no tracked
     production/doc/script/Cargo/workflow/schema/task-brief files changed.

72. **[DONE 2026-05-27 via ISSUE-223 / commit `6a24f51`] Add PowerShell CI guardrails for dispatch automation.**
   Branch-mode infrastructure task. Add the first PowerShell quality gate for
   the dispatch scripts by combining (a) focused Pester behavior coverage for
   the queue audit-log writer and (b) repository-wide PSScriptAnalyzer static
   analysis in a new GitHub Actions workflow. This is the prevention task for
   audit-log string-template regressions such as literal `$Id` / `$Branch`
   tokens leaking into committed queue logs.

   **Required TASK packet shape**:
   - The generated TASK packet MUST state that this task adds PowerShell CI
     guardrails only and MUST NOT modify Rust runtime/editor/kernel behavior.
   - The generated TASK packet MUST include a `### MAY edit` section covering
     `.github/workflows/powershell.yml`, `Invoke-AiDispatchQueue.ps1`,
     PowerShell test files under `tools/dispatch-tests/**`, optional
     PSScriptAnalyzer settings, and only those other `*.ps1` / `*.psm1` files
     that need no-semantics lint cleanups for the analyzer gate to pass.
   - The generated TASK packet MUST state that `.ai/dispatch.verify.ps1` is
     not the place for this new smoke gate; keep the new PowerShell checks in
     the dedicated workflow/tests unless a later task explicitly changes the
     canonical verify contract.

   **Allowed file surface**:
   - MAY add `.github/workflows/powershell.yml`.
   - MAY add `tools/dispatch-tests/**` Pester tests and helper files.
   - MAY edit `Invoke-AiDispatchQueue.ps1` only to make the audit-log body
     generation testable through production code and to fix the unexpanded
     audit-log variables.
   - MAY add a PSScriptAnalyzer settings file if needed to keep the rule set
     explicit.
   - MAY edit other existing `*.ps1` / `*.psm1` files only for narrowly scoped
     analyzer cleanups that preserve behavior.
   - MAY add this dispatch's own handoff packets, handoff sidecars, and queue
     log as produced by the orchestrator/queue.

   **Files that MUST NOT be touched**:
   - Do not edit Rust source, Cargo files, architecture-lint source, fixtures,
     PLAN/Status/HANDOFF/change docs, existing handoff/log artifacts, or
     `.ai/dispatch.tasks.md`.
   - Do not add the PowerShell smoke test to `.ai/dispatch.verify.ps1`.
   - Do not change publish policy, scheduler behavior, dispatch queue labels,
     retry semantics, or auto-publish eligibility.

   **Pester requirement**:
   - Add at least one Pester test that exercises the production queue
     audit-log writer/body builder with synthetic issue/run inputs.
   - The test MUST assert that the generated log contains the synthetic values
     for dispatch id, branch, loop exit code, Codex verdict, and loop log path.
   - The test MUST assert that the generated log contains no unexpanded
     PowerShell variable tokens matching ``\$[A-Za-z_][A-Za-z0-9_]*``.
   - The test MUST call production code, not a copied template string. A small
     pure helper such as `New-DispatchLogBody` is acceptable if
     `Write-DispatchLog` delegates to it.

   **PSScriptAnalyzer requirement**:
   - The new workflow MUST install or otherwise make available Pester and
     PSScriptAnalyzer on `windows-latest`.
   - The workflow MUST run Pester for `tools/dispatch-tests/**`.
   - The workflow MUST run PSScriptAnalyzer over tracked `*.ps1` / `*.psm1`
     files while excluding generated/ignored dispatch scratch such as
     `.ai/dispatch-*` and `OLD/`.
   - Any suppression or custom analyzer setting MUST be explicit and justified
     in code/config comments; do not hide real errors by broadly disabling the
     analyzer.

   **Halt conditions**:
   - Halt if making the queue log writer testable would require changing
     queue selection, branch management, publish policy, retry behavior, issue
     labelling, or scheduler behavior.
   - Halt if PSScriptAnalyzer produces a large unrelated remediation set that
     cannot be cleaned without changing behavior; report the findings and
     propose a smaller baseline task instead.
   - Halt if the Pester test cannot exercise production queue log generation
     without duplicating the template.
   - Halt if any required fix would touch Rust/Cargo/architecture-lint code or
     existing generated dispatch artifacts.

   **Verbatim review-gate strings** - the autonomous selector MUST copy these
   seven strings, character-for-character, into the filed GitHub issue body.
   No paraphrasing, no substitution, no reflowing. A packet that lacks any one
   of them verbatim is bounced at review:

   ```
   MUST add a Windows powershell.yml workflow that runs both Pester and PSScriptAnalyzer
   MUST add a Pester test for the production queue audit-log writer using synthetic inputs
   MUST assert the generated audit log contains no unexpanded PowerShell variable tokens matching \$[A-Za-z_][A-Za-z0-9_]*
   MUST run PSScriptAnalyzer over tracked ps1 and psm1 files while excluding generated dispatch scratch
   MUST keep the new PowerShell smoke gate out of .ai/dispatch.verify.ps1
   MUST NOT change publish policy scheduler behavior queue labels retry semantics or auto-publish eligibility
   MUST NOT edit Rust source Cargo files architecture-lint source fixtures status docs task brief or existing dispatch artifacts
   ```

   **Verification required**:
   - `git diff --check` reports no whitespace errors.
   - The new Pester test passes locally on Windows PowerShell or `pwsh`.
   - PSScriptAnalyzer passes locally with the same target set/settings used by
     the workflow.
   - Static inspection confirms `Invoke-AiDispatchQueue.ps1` still writes the
     same audit-log sections, now with expanded synthetic/runtime values.
   - Static inspection confirms `.ai/dispatch.verify.ps1`, Rust/Cargo files,
     architecture-lint source, scheduler behavior, publish policy, retry
     semantics, and existing dispatch artifacts were not changed.

73. **[DONE-SUPERSEDED 2026-05-27 via issue #225 / commit `8c8da1d`] Docs-only Phase 9 PREFLIGHT: scope `rge-editor -> rge-scene-loader` `--scene` integration.**
   The branch-only ISSUE-224 PREFLIGHT was used as the scoping basis for
   ISSUE-225, and ISSUE-225 landed the bounded `--scene` implementation on
   `main`. Do not auto-select this stale PREFLIGHT as a new dispatch. Original
   task text follows for provenance.

   Branch-mode docs-only follow-up from ISSUE-219 / ISSUE-222. Append one new
   Phase 9 PREFLIGHT section to `plans/BASELINE.md` that scopes the future
   `rge-editor` integration with `rge-scene-loader` for opening
   `golden-projects/simple-scene/.rge-project` / `.rge-scene` through a
   `--scene <path>` CLI path.

   **Allowed file surface**:
   - MAY edit `plans/BASELINE.md` only.
   - MAY add this dispatch's own handoff packets, sidecars, queue log, and
     ignored `.ai/dispatch-*` scratch.
   - MUST NOT edit source, tests, Cargo files, lints, ADRs, workflows,
     automation scripts, scheduler config, `.ai/dispatch.tasks.md`, existing
     handoff/log artifacts, `Status.md`, `HANDOFF.md`, or `change.md`.

   **Required section content**:
   - Cite ISSUE-219's accepted decision block:
     `EDITOR_USABILITY_PRESSURE_VERDICT: substrate_prerequisite_triggered`.
   - Record the current gap: `rge-editor` has no `--scene <path>` parser arm
     and no `load_scene_into_world` call site; only `--glb` and the default
     cuboid demo path are available.
   - Scope the future implementation, without performing it: add
     `rge-scene-loader` + `rge-data` Cargo edges to
     `editor/rge-editor/Cargo.toml`; add a `--scene <path>` parser arm beside
     the existing `--glb` path; call `load_scene_into_world`; hand the loaded
     `World` to `EditorShell::with_world`; re-verify the existing
     `cad_world == None` render-path branch for the simple-scene fixture;
     mirror the `runtime-headless` integration-test strategy.
   - Explicitly defer ISSUE-212 CadCheckpoint work, MenuRegistry wiring,
     hierarchy panel UI, Save `.rge-scene`, workspace/project selection, and
     any source/test/Cargo implementation to later separately authorized
     dispatches.

   **Verbatim review-gate strings** - the autonomous selector MUST copy these
   strings character-for-character into the filed GitHub issue body. No
   paraphrasing, no substitution, no reflowing. A packet that lacks any one
   of them verbatim is bounced at review:

   ```text
   MUST append exactly one docs-only Phase 9 PREFLIGHT section to plans/BASELINE.md
   MUST scope rge-editor -> rge-scene-loader --scene integration without implementing it
   MUST cite ISSUE-219's substrate_prerequisite_triggered verdict
   MUST state that rge-editor currently has no --scene parser arm and no load_scene_into_world call site
   MUST explicitly defer CadCheckpoint, MenuRegistry, hierarchy panel, Save .rge-scene, and workspace selection
   MUST NOT edit source tests Cargo lints ADRs workflows automation scripts scheduler config task brief Status HANDOFF or change docs
   ```

   **Verification required**:
   - `git diff --check` reports no whitespace errors.
   - Static inspection confirms only `plans/BASELINE.md` changed among
     tracked files.
   - Static inspection confirms no source, tests, Cargo, lint, ADR, workflow,
     automation, scheduler, task-brief, `Status.md`, `HANDOFF.md`, or
     `change.md` files changed.
   - The appended section is clearly labelled Phase 9 PREFLIGHT and names the
     future implementation as branch-mode unless later human-authorized.

74. **[DONE 2026-06-07 via local commit `69676be`] Command palette selection model helper.**
   PR-mode editor-usability task. Add a small host-local command-palette
   selection helper in `editor-egui-host` so the palette can track "which
   enabled filtered row is selected" without changing current rendering yet.
   This is the substrate task for arrow-key navigation; keep behavior changes
   out of this slice except any private helper call needed by tests.

   **Allowed file surface**:
   - MAY edit `crates/editor-egui-host/src/menu.rs`.
   - MAY edit `crates/editor-egui-host/src/menu_tests.rs`.
   - MAY add this dispatch's own handoff packets, sidecars, queue log, and
     ignored `.ai/dispatch-*` scratch.
   - MUST NOT edit `crates/editor-ui/**`, `crates/editor-shell/**`,
     `editor/rge-editor/**`, Cargo files, docs, workflows, automation scripts,
     scheduler config, existing handoff/log artifacts, or this task brief.

   **Required behavior**:
   - Add a helper that receives the filtered palette entries and a current
     selected index, then returns a valid selected index for the first enabled
     row when needed.
   - Disabled entries may remain visible but MUST NOT become the selected
     keyboard target.
   - Empty or disabled-only result sets MUST yield no selected target.
   - Preserve the existing `first_enabled_command_palette_entry` behavior until
     a later task explicitly changes Enter activation.

   **Verbatim review-gate strings** - copy these into the filed issue body:

   ```text
   MUST add a command-palette selection helper without changing visible palette behavior
   MUST keep disabled rows visible but ineligible as the selected keyboard target
   MUST preserve existing Enter activation behavior in this dispatch
   MUST NOT edit editor-ui editor-shell rge-editor Cargo docs workflows automation scripts scheduler config existing dispatch artifacts or this task brief
   ```

   **Verification required**:
   - `cargo +nightly fmt --all -- --check`.
   - `git diff --check`.
   - Focused `editor-egui-host` tests covering first enabled selection,
     disabled-only none, empty none, and preserving a still-valid selected row.

75. **[DONE 2026-06-07 via local commit `69676be`] Command palette ArrowUp / ArrowDown navigation.**
   PR-mode editor-usability task. Wire the helper from task 74 into the actual
   command-palette window so ArrowDown and ArrowUp move a visible selection
   cursor through enabled filtered rows.

   **Allowed file surface**:
   - MAY edit `crates/editor-egui-host/src/lib.rs`.
   - MAY edit `crates/editor-egui-host/src/menu.rs`.
   - MAY edit `crates/editor-egui-host/src/menu_tests.rs`.
   - MAY add this dispatch's own handoff packets, sidecars, queue log, and
     ignored `.ai/dispatch-*` scratch.
   - MUST NOT edit `crates/editor-ui/**`, `crates/editor-shell/**`,
     `editor/rge-editor/**`, Cargo files, docs, workflows, automation scripts,
     scheduler config, existing handoff/log artifacts, or this task brief.

   **Required behavior**:
   - `EguiHost` may own a small selected-row state field if needed.
   - ArrowDown moves to the next enabled filtered row.
   - ArrowUp moves to the previous enabled filtered row.
   - Navigation must skip disabled rows and do nothing for empty or
     disabled-only result sets.
   - Keep command execution on the existing `MenuCommandHandoff` path.
   - Do not add fuzzy matching, command history, a separate command model, or
     plugin action execution.

   **Verbatim review-gate strings** - copy these into the filed issue body:

   ```text
   MUST wire ArrowDown and ArrowUp navigation inside the command-palette window
   MUST skip disabled rows during keyboard navigation
   MUST keep command execution on the existing MenuCommandHandoff path
   MUST NOT add fuzzy matching command history a separate command model or plugin action execution
   MUST NOT edit editor-ui editor-shell rge-editor Cargo docs workflows automation scripts scheduler config existing dispatch artifacts or this task brief
   ```

   **Verification required**:
   - `cargo +nightly fmt --all -- --check`.
   - `git diff --check`.
   - Focused helper or host tests proving ArrowDown / ArrowUp movement, wrap or
     boundary semantics as chosen by the implementation, and disabled-row skip.

76. **[DONE 2026-06-07 via local commit `69676be`] Command palette Enter activates selected row.**
   PR-mode editor-usability task. Change Enter activation from "first enabled
   filtered row" to "currently selected enabled filtered row" now that the
   palette has navigation state.

   **Allowed file surface**:
   - MAY edit `crates/editor-egui-host/src/lib.rs`.
   - MAY edit `crates/editor-egui-host/src/menu.rs`.
   - MAY edit `crates/editor-egui-host/src/menu_tests.rs`.
   - MAY add this dispatch's own handoff packets, sidecars, queue log, and
     ignored `.ai/dispatch-*` scratch.
   - MUST NOT edit `crates/editor-ui/**`, `crates/editor-shell/**`,
     `editor/rge-editor/**`, Cargo files, docs, workflows, automation scripts,
     scheduler config, existing handoff/log artifacts, or this task brief.

   **Required behavior**:
   - Enter activates the selected enabled row, not always the first enabled row.
   - If the selected row becomes invalid after filtering or enablement changes,
     clamp or reset to a valid enabled row before Enter is evaluated.
   - Empty or disabled-only result sets still dispatch nothing.
   - Closing the palette or activating a command must clear transient selection
     state along with the existing filter clear.

   **Verbatim review-gate strings** - copy these into the filed issue body:

   ```text
   MUST make Enter activate the selected enabled command-palette row
   MUST dispatch nothing for empty or disabled-only command-palette results
   MUST clear transient selection state when the palette closes or activates a command
   MUST NOT change menu registry semantics or editor-shell command routing
   MUST NOT edit editor-ui editor-shell rge-editor Cargo docs workflows automation scripts scheduler config existing dispatch artifacts or this task brief
   ```

   **Verification required**:
   - `cargo +nightly fmt --all -- --check`.
   - `git diff --check`.
   - Focused tests proving Enter uses selected row, selection clamps after
     filtering, and disabled-only results dispatch nothing.

77. **[DONE 2026-06-07 via local commit `69676be`] Command palette search-field focus on open.**
   PR-mode editor-usability task. Make the command-palette search field receive
   keyboard focus when the palette opens so users can type immediately after
   `Ctrl+Shift+P`.

   **Allowed file surface**:
   - MAY edit `crates/editor-egui-host/src/lib.rs`.
   - MAY edit `crates/editor-egui-host/src/menu.rs`.
   - MAY edit `crates/editor-egui-host/src/menu_tests.rs` only if a practical
     pure helper or public-state test can pin the behavior without brittle egui
     internals.
   - MAY add this dispatch's own handoff packets, sidecars, queue log, and
     ignored `.ai/dispatch-*` scratch.
   - MUST NOT edit `crates/editor-ui/**`, `crates/editor-shell/**`,
     `editor/rge-editor/**`, Cargo files, docs, workflows, automation scripts,
     scheduler config, existing handoff/log artifacts, or this task brief.

   **Required behavior**:
   - Opening the palette through `toggle_command_palette()` must mark the
     search field for focus on the next render.
   - Focus request state must be one-shot; it must not repeatedly steal focus
     every frame while the palette remains open.
   - Closing the palette without activation must clear the one-shot focus
     request.

   **Verbatim review-gate strings** - copy these into the filed issue body:

   ```text
   MUST request focus for the command-palette search field when the palette opens
   MUST make the focus request one-shot rather than stealing focus every frame
   MUST clear the focus request when the palette closes
   MUST NOT change editor-shell accelerator routing or menu registry semantics
   MUST NOT edit editor-ui editor-shell rge-editor Cargo docs workflows automation scripts scheduler config existing dispatch artifacts or this task brief
   ```

   **Verification required**:
   - `cargo +nightly fmt --all -- --check`.
   - `git diff --check`.
   - Focused compile/tests where practical; if egui focus cannot be asserted
     cleanly, record static inspection evidence in the EXEC packet and avoid
     brittle UI tests.

78. **[DONE 2026-06-07 via local commit `69676be`] Command palette selected-row visibility polish.**
   PR-mode editor-usability task. Give the selected command-palette row a clear
   visual affordance and keep it scrolled into view during keyboard navigation.

   **Allowed file surface**:
   - MAY edit `crates/editor-egui-host/src/menu.rs`.
   - MAY edit `crates/editor-egui-host/src/menu_tests.rs` only for helper-level
     tests that do not depend on pixel snapshots.
   - MAY add this dispatch's own handoff packets, sidecars, queue log, and
     ignored `.ai/dispatch-*` scratch.
   - MUST NOT edit `crates/editor-ui/**`, `crates/editor-shell/**`,
     `editor/rge-editor/**`, Cargo files, docs, workflows, automation scripts,
     scheduler config, existing handoff/log artifacts, or this task brief.

   **Required behavior**:
   - Render a selected-row affordance using egui-native row/button styling.
   - Keep the selected row visible when keyboard navigation moves through a
     long filtered list.
   - Do not introduce bitmap/screenshot tests or a new visual-test harness.
   - Do not add fuzzy matching, command history, a separate command model, or
     keybinding-editor behavior.

   **Verbatim review-gate strings** - copy these into the filed issue body:

   ```text
   MUST add a visible selected-row affordance for command-palette keyboard navigation
   MUST keep the selected row visible when navigating long command-palette result lists
   MUST use egui-native styling and avoid bitmap screenshot tests
   MUST NOT add fuzzy matching command history a separate command model or keybinding-editor behavior
   MUST NOT edit editor-ui editor-shell rge-editor Cargo docs workflows automation scripts scheduler config existing dispatch artifacts or this task brief
   ```

   **Verification required**:
   - `cargo +nightly fmt --all -- --check`.
   - `git diff --check`.
   - Static inspection of the selected-row render path plus any pure helper
     tests that are practical.

79. **[DONE 2026-06-07 via local commit `69676be`] Command palette keyboard-navigation documentation reconcile.**
   PR-mode docs-only reconciliation task after tasks 74-78 land. Record the
   shipped command-palette keyboard-navigation behavior in the live planning
   docs using the existing forward-only pattern.

   **Allowed file surface**:
   - MAY edit `plans/BASELINE.md`.
   - MAY edit `Status.md`.
   - MAY edit `HANDOFF.md`.
   - MAY edit `change.md`.
   - MAY add this dispatch's own handoff packets, sidecars, queue log, and
     ignored `.ai/dispatch-*` scratch.
   - MUST NOT edit Rust source, tests, Cargo files, architecture lints, ADRs,
     workflows, automation scripts, scheduler config, existing handoff/log
     artifacts, or this task brief.

   **Required content**:
   - Add one forward-only subsection above the existing command-palette entries
     in `plans/BASELINE.md`.
   - State exactly what keyboard-navigation behavior exists after tasks 74-78.
   - Keep historical command-palette subsections byte-preserved below the new
     entry.
   - Preserve open non-goals: fuzzy matching, command history, separate command
     model, plugin runtime/action execution, keybinding editor, and host-shell
     FIFO replacement unless a prior task explicitly closed one of them.

   **Verbatim review-gate strings** - copy these into the filed issue body:

   ```text
   MUST add exactly one forward-only command-palette keyboard-navigation subsection to plans/BASELINE.md
   MUST preserve older command-palette history byte-identical below the new subsection
   MUST update Status HANDOFF and change consistently with the new current state
   MUST preserve fuzzy matching command history separate command model plugin runtime keybinding editor and host-shell FIFO replacement as open non-goals unless already closed by a prior task
   MUST NOT edit Rust source tests Cargo lints ADRs workflows automation scripts scheduler config existing dispatch artifacts or this task brief
   ```

   **Verification required**:
   - `git diff --check`.
   - Static inspection confirming only allowed docs and this dispatch's own
     generated artifacts changed.

80. **[DONE 2026-06-07 via local commit `6203e2c`] Command palette filter-edit selection reset.**
   PR-mode editor-usability polish task. Tighten command-palette keyboard
   selection after search edits so a selected numeric row index from one
   filtered result set is not preserved against different rows in the next
   filtered result set.

   **Allowed file surface**:
   - MAY edit `crates/editor-egui-host/src/menu.rs`.
   - MAY edit `crates/editor-egui-host/src/menu_tests.rs`.
   - MAY edit `plans/BASELINE.md`.
   - MAY edit `Status.md`.
   - MAY edit `HANDOFF.md`.
   - MAY edit `change.md`.
   - MAY add this dispatch's own handoff packets, sidecars, queue log, and
     ignored `.ai/dispatch-*` scratch.
   - MUST NOT edit `crates/editor-ui/**`, `crates/editor-shell/**`,
     `editor/rge-editor/**`, Cargo files, architecture lints, ADRs, workflows,
     automation scripts, scheduler config, or existing handoff/log artifacts.

   **Required behavior**:
   - When the command-palette search filter changes, selected-row state MUST
     restart at the first enabled row in the new filtered result set.
   - Non-filter frames MUST preserve a still-valid enabled selected row.
   - Disabled rows MUST remain visible but ineligible as keyboard targets.
   - Keep command execution on the existing `MenuCommandHandoff` path.
   - Do not add fuzzy matching, command history, a separate command model,
     plugin runtime/action execution, host-shell FIFO replacement, keybinding
     editor, or generalized conflict UI.

   **Verification required**:
   - `cargo +nightly fmt --all -- --check`.
   - `cargo check -p rge-editor-egui-host --lib`.
   - `cargo test -p rge-editor-egui-host --lib`.
   - `cargo run -q -p rge-tool-architecture-lints -- all`.
   - `git diff --check`.

81. **[DONE 2026-06-07 via auto-published commit `58ec48a`] Full-automation first-batch readiness reconcile.**
   Docs-only delegated-human auto-publish smoke task. Record the current
   post-command-palette and automation-readiness state after tasks 74-80, so
   the first guarded `-PublishMode main` batch stays on the lower-risk docs
   surface while proving the autonomous selector / queue / guard path can run
   against a real task.

   **Allowed file surface**:
   - MAY edit `Status.md`.
   - MAY edit `HANDOFF.md`.
   - MAY edit `change.md`.
   - MAY edit `plans/BASELINE.md` only if needed to clarify the command-palette
     or full-automation posture.
   - MAY add this dispatch's own handoff packets, sidecars, queue log, and
     ignored `.ai/dispatch-*` scratch.
   - MUST NOT edit Rust source, tests, Cargo files, architecture lints, ADRs,
     workflows, automation scripts, scheduler config, `.ai/dispatch.tasks.md`,
     existing handoff/log artifacts, or registered Windows Scheduled Tasks.

   **Required content**:
   - State that command-palette keyboard navigation tasks 74-80 are complete
     on `main`, including filter-edit selection reset.
   - State that issue #319 was manually salvaged and closed after the work
     landed, so it is no longer an open autonomous failure blocker.
   - State that the next autonomous batch is intentionally bounded to one
     docs-only task under delegated-human `-PublishMode main` authorization.
   - Preserve open non-goals: fuzzy matching/scoring, command history, separate
     command model, plugin runtime/action execution, host-shell FIFO replacement,
     keybinding editor, generalized conflict UI, scheduler registration, and
     standing/default `-PublishMode main` authorization.
   - Do not claim scheduler registration or indefinite automation.

   **Verbatim review-gate strings** - copy these into the filed issue body:

   ```text
   MUST keep this first full-automation batch docs-only
   MUST state tasks 74-80 are complete on main including filter-edit selection reset
   MUST state issue 319 was manually salvaged and is no longer an open autonomous failure blocker
   MUST preserve scheduler registration and standing PublishMode main authorization as non-goals
   MUST NOT edit Rust source tests Cargo lints ADRs workflows automation scripts scheduler config .ai/dispatch.tasks.md existing dispatch artifacts or registered Windows Scheduled Tasks
   ```

   **Verification required**:
   - `git diff --check`.
   - Static inspection confirming only allowed docs and this dispatch's own
     generated artifacts changed.

82. **[DONE 2026-06-07 via manual harness-first task] Phase 9 compile timing harness.**
   Add the non-destructive compile timing harness named by the §13.3 baseline
   deferral before any clean-build cache wipe or 1-line incremental p95
   measurement.

   **Allowed file surface**:
   - MAY add `tools/compile-timing.ps1`.
   - MAY edit `plans/BASELINE.md`.
   - MAY edit `Status.md`.
   - MAY edit `HANDOFF.md`.
   - MAY edit `change.md`.
   - MAY edit `.ai/dispatch.tasks.md` only to record this completed manual task.
   - MUST NOT edit Rust source, tests, Cargo files, architecture lints, ADRs,
     workflows, scheduler config, dispatch automation behavior, or target-cache
     contents.

   **Required content**:
   - The harness MUST measure warm-cache workspace `cargo check` and/or
     `cargo build` wall time.
   - The harness MUST use the shared `A:\RustCache` cargo/rustup/target cache
     when present and unset.
   - The harness MUST NOT expose target deletion or `cargo clean`.
   - Docs MUST keep true clean-build certification and 1-line incremental p95
     open as separate explicitly authorized tasks.

   **Verification completed**:
   - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\tools\compile-timing.ps1 -Mode check -Iterations 1 -TimeoutSeconds 30`.
   - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\tools\compile-timing.ps1 -Mode build -Iterations 1 -TimeoutSeconds 120`.
   - `git diff --check`.

83. **[DONE 2026-06-07 via manual isolated-target measurement] Phase 9 clean release build measurement.**
   Measure the §13.3 true clean release build budget without wiping the shared
   `A:\RustCache\target` cache.

   **Allowed file surface**:
   - MAY edit `plans/BASELINE.md`.
   - MAY edit `Status.md`.
   - MAY edit `HANDOFF.md`.
   - MAY edit `change.md`.
   - MAY edit `.ai/dispatch.tasks.md` only to record this completed manual task.
   - MAY create and remove an isolated scratch target under `B:\sdk`.
   - MUST NOT edit Rust source, tests, Cargo files, architecture lints, ADRs,
     workflows, scheduler config, dispatch automation behavior, or shared
     `A:\RustCache\target` contents.
   - MUST NOT run `cargo clean`.

   **Required content**:
   - Record `cargo build --workspace --release` from a fresh isolated target.
   - State clearly whether the measurement passes or misses the §13.3 ≤120s
     clean-build budget.
   - Keep clean-build remediation/remeasurement and 1-line incremental p95 open
     if the budget is not passed.

   **Verification completed**:
   - `powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\tools\compile-timing.ps1 -Mode build -Release -Iterations 1 -TimeoutSeconds 1200` with `CARGO_TARGET_DIR=B:\sdk\rge-clean-target-20260607-1855`.
   - Verified and removed isolated scratch target `B:\sdk\rge-clean-target-20260607-1855`.
   - `git diff --check`.

84. **[DONE 2026-06-07 via manual guarded-loop task] Guarded multi-tick auto task selection.**
   Let a guarded full-automation run select the next best task before each
   sequential Auto tick without collapsing multiple queue dispatches into one
   queue run.

   **Allowed file surface**:
   - MAY edit `Invoke-AiDispatchGuard.ps1`.
   - MAY edit `Invoke-AiDispatchAuto.ps1`.
   - MAY edit `tools/dispatch-tests/GuardSafetyMonitor.Tests.ps1`.
   - MAY edit `AI_DISPATCH_AUTOMATION.md`.
   - MAY edit `Status.md`.
   - MAY edit `HANDOFF.md`.
   - MAY edit `change.md`.
   - MAY edit `.ai/dispatch.tasks.md` only to record this completed manual task.
   - MUST NOT edit Rust source, tests, Cargo files, architecture lints, ADRs,
     workflows, scheduler config, queue publish behavior, verification gates,
     or registered Windows Scheduled Tasks.

   **Required behavior**:
   - Guard default behavior MUST remain one Auto tick.
   - A finite opt-in guard parameter MUST allow sequential Auto ticks.
   - Each tick MUST launch a fresh `Invoke-AiDispatchAuto.ps1` invocation so
     Codex re-reads current task and issue state before selecting.
   - Each tick MUST still drain at most one queue issue through the existing
     queue boundary.
   - The guarded sequence MUST stop early on
     cap/no-work/ambiguous/lock/halt-sentinel/failed-issue states.

   **Verification completed**:
   - PowerShell parser validation for `Invoke-AiDispatchGuard.ps1`.
   - PowerShell parser validation for `Invoke-AiDispatchAuto.ps1`.
   - `Invoke-Pester -Path .\tools\dispatch-tests\GuardSafetyMonitor.Tests.ps1 -Output Detailed` (43/43).
   - `Invoke-Pester -Path .\tools\dispatch-tests -Output Normal` (399/399).
   - `git diff --check`.

85. **[DONE 2026-06-07 via ISSUE-321 dispatch — p95 1.507s PASS vs ≤10s] Phase 9 one-line incremental p95 build measurement.**
   Measure the still-open PLAN section 13.3 incremental p95 budget using the
   existing compile timing harness, without leaving a source edit in the final
   diff.

   **Allowed file surface**:
   - MAY edit `plans/BASELINE.md`.
   - MAY edit `Status.md`.
   - MAY edit `HANDOFF.md`.
   - MAY edit `change.md`.
   - MAY edit `.ai/dispatch.tasks.md` only to mark this task done.
   - MAY temporarily edit exactly one low-risk leaf Rust source file for the
     measurement, but the final committed diff MUST NOT include that temporary
     source edit.
   - MAY create and remove scratch measurement output under `B:\sdk`.
   - MUST NOT edit Cargo manifests, architecture lints, workflows, scheduler
     config, dispatch automation scripts, or production behavior.
   - MUST NOT run `cargo clean` or delete the shared `A:\RustCache\target`.

   **Required behavior**:
   - Use `tools/compile-timing.ps1` for the measurement.
   - Measure `cargo build` after a one-line source change, with enough samples
     to report p95 or explain why a smaller bounded sample was used.
   - Revert the temporary source touch before committing.
   - Record whether the result passes or misses the section 13.3 <=10s
     incremental p95 budget.

   **Verification required**:
   - The measurement command(s) must exit 0.
   - `git diff --check`.
   - `git status --short --untracked-files=no` must show only the intended
     docs/task record changes before commit.

86. **[DONE 2026-06-07 via ISSUE-322 dispatch — `cranelift-codegen` 125.64s single-unit long pole = 85% of the 147.82s clean MISS; from `wasmtime` via `rge-expr-wasm`/`rge-runtime-wasmtime-engine`] Phase 9 clean release build hotspot attribution.**
   Turn the measured clean release build miss into an actionable remediation
   plan by attributing the largest compile-time costs without changing source
   behavior yet.

   **Allowed file surface**:
   - MAY edit `plans/BASELINE.md`.
   - MAY edit `Status.md`.
   - MAY edit `HANDOFF.md`.
   - MAY edit `change.md`.
   - MAY edit `.ai/dispatch.tasks.md` only to mark this task done.
   - MAY create and remove an isolated scratch target under `B:\sdk`.
   - MAY write throw-away timing artifacts under `.ai/` or `B:\sdk`; do not
     commit them unless an existing documented convention requires it.
   - MUST NOT edit Rust source, tests, Cargo manifests, architecture lints,
     workflows, scheduler config, dispatch automation scripts, or shared
     `A:\RustCache\target` contents.
   - MUST NOT run `cargo clean`.

   **Required behavior**:
   - Reuse `tools/compile-timing.ps1` and/or Cargo-supported timing output to
     identify the dominant clean release build cost drivers.
   - Preserve the existing 156.591s miss as the current certified measurement
     unless a fresh isolated-target remeasurement is actually run.
   - Record the smallest next remediation candidates, with expected risk and
     why each is or is not suitable for automation.

   **Verification required**:
   - Any measurement command(s) used must exit 0.
   - `git diff --check`.

87. **[DONE 2026-06-08 via ISSUE-323 dispatch — docs-only; `crates/io-3mf` confirmed present as `rge-io-3mf` stub, stale "entirely missing" wording superseded in Status.md/HANDOFF.md/change.md] Reconcile `io-3mf` plan/status drift and stub boundary.**
   Current source contains `crates/io-3mf`, while older status text still says
   the crate is entirely missing. Reconcile the docs and make the remaining
   3MF work precise without pretending an importer exists.

   **Allowed file surface**:
   - MAY edit `plans/BASELINE.md`.
   - MAY edit `Status.md`.
   - MAY edit `HANDOFF.md`.
   - MAY edit `change.md`.
   - MAY edit `.ai/dispatch.tasks.md` only to mark this task done.
   - MAY edit `crates/io-3mf/src/lib.rs` only for documentation/stub-boundary
     clarity if needed.
   - MUST NOT implement a 3MF parser/exporter.
   - MUST NOT add dependencies, edit Cargo manifests, workflows, architecture
     lints, scheduler config, or dispatch automation scripts.

   **Required behavior**:
   - State that `crates/io-3mf` exists in the workspace.
   - State that it is still a stub and the real format-handler implementation
     remains deferred until format-handler pressure appears.
   - Remove or supersede stale "entirely missing" claims via forward-only
     snapshot style.

   **Verification required**:
   - `cargo check -p rge-io-3mf` if any Rust file is changed.
   - `git diff --check`.

88. **[DONE 2026-06-08 via ISSUE-324 salvage PR #325 - `restore_from_snapshot_with_diagnostics` merged at `95f2c25`; unregistered snapshot components now emit structured `SnapshotRecoverable` warnings through `DiagnosticSink` while `restore_from_snapshot` preserves its signature/behavior] Route ECS snapshot restore skip warnings through diagnostics.**
   Narrow the persistent kernel/ecs snapshot warning-routing gap by adding a
   diagnostics-aware restore path while preserving the existing simple
   `restore_from_snapshot` API.

   **Allowed file surface**:
   - MAY edit `kernel/ecs/src/**`.
   - MAY edit `kernel/ecs/tests/**`.
   - MAY edit `kernel/ecs/Cargo.toml` only if a diagnostics dependency is
     required and allowed by existing architecture rules.
   - MAY edit `plans/BASELINE.md`, `Status.md`, `HANDOFF.md`, and `change.md`.
   - MAY edit `.ai/dispatch.tasks.md` only to mark this task done.
   - MUST NOT edit unrelated crates, workflows, scheduler config, dispatch
     automation scripts, or architecture-lint policy.

   **Required behavior**:
   - Preserve `World::restore_from_snapshot(&mut self, bytes)` behavior and
     signature for existing callers.
   - Add a bounded diagnostics-aware path for unregistered snapshot components
     so tests can assert structured warning emission.
   - Keep malformed snapshot errors as `SnapshotError`; do not turn snapshot
     parsing failures into diagnostics-only behavior.

   **Verification required**:
   - `cargo test -p rge-kernel-ecs`.
   - `cargo +nightly fmt --all -- --check`.
   - `git diff --check`.

89. **[DONE 2026-06-08 via ISSUE-327 dispatch — docs-only; documentation drift confirmed: `rge-physics` already depends on `rge-kernel-diagnostics` (`Cargo.toml:44`) and plugin contract violations auto-emit as `Severity::Warning`, stale "no kernel/diagnostics integration" wording superseded in Status.md/HANDOFF.md/change.md while preserving the `PhysicsInputLedger` domain-ledger boundary] Physics diagnostics integration reconciliation.**
   Re-read the current physics diagnostics posture and either close stale
   status text or perform one small diagnostics integration improvement that is
   justified by current source.

   **Allowed file surface**:
   - MAY edit `crates/physics/src/**`.
   - MAY edit `crates/physics/tests/**`.
   - MAY edit `plans/BASELINE.md`, `Status.md`, `HANDOFF.md`, and `change.md`.
   - MAY edit `.ai/dispatch.tasks.md` only to mark this task done.
   - MUST NOT edit unrelated crates, Cargo manifests, workflows, scheduler
     config, dispatch automation scripts, or architecture-lint policy.

   **Required behavior**:
   - Inspect `crates/physics` before changing code; if existing
     `rge-kernel-diagnostics` integration already covers the old gap, prefer a
     docs/status reconciliation over source churn.
   - If source changes are justified, keep them narrowly focused on diagnostics
     emission or test coverage; do not redesign `PhysicsInputLedger`.
   - Preserve the existing documented boundary that `PhysicsInputLedger` is a
     domain ledger, not a replacement for `kernel/audit-ledger`.

   **Verification required**:
   - `cargo test -p rge-physics` if physics source/tests change.
   - `cargo +nightly fmt --all -- --check` if Rust files change.
   - `git diff --check`.

90. **[DONE-BLOCKED 2026-06-08 via ISSUE-329 failed experiment - `opt-level = 1` worsened clean release to `178.4s` total / Cargo `2m 58s`, with `cranelift-codegen` still the critical-path tail at `148.38s`; override reverted, clean-build gate remains MISS] Phase 9 clean release `cranelift-codegen` package opt-level experiment.**
   Execute remediation candidate A from the ISSUE-322 clean-release hotspot
   attribution: test whether lowering only the release profile for
   `cranelift-codegen` removes the 125.64s critical-path long pole enough to
   satisfy the PLAN Section 13.3 clean-build budget, without changing source
   behavior or widening the wasm runtime dependency graph.

   **Allowed file surface**:
   - MAY edit root `Cargo.toml` only to add a minimal
     `[profile.release.package."cranelift-codegen"]` override.
   - MAY edit `plans/BASELINE.md`, `Status.md`, `HANDOFF.md`, and `change.md`
     to record the experiment result.
   - MAY edit `.ai/dispatch.tasks.md` only to mark this task done or
     done-blocked after the experiment is recorded.
   - MAY create and remove an isolated scratch target under `B:\sdk`.
   - MAY write throw-away timing artifacts under `.ai/` or `B:\sdk`; do not
     commit them unless an existing documented convention requires it.
   - MUST NOT edit Rust source/tests, dependency features, `Cargo.lock`,
     workflows, scheduler config, dispatch automation scripts, architecture
     lints, or shared `A:\RustCache\target` contents.
   - MUST NOT run `cargo clean`.

   **Required behavior**:
   - Prefer the least risky useful override first: `opt-level = 1` for
     `cranelift-codegen` under release package profiles. Do not change global
     release settings.
   - Re-run a true clean release build from a fresh isolated target under
     `B:\sdk` with Cargo `--timings` after the override.
   - Record the new wall time, Cargo "Finished" time, `--timings` total, and
     whether `cranelift-codegen` remains the critical-path tail.
   - Run a focused release-mode wasm/script smoke/perf check so an obvious
     Cranelift runtime-compiler regression is visible before keeping the
     profile override.
   - Keep the `Cargo.toml` override only if the clean release build improves
     materially and the focused wasm/script check remains within its documented
     assertions. If the experiment misses the budget or shows a catastrophic
     regression, revert the `Cargo.toml` change and record a docs-only failed
     experiment instead of landing a harmful profile change.

   **Verification required**:
   - `cargo build --workspace --release --timings` from a fresh isolated
     `B:\sdk` target.
   - `cargo test -p rge-script-bench --release --lib wasmtime_cranelift::tests -- --nocapture`.
   - `cargo +nightly fmt --all -- --check` if any manifest formatting command
     or Rust formatting-relevant change is made.
   - `git diff --check`.

91. **[DONE 2026-06-08 via ISSUE-331 manual salvage - guard monitor parser recovers exact ok/abort verdicts from malformed object responses while failing closed on suffix values such as `ok-bad`] Harden guarded automation monitor-response parsing.**
   Close the false-abort gap observed during ISSUE-329, where
   `Invoke-AiDispatchGuard.ps1` killed an otherwise scoped run after Claude's
   monitor response tripped `ConvertFrom-Json` with `Invalid JSON primitive:
   ok`. The guard already handles bare `ok` / `abort` and an unquoted verdict
   token; this task should make the parser tolerate a recognizable verdict even
   when another field, especially `reason`, is malformed.

   **Allowed file surface**:
   - MAY edit `Invoke-AiDispatchGuard.ps1`.
   - MAY edit `tools/dispatch-tests/GuardSafetyMonitor.Tests.ps1`.
   - MAY edit `AI_DISPATCH_AUTOMATION.md` only if a short operator-facing note
     is useful.
   - MAY edit `Status.md`, `HANDOFF.md`, and `change.md`.
   - MAY edit `.ai/dispatch.tasks.md` only to mark this task done.
   - MUST NOT edit Rust source/tests, Cargo manifests, workflows, scheduler
     config, queue/loop/auto publish semantics, architecture lints, or
     unrelated automation scripts.

   **Required behavior**:
   - Preserve strict JSON parsing for valid monitor responses.
   - Preserve existing fail-safe abort behavior for truly unrecognizable
     malformed monitor output.
   - Accept and normalize object-like responses that contain a recognizable
     `verdict` of `ok` or `abort` even if `ConvertFrom-Json` fails because a
     non-verdict field is malformed, such as a quoted verdict with an unquoted
     primitive `reason`.
   - Keep `abort` verdicts authoritative; do not accidentally convert malformed
     abort responses into ok.
   - Add focused Pester coverage for strict JSON ok/abort, bare ok/abort,
     unquoted verdict token, quoted verdict with malformed reason, and
     unrecognizable malformed output.

   **Verification required**:
   - Focused `Invoke-Pester` for `GuardSafetyMonitor.Tests.ps1`.
   - Full `Invoke-Pester -Path .\tools\dispatch-tests` if practical.
   - `git diff --check`.

92. **[DONE 2026-06-08 via ISSUE-333 dispatch - selected candidate C default package-set follow-up] Audit Pulley / wasm-stack feature-gate feasibility across all direct `wasmtime` dependents.**
   Convert the remaining PLAN Section 13.3 clean-release remediation options
   from high-level candidates into one concrete next implementation choice.
   Candidate A (`cranelift-codegen` package opt-level) has already been
   measured and rejected, so this task should inspect candidates B and C:
   Pulley-only Wasmtime configuration and feature-gating the default wasm
   scripting stack out of the default clean release build.

   **Allowed file surface**:
   - MAY edit `plans/BASELINE.md`, `Status.md`, `HANDOFF.md`, and
     `change.md` to record the audit result.
   - MAY edit `.ai/dispatch.tasks.md` only to mark this task done or
     done-blocked after the audit is recorded.
   - MAY create small gitignored notes under `.ai/dispatch-ISSUE-*` if useful
     for command output provenance.
   - MUST NOT edit Rust source/tests, `Cargo.toml`, `Cargo.lock`, workflows,
     scheduler config, dispatch automation scripts, architecture lints, or
     shared `A:\RustCache\target` contents.
   - MUST NOT run `cargo clean`.

   **Required behavior**:
   - Inspect all four direct `wasmtime` dependents before drawing a conclusion:
     `rge-expr-wasm`, `rge-runtime-wasmtime-engine`, `rge-script-host`, and
     `rge-script-bench`.
   - For the Pulley path, identify exactly which manifest feature changes would
     be required, whether the current source/API usage appears compatible with
     removing Cranelift/Winch, and which script/expr behavior or performance
     gates would need to be re-run before a real implementation could land.
   - For the default-build feature-gate path, identify exactly which workspace
     crates, tests, benches, binaries, and CI/check commands would be affected
     by excluding the wasm scripting stack from default `cargo build
     --workspace --release`.
   - Decide exactly one safest next implementation follow-up, or record
     `NEEDS_HUMAN` if neither path is safe for autonomous implementation.
   - Do not implement either path in this task. This is an audit and task
     selection pass only.

   **Verification required**:
   - `Select-String -Path crates/*/Cargo.toml -Pattern wasmtime` or equivalent
     command proving the four direct dependents considered by the audit.
   - `cargo tree -i wasmtime -e normal` and `cargo tree -i cranelift-codegen -e
     normal`, unless a command is unavailable; record any failure.
   - `git diff --check`.
   - Confirm the tracked diff contains no Rust source/test changes and no
     `Cargo.toml` / `Cargo.lock` changes.

93. **[DONE 2026-06-08 via ISSUE-335 dispatch - resolver-backed DefaultCleanRelease package set excludes the Wasmtime scripting stack and preserves explicit script-bench coverage] Implement explicit default clean-release package-set gate (candidate C from ISSUE-333).**
   Candidate C from the ISSUE-333 audit is now the next clean-release
   remediation. Implement a documented, machine-readable default release
   package set for the clean-release measurement path so the default release
   build excludes the wasm scripting stack while wasm/script coverage remains
   explicitly visible.

   **Allowed file surface**:
   - MAY edit `tools/compile-timing.ps1`.
   - MAY add one small helper under `tools/` if a separate package-set helper is
     cleaner than embedding the list directly in `compile-timing.ps1`.
   - MAY edit `.ai/dispatch.verify.ps1`, `.github/workflows/tests.yml`, and
     `.github/workflows/bench.yml` only if needed to keep default-package-set
     and explicit wasm/script opt-in coverage consistent.
   - MAY edit `plans/BASELINE.md`, `Status.md`, `HANDOFF.md`, and `change.md`.
   - MAY edit `.ai/dispatch.tasks.md` only to mark this task done or
     done-blocked after the result is recorded.
   - MAY create and remove an isolated scratch target under `B:\sdk`.
   - MUST NOT edit Rust source/tests, `Cargo.toml`, `Cargo.lock`, dependency
     feature flags, architecture lints, scheduler config, queue/loop publish
     semantics, or shared `A:\RustCache\target` contents.
   - MUST NOT run `cargo clean`.

   **Required behavior**:
   - Define a default release package set for clean-release measurements that
     excludes exactly these wasm scripting packages from the default release
     build: `rge-runtime-wasmtime`, `rge-runtime-wasmtime-engine`,
     `rge-script-host`, `rge-expr-wasm`, and `rge-script-bench`.
   - Make an explicit include/exclude decision for `rge-tool-wasm-bench`; record
     the rationale. It currently has an empty dependency table and does not pull
     `wasmtime`, but the wasm-named tool must not be left implicit.
   - Do not rely on Cargo workspace `default-members` as the only mechanism:
     `cargo build --workspace --release` still selects every workspace member.
     The clean-release command must become an explicit package-set build or a
     documented equivalent that actually excludes the wasm scripting packages.
   - Update the documented Phase 9 clean-release measurement command away from
     `cargo build --workspace --release` to the new default package-set command.
   - Preserve explicit opt-in coverage for the excluded wasm stack. If any
     verify or workflow command is narrowed from full workspace to the default
     package set, add explicit checks for the excluded packages and keep
     `cargo bench -p rge-script-bench --no-run` visible.
   - Record whether the new default package-set clean release measurement
     passes or misses the section 13.3 <=120s clean-build budget if a fresh
     isolated-target measurement is run. If the measurement cannot be run within
     this dispatch, record the exact command that must be run next and do not
     claim budget closure.

   **Verification required**:
   - PowerShell parser validation for every edited `.ps1` file.
   - A command-level proof that the default package set excludes the five wasm
     scripting packages and resolves every included package name against
     `cargo metadata --format-version 1 --no-deps`.
   - If the implementation adds or changes a dry-run/list mode, run it and show
     the generated `cargo build --release -p ...` command.
   - If a fresh isolated-target measurement is run, use a target under `B:\sdk`,
     do not wipe shared caches, and remove the scratch target after recording
     the result.
   - Run any newly added focused Pester tests.
   - `git diff --check`.

94. **[DONE 2026-06-08 via ISSUE-337 manual salvage - DefaultCleanRelease clean build 125.467s MISS vs <=120s] Measure default clean-release package-set build time.**
   Task 93 implemented the resolver-backed `DefaultCleanRelease` package set
   but intentionally did not run a fresh isolated-target measurement. Run the
   documented measurement now and record whether the default release package set
   closes the Phase 9 section 13.3 clean-build budget.

   **Allowed file surface**:
   - MAY edit `plans/BASELINE.md`, `Status.md`, `HANDOFF.md`, and `change.md`
     to record the measurement result.
   - MAY edit `.ai/dispatch.tasks.md` only to mark this task done or
     done-blocked after the result is recorded.
   - MAY create temporary measurement output under a gitignored `.ai/` folder
     if useful for command provenance.
   - MAY create and remove exactly one fresh isolated scratch target under
     `B:\sdk`.
   - MUST NOT edit Rust source/tests, Cargo manifests/lock, dependency
     features, `tools/compile-timing.ps1`,
     `tools/Resolve-CleanReleasePackageSet.ps1`, workflows, scheduler config,
     dispatch automation scripts, architecture lints, or shared
     `A:\RustCache\target` contents.
   - MUST NOT run `cargo clean`.

   **Current-state claims / falsification to include in the TASK packet**:
   - Claim: no completed fresh isolated `DefaultCleanRelease` measurement has
     been recorded yet.
     Falsifying search:
     `rg -n "DefaultCleanRelease|rge-clean-default|default clean-release" plans/BASELINE.md Status.md HANDOFF.md change.md .ai/dispatch.tasks.md ai_handoffs`
     -> current results show ISSUE-335 implementation / "not run" / "command
     to run next" evidence and this task authoring, but no completed
     PASS/MISS measurement row.
   - Claim: no finalized ISSUE-337 handoff packet exists on `main` before this
     retry.
     Falsifying search:
     `Get-ChildItem -LiteralPath ai_handoffs -Filter 'ISSUE-337*'`
     -> no files on `main` before this retry.
   - Do not include a bare claim such as "No prior executor gate exists for
     revision 0" unless it is backed by a concrete falsifying search. If retry
     artifacts exist only in archived local `.ai/dispatch-ISSUE-337*` scratch
     or `A:\rcad\dispatch-worktrees\ISSUE-337.attempt*`, treat them as failed
     attempt evidence, not as a completed measurement.

   **Required behavior**:
   - Use a fresh empty isolated `CARGO_TARGET_DIR` under `B:\sdk`; verify the
     resolved path is under `B:\sdk` before deleting it.
   - Run exactly this measurement shape, with only the scratch target suffix
     adjusted for the new dispatch if desired:

     ```
     $env:CARGO_TARGET_DIR = 'B:\sdk\rge-clean-default-ISSUE-335-next'
     powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\tools\compile-timing.ps1 -Mode build -Release -PackageSet DefaultCleanRelease -Iterations 1 -TimeoutSeconds 1200
     ```

   - Record wall time, Cargo `Finished` line, the generated package-set command
     shape, and whether the result passes or misses the <=120s clean-build
     budget.
   - Confirm the generated Cargo command is an explicit `cargo build --release
     -p ...` package list and does not contain `--workspace`.
   - Remove only the isolated scratch target after recording the result.
   - If the measurement fails or times out, record the failure exactly and mark
     the task done-blocked; do not invent a budget verdict.

   **Verification required**:
   - The measurement command exits 0, or the failure/timeout is recorded with
     command output and exit code.
   - Scratch target removal is verified after the result is recorded.
   - `git diff --check`.

95. **[DONE 2026-06-08 via ISSUE-338 manual salvage - DefaultCleanRelease `--timings` run 111.072s wall / 110.8s total; critical tail `rge-editor` 30.23s ending at 110.79s; budget verdict inconclusive vs task-94 125.467s MISS due variance] Attribute remaining DefaultCleanRelease clean-build hotspot.**
   Task 94 measured the resolver-backed `DefaultCleanRelease` clean build at
   125.467s, still a MISS vs the <=120s clean-build budget by 5.467s. Run one
   attribution-only follow-up with Cargo `--timings` for the same package set
   so the next remediation is evidence-based instead of guessing from the old
   full-workspace Cranelift hotspot.

   **Allowed file surface**:
   - MAY edit `plans/BASELINE.md`, `Status.md`, `HANDOFF.md`, and `change.md`
     to record the attribution result and selected next follow-up.
   - MAY edit `.ai/dispatch.tasks.md` only to mark this task done,
     done-blocked, or to record the selected next task after attribution.
   - MAY copy timing artifacts and derived JSON into this dispatch's gitignored
     `.ai/dispatch-ISSUE-*` run directory.
   - MAY create and remove exactly one fresh isolated scratch target under
     `B:\sdk`.
   - MUST NOT edit Rust source/tests, Cargo manifests/lock, dependency
     features, `tools/compile-timing.ps1`,
     `tools/Resolve-CleanReleasePackageSet.ps1`, workflows, scheduler config,
     dispatch automation scripts, architecture lints, or shared
     `A:\RustCache\target` contents.
   - MUST NOT run `cargo clean`.

   **Current-state claims / falsification to include in the TASK packet**:
   - Claim: no completed `DefaultCleanRelease` `--timings` hotspot attribution
     has been recorded yet.
     Falsifying search:
     `rg -n "DefaultCleanRelease.*timings|rge-clean-default-hotspots|cargo-timing.*DefaultCleanRelease|default clean-release.*hotspot" plans/BASELINE.md Status.md HANDOFF.md change.md .ai/dispatch.tasks.md ai_handoffs`
     -> no matches before this task authoring.
   - Claim: task 94 recorded only the plain measurement, not per-unit
     attribution.
     Falsifying search:
     `rg -n "Default clean-release measurement result|125\\.467s|Cargo \`Finished\`.*2m 05s" plans/BASELINE.md Status.md HANDOFF.md change.md .ai/dispatch.tasks.md`
     -> matches the task-94 measurement result but not a `--timings` unit table.

   **Required behavior**:
   - Use a fresh empty isolated `CARGO_TARGET_DIR` under `B:\sdk`; verify the
     resolved path is under `B:\sdk` before deleting it.
   - Resolve the package list with
     `.\tools\Resolve-CleanReleasePackageSet.ps1 -Output IncludedNames` or the
     equivalent resolver output.
   - Run an explicit package-list release build with Cargo `--timings`, shaped
     as `cargo build --release -p PACKAGE ... --timings`; do not use
     `--workspace`.
   - Record wall time, Cargo `Finished` line, Cargo timings total if available,
     the top compile units by duration, and the inferred current critical-tail
     unit from the timing data.
   - Preserve the timing HTML and extracted `UNIT_DATA` JSON under gitignored
     `.ai/dispatch-ISSUE-*` provenance before removing the scratch target.
   - Decide exactly one next follow-up after attribution: a bounded remediation
     task if the long pole is clear, a remeasurement/variance task if the result
     is inconclusive, or `NEEDS_HUMAN` if no safe autonomous follow-up exists.
   - Remove only the isolated scratch target after recording the result.

   **Verification required**:
   - The `--timings` build exits 0, or the failure/timeout is recorded with
     command output and exit code.
   - Confirm the generated command is an explicit `cargo build --release -p ...`
     package list and does not contain `--workspace`.
   - Scratch target removal is verified after artifacts are copied.
   - `git diff --check`.

   **Attribution result:** completed manually after the automated Codex executor
   could not create the required `B:\sdk` scratch target from its sandbox. The
   salvaged run used fresh isolated target
   `B:\sdk\rge-clean-default-hotspots-ISSUE-338`, resolved the same
   `DefaultCleanRelease` package set (92 included / 5 excluded, with
   `rge-tool-wasm-bench` included), and ran an explicit
   `cargo build --release -p ... --timings` command with no `--workspace`.
   Result: exit 0, wall **111.072s**, Cargo `Finished` **1m 50s**, Cargo
   timings total **110.8s**, 569 timing units. The timing HTML and extracted
   `UNIT_DATA` JSON were preserved under the gitignored
   `.ai/dispatch-ISSUE-338/default-clean-release-hotspots/` provenance before
   the scratch target was removed. Top duration units were `vello_cpu` 54.32s,
   `zstd-sys` build-script run 43.46s, `windows` 38.76s, `gltf-json` 38.66s,
   `naga` 36.29s, `egui` 32.09s, then workspace unit `rge-editor` 30.23s.
   Current critical tail was `rge-editor` bin, start 80.56s, duration 30.23s,
   end 110.79s; next workspace tails were `rge-tool-architecture-lints` bin
   ending at 107.13s and `rge-physics` ending at 104.94s. Because this fresh
   `--timings` run passes <=120s while task 94's plain measurement missed at
   125.467s, the selected next follow-up is a variance confirmation task, not
   immediate source remediation.

96. **[DONE 2026-06-08 via ISSUE-339 manual salvage - three fresh `DefaultCleanRelease` plain clean-build samples all passed <=120s: 109.273s / 115.303s / 108.280s; max 115.303s] Confirm DefaultCleanRelease clean-build variance before remediation.**
   The queued Codex executor reproduced the same sandbox limitation as task 95:
   it finalized a blocked EXEC packet after `New-Item` for
   `B:\sdk\rge-clean-default-variance-ISSUE-339-sample1` failed with
   `UnauthorizedAccessException`, before any build sample started. Manual
   salvage from the root shell then ran the task exactly against three fresh
   isolated targets:

   - sample 1: `B:\sdk\rge-clean-default-variance-ISSUE-339-sample1`, exit 0,
     wall **109.273s**, Cargo `Finished` **1m 48s**.
   - sample 2: `B:\sdk\rge-clean-default-variance-ISSUE-339-sample2`, exit 0,
     wall **115.303s**, Cargo `Finished` **1m 55s**.
   - sample 3: `B:\sdk\rge-clean-default-variance-ISSUE-339-sample3`, exit 0,
     wall **108.280s**, Cargo `Finished` **1m 48s**.

   Every generated command was an explicit
   `cargo build --release -p ...` package list with no `--workspace`; the
   resolver reported the same **92 included** / **5 excluded** package set and
   `rge-tool-wasm-bench` remained included. All three scratch targets were
   resolved under `B:\sdk`, then removed and verified absent. Provenance is
   retained under gitignored
   `.ai/dispatch-ISSUE-339/default-clean-variance/`, including per-sample
   stdout/stderr, JSON, and `variance-summary.json`.

   Result: the recorder-host `DefaultCleanRelease` clean-build gate is now a
   **provisional PASS** at max **115.303s** vs the <=120s budget. Do not start
   source or package-policy remediation from the earlier single 125.467s miss
   unless a future measurement regresses.

97. **[DONE 2026-06-08 via ISSUE-340 - explicit `-CodexExecutorExternalScratch` opt-in added for future `B:\sdk` measurement dispatches; no measurement run] Add an explicit Codex executor external-scratch option for `B:\sdk` measurement dispatches.**
   Tasks 95 and 96 both needed manual salvage because the queue runs Codex
   execution with a workspace sandbox that cannot create required
   `B:\sdk` scratch targets. Fix the automation substrate before selecting
   another build-measurement task that requires external scratch space.

   **Allowed file surface**:
   - MAY edit `Invoke-AiDispatchLoop.ps1`, `Invoke-AiDispatchQueue.ps1`,
     `Invoke-AiDispatchAuto.ps1`, `Invoke-AiDispatchGuard.ps1`,
     `Register-AiDispatchSchedule.ps1`, `AI_DISPATCH_AUTOMATION.md`,
     `tools/dispatch-tests/*.ps1`, `Status.md`, `HANDOFF.md`, `change.md`,
     and `.ai/dispatch.tasks.md`.
   - MAY add focused dispatch-test fixtures under `tools/dispatch-tests/`.
   - MUST NOT edit Rust source/tests, Cargo manifests/lock, workflows,
     architecture lints, package-set/compile-timing tooling, scheduler
     registration state, or publish-mode semantics.

   **Current-state claims / falsification to include in the TASK packet**:
   - Claim: Codex execution currently uses workspace sandboxing and cannot
     create external `B:\sdk` scratch directories for measurement tasks.
     Falsifying search:
     `git grep -n "codex exec.*sandbox\\|--sandbox workspace-write\\|ExecutorSandbox\\|B:\\\\sdk" Invoke-AiDispatchLoop.ps1 Invoke-AiDispatchQueue.ps1 Invoke-AiDispatchAuto.ps1 Invoke-AiDispatchGuard.ps1 Register-AiDispatchSchedule.ps1 AI_DISPATCH_AUTOMATION.md tools/dispatch-tests .ai/dispatch.tasks.md Status.md HANDOFF.md change.md`
     -> should show current Codex execution sandbox routing plus the ISSUE-338
     and ISSUE-339 manual-salvage notes, but no implemented explicit
     external-scratch executor option before this task runs.

   **Required behavior**:
   - Add a narrow, explicit operator-controlled way to run Codex execution with
     the filesystem access needed for `B:\sdk` scratch targets, while keeping
     the existing workspace-sandbox default.
   - Propagate the option through Loop/Queue/Auto/Guard/Scheduler entrypoints
     as needed so full automation can opt into it deliberately for measurement
     batches.
   - Keep publish behavior unchanged: no new auto-main default, no automatic
     commit/push from the inner loop, and no widening of queue publish gates.
   - Add focused tests that prove the default remains workspace-scoped and the
     explicit external-scratch mode changes only the Codex execution sandbox
     invocation/pass-through.
   - Update `AI_DISPATCH_AUTOMATION.md`, `Status.md`, `HANDOFF.md`,
     `change.md`, and this task list with the exact operator command to use
     for future `B:\sdk` measurement dispatches.

   **Verification required**:
   - Focused dispatch-test suite for the new option.
   - Full `tools/dispatch-tests` Pester suite if practical; otherwise record
     why not and run all tests touching edited scripts.
   - PowerShell parser validation for edited `.ps1` files.
   - `git diff --check`.

   **Implementation result:** ISSUE-340 added a Codex-only
   `-CodexExecutorExternalScratch` switch. The switch defaults off; when
   supplied with `-Executor codex`, only the Codex execution phase uses
   `danger-full-access` so TASK packets that explicitly authorize `B:\sdk`
   scratch targets can run under automation. Codex plan fill, Codex plan gate,
   optional preflight audit, correction-packet authoring, and Codex control
   review keep their existing `workspace-write` / `read-only` sandbox routing.
   Supplying the switch with `-Executor claude` fails fast before child work.
   Queue, Auto, Guard, and Scheduler append the switch to child argument
   vectors only when the operator supplies it; no publish-mode default or
   publish gate changed.

   Future `B:\sdk` measurement dispatches should be launched with:
   `.\Invoke-AiDispatchAuto.ps1 -PublishMode pr -MaxAutonomousTasks 1 -MaxPlanRevisions 1 -MaxCorrectionRounds 2 -Executor codex -CodexExecutorExternalScratch`

   ISSUE-340 did not run a new clean-build measurement, hotspot attribution,
   variance sample, or any command that creates or deletes a real `B:\sdk`
   target.

98. **[DONE 2026-06-08 via ISSUE-342 - command palette fuzzy scoring shipped] Add fuzzy matching/scoring to the command palette filter.**
   Completed via ISSUE-342 manual salvage after the guarded/full-automation
   route failed twice before execution in Codex plan-gate revision prompting.
   The `editor-egui-host` command-palette filter now adds deterministic
   ordered-subsequence fuzzy matching over label, shortcut display, and
   `Command::diagnostic_id()`, while preserving exact word/field, prefix, and
   substring matches ahead of fuzzy-only matches. The score key uses match
   class, fuzzy gap/span compactness, matched field priority, label length, and
   original menu order. Focused tests cover fuzzy label/shortcut/diagnostic-id
   matches, exact/prefix/substring outranking fuzzy-only matches, stable fuzzy
   ordering, and no-match behavior. No plugin runtime/discovery,
   `editor-shell` routing, `editor-ui` menu registry, keybinding editor, Cargo
   manifests/lock, workflows, scheduler config, architecture lints, or dispatch
   automation behavior changed.

99. **[DONE 2026-06-08 via manual automation hygiene] Bound plan-revision gate context.**
   The ISSUE-342 automation failure exposed that `Invoke-AiDispatchLoop.ps1`
   fed the entire prior executor plan-gate review back into Codex when
   `GATE_VERDICT: needs_changes` triggered a revision. Verbose gate output can
   exceed Codex's 1,048,576-character input ceiling before execution starts.
   The loop now caps only the prior gate-review prose to the last 20,000
   characters with an explicit truncation notice before injecting it into the
   next planner prompt. The task goal, TASK packet, gate verdict semantics,
   same-phase retry behavior, execution, control review, queue publish modes,
   scheduler config, and sandbox routing are unchanged.

100. **[DONE 2026-06-08 via ISSUE-345 - host-local recent ordering shipped] Add host-local recent-command ordering to the command palette.**
   Implement the smallest command-history slice for the existing
   `editor-egui-host` command palette. The history is host-local, in-memory,
   and derived from the current projected menu entries; it must not introduce a
   second command model or persistent storage.

   Completed via ISSUE-345. `EguiHost` now records successful command-palette
   activations by `Command::diagnostic_id()` into a host-local, in-memory,
   most-recent-first list capped at 16 ids. Blank or whitespace-only filters
   promote currently projected, enabled recent commands first, then append all
   remaining projected rows in their existing order. Stale ids are ignored,
   disabled recent rows are not promoted, and non-blank task-98 fuzzy scoring is
   unchanged.

   **Scope / MAY edit:**
   - `crates/editor-egui-host/src/lib.rs`
   - `crates/editor-egui-host/src/menu.rs`
   - `crates/editor-egui-host/src/menu_tests.rs`
   - `plans/BASELINE.md`
   - `Status.md`
   - `HANDOFF.md`
   - `change.md`
   - `.ai/dispatch.tasks.md`

   **MUST NOT edit:** `editor-shell`, `editor-ui`, plugin runtime/discovery,
   Cargo manifests/lockfiles, workflows, scheduler/dispatch automation scripts,
   architecture-lint rules/config, or any generated handoff sidecars.

   **Done-criterion:** `EguiHost` records successful command-palette
   activations by stable `Command::diagnostic_id()` into a bounded
   most-recent-first list (capacity 16), de-duplicates by moving an
   existing id to the front, and uses only the current
   `command_palette_entries(&main_menu)` projection to render/activate entries.
   Blank-filter palette ordering should place currently available recent
   enabled commands first, in most-recent order, then all remaining entries in
   their existing projected order. Non-blank fuzzy filtering/scoring from task
   98 must remain unchanged. Stale history ids not present in the current
   projection are ignored, not rendered as phantom commands.

   **Verification:** focused command-palette host/menu tests proving bounded
   de-duplication, stale-id ignoring, blank-filter recent ordering, and
   non-blank fuzzy ordering unchanged; `cargo +nightly fmt --all -- --check`;
   `cargo test -p rge-editor-egui-host --lib`; `cargo check -p
   rge-editor-egui-host --lib`; `git diff --check`.

101. **[DONE 2026-06-08 via ISSUE-347 - selected task 102] Phase 9 editor-usability next-task selection audit.**
   Queue is empty after task 100. Run a docs/source-read selection pass and
   choose exactly one bounded Phase 9/editor-usability implementation follow-up
   for task 102, or record `NEEDS_HUMAN` if the evidence does not support a
   safe bounded task.

   **Purpose:** inspect the current plan/status docs and current source before
   deciding the next automation item. Do not infer the next task from stale
   summary text alone. Use `plans/BASELINE.md`, `Status.md`, `HANDOFF.md`,
   `change.md`, `.ai/dispatch.tasks.md`, and source searches under the relevant
   editor/plugin crates to prove what is actually open.

   **Scope / MAY edit:**
   - `plans/BASELINE.md`
   - `Status.md`
   - `HANDOFF.md`
   - `change.md`
   - `.ai/dispatch.tasks.md`
   - generated ISSUE-101 handoff/audit/log artifacts for this dispatch only

   **MAY read/search:** `crates/editor-*`, `editor/`, `crates/plugin-*`,
   `plugins/`, `crates/cad-*`, `crates/kernel-*`, and other source paths needed
   to falsify current claims with `rg` / `git grep`. Reads/searches are allowed;
   source edits are not.

   **MUST NOT edit:** Rust source/tests, Cargo manifests/lockfiles, workflows,
   scheduler/dispatch automation scripts, architecture-lint rules/config,
   schemas, plugin runtime/discovery/loading code, existing handoff/log
   artifacts from other dispatches, or unrelated generated scratch files.

   **Audit requirements:**
   - Record the remaining Phase 9/editor-usability gaps using current source
     evidence, not just historical docs.
   - Include falsifying searches for stale or risky claims.
   - Compare at least these candidate classes:
     1. plugin/extension command execution policy beyond command capture,
     2. host-shell FIFO/menu-click replacement and generalized registry
        execution,
     3. conflict resolution, keybinding editor, and fatal gating,
     4. persistent command-palette history/favorites,
     5. unsaved-changes prompt and graceful quit,
     6. OS clipboard / typed clipboard,
     7. authoritative CAD deletion/duplication/undo integration,
     8. broader camera UI.
   - Select exactly one next implementation task and append it as task 102 with
     explicit `MAY edit`, `MUST NOT edit`, done criteria, and verification.
     If no candidate can be safely bounded from evidence, append no
     implementation task and record `NEEDS_HUMAN`.

   **Strong suggested prior:** plugin/extension command execution policy beyond
   command capture is likely the best next item because `Command::Custom` and
   `Command::Plugin` have recent capture/queue work but may still lack a
   bounded execution/routing policy. The audit must verify that premise and may
   choose a different candidate if the evidence is stronger.

   **Verification:** `git diff --check`; final
   `git status --short --untracked-files=all`; recorded source/doc search
   commands and results sufficient to support the selected task. No Rust build
   or test is expected unless the audit changes a verifier/tooling file, which
   this task should not do.

   **Halt condition:** if selecting task 102 would require implementation edits
   during task 101, or current docs/source contradict the premise enough that a
   bounded follow-up cannot be defended, record `NEEDS_HUMAN` with evidence
   instead of manufacturing an unsafe task.

102. **[DONE 2026-06-08 via ISSUE-349 - editor-shell injected handler seam only; no real plugin runtime/discovery/loading] Add an editor-shell extension-command executor seam.**
   Implement the smallest execution-policy step beyond the existing extension
   command capture FIFO. Current source shows `Command::Custom` and
   `Command::Plugin` menu activations already reach
   `EditorShell::route_menu_command`, which captures them into
   `extension_menu_commands`; task 102 should add the shell-owned seam that can
   execute those captured commands through an injected handler, without wiring
   real plugin runtime/discovery/loading.

   **Scope / MAY edit:**
   - `crates/editor-shell/src/lifecycle/mod.rs`
   - `crates/editor-shell/src/lifecycle/extension_command.rs` (new, optional)
   - `crates/editor-shell/src/render_path.rs`
   - `crates/editor-shell/src/lifecycle/tests.rs`
   - `plans/BASELINE.md`
   - `Status.md`
   - `HANDOFF.md`
   - `change.md`
   - `.ai/dispatch.tasks.md`
   - generated ISSUE-102 handoff/audit/log artifacts for this dispatch only

   **MUST NOT edit:**
   - `crates/editor-ui/**`
   - `crates/editor-egui-host/**`
   - `kernel/plugin-host/**`
   - `crates/plugin-*`
   - `plugins/**`
   - `runtime/**`
   - Rust source outside the `crates/editor-shell/**` files listed above
   - Cargo manifests or lockfiles
   - workflows, dispatch automation scripts, schemas, architecture-lint
     rules/config, scheduler config, or unrelated generated artifacts

   **Current-state claims / falsification to include in the TASK packet:**
   - Claim: the current public route-menu command handler is
     `EditorShell::route_menu_command`.
     Falsifying searches:
     `git grep -n "pub fn route_menu_command" -- crates/editor-* editor/`
     -> exactly one definition in `crates/editor-shell/src/render_path.rs`;
     `git grep -n -E "impl EditorShell|impl EditorState|route_menu_command" -- crates/editor-shell/src/render_path.rs`
     -> the definition is inside `impl EditorShell`, with no `impl
     EditorState` match in that file.
   - Claim: extension commands are currently captured but not executed.
     Falsifying search:
     `git grep -n -E "drain_extension_menu_commands|extension_menu_commands|future plugin/action executor|extension menu command captured" -- crates/editor-shell/src/lifecycle/mod.rs crates/editor-shell/src/render_path.rs crates/editor-shell/src/lifecycle/tests.rs`
     -> current FIFO field, one-shot drain, capture log, and retention test
     are present.
   - Claim: editor-shell currently has no plugin runtime/discovery/loading path
     to wire safely in this task.
     Falsifying search:
     `git grep -n -E "PluginHost|PluginContext|runtime-wasmtime|plugin-discovery|rge_kernel_plugin_host|rge-runtime" -- crates/editor-shell editor/rge-editor`
     -> no matches; exit 1 is the expected no-match result.
   - Claim: the previous failed route-menu owner defect must not recur.
     Falsifying search before and after edits: run the stale-symbol grep named
     in the ISSUE-347 TASK packet against `.ai/dispatch.tasks.md`,
     `plans/BASELINE.md`, `Status.md`, `HANDOFF.md`, and `change.md`; no
     matches are expected, and exit 1 is the expected no-match result.

   **Required behavior:**
   - Add a small editor-shell-owned extension-command executor seam for
     commands already captured by `EditorShell::route_menu_command`.
   - The seam must accept only extension commands (`Command::Custom` and
     `Command::Plugin`) and must never receive core commands.
   - Keep the existing capture boundary clear: core commands stay routed by
     `EditorShell::route_menu_command`; extension commands are captured first,
     then drained to the executor seam in FIFO order when a handler is
     configured.
   - Missing-handler behavior must be explicit and non-fatal. It may preserve
     the existing observable FIFO/drain behavior, but it must not silently drop
     extension commands.
   - Handler failure or unhandled results must be non-fatal, must not run
     document handlers, and must continue processing later extension commands
     unless the task discovers an existing local pattern that strongly dictates
     otherwise.
   - Tests must use an injected/synthetic handler. Do not introduce a real
     plugin host, plugin discovery, plugin loading, WASM runtime, capability
     manifest, async executor, or sandbox integration.

   **Done criteria:**
   - A configured synthetic handler receives `Command::Plugin` and
     `Command::Custom` activations in FIFO order after menu routing.
   - Core commands are not delivered to the extension executor seam.
   - No-handler behavior is covered and keeps extension activations observable
     rather than silently dropping them.
   - Failure/unhandled behavior is covered and does not prevent later extension
     commands from being processed.
   - Existing document/menu handlers still behave unchanged for representative
     core commands such as Save and Toggle Command Palette.
   - The docs/task bookkeeping records that this task adds only the
     editor-shell execution seam and does not wire real plugin runtime,
     discovery, or loading.

   **Verification required:**
   - Focused editor-shell tests covering extension-command executor FIFO,
     no-handler behavior, unhandled/failure behavior, and core-command
     non-delivery.
   - `cargo +nightly fmt --all -- --check`
   - `cargo test -p rge-editor-shell --lib`
   - `cargo check -p rge-editor-shell --lib`
   - `git diff --check`

   **Halt conditions:**
   - The implementation requires editing plugin runtime/discovery/loading,
     `kernel/plugin-host`, `runtime/**`, `plugins/**`, or Cargo metadata.
   - The implementation requires changing `crates/editor-ui` command variants,
     `crates/editor-egui-host` menu projection/registration behavior, host to
     shell FIFO semantics, generalized registry execution, keybinding editor
     behavior, conflict fatality policy, OS clipboard, typed clipboard, CAD
     graph/projection mutation, or undo/dirty integration.
   - The executor cannot state the current route-menu owner as
     `EditorShell::route_menu_command` with confidence.
   - The final docs/task brief names the stale editor-state route-menu owner
     instead of `EditorShell::route_menu_command`.

103. **[DONE 2026-06-08 via ISSUE-351 - selected task 104 command-palette recent-history persistence; no source edits] Phase 9 editor-usability next-task selection audit after extension seam.**
   Queue is empty after task 102. Run a docs/source-read selection pass and
   choose exactly one bounded Phase 9/editor-usability implementation follow-up
   for task 104, or record `NEEDS_HUMAN` if the evidence does not support a
   safe task.

   **Purpose:** keep full automation moving without guessing from stale
   history. Task 102 closed only the editor-shell extension-command executor
   seam; it explicitly did not wire real plugin runtime/discovery/loading,
   host FIFO replacement, generalized registry execution, keybinding editor,
   conflict fatality policy, OS/typed clipboard, CAD mutation, or broader
   camera UI. Re-evaluate the current source and docs before deciding the next
   automation item.

   **MAY edit:**
   - `.ai/dispatch.tasks.md`
   - `plans/BASELINE.md`
   - `Status.md`
   - `HANDOFF.md`
   - `change.md`
   - generated ISSUE-103 handoff/audit/log artifacts for this dispatch only

   **MAY read/search:** `crates/editor-*`, `editor/`, `crates/plugin-*`,
   `plugins/`, `kernel/plugin-host`, `runtime/`, `crates/cad-*`,
   `crates/kernel-*`, docs/status/task files, and other source paths needed to
   falsify current claims with `rg` / `git grep`. Reads/searches are allowed;
   source edits are not.

   **MUST NOT edit:** Rust source/tests, Cargo manifests/lockfiles, workflows,
   scheduler/dispatch automation scripts, architecture-lint rules/config,
   schemas, plugin runtime/discovery/loading code, existing handoff/log
   artifacts from other dispatches, or unrelated generated scratch files.

   **Audit requirements:**
   - Record the remaining Phase 9/editor-usability gaps using current source
     evidence, not only historical docs.
   - Include falsifying searches for stale or risky claims.
   - Confirm task 102's closure boundary: extension commands now have an
     injected editor-shell handler seam, but no real plugin runtime,
     discovery, loading, host FIFO replacement, generalized registry
     execution, keybinding editor, clipboard, CAD mutation, or camera UI work
     was added.
   - Compare at least these candidate classes:
     1. host-shell FIFO/menu-click replacement and generalized registry
        execution,
     2. conflict resolution, keybinding editor, and fatal gating,
     3. persistent command-palette history/favorites,
     4. unsaved-changes prompt and graceful quit,
     5. OS clipboard / typed clipboard,
     6. authoritative CAD deletion/duplication/undo integration,
     7. broader camera UI,
     8. real plugin runtime/discovery/loading beyond the task-102 seam.
   - Select exactly one next implementation task and append it as task 104 with
     explicit `MAY edit`, `MUST NOT edit`, done criteria, verification, and
     halt conditions. If no candidate can be safely bounded from evidence,
     append no implementation task and record `NEEDS_HUMAN`.

   **Strong suggested prior:** host-shell FIFO/menu-click replacement and
   generalized registry execution may be the best next class because recent
   work improved command-palette projection/activation and extension-command
   shell handling while preserving the host-to-shell FIFO boundary. The audit
   must verify that premise from source and may choose a different candidate if
   the evidence is stronger.

   **Verification:** `git diff --check`; final
   `git status --short --untracked-files=all`; recorded source/doc search
   commands and results sufficient to support the selected task. No Rust build
   or test is expected unless the audit changes a verifier/tooling file, which
   this task should not do.

   **Result:** audit completed from current source and docs in ISSUE-351.
   Task 102's closure boundary is confirmed: the editor-shell extension-command
   injected handler seam exists for captured `Command::Custom` /
   `Command::Plugin` activations, while real plugin runtime/discovery/loading,
   host FIFO replacement, generalized registry execution, keybinding editor,
   conflict fatality policy, OS/typed clipboard, CAD graph/projection mutation,
   and broader camera UI remain outside task 102. The candidate comparison
   found host-shell FIFO/generalized registry execution still live but broader
   than the next safest one-dispatch step; keybinding/conflict policy,
   unsaved quit prompts, OS clipboard, CAD mutation/undo, camera controls, and
   real plugin runtime work all need larger policy or substrate decisions. The
   safest bounded follow-up is the already-substrated command-palette path:
   task 100 added host-local in-memory recent command ids, and current source
   has no persistent command-palette history/favorites surface. Task 104 is
   therefore a focused persistence follow-up for recent command-palette
   activation ids only; it does not implement favorites.

   **Halt condition:** if selecting task 104 would require implementation edits
   during task 103, or current docs/source contradict the premise enough that a
   bounded follow-up cannot be defended, record `NEEDS_HUMAN` with evidence
   instead of manufacturing an unsafe task.

104. **[DONE 2026-06-09 via PR #354 / commit `876f184`] Persist command-palette recent activations across editor sessions.**
   Add a bounded persistence layer for the existing `editor-egui-host`
   command-palette recent activation ids. The current source already records
   successful command-palette activations in memory via
   `EguiHost::command_palette_recent_command_ids` and
   `record_command_palette_recent_command`; blank palette filters promote those
   ids only when they match currently projected, enabled menu entries. Persist
   that recent-id list across host construction/destruction without introducing
   favorites, a second command model, or a new command execution path.

   **MAY edit:**
   - `crates/editor-egui-host/src/lib.rs`
   - `crates/editor-egui-host/src/menu.rs`
   - `crates/editor-egui-host/src/menu_tests.rs`
   - `crates/editor-egui-host/src/palette_recent.rs` (new, optional)
   - `plans/BASELINE.md`
   - `Status.md`
   - `HANDOFF.md`
   - `change.md`
   - `.ai/dispatch.tasks.md`
   - generated ISSUE-104 handoff/audit/log artifacts for this dispatch only

   **MUST NOT edit:**
   - `crates/editor-shell/**`
   - `crates/editor-ui/**` except read-only use of existing public APIs
   - `crates/editor-actions/**`
   - `crates/editor-state/**`
   - `crates/cad-core/**`
   - `crates/cad-projection/**`
   - `kernel/**`
   - `editor/**`
   - `runtime/**`
   - `plugins/**`
   - `crates/plugin-*`
   - `Cargo.toml`
   - `Cargo.lock`
   - `**/Cargo.toml`
   - workflows, dispatch/scheduler scripts, schemas, architecture-lint
     rules/config, plugin runtime/discovery/loading code, unrelated source,
     unrelated tests, or existing handoff/log artifacts

   **MAY add new files:**
   - `crates/editor-egui-host/src/palette_recent.rs`
   - generated ISSUE-104 handoff/audit/log artifacts for this dispatch only

   **Current-state claims / falsification to include in the TASK packet:**
   - Claim: command-palette recents are currently in-memory only.
     Falsifying search:
     `git grep -n -E "command_palette_recent_command_ids|record_command_palette_recent_command" -- crates/editor-egui-host/src crates/editor-shell/src crates/editor-ui/src crates/editor-actions/src editor/rge-editor/src`
     -> matches host-local fields/helpers and tests in `editor-egui-host`.
   - Claim: no persistent command-palette history/favorites surface exists.
     Falsifying search:
     `git grep -n -E "favorite|favorites|Favourite|favourite|persist.*command_palette|command_palette.*persist|recent_command_ids.*(load|save|serde|ron|json|config|file)|COMMAND_PALETTE_RECENT.*(load|save|serde|ron|json|config|file)" -- crates/editor-egui-host/src crates/editor-shell/src crates/editor-ui/src crates/editor-actions/src editor/rge-editor/src`
     -> no matches; exit 1 is expected.
   - Claim: existing persistence precedent is available through editor-ui
     layout services, but editor-egui-host has no direct serde/ron dependency.
     Falsifying search:
     `git grep -n -E "LayoutService|default_config_dir|default_layout_path|std::fs::(read|write)|ron::|serde_json" -- crates/editor-ui/src/dock/layout_service.rs crates/editor-egui-host/Cargo.toml crates/editor-ui/Cargo.toml`
     -> matches editor-ui layout persistence and deps; host Cargo has no
     direct serde/ron slot.

   **Required behavior:**
   - Load persisted command-palette recent ids when `EguiHost` is constructed,
     using a deterministic default path under the existing RGE config
     directory pattern or an injectable test path.
   - Save the capped, de-duplicated recent-id list after successful
     command-palette activation.
   - Preserve task-100 ordering semantics: blank filters promote only currently
     projected, enabled recent commands; stale ids are ignored; disabled rows
     remain in the normal remainder; non-blank fuzzy ranking is unchanged.
   - Persistence I/O failures must be non-fatal and must not prevent rendering,
     menu projection, palette activation, or command dispatch.
   - Main-menu activations must not update command-palette recents.
   - Do not add favorites, pinning UI, command metadata, a second command
     registry, generalized registry execution, plugin runtime/discovery/loading,
     OS clipboard integration, keybinding editor behavior, CAD mutation, camera
     controls, or Cargo dependencies.

   **Done criteria:**
   - A recent command-palette activation is visible in blank-filter ordering
     after constructing a fresh `EguiHost` or equivalent host-state helper with
     the same persistence path.
   - Persisted stale ids do not create rows, and persisted disabled ids are not
     promoted ahead of enabled rows.
   - The persisted list stays capped and de-duplicated with most-recent-first
     order.
   - Corrupt/missing/unwritable persistence files are handled non-fatally.
   - Non-blank fuzzy search ordering remains covered and unchanged.

   **Verification required:**
   - Focused `editor-egui-host` tests for load/save round trip, cap and
     de-duplication across persistence, stale/disabled persisted ids,
     non-fatal corrupt/missing/unwritable persistence behavior where feasible,
     main-menu non-recording, and non-blank fuzzy ordering preservation.
   - `cargo +nightly fmt --all -- --check`
   - `cargo test -p rge-editor-egui-host --lib command_palette_recent`
   - `cargo test -p rge-editor-egui-host --lib`
   - `cargo check -p rge-editor-egui-host --lib`
   - `git diff --check`

   **Result:** implemented in ISSUE-353 / PR #354. The editor-egui-host now
   persists command-palette recent activation ids as capped, de-duplicated
   newline-delimited `Command::diagnostic_id()` values under the per-user RGE
   config path; loads them during `EguiHost` construction; keeps load/save
   failures non-fatal; and leaves main-menu activations out of
   command-palette recent recording. Local recovery ran the focused
   editor-egui-host tests plus the canonical dispatch verifier, and GitHub
   Actions for PR #354 completed successfully before merge.

   **Halt conditions:**
   - The implementation requires adding Cargo dependencies or editing any Cargo
     manifest/lockfile.
   - The implementation requires changing `editor-shell`, `editor-ui`,
     `editor-actions`, command routing, host-to-shell FIFO semantics, menu
     registry semantics, generalized registry execution, plugin
     runtime/discovery/loading, keybinding/conflict policy, OS clipboard,
     typed clipboard, CAD graph/projection mutation, undo/dirty integration,
     broader camera UI, workflows, schemas, dispatch automation, or
     architecture-lint rules/config.
   - A safe persistence path cannot be chosen without a human product decision.
   - Favorites/pinning becomes necessary to satisfy the task; record that as a
     separate human/product decision instead of implementing it.

105. **[DONE 2026-06-09 via ISSUE-355 manual salvage] Fix ADR-121 advisory scope for generated verify target dirs.**
   Harden the non-blocking ADR-121 handoff-packet advisory validator so the
   scope check does not treat generated build artifacts from the active
   verification target directory as dispatch-touched files. ISSUE-353 recovery
   showed `.ai/dispatch.verify.ps1` could complete all seven CI-parity steps,
   then have `Test-HandoffPacket.ps1` report a scope FAIL because
   `git ls-files --others --exclude-standard` included thousands of
   `target-issue-353/**` files created by that same verify run. After manual
   cleanup of the generated target directory, the same validator passed.

   **MAY edit:**
   - `Test-HandoffPacket.ps1`
   - `.ai/dispatch.verify.ps1`
   - `AI_DISPATCH_AUTOMATION.md`
   - `.ai/dispatch.tasks.md`
   - focused PowerShell tests under `tools/dispatch-tests/**` if an existing
     dispatch/validator test harness fits this change
   - generated ISSUE-105 handoff/audit/log artifacts for this dispatch only

   **MUST NOT edit:**
   - Rust source/tests
   - Cargo manifests or `Cargo.lock`
   - GitHub workflows
   - scheduler registration scripts
   - unrelated dispatch scripts
   - unrelated documentation
   - existing handoff/log artifacts from other dispatches

   **Current-state claims / falsification to include in the TASK packet:**
   - Claim: advisory scope currently enumerates untracked files without
     filtering the active build target directory.
     Falsifying search:
     `git grep -n "git ls-files --others --exclude-standard\\|CARGO_TARGET_DIR\\|target-issue" -- Test-HandoffPacket.ps1 .ai/dispatch.verify.ps1 .gitignore AI_DISPATCH_AUTOMATION.md`
   - Claim: ISSUE-353 recovery observed a false advisory scope failure caused
     by generated `target-issue-353/**` paths, while the cleaned tree advisory
     passed.
     Falsifying search:
     `git grep -n "ISSUE-353\\|target-issue-353\\|HANDOFF_ADVISORY" -- ai_handoffs/ISSUE-353_* AI_DISPATCH_AUTOMATION.md .ai/dispatch.tasks.md`
   - Claim: this is an advisory-scope hygiene problem, not a Rust test/build
     failure.
     Falsifying search:
     `git grep -n "ADR-121 handoff packet validation\\|advisory-only\\|all 7 verification step" -- .ai/dispatch.verify.ps1 AI_DISPATCH_AUTOMATION.md ai_handoffs/ISSUE-353_EXEC_*.md`

   **Required behavior:**
   - When the advisory validator is run from `.ai/dispatch.verify.ps1`, files
     under the active `CARGO_TARGET_DIR` must be excluded from the touched-file
     scope set if that target directory resolves inside the repo.
   - The filter must be path-normalized and must not exclude source,
     documentation, handoff, queue-log, workflow, or script changes.
   - The validator must still catch an out-of-envelope untracked file outside
     the generated build target directory.
   - Direct standalone validator usage must remain compatible with the
     existing CLI shape. If a new optional parameter is added, old invocations
     must continue to work.
   - The canonical verifier must remain advisory-only for ADR-121 validation:
     validator WARN/FAIL must not make `.ai/dispatch.verify.ps1` exit nonzero.

   **Done criteria:**
   - A focused test or scripted smoke demonstrates that an untracked file under
     a supplied/generated verify target directory is ignored by scope checking.
   - A focused test or scripted smoke demonstrates that an untracked file
     outside that target directory is still reported as a scope violation.
   - `.ai/dispatch.verify.ps1` passes from a clean tree with an in-repo
     `CARGO_TARGET_DIR` without emitting target-dir scope violations.
   - Documentation/task notes explain why generated verify targets are ignored
     and why the check remains advisory-only.

   **Verification required:**
   - PowerShell parser validation for changed `.ps1` files.
   - Focused dispatch/validator test or smoke covering target-dir filtering and
     outside-target violation preservation.
   - `git diff --check`
   - Canonical `.ai/dispatch.verify.ps1`

   **Result:** implemented after the automated ISSUE-355 task-gate attempts
   exhausted their plan revisions. `Test-HandoffPacket.ps1` now accepts an
   optional `-ExcludeTouchedPath` list, normalizes those paths to repo-local
   prefixes, and filters only touched files under those prefixes before scope
   evaluation. `.ai/dispatch.verify.ps1` passes the active `CARGO_TARGET_DIR`
   to that validator path, so in-repo generated target directories no longer
   produce false advisory scope violations. Focused Pester coverage confirms
   target-dir files are ignored while an outside-target out-of-envelope file
   still fails scope, and wrapper coverage confirms the verifier advisory call
   forwards `CARGO_TARGET_DIR`.

   **Halt conditions:**
   - The fix requires changing Rust source, Cargo metadata, GitHub workflow
     definitions, branch/issue publish policy, or queue retry semantics.
   - The only available fix would hide arbitrary untracked files rather than a
     path-normalized generated target directory.
   - The validator must become blocking to make the behavior testable.

106. **[DONE 2026-06-09 via local source-read audit - selected task 107 command-palette pinned favorites] Select the next bounded Phase 9/editor-usability implementation follow-up.**
   Run a docs/source-read audit after the task-104/task-105 closure and select
   exactly one new bounded implementation task as task 107, or record
   `NEEDS_HUMAN` if the current evidence does not support a safe dispatch. This
   is a selection audit only: it may read/search source to falsify stale claims,
   but it must not implement the selected follow-up.

   **MAY edit:**
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - generated ISSUE-106 handoff/audit/log artifacts for this dispatch only

   **MUST NOT edit:**
   - Rust source or tests under `crates/**`, `kernel/**`, `runtime/**`,
     `editor/**`, or `tools/**`
   - `Cargo.toml`, `Cargo.lock`, or any `**/Cargo.toml`
   - GitHub workflows
   - dispatch automation, guard, queue, scheduler, or verification scripts
   - schemas, architecture-lint rules/config, ADR files, packet templates, or
     existing handoff/log artifacts from other dispatches
   - plugin runtime/discovery/loading code
   - unrelated local `.ai/**`, `ai_handoffs/**`, or `ai_dispatch_logs/**`
     artifacts

   **MAY add new files:**
   - generated ISSUE-106 handoff/audit/log artifacts for this dispatch only

   **Current-state claims / falsification to include in the TASK packet:**
   - Claim: task 104 and task 105 are complete, and this task 106 is the only
     newly armed follow-up after the previously exhausted 105/105 queue.
     Falsifying search:
     `git grep -n -E "105/105|task 104|task 105|task 106|ISSUE-353|ISSUE-355|PR #354|PR #356" -- .ai/dispatch.tasks.md Status.md HANDOFF.md plans/BASELINE.md change.md`
   - Claim: task 102 added only an editor-shell extension-command seam, not
     real plugin runtime/discovery/loading.
     Falsifying search:
     `git grep -n -E "ExtensionCommandHandler|ExtensionCommandEvent|Command::Custom|Command::Plugin|PluginHost|PluginContext|runtime-wasmtime|plugin-discovery|rge_kernel_plugin_host" -- crates/editor-shell/src editor/rge-editor/src crates/editor-egui-host/src crates/editor-ui/src`
   - Claim: the host-to-shell `MenuCommandHandoff` FIFO and
     `EditorShell::route_menu_command` remain the active menu/palette dispatch
     route, while the canonical menu registry owns menu definitions.
     Falsifying search:
     `git grep -n -E "MenuCommandHandoff|drain_and_route_menu_commands|route_menu_command|default_editor_menu|MenuRegistry|enabled_command_for_shortcut" -- crates/editor-egui-host/src crates/editor-shell/src crates/editor-ui/src editor/rge-editor/src`
   - Claim: command-palette recent ordering and persistence are already done,
     so task 106 must not select another recent-history persistence slice.
     Falsifying search:
     `git grep -n -E "palette_recent|command_palette_recent_command_ids|record_command_palette_recent_command|load_command_palette_recent_command_ids|save_command_palette_recent_command_ids" -- crates/editor-egui-host/src Status.md HANDOFF.md plans/BASELINE.md .ai/dispatch.tasks.md`

   **Candidate classes to compare before selecting task 107:**
   - Host-shell FIFO replacement or generalized registry execution beyond the
     current `MenuCommandHandoff` -> `EditorShell::route_menu_command` path.
   - Extension/plugin command execution beyond the injected handler seam, while
     keeping real plugin runtime/discovery/loading out unless source evidence
     shows a tiny safe slice.
   - Keybinding/conflict policy or shortcut-surface improvements.
   - Unsaved quit/close prompting and save-state UX.
   - OS clipboard or typed editor clipboard behavior.
   - Authoritative CAD graph/projection mutation with undo/dirty integration.
   - Camera/navigation UI beyond the existing reset/zoom commands.
   - Command-palette favorites/pinning or other palette UX that is distinct
     from already-complete recent ordering and persistence.

   **Selection requirements:**
   - Read current docs and source before choosing; do not infer from stale
     roadmap prose.
   - Compare the full candidate set above and record why the selected follow-up
     is smaller or safer than the deferred alternatives.
   - Append exactly one task 107 with a bounded MAY-edit/MUST-NOT-edit envelope,
     current-state falsification searches, required behavior, done criteria,
     verification, and halt conditions.
   - If no candidate can be defended as one bounded implementation dispatch,
     record `NEEDS_HUMAN` with concrete evidence instead of manufacturing work.
   - Do not implement task 107 during this audit.

   **Verification required:**
   - Required source/doc `git grep` or `rg` searches recorded in the EXEC packet.
   - `git diff --check`
   - `git diff --name-only`

   **Result:** local source-read audit completed after task 104/task 105
   closure. Required searches confirmed task 104 and task 105 are complete, the
   editor-shell extension-command executor remains an injected seam only, the
   canonical menu registry plus `MenuCommandHandoff`/`route_menu_command` remain
   the active menu/palette route, and command-palette recents are already
   persisted. Follow-up source reads found that the File/Edit/Play/View command
   surface is now much more complete than stale backlog wording implied:
   `SelectAll`, `Cut`, `Copy`, `Paste`, `Delete`, `Duplicate`, Close/Quit,
   camera reset/zoom, enabled-only accelerators, plugin menu projection,
   shortcut-conflict diagnostics, and extension-command injected-handler events
   are all already represented in source and tests.

   **Candidate comparison:** host-shell FIFO replacement/generalized registry
   execution is still live but broader than one safe dispatch because the FIFO is
   also the deliberate host-shell boundary and core execution already unifies
   menu clicks and keyboard accelerators through `Command`. Real plugin command
   execution remains beyond the task-102 seam because current editor source has
   no plugin runtime/discovery/loading integration. Keybinding conflict fatality
   policy, unsaved quit/close prompting, OS/typed clipboard behavior,
   authoritative CAD graph/projection mutation with undo/dirty integration, and
   broader camera/navigation UI each require wider policy or substrate decisions
   than this audit should manufacture. The safest one-dispatch implementation
   slice is command-palette pinned favorites in `editor-egui-host`: it is
   distinct from task-104 recent-history persistence, reuses the same
   host-local projection and persistence pattern, and does not alter command
   routing, plugin runtime/discovery/loading, keybinding policy, clipboard, CAD
   mutation, or undo/dirty semantics.

   **Halt conditions:**
   - Selecting task 107 would require implementation edits during task 106.
   - The safest next step requires human product/architecture policy rather than
     source-backed dispatch selection.
   - Current source contradicts the premise enough that none of the candidate
     classes can be scoped safely.

107. **[DONE 2026-06-09] Add command-palette pinned favorites in `editor-egui-host`.**
   Add a bounded host-local pinned/favorite command slice to the existing
   command-palette UI. This is the next smallest Phase 9 editor-usability
   follow-up after recent-history persistence: users can pin frequently used
   projected commands so blank-filter palette ordering is stable and intentional
   across sessions, while command execution continues to use the existing
   `MenuCommandHandoff` route.

   **MAY edit:**
   - `crates/editor-egui-host/src/lib.rs`
   - `crates/editor-egui-host/src/menu.rs`
   - `crates/editor-egui-host/src/menu_tests.rs`
   - `crates/editor-egui-host/src/palette_recent.rs`
   - new focused `crates/editor-egui-host/src/palette_pinned.rs` or similarly
     named host-local helper module if it keeps the host/menu files under the
     line-cap and cohesion rules
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - generated ISSUE-107 handoff/audit/log artifacts for this dispatch only

   **MUST NOT edit:**
   - `crates/editor-shell/**`
   - `crates/editor-ui/**`
   - `crates/editor-actions/**`
   - `kernel/**`
   - `runtime/**`
   - `editor/rge-editor/**`
   - Cargo manifests or `Cargo.lock`
   - GitHub workflows
   - dispatch automation, guard, queue, scheduler, or verification scripts
   - schemas, architecture-lint rules/config, ADR files, packet templates, or
     existing handoff/log artifacts from other dispatches
   - plugin runtime/discovery/loading code

   **Current-state claims / falsification to include in the TASK packet:**
   - Claim: command-palette recent ordering and persistence already exist and
     must not be reimplemented.
     Falsifying search:
     `git grep -n -E "palette_recent|command_palette_recent_command_ids|record_command_palette_recent_command|load_command_palette_recent_command_ids|save_command_palette_recent_command_ids|enqueue_command_palette_activation" -- crates/editor-egui-host/src`
   - Claim: no command-palette pinned/favorite command state exists in the
     editor source today.
     Falsifying search:
     `git grep -n -E "favorite|favorites|Favourite|favourite|command_palette.*pin|pin.*command_palette|pinned_command|pinned.*command" -- crates/editor-egui-host/src crates/editor-shell/src crates/editor-ui/src editor/rge-editor/src`
     -> expected matches may include generic "pin" wording in layout/dock docs;
     no actual command-palette favorite/pinned state or persistence should
     exist.
   - Claim: command-palette entries are host projections of the current
     `MenuRegistry`, and activation still enqueues a `Command` through the
     host-to-shell handoff.
     Falsifying search:
     `git grep -n -E "command_palette_entries|command_palette_window|MenuCommandHandoff|enqueue_command_palette_activation|project_main_menu|Command::diagnostic_id" -- crates/editor-egui-host/src`
   - Claim: plugin menu entries and extension commands must remain projected
     data only; task 107 must not add real plugin runtime/discovery/loading or
     a new command execution path.
     Falsifying search:
     `git grep -n -E "register_plugin_menu_entry|Command::Plugin|Command::Custom|ExtensionCommandHandler|PluginHost|runtime-wasmtime|plugin-discovery" -- crates/editor-egui-host/src crates/editor-shell/src editor/rge-editor/src`

   **Required behavior:**
   - Add host-owned pinned command ids keyed by `Command::diagnostic_id()`,
     capped and de-duplicated.
   - Load pinned ids when `EguiHost` is constructed, using the existing
     command-palette persistence directory pattern or an injectable test path.
   - Persist pin/unpin changes non-fatally; missing, corrupt, or unwritable
     persistence must not prevent rendering, filtering, palette activation, or
     command dispatch.
   - Add a compact command-palette row affordance to pin and unpin a command
     without dispatching it. The affordance must be available for enabled and
     disabled rows, must not close the palette when toggled, and must not update
     recent-history ordering.
   - For blank or whitespace-only filters, promote currently projected and
     enabled pinned commands first, in pinned-list order; then promote enabled
     recent commands that are not already pinned; then append all remaining
     projected rows in normal menu order.
   - Stale pinned ids must be ignored. Disabled pinned rows must stay visible in
     the normal remainder but must not be promoted ahead of enabled rows.
   - Non-blank fuzzy search/filter ordering from task 98 must remain unchanged
     except for rendering the pinned/unpinned affordance state.
   - Main-menu activations must not pin commands or change pinned ordering.
   - Preserve task-104 recent-history persistence semantics: successful palette
     activations still record recents; pin/unpin clicks do not count as command
     activations.

   **Done criteria:**
   - A command pinned in one host/helper instance is promoted in blank-filter
     ordering after constructing a fresh host/helper with the same persistence
     path.
   - Unpinning removes the command from pinned promotion and persists across a
     fresh host/helper load.
   - Pinned commands outrank recent commands for blank filters, and recent
     commands do not duplicate rows already promoted by pins.
   - Stale pinned ids and disabled pinned rows are not promoted.
   - Non-blank fuzzy ranking remains covered and unchanged.
   - Pin/unpin interaction does not enqueue a `Command`, close the palette, or
     update recent-history ids.

   **Verification required:**
   - Focused `editor-egui-host` tests for pinned load/save round trip,
     cap/deduplication, unpin persistence, blank-filter ordering with pinned
     before recent, stale/disabled pinned ids, pin/unpin non-dispatch behavior,
     corrupt/missing/unwritable persistence where feasible, and non-blank fuzzy
     ordering preservation.
   - `cargo +nightly fmt --all -- --check`
   - `cargo test -p rge-editor-egui-host --lib command_palette`
   - `cargo test -p rge-editor-egui-host --lib`
   - `cargo check -p rge-editor-egui-host --lib`
   - `git diff --check`

   **Result:** implemented in `editor-egui-host`. Added
   `palette_pinned.rs` with capped, de-duplicated newline-delimited
   `Command::diagnostic_id()` persistence at the same per-user config root used
   by command-palette recents. `EguiHost` now loads pinned ids at construction,
   keeps host-local pinned state, and passes it into the command-palette window.
   The palette renders a compact Pin/Unpin row affordance that works for enabled
   and disabled rows, does not close the palette, does not enqueue commands, and
   does not record recent history. Blank filters now promote enabled pinned
   commands first, then enabled recents not already promoted, then all remaining
   rows in projected menu order. Stale pinned ids are ignored, disabled pinned
   rows stay in the normal remainder, and non-blank fuzzy ordering is unchanged.

   **Verification run:**
   - `cargo test -p rge-editor-egui-host --lib command_palette` -> 41 passed.
   - `cargo test -p rge-editor-egui-host --lib` -> 71 passed.
   - `cargo +nightly fmt --all -- --check` -> passed.
   - `cargo check -p rge-editor-egui-host --lib` -> passed warning-clean.

   **Halt conditions:**
   - The implementation requires editing `editor-shell`, `editor-ui`,
     `editor-actions`, plugin runtime/discovery/loading, Cargo manifests,
     workflows, dispatch automation, schemas, or architecture-lint config.
   - The implementation requires replacing `MenuCommandHandoff`, adding a second
     command registry/model, changing `Command` routing, or changing shortcut
     conflict policy.
   - A safe persistence path cannot be chosen from the existing
     command-palette recent-history precedent.
   - A full keybinding editor, plugin runtime execution, OS/typed clipboard,
     CAD graph/projection mutation, or undo/dirty integration becomes necessary
     to satisfy the task.

108. **[DONE 2026-06-09 via local source-read audit - selected task 109 keyboard-shortcuts help] Post-palette Phase 9 next-task source audit.**
   The task queue is exhausted after task 107. Re-arm automation with a
   docs/source-read audit that selects exactly one bounded Phase 9
   editor-usability implementation follow-up as task 109, or records
   `NEEDS_HUMAN` if the remaining candidates require product/architecture
   policy before code.

   **MAY edit:**
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - generated ISSUE-108 handoff/audit/log artifacts for this dispatch only

   **MUST NOT edit:**
   - Rust source or tests under `crates/**`, `kernel/**`, `runtime/**`, or
     `editor/**`
   - Cargo manifests or `Cargo.lock`
   - GitHub workflows
   - dispatch automation, guard, queue, scheduler, or verification scripts
   - schemas, architecture-lint rules/config, ADR files, packet templates, or
     existing handoff/log artifacts from other dispatches
   - plugin runtime/discovery/loading implementation code

   **Current-state claims / falsification to include in the EXEC packet:**
   - Claim: command-palette fuzzy search, recent persistence, and pinned
     favorites are already complete and should not be reselected as the next
     implementation slice.
     Falsifying search:
     `git grep -n -E "palette_recent|palette_pinned|command_palette_recent_command_ids|command_palette_pinned_command_ids|filter_command_palette_entries_with_pinned_and_recents|toggle_command_palette_pinned_command|enqueue_command_palette_activation" -- crates/editor-egui-host/src Status.md HANDOFF.md plans/BASELINE.md .ai/dispatch.tasks.md`
   - Claim: core menu/palette activations still cross the host-shell boundary
     through `MenuCommandHandoff` and are routed by
     `EditorShell::route_menu_command`; replacing that route is broader than
     a UI-only task unless source evidence shows a tiny safe slice.
     Falsifying search:
     `git grep -n -E "MenuCommandHandoff|drain_and_route_menu_commands|route_menu_command|command_palette_window|enqueue_command_palette_activation|enabled_command_for_shortcut|default_editor_menu" -- crates/editor-egui-host/src crates/editor-shell/src crates/editor-ui/src editor/rge-editor/src`
   - Claim: extension/plugin commands currently stop at the task-102 injected
     handler seam; no real plugin runtime/discovery/loading path is wired into
     the editor command route.
     Falsifying search:
     `git grep -n -E "ExtensionCommandHandler|ExtensionCommandEvent|Command::Custom|Command::Plugin|PluginHost|PluginContext|plugin-discovery|runtime-wasmtime|rge_kernel_plugin_host" -- crates/editor-shell/src crates/editor-egui-host/src crates/editor-ui/src editor/rge-editor/src`
   - Claim: several stale roadmap candidates may already have partial source
     closure and must be rechecked before selection, especially shortcut
     conflicts, shell-local clipboard, close/quit save behavior, camera
     commands, and CAD/editor mutation routes.
     Falsifying search:
     `git grep -n -E "Shortcut Conflicts|shortcut_conflicts|has_clipboard_entities|Command::Close|Command::Quit|unsaved|dirty|Command::ResetCamera|Command::ZoomIn|Command::ZoomOut|Command::Cut|Command::Copy|Command::Paste|Command::Duplicate|cad|CadGraph" -- crates/editor-egui-host/src crates/editor-shell/src crates/editor-ui/src editor/rge-editor/src`

   **Candidate classes to compare before selecting task 109:**
   - Host-shell FIFO replacement or generalized registry execution beyond the
     current `MenuCommandHandoff` -> `EditorShell::route_menu_command` path.
   - Extension/plugin command execution beyond the injected handler seam,
     including whether any substrate-only step exists before real runtime or
     discovery/loading work.
   - Keybinding/conflict policy, shortcut diagnostics, or shortcut-surface
     improvements after existing conflict surfacing.
   - Unsaved close/quit prompting and save-state UX beyond current save/dirty
     routing.
   - OS clipboard or typed editor clipboard behavior beyond the current
     shell-local entity clipboard.
   - Authoritative CAD graph/projection mutation with undo/dirty integration.
   - Camera/navigation UI beyond existing reset/zoom commands and scene-aware
     labels.
   - Any other Phase 9/editor-usability candidate that current source shows is
     smaller and safer than the listed deferred areas.

   **Selection requirements:**
   - Read current docs and source before choosing; do not infer from stale
     backlog prose or from `ai_handoffs/` file names.
   - Compare the full candidate set above and record why the selected follow-up
     is smaller or safer than the deferred alternatives.
   - Append exactly one task 109 with a bounded MAY-edit/MUST-NOT-edit envelope,
     current-state falsification searches, required behavior, done criteria,
     verification, and halt conditions.
   - If no candidate can be defended as one bounded implementation dispatch,
     record `NEEDS_HUMAN` with concrete source evidence instead of
     manufacturing work.
   - Do not implement task 109 during this audit.

   **Verification required:**
   - Required source/doc `git grep` or `rg` searches recorded in the EXEC
     packet or status update.
   - `git diff --check`
   - `git diff --name-only`

   **Halt conditions:**
   - Selecting task 109 would require implementation edits during task 108.
   - Current source shows all plausible next slices require human
     product/architecture policy before implementation.
   - The selected follow-up would need Cargo/workflow/automation/schema/ADR
     edits, real plugin runtime/discovery/loading, command-route replacement,
     OS clipboard integration, authoritative CAD mutation, or undo/dirty policy
     unless those are explicitly scoped as the narrow audit-selected task with
     source-backed safety.

   **Result:** local source-read audit completed after task 107 closure. Required
   searches confirmed task 107's fuzzy/recent/pinned command-palette work is
   already complete; menu clicks, palette activations, and enabled keyboard
   accelerators still share the `MenuCommandHandoff` -> `EditorShell::route_menu_command`
   route; extension/plugin activations still stop at the task-102 injected
   `ExtensionCommandHandler` seam; shortcut conflicts are represented as
   registry data and host diagnostics; close/quit still avoid unsaved prompts
   while dirty state is visible through save-status/window-title paths; clipboard
   behavior remains shell-local entity blobs; Edit commands operate on the
   wrapper-world selection/entities; and View camera reset/zoom commands already
   exist and route through the menu sink.

   **Candidate comparison:** host-shell FIFO replacement/generalized registry
   execution remains broader than a single UI follow-up because it would replace
   the deliberate host-shell boundary. Real plugin command execution still
   requires runtime/discovery/loading decisions. Keybinding conflict fatality or
   a keybinding editor needs product policy, but a read-only shortcut help
   surface is source-backed by the existing `ProjectedMainMenu` shortcut
   projection. Unsaved close/quit prompts, OS/typed clipboard, authoritative CAD
   mutation with undo/dirty integration, and broader camera/navigation controls
   each require policy or substrate work beyond this audit's safe automation
   bar. The selected one-dispatch follow-up is therefore host-local keyboard
   shortcuts help in `editor-egui-host`: a discoverability surface over the
   current projected menu/shortcut data, with no routing, binding, conflict
   policy, plugin runtime, clipboard, CAD, or undo/dirty changes.

109. **[DONE 2026-06-09 via ISSUE-358 / commit `9c789f9`] Add host-local keyboard shortcuts help in `editor-egui-host`.**
   Add a bounded shortcut-discoverability surface to the existing egui host. The
   help surface must be derived from the already-resolved main-menu projection
   so it reflects current labels, shortcut display strings, passive Play hints,
   plugin menu entries, and enablement without adding a second shortcut model.

   **MAY edit:**
   - `crates/editor-egui-host/src/lib.rs`
   - `crates/editor-egui-host/src/menu.rs`
   - `crates/editor-egui-host/src/menu_tests.rs`
   - new focused `crates/editor-egui-host/src/shortcut_help.rs` or similarly
     named host-local helper module if it keeps the host/menu files cohesive
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - generated ISSUE-109 handoff/audit/log artifacts for this dispatch only

   **MUST NOT edit:**
   - `crates/editor-shell/**`
   - `crates/editor-ui/**`
   - `crates/editor-actions/**`
   - `crates/cad-core/**`
   - `crates/cad-projection/**`
   - `kernel/**`
   - `runtime/**`
   - `editor/rge-editor/**`
   - Cargo manifests or `Cargo.lock`
   - GitHub workflows
   - dispatch automation, guard, queue, scheduler, or verification scripts
   - schemas, architecture-lint rules/config, ADR files, packet templates, or
     existing handoff/log artifacts from other dispatches
   - plugin runtime/discovery/loading code

   **Current-state claims / falsification to include in the TASK packet:**
   - Claim: task 107 completed command-palette fuzzy/recent/pinned work, so task
     109 must not select another palette fuzzy/recent/pinned persistence slice.
     Falsifying search:
     `git grep -n -E "palette_recent|palette_pinned|command_palette_recent_command_ids|command_palette_pinned_command_ids|filter_command_palette_entries_with_pinned_and_recents|toggle_command_palette_pinned_command|enqueue_command_palette_activation" -- crates/editor-egui-host/src Status.md HANDOFF.md plans/BASELINE.md .ai/dispatch.tasks.md`
   - Claim: shortcut labels and enablement are already projected through
     `ProjectedMainMenu`, but no dedicated keyboard-shortcuts help/window exists
     in the host today.
     Falsifying search:
     `git grep -n -E "Shortcut Help|Keyboard Shortcuts|shortcut_help|shortcut.*window|ShortcutsOverlay|shortcuts_overlay|Shortcut Conflicts|ProjectMainMenu|ProjectedMenuEntry|command_palette_entries" -- crates/editor-egui-host/src crates/editor-ui/src crates/editor-shell/src`
     -> expected matches include `ShortcutsOverlay` layout configuration,
     `Shortcut Conflicts` diagnostics, and `project_main_menu` /
     `command_palette_entries`; no current shortcut-help window or helper should
     exist.
   - Claim: the executable shortcut source of truth is still
     `default_editor_menu` / `Shortcut::display`, while editor-shell executes
     enabled accelerators via `ResolveResult::enabled_command_for_shortcut`.
     Falsifying search:
     `git grep -n -E "Shortcut::new|Shortcut::plain|shortcut_hint|Shortcut::display|command_for_shortcut|enabled_command_for_shortcut" -- crates/editor-ui/src/menus crates/editor-egui-host/src crates/editor-shell/src/lifecycle/accelerator.rs`
   - Claim: `editor-egui-host` already has host-local UI surfaces that do not
     dispatch commands, alongside routed menu/palette command activation; task
     109 can add a host-local help window without replacing command routing.
     Falsifying search:
     `git grep -n -E 'egui::Window::new|menu_button\("Shortcut Conflicts"|command_palette_window|MenuCommandHandoff|menu_commands\.push|toggle_command_palette' -- crates/editor-egui-host/src`

   **Required behavior:**
   - Add a host-local keyboard-shortcuts help affordance in the egui menu bar and
     a transient egui window/surface for the help content.
   - Build help rows only from the current `ProjectedMainMenu` data. Preserve
     menu grouping and projected order for File, Edit, Play, View, and Plugins.
   - Include entries that carry `Some(shortcut)` from either executable
     shortcuts or passive `shortcut_hint` values; omit entries with no shortcut.
   - Display current projected labels, shortcut strings, command diagnostic ids,
     and enabled/disabled state so the help surface reflects live predicates and
     dynamic labels such as View -> Frame Scene.
   - Plugin menu rows may appear only as projected menu data; do not add real
     plugin runtime/discovery/loading or a new execution path.
   - Opening, closing, or interacting with the help surface must not enqueue a
     `Command`, close the command palette, update command-palette recents or
     pins, change menu enablement, or alter accelerator execution.

   **Explicit non-goals:**
   - No keybinding editor, shortcut remapping, conflict fatality policy, or
     conflict resolution.
   - No `?` keyboard shortcut, workspace `ShortcutsOverlay` integration,
     `editor-ui` layout migration, or new `Command` variant.
   - No command-route replacement, generalized command executor, plugin runtime
     execution, OS clipboard integration, CAD graph/projection mutation, or
     undo/dirty policy.

   **Done criteria:**
   - Focused host tests prove shortcut-help row derivation groups rows by menu
     section, preserves current projected order, includes passive Play hints,
     omits unshortcuted rows, retains enablement, and includes plugin rows only
     when they are projected.
   - A default-menu projection test pins representative rows for File/Edit/Play/View,
     including `Ctrl+S`, `Ctrl+Z`, `Space`/`Escape`, and Home/PageUp/PageDown.
   - A scene-aware predicate test proves the dynamic View label flows through the
     help rows without changing command identity.
   - A non-dispatch test proves the help affordance/window does not push into
     `MenuCommandHandoff` and does not mutate command-palette recent or pinned
     state.

   **Verification required:**
   - `cargo test -p rge-editor-egui-host --lib shortcut`
   - `cargo test -p rge-editor-egui-host --lib`
   - `cargo check -p rge-editor-egui-host --lib`
   - `cargo +nightly fmt --all -- --check`
   - `git diff --check`

   **Halt conditions:**
   - The implementation requires editing `editor-shell`, `editor-ui`,
     `editor-actions`, CAD crates, kernel/runtime crates, plugin
     runtime/discovery/loading code, Cargo manifests/lockfiles, workflows,
     dispatch automation, schemas, ADRs, or architecture-lint config.
   - The implementation requires a new `Command` variant, new accelerator,
     `ShortcutsOverlay` workspace integration, keybinding remap/editor work,
     shortcut conflict policy, or command-route replacement.
   - Shortcut-help rows cannot be sourced from existing projected menu data and
     would require a parallel hard-coded shortcut table.
   - Real plugin execution, OS/typed clipboard behavior, authoritative CAD
     mutation, or undo/dirty integration becomes necessary to satisfy the task.

110. **[DONE 2026-06-09 via ISSUE-359] Post-shortcut-help Phase 9 next-task source audit.**
   The automation queue is exhausted after task 109. Re-arm automation with a
   docs/source-read audit that selects exactly one bounded Phase 9
   editor-usability implementation follow-up as task 111, or records
   `NEEDS_HUMAN` if the remaining candidates require product/architecture
   policy before code.

   **MAY edit:**
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - generated ISSUE-110 handoff/audit/log artifacts for this dispatch only

   **MUST NOT edit:**
   - Rust source or tests under `crates/**`, `kernel/**`, `runtime/**`, or
     `editor/**`
   - Cargo manifests or `Cargo.lock`
   - GitHub workflows
   - dispatch automation, guard, queue, scheduler, or verification scripts
   - schemas, architecture-lint rules/config, ADR files, packet templates, or
     existing handoff/log artifacts from other dispatches
   - plugin runtime/discovery/loading implementation code

   **Current-state claims / falsification to include in the EXEC packet:**
   - Claim: task 109 completed host-local shortcut help in
     `editor-egui-host`, so task 111 must not reselect another shortcut-help
     window/discoverability slice.
     Falsifying search:
     `git grep -n -E "shortcut_help|shortcut_help_open|Keyboard Shortcuts|ShortcutHelpRow|ShortcutHelpGroup|shortcut_help_rows|show_shortcut_help_window" -- crates/editor-egui-host/src Status.md HANDOFF.md plans/BASELINE.md .ai/dispatch.tasks.md`
   - Claim: core menu, palette, and accelerator activations still cross the
     host-shell boundary through `MenuCommandHandoff` and
     `EditorShell::route_menu_command`; replacing that route is broader than a
     UI-only task unless source evidence shows a tiny safe slice.
     Falsifying search:
     `git grep -n -E "MenuCommandHandoff|drain_and_route_menu_commands|route_menu_command|command_palette_window|enqueue_command_palette_activation|enabled_command_for_shortcut|default_editor_menu" -- crates/editor-egui-host/src crates/editor-shell/src crates/editor-ui/src editor/rge-editor/src`
   - Claim: extension/plugin commands still stop at the task-102 injected
     handler seam; no real plugin runtime/discovery/loading path is wired into
     the editor command route.
     Falsifying search:
     `git grep -n -E "ExtensionCommandHandler|ExtensionCommandEvent|Command::Custom|Command::Plugin|PluginHost|PluginContext|plugin-discovery|runtime-wasmtime|rge_kernel_plugin_host" -- crates/editor-shell/src crates/editor-egui-host/src crates/editor-ui/src editor/rge-editor/src`
   - Claim: several stale roadmap candidates may already have partial source
     closure and must be rechecked before selection, especially shortcut
     conflicts/keybinding policy, shell-local clipboard, close/quit save
     behavior, camera commands, and CAD/editor mutation routes.
     Falsifying search:
     `git grep -n -E "Shortcut Conflicts|shortcut_conflicts|keybinding|has_clipboard_entities|Command::Close|Command::Quit|unsaved|dirty|Command::ResetCamera|Command::ZoomIn|Command::ZoomOut|Command::Cut|Command::Copy|Command::Paste|Command::Duplicate|cad|CadGraph" -- crates/editor-egui-host/src crates/editor-shell/src crates/editor-ui/src editor/rge-editor/src`

   **Candidate classes to compare before selecting task 111:**
   - Host-shell FIFO replacement or generalized registry execution beyond the
     current `MenuCommandHandoff` -> `EditorShell::route_menu_command` path.
   - Extension/plugin command execution beyond the injected handler seam,
     including whether any substrate-only step exists before real runtime or
     discovery/loading work.
   - Keybinding/conflict policy, shortcut diagnostics, or other shortcut
     surfaces after task 109's read-only help window.
   - Unsaved close/quit prompting and save-state UX beyond current save/dirty
     routing.
   - OS clipboard or typed editor clipboard behavior beyond the current
     shell-local entity clipboard.
   - Authoritative CAD graph/projection mutation with undo/dirty integration.
   - Camera/navigation UI beyond existing reset/zoom commands and scene-aware
     labels.
   - Any other Phase 9/editor-usability candidate that current source shows is
     smaller and safer than the listed deferred areas.

   **Selection requirements:**
   - Read current docs and source before choosing; do not infer from stale
     backlog prose or from `ai_handoffs/` file names.
   - Compare the full candidate set above and record why the selected follow-up
     is smaller or safer than the deferred alternatives.
   - Append exactly one task 111 with a bounded MAY-edit/MUST-NOT-edit envelope,
     current-state falsification searches, required behavior, done criteria,
     verification, and halt conditions.
   - If no candidate can be defended as one bounded implementation dispatch,
     record `NEEDS_HUMAN` with concrete source evidence instead of
     manufacturing work.
   - Do not implement task 111 during this audit.

   **Verification required:**
   - Required source/doc `git grep` or `rg` searches recorded in the EXEC
     packet or status update.
   - `git diff --check`
   - `git diff --name-only`

   **Halt conditions:**
   - Selecting task 111 would require implementation edits during task 110.
   - Current source shows all plausible next slices require human
     product/architecture policy before implementation.
   - The selected follow-up would need Cargo/workflow/automation/schema/ADR
     edits, real plugin runtime/discovery/loading, command-route replacement,
     OS clipboard integration, authoritative CAD mutation, or undo/dirty policy
     unless those are explicitly scoped as the narrow audit-selected task with
     source-backed safety.

111. **[DONE 2026-06-09 via ISSUE-360 / commit `f831558`] Add unsaved Close/Quit confirmation in `editor-shell` / `rge-editor`.**
   Add a bounded dirty-state guard for destructive document close and
   application quit paths. Current source already has the save/source/dirty
   model and native-dialog ownership split needed for this: `EditorShell`
   exposes `command_bus().is_dirty()`, publishes save status, updates the window
   title, routes File -> Close / Quit through `route_menu_command`, and the
   binary already owns `rfd` while `editor-shell` stays dependency-clean through
   dialog traits. The follow-up must not add a new dirty model, auto-save flow,
   command route, command variant, or Cargo dependency.

   **MAY edit:**
   - `crates/editor-shell/src/lifecycle/mod.rs`
   - `crates/editor-shell/src/lifecycle/tests.rs`
   - new focused `crates/editor-shell/src/lifecycle/unsaved_changes.rs` or
     similarly named lifecycle helper module if it keeps `mod.rs` cohesive
   - `editor/rge-editor/src/main.rs`
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - generated ISSUE-111 handoff/audit/log artifacts for this dispatch only

   **MUST NOT edit:**
   - `crates/editor-ui/**`
   - `crates/editor-egui-host/**`
   - `crates/editor-actions/**`
   - CAD crates, kernel crates, runtime crates, plugin runtime/discovery/loading
     code, or architecture-lint code
   - Cargo manifests or `Cargo.lock`
   - GitHub workflows
   - dispatch automation, guard, queue, scheduler, or verification scripts
   - schemas, ADR files, packet templates, or existing handoff/log artifacts
     from other dispatches

   **Current-state claims / falsification to include in the TASK packet:**
   - Claim: dirty state and source display already exist and must be reused, not
     reinvented.
     Falsifying search:
     `git grep -n -E "command_bus\\(\\)\\.is_dirty|SaveStatusSnapshot|save_status_snapshot|sync_window_title|with_scene_save_dialog|with_project_save_hook|with_new_project_save_dialog|rfd::FileDialog" -- crates/editor-shell/src editor/rge-editor/src crates/editor-state/src`
   - Claim: current Close / Quit / window-close behavior can discard dirty work
     without confirmation, so the task is a real missing guard rather than a
     duplicate of shipped behavior.
     Falsifying search:
     `git grep -n -E "handle_close_file_request|handle_quit_request|WindowEvent::CloseRequested|take_quit_request|does not prompt for unsaved|unsaved changes|event_loop.exit" -- crates/editor-shell/src editor/rge-editor/src`
   - Claim: File Close and Quit already route through the canonical menu command
     path, so no new `Command`, accelerator, menu registry entry, or host-shell
     FIFO replacement is needed.
     Falsifying search:
     `git grep -n -E "Command::Close|Command::Quit|file.close|file.quit|route_menu_command|enabled_command_for_shortcut|default_editor_menu" -- crates/editor-ui/src crates/editor-egui-host/src crates/editor-shell/src`
   - Claim: no existing unsaved-discard confirmation hook/dialog is wired in the
     current editor surface.
     Falsifying search:
     `git grep -n -E "ConfirmUnsaved|UnsavedChanges|Discard.*Changes|MessageDialog|prompt.*unsaved|unsaved.*prompt|ConfirmDiscard|CloseIntent|QuitIntent" -- crates/editor-shell/src editor/rge-editor/src crates/editor-egui-host/src`

   **Required behavior:**
   - Add an `editor-shell` owned confirmation seam for dirty destructive actions,
     exposed as a small trait/hook or equivalent injectable boundary. The seam
     must distinguish document close from application quit so the prompt wording
     can be specific.
   - `editor-shell` must depend only on the trait/hook. The `rge-editor` binary
     must own the native dialog implementation using its existing `rfd`
     dependency and attach it in every launch mode where the save/open dialogs
     are already attached.
   - File -> Close / `Ctrl+W`: when `command_bus().is_dirty()` is false, preserve
     the current `replace_world(KernelWorld::new())` reset behavior. When dirty,
     cancel/no-hook must leave the world, save source, selection, clipboard, and
     dirty command bus unchanged; explicit discard confirmation must perform the
     existing close reset.
   - File -> Quit / `Ctrl+Q`: when clean, preserve the existing one-shot
     `quit_requested` path. When dirty, cancel/no-hook must leave
     `quit_requested` false; explicit discard confirmation must set the pending
     quit request.
   - `WindowEvent::CloseRequested` must route through the same dirty guard before
     calling `ActiveEventLoop::exit()`. A dirty cancel/no-hook result must keep
     the app open.
   - Confirmation must be discard-or-cancel only. Do not add auto-save,
     save-before-close, Save-As fallback, last-directory memory, async dialog
     work, or multi-document/session shutdown orchestration.
   - Existing save behavior, `SaveSource`, `CommandBus` dirty semantics, undo/redo
     stack behavior, menu definitions, accelerators, host projection, and
     command-palette behavior must remain unchanged except for the guarded
     destructive actions.

   **Done criteria:**
   - Focused `editor-shell` tests cover clean Close unchanged, dirty Close
     cancel/no-hook unchanged, dirty Close discard resets through the existing
     close path, dirty Quit cancel/no-hook leaves no pending quit, and dirty Quit
     discard sets the one-shot pending quit.
   - A focused lifecycle test or factored helper test covers window-close request
     behavior: clean exits, dirty cancel/no-hook does not exit, dirty discard
     exits.
   - Tests prove cancel/no-hook paths do not mutate world entity count,
     `SaveSource`, selection, shell-local clipboard, or `command_bus().is_dirty()`.
   - `editor/rge-editor` attaches the native confirmation implementation in the
     same construction branches that attach open/save dialogs, without adding a
     new Cargo dependency.

   **Verification required:**
   - `cargo test -p rge-editor-shell --lib unsaved`
   - `cargo test -p rge-editor-shell --lib`
   - `cargo check -p rge-editor --bin rge-editor`
   - `cargo +nightly fmt --all -- --check`
   - `git diff --check`

   **Halt conditions:**
   - The implementation requires editing `editor-ui`, `editor-egui-host`,
     `editor-actions`, CAD crates, kernel/runtime crates, plugin
     runtime/discovery/loading code, Cargo manifests/lockfiles, workflows,
     dispatch automation, schemas, ADRs, or architecture-lint config.
   - The implementation requires a new `Command` variant, new accelerator,
     command-route replacement, host-shell FIFO replacement, auto-save flow,
     save-before-close flow, async dialog orchestration, OS clipboard behavior,
     authoritative CAD mutation, or undo/dirty policy change.
   - The dirty guard cannot be expressed over the existing
     `CommandBus::is_dirty()` / save-source state and would require a parallel
     dirty-state model.
   - The binary cannot provide the native confirmation implementation using its
     existing dependency surface, or adding a dependency would be necessary.

112. **[DONE 2026-06-09 via ISSUE-361] Post-unsaved-confirmation Phase 9 next-task source audit.**
   The automation queue is exhausted after task 111. Re-arm automation with a
   docs/source-read audit that selects exactly one bounded Phase 9
   editor-usability implementation follow-up as task 113, or records
   `NEEDS_HUMAN` if the remaining candidates require product/architecture
   policy before code.

   **MAY edit:**
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - generated ISSUE-112 handoff/audit/log artifacts for this dispatch only

   **MUST NOT edit:**
   - Rust source or tests under `crates/**`, `kernel/**`, `runtime/**`, or
     `editor/**`
   - Cargo manifests or `Cargo.lock`
   - GitHub workflows
   - dispatch automation, guard, queue, scheduler, or verification scripts
   - schemas, architecture-lint rules/config, ADR files, packet templates, or
     existing handoff/log artifacts from other dispatches
   - plugin runtime/discovery/loading implementation code

   **Current-state claims / falsification to include in the EXEC packet:**
   - Claim: task 111 completed the unsaved Close/Quit/window-close guard, so
     task 113 must not reselect dirty-close prompting.
     Falsifying search:
     `git grep -n -E "UnsavedChanges|with_unsaved_changes_dialog|handle_close_file_request|handle_quit_request|CloseRequested|MessageDialog|command_bus\\(\\)\\.is_dirty" -- crates/editor-shell/src editor/rge-editor/src Status.md HANDOFF.md plans/BASELINE.md .ai/dispatch.tasks.md`
   - Claim: menu, palette, and accelerator activations still cross the
     host-shell boundary through `MenuCommandHandoff` and
     `EditorShell::route_menu_command`; replacing that route remains broader
     than a UI-only task unless current source shows a tiny safe slice.
     Falsifying search:
     `git grep -n -E "MenuCommandHandoff|drain_and_route_menu_commands|route_menu_command|command_palette_window|enqueue_command_palette_activation|enabled_command_for_shortcut|default_editor_menu" -- crates/editor-egui-host/src crates/editor-shell/src crates/editor-ui/src editor/rge-editor/src`
   - Claim: extension/plugin commands still stop at the task-102 injected
     handler seam; no real plugin runtime/discovery/loading path is wired into
     the editor command route.
     Falsifying search:
     `git grep -n -E "ExtensionCommandHandler|ExtensionCommandEvent|Command::Custom|Command::Plugin|PluginHost|PluginContext|plugin-discovery|runtime-wasmtime|rge_kernel_plugin_host" -- crates/editor-shell/src crates/editor-egui-host/src crates/editor-ui/src editor/rge-editor/src`
   - Claim: remaining stale roadmap candidates must be rechecked from current
     source before selection, especially host FIFO/generalized execution,
     shortcut/keybinding policy, OS/typed clipboard, CAD/editor mutation routes,
     and camera/navigation controls.
     Falsifying search:
     `git grep -n -E "Shortcut Conflicts|shortcut_conflicts|keybinding|has_clipboard_entities|Command::Cut|Command::Copy|Command::Paste|Command::Duplicate|Command::Delete|cad|CadGraph|Command::ResetCamera|Command::ZoomIn|Command::ZoomOut|Command::Custom|Command::Plugin" -- crates/editor-egui-host/src crates/editor-shell/src crates/editor-ui/src editor/rge-editor/src`

   **Candidate classes to compare before selecting task 113:**
   - Host-shell FIFO replacement or generalized registry execution beyond the
     current `MenuCommandHandoff` -> `EditorShell::route_menu_command` path.
   - Extension/plugin command execution beyond the injected handler seam,
     including whether any substrate-only step exists before real runtime or
     discovery/loading work.
   - Keybinding/conflict policy, shortcut diagnostics, or shortcut remapping
     after task 109's read-only help window.
   - OS clipboard or typed editor clipboard behavior beyond the current
     shell-local entity clipboard.
   - Authoritative CAD graph/projection mutation with undo/dirty integration.
   - Camera/navigation UI beyond existing reset/zoom commands and scene-aware
     labels.
   - Any other Phase 9/editor-usability candidate that current source shows is
     smaller and safer than the listed deferred areas.

   **Selection requirements:**
   - Read current docs and source before choosing; do not infer from stale
     backlog prose or from `ai_handoffs/` file names.
   - Compare the full candidate set above and record why the selected follow-up
     is smaller or safer than the deferred alternatives.
   - Append exactly one task 113 with a bounded MAY-edit/MUST-NOT-edit envelope,
     current-state falsification searches, required behavior, done criteria,
     verification, and halt conditions.
   - If no candidate can be defended as one bounded implementation dispatch,
     record `NEEDS_HUMAN` with concrete source evidence instead of
     manufacturing work.
   - Do not implement task 113 during this audit.

   **Verification required:**
   - Required source/doc `git grep` or `rg` searches recorded in the EXEC
     packet or status update.
   - `git diff --check`
   - `git diff --name-only`

   **Halt conditions:**
   - Selecting task 113 would require implementation edits during task 112.
   - Current source shows all plausible next slices require human
     product/architecture policy before implementation.
   - The selected follow-up would need Cargo/workflow/automation/schema/ADR
     edits, real plugin runtime/discovery/loading, command-route replacement,
     OS clipboard integration, authoritative CAD mutation, or undo/dirty policy
     unless those are explicitly scoped as the narrow audit-selected task with
     source-backed safety.

113. **[DONE 2026-06-09 via ISSUE-362] Add host-local shortcut conflict diagnostics in `editor-egui-host`.**
   Add a bounded host-only diagnostics surface for shortcut conflicts that are
   already computed by `editor-ui`'s menu registry and projected into
   `editor-egui-host` as `ProjectedMainMenu.conflicts`. The current host only
   exposes a transient inline `"Shortcut Conflicts"` menu when conflicts exist;
   this follow-up should make those diagnostics easier to inspect without
   changing conflict policy, remapping, command routing, or plugin execution.

   **Implemented via ISSUE-362:** `editor-egui-host` now projects conflict rows
   through a focused host helper and opens a persistent egui `Shortcut
   Conflicts` diagnostics window from a non-command top-bar affordance when the
   current `ProjectedMainMenu.conflicts` is non-empty. Rows are copied only from
   the projected conflict data, preserving shortcut display strings and entry-id
   order; the window closes/clears when the current projection has no conflicts.
   Focused tests cover multiple conflicts, default-menu no-conflict behavior,
   first-winner conflict semantics, and read-only host state behavior. No
   `editor-ui`, `editor-shell`, command routing, conflict policy, Cargo,
   plugin-runtime, clipboard, CAD, or undo/dirty behavior changed.

   **MAY edit:**
   - `crates/editor-egui-host/src/lib.rs`
   - `crates/editor-egui-host/src/menu.rs`
   - `crates/editor-egui-host/src/menu_tests.rs`
   - new focused `crates/editor-egui-host/src/shortcut_conflicts.rs` helper
     module if it keeps `lib.rs` / `menu.rs` cohesive
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - generated ISSUE-113 handoff/audit/log artifacts for this dispatch only

   **MUST NOT edit:**
   - `crates/editor-ui/**`
   - `crates/editor-shell/**`
   - `crates/editor-actions/**`
   - `editor/**`
   - CAD crates, kernel crates, runtime crates, plugin runtime/discovery/loading
     code, or architecture-lint code
   - Cargo manifests or `Cargo.lock`
   - GitHub workflows
   - dispatch automation, guard, queue, scheduler, or verification scripts
   - schemas, ADR files, packet templates, or existing handoff/log artifacts
     from other dispatches

   **Current-state claims / falsification to include in the TASK packet:**
   - Claim: shortcut-conflict data already exists in the menu registry and is
     projected into the host, so task 113 must not add a second shortcut
     registry or hard-code shortcut rows.
     Falsifying search:
     `git grep -n -E "ProjectedShortcutConflict|conflicts: Vec|Shortcut Conflicts|shortcut_conflicts_project_as_host_diagnostics|ShortcutConflict|detect_conflicts" -- crates/editor-egui-host/src crates/editor-ui/src`
   - Claim: task 109's keyboard-shortcuts help window is already present, so
     task 113 must not reselect another generic shortcut-help surface.
     Falsifying search:
     `git grep -n -E "shortcut_help|Keyboard Shortcuts|shortcut_help_rows|shortcut_help_window|view_menu_affordance" -- crates/editor-egui-host/src`
   - Claim: menu and palette command activations still cross
     `MenuCommandHandoff` into `EditorShell::route_menu_command`; conflict
     diagnostics must be non-activating host UI and must not replace or bypass
     that route.
     Falsifying search:
     `git grep -n -E "MenuCommandHandoff|menu_commands\\.push|enqueue_command_palette_activation|command_palette_window|route_menu_command" -- crates/editor-egui-host/src crates/editor-shell/src crates/editor-ui/src`
   - Claim: the default menu has no executable shortcut conflicts today, while
     synthetic extension/plugin conflicts already project as host diagnostics.
     Falsifying search:
     `git grep -n -E "executable_accelerators_have_no_conflicts|shortcut_conflicts_project_as_host_diagnostics|shortcut_conflicts_surface_in_resolve|Command::Custom|Command::Plugin|plugins_menu_point" -- crates/editor-egui-host/src crates/editor-ui/src`

   **Required behavior:**
   - Replace or augment the current inline `"Shortcut Conflicts"` dropdown with
     a host-local diagnostics affordance that opens a persistent egui window or
     similarly inspectable host surface when `ProjectedMainMenu.conflicts` is
     non-empty.
   - Source every displayed row from `ProjectedMainMenu.conflicts`, preserving
     the registry-provided shortcut display and entry-id order. Multiple
     conflicts must render deterministically.
   - The surface must be read-only diagnostics: opening, closing, or inspecting
     it must not enqueue `MenuCommandHandoff` commands, change command-palette
     recents/pins/filter/selection state, toggle keyboard-shortcuts help, or
     mutate editor-shell state.
   - When the resolved default menu has no conflicts, no conflict window should
     appear and no stale conflict rows should remain visible.
   - Preserve `MenuRegistry::resolve` conflict semantics: the first registered
     shortcut winner remains unchanged and conflicts remain non-fatal data.
   - Do not add a new `Command`, shortcut, keybinding editor/remapper, conflict
     policy, fatal conflict gate, OS/typed clipboard behavior, CAD mutation,
     undo/dirty integration, command-route replacement, or plugin runtime /
     discovery / loading behavior.

   **Done criteria:**
   - Focused host tests prove conflict rows are derived from
     `ProjectedMainMenu.conflicts`, preserve shortcut display plus entry-id
     order, and handle more than one conflict deterministically.
   - Tests prove the default `default_editor_menu()` projection remains
     conflict-free.
   - Tests or factored helper coverage prove toggling/closing the conflict
     diagnostics surface does not enqueue menu commands and does not mutate
     command-palette or keyboard-shortcuts-help state.
   - Existing command-palette, pinned/recent, shortcut-help, menu projection,
     and `MenuCommandHandoff` tests still pass without behavior changes outside
     the conflict diagnostics surface.

   **Verification required:**
   - `cargo test -p rge-editor-egui-host --lib shortcut_conflict`
   - `cargo test -p rge-editor-egui-host --lib`
   - `cargo check -p rge-editor-egui-host --lib`
   - `cargo +nightly fmt --all -- --check`
   - `git diff --check`

   **Halt conditions:**
   - The implementation requires editing `editor-ui`, `editor-shell`,
     `editor-actions`, CAD crates, kernel/runtime crates, plugin
     runtime/discovery/loading code, Cargo manifests/lockfiles, workflows,
     dispatch automation, schemas, ADRs, or architecture-lint config.
   - The implementation requires a new `Command`, new accelerator, keyboard
     remapping/editor UI, conflict fatality policy, command-route replacement,
     host-shell FIFO replacement, real plugin execution, OS/typed clipboard
     behavior, authoritative CAD mutation, or undo/dirty policy change.
   - Conflict rows cannot be sourced from existing `ProjectedMainMenu.conflicts`
     and would require a parallel hard-coded shortcut table or a second
     registry.
   - The default menu no-conflict invariant would need to be weakened to make
     the diagnostics visible.

114. **[DONE 2026-06-09 via ISSUE-364 - selected task 115 viewport mouse-wheel zoom] Post-shortcut-conflict-diagnostics Phase 9 next-task source audit.**
   The automation queue is exhausted after task 113 (ISSUE-362 / commit `66bc010`,
   host-local shortcut conflict diagnostics). Re-arm automation with a docs/
   source-read audit that selects exactly one bounded Phase 9 editor-usability
   implementation follow-up as task 115, or records `NEEDS_HUMAN` if the remaining
   candidates require product/architecture policy before code.

   Completed via ISSUE-364 as a source/docs audit only. Current source confirms
   task 113's shortcut-conflict diagnostics are present, menu/palette/accelerator
   activation still intentionally crosses `MenuCommandHandoff` into
   `EditorShell::route_menu_command`, extension/plugin commands still stop at the
   task-102 `ExtensionCommandHandler` seam with no editor route to real plugin
   runtime/discovery/loading, and shell-local entity Cut/Copy/Paste/Delete/
   Duplicate already exists without OS clipboard, typed component, CAD, undo, or
   dirty semantics. The selected follow-up is task 115: a viewport-only
   mouse-wheel camera zoom slice in `editor-shell`, using the existing camera zoom
   behavior and existing viewport hit-test boundary. Host FIFO/generalized command
   execution, real plugin execution, shortcut remapping/conflict policy, OS/typed
   clipboard, authoritative CAD/editor mutation, orbit/pan/drag navigation, new
   commands/accelerators, and route replacement remain deferred.

   This is a SOURCE AUDIT ONLY: read current source, compare candidate classes,
   and append exactly one task 115 (or `NEEDS_HUMAN`). It selects work; it does
   not do work.

   **MAY edit:**
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - generated ISSUE-<n> handoff/audit/log artifacts for this dispatch only

   **MAY read (read-only, classification only):**
   - `docs/EXTERNAL_ENGINE_LESSONS.md` — only to TAG the selected candidate by
     kernel pressure (composition / declarative-content / subsystem-boundary /
     execution-time / authority-arbitration) when describing task 115. The ledger
     is a reporter, NOT implementation authority.

   **MUST NOT edit:**
   - Rust source or tests under `crates/**`, `kernel/**`, `runtime/**`, or `editor/**`
   - Cargo manifests or `Cargo.lock`
   - GitHub workflows
   - dispatch automation, guard, queue, scheduler, or verification scripts
   - schemas, architecture-lint rules/config, ADR files, packet templates, or
     existing handoff/log artifacts from other dispatches
   - plugin runtime/discovery/loading implementation code
   - `docs/EXTERNAL_ENGINE_LESSONS.md` — read-only above; this audit MUST NOT add
     ledger rows, mine Bevy or any donor engine, wire the deferred reporter, or
     treat the ledger as implementation authority

   **Current-state claims / falsification to include in the EXEC packet (re-grep
   current source before trusting any roadmap text):**
   - Claim: task 113 shipped shortcut CONFLICT diagnostics, so task 115 must not
     reselect shortcut-conflict surfacing.
     Falsifying search:
     `git grep -n -E "shortcut_conflict|ShortcutConflict|ProjectedMainMenu.*conflicts|conflict_diagnostics"`
   - Claim: menu/palette/accelerator activations still cross the host<->shell
     boundary via `MenuCommandHandoff` / `EditorShell::route_menu_command`;
     replacing that route is broader than a UI-only task unless current source
     shows a tiny safe slice.
     Falsifying search:
     `git grep -n -E "MenuCommandHandoff|route_menu_command|command_palette_window"`
   - Claim: extension/plugin commands still stop at the injected handler seam; no
     real plugin runtime/discovery/loading is wired into the editor command route.
     Falsifying search:
     `git grep -n -E "ExtensionCommandHandler|Command::Custom|PluginHost|PluginContext|plugin-discovery"`
   - Claim: remaining stale roadmap candidates must be rechecked from current
     source before selection (host FIFO/generalized execution, keybinding/remap
     policy, OS/typed clipboard, CAD/editor mutation routes, camera/navigation).
     Falsifying search:
     `git grep -n -E "has_clipboard_entities|Command::(Cut|Copy|Paste|Duplicate)|keybinding|camera_controls|navigation"`

   **Candidate classes to compare before selecting task 115:**
   - Host-shell FIFO replacement / generalized registry execution beyond the
     current `MenuCommandHandoff` -> `EditorShell::route_menu_command` path.
   - Extension/plugin command execution beyond the injected handler seam,
     including whether any substrate-only step precedes real runtime or
     discovery/loading work.
   - Keybinding/conflict-resolution policy or shortcut remapping after task 113's
     read-only conflict diagnostics.
   - OS/typed clipboard for editor entities.
   - CAD/editor mutation routes; camera/navigation controls.

   **Output:** append exactly ONE task 115 with a source-backed safety rationale
   and explicit MAY / MUST-NOT / Done / Verification / Halt sections like the prior
   tasks, OR record `NEEDS_HUMAN` with the blocking product/architecture policy
   question.

   **Halt conditions (hard):**
   - This audit begins IMPLEMENTING task 115 — writing Rust, editing
     editor/kernel/runtime source, adding a `Command`/accelerator, or any code
     change — instead of only selecting + appending it. Selecting work is in
     scope; DOING the selected work is NOT; that is a separate future tick.
   - No bounded, source-safe candidate exists → record `NEEDS_HUMAN` rather than
     forcing a selection.
   - Describing task 115 would require editing any MUST-NOT path.

115. **[DONE 2026-06-10 via ISSUE-367 / PR #368 / commit `265d540`] Add viewport-only mouse-wheel camera zoom in `editor-shell`.**
   Add the smallest interactive camera-navigation slice now that View menu
   Reset/Zoom commands already route through the canonical menu/accelerator path.
   Current source has `EditorShell::zoom_camera_in`, `zoom_camera_out`, and
   `reset_camera`; `WindowEvent::MouseInput` explicitly leaves scroll/drag/hover
   as later work; and no `MouseWheel` / `MouseScrollDelta` branch exists in the
   scoped editor source. This task should route mouse-wheel scroll over the
   transparent Viewport tab body to the existing zoom behavior without adding a
   command, accelerator, menu item, command-route replacement, or broader camera
   controller.

   **Safety rationale:** this is narrower than the deferred alternatives because
   it uses the existing `editor-shell` camera intent and the existing
   `is_pointer_over_viewport_tab()` host boundary. It does not require changing
   `editor-egui-host`, `editor-ui`, command registry definitions, plugin
   runtime/discovery/loading, OS clipboard, CAD graph/projection mutation, or
   undo/dirty policy. Treat it as a camera/navigation usability slice, not as a
   general input-router redesign.

   **MAY edit:**
   - `crates/editor-shell/src/lifecycle/mod.rs`
   - `crates/editor-shell/src/lifecycle/tests.rs`
   - new focused `crates/editor-shell/src/lifecycle/viewport_navigation.rs` helper
     module if it keeps `mod.rs` cohesive
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - generated ISSUE-115 handoff/audit/log artifacts for this dispatch only

   **MUST NOT edit:**
   - `crates/editor-egui-host/**`
   - `crates/editor-ui/**`
   - `crates/editor-actions/**`
   - `crates/cad-core/**`
   - `crates/cad-projection/**`
   - `kernel/**`
   - `runtime/**`
   - `editor/rge-editor/**`
   - Cargo manifests or `Cargo.lock`
   - GitHub workflows
   - dispatch automation, guard, queue, scheduler, or verification scripts
   - schemas, ADR files, architecture-lint rules/config, packet templates, or
     existing handoff/log artifacts from other dispatches
   - plugin runtime/discovery/loading implementation code

   **Done criteria:**
   - `WindowEvent::MouseWheel` over the current Viewport tab body zooms the
     `EditorShell` camera in for positive wheel delta and out for negative wheel
     delta, using the existing camera zoom semantics or a shell-private helper
     with equivalent target/direction/clip-plane preservation.
   - Wheel events over Inspector panels, menus, tab chrome, text fields, or any
     non-viewport egui-owned region do not zoom the scene.
   - No cursor / no viewport-rect / no egui host cases are no-ops, not panics.
   - The task does not add or modify `Command`, menu registry entries,
     accelerators, command-palette behavior, shortcut conflict policy,
     extension/plugin execution, OS clipboard behavior, CAD graph/projection
     mutation, undo/dirty semantics, or drag/orbit/pan navigation.

   **Verification required:**
   - `git grep -n -E "MouseWheel|MouseScrollDelta|zoom_camera_in|zoom_camera_out|is_pointer_over_viewport_tab|should_fire_face_pick" -- crates/editor-shell/src crates/editor-egui-host/src` before implementation, summarized in the EXEC packet.
   - Focused `rge-editor-shell` tests proving wheel-delta direction mapping,
     viewport-only gating, no-cursor/no-viewport no-op behavior, and preservation
     of the existing zoom target/direction invariants.
   - `cargo test -p rge-editor-shell --lib`
   - `cargo check -p rge-editor-shell --lib`
   - `cargo +nightly fmt --all -- --check`
   - `git diff --check`

   **Halt conditions:**
   - Implementing viewport wheel zoom requires editing outside the MAY list.
   - The implementation needs a new `Command`, accelerator, menu item, command
     route, command-palette change, shortcut remapping/conflict policy, or host/UI
     registry edit.
   - The implementation starts orbit, pan, drag navigation, camera persistence,
     OS/typed clipboard, authoritative CAD mutation, undo/dirty policy, plugin
     runtime/discovery/loading, or generalized input routing.
   - Existing viewport hit-test state is insufficient to distinguish viewport
     scroll from panel/menu/tab scroll without editing `editor-egui-host`; halt
     rather than broadening scope.

116. **[DONE 2026-06-11 via ISSUE-369] Post-viewport-wheel-zoom Phase 9 next-task source audit.**
   The automation queue is exhausted after task 115 (ISSUE-367 / PR #368,
   viewport-only mouse-wheel camera zoom). Re-arm with a docs/source-read audit
   that selects exactly one bounded Phase 9 editor-usability implementation
   follow-up as task 117, or records `NEEDS_HUMAN` if remaining candidates need
   product or architecture policy before code.

   This is a SOURCE AUDIT ONLY: read current source, compare candidate classes,
   and append exactly one task 117 (or `NEEDS_HUMAN`). It selects work; it does
   not do work.

   **MAY edit:**
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - generated ISSUE-<n> handoff/audit/log artifacts for this dispatch only

   **MAY read (read-only, classification only):**
   - `docs/EXTERNAL_ENGINE_LESSONS.md` to tag the selected candidate by kernel
     pressure when useful. The ledger is a reporter, not implementation
     authority.

   **MUST NOT edit:**
   - Rust source or tests under `crates/**`, `kernel/**`, `runtime/**`, or
     `editor/**`
   - Cargo manifests or `Cargo.lock`
   - GitHub workflows
   - dispatch automation, guard, queue, scheduler, or verification scripts
   - schemas, architecture-lint rules/config, ADR files, packet templates, or
     existing handoff/log artifacts from other dispatches
   - plugin runtime/discovery/loading implementation code
   - `docs/EXTERNAL_ENGINE_LESSONS.md`

   **Current-state claims / falsification to include in the EXEC packet (re-grep
   current source before trusting any roadmap text):**
   - Claim: task 115 shipped viewport-only `MouseWheel` zoom; do not reselect
     wheel zoom.
     Falsifying search:
     `git grep -n -E "MouseWheel|MouseScrollDelta|zoom_camera_in|zoom_camera_out|is_pointer_over_viewport_tab" -- crates/editor-shell/src crates/editor-egui-host/src`
   - Claim: menu/palette/accelerator activations still cross the host<->shell
     boundary via `MenuCommandHandoff` / `EditorShell::route_menu_command`;
     replacing that route is broader than a UI-only task unless current source
     shows a tiny safe slice.
     Falsifying search:
     `git grep -n -E "MenuCommandHandoff|drain_and_route_menu_commands|route_menu_command|command_palette_window"`
   - Claim: extension/plugin commands still stop at the injected handler seam;
     no real plugin runtime/discovery/loading is wired into the editor command
     route.
     Falsifying search:
     `git grep -n -E "ExtensionCommandHandler|Command::Custom|Command::Plugin|PluginHost|PluginContext|plugin-discovery|runtime-wasmtime"`
   - Claim: remaining stale roadmap candidates must be rechecked from current
     source before selection.
     Falsifying search:
     `git grep -n -E "MouseInput|CursorMoved|orbit|pan|clipboard|keybinding|ShortcutConflict|Command::(Cut|Copy|Paste|Duplicate|Delete)|CommandBus|Action"`

   **Candidate classes to compare before selecting task 117:**
   - Next camera/navigation slice after wheel zoom, such as viewport-only orbit,
     pan, frame/focus, or a source-backed decision to stop camera work.
   - Host-shell FIFO replacement / generalized registry execution beyond the
     current `MenuCommandHandoff` -> `EditorShell::route_menu_command` path.
   - Extension/plugin command execution beyond the injected handler seam,
     including whether any substrate-only precursor precedes real runtime or
     discovery/loading work.
   - Keybinding/conflict-resolution policy or shortcut remapping after task
     113's read-only conflict diagnostics.
   - OS/typed clipboard for editor entities.
   - CAD/editor mutation routes through the authoritative command bus.

   **Output:** append exactly ONE task 117 with a source-backed safety rationale
   and explicit MAY / MUST-NOT / Done / Verification / Halt sections like the
   prior tasks, OR record `NEEDS_HUMAN` with the blocking product/architecture
   policy question.

   **Halt conditions (hard):**
   - This audit begins IMPLEMENTING task 117 -- writing Rust, editing
     editor/kernel/runtime source, adding a `Command`/accelerator, or any code
     change -- instead of only selecting + appending it.
   - No bounded, source-safe candidate exists -> record `NEEDS_HUMAN` rather
     than forcing a selection.
   - Describing task 117 would require editing any MUST-NOT path.

117. **[DONE 2026-06-13 via ISSUE-371] Add viewport-only right-button camera orbit in `editor-shell`.**
   Add the smallest next camera/navigation slice after task 115's viewport
   mouse-wheel zoom. Current source now has `WindowEvent::MouseWheel` routed
   through `zoom_camera_for_viewport_mouse_wheel`, `cursor_pos` tracking from
   `WindowEvent::CursorMoved`, `is_pointer_over_viewport_tab()` for the host
   viewport boundary, and a `WindowEvent::MouseInput` branch that already
   treats non-left buttons as no-ops. This task should use those existing
   pieces to make a right-button drag that starts over the transparent Viewport
   tab orbit the camera around its current target.

   **Safety rationale:** this is narrower than the deferred alternatives
   because it stays inside `editor-shell` camera/input coordination and does
   not alter the menu/palette/accelerator route, command registry, host-shell
   FIFO, plugin-command seam, shortcut policy, OS clipboard, CAD graph or
   projection state, or undo/dirty semantics. The required source audit found
   that `MenuCommandHandoff` plus `EditorShell::route_menu_command` remain the
   command execution path, extension/plugin commands still stop at the injected
   handler seam with no editor-side plugin runtime/discovery/loading matches,
   shortcut conflicts are still diagnostic data rather than remap policy, the
   clipboard is shell-local, and authoritative CAD/editor mutation through
   `CommandBus` would reopen broader mutation/undo policy. A viewport-only
   right-button orbit can be specified as camera state math plus existing
   viewport hit-test gating, with no new command surface.

   **MAY edit:**
   - `crates/editor-shell/src/camera.rs`
   - `crates/editor-shell/src/lifecycle/mod.rs`
   - `crates/editor-shell/src/lifecycle/tests.rs`
   - new focused `crates/editor-shell/src/lifecycle/viewport_navigation.rs`
     helper module if it keeps `mod.rs` cohesive
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - `ai_handoffs/ISSUE-117_*.md`
   - `ai_handoffs/ISSUE-117_*.meta.json`
   - `.ai/dispatch-ISSUE-117/**`
   - `ai_dispatch_logs/log_*ISSUE-117*.md`

   **MUST NOT edit:**
   - `crates/editor-egui-host/**`
   - `crates/editor-ui/**`
   - `crates/editor-actions/**`
   - `crates/cad-core/**`
   - `crates/cad-projection/**`
   - `kernel/**`
   - `runtime/**`
   - `editor/rge-editor/**`
   - Cargo manifests or `Cargo.lock`
   - GitHub workflows
   - dispatch automation, guard, queue, scheduler, or verification scripts
   - schemas, ADR files, architecture-lint rules/config, packet templates, or
     existing handoff/log artifacts from other dispatches
   - plugin runtime/discovery/loading implementation code

   **Done criteria:**
   - Pressing the right mouse button while the pointer is over the current
     Viewport tab body starts a shell-private orbit drag; moving the cursor
     while that drag is active rotates `EditorShell::editor_camera.eye` around
     `editor_camera.target`.
   - The orbit preserves camera target, eye-target distance, FOV, clip planes,
     and finite camera state. Degenerate eye-target/up vectors are no-ops or use
     an existing/default finite fallback; they must not panic.
   - Releasing the right mouse button stops the orbit drag even if the release
     event is egui-consumed. Presses that start outside the Viewport tab body,
     presses before any cursor position exists, and shells without an egui host
     do not start an orbit.
   - Existing left-click face-pick behavior, viewport mouse-wheel zoom behavior,
     View menu Reset/Zoom commands, command-palette/menu routing, and keyboard
     accelerator routing remain unchanged.
   - The task does not add or modify `Command`, menu registry entries,
     accelerators, command-palette behavior, shortcut conflict/remap policy,
     extension/plugin execution, OS clipboard behavior, CAD graph/projection
     mutation, undo/dirty semantics, wheel zoom semantics, pan, frame/focus,
     camera persistence, or generalized input routing.

   **Verification required:**
   - Before implementation, summarize:
     `git grep -n -E "MouseWheel|MouseScrollDelta|MouseInput|CursorMoved|is_pointer_over_viewport_tab|zoom_camera_for_viewport_mouse_wheel|should_fire_face_pick|EditorCameraState" -- crates/editor-shell/src`
   - Before implementation, summarize:
     `git grep -n -E "MenuCommandHandoff|drain_and_route_menu_commands|route_menu_command|Command::ResetCamera|Command::ZoomIn|Command::ZoomOut|command_palette_window" -- crates/editor-egui-host/src crates/editor-shell/src crates/editor-ui/src`
   - Focused `rge-editor-shell` tests proving right-button press/start gating,
     cursor-delta orbit math, release stop behavior, no-cursor/no-host no-op
     behavior, non-viewport no-op behavior, left-click face-pick preservation,
     and wheel-zoom preservation.
   - `cargo test -p rge-editor-shell --lib`
   - `cargo check -p rge-editor-shell --lib`
   - `cargo +nightly fmt --all -- --check`
   - `git diff --check`

   **Halt conditions:**
   - Implementing viewport-only right-button orbit requires editing outside the
     MAY list.
   - The implementation needs a new `Command`, accelerator, menu item, command
     route, command-palette change, shortcut remapping/conflict policy, host/UI
     registry edit, plugin runtime/discovery/loading, OS/typed clipboard,
     authoritative CAD mutation, undo/dirty policy, or Cargo change.
   - The implementation starts pan, frame/focus, drag-selection, camera
     persistence, pointer-capture/window-grab policy, generalized input routing,
     or camera/navigation work beyond the selected right-button orbit slice.
   - Existing viewport hit-test state is insufficient to distinguish viewport
     right-button drags from panel/menu/tab interactions without editing
     `editor-egui-host`; halt rather than broadening scope.

118. **[DONE 2026-06-13 via ISSUE-372 - selected task 119 viewport pan] Post-viewport-orbit Phase 9 next-task source audit.**
   The automation queue is exhausted after task 117 (ISSUE-371, viewport-only
   right-button camera orbit). Re-arm with a docs/source-read audit that
   selects exactly one bounded Phase 9 editor-usability implementation
   follow-up as task 119, or records `NEEDS_HUMAN` if remaining candidates need
   product or architecture policy before code.

   Completed via ISSUE-372 as a source/docs audit only. Current source confirms
   task 117's right-button orbit and task 115's mouse-wheel zoom are present, so
   neither is reselected. Menu, command-palette, and accelerator activations
   still cross `MenuCommandHandoff` into `EditorShell::route_menu_command`;
   extension/plugin commands still stop at the injected
   `ExtensionCommandHandler` seam with no editor route-owner matches for plugin
   runtime/discovery/loading; shortcut conflicts remain diagnostic data rather
   than remap policy; the clipboard is shell-local; and authoritative CAD/editor
   mutation through `CommandBus` remains a broader mutation/undo-policy
   candidate. The selected follow-up is task 119: viewport-only middle-button
   camera pan in `editor-shell`, reusing the existing cursor tracking, viewport
   hit-test state, camera state, and lifecycle-private navigation helper
   boundary. Host-shell FIFO/generalized command execution, real plugin command
   execution, shortcut remapping/conflict policy, OS/typed clipboard,
   authoritative CAD/editor mutation, frame/focus, and broader camera-controller
   work remain deferred.

   This is a SOURCE AUDIT ONLY: read current source, compare candidate classes,
   and append exactly one task 119 (or `NEEDS_HUMAN`). It selects work; it does
   not do work.

   **MAY edit:**
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - generated ISSUE-<n> handoff/audit/log artifacts for this dispatch only

   **MAY read (read-only, classification only):**
   - `docs/EXTERNAL_ENGINE_LESSONS.md` to tag the selected candidate by kernel
     pressure when useful. The ledger is a reporter, not implementation
     authority.

   **MUST NOT edit:**
   - Rust source or tests under `crates/**`, `kernel/**`, `runtime/**`, or
     `editor/**`
   - Cargo manifests or `Cargo.lock`
   - GitHub workflows
   - dispatch automation, guard, queue, scheduler, or verification scripts
   - schemas, architecture-lint rules/config, ADR files, packet templates, or
     existing handoff/log artifacts from other dispatches
   - plugin runtime/discovery/loading implementation code
   - `docs/EXTERNAL_ENGINE_LESSONS.md`

   **Current-state claims / falsification to include in the EXEC packet
   (re-grep current source before trusting any roadmap text):**
   - Claim: task 117 shipped viewport-only right-button orbit; do not reselect
     right-button orbit.
     Falsifying search:
     `git grep -n -E "MouseButton::Right|ViewportOrbitDrag|viewport_orbit_drag|orbit_around_target|CursorMoved|MouseInput" -- crates/editor-shell/src`
   - Claim: menu/palette/accelerator activations still cross the host<->shell
     boundary via `MenuCommandHandoff` / `EditorShell::route_menu_command`;
     replacing that route is broader than a UI-only task unless current source
     shows a tiny safe slice.
     Falsifying search:
     `git grep -n -E "MenuCommandHandoff|drain_and_route_menu_commands|route_menu_command|command_palette_window"`
   - Claim: extension/plugin commands still stop at the injected handler seam;
     no real plugin runtime/discovery/loading is wired into the editor command
     route.
     Falsifying search:
     `git grep -n -E "ExtensionCommandHandler|Command::Custom|Command::Plugin|PluginHost|PluginContext|plugin-discovery|runtime-wasmtime"`
   - Claim: remaining stale roadmap candidates must be rechecked from current
     source before selection.
     Falsifying search:
     `git grep -n -E "MouseInput|pan|frame|focus|clipboard|keybinding|ShortcutConflict|Command::(Cut|Copy|Paste|Duplicate|Delete)|CommandBus|Action"`

   **Candidate classes to compare before selecting task 119:**
   - Next camera/navigation slice after wheel zoom and right-button orbit, such
     as viewport-only pan, frame/focus, or a source-backed decision to stop
     camera work.
   - Host-shell FIFO replacement / generalized registry execution beyond the
     current `MenuCommandHandoff` -> `EditorShell::route_menu_command` path.
   - Extension/plugin command execution beyond the injected handler seam,
     including whether any substrate-only precursor precedes real runtime or
     discovery/loading work.
   - Keybinding/conflict-resolution policy or shortcut remapping after task
     113's read-only conflict diagnostics.
   - OS/typed clipboard for editor entities.
   - CAD/editor mutation routes through the authoritative command bus.

   **Output:** append exactly ONE task 119 with a source-backed safety rationale
   and explicit MAY / MUST-NOT / Done / Verification / Halt sections like the
   prior tasks, OR record `NEEDS_HUMAN` with the blocking product/architecture
   policy question.

   **Halt conditions (hard):**
   - This audit begins IMPLEMENTING task 119 -- writing Rust, editing
     editor/kernel/runtime source, adding a `Command`/accelerator, or any code
     change -- instead of only selecting + appending it.
   - No bounded, source-safe candidate exists -> record `NEEDS_HUMAN` rather
     than forcing a selection.
   - Describing task 119 would require editing any MUST-NOT path.

119. **[DONE 2026-06-13 via ISSUE-373] Add viewport-only middle-button camera pan in `editor-shell`.**
   Add the smallest next camera/navigation slice after task 115's viewport
   mouse-wheel zoom and task 117's viewport-only right-button orbit. Current
   source has `WindowEvent::CursorMoved` tracking `cursor_pos`,
   `is_pointer_over_viewport_tab()` for the host viewport boundary,
   `ViewportOrbitDrag` in `lifecycle/viewport_navigation.rs`, and a
   `WindowEvent::MouseInput` branch that currently handles left-click face-pick
   plus right-button orbit while leaving middle/generalized drag work as a
   non-goal. This task should make a middle-button drag that starts only over
   the transparent Viewport tab body and pans the camera in its view plane by
   translating `EditorCameraState.eye` and `EditorCameraState.target` together.

   **Safety rationale:** this is narrower than the deferred alternatives
   because it stays inside `editor-shell` camera/input coordination and extends
   the same private navigation boundary used by right-button orbit. It does not
   alter the menu/palette/accelerator route, command registry, host-shell FIFO,
   plugin-command seam, shortcut policy, OS clipboard, CAD graph or projection
   state, or undo/dirty semantics. Frame/focus is deferred because `Reset Camera`
   already frames available scene bounds through the existing View command route,
   while a richer focus target policy would need product decisions. A
   viewport-only middle-button pan can be specified as camera-state math plus
   existing viewport hit-test gating, with no new command surface.

   **MAY edit:**
   - `crates/editor-shell/src/camera.rs`
   - `crates/editor-shell/src/lifecycle/mod.rs`
   - `crates/editor-shell/src/lifecycle/viewport_navigation.rs`
   - `crates/editor-shell/src/lifecycle/tests.rs`
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`

   **MAY add:**
   - `ai_handoffs/ISSUE-119_*.md`
   - `ai_handoffs/ISSUE-119_*.meta.json`
   - `.ai/dispatch-ISSUE-119/**`
   - `ai_dispatch_logs/log_*ISSUE-119*.md`

   **MUST NOT edit:**
   - `crates/editor-egui-host/**`
   - `crates/editor-ui/**`
   - `crates/editor-actions/**`
   - `crates/cad-core/**`
   - `crates/cad-projection/**`
   - `kernel/**`
   - `runtime/**`
   - `editor/rge-editor/**`
   - Cargo manifests or `Cargo.lock`
   - GitHub workflows
   - dispatch automation, guard, queue, scheduler, or verification scripts
   - schemas, ADR files, architecture-lint rules/config, packet templates, or
     existing handoff/log artifacts from other dispatches
   - plugin runtime/discovery/loading implementation code
   - `docs/EXTERNAL_ENGINE_LESSONS.md`

   **Done criteria:**
   - Pressing the middle mouse button while the pointer is over the current
     Viewport tab body starts a shell-private pan drag; moving the cursor while
     that drag is active translates both `EditorShell::editor_camera.eye` and
     `editor_camera.target` in the camera view plane.
   - The pan preserves the eye-target offset, FOV, clip planes, up vector, and
     finite camera state. Degenerate eye-target/up vectors, non-finite cursor
     positions, and zero deltas are no-ops rather than panics.
   - Releasing the middle mouse button stops the pan drag even if the release
     event is egui-consumed. Presses that start outside the Viewport tab body,
     presses before any cursor position exists, and shells without an egui host
     do not start a pan.
   - Existing left-click face-pick behavior, viewport mouse-wheel zoom behavior,
     right-button orbit behavior, View menu Reset/Zoom commands,
     command-palette/menu routing, and keyboard accelerator routing remain
     unchanged.
   - The task does not add or modify `Command`, menu registry entries,
     accelerators, command-palette behavior, shortcut conflict/remap policy,
     host-shell FIFO behavior, generalized command/registry execution,
     `EditorShell::route_menu_command`, extension/plugin execution, OS
     clipboard behavior, CAD graph/projection mutation, undo/dirty semantics,
     wheel zoom semantics, orbit semantics, frame/focus behavior, camera
     persistence, pointer capture/window grab policy, or generalized input
     routing.

   **Verification required:**
   - Before implementation, summarize:
     `git grep -n -E "ViewportOrbitDrag|viewport_orbit_drag|MouseButton::Right|MouseButton::Middle|CursorMoved|MouseInput|is_pointer_over_viewport_tab|zoom_camera_for_viewport_mouse_wheel|should_fire_face_pick|EditorCameraState" -- crates/editor-shell/src`
   - Before implementation, summarize:
     `git grep -n -E "MenuCommandHandoff|drain_and_route_menu_commands|route_menu_command|Command::ResetCamera|Command::ZoomIn|Command::ZoomOut|command_palette_window|keycode_to_shortcut" -- crates/editor-egui-host/src crates/editor-shell/src crates/editor-ui/src`
   - Before implementation, summarize:
     `git grep -n -E "ExtensionCommandHandler|Command::Custom|Command::Plugin|PluginHost|PluginContext|plugin-discovery|runtime-wasmtime|has_clipboard_entities|clipboard|ShortcutConflict|keybinding|CommandBus|\\bAction\\b" -- crates/editor-shell/src crates/editor-egui-host/src crates/editor-ui/src crates/editor-actions/src crates/cad-core/src crates/cad-projection/src editor/rge-editor/src`
   - Focused `rge-editor-shell` tests proving middle-button press/start gating,
     cursor-delta pan math, release stop behavior, no-cursor/no-host no-op
     behavior, non-viewport no-op behavior, non-finite no-op behavior,
     left-click face-pick preservation, wheel-zoom preservation, and
     right-button orbit preservation.
   - `cargo test -p rge-editor-shell --lib viewport_middle_button_pan`
   - `cargo test -p rge-editor-shell --lib`
   - `cargo check -p rge-editor-shell --lib`
   - `cargo +nightly fmt --all -- --check`
   - `git diff --check`

   **Halt conditions:**
   - Implementing viewport-only middle-button pan requires editing outside the
     MAY list.
   - The implementation needs a new `Command`, accelerator, menu item, command
     route, command-palette change, shortcut remapping/conflict policy, host/UI
     registry edit, host-shell FIFO replacement, generalized command/registry
     execution, plugin runtime/discovery/loading, OS/typed clipboard,
     authoritative CAD mutation, undo/dirty policy, or Cargo change.
   - The implementation starts frame/focus, drag-selection, camera persistence,
     pointer-capture/window-grab policy, generalized input routing, or
     camera/navigation work beyond the selected middle-button pan slice.
   - Existing viewport hit-test state is insufficient to distinguish viewport
     middle-button drags from panel/menu/tab interactions without editing
     `editor-egui-host`; halt rather than broadening scope.

120. **[DONE 2026-06-13 via manual salvage of failed ISSUE-374 — selected task 121 left-double-click frame-all camera gesture] Post-viewport-pan Phase 9 next-task source audit.**
   Re-arm the automation after task 119 (ISSUE-373 viewport-only middle-button
   pan). This is a docs/source-read-only audit that must inspect current source
   and status after viewport wheel zoom, right-button orbit, and middle-button
   pan, then select exactly one bounded Phase 9/editor-usability implementation
   follow-up as task 121 or record `NEEDS_HUMAN`.

   **MAY edit:**
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`

   **MAY add:**
   - `ai_handoffs/ISSUE-120_*.md`
   - `ai_handoffs/ISSUE-120_*.meta.json`
   - `.ai/dispatch-ISSUE-120/**`
   - `ai_dispatch_logs/log_*ISSUE-120*.md`

   **MUST NOT edit:**
   - Rust source or tests under `crates/**`, `editor/**`, `kernel/**`, or
     `runtime/**`
   - Cargo manifests or `Cargo.lock`
   - GitHub workflows
   - dispatch automation, guard, queue, scheduler, or verification scripts
   - schemas, ADR files, architecture-lint rules/config, packet templates, or
     existing handoff/log artifacts from other dispatches
   - plugin runtime/discovery/loading implementation code
   - `docs/EXTERNAL_ENGINE_LESSONS.md`

   **Required source/status checks before selecting:**
   - Re-read the latest `Status.md`, `HANDOFF.md`, `plans/BASELINE.md`, and
     tasks 115-119 in this file.
   - Summarize current camera/navigation state with:
     `git grep -n -E "MouseWheel|MouseButton::Right|MouseButton::Middle|ViewportOrbitDrag|ViewportPanDrag|viewport_.*drag|CursorMoved|MouseInput|is_pointer_over_viewport_tab|EditorCameraState|reset_camera|zoom_camera" -- crates/editor-shell/src`
   - Summarize current menu/palette/accelerator routing with:
     `git grep -n -E "MenuCommandHandoff|drain_and_route_menu_commands|route_menu_command|Command::ResetCamera|Command::ZoomIn|Command::ZoomOut|command_palette_window|keycode_to_shortcut|ProjectedMainMenu|ShortcutConflict" -- crates/editor-egui-host/src crates/editor-shell/src crates/editor-ui/src`
   - Summarize current deferred extension/plugin, clipboard, and mutation
     surfaces with:
     `git grep -n -E "ExtensionCommandHandler|Command::Custom|Command::Plugin|PluginHost|PluginContext|plugin-discovery|runtime-wasmtime|has_clipboard_entities|clipboard|CommandBus|\\bAction\\b|undo|dirty" -- crates/editor-shell/src crates/editor-egui-host/src crates/editor-ui/src crates/editor-actions/src crates/cad-core/src crates/cad-projection/src editor/rge-editor/src`
   - Confirm there is no open `ai-dispatch` issue and no already-filed task 121
     in GitHub or this task brief.

   **Candidate classes to compare:**
   - Whether camera/navigation work should stop after wheel zoom, orbit, and pan,
     or whether a smaller source-backed frame/focus/camera-state follow-up is
     still safe.
   - Host-shell FIFO replacement / generalized registry execution beyond the
     current `MenuCommandHandoff` -> `EditorShell::route_menu_command` path.
   - Extension/plugin command execution beyond the injected handler seam,
     including whether a substrate-only precursor is safer than runtime or
     discovery/loading work.
   - Keybinding/conflict-resolution policy or shortcut remapping after the
     existing read-only conflict diagnostics.
   - OS/typed clipboard for editor entities.
   - CAD/editor mutation routes through the authoritative command bus, including
     undo/dirty policy risk.

   **Output:** append exactly ONE task 121 with a source-backed safety rationale
   and explicit MAY / MUST-NOT / Done / Verification / Halt sections like prior
   tasks, OR record `NEEDS_HUMAN` with the blocking product/architecture policy
   question. Do not implement the selected work in task 120.

   **Halt conditions (hard):**
   - This audit begins implementing task 121, writing Rust, editing Cargo,
     changing workflows, or changing automation.
   - No bounded, source-safe candidate exists.
   - Describing task 121 would require editing a MUST-NOT path.

121. **[DONE 2026-06-13 via ISSUE-375 / commit `9720a10`] Add viewport-only left-double-click frame-all camera gesture in `editor-shell`.**
   Add the smallest next camera/navigation slice after tasks 115 (wheel zoom),
   117 (right-button orbit), and 119 (middle-button pan). The reframe pipeline
   already exists: `EditorShell::reset_camera()`
   (`crates/editor-shell/src/lifecycle/mod.rs:2062-2067`) computes a framed
   isometric pose from `current_scene_bounds()` (mod.rs:2036-2049) +
   `isometric_camera_for_bounds(min, max)` (mod.rs:2407-2432), falling back to
   `EditorCameraState::default()` for an empty/non-finite scene — but today that
   infallible reframe is reachable only via the `ResetCamera` menu command / Home
   key. This task exposes the SAME reframe as a viewport-only **left double-click**
   over the Viewport tab body, reusing `is_pointer_over_viewport_tab()`
   (mod.rs:2247-2255), the existing `WindowEvent::MouseInput` match
   (mod.rs:2660-2703), and a private double-click detector housed in
   `viewport_navigation.rs` alongside `ViewportOrbitDrag`/`ViewportPanDrag`.

   **Safety rationale:** camera/navigation is the safest of the six task-120
   candidate classes and still has exactly one small, source-backed slice
   (frame/zoom-to-fit; flagged in the not-yet-done survey). The gesture mutates only
   the existing `EditorCameraState` via the already-present reframe primitives, adds
   NO new `Command` variant, menu entry, accelerator, or route, and bypasses the
   command bus exactly as orbit/pan do. Frame-SELECTED is out of scope (per-entity
   world-AABB would cross into `cad-projection`/`editor-actions`); frame-ALL reuses
   only the existing whole-scene bounds query. The higher-risk classes are avoided:
   host-shell FIFO/route replacement, plugin runtime/discovery/loading, keybinding
   remap policy, OS clipboard, and CAD/CommandBus authoritative mutation (undo/dirty)
   all remain deferred.

   **MAY edit:**
   - `crates/editor-shell/src/lifecycle/viewport_navigation.rs`
   - `crates/editor-shell/src/lifecycle/mod.rs`
   - `crates/editor-shell/src/lifecycle/tests.rs`
   - `crates/editor-shell/src/camera.rs`
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - `ai_handoffs/ISSUE-121_*.md`
   - `ai_handoffs/ISSUE-121_*.meta.json`
   - `.ai/dispatch-ISSUE-121/**`
   - `ai_dispatch_logs/log_*ISSUE-121*.md`

   **MUST NOT edit:**
   - `crates/editor-shell/src/render_path.rs` (the gesture MUST bypass the command route)
   - `crates/editor-egui-host/**`
   - `crates/editor-ui/**`
   - `crates/editor-actions/**`
   - `crates/cad-core/**`
   - `crates/cad-projection/**`
   - `crates/plugin-discovery/**`, `crates/runtime-wasmtime/**`, `crates/runtime-wasmtime-engine/**`
   - `kernel/**`, `runtime/**`, `editor/rge-editor/**`
   - Cargo manifests or `Cargo.lock`
   - GitHub workflows
   - dispatch automation, guard, queue, scheduler, or verification scripts
   - schemas, ADR files, architecture-lint rules/config, packet templates, or
     existing handoff/log artifacts from other dispatches
   - plugin runtime/discovery/loading implementation code

   **Done criteria:**
   - A left double-click whose press lands over the Viewport tab body (per
     `is_pointer_over_viewport_tab()`) reframes `self.editor_camera` by reusing the
     existing `current_scene_bounds()` + `isometric_camera_for_bounds()` path,
     identically to `reset_camera()`: framed isometric pose for finite/non-empty
     bounds, `EditorCameraState::default()` fallback when bounds are `None`.
   - Double-click detection lives in a private detector type in
     `viewport_navigation.rs` (mirroring `ViewportOrbitDrag`/`ViewportPanDrag`),
     parameterized by a within-window time threshold and a max pointer-movement
     threshold; reset on non-Left buttons and out-of-threshold clicks.
   - A first single left-click still routes to `handle_left_click()` (face-pick)
     exactly as today; the frame fires only on the qualifying SECOND click, and the
     `should_fire_face_pick()` truth table is unchanged for single clicks.
   - The frame gesture is gated on `over_viewport_tab` AND a present `cursor_pos`; a
     double-click outside the Viewport tab body, or with no cursor/viewport/egui
     state, is a no-op.
   - No new `Command` variant, menu entry, or accelerator is added; the gesture does
     not route through `route_menu_command` or the menu command handoff.
     Right-button orbit, middle-button pan, and mouse-wheel zoom are unchanged.
   - Camera invariants remain enforced by the reused primitives (no NaN/inf
     eye/target/up; valid clip planes).
   - Focused tests cover: (a) double-click over viewport frames to scene bounds,
     (b) empty/None bounds falls back to the default camera, (c) double-click outside
     the viewport / with absent cursor is a no-op, (d) a single click does not frame
     and still reaches face-pick, (e) two clicks separated beyond the time or
     movement threshold are treated as two single clicks (no frame).
   - `cargo fmt` clean and `git diff --check` reports no whitespace errors; only
     MAY-edit files change.

   **Verification required:**
   - Before implementation, summarize:
     `git grep -n -E "is_pointer_over_viewport_tab|current_scene_bounds|isometric_camera_for_bounds|reset_camera|should_fire_face_pick|handle_left_click|MouseInput|MouseButton::Left|ViewportOrbitDrag|ViewportPanDrag" -- crates/editor-shell/src`
   - `cargo test -p rge-editor-shell --lib` (full editor-shell lib suite passes;
     baseline ~277 passed / 1 ignored plus the new focused frame-gesture tests).
   - `cargo check -p rge-editor-shell` introduces no new warnings.
   - `cargo +nightly fmt --all -- --check` clean; `git diff --check` clean.
   - `git diff --name-only` confirms ZERO modifications under
     `crates/editor-egui-host`, `crates/editor-ui`, `crates/editor-actions`,
     `crates/cad-*`, `kernel`, `runtime`, `plugin-discovery`, any `Cargo.toml` /
     `Cargo.lock`, `.github`, or `crates/editor-shell/src/render_path.rs`.

   **Halt conditions (hard):**
   - Implementing the gesture would require a new `Command` variant, menu entry,
     accelerator, or routing through `route_menu_command` / `MenuCommandHandoff`.
   - Framing the selection (not the whole scene) is attempted and requires
     per-entity world-AABB computation touching `cad-projection` / `editor-actions`
     / the selection-projection layer.
   - Any edit is needed outside `crates/editor-shell/src` (camera.rs, lifecycle/*)
     or the doc set — including `render_path.rs`, host/UI, actions, cad-*, kernel,
     runtime, plugin runtime, Cargo, workflows, or automation.
   - Double-click detection cannot be implemented from existing winit `MouseInput`
     events plus `cursor_pos`/time without a new dependency, a host-provided event,
     or a new public `EditorShell` API — STOP and record `NEEDS_HUMAN`.
   - The reframe cannot reuse `current_scene_bounds()` + `isometric_camera_for_bounds()`
     unchanged, or single-left-click face-pick / orbit / pan / wheel-zoom behavior
     would change.
   - Verification gate fails, or `git diff --name-only` shows any MUST-NOT path.

122. **[DONE 2026-06-13 via ISSUE-376 + delegated Codex decision] Post-viewport-frame-all Phase 9 next-task source audit.**
   Re-arm after task 121 (viewport-only left-double-click frame-all camera
   gesture). This is a docs/source-read-only selection audit: inspect current
   source and status after viewport wheel zoom, right-button orbit,
   middle-button pan, and left-double-click frame-all have all shipped. Compare
   the remaining Phase 9/editor-usability candidate classes and append exactly
   one bounded implementation follow-up as task 123, or record `NEEDS_HUMAN`
   with the blocking product/architecture question.

   **GitHub state rule:** use the "Dispatcher GitHub state snapshot" that
   `Invoke-AiDispatchAuto.ps1` injects into the filed issue body as the GitHub
   queue/already-filed-task evidence. Do not call `gh` or the network from the
   executor sandbox for that confirmation; use local source reads and `git grep`
   for repo/source evidence.

   **Candidate classes to compare (minimum):**
   - Camera/navigation follow-up after zoom/orbit/pan/frame-all (for example
     focus/stop/camera-state affordance), but only if it stays inside
     `editor-shell` camera/input state and does not require selection/world-AABB,
     pointer capture, persistence, or host/UI policy.
   - Host-shell FIFO replacement or generalized registry execution after the
     existing `MenuCommandHandoff` / `route_menu_command` path.
   - Real plugin command execution beyond the injected `ExtensionCommandHandler`
     seam, including runtime/discovery/loading/capability policy.
   - Keybinding/remap/conflict policy beyond current projection diagnostics.
   - OS/typed clipboard integration beyond shell-local entity clipboard.
   - Authoritative CAD/editor mutation through `CommandBus`, including
     projection, undo/dirty, and user-facing command semantics.

   **MAY edit:**
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - generated ISSUE-<n> handoff/audit/log artifacts for this dispatch only

   **MUST NOT edit:**
   - Rust source or tests
   - Cargo manifests or `Cargo.lock`
   - GitHub workflows
   - dispatch automation, guard, queue, scheduler, or verification scripts
   - schemas, ADR files, architecture-lint rules/config, packet templates, or
     existing unrelated handoff/log artifacts
   - plugin runtime/discovery/loading implementation code

   **Required source/status checks to record in the EXEC packet:**
   - Confirm there is no open `ai-dispatch` issue and no already-filed task 123
     using the dispatcher-provided GitHub snapshot plus `.ai/dispatch.tasks.md`.
   - Re-grep current source for the shipped camera/navigation closures:
     `git grep -n -E "zoom_camera|ViewportOrbitDrag|ViewportPanDrag|ViewportLeftDoubleClick|reset_camera|current_scene_bounds|isometric_camera_for_bounds|is_pointer_over_viewport_tab|MouseWheel|MouseButton::Right|MouseButton::Middle|MouseButton::Left" -- crates/editor-shell/src`
   - Re-grep current routing/extension boundaries:
     `git grep -n -E "MenuCommandHandoff|route_menu_command|ExtensionCommandHandler|ExtensionCommandEvent|Command::Custom|Command::Plugin|plugin runtime|PluginHost|PluginContext|clipboard|CommandBus|undo|dirty|shortcut.*conflict|keybinding|accelerator" -- crates editor runtime kernel Status.md HANDOFF.md plans/BASELINE.md .ai/dispatch.tasks.md`
   - Include falsifying searches for stale claims before selecting task 123.

   **Done criteria:**
   - The audit compares every candidate class above against current source,
     not stale roadmap prose.
   - Exactly one task 123 is appended with clear MAY/MUST-NOT/Done/Verification/
     Halt sections, OR the audit records `NEEDS_HUMAN` with concrete evidence.
   - The selected task 123 is one-dispatch, bounded, source-grounded, and does
     not depend on unapproved automation/tooling changes.
   - Task 122 does not implement task 123.
   - `git diff --name-only` shows only MAY-edit docs plus this dispatch's own
     generated artifacts; `git diff --check` is clean.

   **Halt conditions (hard):**
   - This audit begins implementing task 123, writing Rust, editing Cargo,
     changing workflows, or changing automation.
   - No bounded, source-safe candidate exists.
   - Describing task 123 would require editing a MUST-NOT path.

   **ISSUE-376 audit outcome: NEEDS_HUMAN.**
   The dispatcher-provided GitHub snapshot embedded in the ISSUE-376 task packet
   reported no open `ai-dispatch` issues before #376 was created, no open failed
   autonomous issues, and an already-filed autonomous issue list that stops at
   closed #375 with no task 123. Local `rg -n "^122\.|^123\." .ai/dispatch.tasks.md`
   before editing found task 122 only and no `^123\.` entry.

   Current source confirms the camera/navigation run is no longer carrying an
   obvious one-dispatch local gesture: `editor-shell` has viewport-only
   `MouseWheel` zoom, right-button `ViewportOrbitDrag`, middle-button
   `ViewportPanDrag`, and left-double-click `ViewportLeftDoubleClick` frame-all,
   all gated by the existing Viewport tab hit-test and `EditorCameraState`.
   Further camera work now points at frame-selected/world-AABB selection,
   pointer capture/window-grab, camera persistence/state policy, or a broader
   camera-controller design instead of a small local input slice.

   The non-camera candidates are also policy or architecture decisions in
   current source: menu and palette execution still crosses
   `MenuCommandHandoff` into `EditorShell::route_menu_command`; extension
   activations stop at the injected `ExtensionCommandHandler` seam; real
   plugin execution would require runtime/discovery/loading/capability policy;
   shortcut conflicts remain projection diagnostics rather than remap/fatal
   policy; clipboard behavior is shell-local entity data rather than OS/typed
   clipboard semantics; and CAD/editor mutation through `CommandBus` remains
   blocked on authoritative CAD/projection access plus undo/dirty semantics
   beyond the current World-only `Action` contract.

   No task 123 was appended. Blocking decision for the human reviewer: choose
   which Phase 9 boundary to cross next and authorize its product/architecture
   policy explicitly -- broader camera controller/persistence/pointer-capture,
   host-shell command-route replacement, real plugin runtime/discovery/loading,
   keybinding/remap/conflict policy, OS/typed clipboard semantics, or
   authoritative CAD/editor mutation through a richer CommandBus/undo/dirty
   model.

   **Delegated-human decision:** per the standing "Human=Codex / non-stop"
   authorization, choose the keybinding conflict-policy boundary as the next
   smallest implementation surface. The policy for task 123 is: conflicted
   shortcuts remain visible in diagnostics, but keyboard execution must not pick
   a first-registered winner while a shortcut has a live conflict. This crosses
   the minimum policy boundary needed to continue automation while staying inside
   the existing `editor-ui` menu-resolution substrate and the shell's existing
   `enabled_command_for_shortcut` execution path.

123. **[DONE 2026-06-13 via ISSUE-377] Make conflicted shortcuts non-executable while preserving diagnostics.**
   Implement the delegated task-122 decision: when a resolved shortcut has a
   conflict, keyboard execution must treat it as unexecutable even if the
   first-registered entry is enabled. Conflict diagnostics and display/introspection
   lookups remain available, so hosts can still show the conflict and the
   accelerator table can still report its deterministic first entry.

   **Policy fixed for this task:**
   - `ResolveResult::enabled_command_for_shortcut` is the keyboard-execution
     resolver and MUST return `None` for any shortcut listed in
     `ResolveResult::conflicts`.
   - `ResolveResult::command_for_shortcut` and `AcceleratorTable::resolve` MAY
     keep their existing first-registered winner behavior for display,
     diagnostics, and compatibility; do not use them as the execution gate.
   - Hidden or predicate-filtered entries continue not to occupy shortcut slots.
   - Disabled-but-visible entries continue to resolve as bindings for display
     but do not execute through `enabled_command_for_shortcut`.

   **MAY edit:**
   - `crates/editor-ui/src/menus/registry.rs`
   - `crates/editor-ui/src/menus/shortcut.rs`
   - `crates/editor-ui/tests/menus_ordering.rs`
   - `crates/editor-ui/src/menus/default_menu.rs`
   - `crates/editor-shell/src/lifecycle/mod.rs`
   - `crates/editor-shell/src/render_path.rs`
   - `crates/editor-shell/src/lifecycle/accelerator.rs`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - `.ai/dispatch.tasks.md`
   - generated ISSUE-<n> handoff/audit/log artifacts for this dispatch only

   **MUST NOT edit:**
   - Cargo manifests or `Cargo.lock`
   - GitHub workflows
   - dispatch automation, guard, queue, scheduler, or verification scripts
   - schemas, ADR files, architecture-lint rules/config, packet templates, or
     existing unrelated handoff/log artifacts
   - plugin runtime/discovery/loading implementation
   - host-shell FIFO replacement, `MenuCommandHandoff` storage semantics, or
     `EditorShell::route_menu_command`
   - OS/typed clipboard behavior
   - CAD graph/projection mutation, `CommandBus` action signatures, undo/dirty
     authority, or save/load behavior
   - camera/navigation behavior
   - shortcut remapping UI, user preferences, persistence, or fatal application
     startup policy

   **Done criteria:**
   - `ResolveResult::enabled_command_for_shortcut` returns `None` for a
     conflicted shortcut, while unconflicted enabled shortcuts still return
     their command and disabled shortcuts still return `None`.
   - Existing conflict diagnostics still report all conflicting entry ids in
     deterministic order.
   - The first-winner display/introspection behavior remains available through
     `command_for_shortcut` and/or `AcceleratorTable::resolve`.
   - Tests cover conflicted execution suppression, unconflicted execution,
     disabled entry suppression, hidden entries releasing their shortcut slot,
     and predicate-filtered entries releasing their shortcut slot.
   - Shell comments or parity tests are updated only as needed to make clear
     that keyboard dispatch goes through the conflict-aware
     `enabled_command_for_shortcut` path.
   - No task 124 is appended.

   **Verification:**
   - `cargo test -p rge-editor-ui --test menus_ordering`
   - `cargo test -p rge-editor-ui --lib`
   - `cargo test -p rge-editor-shell --lib accelerator`
   - `cargo check -p rge-editor-ui -p rge-editor-shell --lib`
   - `cargo +nightly fmt --all -- --check`
   - `git diff --check`
   - The EXEC packet records a Rule 8 falsifying search for hidden-entry slot
     release, e.g. `rg -n -C 10 "visible == false|filter\(\|e\| e\.visible|with_visible\(false\)" crates/editor-ui/src/menus/registry.rs crates/editor-ui/src/menus/entry.rs crates/editor-ui/tests/menus_ordering.rs`, and summarizes the live evidence that `resolve_slot` filters `e.visible && e.predicate.evaluate(ctx)` before shortcut registration.
   - `git diff --name-only` contains only MAY-edit files plus this dispatch's
     generated artifacts.

   **Halt conditions:**
   - Implementing the policy requires a new command route, menu handoff
     replacement, plugin runtime/discovery/loading work, Cargo dependency, OS
     clipboard behavior, CAD/editor mutation, camera behavior, persistence, or
     user-facing shortcut remapping UI.
   - The change would make conflicted shortcuts disappear from diagnostics rather
     than remain visible but non-executable.
   - The change would make disabled entries execute, or make hidden/predicate
     suppressed entries occupy shortcut slots.
   - Verification fails, or `git diff --name-only` shows any MUST-NOT path.

124. **[DONE 2026-06-13 via ISSUE-378] Post-conflicted-shortcut Phase 9 next-task audit.**
   Perform a docs/source-read-only audit after task 123. Use the
   dispatcher-provided GitHub-state snapshot embedded in the auto-created issue
   body as the only GitHub queue/already-filed evidence; do not call `gh`,
   network APIs, or browse. Re-check current local source and docs, compare the
   remaining Phase 9/editor-usability candidate classes, and append exactly one
   bounded implementation task 125, using the standing Human=Codex delegated
   authorization to choose the smallest policy boundary when the evidence
   requires a product/architecture choice. Record `NEEDS_HUMAN` only if the
   local source evidence is contradictory or no bounded, source-safe task can be
   specified even with delegated policy.

   **Candidate classes to compare from current source:**
   - Keybinding/remap policy after task 123: conflict diagnostics remain
     visible and conflicted shortcuts no longer execute; remaining work includes
     remapping UI, preferences/persistence, fatal startup policy, or narrower
     diagnostics/policy slices.
   - Host-shell command execution: menu, palette, and accelerator activation
     still cross `MenuCommandHandoff` into `EditorShell::route_menu_command`;
     replacing that route or generalizing registry execution is a broader
     host-shell boundary unless a smaller source-safe slice exists.
   - Real plugin command execution: extension commands stop at the injected
     `ExtensionCommandHandler` seam; runtime/discovery/loading/capability
     policy remains broader unless a bounded seam-only follow-up exists.
   - OS/typed clipboard: Edit Cut/Copy/Paste is shell-local legacy-blob entity
     data; OS clipboard, typed components, CAD identity, and cross-process
     semantics remain broader unless a narrow policy/documented substrate slice
     exists.
   - CAD/editor mutation through CommandBus: current actions remain World-only;
     authoritative CAD/projection mutation, undo/dirty authority, and save/load
     semantics remain broader unless a bounded source-safe slice exists.
   - Camera/navigation follow-up after wheel zoom, right-button orbit,
     middle-button pan, and left-double-click frame-all: remaining work includes
     frame-selected/world-AABB, pointer capture/window-grab, camera persistence,
     or a broader controller policy.

   **MAY edit:**
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - generated ISSUE-<n> handoff/audit/log artifacts for this dispatch only

   **MUST NOT edit:**
   - Rust source or tests under `crates/**`, `editor/**`, `kernel/**`,
     `runtime/**`, or `tools/**`
   - Cargo manifests or `Cargo.lock`
   - GitHub workflows
   - dispatch automation, guard, queue, scheduler, watcher, verification, or
     health/trend scripts
   - schemas, ADR files, architecture-lint rules/config, packet templates, or
     existing unrelated handoff/log artifacts
   - plugin runtime/discovery/loading implementation
   - `MenuCommandHandoff` storage semantics, host-shell FIFO replacement, or
     `EditorShell::route_menu_command`
   - OS/typed clipboard behavior
   - CAD graph/projection mutation, `CommandBus` action signatures,
     undo/dirty/save-load authority, or save/load behavior
   - camera/navigation behavior
   - shortcut remapping UI, user preferences, persistence, or fatal startup
     policy as implementation

   **Done criteria:**
   - The audit records the embedded GitHub-state snapshot facts and a local
     `rg -n "^124\.|^125\." .ai/dispatch.tasks.md` check from before editing.
   - The audit records source-grounded evidence for each candidate class above,
     including at least one falsifying grep per class where practical.
   - Exactly one bounded implementation task 125 is appended with explicit MAY
     edit, MUST-NOT edit, done criteria, verification, and halt conditions; or
     task 124 records `NEEDS_HUMAN` with concrete source evidence explaining
     why no bounded task can be safely specified.
   - If delegated policy is used, the selected policy is stated explicitly in
     task 125 and kept to the smallest source-safe boundary.
   - No implementation work for task 125 is done, and no task 126 is appended.
   - `git diff --name-only` shows only MAY-edit docs plus this dispatch's own
     generated artifacts; `git diff --check` is clean.

   **Verification:**
   - `rg -n "^124\.|^125\." .ai/dispatch.tasks.md`
   - Candidate source greps covering keybinding/remap, command routing,
     plugin/extension execution, clipboard, CommandBus/CAD mutation, and camera
     follow-up surfaces
   - `git diff --name-only`
   - `git diff --check`

   **Halt conditions:**
   - The audit begins implementing task 125, writing Rust, editing Cargo,
     changing workflows, or changing automation.
   - The audit requires live GitHub/network evidence instead of the embedded
     snapshot plus local source reads.
   - No bounded task 125 can be specified without editing a MUST-NOT path.

   **Audit result (ISSUE-378):**
   The embedded dispatcher GitHub-state snapshot reported no open
   `ai-dispatch` issues before #378 was created, no open failed autonomous
   issues, and an already-filed autonomous issue list ending at closed #377
   with no task 125. No `gh` or network command was used. Local
   `rg -n "^124\.|^125\." .ai/dispatch.tasks.md` before editing found task
   124 only and no `^125\.` entry.

   Current source re-checks confirmed task 123's policy is present:
   `ResolveResult::enabled_command_for_shortcut` suppresses live conflicted
   shortcuts while `command_for_shortcut`, `AcceleratorTable::resolve`, and
   `ProjectedMainMenu.conflicts` preserve display/introspection and diagnostics.
   Menu, command-palette, and accelerator activation still cross
   `MenuCommandHandoff` into `EditorShell::route_menu_command`; extension
   activations still stop at the injected `ExtensionCommandHandler` seam
   rather than a real plugin runtime/discovery/loading path; clipboard behavior
   remains shell-local legacy-blob entity data; CAD/editor mutation through
   `CommandBus` still crosses broader authoritative CAD/projection plus
   undo/dirty/save-load policy; and the small viewport camera run is shipped at
   wheel zoom, right-button orbit, middle-button pan, and left-double-click
   frame-all.

   The selected bounded implementation task is the smallest post-task-123
   diagnostics slice: annotate currently conflicted shortcuts in the existing
   host-local Keyboard Shortcuts help surface. This uses `ProjectedMainMenu`
   data already present in `editor-egui-host`, does not alter keyboard
   execution, menu/palette activation, remapping, persistence, conflict fatality,
   command routing, plugin runtime, OS clipboard, CAD, or camera behavior, and
   defers the broader alternatives explicitly.

125. **~~Annotate conflicted shortcuts in host keyboard-shortcuts help.~~ DONE 2026-06-14 (ISSUE-379).**
   Implement the smallest post-task-123 keybinding diagnostics slice: the
   host-local Keyboard Shortcuts help window must distinguish displayed
   shortcuts that are currently conflicted and therefore non-executable by the
   keyboard path. This is an informational host diagnostic only; it must not
   change shortcut resolution, menu click execution, command-palette activation,
   conflict diagnostics, remapping, persistence, or fatal-startup policy.

   **Policy fixed for this task:**
   - A shortcut display string present in `ProjectedMainMenu.conflicts` is shown
     in Keyboard Shortcuts help as a conflict state, even when the underlying
     menu command is otherwise enabled.
   - Existing disabled-state display remains distinct from conflict-state
     display: disabled rows stay disabled, unconflicted enabled rows stay
     enabled, and conflicted enabled rows show the conflict state.
   - The conflict state is sourced only from the already-projected
     `ProjectedMainMenu.conflicts` data. Do not re-resolve the menu registry in
     the shortcut-help helper and do not add a second conflict detector.
   - Menu clicks, command-palette activations, and the `Shortcut Conflicts`
     diagnostics window remain behaviorally unchanged.

   **MAY edit:**
   - `crates/editor-egui-host/src/shortcut_help.rs`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - `.ai/dispatch.tasks.md`
   - `ai_handoffs/ISSUE-<n>_*.md`
   - `ai_handoffs/ISSUE-<n>_*.meta.json`
   - `.ai/dispatch-ISSUE-<n>/**`
   - `ai_dispatch_logs/log_*ISSUE-<n>*.md`

   **MUST NOT edit:**
   - `crates/editor-ui/**`
   - `crates/editor-shell/**`
   - `crates/editor-egui-host/src/menu.rs`
   - `crates/editor-egui-host/src/shortcut_conflicts.rs`
   - `crates/editor-egui-host/src/lib.rs`
   - `crates/editor-actions/**`
   - `crates/cad-core/**`
   - `crates/cad-projection/**`
   - `editor/**`
   - `kernel/**`
   - `runtime/**`
   - `tools/**`
   - Cargo manifests or `Cargo.lock`
   - GitHub workflows, dispatch automation scripts, schemas, ADRs,
     architecture-lint rules/config, packet templates, or unrelated
     handoff/log artifacts
   - plugin runtime/discovery/loading implementation
   - `MenuCommandHandoff` storage semantics, host-shell FIFO replacement, or
     `EditorShell::route_menu_command`
   - shortcut remapping UI, user preferences, shortcut persistence, or fatal
     startup policy
   - OS/typed clipboard behavior
   - CAD graph/projection mutation, `CommandBus` action signatures,
     undo/dirty/save-load authority, or save/load behavior
   - camera/navigation behavior

   **Done criteria:**
   - Keyboard Shortcuts help rows whose displayed shortcut appears in
     `ProjectedMainMenu.conflicts` show a clear conflict state in the existing
     State column.
   - Unconflicted enabled rows still show enabled, and ordinary disabled rows
     still show disabled.
   - The helper consumes only `ProjectedMainMenu` data and remains read-only:
     building shortcut-help rows does not enqueue menu commands, mutate
     command-palette state, open/close conflict diagnostics, or alter the
     projected menu.
   - Focused tests cover an enabled conflicted row, an unconflicted enabled row,
     an ordinary disabled row, and the existing read-only no-enqueue behavior.
   - No task 126 is appended.

   **Verification:**
   - `cargo test -p rge-editor-egui-host --lib shortcut_help`
   - `cargo test -p rge-editor-egui-host --lib shortcut_conflict`
   - `cargo check -p rge-editor-egui-host --lib`
   - `cargo +nightly fmt --all -- --check`
   - `git diff --check`
   - `git diff --name-only` contains only MAY-edit files plus this dispatch's
     generated artifacts.

   **Halt conditions:**
   - Implementing the conflict state requires editing `editor-ui`,
     `editor-shell`, `menu.rs`, `shortcut_conflicts.rs`, `lib.rs`, command
     routing, menu handoff semantics, plugin runtime/discovery/loading, Cargo,
     OS clipboard, CAD/projection/CommandBus mutation, camera/navigation code,
     or any other MUST-NOT path.
   - The change would disable menu clicks or command-palette activation for
     conflicted shortcuts instead of annotating the help surface only.
   - The change would hide conflicts, make conflicts fatal, add remapping or
     persistence, or change first-winner display/introspection behavior.
   - Verification fails, or `git diff --name-only` shows any MUST-NOT path.

   **Implementation result (ISSUE-379):**
   `shortcut_help_rows` now derives a host-local conflict flag only from
   `ProjectedMainMenu.conflicts` shortcut display strings. The Keyboard
   Shortcuts help State column renders three distinct states: `Enabled`,
   `Disabled`, and `Conflicted`. Conflicted rows remain informational only; menu
   clicks, command-palette activation, shortcut execution, conflict diagnostics,
   remapping, persistence, and routing semantics are unchanged.

   Focused coverage in `shortcut_help.rs` now pins an enabled conflicted row,
   an unconflicted enabled row, an ordinary disabled row, and the read-only
   no-enqueue behavior with a projected conflict present. Verification for the
   implementation run passed: `cargo test -p rge-editor-egui-host --lib
   shortcut_help` (8/8), `cargo test -p rge-editor-egui-host --lib
   shortcut_conflict` (7/7), `cargo check -p rge-editor-egui-host --lib`,
   `cargo +nightly fmt --all -- --check`, and no task 126 was appended.

126. **~~Post-shortcut-help Phase 9 next-task audit.~~ DONE 2026-06-14 (manual salvage after ISSUE-380 plan-gate failure).**
   Perform the next docs/source-read-only Phase 9 audit after task 125. The
   immediate objective is to compare the remaining editor-usability candidate
   classes from current local source and append exactly one bounded
   implementation task 127, or record a source-grounded `NEEDS_HUMAN` result if
   no single safe implementation slice exists.

   **Dispatcher GitHub-state snapshot rule:**
   - Use the dispatcher-provided GitHub-state snapshot embedded in this
     dispatch issue body as the only GitHub queue/already-filed-task evidence.
   - Do not call `gh`, do not use network access, and do not infer GitHub state
     from local issue artifacts alone.
   - Local repository/source evidence must come from source reads and falsifying
     searches in this worktree.

   **Starting facts from the re-arm commit:**
   - ISSUE-379 auto-published task 125 as `214a217` and closed with
     `ai-dispatch-done`.
   - A dry-run autonomous selector after #379 reported no real task to select.
   - `rg -n "^125\.|^126\.|^127\." .ai/dispatch.tasks.md` found task 125 done
     and no task 126 or 127 before this re-arm edit.
   - No open `ai-dispatch` issue existed at re-arm time.

   **Candidate classes to compare from current source:**
   - Keybinding/remap policy after tasks 123 and 125: conflicted shortcuts are
     diagnostic-visible, non-executable through keyboard activation, and
     annotated in Keyboard Shortcuts help; remaining work includes remapping UI,
     preferences/persistence, fatal startup policy, or a narrower diagnostic or
     policy follow-up if one exists.
   - Host-shell command execution: menu, command-palette, and accelerator
     activation still cross `MenuCommandHandoff` into
     `EditorShell::route_menu_command`; replacing that route or generalizing
     registry execution is broader unless a small host-owned source-safe slice
     exists.
   - Real plugin command execution: extension commands still stop at the
     injected `ExtensionCommandHandler` seam; runtime/discovery/loading,
     capability policy, and editor route ownership remain broader unless a
     bounded seam-only follow-up exists.
   - OS/typed clipboard: Edit Cut/Copy/Paste remains shell-local legacy-blob
     entity data; OS clipboard, typed components, CAD identity, and
     cross-process semantics remain broader unless a narrow policy/documented
     substrate slice exists.
   - CAD/editor mutation through CommandBus: current actions remain World-only;
     authoritative CAD/projection mutation, undo/dirty authority, and save/load
     semantics remain broader unless a bounded source-safe slice exists.
   - Camera/navigation follow-up after wheel zoom, right-button orbit,
     middle-button pan, and left-double-click frame-all: remaining work includes
     frame-selected/world-AABB, pointer capture/window-grab, camera persistence,
     or a broader controller policy.

   **MAY edit:**
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - generated ISSUE-<n> handoff/audit/log artifacts for this dispatch only

   **MUST NOT edit:**
   - Rust source or tests under `crates/**`, `editor/**`, `kernel/**`,
     `runtime/**`, or `tools/**`
   - Cargo manifests or `Cargo.lock`
   - GitHub workflows
   - dispatch automation, guard, queue, scheduler, watcher, verification, or
     health/trend scripts
   - schemas, ADR files, architecture-lint rules/config, packet templates, or
     existing unrelated handoff/log artifacts
   - plugin runtime/discovery/loading implementation
   - `MenuCommandHandoff` storage semantics, host-shell FIFO replacement, or
     `EditorShell::route_menu_command`
   - command-palette activation semantics, accelerator execution semantics, or
     menu-click behavior
   - shortcut remapping UI, user preferences, persistence, or fatal startup
     policy as implementation
   - OS/typed clipboard behavior
   - CAD graph/projection mutation, `CommandBus` action signatures,
     undo/dirty/save-load authority, or save/load behavior
   - camera/navigation behavior

   **Done criteria:**
   - The audit records the embedded GitHub-state snapshot facts and a local
     `rg -n "^125\.|^126\.|^127\." .ai/dispatch.tasks.md` check from before
     editing.
   - The audit records source-grounded evidence for each candidate class above,
     including at least one falsifying grep per class where practical.
   - Exactly one bounded implementation task 127 is appended with explicit MAY
     edit, MUST-NOT edit, done criteria, verification, and halt conditions; or
     task 126 records `NEEDS_HUMAN` with concrete source evidence explaining why
     no bounded task can be safely specified.
   - If delegated Human=Codex policy is used, the selected policy is stated
     explicitly in task 127 and kept to the smallest source-safe boundary.
   - No implementation work for task 127 is done, and no task 128 is appended.
   - `git diff --name-only` shows only MAY-edit docs plus this dispatch's own
     generated artifacts; `git diff --check` is clean.

   **Verification:**
   - `rg -n "^125\.|^126\.|^127\." .ai/dispatch.tasks.md`
   - Candidate source greps covering keybinding/remap, command routing,
     plugin/extension execution, clipboard, CommandBus/CAD mutation, and camera
     follow-up surfaces
   - `git diff --name-only`
   - `git diff --check`

   **Halt conditions:**
   - The audit begins implementing task 127, writing Rust, editing Cargo,
     changing workflows, or changing automation.
   - The audit requires live GitHub/network evidence instead of the embedded
     snapshot plus local source reads.
   - No bounded task 127 can be specified without editing a MUST-NOT path.

   **Audit result (manual salvage after ISSUE-380):**
   ISSUE-380 did not complete through the orchestrator. The first attempt
   stalled during execution and was archived as `ISSUE-380.attempt1`; the retry
   stopped at plan-gate revision 1 under Protocol Rule 8 because the task packet
   did not embed enough raw dispatcher-snapshot / task-128 falsification
   evidence for the sandboxed executor to verify every negative premise without
   live `gh`. No ISSUE-380 work was published.

   Manual salvage used the ISSUE-380 body snapshot for GitHub queue evidence:
   generated `2026-06-14T00:52:49.8155822+03:00`, it reported no open
   `ai-dispatch` issue before #380 was created, no open failed autonomous issue,
   and autonomous issues already filed only through closed #379 for this local
   Phase 9 sequence. The pre-edit local task grep
   `rg -n "^125\.|^126\.|^127\." .ai/dispatch.tasks.md` showed only task 125
   done and this task 126 open; no task 127 existed before this salvage edit.

   Source audit:
   - Keybinding/remap: `ResolveResult::enabled_command_for_shortcut` suppresses
     live conflicted shortcuts while `command_for_shortcut` keeps the
     first-registered display/introspection lookup; `ProjectedMainMenu.conflicts`
     already reaches `editor-egui-host`. Task 125 marks help rows as
     `Conflicted`, but `shortcut_help_rows` currently collapses conflicts to a
     `BTreeSet` of shortcut display strings and does not expose the projected
     conflict `entries`. `shortcut_conflict_rows` proves those entry ids are
     already present as ordered host-local diagnostic data.
   - Host-shell execution: menu, palette, and accelerator activation still route
     through `MenuCommandHandoff` / `EditorShell::route_menu_command`; replacing
     that path or generalizing registry execution remains broader than a
     follow-up diagnostic in the help surface.
   - Plugin execution: extension commands still stop at the injected
     `ExtensionCommandHandler` seam; real plugin runtime/discovery/loading and
     route ownership remain broader.
   - OS/typed clipboard: `EditorShell` still documents and stores a shell-local
     legacy-blob entity clipboard, not OS clipboard or typed component/CAD
     clipboard state.
   - CAD/CommandBus mutation: `CommandBus` remains the editor action/undo/dirty
     substrate; extending authoritative CAD/projection mutation, action
     signatures, or save/load authority is a larger policy change.
   - Camera/navigation: viewport wheel zoom, right-button orbit, middle-button
     pan, and left-double-click frame-all are present; remaining
     frame-selected, pointer-capture/window-grab, camera persistence, and
     controller-policy work is wider than this next slice.

   Selection: append exactly one bounded implementation task 127,
   **Expose shortcut-help conflict peer entry IDs**, because it reuses existing
   projected conflict data and stays inside the already-active
   `editor-egui-host` Keyboard Shortcuts help surface. Broader remapping,
   routing, plugin execution, OS clipboard, CAD mutation, and camera policy stay
   deferred. No implementation work for task 127 was done, and no task 128 was
   appended.

127. **[DONE 2026-06-14 via ISSUE-381] Expose shortcut-help conflict peer entry IDs.**
   Extend the host-local Keyboard Shortcuts help surface so a row whose State is
   `Conflicted` also exposes the ordered peer entry ids already carried by
   `ProjectedMainMenu.conflicts.entries`. This is an informational diagnostic
   follow-up only; it must not change shortcut execution, menu-click behavior,
   command-palette activation, remapping, persistence, routing, or the separate
   Shortcut Conflicts window.

   **Policy boundary:**
   The selected policy remains the task-123/task-125 boundary: conflicted
   shortcuts are visible diagnostics and are not executable by keyboard, but
   they are not fatal and are not remapped here. Task 127 only makes the
   existing conflict peers easier to inspect from the Keyboard Shortcuts help
   row.

   **Completion note (ISSUE-381):**
   Implemented in `crates/editor-egui-host/src/shortcut_help.rs` by carrying
   ordered peer entry ids from `ProjectedMainMenu.conflicts.entries` on enabled
   conflicted shortcut-help rows and rendering them in the host-local Keyboard
   Shortcuts help table. The ISSUE-381 task packet deliberately narrowed status
   sync to `.ai/dispatch.tasks.md`, `Status.md`, `HANDOFF.md`, and `change.md`;
   `plans/BASELINE.md` sync is deferred and was not edited. No task 128 was
   appended.

   **MAY edit:**
   - `crates/editor-egui-host/src/shortcut_help.rs`
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - generated ISSUE-<n> handoff/audit/log artifacts for this dispatch only

   **MUST NOT edit:**
   - `crates/editor-egui-host/src/menu.rs`
   - `crates/editor-egui-host/src/shortcut_conflicts.rs`
   - `crates/editor-egui-host/src/lib.rs`
   - `crates/editor-egui-host/src/menu_tests.rs`
   - `crates/editor-ui/**`
   - `crates/editor-shell/**`
   - `crates/editor-actions/**`
   - `kernel/**`, `runtime/**`, `tools/**`, `editor/**`
   - Cargo manifests or `Cargo.lock`
   - GitHub workflows
   - dispatch automation, guard, queue, scheduler, watcher, verification, or
     health/trend scripts
   - schemas, ADR files, architecture-lint rules/config, packet templates, or
     existing unrelated handoff/log artifacts
   - `ProjectedMainMenu`, `ProjectedShortcutConflict`, `ResolveResult`, or
     `AcceleratorTable` definitions/semantics
   - `MenuCommandHandoff` storage semantics, host-shell FIFO replacement,
     `EditorShell::route_menu_command`, command-palette activation, accelerator
     execution, menu-click behavior, shortcut remapping UI, user preferences,
     persistence, fatal startup policy, plugin runtime/discovery/loading,
     OS/typed clipboard behavior, CAD/projection mutation, `CommandBus` action
     signatures, undo/dirty/save-load authority, save/load behavior, or
     camera/navigation behavior

   **Done criteria:**
   - `ShortcutHelpRow` exposes deterministic conflict peer entry ids for
     enabled conflicted rows, sourced only from the matching
     `ProjectedMainMenu.conflicts.entries` vector and preserving projection
     order.
   - Unconflicted enabled rows and ordinary disabled rows expose no conflict
     peer detail. A disabled row whose displayed shortcut appears in
     `ProjectedMainMenu.conflicts` remains `Disabled`, not `Conflicted`, and
     exposes no peer detail.
   - `shortcut_help_window` keeps the existing columns and the distinct State
     labels (`Enabled`, `Disabled`, `Conflicted`), and exposes peer entry ids
     only as host-local informational detail for conflicted rows.
   - Projected plugin rows can carry conflict peer detail when the existing
     projected menu data says they are enabled and conflicted; no registry
     resolution is added to shortcut help.
   - Existing read-only guarantees remain: building or showing shortcut help
     does not enqueue menu commands, mutate command-palette rows/recents/pins,
     or touch the Shortcut Conflicts window state.
   - `.ai/dispatch.tasks.md`, `Status.md`, `HANDOFF.md`, and `change.md` mark
     task 127 complete for ISSUE-381; `plans/BASELINE.md` sync is deferred by
     the narrowed task packet, and no task 128 is appended.

   **Verification:**
   - `cargo test -p rge-editor-egui-host --lib shortcut_help`
   - `cargo test -p rge-editor-egui-host --lib shortcut_conflict`
   - `cargo check -p rge-editor-egui-host --lib`
   - `cargo +nightly fmt --all -- --check`
   - `git diff --name-only`
   - `git diff --check`

   **Halt conditions:**
   - The implementation needs to edit any MUST-NOT path or change any routing,
     execution, remapping, persistence, plugin runtime, OS clipboard,
     CAD/CommandBus, save/load, or camera behavior.
   - Conflict peer detail cannot be sourced solely from
     `ProjectedMainMenu.conflicts.entries`.
   - The UI change requires adding a command, menu entry, accelerator,
     activation path, preferences surface, or a new diagnostics window.
   - Verification fails, or `git diff --name-only` shows any file outside the
     MAY-edit list plus generated artifacts for this dispatch.

128. **[DONE 2026-06-14 via ISSUE-382] Post-shortcut-conflict-help Phase 9 next-task audit.**
   Perform the next docs/source-read-only Phase 9 audit after task 127 /
   ISSUE-381. The immediate objective is to compare the remaining
   editor-usability candidate classes from current local source and append
   exactly one bounded implementation task 129, or record a source-grounded
   `NEEDS_HUMAN` result if no single safe implementation slice exists.

   **Dispatcher snapshot / Rule 8 requirement:**
   - The TASK packet for this audit must copy the exact
     `Dispatcher GitHub state snapshot` excerpt from the auto-created issue body
     into the packet, or cite a concrete local artifact path plus exact read
     command containing that excerpt. A summary such as "no open issue" is not
     enough.
   - Use that embedded snapshot as the only GitHub queue/already-filed-task
     evidence. Do not call `gh`, browse, or use network access from the executor
     sandbox.
   - Any negative current-state claim in the TASK or EXEC packet must include a
     rerunnable falsifying search/read and observed result, following
     `ai_handoffs/AI_HANDOFF_PROTOCOL.md` Rule 8.
   - Local repository/source evidence must come from source reads and
     falsifying searches in this worktree.

   **Starting facts from the re-arm commit:**
   - ISSUE-381 auto-published task 127 as `3b817b7` and closed with
     `ai-dispatch-done`.
   - Task 127 deliberately did not append task 128 in its implementation
     commit; this re-arm commit appends task 128 under the standing
     Human=Codex / non-stop authorization.
   - `rg -n "^126\.|^127\.|^128\.|^129\." .ai/dispatch.tasks.md` before this
     re-arm edit found task 126 done and task 127 done, with no task 128 or
     task 129 heading.
   - The queue had no open `ai-dispatch` issue and no open
     `ai-dispatch-failed` issue before this re-arm edit.

   **Candidate classes to compare from current source:**
   - Keybinding/remap policy after tasks 123, 125, and 127: conflicted
     shortcuts are diagnostic-visible, non-executable through keyboard
     activation, annotated in Keyboard Shortcuts help, and now show peer entry
     ids there. Remaining work includes remapping UI, preferences/persistence,
     fatal startup policy, or a narrower diagnostic/policy follow-up if one is
     still source-safe.
   - Host-shell command execution: menu, command-palette, and accelerator
     activation still cross `MenuCommandHandoff` into
     `EditorShell::route_menu_command`; replacing that route or generalizing
     registry execution is broader unless a small host-owned source-safe slice
     exists.
   - Real plugin command execution: extension commands still stop at the
     injected `ExtensionCommandHandler` seam; runtime/discovery/loading,
     capability policy, and editor route ownership remain broader unless a
     bounded seam-only follow-up exists.
   - OS/typed clipboard: Edit Cut/Copy/Paste remains shell-local legacy-blob
     entity data; OS clipboard, typed components, CAD identity, and
     cross-process semantics remain broader unless a narrow policy/documented
     substrate slice exists.
   - CAD/editor mutation through CommandBus: current actions remain World-only;
     authoritative CAD/projection mutation, undo/dirty authority, and save/load
     semantics remain broader unless a bounded source-safe slice exists.
   - Camera/navigation follow-up after wheel zoom, right-button orbit,
     middle-button pan, and left-double-click frame-all: remaining work includes
     frame-selected/world-AABB, pointer capture/window-grab, camera persistence,
     or a broader controller policy.

   **Audit result (ISSUE-382):**
   - Dispatcher snapshot evidence came only from the local planner log:
     `Get-Content -LiteralPath '.ai\dispatch-ISSUE-382\codex.plan.rev0.log' | Select-Object -Skip 40 -First 205`.
     That read returned `Dispatcher GitHub state snapshot`, generated by
     `Invoke-AiDispatchAuto.ps1 at 2026-06-14T02:40:09.4240537+03:00 before
     this issue was created`, `(none)` for open `ai-dispatch` issues, `(none)`
     for open failed autonomous issues, already-filed autonomous issues through
     `#381 [CLOSED] Expose shortcut-help conflict peer entry IDs`, and the
     executor instruction not to call `gh` or the network for that
     confirmation. No `gh` or network command was used.
   - Pre-edit heading check:
     `rg -n "^126\.|^127\.|^128\.|^129\." .ai/dispatch.tasks.md` returned
     `11244` task 126 done, `11405` task 127 done, `11504` task 128 open, and
     no `^129.` match.
   - Keybinding/remap evidence:
     `rg -n "enabled_command_for_shortcut|command_for_shortcut|AcceleratorTable|ProjectedShortcutConflict|shortcut_help_rows|shortcut_conflict_rows|remap|preferences|persist|fatal" crates/editor-ui/src/menus crates/editor-egui-host/src`
     confirmed the current conflict-aware shortcut execution/display split:
     `ResolveResult::enabled_command_for_shortcut` suppresses disabled or
     conflicted accelerators, `command_for_shortcut` and `AcceleratorTable`
     remain display/introspection lookups, `ProjectedMainMenu.conflicts` feeds
     `shortcut_conflict_rows`, and `shortcut_help_rows` now carries
     `conflict_peer_entry_ids`. Falsifying search
     `rg -n "conflicted|conflict_peer_entry_ids|ProjectedCommandPaletteEntry|ShortcutHelpRow|command_palette_entries" crates/editor-egui-host/src/menu.rs crates/editor-egui-host/src/shortcut_help.rs crates/editor-egui-host/src/menu_tests.rs`
     shows the conflict fields exist on `ShortcutHelpRow`, while
     `ProjectedCommandPaletteEntry` remains only label/shortcut/command/enabled
     and `command_palette_entries` has no conflict state. This leaves a narrow
     host-local diagnostic slice: annotate command-palette rows whose displayed
     shortcut is conflicted, without changing activation.
   - Host-shell command execution evidence:
     `rg -n "MenuCommandHandoff|drain_and_route_menu_commands|route_menu_command|command_palette_window|selected_command_palette_entry|enabled_command_for_shortcut|WindowEvent::KeyboardInput" crates/editor-egui-host/src crates/editor-shell/src crates/editor-ui/src/menus`
     confirmed the existing path: host menu and command-palette activations
     enqueue to `MenuCommandHandoff`, `EditorShell::drain_and_route_menu_commands`
     drains at frame top, and `EditorShell::route_menu_command` remains the
     route owner. Replacing the FIFO, generalized registry execution, or palette
     activation semantics stays broader than the selected diagnostic task.
   - Plugin execution evidence:
     `rg -n "ExtensionCommandHandler|extension_command|plugin command|PluginCommand|Command::Plugin|plugin runtime|plugin-discovery" crates/editor-shell/src crates/editor-ui/src crates/plugin-discovery/src crates/runtime-wasmtime/src crates/runtime-wasmtime-engine/src`
     and
     `rg -n "PluginHost|PluginContext|runtime_wasmtime|runtime-wasmtime|plugin_discovery|plugin-discovery|ExtensionCommandHandler|Command::Plugin|Command::Custom" crates/editor-shell/src crates/editor-ui/src crates/plugin-discovery/src crates/runtime-wasmtime/src crates/runtime-wasmtime-engine/src editor/rge-editor/src`
     showed `Command::Custom` / `Command::Plugin` capture plus the injected
     `ExtensionCommandHandler` seam, a `rge-plugin-discovery` stub, and
     separate runtime crates, but no editor route that wires real plugin
     discovery/loading/runtime execution. Real plugin execution remains wider.
   - OS/typed clipboard evidence:
     `rg -n "clipboard|Cut|Copy|Paste|Duplicate|Delete|legacy|entity" crates/editor-shell/src crates/editor-actions/src crates/editor-egui-host/src crates/editor-ui/src`
     and
     `rg -n "OS clipboard|system clipboard|arboard|copypasta|Clipboard|entity_clipboard|clone_entity_blobs|paste_copied_entities|copy_selected_entities|cut_selected_entities" crates/editor-shell/src crates/editor-actions/src crates/editor-egui-host/src crates/editor-ui/src Cargo.toml crates`
     showed `entity_clipboard`, `clone_entity_blobs`,
     `copy_selected_entities`, `paste_copied_entities`, and comments stating
     the path is not the OS clipboard or authoritative CAD/projection clone.
     OS/typed clipboard semantics remain a product/substrate decision.
   - CAD/CommandBus mutation evidence:
     `rg -n "CommandBus|Action|Cad|cad_|projection|dirty|undo|save|load" crates/editor-shell/src crates/editor-actions/src crates/cad-core/src crates/cad-projection/src`
     and
     `rg -n "CadCheckpoint|CadGraph|CadProjection|CommandBus|pub trait Action|fn submit\(|fn mark_saved\(|fn is_dirty\(|save_source|render_mesh_for" crates/editor-actions/src crates/editor-shell/src crates/cad-core/src crates/cad-projection/src`
     confirmed `CommandBus::submit` still accepts `Action` over `World`,
     save/dirty is owned through the bus/save-source paths, and CAD graph plus
     projection have their own checkpoint/projection surfaces. Authoritative
     CAD/projection mutation through the bus remains too broad for this slice.
   - Camera/navigation evidence:
     `rg -n "MouseWheel|right|middle|double|frame|reset_camera|is_pointer_over_viewport_tab|pointer capture|camera|viewport_navigation|current_scene_bounds" crates/editor-shell/src`
     and
     `rg -n "frame_selected|pointer capture|window grab|camera persistence|ViewportLeftDoubleClick|ViewportOrbitDrag|ViewportPanDrag|zoom_camera_for_viewport_mouse_wheel|reset_camera\(|current_scene_bounds|is_pointer_over_viewport_tab" crates/editor-shell/src`
     confirmed viewport-local wheel zoom, right-button orbit, middle-button pan,
     left-double-click frame-all via `reset_camera`, and scene bounds framing.
     Frame-selected, pointer-capture/window-grab, camera persistence, and
     controller policy remain broader than this dispatch.
   - Selection: under the standing delegated Human=Codex / non-stop policy, the
     smallest source-safe boundary is task 129, a host-local command-palette
     shortcut-conflict annotation that reuses already-projected conflict data.
     No task 129 implementation was performed, no Rust/Cargo/workflow/automation
     files were edited, and no task 130 was appended.

   **MAY edit:**
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - generated ISSUE-<n> handoff/audit/log artifacts for this dispatch only

   **MUST NOT edit:**
   - Rust source or tests under `crates/**`, `editor/**`, `kernel/**`,
     `runtime/**`, `tools/**`, or `plugins/**`
   - Cargo manifests or `Cargo.lock`
   - GitHub workflows
   - dispatch automation, guard, queue, scheduler, watcher, verification, or
     health/trend scripts
   - schemas, ADR files, architecture-lint rules/config, packet templates, or
     existing unrelated handoff/log artifacts
   - plugin runtime/discovery/loading implementation
   - `MenuCommandHandoff` storage semantics, host-shell FIFO replacement, or
     `EditorShell::route_menu_command`
   - command-palette activation semantics, accelerator execution semantics, or
     menu-click behavior
   - shortcut remapping UI, user preferences, persistence, or fatal startup
     policy as implementation
   - OS/typed clipboard behavior
   - CAD graph/projection mutation, `CommandBus` action signatures,
     undo/dirty/save-load authority, or save/load behavior
   - camera/navigation behavior

   **Done criteria:**
   - The audit records the raw embedded dispatcher snapshot evidence or concrete
     local artifact/read path in the TASK packet, then records the snapshot
     facts in the audit result.
   - The audit records a local
     `rg -n "^126\.|^127\.|^128\.|^129\." .ai/dispatch.tasks.md` check from
     before editing.
   - The audit records source-grounded evidence for each candidate class above,
     including at least one falsifying grep per class where practical.
   - Exactly one bounded implementation task 129 is appended with explicit MAY
     edit, MUST-NOT edit, done criteria, verification, and halt conditions; or
     task 128 records `NEEDS_HUMAN` with concrete source evidence explaining why
     no bounded task can be safely specified.
   - If delegated Human=Codex policy is used, the selected policy is stated
     explicitly in task 129 and kept to the smallest source-safe boundary.
   - No implementation work for task 129 is done, and no task 130 is appended.
   - `git diff --name-only` shows only MAY-edit docs plus this dispatch's own
     generated artifacts; `git diff --check` is clean.

   **Verification:**
   - `rg -n "^126\.|^127\.|^128\.|^129\." .ai/dispatch.tasks.md`
   - Candidate source greps covering keybinding/remap, command routing,
     plugin/extension execution, clipboard, CommandBus/CAD mutation, and camera
     follow-up surfaces
   - `git diff --name-only`
   - `git diff --check`

   **Halt conditions:**
   - The TASK packet for the audit does not include the raw dispatcher snapshot
     excerpt or an exact local artifact/read path to that excerpt, and a plan
     revision cannot fix it within `MaxPlanRevisions`.
   - The audit begins implementing task 129, writing Rust, editing Cargo,
     changing workflows, or changing automation.
   - The audit requires live GitHub/network evidence instead of the embedded
     snapshot plus local source reads.
   - No bounded task 129 can be specified without editing a MUST-NOT path.

129. **[DONE 2026-06-14 via ISSUE-383] Annotate command-palette shortcut conflicts.**
   Extend the host-local command palette so a projected row whose displayed
   shortcut is currently conflicted exposes that conflict as informational row
   detail. Source the annotation only from the existing
   `ProjectedMainMenu.conflicts` projection, preserve current command-palette
   activation behavior, and do not add remapping, preferences, persistence,
   fatal startup policy, routing changes, or shortcut execution changes.

   **Policy boundary:**
   Standing delegated Human=Codex / non-stop policy is used only to choose the
   smallest source-safe diagnostic boundary. The established policy remains:
   shortcut conflicts are visible diagnostics and are not executable through
   keyboard accelerators, but they are not fatal and are not remapped here.
   Task 129 only makes the same conflict fact visible inside the command
   palette rows that already display shortcut hints.

   **MAY edit:**
   - `crates/editor-egui-host/src/menu.rs`
   - `crates/editor-egui-host/src/menu_tests.rs`
   - `crates/editor-egui-host/src/lib.rs` only if needed to thread/display the
     new row detail from the existing command-palette call site
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - generated ISSUE-<n> handoff/audit/log artifacts for this dispatch only

   **MUST NOT edit:**
   - `crates/editor-egui-host/src/shortcut_help.rs`
   - `crates/editor-egui-host/src/shortcut_conflicts.rs`
   - `crates/editor-ui/**`
   - `crates/editor-shell/**`
   - `crates/editor-actions/**`
   - `kernel/**`, `runtime/**`, `tools/**`, `editor/**`, `plugins/**`
   - Cargo manifests or `Cargo.lock`
   - GitHub workflows
   - dispatch automation, guard, queue, scheduler, watcher, verification, or
     health/trend scripts
   - schemas, ADR files, architecture-lint rules/config, packet templates, or
     existing unrelated handoff/log artifacts
   - `MenuCommandHandoff` storage semantics, host-shell FIFO replacement,
     `EditorShell::route_menu_command`, command-palette activation semantics,
     accelerator execution semantics, menu-click behavior, shortcut remapping
     UI, user preferences, shortcut persistence, fatal startup policy,
     plugin runtime/discovery/loading, OS/typed clipboard behavior, CAD graph
     or projection mutation, `CommandBus` action signatures, undo/dirty/save-
     load authority, save/load behavior, or camera/navigation behavior

   **Done criteria:**
   - `ProjectedCommandPaletteEntry` or an equivalent host-local palette row
     projection exposes deterministic conflict detail for enabled rows whose
     displayed shortcut matches one `ProjectedMainMenu.conflicts` shortcut.
   - The detail includes the ordered peer entry ids from the matching
     `ProjectedShortcutConflict.entries` vector, preserving projection order.
   - Unconflicted enabled rows expose no conflict detail. Disabled rows keep the
     existing disabled command-palette behavior and are not made activatable by
     the annotation.
   - The command-palette UI renders the conflict detail as informational text
     or a stable row state without changing filtering, fuzzy scoring, pinned or
     recent ordering, keyboard selection, Enter activation, mouse click
     activation, Pin/Unpin behavior, or close/search-focus behavior.
   - The existing Shortcut Conflicts window and Keyboard Shortcuts help surfaces
     continue to behave as before; task 129 does not move their ownership or
     change their rows.
   - `.ai/dispatch.tasks.md`, `Status.md`, `HANDOFF.md`, `plans/BASELINE.md`,
     and `change.md` mark task 129 complete when implemented, and no task 130
     is appended by the implementation unless a later audit packet explicitly
     authorizes it.

   **Verification:**
   - `cargo test -p rge-editor-egui-host --lib command_palette`
   - `cargo test -p rge-editor-egui-host --lib shortcut_conflict`
   - `cargo test -p rge-editor-egui-host --lib shortcut_help`
   - `cargo check -p rge-editor-egui-host --lib`
   - `cargo +nightly fmt --all -- --check`
   - `rg -n "^129\.|^130\." .ai/dispatch.tasks.md`
   - `git diff --name-only`
   - `git diff --check`

   **Halt conditions:**
   - The implementation needs to edit any MUST-NOT path or change command
     routing, command-palette activation, shortcut execution, remapping,
     persistence, fatal policy, plugin runtime/discovery/loading, OS clipboard,
     CAD/CommandBus, save/load, or camera behavior.
   - Conflict detail cannot be sourced solely from the already-projected
     `ProjectedMainMenu.conflicts` data.
   - The UI change requires adding a command, menu entry, accelerator,
     preferences surface, new diagnostics window, or registry re-resolution in
     the command-palette rendering path.
   - Verification fails, or `git diff --name-only` shows any file outside the
     MAY-edit list plus generated artifacts for this dispatch.

130. **[DONE 2026-06-14 via ISSUE-384] Post-command-palette-conflict Phase 9 next-task source audit.**
   Perform a docs/source-read-only audit after task 129. Re-check current local
   source and status docs, compare the remaining Phase 9/editor-usability
   candidate classes, and append exactly one bounded implementation follow-up
   as task 131 or record source-grounded `NEEDS_HUMAN`.

   **Context snapshot:**
   - Task 129 shipped as ISSUE-383 / commit `64061a5`: command-palette rows now
     expose ordered conflict peer entry ids from already-projected
     `ProjectedMainMenu.conflicts.entries` for enabled rows whose displayed
     shortcut exactly matches a projected conflict. Filtering, fuzzy scoring,
     pinned/recent ordering, selection, Enter/mouse activation, Pin/Unpin,
     search focus, Shortcut Help, Shortcut Conflicts, shortcut execution, menu
     clicks, `MenuCommandHandoff`, `EditorShell::route_menu_command`,
     remapping/persistence/fatal policy, plugin runtime/discovery/loading, OS
     clipboard, CAD/CommandBus, save/load, and camera/navigation behavior were
     unchanged.
   - The queue is empty and unblocked at re-arm time; `gh issue list --state open
     --label ai-dispatch` returned `[]`, and task headings show tasks 120-129
     complete with no task 130 before this re-arm.
   - The auto-created issue body will include the dispatcher GitHub-state
     snapshot. The audit must use that embedded snapshot, or an exact local
     artifact/read path to it, for GitHub queue/already-filed-task evidence. Do
     not call `gh` or the network from inside the executor sandbox for those
     claims.

   **Candidate classes to compare from current source:**
   - Keybinding/remap policy after tasks 123, 125, 127, and 129: conflicts are
     non-executable through accelerators and visible in Shortcut Conflicts,
     Keyboard Shortcuts help, and command-palette rows. Remaining work may
     include shortcut remapping UI, preferences/persistence, fatal startup
     policy, or another smaller diagnostic/policy slice if source-safe.
   - Host-shell command execution: menu clicks, command-palette activation, and
     accelerators still route through `MenuCommandHandoff` into
     `EditorShell::route_menu_command`; replacing that route or generalizing
     registry execution is broader unless a small host/shell seam exists.
   - Real plugin command execution: `Command::Custom` / `Command::Plugin`
     activations still stop at the injected `ExtensionCommandHandler` seam;
     real discovery/loading/runtime/capability execution remains broader unless
     a bounded seam-only follow-up is proven.
   - OS/typed clipboard: Edit Cut/Copy/Paste remains shell-local entity/legacy
     data; OS clipboard, typed component semantics, CAD identity, and
     cross-process behavior remain broader unless a narrow policy/substrate
     step is proven.
   - CAD/editor mutation through CommandBus: actions remain World-oriented;
     authoritative CAD/projection mutation, undo/dirty authority, and save/load
     semantics remain broader unless a bounded source-safe adapter/design slice
     is proven.
   - Camera/navigation follow-up after wheel zoom, right-button orbit,
     middle-button pan, and left-double-click frame-all: remaining work includes
     frame-selected/world-AABB, pointer capture/window grab, camera
     persistence, or broader controller policy.

   **MAY edit:**
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - generated ISSUE-<n> handoff/audit/log artifacts for this dispatch only

   **MUST NOT edit:**
   - Rust source or tests under `crates/**`, `editor/**`, `kernel/**`,
     `runtime/**`, `tools/**`, or `plugins/**`
   - Cargo manifests or `Cargo.lock`
   - GitHub workflows
   - dispatch automation, guard, queue, scheduler, watcher, verification, or
     health/trend scripts
   - schemas, ADR files, architecture-lint rules/config, packet templates, or
     existing unrelated handoff/log artifacts
   - shortcut remapping UI, user preferences, persistence, or fatal startup
     policy as implementation
   - `MenuCommandHandoff` storage semantics, host-shell FIFO replacement,
     generalized registry execution, or `EditorShell::route_menu_command`
   - command-palette activation semantics, accelerator execution semantics, or
     menu-click behavior
   - plugin runtime/discovery/loading implementation
   - OS/typed clipboard behavior
   - CAD graph/projection mutation, `CommandBus` action signatures,
     undo/dirty/save-load authority, or save/load behavior
   - camera/navigation behavior

   **Done criteria:**
   - The TASK packet records the raw embedded dispatcher snapshot evidence or a
     concrete local artifact/read path to it before making GitHub queue or
     already-filed-task claims.
   - The audit records a local
     `rg -n "^128\.|^129\.|^130\.|^131\." .ai/dispatch.tasks.md` check from
     before editing.
   - The audit records source-grounded evidence for each candidate class above,
     including at least one falsifying grep per class where practical.
   - Exactly one bounded implementation task 131 is appended with explicit MAY
     edit, MUST-NOT edit, done criteria, verification, and halt conditions; or
     task 130 records `NEEDS_HUMAN` with concrete source evidence explaining why
     no bounded task can be safely specified.
   - If delegated Human=Codex policy is used, the selected policy is stated
     explicitly in task 131 and kept to the smallest source-safe boundary.
   - No implementation work for task 131 is done, and no task 132 is appended.
   - `git diff --name-only` shows only MAY-edit docs plus this dispatch's own
     generated artifacts; `git diff --check` is clean.

   **Verification:**
   - `rg -n "^128\.|^129\.|^130\.|^131\." .ai/dispatch.tasks.md`
   - Candidate source greps covering keybinding/remap, command routing,
     plugin/extension execution, clipboard, CommandBus/CAD mutation, and camera
     follow-up surfaces
   - `git diff --name-only`
   - `git diff --check`

   **Halt conditions:**
   - The TASK packet for the audit does not include the raw dispatcher snapshot
     excerpt or an exact local artifact/read path to that excerpt, and a plan
     revision cannot fix it within `MaxPlanRevisions`.
   - The audit begins implementing task 131, writing Rust, editing Cargo,
     changing workflows, or changing automation.
   - The audit requires live GitHub/network evidence instead of the embedded
     snapshot plus local source reads.
   - No bounded task 131 can be specified without editing a MUST-NOT path.

   **Audit result (ISSUE-384):**
   Used only the embedded dispatcher GitHub snapshot in the ISSUE-384 TASK
   packet for queue/already-filed-task claims: it was generated at
   `2026-06-14T04:41:54.2273051+03:00`, showed no open `ai-dispatch` issue
   before ISSUE-384 was created, no open failed autonomous issues, and the
   relevant already-filed autonomous tail through closed #383. No `gh` or
   network command was run for those claims.

   Required pre-edit task-list check:
   `rg -n "^128\.|^129\.|^130\.|^131\." .ai/dispatch.tasks.md` ->
   `11504:128. **[DONE 2026-06-14 via ISSUE-382] Post-shortcut-conflict-help Phase 9 next-task audit.**`;
   `11705:129. **[DONE 2026-06-14 via ISSUE-383] Annotate command-palette shortcut conflicts.**`;
   `11798:130. **Post-command-palette-conflict Phase 9 next-task source audit.**`;
   no `131.` match.

   Source audit:
   - Keybinding/remap: `rg -n "struct ProjectedMenuEntry|ProjectedCommandPaletteEntry|ProjectedShortcutConflict|ProjectedMainMenu|pub\(crate\) fn project_main_menu|pub\(crate\) fn command_palette_entries|command_palette_conflict_peer_entry_ids|pub\(crate\) fn menu_item|fn command_palette_menu_item|conflict_peer_entry_ids|conflicted" crates/editor-egui-host/src/menu.rs`
     and
     `rg -n "menu_item\(|conflict_peer_entry_ids|shortcut_conflict_rows|shortcut_help_rows|command_palette_entries" crates/editor-egui-host/src/lib.rs crates/editor-egui-host/src/menu.rs crates/editor-egui-host/src/shortcut_conflicts.rs crates/editor-egui-host/src/shortcut_help.rs`
     confirmed that `ProjectedMainMenu.conflicts` feeds Shortcut Conflicts,
     Keyboard Shortcuts help, and command-palette conflict peer ids, while
     `menu_item` and its File/Edit/Play/View/Plugins call sites still receive
     only enabled/label/shortcut. The falsifying search
     `rg -n "conflict_peer_entry_ids|conflicted|shortcut_text|menu_item|Project(ed)?MenuEntry" crates/editor-egui-host/src/menu_tests.rs`
     found conflict-peer assertions only for command-palette entries, not for
     main-menu item presentation. This leaves one narrow host-local diagnostic
     slice before any remapping/preferences/fatal-policy work.
   - Host-shell command routing:
     `rg -n "MenuCommandHandoff|drain_and_route_menu_commands|route_menu_command|command_palette_window|selected_command_palette_entry|enabled_command_for_shortcut|WindowEvent::KeyboardInput|menu_command_handoff" crates/editor-egui-host/src crates/editor-shell/src crates/editor-ui/src/menus`
     confirmed menu clicks and command-palette activations still enqueue to
     `MenuCommandHandoff`, `EditorShell::drain_and_route_menu_commands` drains
     at frame top, and `EditorShell::route_menu_command` remains the route
     owner. Replacing the FIFO, route owner, or generalized registry execution
     remains broader than a presentation-only menu annotation.
   - Plugin execution:
     `rg -n "ExtensionCommandHandler|extension_command|plugin command|PluginCommand|Command::Plugin|Command::Custom|PluginHost|PluginContext|plugin_discovery|plugin-discovery|runtime_wasmtime|runtime-wasmtime" crates/editor-shell/src crates/editor-ui/src crates/plugin-discovery/src crates/runtime-wasmtime/src crates/runtime-wasmtime-engine/src editor/rge-editor/src`
     showed `Command::Custom` / `Command::Plugin` capture plus the injected
     `ExtensionCommandHandler` seam, a `rge-plugin-discovery` stub, and
     runtime crates, but no bounded editor-side real plugin discovery/loading
     route. Real plugin execution remains broader.
   - OS/typed clipboard:
     `rg -n "clipboard|Cut|Copy|Paste|Duplicate|Delete|legacy|entity_clipboard|clone_entity_blobs|paste_copied_entities|copy_selected_entities|cut_selected_entities|OS clipboard|system clipboard|arboard|copypasta|Clipboard" crates/editor-shell/src crates/editor-actions/src crates/editor-egui-host/src crates/editor-ui/src Cargo.toml crates`
     confirmed the current Edit Cut/Copy/Paste path is shell-local
     `entity_clipboard` / legacy component blobs and is explicitly not OS
     clipboard, typed kernel components, CAD graph/projection data, render
     meshes, command bus, or dirty/undo state. OS/typed clipboard semantics
     remain a product/substrate decision.
   - CAD/CommandBus mutation:
     `rg -n "CommandBus|Action|Cad|cad_|projection|dirty|undo|save|load|CadCheckpoint|CadGraph|CadProjection|fn submit\(|fn mark_saved\(|fn is_dirty\(|save_source|render_mesh_for" crates/editor-shell/src crates/editor-actions/src crates/cad-core/src crates/cad-projection/src`
     confirmed `CommandBus::submit` still accepts `Action` over `World`,
     dirty/save state is tied to the command bus/save source, and CAD graph plus
     projection remain separate surfaces. Authoritative CAD/projection mutation
     through the bus remains too broad for one editor-usability follow-up.
   - Camera/navigation:
     `rg -n "MouseWheel|right|middle|double|frame|reset_camera|is_pointer_over_viewport_tab|pointer capture|window grab|camera|viewport_navigation|current_scene_bounds|frame_selected|ViewportLeftDoubleClick|ViewportOrbitDrag|ViewportPanDrag|zoom_camera_for_viewport_mouse_wheel" crates/editor-shell/src`
     confirmed viewport wheel zoom, right-button orbit, middle-button pan,
     left-double-click frame-all, `reset_camera`, scene bounds framing, and
     viewport hit gating are present. The falsifying grep
     `rg -n "frame_selected|selected.*frame|selection.*frame|pointer capture|window grab|camera persistence|persist.*camera|camera.*persist" crates/editor-shell/src`
     returned no matches, so frame-selected, pointer capture/window grab, and
     camera persistence remain policy/substrate work.

   Selection: under the standing delegated Human=Codex / non-stop policy, the
   smallest source-safe boundary is task 131, a host-local main-menu shortcut
   conflict annotation that reuses already-projected conflict data. Broader
   remapping/preferences/fatal startup policy, host-shell route replacement,
   real plugin execution, OS/typed clipboard, CAD/CommandBus mutation, and
   camera/navigation policy remain deferred. No task 131 implementation was
   performed, no Rust/Cargo/workflow/automation files were edited, and no task
   132 was appended.

131. **[DONE 2026-06-14 via ISSUE-385] Annotate main-menu shortcut conflicts.**
   Extend the host-local main-menu item presentation so a menu item whose
   displayed shortcut is currently conflicted exposes that conflict as
   informational item detail. Source the annotation only from the existing
   `ProjectedMainMenu.conflicts` projection, preserve current menu-click,
   command-palette, and accelerator behavior, and do not add remapping,
   preferences, persistence, fatal startup policy, routing changes, or shortcut
   execution changes.

   **Policy boundary:**
   Standing delegated Human=Codex / non-stop policy is used only to choose the
   smallest source-safe diagnostic boundary. The established policy remains:
   shortcut conflicts are visible diagnostics and are not executable through
   keyboard accelerators, but they are not fatal and are not remapped here.
   Task 131 only makes the same conflict fact visible on the main-menu item
   that already displays the shortcut hint.

   **MAY edit:**
   - `crates/editor-egui-host/src/menu.rs`
   - `crates/editor-egui-host/src/menu_tests.rs`
   - `crates/editor-egui-host/src/lib.rs` only if needed to thread already-
     projected conflict detail into existing `menu_item` call sites
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - generated ISSUE-<n> handoff/audit/log artifacts for this dispatch only

   **MUST NOT edit:**
   - `crates/editor-egui-host/src/shortcut_help.rs`
   - `crates/editor-egui-host/src/shortcut_conflicts.rs`
   - `crates/editor-egui-host/src/palette_recent.rs`
   - `crates/editor-egui-host/src/palette_pinned.rs`
   - `crates/editor-ui/**`
   - `crates/editor-shell/**`
   - `crates/editor-actions/**`
   - `kernel/**`, `runtime/**`, `tools/**`, `editor/**`, `plugins/**`
   - Cargo manifests or `Cargo.lock`
   - GitHub workflows
   - dispatch automation, guard, queue, scheduler, watcher, verification, or
     health/trend scripts
   - schemas, ADR files, architecture-lint rules/config, packet templates, or
     existing unrelated handoff/log artifacts
   - `MenuCommandHandoff` storage semantics, host-shell FIFO replacement,
     `EditorShell::route_menu_command`, command-palette activation semantics,
     accelerator execution semantics, menu-click behavior, shortcut remapping
     UI, user preferences, shortcut persistence, fatal startup policy,
     plugin runtime/discovery/loading, OS/typed clipboard behavior, CAD graph
     or projection mutation, `CommandBus` action signatures, undo/dirty/save-
     load authority, save/load behavior, or camera/navigation behavior

   **Done criteria:**
   - Enabled main-menu entries whose displayed shortcut matches one
     `ProjectedMainMenu.conflicts` shortcut expose deterministic conflict
     detail sourced only from that matching `ProjectedShortcutConflict.entries`
     vector, preserving projection order.
   - Unconflicted enabled entries expose no conflict detail. Disabled entries
     keep their existing disabled menu behavior and are not made activatable by
     the annotation.
   - The menu item UI renders the conflict detail as informational text or a
     stable item state without changing top-level menu ordering, shortcut text,
     hover/click behavior, command enqueueing, command-palette projection,
     shortcut-help rows, or Shortcut Conflicts rows.
   - Projected plugin menu entries can carry conflict detail when the existing
     projected menu data says they are enabled and conflicted; no plugin
     runtime/discovery/loading or registry re-resolution is added.
   - Existing read-only guarantees remain: building/rendering menu item
     annotations does not enqueue menu commands, mutate command-palette
     recents/pins/search state, open/close diagnostic windows, or alter the
     projected menu.
   - `.ai/dispatch.tasks.md`, `Status.md`, `HANDOFF.md`,
     `plans/BASELINE.md`, and `change.md` mark task 131 complete when
     implemented, and no task 132 is appended by the implementation unless a
     later audit packet explicitly authorizes it.

   **Verification:**
   - `cargo test -p rge-editor-egui-host --lib menu`
   - `cargo test -p rge-editor-egui-host --lib command_palette`
   - `cargo test -p rge-editor-egui-host --lib shortcut_conflict`
   - `cargo check -p rge-editor-egui-host --lib`
   - `cargo +nightly fmt --all -- --check`
   - `rg -n "^130\.|^131\.|^132\." .ai/dispatch.tasks.md`
   - `git diff --name-only`
   - `git diff --check`

   **Halt conditions:**
   - The implementation needs to edit any MUST-NOT path or change command
     routing, menu-click behavior, command-palette activation, shortcut
     execution, remapping, persistence, fatal policy, plugin runtime/discovery/
     loading, OS clipboard, CAD/CommandBus, save/load, or camera behavior.
   - Conflict detail cannot be sourced solely from the already-projected
     `ProjectedMainMenu.conflicts` data.
   - The UI change requires adding a command, menu entry, accelerator,
     preferences surface, new diagnostics window, or registry re-resolution in
     the main-menu rendering path.
   - Verification fails, or `git diff --name-only` shows any file outside the
     MAY-edit list plus generated artifacts for this dispatch.

   **Implementation result (ISSUE-385):**
   Added a host-local `ProjectedMainMenuItem` annotation path in
   `editor-egui-host` that copies ordered conflict peer ids only from the
   matching already-projected `ProjectedMainMenu.conflicts` entry when the row
   is enabled and its displayed shortcut matches exactly. `EguiHost::render`
   now threads those annotated rows into the existing `menu_item` click response;
   `menu_item` exposes peer ids as non-command informational text beside the
   button response, so no second command target, menu entry, shortcut text
   mutation, top-level menu change, or command routing change is introduced.
   Focused `menu_tests.rs` coverage pins
   enabled conflicted, unconflicted enabled, disabled-conflicted, and
   plugin-projected annotation behavior, including row order, shortcut text,
   command identity, and enabled state preservation.

   **Explicit non-changes:** no task 132 was appended, and no shortcut help,
   Shortcut Conflicts, palette recent/pinned, `editor-ui`, `editor-shell`,
   `editor-actions`, Cargo, workflow, automation, schema, ADR, plugin runtime,
   routing, shortcut execution, remapping/persistence/fatal policy, OS
   clipboard, CAD/CommandBus, save/load, or camera/navigation behavior changed.

132. **[DONE 2026-06-14 via ISSUE-386] Post-main-menu-conflict Phase 9 next-task source audit.**
   Run a docs/source-read-only audit after ISSUE-385 / task 131. Use current
   local source reads plus the dispatcher-provided GitHub-state snapshot from
   the auto-created issue body for queue/already-filed-task evidence; do not
   call `gh`, browse the network, or use live GitHub state from inside the
   executor sandbox. Compare the remaining editor-usability candidate classes
   after Shortcut Conflicts, Keyboard Shortcuts help, command-palette rows, and
   main-menu items all expose shortcut-conflict diagnostics:

   - keybinding/remap/preferences/fatal-policy work after tasks 123, 125, 127,
     129, and 131;
   - host-shell command routing through `MenuCommandHandoff` /
     `EditorShell::route_menu_command`;
   - real plugin command execution after the injected extension-command seam;
   - OS/typed clipboard behavior beyond the current shell-local clipboard;
   - CAD/editor mutation through `CommandBus`, projection, undo/dirty, and
     save/load authority;
   - camera/navigation follow-up after wheel zoom, right-button orbit,
     middle-button pan, and left-double-click frame-all.

   Append exactly one bounded implementation follow-up as task 133, or record
   source-grounded `NEEDS_HUMAN` if every remaining candidate crosses a policy
   or architecture boundary that cannot be safely delegated.

   **MAY edit:**
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - generated ISSUE-<n> handoff/audit/log artifacts for this dispatch only

   **MUST NOT edit:**
   - Rust source or tests
   - Cargo manifests or `Cargo.lock`
   - workflows
   - dispatch automation, guard, queue, scheduler, watcher, verification, or
     health/trend scripts
   - schemas, ADR files, architecture-lint rules/config, packet templates, or
     unrelated existing handoff/log artifacts
   - plugin runtime/discovery/loading code, command routing, shortcut
     execution, remapping/persistence/fatal policy, OS clipboard behavior,
     CAD/projection/CommandBus mutation, undo/dirty/save-load authority, or
     camera/navigation behavior

   **Done criteria:**
   - The audit records the pre-edit task-heading check for
     `^130\.|^131\.|^132\.|^133\.`.
   - Queue/already-filed-task claims cite only the dispatcher-provided snapshot
     embedded in the issue body or an exact local artifact path copied from it;
     no live `gh`/network query is run by the sandboxed executor.
   - Each candidate class above has positive source references and falsifying
     searches for negative claims where practical.
   - Exactly one bounded implementation task 133 is appended with explicit
     `MAY edit`, `MUST NOT edit`, `Done criteria`, `Verification`, and `Halt
     conditions`, or a source-grounded `NEEDS_HUMAN` record is written.
   - No implementation work for task 133 is done, and no task 134 is appended.

   **Verification:**
   - `rg -n "^130\.|^131\.|^132\.|^133\." .ai/dispatch.tasks.md` before edits
     and after edits
   - candidate-class source greps recorded in the audit
   - `git diff --name-only`
   - `git diff --check`

   **Halt conditions:**
   - The executor cannot cite the dispatcher-provided GitHub-state snapshot
     without live `gh`/network access.
   - The audit would require editing a MUST-NOT path or implementing task 133.
   - More than one implementation follow-up would be required to make the
     selected boundary coherent.
   - No bounded task 133 can be specified without crossing a policy or
     architecture boundary; record `NEEDS_HUMAN` instead of forcing a task.

   **Audit result (ISSUE-386):**
   Used only the embedded dispatcher GitHub snapshot in the ISSUE-386 TASK
   packet for queue/already-filed-task claims: it was generated at
   `2026-06-14T06:37:54.1087048+03:00`, showed no open `ai-dispatch` issue
   before ISSUE-386 was created, no open failed autonomous issues, and the
   relevant already-filed autonomous tail through closed #385. No `gh` or
   network command was run for those claims. Falsifying search:
   `rg -n '^Generated by .*Invoke-AiDispatchAuto\.ps1|^- Open .*ai-dispatch.*issues|^- Open failed autonomous|^- Autonomous issues already filed|^  - .*#385 \[CLOSED\] Annotate main-menu shortcut conflicts' ai_handoffs/ISSUE-386_TASK_2026-06-14_06-38-22+0300.md` ->
   ``42:Generated by `Invoke-AiDispatchAuto.ps1` at``;
   ``46:- Open `ai-dispatch` issues before ISSUE-386 was created: `(none)```;
   ``47:- Open failed autonomous issues with `ai-auto` + `ai-dispatch-failed`: `(none)```;
   ``48:- Autonomous issues already filed (`ai-auto`, all states), relevant recent tail:``;
   ``78:  - `#385 [CLOSED] Annotate main-menu shortcut conflicts``` .

   Required pre-edit task-list check:
   `rg -n "^130\.|^131\.|^132\.|^133\." .ai/dispatch.tasks.md` ->
   `11798:130. **[DONE 2026-06-14 via ISSUE-384] Post-command-palette-conflict Phase 9 next-task source audit.**`;
   `11988:131. **[DONE 2026-06-14 via ISSUE-385] Annotate main-menu shortcut conflicts.**`;
   `12107:132. **Post-main-menu-conflict Phase 9 next-task source audit.**`;
   no `^133.` match.

   Status-doc cross-check:
   `rg -n "post-ISSUE-385 full-automation re-arm|ISSUE-385 task-131 main-menu conflict annotation|task 132|task 133|ISSUE-385|main-menu conflict|post-ISSUE-385" Status.md HANDOFF.md plans/BASELINE.md change.md`
   confirmed current status docs record ISSUE-385 / task 131 complete, the
   post-ISSUE-385 re-arm as task 132, and the task-132 requirement to append
   exactly one task 133 or record `NEEDS_HUMAN`.

   Source audit:
   - Keybinding/remap/preferences/fatal policy:
     `rg -n "enabled_command_for_shortcut|command_for_shortcut|AcceleratorTable|ProjectedShortcutConflict|shortcut_help_rows|shortcut_conflict_rows|command_palette_conflict_peer_entry_ids|annotated_main_menu_items|remap|preferences|persist|fatal|conflicted" crates/editor-ui/src/menus crates/editor-egui-host/src`
     confirmed the complete shortcut-conflict diagnostic chain:
     `ResolveResult::enabled_command_for_shortcut` suppresses conflicted
     accelerators, `shortcut_conflict_rows` and `shortcut_help_rows` consume
     `ProjectedMainMenu.conflicts`, command-palette rows expose conflict peer
     ids, and `annotated_main_menu_items` now feeds the main menu. The
     falsifying search
     `rg -n "remap|rebind|preferences|shortcut.*persist|persist.*shortcut|fatal|startup.*conflict|conflict.*fatal" crates/editor-ui/src/menus crates/editor-egui-host/src crates/editor-shell/src`
     found no shortcut remapping UI, shortcut preferences persistence, or fatal
     startup-conflict policy; remaining matches were non-fatal diagnostics,
     command-palette recent/pinned persistence, or unrelated remap text.
   - Host-shell command routing:
     `rg -n "MenuCommandHandoff|drain_and_route_menu_commands|route_menu_command|command_palette_window|selected_command_palette_entry|enabled_command_for_shortcut|WindowEvent::KeyboardInput|menu_command_handoff|Command::" crates/editor-egui-host/src crates/editor-shell/src crates/editor-ui/src/menus`
     confirmed menu clicks and command-palette activations enqueue through
     `MenuCommandHandoff`, `EditorShell::drain_and_route_menu_commands` drains
     the FIFO at frame top, keyboard accelerators resolve through
     `enabled_command_for_shortcut`, and `EditorShell::route_menu_command`
     remains the shared route owner. Replacing the FIFO, the route owner, or
     registry execution remains broader than one source-safe follow-up.
   - Real plugin command execution:
     `rg -n "ExtensionCommandHandler|extension_command|plugin command|PluginCommand|Command::Plugin|Command::Custom|PluginHost|PluginContext|plugin_discovery|plugin-discovery|runtime_wasmtime|runtime-wasmtime" crates/editor-shell/src crates/editor-ui/src crates/plugin-discovery/src crates/runtime-wasmtime/src crates/runtime-wasmtime-engine/src editor/rge-editor/src`
     confirmed `Command::Custom` / `Command::Plugin` are captured by the
     injected `ExtensionCommandHandler` seam, `rge-plugin-discovery` is still a
     stub, and runtime crates exist separately. The extension-command module
     explicitly says it does not own plugin discovery, loading, runtime
     execution, capabilities, async dispatch, sandboxing, or registry
     execution, so real plugin command execution remains broader.
   - OS/typed clipboard:
     `rg -n "clipboard|Cut|Copy|Paste|Duplicate|Delete|legacy|entity_clipboard|clone_entity_blobs|paste_copied_entities|copy_selected_entities|cut_selected_entities|OS clipboard|system clipboard|arboard|copypasta|Clipboard" crates/editor-shell/src crates/editor-actions/src crates/editor-egui-host/src crates/editor-ui/src Cargo.toml crates`
     confirmed Edit Cut/Copy/Paste uses shell-local `entity_clipboard` cloned
     legacy component blobs. `copy_selected_entities` and
     `cut_selected_entities` state that they do not touch the OS clipboard,
     typed kernel components, CAD graph/projection data, render meshes, the
     command bus, or dirty/undo state. The falsifying search
     `rg -n "arboard|copypasta|Clipboard|system clipboard|OS clipboard|clipboard.*system|with_clipboard|clipboard_provider" Cargo.toml crates/editor-shell/src crates/editor-actions/src crates/editor-egui-host/src crates/editor-ui/src crates`
     found no OS/system clipboard dependency or provider, only the current
     shell-local comments and tests.
   - CAD/editor mutation through `CommandBus`:
     `rg -n "CommandBus|Action|Cad|cad_|projection|dirty|undo|save|load|CadCheckpoint|CadGraph|CadProjection|fn submit\(|fn mark_saved\(|fn is_dirty\(|save_source|render_mesh_for" crates/editor-shell/src crates/editor-actions/src crates/cad-core/src crates/cad-projection/src`
     confirmed `CommandBus::submit` still applies boxed `Action`s to
     `rge_kernel_ecs::World`, owns undo/redo plus dirty/save markers, while
     CAD graph and projection remain separate surfaces. Current Edit delete,
     duplicate, copy, paste, and cut comments explicitly avoid CAD graph,
     projection, render mesh, command bus, dirty, and undo mutation, so an
     authoritative CAD/projection mutation follow-up remains too broad.
   - Camera/navigation:
     `rg -n "MouseWheel|right|middle|double|frame|reset_camera|is_pointer_over_viewport_tab|pointer capture|window grab|camera|viewport_navigation|current_scene_bounds|frame_selected|ViewportLeftDoubleClick|ViewportOrbitDrag|ViewportPanDrag|zoom_camera_for_viewport_mouse_wheel" crates/editor-shell/src`
     confirmed viewport wheel zoom, right-button orbit, middle-button pan,
     left-double-click frame-all, `reset_camera`, live scene bounds framing, and
     viewport hit gating are present. The focused cursor-grab read
     `rg -n "start_viewport_orbit_drag|stop_viewport_orbit_drag|start_viewport_pan_drag|stop_viewport_pan_drag|CursorGrabMode|set_cursor_grab|cursor grab|window grab|pointer capture" crates/editor-shell/src/lifecycle/mod.rs crates/editor-shell/src/lifecycle/viewport_navigation.rs crates/editor-shell/src/lifecycle/tests.rs`
     showed existing drag start/stop methods and tests but no cursor-grab
     symbols. The falsifying search
     `rg -n "CursorGrabMode|set_cursor_grab|cursor grab|window grab|pointer capture" crates/editor-shell/src/lifecycle/mod.rs crates/editor-shell/src/lifecycle/viewport_navigation.rs crates/editor-shell/src/lifecycle/tests.rs`
     returned no matches.

   Selection: under the standing delegated Human=Codex / non-stop policy, the
   smallest source-safe remaining implementation boundary is task 133: add
   viewport-only cursor grab/release around the already-existing right-button
   orbit and middle-button pan drag paths. This is narrower than shortcut
   remapping/preferences/fatal policy, host-shell route replacement, real plugin
   runtime/discovery/loading, OS/typed clipboard semantics, or authoritative
   CAD/projection/CommandBus mutation. No implementation work for task 133 was
   performed, no Rust/Cargo/workflow/automation files were edited, and no task
   134 was appended.

133. **[DONE 2026-06-14 via ISSUE-387] Add viewport drag cursor grab for camera orbit and pan.**
   Implement a bounded editor-shell camera/navigation follow-up: when a
   viewport-only right-button orbit drag or middle-button pan drag starts, the
   shell should attempt to grab/confine the cursor through the current winit
   `Window`; when the active viewport drag ends, it should release that grab.
   The grab is interaction polish around the existing drag modes only: it must
   not change camera math, mouse-button bindings, viewport hit testing,
   left-click picking, left-double-click frame-all, menu/accelerator routing, or
   any non-camera subsystem.

   **Policy boundary:**
   Standing delegated Human=Codex / non-stop policy is used only to choose this
   smallest source-safe camera-navigation boundary. Cursor-grab failure must be
   non-fatal and must not prevent the existing drag from starting; headless
   shells and shells without a winit window keep current no-grab behavior.

   **MAY edit:**
   - `crates/editor-shell/src/lifecycle/mod.rs`
   - `crates/editor-shell/src/lifecycle/viewport_navigation.rs` only if a small
     drag/grab state helper is needed
   - `crates/editor-shell/src/lifecycle/tests.rs`
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - generated ISSUE-<n> handoff/audit/log artifacts for this dispatch only

   **MUST NOT edit:**
   - `crates/editor-ui/**`
   - `crates/editor-egui-host/**`
   - `crates/editor-actions/**`
   - `crates/cad-core/**`
   - `crates/cad-projection/**`
   - `crates/plugin-discovery/**`
   - `crates/runtime-wasmtime/**`
   - `crates/runtime-wasmtime-engine/**`
   - `editor/**`, `kernel/**`, `runtime/**`, `tools/**`, or `plugins/**`
   - Cargo manifests or `Cargo.lock`
   - GitHub workflows
   - dispatch automation, guard, queue, scheduler, watcher, verification, or
     health/trend scripts
   - schemas, ADR files, architecture-lint rules/config, packet templates, or
     unrelated existing handoff/log artifacts
   - menu registry contents, `MenuCommandHandoff`, `EditorShell::route_menu_command`,
     command-palette activation, keyboard accelerator execution, shortcut
     remapping/preferences/persistence/fatal policy, plugin runtime/discovery/
     loading, OS/typed clipboard behavior, CAD graph/projection mutation,
     `CommandBus` action signatures, undo/dirty/save-load authority, save/load
     behavior, camera reset/frame/zoom/orbit/pan math, viewport hit testing,
     face picking, or left-double-click frame-all semantics

   **Done criteria:**
   - A right-button orbit drag that passes the existing finite-cursor and
     viewport-hit gates attempts a winit cursor grab before or as the drag
     becomes active; a failed grab is logged or ignored non-fatally and the drag
     remains active as it does today.
   - A middle-button pan drag that passes the existing finite-cursor and
     viewport-hit gates follows the same cursor-grab policy.
   - Releasing right or middle mouse stops the corresponding drag and releases
     the cursor grab when no viewport drag remains active.
   - Headless shells and shells without a winit window do not panic and preserve
     existing drag behavior without an OS cursor grab.
   - Existing wheel zoom, right-button orbit, middle-button pan, left-click
     picking, left-double-click frame-all, View menu camera commands, and
     command routing behavior remain unchanged except for the cursor grab/release
     side effect.
   - `.ai/dispatch.tasks.md`, `Status.md`, `HANDOFF.md`,
     `plans/BASELINE.md`, and `change.md` mark task 133 complete when
     implemented, and no task 134 is appended by the implementation unless a
     later audit packet explicitly authorizes it.

   **Verification:**
   - `cargo test -p rge-editor-shell --lib viewport_right_button_orbit`
   - `cargo test -p rge-editor-shell --lib viewport_middle_button_pan`
   - `cargo test -p rge-editor-shell --lib viewport_drag_cursor_grab`
   - `cargo check -p rge-editor-shell --lib`
   - `cargo +nightly fmt --all -- --check`
   - `rg -n "^132\.|^133\.|^134\." .ai/dispatch.tasks.md`
   - `git diff --name-only`
   - `git diff --check`

   **Halt conditions:**
   - The implementation needs to edit any MUST-NOT path or change command
     routing, shortcut execution, menu-click behavior, command-palette
     activation, shortcut remapping/preferences/persistence/fatal policy,
     plugin runtime/discovery/loading, OS clipboard, CAD/CommandBus, save/load,
     viewport hit testing, face picking, left-double-click frame-all, or camera
     motion math.
   - The winit cursor-grab API cannot be used without introducing platform-
     specific policy, a new dependency, or a broader window/input abstraction.
   - The implementation cannot keep cursor-grab failure non-fatal while
     preserving existing orbit/pan behavior.
   - More than one implementation follow-up would be required to make the
     cursor-grab boundary coherent.
   - Verification fails, or `git diff --name-only` shows any file outside the
     MAY-edit list plus generated artifacts for this dispatch.

   **Implementation result (ISSUE-387):**
   `editor-shell` now attempts `CursorGrabMode::Confined` through the existing
   optional winit `Window` when a valid viewport right-button orbit or
   middle-button pan drag starts, logs cursor-grab failures non-fatally, and
   releases with `CursorGrabMode::None` only after the final active viewport
   drag stops. Headless/no-window shells keep the existing drag behavior without
   an OS cursor grab. Focused lifecycle tests cover orbit start gating, pan
   start gating, failed/no-window grab behavior, and right/middle release
   ordering while both drags are active. No task 134 was appended.

134. **[DONE 2026-06-14 via ISSUE-388] Post-viewport-cursor-grab Phase 9 next-task source audit.**
   Run a docs/source-read-only audit after ISSUE-387 / task 133. Use current
   local source reads plus the dispatcher-provided GitHub-state snapshot from
   the auto-created issue body for queue/already-filed-task evidence; do not
   call `gh`, browse the network, or use live GitHub state from inside the
   executor sandbox. Compare the remaining editor-usability candidate classes
   after viewport wheel zoom, right-button orbit, middle-button pan,
   left-double-click frame-all, and cursor grab/release all exist:

   - keybinding/remap/preferences/fatal-policy work after tasks 123, 125, 127,
     129, and 131;
   - host-shell command routing through `MenuCommandHandoff` /
     `EditorShell::route_menu_command`;
   - real plugin command execution after the injected extension-command seam;
   - OS/typed clipboard behavior beyond the current shell-local clipboard;
   - CAD/editor mutation through `CommandBus`, projection, undo/dirty, and
     save/load authority;
   - camera/navigation follow-up after wheel zoom, orbit, pan, frame-all, and
     drag cursor grab/release.

   Append exactly one bounded implementation follow-up as task 135, or record
   source-grounded `NEEDS_HUMAN` if every remaining candidate crosses a policy
   or architecture boundary that cannot be safely delegated.

   **Context snapshot:**
   - Task 133 shipped as ISSUE-387 / commit `53644c2`: valid viewport
     right-button orbit and middle-button pan drag starts now attempt
     `CursorGrabMode::Confined` through the existing optional winit `Window`;
     failures are warn-logged and non-fatal, and `CursorGrabMode::None` is
     requested only after the final active viewport drag stops. Headless or
     no-window shells keep existing drag behavior without an OS cursor grab.
   - Existing camera reset/frame, wheel zoom, orbit/pan math, viewport hit
     testing, face picking, left-double-click frame-all, View menu commands,
     command routing, shortcuts, plugin runtime/discovery/loading, OS
     clipboard, CAD/projection/CommandBus mutation, undo/dirty/save-load
     behavior, Cargo metadata, workflows, automation, schemas, and non-
     `editor-shell` subsystems were unchanged by task 133.
   - Re-arm check before authoring task 134: `origin/main` and local `main`
     were synced at `53644c2`; `gh issue list --repo RustCADs/RGE --state open
     --label ai-dispatch --json number,title,labels,url` returned `[]`;
     `gh issue list --repo RustCADs/RGE --state open --label ai-dispatch-failed
     --json number,title,labels,url` returned `[]`; and
     `rg -n "^132\.|^133\.|^134\." .ai/dispatch.tasks.md` showed task 132
     DONE, task 133 DONE, and no task 134.
   - The queue has stale-claim protection from `fe6dbb4` / the dispatch
     stale-claim hardening update: `Invoke-AiDispatchQueue.ps1` sweeps
     queue-owned ADR-121 claims at startup and releases claims whose
     `Invoke-AiDispatchQueue.ps1:<pid>` owner is dead, no longer a queue
     process, or recycled after the claim timestamp. This re-arm addresses the
     different idle condition: the task brief was exhausted.
   - The auto-created issue body will include the dispatcher GitHub-state
     snapshot. The audit must use that embedded snapshot, or an exact local
     artifact/read path to it, for GitHub queue/already-filed-task evidence.
     Do not call `gh` or the network from inside the executor sandbox for those
     claims.

   **MAY edit:**
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - generated ISSUE-<n> handoff/audit/log artifacts for this dispatch only

   **MUST NOT edit:**
   - Rust source or tests
   - Cargo manifests or `Cargo.lock`
   - workflows
   - dispatch automation, guard, queue, scheduler, watcher, verification, or
     health/trend scripts
   - schemas, ADR files, architecture-lint rules/config, packet templates, or
     unrelated existing handoff/log artifacts
   - plugin runtime/discovery/loading code, command routing, shortcut
     execution, remapping/persistence/fatal policy, OS clipboard behavior,
     CAD/projection/CommandBus mutation, undo/dirty/save-load authority, or
     camera/navigation behavior

   **Done criteria:**
   - The audit records the pre-edit task-heading check for
     `^132\.|^133\.|^134\.|^135\.`.
   - Queue/already-filed-task claims cite only the dispatcher-provided snapshot
     embedded in the issue body or an exact local artifact path copied from it;
     no live `gh`/network query is run by the sandboxed executor.
   - Each candidate class above has positive source references and falsifying
     searches for negative claims where practical.
   - Exactly one bounded implementation task 135 is appended with explicit
     `MAY edit`, `MUST NOT edit`, `Done criteria`, `Verification`, and `Halt
     conditions`, or a source-grounded `NEEDS_HUMAN` record is written.
   - No implementation work for task 135 is done, and no task 136 is appended.

   **Verification:**
   - `rg -n "^132\.|^133\.|^134\.|^135\." .ai/dispatch.tasks.md` before edits
     and after edits
   - status-doc cross-check against `Status.md`, `HANDOFF.md`,
     `plans/BASELINE.md`, and `change.md`
   - candidate-class source greps recorded in the audit
   - `git diff --name-only`
   - `git diff --check`
   - `.\new-handoff.ps1 -Finalize -PacketPath <EXEC_PACKET> -DryRun`

   **Halt conditions:**
   - The executor cannot cite the dispatcher-provided GitHub-state snapshot
     without live `gh`/network access.
   - The audit would require editing a MUST-NOT path or implementing task 135.
   - More than one implementation follow-up would be required to make the
     selected boundary coherent.
   - No bounded task 135 can be specified without crossing a policy or
     architecture boundary; record `NEEDS_HUMAN` instead of forcing a task.

   **Audit result (ISSUE-388):**
   - Dispatcher GitHub-state evidence came only from the embedded ISSUE-388 TASK
     packet snapshot. The executor did not call `gh`, browse, or use network
     state. Snapshot verification:
     `rg -n '^(Generated by `Invoke-AiDispatchAuto\.ps1`|- Open `ai-dispatch` issues before ISSUE-388|- Open failed autonomous issues|  - `#387 \[CLOSED\] Add viewport drag cursor grab)' ai_handoffs/ISSUE-388_TASK_2026-06-14_08-29-03+0300.md`
     returned line 45 for the `Invoke-AiDispatchAuto.ps1` generation marker,
     line 49 for no open `ai-dispatch` issue before ISSUE-388, line 50 for no
     open failed autonomous issue, and line 83 for closed `#387 [CLOSED] Add
     viewport drag cursor grab for camera orbit and pan`.
   - Required pre-edit heading check:
     `rg -n "^132\.|^133\.|^134\.|^135\." .ai/dispatch.tasks.md`
     returned `12107:132. **[DONE 2026-06-14 via ISSUE-386] Post-main-menu-conflict Phase 9 next-task source audit.**`,
     `12276:133. **[DONE 2026-06-14 via ISSUE-387] Add viewport drag cursor grab for camera orbit and pan.**`,
     and `12384:134. **Post-viewport-cursor-grab Phase 9 next-task source audit.**`;
     there was no `^135.` match before editing.
   - Status-doc cross-check:
     `rg -n "ISSUE-387|task 133|task 134|task 135|viewport cursor grab|post-ISSUE-387|post-viewport-cursor-grab" .ai/dispatch.tasks.md Status.md HANDOFF.md plans/BASELINE.md change.md`
     matched `Status.md:3`, `HANDOFF.md:3`, `plans/BASELINE.md:5-12`,
     and `change.md:1907`, all recording ISSUE-387/task 133 complete and
     task 134 as the re-armed audit that must append one task 135 or record
     `NEEDS_HUMAN`.
   - Keybinding/remap/preferences/fatal-policy comparison: current source keeps
     conflicted shortcut execution suppressed through
     `ResolveResult::enabled_command_for_shortcut` in
     `crates/editor-ui/src/menus/registry.rs:320-321`; diagnostic surfaces now
     include Shortcut Conflicts (`crates/editor-egui-host/src/shortcut_conflicts.rs:11`),
     Keyboard Shortcuts help (`crates/editor-egui-host/src/shortcut_help.rs:66`),
     command-palette conflict peer ids (`crates/editor-egui-host/src/menu.rs:264`),
     and annotated main-menu items (`crates/editor-egui-host/src/menu.rs:209`).
     Falsifying search
     `rg -n "remap|keymap|preferences|ShortcutPreference|ShortcutRemap|fatal" crates/editor-ui/src/menus crates/editor-egui-host/src crates/editor-shell/src`
     found only existing fatal/non-fatal comments, command-palette
     recent/pinned persistence tests, and unrelated face-ID remapping comments;
     no shortcut-remap or shortcut-preferences implementation surface was found.
     Broader remap/preferences/fatal policy remains a policy boundary, not the
     smallest next task.
   - Host-shell command-routing comparison: `MenuCommandHandoff` is an existing
     bounded FIFO in `crates/editor-egui-host/src/handoff.rs:96-127`, drained at
     `crates/editor-shell/src/render_path.rs:366` and routed through
     `EditorShell::route_menu_command` at `crates/editor-shell/src/render_path.rs:415`.
     Route coverage already spans File/Edit/Play/View and extension commands in
     `crates/editor-shell/src/lifecycle/tests.rs:4112-5028`. Replacing route
     ownership or FIFO semantics would be broad and is not selected.
   - Real plugin-command execution comparison: extension activations are captured
     by the seam in `crates/editor-shell/src/lifecycle/extension_command.rs:61`,
     `:103`, `:113`, `:122`, and `:161`, while
     `crates/plugin-discovery/src/lib.rs:1` is still a stub crate and real
     Wasmtime loading/instantiation lives under `crates/runtime-wasmtime*`.
     The seam explicitly does not imply discovery/loading/runtime execution
     (`crates/editor-shell/src/lifecycle/extension_command.rs:101` and
     `crates/editor-shell/src/lifecycle/mod.rs:772`). Real execution would cross
     plugin discovery/loading/runtime ownership, so it is deferred.
   - OS/typed clipboard comparison: Edit Cut/Copy/Paste are shell-local legacy
     blob operations. `crates/editor-shell/src/lifecycle/mod.rs:778-783` stores
     `entity_clipboard`, and `copy_selected_entities`, `paste_copied_entities`,
     and `cut_selected_entities` at `:1788-1844` explicitly avoid the OS
     clipboard, typed kernel components, CAD/projection, render identity, and
     dirty/undo state. Falsifying search
     `rg -n "arboard|copypasta" Cargo.toml crates -g Cargo.toml` returned no
     matches. OS/typed clipboard work remains broader than the smallest
     source-safe next task.
   - CAD/editor mutation comparison: the mutation authority remains the
     `CommandBus` and undo/dirty/save/load stack. `CommandBus::submit`,
     `mark_saved`, and `is_dirty` live at `crates/editor-actions/src/bus.rs:143`,
     `:277`, and `:284`; shell wrappers are in
     `crates/editor-shell/src/lifecycle/commands.rs:274-313`; save/load and
     world replacement own `save_source`, dirty state, and CAD-field reset in
     `crates/editor-shell/src/lifecycle/mod.rs:909-943` and
     `crates/editor-shell/src/lifecycle/save_request.rs:221-267`. A CAD mutation
     task would cross CommandBus, projection, undo/dirty, and save/load
     authority, so it is deferred.
   - Camera/navigation comparison: wheel zoom, orbit, pan, frame-all, and drag
     cursor grab/release are all present. Current source starts viewport orbit
     and pan drags at `crates/editor-shell/src/lifecycle/mod.rs:2212` and
     `:2241`, requests `CursorGrabMode::Confined` at `:2219` and `:2248`, and
     releases with `CursorGrabMode::None` only through
     `stop_viewport_orbit_drag` / `stop_viewport_pan_drag` at `:2228-2262`.
     Existing tests cover release ordering at
     `crates/editor-shell/src/lifecycle/tests.rs:1391` and `:1415`. Falsifying
     search
     `rg -n "WindowEvent::Focused|Focused\(|focus loss|focus_lost|lost focus|CursorLeft|CursorExited|CursorEntered|cancel_viewport|stop_viewport.*drag.*focus" crates/editor-shell/src/lifecycle/mod.rs crates/editor-shell/src/lifecycle/tests.rs`
     returned no matches. The smallest source-safe remaining implementation
     boundary is therefore task 135: cancel active viewport orbit/pan drags and
     release the viewport drag cursor grab on window focus loss. No task 135
     implementation was done, and no task 136 was appended.

135. **[DONE 2026-06-14 via ISSUE-389] Release viewport drag cursor grab on window focus loss.**
   Implement one bounded `editor-shell` camera/navigation lifecycle polish:
   when the window loses focus while a viewport right-button orbit drag and/or
   middle-button pan drag is active, cancel the active viewport drag state and
   release the viewport drag cursor grab. This is a follow-up to task 133's
   cursor-grab start/release behavior; it must preserve current camera math,
   viewport hit testing, wheel zoom, left-click face picking, left-double-click
   frame-all, View menu camera commands, command routing, shortcuts, plugin
   runtime/discovery/loading, OS clipboard, CAD/projection/CommandBus mutation,
   undo/dirty, and save/load behavior.

   **MAY edit:**
   - `crates/editor-shell/src/lifecycle/mod.rs`
   - `crates/editor-shell/src/lifecycle/tests.rs`
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - generated ISSUE-<n> handoff/audit/log artifacts for this dispatch only

   **MUST NOT edit:**
   - Rust files outside `crates/editor-shell/src/lifecycle/mod.rs` and
     `crates/editor-shell/src/lifecycle/tests.rs`
   - Cargo manifests or `Cargo.lock`
   - workflows
   - dispatch automation, guard, queue, scheduler, watcher, verification, or
     health/trend scripts
   - schemas, ADR files, architecture-lint rules/config, packet templates, or
     unrelated existing handoff/log artifacts
   - command routing, `MenuCommandHandoff`, command-palette activation,
     accelerator execution, shortcut remapping/preferences/fatal policy, plugin
     runtime/discovery/loading, OS clipboard behavior, CAD/projection/CommandBus
     mutation, undo/dirty/save-load authority, camera reset/frame/zoom/orbit/pan
     math, viewport hit testing, face picking, or left-double-click frame-all
     behavior

   **Done criteria:**
   - `WindowEvent::Focused(false)` (or the current winit 0.30 equivalent in the
     existing `window_event` match) cancels any active viewport orbit drag and
     any active viewport pan drag.
   - If at least one viewport drag was active, focus loss requests
     `CursorGrabMode::None` through the existing cursor-grab helper after both
     drag states are idle. If no viewport drag was active, focus loss does not
     emit a new cursor-grab release request.
   - Focus gain preserves existing behavior and does not start, stop, or release
     viewport drags.
   - The implementation reuses the existing drag-stop/cursor-release helpers or
     an equivalently narrow private helper; it does not change drag math,
     `CursorGrabMode::Confined` start behavior, failed-grab behavior, dual-drag
     release ordering, wheel zoom, frame-all, or face-pick gates.
   - Focused tests cover focus-loss cancellation for active orbit/pan drags and
     the no-active-drag no-release case. Existing viewport drag cursor-grab tests
     continue to pass.
   - `.ai/dispatch.tasks.md`, `Status.md`, `HANDOFF.md`, `plans/BASELINE.md`,
     and `change.md` mark task 135 complete when implemented, and no task 136 is
     appended by the implementation unless a later audit dispatch explicitly
     authorizes it.

   **Verification:**
   - `cargo test -p rge-editor-shell --lib viewport_drag_cursor_grab`
   - `cargo test -p rge-editor-shell --lib viewport_focus_loss`
   - `cargo test -p rge-editor-shell --lib viewport_right_button_orbit`
   - `cargo test -p rge-editor-shell --lib viewport_middle_button_pan`
   - `cargo check -p rge-editor-shell --lib`
   - `cargo +nightly fmt --all -- --check`
   - `rg -n "^134\.|^135\.|^136\." .ai/dispatch.tasks.md`
   - `git diff --name-only`
   - `git diff --check`

   **Halt conditions:**
   - The focus-loss behavior cannot be implemented inside the two MAY-edit
     lifecycle files without touching broader event routing, host UI, window
     ownership, or platform abstraction code.
   - The implementation would require changing camera math, viewport hit
     testing, wheel zoom, left-click picking, frame-all, menu camera commands,
     command routing, shortcuts, plugin runtime/discovery/loading, OS clipboard,
     CAD/projection/CommandBus, undo/dirty, save/load, Cargo metadata,
     workflows, automation, schemas, ADRs, architecture-lint config, or packet
     templates.
   - More than one follow-up is needed to make focus-loss drag cancellation
     coherent.

136. **[DONE 2026-06-14 via ISSUE-390] Post-focus-loss camera/navigation Phase 9 next-task source audit.**
   Run a docs/source-read-only audit after ISSUE-389 / task 135. Use current
   local source reads plus the dispatcher-provided GitHub-state snapshot from
   the auto-created issue body for queue/already-filed-task evidence; do not
   call `gh`, browse the network, or use live GitHub state from inside the
   executor sandbox. Compare the remaining editor-usability candidate classes
   after viewport wheel zoom, right-button orbit, middle-button pan,
   left-double-click frame-all, viewport drag cursor grab/release, and
   focus-loss drag cancellation all exist:

   - keybinding/remap/preferences/fatal-policy work after tasks 123, 125, 127,
     129, and 131;
   - host-shell command routing through `MenuCommandHandoff` /
     `EditorShell::route_menu_command`;
   - real plugin command execution after the injected extension-command seam;
   - OS/typed clipboard behavior beyond the current shell-local clipboard;
   - CAD/editor mutation through `CommandBus`, projection, undo/dirty, and
     save/load authority;
   - camera/navigation follow-up after wheel zoom, orbit, pan, frame-all,
     cursor grab/release, and focus-loss drag cancellation.

   Append exactly one bounded implementation follow-up as task 137, or record
   source-grounded `NEEDS_HUMAN` if every remaining candidate crosses a policy
   or architecture boundary that cannot be safely delegated.

   **Context snapshot:**
   - Task 135 shipped as ISSUE-389 / commit `82b2e95`: `editor-shell` now
     handles `WindowEvent::Focused(false)` by cancelling active viewport
     right-button orbit and middle-button pan drags, then requesting
     `CursorGrabMode::None` through the existing viewport drag cursor-grab
     helper only when at least one viewport drag was active before focus loss.
     `WindowEvent::Focused(true)` preserves existing drag and grab state.
   - Focused lifecycle tests cover active orbit cancellation, active pan
     cancellation, combined active drags with one release after both states are
     idle, no-active no-release behavior, and focus-gain preservation. Camera
     math, viewport hit testing, wheel zoom, face picking, left-double-click
     frame-all, command routing, shortcuts, plugin runtime/discovery/loading,
     clipboard, CAD/CommandBus, undo/dirty, save/load, Cargo metadata,
     workflows, automation, schemas, and non-`editor-shell` subsystems were
     unchanged by task 135.
   - Re-arm check before authoring task 136: `origin/main` and local `main`
     were synced at `82b2e95`; `gh issue list --repo RustCADs/RGE --state open
     --label ai-dispatch --json number,title,labels,url,state` returned `[]`;
     `gh issue list --repo RustCADs/RGE --state open --label
     ai-dispatch-failed --json number,title,labels,url,state` returned `[]`;
     `.ai/handoff-claims` had no live issue claim directories; and
     `rg -n "^134\.|^135\.|^136\.|^137\." .ai/dispatch.tasks.md` showed task
     134 DONE, task 135 DONE, and no task 136 or 137.
   - ISSUE-389's first attempt stalled during Codex planning and was archived
     as `A:\rcad\dispatch-worktrees\ISSUE-389.attempt1`; the queue's no-log-
     growth guard killed the stalled process tree, labelled the issue for
     retry, and the retry completed, verified, control-passed, published, and
     closed. That is evidence the stale/stalled-run guard is active. This
     re-arm addresses the different idle condition: the task brief was
     exhausted.
   - The queue has stale-claim protection from `fe6dbb4` / the dispatch
     stale-claim hardening update: `Invoke-AiDispatchQueue.ps1` sweeps
     queue-owned ADR-121 claims at startup and releases claims whose
     `Invoke-AiDispatchQueue.ps1:<pid>` owner is dead, no longer a queue
     process, or recycled after the claim timestamp.
   - The auto-created issue body will include the dispatcher GitHub-state
     snapshot. The audit must use that embedded snapshot, or an exact local
     artifact/read path to it, for GitHub queue/already-filed-task evidence.
     Do not call `gh` or the network from inside the executor sandbox for those
     claims.

   **MAY edit:**
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - generated ISSUE-<n> handoff/audit/log artifacts for this dispatch only

   **MUST NOT edit:**
   - Rust source or tests
   - Cargo manifests or `Cargo.lock`
   - workflows
   - dispatch automation, guard, queue, scheduler, watcher, verification, or
     health/trend scripts
   - schemas, ADR files, architecture-lint rules/config, packet templates, or
     unrelated existing handoff/log artifacts
   - plugin runtime/discovery/loading code, command routing, shortcut
     execution, remapping/persistence/fatal policy, OS clipboard behavior,
     CAD/projection/CommandBus mutation, undo/dirty/save-load authority, or
     camera/navigation behavior

   **Done criteria:**
   - The audit records the pre-edit task-heading check for
     `^134\.|^135\.|^136\.|^137\.`.
   - Queue/already-filed-task claims cite only the dispatcher-provided snapshot
     embedded in the issue body or an exact local artifact path copied from it;
     no live `gh`/network query is run by the sandboxed executor.
   - Each candidate class above has positive source references and falsifying
     searches for negative claims where practical.
   - Exactly one bounded implementation task 137 is appended with explicit
     `MAY edit`, `MUST NOT edit`, `Done criteria`, `Verification`, and `Halt
     conditions`, or a source-grounded `NEEDS_HUMAN` record is written.
   - No implementation work for task 137 is done, and no task 138 is appended.

   **Verification:**
   - `rg -n "^134\.|^135\.|^136\.|^137\." .ai/dispatch.tasks.md` before edits
     and after edits
   - status-doc cross-check against `Status.md`, `HANDOFF.md`,
     `plans/BASELINE.md`, and `change.md`
   - candidate-class source greps recorded in the audit
   - `rg -n "^138\." .ai/dispatch.tasks.md` returns no matches
   - `git diff --name-only`
   - `git diff --check`
   - `.\new-handoff.ps1 -Finalize -PacketPath <EXEC_PACKET> -DryRun`

   **Halt conditions:**
   - The executor cannot cite the dispatcher-provided GitHub-state snapshot
     without live `gh`/network access.
   - The audit would require editing a MUST-NOT path or implementing task 137.
   - More than one implementation follow-up would be required to make the
     selected boundary coherent.
   - No bounded task 137 can be specified without crossing a policy or
     architecture boundary; record `NEEDS_HUMAN` instead of forcing a task.

   **Audit result (ISSUE-390):**
   - Dispatcher GitHub-state evidence came only from the embedded ISSUE-390 TASK
     packet snapshot. The executor did not call `gh`, browse, or use network
     state for queue/already-filed-task claims. Snapshot verification:
     `rg -n '^(Generated by `Invoke-AiDispatchAuto\.ps1`|- Open `ai-dispatch` issues before ISSUE-390|- Open failed autonomous issues|  - `#389 \[CLOSED\] Release viewport drag cursor grab)' ai_handoffs/ISSUE-390_TASK_2026-06-14_10-22-48+0300.md`
     returned line 46 for the `Invoke-AiDispatchAuto.ps1` generation marker,
     line 50 for no open `ai-dispatch` issue before ISSUE-390, line 51 for no
     open failed autonomous issue, and line 87 for closed `#389 [CLOSED]
     Release viewport drag cursor grab on focus loss`.
   - Required pre-edit heading check:
     `rg -n "^134\.|^135\.|^136\.|^137\." .ai/dispatch.tasks.md`
     returned `12384:134. **[DONE 2026-06-14 via ISSUE-388] Post-viewport-cursor-grab Phase 9 next-task source audit.**`,
     `12579:135. **[DONE 2026-06-14 via ISSUE-389] Release viewport drag cursor grab on window focus loss.**`,
     and `12662:136. **Post-focus-loss camera/navigation Phase 9 next-task source audit.**`;
     there was no `^137.` match before editing.
   - Status-doc cross-check:
     `rg -n "ISSUE-389|task 135|task 136|task 137|focus-loss|focus loss|post-ISSUE-389|post-focus-loss|viewport drag cancellation|^137\." .ai/dispatch.tasks.md Status.md HANDOFF.md plans/BASELINE.md change.md`
     matched `Status.md:3`, `HANDOFF.md:3`, `plans/BASELINE.md:5-15`, and
     `change.md:1913`, all recording ISSUE-389/task 135 complete and task 136
     as the re-armed audit that must append one task 137 or record
     `NEEDS_HUMAN`.
   - Keybinding/remap/preferences/fatal-policy comparison:
     `rg -n "enabled_command_for_shortcut|command_for_shortcut|AcceleratorTable|ProjectedShortcutConflict|shortcut_help_rows|shortcut_conflict_rows|conflict_peer_entry_ids|annotated_main_menu_items|remap|preferences|persist|fatal|conflicted" crates/editor-ui/src/menus crates/editor-egui-host/src`
     found the current diagnostic/execution split: `ResolveResult` preserves
     display lookups through `command_for_shortcut` while
     `enabled_command_for_shortcut` suppresses disabled and conflicted
     accelerators (`crates/editor-ui/src/menus/registry.rs:248-321`);
     Shortcut Conflicts rows, Keyboard Shortcuts rows, command-palette rows,
     and main-menu rows consume already-projected conflict data
     (`crates/editor-egui-host/src/shortcut_conflicts.rs:11`,
     `crates/editor-egui-host/src/shortcut_help.rs:159-172`,
     `crates/editor-egui-host/src/menu.rs:177-231`). Falsifying search:
     `rg -n "ShortcutRemap|Remap|remap|Preferences|Preference|settings|shortcut.*persist|persist.*shortcut|shortcut.*override|override.*shortcut|fatal.*shortcut|ShortcutConflict.*fatal|conflict.*fatal" crates/editor-ui/src/menus crates/editor-egui-host/src crates/editor-shell/src`
     returned only the existing note that the host decides whether a conflict
     is fatal plus unrelated style/face-ID/remap wording; no bounded shortcut
     remap/preferences/fatal-policy implementation slice was selected.
   - Host-shell command-routing comparison:
     `rg -n "MenuCommandHandoff|drain_and_route_menu_commands|route_menu_command|command_palette_window|selected_command_palette_entry|enabled_command_for_shortcut|WindowEvent::KeyboardInput|menu_command_handoff|Command::" crates/editor-egui-host/src crates/editor-shell/src crates/editor-ui/src/menus`
     found the bounded FIFO (`crates/editor-egui-host/src/handoff.rs:73-128`),
     top-of-frame drain (`crates/editor-shell/src/render_path.rs:366`), and the
     shared menu/accelerator sink (`crates/editor-shell/src/render_path.rs:415`).
     The keyboard parity surface remains canonical-menu based
     (`crates/editor-shell/src/lifecycle/accelerator.rs:16-24`), so replacing
     route ownership or FIFO semantics would cross a broader routing boundary.
   - Real plugin command execution comparison:
     `rg -n "ExtensionCommandHandler|extension_command|plugin command|PluginCommand|Command::Plugin|Command::Custom|PluginHost|PluginContext|plugin_discovery|plugin-discovery|runtime_wasmtime|runtime-wasmtime" crates/editor-shell/src crates/editor-ui/src crates/plugin-discovery/src crates/runtime-wasmtime/src crates/runtime-wasmtime-engine/src editor/rge-editor/src`
     found `Command::Custom` / `Command::Plugin` capture and the injected
     `ExtensionCommandHandler` seam (`crates/editor-shell/src/lifecycle/mod.rs:763-776`,
     `crates/editor-shell/src/lifecycle/extension_command.rs:58-163`) plus
     runtime/discovery crates (`crates/plugin-discovery/src/lib.rs:1`,
     `crates/runtime-wasmtime/src/lib.rs:1-18`,
     `crates/runtime-wasmtime-engine/src/engine.rs:13`). The seam explicitly
     states that no plugin runtime, discovery, loading, or registry execution
     is implied, so real plugin execution remains broader than a one-file
     usability follow-up.
   - OS/typed clipboard comparison:
     `rg -n "clipboard|Cut|Copy|Paste|Duplicate|Delete|legacy|entity_clipboard|clone_entity_blobs|paste_copied_entities|copy_selected_entities|cut_selected_entities|OS clipboard|system clipboard|arboard|copypasta|Clipboard" crates/editor-shell/src crates/editor-actions/src crates/editor-egui-host/src crates/editor-ui/src Cargo.toml crates`
     and the narrower `rg -n "entity_clipboard|copy_selected_entities|cut_selected_entities|paste_copied_entities|clone_entity_blobs|Command::Cut|Command::Copy|Command::Paste|has_clipboard_entities" crates/editor-shell/src crates/editor-actions/src crates/editor-egui-host/src crates/editor-ui/src`
     found a shell-local legacy-blob clipboard only
     (`crates/editor-shell/src/lifecycle/mod.rs:780-783`,
     `crates/editor-shell/src/lifecycle/mod.rs:1793-1846`) routed by
     `Command::Cut` / `Copy` / `Paste` (`crates/editor-shell/src/render_path.rs:444-451`).
     Falsifying search `rg -n "arboard|copypasta|system clipboard|OS clipboard|Clipboard" Cargo.toml crates/editor-shell/src crates/editor-actions/src crates/editor-egui-host/src crates/editor-ui/src`
     returned OS clipboard comments but no dependency or production OS clipboard
     integration; typed/OS clipboard behavior remains deferred.
   - CAD/editor mutation through `CommandBus` comparison:
     `rg -n "CommandBus|Action|Cad|cad_|projection|dirty|undo|save|load|CadCheckpoint|CadGraph|CadProjection|fn submit\(|fn mark_saved\(|fn is_dirty\(|save_source|render_mesh_for" crates/editor-shell/src crates/editor-actions/src crates/cad-core/src crates/cad-projection/src`
     found authoritative bus submission/undo/redo/dirty state in
     `rge-editor-actions` (`crates/editor-actions/src/bus.rs:86-284`), shell
     wrappers around bus actions (`crates/editor-shell/src/lifecycle/commands.rs:274-313`),
     save/load authority tied to `SaveSource` and `mark_saved_command`
     (`crates/editor-shell/src/lifecycle/save_request.rs:205-393`), and CAD
     projection/render gates (`crates/cad-projection/src/lib.rs:38-49`,
     `crates/cad-projection/src/render_adapter.rs:34-90`). The Edit
     Cut/Copy/Paste/Delete path explicitly avoids CAD graph/projection,
     render meshes, undo, and dirty state (`crates/editor-shell/src/lifecycle/mod.rs:1705-1846`).
     A CAD/CommandBus mutation follow-up would need product/authority choices
     beyond this audit.
   - Camera/navigation comparison:
     `rg -n "MouseWheel|right|middle|double|frame|reset_camera|WindowEvent::Focused|Focused\(|focus loss|CursorGrabMode|set_cursor_grab|set_viewport_drag_cursor_grab|release_viewport_drag_cursor_if_idle|viewport_drag_cursor_grab|camera|viewport_navigation|current_scene_bounds|frame_selected|ViewportLeftDoubleClick|ViewportOrbitDrag|ViewportPanDrag|zoom_camera_for_viewport_mouse_wheel" crates/editor-shell/src`
     found current reset/frame, wheel zoom, left-double-click frame-all,
     right-button orbit, middle-button pan, cursor grab/release, and focus-loss
     cancellation surfaces (`crates/editor-shell/src/lifecycle/mod.rs:2068-2284`,
     `crates/editor-shell/src/lifecycle/mod.rs:2786-2850`,
     `crates/editor-shell/src/lifecycle/tests.rs:1488-1580`). Falsifying
     search `rg -n "cursor_pos = None|self\.cursor_pos = None|CursorLeft|CursorExited|CursorEntered|WindowEvent::Cursor" crates/editor-shell/src`
     found no `WindowEvent::CursorLeft` / cursor-exit handling in
     `lifecycle/mod.rs`; only `CursorMoved` updates `cursor_pos`. The selected
     smallest bounded follow-up is therefore task 137: cancel active viewport
     drags and release the drag cursor grab when the cursor leaves the window,
     while also clearing stale cursor position and preserving existing camera
     math/routes.
   - Selected policy boundary: defer broad shortcut remapping/preferences/fatal
     policy, route ownership, real plugin runtime/discovery/loading, OS/typed
     clipboard, and CAD/CommandBus authority. Delegate only the smallest
     source-safe `editor-shell` lifecycle cursor-leave polish below.
   - No implementation work for task 137 was done, and no task 138 was
     appended by this audit.

137. **[DONE 2026-06-14 via ISSUE-391] Cancel viewport drag state when the cursor leaves the window.**
   Implement one bounded `editor-shell` camera/navigation lifecycle polish:
   when the window reports a cursor-leave event while a viewport right-button
   orbit drag and/or middle-button pan drag is active, cancel the active
   viewport drag state, clear the stale cursor position, reset the viewport
   left-double-click tracker, and release the viewport drag cursor grab. This
   is the cursor-leave sibling of task 135's focus-loss cancellation. Preserve
   current camera math, viewport hit testing, wheel zoom, left-click face
   picking, left-double-click frame-all semantics except for clearing pending
   double-click state on cursor leave, View menu camera commands, command
   routing, shortcuts, plugin runtime/discovery/loading, OS clipboard,
   CAD/projection/CommandBus mutation, undo/dirty, and save/load behavior.

   **MAY edit:**
   - `crates/editor-shell/src/lifecycle/mod.rs`
   - `crates/editor-shell/src/lifecycle/tests.rs`
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - generated ISSUE-<n> handoff/audit/log artifacts for this dispatch only

   **MUST NOT edit:**
   - Rust files outside `crates/editor-shell/src/lifecycle/mod.rs` and
     `crates/editor-shell/src/lifecycle/tests.rs`
   - Cargo manifests or `Cargo.lock`
   - workflows
   - dispatch automation, guard, queue, scheduler, watcher, verification, or
     health/trend scripts
   - schemas, ADR files, architecture-lint rules/config, packet templates, or
     unrelated existing handoff/log artifacts
   - command routing, `MenuCommandHandoff`, command-palette activation,
     accelerator execution, shortcut remapping/preferences/fatal policy, plugin
     runtime/discovery/loading, OS clipboard behavior, CAD/projection/CommandBus
     mutation, undo/dirty/save-load authority, camera reset/frame/zoom/orbit/pan
     math, viewport hit testing, face picking, or View menu camera behavior

   **Done criteria:**
   - The existing `WindowEvent::CursorLeft` branch, or the current winit 0.30
     equivalent if the event shape differs, clears `self.cursor_pos`.
   - Cursor leave resets the viewport left-double-click tracker so a later
     left press after re-entry cannot pair with a stale pre-leave press.
   - If at least one viewport orbit/pan drag was active before cursor leave,
     both drag states are stopped and `CursorGrabMode::None` is requested
     through the existing viewport drag cursor-grab helper after both states are
     idle.
   - If no viewport drag was active, cursor leave does not emit a new
     cursor-grab release request.
   - `WindowEvent::CursorMoved` remains the only path that installs a fresh
     cursor position; focus-loss behavior from task 135 remains unchanged.
   - Focused tests cover active orbit cancellation, active pan cancellation,
     combined active drag release-once behavior, no-active no-release behavior,
     cursor-position clearing, and stale double-click reset on cursor leave.
   - `.ai/dispatch.tasks.md`, `Status.md`, `HANDOFF.md`, `plans/BASELINE.md`,
     and `change.md` mark task 137 complete when implemented, and no task 138 is
     appended by the implementation unless a later audit dispatch explicitly
     authorizes it.

   **Verification:**
   - `cargo test -p rge-editor-shell --lib viewport_cursor_left`
   - `cargo test -p rge-editor-shell --lib viewport_focus_loss`
   - `cargo test -p rge-editor-shell --lib viewport_drag_cursor_grab`
   - `cargo test -p rge-editor-shell --lib viewport_right_button_orbit`
   - `cargo test -p rge-editor-shell --lib viewport_middle_button_pan`
   - `cargo check -p rge-editor-shell --lib`
   - `cargo +nightly fmt --all -- --check`
   - `rg -n "^136\.|^137\.|^138\." .ai/dispatch.tasks.md`
   - `git diff --name-only`
   - `git diff --check`

   **Halt conditions:**
   - The current winit event surface has no cursor-leave/cursor-exit event that
     can be handled inside `crates/editor-shell/src/lifecycle/mod.rs`.
   - The implementation would require changing broader window ownership,
     host-egui viewport hit testing, command routing, shortcut execution,
     plugin runtime/discovery/loading, OS clipboard, CAD/projection/CommandBus,
     undo/dirty, save/load, Cargo metadata, workflows, automation, schemas,
     ADRs, architecture-lint config, or packet templates.
   - The implementation would require changing camera reset/frame/zoom/orbit/pan
     math, face picking, wheel zoom, left-double-click frame-all behavior beyond
     clearing stale pending state on cursor leave, or View menu camera behavior.
   - More than one follow-up is needed to make cursor-leave drag cancellation
     coherent.

138. **[DONE 2026-06-14 via ISSUE-392] Post-cursor-left camera/navigation Phase 9 next-task source audit.**
   Run a docs/source-read-only audit after ISSUE-391 / task 137. Use current
   local source reads plus the dispatcher-provided GitHub-state snapshot from
   the auto-created issue body for queue/already-filed-task evidence; do not
   call `gh`, browse the network, or use live GitHub state from inside the
   executor sandbox. Compare the remaining editor-usability candidate classes
   after viewport wheel zoom, right-button orbit, middle-button pan,
   left-double-click frame-all, viewport drag cursor grab/release, focus-loss
   drag cancellation, and cursor-left drag cancellation all exist:

   - keybinding/remap/preferences/fatal-policy work after tasks 123, 125, 127,
     129, and 131;
   - host-shell command routing through `MenuCommandHandoff` /
     `EditorShell::route_menu_command`;
   - real plugin command execution after the injected extension-command seam;
   - OS/typed clipboard behavior beyond the current shell-local clipboard;
   - CAD/editor mutation through `CommandBus`, projection, undo/dirty, and
     save/load authority;
   - camera/navigation follow-up after wheel zoom, orbit, pan, frame-all,
     cursor grab/release, focus-loss drag cancellation, and cursor-left drag
     cancellation.

   Append exactly one bounded implementation follow-up as task 139, or record
   source-grounded `NEEDS_HUMAN` if every remaining candidate crosses a policy
   or architecture boundary that cannot be safely delegated.

   **Context snapshot:**
   - Task 137 shipped as ISSUE-391 / commit `8fe95bc`: `editor-shell` now
     handles `WindowEvent::CursorLeft` by clearing stale `cursor_pos`, resetting
     the viewport left-double-click tracker, cancelling active viewport
     right-button orbit and middle-button pan drags, and requesting
     `CursorGrabMode::None` only when at least one viewport drag was active
     before cursor leave.
   - The focus-loss path from task 135 now shares the same active-drag
     cancellation helper, preserving focus-gain no-op, release-once, and
     no-active no-release behavior. Focused lifecycle tests cover cursor-left
     active orbit, active pan, combined drags, no-active no-release,
     cursor-position clearing, and stale double-click reset. Camera math,
     viewport hit testing, wheel zoom, face picking, frame-all behavior beyond
     stale pending reset on cursor leave, command routing, shortcuts, plugin
     runtime/discovery/loading, clipboard, CAD/CommandBus, undo/dirty,
     save/load, Cargo metadata, workflows, automation, schemas, and
     non-`editor-shell` subsystems were unchanged by task 137.
   - Re-arm check before authoring task 138: `origin/main` and local `main`
     were synced at `8fe95bc`; `gh issue list --repo RustCADs/RGE --state open
     --label ai-dispatch --json number,title,state,labels,url` returned `[]`;
     `gh issue list --repo RustCADs/RGE --state open --label
     ai-dispatch-failed --json number,title,state,labels,url` returned `[]`;
     `.ai/handoff-claims` had no live issue claim directories; and
     `rg -n "^136\.|^137\.|^138\.|^139\." .ai/dispatch.tasks.md` showed task
     136 DONE, task 137 DONE, and no task 138 or 139.
   - ISSUE-391 passed the full canonical verification gate and Codex control:
     the queue reported `Verification round 0: pass (exit 0)`, `Codex control
     round 0: pass`, published `8fe95bc` to `origin/main`, closed #391, and
     removed the isolated worktree.
   - The queue has stale-claim protection from `fe6dbb4` / the dispatch
     stale-claim hardening update: `Invoke-AiDispatchQueue.ps1` sweeps
     queue-owned ADR-121 claims at startup and releases claims whose
     `Invoke-AiDispatchQueue.ps1:<pid>` owner is dead, no longer a queue
     process, or recycled after the claim timestamp. This re-arm addresses the
     separate idle condition: the task brief was exhausted.
   - The auto-created issue body will include the dispatcher GitHub-state
     snapshot. The audit must use that embedded snapshot, or an exact local
     artifact/read path to it, for GitHub queue/already-filed-task evidence.
     Do not call `gh` or the network from inside the executor sandbox for those
     claims.

   **MAY edit:**
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - generated ISSUE-<n> handoff/audit/log artifacts for this dispatch only

   **MUST NOT edit:**
   - Rust source or tests
   - Cargo manifests or `Cargo.lock`
   - workflows
   - dispatch automation, guard, queue, scheduler, watcher, verification, or
     health/trend scripts
   - schemas, ADR files, architecture-lint rules/config, packet templates, or
     unrelated existing handoff/log artifacts
   - plugin runtime/discovery/loading code, command routing, shortcut
     execution, remapping/persistence/fatal policy, OS clipboard behavior,
     CAD/projection/CommandBus mutation, undo/dirty/save-load authority, or
     camera/navigation behavior

   **Done criteria:**
   - The audit records the pre-edit task-heading check for
     `^136\.|^137\.|^138\.|^139\.`.
   - Queue/already-filed-task claims cite only the dispatcher-provided snapshot
     embedded in the issue body or an exact local artifact path copied from it;
     no live `gh`/network query is run by the sandboxed executor.
   - Each candidate class above has positive source references and falsifying
     searches for negative claims where practical.
   - Exactly one bounded implementation task 139 is appended with explicit
     `MAY edit`, `MUST NOT edit`, `Done criteria`, `Verification`, and `Halt
     conditions`, or a source-grounded `NEEDS_HUMAN` record is written.
   - No implementation work for task 139 is done, and no task 140 is appended.

   **Verification:**
   - `rg -n "^136\.|^137\.|^138\.|^139\." .ai/dispatch.tasks.md` before edits
     and after edits
   - status-doc cross-check against `Status.md`, `HANDOFF.md`,
     `plans/BASELINE.md`, and `change.md`
   - candidate-class source greps recorded in the audit
   - `rg -n "^140\." .ai/dispatch.tasks.md` returns no matches
   - `git diff --name-only`
   - `git diff --check`
   - `.\new-handoff.ps1 -Finalize -PacketPath <EXEC_PACKET> -DryRun`

   **Halt conditions:**
   - The executor cannot cite the dispatcher-provided GitHub-state snapshot
     without live `gh`/network access.
   - The audit would require editing a MUST-NOT path or implementing task 139.
   - More than one implementation follow-up would be required to make the
     selected boundary coherent.
   - No bounded task 139 can be specified without crossing a policy or
     architecture boundary; record `NEEDS_HUMAN` instead of forcing a task.

   **Audit result (ISSUE-392):**
   - Executor pre-edit task-heading check was run before modifying this file:
     ```text
     rg -n "^136\.|^137\.|^138\.|^139\." .ai/dispatch.tasks.md
     12662:136. **[DONE 2026-06-14 via ISSUE-390] Post-focus-loss camera/navigation Phase 9 next-task source audit.**
     12881:137. **[DONE 2026-06-14 via ISSUE-391] Cancel viewport drag state when the cursor leaves the window.**
     12966:138. **Post-cursor-left camera/navigation Phase 9 next-task source audit.**
     ```
     No `^139.` heading existed before edits.
   - GitHub queue/already-filed-task evidence came only from the embedded
     dispatcher snapshot in
     `ai_handoffs/ISSUE-392_TASK_2026-06-14_12-07-01+0300.md`. The local
     falsifying read
     `rg -n '^(Generated by `Invoke-AiDispatchAuto\.ps1`|- Open `ai-dispatch` issues before ISSUE-392|- Open failed autonomous issues|  - `#391 \[CLOSED\] Cancel viewport drags)' ai_handoffs/ISSUE-392_TASK_2026-06-14_12-07-01+0300.md`
     returned lines 46, 49, 50, and 88: snapshot generated
     `2026-06-14T12:06:30.1448686+03:00`, no open `ai-dispatch` issue before
     ISSUE-392, no open failed autonomous issue, and already-filed autonomous
     issues through closed #391. No `gh`, browser, or network command was run.
   - Status-doc cross-check
     `rg -n "ISSUE-391|task 137|task 138|task 139|cursor-left|cursor left|CursorLeft|post-ISSUE-391|post-cursor-left|viewport drag cancellation|^139\." .ai/dispatch.tasks.md Status.md HANDOFF.md plans/BASELINE.md change.md`
     confirmed `Status.md:3`, `HANDOFF.md:3`, `plans/BASELINE.md:5-15`, and
     `change.md:1919` recorded ISSUE-391 / task 137 complete and task 138 as
     the active re-arm that must append one task 139 or record `NEEDS_HUMAN`.
   - Keybinding/remap/preferences/fatal-policy comparison: registry execution
     already suppresses conflicted accelerators through
     `ResolveResult::enabled_command_for_shortcut`
     (`crates/editor-ui/src/menus/registry.rs:248-321`); host diagnostics
     already project conflict rows, shortcut-help peer ids, command-palette
     annotations, and main-menu annotations
     (`crates/editor-egui-host/src/shortcut_conflicts.rs:11-77`,
     `crates/editor-egui-host/src/shortcut_help.rs:159-190`,
     `crates/editor-egui-host/src/menu.rs:208-246`). The falsifying search
     `rg -n "shortcut.*remap|remap.*shortcut|keymap|shortcut.*preference|preferences" crates/editor-ui/src/menus crates/editor-egui-host/src crates/editor-shell/src/lifecycle`
     returned `NO_MATCHES`, so shortcut remapping/preferences remain a broader
     policy/substrate task rather than the next small diagnostic slice.
   - Host-shell command routing comparison: `MenuCommandHandoff` remains a
     host-to-shell FIFO (`crates/editor-egui-host/src/handoff.rs:93-137`),
     `EditorShell::drain_and_route_menu_commands` drains it at the top of
     render (`crates/editor-shell/src/render_path.rs:357-371`), and
     `EditorShell::route_menu_command` is still the one-way command sink for
     menu clicks and canonical accelerators
     (`crates/editor-shell/src/render_path.rs:389-475`;
     `crates/editor-shell/src/lifecycle/mod.rs:2874-2929`). Replacing route
     ownership or FIFO behavior is broader than one safe follow-up.
   - Real plugin command execution comparison: extension commands are captured
     and optionally delivered to an injected handler, but the seam explicitly
     stops before plugin discovery, loading, runtime execution, capabilities,
     async dispatch, sandboxing, or registry execution
     (`crates/editor-shell/src/lifecycle/extension_command.rs:3-5`,
     `:100-101`). `crates/plugin-discovery/src/lib.rs:1` is still a stub
     crate, while runtime-wasmtime crates expose lower-level cap/runtime
     surfaces. A real plugin executor remains a multi-owner runtime/discovery
     task and was not selected.
   - OS/typed clipboard comparison: Edit Cut/Copy/Paste are shell-local
     legacy-blob operations. The source states copy/cut do not touch the OS
     clipboard, typed kernel components, CAD graph/projection data, render
     meshes, the command bus, or dirty/undo state
     (`crates/editor-shell/src/lifecycle/mod.rs:1788-1840`), and route tests
     pin that Copy/Paste are not OS clipboard or authoritative CAD/projection
     clones (`crates/editor-shell/src/lifecycle/tests.rs:4744-4810`). The
     falsifying search `rg -n "arboard|copypasta" Cargo.toml crates` returned
     `NO_MATCHES`; OS/typed clipboard behavior remains too broad for this
     audit's selected task.
   - CAD/editor mutation comparison: `CommandBus` owns submit/undo/redo,
     mark-saved, and dirty state
     (`crates/editor-actions/src/bus.rs:86-285`;
     `crates/editor-shell/src/lifecycle/commands.rs:262-313`), save/load paths
     route through `save_source` and mark the bus saved
     (`crates/editor-shell/src/lifecycle/save_request.rs:178-266`), and
     `CadProjection::tick` owns dirty projection synchronization
     (`crates/cad-projection/src/lib.rs:486-545`). The next task must not
     mutate CAD/projection/CommandBus, undo/dirty, or save/load authority.
   - Camera/navigation comparison: wheel zoom, right-button orbit,
     middle-button pan, cursor grab/release, focus-loss cancellation, and
     cursor-left cancellation are all present with focused tests
     (`crates/editor-shell/src/lifecycle/tests.rs:1015-1707`).
     `handle_viewport_left_press` still treats a qualifying viewport
     left-double-click as scene-wide frame-all by calling `self.reset_camera()`
     (`crates/editor-shell/src/lifecycle/mod.rs:2129-2144`), and
     `reset_camera` frames `current_scene_bounds()` from either prebuilt render
     meshes or the single CAD projection entity
     (`crates/editor-shell/src/lifecycle/mod.rs:2068-2096`). Selection is
     already coordination state backed by `rge_kernel_ecs::EntityId`
     (`crates/editor-state/src/selection.rs:52-129`) and accessible through
     `EditorShell::coord()` / `coord_mut()`
     (`crates/editor-shell/src/lifecycle/mod.rs:1717-1723`). The falsifying
     search
     `rg -n "Frame Selected|frame selected|frame_selected|selected bounds|selection bounds|selected.*AABB|AABB.*selected|selected.*camera|camera.*selected" crates/editor-shell/src crates/editor-ui/src/menus crates/editor-egui-host/src`
     returned `NO_MATCHES`. The selected bounded follow-up is therefore
     selection-aware viewport left-double-click framing using only existing
     selection plus render-mesh/AABB read APIs.
   - Task 138 did not implement task 139, did not edit Rust/Cargo/workflows or
     automation, and did not append task 140.

139. **[DONE 2026-06-14 via ISSUE-393] Frame selected CAD entities on viewport left double-click.**
   Implement one bounded `editor-shell` camera/navigation polish: when a
   qualifying viewport left double-click occurs, first attempt to frame the
   currently selected CAD projection entity or selected CAD projection entity
   union using existing read-only render-mesh/AABB helpers; if no selected
   entity can be resolved to renderable bounds, preserve the current scene-wide
   frame-all fallback. Do not add a new menu command, do not change
   `View -> Reset Camera` / `Frame Scene`, and do not change CAD/projection
   ownership or mutation semantics.

   **MAY edit:**
   - `crates/editor-shell/src/lifecycle/mod.rs`
   - `crates/editor-shell/src/lifecycle/tests.rs`
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - generated ISSUE-<n> handoff/audit/log artifacts for this dispatch only

   **MUST NOT edit:**
   - Rust files outside `crates/editor-shell/src/lifecycle/mod.rs` and
     `crates/editor-shell/src/lifecycle/tests.rs`
   - Cargo manifests or `Cargo.lock`
   - workflows
   - dispatch automation, guard, queue, scheduler, watcher, verification, or
     health/trend scripts
   - schemas, ADR files, architecture-lint rules/config, packet templates, or
     unrelated existing handoff/log artifacts
   - `crates/editor-ui/**`, `crates/editor-egui-host/**`,
     `crates/editor-actions/**`, `crates/cad-core/**`,
     `crates/cad-projection/**`, plugin runtime/discovery/loading code,
     command routing, `MenuCommandHandoff`, command-palette activation,
     accelerator execution, shortcut remapping/preferences/fatal policy,
     OS clipboard behavior, CAD/projection/CommandBus mutation,
     undo/dirty/save-load authority, View menu camera behavior, face picking,
     wheel zoom, right-button orbit math, middle-button pan math, or viewport
     hit testing

   **Done criteria:**
   - A qualifying viewport left double-click still uses the existing
     `ViewportLeftDoubleClick` timing/distance detector and still fires the
     existing left-click face-pick path exactly as before.
   - If one or more selected entities can be resolved to renderable CAD
     projection meshes through existing `CadProjection::render_mesh_for` and
     current shell state, the camera frames the union of those selected bounds
     through the existing `compute_aabb_union` /
     `isometric_camera_for_bounds` framing math.
   - If there is no selection, no selected entity can be resolved to renderable
     bounds, the selected bounds are non-finite, or the shell is in the prebuilt
     render-mesh path with no CAD entity mapping, left-double-click preserves the
     current scene-wide `reset_camera()` fallback.
   - The implementation is read-only with respect to CAD/projection state,
     `CommandBus`, undo/dirty, save/load, and shell-local clipboard state.
   - Focused lifecycle tests cover selected CAD-entity framing, selected
     multiple-CAD-entity union framing when the existing source permits it,
     fallback to scene-wide frame-all for no selection, fallback for
     unresolved/non-renderable selections, and unchanged face-pick behavior on
     the double-click path.
   - `.ai/dispatch.tasks.md`, `Status.md`, `HANDOFF.md`,
     `plans/BASELINE.md`, and `change.md` mark task 139 complete when
     implemented, and no task 140 is appended by the implementation unless a
     later audit dispatch explicitly authorizes it.

   **Verification:**
   - `cargo test -p rge-editor-shell --lib viewport_left_double_click`
   - `cargo test -p rge-editor-shell --lib reset_camera`
   - `cargo test -p rge-editor-shell --lib viewport_cursor_left`
   - `cargo check -p rge-editor-shell --lib`
   - `cargo +nightly fmt --all -- --check`
   - `rg -n "^138\.|^139\.|^140\." .ai/dispatch.tasks.md`
   - `rg -n "FrameSelected|Frame Selected|frame_selected|Command::FrameSelected" crates/editor-shell/src crates/editor-ui/src crates/editor-egui-host/src` with expected no matches
   - `git diff --name-only`
   - `git diff --check`

   **Halt conditions:**
   - The implementation cannot identify selected CAD projection entity bounds
     using only `EditorShell` state plus existing read-only render-mesh/AABB
     helpers.
   - Supporting selected-entity framing would require editing CAD/projection
     crates, `CommandBus`, undo/dirty, save/load, command routing,
     `MenuCommandHandoff`, `editor-ui`, `editor-egui-host`, plugin
     runtime/discovery/loading, OS clipboard code, Cargo metadata, workflows,
     automation, schemas, ADRs, architecture-lint config, or packet templates.
   - More than one implementation follow-up is required to make the behavior
     coherent.
   - The implementation would need a new View menu command, shortcut, command
     enum variant, route ownership change, face-pick policy change, wheel zoom
     change, orbit/pan math change, or viewport hit-test change.

140. **[DONE 2026-06-14 via ISSUE-394] Post-selected-CAD double-click Phase 9 next-task source audit.**
   Run a docs/source-read-only audit after ISSUE-393 / task 139. Use current
   local source reads plus the dispatcher-provided GitHub-state snapshot from
   the auto-created issue body for queue/already-filed-task evidence; do not
   call `gh`, browse the network, or use live GitHub state from inside the
   executor sandbox. Compare the remaining editor-usability candidate classes
   after viewport wheel zoom, right-button orbit, middle-button pan,
   left-double-click frame-all, selected-CAD left-double-click framing,
   viewport drag cursor grab/release, focus-loss drag cancellation, and
   cursor-left drag cancellation all exist:

   - keybinding/remap/preferences/fatal-policy work after tasks 123, 125, 127,
     129, and 131;
   - host-shell command routing through `MenuCommandHandoff` /
     `EditorShell::route_menu_command`;
   - real plugin command execution after the injected extension-command seam;
   - OS/typed clipboard behavior beyond the current shell-local clipboard;
   - CAD/editor mutation through `CommandBus`, projection, undo/dirty, and
     save/load authority;
   - camera/navigation follow-up after wheel zoom, orbit, pan, frame-all,
     selected-CAD framing, cursor grab/release, focus-loss cancellation, and
     cursor-left cancellation.

   Append exactly one bounded implementation follow-up as task 141, or record
   source-grounded `NEEDS_HUMAN` if every remaining candidate crosses a policy
   or architecture boundary that cannot be safely delegated.

   **Context snapshot:**
   - Task 139 shipped as ISSUE-393 / commit `3beb79b`: `editor-shell` keeps the
     existing viewport left-double-click detector and face-pick gate, then
     frames selected CAD projection bounds through existing read-only
     `CadProjection::render_mesh_for` plus `compute_aabb_union` /
     `isometric_camera_for_bounds` when selected entities resolve. Multiple
     selected CAD entities frame their selected-bounds union. No selection,
     unresolved selected entities, empty/non-finite selected bounds, or
     prebuilt render-only paths fall back to the existing scene-wide
     `reset_camera()` path.
   - `reset_camera()` itself remains scene-wide for View menu/Home routing.
     Task 139 did not change CAD/projection mutation, CommandBus/undo/dirty,
     save/load authority, command routing, shortcuts, plugin runtime/discovery,
     OS clipboard, wheel zoom, orbit/pan math, cursor-left/focus-loss
     cancellation, viewport hit testing, Cargo metadata, workflows, schemas, or
     automation scripts.
   - Re-arm check before authoring task 140: `origin/main` and local `main`
     were synced at `3beb79b`; `gh issue list --repo RustCADs/RGE --state open
     --label ai-dispatch --json number,title,state,labels,url` returned `[]`;
     `gh issue list --repo RustCADs/RGE --state open --label
     ai-dispatch-failed --json number,title,state,labels,url` returned `[]`;
     `.ai/handoff-claims` had no live issue claim directories; and
     `rg -n "^138\.|^139\.|^140\.|^141\." .ai/dispatch.tasks.md` showed task
     138 DONE, task 139 DONE, and no task 140 or 141.
   - ISSUE-393 passed canonical verification and Codex control: the committed
     queue record reports loop exit 0, control verdict `pass`, and publish to
     `origin/main`.
   - The scheduler remains armed and has since exited cleanly with no new issue
     because the task brief was exhausted. This re-arm addresses only that
     idle condition.
   - The auto-created issue body will include the dispatcher GitHub-state
     snapshot. The audit must use that embedded snapshot, or an exact local
     artifact/read path to it, for GitHub queue/already-filed-task evidence.
     Do not call `gh` or the network from inside the executor sandbox for those
     claims.

   **MAY edit:**
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - generated ISSUE-<n> handoff/audit/log artifacts for this dispatch only

   **MUST NOT edit:**
   - Rust source or tests
   - Cargo manifests or `Cargo.lock`
   - workflows
   - dispatch automation, guard, queue, scheduler, watcher, verification, or
     health/trend scripts
   - schemas, ADR files, architecture-lint rules/config, packet templates, or
     unrelated existing handoff/log artifacts
   - plugin runtime/discovery/loading code, command routing, shortcut
     execution, remapping/persistence/fatal policy, OS clipboard behavior,
     CAD/projection/CommandBus mutation, undo/dirty/save-load authority, or
     camera/navigation behavior

   **Done criteria:**
   - The audit records the pre-edit task-heading check for
     `^138\.|^139\.|^140\.|^141\.`.
   - Queue/already-filed-task claims cite only the dispatcher-provided snapshot
     embedded in the issue body or an exact local artifact path copied from it;
     no live `gh`/network query is run by the sandboxed executor.
   - Each candidate class above has positive source references and falsifying
     searches for negative claims where practical.
   - Exactly one bounded implementation task 141 is appended with explicit
     `MAY edit`, `MUST NOT edit`, `Done criteria`, `Verification`, and `Halt
     conditions`, or a source-grounded `NEEDS_HUMAN` record is written.
   - No implementation work for task 141 is done, and no task 142 is appended.

   **Verification:**
   - `rg -n "^138\.|^139\.|^140\.|^141\." .ai/dispatch.tasks.md` before edits
     and after edits
   - status-doc cross-check against `Status.md`, `HANDOFF.md`,
     `plans/BASELINE.md`, and `change.md`
   - candidate-class source greps recorded in the audit
   - `rg -n "^142\." .ai/dispatch.tasks.md` returns no matches
   - `git diff --name-only`
   - `git diff --check`
   - `.\new-handoff.ps1 -Finalize -PacketPath <EXEC_PACKET> -DryRun`

   **Halt conditions:**
   - The executor cannot cite the dispatcher-provided GitHub-state snapshot
     without live `gh`/network access.
   - The audit would require editing a MUST-NOT path or implementing task 141.
   - More than one implementation follow-up would be required to make the
     selected boundary coherent.
   - No bounded task 141 can be specified without crossing a policy or
     architecture boundary; record `NEEDS_HUMAN` instead of forcing a task.

   **Execution audit (ISSUE-394):**
   - Pre-edit task-heading check:
     `rg -n "^138\.|^139\.|^140\.|^141\.|^142\." .ai/dispatch.tasks.md`
     returned task 138 at line 12966, task 139 at line 13181, task 140 at
     line 13271, and no task 141 or task 142 heading.
   - Dispatcher snapshot evidence came only from
     `ai_handoffs/ISSUE-394_TASK_2026-06-14_22-14-21+0300.md`: the snapshot
     was generated by `Invoke-AiDispatchAuto.ps1` at
     `2026-06-14T22:13:56.8779920+03:00` before ISSUE-394 was created for
     `RustCADs/RGE`; it reported no open `ai-dispatch` issue, no open failed
     autonomous issue, and autonomous issues already filed through closed
     #393. No `gh`, browser, network, or GitHub API command was run by this
     audit.
   - Status-doc cross-check:
     `rg -n "ISSUE-393|task 139|task 140|task 141|selected-CAD|selected CAD|Post-selected-CAD" Status.md HANDOFF.md plans/BASELINE.md change.md .ai/dispatch.tasks.md`
     matched the post-ISSUE-393 re-arm in `Status.md:3`, `HANDOFF.md:3`,
     `plans/BASELINE.md:5-14`, `change.md:1925`, and this task body, confirming
     task 140 was the active audit that must append task 141 or record
     `NEEDS_HUMAN`.
   - Keybinding/remap/preferences/fatal-policy: conflict-aware shortcut
     execution and diagnostics are present in
     `crates/editor-ui/src/menus/registry.rs:204-238` and `:320`,
     `crates/editor-ui/src/menus/shortcut.rs:230-247`,
     `crates/editor-egui-host/src/shortcut_help.rs:66-96`,
     `crates/editor-egui-host/src/shortcut_conflicts.rs:11-29`, and
     `crates/editor-egui-host/src/menu.rs:147-161` / `:208-243`. The inverse
     search
     `rg -n "remap|keymap|shortcut.*preference|preferences|preference" crates/editor-ui/src/menus crates/editor-egui-host/src crates/editor-shell/src/lifecycle`
     returned only entity face-ID remapping comments in
     `crates/editor-shell/src/lifecycle/mod.rs:1771,1811` and related tests,
     not shortcut remapping, keymaps, or shortcut preferences. Conclusion:
     remaining shortcut work is broader remapping/persistence/policy, not a
     one-dispatch local polish item.
   - Host-shell routing: the host-to-shell FIFO and single route sink are
     present in `crates/editor-egui-host/src/handoff.rs:93-137`,
     `crates/editor-egui-host/src/lib.rs:273-279` / `:581-587`, and
     `crates/editor-shell/src/render_path.rs:357-371` / `:415-477`. Conclusion:
     replacing or restructuring command routing would cross the established
     host-shell ownership boundary and is deferred.
   - Plugin execution: extension commands are represented and captured through
     `Command::Custom` / `Command::Plugin` in
     `crates/editor-ui/src/menus/command.rs:11`,
     `crates/editor-shell/src/render_path.rs:475`, and
     `crates/editor-shell/src/lifecycle/extension_command.rs:58-163`. The
     inverse search
     `rg -n "plugin_discovery|plugin-discovery|discover.*plugin|load.*plugin|PluginRegistry|PluginDiscovery" crates/editor-shell/src crates/editor-egui-host/src crates/editor-ui/src/menus crates/plugin-discovery/src runtime`
     returned only `crates/plugin-discovery/src/lib.rs:1`, a stub-crate
     marker. Conclusion: real plugin execution still requires discovery/loading
     and runtime policy work beyond a narrow editor-usability follow-up.
   - OS/typed clipboard: Edit menu copy/paste routes through shell-local entity
     clipboard state in `crates/editor-shell/src/render_path.rs:448-451`,
     `crates/editor-shell/src/lifecycle/mod.rs:227` / `:784` /
     `:1794-1825`, and menu predicates in
     `crates/editor-ui/src/menus/default_menu.rs:248-280` plus
     `crates/editor-ui/src/menus/predicate.rs:51-52`. The inverse search
     `rg -n "arboard|copypasta" Cargo.toml crates/editor-shell/src crates/editor-ui/src crates/editor-egui-host/src`
     returned no matches (exit 1). Conclusion: OS or typed clipboard support
     would require dependency/format/product decisions and is deferred.
   - CAD/CommandBus mutation: mutation authority is centered on
     `CommandBus` and save/dirty state in `crates/editor-actions/src/lib.rs:10`,
     `crates/editor-actions/src/bus.rs:86-304`,
     `crates/editor-shell/src/lifecycle/commands.rs:262-339`,
     `crates/editor-shell/src/lifecycle/save_request.rs:178-267`, and
     projection reads in `crates/cad-projection/src/render_adapter.rs:90` /
     `crates/editor-shell/src/lifecycle/mod.rs:2077-2098`. Conclusion:
     authoritative CAD/editor mutation remains coupled to CommandBus,
     projection, undo/dirty, and save/load policy, so it is deferred.
   - Camera/navigation: viewport camera features now include scene reset,
     selected-CAD double-click framing, wheel zoom, orbit, pan, cursor grab,
     focus-loss drag cancellation, and cursor-left cancellation in
     `crates/editor-shell/src/lifecycle/mod.rs:2114-2324`,
     `crates/editor-shell/src/lifecycle/viewport_navigation.rs:1-165`, and
     tests around `crates/editor-shell/src/lifecycle/tests.rs:891-1927`.
     Fresh source reads show a narrow remaining lifecycle gap:
     `handle_window_focus_change(false)` calls only
     `cancel_viewport_drags_for_focus_loss()` at
     `crates/editor-shell/src/lifecycle/mod.rs:2299-2306`, while
     `handle_cursor_left()` also calls `reset_viewport_left_double_click()` at
     `:2309-2312`; existing focus-loss tests cover drag cancellation at
     `crates/editor-shell/src/lifecycle/tests.rs:1712-1808`, and cursor-left
     has a stale-double-click reset test at `:1906-1927`. The inverse search
     `rg -n "focus_loss.*double|double_click.*focus|reset_viewport_left_double_click\\(\\).*focus|Focused\\(false\\).*double" crates/editor-shell/src/lifecycle/mod.rs crates/editor-shell/src/lifecycle/tests.rs`
     returned no matches (exit 1). Conclusion: resetting stale
     viewport-left-double-click state on focus loss is a single bounded
     `editor-shell` lifecycle/test follow-up.
   - Selected outcome: append exactly one task 141 for focus-loss stale
     double-click reset. No implementation work for task 141 was performed,
     and no task 142 was appended.

141. **[DONE 2026-06-14 via ISSUE-395] Reset stale viewport left-double-click state on focus loss.**
   Implement the bounded camera/navigation lifecycle polish identified by task
   140: when the window loses focus, clear any pending viewport left-double-click
   state before or while cancelling active viewport drags, so a pre-focus-loss
   left press cannot combine with a post-focus-regain left press to trigger
   frame-all or selected-CAD framing. Preserve the existing focus-loss drag
   cancellation/cursor-grab release behavior and leave focus-gained events as a
   no-op for active drag state.

   **Self-re-arm (final step, required):** After the implementation is complete
   and verified, follow the Self-re-arm protocol in this file's header: append
   exactly one bounded next-task-source AUDIT task as task 142 — a
   "Post-focus-loss-double-click-reset Phase 9 next-task source audit"
   (docs/source-read-only) — or, if no bounded in-policy next task exists, append
   a `NEEDS_HUMAN_RECORDED: <ISO-date> — <reason>` line instead. Mirror task
   140's audit STRUCTURE (sections, scope discipline) but **NOT** its
   "no task N+1 is appended" / "rg ^142 returns no matches" criteria: task 142 is
   itself a self-re-arming audit, so its done-criteria MUST require appending
   exactly one bounded FEATURE task 143 (carrying the copied Self-re-arm
   requirement). Copy this Self-re-arm (final step) requirement into the audit
   task you author. Edit `.ai/dispatch.tasks.md` to do this (already in
   `MAY edit`).

   **MAY edit:**
   - `crates/editor-shell/src/lifecycle/mod.rs`
   - `crates/editor-shell/src/lifecycle/tests.rs`
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - generated ISSUE-<n> handoff/audit/log artifacts for the dispatch

   **MUST NOT edit:**
   - `crates/editor-ui/**`
   - `crates/editor-egui-host/**`
   - `crates/editor-shell/src/render_path.rs`
   - `crates/editor-shell/src/camera.rs`
   - `crates/editor-shell/src/lifecycle/viewport_navigation.rs`
   - `crates/editor-actions/**`
   - `crates/cad-projection/**`
   - `crates/plugin-discovery/**`
   - `runtime/**`
   - Cargo manifests or `Cargo.lock`
   - workflows, schemas, ADR files, architecture-lint rules/config, packet
     templates, or dispatch automation scripts
   - command routing, command enum/menu definitions, shortcut execution,
     remapping/persistence/fatal policy, plugin runtime/discovery/loading, OS
     clipboard behavior, CAD/projection/CommandBus mutation, undo/dirty,
     save/load authority, camera math, viewport hit testing, face-pick policy,
     wheel zoom, right-button orbit math, or middle-button pan math

   **Done criteria:**
   - `WindowEvent::Focused(false)` / `handle_window_focus_change(false)` resets
     pending `ViewportLeftDoubleClick` state.
   - Existing focus-loss drag cancellation still cancels active orbit and pan
     drags and releases cursor grab once when appropriate.
   - `handle_window_focus_change(true)` remains a no-op for active drag state.
   - Cursor-left cancellation and existing left-double-click behavior remain
     unchanged except for the new focus-loss stale-state reset.
   - Focused lifecycle tests cover a first left press, focus loss, and a second
     in-threshold left press that must not frame the scene or selected CAD
     bounds.
   - Exactly one bounded next-task-source audit task 142 is appended per the
     Self-re-arm protocol (with its own `MAY edit` / `MUST NOT edit` /
     `Done criteria` / `Verification` / `Halt conditions` and the copied
     Self-re-arm requirement), or a single `NEEDS_HUMAN_RECORDED:` line is
     appended instead. No other task is added.

   **Verification:**
   - `cargo test -p rge-editor-shell --lib viewport_focus_loss`
   - `cargo test -p rge-editor-shell --lib viewport_cursor_left`
   - `cargo test -p rge-editor-shell --lib viewport_left_double_click`
   - `git diff --name-only`
   - `git diff --check`

   **Halt conditions:**
   - The implementation would require edits outside the `MAY edit` list.
   - The fix requires changing command routing, menus, shortcuts, command enum
     variants, camera math, viewport hit testing, face-pick policy, CAD
     projection/CommandBus state, save/load behavior, plugin runtime/discovery,
     OS clipboard behavior, Cargo metadata, workflows, schemas, automation, or
     packet templates.
   - More than one behavioral follow-up is required to make the stale-focus-loss
     double-click reset coherent.

142. **[DONE 2026-06-15 via ISSUE-396] Post-focus-loss-double-click-reset Phase 9 next-task source audit.**
   Perform a docs/source-read-only Phase 9 next-task audit after task 141's
   focus-loss double-click reset. Use current local source plus the
   dispatcher-provided GitHub-state snapshot embedded in the auto-created issue
   body for queue/already-filed evidence. Do not use `gh`, browser, network, or
   live GitHub state for those claims.

   Audit the remaining editor-usability candidate classes from current source:
   - keybinding remapping, preferences, conflict policy, and fatal-policy
     behavior;
   - host-shell command routing through `MenuCommandHandoff` /
     `EditorShell::route_menu_command`;
   - real plugin command execution beyond the injected extension-command seam;
   - OS or typed clipboard behavior beyond the current shell-local clipboard;
   - CAD/editor mutation through `CommandBus`, projection, undo/dirty, and
     save/load authority;
   - camera/navigation follow-up after wheel zoom, orbit, pan, frame-all,
     selected-CAD framing, cursor grab/release, focus-loss drag cancellation,
     cursor-left cancellation, and focus-loss stale double-click reset.

   Append exactly one bounded FEATURE follow-up as task 143, or record
   source-grounded `NEEDS_HUMAN_RECORDED` if every remaining candidate crosses a
   policy or architecture boundary that cannot be safely delegated.

   **Context snapshot:**
   - Task 141 shipped as ISSUE-395: `WindowEvent::Focused(false)` /
     `handle_window_focus_change(false)` now resets pending
     `ViewportLeftDoubleClick` state before cancelling active viewport drags.
     Existing focus-loss drag cancellation and cursor-grab release behavior are
     preserved, and `handle_window_focus_change(true)` remains a no-op for
     active drag state.
   - Focused lifecycle tests cover stale scene-wide frame prevention and stale
     selected-CAD frame prevention across first viewport left press, focus loss,
     and a second in-threshold viewport left press, plus the pre-existing
     focus-loss orbit/pan/cursor-grab cases.
   - Cursor-left cancellation and ordinary viewport left-double-click behavior
     remain unchanged except for clearing stale pending state on focus loss.
     No command routing, command enum/menu definitions, shortcut behavior,
     CAD/projection mutation, CommandBus/undo/dirty/save-load authority, camera
     math, viewport hit testing, face-pick policy, wheel zoom, right-button
     orbit math, middle-button pan math, plugin, clipboard, Cargo, workflow,
     schema, automation, or packet-template behavior changed.
   - The auto-created issue body will include the dispatcher GitHub-state
     snapshot. The audit must use that embedded snapshot, or an exact local
     artifact/read path to it, for GitHub queue/already-filed-task evidence.
     Do not call `gh` or the network from inside the executor sandbox for those
     claims.

   **Self-re-arm (final step, required):** After the audit is complete and
   verified, follow the Self-re-arm protocol in this file's header: append
   exactly one bounded FEATURE task as task 143, carrying this same
   Self-re-arm final-step requirement so the chain continues, or, if no bounded
   in-policy next task exists, append a single
   `NEEDS_HUMAN_RECORDED: <ISO-date> - <reason>` line instead. Edit
   `.ai/dispatch.tasks.md` to do this. The task 143 feature must include
   explicit `MAY edit`, `MUST NOT edit`, `Done criteria`, `Verification`, and
   `Halt conditions` sections.

   **MAY edit:**
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - generated ISSUE-<n> handoff/audit/log artifacts for the dispatch

   **MUST NOT edit:**
   - Rust source or tests
   - Cargo manifests or `Cargo.lock`
   - workflows
   - dispatch automation, guard, queue, scheduler, watcher, verification, or
     health/trend scripts
   - schemas, ADR files, architecture-lint rules/config, packet templates, or
     unrelated existing handoff/log artifacts
   - plugin runtime/discovery/loading code, command routing, shortcut
     execution, remapping/persistence/fatal policy, OS clipboard behavior,
     CAD/projection/CommandBus mutation, undo/dirty/save-load authority,
     camera/navigation behavior, camera math, viewport hit testing, face-pick
     policy, wheel zoom, right-button orbit math, or middle-button pan math

   **Done criteria:**
   - The audit records the pre-edit task-heading check for
     `^140\.|^141\.|^142\.|^143\.`.
   - Queue/already-filed-task claims cite only the dispatcher-provided snapshot
     embedded in the issue body or an exact local artifact path copied from it;
     no live `gh`, browser, network, or GitHub API query is run by the
     sandboxed executor.
   - Each candidate class above has positive source references and falsifying
     searches for negative claims where practical.
   - Exactly one bounded FEATURE task 143 is appended with explicit `MAY edit`,
     `MUST NOT edit`, `Done criteria`, `Verification`, and `Halt conditions`,
     and it carries the copied Self-re-arm final-step requirement, or a
     source-grounded `NEEDS_HUMAN_RECORDED` record is written.
   - No implementation work for task 143 is done, and no other task is added.

   **Verification:**
   - `rg -n "^140\.|^141\.|^142\.|^143\." .ai/dispatch.tasks.md` before edits
     and after edits
   - status-doc cross-check against `Status.md`, `HANDOFF.md`,
     `plans/BASELINE.md`, and `change.md`
   - candidate-class source greps recorded in the audit
   - `git diff --name-only`
   - `git diff --check`
   - `.\new-handoff.ps1 -Finalize -PacketPath <EXEC_PACKET> -DryRun`

   **Halt conditions:**
   - The executor cannot cite the dispatcher-provided GitHub-state snapshot
     without live `gh`, browser, network, or GitHub API access.
   - The audit would require editing a MUST-NOT path or implementing task 143.
   - More than one feature follow-up would be required to make the selected
     boundary coherent.
   - No bounded task 143 can be specified without crossing a policy or
     architecture boundary; record `NEEDS_HUMAN_RECORDED` instead of forcing a
     task.

   **Execution audit (ISSUE-396, 2026-06-15):**
   - Pre-edit task-heading check:
     `rg -n "^140\.|^141\.|^142\.|^143\." .ai/dispatch.tasks.md` returned
     task 140 done at line 13308, task 141 done at line 13514, task 142 open at
     line 13600, and no task 143 heading.
   - Dispatcher snapshot evidence: queue/already-filed-task claims use only
     the ISSUE-396 packet's dispatcher snapshot, generated by
     `Invoke-AiDispatchAuto.ps1` at `2026-06-15T00:02:36.6430933+03:00` for
     `RustCADs/RGE`, which reported no open `ai-dispatch` issue before
     ISSUE-396 was created, no open failed autonomous issue, and autonomous
     issues already filed through closed #395. No `gh`, browser, network, or
     GitHub API command was run.
   - Status-doc cross-check:
     `rg -n "ISSUE-395|task 141|task 142|task 143|focus-loss|double-click reset|Post-focus-loss" Status.md HANDOFF.md plans/BASELINE.md change.md .ai/dispatch.tasks.md`
     showed the allowed status docs identifying ISSUE-395/task 141 as complete
     and task 142 as the open post-focus-loss audit.
   - Keybinding/remap/preferences/fatal policy: current source has shortcut
     conflict detection and host diagnostics in
     `crates/editor-ui/src/menus/registry.rs:248-321`,
     `crates/editor-ui/src/menus/shortcut.rs:288-293`,
     `crates/editor-egui-host/src/shortcut_conflicts.rs:1-68`,
     `crates/editor-egui-host/src/shortcut_help.rs:159-190`, and
     `crates/editor-egui-host/src/menu.rs:147-161`. The inverse search
     `rg -n "shortcut.*remap|remap.*shortcut|keymap|shortcut.*preference|shortcut.*preferences|preferences.*shortcut" crates/editor-ui/src/menus crates/editor-egui-host/src crates/editor-shell/src/lifecycle`
     returned no matches (exit 1), so shortcut remapping/preferences remain a
     broader policy/persistence surface and are deferred.
   - Host-shell routing: `MenuCommandHandoff` and `route_menu_command` already
     route core menu commands through the shell in
     `crates/editor-shell/src/lifecycle/mod.rs:759-772`,
     `crates/editor-shell/src/render_path.rs:406-475`, and route tests around
     `crates/editor-shell/src/lifecycle/tests.rs:4564-5318`. Replacing or
     broadening host-shell routing would cross the command-routing surface and
     is deferred.
   - Plugin execution: `Command::Custom` / `Command::Plugin` activations are
     retained for the extension-command seam in
     `crates/editor-shell/src/render_path.rs:377-475` and delivered to an
     injected `ExtensionCommandHandler` in
     `crates/editor-shell/src/lifecycle/extension_command.rs:58-177`, with
     FIFO/missing-handler/handler-failure tests around
     `crates/editor-shell/src/lifecycle/tests.rs:5317-5496`. The discovery
     search `rg -n "PluginRegistry|PluginDiscovery|plugin-discovery|plugin_discovery" crates/plugin-discovery/src crates/editor-shell/src crates/editor-ui/src/menus runtime crates`
     found only the `rge-plugin-discovery` stub crate metadata/source, so real
     plugin discovery/loading/execution remains a broader runtime surface and
     is deferred.
   - OS/typed clipboard: Edit-menu copy/paste commands route to the shell-local
     legacy entity clipboard in `crates/editor-shell/src/render_path.rs:448-451`,
     `crates/editor-shell/src/lifecycle/mod.rs:1792-1825`, and tests around
     `crates/editor-shell/src/lifecycle/tests.rs:5020-5158`. The inverse search
     `rg -n "arboard|copypasta" Cargo.toml crates/editor-shell/src crates/editor-ui/src crates/editor-egui-host/src`
     returned no matches (exit 1), so OS clipboard integration or typed
     clipboard formats would require dependency/format decisions and are
     deferred.
   - CAD/CommandBus mutation: mutation authority remains centered on
     `CommandBus` and save/dirty state in `crates/editor-actions/src/lib.rs:10`,
     `crates/editor-actions/src/bus.rs:86-304`,
     `crates/editor-shell/src/lifecycle/commands.rs:262-339`, and
     `crates/editor-shell/src/lifecycle/save_request.rs:178-267`; CAD projection
     reads remain exposed through `crates/cad-projection/src/render_adapter.rs:90`
     and `crates/editor-shell/src/lifecycle/mod.rs:2084-2098`. This is broader
     authority than a single follow-up and is deferred.
   - Camera/navigation: current source shows the wheel path at
     `crates/editor-shell/src/lifecycle/mod.rs:2145-2153` and
     `:2841-2843`, existing double-click reset calls at `:2301`, `:2312`,
     `:2875`, `:2880`, `:2884`, `:2889`, and `:2892`, and wheel tests around
     `crates/editor-shell/src/lifecycle/tests.rs:1284-1351`. The inverse search
     `rg -n "wheel.*double|double.*wheel|mouse_wheel.*left_double|left_double.*mouse_wheel|viewport_mouse_wheel.*double|double_click.*wheel" crates/editor-shell/src/lifecycle/mod.rs crates/editor-shell/src/lifecycle/tests.rs`
     returned no matches (exit 1). Conclusion: resetting pending
     viewport-left-double-click state on mouse-wheel input is a single bounded
     `editor-shell` lifecycle/test follow-up.
   - Selected outcome: append exactly one task 143 for stale viewport
     left-double-click reset on mouse wheel. No implementation work for task 143
     was performed, and no task 144 was appended.

143. **[DONE 2026-06-15 via ISSUE-397] Reset stale viewport left-double-click state on viewport mouse wheel.**
   Implement the bounded camera/navigation lifecycle polish identified by task
   142: when a mouse-wheel event is received, clear any pending viewport
   left-double-click state before or while handling the existing viewport wheel
   zoom/no-op decision, so a left press before wheel input cannot combine with a
   later in-threshold left press to trigger scene-wide or selected-CAD framing.
   Preserve existing wheel zoom behavior, left-click face picking,
   left-double-click frame-all/selected-CAD behavior after valid consecutive
   left presses, focus-loss and cursor-left cancellation, cursor-grab handling,
   and camera math.

   **Self-re-arm (final step, required):** After the implementation is complete
   and verified, follow the Self-re-arm protocol in this file's header: append
   exactly one bounded next-task-source AUDIT task as task 144 - a
   "Post-wheel-double-click-reset Phase 9 next-task source audit"
   (docs/source-read-only) - or, if no bounded in-policy next task exists,
   append a `NEEDS_HUMAN_RECORDED: <ISO-date> - <reason>` line instead. Mirror
   task 142's audit structure (sections, scope discipline) but not its
   task-number-specific criteria: task 144 is itself a self-re-arming audit, so
   its done-criteria must require appending exactly one bounded FEATURE task 145
   carrying the copied Self-re-arm requirement, or a single
   `NEEDS_HUMAN_RECORDED:` line. Copy this Self-rearm final-step requirement
   into the audit task you author. Edit `.ai/dispatch.tasks.md` to do this
   (already in `MAY edit`).

   **MAY edit:**
   - `crates/editor-shell/src/lifecycle/mod.rs`
   - `crates/editor-shell/src/lifecycle/tests.rs`
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - generated ISSUE-<n> handoff/audit/log artifacts for the dispatch

   **MUST NOT edit:**
   - `crates/editor-ui/**`
   - `crates/editor-egui-host/**`
   - `crates/editor-shell/src/render_path.rs`
   - `crates/editor-shell/src/camera.rs`
   - `crates/editor-shell/src/lifecycle/viewport_navigation.rs`
   - `crates/editor-actions/**`
   - `crates/cad-projection/**`
   - `crates/plugin-discovery/**`
   - `runtime/**`
   - Cargo manifests or `Cargo.lock`
   - workflows, schemas, ADR files, architecture-lint rules/config, packet
     templates, or dispatch automation scripts
   - command routing, command enum/menu definitions, shortcut execution,
     remapping/persistence/fatal policy, plugin runtime/discovery/loading, OS
     clipboard behavior, CAD/projection/CommandBus mutation, undo/dirty,
     save/load authority, camera math, viewport hit testing, face-pick policy,
     focus-loss behavior, cursor-left behavior, right-button orbit math, or
     middle-button pan math

   **Done criteria:**
   - `WindowEvent::MouseWheel` or its existing lifecycle helper resets pending
     `ViewportLeftDoubleClick` state before the wheel zoom/no-op result can be
     followed by another left press.
   - A first viewport left press, then mouse-wheel input, then a second
     in-threshold viewport left press does not frame the scene or selected CAD
     bounds.
   - Existing vertical wheel zoom over the viewport still zooms in/out, and
     horizontal-only, non-viewport, no-cursor, and no-host wheel cases remain
     no-ops except for clearing pending left-double-click state.
   - Existing ordinary viewport left-double-click, selected-CAD double-click
     framing, focus-loss reset/cancellation, cursor-left reset/cancellation,
     right-button orbit, and middle-button pan behavior remain unchanged.
   - Exactly one bounded next-task-source audit task 144 is appended per the
     Self-rearm protocol (with its own `MAY edit` / `MUST NOT edit` /
     `Done criteria` / `Verification` / `Halt conditions` and the copied
     Self-rearm requirement), or a single `NEEDS_HUMAN_RECORDED:` line is
     appended instead. No other task is added.

   **Verification:**
   - `cargo test -p rge-editor-shell --lib viewport_mouse_wheel`
   - `cargo test -p rge-editor-shell --lib viewport_left_double_click`
   - `cargo test -p rge-editor-shell --lib viewport_focus_loss`
   - `cargo test -p rge-editor-shell --lib viewport_cursor_left`
   - `rg -n "^142\.|^143\.|^144\." .ai/dispatch.tasks.md`
   - `git diff --name-only`
   - `git diff --check`

   **Halt conditions:**
   - The implementation would require edits outside the `MAY edit` list.
   - The fix requires changing command routing, menus, shortcuts, command enum
     variants, camera math, viewport hit testing, face-pick policy, CAD
     projection/CommandBus state, save/load behavior, plugin runtime/discovery,
     OS clipboard behavior, Cargo metadata, workflows, schemas, automation, or
     packet templates.
   - More than one behavioral follow-up is required to make the wheel
     double-click reset coherent.

   **Execution audit (ISSUE-397, 2026-06-15):**
   - Pre-edit task-heading check:
     `rg -n "^141\.|^142\.|^143\.|^144\." .ai/dispatch.tasks.md` returned
     task 141 done at line 13514, task 142 done at line 13600, task 143 open at
     line 13788, and no task 144 heading.
   - Implementation: `zoom_camera_for_viewport_mouse_wheel` now resets pending
     `ViewportLeftDoubleClick` state before preserving the existing wheel
     zoom/no-op direction handling. Focused tests cover stale scene-wide
     framing prevention after an actual wheel zoom and stale selected-CAD
     framing prevention after horizontal-only wheel input.
   - Verification is recorded in the ISSUE-397 EXEC packet. The required
     focused lifecycle gates passed, and the diff stayed inside the task's
     MAY-edit surface plus generated ISSUE-397 artifacts.
   - Selected outcome: append exactly one task 144 docs/source-read-only audit,
     `Post-wheel-double-click-reset Phase 9 next-task source audit`. No task
     145 implementation was performed and no task 145 was appended.

144. **[DONE 2026-06-15 via ISSUE-398] Post-wheel-double-click-reset Phase 9 next-task source audit.**
   Perform a docs/source-read-only Phase 9 next-task audit after task 143's
   mouse-wheel double-click reset. Use current local source plus the
   dispatcher-provided GitHub-state snapshot embedded in the auto-created issue
   body for queue/already-filed evidence. Do not use `gh`, browser, network, or
   live GitHub state for those claims.

   Audit the remaining editor-usability candidate classes from current source:
   - keybinding remapping, preferences, conflict policy, and fatal-policy
     behavior;
   - host-shell command routing through `MenuCommandHandoff` /
     `EditorShell::route_menu_command`;
   - real plugin command execution beyond the injected extension-command seam;
   - OS or typed clipboard behavior beyond the current shell-local clipboard;
   - CAD/editor mutation through `CommandBus`, projection, undo/dirty, and
     save/load authority;
   - camera/navigation follow-up after wheel zoom, orbit, pan, frame-all,
     selected-CAD framing, cursor grab/release, focus-loss drag cancellation,
     cursor-left cancellation, focus-loss stale double-click reset, and
     mouse-wheel stale double-click reset.

   Append exactly one bounded FEATURE follow-up as task 145, or record
   source-grounded `NEEDS_HUMAN_RECORDED` if every remaining candidate crosses a
   policy or architecture boundary that cannot be safely delegated.

   **Context snapshot:**
   - Task 143 shipped as ISSUE-397: mouse-wheel handling now clears pending
     `ViewportLeftDoubleClick` state at the lifecycle helper before the
     existing wheel zoom/no-op result is applied. A left press before wheel
     input can no longer pair with a later in-threshold left press to frame the
     scene or selected CAD bounds.
   - Focused lifecycle tests cover stale scene-wide frame prevention after a
     first viewport left press, vertical wheel zoom, and a second in-threshold
     viewport left press, plus stale selected-CAD frame prevention after a
     first viewport left press, horizontal-only wheel input, and a second
     in-threshold viewport left press.
   - Existing vertical wheel zoom, horizontal-only wheel no-op,
     false-viewport-hit wheel no-op, no-cursor/no-host wheel no-op, ordinary
     viewport double-click framing, selected-CAD double-click framing,
     focus-loss reset/cancellation, cursor-left reset/cancellation,
     right-button orbit, and middle-button pan behavior remained unchanged in
     intent.
   - The auto-created issue body will include the dispatcher GitHub-state
     snapshot. The audit must use that embedded snapshot, or an exact local
     artifact/read path to it, for GitHub queue/already-filed-task evidence.
     Do not call `gh` or the network from inside the executor sandbox for those
     claims.

   **Self-re-arm (final step, required):** After the audit is complete and
   verified, follow the Self-re-arm protocol in this file's header: append
   exactly one bounded FEATURE task as task 145, carrying this same
   Self-re-arm final-step requirement so the chain continues, or, if no bounded
   in-policy next task exists, append a single
   `NEEDS_HUMAN_RECORDED: <ISO-date> - <reason>` line instead. Edit
   `.ai/dispatch.tasks.md` to do this. The task 145 feature must include
   explicit `MAY edit`, `MUST NOT edit`, `Done criteria`, `Verification`, and
   `Halt conditions` sections.

   **MAY edit:**
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - generated ISSUE-<n> handoff/audit/log artifacts for the dispatch

   **MUST NOT edit:**
   - Rust source or tests
   - Cargo manifests or `Cargo.lock`
   - workflows
   - dispatch automation, guard, queue, scheduler, watcher, verification, or
     health/trend scripts
   - schemas, ADR files, architecture-lint rules/config, packet templates, or
     unrelated existing handoff/log artifacts
   - plugin runtime/discovery/loading code, command routing, shortcut
     execution, remapping/persistence/fatal policy, OS clipboard behavior,
     CAD/projection/CommandBus mutation, undo/dirty/save-load authority,
     camera/navigation behavior, camera math, viewport hit testing, face-pick
     policy, wheel zoom, right-button orbit math, or middle-button pan math

   **Done criteria:**
   - The audit records the pre-edit task-heading check for
     `^141\.|^142\.|^143\.|^144\.|^145\.`.
   - Queue/already-filed-task claims cite only the dispatcher-provided snapshot
     embedded in the issue body or an exact local artifact path copied from it;
     no live `gh`, browser, network, or GitHub API query is run by the
     sandboxed executor.
   - Each candidate class above has positive source references and falsifying
     searches for negative claims where practical.
   - Exactly one bounded FEATURE task 145 is appended with explicit `MAY edit`,
     `MUST NOT edit`, `Done criteria`, `Verification`, and `Halt conditions`,
     and it carries the copied Self-re-arm final-step requirement, or a
     source-grounded `NEEDS_HUMAN_RECORDED` record is written.
   - No implementation work for task 145 is done, and no other task is added.

   **Verification:**
   - `rg -n "^141\.|^142\.|^143\.|^144\.|^145\." .ai/dispatch.tasks.md` before
     edits and after edits
   - status-doc cross-check against `Status.md`, `HANDOFF.md`,
     `plans/BASELINE.md`, and `change.md`
   - candidate-class source greps recorded in the audit
   - `git diff --name-only`
   - `git diff --check`
   - `.\new-handoff.ps1 -Finalize -PacketPath <EXEC_PACKET> -DryRun`

   **Halt conditions:**
   - The executor cannot cite the dispatcher-provided GitHub-state snapshot
     without live `gh`, browser, network, or GitHub API access.
   - The audit would require editing a MUST-NOT path or implementing task 145.
   - More than one feature follow-up would be required to make the selected
     boundary coherent.
   - No bounded task 145 can be specified without crossing a policy or
     architecture boundary; record `NEEDS_HUMAN_RECORDED` instead of forcing a
     task.

   **Execution audit (ISSUE-398, 2026-06-15):**
   - Pre-edit task-heading check:
     `rg -n "^141\.|^142\.|^143\.|^144\.|^145\." .ai/dispatch.tasks.md`
     returned task 141 done at line 13514, task 142 done at line 13600, task
     143 done at line 13788, task 144 open at line 13898, and no task 145
     heading.
   - Dispatcher snapshot evidence: queue/already-filed-task claims use only
     the ISSUE-398 task packet's dispatcher snapshot, generated by
     `Invoke-AiDispatchAuto.ps1` at `2026-06-15T01:52:55.9305454+03:00` for
     `RustCADs/RGE`, which reported no open `ai-dispatch` issue before
     ISSUE-398 was created, no open failed autonomous issue, and autonomous
     issues already filed through closed ISSUE-397 / task 143. No `gh`,
     browser, network, or GitHub API command was run.
   - Status-doc cross-check:
     `rg -n "ISSUE-397|task 143|task 144|task 145|wheel double-click|mouse-wheel" Status.md HANDOFF.md plans/BASELINE.md change.md .ai/dispatch.tasks.md`
     showed ISSUE-397/task 143 recorded complete in the allowed status docs and
     task 144 as the open post-wheel audit before this edit; no task 145 heading
     was present.
   - Keybinding/remap/preferences/fatal policy: current source keeps shortcut
     translation and conflict handling in
     `crates/editor-shell/src/lifecycle/accelerator.rs:48` and `:220-254`,
     `crates/editor-ui/src/menus/shortcut.rs:230-293`,
     `crates/editor-ui/src/menus/registry.rs:248-321`, and host diagnostics in
     `crates/editor-egui-host/src/shortcut_conflicts.rs:144-177` and
     `crates/editor-egui-host/src/shortcut_help.rs:159-190`. The inverse
     search
     `rg -n "shortcut.*remap|remap.*shortcut|keymap|shortcut.*preference|shortcut.*preferences|preferences.*shortcut|shortcut.*persist|persist.*shortcut" crates/editor-ui/src/menus crates/editor-egui-host/src crates/editor-shell/src/lifecycle/accelerator.rs`
     returned no matches (exit 1). The fatal-policy search
     `rg -n "fatal|non-fatal" crates/editor-ui/src/menus crates/editor-egui-host/src/shortcut_conflicts.rs crates/editor-egui-host/src/shortcut_help.rs crates/editor-egui-host/src/menu.rs crates/editor-shell/src/lifecycle/accelerator.rs`
     found only the existing "host decides whether a conflict is fatal" comment
     and non-fatal host diagnostics, so shortcut remapping/preferences/fatal
     policy remains a broader policy/persistence surface and is deferred.
   - Host-shell routing: menu and palette activation still use
     `MenuCommandHandoff` and route through `EditorShell::route_menu_command` in
     `crates/editor-egui-host/src/handoff.rs:73-128`,
     `crates/editor-egui-host/src/lib.rs:855-899`,
     `crates/editor-egui-host/src/palette_recent.rs:77-94`, and
     `crates/editor-shell/src/render_path.rs:357-415`, with route coverage in
     `crates/editor-shell/src/lifecycle/tests.rs:4704-5594`. Replacing or
     broadening this route would cross the command-routing surface and is
     deferred.
   - Plugin execution: extension commands still stop at the injected seam:
     `crates/editor-shell/src/lifecycle/extension_command.rs:1-4` explicitly
     excludes plugin discovery/loading/runtime execution, `:58-122` defines the
     injectable handler, `:161-186` captures extension commands, and
     `crates/editor-shell/src/render_path.rs:377-476` routes `Command::Custom`
     / `Command::Plugin` into that seam. The inverse discovery/loading search
     `rg -n "PluginRegistry|PluginDiscovery|plugin-discovery|plugin_discovery|load_plugin|plugin.*load|discover.*plugin" crates/plugin-discovery/src crates/editor-shell/src crates/editor-ui/src/menus crates/editor-egui-host/src runtime`
     returned only the `rge-plugin-discovery` stub and seam comments, so real
     plugin discovery/loading/runtime execution remains a broader runtime
     surface and is deferred.
   - OS/typed clipboard: Edit Cut/Copy/Paste still route to the shell-local
     entity clipboard in `crates/editor-shell/src/render_path.rs:444-451`,
     `crates/editor-shell/src/lifecycle/mod.rs:779-784` and `:1789-1845`, with
     menu predicates in `crates/editor-ui/src/menus/default_menu.rs:238-280`.
     `Cargo.lock` contains a transitive `arboard` package, so lockfile absence
     is not used as evidence; the editor-slice inverse searches
     `rg -n "arboard|copypasta|clipboard|Clipboard" Cargo.toml crates -g Cargo.toml`
     and
     `rg -n "arboard|copypasta|ClipboardProvider|SystemClipboard|set_clipboard|get_clipboard|clipboard_text|copy_text|OutputCommand::CopyText" crates/editor-shell/src crates/editor-ui/src crates/editor-egui-host/src`
     returned no matches (exit 1). OS clipboard integration or typed clipboard
     formats are deferred.
   - CAD/editor mutation: mutation authority remains centered on `CommandBus`
     and save/dirty state in `crates/editor-actions/src/lib.rs:10`,
     `crates/editor-actions/src/bus.rs:78-143` and `:277-285`,
     `crates/editor-shell/src/lifecycle/commands.rs:262-313`, and
     `crates/editor-shell/src/lifecycle/save_request.rs:175-267`; CAD
     projection reads remain exposed through
     `crates/cad-projection/src/render_adapter.rs:90` and selected-CAD bounds
     in `crates/editor-shell/src/lifecycle/mod.rs:2084-2098`. This is broader
     authority than a single follow-up and is deferred.
   - Camera/navigation: current source has View/Home reset-camera routing in
     `crates/editor-ui/src/menus/default_menu.rs:342-345`,
     `crates/editor-shell/src/lifecycle/accelerator.rs:122` and `:246`, and
     `crates/editor-shell/src/render_path.rs:468-471`. The helper itself is
     `crates/editor-shell/src/lifecycle/mod.rs:2114-2125`; viewport
     left-double-click state and reset helpers are at `:604`, `:2158-2178`,
     with existing stale-state resets for wheel/focus/cursor/right/middle/other
     mouse input at `:2150`, `:2302`, `:2313`, `:2876`, `:2881`, `:2885`,
     `:2890`, and `:2893`. The inverse search
     `rg -n "reset_camera.*reset_viewport_left_double_click|reset_viewport_left_double_click.*reset_camera|Command::ResetCamera.*reset_viewport_left_double_click|frame_cad_selection_or_reset_camera.*reset_viewport_left_double_click" crates/editor-shell/src/lifecycle/mod.rs crates/editor-shell/src/render_path.rs crates/editor-shell/src/lifecycle/tests.rs`
     returned no matches (exit 1). Conclusion: clearing pending viewport
     left-double-click state when View/Home reset-camera framing runs is a
     single bounded `editor-shell` lifecycle/test follow-up.
   - Selected outcome: append exactly one task 145 for resetting stale viewport
     left-double-click state on View/Home reset-camera framing. No implementation
     work for task 145 was performed, and no task 146 was appended.

145. **[DONE 2026-06-15 via ISSUE-399] Reset stale viewport left-double-click state on View/Home reset camera.**
   Implement the bounded camera/navigation lifecycle polish identified by task
   144: when `EditorShell::reset_camera` runs through View -> Reset Camera /
   Frame Scene or the Home accelerator's existing `Command::ResetCamera` path,
   clear any pending viewport left-double-click state before a later
   in-threshold viewport left press can be interpreted as the second press of
   the earlier gesture. Preserve existing reset-camera framing/default fallback,
   ordinary valid consecutive-left-click scene framing, selected-CAD
   double-click framing, focus-loss/cursor-left/mouse-wheel stale reset
   behavior, right-button orbit, middle-button pan, camera math, viewport hit
   testing, and face-pick policy.

   **Self-re-arm (final step, required):** After the implementation is complete
   and verified, follow the Self-re-arm protocol in this file's header: append
   exactly one bounded next-task-source AUDIT task as task 146 - a
   "Post-reset-camera-double-click-reset Phase 9 next-task source audit"
   (docs/source-read-only) - or, if no bounded in-policy next task exists,
   append a `NEEDS_HUMAN_RECORDED: <ISO-date> - <reason>` line instead. Mirror
   task 144's audit structure (sections, scope discipline) but not its
   task-number-specific criteria: task 146 is itself a self-re-arming audit, so
   its done-criteria must require appending exactly one bounded FEATURE task 147
   carrying the copied Self-re-arm requirement, or a single
   `NEEDS_HUMAN_RECORDED:` line. Copy this Self-rearm final-step requirement
   into the audit task you author. Edit `.ai/dispatch.tasks.md` to do this
   (already in `MAY edit`).

   **MAY edit:**
   - `crates/editor-shell/src/lifecycle/mod.rs`
   - `crates/editor-shell/src/lifecycle/tests.rs`
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - generated ISSUE-<n> handoff/audit/log artifacts for the dispatch

   **MUST NOT edit:**
   - `crates/editor-ui/**`
   - `crates/editor-egui-host/**`
   - `crates/editor-shell/src/render_path.rs`
   - `crates/editor-shell/src/camera.rs`
   - `crates/editor-shell/src/lifecycle/viewport_navigation.rs`
   - `crates/editor-actions/**`
   - `crates/cad-projection/**`
   - `crates/plugin-discovery/**`
   - `runtime/**`
   - Cargo manifests or `Cargo.lock`
   - workflows, schemas, ADR files, architecture-lint rules/config, packet
     templates, or dispatch automation scripts
   - command routing, command enum/menu definitions, shortcut execution,
     remapping/persistence/fatal policy, plugin runtime/discovery/loading, OS
     clipboard behavior, CAD/projection/CommandBus mutation, undo/dirty,
     save/load authority, camera math, viewport hit testing, face-pick policy,
     wheel zoom, right-button orbit math, middle-button pan math, focus-loss
     behavior, cursor-left behavior, or mouse-wheel behavior

   **Done criteria:**
   - `EditorShell::reset_camera` or its existing lifecycle caller clears pending
     `ViewportLeftDoubleClick` state before a following viewport left press can
     pair with a pre-reset-camera left press.
   - A first viewport left press, then View/Home reset-camera framing, then a
     second in-threshold viewport left press does not frame the scene or
     selected CAD bounds as a stale double-click.
   - Existing direct `reset_camera` framing still frames prebuilt render meshes,
     CAD projection scenes, and empty/no-scene fallback exactly as before except
     for clearing pending left-double-click state.
   - Existing ordinary viewport left-double-click scene framing, selected-CAD
     double-click framing, focus-loss/cursor-left/mouse-wheel stale reset
     behavior, right-button orbit, and middle-button pan remain unchanged.
   - Exactly one bounded next-task-source audit task 146 is appended per the
     Self-rearm protocol (with its own `MAY edit` / `MUST NOT edit` /
     `Done criteria` / `Verification` / `Halt conditions` and the copied
     Self-rearm requirement), or a single `NEEDS_HUMAN_RECORDED:` line is
     appended instead. No other task is added.

   **Verification:**
   - `cargo test -p rge-editor-shell --lib reset_camera`
   - `cargo test -p rge-editor-shell --lib viewport_left_double_click`
   - `cargo test -p rge-editor-shell --lib viewport_mouse_wheel`
   - `cargo test -p rge-editor-shell --lib viewport_focus_loss`
   - `cargo test -p rge-editor-shell --lib viewport_cursor_left`
   - `rg -n "^143\.|^144\.|^145\.|^146\." .ai/dispatch.tasks.md`
   - `git diff --name-only`
   - `git diff --check`

   **Halt conditions:**
   - The implementation would require edits outside the `MAY edit` list.
   - The fix requires changing command routing, command enum/menu definitions,
     shortcut execution/remapping/persistence/fatal policy, plugin
     runtime/discovery/loading, OS clipboard behavior, CAD/projection/CommandBus
     mutation, undo/dirty/save-load authority, camera math, viewport hit
     testing, face-pick policy, wheel zoom, right-button orbit math,
     middle-button pan math, focus-loss behavior, cursor-left behavior,
     mouse-wheel behavior, Cargo metadata, workflows, schemas, automation, or
     packet templates.
   - More than one behavioral follow-up is required to make reset-camera stale
     double-click handling coherent.

   **Execution audit (ISSUE-399, 2026-06-15):**
   - Pre-edit task-heading check:
     `rg -n "^143\.|^144\.|^145\.|^146\." .ai/dispatch.tasks.md` returned
     task 143 done at line 13788, task 144 done at line 13898, task 145 open at
     line 14101, and no task 146 heading.
   - Implementation: `EditorShell::reset_camera` now clears pending
     `ViewportLeftDoubleClick` state before preserving the existing scene-bounds
     or default-camera framing behavior. Focused tests cover stale scene-wide
     framing prevention and stale selected-CAD framing prevention after a first
     viewport left press, reset-camera framing, and a second in-threshold
     viewport left press.
   - Verification is recorded in the ISSUE-399 EXEC packet. The required
     focused lifecycle gates passed, and the diff stayed inside the task's
     MAY-edit surface plus generated ISSUE-399 artifacts.
   - Selected outcome: append exactly one task 146 docs/source-read-only audit,
     `Post-reset-camera-double-click-reset Phase 9 next-task source audit`. No
     task 147 implementation was performed and no task 147 was appended.

146. **[DONE 2026-06-15 via ISSUE-400] Post-reset-camera-double-click-reset Phase 9 next-task source audit.**
   Perform a docs/source-read-only Phase 9 next-task audit after task 145's
   reset-camera stale double-click reset. Use current local source plus the
   dispatcher-provided GitHub-state snapshot embedded in the auto-created issue
   body for queue/already-filed evidence. Do not use `gh`, browser, network, or
   live GitHub state for those claims.

   Audit the remaining editor-usability candidate classes from current source:
   - keybinding remapping, preferences, conflict policy, and fatal-policy
     behavior;
   - host-shell command routing through `MenuCommandHandoff` /
     `EditorShell::route_menu_command`;
   - real plugin command execution beyond the injected extension-command seam;
   - OS or typed clipboard behavior beyond the current shell-local clipboard;
   - CAD/editor mutation through `CommandBus`, projection, undo/dirty, and
     save/load authority;
   - camera/navigation follow-up after wheel zoom, orbit, pan, frame-all,
     selected-CAD framing, cursor grab/release, focus-loss drag cancellation,
     cursor-left cancellation, focus-loss stale double-click reset,
     mouse-wheel stale double-click reset, and reset-camera stale double-click
     reset.

   Append exactly one bounded FEATURE follow-up as task 147, or record
   source-grounded `NEEDS_HUMAN_RECORDED` if every remaining candidate crosses a
   policy or architecture boundary that cannot be safely delegated.

   **Context snapshot:**
   - Task 145 shipped as ISSUE-399: reset-camera handling now clears pending
     `ViewportLeftDoubleClick` state before the existing scene-bounds/default
     camera framing is applied. A left press before View/Home reset-camera
     framing can no longer pair with a later in-threshold left press to frame
     the scene or selected CAD bounds.
   - Focused lifecycle tests cover stale scene-wide frame prevention after a
     first viewport left press, reset-camera framing, and a second in-threshold
     viewport left press, plus stale selected-CAD frame prevention for the same
     sequence while a CAD entity is selected.
   - Existing reset-camera prebuilt-mesh framing, CAD projection framing,
     empty/no-scene default fallback, ordinary viewport double-click framing,
     selected-CAD double-click framing, focus-loss reset/cancellation,
     cursor-left reset/cancellation, mouse-wheel reset/zoom behavior,
     right-button orbit, and middle-button pan behavior remained unchanged in
     intent.
   - The auto-created issue body will include the dispatcher GitHub-state
     snapshot. The audit must use that embedded snapshot, or an exact local
     artifact/read path to it, for GitHub queue/already-filed-task evidence.
     Do not call `gh` or the network from inside the executor sandbox for those
     claims.

   **Self-re-arm (final step, required):** After the audit is complete and
   verified, follow the Self-re-arm protocol in this file's header: append
   exactly one bounded FEATURE task as task 147, carrying this same
   Self-re-arm final-step requirement so the chain continues, or, if no bounded
   in-policy next task exists, append a single
   `NEEDS_HUMAN_RECORDED: <ISO-date> - <reason>` line instead. Edit
   `.ai/dispatch.tasks.md` to do this. The task 147 feature must include
   explicit `MAY edit`, `MUST NOT edit`, `Done criteria`, `Verification`, and
   `Halt conditions` sections.

   **MAY edit:**
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - generated ISSUE-<n> handoff/audit/log artifacts for the dispatch

   **MUST NOT edit:**
   - Rust source or tests
   - Cargo manifests or `Cargo.lock`
   - workflows
   - dispatch automation, guard, queue, scheduler, watcher, verification, or
     health/trend scripts
   - schemas, ADR files, architecture-lint rules/config, packet templates, or
     unrelated existing handoff/log artifacts
   - plugin runtime/discovery/loading code, command routing, shortcut
     execution, remapping/persistence/fatal policy, OS clipboard behavior,
     CAD/projection/CommandBus mutation, undo/dirty/save-load authority,
     camera/navigation behavior, camera math, viewport hit testing, face-pick
     policy, wheel zoom, right-button orbit math, middle-button pan math,
     focus-loss behavior, cursor-left behavior, mouse-wheel behavior, or
     reset-camera behavior

   **Done criteria:**
   - The audit records the pre-edit task-heading check for
     `^143\.|^144\.|^145\.|^146\.|^147\.`.
   - Queue/already-filed-task claims cite only the dispatcher-provided snapshot
     embedded in the issue body or an exact local artifact path copied from it;
     no live `gh`, browser, network, or GitHub API query is run by the
     sandboxed executor.
   - Each candidate class above has positive source references and falsifying
     searches for negative claims where practical.
   - Exactly one bounded FEATURE task 147 is appended with explicit `MAY edit`,
     `MUST NOT edit`, `Done criteria`, `Verification`, and `Halt conditions`,
     and it carries the copied Self-re-arm final-step requirement, or a
     source-grounded `NEEDS_HUMAN_RECORDED` record is written.
   - No implementation work for task 147 is done, and no other task is added.

   **Verification:**
   - `rg -n "^143\.|^144\.|^145\.|^146\.|^147\." .ai/dispatch.tasks.md` before
     edits and after edits
   - status-doc cross-check against `Status.md`, `HANDOFF.md`,
     `plans/BASELINE.md`, and `change.md`
   - candidate-class source greps recorded in the audit
   - `git diff --name-only`
   - `git diff --check`
   - `.\new-handoff.ps1 -Finalize -PacketPath <EXEC_PACKET> -DryRun`

   **Halt conditions:**
   - The executor cannot cite the dispatcher-provided GitHub-state snapshot
     without live `gh`, browser, network, or GitHub API access.
   - The audit would require editing a MUST-NOT path or implementing task 147.
   - More than one feature follow-up would be required to make the selected
     boundary coherent.
   - No bounded task 147 can be specified without crossing a policy or
     architecture boundary; record `NEEDS_HUMAN_RECORDED` instead of forcing a
     task.

   **Execution audit (ISSUE-400, 2026-06-15):**
   - Pre-edit task-heading check:
     `rg -n "^143\.|^144\.|^145\.|^146\.|^147\." .ai/dispatch.tasks.md`
     returned task 143 done at line 13788, task 144 done at line 13898, task
     145 done at line 14101, task 146 open at line 14217, and no task 147
     heading.
   - Dispatcher snapshot evidence: queue/already-filed-task claims use only
     the ISSUE-400 task packet's dispatcher snapshot, generated by
     `Invoke-AiDispatchAuto.ps1` at `2026-06-15T03:32:57.7419822+03:00` for
     `RustCADs/RGE`, which reported no open `ai-dispatch` issue before
     ISSUE-400 was created, no open failed autonomous issue, and autonomous
     issues already filed through closed ISSUE-399 / task 145. No `gh`,
     browser, network, or GitHub API command was run.
   - Status-doc cross-check:
     `rg -n "ISSUE-399|task 145|task 146|task 147|reset-camera|double-click" Status.md HANDOFF.md plans/BASELINE.md change.md .ai/dispatch.tasks.md`
     showed task 145 marked done and task 146 open in this task brief; the
     allowed status docs were still at the prior ISSUE-398/task-144 audit
     snapshot before this edit, so this dispatch updates them to the ISSUE-400
     audit outcome.
   - Keybinding/remap/preferences/fatal policy: shortcut translation and
     conflict handling still live in
     `crates/editor-shell/src/lifecycle/accelerator.rs:48`,
     `crates/editor-ui/src/menus/registry.rs:248-320`,
     `crates/editor-ui/src/menus/shortcut.rs:230-293`,
     `crates/editor-egui-host/src/shortcut_conflicts.rs:144-177`, and
     `crates/editor-egui-host/src/shortcut_help.rs:159-190`. The inverse
     search
     `rg -n "shortcut.*remap|remap.*shortcut|keymap|shortcut.*preference|shortcut.*preferences|preferences.*shortcut|shortcut.*persist|persist.*shortcut" crates/editor-ui/src/menus crates/editor-egui-host/src crates/editor-shell/src/lifecycle/accelerator.rs`
     returned no matches (exit 1). The fatal-policy search
     `rg -n "fatal|non-fatal" crates/editor-ui/src/menus crates/editor-egui-host/src/shortcut_conflicts.rs crates/editor-egui-host/src/shortcut_help.rs crates/editor-egui-host/src/menu.rs crates/editor-shell/src/lifecycle/accelerator.rs`
     found only the existing "host decides whether a conflict is fatal" comment
     and non-fatal host diagnostics. Shortcut remapping/preferences/fatal
     policy remains a broader policy/persistence surface and is deferred.
   - Host-shell routing: menu and palette activation still enqueue through
     `MenuCommandHandoff` in `crates/editor-egui-host/src/handoff.rs:92-127`,
     the host exposes the handoff at `crates/editor-egui-host/src/lib.rs:275`
     and `:581-588`, and the shell drains and routes it through
     `EditorShell::route_menu_command` in
     `crates/editor-shell/src/render_path.rs:366-475`; route coverage includes
     `crates/editor-shell/src/lifecycle/tests.rs:4759-5402`. Replacing or
     broadening this route would cross the command-routing surface and is
     deferred.
   - Plugin execution: extension commands still stop at the injected shell
     seam. `crates/editor-shell/src/lifecycle/extension_command.rs:4`
     explicitly excludes plugin discovery/loading/runtime execution, `:58-61`
     defines the injectable handler, `:113-177` drains/captures shell-local
     extension commands, and `crates/editor-shell/src/render_path.rs:475` routes
     `Command::Custom` / `Command::Plugin` into that seam. The inverse
     discovery/loading search
     `rg -n "PluginRegistry|PluginDiscovery|plugin-discovery|plugin_discovery|load_plugin|plugin.*load|discover.*plugin" crates/plugin-discovery/src crates/editor-shell/src crates/editor-ui/src/menus crates/editor-egui-host/src runtime`
     returned only the `rge-plugin-discovery` stub and seam comments. Real
     plugin discovery/loading/runtime execution remains a broader runtime
     surface and is deferred.
   - OS/typed clipboard: Edit Cut/Copy/Paste still route to the shell-local
     entity clipboard in `crates/editor-shell/src/render_path.rs:444-451`,
     `crates/editor-shell/src/lifecycle/mod.rs:779-784` and `:1789-1845`, with
     menu predicates in `crates/editor-ui/src/menus/default_menu.rs:238-280`.
     The OS/typed clipboard inverse search
     `rg -n "arboard|copypasta|ClipboardProvider|SystemClipboard|set_clipboard|get_clipboard|clipboard_text|copy_text|OutputCommand::CopyText" Cargo.toml crates -g Cargo.toml crates/editor-shell/src crates/editor-ui/src crates/editor-egui-host/src`
     returned no matches (exit 1) in the editor slice. OS clipboard integration
     or typed clipboard formats are deferred.
   - CAD/editor mutation: mutation authority remains centered on `CommandBus`
     and save/dirty state in `crates/editor-actions/src/lib.rs:10`,
     `crates/editor-actions/src/bus.rs:86-284`,
     `crates/editor-shell/src/lifecycle/commands.rs:274-303`, and
     `crates/editor-shell/src/lifecycle/save_request.rs:205-393`; CAD
     projection reads remain exposed through
     `crates/cad-projection/src/render_adapter.rs:90` and selected-CAD bounds
     in `crates/editor-shell/src/lifecycle/mod.rs:2084-2123`. This is broader
     authority than a single follow-up and is deferred.
   - Camera/navigation: current source has View/PageUp/PageDown Zoom In/Out
     entries at `crates/editor-ui/src/menus/default_menu.rs:348-356`,
     accelerator parity at `crates/editor-shell/src/lifecycle/accelerator.rs:247-248`,
     and routing to `zoom_camera_in` / `zoom_camera_out` at
     `crates/editor-shell/src/render_path.rs:472-473`. The zoom helpers are
     `crates/editor-shell/src/lifecycle/mod.rs:2134-2142` and currently call
     `zoom_camera_by` at `:2327`; unlike reset-camera at `:2114-2115` and
     mouse-wheel zoom at `:2146-2151`, they do not clear
     `ViewportLeftDoubleClick` state. Existing resets for right/middle
     orbit/pan inputs, focus loss, cursor leave, other mouse inputs, and
     reset-camera are visible at `:2301-2314` and `:2876-2894`; route tests for
     menu zoom are at `crates/editor-shell/src/lifecycle/tests.rs:5380-5402`.
     The inverse search
     `rg -n "zoom_camera_in.*reset_viewport_left_double_click|reset_viewport_left_double_click.*zoom_camera_in|zoom_camera_out.*reset_viewport_left_double_click|reset_viewport_left_double_click.*zoom_camera_out|Command::ZoomIn.*reset_viewport_left_double_click|Command::ZoomOut.*reset_viewport_left_double_click|PageUp.*reset_viewport_left_double_click|PageDown.*reset_viewport_left_double_click" crates/editor-shell/src/lifecycle/mod.rs crates/editor-shell/src/render_path.rs crates/editor-shell/src/lifecycle/accelerator.rs crates/editor-shell/src/lifecycle/tests.rs`
     returned no matches (exit 1). Conclusion: resetting pending viewport
     left-double-click state when View/PageUp/PageDown Zoom In/Out runs is a
     single bounded `editor-shell` lifecycle/test follow-up.
   - Selected outcome: append exactly one task 147 for resetting stale viewport
     left-double-click state on View/PageUp/PageDown Zoom In/Out camera
     commands. No implementation work for task 147 was performed, and no task
     148 was appended.

147. **[DONE 2026-06-15 via ISSUE-401] Reset stale viewport left-double-click state on View/PageUp/PageDown Zoom In/Out.**
   Implement the bounded camera/navigation lifecycle polish identified by task
   146: when `EditorShell::zoom_camera_in` or `EditorShell::zoom_camera_out`
   runs through View -> Zoom In / Zoom Out or the PageUp / PageDown accelerator
   path, clear any pending viewport left-double-click state before the existing
   camera zoom is applied. Preserve existing zoom distance math, target/up/FOV
   behavior, menu and accelerator routing, valid consecutive-left-click
   scene/selected-CAD framing, View/Home reset-camera behavior, wheel zoom,
   right-button orbit, middle-button pan, cursor grab/release, focus-loss and
   cursor-left cancellation, viewport hit testing, and face-pick policy.

   **Self-re-arm (final step, required):** After the implementation is complete
   and verified, follow the Self-re-arm protocol in this file's header: append
   exactly one bounded next-task-source AUDIT task as task 148 - a
   "Post-zoom-command-double-click-reset Phase 9 next-task source audit"
   (docs/source-read-only) - or, if no bounded in-policy next task exists,
   append a `NEEDS_HUMAN_RECORDED: <ISO-date> - <reason>` line instead. Mirror
   task 146's audit structure (sections, scope discipline) but not its
   task-number-specific criteria: task 148 is itself a self-re-arming audit, so
   its done-criteria must require appending exactly one bounded FEATURE task 149
   or recording `NEEDS_HUMAN_RECORDED`.

   **MAY edit:**
   - `crates/editor-shell/src/lifecycle/mod.rs`
   - `crates/editor-shell/src/lifecycle/tests.rs`
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - generated ISSUE-<n> handoff/audit/log artifacts for the dispatch

   **MUST NOT edit:**
   - any Rust source or tests outside `crates/editor-shell/src/lifecycle/mod.rs`
     and `crates/editor-shell/src/lifecycle/tests.rs`
   - Cargo manifests or `Cargo.lock`
   - workflows
   - dispatch automation, guard, queue, scheduler, watcher, verification, or
     health/trend scripts
   - schemas, ADR files, architecture-lint rules/config, packet templates, or
     unrelated existing handoff/log artifacts
   - command routing, shortcut execution, menu definitions, accelerator
     mappings, plugin runtime/discovery/loading code, remapping/persistence/
     fatal-policy behavior, OS clipboard behavior, CAD/projection/CommandBus
     mutation, undo/dirty/save-load authority, reset-camera behavior,
     mouse-wheel behavior, right-button orbit math, middle-button pan math,
     camera zoom math beyond clearing pending double-click state, viewport hit
     testing, or face-pick policy

   **Done criteria:**
   - `EditorShell::zoom_camera_in` and `EditorShell::zoom_camera_out` clear
     pending `ViewportLeftDoubleClick` state before applying the existing
     `zoom_camera_by` behavior.
   - Focused lifecycle tests cover a first viewport left press, View/PageUp
     Zoom In, and a later in-threshold viewport left press not being treated as
     a stale scene-wide double-click.
   - Focused lifecycle tests cover the same stale-reset behavior for
     View/PageDown Zoom Out while selected CAD bounds are available, proving the
     second press does not frame selected CAD bounds from the earlier press.
   - Existing route tests still prove `Command::ZoomIn` and `Command::ZoomOut`
     reach the same zoom helpers; no command enum, menu, shortcut, or
     accelerator mapping changes are made.
   - Existing valid consecutive-left-click scene and selected-CAD framing,
     reset-camera stale reset, mouse-wheel stale reset, focus-loss stale reset,
     cursor-left stale reset, right-button orbit, middle-button pan, and
     cursor-grab behavior remain unchanged in intent.
   - Exactly one bounded next-task-source AUDIT task 148 is appended, or exactly
     one `NEEDS_HUMAN_RECORDED: <ISO-date> - <reason>` line is recorded. No task
     149 is appended during task 147.

   **Verification:**
   - `cargo test -p rge-editor-shell --lib viewport_left_double_click`
   - `cargo test -p rge-editor-shell --lib menu_zoom_commands_route_via_view`
   - `cargo check -p rge-editor-shell --lib`
   - `cargo +nightly fmt --all -- --check`
   - `rg -n "^145\.|^146\.|^147\.|^148\.|^149\." .ai/dispatch.tasks.md`
     before and after edits
   - `git diff --name-only`
   - `git diff --check`

   **Halt conditions:**
   - The implementation would require changing command routing, menu
     definitions, accelerator mappings, camera zoom math beyond clearing stale
     double-click state, CAD/projection/CommandBus mutation, undo/dirty/save-load
     authority, plugin runtime/discovery/loading, OS clipboard behavior,
     remapping/persistence/fatal policy, or any file outside the `MAY edit`
     list.
   - More than one behavioral follow-up is required to make Zoom In/Out stale
     double-click handling coherent.
   - Existing reset-camera, wheel, orbit, pan, focus-loss, cursor-left, valid
     consecutive-double-click, selected-CAD framing, viewport hit testing, or
     face-pick behavior would need to change beyond the stale state reset.

   **Execution audit (ISSUE-401, 2026-06-15):**
   - Pre-edit task-heading check:
     `rg -n "^145\.|^146\.|^147\.|^148\.|^149\." .ai/dispatch.tasks.md`
     returned task 145 done at line 14101, task 146 done at line 14217, task
     147 open at line 14426, and no task 148 or task 149 heading.
   - Implementation: `EditorShell::zoom_camera_in` now clears pending
     `ViewportLeftDoubleClick` state immediately before the existing
     `self.zoom_camera_by(0.8)` call, and `EditorShell::zoom_camera_out` now
     clears the same pending state immediately before the existing
     `self.zoom_camera_by(1.25)` call. The zoom factors and `zoom_camera_by`
     implementation are unchanged.
   - Focused tests cover stale scene-wide framing prevention after a first
     viewport left press, `zoom_camera_in`, and a second in-threshold viewport
     left press; they also cover stale selected-CAD framing prevention after a
     first viewport left press, selected CAD bounds, `zoom_camera_out`, and a
     second in-threshold viewport left press.
   - Route coverage still proves `Command::ZoomIn` and `Command::ZoomOut`
     reach the View zoom helpers. Existing valid consecutive-left-click scene
     framing, selected-CAD framing, reset-camera stale reset, mouse-wheel stale
     reset, focus-loss stale reset, cursor-left stale reset, orbit, pan, and
     cursor-grab behavior remain unchanged in intent.
   - Verification is recorded in the ISSUE-401 EXEC packet. The focused
     lifecycle gates passed before this self-rearm update, and the final diff
     stayed inside the task's MAY-edit surface plus generated ISSUE-401
     artifacts.
   - Selected outcome: append exactly one task 148 docs/source-read-only audit,
     `Post-zoom-command-double-click-reset Phase 9 next-task source audit`. No
     task 149 implementation was performed and no task 149 was appended.

148. **[DONE 2026-06-15 via ISSUE-402] Post-zoom-command-double-click-reset Phase 9 next-task source audit.**
   Perform a docs/source-read-only Phase 9 next-task audit after task 147's
   Zoom In/Out command stale double-click reset. Use current local source plus
   the dispatcher-provided GitHub-state snapshot embedded in the auto-created
   issue body for queue/already-filed evidence. Do not use `gh`, browser,
   network, or live GitHub state for those claims.

   Audit the remaining editor-usability candidate classes from current source:
   - keybinding remapping, preferences, conflict policy, and fatal-policy
     behavior;
   - host-shell command routing through `MenuCommandHandoff` /
     `EditorShell::route_menu_command`;
   - real plugin command execution beyond the injected extension-command seam;
   - OS or typed clipboard behavior beyond the current shell-local clipboard;
   - CAD/editor mutation through `CommandBus`, projection, undo/dirty, and
     save/load authority;
   - camera/navigation follow-up after wheel zoom, orbit, pan, frame-all,
     selected-CAD framing, cursor grab/release, focus-loss drag cancellation,
     cursor-left cancellation, focus-loss stale double-click reset,
     mouse-wheel stale double-click reset, reset-camera stale double-click
     reset, and Zoom In/Out command stale double-click reset.

   Append exactly one bounded FEATURE follow-up as task 149, or record
   source-grounded `NEEDS_HUMAN_RECORDED` if every remaining candidate crosses a
   policy or architecture boundary that cannot be safely delegated.

   **Context snapshot:**
   - Task 147 shipped as ISSUE-401: View/PageUp Zoom In and View/PageDown Zoom
     Out now clear pending `ViewportLeftDoubleClick` state before applying the
     existing camera zoom math. A left press before either zoom command can no
     longer pair with a later in-threshold left press to frame the scene or
     selected CAD bounds.
   - Focused lifecycle tests cover stale scene-wide frame prevention after a
     first viewport left press, Zoom In, and a second in-threshold viewport
     left press, plus stale selected-CAD frame prevention for the same sequence
     through Zoom Out while a CAD entity is selected.
   - Existing zoom distance math, target/up/FOV/clipping invariants,
     View-menu/PageUp/PageDown routing, reset-camera stale reset, mouse-wheel
     stale reset/no-op behavior, ordinary viewport double-click framing,
     selected-CAD double-click framing, focus-loss/cursor-left behavior,
     right-button orbit, middle-button pan, cursor grab/release, viewport hit
     testing, and face-pick policy remained unchanged in intent.
   - The auto-created issue body will include the dispatcher GitHub-state
     snapshot. The audit must use that embedded snapshot, or an exact local
     artifact/read path to it, for GitHub queue/already-filed-task evidence.
     Do not call `gh` or the network from inside the executor sandbox for those
     claims.

   **Self-re-arm (final step, required):** After the audit is complete and
   verified, follow the Self-re-arm protocol in this file's header: append
   exactly one bounded FEATURE task as task 149, carrying this same
   Self-re-arm final-step requirement so the chain continues, or, if no bounded
   in-policy next task exists, append a single
   `NEEDS_HUMAN_RECORDED: <ISO-date> - <reason>` line instead. Edit
   `.ai/dispatch.tasks.md` to do this. The task 149 feature must include
   explicit `MAY edit`, `MUST NOT edit`, `Done criteria`, `Verification`, and
   `Halt conditions` sections.

   **MAY edit:**
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - generated ISSUE-<n> handoff/audit/log artifacts for the dispatch

   **MUST NOT edit:**
   - Rust source or tests
   - Cargo manifests or `Cargo.lock`
   - workflows
   - dispatch automation, guard, queue, scheduler, watcher, verification, or
     health/trend scripts
   - schemas, ADR files, architecture-lint rules/config, packet templates, or
     unrelated existing handoff/log artifacts
   - plugin runtime/discovery/loading code, command routing, shortcut
     execution, remapping/persistence/fatal policy, OS clipboard behavior,
     CAD/projection/CommandBus mutation, undo/dirty/save-load authority,
     camera/navigation behavior, camera math, viewport hit testing,
     face-pick policy, wheel zoom, right-button orbit math, middle-button pan
     math, focus-loss behavior, cursor-left behavior, mouse-wheel behavior,
     reset-camera behavior, or Zoom In/Out behavior

   **Done criteria:**
   - The audit records the pre-edit task-heading check for
     `^145\.|^146\.|^147\.|^148\.|^149\.`.
   - Queue/already-filed-task claims cite only the dispatcher-provided snapshot
     embedded in the issue body or an exact local artifact path copied from it;
     no live `gh`, browser, network, or GitHub API query is run by the
     sandboxed executor.
   - Each candidate class above has positive source references and falsifying
     searches for negative claims where practical.
   - Exactly one bounded FEATURE task 149 is appended with explicit `MAY edit`,
     `MUST NOT edit`, `Done criteria`, `Verification`, and `Halt conditions`,
     and it carries the copied Self-re-arm final-step requirement, or a
     source-grounded `NEEDS_HUMAN_RECORDED` record is written.
   - No implementation work for task 149 is done, and no other task is added.

   **Verification:**
   - `rg -n "^145\.|^146\.|^147\.|^148\.|^149\." .ai/dispatch.tasks.md` before
     edits and after edits
   - status-doc cross-check against `Status.md`, `HANDOFF.md`,
     `plans/BASELINE.md`, and `change.md`
   - candidate-class source greps recorded in the audit
   - `git diff --name-only`
   - `git diff --check`
   - `.\new-handoff.ps1 -Finalize -PacketPath <EXEC_PACKET> -DryRun`

   **Halt conditions:**
   - The executor cannot cite the dispatcher-provided GitHub-state snapshot
     without live `gh`, browser, network, or GitHub API access.
   - The audit would require editing a MUST-NOT path or implementing task 149.
   - More than one feature follow-up would be required to make the selected
     boundary coherent.
   - No bounded task 149 can be specified without crossing a policy or
     architecture boundary; record `NEEDS_HUMAN_RECORDED` instead of forcing a
     task.

   **Execution audit (ISSUE-402, 2026-06-15):**
   - Pre-edit task-heading check:
     `rg -n "^145\.|^146\.|^147\.|^148\.|^149\." .ai/dispatch.tasks.md`
     returned task 145 done at line 14101, task 146 done at line 14217, task
     147 done at line 14426, task 148 open at line 14548, and no task 149
     heading.
   - Dispatcher snapshot evidence: queue/already-filed-task claims use only
     the ISSUE-402 task packet's embedded dispatcher snapshot facts, generated
     by `Invoke-AiDispatchAuto.ps1` at `2026-06-15T05:12:54.9060513+03:00`
     for `RustCADs/RGE`: no open `ai-dispatch` issues before ISSUE-402 was
     created, no open failed autonomous issues, and autonomous issues already
     filed through closed ISSUE-401 / task 147. No `gh`, browser, network, or
     GitHub API command was run for these claims.
   - Status-doc cross-check:
     `rg -n "ISSUE-401|task 147|task 148|task 149|Zoom In|Zoom Out|double-click" .ai/dispatch.tasks.md Status.md HANDOFF.md plans/BASELINE.md change.md`
     confirmed `Status.md`, `HANDOFF.md`, `plans/BASELINE.md`, `change.md`,
     and this task brief all recorded ISSUE-401 / task 147 as done and task
     148 as the open post-Zoom-command audit before this edit.
   - Keybinding/remapping/preferences/fatal policy: shortcut execution and
     diagnostics remain centered on
     `crates/editor-ui/src/menus/registry.rs:248-321`,
     `crates/editor-ui/src/menus/shortcut.rs:230-293`,
     `crates/editor-egui-host/src/shortcut_conflicts.rs:11-177`,
     `crates/editor-egui-host/src/shortcut_help.rs:159-190`, and
     `crates/editor-egui-host/src/menu.rs:209-246`. The inverse search
     `rg -n "shortcut.*remap|remap.*shortcut|keymap|shortcut.*preference|shortcut.*preferences|preferences.*shortcut|shortcut.*persist|persist.*shortcut" crates/editor-ui/src/menus crates/editor-egui-host/src crates/editor-shell/src/lifecycle/accelerator.rs`
     returned no matches (exit 1). The fatal-policy search
     `rg -n "fatal|non-fatal" crates/editor-ui/src/menus crates/editor-egui-host/src/shortcut_conflicts.rs crates/editor-egui-host/src/shortcut_help.rs crates/editor-egui-host/src/menu.rs crates/editor-shell/src/lifecycle/accelerator.rs`
     found only the existing "host decides whether a conflict is fatal" comment
     and non-fatal diagnostic/persistence wording. Remapping, preferences, and
     fatal startup policy remain broader product/persistence policy.
   - Host-shell command routing: menu and palette commands still enqueue
     through `MenuCommandHandoff` in
     `crates/editor-egui-host/src/handoff.rs:96-127`, the shell drains at
     `crates/editor-shell/src/render_path.rs:366-371`, and
     `EditorShell::route_menu_command` owns File/Edit/Play/View/extension
     routing at `crates/editor-shell/src/render_path.rs:415-475`. Replacing
     the FIFO, route owner, or registry execution path crosses the forbidden
     command-routing surface and is deferred.
   - Real plugin command execution: extension activations still stop at the
     injected seam. `crates/editor-shell/src/lifecycle/extension_command.rs:4`
     explicitly excludes plugin discovery/loading/runtime execution, `:58-63`
     defines the injectable handler, and `:113-177` drains/captures
     shell-local extension commands. The inverse discovery/runtime search
     `rg -n "PluginRegistry|PluginDiscovery|plugin-discovery|plugin_discovery|load_plugin|plugin.*load|discover.*plugin|runtime.*plugin|PluginHost|Command::Plugin" crates/plugin-discovery/src crates/editor-shell/src crates/editor-ui/src/menus crates/editor-egui-host/src runtime`
     returned only the `rge-plugin-discovery` stub, extension seam, and
     `Command::Plugin` projection/test references. Real plugin execution still
     needs runtime/discovery/loading/capability policy.
   - OS/typed clipboard: Edit Cut/Copy/Paste still use shell-local
     `entity_clipboard` cloned legacy blobs in
     `crates/editor-shell/src/lifecycle/mod.rs:779-784` and
     `:1789-1845`, with menu predicates in
     `crates/editor-ui/src/menus/default_menu.rs:238-280`. The inverse
     searches
     `rg -n "arboard|copypasta|ClipboardProvider|SystemClipboard|set_clipboard|get_clipboard|clipboard_text|copy_text|OutputCommand::CopyText" Cargo.toml crates/editor-shell/src crates/editor-ui/src crates/editor-egui-host/src`
     and
     `rg -n "arboard|copypasta|ClipboardProvider|SystemClipboard|set_clipboard|get_clipboard|clipboard_text|copy_text|OutputCommand::CopyText" crates -g "Cargo.toml"`
     both returned no matches (exit 1). OS/system or typed clipboard semantics
     remain a separate policy/dependency boundary.
   - CAD/editor mutation: authoritative mutation remains broader than one
     follow-up. `crates/editor-actions/src/lib.rs:10` states editor mutation
     flows through `CommandBus::submit`; `crates/editor-actions/src/bus.rs:143`,
     `:216`, `:245`, `:277`, and `:284` own submit/undo/redo/save-mark/dirty
     state. Shell save/load authority is in
     `crates/editor-shell/src/lifecycle/open_request.rs:194-411`,
     `crates/editor-shell/src/lifecycle/save_request.rs:205-397`, and
     `crates/editor-shell/src/lifecycle/mod.rs:876-943`; CAD projection reads
     remain at `crates/cad-projection/src/render_adapter.rs:90` and selected
     CAD bounds at `crates/editor-shell/src/lifecycle/mod.rs:2084-2100`.
     A CAD mutation task would cross CommandBus, projection, undo/dirty, and
     save/load authority.
   - Camera/navigation: current source already covers the required local
     camera sequence. View/Home reset and View/PageUp/PageDown zoom route at
     `crates/editor-shell/src/render_path.rs:471-473`; reset, zoom, wheel,
     focus-loss, cursor-left, right-button, middle-button, and fallback mouse
     input all clear pending `ViewportLeftDoubleClick` state at
     `crates/editor-shell/src/lifecycle/mod.rs:2115`, `:2135`, `:2144`,
     `:2153`, `:2305`, `:2316`, `:2879`, `:2884`, `:2888`, `:2893`, and
     `:2896`. Focused tests cover valid frame-all / selected-CAD double-click
     behavior and stale reset cases at
     `crates/editor-shell/src/lifecycle/tests.rs:891-1082`,
     `:1100-1208`, `:1486-1540`, `:2001-2046`, and `:2155-2168`. The inverse
     search
     `rg -n "camera.*persist|persist.*camera|camera controller|frame_selected|frame selected|pointer capture|window grab" crates/editor-shell/src/lifecycle/mod.rs crates/editor-shell/src/lifecycle/viewport_navigation.rs crates/editor-shell/src/lifecycle/tests.rs crates/editor-shell/src/render_path.rs`
     returned no matches (exit 1). Further camera work would require a new
     product/architecture decision rather than a single bounded stale-state
     follow-up.
   - Selected outcome: no bounded in-policy FEATURE task 149 can be named
     without crossing policy, runtime, routing, clipboard, CAD mutation, or
     broader camera architecture boundaries. No implementation work for task
     149 was performed and no task 149 or task 150 was appended.

149. **[DONE 2026-06-15 via ISSUE-403] Read-only preflight: smallest-safe CAD/CommandBus first-mutation boundary (recommendation only).**
   Perform a docs/source-read-only Phase 9 preflight of the CAD/CommandBus
   editor-mutation boundary that triggered the prior `NEEDS_HUMAN_RECORDED`.
   Identify the single smallest coherent first real CAD/editor mutation that
   could route through the existing `CommandBus` authority, and produce a
   concrete recommendation for human approval. This is analysis ONLY: no source,
   test, or behavior change. Use current local source reads plus the
   dispatcher-provided GitHub-state snapshot from the auto-created issue body for
   queue/already-filed evidence; do not call `gh` or the network from inside the
   executor sandbox.

   **Self-re-arm (final step, required) — RECORD A RECOMMENDATION AS NEEDS_HUMAN; DO NOT APPEND AN IMPLEMENTATION TASK.**
   This task is explicitly gated: it MUST NOT append task 150 or any other
   implementation/feature task, and MUST NOT self-re-arm with a follow-up task.
   Instead, as its final step, append to "## Tasks" exactly one
   `NEEDS_HUMAN_RECORDED: <ISO-date> - CAD/CommandBus first-mutation recommendation`
   marker line, immediately followed by a "Recommendation for human approval"
   block containing all five of:
   - **Proposed mutation** — the single smallest coherent first CAD/editor
     mutation, and the exact `CommandBus` / `Action` path it routes through.
   - **Exact edit surface** — the precise files / functions / types a future
     implementation would touch, and which it must not.
   - **Risks** — projection invalidation, undo/dirty, save-mark, save/load
     authority, and any cross-cutting concerns.
   - **Verification** — the test/verify plan a future implementation would run.
   - **Why smallest coherent boundary** — why this is the minimal in-policy first
     step vs. the alternatives, and the exact human product/architecture decision
     that must be approved before any code ships.
   Editing `.ai/dispatch.tasks.md` for this purpose is in `MAY edit`. The
   autonomous driver detects the marker, files a `needs-human` review issue, and
   pauses for human approval — no code is authored or shipped from this task.

   **Context snapshot:**
   - Tasks 143-148 cleaned up stale viewport left-double-click state across
     focus-loss, mouse-wheel, reset-camera, and zoom commands; task 148's audit
     recorded `NEEDS_HUMAN` (no bounded in-policy task remained).
   - An operator-requested Codex preflight flagged that a CAD/editor mutation
     path may already exist via `CommandBus::submit`
     (`crates/editor-actions/src/bus.rs`), with CAD projection read-side in the
     shell via `render_mesh_for` / `selected_cad_scene_bounds`
     (`crates/cad-projection/src/render_adapter.rs`,
     `crates/editor-shell/src/lifecycle/mod.rs`). The preflight must confirm or
     falsify this and scope the smallest first mutation accordingly.

   **MAY edit:**
   - `.ai/dispatch.tasks.md`
   - `Status.md`
   - `HANDOFF.md`
   - `plans/BASELINE.md`
   - `change.md`
   - generated ISSUE-<n> handoff/audit/log artifacts for this dispatch only

   **MUST NOT edit:**
   - Rust source or tests
   - Cargo manifests or `Cargo.lock`
   - workflows; dispatch automation / guard / queue / scheduler / watcher /
     verification / health scripts
   - schemas, ADR files, architecture-lint rules/config, packet templates
   - command routing, plugin runtime/discovery/loading, OS clipboard behavior,
     shortcut remapping/fatal policy, camera/navigation behavior,
     CAD/projection/CommandBus mutation, undo/dirty/save-load authority, or
     viewport hit testing

   **Done criteria:**
   - The audit records the pre-edit task-heading check for
     `^148\.|^149\.|^150\.` and the prior `NEEDS_HUMAN_RECORDED` marker state.
   - The audit answers a 5-question CAD/CommandBus boundary preflight: (Q1) the
     current `CommandBus` public contract and action context; (Q2) where CAD
     projection, selected-CAD bounds, open/save/load, undo, dirty, and save-mark
     authority currently live; (Q3) which candidate first mutations are possible
     and which require wider architecture; (Q4) the exact human decision needed
     before implementation; (Q5) the single smallest coherent recommended first
     mutation.
   - Exactly one `NEEDS_HUMAN_RECORDED:` marker plus the five-part Recommendation
     block is appended. NO task 150 or any other task is appended, and NO
     implementation work is done.
   - Negative claims include falsifying `rg` searches where practical.

   **Verification:**
   - `rg -n "^148\.|^149\.|^150\.|NEEDS_HUMAN_RECORDED" .ai/dispatch.tasks.md`
     before and after edits
   - source greps for `CommandBus::submit`, `submit_action`, `undo_command`,
     `redo_command`, `mark_saved_command`, `render_mesh_for`,
     `selected_cad_scene_bounds`, `replace_world`, `handle_open_request`,
     `handle_save_request`
   - status-doc cross-check against `Status.md`, `HANDOFF.md`,
     `plans/BASELINE.md`, and `change.md`
   - `git diff --name-only`
   - `git diff --check`

   **Halt conditions:**
   - Any source, test, Cargo, workflow, script, schema, ADR, lint, routing,
     plugin, clipboard, shortcut-policy, camera, CAD, projection, undo/dirty, or
     save/load edit is required.
   - The preflight cannot name a single smallest coherent first mutation without
     more than one architecture/product decision — in that case the
     Recommendation must say so explicitly and still record `NEEDS_HUMAN`
     (never append task 150).

RESOLVED 2026-06-15 (approved via task 150) - prior NEEDS_HUMAN CAD/CommandBus first-mutation recommendation, kept for provenance:
Recommendation for human approval
- Proposed mutation: approve a future bus-routed "add one CAD cuboid primitive" command as the first real CAD mutation. The command should submit one reversible CAD action through the CommandBus authority after approving a narrow editor-owned command context, because the current `Action::apply` / `revert` contract receives only `&mut rge_kernel_ecs::World` while the live CAD state is held separately in `EditorShell` as `cad_world`, `cad_graph`, `projection`, and `cad_entity`.
- Exact edit surface: future implementation should be limited to the CommandBus context/API surface in `crates/editor-actions/src/action.rs`, `crates/editor-actions/src/bus.rs`, and dependent action tests; the shell adapter/action and command wrapper in `crates/editor-shell/src/lifecycle/commands.rs` and `crates/editor-shell/src/lifecycle/mod.rs`; the render refresh path needed to display the newly projected mesh in `crates/editor-shell/src/render_path.rs`; and focused tests under the same crates. It should use existing `CadGraph::begin_operation` / `graph_mut` / `commit`, `OperatorGraph::add_operator` / `set_root`, and `CadProjection::spawn_brep_entity` / `tick` APIs rather than editing CAD core or projection APIs unless a fresh human-approved gap is found.
- Risks: the approval must decide that the bus may grow an editor command context beyond the current World-only action contract. The future action must keep graph commit, projection tick/cache invalidation, render-mesh upload, undo/revert, dirty-state movement, save-mark clearing, and save/load/reset semantics coherent; failed apply or revert must not leave mismatched `CadGraph`, `CadProjection`, `cad_world`, selection, or GPU render state.
- Verification: future implementation should include editor-action unit coverage for the widened action context and bus undo/redo/dirty/save-mark behavior, editor-shell lifecycle tests proving add-cuboid submit/undo/redo round-trips CAD graph/projection/world/render state, save/load dirty-state tests proving successful save marks the new CAD mutation saved, and focused render/projection tests showing the added primitive is frameable/selectable via existing `render_mesh_for` and selected-CAD bounds paths. Run the affected crate tests plus repository verification required by that future task.
- Why smallest coherent boundary: adding one primitive is smaller than deleting selected CAD geometry, transforming selected CAD geometry, editing primitive parameters, clearing or replacing the CAD graph, or treating open/new/save transitions as the first mutation. It needs only creation, commit, projection, display, undo, dirty, and save-mark semantics; the alternatives add selection remapping, root/dependency deletion policy, transform/product UX, inspector/parameter identity, or document lifecycle authority before the bus/CAD context question is settled.

150. **Add one CAD cuboid primitive to a new/empty CAD scene (first real CAD mutation; via the CAD graph checkpoint path, NOT the CommandBus action contract).**
   Add a bounded shell command/entry point that inserts a single cuboid primitive
   into the editor's CAD graph and makes it render and be selectable, using ONLY
   existing public APIs. Do NOT widen the `Action`/`CommandBus` contract or move
   CAD-graph mutation onto the bus — that is the explicitly deferred "CAD-state
   into ECS" architecture decision (`crates/editor-shell/src/lifecycle/mod.rs`
   ~:669) and is out of scope.

   Mechanism (existing APIs only): `CadGraph::begin_operation` ->
   `OperatorGraph::add_operator(OperatorNode::Cuboid(CuboidOp::default()))` ->
   `set_root` (empty/new-scene case only; see scene semantics) ->
   `CadGraph::commit` -> `CadProjection::tick` (and `spawn_brep_entity` if the
   entity is not already present) so the primitive projects to a `BRepHandle`
   mesh in `cad_world` and the next render frame displays it.

   **Reversibility scope (NO global-undo claim):**
   Capture `pre_add_head = CadGraph::head()` before the add. After the cuboid is
   committed and projected, demonstrate test-only reversibility with
   `CadGraph::restore_to(pre_add_head)` plus existing projection cleanup
   (`despawn_brep_entity` if an entity was spawned), then re-tick/assert no stale
   mesh/entity remains. Use `CadGraph::rollback` only to test aborting an
   in-progress uncommitted operation, not as undo for the committed add. Do NOT
   claim or wire CommandBus undo, global Ctrl+Z, or any user-facing undo — no
   CAD-specific rollback entry point exists in the shell and adding one is out of
   scope.

   **Scene semantics (hard gate):**
   Add the cuboid to a clearly empty/new CAD scene, or preserve existing state
   using already-existing APIs only. If adding it would replace an existing graph
   root, or needs multi-root / composition / connect behavior to coexist with
   existing CAD content -> HALT with `NEEDS_HUMAN` (separate product/architecture
   decision, not this task).

   **Self-re-arm (final step, required):** after the implementation is complete
   and verified, follow the Self-re-arm protocol in this file's header: append
   exactly one bounded next-task-source AUDIT task as task 151 (mirror the audit
   STRUCTURE, not any "no successor" rule), or, if no bounded in-policy next task
   exists, append a `NEEDS_HUMAN_RECORDED: <ISO-date> - <reason>` line. Copy this
   Self-re-arm requirement into the audit task you author. Edit
   `.ai/dispatch.tasks.md` to do this.

   **MAY edit:**
   - `crates/editor-shell/src/lifecycle/commands.rs`
   - `crates/editor-shell/src/lifecycle/mod.rs`
   - `crates/editor-shell/src/lifecycle/tests.rs`
   - `crates/editor-shell/src/render_path.rs` — ONLY if a minimal existing-path
     refresh hook is needed to display the single new mesh (no new render
     architecture)
   - `.ai/dispatch.tasks.md`, `Status.md`, `HANDOFF.md`, `plans/BASELINE.md`, `change.md`
   - generated ISSUE-<n> handoff/audit/log artifacts for this dispatch only

   **MUST NOT edit:**
   - `crates/editor-actions/**` — the `Action` trait, `CommandBus`, or submit
     signature MUST NOT change (the deferred CAD-into-ECS boundary)
   - `crates/cad-core/**`, `crates/cad-projection/**` — existing public APIs only
   - `crates/editor-ui/**`, `crates/editor-egui-host/**`, plugin runtime/discovery/loading
   - Cargo manifests / `Cargo.lock`, workflows, schemas, ADR files,
     architecture-lint config, packet templates, dispatch automation scripts
   - command routing/menus/shortcuts, OS clipboard, camera/navigation math,
     viewport hit-testing, save/load authority

   **Done criteria:**
   - A bounded shell entry point adds exactly one cuboid to an empty/new CAD
     scene: `begin_operation` -> add `CuboidOp` -> `set_root` -> `commit`, then
     `projection.tick` (+ `spawn_brep_entity` if needed) so the cuboid has a
     `BRepHandle` mesh.
   - The added cuboid renders and is frameable/selectable via the EXISTING
     `render_mesh_for` / selected-CAD-bounds single-entity paths (no new render
     or multi-entity architecture).
   - A focused test exercises committed-add reversibility via
     `CadGraph::restore_to(pre_add_head)` + projection cleanup
     (`despawn_brep_entity` if spawned) + re-tick, asserting no stale mesh/entity
     remains; a separate test may exercise `CadGraph::rollback` for an aborted
     uncommitted operation. No CommandBus/global-undo claim or wiring.
   - No change to the `Action`/`CommandBus` contract; CAD mutation stays off the bus.
   - Focused headless lifecycle/CAD tests cover add + restore_to round-trip. Apply
     the per-binary `test_lock::guard()` GPU-serialization pattern ONLY if a test
     constructs real `wgpu` resources (existing lifecycle tests are headless).

   **Verification:**
   - `cargo test -p rge-editor-shell --lib` (the new cuboid add/restore_to tests)
   - `cargo check -p rge-editor-shell --lib`
   - `cargo +nightly fmt --all -- --check`
   - `rg -n "CuboidOp|begin_operation|spawn_brep_entity|restore_to" crates/editor-shell/src`
   - `rg -n "fn undo_command|CommandBus|Action " crates/editor-actions/src` expected UNCHANGED (no edits)
   - `git diff --name-only`; `git diff --check`

   **Halt conditions (any -> record NEEDS_HUMAN, do not force):**
   - Adding or reverting the cuboid cannot be done without widening the
     `Action`/`CommandBus` contract or moving CAD-graph mutation onto the bus.
   - Adding the cuboid would replace an existing graph root, or needs multi-root /
     composition / connect behavior to coexist with existing CAD content.
   - Making the cuboid render or selectable requires broader render-path or
     multi-entity architecture, or changes to `cad-core` / `cad-projection` /
     camera math / viewport hit-testing.
   - More than one coherent implementation follow-up is required.

151. **Audit first-CAD-cuboid add aftermath for the next bounded feature source (source/docs-read-only; no implementation).**
   This is a SOURCE AUDIT ONLY: read current source/tests/docs, compare candidate
   follow-up classes, and choose exactly one smallest bounded FEATURE task that
   can safely follow ISSUE-405, or record `NEEDS_HUMAN_RECORDED` if every
   candidate crosses an architecture/product boundary. Do not implement the
   chosen feature during this audit.

   **Context snapshot:**
   - Task 150 shipped the first bounded editor-shell CAD add entry point:
     `EditorShell::add_cad_cuboid_to_empty_scene` creates a fresh `CadGraph`,
     adds `OperatorNode::Cuboid(CuboidOp::default())`, sets it as root, commits,
     spawns one `BRepHandle` entity through `CadProjection::spawn_brep_entity`,
     and ticks projection so existing `render_mesh_for`, scene-bounds, and
     selected-CAD-bounds paths can see the mesh.
   - The implementation deliberately did not touch `Action`, `CommandBus`,
     menu routing, shortcuts, save/load, dirty state, UI/host crates, CAD core,
     CAD projection, Cargo metadata, workflows, schemas, or dispatch automation.
   - Focused lifecycle tests cover add success, duplicate/partial/render-content
     rejection, renderability/frameability/selectability through existing paths,
     and test-only cleanup with `CadGraph::restore_to(pre_add_head)` plus
     `CadProjection::despawn_brep_entity`.
   - The auto-created issue body will include the dispatcher GitHub-state
     snapshot. The audit must use that embedded snapshot, or an exact local
     artifact/read path to it, for GitHub queue/already-filed-task evidence.
     Do not call `gh`, browser, network, or GitHub APIs from inside the executor
     sandbox for those claims.

   **Candidate classes to compare:**
   - A minimal render-path refresh for a cuboid added after Phase-1-only
     editor window initialization, if current source proves the headless add
     path cannot reach already-initialized GPU mesh upload without a small
     existing-path hook.
   - A bounded lifecycle/source cleanup around CAD restore semantics only if it
     can be specified without editing `crates/cad-core/**` or weakening the
     task-150 no-global-undo boundary; otherwise record it as needing human
     product/architecture guidance.
   - A strictly headless editor-shell follow-up that improves CAD add
     observability or test coverage without adding menus, shortcuts,
     CommandBus authority, save/load behavior, multi-root composition, deletion,
     transforms, or parameter editing.

   **Self-re-arm (final step, required):** After the audit is complete and
   verified, follow the Self-re-arm protocol in this file's header: append
   exactly one bounded FEATURE task as task 152, carrying this same
   Self-re-arm final-step requirement so the chain continues, or, if no bounded
   in-policy next task exists, append a single
   `NEEDS_HUMAN_RECORDED: <ISO-date> - <reason>` line instead. Edit
   `.ai/dispatch.tasks.md` to do this. The task 152 feature must include
   explicit `MAY edit`, `MUST NOT edit`, `Done criteria`, `Verification`, and
   `Halt conditions` sections.

   **MAY edit:**
   - `.ai/dispatch.tasks.md`
   - generated ISSUE-<n> handoff/audit/log artifacts for the dispatch

   **MUST NOT edit:**
   - Rust source or tests
   - Cargo manifests or `Cargo.lock`
   - workflows
   - dispatch automation, guard, queue, scheduler, watcher, verification, or
     health/trend scripts
   - schemas, ADR files, architecture-lint rules/config, packet templates, or
     unrelated existing handoff/log artifacts
   - plugin runtime/discovery/loading code, command routing, shortcut
     execution, remapping/persistence/fatal policy, OS clipboard behavior,
     CAD/projection/CommandBus mutation, undo/dirty/save-load authority,
     camera/navigation behavior, camera math, viewport hit testing,
     face-pick policy, render architecture, or GPU resource lifetime behavior

   **Done criteria:**
   - The audit records the pre-edit task-heading check for `^149\.|^150\.|^151\.|^152\.`
     and proves no task 152 heading existed before self-rearm.
   - Queue/already-filed-task claims cite only the dispatcher-provided snapshot
     embedded in the issue body or an exact local artifact path copied from it;
     no live `gh`, browser, network, or GitHub API query is run by the sandboxed
     executor.
   - Each candidate class above has positive source references and falsifying
     searches for negative claims where practical.
   - Exactly one bounded FEATURE task 152 is appended with explicit `MAY edit`,
     `MUST NOT edit`, `Done criteria`, `Verification`, and `Halt conditions`,
     and it carries the copied Self-rearm final-step requirement, or a
     source-grounded `NEEDS_HUMAN_RECORDED` record is written.
   - No implementation work for task 152 is done, and no other task is added.

   **Verification:**
   - `rg -n "^149\.|^150\.|^151\.|^152\." .ai/dispatch.tasks.md` before edits
     and after edits
   - candidate-class source greps recorded in the audit
   - `rg -n "add_cad_cuboid_to_empty_scene|CuboidOp|begin_operation|spawn_brep_entity|restore_to|despawn_brep_entity" crates/editor-shell/src`
   - `git diff --name-only`
   - `git diff --check`
   - `.\new-handoff.ps1 -Finalize -PacketPath <EXEC_PACKET> -DryRun`

   **Halt conditions:**
   - The executor cannot cite the dispatcher-provided GitHub-state snapshot
     without live `gh`, browser, network, or GitHub API access.
   - The audit would require editing a MUST-NOT path or implementing task 152.
   - More than one feature follow-up would be required to make the selected
     boundary coherent.
   - No bounded task 152 can be specified without crossing a policy or
     architecture boundary; record `NEEDS_HUMAN_RECORDED` instead of forcing a
     task.
