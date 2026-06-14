# RGE — Performance Baselines

## Phase 9 editor-usability and dispatch-advisory closure

**2026-06-14 update 41:** After ISSUE-391 published task 137 as `8fe95bc`,
the queue had no open `ai-dispatch` or `ai-dispatch-failed` issue, no live
issue claim directory, and the task brief was exhausted. The brief is re-armed
with task 138, a docs/source-read-only Phase 9 audit after cursor-left
viewport drag cancellation. Task 138 must use the dispatcher-provided
GitHub-state snapshot instead of live `gh`, compare remaining keybinding/remap,
host-shell routing, plugin execution, OS/typed clipboard, CAD/CommandBus
mutation, and camera/navigation follow-up candidates from current local source,
and append exactly one bounded task 139 or record source-grounded
`NEEDS_HUMAN`. ISSUE-391 passed full canonical verification and Codex control
before publish.

**2026-06-14 update 40:** ISSUE-391 implemented task 137. The lifecycle
`WindowEvent::CursorLeft` path now clears stale cursor position, resets the
viewport left-double-click tracker, cancels active viewport orbit/pan drags,
and releases the viewport drag cursor grab only when at least one viewport drag
was active before cursor leave. Focus loss shares the same active-drag
cancellation helper, preserving ISSUE-389 focus-loss behavior. Focused tests
cover active orbit, active pan, combined active drags with one release,
no-active no-release, cursor-position clearing, and stale double-click reset.
Task 137 is marked done and no task 138 was appended.

**2026-06-14 update 39:** ISSUE-390 completed task 136 as a
docs/source-read-only selection audit. The audit used only the embedded
dispatcher snapshot from the ISSUE-390 TASK packet for GitHub queue evidence;
no `gh` or network command was run. Source checks compared the remaining
keybinding/remap/preferences/fatal-policy, host-shell routing, plugin
execution, OS/typed clipboard, CAD/CommandBus mutation, and camera/navigation
follow-up classes. The smallest source-safe remaining implementation boundary
is task 137: handle cursor-leave lifecycle by clearing stale cursor position,
resetting pending viewport double-click state, cancelling active viewport
orbit/pan drags, and releasing the viewport drag cursor grab through the
existing `editor-shell` lifecycle helpers. Broader remapping, host-shell route
replacement, real plugin runtime/discovery/loading, OS clipboard, and
CAD/CommandBus mutation remain deferred. Task 136 did not implement task 137
and did not append task 138.

**2026-06-14 update 38:** After ISSUE-389 published task 135 as `82b2e95`,
the queue had no open `ai-dispatch` or `ai-dispatch-failed` issue, no live
issue claim directory, and the task brief was exhausted. The brief is re-armed
with task 136, a docs/source-read-only Phase 9 audit after focus-loss viewport
drag cancellation. Task 136 must use the dispatcher-provided GitHub-state
snapshot instead of live `gh`, compare remaining keybinding/remap,
host-shell routing, plugin execution, OS/typed clipboard, CAD/CommandBus
mutation, and camera/navigation follow-up candidates from current local source,
and append exactly one bounded task 137 or record source-grounded
`NEEDS_HUMAN`. The stale/stalled-run guard is active: ISSUE-389's first
planning attempt stalled, was archived as `ISSUE-389.attempt1`, and the retry
completed/published.

**2026-06-14 update 37:** ISSUE-389 implemented task 135. The lifecycle
`WindowEvent::Focused(false)` path now cancels active viewport orbit and pan
drags and releases the viewport drag cursor grab through the existing helper
only when a drag was active before focus loss. Focus gain is preserved as a
no-op for viewport drag state. Focused tests cover active orbit, active pan,
combined active drags with one release, no-active no-release, and focus-gain
preservation. Task 135 is marked done and no task 136 was appended.

**2026-06-14 update 36:** ISSUE-388 completed task 134 as a
docs/source-read-only selection audit. The audit used only the embedded
dispatcher snapshot from the ISSUE-388 TASK packet for GitHub queue evidence;
no `gh` or network command was run. Source checks compared the remaining
keybinding/remap/preferences/fatal-policy, host-shell routing, plugin
execution, OS/typed clipboard, CAD/CommandBus mutation, and camera/navigation
follow-up classes. The smallest source-safe remaining implementation boundary
is task 135: cancel active viewport orbit/pan drags and release the viewport
drag cursor grab when the window loses focus, limited to the existing
`editor-shell` lifecycle drag/cursor-grab surface. Broader remapping,
host-shell route replacement, real plugin runtime/discovery/loading, OS
clipboard, and CAD/CommandBus mutation remain deferred. Task 134 did not
implement task 135 and did not append task 136.

**2026-06-14 update 35:** After ISSUE-387 published task 133 as `53644c2`,
the queue had no open `ai-dispatch` or `ai-dispatch-failed` issue and the task
brief was exhausted. The brief is re-armed with task 134, a
docs/source-read-only Phase 9 audit after viewport cursor grab. Task 134 must
use the dispatcher-provided GitHub-state snapshot instead of live `gh`, compare
remaining keybinding/remap, host-shell routing, plugin execution, OS/typed
clipboard, CAD/CommandBus mutation, and camera/navigation follow-up candidates
from current local source, and append exactly one bounded task 135 or record
source-grounded `NEEDS_HUMAN`. The stale-claim mechanism is already active
from `fe6dbb4`; this update fixes the separate idle condition where no task
remained to select.

**2026-06-14 update 34:** ISSUE-387 implemented task 133. `editor-shell`
now attempts `CursorGrabMode::Confined` through the existing optional winit
`Window` when a valid viewport right-button orbit or middle-button pan drag
starts, logs cursor-grab failures non-fatally, and releases with
`CursorGrabMode::None` only after the final active viewport drag stops.
Headless/no-window shells keep the existing drag behavior without an OS cursor
grab. Focused lifecycle tests cover orbit start gating, pan start gating,
failed/no-window grab behavior, and right/middle release ordering while both
drags are active. Existing camera math, viewport hit testing, wheel zoom, face
picking, left-double-click frame-all, View menu commands, command routing,
shortcuts, Cargo metadata, workflows, automation, and non-editor-shell
subsystems were not changed. Task 133 is marked done and no task 134 was
appended.

**2026-06-14 update 33:** ISSUE-386 completed task 132 as a
docs/source-read-only selection audit. The audit used only the embedded
dispatcher snapshot from the ISSUE-386 TASK packet for GitHub queue evidence;
no `gh` or network command was run. Source checks confirmed that shortcut
conflict diagnostics now reach Shortcut Conflicts, Keyboard Shortcuts help,
command-palette rows, and main-menu rows. The remaining bounded camera gap is
cursor grab/release for the already-existing viewport-only right-button orbit
and middle-button pan drags: the drag start/stop methods and tests exist, while
`CursorGrabMode` / `set_cursor_grab` / pointer-capture searches have no
matches. Task 133 is appended for that narrow editor-shell follow-up. Broader
remapping/preferences/fatal policy, route replacement, real plugin execution,
OS/typed clipboard, CAD/CommandBus mutation, and camera math/persistence policy
remain deferred. Task 132 did not implement task 133 or append task 134.

**2026-06-14 update 32:** ISSUE-385 was manually salvaged and published as
`187e5bc` after the queue preserved the run as blocked. The remaining blocker
was the Codex executor sandbox failing `cargo deny` because it could not lock
the Cargo advisory DB; an out-of-sandbox canonical verification run passed all
gates. #385 is closed and the queue is unblocked. The task brief is now
re-armed with task 132: a docs/source-read-only audit after main-menu conflict
annotation. Task 132 must use the dispatcher-provided GitHub-state snapshot
instead of live `gh`, compare the remaining keybinding/remap, host-shell
execution, plugin execution, OS/typed clipboard, CAD/CommandBus mutation, and
camera/navigation follow-up candidates, and append exactly one bounded task 133
or record source-grounded `NEEDS_HUMAN`.

**2026-06-14 update 31:** ISSUE-385 implemented task 131. The
`editor-egui-host` main-menu presentation now derives annotated rows from the
already-projected `ProjectedMainMenu.conflicts` data, copying ordered peer
entry ids only when an enabled menu item's displayed shortcut exactly matches a
projected conflict shortcut. The annotation is non-command informational text
beside the existing `menu_item` response; it does not change menu order, labels,
shortcut text, disabled behavior, command enqueueing, command-palette
projection/activation, shortcut-help rows, Shortcut Conflicts rows, shortcut
execution, routing, remapping/persistence/fatal policy, plugin runtime,
OS/typed clipboard, CAD/CommandBus mutation, save/load, or camera behavior.
Task 131 is marked done and no task 132 was appended.

**2026-06-14 update 30:** ISSUE-384 completed task 130 as a
docs/source-read-only selection audit. The audit used only the embedded
dispatcher snapshot from the TASK packet for GitHub queue evidence; no `gh` or
network command was run. Source checks confirmed that conflict data now reaches
Shortcut Conflicts, Keyboard Shortcuts help, and command-palette rows, while
the main-menu `menu_item` presentation still receives only enabled/label/
shortcut. The selected task 131 is a narrow host-local diagnostic: annotate
main-menu items whose displayed shortcut is conflicted from existing
`ProjectedMainMenu.conflicts`, preserving menu-click behavior, command-palette
activation, shortcut execution, routing, remapping/persistence/fatal policy,
plugin runtime, OS/typed clipboard, CAD/CommandBus mutation, and camera
behavior. Task 130 did not implement task 131 or append task 132.

**2026-06-14 update 29:** After ISSUE-383 published task 129 as
`64061a5`, the task brief was exhausted again. The queue is empty and
unblocked, so `.ai/dispatch.tasks.md` is re-armed with task 130: a
docs/source-read-only audit after command-palette conflict annotation. Task 130
must use the dispatcher-provided GitHub-state snapshot instead of live `gh`,
compare current keybinding/remap, host-shell execution, plugin execution,
OS/typed clipboard, CAD/CommandBus mutation, and camera/navigation follow-up
candidates, and append exactly one bounded task 131 or record source-grounded
`NEEDS_HUMAN`. No implementation work is authorized in task 130.

**2026-06-14 update 28:** ISSUE-383 implemented task 129. The
command-palette projection now exposes ordered conflict peer entry ids on
enabled rows whose displayed shortcut exactly matches an existing
`ProjectedMainMenu.conflicts` shortcut, copying the ids directly from the
matching `ProjectedShortcutConflict.entries` vector. The palette renders those
ids as informational row text. Unconflicted enabled rows and disabled rows do
not expose conflict detail, and the row annotation does not change filtering,
fuzzy scoring, pinned/recent ordering, selection, Enter/mouse activation,
Pin/Unpin, search focus, Shortcut Help, Shortcut Conflicts, shortcut execution,
menu clicks, routing, remapping/persistence/fatal policy, plugin runtime,
OS/typed clipboard, CAD/CommandBus mutation, save/load, or camera behavior.
Task 129 is marked done and no task 130 was appended.

**2026-06-14 update 27:** ISSUE-382 completed task 128 as a
docs/source-read-only selection audit. Dispatcher queue evidence came only from
`Get-Content -LiteralPath '.ai\dispatch-ISSUE-382\codex.plan.rev0.log' | Select-Object -Skip 40 -First 205`:
the snapshot was generated at `2026-06-14T02:40:09.4240537+03:00`, showed no
open `ai-dispatch`, no open failed autonomous issues, and already-filed
autonomous issues through closed #381; no `gh` or network command was run.
Current source confirms the narrow remaining diagnostic gap is in the
command-palette projection: shortcut-help rows now expose conflict peer ids, but
`ProjectedCommandPaletteEntry` still has no conflict state. The selected task
129 is to annotate command-palette shortcut conflicts from existing
`ProjectedMainMenu.conflicts` only, preserving palette activation, shortcut
execution, menu clicks, routing, remapping/persistence/fatal policy, plugin
runtime, OS/typed clipboard, CAD/CommandBus mutation, and camera behavior. Task
128 did not implement task 129 and did not append task 130.

**2026-06-14 update 26:** ISSUE-381 published task 127 as `3b817b7`.
Keyboard Shortcuts help now exposes ordered conflict peer entry ids from
`ProjectedMainMenu.conflicts.entries` for enabled conflicted rows, while
disabled rows stay disabled with no peer detail and the existing State labels
remain distinct. No routing, shortcut execution, menu-click,
command-palette activation, remapping/persistence, plugin runtime,
OS/typed clipboard, CAD/CommandBus mutation, or camera behavior changed. The
brief is re-armed with task 128, a docs/source-read-only audit that must
include raw dispatcher-snapshot evidence in its TASK packet to avoid the #380
Rule 8 failure mode, then select exactly one bounded implementation task 129 or
record source-grounded `NEEDS_HUMAN`.

**2026-06-14 update 25:** ISSUE-380 failed before publish while trying to run
task 126: the first attempt stalled during execution, then the retry halted at
plan-gate revision 1 because the TASK packet did not make every negative
current-state premise verifiable from embedded local evidence. Manual salvage
completed the docs/source-read audit using the ISSUE-380 dispatcher snapshot and
local source reads. The selected task 127 is a narrow host diagnostic:
Keyboard Shortcuts help should expose the ordered peer entry ids already carried
by `ProjectedMainMenu.conflicts.entries` for enabled conflicted rows. The slice
is limited to `crates/editor-egui-host/src/shortcut_help.rs` plus status docs;
routing/execution, remapping/persistence/fatal policy, plugin runtime,
OS/typed clipboard, CAD/CommandBus mutation, and camera work remain deferred.

**2026-06-14 update 24:** After ISSUE-379 published task 125 as `214a217`,
the autonomous selector found the task brief exhausted again. The queue is
empty and unblocked, so the brief is re-armed with task 126: a
docs/source-read-only Phase 9 audit after shortcut conflict execution and help
annotation. Task 126 must use the dispatcher-provided GitHub-state snapshot
instead of live `gh`, compare the remaining keybinding/remap, host-shell
execution, plugin execution, OS/typed clipboard, CAD/CommandBus mutation, and
camera/navigation candidate classes from current source, and append exactly one
bounded implementation task 127 or record source-grounded `NEEDS_HUMAN`.

**2026-06-14 update 23:** ISSUE-379 implemented task 125. The host-local
Keyboard Shortcuts help surface now marks enabled rows whose displayed shortcut
matches `ProjectedMainMenu.conflicts` as `Conflicted` in the existing State
column, while unconflicted enabled rows still show `Enabled` and ordinary
disabled rows still show `Disabled`. The annotation is sourced only from the
already-projected main-menu conflict data and remains informational: shortcut
execution, menu clicks, command-palette activation, shortcut-conflict
diagnostics, remapping/persistence, command routing, plugin runtime,
OS/typed clipboard, CAD/editor mutation, and camera/navigation behavior are
unchanged. Focused host tests passed for shortcut-help conflict states and the
existing conflict diagnostics surface.

**2026-06-13 update 22:** ISSUE-378 completed task 124 as a
docs/source-read-only selection audit. The embedded dispatcher snapshot was the
only GitHub evidence used and reported no open queue issue, no open failed
autonomous issue, and no already-filed task 125; no `gh` or network command was
run. Current source checks confirmed conflicted shortcuts are now
non-executable through `ResolveResult::enabled_command_for_shortcut` and that
`ProjectedMainMenu.conflicts` already feeds `editor-egui-host`. The selected
task 125 is a narrow host diagnostic: annotate conflicted shortcuts in the
Keyboard Shortcuts help State column by editing
`crates/editor-egui-host/src/shortcut_help.rs` only. Host-shell route
replacement, real plugin runtime/discovery/loading, OS/typed clipboard,
authoritative CAD/editor mutation, and further camera/navigation policy remain
deferred.

**2026-06-13 update 21:** ISSUE-377 published task 123 as `540d16e`.
Conflicted shortcuts remain diagnostic-visible but no longer execute through
`ResolveResult::enabled_command_for_shortcut`; first-winner lookup remains
available for display/introspection. The queue is empty and unblocked, and the
brief is re-armed with task 124, a docs/source-read-only audit that must use the
dispatcher-provided GitHub-state snapshot instead of live `gh`, compare the
remaining Phase 9 editor-usability candidate classes, and append exactly one
bounded implementation task 125 or record source-grounded `NEEDS_HUMAN`. The
standing Human=Codex delegation is explicit for choosing the smallest policy
boundary; task 124 must not implement task 125 or edit Rust/Cargo/workflows/
automation.

**2026-06-13 update 17:** ISSUE-375 implements task 121: viewport-only
left-double-click frame-all in `editor-shell`. The gesture is handled inside
the existing lifecycle mouse path with a private `viewport_navigation` detector
(500 ms / 6 px thresholds) and calls the existing `reset_camera()` path on the
qualifying second left press over the Viewport tab body. Whole-scene bounds and
empty-scene fallback remain owned by `current_scene_bounds()` +
`isometric_camera_for_bounds()` / `EditorCameraState::default()`. Single-click
face-pick, wheel zoom, right-button orbit, middle-button pan, and View
menu/Home reset routing remain unchanged; no command/menu/accelerator,
host/UI, Cargo, CAD/projection/action, plugin runtime, clipboard, undo/dirty,
render-path, persistence, pointer-capture, or generalized input surface changed.

**2026-06-13 update 18:** task 121 is published as `9720a10`, #375 is closed,
and the brief is re-armed with task 122: a docs/source-read-only post-frame-all
audit. `Invoke-AiDispatchAuto.ps1` now injects a dispatcher-generated
GitHub-state snapshot into auto-created issue bodies so audit executors can
confirm queue/already-filed-task state without calling `gh` from the sandbox.
Task 122 must compare the remaining editor-usability candidate classes after
zoom/orbit/pan/frame-all and append one bounded task 123 or record
`NEEDS_HUMAN`; it is not an implementation task.

**2026-06-13 update 19:** ISSUE-376 completed task 122 as `NEEDS_HUMAN`.
The audit used only the embedded dispatcher GitHub-state snapshot for queue
evidence: no open `ai-dispatch` issue, no open failed autonomous issue, and no
already-filed task 123. Source checks confirmed the safe viewport-local camera
sequence is exhausted at wheel zoom, right-button orbit, middle-button pan, and
left-double-click frame-all. The remaining candidate classes require a human
product/architecture decision before the automation can name a bounded task:
broader camera controller/persistence/pointer-capture/frame-selected semantics,
host-shell command-route replacement, real plugin runtime/discovery/loading,
keybinding/remap/conflict policy, OS/typed clipboard, or authoritative
CAD/editor mutation through a richer CommandBus/undo/dirty model. No task 123
was appended.

**2026-06-13 update 20:** the delegated-human follow-up to ISSUE-376 chooses
the keybinding conflict-policy boundary. The next automated implementation is
task 123: conflicted shortcuts remain visible in diagnostics, but keyboard
execution through `ResolveResult::enabled_command_for_shortcut` must return
`None` for any live conflicted shortcut. First-winner lookup remains available
for display/introspection through `command_for_shortcut` /
`AcceleratorTable::resolve`. This is intentionally narrower than shortcut
remapping, fatal startup policy, host-shell FIFO replacement, plugin runtime,
OS clipboard, CAD/editor mutation, camera work, Cargo changes, workflows, or
automation changes.

**2026-06-13 update:** ISSUE-373 published task 119, completing the
viewport-only middle-button pan slice in `editor-shell`, and `c1daf94` added
queue-owned stale-claim cleanup. The next scheduler tick reached task selection
but found the task brief exhausted at 119/119 done, so the queue is re-armed
with task 120: a docs/source-read-only audit that must select exactly one
bounded Phase 9/editor-usability implementation follow-up as task 121, or
record `NEEDS_HUMAN`. It is not an implementation task.

**2026-06-09:** ISSUE-353 / PR #354 completed the task-104 follow-up selected
by the ISSUE-351 audit. `editor-egui-host` now persists command-palette recent
activation ids across host sessions as capped, de-duplicated
`Command::diagnostic_id()` lines under the per-user RGE config path. The
existing blank-filter promotion and task-98 non-blank fuzzy ordering remain the
behavioral contract; favorites, second command models, generalized command
routing, plugin runtime/discovery/loading, Cargo changes, and `editor-shell` /
`editor-ui` changes remained out of scope.

**2026-06-09:** ISSUE-355 / PR #356 completed the advisory-scope hygiene
follow-up. ADR-121 packet validation remains advisory-only, but the verifier
now passes the active `CARGO_TARGET_DIR` to `Test-HandoffPacket.ps1` so generated
in-repo target directories are excluded from touched-file scope checks while
out-of-envelope files outside that target remain visible.

**Automation posture:** `.ai/dispatch.tasks.md` accounts 105/105 tasks done,
GitHub has no open `ai-dispatch` issues, and the autonomous dry-run selector
reports no real task to select. Any next work should start as a fresh bounded
roadmap/audit task rather than continuing the exhausted dispatch brief.

**2026-06-09 update:** task 106 now performs that fresh bounded roadmap/audit
step. It is docs/source-read only and must select exactly one scoped
Phase 9/editor-usability implementation follow-up as task 107, or record
`NEEDS_HUMAN`; it is not authorized to implement the selected work.

**2026-06-09 update 2:** task 106 completed that source-read audit and selected
task 107: command-palette pinned favorites in `editor-egui-host`. The audit
found host-shell FIFO replacement/generalized execution still broader than a
safe one-dispatch step because core menu clicks and accelerators already route
through the canonical `Command` sink while the FIFO remains the deliberate
host-shell boundary. Real plugin execution, keybinding conflict policy,
unsaved prompt UX, OS/typed clipboard, authoritative CAD mutation/undo, and
broader camera UI stay deferred as policy/substrate-heavy. Task 107 is scoped
to host-local pinned command ids, non-fatal persistence, and blank-filter
promotion ahead of recents, with no command-routing, plugin-runtime, Cargo,
`editor-shell`, or `editor-ui` change.

**2026-06-09 update 3:** task 107 implemented command-palette pinned favorites.
`editor-egui-host` persists pinned command ids as capped, de-duplicated
`Command::diagnostic_id()` lines next to the recent-history file, loads them at
host construction, and lets users pin/unpin rows from the palette without
dispatching commands or touching recents. Blank filters rank enabled pinned
commands first, enabled recents second, and all remaining projected rows last;
stale pins and disabled pinned rows are not promoted, and task-98 fuzzy ordering
for non-blank filters remains unchanged. The automation queue is again
exhausted at 107/107 done.

**2026-06-09 update 4:** task 107 landed locally as `876c3ed` after the full
canonical `.ai/dispatch.verify.ps1` gate passed. The queue is re-armed with
task 108, a docs/source-read-only post-palette Phase 9 audit. Its only job is
to compare the remaining editor-usability candidates against current docs and
source, then append exactly one bounded implementation task 109 or record
`NEEDS_HUMAN`; it is not an implementation task.

**2026-06-09 update 5:** task 108 completed that audit and selected task 109:
host-local keyboard shortcuts help in `editor-egui-host`. The audit deferred
host-shell FIFO replacement, real plugin execution, keybinding conflict
policy/remapping, unsaved close/quit prompts, OS/typed clipboard, authoritative
CAD mutation/undo, and broader camera/navigation controls as wider or
policy-heavy. Task 109 is a discoverability slice over existing
`ProjectedMainMenu` shortcut projection only: no new command, binding,
shortcut policy, command routing, plugin runtime/discovery/loading, Cargo,
`editor-shell`, or `editor-ui` change.

**2026-06-09 update 6:** ISSUE-358 published task 109 as commit `9c789f9`.
`editor-egui-host` now has host-local keyboard shortcuts help derived from the
projected main menu and current enablement. The manual five-tick automation run
then exhausted the task brief: one tick was consumed by the ISSUE-358 planning
retry and the final two ticks returned no-selection because no task 110 existed.
The queue is re-armed with task 110, a docs/source-read-only audit that must
select exactly one bounded Phase 9/editor-usability implementation task 111 or
record `NEEDS_HUMAN`.

**2026-06-09 update 7:** task 110 completed that audit and selected task 111:
unsaved Close/Quit confirmation in `editor-shell` plus the `rge-editor` binary
dialog hook. Current source already has the pieces that make this smaller than
the deferred alternatives: `CommandBus::is_dirty()` is the dirty source of
truth, save/source status is already published to the host and window title,
File -> Close / Quit already route through `EditorShell::route_menu_command`,
`WindowEvent::CloseRequested` is the direct app-close path, and the binary
already owns `rfd` while `editor-shell` stays dependency-clean through dialog
traits. Host-shell FIFO replacement, real plugin runtime/discovery/loading,
shortcut/keybinding policy, OS/typed clipboard, CAD mutation/undo, and broader
camera/navigation work remain deferred as wider or policy-heavy.

**2026-06-09 update 8:** ISSUE-360 published task 111 as commit `f831558`.
The shipped implementation adds the unsaved dirty guard through an
`editor-shell` trait boundary and wires the binary-owned native dialog in
`rge-editor`; canonical verification and Codex control both passed. The next
automation tick returned no-selection because the implementation commit did not
also mark task 111 done or append a task 112. The task source is re-armed with
task 112, a docs/source-read-only audit that must compare the remaining
Phase 9/editor-usability candidates from current source and append exactly one
bounded task 113 or record `NEEDS_HUMAN`.

**2026-06-09 update 9:** task 112 completed that source-read audit and selected
task 113: host-local shortcut conflict diagnostics in `editor-egui-host`.
Current source already computes shortcut conflicts in `editor-ui` and projects
them as `ProjectedMainMenu.conflicts`; the host currently renders only a
transient inline `"Shortcut Conflicts"` menu when conflicts exist. The selected
follow-up is limited to making that existing diagnostic data inspectable in the
host without adding remapping, conflict fatality policy, new commands or
accelerators, command-route replacement, plugin runtime/discovery/loading,
Cargo changes, `editor-ui` / `editor-shell` edits, OS clipboard behavior, CAD
mutation, or undo/dirty behavior. Broader host FIFO/generalized execution, real
plugin execution, OS/typed clipboard, CAD mutation/undo, and camera/navigation
work remain deferred.

**2026-06-09 update 10:** ISSUE-362 implemented task 113. The previous inline
`"Shortcut Conflicts"` top-bar dropdown is replaced by a host-local diagnostics
affordance that opens a persistent egui window over the existing
`ProjectedMainMenu.conflicts` rows. The projection remains read-only and
non-activating: no command handoff, palette state, shortcut-help state,
registry policy, shell route, plugin runtime, Cargo, `editor-ui`, or
`editor-shell` behavior changed. The task brief is now exhausted at 113/113
done.

**2026-06-09 update 11:** task 114 completed the post-conflict-diagnostics
source audit and selected task 115: viewport-only mouse-wheel camera zoom in
`editor-shell`. Current source already has menu/keyboard Reset Camera, Zoom In,
and Zoom Out routed through the canonical menu command path, while
`WindowEvent::MouseInput` still names scroll/drag/hover as later work and the
scoped source has no `MouseWheel` branch. The selected follow-up is limited to
using existing camera zoom semantics and existing viewport hit-test state; host
FIFO/generalized execution, real plugin execution, shortcut remapping/conflict
policy, OS/typed clipboard, authoritative CAD/editor mutation, undo/dirty
policy, route replacement, new commands/accelerators, and broader orbit/pan/drag
navigation remain deferred.

**2026-06-10 update 12:** PR #368 merged task 115 as commit `265d540`, adding
viewport-only mouse-wheel zoom in `editor-shell`; ISSUE-367 is closed. The
automation queue is re-armed with task 116, a docs/source-read-only audit that
must re-grep current source after wheel zoom, compare the remaining
Phase 9/editor-usability candidate classes, and append exactly one bounded task
117 or record `NEEDS_HUMAN`. Candidate classes to compare are the next
camera/navigation slice, host-shell FIFO/generalized registry execution, real
plugin command execution, keybinding/remap policy, OS/typed clipboard, and
authoritative CAD/editor mutation routes. Task 116 is not authorized to
implement the selected work.

**2026-06-11 update 13:** task 116 completed that source-read audit and
selected task 117: viewport-only right-button camera orbit in `editor-shell`.
Current source already has the pieces that make this smaller than the deferred
alternatives: `WindowEvent::MouseWheel` zoom is present, `WindowEvent::CursorMoved`
tracks `cursor_pos`, `is_pointer_over_viewport_tab()` provides the viewport
boundary, and `EditorCameraState` owns the CPU-side camera intent. Host-shell
FIFO/generalized execution, real plugin command execution, keybinding/remap
policy, OS/typed clipboard, authoritative CAD/editor mutation policy, pan,
frame/focus, and broader camera-controller work remain deferred. Task 117 is
scoped to right-button orbit only, with no commands, accelerators, host/UI edit,
route replacement, plugin runtime/discovery/loading, Cargo change, clipboard,
CAD mutation, undo/dirty change, or implementation work in the selection audit.

**2026-06-13 update 14:** ISSUE-371 implemented task 117. `editor-shell` now
has viewport-only right-button orbit using the existing cursor tracking,
viewport hit-test boundary, and `EditorCameraState`: a right press over the
Viewport tab starts a private drag, cursor movement rotates the eye around the
current target while preserving target/distance/up/FOV/clip invariants, and
right release stops the drag. Missing cursor/host/viewport-hit and inactive
drags remain no-ops. This closes the selected orbit slice without adding
commands, accelerators, menu entries, host/UI edits, route replacement, plugin
runtime/discovery/loading, Cargo change, clipboard behavior, CAD mutation,
undo/dirty behavior, pan, frame/focus, camera persistence, or generalized input
routing.

**2026-06-13 update 15:** the follow-up automation tick after ISSUE-371
returned no-selection because the task brief was exhausted at 117/117 done. The
queue is re-armed with task 118, a docs/source-read-only audit that must
re-check current source after wheel zoom and right-button orbit, compare the
remaining Phase 9/editor-usability candidate classes, and append exactly one
bounded implementation task 119 or record `NEEDS_HUMAN`. Candidate classes to
compare are the next camera/navigation slice, host-shell FIFO/generalized
registry execution, real plugin command execution, keybinding/remap policy,
OS/typed clipboard, and authoritative CAD/editor mutation routes. Task 118 is
not authorized to implement the selected work.

**2026-06-13 update 16:** task 118 completed that source-read audit and selected
task 119: viewport-only middle-button camera pan in `editor-shell`. Current
source already has the pieces that make this the smallest safe follow-up after
wheel zoom and right-button orbit: `CursorMoved` cursor tracking, the existing
Viewport tab hit-test boundary, mutable `EditorCameraState`, and the private
`viewport_navigation` helper module. Host-shell FIFO/generalized execution, real
plugin command execution, shortcut/remap policy, OS/typed clipboard,
authoritative CAD/editor mutation, frame/focus, and broader camera-controller
work remain deferred as wider or policy-heavy. Task 118 did not implement the
selected pan slice.

---

## Phase 9 editor-usability task-104 selection audit

**2026-06-08 (ISSUE-351):** post-task-102 source/doc audit completed. Current
source confirms task 102 closed only the editor-shell injected handler seam for
captured extension commands. `EditorShell::route_menu_command` still routes
core commands; captured `Command::Custom` / `Command::Plugin` activations can
be drained to an injected handler; `crates/editor-shell` and `editor/rge-editor`
still have no real plugin runtime/discovery/loading path.

**Candidate comparison outcome:** host-shell FIFO replacement and generalized
registry execution remain live but are broader than the next safest bounded
step; keybinding/conflict policy, unsaved quit prompts, OS/typed clipboard,
authoritative CAD mutation/undo, broader camera controls, and real plugin
runtime/discovery/loading each need either wider substrate work or a human
policy decision before they are the best next automation item. The bounded
follow-up selected as task 104 is command-palette recent-history persistence:
persist the existing `editor-egui-host` capped in-memory recent activation ids
across sessions while preserving blank-filter promotion and task-98 non-blank
fuzzy ordering. Favorites are deliberately deferred.

**Non-changes in ISSUE-351:** no Rust source/test edit, no Cargo manifest or
lockfile edit, no workflow/schema/dispatch automation edit, no plugin runtime
or discovery/loading work, no host FIFO replacement, no generalized registry
execution, no keybinding editor, no clipboard work, no CAD mutation, and no
camera UI work.

---

> **Purpose:** Per-wave perf baselines for the metrics that gate `IMPLEMENTATION.md`'s
> "abort condition" thresholds. Each section is appended by the wave that owns the
> measurement; trend tracking is part of the §1.10.4 metrics review at every minor
> version bump.

---

## W03 — PIE snapshot/restore (Phase 5 abort gate)

**Threshold (per `IMPLEMENTATION.md` Phase 5):** if PIE snapshot+restore exceeds
**500ms on a 10k-entity scene**, ECS storage layout needs redesign.

**Harness:** `crates/editor-shell/tests/timing_baseline.rs` — runs
`measure_round_trip` 4× (1 warmup + 3 timed) and reports `min(total)`.

**Run mode:** `cargo test -p rge-editor-shell --release --test timing_baseline -- --nocapture`

**Workload:** entities each carry one `TickCounter` (8 bytes) + one `Position`
(12 bytes); deterministic `BTreeMap`-backed stub `World` (per `world.rs`).

### 2026-05-05 — initial baseline (W03 stub ECS)

| Entities | Serialized bytes | Capture | Restore | Total | Threshold breached |
|---:|---:|---:|---:|---:|---:|
|     100 |     6,048 |  14.1µs |  33.7µs |  47.8µs | no |
|   1,000 |    60,048 |  77.7µs |  92.5µs | 170.2µs | no |
|  10,000 |   600,048 | 1.897ms | 1.955ms | 3.852ms | no |

**Status:** PASS. 10k-entity round-trip is **3.85ms vs 500ms threshold** —
~130× headroom. Phase 5 abort condition not engaged.

### Notes / caveats

- `world.rs` is a v0 stub; real `kernel/ecs::World` (W02) is archetype-based
  and may have different scaling. Re-run after W02 lands to update the table
  in place (do **not** delete this row — keeps the trend visible).
- Capture/restore approximately equal because both go through a single
  `World::clone` (clone-on-capture, clone-on-restore). Real ECS may diverge
  if structural sharing is added.
- Hardware: per `change.log`'s W03 run on Windows 11 / x86_64; release profile
  uses workspace defaults (opt-level 3, lto thin, codegen-units 1).

---

## Phase 5.3 — kernel/ecs PIE round-trip (re-baseline post-migration)

**Threshold (per `IMPLEMENTATION.md` Phase 5):** if PIE snapshot+restore exceeds
**500ms on a 10k-entity scene**, ECS storage layout needs redesign.

**Harness:** `crates/editor-shell/tests/timing_baseline.rs` — same harness as
W03, now driven by `rge_kernel_ecs::World` + 2 `SnapshotComponent`s (Position + `TickCounter`).

**Run mode:** `cargo test -p rge-editor-shell --release --test timing_baseline -- --nocapture`

### 2026-05-06 — re-baseline post Phase 5.3 (real kernel/ecs::World, snapshot v1 = RON payloads)

| Entities | Serialized bytes | Capture | Restore | Total | Threshold breached |
|---:|---:|---:|---:|---:|---:|
|     100 |     11,370 |  50.7µs |  78.9µs | 129.6µs | no |
|   1,000 |    116,570 | 514.3µs | 798.4µs |   1.3ms | no |
|  10,000 |  1,195,570 |   5.3ms |   8.3ms |  13.6ms | no |

**Status:** PASS — 10k-entity round-trip is **13.6ms vs 500ms threshold** —
~36× headroom. Phase 5 abort condition not engaged.

### 2026-05-05 — snapshot v2 (postcard payloads, format VERSION bump 1 → 2)

| Entities | Serialized bytes | Capture | Restore | Total | Threshold breached |
|---:|---:|---:|---:|---:|---:|
|     100 |     10,210 |  22.9µs |  22.0µs |  44.9µs | no |
|   1,000 |    102,882 | 257.0µs | 215.4µs | 472.4µs | no |
|  10,000 |  1,029,882 |   2.8ms |   2.6ms |   5.3ms | no |

**Status:** PASS — 10k-entity round-trip is **5.3ms vs 500ms threshold** —
~94× headroom. Phase 5 abort condition not engaged.

### Comparison: v1 (RON) vs v2 (postcard)

| Entities | v1 bytes | v2 bytes | size delta | v1 total | v2 total | speedup |
|---:|---:|---:|---:|---:|---:|---:|
|   100 |    11,370 |    10,210 | -10.2% | 129.6µs | 44.9µs  | 2.89× |
|   1k  |   116,570 |   102,882 | -11.7% |   1.3ms | 472.4µs | 2.75× |
|  10k  | 1,195,570 | 1,029,882 | -13.9% |  13.6ms |   5.3ms | 2.55× |

Size reduction is modest (~10–14%) because the snapshot framing — entity ULIDs, component
type names (`snapshot_round_trip::Position` etc.), and length prefixes — dominates the
per-component payload bytes. The wall-time speedup (~2.5–2.9×) reflects postcard's faster
encode/decode path vs RON's text parsing on the small payloads we have here. The original
hesitation to adopt postcard ("non-deterministic without explicit key ordering") was
unfounded for our case: postcard serializes structs in declaration order, and the snapshot
framing already sorts entities by ULID and component types by `snapshot_name()`, so v2
output is byte-identical across runs. (Verified by `serialize_restore_serialize_byte_identical`
test in `kernel/ecs/tests/snapshot_round_trip.rs`.)

### Comparison vs W03 stub baseline (v2 numbers)

| Entities | W03 stub (BTreeMap blob) | Phase 5.3 v2 (kernel/ecs + postcard) | delta |
|---:|---:|---:|---:|
|   100 |  47.8µs  |  44.9µs | -6%   |
|  1k   | 170.2µs  | 472.4µs | +2.8× |
|  10k  |  3.852ms |   5.3ms | +1.4× |

The stub used a flat `BTreeMap<EntityId, Vec<u8>>` with raw byte blobs (zero serde cost);
real kernel/ecs adds archetype iteration + postcard encoding. With v2, 10k overhead vs
the stub floor shrinks to 1.4× (was 3.5× under v1). Abort gate is informational here —
correctness matters, not the absolute comparison.

### Notes / caveats

- v2 wire format: postcard per-component payloads, custom binary framing (RGES magic +
  LE integers + `VERSION = 2`). Entity iteration sorted by ULID `u128`; component type
  iteration sorted by `snapshot_name()` string. v1 (RON) snapshots are not readable by v2
  — bump-only migration; no on-disk persistence existed at the time of the bump.
- The kernel/ecs snapshot test (`kernel/ecs/tests/snapshot_round_trip.rs` test 6) reports
  6.85ms for 10k entities under v2 (was 14.5ms under v1). Single-shot measurement, not
  the min-of-3 used by the editor-shell harness above.
- Archetype iteration determinism: the single catch-all archetype means entity row order
  depends on spawn/despawn history; snapshot sorts by EntityId before iterating, ensuring
  byte-identical output regardless of insertion order.
- Hardware: Windows 11 / x86_64 / release profile (opt-level 3, lto thin, codegen-units 1).

---

## Phase 3.2 — script-host module swap (Phase 3 hot-reload abort gate)

**Threshold (per `IMPLEMENTATION.md` Phase 3 + §5.6):**
- Hot-reload swap p95 **< 100ms** (gate)
- Cold-start (Module compile + first instantiate) **< 50ms** (PLAN §5.6 budget)
- Hard abort: hot-reload p95 **> 500ms** triggers ADR-077 review

**Harness:** `crates/script-host/tests/swap_smoke.rs` — measures the swap
window (capture state → drop old instance → instantiate v2 module → restore
state) on a 1-entity Counter scene with two WAT fixtures (`counter_v1.wat`
increments by 1; `counter_v2.wat` increments by 2).

`crates/script-host/tests/cold_start_smoke.rs` — measures Module compile +
fresh instantiate latency on a hello-world module.

**Run mode:** `cargo test -p rge-script-host` (debug build).

### 2026-05-05 — initial baseline (single-iteration, debug, 1-entity scene)

| Measurement | Value | Threshold | Result |
|---|---|---|---|
| Module swap window (capture → drop → compile → instantiate → restore) | **0.31 ms** | <100 ms p95 | ~320× headroom |
| Cold-start (Module compile + Instance new on hello-world) | **9.1 ms** | <50 ms | ~5× headroom |

**Status:** Constitutional hot-reload bet **validated** at the substrate level.
The swap mechanism (state capture via RON over Counter + wasmtime instance
re-instantiation + state restore) clears the abort gate by two orders of
magnitude.

### Deferred to formal Phase 3.3/3.4 dispatch

The numbers above are single-iteration debug-mode smoke tests on a 1-entity
scene. The full Phase-3 exit criteria (per `IMPLEMENTATION.md`) require:

| Gate | Status |
|---|---|
| Hot-reload p95 < 100ms on a **1000-entity scene** | not yet measured |
| ECS iteration via WASM ≤ **1.5×** native Rust | not yet measured |
| **1-hour** session without memory leak | not yet measured |
| Component data preserved across **100 hot-reload cycles** | only 1 cycle smoke-tested |

The criterion benchmarks in `crates/script-bench/benches/{cold_start,hot_reload_swap,memory_overhead,script_tick_1m}.rs`
are scaffolded but currently driven by a stub engine; they need re-wiring
against `rge-script-host` + a 1000-entity Counter fixture before the formal
p95 gate can be measured. Tracked as Phase 3.3+3.4 follow-up dispatch.

### Notes / caveats

- ECS bridge is hard-coded for `Counter(i64)` — generic component bridge
  (WIT-typed, reflection-driven over `kernel/types`) is Phase 4-Foundation.
- Swap state capture uses direct `ron::to_string` on a hand-shaped
  `CounterSnapshot`, not the generalized `kernel/types` reflect-roundtrip
  pathway. Real-scene swap latency depends on the reflection cost; pending
  the generic bridge, the 0.31ms above is a lower bound.
- Wasmtime version: 44 (per workspace.dependencies). `unsafe_code = "deny"`
  override at the script-host crate root (3 sites with `// SAFETY:` proofs)
  for the wasmtime call-scope pointer pattern; mirror of the pak-format
  precedent for `mmap`.

---

## §13.2 Editor frame idle (Phase 6 §6.3 Gate B)

| Date | Hardware | Methodology | Scope | P50 | P95 | Variance | Gate (≤ 8 ms) |
|---|---|---|---|---|---|---|---|
| 2026-05-11 | dev box (Windows / cargo 1.94 / wasmtime 44) | batch N=1000 × K=10 | **empty-shell CPU-idle baseline** | 0.000044 ms | 0.000047 ms | 9.7% | PASS |

**Methodology**: batch timing around `EditorShell::tick_redraw()` calls
to clear Windows `Instant` resolution floor (~100 ns per call). K=10
batches × N=1000 frames each. P50/P95 computed across the 10
per-frame batch means. Variance gate applies across batch means.

**Scope limitation (LOAD-BEARING)**: This is the CURRENT empty-shell
CPU-idle baseline — `EditorShell::new()` with no `cad_world`, no
projection, no scene, no GPU, no winit event loop. It is NOT a
loaded-editor idle measurement. **Future re-measure required** once
non-trivial editor systems / idle scene are wired (driven by future
Phase 6 dispatches), at which point the same harness shape can be
re-run against the loaded shell.

**Gate B status**: CLOSED for current CPU-idle interpretation
(P95 = 0.000047 ms, ~170 000× under 8 ms gate). Re-measure required
for loaded-editor interpretation.

**Harness**: `crates/editor-shell/tests/editor_frame_idle.rs` (annotated
`#[ignore]` — release-only timing test; debug build trips variance gate).
Invoke via:

```
cargo test -p rge-editor-shell --release --test editor_frame_idle -- --ignored --nocapture
```

---

## §6.3 Gate A — 60fps simple-scene golden (1k cubes, 1 directional light)

| Date | Adapter | Backend | Methodology | Scope | P50 | min-P95 | median P95 | max P95 | Worst frame | Variance | Gate (≤ 16.67 ms) |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 2026-05-11 | NVIDIA GeForce RTX 4060 Ti (DiscreteGpu, NVIDIA driver) | Vulkan | 600 frames after 60-frame warmup; 3 runs, min-of-3 reported | 1280×720, static camera, release mode | 0.085 ms | **0.112 ms** | 0.116 ms | 0.117 ms | 1.803 ms | 4.9% | **PASS** |

**Methodology**: release-mode headless wgpu render-loop. 1000 axis-aligned cubes baked into a single `VertexBuffer` + `IndexBuffer` (option-(a) single-draw-call strategy — `LitMeshPipeline` has no instance-attribute or per-draw-transform support and the D1 dispatch forbade non-test `crates/gfx/src/**` edits). Single `DirectionalLight`; static camera at Z=-40; 1280×720 viewport; shared PSO + 1 material across all 1000 cubes; one `draw_indexed` call per frame. 600 sampled frames after a 60-frame warmup. 3 runs; min-of-3 P95 reported. Variance gate applies across the 3 runs' P95 values (threshold ≤ 30%).

**Scope limitation (LOAD-BEARING)**: This Gate A closure is **CONSTRAINED-CERTIFIED on the recorder host only**. It does NOT certify:

- universal 60fps across hardware classes
- vendor parity (NVIDIA vs AMD vs Intel; Vulkan vs DX12 vs Metal vs WebGPU)
- cold-start frame cost (the 60-frame warmup explicitly discards it)
- sustained thermal behavior (3 runs × 600 frames is too short)
- realistic geometry complexity (1000 axis-aligned cubes sharing 1 PSO is fragment-light, vertex-light, draw-call-medium)
- CI regression coverage (release-only `#[ignore]` test — PR-time regressions surface only on the next manual recorder invocation)
- memory or VRAM footprint (orthogonal PLAN §13.2 350 MB simple-scene gate, not measured here)

**Gate A status**: **CLOSED** on recorder host only (min-of-3 P95 = 0.112 ms, ~150× under the 16.67 ms gate). Re-measure required for any new recorder host / adapter / backend / viewport / camera path.

**Harness**: `crates/gfx/tests/gate_a_simple_scene_60fps.rs` (annotated `#[ignore]` — release-only timing test). Invoke via:

```
cargo test -p rge-gfx --release --test gate_a_simple_scene_60fps -- --ignored --nocapture
```

**Sequencing note**: Gate B (CPU-idle empty-shell baseline) closed earlier 2026-05-11; Gate A (this entry) closes for current recorder constraints; **Gate C (render-thread sees stable snapshot; sim-thread mutations don't race) remains DEFERRED** — blocked on the sim/render thread split landing per PLAN §1.5.2 (today's substrate is single-threaded, so the property is vacuously true and the gate is structurally unmeasurable until the split exists).

**Post-depth Gate A — CLOSED 2026-05-14 (MAIN-RENDER-POSTDEPTH-GATEA-001 dispatch, gfx-level synthetic harness)**: The "depth-attached gfx-level harness" option (a) listed in the prior `Post-sub-β measurement gap` note landed as `crates/gfx/tests/gate_a_simple_scene_depth_60fps.rs` — an additive, release-only, `#[ignore]` integration test that mirrors the pre-depth Gate A methodology byte-for-byte (1000 cubes / 10×10×10 / 1280×720 / 60 warmup + 600 sample / 3 runs / P95 ≤ 16.67 ms / variance ≤ 30%) but constructs the pipeline via `LitMeshPipeline::new_with_depth(.., Some(DepthStateKey { Depth24Plus, depth_write_enabled: false, LessEqual }))` (sub-α API) and passes `Some(&depth_view)` to `record_lit_mesh_pass(...)` (per-frame `Depth24Plus` depth texture allocated once and reused). Zero non-test `crates/gfx/src/` edits; the existing `record_lit_mesh_pass` already supports the `Option<&wgpu::TextureView>` arg. Recorder-host run on **NVIDIA GeForce RTX 4060 Ti / Vulkan / DiscreteGpu**: run 0 P95 = 0.125 ms, run 1 P95 = 0.122 ms, run 2 P95 = 0.122 ms → **min-of-3 P95 = 0.122 ms** (median P95 = 0.122 ms, max P95 = 0.125 ms, worst frame = 1.996 ms, **variance across runs = 2.6%**). About 9% slower than pre-depth (0.122 ms vs 0.112 ms) — the measured cost of the depth attachment — and still ~137× under the 16.67 ms gate. **The 0.112 ms pre-depth claim above remains valid for the pre-depth gfx path; this post-depth claim is the additional valid measurement for the depth-attached gfx path.** **Scope (recorder-host-only)**: NOT universal, NOT vendor parity, NOT cold-start, NOT sustained thermal, NOT realistic geometry complexity, NOT CI regression coverage, NOT editor-shell `render_frame` end-to-end (the harness exercises the gfx-level primitives that editor-shell production consumes post-sub-β; it does not exercise editor-shell's winit + `SurfaceContext` + `FrameGraph` + `build_resource_map` substrate ceremony — that remains a separate non-winit-perf-harness scope, blocked on `EditorShell::render_frame` accepting a mock event loop, not pursued by this dispatch). **What's still deferred**: option (b) non-winit editor-shell perf harness (unchanged scope; pressure-driven future dispatch); option (c) manual user report (unchanged; orthogonal to harness-level proof). **No new architecture, no production-source edits, no PLAN target retargeting in this dispatch.**

**2026-05-23 supersession of the "option (b) non-winit editor-shell perf harness" deferral (ISSUE-118; docs-only history reconciliation)**: The clause directly above stating that "option (b) non-winit editor-shell perf harness (unchanged scope; pressure-driven future dispatch)" is "still deferred" is **HISTORICAL ONLY as of the 2026-05-14 paragraph that records it**, and is **SUPERSEDED** by the post-v0 landing recorded here. The non-winit editor-shell `render_frame` perf harness landed **post-v0** at commit `f8b8ed4` as `crates/editor-shell/src/render_frame_e2e_perf.rs` — a release-only `#[ignore]` recorder-host integration test that drives `EditorShell::render_frame` end-to-end without a winit event loop and measures the encode/submit window excluding surface acquire/present. Provenance for existence, path, and commit: `ai_handoffs/POSTV0-EDITOR-SHELL-PERF-HARNESS-001_EXEC_2026-05-14_21-51-40+0300.md`. **v0 release certification at commit `6aaf7f1` is unchanged** — that certification predates `f8b8ed4` and remains the v0 reference; the editor-shell perf harness landed *after* v0 certification and resolves the option (b) deferral without retroactively altering the cert. **No new BASELINE.md measurement row is added** for the editor-shell harness, and **no recorder-host P95 / P50 / worst-sample numbers from the POSTV0 EXEC packet are copied into this doc**. **Hard P95 / worst-sample / variance threshold pinning** for this harness remains a **future, explicitly-authorized certification scope** and is **not chosen here**; the POSTV0 EXEC packet itself documents that threshold pinning was left deferred. ISSUE-118 is documentation reconciliation only — no source / test / bench / Cargo / schema / lint / automation / protocol-doc / `plans/IMPLEMENTATION.md` edits, and no Cargo or perf-harness execution in this dispatch.

---

## §13.3 Compile-time baseline (Phase 9 preflight)

**Budget anchors (per `plans/PLAN.md` §1.10 + `plans/IMPLEMENTATION.md` §6 table at line 689–690):**

- Clean-build budget: **≤ 120 s** (`cargo build --release` from a wiped `target/`)
- Incremental p95 budget: **≤ 10 s** (`cargo build` after a 1-line source change)
- Reflection compile-time gate (Phase 1.1): **> 30 s on 5 pilot types ⇒ STOP**
- Incremental invalidation radius (v0.7, NEW): **> 30 % of workspace rebuilt after touching one core type ⇒ lint warn**

**This entry is a Phase 9 PREFLIGHT — a warm-cache `cargo check` baseline ONLY.** It is explicitly **NOT** a proof that the clean-build or incremental p95 budgets are satisfied, and it does NOT close any §13.3 gate. It establishes the first recorded compile-time reference number for the workspace so future regressions can be detected; the formal clean-build and 1-line-edit incremental measurements are deferred to a future dispatch that owns the target-dir rewarm cost and a dedicated harness script.

**Harness:** The original 2026-05-21 row used manual PowerShell `[System.Diagnostics.Stopwatch]` wrapping. As of 2026-06-07, use `tools/compile-timing.ps1` for repeatable warm-cache measurements. The script uses `A:\RustCache\cargo`, `A:\RustCache\rustup`, and `A:\RustCache\target` when present and unset, supports `-Mode check|build|both`, `-Iterations`, `-AllTargets`, `-Release`, `-PackageSet Workspace|DefaultCleanRelease`, `-TimeoutSeconds`, and optional `-JsonPath`, and intentionally exposes no target-deletion or `cargo clean` path. `Workspace` remains the default for existing timing runs. `DefaultCleanRelease` is valid only with `-Mode build -Release` and resolves the explicit Phase 9 package set through `tools/Resolve-CleanReleasePackageSet.ps1`.

```
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\tools\compile-timing.ps1 -Mode check -Iterations 1 -TimeoutSeconds 30
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\tools\compile-timing.ps1 -Mode build -Iterations 1 -TimeoutSeconds 120
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\tools\compile-timing.ps1 -Mode build -Release -PackageSet DefaultCleanRelease -Iterations 1 -TimeoutSeconds 1200
```

For the `--all-targets` variants, pass `-AllTargets`. For full-workspace release-mode measurements, pass `-Release`. For the default clean-release gate, pass `-Release -PackageSet DefaultCleanRelease`; the generated Cargo command is an explicit `cargo build --release -p ...` package list, not `cargo build --workspace --release`.

### 2026-05-21 — initial warm-cache `cargo check` baseline (Phase 9 preflight; recorder host)

| Measurement | Command | Elapsed (wall) | Cargo "Finished" | Notes |
|---|---|---:|---:|---|
| Warm, fingerprint-stale full-workspace check | `cargo check --workspace` | **17.65 s** | 17.42 s | Many workspace crates re-checked despite warm cache → fingerprint drift since last build (recent dispatch-publish commits touched source). Worst-of-pair for this preflight. |
| Warm no-op rerun (full workspace, no `--all-targets`) | `cargo check --workspace` (immediate rerun) | **0.93 s** | 0.76 s | Sentinel scan only — cargo overhead floor for this workspace under the warm cache. |
| Warm `--all-targets` first run (adds tests + benches) | `cargo check --workspace --all-targets` | **13.69 s** | 13.40 s | Tests/benches for two crates (`rge-io-3mf`, `rge-kernel-shared`) checked for the first time this session; rest were already up-to-date. |
| Warm `--all-targets` no-op rerun | `cargo check --workspace --all-targets` (immediate rerun) | **1.18 s** | 0.91 s | Sentinel scan only with tests + benches included. |

### 2026-06-07 - compile timing harness landing (warm-cache no-op; recorder host)

| Measurement | Command | Elapsed (wall) | Cargo "Finished" | Notes |
|---|---|---:|---:|---|
| Warm no-op full-workspace check | `tools/compile-timing.ps1 -Mode check -Iterations 1 -TimeoutSeconds 30` -> `cargo check --workspace` | **0.896 s** | 0.73 s | Harness validation run on the shared `A:\RustCache\target`; emitted pre-existing `rge-ui-theme` missing-docs warnings; exit 0. |
| Warm no-op full-workspace build | `tools/compile-timing.ps1 -Mode build -Iterations 1 -TimeoutSeconds 120` -> `cargo build --workspace` | **1.024 s** | 0.84 s | Harness validation run on the shared `A:\RustCache\target`; emitted the same pre-existing `rge-ui-theme` missing-docs warnings; exit 0. |

**Status:** harness-first step complete. These rows are warm no-op sentinel measurements only. They do **not** wipe `target/`, do **not** run `cargo clean`, do **not** certify the true clean-build budget, and do **not** measure the 1-line-edit incremental p95 budget.

### 2026-06-07 - clean release build measurement (isolated target; recorder host)

| Measurement | Command | Budget | Elapsed (wall) | Cargo "Finished" | Verdict | Notes |
|---|---|---:|---:|---:|---|---|
| True clean release workspace build | `CARGO_TARGET_DIR=B:\sdk\rge-clean-target-20260607-1855` + `tools/compile-timing.ps1 -Mode build -Release -Iterations 1 -TimeoutSeconds 1200` -> `cargo build --workspace --release` | <=120 s | **156.591 s** | 2m 36s | **MISS** | Fresh empty isolated target under `B:\sdk`; shared `A:\RustCache\target` was not wiped; no `cargo clean`; build exited 0 with the pre-existing `rge-ui-theme` missing-docs warnings; scratch target removed after measurement. |

**Status:** clean-build measurement gap closed, but the gate is **not** closed. The current recorder-host clean release build misses the §13.3 budget by 36.591s (~30.5%). Remediation and remeasurement remain open.

**Recorder context (for trend tracking):**

| Field | Value |
|---|---|
| Workspace members (Cargo.toml count) | **94 crates** (kernel 15 / crates 65 / tools 8 / runtime 4 / editor 1 + 1 proc-macro at `crates/macros-reflect`) |
| Source files (non-vendor `.rs`, excludes `target/` / `.claude/` / `OLD/` / `third_party/`) | **673** |
| Source LoC (non-vendor `.rs`, same exclusions) | **144,754** (kernel 21,324 / crates 116,806 / runtime 20 / editor 96 / tools 6,508) |
| Largest single crate by `src/` LoC | **`cad-core` = 24,842 LoC** (next: `gfx` 8,950, `editor-ui` 5,779, `editor-shell` 5,256) |
| Rust toolchain | **1.92.0** (pinned via `rust-toolchain.toml`; floor driven by `egui_dock 0.19` MSRV) |
| `CARGO_TARGET_DIR` | **`A:\RustCache\target`** (shared across dispatches; not the workspace-local `target/`) |
| Shared target dir on-disk size | **≈ 385 GB** (~395 GB measured at sample time; warm with all transitive deps from prior dispatches) |
| Host OS | Windows 11 / x86_64 |

**Status:** **PHASE 9 compile-time baseline — measured clean-build miss.**

- The 2026-05-21 four numbers establish the first recorded compile/check reference for the workspace. They do NOT satisfy or close any §13.3 budget gate.
- **2026-05-21 rows are NOT a clean-build measurement**: `target/` was deliberately not wiped (would cost hours of recompile time across the ~385 GB shared cache and would have broken every subsequent dispatch). The 17.65 s number is best read as "warm cache after fingerprint drift from the most recent source touches", not as the §13.3 ≤ 120 s clean-build budget.
- **2026-06-07 isolated-target clean release measurement is a true clean-build measurement and currently misses**: `cargo build --workspace --release` from an empty isolated target took wall 156.591s / cargo 2m 36s against the ≤120s budget. The measurement closes the evidence gap but leaves the performance gate open.
- **NOT a 1-line-edit incremental p95 measurement**: this preflight was docs-only by directive — no source touch, no Cargo touch, no lint/ADR/automation touch. The "no-op rerun" floors (0.93 s / 1.18 s) are a lower bound on cargo overhead, not the p95 metric the §13.3 budget targets.
- **`cargo check` not `cargo build`**: §13.3's ≤ 120 s clean / ≤ 10 s incremental budgets are written against `cargo build`. `cargo check` is a strict subset (no codegen / no linking), so a passing `cargo check` time is necessary but not sufficient evidence for the build budget.

**Top 3 compile-time pressure risks identified by this preflight (qualitative; no measurement yet):**

1. **No formal compile-time baseline existed prior to this entry.** Every other Phase 9 compile-time axis is downstream of this row.
2. **Incremental invalidation radius was originally suspected to be near the 30 % lint-warn threshold.** The later §1.10.4 preflight below corrected this estimate downward and should be treated as the current source of truth.
3. **`cad-core` at 24,842 LoC is the dominant single-crate compile cost.** Already internally split (`topology/` / `operators/` / `topo_lineage/` / `tessellation/` / `checkpoints/` / `graph/`), but fingerprinted as one unit, so any cad-core source edit recompiles the full 25 k LoC plus the csgrs / nalgebra / blake3 link tail. Severity is low–medium today; would matter only when iteration on cad-core becomes the bottleneck (constraint solver, Fillet G2 patches, a second CAD-kernel adapter under ADR-113-deferred).

**Remaining follow-ups:**

1. **Clean-build remediation and remeasurement** (§13.3 ≤ 120 s gate) — the first true measurement exists and misses at 156.591s wall / 2m 36s cargo.
2. **Incremental invalidation radius refresh before linting** — the §1.10.4 preflight below corrected the earlier qualitative risk estimate; convert it into a lint only when its documented triggers fire.
3. **1-line-edit incremental p95 sample** (§13.3 ≤ 10 s gate) — **MEASURED 2026-06-07 (ISSUE-321): p95 = 1.507 s, PASS.** See the dedicated entry below.

**Notes / caveats:**

- Cargo's "Checking …" lines do not imply work was done; only the "Finished … in N.NNs" line counts. The "wall" column above is the PowerShell-stopwatch wall-clock around the whole `cargo` invocation (includes process startup + stdout drain); the "Cargo `Finished`" column is what cargo itself reports.
- Two warnings were emitted during the runs (`rge-ui-theme` missing-docs, `rge-cad-core revolve_fillet_smoke.rs` unused variable). They are pre-existing and unrelated to this preflight; they did not affect timing meaningfully.
- The shared `CARGO_TARGET_DIR=A:\RustCache\target` setup means individual dispatch sessions inherit a fully warm cache; a fresh-checkout developer on a different machine will see materially different numbers on first build. That asymmetry is exactly why a future clean-build dispatch is non-trivial to schedule.
- Hardware identity is deliberately not pinned in this row beyond "recorder host / Windows / x86_64". A future dispatch that owns the cleaner harness should record the CPU model, NVMe vs SATA on `A:\`, and antivirus posture (NTFS realtime scan is a known cargo-throughput drag on Windows).

### 2026-06-07 — one-line incremental p95 build measurement (warm-cache; recorder host; ISSUE-321)

**Gate:** PLAN §13.3 incremental `cargo build --workspace` p95 ≤ 10 s.

**Result: p95 = 1.507 s wall → PASS (≤ 10 s budget), with ~8.5 s / ~85 % of headroom remaining.**

| Field | Value |
|---|---|
| Metric | One-line incremental `cargo build --workspace` wall seconds (warm shared cache) |
| Budget | **≤ 10 s** incremental p95 (§13.3) |
| Harness | `tools/compile-timing.ps1 -Mode build -Iterations 1 -TimeoutSeconds 120` per sample (one timed `cargo build --workspace` each) |
| Cache policy | Warm shared `CARGO_TARGET_DIR=A:\RustCache\target`; **no `cargo clean`, no target deletion/wipe** |
| Warm-up (not counted) | 1 build to resync this worktree's fingerprints against the shared cache → 13.788 s wall (recompiled the drifted crate set); exit 0 |
| Selected source touch | `runtime/runtime-headless/src/main.rs` — a thin leaf **binary** crate (nothing depends on it; touch recompiles only `rge-runtime-headless` + relink) |
| Touch method | Append exactly one harmless unique trailing comment line (`// ISSUE-321 incremental timing touch NN`) before each counted sample; identical file used for every sample; restored to original via `git checkout --` before verification (`git diff` empty) |
| Sample count | **24** counted incremental samples (each preceded by a one-line touch); all exit 0, none timed out, none no-op |
| Wall seconds (sorted, s) | 1.351, 1.371, 1.409, 1.411, 1.411, 1.420, 1.421, 1.429, 1.430, 1.445, 1.450, 1.451, 1.457, 1.464, 1.466, 1.479, 1.480, 1.486, 1.490, 1.503, 1.507, 1.507, 1.507, 1.511 |
| Summary | min 1.351 s · mean 1.452 s · p50 1.451 s · max 1.511 s |
| p95 method | Nearest-rank over `wall_seconds` ascending: rank = `ceil(0.95 × 24)` = **23** → 23rd value = **1.507 s** |
| **Verdict vs ≤ 10 s** | **PASS** (1.507 s ≪ 10 s) |

**Status:** **§13.3 incremental p95 gate — measured PASS.** The one-line-edit incremental build comfortably meets the ≤ 10 s budget for a low-risk leaf-crate touch. The §13.3 *clean*-build sub-gate remains a separate MISS (156.591 s clean release, recorded above) and is unaffected by this incremental result.

**Notes / caveats:**

- The chosen file is a **leaf binary**, so the recompile radius is minimal (one thin crate + link). A touch to a high-fan-in core type would recompile a much larger reverse-dep closure and would not necessarily share this headroom; this measurement characterizes the low-risk leaf-touch case the §13.3 budget anchors on, not a worst-case core-type edit. The §1.10.4 invalidation-radius preflight below tracks the worst-case fan-out separately.
- Per-sample wall includes cargo process startup + stdout drain, so it sits a little above the cargo-reported `Finished` codegen time (e.g. sample wall 1.49 s vs cargo `Finished … in 1.30s`); the delta above the warm no-op floor (~1.02 s, recorded above) confirms each counted sample performed a real recompile rather than a sentinel scan.
- All 24 timing JSON payloads live under the gitignored `.ai/dispatch-ISSUE-321/` run dir; the final tracked diff is documentation/task-record only. No `cargo clean` was run and `A:\RustCache\target` was not deleted or wiped.

### 2026-06-07 — clean release build hotspot attribution (isolated target; recorder host; ISSUE-322)

**Gate:** PLAN §13.3 clean release `cargo build --workspace --release` ≤ 120 s. **Still a MISS** after this attribution; the gate is **not** closed.

**Attribution method (fresh isolated-target remeasurement, not attribution-only):** a true clean release workspace build was re-run from a *new empty* isolated target with Cargo's built-in `--timings` per-unit profiler. `tools/compile-timing.ps1` was not used for this row because it intentionally exposes no `--timings` path and cannot emit per-unit attribution. The shared `A:\RustCache\target` was not used or wiped, and no `cargo clean` was run; only `CARGO_TARGET_DIR` was pointed at a fresh empty directory under `B:\sdk`. The reused `CARGO_HOME`/`RUSTUP_HOME` (`A:\RustCache\cargo` / `A:\RustCache\rustup`) supply the already-downloaded registry + toolchain, so the empty target still forces a full from-scratch recompile of every dependency and workspace crate.

**Exact commands (exit 0):**

```
# CARGO_HOME=A:\RustCache\cargo  RUSTUP_HOME=A:\RustCache\rustup
$env:CARGO_TARGET_DIR = 'B:\sdk\rge-clean-hotspots-ISSUE-322-20260607-234243'   # fresh empty dir
cargo build --workspace --release --timings                                     # exit 0
cargo tree -i cranelift-codegen -e normal     # provenance (exit 0)
cargo tree -i wasmtime -e normal              # provenance (exit 0)
```

**Fresh clean-release result — supersedes 156.591 s as the current measurement (both confirm MISS):**

| Measurement | Target | Budget | Elapsed (wall) | Cargo "Finished" | `--timings` critical wall | Verdict |
|---|---|---:|---:|---:|---:|---|
| True clean release workspace build (`--timings`) | `B:\sdk\rge-clean-hotspots-ISSUE-322-20260607-234243` (fresh empty; removed after run) | ≤ 120 s | **147.82 s** | **2m 27s** | 147.66 s | **MISS** |

This fresh isolated-target run **supersedes the earlier 156.591 s** figure as the *current* clean-release reference (more recent, same recorder host + method, plus per-unit attribution). The ~8.8 s delta (156.591 → 147.82) is ordinary run-to-run variance (host load / NTFS scan); **both runs independently confirm the MISS** — the budget is missed by ≈ 27.8 s (≈ 23 %). The earlier 156.591 s rows above are retained as historical record (forward-only; not rewritten). Build exited 0 with the pre-existing `rge-ui-theme` missing-docs warnings; the scratch target (2.28 GB) was verified to resolve under `B:\sdk\rge-clean-hotspots-ISSUE-322-*` and then removed.

**Dominant clean-build cost drivers, largest first** (from `--timings` `UNIT_DATA`; 685 compile units, 1517.6 CPU-compile-seconds at max concurrency 16/16 cores):

| Rank | Unit | Unit wall | Share of 147.66 s critical wall | On critical path? | Provenance |
|---:|---|---:|---:|---|---|
| 1 | **`cranelift-codegen` 0.131.1** (single codegen unit) | **125.64 s** | **85 %** | **Yes — it is the long pole** (starts 22.02 s, ends 147.66 s = build end) | `wasmtime` Cranelift backend |
| 2 | `vello_cpu` 0.0.6 | 53.36 s | 36 % | No (ends 72.7 s) | CPU vector renderer |
| 3 | `windows` 0.62.2 | 38.56 s | 26 % | No | Win32 bindings |
| 4 | `gltf-json` 1.4.1 | 37.55 s | 25 % | No | glTF asset import |
| 5 | `naga` 29.0.3 | 36.36 s (3 units) | 25 % | No | wgpu shader translation |
| 6 | `wasmtime` 44.0.1 | 31.69 s (3 units) | 21 % | Partial | WASM runtime |
| 7 | `egui` 0.34.2 | 32.12 s | 22 % | No | editor UI |
| 8 | `wgpu-core` 29.0.3 | 28.88 s (3 units) | 20 % | No | GPU abstraction |

**Headline finding — the build is bounded by one serial unit, not by parallelism.** Max concurrency was 16 jobs on 16 cores, yet `cranelift-codegen` compiles as a *single* 125.64 s rustc unit that sits on the critical path from 22.02 s to the build's end (147.66 s). Total CPU compile-work is 1517.6 s spread ~10.3× across cores, so adding cores cannot help: the clean build can never finish before `cranelift-codegen`'s own compile time (~125 s) plus its ~22 s of prerequisites (`cranelift-codegen-meta` build script, `cranelift-isle`, etc.). **The only path under 120 s is to make `cranelift-codegen` cheaper or remove it from the graph.**

**Provenance of the long pole:** `cranelift-codegen` ← `cranelift-frontend`/`cranelift-native` ← `wasmtime-internal-cranelift` ← `wasmtime` 44.0.1. On the clean-release **attribution path** that `cargo tree -i wasmtime -e normal` surfaces, `wasmtime` is reached through **`rge-expr-wasm`** and **`rge-runtime-wasmtime-engine`** — but that is the attribution path that owns the critical-path unit, **not** the complete set of direct dependents. A direct manifest scan (`Select-String -Path crates/*/Cargo.toml -Pattern wasmtime`) shows **four** workspace crates declare `wasmtime` as a *direct* dependency: **`rge-expr-wasm`** (`crates/expr-wasm/Cargo.toml:16`), **`rge-runtime-wasmtime-engine`** (`crates/runtime-wasmtime-engine/Cargo.toml:33`, behind the default-on `engine_wasmtime` feature), **`rge-script-host`** (`crates/script-host/Cargo.toml:20`), and **`rge-script-bench`** (`crates/script-bench/Cargo.toml:39`, `winch` feature). Any remediation that scopes or removes the wasmtime/cranelift dependency must therefore account for all four of these direct dependents, not only the two on the attribution path. The whole WASM/Wasmtime/Cranelift family is 340.8 s of CPU-compile across 54 units (≈ 22 % of all compile work) and owns the critical path.

**Estimated post-remediation floor:** with the WASM/Cranelift family removed or cheapened, the next-latest-finishing units are the workspace binary links — `rge-editor` (ends 109.88 s) and `rge-tool-architecture-lints` (ends 109.90 s), behind `rge-physics` (107.0 s). So cutting the `cranelift-codegen` long pole alone would likely land the clean build near **~110 s — under the 120 s budget** — before any other driver needs touching. The non-Cranelift drivers (vello_cpu, windows, gltf-json, naga, egui, wgpu) are second-order: they run in parallel and are *not* on today's critical path, so trimming them first would not move the wall time while Cranelift remains the pole.

**Smallest next remediation candidates (intentionally small; none implemented here):**

| # | Candidate | Affected crate / dep / build phase | Expected compile upside | Source-behavior risk | Review risk | Automation suitable? |
|---:|---|---|---|---|---|---|
| A | **Lower the release `opt-level` for `cranelift-codegen` via a `[profile.release.package."cranelift-codegen"]` override** (workspace `Cargo.toml`) | `cranelift-codegen` codegen unit (the long pole) | Large — typically 3–5× faster compile of the 125.64 s unit; likely drops the critical path toward the ~110 s floor | Low–medium: no API/source change, but Cranelift's *runtime* wasm-JIT throughput may regress and must be re-checked against the script/expr perf path | Low (one Cargo profile block) | **Only after a separate task** (needs a Cargo edit + a script/expr perf re-check) |
| B | **Switch the wasm runtime to wasmtime's Pulley interpreter** (`wasmtime` `default-features=false`, drop `cranelift`/`winch`) across **all four direct dependents** — `rge-expr-wasm`, `rge-runtime-wasmtime-engine`, `rge-script-host`, **and** `rge-script-bench` (scoping it to only the first two would leave `script-host`/`script-bench` still pulling cranelift/winch) | removes `cranelift-codegen` / `winch-codegen` / `wasmtime-internal-cranelift` from the graph entirely (~the full 340.8 s family) only if every direct dependent drops the cranelift/winch features | Largest — eliminates the long pole and most of the WASM family | **High**: Pulley interprets instead of JIT-compiling — script execution gets much slower at runtime and any AOT/precompile path changes; note `rge-script-bench` currently pins the `winch` feature, so its bench semantics also change | High (scripting runtime semantics) | **No** (architectural; needs human + perf/feature design) |
| C | **Feature-gate the whole wasm scripting stack behind an optional workspace feature** so default `cargo build --workspace --release` excludes wasmtime/cranelift unless `scripting` is enabled | `rge-expr-wasm`, `rge-runtime-wasmtime-engine`, `rge-script-host`, `rge-script-bench` + their wasmtime/cranelift closure | Large — removes the family (and the long pole) from the *default* build | Medium–high: changes what the default build contains; scripting tests/CI must opt in | High (build-surface / CI policy) | **No** (needs human decision on default build contents) |

**Candidate A experiment result (2026-06-08 / ISSUE-329):** `[profile.release.package."cranelift-codegen"] opt-level = 1` was tested from fresh isolated target `B:\sdk\rge-cranelift-opt1-ISSUE-329-20260608-045939` using `cargo build --workspace --release --timings`. The result was worse, not better: Cargo `Finished` = **2m 58s**, `--timings` Total = **178.4s (2m 58.4s)**, and `cranelift-codegen` remained the critical-path tail at **148.38s** (start 30.01s, end 178.39s). This regressed the prior ISSUE-322 reference (**147.82s** total, `cranelift-codegen` **125.64s**) and still misses the <=120s clean-build budget. The override was therefore reverted and is not retained. The focused `cargo test -p rge-script-bench --release --lib wasmtime_cranelift::tests -- --nocapture` check was started, but the guard aborted the run during compile because its monitor emitted a JSON parse error; because the clean-build gate had already failed decisively, no Cargo profile change was kept.

**Candidate B/C feasibility audit (2026-06-08 / ISSUE-333):** audit-only; no Rust source, tests, benches, Cargo manifests/lock, workflows, tooling, schemas, scheduler config, dispatch automation scripts, or shared `A:\RustCache\target` contents were changed.

**Command evidence:**

- `git grep -n -E '^[[:space:]]*wasmtime[[:space:]]*=' -- Cargo.toml 'crates/**/Cargo.toml' 'kernel/**/Cargo.toml' 'runtime/**/Cargo.toml' 'editor/**/Cargo.toml' 'tools/**/Cargo.toml'` -> exit 0. Output was exactly one workspace slot plus four direct crate declarations: `Cargo.toml:189` (`features = ["cranelift", "runtime", "std"]`), `crates/expr-wasm/Cargo.toml:16` (`features = ["parallel-compilation"]`), `crates/runtime-wasmtime-engine/Cargo.toml:33` (optional `wasmtime` behind default-on `engine_wasmtime`), `crates/script-host/Cargo.toml:20`, and `crates/script-bench/Cargo.toml:39` (`features = ["winch"]`).
- `cargo tree -i wasmtime -e normal` -> exit 0. Reverse tree: `wasmtime v44.0.1` is reached directly by `rge-expr-wasm`, `rge-runtime-wasmtime-engine`, `rge-script-host`, and `rge-script-bench`; `rge-runtime-wasmtime-engine` also flows through `rge-script-host -> rge-script-bench`, and `rge-script-host` flows through `rge-script-bench`.
- `cargo tree -i cranelift-codegen -e normal` -> exit 0. Reverse tree: `cranelift-codegen v0.131.1` flows through `cranelift-frontend` / `cranelift-native` -> `wasmtime-internal-cranelift` -> `wasmtime`, then through the same four direct `wasmtime` dependents. The tree also shows the `wasmtime-internal-winch` / `winch-codegen` branch under the same `wasmtime` graph.
- Per-package feature inspections all exited 0: `cargo tree -p rge-expr-wasm -e features --depth 1`, `cargo tree -p rge-runtime-wasmtime-engine -e features --depth 1`, `cargo tree -p rge-script-host -e features --depth 1`, and `cargo tree -p rge-script-bench -e features --depth 1`. The compact feature roots were: `rge-expr-wasm` adds `wasmtime/parallel-compilation`; `rge-runtime-wasmtime-engine` uses `wasmtime/cranelift`, `runtime`, and `std`; `rge-script-host` forces `rge-runtime-wasmtime-engine/engine_wasmtime` and uses `wasmtime/cranelift`, `runtime`, and `std`; `rge-script-bench` uses `wasmtime/cranelift`, `runtime`, `std`, and `winch`.
- Source/API inspection used `git grep -n -E "wasmtime|Engine|Config|Strategy|parallel|winch|cranelift|Compilation|Profiler|OptLevel|Module::|Instance::|Linker|Store" -- ...` after `rg` was unavailable in this shell. The `git grep` passes exited 0 for all four direct dependents. `cargo metadata --format-version 1 --no-deps` exited 0 and was used to enumerate the affected package target surfaces. `git grep -n -E '^default-members\s*=' -- Cargo.toml` returned no matches (exit 1), so the workspace currently has no explicit `default-members` list; current clean-release measurements use the stronger `cargo build --workspace --release` selector.
- Local cached Wasmtime metadata was read from `A:\RustCache\cargo\registry\src\index.crates.io-1949cf8c6b5b557f\wasmtime-44.0.1` without modifying it. The feature table shows `cranelift = ["dep:wasmtime-cranelift", "std", "wasmtime-unwinder/cranelift"]`, `winch = ["dep:wasmtime-winch", "std"]`, `runtime` includes `pulley-interpreter/interp`, `std` includes `pulley-interpreter/std`, and `pulley = []`. `Cargo.toml.orig` documents that `pulley` enables Wasmtime's interpreter: paired with `cranelift`, compiler backends for `pulley32` / `pulley64` are available; paired with `runtime`, the interpreter can execute modules compiled to Pulley bytecode. `Config::strategy` and `Config::cranelift_opt_level` are compiled only with `feature = "cranelift"` or `feature = "winch"`; `Strategy` has `Auto`, `Cranelift`, and `Winch` only; Pulley selection is by target triple (`Config::target`, `Triple::pulley_host()` -> `pulley64` on this 64-bit little-endian host), not by `Strategy::Pulley`.

**Per-dependent assessment:**

| Direct dependent | Current Wasmtime feature/source posture | Candidate B Pulley-only consequence | Candidate C default-build-gate consequence |
|---|---|---|---|
| `rge-expr-wasm` | Direct `wasmtime` with `parallel-compilation`; source uses `Engine::default`, `Module::new`, `Linker`, `Store`, typed funcs, and expression cache/evaluator paths. | Manifest would need the workspace `cranelift` feature removed and `pulley` added; `parallel-compilation` should be removed or justified because it is compiler-oriented. Source has no explicit `cranelift_opt_level`, but behavior/perf must be rechecked because `Engine::default` currently compiles expression modules through the Cranelift-enabled host path. | Excluding this crate from default release removes its lib target, tests (`cache_test`, `correctness_test`, `whitelist_test`), doctest surface, and benches (`eval_speed`, `compile_speed`) unless CI selects them explicitly. |
| `rge-runtime-wasmtime-engine` | Direct optional `wasmtime` behind default-on `engine_wasmtime`; source calls `wasmtime::Config::new()` then `cfg.cranelift_opt_level(wasmtime::OptLevel::Speed)`, compiles `Module::new`, and instantiates through `Linker<HostState>` / `Store<HostState>`. | Not manifest-only: with `cranelift` and `winch` removed, `cranelift_opt_level` is not compiled by Wasmtime 44. A Pulley implementation would need source changes to stop setting Cranelift optimization and to choose a Pulley target/config path, then re-run cap-gate, trap, link-error, and tick behavior tests. | Excluding this crate from default release also affects `rge-script-host` and `rge-script-bench` through the reverse tree. Its lib/doctest and `hello_world` integration test must remain explicitly covered. |
| `rge-script-host` | Direct `wasmtime`, plus `rge-runtime-wasmtime-engine` with `features = ["engine_wasmtime"]`; source uses `Engine`, `Module`, `Instance`, `Linker`, `Store`, host memory access, and ECS bridge host functions. | Mostly follows the engine configuration; no explicit Cranelift strategy was found in this crate, but its tests use `wasmtime::Engine` directly and its dependency forces the runtime engine feature. Pulley behavior would still need cold-start, panic-isolation, swap, host-memory, and ECS bridge tests. | Excluding this crate removes its lib/doctest and integration tests (`cold_start_smoke`, `host_panic_isolation`, `swap_smoke`), and it removes the downstream `rge-script-bench` script-host path from default release unless selected explicitly. |
| `rge-script-bench` | Direct `wasmtime` with `winch`; depends on `rge-script-host`; source contains `ScriptHostBench::new_with_strategy(Strategy::Winch)`, raw `wasmtime_cranelift` fixtures using `cranelift_opt_level(OptLevel::Speed)`, and raw `wasmtime_singlepass` fixtures using `Strategy::Winch`. | Not manifest-only: dropping `cranelift` and `winch` invalidates the current Cranelift/Winch comparison semantics and touches the benchmark source surface. Pulley would need a new benchmark meaning and rerun focused release gates before any runtime claim. | Excluding this crate removes its lib tests (including `script_host`, `wasmtime_cranelift`, and `wasmtime_singlepass` modules) and benches (`script_tick_1m`, `cold_start`, `memory_overhead`, `hot_reload_swap`) from default release unless CI invokes them explicitly. |

**Candidate B requirements:** a real Pulley-only implementation would appear to require root workspace `wasmtime` to remove `cranelift` and add `pulley` while keeping `runtime` and `std`; `crates/script-bench` would need to remove `winch`; `crates/expr-wasm` would need to remove or justify `parallel-compilation`; `crates/runtime-wasmtime-engine` would need source changes away from `cranelift_opt_level(OptLevel::Speed)` and toward an explicit Pulley target/config; `crates/script-bench` would need source/test changes because current raw Cranelift and Winch benchmark rows are compiler-strategy comparisons. Required gates before landing: `cargo test -p rge-expr-wasm --all-targets`, `cargo test -p rge-runtime-wasmtime-engine --all-targets`, `cargo test -p rge-script-host --all-targets`, `cargo test -p rge-script-bench --lib` including script-host/Cranelift/Winch-or-replacement modules, `cargo bench -p rge-script-bench --no-run`, `cargo bench -p rge-expr-wasm --no-run`, and focused release/performance checks for expression compile/eval, script cold-start, ECS iteration, hot-reload swap, memory overhead, and any Pulley replacement for the raw Cranelift/Winch rows. This path is less safe right now because it changes runtime execution semantics and cannot be reduced to manifest edits.

**Candidate C requirements:** default-build gating has to be package-selection and CI policy, not only a Cargo feature flip. The current clean-release command is `cargo build --workspace --release`; `--workspace` will still select every workspace member even if a `default-members` list is added. A real implementation must therefore define an explicit default release package set (or change the clean-release command to a documented `default-members` build), exclude the wasm scripting stack from that default set, and add explicit opt-in checks so scripting coverage remains visible. The affected workspace package set is `rge-runtime-wasmtime` (engine-independent cap API), `rge-runtime-wasmtime-engine`, `rge-script-host`, `rge-expr-wasm`, and `rge-script-bench`; `rge-tool-wasm-bench` is a named wasm bench wrapper binary with an empty dependency table and should be an explicit include/exclude decision even though it does not currently pull `wasmtime`. Affected test/bench surfaces are the package targets listed above plus `rge-runtime-wasmtime-engine::hello_world`, `rge-script-host::{cold_start_smoke,host_panic_isolation,swap_smoke}`, `rge-expr-wasm::{cache_test,correctness_test,whitelist_test,eval_speed,compile_speed}`, and `rge-script-bench::{script_tick_1m,cold_start,memory_overhead,hot_reload_swap}` plus its lib-test modules. Affected commands include the clean-release proof command (`cargo build --workspace --release --timings` must become the documented default package-set build), `.ai/dispatch.verify.ps1` step 4 (`cargo test --workspace --all-targets` and `cargo test --workspace --doc`) if default-only verification is desired, `.ai/dispatch.verify.ps1` step 5 (`cargo bench -p rge-script-bench --no-run`, which should stay as explicit opt-in coverage), `.github/workflows/tests.yml` workspace test/doc commands, `.github/workflows/bench.yml` `cargo bench -p rge-script-bench --no-run`, and `.github/workflows/bench.yml`'s release `rge-script-bench` cold-start observational test. `cargo deny check` may remain workspace-wide if the policy is "default release excludes wasm but dependency audit still covers all members"; otherwise that policy must be stated explicitly.

**Selected follow-up (candidate C):** implement an explicit default clean-release package-set gate that excludes the wasm scripting stack (`rge-runtime-wasmtime`, `rge-runtime-wasmtime-engine`, `rge-script-host`, `rge-expr-wasm`, `rge-script-bench`) from the default release build, updates the clean-release measurement command away from `cargo build --workspace --release` to the documented default package set, and adds/keeps explicit CI/verify opt-in commands for the excluded wasm stack. Candidate B is less safe right now because Pulley-only Wasmtime requires source/API and benchmark-semantics changes before behavior or performance can be trusted.

**Candidate C implementation result (2026-06-08 / ISSUE-335):** the default clean-release gate is now a resolver-backed package selection, not a Cargo manifest/default-member change and not a dependency-feature change.

- `tools/Resolve-CleanReleasePackageSet.ps1` is the machine-readable surface. It runs `cargo metadata --format-version 1 --no-deps`, emits `schema_version = clean-release-package-set-v1`, `set_name = DefaultCleanRelease`, included package names, excluded package names, the generated cargo arguments/command, and validation booleans.
- The default clean-release excluded set is exactly `rge-runtime-wasmtime`, `rge-runtime-wasmtime-engine`, `rge-script-host`, `rge-expr-wasm`, and `rge-script-bench`.
- `rge-tool-wasm-bench` is explicitly **included**. Rationale: current metadata shows no dependency on `wasmtime` or the excluded scripting packages, so excluding it solely because its name contains `wasm` would hide a current non-Wasmtime package from the default release set.
- `tools/compile-timing.ps1 -Mode build -Release -PackageSet DefaultCleanRelease` resolves that set and generates a command shaped as `cargo build --release -p PACKAGE_NAME ...`; the generated command does not contain `--workspace`.
- Existing `tools/compile-timing.ps1` invocations remain workspace timing by default (`-PackageSet Workspace`), so warm/full workspace check/build measurements are still available.
- `.ai/dispatch.verify.ps1`, `.github/workflows/tests.yml`, and `.github/workflows/bench.yml` were not narrowed. The repository's normal verify/workflow test coverage remains workspace-wide, and the explicit wasm/script opt-in bench compile remains visible as `cargo bench -p rge-script-bench --no-run`.

**Default clean-release measurement result (2026-06-08 / ISSUE-337 manual salvage):** the follow-up measurement was run from a fresh isolated target with the `DefaultCleanRelease` package set:

```
$env:CARGO_TARGET_DIR = 'B:\sdk\rge-clean-default-ISSUE-337-manual-20260608'
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\tools\compile-timing.ps1 -Mode build -Release -PackageSet DefaultCleanRelease -Iterations 1 -TimeoutSeconds 1200
```

Result: exit 0, wall **125.467s**, Cargo `Finished` **2m 05s**. This is a **MISS** vs the <=120s clean-build budget by **5.467s**. The generated command was an explicit `cargo build --release -p ...` package list and did **not** contain `--workspace`; the resolver reported **92 included** packages and **5 excluded** packages (`rge-runtime-wasmtime`, `rge-runtime-wasmtime-engine`, `rge-script-host`, `rge-expr-wasm`, `rge-script-bench`), with `rge-tool-wasm-bench` included. The run emitted the pre-existing `rge-ui-theme` missing-docs warnings. The isolated scratch target was verified under `B:\sdk` and removed after recording; shared `A:\RustCache\target` was not deleted and `cargo clean` was not run.

**DefaultCleanRelease hotspot attribution result (2026-06-08 / ISSUE-338 manual salvage):** an attribution-only follow-up ran the same resolver-backed package set through Cargo `--timings` from fresh isolated target `B:\sdk\rge-clean-default-hotspots-ISSUE-338`. The command was explicit `cargo build --release -p ... --timings`, did **not** contain `--workspace`, and resolved **92 included** / **5 excluded** packages with `rge-tool-wasm-bench` included. Result: exit 0, wall **111.072s**, Cargo `Finished` **1m 50s**, Cargo timings total **110.8s**, **569** timing units. Timing HTML, extracted `UNIT_DATA`, and a derived `hotspot-analysis.json` were preserved under gitignored `.ai/dispatch-ISSUE-338/default-clean-release-hotspots/`; the scratch target was verified under `B:\sdk` and removed after copying artifacts.

Top units by duration in that run:

| Rank | Unit | Start | Duration | End | Note |
|---:|---|---:|---:|---:|---|
| 1 | `vello_cpu 0.0.6` | 19.13s | 54.32s | 73.45s | dependency cost, not tail |
| 2 | `zstd-sys 2.0.16+zstd.1.5.7` build script run | 28.61s | 43.46s | 72.07s | dependency build script |
| 3 | `windows 0.62.2` | 13.56s | 38.76s | 52.32s | dependency cost |
| 4 | `gltf-json 1.4.1` | 37.60s | 38.66s | 76.26s | dependency cost |
| 5 | `naga 29.0.3` | 18.99s | 36.29s | 55.28s | dependency cost |
| 6 | `egui 0.34.2` | 30.66s | 32.09s | 62.75s | dependency cost |
| 7 | `rge-editor 0.0.1` bin | 80.56s | 30.23s | **110.79s** | **critical tail** |
| 8 | `rge-physics 0.0.1` | 75.84s | 29.10s | 104.94s | workspace long unit |
| 9 | `wgpu-core 29.0.3` | 51.59s | 28.54s | 80.13s | dependency cost |
| 10 | `parry3d-f64 0.19.0` | 30.01s | 26.43s | 56.44s | dependency cost |
| 11 | `wgpu-hal 29.0.3` | 44.98s | 26.16s | 71.14s | dependency cost |
| 12 | `rapier3d-f64 0.24.0` | 34.62s | 23.21s | 57.83s | dependency cost |
| 13 | `rge-tool-architecture-lints 0.0.1` bin | 84.85s | 22.28s | 107.13s | late workspace bin |

Critical-tail interpretation: after excluding the Wasmtime scripting stack, the remaining wall clock is no longer dominated by `cranelift-codegen`. In the timed run it is bounded by late workspace binary/link units: `rge-editor` ends at **110.79s**, followed by `rge-tool-architecture-lints` ending at **107.13s**, while `rge-physics` is the largest non-bin workspace lib at **29.10s** duration ending at **104.94s**.

**DefaultCleanRelease variance confirmation result (2026-06-08 / ISSUE-339 manual salvage):** the queued Codex executor could not create the required `B:\sdk` scratch target from its workspace sandbox, so no automated build sample ran. Manual root-shell salvage then ran three fresh isolated plain samples with the same resolver-backed package set:

| Sample | Isolated `CARGO_TARGET_DIR` | Wall | Cargo `Finished` | Exit | Verdict |
|---:|---|---:|---|---:|---|
| 1 | `B:\sdk\rge-clean-default-variance-ISSUE-339-sample1` | **109.273s** | `Finished` **1m 48s** | 0 | PASS |
| 2 | `B:\sdk\rge-clean-default-variance-ISSUE-339-sample2` | **115.303s** | `Finished` **1m 55s** | 0 | PASS |
| 3 | `B:\sdk\rge-clean-default-variance-ISSUE-339-sample3` | **108.280s** | `Finished` **1m 48s** | 0 | PASS |

All three generated commands were explicit `cargo build --release -p ...` package lists with no `--workspace`; the resolver reported **92 included** / **5 excluded** packages and `rge-tool-wasm-bench` included. Each scratch target was verified under `B:\sdk`, removed after recording, and verified absent. Per-sample JSON/stdout/stderr plus `variance-summary.json` are retained under gitignored `.ai/dispatch-ISSUE-339/default-clean-variance/`.

**Current verdict:** candidate C materially improved the clean-release selector relative to the full-workspace Wasmtime/Cranelift build, and the repeated plain-sample check now records a recorder-host **provisional PASS** for the `DefaultCleanRelease` clean-build gate. The maximum of the three fresh plain samples is **115.303s**, under the <=120s budget. Keep the ISSUE-338 hotspot attribution as a future regression guide (`rge-editor` / late binary tail), but do not start source or package-policy remediation from the earlier one-off **125.467s** miss unless a future fresh isolated measurement regresses over budget.

**What would prove improvement later:** for candidates that keep the full workspace release selector, re-run this exact `cargo build --workspace --release --timings` from a fresh isolated `B:\sdk\rge-clean-hotspots-ISSUE-322-*` target after a candidate lands and confirm (1) the new `Total time` <= 120 s and (2) `cranelift-codegen` is no longer the critical-path tail in `UNIT_DATA`. For the selected candidate C follow-up, the proof command must instead be the documented default package-set release build that replaces the current `--workspace` selector, with a separate explicit wasm-stack opt-in verification command.

**Notes / caveats:**

- "Unit wall" is each compile unit's own duration from `--timings`; several of these units overlap in time, so their shares of the 147.66 s critical wall sum to more than 100 % — only **rank 1 (`cranelift-codegen`) is actually on the critical path**. The ranking answers "which units cost the most to compile," and the critical-path annotation answers "which of those actually bound the wall clock." Both are needed for an honest remediation order.
- This is a fresh isolated-target *clean* build, exit 0 — not a warm-cache, no-op, partial, failed, timed-out, or non-isolated run — so it is eligible to supersede 156.591 s. The §13.3 incremental p95 PASS (1.507 s, ISSUE-321) is unaffected; only the clean-build sub-gate is in scope here and it remains open.
- Attribution artifacts (`UNIT_DATA` JSON, the `cargo-timing.html` report, and the build log) are retained under the gitignored `.ai/dispatch-ISSUE-322/` run dir; the final tracked diff is documentation/task-record only. The per-unit table above is derived from `.ai/dispatch-ISSUE-322/unit-data.json`, which is the `const UNIT_DATA = [ … ];` array (lines 6414–16140) extracted from the preserved `.ai/dispatch-ISSUE-322/cargo-timing-clean-release.html`. The original ad-hoc extraction command was not captured at measurement time; the ISSUE-322 correction round re-ran an equivalent local extraction against the preserved HTML — `$h = Get-Content .ai/dispatch-ISSUE-322/cargo-timing-clean-release.html -Raw; $s = $h.IndexOf('const UNIT_DATA = [') + ('const UNIT_DATA = ['.Length) - 1; $e = $h.IndexOf("\`n];", $s); ($h.Substring($s, $e - $s + 2) | ConvertFrom-Json) | ConvertTo-Json -Depth 6 | Set-Content .ai/dispatch-ISSUE-322/unit-data.regen.json -Encoding UTF8` (exit 0) — which corroborated the artifact exactly: 685 units in both, with `cranelift-codegen` 0.131.1 = 125.64 s. See the ISSUE-322 correction EXEC packet for the full command record. No source, tests, Cargo manifests/lock, tooling, workflows, scheduler, or dispatch automation were changed; no `cargo clean`; `A:\RustCache\target` was not deleted or wiped.

---

## §1.10.4 Incremental-invalidation-radius preflight (Phase 9)

**Budget anchor (per `plans/PLAN.md` §1.10.4 / risk-table line 1218):**

> **Incremental invalidation radius** (crates rebuilt after touching one core type) **> 30 % of workspace ⇒ lint warn**.

For the current workspace (95 members per `cargo metadata`), the lint-warn threshold is **28.5 crates** (i.e. a crate whose transitive reverse-dep closure includes ≥ 29 workspace members would trip the warning).

**This entry is a Phase 9 PREFLIGHT — pure read-only `cargo metadata` measurement.** It is NOT a lint, it is NOT a harness wired into CI, and it does NOT touch source / Cargo / lint code. It establishes the first recorded radius reference for the workspace so future regressions are visible, and it codifies the revisit triggers under which a real lint / harness becomes warranted.

**Methodology (read-only):**

1. `cargo metadata --format-version 1 > meta.json` — produces the full resolved dep tree including `dep_kinds` per edge.
2. Parse the JSON: collect `workspace_members` (set of package IDs), then walk `resolve.nodes[].deps[].dep_kinds[]` to build the workspace-internal forward graph in two flavours:
   - **NORMAL** = edges with `dep_kinds.kind = null` (normal lib) ∪ `"build"` (build-deps invalidate too).
   - **NORMAL+DEV** = above ∪ edges with `dep_kinds.kind = "dev"` (counts test/bench rebuilds).
3. Invert each forward graph to a reverse graph, then compute the transitive reverse-dependency closure for every workspace crate (DFS through the reverse adjacency map).
4. The percentage **closure / 95** is the invalidation-radius measurement.

No source files were read; only `Cargo.toml` (via cargo's own resolver) and the resolved metadata JSON. The Python parser is throw-away (lives outside the repo at `C:/Users/halil/AppData/Local/Temp/rge_radius2.py`); reproducer is below.

### 2026-05-21 — initial workspace radius snapshot (recorder host, Rust 1.92.0)

**Workspace context:**

| Field | Value |
|---|---|
| `cargo metadata` workspace members | **95** crates |
| Older `Status.md` / `HANDOFF.md` / `README.md` wording | "94 crates" (one-off doc drift; one extra crate has landed since those rows were last refreshed; not material to threshold analysis — the discrepancy is < 2 %) |
| Workspace-internal edges (normal + build) | **64** |
| Workspace-internal edges (dev) | **13** |
| Distinct workspace-internal edges | **75** |
| **Isolated crates** (zero workspace-internal edges in either direction) | **57 of 95 (60 %)** |
| Examples of isolated crates | `anim-clip`, `anim-ik`, `anim-retarget`, `cad-native`, `cad-occt`, `components-{editor, interaction, lifecycle, networking, physics, spatial}`, `runtime-{web, mobile, headless}`, 7 of 8 `tools/*` (only `architecture-lints` is connected) |

**Top 10 workspace crates by reverse-dep closure (descending):**

| Rank | Crate | Normal closure | % of 95 | Direct (normal) | +Dev closure | +Dev % |
|---:|---|---:|---:|---:|---:|---:|
| 1 | `rge-kernel-graph-foundation` | **18** | **18.9 %** | 9 | 18 | 18.9 % |
| 2 | `rge-kernel-diagnostics` | 15 | 15.8 % | 12 | 15 | 15.8 % |
| 3 | `rge-kernel-ecs` | 10 | 10.5 % | 9 | 10 | 10.5 % |
| 4 | `rge-kernel-asset` | 7 | 7.4 % | 7 | 7 | 7.4 % |
| 5 | `rge-kernel-plugin-host` | 7 | 7.4 % | 5 | 7 | 7.4 % |
| 6 | `rge-cad-core` | 5 | 5.3 % | 4 | 5 | 5.3 % |
| 7 | `rge-brep-render` | 4 | 4.2 % | 3 | 4 | 4.2 % |
| 8 | `rge-editor-state` | 3 | 3.2 % | 2 | 4 | 4.2 % |
| 9 | `rge-material-runtime` | 3 | 3.2 % | 1 | 4 | 4.2 % |
| 10 | `rge-runtime-wasmtime` | 3 | 3.2 % | 2 | 3 | 3.2 % |

**Requested candidates (explicit) — side-by-side normal vs +dev:**

| Crate | Normal closure / % | +Dev closure / % | Direct normal revdeps |
|---|---:|---:|---|
| `rge-kernel-types` | 2 / 2.1 % | 3 / 3.2 % | `rge-macros-reflect`, `rge-script-host` |
| `rge-kernel-graph-foundation` | **18 / 18.9 %** | 18 / 18.9 % | `rge-anim-graph`, `rge-asset-store`, `rge-cad-core`, `rge-cad-projection`, `rge-editor-ui`, `rge-gfx`, `rge-kernel-asset`, `rge-material-graph`, `rge-script-graph` |
| `rge-cad-core` | 5 / 5.3 % | 5 / 5.3 % | `rge-cad-projection`, `rge-editor`, `rge-editor-shell`, `rge-editor-state`, `rge-editor-ui` |
| `rge-macros-reflect` | **0 / 0.0 %** | 0 / 0.0 % | — (only its own internal tests/fixtures use it) |
| `rge-kernel-app` | 0 / 0.0 % | 0 / 0.0 % | — (declared in workspace; no consumer) |
| `rge-kernel-schedule` | 0 / 0.0 % | 0 / 0.0 % | — (declared in workspace; no consumer) |

**Status:** **PHASE 9 PREFLIGHT — no breach. Defer lint and tool implementation.**

- **No crate is anywhere near the 30 % threshold today.** Highest fanout is `kernel/graph-foundation` at **18.9 %** (18 of 95 crates) — **11.1 pp under** the lint-warn ceiling, with **~10.5 crates of headroom** before the warn level fires.
- The earlier rough qualitative estimate (`graph-foundation NodeId ~32 %`, recorded in the §13.3 Compile-time baseline section's "Top 3 risks" #2 entry) was **wrong** in direction-of-error: it conflated *VizAdapter trait usage via `&dyn`* (which doesn't add a crate-level Cargo edge) with *transitive Cargo deps*. The current radius is materially safer than that section implied. The §13.3 entry's qualitative claim should be read in that corrected light.

**Top 3 invalidation-radius risks (qualitative; baseline-state findings):**

1. **No present breach, but 60 % of the workspace is structurally isolated.** 57 of 95 crates have zero workspace-internal Cargo edges in either direction. The current 18.9 % top is a **temporary low-water mark, not a stable equilibrium** — radius will increase materially as stubs land and start consuming kernel substrate. Implication: this baseline must be **revisited periodically**, not treated as evergreen.
2. **`kernel/diagnostics` is the second-place fanout at 15.8 %** with **12 direct normal revdeps** — the densest direct edge count of any crate. Any signature-breaking change to `Diagnostic` / `Severity` / `DiagnosticSink` / `FailureClass` would cascade across 12 crates immediately and 15 transitively. Today this is well under threshold; if `kernel/diagnostics` ever absorbs additional concerns (e.g. structured telemetry, metrics, plugin telemetry), it is the most likely first crate to pierce 25 %.
3. **Three "architectural-root" crates are effectively orphaned by Cargo:** `kernel/types` (2 normal revdeps), `kernel/app` (0), `kernel/schedule` (0); plus `macros-reflect` itself (0). `kernel/types` is documented in PLAN §1.1 as *the* reflection root, but no production crate currently goes through `macros-reflect`-derived reflection — only the macro crate's own `tests/compile_budget_5_pilots.rs` exercises 5 pilot types. This is a **honesty gap between the §1.1 framing and the dep graph**, not a compile-time risk today, but it explains why the §13.3 reflection compile-time gate (`> 30 s on 5 pilot types ⇒ STOP`) has never fired: there *are* no production-reflected types in the workspace yet.

**Revisit triggers** — re-run this `cargo metadata`-based preflight when **either** of the following becomes true:

1. **Any single crate's normal-closure percentage crosses 25 %** (≈ 24 of 95 crates today; ≈ a 5 pp jump from the current top of 18.9 %). At that point the warn-level breach at 30 % is one substrate-merger or one kernel-substrate-consumer landing away, and a real lint becomes warranted.
2. **The isolated-stubs population drops below 30 of 95** (i.e. **more than ~ 65 of 95 workspace crates have wired up to workspace-internal deps**). At that connectivity level, the closure percentages of the existing top crates will have grown enough that radius regression is no longer dominated by stub-state.

Until **at least one** of those fires, treat the current radius as observed-safe and **defer both the lint and any tool wiring**. `tools/invalidation-profiler/` is currently a 5-line `main.rs` stub; that is the correct state for now — building it before either revisit trigger fires would be premature mechanism per PLAN §1.10's "pressure-driven" doctrine.

**Reproducer (read-only, no harness in-tree):**

```
$env:CARGO_HOME='A:\RustCache\cargo'; $env:RUSTUP_HOME='A:\RustCache\rustup'
$env:Path='A:\RustCache\cargo\bin;' + $env:Path
cd A:\RCAD\RGE
cargo metadata --format-version 1 > meta.json
# Parse meta.json with any JSON-aware tool:
#   - workspace IDs: .workspace_members[]
#   - graph: .resolve.nodes[] (each node has .id, .deps[].pkg, .deps[].dep_kinds[].kind)
#   - filter dep_kinds where kind is null (normal) or "build" for the normal-closure;
#     include "dev" for the +dev variant.
#   - transitive reverse closure = DFS through reverse adjacency map.
# The throw-away parser used for this entry lives at
#   C:/Users/halil/AppData/Local/Temp/rge_radius2.py
# but is not committed and not required (any JSON path tool reproduces the same numbers
# from meta.json — Python with `json` stdlib, jq, or PowerShell ConvertFrom-Json).
```

**Notes / caveats:**

- All numbers above are workspace-internal only. External crates.io deps are NOT counted in the percentages — they don't trigger workspace-crate recompilation when their version is unchanged.
- The `+Dev` column matters for `cargo test --workspace` invalidation but NOT for `cargo build --workspace`; the §1.10.4 budget targets the latter, so the **normal-closure column is the primary signal**. The `+Dev` column is included for completeness and to highlight cases (e.g. `kernel/types` 2 → 3, `kernel/diagnostics` 15 → 15, `cad-core` 5 → 5) where test/bench-only deps don't materially shift the picture today.
- The "94 vs 95" workspace-member count discrepancy is harmless. `cargo metadata` is the authoritative count and reports 95; the older "94" wording in `Status.md` / `HANDOFF.md` / `README.md` predates the latest workspace-Cargo.toml addition. A future docs-only reconciliation can refresh those numbers when a meatier Status/HANDOFF sweep is warranted; not in scope for this baseline-record dispatch.
- Two crates show **direct revdep count > normal closure** in the top 10 (`kernel/diagnostics`: direct 12, closure 15; `kernel/ecs`: direct 9, closure 10). That happens when most direct consumers are leaf crates (no further fanout); good news structurally — diagnostics has wide *direct* reach but doesn't compound transitively.
- This preflight does **NOT** measure: compile-time wall-clock impact of a 1-line edit to a core type (that's a §13.3 incremental p95 measurement, separately deferred); reflection schema explosion (separately gated by PLAN §1.1's "> 30 s on 5 pilot types" reflection gate, never fired); or generic-monomorphization count per crate (PLAN §1.10's "5,000 warn / 15,000 hard" threshold, not measured here). It strictly measures *which* crates would be invalidated, not *how long* that invalidation would take to resolve.
- The `cad-projection` closure (2 normal revdeps: `rge-editor`, `rge-editor-shell`) is much smaller than expected given its central architectural role — this is because `cad-projection` is consumed at the *application* layer (editor binary + editor-shell orchestrator), not by downstream Tier-2 crates. The cad-projection moat is wide-but-shallow in graph-shape terms.

---

## §1.1 Reflection-scale honesty preflight (Phase 9)

**Budget anchors and gate references:**

- IMPLEMENTATION.md Phase 1 §1.1 (line 117): "`kernel/types` — FIRST REAL CRATE. The architectural root. Everything depends on this."
- IMPLEMENTATION.md Phase 1 abort (line 190): "> 30 s on 5 pilot types ⇒ STOP and replan reflection strategy."
- IMPLEMENTATION.md Phase 9 §9 (line 597): "Reflection scale — compile time + binary size at 100+ reflected types."
- PLAN.md §13.2 (line 1124): "reflection cache 1000 components ≤ 2 MB."
- PLAN.md §13.3 (line 1128): "gen instantiations per crate ≤ 5,000 warn / ≤ 15,000 hard · trait expansion depth ≤ 8/16."
- PLAN.md §13.10 / §1.10.4 (line 526): "Reflection schema size (typed components × fields) > 10 K = warn."
- **Phase 1.1 compile-budget source of truth: [`kernel/types/BUDGET.md`](../kernel/types/BUDGET.md)** (baseline taken 2026-05-05; not duplicated here).

**This entry is a Phase 9 PREFLIGHT — pure read-only audit of current reflection adoption.** It does NOT change the substrate, add pilot types, or touch any reflection consumer. It establishes the first recorded *adoption* baseline (distinct from the *compile-budget* baseline already in `BUDGET.md`) so future production-reflection landings have an honest before-and-after reference.

**Methodology (read-only):**

1. Inspect crate state via `wc -l` on `kernel/types/src/` and `crates/macros-reflect/{src,tests}/`.
2. Grep-based inventory of `#[derive(Reflect)]`, `rge_macros_reflect::`, and `rge_kernel_types::*` reflection-API imports across all workspace `*.rs` files (excluding `target/`, `.claude/`, `OLD/`, `worktrees/`).
3. Cross-check against the existing Cargo dep declarations (using yesterday's `cargo metadata` parse for `kernel/types` reverse-dependency closure: 2 normal revdeps = `macros-reflect` dev-dep, `script-host`).
4. Distinguish production `src/` usage from `tests/` usage and from doc-comment-only mentions.

### 2026-05-21 — initial reflection adoption snapshot (recorder host, Rust 1.92.0)

**Substrate is real, not a stub:**

| Crate | Source LoC | Test LoC | Cargo shape | Purpose |
|---|---:|---:|---|---|
| `kernel/types` | **1,151** across 7 files (`field_descriptor.rs` 178 / `lib.rs` 63 / `reflect.rs` 283 / `schema_version.rs` 95 / `serde_bridge.rs` 165 / `type_id.rs` 202 / `ui_hint.rs` 165) | (its own `tests/reflect_round_trip.rs` is 1 file) | normal deps `serde` / `ron` / `thiserror` (workspace floor only — explicitly no `blake3` / no `inventory` / no `linkme`) | Hand-rolled FNV-1a-128 `TypeId`, closed-set `UiHint`, `Reflect` trait, `FieldDescriptor`, `SchemaVersion`, RON serde bridge via reflection walk |
| `crates/macros-reflect` | **819** (`attrs.rs` 314 / `codegen.rs` 360 / `derive.rs` 60 / `lib.rs` 85) | **301** (5-pilot probe 99 / `derive_test.rs` 82 / `ui_hints_test.rs` 68 / `validate_attr_test.rs` 52) + `fixtures/render_pass.rs` 90 | `proc-macro = true`; normal deps `proc-macro2` / `quote` / `syn`; dev-dep on `rge-kernel-types` | proc-macro emits `impl rge_kernel_types::Reflect` from `#[derive(Reflect)]`; no `darling`, no `proc-macro-crate`, no generic helpers in emitted code |
| `kernel/types/BUDGET.md` | 84 lines | — | — | Phase 1.1 compile-budget baseline document; recorded 2026-05-05 |

The substrate is **complete and well-engineered**. It is NOT empty, NOT a stub, NOT a placeholder. The Phase 1.1 abort gate has been formally measured; see **[`kernel/types/BUDGET.md`](../kernel/types/BUDGET.md)** for the canonical 5-pilot wall-clock (**7.5 s**, ~4× under the 30 s abort), per-field LLVM-line cost (**~23 lines/field**), and the 100-type extrapolation (**~9,000 LLVM lines**, well under the 15,000 warn threshold). Those numbers are not duplicated here; the BUDGET doc remains the source of truth.

**Production-vs-test adoption inventory:**

| Symbol / pattern | Production `src/` uses (workspace, non-test) | Test uses | Doc-comment-only mentions |
|---|---:|---:|---|
| `#[derive(Reflect)]` | **0** | 7 (all in `crates/macros-reflect/tests/`) | n/a |
| `use rge_macros_reflect::*` / `rge_macros_reflect::Reflect` | **0** (no consumer outside macros-reflect itself) | 3 files (`compile_budget_5_pilots.rs`, `fixtures/render_pass.rs`, `macros-reflect/src/lib.rs` doc example) | n/a |
| `use rge_kernel_types::{Reflect,TypeId,FieldDescriptor,SchemaVersion,UiHint,ReflectValue,from_ron,to_ron}` | **0** in production `src/` | 4 test files (`kernel/types/tests/reflect_round_trip.rs` + 3 in `crates/macros-reflect/tests/`) | 2 doc-only mentions: `crates/components-spatial/src/lib.rs:20` (comment saying "callers should `use rge_kernel_types::Entity;`"); `crates/rge-data/src/lib.rs:39,75` (comment promising `pub use rge_kernel_types::Reflect;` is "a one-line change") — **neither actually imports** |
| Cargo declared dep on `rge-kernel-types` (normal lib) | **2 crates** — `rge-macros-reflect` (dev-dep only, used solely by tests), `rge-script-host` (declared but **0 actual `use rge_kernel_types::...` lines in `script-host/src/` or `script-host/tests/`**) | — | — |

**Reflected-type inventory:**

| Type | File | Production / Test | Real semantic identity? |
|---|---|---|---|
| `Pilot1` | `crates/macros-reflect/tests/compile_budget_5_pilots.rs:18` | Test | No — anonymous compile-cost calibration probe (4 fields) |
| `Pilot2` | same file:29 | Test | No — calibration probe (4 fields) |
| `Pilot3` | same file:40 | Test | No — calibration probe (5 fields) |
| `Pilot4` | same file:55 | Test | No — calibration probe (4 fields) |
| `Pilot5` | same file:67 | Test | No — calibration probe (7 fields; exercises all UI-hint variants) |
| `RenderPass` | `crates/macros-reflect/tests/fixtures/render_pass.rs:16` | Test fixture | Mirrors the rustforge `editor-app/RenderPass` shape from W02; **not** wired into any production renderer in the workspace |
| `WithValidate` | same file:59 | Test fixture | No — exercises `validate` / `custom_drawer` attribute plumbing |

**Total reflected types in workspace: 7. Production: 0. Test-only: 7.**

**Phase 9 + §13.x reflection-gate signal status:**

| Gate | Threshold | Today | Signal status |
|---|---|---|---|
| Phase 1.1 abort (IMPLEMENTATION.md:190) | > 30 s on 5 pilot types | 7.5 s (5 pilots; recorded in BUDGET.md) | **PASS, recorded** |
| §13.3 reflection compile-time projection | ≤ 15,000 LLVM lines for 100-type estimate | ~9,000 (extrapolated in BUDGET.md) | **PASS, recorded (extrapolated)** |
| §13.3 generic instantiations / crate | 5,000 warn / 15,000 hard | 0 (macro emits no generic helpers by design) | **PASS** |
| §13.2 reflection cache 1000 components ≤ 2 MB | 2 MB | n/a — no reflection cache deployed; "no global registry" is a hard architectural constraint per `BUDGET.md` constraint #1 | **VACUOUSLY SATISFIED** |
| §13.10 / §1.10.4 reflection schema size metric | > 10 K typed-components × fields = warn | 0 production fields (24 fields total across 5 test-only pilots) | **VACUOUSLY SATISFIED** |
| Phase 9 §9 evaluation axis: 100+ reflected types | qualitative | 0 production types | **STRUCTURALLY UNMEASURABLE** until production adoption begins |

**Status:** **PHASE 9 PREFLIGHT — substrate complete, production adoption zero. Defer.**

- **`kernel/types` is real substrate but not load-bearing in production yet.** The crate is fully implemented (1,151 LoC across 7 source files with proper trait/serde plumbing), the Phase 1.1 compile-budget is recorded and PASS, and the `#[derive(Reflect)]` proc-macro works end-to-end — but no production code path currently consumes any of it.
- **7 reflected types in the workspace, all 7 test-only. 0 production reflected types. 0 production consumers of `rge-macros-reflect`.** The `RenderPass` fixture mirrors the spec's named pilot type (rustforge `editor-app/RenderPass`) but is in `crates/macros-reflect/tests/fixtures/`, not in `crates/gfx/` or `crates/editor-ui/`.
- **The Phase 9 §9 reflection-scale evaluation is structurally unmeasurable until production adoption begins.** With zero production reflected types, neither compile-time-at-100-types nor binary-size-at-100-types can be sampled against any real workload. The §13.2 reflection-cache budget and the §13.10 schema-size metric are vacuously satisfied for the same reason.

**Top 3 honesty gaps (qualitative; baseline-state findings):**

1. **`kernel/types` is documented as "the architectural root" but has zero production consumers today.** IMPLEMENTATION.md Phase 1 §1.1 line 117 says verbatim: "Everything depends on this." Reality: 0 production `.rs` files import any reflection API. The two Cargo revdeps (`macros-reflect`, `script-host`) are either dev-only or declared-but-unused. This is **aspirational framing**, not load-bearing today. (Same crate showed up in yesterday's `## §1.10.4` invalidation-radius preflight at 2.1 % normal-closure — both preflights triangulate the same gap from different angles.)
2. **Phase 9 §9 reflection-scale evaluation has nothing to evaluate.** The §13.3 compile-time scaling table in `BUDGET.md` extrapolates linearly from 5 → 100 types and predicts ~9,000 LLVM lines, well under the warn threshold. But that prediction is **unverified against any production workload**: no production type has ever been reflected, so the per-type LLVM cost in a real consumer crate (which would also link `serde` / `ron` infrastructure separately) is unknown. The Phase 9 gate cannot fire and cannot regress; it can only be unblocked by a real consumer landing first.
3. **`script-host`'s declared `kernel/types` Cargo dep is dead substrate.** `crates/script-host/Cargo.toml` carries `rge-kernel-types = { path = "../../kernel/types" }`, but `crates/script-host/src/**/*.rs` contains zero `use rge_kernel_types::...` lines and `crates/script-host/tests/**/*.rs` is the same. The dep is either a forward-looking declaration awaiting the generic reflect-based hot-reload migration referenced in this BASELINE.md at `Phase 3.2` Notes/caveats ("real-scene swap latency depends on the reflection cost; pending the generic bridge, the 0.31 ms above is a lower bound") or accumulated cruft. Either way it's the **only** workspace-Cargo-graph signal that something outside `macros-reflect` "intends" to use reflection — and that intent is currently un-acted-upon.

**Revisit triggers** — re-run this preflight when **either** of the following becomes true:

1. **Any production crate (non-test) adds its first `#[derive(Reflect)]` derive or its first `use rge_kernel_types::Reflect` (or other reflection API) import.** This signals real adoption pressure has begun and the Phase 9 §9 evaluation axis becomes meaningfully measurable.
2. **`script-host` actually wires its declared `kernel/types` Cargo dep into the generic hot-reload migration path** (i.e. replaces the hand-rolled `CounterSnapshot` per this BASELINE.md's Phase 3.2 Notes/caveats with a `Reflect`-driven value-walk). This signals the canonical "generic bridge" consumer referenced in the existing baseline is materializing.

Until **at least one** of those fires, treat the reflection substrate as observed-deployed-but-unused, **defer any reflection adoption work**, and **do not add new pilot types** — adding more synthetic pilots would conflate compile-budget calibration with adoption signal. The substrate's correctness is already proven by the existing 7 test-only types + the `kernel/types/tests/reflect_round_trip.rs` round-trip test; further calibration is only warranted once a real consumer dictates the value-walk shape (inspector vs hot-reload-migration vs asset-metadata vs component-RON have different optimal trait surfaces).

**Notes / caveats:**

- The "tiny adoption task" path was explicitly considered and rejected per the user directive after the preflight ("document and defer"). The closest candidates were: (a) **editor inspector widget** consuming `Reflect` for `Slider` / `ColorRgb` / `FilePath` UI hints — but no `inspector.rs` exists in `crates/editor-ui/src/widgets/` today; (b) **`script-host` generic hot-reload migration** — substantive substrate dispatch, not "tiny"; (c) **`rge-data` `pub use rge_kernel_types::Reflect;`** per the doc-comment promise at `crates/rge-data/src/lib.rs:39,75` — would be a one-line edit but landing it without a simultaneous consumer would be premature mechanism per PLAN §1.10's pressure-driven doctrine.
- This preflight does NOT propose shrinking or simplifying `kernel/types`. The substrate is healthy and well-bounded (the BUDGET.md constraints — no global registry / no generic helpers in derive output / no heavy hash crate / `UiHint` serialize-only — are load-bearing). Shrinking it before a consumer materializes would risk later having to re-add what was removed, at higher cost.
- The 95-vs-94 workspace-crate count discrepancy noted in the §1.10.4 preflight is also visible here in the form of `kernel/types`-related crates: the workspace has 1 macro crate (`macros-reflect`) and 1 reflection-substrate crate (`kernel/types`); no reflection-consuming production crate exists. Both counts agree with `cargo metadata`.
- Reproducer for the consumer inventory (read-only grep, no harness in-tree):
  ```
  # production-reflect-derives (expect zero outside crates/macros-reflect/tests/):
  rg "#\[derive\([^)]*\bReflect\b" --type rust
  # kernel/types reflection-API imports outside its own tests + macros-reflect tests:
  rg "use rge_kernel_types::(Reflect|TypeId|FieldDescriptor|SchemaVersion|UiHint|ReflectValue|from_ron|to_ron)" --type rust
  # macros-reflect imports outside macros-reflect itself:
  rg "use rge_macros_reflect::" --type rust
  ```
- This preflight is read-only and complementary to (not a replacement for) `kernel/types/BUDGET.md`. The BUDGET doc owns Phase 1.1 compile-budget numbers and their re-running instructions; this entry owns the **adoption** baseline and the two-arm revisit trigger. They should be re-read together when either trigger fires.

---

## Editor-usability preflight (Phase 9)

**Budget anchors and gate references:**

- IMPLEMENTATION.md Phase 9 §9 (line 600): "Editor usability — friction points from real authoring."
- IMPLEMENTATION.md Phase 5 §5.1–§5.2 (lines 374, 384): `editor-shell` + `editor-state` (narrow per §1.15).
- IMPLEMENTATION.md Phase 2 §2.2 (line 217): `editor-actions` (Command Bus) — **VERY EARLY**.
- PLAN.md §1.15: editor-state coordination-not-authority rule (selection / hover / active-tool only; no authoritative content types).
- PLAN.md §6.16.7: Command Bus 500 ms coalesce semantics.

**This entry is a Phase 9 PREFLIGHT — pure read-only audit of the editor's actual user-facing surface.** It does NOT add commands, wire keyboard handlers, build scene serialization, or change any substrate. It is the first recorded *editor-usability* baseline (distinct from the Phase 5 / Phase 6 substrate-closure records already in this doc) so future user-loop landings have an honest before-and-after reference.

**Methodology (read-only):**

1. Inventory of `[[bin]]` entry points and `src/main.rs` across `crates/editor-*/`, `editor/`, `apps/`, `tools/`.
2. Read `lib.rs` + key modules + `Cargo.toml` of `editor-shell`, `editor-ui`, `editor-actions`, `editor-state`, `components-editor`, `anim-graph-editor`, `material-graph-editor`.
3. Grep across `editor-*` for: `open_file`, `load_project`, `save_project`, `.rge` / `.rgeproj` / `.scene` / `.project` file-extension string literals, `unimplemented!()` / `todo!()` / stub markers, `KeyboardInput` event branches, call sites of `io-gltf` / `io-image` / `io-3mf` public APIs.
4. Cross-check the editor's call graph against the `CommandBus::submit` / `Action::apply` / `Action::revert` signatures to determine whether user-visible CAD mutations can flow through the existing bus.
5. Test inventory across `editor-*` (`#[test]` count + integration vs unit breakdown + workflow coverage).

### 2026-06-08 - Editor-shell extension-command executor seam

**Forward-only follow-up (ISSUE-349 / task 102).** Narrows the gap after
extension menu command capture without wiring a real plugin stack.
`EditorShell::route_menu_command` remains the menu-route owner: core commands
stay on the existing shell/document paths, while `Command::Custom` and
`Command::Plugin` activations are captured first and then drained to an
injected `ExtensionCommandHandler` when one is configured.

**Now shipped - injectable shell seam only.**
- `lifecycle::extension_command` defines the handler trait, handled/unhandled
  result, non-fatal error type, and observable seam events.
- `EditorShell` owns an optional handler plus event FIFO; missing-handler
  activations remain observable through the retained extension-command FIFO
  and explicit missing-handler events.
- Handler `Unhandled` and failure results are non-fatal, recorded as events,
  and do not prevent later extension commands from being delivered.
- Shell tests use a synthetic handler to prove FIFO delivery, missing-handler
  observability, failure/unhandled continuation, and core-command
  non-delivery for representative Save and Toggle Command Palette routes.

**Still open - explicitly NOT closed here:** real plugin runtime, plugin
discovery, plugin loading, WASM execution, capability manifests, async
execution, sandbox integration, plugin registration UX beyond existing menu
entries, host-to-shell FIFO replacement, generalized registry execution,
keybinding editor behavior, CAD mutation, clipboard behavior, Cargo,
workflows, scheduler, architecture-lint, or dispatch automation behavior.

**Scope:** `editor-shell` lifecycle/render-path routing state and tests plus
top-level status/task bookkeeping; no `editor-ui`, no `editor-egui-host`, no
plugin/runtime/kernel-plugin-host crates, no Cargo/workflow/scheduler/
automation behavior.

### 2026-06-08 - Phase 9 editor-usability task-102 selection audit

**Source-read selection result (ISSUE-347).** Current source supports one
bounded next implementation task: add an editor-shell extension-command
executor seam for already-captured `Command::Custom` / `Command::Plugin`
menu activations. This is intentionally smaller than real plugin runtime,
discovery, loading, sandboxing, or generalized registry execution.

**Current evidence.**
- `git grep -n "pub fn route_menu_command" -- crates/editor-* editor/`
  returns exactly one definition:
  `crates/editor-shell/src/render_path.rs:415`. The surrounding source is
  inside `impl EditorShell`; the correct ownership is
  `EditorShell::route_menu_command`.
- `git grep -n -E "drain_extension_menu_commands|extension_menu_commands|future plugin/action executor|extension menu command captured" -- crates/editor-shell/src/lifecycle/mod.rs crates/editor-shell/src/render_path.rs crates/editor-shell/src/lifecycle/tests.rs`
  shows the current FIFO, the one-shot drain, and the test that proves
  extension commands are retained for a future executor.
- `git grep -n -E "PluginHost|PluginContext|runtime-wasmtime|plugin-discovery|rge_kernel_plugin_host|rge-runtime" -- crates/editor-shell editor/rge-editor`
  exits 1 with no matches. The editor surface has no current plugin runtime,
  discovery, or loading path to wire in one bounded step.

**Candidate comparison.**
- Plugin/extension command execution policy beyond capture is the smallest
  open gap: the menu/host/shell path already carries extension `Command`
  values to the shell and currently stops at an inert FIFO.
- Host-shell FIFO/menu-click replacement and generalized registry execution
  would cut across the menu architecture and command routing model; defer.
- Conflict resolution/keybinding editor/fatal gating has diagnostics and
  conflict projection, but a product policy/editor UI is still broader than
  one small implementation task.
- Persistent command-palette history/favorites is explicitly open after task
  100, but persistence/favorites need storage and UX choices; the in-memory
  recent-ordering slice already shipped.
- Unsaved quit has dirty-state observation and quit/close handlers, but
  prompting and graceful shutdown semantics require UI/dialog policy.
- OS clipboard / typed clipboard remains broader than the shell-local
  legacy-blob clipboard; typed/CAD identity cloning is explicitly out of the
  current clipboard path.
- CAD delete/duplicate/undo integration remains authoritative-content work:
  current Delete/Duplicate/Cut/Copy/Paste are wrapper-world legacy-blob
  operations, not CAD graph/projection/render identity mutations through the
  command bus.
- Broader camera UI is beyond the current reset/frame/zoom commands; orbit,
  pan, and richer camera controls need interaction design.

**Selected follow-up.** Task 102 is the editor-shell extension-command
executor seam. It should define the shell-owned execution policy and test
FIFO, unhandled, and failure behavior with an injected handler. It must not
wire real plugin runtime/discovery/loading, add Cargo dependencies, replace
the host-to-shell menu FIFO, or rename the route-menu owner away from
`EditorShell::route_menu_command`.

### 2026-06-07 - First full-automation batch readiness reconciliation

**Docs-only reconciliation after tasks 74-80.** Tasks 74-80 are complete on `main`, and the command-palette keyboard navigation work is complete on `main` including filter-edit selection reset / filter-change reset: search-filter edits restart selection at the first enabled filtered result. This supersedes earlier dated still-open references to richer command-palette keyboard navigation; those older notes remain historical records, not current readiness blockers.

**Automation readiness.** GitHub issue #319 was manually salvaged and closed, so it is no longer an open autonomous failure blocker. The first guarded full-automation batch subsequently completed: ISSUE-320 was published at `58ec48a`, and task 81 was marked done at `867f026`. Post-run audit state is idle: no open `ai-dispatch` issues, no open `ai-dispatch-failed` issues, `.ai/dispatch.auto-halt` absent, and no remaining uncompleted task headings in `.ai/dispatch.tasks.md`. The archived interrupted first attempt at `A:\rcad\dispatch-worktrees\ISSUE-320.interrupt1` was abandoned/replaced by the successful retry and removed locally.

**Non-goals preserved.** This reconciliation does not register, arm, or modify scheduler state; does not create a standing `PublishMode main` authorization; does not change default publish mode, queue policy, guard policy, task selection, or `.ai/dispatch.tasks.md`; and does not change source, Cargo, workflow, automation, schema, or scheduler files.

### 2026-06-08 - Command palette recent ordering

**Forward-only follow-up (MENU-COMMAND-PALETTE-RECENT).** Closes the smallest command-history slice for the existing `editor-egui-host` command palette without persisting state, adding a second command model, or changing activation routing. The palette still operates over current menu projection rows and still returns commands for host-side `MenuCommandHandoff` enqueueing.

**Now shipped - host-local recent ordering for blank filters.**
- `EguiHost` owns an in-memory most-recent-first list of command-palette activation ids, stored as `Command::diagnostic_id()` strings and capped at 16.
- Successful command-palette activations record the id at the existing palette return -> `MenuCommandHandoff` enqueue point. Main-menu activations do not update palette recents.
- Re-recording an existing id moves it to the front without duplication, then the list is truncated to the cap.
- Blank or whitespace-only filters promote currently projected, enabled recent commands first, in recent-list order, then append every remaining projected row in original order.
- Stale recent ids are ignored. Disabled recent rows are not promoted, but remain visible in the original-order remainder when still projected.
- Non-blank filters ignore recency and keep the task-98 exact word/field, prefix, substring, fuzzy ordered-subsequence score ordering and original-order tie-breaks.
- Host tests pin bounded de-duplication, stale-id ignoring, blank-filter recent ordering, disabled-row remainder behavior, and unchanged non-blank fuzzy ordering with recents present.

**Still open - explicitly NOT closed here:** persistent command history/favorites, a second command model, plugin runtime/action execution beyond FIFO enqueue, host->shell FIFO replacement, keybinding editor, generalized conflict-resolution UI, Cargo, scheduler, architecture-lint rule/config behavior, dispatch automation behavior, and task arming.

**Scope:** `editor-egui-host` palette state/filter helpers/tests plus top-level status docs and task-list bookkeeping; no `editor-ui`, no `editor-shell`, no plugin runtime, no persistence, no Cargo/workflow/scheduler/automation behavior.

### 2026-06-08 - Command palette fuzzy matching

**Forward-only follow-up (MENU-COMMAND-PALETTE-FUZZY).** Closes the command-palette fuzzy matching/scoring gap in the existing `editor-egui-host` palette filter without inventing a second command model or changing activation routing. The palette still filters the already-projected menu rows and still returns commands for host-side `MenuCommandHandoff` enqueueing.

**Now shipped - deterministic fuzzy search.**
- `filter_command_palette_entries()` still returns blank filters in original projected menu order.
- Non-blank filters still require every whitespace-separated term to match the same row across menu-path label, shortcut display, or `Command::diagnostic_id()`.
- Exact word/field, prefix, and substring matches remain ranked ahead of fuzzy ordered-subsequence matches.
- Fuzzy matches use a deterministic score key based on match class, fuzzy gap/span compactness, matched field priority, label length, and original menu order.
- Fuzzy matching covers label text, shortcut display such as `Ctrl+Shift+P`, and diagnostic ids such as `toggle_command_palette`.
- Host tests pin label/shortcut/diagnostic-id fuzzy-only matches, exact/prefix/substring outranking fuzzy-only matches, stable fuzzy compactness ordering, and no-match behavior.
- The cohesive host-menu test module now carries a file-local `// SPLIT-EXEMPTION:` annotation because the added palette coverage takes it past the 1000-line architecture-lint threshold; no architecture-lint rule/config behavior changed.

**Still open - explicitly NOT closed here:** command history, a separate command model, plugin runtime/action execution beyond FIFO enqueue, host->shell FIFO replacement, keybinding editor, generalized conflict-resolution UI, Cargo, scheduler, architecture-lint rule/config behavior, dispatch automation behavior, and task arming.

**Scope:** `editor-egui-host` palette filter helpers/tests plus top-level status docs and task-list bookkeeping; no `editor-ui`, no `editor-shell`, no plugin runtime, no Cargo/workflow/scheduler/automation behavior.

### 2026-06-07 - Command palette keyboard navigation polish

**Forward-only follow-up (MENU-COMMAND-PALETTE-KEYBOARD-NAV).** Narrows the remaining command-palette keyboard usability gap without introducing fuzzy scoring, command history, a separate command model, or a new execution path. The palette still operates over the already-projected menu rows and still returns commands for `MenuCommandHandoff` enqueueing by the host.

**Now shipped - selected-row keyboard model and visibility.**
- `command_palette_selected_index()` normalizes filtered-row selection to an enabled row, preserving a still-valid current selection or falling back to the first enabled row.
- `ArrowDown` / `ArrowUp` move through enabled filtered rows with wrap-around and skip disabled rows.
- `Enter` activates the currently selected enabled row instead of always using the first enabled match; stale or disabled selection still falls back through the same normalization helper.
- Editing the search filter restarts selection at the first enabled row in the new filtered result set, so a numeric row index from the prior filter is not preserved against different rows.
- Opening the palette via `EguiHost::toggle_command_palette()` arms a one-shot search-field focus request. The search field consumes it on the next render, and close / activation / closed-window paths clear it so later frames do not steal focus.
- The selected palette row now uses egui's selected button affordance and calls `scroll_to_me(Some(egui::Align::Center))` inside a bounded results scroll area so keyboard navigation keeps the active row visible.
- Host tests pin selection normalization, filter-change reset, ArrowUp / ArrowDown wrap + disabled skipping, selected-row Enter activation, one-shot focus consumption, and the selected-row enabled-only predicate.

**Still open - explicitly NOT closed here:** fuzzy matching/scoring, command history, a separate command model, plugin runtime/action execution beyond FIFO enqueue, host->shell FIFO replacement, keybinding editor, generalized conflict-resolution UI, Cargo, scheduler, dispatch automation, and task arming.

**Scope:** `editor-egui-host` palette helpers/render state/tests plus top-level status docs; no `editor-ui`, no `editor-shell`, no plugin runtime, no Cargo/workflow/scheduler/automation behavior, and no dispatch task-brief edit.

### 2026-06-06 - Command palette window extraction

**Forward-only follow-up (MENU-COMMAND-PALETTE-WINDOW-EXTRACT).** Relieves `editor-egui-host/src/lib.rs` line-cap pressure after the palette filter/keyboard slices. The extraction is behavior-preserving and keeps the host as the owner of palette state and command enqueueing.

**Now shipped - palette presentation extraction.**
- `menu::command_palette_window(...)` owns the egui window body, filter rendering, empty state, Enter/Escape handling, click handling, and filter clearing.
- `EguiHost::render` now calls the helper and only pushes the returned command into `MenuCommandHandoff`.
- `editor-egui-host/src/lib.rs` drops back under the line cap with room for later host work.

**Still open - explicitly NOT closed here:** arrow-key selection cursor, fuzzy matching/scoring, command history, a separate command model, plugin runtime/action execution beyond FIFO enqueue, host->shell FIFO replacement, and conflict resolution/keybinding editor/fatal gating.

**Scope:** `editor-egui-host` menu/render extraction plus top-level status docs; no behavior change, no tests beyond existing coverage, no `editor-ui`, no `editor-shell`, no plugin runtime, no Cargo, scheduler, dispatch automation, or task arming.

### 2026-06-06 - Command palette keyboard basics

**Forward-only follow-up (MENU-COMMAND-PALETTE-KEYBOARD).** Narrows the command-palette keyboard gap without adding a selection cursor or a new command model. Keyboard activation uses the same filtered result list and the same `MenuCommandHandoff` activation path as mouse clicks.

**Now shipped - basic palette keyboard handling.**
- `Escape` closes the `Command Palette` window without dispatching a command.
- `Enter` activates the first enabled row in the current filtered result set.
- Disabled rows stay visible but are skipped by keyboard activation, matching `menu_item` click behavior.
- Host tests pin first-enabled selection and disabled-only no-dispatch behavior through a pure helper.

**Still open - explicitly NOT closed here:** arrow-key selection cursor, fuzzy matching/scoring, command history, a separate command model, plugin runtime/action execution beyond FIFO enqueue, host->shell FIFO replacement, and conflict resolution/keybinding editor/fatal gating.

**Scope:** `editor-egui-host` palette render/helper/tests plus top-level status docs; no `editor-ui` default-menu change, no `editor-shell` routing change, no plugin runtime, no Cargo, scheduler, dispatch automation, or task arming.

### 2026-06-06 - Command palette filter

**Forward-only follow-up (MENU-COMMAND-PALETTE-FILTER).** Narrows the command-palette gap from a static list to a searchable host-local view with deterministic result ordering. The filter works over the already-projected menu rows, so it does not create a second command model or alter activation semantics.

**Now shipped - basic command-palette filtering and ordering.**
- `EguiHost` owns `command_palette_filter` beside `command_palette_open`.
- `toggle_command_palette`, close, and command activation clear the filter so stale queries do not carry into the next palette invocation.
- `filter_command_palette_entries()` matches whitespace-separated terms against menu-path label, shortcut display, and `Command::diagnostic_id()`, then orders matches by exact word/field match, prefix match, substring match, and original menu order.
- The `Command Palette` window renders a search field and shows only matching enabled/disabled rows; activation still enqueues through `MenuCommandHandoff`.
- Host tests pin blank filters, shortcut search (`Ctrl+Shift+P`), diagnostic-id search (`toggle_command_palette`), multi-term matching, exact-word ordering, and no-match behavior.

**Still open - explicitly NOT closed here:** fuzzy matching/scoring, command history, a separate command model, richer palette keyboard navigation, plugin runtime/action execution beyond FIFO enqueue, host->shell FIFO replacement, and conflict resolution/keybinding editor/fatal gating.

**Scope:** `editor-egui-host` palette state/render/filter helper/tests plus top-level status docs; no `editor-ui` default-menu change, no `editor-shell` routing change, no plugin runtime, no Cargo, scheduler, dispatch automation, or task arming.

### 2026-06-06 - Command palette menu binding

**Forward-only follow-up (MENU-COMMAND-PALETTE-BINDING).** Narrows the command-palette gap from "window exists but has no default entry or accelerator" to a reachable menu command. The binding uses the existing canonical menu + accelerator execution path, not a new keybinding table.

**Now shipped - default command-palette entry and shortcut.**
- `default_editor_menu` registers `View -> Command Palette` before the camera commands.
- The entry dispatches `Command::ToggleCommandPalette` with executable `Ctrl+Shift+P` and no enablement predicate, so the palette remains reachable while PIE is active.
- `editor-shell` keyboard parity proves `Ctrl+Shift+P` translates through `keycode_to_shortcut` and resolves through the canonical menu, while `EditorKeyCommand::from_key_press` still does not shadow it.
- Host projection tests pin the View row/shortcut display and move synthetic plugin fixture shortcuts to `Ctrl+Alt+M`, reserving `Ctrl+Shift+P` for the core palette binding.
- The default executable accelerator set is conflict-free at 18 bindings.

**Historical non-closure for this slice:** fuzzy search, text input/filtering (closed by the filter section above), ranking, command history, a separate command model, richer palette keyboard navigation, plugin runtime/action execution beyond FIFO enqueue, host->shell FIFO replacement, and conflict resolution/keybinding editor/fatal gating.

**Scope:** `editor-ui` default View menu/tests, `editor-shell` accelerator parity/routing comments/tests, `editor-egui-host` projection tests, and top-level status docs; no palette search/filter model, plugin runtime, Cargo, scheduler, dispatch automation, or task arming.

### 2026-06-06 - Menu-backed command palette window

**Forward-only follow-up (MENU-COMMAND-PALETTE-WINDOW).** Narrows the command-palette gap past the request latch. The palette is now a real egui window, but it intentionally reuses the menu registry projection rather than inventing a second command model.

**Now shipped - minimal command palette surface.**
- `EguiHost` owns `command_palette_open` plus `toggle_command_palette()` / `is_command_palette_open()`.
- `EditorShell` consumes the one-shot `Command::ToggleCommandPalette` request immediately before `EguiHost::render` in both the cuboid and egui-only render paths.
- `editor-egui-host::menu::command_palette_entries()` flattens the live `ProjectedMainMenu` into File/Edit/Play/View/Plugins command rows, preserving menu-path labels, shortcut display, command identity, and enablement.
- The egui `Command Palette` window renders those rows and enqueues the clicked enabled command through `MenuCommandHandoff`, then closes.
- Tests prove palette projection includes plugin entries, preserves disabled state, and the public host API remains pinned.

**Historical non-closure for this slice:** fuzzy search, text input/filtering (closed by the filter section above), ranking, command history, a separate command model, default menu entry or shortcut binding to open the palette (closed by the follow-up section above), richer keyboard navigation, plugin runtime/action execution beyond FIFO enqueue, host->shell FIFO replacement, and conflict resolution/keybinding editor/fatal gating.

**Scope:** `editor-egui-host` palette projection/render state/tests, `editor-shell` host-toggle consumption, and top-level status docs; no `editor-ui` default-menu change, plugin runtime, Cargo, scheduler, dispatch automation, or task arming.

### 2026-06-06 - Command palette toggle request

**Forward-only follow-up (MENU-COMMAND-PALETTE-REQUEST).** Narrows the command-palette gap without pretending the palette exists. The `Command::ToggleCommandPalette` variant already existed, but after extension-command capture it was the lone meaningful core command that still vanished at `route_menu_command`.

**Now shipped - one-shot palette toggle request.**
- `EditorShell` owns a pending command-palette toggle flag.
- `route_menu_command` routes `Command::ToggleCommandPalette` to `handle_command_palette_toggle_request()`.
- `take_command_palette_toggle_request()` consumes the request once, matching the existing quit-request latch shape.
- Shell tests prove the request is set and single-consume, does not run document handlers, and is not captured as an extension command.

**Historical non-closure for this slice:** command-palette UI (closed by the follow-up window section above), command search/list model, default menu entry or shortcut binding for the palette (closed by the follow-up binding section above), plugin action execution/routing policy beyond capture, plugin runtime/discovery/loading, host->shell FIFO menu-click replacement, generalized command execution beyond the existing menu command router, and conflict resolution/keybinding editor/fatal gating.

**Scope:** `editor-shell` routing/state/tests plus top-level status docs; no `editor-ui` default-menu change, `editor-egui-host` projection change, command-palette UI, plugin runtime, Cargo, scheduler, dispatch automation, or task arming.

### 2026-06-06 - Extension menu command capture

**Forward-only follow-up (MENU-EXTENSION-COMMAND-CAPTURE).** Narrows the registration-to-execution gap opened by the optional Plugins menu and generic registration hooks. Registered extension entries can already reach the shell through `MenuCommandHandoff`; this beat prevents their `Command::Custom` / `Command::Plugin` activations from disappearing at `EditorShell::route_menu_command`.

**Now shipped - shell-side extension command capture.**
- `EditorShell` owns a shell-local FIFO for extension menu commands.
- `route_menu_command` captures `Command::Custom` and `Command::Plugin` into that FIFO, logging the stable `diagnostic_id()` instead of treating them as ordinary ignored commands.
- `EditorShell::drain_extension_menu_commands()` exposes a one-shot FIFO drain for a future plugin/action executor.
- Shell tests prove `Plugin` then `Custom` preserves FIFO order, the drain is one-shot, no document handlers fire, and the unrouted core `ToggleCommandPalette` remains a no-op that is not captured as an extension command.

**Still open - explicitly NOT closed here:** plugin action execution/routing policy beyond capture, plugin runtime/discovery/loading, command-palette integration, new default extension menu entries, host->shell FIFO menu-click replacement, generalized command execution beyond the existing menu command queue plus extension capture, and conflict resolution/keybinding editor/fatal gating.

**Scope:** `editor-shell` routing/state/tests plus top-level status docs; no `editor-ui` registry semantics, `editor-egui-host` registration/projection changes, plugin runtime, command-palette UI, Cargo, scheduler, dispatch automation, or task arming.

### 2026-06-06 - Generic menu extension registration

**Forward-only follow-up (MENU-EXTENSION-REGISTRATION).** Narrows the host registration surface beyond the Plugins-only convenience method. The live `EguiHost` can now register extension entries against any declared menu extension point in its owned registry.

**Now shipped - generic menu registration.**
- `EguiHost::register_menu_entry(&ExtensionPoint, MenuEntry) -> Result<(), RegistryError>` registers against any declared point in the host's canonical menu registry.
- `EguiHost::register_plugin_menu_entry` now delegates through the generic method using `plugins_menu_point()`.
- Host tests register a synthetic extension entry into File, assert it projects through the host menu surface, and keep the Plugins projection/FIFO/duplicate-id coverage intact.
- The public API smoke pins both the generic method and the Plugins convenience method without constructing a GPU-backed host.

**Still open - explicitly NOT closed here:** plugin action execution/routing policy, `editor-shell` handling for `Command::Plugin` or `Command::Custom`, plugin runtime/discovery/loading, command-palette integration, host->shell FIFO menu-click replacement, generalized command execution beyond the host menu command queue, broader camera UI beyond reset/frame/zoom, and conflict resolution/keybinding editor/fatal gating.

**Scope:** `editor-egui-host` menu registration helper/API/tests, public API smoke, and top-level status docs; no new default menu entries, no `editor-ui` default-menu behavior change, no `editor-shell` routing, no plugin runtime, no plugin discovery, no command palette, no keybinding editor, no FIFO replacement, no Cargo, scheduler, dispatch automation, or task arming.

### 2026-06-06 - Plugin menu registration hook

**Forward-only follow-up (MENU-PLUGIN-REGISTRATION).** Narrows the remaining plugin menu registration UX gap from the optional Plugins projection slice. The live `EguiHost` now exposes a production registration hook that writes plugin-provided entries into the host-owned Plugins menu registry that `render()` already resolves each frame.

**Now shipped - plugin menu registration.**
- `EguiHost::register_plugin_menu_entry(MenuEntry) -> Result<(), RegistryError>` registers into the canonical `plugins_menu_point()`.
- The host API keeps the registration surface on the live `EguiHost` rather than only in tests that manually own a `MenuRegistry`.
- The existing render path remains unchanged: registered entries project into `ProjectedMainMenu::plugins`, the top-level Plugins menu stays hidden while empty, and activation enqueues the entry's `Command` through `MenuCommandHandoff`.
- Host tests register a synthetic `Command::Plugin`, assert projection + FIFO round-trip, and assert duplicate plugin entry ids return `RegistryError::DuplicateEntryId`.
- The public API smoke pins the method signature without constructing a GPU-backed host.

**Still open - explicitly NOT closed here:** plugin action execution/routing policy, `editor-shell` handling for `Command::Plugin`, plugin runtime/discovery/loading, command-palette integration, host->shell FIFO menu-click replacement, generalized registry execution beyond the now-wired menu command queue, broader camera UI beyond reset/frame/zoom, and conflict resolution/keybinding editor/fatal gating.

**Scope:** `editor-egui-host` menu registration helper, `EguiHost` public API, projection/FIFO/duplicate-id tests, public API smoke, and top-level status docs; no `editor-ui` default-menu behavior change, no `editor-shell` routing, no plugin runtime, no plugin discovery, no command palette, no keybinding editor, no FIFO replacement, no Cargo, scheduler, dispatch automation, or task arming.

### 2026-06-06 - File Quit app exit

**Forward-only follow-up (MENU-FILE-QUIT).** Narrows the File menu surface with a bounded application-exit command. `Quit` now has a visible File menu entry, an executable `Ctrl+Q` accelerator, and a shell route that requests app exit without owning the winit event loop.

**Now shipped - File Quit.**
- `default_editor_menu` registers File -> Quit after Close, with `Command::Quit` and executable `Ctrl+Q`.
- Quit has no enablement predicate and remains available while PIE is active; document-mutating File commands still gate on Editing.
- `EditorShell::route_menu_command` routes `Command::Quit` to `handle_quit_request()`, which records a one-shot pending app-exit request.
- `window_event` consumes the request at the event-loop boundary and calls the same `ActiveEventLoop::exit()` path used by `WindowEvent::CloseRequested`.
- Quit does not close/reset the current document and does not clear the adopted save source.
- Registry, host projection/FIFO, host enablement, keyboard-bridge parity, and shell routing tests pin the behavior.

**Still open - explicitly NOT closed here:** unsaved-changes prompt/confirmation, graceful shutdown save flow, creating a file or project on disk, choosing templates, Save-As to a new `.rge-project` tree beyond the existing path, OS/system clipboard integration, authoritative CAD graph/projection/render deletion or duplication, undo/redo and dirty-state integration for File Close/Quit and Edit content mutations, plugin action execution/registration UX beyond the optional Plugins projection, command-palette integration, host->shell FIFO menu-click replacement, generalized registry execution beyond the now-wired canonical menu commands, broader camera UI beyond reset/frame/zoom, and conflict resolution/keybinding editor/fatal gating.

**Scope:** `editor-ui` default File menu entries/tests, `editor-egui-host` projection/enablement tests, `editor-shell` Quit routing/event-boundary request/tests, and top-level status docs; no prompt, dialog, disk I/O, project/template creation, document reset, CAD graph mutation, projection-cache invalidation, render-mesh invalidation, CommandBus action, undo stack, dirty-state semantics, OS clipboard, plugin runtime, command palette, keybinding editor, FIFO replacement, Cargo, scheduler, dispatch automation, or task arming.

### 2026-06-06 - File Close current document

**Forward-only follow-up (MENU-FILE-CLOSE).** Narrows the File menu surface with a bounded document-close command. `Close` now has a visible File menu entry, an executable `Ctrl+W` accelerator, and a shell route that reuses the existing `replace_world(KernelWorld::new())` reset substrate without exiting the application.

**Now shipped - File Close.**
- `default_editor_menu` registers File -> Close after Save As New Project, with `Command::Close` and executable `Ctrl+W`.
- Close is enabled only while Editing, like New/Open/Save/Save-As. The shortcut remains bound for display but disabled contexts do not execute it.
- `EditorShell::route_menu_command` routes `Command::Close` to `handle_close_file_request()`, which resets to a fresh unsourced empty world through `replace_world`.
- The reset clears the adopted save source, entity selection, shell-local clipboard, render content, PIE snapshot, and command bus; the fresh world gets default `TimeScale`.
- Registry, host projection/FIFO, host enablement, keyboard-bridge parity, and shell routing tests pin the behavior.

**Still open - explicitly NOT closed here:** app quit/exit behavior, unsaved-changes prompt/confirmation, creating a file or project on disk, choosing templates, Save-As to a new `.rge-project` tree beyond the existing path, OS/system clipboard integration, authoritative CAD graph/projection/render deletion or duplication, undo/redo and dirty-state integration for File Close and Edit content mutations, plugin action execution/registration UX beyond the optional Plugins projection, command-palette integration, host->shell FIFO menu-click replacement, generalized registry execution beyond the now-wired canonical menu commands, broader camera UI beyond reset/frame/zoom, and conflict resolution/keybinding editor/fatal gating.

**Scope:** `editor-ui` default File menu entries/tests, `editor-egui-host` projection/enablement tests, `editor-shell` Close routing/tests, and top-level status docs; no app exit, prompt, dialog, disk I/O, project/template creation, CAD graph mutation, projection-cache invalidation, render-mesh invalidation beyond the existing reset, CommandBus action, undo stack, dirty-state semantics, OS clipboard, plugin runtime, command palette, keybinding editor, FIFO replacement, Cargo, scheduler, dispatch automation, or task arming.

### 2026-06-06 - Edit Cut selected entities

**Forward-only follow-up (MENU-EDIT-CUT).** Narrows the generalized Edit menu execution and clipboard surface with a bounded shell-local Cut command. `Cut` now has a visible Edit menu entry, an executable `Ctrl+X` accelerator, and a shell route that composes the Copy/Paste legacy-blob clipboard substrate with the existing wrapper-world Delete path.

**Now shipped - Edit Cut.**
- `default_editor_menu` registers Edit -> Cut between Select All and Copy, with `Command::Cut` and executable `Ctrl+X`.
- Cut is enabled only while Editing with a non-empty entity selection. The shortcut remains bound for display but disabled contexts do not execute it.
- `EditorShell::route_menu_command` routes `Command::Cut` to `cut_selected_entities()`, which clones selected legacy component blobs into the shell-local clipboard and then delegates deletion to `delete_selected_entities()`.
- Cut removes selected wrapper-world entities, clears entity selection, prunes face selection for deleted entities, and leaves the shell-local clipboard available for Paste. A later Paste creates fresh wrapper-world entities from the copied blobs.
- Registry, host projection/FIFO, host enablement, keyboard-bridge parity, and shell routing tests pin the behavior, including Paste-after-Cut.

**Still open - explicitly NOT closed here:** OS/system clipboard integration, typed kernel component cloning, authoritative CAD graph/projection/render cut/copy/paste, undo/redo and dirty-state integration for Cut/Copy/Paste, authoritative CAD graph/projection/render deletion/duplication, plugin action execution/registration UX beyond the optional Plugins projection, command-palette integration, host->shell FIFO menu-click replacement, generalized registry execution beyond the now-wired canonical menu commands, broader camera UI beyond reset/frame/zoom, and conflict resolution/keybinding editor/fatal gating.

**Scope:** `editor-ui` default Edit menu entries/tests, `editor-egui-host` projection/enablement tests, `editor-shell` wrapper-world cut/routing/tests, and top-level status docs; no OS clipboard, typed ECS clone, CAD graph mutation, projection-cache invalidation, render-mesh invalidation, CommandBus action, undo stack, dirty-state semantics, plugin runtime, command palette, keybinding editor, FIFO replacement, Cargo, scheduler, dispatch automation, or task arming.

### 2026-06-06 - Edit Copy/Paste selected entities

**Forward-only follow-up (MENU-EDIT-COPY-PASTE).** Narrows the generalized Edit menu execution and clipboard surface with bounded shell-local Copy/Paste commands. `Copy` and `Paste` now have visible Edit menu entries, executable `Ctrl+C` / `Ctrl+V` accelerators, and shell routes over the same legacy-blob wrapper-world substrate used by Duplicate.

**Now shipped - Edit Copy/Paste.**
- `default_editor_menu` registers Edit -> Copy and Edit -> Paste after Select All, with `Command::Copy` / `Command::Paste` and executable `Ctrl+C` / `Ctrl+V`.
- Copy is enabled only while Editing with a non-empty entity selection. Paste is enabled only while Editing with a non-empty shell-local entity clipboard. The shortcuts remain bound for display but disabled contexts do not execute them.
- `PredicateContext` carries `has_clipboard_entities`, filled by `EditorShell::predicate_context()` from the shell-local clipboard.
- `World::clone_entity_blobs` and `World::spawn_with_component_blobs` provide the shared legacy-blob clone/spawn substrate; `World::duplicate_entity_blobs` reuses that substrate.
- `EditorShell::route_menu_command` routes `Command::Copy` to `copy_selected_entities()` and `Command::Paste` to `paste_copied_entities()`. Copy stores cloned legacy component blobs in a shell-local clipboard; Paste spawns fresh wrapper-world entities, selects the pasted entities, and clears face selection because no authoritative face-ID remapping exists.
- `replace_world` clears the shell-local clipboard, so File New and scene Open drop stale copied entities with the old world.
- Registry, host projection/FIFO, host enablement, keyboard-bridge parity, world clone/spawn helper, shell routing, and File New clipboard-clearing tests pin the behavior.

**Still open - explicitly NOT closed here:** Cut semantics, OS/system clipboard integration, typed kernel component cloning, authoritative CAD graph/projection/render copy/paste, undo/redo and dirty-state integration for Copy/Paste, authoritative CAD graph/projection/render deletion/duplication, plugin action execution/registration UX beyond the optional Plugins projection, command-palette integration, host->shell FIFO menu-click replacement, generalized registry execution beyond the now-wired canonical menu commands, broader camera UI beyond reset/frame/zoom, and conflict resolution/keybinding editor/fatal gating.

**Scope:** `editor-ui` default Edit menu entries/tests, `editor-egui-host` projection/enablement tests, `editor-shell` wrapper-world clipboard/routing/tests, `world.rs` legacy-blob clone/spawn helpers, and top-level status docs; no OS clipboard, typed ECS clone, CAD graph mutation, projection-cache invalidation, render-mesh invalidation, CommandBus action, undo stack, dirty-state semantics, plugin runtime, command palette, keybinding editor, FIFO replacement, Cargo, scheduler, dispatch automation, or task arming.

### 2026-06-06 - File New empty scene command

**Forward-only follow-up (MENU-FILE-NEW).** Narrows the File menu surface with a bounded reset-to-empty command. `New` now has a visible File menu entry, an executable `Ctrl+N` accelerator, and a shell route that reuses the existing `replace_world(KernelWorld::new())` reset substrate.

**Now shipped - File New.**
- `default_editor_menu` registers File -> New before Open, with `Command::NewFile` and executable `Ctrl+N`.
- New is enabled only while Editing, like Open/Save/Save-As. The shortcut remains bound for display but disabled contexts do not execute it.
- `EditorShell::route_menu_command` routes `Command::NewFile` to `handle_new_file_request()`, which resets to a fresh unsourced empty world through `replace_world`.
- The reset clears the adopted save source, entity selection, render content, PIE snapshot, and command bus; the fresh world gets default `TimeScale`.
- Registry, host projection/FIFO, host enablement, keyboard-bridge parity, and shell routing tests pin the behavior.

**Still open - explicitly NOT closed here:** unsaved-changes prompt/confirmation, creating a file or project on disk, choosing templates, Save-As to a new `.rge-project` tree, Cut/Copy/Paste semantics, authoritative CAD graph/projection/render duplication/deletion, undo/redo and dirty-state integration for Edit content mutations, clipboard, plugin action execution/registration UX beyond the optional Plugins projection, command-palette integration, host->shell FIFO menu-click replacement, generalized registry execution beyond the now-wired canonical menu commands, broader camera UI beyond reset/frame/zoom, and conflict resolution/keybinding editor/fatal gating.

**Scope:** `editor-ui` default File menu entries/tests, `editor-egui-host` projection/enablement tests, `editor-shell` New routing/tests, and top-level status docs; no prompt, dialog, disk I/O, project/template creation, CAD graph mutation, projection-cache invalidation, render-mesh invalidation beyond the existing reset, CommandBus action, undo stack, dirty-state semantics, clipboard, plugin runtime, command palette, keybinding editor, FIFO replacement, Cargo, scheduler, dispatch automation, or task arming.

### 2026-06-06 - Edit Duplicate selected entities

**Forward-only follow-up (MENU-EDIT-DUPLICATE).** Narrows the generalized menu-command execution item with a bounded duplication command. `Duplicate` now has a visible menu entry, an executable `Ctrl+D` accelerator, and a shell route that clones selected legacy-blob entities in the editor wrapper world while explicitly staying out of authoritative CAD duplication.

**Now shipped - Edit Duplicate.**
- `default_editor_menu` registers Edit -> Duplicate after Delete, with `Command::Duplicate` and executable `Ctrl+D`.
- Duplicate is enabled only while Editing with a non-empty entity selection. The shortcut remains bound for display but disabled contexts do not execute it.
- `World::duplicate_entity_blobs` spawns a fresh entity and clones the selected entity's legacy component blobs onto it. It intentionally does not clone type-erased kernel components because there is no safe generic clone path for arbitrary typed ECS components.
- `EditorShell::route_menu_command` routes `Command::Duplicate` to `duplicate_selected_entities()`, which duplicates selected wrapper-world entities, selects the new duplicates, and clears face selection because no authoritative face-ID remapping exists.
- Registry, host projection/FIFO, host enablement, keyboard-bridge parity, world duplicate, and shell routing tests pin the behavior.

**Still open - explicitly NOT closed here:** Cut/Copy/Paste semantics, authoritative CAD graph/projection/render duplication, undo/redo and dirty-state integration for duplication, clipboard, plugin action execution/registration UX beyond the optional Plugins projection, command-palette integration, host->shell FIFO menu-click replacement, generalized registry execution beyond the now-wired canonical menu commands, broader camera UI beyond reset/frame/zoom, and conflict resolution/keybinding editor/fatal gating.

**Scope:** `editor-ui` default Edit menu entries/tests, `editor-egui-host` projection/enablement tests, `editor-shell` wrapper-world duplicate/routing/tests, and top-level status docs; no CAD graph mutation, projection-cache invalidation, render-mesh invalidation, CommandBus action, undo stack, dirty-state semantics, clipboard, plugin runtime, command palette, keybinding editor, FIFO replacement, Cargo, scheduler, dispatch automation, or task arming.

### 2026-06-06 - Edit Delete selected entities

**Forward-only follow-up (MENU-EDIT-DELETE).** Narrows the generalized menu-command execution item with a bounded destructive Edit command. `Delete` now has a visible menu entry, an executable plain `Delete` accelerator, and a shell route that removes selected entities from the editor wrapper world while explicitly staying out of authoritative CAD deletion.

**Now shipped - Edit Delete.**
- `default_editor_menu` registers Edit -> Delete after Select All, with `Command::Delete` and executable plain `Delete`.
- Delete is enabled only while Editing with a non-empty entity selection. Like other greyed menu items, the shortcut remains bound for display but `enabled_command_for_shortcut` withholds execution when disabled.
- `World::despawn` removes an entity from both the kernel world and the legacy blob view, including every legacy component blob keyed by that entity.
- `EditorShell::route_menu_command` routes `Command::Delete` to `delete_selected_entities()`, which deletes selected wrapper-world entities, clears entity selection, and prunes face selections whose entity was deleted.
- Registry, host projection/FIFO, host enablement, keyboard-bridge parity, world despawn, and shell routing tests pin the behavior.

**Still open - explicitly NOT closed here:** Cut/Copy/Paste semantics, authoritative CAD graph/projection/render deletion, undo/redo and dirty-state integration for deletion, clipboard, plugin action execution/registration UX beyond the optional Plugins projection, command-palette integration, host->shell FIFO menu-click replacement, generalized registry execution beyond the now-wired canonical menu commands, broader camera UI beyond reset/frame/zoom, and conflict resolution/keybinding editor/fatal gating.

**Scope:** `editor-ui` default Edit menu entries/tests, `editor-egui-host` projection/enablement tests, `editor-shell` wrapper-world despawn/routing/tests, and top-level status docs; no CAD graph mutation, projection-cache invalidation, render-mesh invalidation, CommandBus action, undo stack, dirty-state semantics, clipboard, plugin runtime, command palette, keybinding editor, FIFO replacement, Cargo, scheduler, dispatch automation, or task arming.

### 2026-06-06 - Edit Select All menu command

**Forward-only follow-up (MENU-EDIT-SELECT-ALL).** Narrows the generalized menu-command execution item with one bounded, non-destructive Edit command. `Select All` now has a visible menu entry, an executable `Ctrl+A` accelerator, and a shell route that mutates only editor coordination state.

**Now shipped - Edit Select All.**
- `default_editor_menu` registers Edit -> Select All after Undo / Redo, with `Command::SelectAll` and executable `Ctrl+A`.
- `PredicateContext` now carries `has_selectable_entities`, and `EditorShell::predicate_context()` fills it from the live world entity count. The menu keeps `Ctrl+A` bound for display but only enables execution while Editing with at least one live entity.
- `EditorShell::route_menu_command` routes `Command::SelectAll` to `select_all_entities()`, which replaces the entity selection with the deterministic live `World::entities()` set. It does not mutate world contents, CAD geometry, face selection, or the undo stack.
- Registry, host projection/FIFO, host enablement, keyboard-bridge parity, shell predicate-context, and shell routing tests pin the behavior.

**Still open - explicitly NOT closed here:** Cut/Copy/Paste semantics, authoritative CAD deletion/duplication, plugin action execution/registration UX beyond the optional Plugins projection, command-palette integration, host->shell FIFO menu-click replacement, generalized registry execution beyond the now-wired canonical menu commands, broader camera UI beyond reset/frame/zoom, and conflict resolution/keybinding editor/fatal gating.

**Scope:** `editor-ui` default Edit menu entries/tests, `editor-egui-host` projection/enablement tests, `editor-shell` predicate context/routing/tests, and top-level status docs; no content deletion/duplication, clipboard, plugin runtime, command palette, keybinding editor, FIFO replacement, Cargo, scheduler, dispatch automation, or task arming.

### 2026-06-06 - View camera zoom commands

**Forward-only follow-up (MENU-VIEW-ZOOM).** Narrows the "broader camera-state UI beyond this label" item from the scene-aware View label entry below. The View menu now has bounded camera zoom commands using existing `Command::ZoomIn` / `Command::ZoomOut` variants and the existing menu registry / host projection / shell command sink.

**Now shipped - View camera zoom.**
- `default_editor_menu` registers View -> Zoom In and View -> Zoom Out after Reset Camera / Frame Scene.
- The new entries carry executable plain `PageUp` / `PageDown` accelerators, so the canonical executable menu set is now eight bindings: File Open/Save/Save-As, Edit Undo/Redo, View Reset Camera, View Zoom In, and View Zoom Out.
- `EditorShell::route_menu_command` routes `Command::ZoomIn` / `Command::ZoomOut` to new infallible camera helpers. The helpers preserve target, direction, up vector, FOV, and clip planes; they only scale the eye-target distance by inverse factors (`0.8` / `1.25`).
- Registry, host projection, keyboard-bridge, and shell tests pin the new View entries, FIFO routing, PageUp/PageDown mapping, and direct zoom math including a degenerate eye-target fallback.

**Still open - explicitly NOT closed here:** broader camera UI beyond reset/frame/zoom, plugin action execution/registration UX beyond the optional Plugins projection, command-palette integration, host->shell FIFO menu-click replacement, generalized registry execution beyond wired canonical accelerators, and conflict resolution/keybinding editor/fatal gating.

**Scope:** `editor-ui` default View menu entries/tests, `editor-egui-host` projection tests, `editor-shell` camera helpers/routing/tests, and top-level status docs; no plugin runtime, command palette, keybinding editor, FIFO replacement, Cargo, scheduler, dispatch automation, or task arming.

### 2026-06-06 - View scene-aware camera label

**Forward-only follow-up (MENU-VIEW-SCENE-LABEL).** Narrows the "broader dynamic labels such as camera-state-aware View" item from the 2026-06-06 menu shortcut reconciliation below. The View camera action keeps its stable `Command::ResetCamera` identity and `Home` accelerator, but its resolved label now reflects whether the shell has frameable scene bounds.

**Now shipped - scene-aware View label.**
- `PredicateContext` gains `has_frameable_scene`, defaulting to `false`.
- `EditorShell::predicate_context()` fills that bit from the same `current_scene_bounds().is_some()` source that `EditorShell::reset_camera()` consumes.
- `default_editor_menu` keeps the static View label as `Reset Camera`, but resolves it to `Frame Scene` when `has_frameable_scene` is true.
- Registry, host projection, and shell tests pin the default label, the frameable-scene override, unchanged `Command::ResetCamera`, unchanged `Home` accelerator, and the shell-side context bit for prebuilt render meshes.

**Still open - explicitly NOT closed here:** broader camera-state UI beyond this label, plugin action execution/registration UX beyond the optional Plugins projection, command-palette integration, host->shell FIFO menu-click replacement, generalized registry execution beyond wired canonical accelerators, and conflict resolution/keybinding editor/fatal gating.

**Scope:** `editor-ui` predicate/menu label substrate, `editor-shell` predicate-context publication/tests, `editor-egui-host` projection tests, and top-level status docs; no `reset_camera` behavior change, no menu command identity/routing change, no plugin runtime, Cargo, scheduler, or dispatch automation change.

### 2026-06-06 - Optional Plugins menu projection

**Forward-only follow-up (MENU-PLUGIN-PROJECTION).** Narrows the "plugin menu entries" item from the 2026-06-06 menu shortcut reconciliation below. The menu registry now has a canonical empty Plugins extension point and the egui host can project/render plugin-provided entries without hardcoding plugin actions.

**Now shipped - plugin menu surface.**
- `editor-ui::menus::default_editor_menu` declares `editor.main_menu.plugins` via `plugins_menu_point()` with no default core entries.
- `editor-egui-host::menu::project_main_menu` projects registered plugin entries into `ProjectedMainMenu::plugins`.
- `EguiHost::render` shows a top-level `Plugins` menu only when that projected list is non-empty; the default editor menu remains visually unchanged.
- Host tests pin a synthetic `Command::Plugin` entry with `Ctrl+Alt+M` display and prove it enqueues through the existing `MenuCommandHandoff` unchanged.

**Still open - explicitly NOT closed here:** plugin action execution/routing policy, plugin registration UX beyond the extension point, command-palette integration, host->shell FIFO menu-click replacement, generalized registry execution beyond wired canonical accelerators, conflict resolution/keybinding editor/fatal gating, and broader camera-state UI beyond the scene-aware View label above.

**Scope:** `editor-ui` menu extension-point declaration/export, `editor-egui-host` projection/render/tests, and top-level status docs; no `editor-shell` routing, plugin runtime, command execution policy, Cargo, scheduler, or dispatch automation change.

### 2026-06-06 - Menu shortcut conflict diagnostics surfaced

**Forward-only follow-up (MENU-CONFLICT-DIAGNOSTIC).** Narrows the "AcceleratorTable conflict UI / conflict population surface" item from the 2026-06-06 menu shortcut reconciliation below. The registry already computed `ResolveResult.conflicts`; the egui host now carries that data through its projection and makes it visible instead of silently ignoring it.

**Now shipped - host-level conflict diagnostics.**
- `editor-egui-host::menu::project_main_menu` now returns a named `ProjectedMainMenu` containing the four menu entry lists plus projected shortcut-conflict diagnostics (`shortcut` display string + conflicting entry ids).
- `EguiHost::render` renders a `Shortcut Conflicts` top-bar menu only when conflicts are present, listing each conflicting shortcut and the entry ids that claimed it.
- The default canonical menu remains conflict-free, so normal users see no extra menu; the new host test injects a synthetic duplicate `Ctrl+S` registration and proves the diagnostic is populated.

**Still open - explicitly NOT closed here:** conflict resolution policy / keybinding editor / fatal gating, plugin action execution/registration UX beyond the optional Plugins projection above, host->shell FIFO menu-click replacement, generalized execution beyond wired canonical accelerators, and broader camera-state UI beyond the scene-aware View label above.

**Scope:** `editor-egui-host` projection/render/tests plus top-level status docs; no `editor-ui` registry semantics, default shortcut values, shell accelerator routing, Cargo, scheduler, or dispatch automation change.

### 2026-06-06 - Menu shortcut/display follow-up shipped: Play hints, Resume label, View Home

**Forward-only follow-up (MENU-SHORTCUT-DOC-RECONCILE).** Narrows the 2026-06-05 registry-enable section's "Play/View accelerator DISPLAY + execution" and "dynamic toggle LABELS" open items, and narrows the 2026-06-04 W08 section's "View -> Reset Camera has no keystroke binding" line. The menu baseline now reflects the three follow-up commits after #313/#314: Play exposes passive Space/Escape hints, the Play start item resolves as `Resume` while paused, and View -> Reset Camera binds the canonical plain `Home` accelerator.

**Now shipped - menu shortcut/display follow-up.**
- **`f3931a7` (`feat(menu): show passive play shortcut hints`)** - `MenuEntry::shortcut_hint` lets the host display Space/Escape beside Play/Pause/Stop without registering those keys in the executable accelerator table. Play keyboard execution remains the existing plain-key PIE path, not a menu accelerator.
- **`67c140a` (`feat(menu): resolve play resume label dynamically`)** - `MenuEntry::with_label_override` lets `MenuRegistry::resolve` clone a context-specific label; `play.start` shows `Resume` only when `PredicateContext.play_state == "paused"`, while command identity and routing stay unchanged.
- **`2df991c` (`feat(menu): bind reset camera to home`)** - `default_editor_menu` gives `view.reset_camera` `Shortcut::plain(Key::Home)`. The host displays `Home`, and editor-shell resolves `KeyCode::Home -> Shortcut::Home -> Command::ResetCamera -> EditorShell::reset_camera` through the same menu source of truth as File/Edit.

**Authority (updated).** `editor-ui::menus::default_editor_menu` now owns six executable menu accelerators: File Open/Save/Save-As, Edit Undo/Redo, and View Reset Camera. Play owns passive display hints only; its Space/Escape execution remains outside the menu accelerator table by design.

**Still open - explicitly NOT closed here:**
- the `AcceleratorTable` conflict UI / conflict population surface.
- dynamic labels beyond the Play-start `Resume` override and scene-aware View `Frame Scene` label.
- plugin action execution/registration UX beyond the optional Plugins menu projection.
- host->shell FIFO menu-click replacement (clicks still use `MenuCommandHandoff`).
- generalized registry/accelerator-driven command execution beyond the canonical menu accelerators already wired.
- the VISIBILITY predicates (the filtering `predicate`) remain available but UNUSED by `default_editor_menu` - all entries stay visible; only enablement and selected labels vary.

**Historical preservation.** The 2026-06-05 and 2026-06-04 subsections below are preserved in place; their older "Play/View open" wording is narrowed by this subsection, not rewritten in place.

**Scope:** docs only (`plans/BASELINE.md` + top-level status docs); no Rust source-logic / test / Cargo / scheduler / dispatch automation change.

### 2026-06-05 — Registry-driven DYNAMIC menu enablement shipped; bespoke Play-greying retired (#313 + #314)

**Forward-only follow-up (MENU-ENABLEMENT-DOC-RECONCILE).** Narrows the W08-accelerator (#308–#311) subsection's "Still open — the W08 registry-driven dynamic predicates + per-frame re-resolve (the menu resolves once with `PredicateContext::default()`; the shortcut→command index is static because every `default_editor_menu` entry is unconditional)" line, and supersedes the PLAYMENU-DYNAMIC (#302) bespoke-greying mechanism: dynamic menu enablement is now the ONE canonical registry path, re-resolved per frame against a live `PredicateContext`, and the bespoke `MenuStateHandoff` / `play_item_enabled` channel is retired. Records the two merged PRs.

**Now shipped — registry-driven dynamic enablement (#313 + #314).**
- **#313 (`459c689`, MENU-ENABLED-SUBSTRATE)** — editor-ui "disabled-but-visible" substrate: a separate `MenuEntry.enabled` predicate (distinct from the visibility `predicate`, which FILTERS the entry out); `ResolvedEntry.enabled` computed in `resolve` WITHOUT filtering (disabled entries stay present + keep their accelerator); `enabled_command_for_shortcut` (the keyboard EXECUTION resolver — `command_for_shortcut` keeps its binding/display semantics, so the W08.3 parity guard is intact); `PredicateContext` gains `can_play`/`can_pause`/`can_stop`/`can_step` + `is_editing`. `default_editor_menu` gates File Open/Save/Save-As on `is_editing` and each Play item on its `can_*`. Additive + behaviour-neutral (no consumer read `enabled` yet).
- **#314 (`10790ba`, MENU-DYNAMIC-RESOLVE)** — wired the consumers + retired the bespoke channel. `editor-egui-host` caches the `MenuRegistry` and RE-RESOLVES it each frame via `project_main_menu(&registry, &ctx)` → `(label, accel, command, enabled)`, greying each item via `add_enabled(enabled)`. `editor-shell` publishes a live `PredicateContext` each frame (`predicate_context()` — `can_*`/`is_editing` from `PlayState`, `has_selection` from the entity selection) through a new `Handoff<PredicateContext>` (aliased host-side, so editor-state gains no editor-ui edge), and routes the keyboard `window_event` path via `enabled_command_for_shortcut`. The bespoke `MenuStateHandoff` / `MenuStateSnapshot` (file deleted) / `play_item_enabled` Play-greying is RETIRED.

**Rough-edge fix.** File `Save`/`Open`/`Save-As` grey out outside the Editing state (where they no-op) — previously always-enabled, so they looked clickable but silently no-op'd during Play. Play items grey via the SAME registry path (no longer a separate mechanism). Keyboard: `Ctrl+S` while greyed no longer fires (was a PIE-gated handler no-op + warn-log; net behaviour identical, the warn-log is dropped).

**Authority note.** `command_for_shortcut` + the W08.3 parity guard are unchanged (the guard tests the binding, not enablement). Menu CLICKS still flow through the host→shell `MenuCommandHandoff` FIFO → `route_menu_command`; #314 unified the KEYBOARD's enablement onto the registry path, NOT the click transport.

**Still open — explicitly NOT closed here:**
- Play/View accelerator DISPLAY + execution (Play's real keys are the plain `Space`/`Escape` PIE binds, not menu accelerators; View ▸ Reset Camera has no keystroke binding).
- the `AcceleratorTable` conflict UI / conflict population (computed every resolve, still unsurfaced).
- dynamic toggle LABELS (Play⇄Pause, camera-state-aware View).
- plugin menu entries.
- host→shell FIFO menu-click replacement (clicks still use `MenuCommandHandoff`).
- generalized registry/accelerator-driven command execution beyond the canonical entries.
- the VISIBILITY predicates (the filtering `predicate`) remain available but UNUSED by `default_editor_menu` — all entries stay visible; only enablement varies.

**Historical preservation.** The W08-accelerator (#308–#311), #304/#305, and PLAYMENU-DYNAMIC (#302) subsections below + all earlier dated entries are preserved byte-identical; their "registry-driven dynamic predicates / per-frame re-resolve … deferred" + bespoke-greying lines are narrowed forward by this subsection (now shipped), not rewritten in place.

**Scope:** docs only (`Status.md` + `HANDOFF.md` + `plans/BASELINE.md` + `change.md`); no Rust source-logic / test / Cargo change (the menu / enablement rustdoc was made current inside #313 + #314 themselves).

### 2026-06-04 — W08 accelerator EXECUTION shipped: File/Edit keystrokes route through the canonical menu (#308–#311)

**Forward-only follow-up (W08-ACCELERATOR-DOC-RECONCILE).** Narrows the #304/#305 subsection below ("Still open — accelerator-table EXECUTION + conflict population"; "Authority: the shown values mirror the live, executing `EditorKeyCommand::from_key_press` … the deferred W08 EXECUTION work unifies the two via the resolved `AcceleratorTable`"): the File/Edit accelerator EXECUTION half is now SHIPPED, and the canonical menu is the SOURCE of that routing rather than a mirror of it. Records the four merged PRs that close the W08 accelerator-execution thread.

**Now shipped — File/Edit accelerator EXECUTION via the canonical menu (#308–#311).**
- **#308 (`453a569`, W08-CANONICAL-MENU-SOURCE)** moved the default editor menu into `editor-ui` (`menus::default_menu::default_editor_menu` — the File/Edit/Play/View definition) and added `ResolveResult::command_for_shortcut(&Shortcut) -> Option<&Command>` (an O(1), first-registered-wins index built in `resolve()`). Both the host menu bar and `editor-shell` build from this one definition; no reverse crate edge.
- **#309 (`9004b4b`, W08-2-KEYCODE-PARITY)** added the shell-local `lifecycle::accelerator::keycode_to_shortcut` (`rge_input::KeyCode` + Ctrl/Shift → `rge_editor_ui::menus::Shortcut`; it lives in editor-shell because `editor-ui ↛ rge-input`, forbidden-dep rule 4) plus a parity guard locking the editor-shell keyboard map to the canonical menu — NO live-path change.
- **#310 (`4fc66cf`, W08-3-KEYSTROKE-CUTOVER)** made the cutover live: `window_event` resolves each un-consumed `KeyDown` via `keycode_to_shortcut → command_for_shortcut` and dispatches the bound `Command` through the shared `EditorShell::route_menu_command` sink (the same one the host→shell menu FIFO drains into). This collapsed the former `EditorKeyCommand` Save/Save-As/Undo/Redo arm and the inline `Ctrl+O` arm into one menu-sourced path; the old Shift-sloppy `key == KeyO && ctrl` check became the precise menu bind, so `Ctrl+Shift+O` is now a no-op.
- **#311 (`e98f41d`, W08-4-RETIRE-SHADOW)** retired the now-shadow `EditorKeyCommand::{Undo, Redo, Save, SaveAsProject}` variants (the File/Edit keystroke→command literals live ONLY in the canonical menu now) and re-anchored the parity guard to pin "the menu binds the five File/Edit accelerators AND `from_key_press` returns None for them" — the invariant became "menu is canonical, no shadow", not "two maps agree". `EditorKeyCommand` survives for the execution-only time-scale binds (`Ctrl+2/0/4`).

**Authority (updated).** `editor-ui::menus::default_editor_menu` is the single source of truth for the File/Edit accelerators; editor-shell resolves keystrokes through it (`keycode_to_shortcut → command_for_shortcut → route_menu_command`). The #304 subsection's "Authority: mirror the live `EditorKeyCommand::from_key_press` + the `Ctrl+O` `handle_open_request` arm" is superseded — those were the shadow map, now retired.

**Still open — explicitly NOT closed here (the File/Edit accelerator EXECUTION half above IS closed):**
- Play/View accelerator display AND execution. Play's real keys are the plain `Space`/`Escape` PIE binds (not menu accelerators); View ▸ Reset Camera has no keystroke binding.
- the `AcceleratorTable` conflict UI / conflict population surface.
- the W08 registry-driven dynamic predicates + per-frame re-resolve (the menu resolves once with `PredicateContext::default()`; the shortcut→command index is static because every `default_editor_menu` entry is unconditional).
- dynamic toggle LABELS (Play⇄Pause, camera-state-aware View).
- plugin menu entries.
- host→shell FIFO menu-click replacement — menu CLICKS still enqueue through the `MenuCommandHandoff` FIFO (`drain_and_route_menu_commands → route_menu_command`); W08 unified the KEYBOARD onto that sink, it did NOT replace the click FIFO.
- generalized registry/accelerator-driven command execution beyond the five File/Edit binds.
- the time-scale binds (`Ctrl+2/0/4`), `Space`/`Escape` playback, and plain-`R` reload remain documented execution-only keybinds with no menu home (an intentional asymmetry retained on `EditorKeyCommand` / the playback + reload axes).

**Historical preservation.** The #304/#305 (File/Edit accelerator display) subsection below and all earlier dated entries are preserved byte-identical; their "accelerator-table EXECUTION … Still open" + "mirror the live `EditorKeyCommand` routing" lines are narrowed forward by this subsection (File/Edit EXECUTION shipped; the menu is now the source), not rewritten in place.

**Scope:** docs only (`Status.md` + `HANDOFF.md` + `plans/BASELINE.md` + `change.md`); no Rust source-logic / test / Cargo change (the accelerator/menu rustdoc was made current inside #308–#311 themselves).

### 2026-06-04 — File/Edit menu accelerators shown; menu construction extracted to a submodule (#304 + #305)

**Forward-only follow-up (MENU-SHORTCUT-DOC-RECONCILE).** Narrows the prior subsections' "accelerator-table execution/display/conflict population … unbuilt" line: per-item accelerator DISPLAY shipped for File/Edit. Records the two merged PRs that close the menu-deepening thread.

**Now shipped — File/Edit accelerator display (#304, `284998e`, MENU-SHORTCUT-DISPLAY).** File + Edit menu items render their real keyboard accelerator next to the label via egui `shortcut_text`:
- `crates/editor-egui-host` attaches `MenuEntry::with_shortcut(Shortcut::new(..))` to the 3 File + 2 Edit entries (Open Ctrl+O / Save Ctrl+S / Save-As Ctrl+Shift+S / Undo Ctrl+Z / Redo Ctrl+Y); the projection widened `(label, Command)` → `(label, Option<accelerator-display>, Command)`, sourced straight from each resolved `MenuEntry.shortcut` via `Shortcut::display`. A `menu_item` render helper builds each `egui::Button` + optional `shortcut_text`.
- Display-only: clicks still dispatch through the host→shell FIFO; no command-execution / keystroke-routing / crate-edge change. Play/View project `None`.

Authority: the shown values mirror the live, executing editor-shell routing (`EditorKeyCommand::from_key_press` + the `Ctrl+O` `handle_open_request` arm), pinned by host tests; the host cannot import editor-shell (reverse edge), so `MenuEntry.shortcut` (editor-ui) is the designated accelerator home and the deferred W08 EXECUTION work unifies the two via the resolved `AcceleratorTable`.

**File/Edit only — Play/View deferred.** Play's only shortcut data is the play-toolbar's ADVISORY `F5/Esc` hints, which don't match the real Space/Escape PIE binds; surfacing them would open the advisory-vs-real accelerator/execution conflict this beat avoids. View ▸ Reset Camera has no keystroke binding.

**Now retired — the split-cap exemption (#305, `892a0ad`, EGUIHOST-MENU-EXTRACTION).** #304's additions re-crossed the §1.3 Rule-3 1000-line cap on `editor-egui-host/src/lib.rs`, so #304 re-added the `// SPLIT-EXEMPTION:` annotation (the same one #301 had retired). #305 then extracted the cohesive menu-construction block (the four extension-point consts + `build_main_menu_entries` + `play_item_enabled` + `menu_item`) VERBATIM into a new `menu.rs` submodule — behaviour-identical (render path + every test assertion byte-unchanged) — dropping lib.rs 1062 → 836 lines and RETIRING the annotation (0 markers crate-wide). Mirrors EGUIHOST-TEST-EXTRACTION (#301), which split the inline tests out as its own dispatch.

**Still open — explicitly NOT closed here:**
- accelerator-table EXECUTION + conflict population (this beat ships DISPLAY only, and only for File/Edit; Play/View accelerator display is deferred with the W08 execution work).
- the W08 registry-driven dynamic predicates + per-frame re-resolve; dynamic toggle LABELS (Play⇄Pause, camera-state-aware View).
- plugin menu entries + generalized registry/accelerator-driven command execution (clicks still flow through the host→shell FIFO).

**Historical preservation.** The PLAYMENU-DYNAMIC / A4 / A3 / A2 / A1 subsections below and all earlier dated entries are preserved byte-identical; their "accelerator-table execution/display/conflict … unbuilt" lines are narrowed forward by this subsection (File/Edit display shipped), not rewritten in place. (The A4 subsection's flagged EGUIHOST-TEST-EXTRACTION follow-up landed as #301; the exemption it + #304 carried is now fully retired by #305.)

**Scope:** docs only (`Status.md` + `HANDOFF.md` + `plans/BASELINE.md` + `change.md`); no Rust source-logic / test / Cargo change (the menu / accelerator rustdoc was made current inside #304 + #305 themselves).

### 2026-06-03 — Play-menu items now dynamically enabled per live PlayState (#302)

**Forward-only follow-up (PLAYMENU-DYNAMIC-DOC-RECONCILE).** Narrows the A4 subsection below, whose "dynamic predicates, and per-frame re-resolve … remain unbuilt" line is now partly superseded: per-frame Play-item ENABLEMENT shipped. #302 (`9960b30`, PLAYMENU-DYNAMIC-ENABLE) is the first menu-deepening beat past breadth.

**Now CLOSED — dynamic Play-menu enablement.** The Play menu items grey out (disabled-but-visible) when the current `PlayState` makes the transition a no-op:
- `crates/editor-shell/src/play_state.rs` adds `PlayState::can_play` / `can_pause` / `can_stop` / `can_step` (derived from the canonical state — `can_play = !Playing`, `can_pause` / `can_stop` = `is_pie_active()`, `can_step` = `Paused`), with a consistency test pinning each to whether the real `play()` / `pause()` / `stop()` / `step()` returns `Ok` (so enablement can't drift from the state machine).
- `crates/editor-state/src/menu_state_snapshot.rs` (new) adds `MenuStateSnapshot` — a read-only observation aggregator (sibling to `SaveStatusSnapshot`, NOT a 6th coordination category, so `editor-state-ownership` stays green) + `all_enabled()` for the host's pre-publish fallback.
- `crates/editor-egui-host` adds `MenuStateHandoff = Handoff<MenuStateSnapshot>` (the third latest-only snapshot handoff, alongside Inspector / SaveStatus); `render` acquires it and `add_enabled`s each Play item via `play_item_enabled` (a pure `Command`→bool router).
- `crates/editor-shell` (`lifecycle/mod.rs` + `render_path.rs`) adds the `menu_state_handoff` field + `EditorShell::menu_state_snapshot()` (pure read from `PlayState::can_*`), published each frame beside `save_status`.

Authority: `PlayState` stays the sole owner of transition validity; the host re-encodes no rule. Behaviour: Editing → only Play enabled; Playing → Pause + Stop; Paused → all four; File / Edit / View stay unconditionally enabled.

**BESPOKE channel, NOT the W08 registry path.** The W08 `MenuRegistry` `Predicate` / per-frame `resolve` machinery (dynamic menu set/visibility) is still unbuilt; this beat ships only Play-item enablement through a dedicated snapshot handoff.

**Still open — explicitly NOT closed here:**
- the W08 registry-driven dynamic predicates + per-frame registry re-resolve (menu set/visibility via `Predicate::Closure`).
- dynamic toggle LABELS (a Play⇄Pause label, a camera-state-aware View item) — needs per-frame label re-resolve.
- accelerator-table execution/display/conflict population, plugin menu entries, and generalized registry/accelerator-driven command execution remain unbuilt (clicks still flow through the host→shell FIFO).
- dynamic enablement for the File / Edit / View menus (this beat is Play-only).

**Historical preservation.** The A4 / A3 / A2 / A1 subsections below and all earlier dated entries are preserved byte-identical; their "dynamic predicates / per-frame re-resolve … unbuilt" lines are narrowed forward by this subsection (enablement shipped), not rewritten in place.

**Scope:** docs only (`Status.md` + `HANDOFF.md` + `plans/BASELINE.md` + `change.md`); no Rust source-logic / test / Cargo change (the menu / handoff / snapshot rustdoc was widened inside #302 itself, incl. its four stale-prose correction rounds).

### 2026-06-03 — View menu now registry-produced; menu-breadth arc complete (#299 A4)

**Forward-only follow-up (MENUARC-DOC-RECONCILE-A4).** Supersedes the A3 subsection below, which listed View as the lone remaining standard menu / a pending feature beat. A4 landed on main at `be4896a` via PR #299. **This closes the menu-breadth arc: File (A1) / Edit (A2) / Play (A3) / View (A4) are all built from the one shared `MenuRegistry`.**

**Now CLOSED — the View menu (Reset Camera).** Unlike A1–A3 (pure FIFO→handler wiring), View needed a NEW runtime action. `crates/editor-egui-host/src/lib.rs` `build_main_menu_entries()` now declares a fourth point `editor.main_menu.view` (`VIEW_MENU_EXTENSION_POINT`), registers Reset Camera (`Command::ResetCamera`), resolves once, and caches all four `(label, Command)` lists; `render()` paints a fourth `menu_button("View")`. `editor-shell` `drain_and_route_menu_commands` routes `Command::ResetCamera` to the new **infallible** `EditorShell::reset_camera` (no swallow — contrast A3's `route_play_button`), which reframes `editor_camera` to `isometric_camera_for_bounds(current_scene_bounds)` — the live scene's AABB union sourced the same way as `render_path.rs` Step 6 (prebuilt meshes, else the CAD projection mesh) — falling back to `EditorCameraState::default()` when nothing is frameable. Product semantics (owner-confirmed): frame the scene, mirroring the constructor's auto-frame, not a fixed default-pose jump.

**Split-exemption.** The View additions tipped `crates/editor-egui-host/src/lib.rs` past 1000L (production ~913L + inline `#[cfg(test)]` ~120L), so a `// SPLIT-EXEMPTION:` annotation was added at the file head per the `host.rs` precedent (annotate in-the-moment; extract tests later). **Flagged follow-up:** an EGUIHOST-TEST-EXTRACTION dispatch to move the inline test module to a sibling file and retire the annotation.

**Still open — explicitly NOT closed here (NO standard menus remain; the breadth arc is closed):**
- Generalized registry/accelerator-driven command execution remains deferred (menu clicks still flow through the host→shell FIFO + `editor-shell` routing, not a registry/accelerator execution path).
- accelerator-table execution/display/conflict population, plugin menu entries, dynamic predicates, and per-frame re-resolve remain unbuilt — including dynamic toggle labels (a Play⇄Pause label, or a camera-state-aware View item).

**Historical preservation.** The A3 / A2 / A1 subsections below and all earlier dated entries are preserved byte-identical as dated history; their "View … the lone remaining standard menu / pending feature beat" lines are superseded forward by this subsection, not rewritten in place.

**Scope:** docs only (`Status.md` + `HANDOFF.md` + `plans/BASELINE.md` + `change.md`); no Rust source-logic / test / Cargo / routing change (the menu rustdoc was widened to File + Edit + Play + View inside #299 itself, incl. its `6cd222d` correction round).

### 2026-06-03 — Play menu now registry-produced (#297 A3)

**Forward-only follow-up (MENUARC-DOC-RECONCILE-A3).** Supersedes the A2 subsection below, which listed the Play menu among the deferred items as the intended next feature beat. A3 landed on main at `6ea5006` via PR #297.

**Now CLOSED — the Play menu (Play / Pause / Stop / Step).** `crates/editor-egui-host/src/lib.rs` builds ALL THREE menus from a single `MenuRegistry`: `build_main_menu_entries()` declares `editor.main_menu.file` + `editor.main_menu.edit` + `editor.main_menu.play` (the new `PLAY_MENU_EXTENSION_POINT`), registers Open/Save/Save-As + Undo/Redo + Play/Pause/Stop/Step, resolves once against an empty `PredicateContext`, and caches all three resolved `(label, Command)` lists on `EguiHost`; `render()` paints a third `menu_button("Play")`. `editor-shell` `drain_and_route_menu_commands` routes `Command::PlayStart` / `PlayPause` / `PlayStop` / `PlayStep` to `EditorShell::handle_button(ToolbarButtonId::Play / Pause / Stop / Step)` via a small `route_play_button` helper — the exact PIE transition path the play-toolbar buttons drive. Because the four items are STATIC (always present/clickable), an item activated in a `PlayState` where its transition is a no-op (e.g. Stop while `Editing` → `PlayStateError::NoSnapshot`) is swallowed at debug before any mutation (`handle_button` returns `Err` before mutating), mirroring the Space/Escape swallow in `handle_playback_command`.

**Still open — explicitly NOT closed here (narrowed from the A2 list to View alone):**
- **View → Reset Camera** is now the LONE remaining standard menu and still needs a NEW action first (e.g. via `isometric_camera_for_bounds`). Unlike Play — which already had both `Command` variants AND a reachable `handle_button` action (so A3 was pure FIFO→`handle_button` wiring) — View has no reachable shell action yet, so it is a feature beat, not pure wiring.
- accelerator-table execution/display/conflict population, plugin menu entries, dynamic predicates, and per-frame re-resolve remain unbuilt — including a Play⇄Pause dynamic toggle label (v0 ships fixed labels; a live toggle needs the deferred per-frame re-resolve).
- Generalized registry/accelerator-driven command execution remains deferred (menu clicks still flow through the host→shell FIFO + `editor-shell` routing, not a registry/accelerator execution path).

**Historical preservation.** The A2 / A1 subsections below and all earlier dated entries are preserved byte-identical as dated history; their "Play … deferred / next beat" lines are superseded forward by this subsection, not rewritten in place.

**Scope:** docs only (`Status.md` + `HANDOFF.md` + `plans/BASELINE.md` + `change.md`); no Rust source-logic / test / Cargo / routing change (the menu rustdoc was already widened to File + Edit + Play inside #297 itself).

### 2026-06-02 — Edit menu now registry-produced (#295 A2)

**Forward-only follow-up (MENUARC-DOC-RECONCILE-A2).** Supersedes the A1 subsection below, which listed the Edit menu among the deferred items. A2 landed on main at `0bc6a0c` via PR #295.

**Now CLOSED — the Edit menu (Undo / Redo).** `crates/editor-egui-host/src/lib.rs` builds BOTH menus from a single `MenuRegistry`: `build_main_menu_entries()` (renamed from `build_file_menu_entries`) declares `editor.main_menu.file` + `editor.main_menu.edit`, registers Open/Save/Save-As + Undo/Redo, resolves once against an empty `PredicateContext`, and caches both resolved `(label, Command)` lists on `EguiHost`; `render()` paints a second `menu_button("Edit")`. `editor-shell` `drain_and_route_menu_commands` routes `Command::Undo` / `Command::Redo` to `EditorShell::undo_command` / `redo_command`, mirroring the `Ctrl+Z` / `Ctrl+Y` keystroke path exactly (swallowing `NothingToUndo` / `NothingToRedo`) — behavior-identical to the keyboard route.

**Still open — explicitly NOT closed here (narrowed from the A1 list):**
- **View / Play menu breadth** remains unbuilt, for DIFFERENT reasons. **View → Reset Camera** needs a NEW action first (e.g. via `isometric_camera_for_bounds`). **Play does NOT need a new action** — `Command::PlayStart` / `PlayStop` / `PlayPause` / `PlayStep` already exist (`crates/editor-ui/src/menus/command.rs`) and the PIE path is already runtime-wired through `EditorShell::handle_button` (Phase 5.3 CLOSED — see the PIE round-trip baseline below); the Play menu is therefore pure FIFO→`handle_button` menu/UI wiring, the intended next feature beat. The A2 scoping criterion was: wire only items with BOTH a `Command` variant AND a reachable shell action — Undo/Redo were the only two scoped into A2 (Play also satisfies that criterion via `handle_button` and is next).
- accelerator-table execution/display/conflict population, plugin menu entries, dynamic predicates, and per-frame re-resolve remain unbuilt.
- Generalized registry/accelerator-driven command execution remains deferred (menu clicks still flow through the host→shell FIFO + `editor-shell` routing, not a registry/accelerator execution path).

**Historical preservation.** The A1 subsection below and all earlier dated entries are preserved byte-identical as dated history; their "Edit … deferred/unbuilt" lines are superseded forward by this subsection, not rewritten in place (the A1 still-open bullet remains exactly as it was at A1).

**Scope:** docs + menu rustdoc/comments + `docs/§18/ARCHITECTURE_LINTS.md` count fixes; no Rust source-logic / test / Cargo / routing change.

### 2026-06-02 — File menu entries now registry-produced (#291 A1)

**Forward-only follow-up (MENUREGISTRY-FILEBAR-A1-DOC-RECONCILE).** Supersedes the #287/#288 snapshot's "MenuRegistry::resolve data-driven dispatch remains deferred" / "hardcoded `file_menu_items()`" framing below. A1 landed on main at `79fa41b` via PR #291.

**Now CLOSED — File menu item production through `MenuRegistry::resolve`.** `crates/editor-egui-host/src/lib.rs` now builds the File menu entries through the W08 `MenuRegistry`: `EguiHost::new` calls `build_file_menu_entries()`, which declares `editor.main_menu.file`, registers Open / Save / Save As New Project as `MenuEntry`s, resolves them with an empty `PredicateContext`, and caches the resolved `(label, Command)` pairs on `EguiHost`. `MenuEntry::new` defaults to `OrderHint::AtEnd`, so the resolved order and labels remain behavior-identical to the former list.

**Still open — explicitly NOT closed here (narrowed):**
- Generalized registry-driven command execution remains deferred: File clicks still use the existing host->shell FIFO and `editor-shell` routing from #288, not a registry/accelerator execution path.
- Edit/View/Play menu breadth, accelerator-table execution/display/conflict population, plugin menu entries, dynamic predicates, and per-frame re-resolve remain unbuilt.
- A last-directory-memory dialog, non-empty-folder confirmation, and the non-Open/Save audit gaps (drag-drop ingestion, `io-image` consumption, World-only Command-Bus `Action` context) remain unchanged.

**Historical preservation.** The older 2026-06-01 and 2026-05-28 audit snapshots below are preserved as dated history; their "no functional `MenuRegistry::resolve`" lines are superseded forward by this subsection, not rewritten in place.

**Scope:** docs-only forward reconcile of A1; no Rust source/test/Cargo change in this dispatch.

### 2026-06-02 — File menu bar landed FUNCTIONAL (#287 substrate + #288 wiring)

**Forward-only follow-up (MENUBAR-RECONCILE-DRAINTEST).** Supersedes the "Menu-entry wiring for Save-As … no functional `MenuRegistry::resolve` … keyboard-only" still-open bullet in the 2026-06-01 subsection below — the File **menu bar** is now functional. (The `MenuRegistry::resolve` data-driven registry remains separately deferred; see "Still open" below.)

**Now FUNCTIONAL — the File menu bar.** End-to-end:

- **Substrate (#287 MENUBAR-FILE-SUBSTRATE, `69b33e6`).** `editor-egui-host` renders a top File menu bar (Open… / Save / Save As New Project) that enqueues the existing `editor_ui::menus::Command` onto a host→shell FIFO `MenuCommandHandoff` (bounded `VecDeque<Command>`, cap 64, drop-newest; a deliberately different shape from the latest-only `Handoff<T>` slots).
- **Wiring (#288 MENUBAR-FILE-WIRING, `7979f8b`).** `editor-shell` drains the FIFO at the **top of `render_frame`** (`drain_and_route_menu_commands`) and routes `Command::{OpenFile, Save, SaveAs}` **one-way** into the existing `handle_open_request` / `handle_save_request` / `handle_save_as_new_project_request`. Adds the `editor-shell → rge-editor-ui` dep (forbidden-dep-validated; both Tier-2). `Command::SaveAs` is labelled "Save As New Project" and routes to the new-project handler.

So **Open / Save / Save-As are now discoverable AND functional from the File menu**, not keyboard-only (the `Ctrl+O`/`Ctrl+S`/`Ctrl+Shift+S` paths are unchanged). This dispatch (MENUBAR-RECONCILE-DRAINTEST) also added a `render_frame` drain-placement test (`crates/editor-shell/src/lifecycle/tests.rs`, closing the audit's P2 gap that the four route tests called the drain directly without driving `render_frame`) and corrected the stale `EguiHost::render` rustdoc.

**Still open — explicitly NOT closed here (narrowed):**

- **`MenuRegistry::resolve` data-driven dispatch** (the W08 registry) remains deferred — the File bar is a **direct host menu bar** (`egui::MenuBar::new().ui`) over a hardcoded `file_menu_items()` list; `Command::OpenFile` still carries only a diagnostic id; **Edit/View/Play menus, accelerators, and plugin menu entries are unbuilt**. The wire-MenuRegistry-vs-ratify-the-FIFO-bypass decision is the named next frontier.
- A last-directory-memory dialog; a non-empty-folder confirmation — unchanged.
- The non-Open/Save audit gaps (drag-drop ingestion, `io-image` consumption, the World-only Command-Bus `Action` context) — unchanged.

**Scope:** docs + one editor-shell test + the `EguiHost::render` rustdoc; no production logic / `Cargo.toml` change; the 2026-06-01 subsections and all earlier dated entries below are byte-identical (pure forward-only prepend).

### 2026-06-01 — Save-As to a NEW `.rge-project` tree landed (#283 substrate + #284 wiring)

**Forward-only follow-up (SAVEAS-STATUS-SNAPSHOT).** The 2026-06-01 subsection below ("Editor Open/Save surface landed (#264–#281)") and its **Still-open** list recorded "Save-As to a *new* `.rge-project` tree (creating a fresh project directory) remains a carried/deferred item." That shipped; this prepend supersedes it. The subsection below is preserved **byte-identical** (no in-place rewrite). Grounded at main commit `a74e479`.

**Now CLOSED — Save-As to a new project tree.** End-to-end:

- **Substrate (#283 NEWPROJECT-SAVE-SUBSTRATE).** `rge_scene_loader::save_world_as_new_project(world, project_dir) -> Result<PathBuf, NewProjectWorldSaveError>` creates `<dir>/.rge-project` (manifest: folder-derived `name`, `V0_1_0`, `target_tiers: [Desktop]`, no plugins, `scenes: ["scenes/main.rge-scene"]`) + `<dir>/scenes/main.rge-scene` from the live world, returning the created `.rge-project`; **no-clobber** — errs if either path already exists. Round-trips through `load_scene_world_from_path`.
- **Wiring (#284 NEWPROJECT-SAVE-WIRING).** **`Ctrl+Shift+S`** → `EditorKeyCommand::SaveAsProject` → `EditorShell::handle_save_as_new_project_request` over the binary-owned `NewProjectSaveDialog` (rfd `pick_folder`) + `NewProjectSaveHook` (over the substrate fn). On success **adopts** `SaveSource::Project { path: <created>, name: <folder-derived; None if non-UTF-8> }` and marks saved — so the next plain `Ctrl+S` overwrites it silently. PIE-gated; cancel / no-dialog / no-hook / hook-error all log + no-op. editor-shell stayed loader-free / rfd-free (`forbidden-dep` rule 7).

The editor now supports the full authoring loop: **Open** (`Ctrl+O`), **Save** (`Ctrl+S` — `.rge-scene` or `.rge-project`, silent overwrite by `SaveSource`), and **Save-As to a new `.rge-project` tree** (`Ctrl+Shift+S`).

**Still open — explicitly NOT closed here:**

- **Menu-entry wiring for Save-As** — there is still no functional `MenuRegistry::resolve` dispatch (`Command::OpenFile` carries only a diagnostic id); Save-As is keyboard-only (`Ctrl+Shift+S`).
- A last-directory-memory dialog; an in-app confirmation when the picked folder is non-empty.
- The non-Open/Save audit gaps (drag-drop ingestion, `io-image` consumption, the World-only Command-Bus `Action` context) are **unchanged** — as the 2026-05-28 ISSUE-256 entry records.

**Scope:** docs-only, forward-only. No source / test / `Cargo.toml` change; the 2026-06-01 (#264–#281) subsection and all earlier dated entries below are byte-identical.

### 2026-06-01 — Editor Open/Save surface landed (#264–#281); SAVE-direction + in-app-open gaps CLOSED

This subsection forward-reconciles the dated 2026-05-28 reconciliation below (grounded at `6e24706`, pre-#264) and the 2026-05-21 snapshot beneath it, both of which recorded the editor's **SAVE direction** as having "no path at all" and **non-CLI open/load UX** as "absent." Both are now stale: the in-app file **Open/Save** authoring loop shipped across the contiguous PR run **#264–#281**. Grounded at main commit `f76e001`. This is a pure prepend — the 2026-05-28 and 2026-05-21 dated content below is preserved byte-identical; reconciliation is never by in-place edit.

**Gap 1 (`:588`, scene/project persistence — the SAVE direction) — now CLOSED.** A runtime serializer path exists end-to-end, directly superseding the 2026-05-28 "(4) SAVE direction has no path at all" / 2026-05-21 "the editor never calls it … cannot save user work":

- `.rge-scene` writer `rge_scene_loader::save_scene_world_to_path` (`crates/rge-scene-loader/src/lib.rs:534`; World→rge-scene extraction + save, #267 SCENE-SAVE-SUBSTRATE), wired to in-app **Ctrl+S** Save / Save-As (#268 SCENE-SAVE-WIRING).
- **True Save** = silent overwrite of the opened source (#269 SCENE-SAVE-SOURCE-PATH).
- `.rge-project` writer `rge_scene_loader::save_project_world_to_path` (`:635`, #273 PROJECT-SAVE-SUBSTRATE).
- **Ctrl+S routed by `SaveSource`** `{ Scene(PathBuf), Project { path, name } }` (`crates/editor-shell/src/lifecycle/save_source.rs:25`), replacing the earlier `scene_source_path` (#274 PROJECT-SAVE-WIRING).

**"Non-CLI open/load UX is absent … no in-app file picker or 'File → Open' gesture" — now CLOSED.** In-app **Ctrl+O** scene Open landed (`crates/editor-shell/src/lifecycle/open_request.rs::handle_open_request` at `:228`, #266 SCENE-OPEN-WIRING) over the `EditorShell::replace_world` runtime world-swap substrate (`crates/editor-shell/src/lifecycle/mod.rs:753`, #265 EDITOR-WORLD-SWAP) and the scene-path resolver promoted into `rge-scene-loader` (#264 SCENE-WORLD-BRIDGE), with GLB-watcher teardown on Open. The Open dialog is mediated by a binary-owned `SceneOpenHook` seam so `editor-shell` stays loader-free.

**Surfacing of the open source (new since 2026-05-28).** Window title reflects the open source + dirty state (#270 EDITOR-WINDOW-TITLE); an in-app bottom status bar shows source name + dirty (#271 EDITOR-SAVE-STATUS-INDICATOR); `SaveSource::display_name()` (`save_source.rs:76`) shows a `.rge-project`'s manifest name (folder name as fallback), not the literal `.rge-project` (#275 SAVE-SOURCE-DISPLAY-NAME, #279 PROJECT-NAME-DISPLAY, tests+prose #281 PROJECT-NAME-DISPLAY-FOLLOWUP); status wording is source-neutral (`scene_file_name`→`source_name`, "No scene"→"No file"; #277 SAVE-STATUS-SOURCE-NEUTRAL); the key-command renamed `MarkSaved`→`Save` (#278 KEYCOMMAND-SAVE-RENAME). Boundary hardening: `editor-shell` is loader-free, machine-enforced by `forbidden-dep` rule 7, and `editor-state-ownership` Part B was revived (#280 ARCH-LINT-EDITOR-BOUNDARIES).

**Still open — explicitly NOT closed by this reconciliation (anti-over-claim, per the §253 / §256 grounding discipline):**

- **Save-As to a *new* `.rge-project` tree** (creating a fresh project directory) remains a carried/deferred item — only saving to an already-known source and the `.rge-project` *writer* shipped.
- The **other** 2026-05-28 still-open gaps are **unchanged** and out of this Open/Save scope: menu-command execution (no functional `MenuRegistry::resolve` dispatch; `Command::OpenFile` still carries only a diagnostic id at `crates/editor-ui/src/menus/command.rs:103`), drag-drop ingestion, `io-image` consumption, and the World-only Command-Bus `Action` context (`crates/editor-actions/src/action.rs:87` — cannot reach `CadGraph` / `CadProjection`). This subsection makes **no** claim about those.

**Scope:** docs-only, forward-only. No source / test / bench / fixture / `Cargo.toml` change; no other `plans/BASELINE.md` subsection (W03 / W04 / W08 / 6.3 / 13.2 / Live-inspector wiring preflight) or the Editor-usability `:622-639` Notes/caveats block touched; the 2026-05-28 and 2026-05-21 dated content below is byte-identical.

### 2026-05-28  Editor-usability preflight reconciliation (post-ISSUE-225 / dispatch-G / ISSUE-249 + Phase-9 keyboard wiring)

This subsection forward-reconciles the dated 2026-05-21 "initial editor-usability adoption snapshot" below, which has aged substantially. It is grounded in the correction-loop-verified audit `ai_handoffs/ISSUE-254_EXEC_2026-05-28_23-49-37+0300.md` (passed Codex control after two CORRECT rounds), and every closure-evidence citation below was re-confirmed against current source at main commit `6e24706` for this reconciliation — the audit is the map; current source is the territory. The 2026-05-21 dated content below (the entry-point table, the "Workflows that work end-to-end TODAY" table, the test-coverage paragraph, the Top 3 gaps, the rejected `F → SpawnCuboidAt` analysis, the Status, the Revisit triggers, and the Notes / caveats block) is preserved byte-identical; reconciliation is by this prepend, never by in-place edit.

This dispatch supersedes the abandoned ISSUE-253 (control-blocked, closed not-planned), which asserted "Gap 2 UNCHANGED" and "neither revisit trigger fired" by extrapolating from the dated 2026-05-21 text instead of reading current source. The #254 audit was filed expressly to ground-truth current state before this reconciliation.

**Headline current reality (grounded at main commit `6e24706`):** the editor now has launch-time load paths — `--glb <path>` (dispatch G) and `--scene <path>` (ISSUE-225) — and `--scene` renders a visible egui-dock window (ISSUE-249). Six Ctrl-bound keyboard commands (Ctrl+Z / Ctrl+Y / Ctrl+S / Ctrl+0 / Ctrl+2 / Ctrl+4) route through the `editor-actions` CommandBus, and two plain-key playback commands (Space / Escape) route through the PIE PlayState. **Still absent:** the SAVE direction (no runtime serializer call site at all), any non-launch-time open/load UX, menu-command execution, drag-drop ingestion, and `io-image` consumption. The bus's `Action` context remains World-only, so the wiring that closed is keyboard-shaped, not CAD-shaped.

**Gap re-classification (each verdict re-grounded against current source):**

- **Gap 1 (`:588`, scene/project persistence) — PARTIALLY STALE.** Load direction is wired: `--scene` parses `.rge-project` / `.rge-scene` RON and lands a populated `World` (`editor/rge-editor/src/main.rs:912-968`; deps `editor/rge-editor/Cargo.toml:36-38`), and `--glb` imports a GLB (`main.rs:971-1026` → `import_glb` at `main.rs:612`; dep `Cargo.toml:29`). The SAVE direction has **no path at all** — there is no `ron::ser` / `ron::to_string` / `save_project` / `save_scene` / `save_to` / `write_to_file` runtime symbol in `editor/rge-editor/src/` or `crates/editor-shell/src/` (the only `fs::write` hits are six test-fixture writes under `#[cfg(test)]` in `main.rs`). Ctrl+S marks the bus saved-cursor (`crates/editor-shell/src/lifecycle/commands.rs:309`), not a filesystem write. So the "the editor never calls" the RON serializer claim is stale for load, still current for save.
- **Gap 2 (`:590`, Command Bus unreachable from editor UI) — SUBSTANTIALLY CLOSED.** The keyboard-to-bus wire is real: `crates/editor-shell/src/lifecycle/commands.rs:280` (`command_bus.submit`, plus undo `:291` / redo `:302` / mark_saved `:309`) is reached from the `WindowEvent::KeyboardInput` arm at `crates/editor-shell/src/lifecycle/mod.rs:1676` (gated on `!egui_consumed`). The 2026-05-21 claim that the keyboard catch-all "swallows every `KeyboardInput`" is stale — the catch-all `_ => {}` moved to `mod.rs:1725`, downstream of the real keyboard arm at `:1676`. The residual narrower gap: the bus `Action` context is World-only (`crates/editor-actions/src/action.rs:87`, unchanged), so CAD-graph mutations still cannot flow through the bus.
- **Gap 3 (`:592`, MenuRegistry + io-* loaders) — PARTIALLY STALE.** `io-gltf::import_glb` is now called (`main.rs:612`, via `--glb`). MenuRegistry is **not** reached functionally from the editor surface — the only literal `MenuRegistry` token in editor-shell / rge-editor / editor-egui-host is a doc-comment cross-reference at `crates/editor-shell/src/play_toolbar.rs:12`; the functional searches `MenuRegistry::`, `ResolvedEntry`, `menus::Command`, and `.resolve(` are individually zero in that surface. `io-image` is **not** consumed — zero `io_image::` / `load_path` / `load_bytes` matches and no `rge-io-image` dep in `editor/rge-editor/Cargo.toml` (image bytes arrive only co-bundled via `rge_io_gltf::MaterialAsset`). Drag-drop ingestion is absent (zero `DroppedFile` / `HoveredFile` / `DragAndDrop` matches).

**Revisit-trigger reality (`:615-620`):**

- **Trigger 2 (a non-CAD user-input path lands first) — FIRED.** PIE Play/Stop is bound to the keyboard: `EditorPlaybackCommand::{TogglePlay, Stop}` maps Space / Escape (`crates/editor-shell/src/lifecycle/playback.rs:110-123`) and drives the PlayState toolbar buttons (`:156-188`), reached from `mod.rs:1706-1709`. This is precisely the 2026-05-21 example "PIE Play/Stop bound to a keyboard shortcut."
- **Trigger 1 (a CommandBus integration design decision) — AMBIGUOUS.** Implementation landed (the keyboard-to-bus wire plus the single production `SetTimeScale` Action), but **no formal design artifact exists** — no ADR, no `EditorCommandCtx` aggregate, no `(&mut CadGraph, &mut CadProjection)` extension. The World-only CommandBus posture is **IMPLICIT-VIA-SHIPPED-CODE** (`crates/editor-actions/src/action.rs:87`, byte-identical to the 2026-05-21 record), not a documented prior decision. This dispatch does **not** declare Trigger 1 fired; whether the de-facto wiring suffices or a docs-only ADR is still required is a human-arbitration call.
- Because Trigger 2 has fired, the 2026-05-21 closing guidance at `:620` — "defer all user-facing editor wire-up dispatches" — is **no longer in force**.

**Phantom-reference callout (governance-surface drift).** `plans/BASELINE.md`'s own narrative and ISSUE-251's 2026-05-28 live-inspector reconciliation both refer in passing to a "CommandBus integration design preflight (decided the bus stays World-only)" as if a standalone section recorded that decision. **No such section exists.** The only BASELINE.md prose that decides "World-only" is this very Editor-usability preflight — the reference is self-referential drift. This reconciliation corrects the drift by acknowledgment: the World-only state is IMPLICIT-VIA-SHIPPED-CODE (`crates/editor-actions/src/action.rs:87`), not a prior formal decision. It deliberately does **not** author an ADR or "design preflight" artifact to make the phantom reference resolve cleanly; manufacturing the referenced artifact is the anti-pattern this dispatch exists to avoid. A forward-looking CommandBus decision record, if later desired, is a separate present-dated chip.

**How the 2026-05-21 text aged (recorded in passing, not edited in place).** The dated `:625` rejected-micro-dispatch (b) — "`--load <gltf-path>` CLI arg invokes `io-gltf::import_glb`" — was subsequently shipped as `--glb` (dispatch G; `main.rs:971-1026` + `:612`). The 2026-05-21 wording is preserved verbatim below as dated history.

**Still-open usability gaps the #254 audit ground-truthed (audit §4, Gaps 4-10), each source-grounded at `6e24706`:**

- Save direction has no path at all (no runtime serializer symbol; the only `fs::write` hits are six test fixtures under `#[cfg(test)]` in `main.rs`).
- Menu-command execution is absent (no functional `MenuRegistry::resolve`; `Command::OpenFile` carries a diagnostic id at `crates/editor-ui/src/menus/command.rs:103` but no editor-surface code dispatches the variant).
- Drag-drop ingestion is absent (zero `DroppedFile` / `HoveredFile` / `DragAndDrop` across the editor surface).
- `io-image` is unused on the editor surface (zero `io_image::` / `load_path` / `load_bytes`; no `rge-io-image` dep in `editor/rge-editor/Cargo.toml`).
- Non-CLI open/load UX is absent (files arrive only via boot-time `--glb` / `--scene` or the ISSUE-85 notify watcher on an already-`--glb`-bound path; there is no in-app file picker or "File → Open" gesture).
- CommandBus coverage is keyboard-shaped, not CAD-shaped — `SetTimeScale` is the only production `Action` impl; the World-only `Action::apply` signature (`crates/editor-actions/src/action.rs:87`) cannot reach `CadGraph` / `CadProjection`.
- The spawner registry still registers `PlaceholderTabBody` for `"tab/inspector"` (`crates/editor-ui/src/dock/spawner_registry.rs:165-168`, unchanged); the inspector renders via the host-internal `TabBody::Inspector` (`crates/editor-egui-host/src/tabs.rs`), a separate path that does not flow through the spawner registry.

**Explicitly preserved as dated methodology history (this reconciliation is bounded and does not edit them):** the `:573-582` "Workflows that work end-to-end TODAY" table, the `:584` test-coverage paragraph (the 312-`#[test]` inventory), and the entire `:622-639` Notes / caveats block (reproducer grep recipes, complementary-baselines text). These remain load-bearing dated records; this subsection references their aging items in prose without editing them.

**Closure-evidence citations (re-confirmed against current source at main `6e24706`):**

- `crates/editor-shell/src/lifecycle/commands.rs` — `EditorKeyCommand` surface + `command_bus.submit` at `:280` (undo `:291` / redo `:302` / mark_saved `:309`).
- `crates/editor-shell/src/lifecycle/playback.rs:110-123` (Space `TogglePlay` / Escape `Stop` mapping) + `:156-188` (`handle_playback_command`).
- `crates/editor-shell/src/lifecycle/mod.rs:1676` (the `WindowEvent::KeyboardInput` arm; catch-all moved to `:1725`).
- `crates/editor-shell/src/render_path.rs:279-285` (post-ISSUE-249 init split: `has_cad_scene || has_prebuilt_mesh` guards Phase 2 only), `:313-328` (EguiHost construction + InspectorHandoff stash), `:510-610` (the egui-only `render_frame_egui_only` branch that makes the `--scene` window visible).
- `editor/rge-editor/src/main.rs:912-968` (`--scene`), `:612` (`import_glb`), `:971-1026` (`--glb`).
- `editor/rge-editor/Cargo.toml:36-38` (`--scene` `rge-data` / `rge-scene-loader` / `ron` deps), `:29` (`rge-io-gltf` dep).
- `crates/editor-actions/src/action.rs:87` (World-only `Action` signature, unchanged).
- `crates/editor-ui/src/dock/spawner_registry.rs:165-168` (`PlaceholderTabBody` registration, unchanged).
- Audit basis: `ai_handoffs/ISSUE-254_EXEC_2026-05-28_23-49-37+0300.md`.

Forward-only snapshot pattern matches the ISSUE-243 / ISSUE-245 / ISSUE-251 precedent (and directly mirrors ISSUE-251's live-inspector prepend in this same file). ISSUE-249 (`--scene` window) and dispatch G (`--glb`) are closure evidence, not snapshot precedent.

### 2026-05-21 — initial editor-usability adoption snapshot (recorder host, Rust 1.92.0)

**Editor binary entry point (confirmed):**

| Binary | Path | Entry | What it does at launch today |
|---|---|---|---|
| `rge-editor` | `editor/rge-editor/Cargo.toml:12-14` → `editor/rge-editor/src/main.rs:38-96` | `fn main()` | Constructs `CadGraph` with one hardcoded `CuboidOp(1.0, 1.0, 1.0)` (line 47); spawns one ECS entity with `BRepHandle` (line 68); ticks `CadProjection` once (line 83); hands world / projection / graph to `EditorShell::with_world_projection_graph()` (line 87); runs winit event loop; renders the cuboid with Lambert+Phong + directional light |

Only one editor binary exists today; `crates/editor-shell/src/bin/` does not contain a binary entry point.

**Workflows that work end-to-end TODAY:**

| Workflow | Status | Citation |
|---|---|---|
| Launch `rge-editor`, render the hardcoded 1×1×1 cuboid | ✅ WORKING | `editor/rge-editor/src/main.rs:38-96`, `crates/editor-shell/src/render_path.rs:471-578` (clear color `0.12, 0.12, 0.14` at `:509`; one `draw_indexed` for the cuboid + optional second `draw_indexed` for the highlight overlay) |
| Mouse cursor tracking | ✅ WORKING | `crates/editor-shell/src/lifecycle.rs:760-765` updates `self.cursor_pos` on `CursorMoved` |
| Left-click face picking + orange highlight overlay (sub-ε) | ✅ WORKING | `lifecycle.rs:767-774` (`MouseInput` left-press → `handle_left_click`); `crates/editor-shell/src/camera.rs::pick_face_at()`; `crates/editor-shell/src/pick_path.rs` (`rebuild_highlight_overlay`); `crates/cad-projection/src/picking.rs:194` (`CadProjection::pick_face()`); highlight color constant at `render_path.rs:69` |
| Play / Stop / Pause / Step PIE — byte-identical world snapshot across 100 ticks | ✅ WORKING (Phase 5.3 CLOSED) | `lifecycle.rs:479` (`handle_button`); `crates/editor-shell/src/snapshot.rs` (`WorldSnapshot::capture_and_audit` / `restore_and_audit`); 8 tests in `lifecycle.rs` |
| Editor-coord state (`Selection` / `Hover` / `ActiveTool` / `FaceSelection`) persists across Play/Stop | ✅ WORKING | `crates/editor-shell/src/coord.rs` (`EditorCoord`); `crates/editor-shell/tests/snapshot_correctness.rs:24,45,84` |
| Workspace layout persistence (egui_dock RON files) | ✅ WORKING | `crates/editor-ui/src/dock/layout_service.rs` (`LayoutService::{load,save}`); `crates/editor-ui/tests/workspace_round_trip.rs:41,57` |
| Render handoff (latest-only single-threaded proxy) | ✅ WORKING (Phase 6.2) | `crates/editor-shell/src/render_input.rs` (`RenderHandoff::{acquire,publish}`); `crates/editor-shell/tests/render_input_boundary.rs:27,72,160,396` |
| Time-scale dilation (0.5× halves game progress; editor unaffected) | ✅ WORKING | `crates/editor-shell/tests/time_scale_test.rs:32` |

**Test coverage:** **312 `#[test]` annotations across `editor-*` crates** — 74 integration (`editor-shell/tests/` 42 + `editor-ui/tests/` 32) + 238 inline / `#[cfg(test)]` unit (`editor-shell/src/` 68 + `editor-ui/src/` 81 + `editor-actions/src/` 23 + `editor-state/src/` 33 + dock/layout subsystem 33). Strong coverage of: PIE snapshot semantics, face picking via camera ray, render-input handoff, editor/game-state boundary discipline, time-scale game-systems dilation, workspace-layout disk round-trip. **No coverage** of: full UI event loop (mouse clicks / keyboard input through `window_event`), WASM script reload via editor, multi-entity scene complexity, fillet/loft operator-graph UX flow.

**Top 3 usability gaps (qualitative; baseline-state findings):**

1. **No scene/project persistence — zero file-I/O paths for actual scene state.** No `open_file` / `load_project` / `save_project` symbols anywhere in `editor-shell` or `rge-editor`. Zero string-literal matches for `.rge` / `.rgeproj` / `.scene` / `.project` extensions. `crates/rge-data` declares `serde` + `ron` dependencies (e.g. `crates/cad-projection/Cargo.toml:19-20`) and a RON serialization API exists, but **the editor never calls it**. The editor cannot save user work and cannot reload anything; the hardcoded `CuboidOp(1.0, 1.0, 1.0)` is the only shape that ever exists. The authoring loop is therefore **structurally unmeasurable** — there is no loop to friction-test.

2. **Command Bus is fully implemented in `editor-actions` but structurally unreachable from the editor UI.** `crates/editor-actions/src/bus.rs:86` defines `CommandBus` with `submit` (line 143), `UndoStack`, `SaveMark`, 500 ms coalesce window, audit-ledger projection, and full unit coverage. **Zero call sites from `editor-shell` / `rge-editor` dispatch any user-triggered command through the bus.** `crates/editor-shell/src/lifecycle.rs::window_event()` ends with a catch-all `_ => {}` (line 776) that silently swallows every `KeyboardInput` event the OS delivers. The user cannot press Ctrl+Z, cannot trigger any command from the keyboard, cannot escape a tool. The Phase 2 doctrine ("Command Bus VERY EARLY", IMPLEMENTATION.md:217) has materialized as substrate but has **no production user yet** at the editor surface.

3. **`MenuRegistry` + `io-*` asset loaders are both ready internally but never called from the editor.** `crates/editor-ui/src/menus/registry.rs` (`MenuRegistry::declare_extension_point` / `register_entry` / `resolve`) is closed per W08 and tested; the `menus::Command` enum is defined. **Zero menu handlers are wired** — `ResolvedEntry` produced by the registry is never acted upon by editor-shell. `crates/io-gltf/src/lib.rs:20-24` (`import_glb` / `export_glb`) and `crates/io-image/src/lib.rs:80,87` (`load_path` / `load_bytes`) are public APIs but **have zero call sites** from `editor-shell` or `rge-editor`; no drag-drop path; no CLI argument parsing for a file path. Users cannot "File → Open" anything. Phase 5 W08 menu substrate and the Phase 4 `io-*` loaders are **paper-only at the editor surface**.

**Cross-cutting pattern.** Substrate-first architecture has worked: PIE / Command Bus / MenuRegistry / io-gltf / io-image / CadProjection / LitMeshPipeline are all closed and tested in isolation, each with their own gate-recorded baselines in this doc. **But no user-input path connects any of them to a visible scene change.** The editor today is a rendering + picking testbed with battle-tested internals nobody can drive from the UI.

**Rejected/boundedness note — F → `SpawnCuboidAt` proposal:**

A direct preflight follow-up was considered and **rejected as not bounded**: "wire `F` keypress → dispatch `Command::SpawnCuboidAt(Vec3)` through `CommandBus::submit()` → spawn a second visible cuboid; map `Ctrl+Z` to `CommandBus::undo()`; one headless integration test asserting entity-count round-trip." On surface inspection this looked like ≤ 200 LoC source + ≤ 100 LoC test. The actual scope is larger:

- **`CommandBus` is World-only today.** `crates/editor-actions/src/bus.rs:143` signs `submit(action: Box<dyn Action>, world: &mut World)`; `crates/editor-actions/src/action.rs:74-96` signs `Action::apply(&self, world: &mut World)` and `revert(&self, world: &mut World)`. The `Action` trait has **no access** to `CadGraph`, `CadProjection`, or any editor-shell render projection state. `BusEntry::apply/revert` at `bus.rs:33,44` mirror the World-only signature.
- **A visible CAD-cuboid spawn requires mutating `CadGraph` and producing a fresh `CadProjection` snapshot** — neither of which lives in `World`. The current cuboid is constructed in `editor/rge-editor/src/main.rs:47-68` against the standalone `CadGraph` + `CadProjection` instances that are handed to `EditorShell` once at construction time (`with_world_projection_graph(world, projection, graph)` at `main.rs:87`).
- **`EditorShell::with_world_projection_graph` is explicitly single-cuboid / sub-δ scope.** The render path assumes a single `BRepHandle` and a single mesh in the projection cache (matching the sub-δ.1.B closure noted in `render_path.rs`). A second visible cuboid would require:
  - Extending the projection-side rendering path to iterate multiple `BRepHandle` meshes (currently single-cuboid by construction).
  - Either (a) a `CommandBus` context redesign that exposes `&mut CadGraph` + `&mut CadProjection` to `Action::apply` (changing the World-only invariant on which the existing 23 `editor-actions` tests rely), or (b) a parallel "editor command" channel that mutates editor-shell state outside the `Action` trait (which forks the architecture and the audit-ledger story).
  - Snapshot/restore semantics for entity-count changes mid-undo-stack, which the current `WorldSnapshot` was not exercised against — `pie_round_trip.rs:156` covers entity-count-preserved-across-Play/Stop but not undo-on-Play/Stop boundary.

The minimum bounded next dispatch is therefore **NOT** a visible-CAD-spawn task. It is a smaller **CommandBus integration design / adapter** dispatch that explicitly decides whether the bus stays World-only or grows an editor command context — and what the cad-graph mutation path looks like either way. That design dispatch is not started here.

**Status:** **PHASE 9 PREFLIGHT — substrate complete in isolation, no user-input path connects to it, defer.**

- The editor binary renders + picks but cannot save / load / spawn / undo through user input.
- All three usability gaps (persistence / Command-Bus-from-UI / menu+asset wiring) share the same shape: the underlying substrate is closed and tested, only the input-to-substrate path is missing.
- The natural "small" follow-up (F → SpawnCuboidAt) is bigger than it looks because of the World-only `CommandBus` / `Action` trait surface.

**Revisit triggers** — re-run this preflight when **either** of the following becomes true:

1. **A CommandBus integration design dispatch lands a decision** on whether `CommandBus::submit` stays `(&mut World)`-only or grows a richer context (e.g. `(&mut World, &mut CadGraph, &mut CadProjection)` or a typed `&mut EditorCommandCtx` aggregate), and what the corresponding `Action::apply`/`revert` signature looks like. The decision itself can be a docs-only ADR / design note plus a stub adapter; it does not have to land the full multi-context bus.
2. **A non-CAD user-input path lands first** — e.g. workspace-layout RON load/save bound to `Ctrl+S` / `Ctrl+O` (already has working substrate per `workspace_round_trip.rs`), or PIE Play/Stop bound to a keyboard shortcut. Either would surface the `KeyboardInput` catch-all at `lifecycle.rs:776` and force a real `window_event` keyboard branch without first redesigning the bus.

Until **at least one** of those fires, **defer all user-facing editor wire-up dispatches**. The substrate quality is high and not at risk; the architectural cost of wiring the wrong abstraction first (e.g. fork the bus, or bypass the audit ledger to "ship something") is high.

**Notes / caveats:**

- This preflight does NOT propose shrinking, refactoring, or rewriting any existing editor substrate. PIE / Command Bus / MenuRegistry / `editor-coord` / `LitMeshPipeline` / `RenderHandoff` are all healthy and well-tested. The gap is purely the absence of input → substrate plumbing.
- **F → `SpawnCuboidAt` is not the only rejected micro-dispatch** — also considered and deferred for the same architectural-cost reason: (a) `Ctrl+S` writes workspace RON to a fixed path (would land easily but ships a half-loop with no `Ctrl+O` partner); (b) `--load <gltf-path>` CLI arg invokes `io-gltf::import_glb` (would land easily but ECS/projection ingestion of an external mesh is unexercised and the projection cache today assumes the `CadGraph`-owned mesh, not an imported one); (c) wire `MenuRegistry::resolve` output to a no-op handler (provides nothing user-visible).
- The asset-ingestion path is itself a Phase 4 / Phase 8 concern (cad-projection invalidation behavior on a foreign mesh isn't covered by the existing `cad-projection` tests). A foreign-mesh-into-editor dispatch would need its own preflight independent of the CommandBus-context question.
- The 312-test surface on `editor-*` provides good regression coverage for the *internals*; the gaps above are pure *external surface* gaps. Adding more `editor-*` unit tests would not move this preflight's headline numbers.
- Reproducer for the consumer inventory (read-only grep; no harness in-tree):
  ```
  # editor-shell call sites of io-* loaders (expect zero today):
  rg "io_gltf::|io_image::|io_3mf::" crates/editor-shell crates/editor-ui editor/rge-editor --type rust
  # KeyboardInput branches in editor-shell window_event (expect zero non-catch-all):
  rg "KeyboardInput|key_code|virtual_keycode" crates/editor-shell/src/lifecycle.rs
  # CommandBus::submit call sites from editor-shell / editor-ui (expect zero):
  rg "CommandBus|\.submit\(|editor_actions::" crates/editor-shell crates/editor-ui editor/rge-editor --type rust
  # File-extension string literals (expect zero in editor surface today):
  rg "\.rge\"|\.rgeproj\"|\.scene\"|\.project\"" crates/editor-shell crates/editor-ui editor/rge-editor --type rust
  ```
- This preflight is read-only and complementary to (not a replacement for) the Phase 5.3 PIE-round-trip baseline, the W10 workspace-round-trip baseline, and the §6.3 Gate A render-performance baseline already in this doc. Those entries own the *substrate-closure* baselines; this entry owns the **user-loop adoption** baseline and the two-arm revisit trigger. They should be re-read together when either trigger fires.

---

## Live-inspector wiring preflight (Phase 9)

**Budget anchors and gate references:**

- IMPLEMENTATION.md Phase 9 §9 (line 600): "Editor usability — friction points from real authoring."
- IMPLEMENTATION.md Phase 5 §5.1 (line 374): `editor-shell` — winit + lifecycle + PIE.
- This entry is a follow-up to two earlier Phase 9 preflights also in this doc: the **Editor-usability preflight** (cataloged the editor's user-facing gaps) and the **CommandBus integration design preflight** (decided the bus stays World-only). Both feed into the question: "now that an inspector widget + headless snapshot exist, how does it get rendered?"
- Companion commits in this dispatch chain (all on `origin/main`): `e3f6d27` (added headless `InspectorSnapshot` model + `EditorShell::inspector_snapshot()` accessor), `1d4ddbc` (added `editor-ui::widgets::inspector::{inspector_lines, ui}` over `&rge_editor_state::InspectorSnapshot`, moved the snapshot struct to `editor-state` so both crates share it without forcing either to depend on the other).

**This entry is a Phase 9 PREFLIGHT — pure read-only audit of the editor's egui host integration status.** It does NOT change source / tests / Cargo / lints. It establishes the negative finding that **no egui host exists in the workspace today**, names the blocker explicitly so future agents do not attempt fake live-wiring, and recommends the next read-only dispatch (egui host integration preflight) rather than a code dispatch.

**Methodology (read-only):**

1. Grep across the workspace (excluding `target/`, `OLD/`, `worktrees/`, `.claude/`, `.ai/`, `ai_handoffs/`) for `egui::Context::new`, `egui::Context::default`, `Context::run`, `egui_dock::DockArea::new`, `DockArea::show`, `egui_wgpu::Renderer`, `egui_winit::State`.
2. Read `editor/rge-editor/src/main.rs` + `Cargo.toml` end-to-end.
3. Read `crates/editor-shell/src/lifecycle/mod.rs::window_event` + `render_path.rs::render_frame_to_target` end-to-end.
4. Read `crates/editor-ui/src/dock/{mod.rs, spawner_registry.rs, tab_manager.rs}` and `widgets/{inspector.rs, node_graph.rs}` to confirm the egui consumer surface.
5. Cross-check workspace `Cargo.toml` for declared-but-unused `egui-*` workspace deps.

### 2026-05-28 - Live-inspector wiring preflight reconciliation (post-Dispatch-F + #249)

**Forward-only snapshot — the 2026-05-21 subsection below is preserved byte-identical as dated history.** This 2026-05-28 entry records that the named blocker the 2026-05-21 entry called "no egui host" no longer exists, and that the residual `--scene` no-window gap surfaced by the ISSUE-247 audit was closed by ISSUE-249 at main commit `007635d`. The pattern (prepend a dated forward snapshot above the prior dated snapshot, do not rewrite history) follows the precedent set by ISSUE-243 and ISSUE-245.

**Headline current reality (post-Dispatch-F + #249):**

- **The egui host exists.** `crates/editor-egui-host` is a workspace member at Dispatch F, with `EguiHost`, `InspectorHandoff`, `EditorTabViewer`, `egui_dock::DockState` carrying `TabBody`, and `ViewportRectSink` shipped through Dispatches A, B, C, D, and F (see closure evidence below).
- **`InspectorHandoff` is the chosen and shipped Option C delivery substrate.** `rge_editor_egui_host::InspectorHandoff` mirrors the canonical `RenderHandoff` latest-only pattern (`Mutex<Option<Arc<T>>>` + generation counter), exactly the "handoff substrate" shape captured as Option C in the 2026-05-21 A/B/C/D table below. A/B/D were not pursued.
- **`--scene` now produces a visible window with egui dock and Inspector chrome painted.** ISSUE-249, landed on main at `007635d`, constructs the window, surface, `EguiHost`, and `InspectorHandoff` unconditionally in `init_render_state`; `render_frame` has an egui-only branch for world-only launches that publishes the inspector snapshot, paints the dock, submits, and presents.

**Stale 2026-05-21 findings (recorded by line number for traceability; not edited in place):**

- `plans/BASELINE.md:652` — "no egui host exists in the workspace today". **STALE.** Superseded: `crates/editor-egui-host` exists as a workspace member at Dispatch F (see closure evidence).
- `plans/BASELINE.md:672` — "egui host — DOES NOT EXIST". **STALE.** Superseded: `EguiHost` is shipped in `crates/editor-egui-host/src/lib.rs` and constructed by `editor-shell::render_path` per frame.
- `plans/BASELINE.md:673` — "Snapshot-delivery substrate — DOES NOT EXIST". **STALE.** Superseded: `InspectorHandoff` ships in `rge_editor_egui_host::handoff` and is the chosen Option C substrate.
- `plans/BASELINE.md:676` — "Headline finding: NOT READY ... no egui host". **STALE.** The named blocker is removed; the inspector ecosystem is now wired end-to-end through editor-shell → editor-egui-host → editor-ui.
- `plans/BASELINE.md:684` — "`crates/editor-shell/Cargo.toml` declares no egui dep". **STALE.** Superseded: `crates/editor-shell/Cargo.toml:26-32` declares the `editor-shell → rge-editor-egui-host` dependency edge directly, which transitively pulls the egui pins. The host crate's own `[dependencies]` consume the workspace `egui`, `egui-winit`, `egui-wgpu`, and `egui_dock` pins.
- `plans/BASELINE.md:686` — "`egui-winit` and `egui-wgpu` ... referenced by no crate". **STALE.** Superseded: both are consumed by `crates/editor-egui-host/Cargo.toml:28-29`.
- `plans/BASELINE.md:687` — "no post-cuboid UI pass". **STALE.** Superseded: `editor-shell::render_path` now drives an egui pass on every frame (the cuboid path and the egui-only path both call `EguiHost::render` between geometry/clear and `queue.submit()`).
- `plans/BASELINE.md:688` — "No UI pass between or after". **STALE.** Same supersession as `:687`.
- `plans/BASELINE.md:716+` — "Recommended next dispatch: egui host integration preflight" recommendation block (lines 716-734). **STALE as a forward recommendation.** The egui host integration preflight already ran, Dispatches A/F shipped the host, the ISSUE-247 audit closed Q3/Q4 with explicit verdicts, and ISSUE-249 closed the residual `--scene` no-window gap. The recommendation block is preserved verbatim as a dated artifact of how the call was framed on 2026-05-21.

**Explicit preserve list — the following 2026-05-21 material remains useful as history and is NOT marked stale:**

- `plans/BASELINE.md:689` — the W03 egui-stripping markers. The historical record that W03 consciously deferred host integration to a later wave remains accurate and load-bearing for anyone reading the `editor-shell::lifecycle::mod.rs:18` / `:810` comments today.
- `plans/BASELINE.md:741` — the workspace egui dependency-pin observation (`egui` 0.34 / `egui-winit` 0.34 / `egui-wgpu` 0.34 / `egui_dock` 0.19). The pins are still the production pins; they are now consumed by `editor-egui-host` and `editor-ui` instead of being workspace-only forward-looking declarations.
- `plans/BASELINE.md:692-701` — the A/B/C/D delivery-options table. Recorded as a historical design-space record: **Option C (handoff substrate) is the chosen and shipped path.** The table itself is preserved verbatim so a future reader can see what alternatives were considered and why C was selected.
- `plans/BASELINE.md:738` — the 11 plus 14 headless test-count inventory for `inspector_snapshot_smoke.rs` + `inspector_widget_smoke.rs`. Those tests still exist and still pin the producer + formatter + render-fn contracts; this dispatch does not re-measure them but does not invalidate them either.

**Closure evidence (grounded at main commit `007635d`):**

- `crates/editor-egui-host/Cargo.toml:1-39` — the crate exists, is published as `rge-editor-egui-host`, and consumes the workspace `egui` / `egui-winit` / `egui-wgpu` / `egui_dock` pins plus `wgpu` / `winit`. This directly closes the stale `:686` claim.
- `crates/editor-egui-host/src/lib.rs:1-100` — the Dispatch A/F arc is documented in the crate doc-comment: Dispatch A scaffold (`EguiHost` struct + constructor + input adapter + resize hook), Dispatch B render pass, Dispatch C `InspectorHandoff` + `TabBody` / `EditorTabViewer` + `DockState<TabBody>` + `EguiHost::inspector_handoff`, Dispatch D split dock layout (Viewport + Inspector panes), Dispatch F `ViewportRectSink` for face-pick routing. This closes the stale `:672` / `:673` claims.
- `crates/editor-shell/Cargo.toml:26-32` — the `editor-shell → rge-editor-egui-host` dependency edge is declared, with an inline comment recording that the reverse edge would create a cycle and is forbidden. This directly supersedes the stale `:684` "no egui dep" claim.
- `crates/editor-shell/src/render_path.rs:279-285` — post-ISSUE-249, the `has_cad_scene || has_prebuilt_mesh` guard gates only Phase 2 render-state setup (`init_render_state_post_surface`); the empty-world branch stashes `gfx_ctx` ourselves so the EguiHost construction below (and the egui-only `render_frame` path) can read `self.gfx_ctx`. This closes the `--scene` no-window gap the ISSUE-247 audit identified.
- `crates/editor-shell/src/render_path.rs:313-328` — `EguiHost::new(device, surface_format, depth_format=None, msaa_samples=1, window, ViewportId::ROOT)` construction and `self.inspector_handoff = Some(Arc::clone(host.inspector_handoff()))` stash, performed unconditionally when `gfx_ctx + surface_ctx + window` are all present. The host and the editor-shell-side handoff clone point at the same underlying slot — the publish/acquire pair is the live Dispatch C wire.
- `crates/editor-shell/src/render_path.rs:510-610` — the egui-only `render_frame_egui_only` branch added by ISSUE-249: acquire surface, clear pass with `DEFAULT_CLEAR`, publish a fresh `InspectorSnapshot` via the handoff, call `EguiHost::render` (egui-winit `take_egui_input` + `Context::run` + `egui-wgpu` paint into the same encoder), submit, present, request next redraw. This is the painted egui frame the `--scene` no-window path now produces. It closes the stale `:687` / `:688` "no post-cuboid UI pass" claims for the world-only launch shape.
- Main commit reference: `007635d` (ISSUE-249 merge to main). All file/line refs above are stable at that commit; the orchestrator dispatch worktree was branched from it.

**Out-of-scope (scope-bounding mention, not reconciled here):** the Editor-usability preflight at `plans/BASELINE.md:588-592` concerning `open_file`, `load_project`, and `save_project` is partially stale post-ISSUE-225 and more stale post-ISSUE-249, but reconciling that section is out of scope for ISSUE-251 and belongs in a separate hygiene dispatch. This 2026-05-28 entry only reconciles the live-inspector wiring preflight (Phase 9).

---

### 2026-05-21 — initial egui-host status snapshot (recorder host, Rust 1.92.0)

**Inspector ecosystem state — what exists today:**

| Component | Status | Citation |
|---|---|---|
| **Producer** — `EditorShell::inspector_snapshot()` | ✅ exists | `crates/editor-shell/src/lifecycle/mod.rs::inspector_snapshot()` (per `e3f6d27`); 11 headless tests in `tests/inspector_snapshot_smoke.rs` |
| **Shared data type** — `rge_editor_state::InspectorSnapshot` | ✅ exists | `crates/editor-state/src/inspector_snapshot.rs` (per `1d4ddbc`); flat `Copy` struct with 10 fields; re-exported as `editor_shell::InspectorSnapshot` |
| **Pure formatter** — `inspector_lines(&InspectorSnapshot) -> Vec<(String, String)>` | ✅ exists | `crates/editor-ui/src/widgets/inspector.rs` (per `1d4ddbc`); 14 headless tests in `tests/inspector_widget_smoke.rs` |
| **egui render fn** — `ui(&InspectorSnapshot, &mut egui::Ui)` | ✅ exists | `crates/editor-ui/src/widgets/inspector.rs::ui` (per `1d4ddbc`) |
| **egui host** — `egui::Context` + `egui_winit::State` + `egui_wgpu::Renderer` driving frames | ❌ DOES NOT EXIST | Zero matches across the workspace |
| **Snapshot-delivery substrate** — mechanism to thread `InspectorSnapshot` to a rendering tab body per frame | ❌ DOES NOT EXIST | Depends on host; no design decision today |
| **Spawner wire-up** — `"tab/inspector"` → real Inspector tab body | ❌ DOES NOT EXIST | `crates/editor-ui/src/dock/spawner_registry.rs:165-169` continues to register `PlaceholderTabBody` for every default tab id including `"tab/inspector"` |

**Headline finding: NOT READY for live inspector-tab wiring. Named blocker: no egui host.**

**Evidence — every component a "live wiring" dispatch would need is absent:**

- **Zero `egui::Context` construction anywhere in production code.** Grep of `egui::Context::new`, `egui::Context::default`, `Context::run` returned zero matches outside `target/` / `OLD/` / `worktrees/`.
- **Zero `egui_dock::DockArea::show` (or any DockArea constructor) call sites.** The only `DockState` construction is `crates/editor-ui/src/dock/tab_manager.rs:219` — a state container builder inside `LayoutBlueprint::into_dock_state_with`, NOT a renderer host.
- **Zero `egui_winit::State` adapter usage.** No code routes winit `WindowEvent` to egui input.
- **Zero `egui_wgpu::Renderer` integration.** No code performs the egui GPU render pass.
- **`crates/editor-shell/Cargo.toml`** declares no egui dep of any kind (verified by reading lines 19-63): the production deps are `rge-editor-state`, `rge-editor-actions`, `rge-kernel-ecs`, `rge-input`, `rge-cad-projection`, `rge-gfx`, `rge-brep-render`, `rge-cad-core`, plus the external `winit`, `tracing`, `glam`, `wgpu`, optional `serde`/`ron` (`fixture-ron` feature).
- **`editor/rge-editor/Cargo.toml`** declares no egui dep of any kind: the production deps are `rge-editor-shell`, `rge-cad-core`, `rge-cad-projection`, `rge-kernel-ecs`, plus `winit`, `tracing-subscriber`.
- **Workspace `Cargo.toml`** does pin `egui = "0.34"`, `egui-winit = "0.34"`, `egui-wgpu = "0.34"`, `egui_dock = "0.19"` — but only `editor-ui` consumes `egui` + `egui_dock`, and only as widget-substrate (`&mut egui::Ui` consumer pattern). `egui-winit` and `egui-wgpu` are declared in the workspace `[workspace.dependencies]` table but **referenced by no crate**.
- **`editor-shell::render_path::render_frame_to_target`** (`crates/editor-shell/src/render_path.rs:471-582`) clears the surface, sets the lit-mesh pipeline + camera/light/material bind groups, encodes one cuboid `draw_indexed`, optionally encodes a second `draw_indexed` for the sub-ε highlight overlay, then closes the pass and calls `gfx_ctx.queue().submit()`. There is **no post-cuboid UI pass**.
- **`editor-shell::lifecycle::window_event::WindowEvent::RedrawRequested`** (the per-frame entry point) ticks game systems via `tick_redraw`, acquires the render-input snapshot via `RenderHandoff::acquire`, and calls `render_frame()`. No UI pass between or after.
- **The egui-stripping was deliberate, not a stub.** `crates/editor-shell/src/lifecycle/mod.rs:18` documents verbatim: *"The original rustforge file pulls in wgpu device/queue/pipeline state and **an egui overlay**; W03 strips those out (gfx wave W21+ owns wgpu) and keeps only the lifecycle skeleton + PIE plumbing."* And `:810`: *"egui-overlay routing + IR-rebuild + close-persist stripped."* These are historical markers indicating the W03 refactor consciously deferred the host integration to a later wave.
- **Inspector widget render fn is callable from nothing today.** `editor-ui::widgets::inspector::ui(&InspectorSnapshot, &mut egui::Ui)` requires a `&mut egui::Ui` scope. No production code obtains one. The widget is structurally unreachable until the host materializes.

**Snapshot-delivery options (academic until the host exists):**

| Option | Shape | When right |
|---|---|---|
| **A — captured closure** | `Arc<dyn Fn() -> InspectorSnapshot + Send + Sync>` registered with spawner; widget pulls per frame | Conceptually clean but blocked by `EditorShell`'s non-`Sync` ownership of winit `Window` + wgpu state |
| **B — shared slot** | `Arc<RwLock<InspectorSnapshot>>` — sim writes per tick, widget reads per frame | Pragmatic if single-threaded host suffices; matches a simple publish/subscribe-per-frame pattern |
| **C — handoff substrate** | Mirror `editor-shell::render_input::RenderHandoff` per ADR-117: `Mutex<Option<Arc<T>>>` + `AtomicU64` generation counter; latest-only | Right answer if the editor grows toward dedicated render thread; precedent + tests already exist |
| **D — static snapshot in tab body** | `pub struct InspectorTabBody { snapshot: InspectorSnapshot }`; host rebuilds tab body each frame or mutates field | Toy demo only; stale-by-construction; collapses to A/B/C the moment refresh is required |

**Recommendation (when the host materializes):** Option C (handoff substrate) — matches the existing `RenderHandoff` pattern in editor-shell, future-proofs for multi-threaded render. Option B is acceptable if simpler suffices and multi-threading is deferred. Option A is blocked today by ownership; Option D is not a real option.

**This comparison is recorded for the future host-design dispatch — picking a delivery mechanism without a host to consume it would be premature.**

**Status:** **PHASE 9 PREFLIGHT — inspector ecosystem 4 of 7 components ready (producer + type + formatter + renderer); 3 missing (host + delivery substrate + spawner wire-up). Defer all live-wiring dispatches until the egui host integration preflight settles the host design.**

**Explicit rejections — what NOT to dispatch next:**

1. **NOT** an `InspectorTabBody { snapshot: InspectorSnapshot }` wrapper added to editor-ui's spawner registry. That is the Option D "static tab body" — scaffolding for a non-existent host. The widget already takes `&InspectorSnapshot` directly; wrapping the snapshot in a tab body adds a layer with no real consumer and lies about progress toward live wiring.
2. **NOT** a snapshot-delivery substrate (RwLock slot, handoff) added to editor-shell before the host exists. Premature; the host's input-routing and render-pass design dictate which substrate fits.
3. **NOT** an `egui-*` dep added to editor-shell or rge-editor today. Both are doctrine-significant decisions (where does the host live? does editor-shell grow a UI subsystem? or is a new `editor-egui-host` crate the right home?) that belong in the host integration preflight, not in an incremental code dispatch.
4. **NOT** replacing `"tab/inspector"`'s `PlaceholderTabBody` registration with a stub `InspectorTab` returning `Default::default()` snapshots. Same reason — there is no host to spawn it, and producing a stub spawner without a host is sham progress.
5. **NOT** a `ShowInspector` menu Command variant added to `editor-ui::menus::Command` enum. The 23 existing Command variants have no menu handlers wired; adding a 24th without a host that resolves any of them is theater.
6. **NOT** an egui-rendering test added (e.g. via `egui::Context::run` constructing a headless context) — the goal of such a test would be to exercise the widget end-to-end, but the value depends on the host design (which Context configuration the production host uses), so a headless test pre-host would either be too generic to be useful or would lock in design choices not yet made.

**Recommended next dispatch:** **read-only `egui host integration preflight`.**

Scope of that future read-only dispatch (NOT this preflight's responsibility to land):

1. **Where the host lives** — `editor-shell` extension vs new `crates/editor-egui-host` between editor-shell and editor-ui vs `editor/rge-editor` binary-only host. Each has distinct implications for the editor-shell ↔ editor-ui dep direction.
2. **Input-routing semantics** — how `egui_winit::State::on_window_event` interacts with the existing Phase 9 `EditorKeyCommand` keyboard branch (Ctrl+Z/Y/S) in `lifecycle::window_event`. Decide ordering (egui-first vs game-first) and whether egui consumes events the bus would otherwise receive. Cite ADR if doctrine settled here.
3. **Render-pass composition** — same encoder vs separate submit; depth-buffer interaction with the existing depth attachment; queue-ordering with the cuboid + highlight overlay pass. The egui pass would slot **after** `render_path.rs:577` (end of highlight overlay encode) and **before** `:580` (`gfx_ctx.queue().submit()`).
4. **DockState ownership** — which crate holds `DockState<TabBody>`, what the `TabBody` enum looks like across `PlaceholderTabBody` / `NodeGraphTabBody` / `InspectorTabBody` / future widgets, how the spawner registry produces `TabBody` values that include InspectorSnapshot-aware widgets.
5. **Snapshot delivery mechanism** — pick from the A/B/C/D table above with explicit rationale tied to the chosen host architecture.
6. **Dep-edge implications** — confirm `forbidden-dep` doesn't fire on the new edges (verified academically: `forbidden-dep` Rule 6 forbids only RENDERER_CRATES → game-domain, none of the proposed edges trigger it); confirm `editor-state-ownership` Part B (forbidden-imports list at `tools/architecture-lints/src/editor_state_ownership.rs:71-102`) isn't triggered; confirm no cycle.
7. **Test strategy** — can the egui host be tested headlessly? `egui` has a render-to-pixels test pattern; `egui-wgpu` does not. What's a smallest end-to-end "render the inspector tab to an off-screen target and assert pixel content" test? Or is the integration only testable interactively?
8. **`resumed`-callback timing** — `egui_winit::State` needs the winit window; `egui_wgpu::Renderer` needs the wgpu device/queue/surface. Both are constructed in `editor-shell::lifecycle::resumed`. Confirm the egui host setup belongs in or immediately after that callback.

**Revisit triggers** — re-run THIS preflight (live-inspector wiring) when **either** of the following becomes true:

1. **The `egui host integration preflight` lands a decision** on where the host lives, how it threads input/render, and which snapshot-delivery option (A/B/C/D) is chosen. At that point the live-wiring dispatch becomes a bounded code dispatch consuming the host design.
2. **A non-inspector consumer of the egui host materializes first** — e.g. a menu-bar implementation, a viewport gizmo overlay, a status-bar dirty indicator. If any of those land before the inspector, the host substrate they require will already exist, and the inspector wiring becomes incremental rather than substrate-defining.

Until **at least one** of those fires, **defer all inspector live-wiring dispatches**. The 4-of-7 ready components (producer / type / formatter / renderer) sit ready for the moment the remaining 3 (host / delivery / spawner) become buildable, and adding more model/widget surface without a host would be carrying scaffolding for a structure that doesn't exist.

**Notes / caveats:**

- The 11 headless tests in `editor-shell/tests/inspector_snapshot_smoke.rs` + the 14 in `editor-ui/tests/inspector_widget_smoke.rs` continue to pin the producer + formatter + render-fn contracts even though no live rendering happens. They are not theater — they pin the API surface so the eventual host can consume them with confidence.
- This preflight does NOT propose shrinking, removing, or simplifying any existing inspector substrate. `InspectorSnapshot` / `inspector_snapshot()` / `inspector_lines()` / `widgets::inspector::ui` are all healthy and well-tested; the gap is purely the absence of an egui host to drive them.
- The historical comments at `crates/editor-shell/src/lifecycle/mod.rs:18` and `:810` should be retained — they are the most concise statement that the W03 refactor *intentionally* stripped egui from editor-shell as a separation-of-concerns move. Future agents should read those before considering whether to re-add egui to editor-shell directly.
- The four `egui-*` workspace dependency pins (`egui` 0.34 / `egui-winit` 0.34 / `egui-wgpu` 0.34 / `egui_dock` 0.19) in the root `Cargo.toml` are correct as-is; the host integration preflight will decide which crates consume them. Pinning them in the workspace is forward-looking, not stale.
- Reproducer for the empty-host finding (read-only grep; no harness in-tree):
  ```
  # production egui::Context construction sites (expect zero):
  rg "egui::Context::new\(|egui::Context::default\(|Context::run\(" --type rust
  # production DockArea::show call sites (expect zero):
  rg "DockArea::(new|show|style)" --type rust
  # egui-winit + egui-wgpu integration (expect zero):
  rg "egui_winit::State|egui_wgpu::Renderer" --type rust
  # editor-shell + rge-editor egui deps (expect zero matches):
  rg "egui" crates/editor-shell/Cargo.toml editor/rge-editor/Cargo.toml
  ```
- This preflight is read-only and complementary to the Editor-usability preflight + the CommandBus integration design preflight already in this doc. Those entries own the substrate-readiness inventory; this entry owns the **host-readiness** baseline for the inspector ecosystem specifically and the named-blocker recommendation. They should be re-read together when the egui host integration preflight is dispatched.
