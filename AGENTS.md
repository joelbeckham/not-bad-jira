## Native UI visual review

For any task that creates or materially changes GPUI user interfaces:

1. Implement the change in Rust using Codex.
2. Build and run the native Windows application.
3. Capture a screenshot of the changed interface and save it under `design/reviews/`.
4. Use T3 Code's `delegate_task` MCP tool to invoke a Claude agent:
   - Target provider: `claudeAgent`
   - Role: `review`
   - Mode: `wait`
   - Task: Critique the screenshot as a professional desktop UI designer.
5. Give Claude the screenshot path, relevant design references, and a brief description of the intended behavior.
6. Ask for concrete, prioritized improvements to spacing, alignment, typography, color, hierarchy, and visual consistency.
7. Implement the substantive improvements with Codex.
8. Capture a new screenshot and perform one more review if necessary.

Claude should act as a read-only design critic. It should not modify application code.

## UI copy: no filler

Apply this rule to all future UI changes, on every screen:

- Keep copy concise and functional: headings, field labels, action names, status,
  validation errors and necessary recovery steps.
- Do not add slogans, marketing copy, welcome blurbs, value propositions,
  redundant instructions or decorative helper text.
- Do not add implementation details or reassurance about storage, security or
  architecture unless explicitly requested or essential to completing an action.
- Show helper text only when it is needed to perform the action or prevent data
  loss, and only while that condition applies.
- Keep demo/test entry points out of the production UI. Use explicit CLI flags
  for demo and test modes.
- Prefer removing unnecessary copy to replacing it with different filler.
- Do not repeat field labels in placeholders or explain standard OS navigation.

## Application

Rust 2024, Windows MSVC. GPUI Kit is pinned to 0.9.1 (GPUI 0.3.12);
zorite-editor 0.11.3 was compiled and rendered with Kit on Windows.
Inter and JetBrains Mono are embedded; their OFL licenses are in assets/fonts.
The title-bar and executable icons use the user-supplied Jira artwork in
`assets/jira.webp`. Run `cargo run --example prepare_icon --offline` to regenerate
the embedded PNG and multi-resolution Windows ICO. Keep the title bar to the
icon and app name, without a workspace/account suffix.

Commands (PowerShell, repository root):

```powershell
cargo run -- --demo
cargo run -- --editor-check
cargo test --offline
cargo build --release --offline
./target/release/not-bad-jira.exe
./target/release/not-bad-jira.exe --demo --measure-startup
./scripts/capture-window.ps1 -Output design/reviews/native.png
```

`--demo` uses only sample data and never authenticates. `--editor-check` opens
the native editor compatibility screen. The optional startup measurement writes
`design/reviews/startup.txt`: Rust main to the second frame callback, an upper
bound on the first frame, excluding process-loader time. It is not a measured
production cached-account startup time. Record actual timings after release runs.

`src/app.rs` owns the UI, navigation stack and command dispatch. `src/actions.rs`
is shared by key bindings, palette and context menus. `src/jira.rs`, `src/adf.rs`
and `src/config.rs` have no GPUI dependency. Network calls use worker threads;
generation-tagged responses prevent old account requests from updating a new
session. Search/detail responses also carry an edit revision, so requests begun
before an edit cannot replace its result. List refreshes merge fields and retain
cached descriptions/comments. Config changes are read on a worker; reload waits for active edits and
saves. Cache writes go through a serial worker and atomic file replacement.

`src/picker.rs` owns filtering and selection without GPUI dependencies. Field
popups keep the filter focused; navigation keys are captured before input editing
actions. Request IDs discard responses for closed or replaced pickers. Keep the
transition action ID separate from the target status ID, and preserve target
status categories for icons. Demo field selections change sample data only in
memory.

`src/avatar.rs` loads assignee avatars from Jira's `avatarUrls`. Rendering only
uses memory and a bounded queue; a separate disk worker keeps cache hits clear
of the two HTTP workers. Cached images work before account authentication and
offline, refresh after 24 hours, and remain visible if refreshing fails. Memory
holds up to 256 avatars; `cache/avatars` holds up to 512 PNGs / 16 MiB, keyed by
account and URL. Requests are deduplicated, size-limited, and retried after five
minutes on failure. Avatar redirects must never forward Jira credentials to
another origin. Demo mode shows placeholders without fetching or reading caches.

Issue type icons use Jira's `issuetype.iconUrl` through the same image cache,
including SVGs rasterized on a worker with external image references disabled.
`src/issue_type.rs` supplies themed fallbacks. Show the icon beside issue keys;
an explicit type column takes its place in list tabs that already include one.

`src/related.rs` groups parent, direct children, subtasks and directional issue
links. Preserve Jira's inward/outward labels, including custom link types.
Related rows reuse the list's type, status, priority and assignee indicators;
keep keys readable and summaries on one line. Groups use virtual lists with a
visible scrollbar when they overflow. Click opens a detail; Tab focuses a group,
arrows/J/K/Home/End navigate and Enter opens. On a detail, Enter or `/` starts
inline related filtering. Keep the filter focused while Up/Down move through
visible rows across groups; J/K remain text input. Enter opens the selection.
Esc cancels filtering and restores the detail's prior scroll; another Esc goes
back. Size key columns from all related rows so filtering does not shift them.
Related metadata loads on a worker, paginates and is cached without replacing
descriptions/comments. Keep account, request and edit-revision response guards.

Config is `%APPDATA%/not-bad-jira/config.yaml`, cache is
`%LOCALAPPDATA%/not-bad-jira/cache/issues.json` (8 MiB cap, versioned and bound
to account and tab queries). Tokens belong only in Windows Credential Manager,
under service `not-bad-jira`. Never log tokens or authorization headers. Legacy
settings were copied once, outside the app. Do not add automatic legacy
config discovery, import/migration code, compatibility paths or migration UI.
The app reads only its own config and credentials. Demo mode bypasses real
config, cache and credentials.

ADF changes need conversion tests: preserve unknown nodes as protected tokens,
reject missing/duplicated tokens, keep failed drafts, and never replace an
unrecognized structure with empty text. Existing blocks with unsupported layout
attributes stay protected. Creating issues can fail when a Jira project requires
additional custom fields; retain the summary and show Jira's validation error.

Native screenshots live under design/reviews. Capture is DPI-aware; Windows'
invisible resize border may appear around a window. Prefer accessibility actions
for native smoke tests. Keyboard smoke tests post only to the test app's exact
window handle. Never send test input to other apps.
Pass `-TargetProcessId` to native smoke and capture scripts so a closed preview
cannot redirect a check to another app window.

Validation on 2026-11-13: 38 tests pass; Clippy passes with `-D warnings`.
Native smoke checks cover list filtering, J/Enter detail navigation, command
filtering, issue-key search + Enter, summary editor and setup.
Field picker checks cover live typing, retained filter focus, Up/Down navigation,
Enter selection, mouse selection, Escape, no matches, current-value preservation
and scrolling to a current value beyond the first eight options.
Related issue checks cover directional/custom labels, parent/subtask/epic
grouping, mouse and keyboard drill-in, End scrolling in large groups and Esc
return navigation. Read-only authenticated checks loaded two custom-direction
links and five direct children of an epic successfully.
Inline navigation checks cover Enter activation, live text filtering, retained
input focus after arrows, selection across groups, safe empty results, scrolling
large groups and separate Esc cancel/back behavior.
Claude reviewed inline related navigation; its substantive findings were applied
and checked natively. The final follow-up review was attempted but failed because
organizational Claude Code subscription access was disabled. Authenticated
Jira writes still need validation against an account/project; local HTTP
fixtures cover search token pagination, 204 updates and authentication errors.
Title-bar branding was checked in the native demo and the live window's large
icon was extracted to verify the taskbar artwork. That Claude review attempt
was also blocked by the same organizational access error.

Release measurement (2026-11-13): 19,018,752 bytes (18.1 MiB). The release
demo reports 423.88 ms from Rust main to the second-frame callback. This upper
bound is not proof of the 150 ms cached-account startup target; actual first
presentation, production-cache startup and input-latency profiling remain open.
The release executable was launched successfully and its window was responsive.
See design/reviews/release-final.png and design/reviews/startup.txt.
